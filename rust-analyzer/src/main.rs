use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use anyhow::{anyhow, Context, Result};
use clap::Parser as ClapParser;
use rayon::prelude::*;
use serde::Serialize;
use tree_sitter::{Node, Parser, Point, TreeCursor};
use walkdir::WalkDir;

#[derive(ClapParser, Debug)]
#[command(author, version, about = "Build full AST for Java/C# projects")]
struct Cli {
    /// Path to project directory
    project_dir: PathBuf,

    /// Pretty-print JSON output
    #[arg(long)]
    pretty: bool,

    /// Maximum worker threads for file parsing. Defaults to available CPUs.
    #[arg(long)]
    max_workers: Option<usize>,
}

#[derive(Debug, Serialize)]
struct AstProject {
    root_path: String,
    max_workers: usize,
    files: Vec<AstFile>,
}

#[derive(Debug, Serialize)]
struct AstFile {
    path: String,
    language: LanguageKind,
    ast: AstNode,
    calls: Vec<CallSite>,
}

#[derive(Debug, Serialize)]
struct AstNode {
    kind: String,
    start_byte: usize,
    end_byte: usize,
    start_position: Position,
    end_position: Position,
    text: Option<String>,
    children: Vec<AstNode>,
}

#[derive(Debug, Serialize)]
struct Position {
    row: usize,
    column: usize,
}

#[derive(Debug, Serialize)]
struct CallSite {
    caller_context: Option<String>,
    callee_snippet: String,
    kind: String,
    location: Position,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum LanguageKind {
    Java,
    Csharp,
}

impl LanguageKind {
    fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("java") => Some(Self::Java),
            Some("cs") => Some(Self::Csharp),
            _ => None,
        }
    }

    fn parser_language(self) -> tree_sitter::Language {
        match self {
            LanguageKind::Java => tree_sitter_java::language(),
            LanguageKind::Csharp => tree_sitter_c_sharp::language(),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project = analyze_project(&cli.project_dir, cli.max_workers)?;
    let output = if cli.pretty {
        serde_json::to_string_pretty(&project)?
    } else {
        serde_json::to_string(&project)?
    };
    println!("{output}");
    Ok(())
}

fn analyze_project(project_dir: &Path, max_workers_override: Option<usize>) -> Result<AstProject> {
    if !project_dir.exists() {
        return Err(anyhow!("Directory does not exist: {}", project_dir.display()));
    }
    if !project_dir.is_dir() {
        return Err(anyhow!("Path is not a directory: {}", project_dir.display()));
    }

    let max_workers = max_workers_override.unwrap_or_else(|| {
        thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
    }).max(1);
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(max_workers)
        .build_global();

    let parse_targets: Vec<(PathBuf, LanguageKind)> = WalkDir::new(project_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.path().to_path_buf();
            LanguageKind::from_path(&path).map(|lang| (path, lang))
        })
        .collect();

    let parsed_files: Vec<Result<AstFile>> = parse_targets
        .par_iter()
        .map(|(path, language)| parse_file(path, *language, project_dir))
        .collect();

    let mut files = Vec::with_capacity(parsed_files.len());
    for parsed in parsed_files {
        files.push(parsed?);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(AstProject {
        root_path: project_dir.display().to_string(),
        max_workers,
        files,
    })
}

fn parse_file(path: &Path, language: LanguageKind, project_dir: &Path) -> Result<AstFile> {
    let mut parser = Parser::new();
    parser
        .set_language(&language.parser_language())
        .with_context(|| format!("Failed to set parser language for {:?}", language))?;

    let source = fs::read_to_string(path)
        .with_context(|| format!("Failed to read source file: {}", path.display()))?;
    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("Tree-sitter parse failed: {}", path.display()))?;

    let root = tree.root_node();
    let ast = build_ast_node(root, source.as_bytes())?;
    let calls = collect_calls(root, source.as_bytes());

    Ok(AstFile {
        path: path
            .strip_prefix(project_dir)
            .unwrap_or(path)
            .display()
            .to_string(),
        language,
        ast,
        calls,
    })
}

fn build_ast_node(node: Node<'_>, source: &[u8]) -> Result<AstNode> {
    let mut cursor = node.walk();
    let children = collect_children(&mut cursor, source)?;
    let text = node
        .utf8_text(source)
        .ok()
        .map(str::trim)
        .filter(|snippet| !snippet.is_empty() && snippet.len() <= 100)
        .map(ToString::to_string);

    Ok(AstNode {
        kind: node.kind().to_string(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start_position: point_to_position(node.start_position()),
        end_position: point_to_position(node.end_position()),
        text,
        children,
    })
}

fn collect_children(cursor: &mut TreeCursor<'_>, source: &[u8]) -> Result<Vec<AstNode>> {
    let mut children = Vec::new();
    if !cursor.goto_first_child() {
        return Ok(children);
    }

    loop {
        let child = cursor.node();
        children.push(build_ast_node(child, source)?);
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();
    Ok(children)
}

fn collect_calls(root: Node<'_>, source: &[u8]) -> Vec<CallSite> {
    let mut calls = Vec::new();
    let mut context_stack: Vec<String> = Vec::new();
    walk_for_calls(root, source, &mut context_stack, &mut calls);
    calls
}

fn walk_for_calls(
    node: Node<'_>,
    source: &[u8],
    context_stack: &mut Vec<String>,
    calls: &mut Vec<CallSite>,
) {
    let node_kind = node.kind();
    let entered_context = extract_context_name(node, source);
    if let Some(ctx) = entered_context.as_ref() {
        context_stack.push(ctx.clone());
    }

    if matches!(
        node_kind,
        "method_invocation" | "invocation_expression" | "object_creation_expression"
    ) {
        let callee_snippet = node
            .utf8_text(source)
            .ok()
            .unwrap_or(node_kind)
            .trim()
            .chars()
            .take(120)
            .collect::<String>();

        calls.push(CallSite {
            caller_context: context_stack.last().cloned(),
            callee_snippet,
            kind: node_kind.to_string(),
            location: point_to_position(node.start_position()),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_for_calls(cursor.node(), source, context_stack, calls);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if entered_context.is_some() {
        context_stack.pop();
    }
}

fn extract_context_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "class_declaration"
        | "method_declaration"
        | "constructor_declaration"
        | "local_function_statement" => node
            .child_by_field_name("name")
            .and_then(|name_node| name_node.utf8_text(source).ok())
            .map(|name| format!("{} {}", node.kind(), name.trim())),
        _ => None,
    }
}

fn point_to_position(point: Point) -> Position {
    Position {
        row: point.row + 1,
        column: point.column + 1,
    }
}
