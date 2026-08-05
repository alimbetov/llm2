#!/usr/bin/env python3
"""FIX487C 60-minute soak planning and classification."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

SOAK_SEED = 487460
SOAK_ARTIFACTS = (
    "bootstrap.json",
    "environment.json",
    "source-identity.json",
    "capacity-source.json",
    "dataset-manifest.json",
    "workload-manifest.json",
    "operations.jsonl",
    "warmup-operations.jsonl",
    "measurement-operations.jsonl",
    "resource-samples.jsonl",
    "retrieval-controls.jsonl",
    "periodic-integrity-checks.jsonl",
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
    "resource-trend-analysis.json",
    "integrity-summary.json",
    "soak-result.json",
    "soak-result.md",
    "terminal-status.json",
    "cleanup.json",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def soak_concurrency(maximum_stable_concurrency: int | None) -> int | None:
    if not maximum_stable_concurrency:
        return None
    return max(1, int(maximum_stable_concurrency * 0.75))


def classify_soak(metrics: dict) -> tuple[str, str | None]:
    hard_failures = (
        "cross_zone_leakage_count",
        "access_level_violation_count",
        "lifecycle_invalid_context_count",
        "orphan_binding_count",
        "orphan_outbox_count",
        "duplicate_canonical_identity_count",
        "failed_outbox",
        "dead_letters",
        "missing_active_qdrant_points_after_cooldown",
        "UNKNOWN",
        "unexpected_INTERNAL",
        "unclassified_timeout",
        "crash",
        "panic",
        "deadlock",
    )
    for key in hard_failures:
        if int(metrics.get(key, 0)) > 0:
            return "FAILED", key
    if float(metrics.get("success_rate", 0.0)) < 0.995:
        return "FAILED", "SUCCESS_RATE_BELOW_99_5"
    if float(metrics.get("sample_completeness_ratio", 0.0)) < 0.98:
        return "FAILED", "RESOURCE_SAMPLE_INCOMPLETE"
    if metrics.get("unbounded_queue_growth", False):
        return "FAILED", "UNBOUNDED_QUEUE_GROWTH"
    if metrics.get("unbounded_memory_growth", False):
        return "FAILED", "UNBOUNDED_MEMORY_GROWTH"
    if metrics.get("file_descriptor_leak", False):
        return "FAILED", "FILE_DESCRIPTOR_LEAK"
    if not metrics.get("cooldown_reached", False):
        return "FAILED", "COOLDOWN_NOT_REACHED"
    return "PASS", None


def plan_from_capacity(capacity: dict) -> dict:
    max_stable = capacity.get("maximum_stable_concurrency")
    concurrency = soak_concurrency(max_stable)
    if concurrency is None:
        return {"status": "BLOCKED", "reason": "NO_STABLE_CAPACITY_LEVEL", "soak_concurrency": None}
    return {
        "status": "READY",
        "seed": SOAK_SEED,
        "soak_concurrency": concurrency,
        "runtime_warmup_seconds": 300,
        "load_warmup_seconds": 600,
        "measurement_seconds": 3600,
        "cooldown_max_seconds": 900,
    }


def verify_soak_evidence(root: Path) -> dict:
    missing: list[str] = []
    artifacts: list[dict] = []
    for name in SOAK_ARTIFACTS:
        path = root / name
        if path.is_file():
            artifacts.append({"path": name, "sha256": sha256_file(path), "bytes": path.stat().st_size})
        else:
            missing.append(name)
    manifest = {
        "schema_version": 1,
        "artifact_count": len(artifacts),
        "missing": missing,
        "artifacts": artifacts,
        "status": "PASS" if not missing else "FAIL",
    }
    (root / "evidence-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capacity-curve")
    parser.add_argument("--classify-json")
    parser.add_argument("--verify-evidence-root")
    args = parser.parse_args()
    if args.capacity_curve:
        print(
            json.dumps(
                plan_from_capacity(json.loads(Path(args.capacity_curve).read_text(encoding="utf-8"))),
                sort_keys=True,
            )
        )
        return 0
    if args.classify_json:
        verdict, reason = classify_soak(json.loads(Path(args.classify_json).read_text(encoding="utf-8")))
        print(json.dumps({"verdict": verdict, "reason": reason}, sort_keys=True))
        return 0 if verdict == "PASS" else 1
    if args.verify_evidence_root:
        manifest = verify_soak_evidence(Path(args.verify_evidence_root))
        print(json.dumps({"status": manifest["status"], "missing": manifest["missing"]}, sort_keys=True))
        return 0 if manifest["status"] == "PASS" else 1
    print(json.dumps({"seed": SOAK_SEED, "measurement_seconds": 3600}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
