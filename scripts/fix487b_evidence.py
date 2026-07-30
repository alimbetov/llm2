#!/usr/bin/env python3
"""FIX487B evidence manifest builder and verifier."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


MANDATORY_ARTIFACTS = (
    "bootstrap.json",
    "environment.json",
    "source-identity.json",
    "dataset-manifest.json",
    "logical-to-runtime.json",
    "workload-manifest.json",
    "scheduled-operations.jsonl",
    "warmup-operations.jsonl",
    "measurement-operations.jsonl",
    "resource-samples.jsonl",
    "postgres-before.json",
    "postgres-after-measurement.json",
    "postgres-after-cooldown.json",
    "qdrant-before.json",
    "qdrant-after-measurement.json",
    "qdrant-after-cooldown.json",
    "outbox-after-measurement.json",
    "outbox-after-cooldown.json",
    "latency-summary.json",
    "grpc-status-summary.json",
    "operation-summary.json",
    "integrity-summary.json",
    "pilot-result.json",
    "pilot-result.md",
    "cleanup.json",
    "terminal-status.json",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def build_manifest(root: Path) -> dict:
    missing = [name for name in MANDATORY_ARTIFACTS if not (root / name).is_file()]
    artifacts = []
    for name in MANDATORY_ARTIFACTS:
        path = root / name
        if path.is_file():
            artifacts.append({"path": name, "sha256": sha256_file(path), "bytes": path.stat().st_size})
    return {
        "schema_version": 1,
        "mandatory_count": len(MANDATORY_ARTIFACTS),
        "artifact_count": len(artifacts),
        "missing": missing,
        "artifacts": artifacts,
        "status": "PASS" if not missing else "FAIL",
    }


def verify_manifest(root: Path, manifest: dict) -> tuple[bool, list[str]]:
    errors: list[str] = []
    for item in manifest.get("artifacts", []):
        path = root / item["path"]
        if not path.is_file():
            errors.append(f"missing:{item['path']}")
        elif sha256_file(path) != item["sha256"]:
            errors.append(f"hash_mismatch:{item['path']}")
    errors.extend(f"missing:{name}" for name in manifest.get("missing", []))
    return not errors, errors


def write_terminal_status(root: Path, status: str, exit_code: int, reason: str | None = None) -> None:
    payload = {"status": status, "exit_code": exit_code, "reason": reason}
    (root / "terminal-status.json").write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    root = Path(args.root)
    manifest = build_manifest(root)
    (root / "evidence-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    ok, errors = verify_manifest(root, manifest)
    print(json.dumps({"status": "PASS" if ok and manifest["status"] == "PASS" else "FAIL", "errors": errors}, sort_keys=True))
    return 0 if ok and manifest["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
