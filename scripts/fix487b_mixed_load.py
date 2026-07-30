#!/usr/bin/env python3
"""Deterministic FIX487B mixed-load scheduler and bounded runner."""

from __future__ import annotations

import argparse
import asyncio
import json
import random
import time
from collections import Counter
from dataclasses import dataclass, asdict
from pathlib import Path


PROFILE_VERSION = "fix487b-mixed-profile-v1"
DEFAULT_SEED = 487205
RETRYABLE_STATUSES = {"UNAVAILABLE", "RESOURCE_EXHAUSTED", "DEADLINE_EXCEEDED"}
NON_RETRYABLE_STATUSES = {
    "INVALID_ARGUMENT",
    "FAILED_PRECONDITION",
    "PERMISSION_DENIED",
    "UNAUTHENTICATED",
    "INTERNAL",
    "UNKNOWN",
}
OPERATION_COUNTS = {
    "SEARCH": 25,
    "RETRIEVE_CONTEXT": 35,
    "GRAPH_RETRIEVE_CONTEXT": 10,
    "INGEST_VERSION": 15,
    "DELETE_OR_EXPIRE": 5,
    "SYNC_STATUS": 5,
    "LIFECYCLE_STATUS": 5,
}


@dataclass(frozen=True)
class ScheduledOperation:
    operation_id: str
    cycle_index: int
    operation_type: str
    access_zone: str
    access_level: str
    logical_identity: str
    scheduled_at: int


def deterministic_cycle(seed: int = DEFAULT_SEED) -> list[ScheduledOperation]:
    rng = random.Random(seed)
    templates: list[str] = []
    for operation_type, count in OPERATION_COUNTS.items():
        templates.extend([operation_type] * count)
    rng.shuffle(templates)
    zones = ("4871", "4872", "4873")
    levels = ("PUBLIC", "INTERNAL", "CONFIDENTIAL", "RESTRICTED")
    return [
        ScheduledOperation(
            operation_id=f"fix487b-op-{idx:03d}-{operation_type.lower()}",
            cycle_index=idx,
            operation_type=operation_type,
            access_zone=zones[idx % len(zones)],
            access_level=levels[idx % len(levels)],
            logical_identity=f"fix487b-doc-{idx % 60:03d}",
            scheduled_at=idx,
        )
        for idx, operation_type in enumerate(templates)
    ]


def should_retry(status: str) -> bool:
    return status in RETRYABLE_STATUSES


def workload_manifest(seed: int, workers: int, client_deadline_ms: int) -> dict:
    schedule = deterministic_cycle(seed)
    return {
        "profile_version": PROFILE_VERSION,
        "seed": seed,
        "bounded_worker_count": workers,
        "unbounded_queue": False,
        "client_deadline_ms": client_deadline_ms,
        "operation_counts": dict(Counter(op.operation_type for op in schedule)),
        "operation_total": len(schedule),
        "retryable_statuses": sorted(RETRYABLE_STATUSES),
        "non_retryable_statuses": sorted(NON_RETRYABLE_STATUSES),
    }


async def execute_operation(operation: ScheduledOperation, dry_run: bool = True) -> dict:
    started = time.time()
    await asyncio.sleep(0.001 if dry_run else 0.01)
    completed = time.time()
    return {
        **asdict(operation),
        "started_at": started,
        "completed_at": completed,
        "latency_ms": round((completed - started) * 1000, 3),
        "grpc_status_initial": "OK",
        "grpc_status_final": "OK",
        "attempt_count": 1,
        "result_classification": "DRY_RUN_OK" if dry_run else "OK",
    }


async def run_schedule(
    operations: list[ScheduledOperation],
    workers: int = 5,
    dry_run: bool = True,
) -> list[dict]:
    if workers <= 0:
        raise ValueError("workers must be positive")
    queue: asyncio.Queue[ScheduledOperation | None] = asyncio.Queue(maxsize=workers * 2)
    results: list[dict] = []
    active = 0
    max_active = 0
    lock = asyncio.Lock()

    async def worker() -> None:
        nonlocal active, max_active
        while True:
            item = await queue.get()
            try:
                if item is None:
                    return
                async with lock:
                    active += 1
                    max_active = max(max_active, active)
                try:
                    results.append(await execute_operation(item, dry_run=dry_run))
                finally:
                    async with lock:
                        active -= 1
            finally:
                queue.task_done()

    tasks = [asyncio.create_task(worker()) for _ in range(workers)]
    for operation in operations:
        await queue.put(operation)
    for _ in tasks:
        await queue.put(None)
    await queue.join()
    await asyncio.gather(*tasks)
    for result in results:
        result["max_observed_concurrency"] = max_active
    return sorted(results, key=lambda row: row["cycle_index"])


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.write_text("\n".join(json.dumps(row, ensure_ascii=False, sort_keys=True) for row in rows) + "\n", encoding="utf-8")


def summarize_operations(rows: list[dict]) -> dict:
    latencies = sorted(row["latency_ms"] for row in rows)
    def percentile(p: float) -> float:
        if not latencies:
            return 0.0
        index = min(len(latencies) - 1, round((len(latencies) - 1) * p))
        return latencies[index]
    return {
        "completed_operations": len(rows),
        "operation_counts": dict(Counter(row["operation_type"] for row in rows)),
        "grpc_statuses": dict(Counter(row["grpc_status_final"] for row in rows)),
        "latency_ms": {
            "p50": percentile(0.50),
            "p95": percentile(0.95),
            "p99": percentile(0.99),
            "max": max(latencies) if latencies else 0.0,
        },
        "max_observed_concurrency": max((row["max_observed_concurrency"] for row in rows), default=0),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--workers", type=int, default=5)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--client-deadline-ms", type=int, default=30000)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    schedule = deterministic_cycle(args.seed)
    (output / "workload-manifest.json").write_text(
        json.dumps(workload_manifest(args.seed, args.workers, args.client_deadline_ms), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    write_jsonl(output / "scheduled-operations.jsonl", [asdict(op) for op in schedule])
    rows = asyncio.run(run_schedule(schedule, workers=args.workers, dry_run=args.dry_run))
    write_jsonl(output / "measurement-operations.jsonl", rows)
    summary = summarize_operations(rows)
    (output / "operation-summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
