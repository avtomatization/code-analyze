import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import List

from fastapi import FastAPI, File, HTTPException, UploadFile
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

ROOT_DIR = Path(__file__).resolve().parent.parent
RUST_ANALYZER_DIR = ROOT_DIR / "rust-analyzer"
SAMPLES_DIR = ROOT_DIR / "samples"
STATIC_DIR = ROOT_DIR / "static"

RUST_BINARY_NAME = "code_analyzer"
RUST_BINARY_PATH = RUST_ANALYZER_DIR / "target" / "release" / RUST_BINARY_NAME

app = FastAPI(title="Code Analyze API", version="0.1.0")
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=False,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")


def ensure_rust_binary() -> None:
    if RUST_BINARY_PATH.exists():
        return

    build_cmd = ["cargo", "build", "--release"]
    build_env = os.environ.copy()
    build_env["CARGO_TARGET_DIR"] = "target"
    result = subprocess.run(
        build_cmd,
        cwd=RUST_ANALYZER_DIR,
        env=build_env,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise HTTPException(
            status_code=500,
            detail=(
                "Rust analyzer build failed.\n"
                f"stdout:\n{result.stdout}\n"
                f"stderr:\n{result.stderr}"
            ),
        )


def run_analyzer(project_dir: Path) -> dict:
    ensure_rust_binary()
    max_workers = os.cpu_count() or 1
    cmd = [
        str(RUST_BINARY_PATH),
        str(project_dir),
        "--pretty",
        "--max-workers",
        str(max_workers),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise HTTPException(
            status_code=500,
            detail=(
                "Analyzer execution failed.\n"
                f"stdout:\n{result.stdout}\n"
                f"stderr:\n{result.stderr}"
            ),
        )

    try:
        import json

        return json.loads(result.stdout)
    except Exception as exc:  # pragma: no cover
        raise HTTPException(
            status_code=500,
            detail=f"Failed to decode analyzer JSON output: {exc}",
        ) from exc


@app.get("/")
def index() -> FileResponse:
    return FileResponse(STATIC_DIR / "index.html")


@app.get("/api/samples")
def list_samples() -> dict:
    if not SAMPLES_DIR.exists():
        return {"samples": []}

    samples = [d.name for d in SAMPLES_DIR.iterdir() if d.is_dir()]
    samples.sort()
    return {"samples": samples}


@app.post("/api/analyze-sample/{sample_name}")
def analyze_sample(sample_name: str) -> dict:
    sample_path = (SAMPLES_DIR / sample_name).resolve()
    if not sample_path.exists() or not sample_path.is_dir():
        raise HTTPException(status_code=404, detail=f"Sample not found: {sample_name}")

    if SAMPLES_DIR.resolve() not in sample_path.parents:
        raise HTTPException(status_code=400, detail="Invalid sample path")

    return run_analyzer(sample_path)


@app.post("/api/analyze-upload")
async def analyze_upload(files: List[UploadFile] = File(...)) -> dict:
    if not files:
        raise HTTPException(status_code=400, detail="No files uploaded")

    with tempfile.TemporaryDirectory(prefix="code-analyze-") as tmp:
        tmp_path = Path(tmp)
        for uploaded in files:
            rel_name = uploaded.filename or "unknown.txt"
            rel_path = Path(rel_name)
            if rel_path.is_absolute() or ".." in rel_path.parts:
                raise HTTPException(status_code=400, detail=f"Invalid upload path: {rel_name}")

            dest_path = tmp_path / rel_path
            dest_path.parent.mkdir(parents=True, exist_ok=True)

            with dest_path.open("wb") as destination:
                shutil.copyfileobj(uploaded.file, destination)

        return run_analyzer(tmp_path)


@app.get("/health")
def health() -> dict:
    return {"status": "ok", "cwd": os.getcwd()}
