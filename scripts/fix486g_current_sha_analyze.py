#!/usr/bin/env python3
"""Analyze focused current-SHA FIX486G raw observations without runtime calls."""
from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

try:
    from scripts.fix486g_statistical_proof import (
        GLOBAL_HARD_GATES,
        evaluate_observation,
        identity_lookup,
        quality_metrics,
        read_jsonl,
        verify_and_load_bank,
    )
except ModuleNotFoundError:
    from fix486g_statistical_proof import (
        GLOBAL_HARD_GATES,
        evaluate_observation,
        identity_lookup,
        quality_metrics,
        read_jsonl,
        verify_and_load_bank,
    )


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def summarize(bank: Path, raw_inputs: list[Path], identity_map: Path | None, output: Path) -> dict[str, Any]:
    bank_data = verify_and_load_bank(bank)
    rows = read_jsonl(raw_inputs)
    identities = identity_lookup(identity_map)
    gates: dict[str, int] = defaultdict(int)
    for gate in GLOBAL_HARD_GATES:
        gates[gate] = 0
    for profile in bank_data["profiles"].values():
        for gate in (profile.get("hard_gate") or {}):
            gates[gate] = 0
    results = [evaluate_observation(row, bank_data, identities, gates) for row in rows]
    metrics = quality_metrics(results)
    failures = [
        {
            "query_id": row["query_id"],
            "entry_point": row["entry_point"],
            "run_kind": row["run_kind"],
            "run_index": row.get("run_index"),
            "pair_id": row.get("pair_id"),
            "profile": row.get("profile"),
            "failure_codes": row["failure_codes"],
            "normalized_contexts": row.get("normalized_contexts", []),
            "graph_rank": row.get("graph_rank"),
            "direct_expected_present": row.get("direct_expected_present"),
            "valid_graph_context_count": row.get("valid_graph_context_count"),
        }
        for row in results
        if row["status"] != "PASS"
    ]
    failure_code_counts = Counter(code for row in failures for code in row["failure_codes"])
    selected_metric_names = [
        "NoAnswerSpecificity",
        "GraphParentRecall@1",
        "GraphParentRecall@3",
        "GraphParentRecall@5",
        "MRR",
        "nDCG@5",
        "DirectPreservationRate",
        "GraphParentAccuracy",
        "GraphEdgePrecision",
        "GraphProvenanceCompleteness",
    ]
    selected_metrics = {
        name: metrics.get(name, {"name": name, "outcome": "NOT_COMPUTED"})
        for name in selected_metric_names
    }
    payload = {
        "schema_version": 1,
        "status": "PASS" if not failures and not any(gates.values()) else "BLOCKED",
        "raw_observation_count": len(rows),
        "entry_point_counts": dict(Counter(row["entry_point"] for row in results)),
        "query_count": len({row["query_id"] for row in results}),
        "status_counts": dict(Counter(row["status"] for row in results)),
        "failure_code_counts": dict(sorted(failure_code_counts.items())),
        "hard_gates": dict(sorted(gates.items())),
        "metrics": selected_metrics,
        "blocker_matrix": failures,
        "raw_inputs": [{"path": str(path)} for path in raw_inputs],
        "identity_map": str(identity_map) if identity_map else None,
    }
    write_json(output, payload)
    return payload


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bank", type=Path, required=True)
    parser.add_argument("--raw-input", action="append", type=Path, required=True)
    parser.add_argument("--identity-map", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    payload = summarize(args.bank, args.raw_input, args.identity_map, args.output)
    print(json.dumps({"status": payload["status"], "raw_observation_count": payload["raw_observation_count"]}, sort_keys=True))
    return 0 if payload["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
