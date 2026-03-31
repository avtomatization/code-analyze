# Code Analyze MVP

MVP для анализа кода на Java/C#:
- Rust CLI строит полный AST по каждому файлу.
- Rust CLI использует параллельный разбор файлов (`max_workers` = доступные CPU).
- FastAPI backend запускает Rust-анализатор и возвращает JSON.
- Frontend позволяет выбрать sample-проект или загрузить папку (directory upload) и визуализирует AST (граф + дерево).

## Структура

- `rust-analyzer/` - Rust-парсер (`tree-sitter` для Java/C#)
- `backend/` - FastAPI API
- `static/` - HTML/JS UI
- `samples/java_demo` - тестовый Java проект
- `samples/csharp_demo` - тестовый C# проект

## Запуск

### 1) Rust analyzer

```bash
cd rust-analyzer
cargo build --release
```

Тест:

```bash
./target/release/code_analyzer ../samples/java_demo --pretty
```

### 2) Backend + Frontend

```bash
cd backend
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
uvicorn main:app --reload --port 8000
```

Открыть:
- `http://127.0.0.1:8000/`

## Формат выхода Rust

JSON:
- `root_path`
- `max_workers`
- `files[]`
  - `path`
  - `language` (`java` / `csharp`)
  - `ast` (полное дерево)
  - `calls[]` (обнаруженные вызовы с контекстом)
