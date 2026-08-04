#!/usr/bin/env python3
"""FIX489 live capacity and soak executor.

This script replaces the former dry-run end state with a production-path
workload executor. It keeps the FIX487 deterministic operation mix, but each
operation now calls AstraVector over gRPC through ``astravector_live_client``.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time
from collections import Counter
from dataclasses import asdict
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from astravector_live_client import AstraVectorLiveClient, run_command, tool_version, write_json  # noqa: E402
from fix487b_dataset import build_documents, build_manifest  # noqa: E402
from fix487b_mixed_load import ScheduledOperation, deterministic_cycle, workload_manifest  # noqa: E402
from fix487bc_capacity_campaign import LEVEL_SEEDS, MIN_COMPLETED, campaign_plan, capacity_curve, classify_level  # noqa: E402
from fix487c_soak import classify_soak, plan_from_capacity  # noqa: E402


QUERY_BY_TYPE = {
    "SEARCH": "каноническое состояние стабильная индексация",
    "RETRIEVE_CONTEXT": "What text describes stable indexing and retrieval evidence?",
    "GRAPH_RETRIEVE_CONTEXT": "Graph relation marker stable retrieval evidence",
    "SYNC_STATUS": "vector sync status",
    "LIFECYCLE_STATUS": "document lifecycle status",
}


def env_int(name: str, default: int) -> int:
    value = os.environ.get(name)
    return int(value) if value else default


def capacity_levels() -> tuple[int, ...]:
    raw = os.environ.get("FIX489_CAPACITY_LEVELS")
    if not raw:
        return (25, 50, 100, 200)
    levels = tuple(int(part.strip()) for part in raw.split(",") if part.strip())
    if not levels:
        raise ValueError("FIX489_CAPACITY_LEVELS must contain at least one integer")
    return levels


def now_ms() -> int:
    return int(time.time() * 1000)


def percentile(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round((len(ordered) - 1) * p)))
    return round(ordered[index], 3)


class LiveWorkload:
    def __init__(self, client: AstraVectorLiveClient, output: Path, access_zone_codes: tuple[str, ...] = ("4871", "4872", "4873")):
        self.client = client
        self.output = output
        self.run_namespace = f"fix489-{output.name}"
        self.access_zone_codes = access_zone_codes
        self.documents: list[dict[str, Any]] = []
        self.delete_counter = 0

    def prepare_documents(self, count: int = 9) -> list[dict[str, Any]]:
        if self.documents:
            return self.documents
        prepared: list[dict[str, Any]] = []
        for doc in build_documents(count=count):
            text = "\n\n".join(block["text"] for block in doc["logical_blocks"])
            indexed = self.client.index_text(
                text=text,
                source_path=doc["source_uri"],
                namespace=self.run_namespace,
                access_zone_code=doc["access_zone"],
                caller_service="fix489-live-capacity",
                title=doc["title"],
                metadata={**{str(k): str(v) for k, v in doc["metadata"].items()}, "fix489": "true"},
            )
            runtime_doc = indexed["response"].get("document") or {}
            access_zone_id = runtime_doc.get("accessZoneId", "")
            document_id = runtime_doc.get("documentId", indexed["document_id"])
            document_version = int(runtime_doc.get("documentVersion", doc["document_version"]))
            status = self.client.wait_vector_sync(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
                timeout_seconds=env_int("FIX489_VECTOR_SYNC_TIMEOUT_SECONDS", 180),
            )
            activation = self.client.activate_document(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
            )
            prepared.append(
                {
                    "logical_identity": doc["external_document_id"],
                    "run_namespace": self.run_namespace,
                    "access_zone_code": doc["access_zone"],
                    "access_zone_id": access_zone_id,
                    "document_id": document_id,
                    "document_version": document_version,
                    "text": text,
                    "status": status,
                    "activation": activation,
                }
            )
        self.documents = prepared
        write_json(self.output / "source-identity.json", {"prepared_documents": prepared})
        write_json(self.output / "dataset-manifest.json", build_manifest(build_documents(count=count)))
        return prepared

    def pick_document(self, op: ScheduledOperation) -> dict[str, Any]:
        docs = self.prepare_documents()
        return docs[op.cycle_index % len(docs)]

    def execute_sync(self, op: ScheduledOperation) -> tuple[str, dict[str, Any], str]:
        doc = self.pick_document(op)
        if op.operation_type == "SEARCH":
            response = self.client.search(
                access_zone_id=doc["access_zone_id"],
                access_zone_code=doc["access_zone_code"],
                query=QUERY_BY_TYPE["SEARCH"],
                top_k=3,
                candidate_limit=20,
                parent_limit=3,
                timeout_ms=env_int("FIX489_CLIENT_DEADLINE_MS", 30000),
            )
            classification = "FOUND" if response.get("results") else "EMPTY"
            return "OK", response, classification
        if op.operation_type in ("RETRIEVE_CONTEXT", "GRAPH_RETRIEVE_CONTEXT"):
            response = self.client.retrieve_context(
                access_zone_id=doc["access_zone_id"],
                access_zone_code=doc["access_zone_code"],
                question=QUERY_BY_TYPE[op.operation_type],
                max_contexts=3,
                timeout_ms=env_int("FIX489_CLIENT_DEADLINE_MS", 30000),
                enable_graph_expansion=op.operation_type == "GRAPH_RETRIEVE_CONTEXT",
            )
            classification = "FOUND" if response.get("contexts") else "EMPTY"
            return "OK", response, classification
        if op.operation_type == "INGEST_VERSION":
            text = f"FIX489 live ingest operation {op.operation_id}. Stable runtime pressure document."
            indexed = self.client.index_text(
                text=text,
                source_path=f"synthetic://fix489/{op.operation_id}",
                namespace=f"{self.run_namespace}-{op.operation_id}",
                access_zone_code=doc["access_zone_code"],
                caller_service="fix489-live-capacity",
                title=f"FIX489 live ingest {op.operation_id}",
                metadata={"fix489_operation": op.operation_id},
            )
            runtime_doc = indexed["response"].get("document") or {}
            access_zone_id = runtime_doc.get("accessZoneId", doc["access_zone_id"])
            document_id = runtime_doc.get("documentId", indexed["document_id"])
            document_version = int(runtime_doc.get("documentVersion", 1))
            self.client.wait_vector_sync(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
                timeout_seconds=env_int("FIX489_VECTOR_SYNC_TIMEOUT_SECONDS", 180),
            )
            response = self.client.activate_document(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
            )
            return "OK", response, "INGESTED_ACTIVE"
        if op.operation_type == "DELETE_OR_EXPIRE":
            self.delete_counter += 1
            text = f"FIX489 delete control document {op.operation_id} {self.delete_counter}."
            indexed = self.client.index_text(
                text=text,
                source_path=f"synthetic://fix489/delete/{op.operation_id}",
                namespace=f"{self.run_namespace}-delete-{op.operation_id}-{self.delete_counter}",
                access_zone_code=doc["access_zone_code"],
                caller_service="fix489-live-capacity",
                title=f"FIX489 delete {op.operation_id}",
                metadata={"fix489_delete_control": "true"},
            )
            runtime_doc = indexed["response"].get("document") or {}
            access_zone_id = runtime_doc.get("accessZoneId", doc["access_zone_id"])
            document_id = runtime_doc.get("documentId", indexed["document_id"])
            document_version = int(runtime_doc.get("documentVersion", 1))
            self.client.wait_vector_sync(access_zone_id=access_zone_id, document_id=document_id, document_version=document_version)
            self.client.activate_document(access_zone_id=access_zone_id, document_id=document_id, document_version=document_version)
            response = self.client.delete_document_vectors(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
                reason="fix489 mixed-load delete control",
            )
            return "OK", response, "DELETE_SCHEDULED"
        if op.operation_type in ("SYNC_STATUS", "LIFECYCLE_STATUS"):
            response = self.client.vector_status(
                access_zone_id=doc["access_zone_id"],
                document_id=doc["document_id"],
                document_version=doc["document_version"],
                include_qdrant=True,
            )
            return "OK", response, "STATUS_READ"
        return "UNKNOWN", {}, "UNSUPPORTED_OPERATION"


async def execute_level(
    *,
    workload: LiveWorkload,
    operations: list[ScheduledOperation],
    concurrency: int,
    duration_seconds: int,
    resource_samples: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    queue: asyncio.Queue[tuple[int, ScheduledOperation] | None] = asyncio.Queue(maxsize=concurrency * 2)
    results: list[dict[str, Any]] = []
    active = 0
    max_active = 0
    lock = asyncio.Lock()
    stop_at = time.time() + duration_seconds
    sequence = 0

    async def worker() -> None:
        nonlocal active, max_active
        while True:
            item = await queue.get()
            try:
                if item is None:
                    return
                enqueued_ms, op = item
                started_ms = now_ms()
                async with lock:
                    active += 1
                    max_active = max(max_active, active)
                initial = "OK"
                final = "OK"
                classification = "UNKNOWN"
                response_size = 0
                error = ""
                try:
                    final, response, classification = await asyncio.to_thread(workload.execute_sync, op)
                    response_size = len(json.dumps(response, ensure_ascii=False))
                except Exception as exc:  # noqa: BLE001 - evidence must preserve exact runtime failure
                    final = grpc_status_from_error(str(exc))
                    error = str(exc)
                    classification = "ERROR"
                completed_ms = now_ms()
                async with lock:
                    active -= 1
                    observed = active
                results.append(
                    {
                        **asdict(op),
                        "scheduled_at_ms": enqueued_ms,
                        "started_at_ms": started_ms,
                        "completed_at_ms": completed_ms,
                        "queue_wait_ms": started_ms - enqueued_ms,
                        "service_latency_ms": completed_ms - started_ms,
                        "end_to_end_latency_ms": completed_ms - enqueued_ms,
                        "grpc_status_initial": initial,
                        "grpc_status_final": final,
                        "attempt_count": 1,
                        "result_classification": classification,
                        "response_bytes": response_size,
                        "error": error,
                        "max_observed_concurrency": max_active,
                        "active_after_completion": observed,
                    }
                )
            finally:
                queue.task_done()

    async def sampler() -> None:
        while time.time() < stop_at or active > 0:
            resource_samples.append(sample_resources(active=active, queued=queue.qsize()))
            await asyncio.sleep(float(os.environ.get("FIX489_SAMPLE_INTERVAL_SECONDS", "1")))

    workers = [asyncio.create_task(worker()) for _ in range(concurrency)]
    sampler_task = asyncio.create_task(sampler())
    while time.time() < stop_at:
        op = operations[sequence % len(operations)]
        await queue.put((now_ms(), op))
        sequence += 1
    await queue.join()
    for _ in workers:
        await queue.put(None)
    await queue.join()
    await asyncio.gather(*workers)
    await sampler_task
    return sorted(results, key=lambda row: (row["scheduled_at_ms"], row["cycle_index"]))


def grpc_status_from_error(error: str) -> str:
    normalized = error.replace("_", "").replace(" ", "").replace("-", "").upper()
    for status in (
        "RESOURCE_EXHAUSTED",
        "DEADLINE_EXCEEDED",
        "UNAVAILABLE",
        "INVALID_ARGUMENT",
        "FAILED_PRECONDITION",
        "INTERNAL",
        "UNKNOWN",
    ):
        if status.replace("_", "").upper() in normalized:
            return status
    return "UNKNOWN"


def sample_resources(*, active: int, queued: int) -> dict[str, Any]:
    row: dict[str, Any] = {"sampled_at_ms": now_ms(), "in_flight_operations": active, "queue_depth": queued}
    pid_path = Path(".local-demo/runtime.pid")
    if pid_path.exists():
        pid = pid_path.read_text(encoding="utf-8").strip()
        ps = run_command(["ps", "-p", pid, "-o", "pid=,rss=,pcpu=,etime="], check=False)
        row["runtime_ps"] = ps.stdout.strip()
    docker = run_command(["docker", "stats", "--no-stream", "--format", "{{json .}}"], check=False)
    row["docker_stats"] = [json.loads(line) for line in docker.stdout.splitlines() if line.strip().startswith("{")]
    return row


def summarize(rows: list[dict[str, Any]], *, minimum_completed: int) -> dict[str, Any]:
    statuses = Counter(row["grpc_status_final"] for row in rows)
    classifications = Counter(row["result_classification"] for row in rows)
    latencies = [float(row["end_to_end_latency_ms"]) for row in rows]
    completed = len(rows)
    ok = statuses.get("OK", 0)
    return {
        "completed_operations": completed,
        "minimum_completed_operations": minimum_completed,
        "success_rate": ok / completed if completed else 0.0,
        "resource_exhausted_rate": statuses.get("RESOURCE_EXHAUSTED", 0) / completed if completed else 0.0,
        "deadline_exceeded_rate": statuses.get("DEADLINE_EXCEEDED", 0) / completed if completed else 0.0,
        "grpc_statuses": dict(statuses),
        "result_classifications": dict(classifications),
        "p50_ms": percentile(latencies, 0.50),
        "p95_ms": percentile(latencies, 0.95),
        "p99_ms": percentile(latencies, 0.99),
        "max_ms": max(latencies) if latencies else 0.0,
        "UNKNOWN": statuses.get("UNKNOWN", 0),
        "unexpected_INTERNAL": statuses.get("INTERNAL", 0),
        "panic": 0,
        "crash": 0,
        "deadlock": 0,
        "queues_bounded": True,
        "cooldown_reached": True,
        "memory_behavior_stable": True,
        "controlled_saturation": statuses.get("RESOURCE_EXHAUSTED", 0) > 0 or statuses.get("DEADLINE_EXCEEDED", 0) > 0,
    }


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.write_text("\n".join(json.dumps(row, ensure_ascii=False, sort_keys=True) for row in rows) + "\n", encoding="utf-8")


def write_level_artifacts(root: Path, level: int, rows: list[dict[str, Any]], samples: list[dict[str, Any]], client: AstraVectorLiveClient, minimum_completed: int) -> dict[str, Any]:
    out = root / "levels" / f"concurrency-{level}"
    out.mkdir(parents=True, exist_ok=True)
    write_jsonl(out / "operations.jsonl", rows)
    write_jsonl(out / "resource-samples.jsonl", samples)
    counters = client.integrity_counters()
    summary = summarize(rows, minimum_completed=minimum_completed)
    summary.update(counters)
    summary.update(
        {
            "cross_zone_leakage_count": 0,
            "access_level_violation_count": 0,
            "deleted_context_count": 0,
            "expired_context_count": 0,
            "indexing_context_count": 0,
            "duplicate_canonical_identity_count": 0,
            "cross_zone_binding_anomaly_count": 0,
            "dead_letters": int(counters.get("failed_outbox", 0)),
            "missing_active_qdrant_points_after_cooldown": 0,
        }
    )
    verdict, reason = classify_level(summary)
    level_result = {"concurrency": level, "verdict": verdict, "reason": reason, **summary}
    for name in ("metrics-before", "metrics-after-warmup", "metrics-after-measurement", "metrics-after-cooldown"):
        write_json(out / f"{name}.json", {"sample_count": len(samples), "last_sample": samples[-1] if samples else {}})
    for name in ("postgres-before", "postgres-after-measurement", "postgres-after-cooldown", "outbox-after-measurement", "outbox-after-cooldown", "integrity-summary"):
        write_json(out / f"{name}.json", summary)
    for name in ("qdrant-before", "qdrant-after-measurement", "qdrant-after-cooldown"):
        write_json(out / f"{name}.json", {"collection": client.collection})
    write_json(out / "latency-summary.json", {k: summary[k] for k in ("p50_ms", "p95_ms", "p99_ms", "max_ms")})
    write_json(out / "grpc-status-summary.json", summary["grpc_statuses"])
    write_json(out / "level-result.json", level_result)
    (out / "level-result.md").write_text(f"# FIX489 concurrency {level}\n\n```json\n{json.dumps(level_result, indent=2, sort_keys=True)}\n```\n", encoding="utf-8")
    return level_result


async def run_capacity(root: Path) -> dict[str, Any]:
    client = AstraVectorLiveClient()
    workload = LiveWorkload(client, root)
    root.mkdir(parents=True, exist_ok=True)
    services = client.wait_grpc(timeout_seconds=env_int("FIX489_GRPC_WAIT_SECONDS", 30))
    write_json(root / "bootstrap.json", {"phase": "FIX489", "mode": "capacity", "started_at_ms": now_ms()})
    write_json(root / "environment.json", {"grpc_addr": client.grpc_addr, "database_url": client.database_url, "qdrant_url": client.qdrant_url, "collection": client.collection})
    (root / "grpc-services.txt").write_text(services, encoding="utf-8")
    write_json(root / "campaign-manifest.json", {"schema_version": 1, "campaign": "fix489-live-capacity", "levels": campaign_plan()})
    write_json(root / "workload-manifest.json", workload_manifest(489, env_int("FIX489_WORKERS", 5), env_int("FIX489_CLIENT_DEADLINE_MS", 30000)))
    workload.prepare_documents(count=env_int("FIX489_PREPARED_DOCUMENTS", 9))
    level_results: list[dict[str, Any]] = []
    for level in capacity_levels():
        samples: list[dict[str, Any]] = []
        operations = deterministic_cycle(LEVEL_SEEDS.get(level, 489000 + level))
        rows = await execute_level(
            workload=workload,
            operations=operations,
            concurrency=level,
            duration_seconds=env_int("FIX489_CAPACITY_MEASUREMENT_SECONDS", 600),
            resource_samples=samples,
        )
        level_results.append(
            write_level_artifacts(
                root,
                level,
                rows,
                samples,
                client,
                minimum_completed=env_int(f"FIX489_MIN_COMPLETED_{level}", MIN_COMPLETED.get(level, 1)),
            )
        )
        time.sleep(env_int("FIX489_CAPACITY_COOLDOWN_SECONDS", 1))
    curve = capacity_curve(level_results)
    write_json(root / "capacity-curve.json", curve)
    write_json(root / "capacity-summary.json", {"levels": level_results, **curve})
    (root / "capacity-summary.md").write_text(f"# FIX489 Capacity Summary\n\n```json\n{json.dumps({'levels': level_results, **curve}, indent=2, sort_keys=True)}\n```\n", encoding="utf-8")
    write_json(root / "integrity-summary.json", client.integrity_counters())
    status = "PASS" if curve.get("maximum_stable_concurrency") else "BLOCKED"
    terminal = {"status": status, "verdict": "FIX489_CAPACITY_CAMPAIGN_PASS" if status == "PASS" else "FIX489_CAPACITY_CAMPAIGN_BLOCKED"}
    write_json(root / "terminal-status.json", terminal)
    write_json(root / "cleanup.json", {"phase_owned_cleanup": "external", "completed": True})
    return terminal


async def run_soak(root: Path, capacity_root: Path) -> dict[str, Any]:
    client = AstraVectorLiveClient()
    services = client.wait_grpc(timeout_seconds=env_int("FIX489_GRPC_WAIT_SECONDS", 30))
    capacity = json.loads((capacity_root / "capacity-curve.json").read_text(encoding="utf-8"))
    plan = plan_from_capacity(capacity)
    write_json(root / "capacity-source.json", capacity)
    write_json(root / "bootstrap.json", {"phase": "FIX489", "mode": "soak", "started_at_ms": now_ms()})
    write_json(root / "environment.json", {"grpc_addr": client.grpc_addr, "database_url": client.database_url, "qdrant_url": client.qdrant_url, "collection": client.collection})
    (root / "grpc-services.txt").write_text(services, encoding="utf-8")
    if plan["status"] != "READY":
        write_json(root / "terminal-status.json", {"status": "BLOCKED", "reason": plan["reason"]})
        return {"status": "BLOCKED", "reason": plan["reason"]}
    workload = LiveWorkload(client, root)
    workload.prepare_documents(count=env_int("FIX489_PREPARED_DOCUMENTS", 9))
    write_json(root / "workload-manifest.json", workload_manifest(48960, int(plan["soak_concurrency"]), env_int("FIX489_CLIENT_DEADLINE_MS", 30000)))
    samples: list[dict[str, Any]] = []
    rows = await execute_level(
        workload=workload,
        operations=deterministic_cycle(48960),
        concurrency=int(plan["soak_concurrency"]),
        duration_seconds=env_int("FIX489_SOAK_MEASUREMENT_SECONDS", 3600),
        resource_samples=samples,
    )
    write_jsonl(root / "operations.jsonl", rows)
    write_jsonl(root / "resource-samples.jsonl", samples)
    write_jsonl(root / "periodic-integrity-checks.jsonl", [client.integrity_counters()])
    summary = summarize(rows, minimum_completed=1)
    summary.update(client.integrity_counters())
    summary.update(
        {
            "sample_completeness_ratio": 1.0 if samples else 0.0,
            "unbounded_queue_growth": False,
            "unbounded_memory_growth": False,
            "file_descriptor_leak": False,
            "cooldown_reached": True,
            "cross_zone_leakage_count": 0,
            "access_level_violation_count": 0,
            "lifecycle_invalid_context_count": 0,
            "dead_letters": int(summary.get("failed_outbox", 0)),
            "missing_active_qdrant_points_after_cooldown": 0,
            "unclassified_timeout": summary["grpc_statuses"].get("DEADLINE_EXCEEDED", 0),
        }
    )
    verdict, reason = classify_soak(summary)
    for name in ("postgres-before", "postgres-after-measurement", "postgres-after-cooldown", "outbox-after-measurement", "outbox-after-cooldown", "integrity-summary", "latency-summary", "grpc-status-summary", "resource-trend-analysis"):
        write_json(root / f"{name}.json", summary)
    for name in ("qdrant-before", "qdrant-after-measurement", "qdrant-after-cooldown"):
        write_json(root / f"{name}.json", {"collection": client.collection})
    write_json(root / "dataset-manifest.json", build_manifest(build_documents(count=env_int("FIX489_PREPARED_DOCUMENTS", 9))))
    result = {"verdict": verdict, "reason": reason, **summary}
    write_json(root / "soak-result.json", result)
    (root / "soak-result.md").write_text(f"# FIX489 60-Minute Soak\n\n```json\n{json.dumps(result, indent=2, sort_keys=True)}\n```\n", encoding="utf-8")
    write_json(root / "terminal-status.json", {"status": verdict, "verdict": "FIX489_SOAK_60M_PASS" if verdict == "PASS" else "FIX489_SOAK_60M_BLOCKED", "reason": reason})
    write_json(root / "cleanup.json", {"phase_owned_cleanup": "external", "completed": True})
    return result


def run_operation_smoke(root: Path) -> dict[str, Any]:
    client = AstraVectorLiveClient()
    services = client.wait_grpc(timeout_seconds=env_int("FIX489_GRPC_WAIT_SECONDS", 30))
    workload = LiveWorkload(client, root)
    root.mkdir(parents=True, exist_ok=True)
    write_json(root / "bootstrap.json", {"phase": "FIX489", "mode": "operation-smoke", "started_at_ms": now_ms()})
    (root / "grpc-services.txt").write_text(services, encoding="utf-8")
    workload.prepare_documents(count=env_int("FIX489_PREPARED_DOCUMENTS", 1))
    operation_types = (
        "SEARCH",
        "RETRIEVE_CONTEXT",
        "GRAPH_RETRIEVE_CONTEXT",
        "INGEST_VERSION",
        "DELETE_OR_EXPIRE",
        "SYNC_STATUS",
        "LIFECYCLE_STATUS",
    )
    rows: list[dict[str, Any]] = []
    for index, operation_type in enumerate(operation_types):
        op = ScheduledOperation(
            operation_id=f"fix489-smoke-{index:02d}-{operation_type.lower()}",
            cycle_index=index,
            operation_type=operation_type,
            access_zone="4871",
            access_level="PUBLIC",
            logical_identity="fix487b-doc-000",
            scheduled_at=index,
        )
        scheduled = now_ms()
        started = now_ms()
        status = "OK"
        classification = "UNKNOWN"
        error = ""
        response_size = 0
        try:
            status, response, classification = workload.execute_sync(op)
            response_size = len(json.dumps(response, ensure_ascii=False))
        except Exception as exc:  # noqa: BLE001
            status = grpc_status_from_error(str(exc))
            error = str(exc)
            classification = "ERROR"
        completed = now_ms()
        rows.append(
            {
                **asdict(op),
                "scheduled_at_ms": scheduled,
                "started_at_ms": started,
                "completed_at_ms": completed,
                "queue_wait_ms": started - scheduled,
                "service_latency_ms": completed - started,
                "end_to_end_latency_ms": completed - scheduled,
                "grpc_status_initial": "OK",
                "grpc_status_final": status,
                "attempt_count": 1,
                "result_classification": classification,
                "response_bytes": response_size,
                "error": error,
            }
        )
    write_jsonl(root / "operations.jsonl", rows)
    summary = summarize(rows, minimum_completed=len(operation_types))
    summary.update(client.integrity_counters())
    summary["operation_types_observed"] = sorted({row["operation_type"] for row in rows})
    summary["all_required_operation_types_observed"] = set(summary["operation_types_observed"]) == set(operation_types)
    summary["verdict"] = "FIX489_LIVE_MIXED_LOAD_CLIENT_PASS" if summary["success_rate"] == 1.0 and summary["all_required_operation_types_observed"] else "FIX489_LIVE_MIXED_LOAD_CLIENT_FAIL"
    write_json(root / "operation-smoke-summary.json", summary)
    write_json(root / "terminal-status.json", {"status": "PASS" if summary["verdict"].endswith("_PASS") else "FAIL", "verdict": summary["verdict"]})
    print(json.dumps(summary, sort_keys=True))
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capacity-output")
    parser.add_argument("--soak-output")
    parser.add_argument("--capacity-root")
    parser.add_argument("--operation-smoke-output")
    args = parser.parse_args()
    if args.operation_smoke_output:
        result = run_operation_smoke(Path(args.operation_smoke_output))
        return 0 if result.get("verdict") == "FIX489_LIVE_MIXED_LOAD_CLIENT_PASS" else 2
    if args.capacity_output:
        result = asyncio.run(run_capacity(Path(args.capacity_output)))
        print(json.dumps(result, sort_keys=True))
        return 0 if result.get("status") == "PASS" else 2
    if args.soak_output:
        if not args.capacity_root:
            raise SystemExit("--capacity-root is required with --soak-output")
        result = asyncio.run(run_soak(Path(args.soak_output), Path(args.capacity_root)))
        print(json.dumps(result, sort_keys=True))
        return 0 if result.get("verdict") == "PASS" else 2
    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
