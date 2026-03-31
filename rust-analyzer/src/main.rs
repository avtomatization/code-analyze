use std::char;
use std::collections::HashMap;
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
#[command(author, version, about = "Build full AST with semantic graph for Java/C#/Python projects")]
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
    semantic: ProjectSemantic,
}

#[derive(Debug, Serialize)]
struct AstFile {
    path: String,
    language: LanguageKind,
    source_text: String,
    ast: AstNode,
    calls: Vec<CallSite>,
    semantic: FileSemantic,
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

#[derive(Debug, Clone, Serialize)]
struct Position {
    row: usize,
    column: usize,
}

#[derive(Debug, Clone, Serialize)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Debug, Clone, Serialize)]
struct CallSite {
    caller_context: Option<String>,
    callee_name: Option<String>,
    callee_snippet: String,
    kind: String,
    location: Position,
}

#[derive(Debug, Clone, Serialize)]
struct Symbol {
    id: String,
    name: String,
    fq_name: String,
    kind: SymbolKind,
    language: LanguageKind,
    file_path: String,
    scope: String,
    range: Range,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum SymbolKind {
    Class,
    Method,
    Constructor,
    Parameter,
    Variable,
}

#[derive(Debug, Clone, Serialize)]
struct SemanticReference {
    kind: String,
    name: String,
    context: Option<String>,
    file_path: String,
    location: Position,
    resolved_to: Option<String>,
    confidence: f32,
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedCall {
    caller_context: Option<String>,
    callee_name: String,
    kind: String,
    file_path: String,
    location: Position,
    resolved_to: Option<String>,
    confidence: f32,
}

#[derive(Debug, Serialize)]
struct FileSemantic {
    symbols: Vec<Symbol>,
    dependencies: Vec<String>,
    references: Vec<SemanticReference>,
    resolved_calls: Vec<ResolvedCall>,
}

#[derive(Debug, Serialize)]
struct ProjectSemantic {
    symbol_count: usize,
    reference_count: usize,
    resolved_call_count: usize,
    unresolved_call_count: usize,
    symbols: Vec<Symbol>,
    resolved_calls: Vec<ResolvedCall>,
}

#[derive(Debug)]
struct ParsedFileRaw {
    path: String,
    language: LanguageKind,
    source_text: String,
    ast: AstNode,
    calls: Vec<CallSite>,
    symbols: Vec<Symbol>,
    dependencies: Vec<String>,
    call_candidates: Vec<CallCandidate>,
}

#[derive(Debug, Clone)]
struct CallCandidate {
    caller_context: Option<String>,
    callee_name: String,
    kind: String,
    file_path: String,
    location: Position,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum LanguageKind {
    Java,
    Csharp,
    Python,
}

impl LanguageKind {
    fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("java") => Some(Self::Java),
            Some("cs") => Some(Self::Csharp),
            Some("py") => Some(Self::Python),
            _ => None,
        }
    }

