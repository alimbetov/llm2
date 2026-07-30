#!/usr/bin/env python3
"""FIX487B/C capacity campaign planning and classification."""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass
from pathlib import Path

CAPACITY_LEVELS = (25, 50, 100, 200)
LEVEL_SEEDS = {25: 487225, 50: 487250, 100: 487300, 200: 487400}
MIN_COMPLETED = {25: 500, 50: 1000, 100: 1500, 200: 2000}
PRACTICAL_RECOMMENDATIONS = (20, 25, 40, 50, 75, 100, 150)
STABLE_MAX_EXPECTED_ERROR_RATE = 0.005


@dataclass(frozen=True)
class LevelPlan:
    concurrency: int
    seed: int
    runtime_warmup_seconds: int = 30
    load_warmup_seconds: int = 300
    measurement_seconds: int = 600
    cooldown_max_seconds: int = 600
    minimum_completed_operations: int = 0


def campaign_plan() -> list[dict]:
    return [
        asdict(LevelPlan(level, LEVEL_SEEDS[level], minimum_completed_operations=MIN_COMPLETED[level]))
        for level in CAPACITY_LEVELS
    ]


def classify_level(metrics: dict) -> tuple[str, str | None]:
    hard_failures = (
        "cross_zone_leakage_count",
        "access_level_violation_count",
        "deleted_context_count",
        "expired_context_count",
        "indexing_context_count",
        "orphan_binding_count",
        "orphan_outbox_count",
        "duplicate_canonical_identity_count",
        "cross_zone_binding_anomaly_count",
        "failed_outbox",
        "dead_letters",
        "missing_active_qdrant_points_after_cooldown",
        "UNKNOWN",
        "unexpected_INTERNAL",
        "panic",
        "crash",
        "deadlock",
    )
    for key in hard_failures:
        if int(metrics.get(key, 0)) > 0:
            return "FAILED", key
    if not metrics.get("cooldown_reached", False):
        return "FAILED", "COOLDOWN_NOT_REACHED"
    if not metrics.get("queues_bounded", False):
        return "FAILED", "UNBOUNDED_QUEUE"
    if not metrics.get("memory_behavior_stable", False):
        return "FAILED", "UNBOUNDED_MEMORY_GROWTH"

    completed = int(metrics.get("completed_operations", 0))
    min_completed = int(metrics.get("minimum_completed_operations", 0))
    success_rate = float(metrics.get("success_rate", 0.0))
    expected_error_rate = float(metrics.get("resource_exhausted_rate", 0.0)) + float(
        metrics.get("deadline_exceeded_rate", 0.0)
    )
    if completed >= min_completed and success_rate >= 0.995 and expected_error_rate <= STABLE_MAX_EXPECTED_ERROR_RATE:
        return "STABLE", None
    if metrics.get("controlled_saturation", False) and success_rate > 0.0:
        return "SATURATED_CONTROLLED", None
    return "FAILED", "INSUFFICIENT_COMPLETED_OPERATIONS"


def capacity_curve(level_results: list[dict]) -> dict:
    stable_levels = [row["concurrency"] for row in level_results if row["verdict"] == "STABLE"]
    saturated_levels = [row["concurrency"] for row in level_results if row["verdict"] == "SATURATED_CONTROLLED"]
    maximum_stable = max(stable_levels) if stable_levels else None
    first_saturation = min(
        [level for level in saturated_levels if maximum_stable is None or level > maximum_stable],
        default=None,
    )
    recommended = None
    if maximum_stable:
        target = int(maximum_stable * 0.75)
        candidates = [value for value in PRACTICAL_RECOMMENDATIONS if value <= target and value <= maximum_stable]
        recommended = max(candidates) if candidates else max(1, target)
    return {
        "maximum_stable_concurrency": maximum_stable,
        "first_controlled_saturation_concurrency": first_saturation or "NOT_REACHED",
        "highest_tested_controlled_concurrency": max(stable_levels + saturated_levels)
        if stable_levels or saturated_levels
        else None,
        "recommended_operating_concurrency": recommended,
    }


def write_plan(output: Path) -> dict:
    output.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema_version": 1,
        "campaign": "fix487bc-capacity",
        "concurrency_5_pilot": {
            "status": "WAIVED_BY_PRODUCT_OWNER",
            "code": "CONCURRENCY_5_PILOT_WAIVED_BY_PRODUCT_OWNER",
            "waiver_reason": "PROCEED_DIRECTLY_TO_CAPACITY_CAMPAIGN",
            "historical_status_preserved": "FIX487B_CONCURRENCY_5_PILOT_BLOCKED",
        },
        "levels": campaign_plan(),
    }
    (output / "campaign-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output")
    parser.add_argument("--classify-json")
    parser.add_argument("--curve-json")
    args = parser.parse_args()
    if args.output:
        print(json.dumps(write_plan(Path(args.output)), sort_keys=True))
        return 0
    if args.classify_json:
        metrics = json.loads(Path(args.classify_json).read_text(encoding="utf-8"))
        verdict, reason = classify_level(metrics)
        print(json.dumps({"verdict": verdict, "reason": reason}, sort_keys=True))
        return 0 if verdict != "FAILED" else 1
    if args.curve_json:
        rows = json.loads(Path(args.curve_json).read_text(encoding="utf-8"))
        print(json.dumps(capacity_curve(rows), sort_keys=True))
        return 0
    print(json.dumps({"levels": campaign_plan()}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
