#!/usr/bin/env python3
"""FIX487B/C capacity evidence manifest verifier."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT_ARTIFACTS = (
    "bootstrap.json",
    "environment.json",
    "source-identity.json",
    "campaign-manifest.json",
    "dataset-manifest.json",
    "workload-manifest.json",
    "capacity-summary.json",
    "capacity-summary.md",
    "capacity-curve.json",
    "integrity-summary.json",
    "terminal-status.json",
    "cleanup.json",
)
LEVEL_ARTIFACTS = (
    "operations.jsonl",
    "resource-samples.jsonl",
    "metrics-before.json",
    "metrics-after-warmup.json",
    "metrics-after-measurement.json",
    "metrics-after-cooldown.json",
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
    "integrity-summary.json",
    "level-result.json",
    "level-result.md",
)
LEVELS = (25, 50, 100, 200)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_paths(root: Path) -> list[Path]:
    paths = [root / name for name in ROOT_ARTIFACTS]
    for level in LEVELS:
        paths.extend(root / "levels" / f"concurrency-{level}" / name for name in LEVEL_ARTIFACTS)
    return paths


def build_manifest(root: Path) -> dict:
    missing: list[str] = []
    artifacts: list[dict] = []
    for path in expected_paths(root):
        relative = str(path.relative_to(root))
        if path.is_file():
            artifacts.append({"path": relative, "sha256": sha256_file(path), "bytes": path.stat().st_size})
        else:
            missing.append(relative)
    return {
        "schema_version": 1,
        "artifact_count": len(artifacts),
        "missing": missing,
        "artifacts": artifacts,
        "status": "PASS" if not missing else "FAIL",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    args = parser.parse_args()
    root = Path(args.root)
    manifest = build_manifest(root)
    (root / "evidence-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(json.dumps({"status": manifest["status"], "missing": manifest["missing"]}, sort_keys=True))
    return 0 if manifest["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