    fn parser_language(self) -> tree_sitter::Language {
        match self {
            LanguageKind::Java => tree_sitter_java::language(),
            LanguageKind::Csharp => tree_sitter_c_sharp::language(),
            LanguageKind::Python => tree_sitter_python::language(),
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

    let max_workers = max_workers_override
        .unwrap_or_else(|| thread::available_parallelism().map(usize::from).unwrap_or(1))
        .max(1);
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

    let parsed_files: Vec<Result<ParsedFileRaw>> = parse_targets
        .par_iter()
        .map(|(path, language)| parse_file(path, *language, project_dir))
        .collect();

    let mut raw_files = Vec::with_capacity(parsed_files.len());
    for parsed in parsed_files {
        raw_files.push(parsed?);
    }
    raw_files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut class_index: HashMap<String, Vec<Symbol>> = HashMap::new();
    let mut method_index: HashMap<String, Vec<Symbol>> = HashMap::new();
    let mut global_symbols: Vec<Symbol> = Vec::new();

    for raw in &raw_files {
        for symbol in &raw.symbols {
            global_symbols.push(symbol.clone());
            match symbol.kind {
                SymbolKind::Class => {
                    class_index
                        .entry(symbol.name.to_lowercase())
                        .or_default()
                        .push(symbol.clone());
                }
                SymbolKind::Method | SymbolKind::Constructor => {
                    method_index
                        .entry(symbol.name.to_lowercase())
                        .or_default()
                        .push(symbol.clone());
                }
                SymbolKind::Parameter | SymbolKind::Variable => {}
            }
        }
    }

    let mut files = Vec::with_capacity(raw_files.len());
    let mut project_calls = Vec::new();
    let mut unresolved_call_count = 0usize;
    let mut project_reference_count = 0usize;

    for raw in raw_files {
        let mut references: Vec<SemanticReference> = raw
            .dependencies
            .iter()
            .map(|dep| SemanticReference {
                kind: "dependency".to_string(),
                name: dep.clone(),
                context: None,
                file_path: raw.path.clone(),
                location: Position { row: 1, column: 1 },
                resolved_to: None,
                confidence: 1.0,
            })
            .collect();

        let mut resolved_calls = Vec::new();
        for call in &raw.call_candidates {
            let (resolved_to, confidence) = resolve_call(call, &method_index, &class_index);
            if resolved_to.is_none() {
                unresolved_call_count += 1;
            }
            resolved_calls.push(ResolvedCall {
                caller_context: call.caller_context.clone(),
                callee_name: call.callee_name.clone(),
                kind: call.kind.clone(),
                file_path: call.file_path.clone(),
                location: call.location.clone(),
                resolved_to: resolved_to.clone(),
                confidence,
            });
            references.push(SemanticReference {
                kind: "call".to_string(),
                name: call.callee_name.clone(),
                context: call.caller_context.clone(),
                file_path: call.file_path.clone(),
                location: call.location.clone(),
                resolved_to,
                confidence,
            });
        }

        project_reference_count += references.len();
        project_calls.extend(resolved_calls.clone());
        files.push(AstFile {
            path: raw.path,
            language: raw.language,
            source_text: raw.source_text,
            ast: raw.ast,
            calls: raw.calls,
            semantic: FileSemantic {
                symbols: raw.symbols,
                dependencies: raw.dependencies,
                references,
                resolved_calls,
            },
        });
    }

    Ok(AstProject {
        root_path: project_dir.display().to_string(),
        max_workers,
        files,
        semantic: ProjectSemantic {
            symbol_count: global_symbols.len(),
            reference_count: project_reference_count,
            resolved_call_count: project_calls.iter().filter(|c| c.resolved_to.is_some()).count(),
            unresolved_call_count,
            symbols: global_symbols,
            resolved_calls: project_calls,
        },
    })
}

fn parse_file(path: &Path, language: LanguageKind, project_dir: &Path) -> Result<ParsedFileRaw> {
    let mut parser = Parser::new();
    parser
        .set_language(&language.parser_language())
        .with_context(|| format!("Failed to set parser language for {:?}", language))?;

    let source_bytes = fs::read(path)
        .with_context(|| format!("Failed to read source file: {}", path.display()))?;
    let source = decode_source(&source_bytes);
    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("Tree-sitter parse failed: {}", path.display()))?;

    let root = tree.root_node();
    let ast = build_ast_node(root, source.as_bytes())?;
    let rel_path = path
        .strip_prefix(project_dir)
        .unwrap_or(path)
        .display()
        .to_string();
    let (calls, call_candidates) = collect_calls(root, source.as_bytes(), &rel_path);
    let (symbols, dependencies) = collect_symbols_and_dependencies(root, source.as_bytes(), &rel_path, language);

    Ok(ParsedFileRaw {
        path: rel_path,
        language,
        source_text: source,
        ast,
        calls,
        symbols,
        dependencies,
        call_candidates,
    })
}

fn resolve_call(
    call: &CallCandidate,
    method_index: &HashMap<String, Vec<Symbol>>,
    class_index: &HashMap<String, Vec<Symbol>>,
) -> (Option<String>, f32) {
    let key = call.callee_name.to_lowercase();
    if call.kind == "object_creation_expression" {
        if let Some(candidates) = class_index.get(&key) {
            return (Some(candidates[0].fq_name.clone()), 0.9);
        }
        return (None, 0.0);
    }

    if let Some(candidates) = method_index.get(&key) {
        if let Some(caller_ctx) = &call.caller_context {
            let caller_ctx_lower = caller_ctx.to_lowercase();
            if let Some(best) = candidates
                .iter()
                .find(|symbol| symbol.fq_name.to_lowercase().contains(&caller_ctx_lower))
            {
                return (Some(best.fq_name.clone()), 0.95);
            }
        }
        return (Some(candidates[0].fq_name.clone()), 0.7);
    }
    (None, 0.0)
}

fn decode_source(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_units(bytes[2..].chunks_exact(2), Endian::Little);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_units(bytes[2..].chunks_exact(2), Endian::Big);
    }
    if let Ok(utf8) = std::str::from_utf8(bytes) {
        return utf8.to_string();
    }

