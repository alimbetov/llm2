#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def quality_gate_failures(report, expected_total=None):
    retrieval = report.get("retrieval", {})
    graph = report.get("graph", {})
    qdrant = report.get("qdrant", {}).get("canonical_comparison", {})
    failures = []
    exact = {
        "runtime_execution": (report.get("runtime_execution"), "MODEL_BACKED_E2E_CONFIRMED"),
        "verdict": (report.get("verdict"), "PASS"),
        "queries_failed": (retrieval.get("queries_failed"), 0),
        "queries_blocked": (retrieval.get("queries_blocked"), 0),
        "queries_skipped": (retrieval.get("queries_skipped"), 0),
        "graph_timeout_count": (graph.get("graph_timeout_count"), 0),
        "graph_db_error_count": (graph.get("graph_db_error_count"), 0),
        "graph_access_violation_count": (graph.get("graph_access_violation_count"), 0),
        "hard_negative_false_positive_count": (retrieval.get("hard_negative_false_positive_count"), 0),
        "positive_query_empty_context_count": (retrieval.get("positive_query_empty_context_count"), 0),
        "cross_zone_leakage_count": (retrieval.get("cross_zone_leakage_count"), 0),
        "access_level_violation_count": (retrieval.get("access_level_violation_count"), 0),
        "forbidden_document_return_count": (retrieval.get("forbidden_document_return_count"), 0),
        "wrong_document_version_count": (retrieval.get("wrong_document_version_count"), 0),
        "citation_incomplete_count": (retrieval.get("citation_incomplete_count"), 0),
        "citation_text_mismatch_count": (retrieval.get("citation_text_mismatch_count"), 0),
        "silent_degraded_response_count": (retrieval.get("silent_degraded_response_count"), 0),
        "qdrant_comparison_completed": (qdrant.get("comparison_completed"), True),
        "qdrant_missing_points": (qdrant.get("missing_points"), 0),
        "qdrant_extra_points": (qdrant.get("extra_points"), 0),
        "outbox_dead_letter_count": (report.get("outbox", {}).get("outbox_dead_letter_count"), 0),
    }
    for name, (actual, expected) in exact.items():
        if actual != expected:
            failures.append({"code": name.upper(), "actual": actual, "expected": expected})
    if expected_total is not None:
        for name in ("queries_total", "queries_passed"):
            if retrieval.get(name) != expected_total:
                failures.append({"code": f"{name.upper()}_MISMATCH", "actual": retrieval.get(name), "expected": expected_total})
    for name, minimum in {
        "recall_at_5": .95, "recall_at_20": .98, "mrr_at_10": .90,
        "ndcg_at_10": .90, "precision_at_5": .90,
    }.items():
        actual = retrieval.get(name)
        if not isinstance(actual, (int, float)) or actual < minimum:
            failures.append({"code": f"{name.upper()}_BELOW_THRESHOLD", "actual": actual, "minimum": minimum})
    if graph.get("graph_expected_related_hits") != graph.get("graph_expected_related_total"):
        failures.append({"code": "GRAPH_EXPECTED_HITS_INCOMPLETE"})
    return failures


def quality_gate_passes(report, expected_total=None):
    return not quality_gate_failures(report, expected_total)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=Path)
    parser.add_argument("--expected-total", type=int)
    args = parser.parse_args()
    report = json.loads(args.report.read_text(encoding="utf-8"))
    failures = quality_gate_failures(report, args.expected_total)
    print(json.dumps({"passed": not failures, "failures": failures}, indent=2, sort_keys=True))
    raise SystemExit(0 if not failures else 1)


if __name__ == "__main__":
    main()
