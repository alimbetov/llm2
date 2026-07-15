#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from datetime import datetime, timezone


def run(args: list[str], *, binary: bool = False) -> bytes | str:
    completed = subprocess.run(args, check=True, capture_output=True)
    return completed.stdout if binary else completed.stdout.decode("utf-8")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write(path: Path, content: bytes | str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(content, bytes):
        path.write_bytes(content)
    else:
        path.write_text(content, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-id")
    parser.add_argument("--model", type=Path, default=Path(os.environ.get(
        "ASTRAVECTOR_MODEL_PATH", "/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx")))
    parser.add_argument("--tokenizer", type=Path, default=Path(os.environ.get(
        "ASTRAVECTOR_TOKENIZER_PATH", "/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json")))
    parser.add_argument("--migration-head", type=int, default=39)
    args = parser.parse_args()

    git_sha = run(["git", "rev-parse", "HEAD"]).strip()
    run_id = args.run_id or (
        f"fix481-{git_sha[:7]}-"
        f"{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}"
    )
    baseline = Path("target/fix481-evidence") / run_id / "baseline"
    baseline.mkdir(parents=True, exist_ok=False)

    captures = {
        "git-head.txt": ["git", "rev-parse", "HEAD"],
        "git-branch.txt": ["git", "branch", "--show-current"],
        "git-remote.txt": ["git", "remote", "get-url", "origin"],
        "git-status.txt": ["git", "status", "--short"],
        "git-submodule-status.txt": ["git", "submodule", "status", "--recursive"],
        "tracked-working-tree.diff": ["git", "diff", "--binary", "HEAD", "--", "."],
        "staged-index.diff": ["git", "diff", "--binary", "--cached"],
        "combined-working-tree.diff": ["git", "diff", "--binary", "HEAD"],
    }
    for name, command in captures.items():
        write(baseline / name, run(command, binary=True))
    write(
        baseline / "git-status-porcelain-v2.bin",
        run(["git", "status", "--porcelain=v2", "-z"], binary=True),
    )

    untracked_raw = run(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"], binary=True
    )
    untracked = []
    for raw_path in untracked_raw.split(b"\0"):
        if not raw_path:
            continue
        path_text = os.fsdecode(raw_path)
        path = Path(path_text)
        if path.is_dir() and not path.is_symlink():
            continue
        if path.is_symlink():
            payload = os.readlink(path).encode("utf-8", errors="surrogateescape")
            file_type = "symlink"
        else:
            payload = path.read_bytes()
            file_type = "regular"
        untracked.append({
            "path": path_text,
            "type": file_type,
            "size_bytes": len(payload),
            "sha256": hashlib.sha256(payload).hexdigest(),
        })
    untracked_json = json.dumps(
        {"schema_version": 1, "files": untracked}, ensure_ascii=False,
        indent=2, sort_keys=True) + "\n"
    write(baseline / "untracked-files.json", untracked_json)
    write(baseline / "untracked-files.sha256",
          hashlib.sha256(untracked_json.encode()).hexdigest() + "\n")

    subprocess.run(["cargo", "build", "--bin", "astravector-runtime",
                    "--bin", "retrieval-load-driver"], check=True)
    env = os.environ.copy()
    env.setdefault("ASTRAVECTOR_PROFILE", "search-production-candidate")
    subprocess.run([
        "python3", "scripts/generate_effective_config.py", "--output",
        str(baseline / "effective-config.redacted.json"), "--model", str(args.model),
        "--tokenizer", str(args.tokenizer)], check=True, env=env)

    corpus_files = []
    for path in sorted(Path("benchmarks/quality/corpora").rglob("*")):
        if path.is_file():
            corpus_files.append({"path": path.as_posix(), "size_bytes": path.stat().st_size,
                                 "sha256": sha256(path)})
    corpus_bytes = json.dumps({"schema_version": 1, "files": corpus_files},
                              sort_keys=True, separators=(",", ":")).encode()
    write(baseline / "corpus-snapshot.json", corpus_bytes + b"\n")

    runtime = Path("target/debug/astravector-runtime")
    driver = Path("target/debug/retrieval-load-driver")
    validation = Path("benchmarks/quality/queries/fix480-validation.jsonl")
    holdout = Path("benchmarks/quality/queries/fix480-holdout.jsonl")
    qrels = Path("benchmarks/quality/qrels/qrels.jsonl")
    identity = {
        "schema_version": 1,
        "git_sha": git_sha,
        "working_tree_clean": not bool(run(["git", "status", "--porcelain"])),
        "runtime_binary_sha256": sha256(runtime),
        "load_driver_sha256": sha256(driver),
        "effective_config_sha256": sha256(baseline / "effective-config.redacted.json"),
        "model_sha256": sha256(args.model),
        "tokenizer_sha256": sha256(args.tokenizer),
        "corpus_sha256": hashlib.sha256(corpus_bytes).hexdigest(),
        "validation_queries_sha256": sha256(validation),
        "validation_qrels_sha256": sha256(qrels),
        "holdout_queries_sha256": sha256(holdout),
        "holdout_qrels_sha256": sha256(qrels),
        "migration_head": args.migration_head,
    }
    write(baseline / "identity.json", json.dumps(identity, indent=2, sort_keys=True) + "\n")

    manifest_lines = []
    for path in sorted(baseline.iterdir()):
        if path.is_file() and path.name != "manifest.sha256":
            manifest_lines.append(f"{sha256(path)}  {path.name}")
    write(baseline / "manifest.sha256", "\n".join(manifest_lines) + "\n")
    print(json.dumps({"status": "FIX481_BASELINE_CAPTURED", "run_id": run_id,
                      "baseline": str(baseline)}, sort_keys=True))


if __name__ == "__main__":
    main()