    let nul_ratio = bytes.iter().filter(|b| **b == 0).count() as f32 / bytes.len().max(1) as f32;
    if nul_ratio > 0.2 {
        let odd_nul = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
        let even_nul = bytes.iter().step_by(2).filter(|b| **b == 0).count();
        if odd_nul >= even_nul {
            return decode_utf16_units(bytes.chunks_exact(2), Endian::Little);
        }
        return decode_utf16_units(bytes.chunks_exact(2), Endian::Big);
    }

    String::from_utf8_lossy(bytes).into_owned()
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

fn decode_utf16_units<'a, I>(chunks: I, endian: Endian) -> String
where
    I: Iterator<Item = &'a [u8]>,
{
    let units = chunks.map(|pair| match endian {
        Endian::Little => u16::from_le_bytes([pair[0], pair[1]]),
        Endian::Big => u16::from_be_bytes([pair[0], pair[1]]),
    });

    char::decode_utf16(units)
        .map(|result| result.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
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

fn collect_calls(
    root: Node<'_>,
    source: &[u8],
    file_path: &str,
) -> (Vec<CallSite>, Vec<CallCandidate>) {
    let mut calls = Vec::new();
    let mut call_candidates = Vec::new();
    let mut context_stack: Vec<String> = Vec::new();
    walk_for_calls(
        root,
        source,
        file_path,
        &mut context_stack,
        &mut calls,
        &mut call_candidates,
    );
    (calls, call_candidates)
}

fn walk_for_calls(
    node: Node<'_>,
    source: &[u8],
    file_path: &str,
    context_stack: &mut Vec<String>,
    calls: &mut Vec<CallSite>,
    call_candidates: &mut Vec<CallCandidate>,
) {
    let node_kind = node.kind();
    let entered_context = extract_context_name(node, source);
    if let Some(ctx) = entered_context.as_ref() {
        context_stack.push(ctx.clone());
    }

    if matches!(
        node_kind,
        "method_invocation" | "invocation_expression" | "object_creation_expression" | "call"
    ) {
        let callee_snippet = node
            .utf8_text(source)
            .ok()
            .unwrap_or(node_kind)
            .trim()
            .chars()
            .take(120)
            .collect::<String>();
        let callee_name = extract_call_name(node, source).or_else(|| extract_name_from_snippet(&callee_snippet));
        let caller_context = context_stack.last().cloned();
        calls.push(CallSite {
            caller_context: caller_context.clone(),
            callee_name: callee_name.clone(),
            callee_snippet,
            kind: node_kind.to_string(),
            location: point_to_position(node.start_position()),
        });
        if let Some(callee_name) = callee_name {
            call_candidates.push(CallCandidate {
                caller_context,
                callee_name,
                kind: node_kind.to_string(),
                file_path: file_path.to_string(),
                location: point_to_position(node.start_position()),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_for_calls(
                cursor.node(),
                source,
                file_path,
                context_stack,
                calls,
                call_candidates,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if entered_context.is_some() {
        context_stack.pop();
    }
}

fn collect_symbols_and_dependencies(
    root: Node<'_>,
    source: &[u8],
    file_path: &str,
    language: LanguageKind,
) -> (Vec<Symbol>, Vec<String>) {
    let mut symbols = Vec::new();
    let mut dependencies = Vec::new();
    let mut scope_stack: Vec<String> = Vec::new();

    walk_semantic(
        root,
        source,
        file_path,
        language,
        &mut scope_stack,
        &mut symbols,
        &mut dependencies,
    );

    dependencies.sort();
    dependencies.dedup();
    (symbols, dependencies)
}

fn walk_semantic(
    node: Node<'_>,
    source: &[u8],
    file_path: &str,
    language: LanguageKind,
    scope_stack: &mut Vec<String>,
    symbols: &mut Vec<Symbol>,
    dependencies: &mut Vec<String>,
) {
    let kind = node.kind();
    if matches!(
        kind,
        "import_declaration"
            | "using_directive"
            | "package_declaration"
            | "import_statement"
            | "import_from_statement"
    ) {
        if let Ok(text) = node.utf8_text(source) {
            let dep = text.trim().replace('\n', " ");
            if !dep.is_empty() {
                dependencies.push(dep);
            }
        }
    }

    let declared = detect_decl_symbol(node, source, language, file_path, scope_stack);
    let mut pushed_scope = false;
    if let Some(symbol) = declared {
        let pushes_scope = matches!(
            symbol.kind,
            SymbolKind::Class | SymbolKind::Method | SymbolKind::Constructor
        );
        if pushes_scope {
            scope_stack.push(symbol.name.clone());
            pushed_scope = true;
        }
        symbols.push(symbol);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_semantic(
                cursor.node(),
                source,
                file_path,
                language,
                scope_stack,
                symbols,
                dependencies,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if pushed_scope {
        scope_stack.pop();
    }
}

fn detect_decl_symbol(
    node: Node<'_>,
    source: &[u8],
    language: LanguageKind,
    file_path: &str,
    scope_stack: &[String],
) -> Option<Symbol> {
    let (kind, name) = match node.kind() {
        "class_declaration" => (SymbolKind::Class, extract_decl_name(node, source)?),
        "class_definition" => (SymbolKind::Class, extract_decl_name(node, source)?),
        "method_declaration" | "local_function_statement" => {
            (SymbolKind::Method, extract_decl_name(node, source)?)
        }
        "function_definition" => {
            let name = extract_decl_name(node, source)?;
            if name == "__init__" {
                (SymbolKind::Constructor, name)
            } else {
                (SymbolKind::Method, name)
            }
        }
        "constructor_declaration" => (SymbolKind::Constructor, extract_decl_name(node, source)?),
        "parameter" | "typed_parameter" | "default_parameter" => {
            (SymbolKind::Parameter, extract_decl_name(node, source)?)
        }
        "variable_declarator" | "assignment" => {
            (SymbolKind::Variable, extract_decl_name(node, source)?)
        }
        _ => return None,
    };

    let scope = if scope_stack.is_empty() {
        "global".to_string()
    } else {
        scope_stack.join("::")
    };
    let fq_name = if scope == "global" {
        name.clone()
    } else {
        format!("{scope}::{name}")
    };
    let start = point_to_position(node.start_position());
    let id = format!("{}::{:?}::{}:{}:{}", file_path, kind, name, start.row, start.column);
    Some(Symbol {
        id,
        name,
        fq_name,
        kind,
        language,
        file_path: file_path.to_string(),
        scope,
        range: Range {
            start,
            end: point_to_position(node.end_position()),
        },
    })
}

fn extract_decl_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name") {
        if let Ok(text) = name_node.utf8_text(source) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    first_identifier_text(node, source)
}

fn first_identifier_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    if node.kind() == "identifier" {
        if let Ok(text) = node.utf8_text(source) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if let Some(found) = first_identifier_text(cursor.node(), source) {
                return Some(found);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

fn extract_call_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name") {
        if let Ok(text) = name_node.utf8_text(source) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    if let Some(function_node) = node.child_by_field_name("function") {
        if let Ok(text) = function_node.utf8_text(source) {
            if let Some(name) = extract_name_from_snippet(text.trim()) {
                return Some(name);
            }
        }
    }
    first_identifier_text(node, source)
}

fn extract_name_from_snippet(snippet: &str) -> Option<String> {
    let before_paren = snippet.split('(').next().unwrap_or(snippet);
    let mut current = String::new();
    let mut last_token = String::new();
    for ch in before_paren.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            last_token = current.clone();
            current.clear();
        }
    }
    if !current.is_empty() {
        last_token = current;
    }
    if last_token.is_empty() {
        None
    } else {
        Some(last_token)
    }
}

fn extract_context_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "class_declaration"
        | "class_definition"
        | "method_declaration"
        | "function_definition"
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
