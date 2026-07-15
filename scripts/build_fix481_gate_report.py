#!/usr/bin/env python3
"""Build the fail-closed fix481 stage registry and final evidence report."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--validation-report", type=Path, required=True)
    parser.add_argument(
        "--judgment-manifest",
        type=Path,
        default=Path("benchmarks/quality/judgments/manifests/validation.json"),
    )
    args = parser.parse_args()
    static = load(args.run_dir / "static" / "static-gate.json")
    trace = load(args.run_dir / "validation-3" / "ranking-first-loss-report.json")
    validation = load(args.validation_report)
    judgments = load(args.judgment_manifest)
    baseline_complete = (args.run_dir / "baseline" / "manifest.sha256").is_file()
    failures = []
    if not baseline_complete:
        failures.append({"stage": "baseline", "code": "BASELINE_IDENTITY_INCOMPLETE"})
    if static.get("status") != "PASS":
        failures.append({"stage": "static", "code": "RANKING_INVARIANT_FAILED"})
    if not trace.get("ranking_trace_complete"):
        failures.append({"stage": "ranking_trace", "code": "TRACE_INCOMPLETE"})
    if not judgments.get("qrels_complete"):
        failures.append({"stage": "evaluation", "code": "EVALUATION_DATA_INCOMPLETE"})
    if validation.get("verdict") != "PASS":
        retrieval = validation.get("retrieval", {})
        query_assertions_pass = (
            retrieval.get("queries_total") == 19
            and retrieval.get("queries_passed") == 19
            and retrieval.get("queries_failed") == 0
            and retrieval.get("queries_blocked") == 0
        )
        failures.append(
            {
                "stage": "validation",
                "code": (
                    "VALIDATION_METRIC_FAILED"
                    if query_assertions_pass
                    else "VALIDATION_19_OF_19_FAILED"
                ),
            }
        )
    retrieval = validation.get("retrieval", {})
    report = {
        "schema_version": 1,
        "task": "v007/fix481",
        "verdict": "PASS" if not failures else "FAIL",
        "status": "FIX481_CLOSED" if not failures else "FIX481_IN_PROGRESS",
        "production_status": "NOT_PRODUCTION_READY",
        "stages": {
            "baseline": "PASS" if baseline_complete else "FAIL",
            "ranking_trace": "PASS" if trace.get("ranking_trace_complete") else "FAIL",
            "ranking_invariants": "PASS" if static.get("status") == "PASS" else "FAIL",
            "evaluation_contract": "PASS" if judgments.get("qrels_complete") else "BLOCKED",
            "validation": "PASS" if validation.get("verdict") == "PASS" else "BLOCKED",
            "holdout": "BLOCKED",
            "load_proof": "BLOCKED",
        },
        "validation": {
            "runtime_execution": validation.get("runtime_execution"),
            "queries_total": retrieval.get("queries_total"),
            "queries_passed": retrieval.get("queries_passed"),
            "queries_failed": retrieval.get("queries_failed"),
            "queries_blocked": retrieval.get("queries_blocked"),
            "hit_at_5": retrieval.get("hit_at_5"),
            "recall_at_5": retrieval.get("recall_at_5"),
            "qrels_complete": retrieval.get("qrels_complete"),
            "unjudged_at_5": retrieval.get("unjudged_at_5"),
        },
        "judgments": {
            "status": judgments.get("status"),
            "pool_depth": judgments.get("requested_pool_depth"),
            "minimum_pool_source_count": judgments.get("minimum_pool_source_count"),
            "judged_candidates_total": judgments.get("judged_candidates_total"),
            "unjudged_candidates_total": judgments.get("unjudged_candidates_total"),
        },
        "failure_registry": failures,
    }
    registry = args.run_dir / "stage-failures.json"
    final_json = args.run_dir / "final-report.json"
    final_md = args.run_dir / "final-report.md"
    registry.write_text(json.dumps(failures, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    final_json.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    final_md.write_text(
        "# fix481 Gate Report\n\n"
        f"- status: `{report['status']}`\n"
        f"- verdict: `{report['verdict']}`\n"
        f"- production status: `{report['production_status']}`\n"
        f"- model-backed query assertions: `{retrieval.get('queries_passed')}/{retrieval.get('queries_total')}`\n"
        f"- qrels complete: `{retrieval.get('qrels_complete')}`\n"
        f"- unjudged candidates: `{judgments.get('unjudged_candidates_total')}`\n"
        f"- holdout: `BLOCKED`\n"
        f"- load proof: `BLOCKED`\n",
        encoding="utf-8",
    )
    checksums = []
    for path in (registry, final_json, final_md):
        checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")
    (args.run_dir / "checksums.sha256").write_text("\n".join(checksums) + "\n", encoding="ascii")
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
