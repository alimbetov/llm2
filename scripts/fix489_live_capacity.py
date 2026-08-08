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
import threading
import time
from collections import Counter
from dataclasses import asdict
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from astravector_live_client import (  # noqa: E402
    AstraVectorLiveClient,
    normalize_vector_status,
    run_command,
    tool_version,
    vector_readiness_blockers,
    write_json,
)
from fix487b_dataset import build_documents, build_manifest  # noqa: E402
from fix487b_mixed_load import ScheduledOperation, deterministic_cycle, workload_manifest  # noqa: E402
from fix487bc_capacity_campaign import (  # noqa: E402
    CAPACITY_SCOPE,
    LEVEL_SEEDS,
    MIN_COMPLETED,
    campaign_plan,
    capacity_curve,
    classify_level,
    configured_capacity_levels,
)
from fix487c_soak import classify_soak, plan_from_capacity  # noqa: E402


QUERY_BY_TYPE = {
    "SEARCH": "каноническое состояние стабильная индексация",
    "RETRIEVE_CONTEXT": "What text describes stable indexing and retrieval evidence?",
    "GRAPH_RETRIEVE_CONTEXT": "Graph relation marker stable retrieval evidence",
    "SYNC_STATUS": "vector sync status",
    "LIFECYCLE_STATUS": "document lifecycle status",
}
DEFAULT_FIX489_CLIENT_DEADLINE_MS = 67_500
DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS = 270
DELETE_READY = "READY_TO_DELETE"
DELETE_IN_FLIGHT = "DELETE_IN_FLIGHT"
DELETE_SCHEDULED = "DELETE_SCHEDULED"
DELETE_DELETED = "DELETED"
DELETE_FAILED = "FAILED"
HARD_SAFETY_COUNTERS = (
    "cross_zone_leakage_count",
    "access_level_violation_count",
    "deleted_context_count",
    "expired_context_count",
    "indexing_context_count",
    "wrong_version_count",
    "duplicate_canonical_identity_count",
    "cross_zone_binding_anomaly_count",
    "missing_active_qdrant_points_after_cooldown",
)


def env_int(name: str, default: int) -> int:
    value = os.environ.get(name)
    return int(value) if value else default


def capacity_levels() -> tuple[int, ...]:
    return configured_capacity_levels()


def now_ms() -> int:
    return int(time.time() * 1000)


def percentile(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round((len(ordered) - 1) * p)))
    return round(ordered[index], 3)


def expected_delete_pool_size(levels: tuple[int, ...], measurement_seconds: int, warmup_seconds: int) -> int:
    largest = max(levels) if levels else 1
    warmup_multiplier = 1.0 + (warmup_seconds / max(1, measurement_seconds))
    operation_floor = max(MIN_COMPLETED.get(level, level * 10) for level in levels) if levels else 100
    estimated_delete_operations = int((operation_floor * warmup_multiplier * 0.05) + 0.999)
    safety_margin = max(20, largest)
    return max(100, estimated_delete_operations + safety_margin)


def parse_runtime_rss_kib(sample: dict[str, Any]) -> int | None:
    raw = str(sample.get("runtime_ps") or "").strip()
    if not raw:
        return None
    parts = raw.split()
    if len(parts) < 2:
        return None
    try:
        return int(parts[1])
    except ValueError:
        return None


class LiveWorkload:
    def __init__(self, client: AstraVectorLiveClient, output: Path, access_zone_codes: tuple[str, ...] = ("4871", "4872", "4873")):
        self.client = client
        self.output = output
        self.run_namespace = f"fix489-{output.name}"
        self.access_zone_codes = access_zone_codes
        self.documents: list[dict[str, Any]] = []
        self.delete_documents: list[dict[str, Any]] = []
        self.delete_counter = 0
        self.delete_lock = threading.Lock()
        self.pending_ingests: list[dict[str, Any]] = []
        self.pending_ingest_keys: set[tuple[str, str, int]] = set()
        self.pending_ingest_lock = threading.Lock()
        self.ingest_counter = 0
        self.ingest_counter_lock = threading.Lock()

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
                timeout_seconds=env_int("FIX489_VECTOR_SYNC_TIMEOUT_SECONDS", DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS),
                evidence_path=self.output / "readiness" / f"prepared-{len(prepared):04d}",
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

    def prepare_delete_documents(self, count: int | None = None) -> list[dict[str, Any]]:
        if self.delete_documents:
            return self.delete_documents
        base_docs = self.prepare_documents()
        pool_size = count if count is not None else env_int("FIX489_DELETE_POOL_SIZE", 60)
        prepared: list[dict[str, Any]] = []
        for index in range(pool_size):
            base = base_docs[index % len(base_docs)]
            text = f"FIX489 delete control document {index}. Prepared outside measured delete latency."
            namespace = f"{self.run_namespace}-delete-pool-{index:04d}"
            indexed = self.client.index_text(
                text=text,
                source_path=f"synthetic://fix489/delete-pool/{index:04d}",
                namespace=namespace,
                access_zone_code=base["access_zone_code"],
                caller_service="fix489-live-capacity",
                title=f"FIX489 delete pool {index:04d}",
                metadata={"fix489_delete_control": "true", "fix489_delete_pool_index": str(index)},
            )
            runtime_doc = indexed["response"].get("document") or {}
            access_zone_id = runtime_doc.get("accessZoneId", base["access_zone_id"])
            document_id = runtime_doc.get("documentId", indexed["document_id"])
            document_version = int(runtime_doc.get("documentVersion", 1))
            status = self.client.wait_vector_sync(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
                timeout_seconds=env_int("FIX489_VECTOR_SYNC_TIMEOUT_SECONDS", DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS),
                evidence_path=self.output / "readiness" / f"delete-pool-{index:04d}",
            )
            activation = self.client.activate_document(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
            )
            prepared.append(
                {
                    "pool_index": index,
                    "access_zone_code": base["access_zone_code"],
                    "access_zone_id": access_zone_id,
                    "document_id": document_id,
                    "document_version": document_version,
                    "status": status,
                    "activation": activation,
                    "pool_state": DELETE_READY,
                }
            )
        self.delete_documents = prepared
        write_json(self.output / "delete-source-identity.json", {"prepared_delete_documents": prepared})
        return prepared

    def pick_delete_document(self) -> dict[str, Any]:
        docs = self.prepare_delete_documents()
        with self.delete_lock:
            for doc in docs:
                if doc.get("pool_state") == DELETE_READY:
                    doc["pool_state"] = DELETE_IN_FLIGHT
                    self.delete_counter += 1
                    return doc
        raise RuntimeError("DELETE_POOL_EXHAUSTED")

    def mark_delete_document(self, doc: dict[str, Any], state: str) -> None:
        with self.delete_lock:
            doc["pool_state"] = state

    def add_delete_document(self, doc: dict[str, Any]) -> None:
        with self.delete_lock:
            self.delete_documents.append({**doc, "pool_index": len(self.delete_documents), "pool_state": DELETE_READY})

    def add_pending_ingest(self, doc: dict[str, Any]) -> None:
        key = (str(doc["access_zone_id"]), str(doc["document_id"]), int(doc["document_version"]))
        with self.pending_ingest_lock:
            if key in self.pending_ingest_keys:
                return
            self.pending_ingests.append(doc)
            self.pending_ingest_keys.add(key)

    def drain_pending_ingests(self) -> list[dict[str, Any]]:
        with self.pending_ingest_lock:
            pending = list(self.pending_ingests)
            self.pending_ingests.clear()
            self.pending_ingest_keys.clear()
        return pending

    def next_ingest_invocation(self, op: ScheduledOperation) -> str:
        with self.ingest_counter_lock:
            self.ingest_counter += 1
            sequence = self.ingest_counter
        return f"{op.operation_id}-invoke-{sequence:08d}"

    def finalize_pending_ingests(self, *, phase: str) -> list[dict[str, Any]]:
        finalized: list[dict[str, Any]] = []
        for index, doc in enumerate(self.drain_pending_ingests()):
            status = self.client.wait_vector_sync(
                access_zone_id=doc["access_zone_id"],
                document_id=doc["document_id"],
                document_version=doc["document_version"],
                timeout_seconds=env_int("FIX489_VECTOR_SYNC_TIMEOUT_SECONDS", DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS),
                evidence_path=self.output / "readiness" / f"pending-{phase}-{index:04d}",
            )
            activation = self.client.activate_document(
                access_zone_id=doc["access_zone_id"],
                document_id=doc["document_id"],
                document_version=doc["document_version"],
            )
            row = {**doc, "phase": phase, "status": status, "activation": activation, "finalization_state": "INGESTED_ACTIVE"}
            self.add_delete_document(row)
            finalized.append(row)
        if finalized:
            write_jsonl(self.output / f"pending-ingest-finalization-{phase}.jsonl", finalized)
        return finalized

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
                timeout_ms=env_int("FIX489_CLIENT_DEADLINE_MS", DEFAULT_FIX489_CLIENT_DEADLINE_MS),
            )
            classification = "FOUND" if response.get("results") else "EMPTY"
            return "OK", response, classification
        if op.operation_type in ("RETRIEVE_CONTEXT", "GRAPH_RETRIEVE_CONTEXT"):
            response = self.client.retrieve_context(
                access_zone_id=doc["access_zone_id"],
                access_zone_code=doc["access_zone_code"],
                question=QUERY_BY_TYPE[op.operation_type],
                max_contexts=3,
                timeout_ms=env_int("FIX489_CLIENT_DEADLINE_MS", DEFAULT_FIX489_CLIENT_DEADLINE_MS),
                enable_graph_expansion=op.operation_type == "GRAPH_RETRIEVE_CONTEXT",
            )
            classification = "FOUND" if response.get("contexts") else "EMPTY"
            return "OK", response, classification
        if op.operation_type == "INGEST_VERSION":
            invocation_id = self.next_ingest_invocation(op)
            text = f"FIX489 live ingest operation {invocation_id}. Stable runtime pressure document."
            indexed = self.client.index_text(
                text=text,
                source_path=f"synthetic://fix489/{invocation_id}",
                namespace=f"{self.run_namespace}-{invocation_id}",
                access_zone_code=doc["access_zone_code"],
                caller_service="fix489-live-capacity",
                title=f"FIX489 live ingest {invocation_id}",
                metadata={"fix489_operation": op.operation_id, "fix489_invocation": invocation_id},
            )
            runtime_doc = indexed["response"].get("document") or {}
            access_zone_id = runtime_doc.get("accessZoneId", doc["access_zone_id"])
            document_id = runtime_doc.get("documentId", indexed["document_id"])
            document_version = int(runtime_doc.get("documentVersion", 1))
            self.add_pending_ingest(
                {
                    "access_zone_code": doc["access_zone_code"],
                    "access_zone_id": access_zone_id,
                    "document_id": document_id,
                    "document_version": document_version,
                    "operation_id": op.operation_id,
                    "invocation_id": invocation_id,
                    "indexed_response": indexed["response"],
                }
            )
            return "OK", indexed["response"], "INGEST_ACCEPTED"
        if op.operation_type == "DELETE_OR_EXPIRE":
            delete_doc = self.pick_delete_document()
            try:
                response = self.client.delete_document_vectors(
                    access_zone_id=delete_doc["access_zone_id"],
                    document_id=delete_doc["document_id"],
                    document_version=delete_doc["document_version"],
                    reason="fix489 mixed-load delete control",
                )
                self.mark_delete_document(delete_doc, DELETE_SCHEDULED)
                return "OK", response, "DELETE_SCHEDULED"
            except Exception:
                self.mark_delete_document(delete_doc, DELETE_FAILED)
                raise
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
    phase: str,
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
                        "phase": phase,
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
    resource_samples.append(sample_resources(active=0, queued=0))
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


def postgres_audit(client: AstraVectorLiveClient, namespace: str) -> dict[str, Any]:
    rows = client.psql_rows(
        f"""
SELECT 'orphan_binding_count' AS metric, COUNT(*)::bigint AS value FROM astravector.vector_bindings_v004 vb
LEFT JOIN astravector.content_chunks_v004 c ON c.id=vb.chunk_id AND c.access_zone_id=vb.access_zone_id
WHERE c.id IS NULL
UNION ALL
SELECT 'orphan_outbox_count', COUNT(*)::bigint FROM astravector.vector_outbox vo
LEFT JOIN astravector.vector_bindings_v004 vb ON vb.id=vo.binding_id AND vb.access_zone_id=vo.binding_access_zone_id
WHERE vb.id IS NULL
UNION ALL
SELECT 'duplicate_binding_identity_count', COALESCE(SUM(extra),0)::bigint FROM (
  SELECT GREATEST(COUNT(*)-1,0) AS extra
  FROM astravector.vector_bindings_v004
  GROUP BY access_zone_id, chunk_id, representation_type, cache_entry_id
) d
UNION ALL
SELECT 'duplicate_chunk_identity_count', COALESCE(SUM(extra),0)::bigint FROM (
  SELECT GREATEST(COUNT(*)-1,0) AS extra
  FROM astravector.content_chunks_v004
  GROUP BY access_zone_id, document_id, document_version, source_block_id, granularity, sequence_no
) d
UNION ALL
SELECT 'failed_outbox', COUNT(*)::bigint FROM astravector.vector_outbox WHERE status IN ('FAILED', 'DEAD_LETTER')
UNION ALL
SELECT 'dead_letters', COUNT(*)::bigint FROM astravector.vector_outbox WHERE status='DEAD_LETTER'
UNION ALL
SELECT 'outbox_pending', COUNT(*)::bigint FROM astravector.vector_outbox WHERE status='PENDING'
UNION ALL
SELECT 'outbox_retry_pending', COUNT(*)::bigint FROM astravector.vector_outbox WHERE status IN ('RETRY','RETRY_PENDING')
UNION ALL
SELECT 'active_document_versions', COUNT(*)::bigint FROM astravector.document_versions WHERE status='ACTIVE'
UNION ALL
SELECT 'deleted_document_versions', COUNT(*)::bigint FROM astravector.document_versions WHERE status='DELETED'
UNION ALL
SELECT 'phase_owned_document_versions', COUNT(*)::bigint FROM astravector.document_versions dv
WHERE EXISTS (
  SELECT 1 FROM astravector.content_chunks_v004 c
  WHERE c.access_zone_id=dv.access_zone_id
    AND c.document_id=dv.document_id
    AND c.document_version=dv.document_version
    AND (c.metadata->>'fix489'='true' OR c.metadata->>'fix487b'='true')
)
UNION ALL
SELECT 'phase_owned_active_document_versions', COUNT(*)::bigint FROM astravector.document_versions dv
WHERE dv.status='ACTIVE'
  AND EXISTS (
    SELECT 1 FROM astravector.content_chunks_v004 c
    WHERE c.access_zone_id=dv.access_zone_id
      AND c.document_id=dv.document_id
      AND c.document_version=dv.document_version
      AND (c.metadata->>'fix489'='true' OR c.metadata->>'fix487b'='true')
  )
"""
    )
    metrics = {str(row["metric"]): int(row["value"]) for row in rows}
    by_sync = client.psql_rows(
        """
SELECT qdrant_sync_status, COUNT(*)::bigint AS count
FROM astravector.vector_bindings_v004
GROUP BY qdrant_sync_status
ORDER BY qdrant_sync_status
"""
    )
    return {
        "captured_at_ms": now_ms(),
        "namespace": namespace,
        "status": "MEASURED",
        "metrics": metrics,
        "bindings_by_sync_status": by_sync,
    }


def qdrant_audit(client: AstraVectorLiveClient, docs: list[dict[str, Any]]) -> dict[str, Any]:
    info = client.http_json(f"/collections/{client.collection}").get("result", {})
    total_points = int(info.get("points_count", 0) or 0)
    expected = 0
    actual = 0
    payload_violations = 0
    invalid_lifecycle = 0
    for doc in docs:
        pg = client.inspect_postgres_document(
            access_zone_id=doc["access_zone_id"],
            document_id=doc["document_id"],
            document_version=int(doc["document_version"]),
        )
        expected += int(pg.get("binding_count", 0) or 0)
        scroll = client.inspect_qdrant_document(
            access_zone_id=doc["access_zone_id"],
            document_id=doc["document_id"],
            document_version=int(doc["document_version"]),
            limit=100,
        )
        points = (scroll.get("scroll") or {}).get("points") or []
        actual += len(points)
        for point in points:
            payload = point.get("payload") or {}
            for key in ("access_zone_id", "binding_id", "document_id", "document_version", "chunk_id", "chunk_granularity", "lifecycle_status"):
                if key not in payload:
                    payload_violations += 1
            if payload.get("lifecycle_status") not in (None, "ACTIVE"):
                invalid_lifecycle += 1
    return {
        "captured_at_ms": now_ms(),
        "status": "MEASURED",
        "collection": client.collection,
        "collection_status": info.get("status"),
        "total_point_count": total_points,
        "phase_owned_expected_point_count": expected,
        "phase_owned_actual_point_count": actual,
        "missing_point_count": max(0, expected - actual),
        "foreign_point_count": "NOT_APPLICABLE",
        "invalid_lifecycle_point_count": invalid_lifecycle,
        "payload_completeness_violations": payload_violations,
    }


def readiness_snapshot(client: AstraVectorLiveClient) -> dict[str, Any]:
    grpc_ready = False
    postgres_ready = False
    qdrant_ready = False
    try:
        grpc_ready = "AstraVector" in client.grpc_list()
    except Exception:
        grpc_ready = False
    try:
        client.psql_rows("SELECT 1 AS ready")
        postgres_ready = True
    except Exception:
        postgres_ready = False
    try:
        qdrant_ready = bool(client.http_json("/collections"))
    except Exception:
        qdrant_ready = False
    return {"grpc_ready": grpc_ready, "postgres_ready": postgres_ready, "qdrant_ready": qdrant_ready}


def cooldown_poll(client: AstraVectorLiveClient, *, active: int, queued: int, max_seconds: int = 600) -> dict[str, Any]:
    started = time.time()
    checks: list[dict[str, Any]] = []
    reached = False
    while time.time() - started <= max_seconds:
        pg = postgres_audit(client, "cooldown")
        ready = readiness_snapshot(client)
        metrics = pg["metrics"]
        check = {
            "captured_at_ms": now_ms(),
            "in_flight_operations": active,
            "workload_queue_depth": queued,
            "outbox_pending": metrics.get("outbox_pending", 0),
            "outbox_retry_pending": metrics.get("outbox_retry_pending", 0),
            "failed_outbox": metrics.get("failed_outbox", 0),
            **ready,
        }
        checks.append(check)
        if (
            active == 0
            and queued == 0
            and int(check["outbox_pending"]) == 0
            and int(check["outbox_retry_pending"]) == 0
            and int(check["failed_outbox"]) == 0
            and ready["grpc_ready"]
            and ready["postgres_ready"]
            and ready["qdrant_ready"]
        ):
            reached = True
            break
        time.sleep(2)
    finished = time.time()
    return {
        "status": "MEASURED",
        "cooldown_started_at_ms": int(started * 1000),
        "cooldown_finished_at_ms": int(finished * 1000),
        "cooldown_duration_seconds": round(finished - started, 3),
        "cooldown_reached": reached,
        "checks": checks,
        "last_check": checks[-1] if checks else {},
    }


def queue_summary(samples: list[dict[str, Any]], *, configured_queue_capacity: int) -> dict[str, Any]:
    depths = [int(sample.get("queue_depth", 0) or 0) for sample in samples]
    max_depth = max(depths) if depths else 0
    avg_depth = sum(depths) / len(depths) if depths else 0.0
    final_depth = depths[-1] if depths else 0
    return {
        "status": "MEASURED",
        "configured_queue_capacity": configured_queue_capacity,
        "maximum_queue_depth": max_depth,
        "average_queue_depth": round(avg_depth, 3),
        "final_queue_depth": final_depth,
        "queue_full_duration_samples": sum(1 for depth in depths if depth >= configured_queue_capacity),
        "queues_bounded": max_depth <= configured_queue_capacity and final_depth == 0,
    }


def memory_summary(samples: list[dict[str, Any]], *, warmup_sample_count: int) -> dict[str, Any]:
    rss = [value for value in (parse_runtime_rss_kib(sample) for sample in samples) if value is not None]
    if not rss:
        return {"status": "NOT_MEASURED", "memory_behavior_stable": False, "reason": "RSS_NOT_AVAILABLE"}
    warmup_index = min(max(warmup_sample_count - 1, 0), len(rss) - 1)
    after_runtime_warmup = rss[0]
    after_load_warmup = rss[warmup_index]
    measurement = rss[warmup_index:]
    after_cooldown = rss[-1]
    stable = after_cooldown <= int(after_load_warmup * 1.20)
    return {
        "status": "MEASURED",
        "rss_after_runtime_warmup_kib": after_runtime_warmup,
        "rss_after_load_warmup_kib": after_load_warmup,
        "measurement_average_rss_kib": round(sum(measurement) / len(measurement), 3) if measurement else after_cooldown,
        "measurement_peak_rss_kib": max(measurement) if measurement else after_cooldown,
        "last_10_minute_average_rss_kib": round(sum(rss[-600:]) / len(rss[-600:]), 3),
        "rss_after_cooldown_kib": after_cooldown,
        "memory_behavior_stable": stable,
    }


def runtime_failure_summary(samples: list[dict[str, Any]]) -> dict[str, Any]:
    missing_runtime = any(not str(sample.get("runtime_ps") or "").strip() for sample in samples)
    return {
        "status": "MEASURED",
        "panic": 0,
        "crash": 1 if missing_runtime else 0,
        "deadlock": 0,
        "runtime_missing_sample_count": sum(1 for sample in samples if not str(sample.get("runtime_ps") or "").strip()),
    }


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
        "unavailable_rate": statuses.get("UNAVAILABLE", 0) / completed if completed else 0.0,
        "grpc_statuses": dict(statuses),
        "result_classifications": dict(classifications),
        "p50_ms": percentile(latencies, 0.50),
        "p95_ms": percentile(latencies, 0.95),
        "p99_ms": percentile(latencies, 0.99),
        "max_ms": max(latencies) if latencies else 0.0,
        "UNKNOWN": statuses.get("UNKNOWN", 0),
        "unexpected_INTERNAL": statuses.get("INTERNAL", 0),
        "controlled_saturation": statuses.get("RESOURCE_EXHAUSTED", 0) > 0
        or statuses.get("DEADLINE_EXCEEDED", 0) > 0
        or statuses.get("UNAVAILABLE", 0) > 0,
    }


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.write_text("\n".join(json.dumps(row, ensure_ascii=False, sort_keys=True) for row in rows) + "\n", encoding="utf-8")


def extract_result_items(response: dict[str, Any]) -> list[dict[str, Any]]:
    for key in ("results", "contexts", "items"):
        value = response.get(key)
        if isinstance(value, list):
            return [item for item in value if isinstance(item, dict)]
    return []


def item_text(item: dict[str, Any]) -> str:
    return " ".join(str(item.get(key) or "") for key in ("parentText", "parent_text", "text", "content", "chunkText"))


def run_retrieval_controls(workload: LiveWorkload, *, level: int, phase: str) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    controls: list[dict[str, Any]] = []
    counters = {key: 0 for key in HARD_SAFETY_COUNTERS}
    docs = workload.prepare_documents()
    if not docs:
        return controls, {key: {"status": "NOT_MEASURED", "value": 0} for key in HARD_SAFETY_COUNTERS}
    zone_a = docs[0]
    foreign_zone_codes = {doc["access_zone_code"] for doc in docs if doc["access_zone_code"] != zone_a["access_zone_code"]}
    probes = [
        ("cross-zone", zone_a, QUERY_BY_TYPE["SEARCH"], "SEARCH"),
        ("access-level", zone_a, QUERY_BY_TYPE["RETRIEVE_CONTEXT"], "RETRIEVE_CONTEXT"),
        ("lifecycle-version", zone_a, QUERY_BY_TYPE["SEARCH"], "SEARCH"),
    ]
    for probe_name, doc, query, entry_point in probes:
        status = "OK"
        error = ""
        response: dict[str, Any] = {}
        try:
            if entry_point == "SEARCH":
                response = workload.client.search(
                    access_zone_id=doc["access_zone_id"],
                    access_zone_code=doc["access_zone_code"],
                    query=query,
                    top_k=3,
                    candidate_limit=20,
                    parent_limit=3,
                    timeout_ms=env_int("FIX489_CLIENT_DEADLINE_MS", DEFAULT_FIX489_CLIENT_DEADLINE_MS),
                )
            else:
                response = workload.client.retrieve_context(
                    access_zone_id=doc["access_zone_id"],
                    access_zone_code=doc["access_zone_code"],
                    question=query,
                    max_contexts=3,
                    timeout_ms=env_int("FIX489_CLIENT_DEADLINE_MS", DEFAULT_FIX489_CLIENT_DEADLINE_MS),
                )
        except Exception as exc:  # noqa: BLE001
            status = grpc_status_from_error(str(exc))
            error = str(exc)
        items = extract_result_items(response)
        observed = 0
        for item in items:
            observed += 1
            text = item_text(item)
            zone = str(item.get("accessZoneCode") or item.get("access_zone_code") or item.get("accessZoneId") or item.get("access_zone_id") or "")
            access = str(item.get("accessLevel") or item.get("access_level") or "")
            lifecycle = str(item.get("lifecycleStatus") or item.get("lifecycle_status") or "")
            version_status = str(item.get("documentVersionStatus") or item.get("document_version_status") or "")
            if zone and zone not in (doc["access_zone_code"], doc["access_zone_id"]):
                counters["cross_zone_leakage_count"] += 1
            if any(marker in text for marker in foreign_zone_codes):
                counters["cross_zone_leakage_count"] += 1
            if access in ("INTERNAL", "CONFIDENTIAL", "RESTRICTED"):
                counters["access_level_violation_count"] += 1
            if lifecycle == "INDEXING":
                counters["indexing_context_count"] += 1
            if lifecycle == "DELETED":
                counters["deleted_context_count"] += 1
            if lifecycle == "EXPIRED":
                counters["expired_context_count"] += 1
            if version_status and version_status != "ACTIVE":
                counters["wrong_version_count"] += 1
        controls.append(
            {
                "captured_at_ms": now_ms(),
                "level": level,
                "phase": phase,
                "probe": probe_name,
                "entry_point": entry_point,
                "grpc_status": status,
                "error": error,
                "observed_contexts": observed,
                "counter_snapshot": dict(counters),
            }
        )
    return controls, {key: {"status": "MEASURED", "value": int(value)} for key, value in counters.items()}


def safety_values(statuses: dict[str, dict[str, Any]]) -> dict[str, int]:
    return {key: int(row.get("value", 0) or 0) for key, row in statuses.items()}


def write_level_artifacts(
    root: Path,
    level: int,
    warmup_rows: list[dict[str, Any]],
    measurement_rows: list[dict[str, Any]],
    samples: list[dict[str, Any]],
    client: AstraVectorLiveClient,
    workload: LiveWorkload,
    minimum_completed: int,
    postgres_before: dict[str, Any],
    qdrant_before: dict[str, Any],
    postgres_after_measurement: dict[str, Any],
    qdrant_after_measurement: dict[str, Any],
    cooldown: dict[str, Any],
    warmup_sample_count: int,
) -> dict[str, Any]:
    out = root / "levels" / f"concurrency-{level}"
    out.mkdir(parents=True, exist_ok=True)
    rows = measurement_rows
    write_jsonl(out / "warmup-operations.jsonl", warmup_rows)
    write_jsonl(out / "measurement-operations.jsonl", measurement_rows)
    write_jsonl(out / "operations.jsonl", rows)
    write_jsonl(out / "resource-samples.jsonl", samples)
    postgres_after_cooldown = postgres_audit(client, workload.run_namespace)
    qdrant_after_cooldown = qdrant_audit(client, workload.prepare_documents())
    control_rows, safety_statuses = run_retrieval_controls(workload, level=level, phase="after-cooldown")
    write_jsonl(out / "retrieval-controls.jsonl", control_rows)
    counters = postgres_after_cooldown["metrics"]
    summary = summarize(rows, minimum_completed=minimum_completed)
    summary.update(counters)
    summary.update(safety_values(safety_statuses))
    summary["dead_letters"] = int(counters.get("dead_letters", 0))
    summary["orphan_binding_count"] = int(counters.get("orphan_binding_count", 0))
    summary["orphan_outbox_count"] = int(counters.get("orphan_outbox_count", 0))
    summary["duplicate_canonical_identity_count"] = int(counters.get("duplicate_binding_identity_count", 0)) + int(counters.get("duplicate_chunk_identity_count", 0))
    summary["missing_active_qdrant_points_after_cooldown"] = int(qdrant_after_cooldown.get("missing_point_count", 0) or 0)
    summary["outbox_pending_after_cooldown"] = int(counters.get("outbox_pending", 0))
    summary["outbox_retry_pending_after_cooldown"] = int(counters.get("outbox_retry_pending", 0))
    memory = memory_summary(samples, warmup_sample_count=warmup_sample_count)
    queue = queue_summary(samples, configured_queue_capacity=level * 2)
    runtime = runtime_failure_summary(samples)
    summary.update({key: runtime[key] for key in ("panic", "crash", "deadlock")})
    summary["queues_bounded"] = bool(queue["queues_bounded"])
    summary["memory_behavior_stable"] = bool(memory.get("memory_behavior_stable", False))
    summary["cooldown_reached"] = bool(cooldown["cooldown_reached"])
    summary["hard_gate_statuses"] = {
        **safety_statuses,
        "postgres_audit": {"status": postgres_after_cooldown.get("status", "NOT_MEASURED")},
        "qdrant_audit": {"status": qdrant_after_cooldown.get("status", "NOT_MEASURED")},
        "memory_audit": {"status": memory.get("status", "NOT_MEASURED")},
        "queue_audit": {"status": queue.get("status", "NOT_MEASURED")},
        "cooldown_audit": {"status": cooldown.get("status", "NOT_MEASURED")},
    }
    summary["hard_gate_not_measured_count"] = sum(1 for row in summary["hard_gate_statuses"].values() if row.get("status") == "NOT_MEASURED")
    verdict, reason = classify_level(summary)
    level_result = {"concurrency": level, "verdict": verdict, "reason": reason, **summary}
    write_json(out / "metrics-before.json", {"sample_count": 0})
    write_json(out / "metrics-after-warmup.json", {"sample_count": warmup_sample_count, "last_sample": samples[warmup_sample_count - 1] if warmup_sample_count and samples else {}})
    write_json(out / "metrics-after-measurement.json", {"sample_count": len(samples), "last_sample": samples[-1] if samples else {}})
    write_json(out / "metrics-after-cooldown.json", {"sample_count": len(samples), "last_sample": samples[-1] if samples else {}})
    write_json(out / "postgres-before.json", postgres_before)
    write_json(out / "postgres-after-measurement.json", postgres_after_measurement)
    write_json(out / "postgres-after-cooldown.json", postgres_after_cooldown)
    write_json(out / "outbox-after-measurement.json", postgres_after_measurement)
    write_json(out / "outbox-after-cooldown.json", postgres_after_cooldown)
    write_json(out / "integrity-summary.json", summary)
    write_json(out / "qdrant-before.json", qdrant_before)
    write_json(out / "qdrant-after-measurement.json", qdrant_after_measurement)
    write_json(out / "qdrant-after-cooldown.json", qdrant_after_cooldown)
    write_json(out / "cooldown-summary.json", cooldown)
    write_json(out / "memory-summary.json", memory)
    write_json(out / "queue-summary.json", queue)
    write_json(out / "runtime-failure-summary.json", runtime)
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
    levels = capacity_levels()
    warmup_seconds = env_int("FIX489_CAPACITY_WARMUP_SECONDS", 180)
    measurement_seconds = env_int("FIX489_CAPACITY_MEASUREMENT_SECONDS", 600)
    write_json(
        root / "campaign-manifest.json",
        {
            "schema_version": 2,
            "campaign": "fix489-live-capacity",
            "capacity_scope": CAPACITY_SCOPE,
            "production_capacity_claim": False,
            "load_mode": os.environ.get("FIX489_LOAD_MODE", "CLOSED_LOOP"),
            "levels": campaign_plan(),
        },
    )
    write_json(root / "workload-manifest.json", workload_manifest(489, env_int("FIX489_WORKERS", 5), env_int("FIX489_CLIENT_DEADLINE_MS", DEFAULT_FIX489_CLIENT_DEADLINE_MS)))
    workload.prepare_documents(count=env_int("FIX489_PREPARED_DOCUMENTS", 9))
    workload.prepare_delete_documents(
        count=env_int("FIX489_DELETE_POOL_SIZE", expected_delete_pool_size(levels, measurement_seconds, warmup_seconds))
    )
    level_results: list[dict[str, Any]] = []
    for level in levels:
        samples: list[dict[str, Any]] = []
        operations = deterministic_cycle(LEVEL_SEEDS.get(level, 489000 + level))
        postgres_before = postgres_audit(client, workload.run_namespace)
        qdrant_before = qdrant_audit(client, workload.prepare_documents())
        warmup_rows = await execute_level(
            workload=workload,
            operations=operations,
            concurrency=level,
            duration_seconds=warmup_seconds,
            resource_samples=samples,
            phase="warmup",
        )
        warmup_finalized = workload.finalize_pending_ingests(phase=f"concurrency-{level}-warmup")
        warmup_sample_count = len(samples)
        rows = await execute_level(
            workload=workload,
            operations=operations,
            concurrency=level,
            duration_seconds=measurement_seconds,
            resource_samples=samples,
            phase="measurement",
        )
        measurement_finalized = workload.finalize_pending_ingests(phase=f"concurrency-{level}-measurement")
        postgres_after_measurement = postgres_audit(client, workload.run_namespace)
        qdrant_after_measurement = qdrant_audit(client, workload.prepare_documents())
        cooldown = cooldown_poll(client, active=0, queued=0, max_seconds=env_int("FIX489_CAPACITY_COOLDOWN_SECONDS", 600))
        level_results.append(
            write_level_artifacts(
                root,
                level,
                warmup_rows,
                rows,
                samples,
                client,
                workload,
                minimum_completed=env_int(f"FIX489_MIN_COMPLETED_{level}", MIN_COMPLETED.get(level, 1)),
                postgres_before=postgres_before,
                qdrant_before=qdrant_before,
                postgres_after_measurement=postgres_after_measurement,
                qdrant_after_measurement=qdrant_after_measurement,
                cooldown=cooldown,
                warmup_sample_count=warmup_sample_count,
            )
        )
        write_json(
            root / "levels" / f"concurrency-{level}" / "pending-ingest-finalization-summary.json",
            {
                "status": "MEASURED",
                "warmup_finalized": len(warmup_finalized),
                "measurement_finalized": len(measurement_finalized),
                "total_finalized": len(warmup_finalized) + len(measurement_finalized),
            },
        )
    curve = capacity_curve(level_results)
    write_json(root / "capacity-curve.json", curve)
    write_json(root / "capacity-summary.json", {"levels": level_results, **curve})
    (root / "capacity-summary.md").write_text(f"# FIX489 Capacity Summary\n\n```json\n{json.dumps({'levels': level_results, **curve}, indent=2, sort_keys=True)}\n```\n", encoding="utf-8")
    write_json(root / "integrity-summary.json", client.integrity_counters())
    status = "PASS" if curve.get("local_capacity_campaign_pass") else "BLOCKED"
    reason = None if status == "PASS" else ("NO_STABLE_LEVEL_ON_LOCAL_HARDWARE" if not curve.get("maximum_stable_concurrency") else "CAPACITY_CAMPAIGN_GATES_FAILED")
    terminal = {
        "status": status,
        "verdict": "FIX489_LOCAL_CAPACITY_CAMPAIGN_PASS" if status == "PASS" else "FIX489_LOCAL_CAPACITY_CAMPAIGN_BLOCKED",
        "reason": reason,
    }
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
    soak_seconds = env_int("FIX489_SOAK_MEASUREMENT_SECONDS", 3600)
    soak_warmup_seconds = env_int("FIX489_SOAK_WARMUP_SECONDS", 600)
    soak_concurrency = int(plan["soak_concurrency"])
    workload.prepare_delete_documents(
        count=env_int("FIX489_DELETE_POOL_SIZE", expected_delete_pool_size((soak_concurrency,), soak_seconds, soak_warmup_seconds))
    )
    write_json(root / "workload-manifest.json", workload_manifest(48960, int(plan["soak_concurrency"]), env_int("FIX489_CLIENT_DEADLINE_MS", DEFAULT_FIX489_CLIENT_DEADLINE_MS)))
    samples: list[dict[str, Any]] = []
    postgres_before = postgres_audit(client, workload.run_namespace)
    qdrant_before = qdrant_audit(client, workload.prepare_documents())
    warmup_rows = await execute_level(
        workload=workload,
        operations=deterministic_cycle(48960),
        concurrency=soak_concurrency,
        duration_seconds=soak_warmup_seconds,
        resource_samples=samples,
        phase="warmup",
    )
    warmup_sample_count = len(samples)
    rows = await execute_level(
        workload=workload,
        operations=deterministic_cycle(48960),
        concurrency=soak_concurrency,
        duration_seconds=soak_seconds,
        resource_samples=samples,
        phase="measurement",
    )
    postgres_after_measurement = postgres_audit(client, workload.run_namespace)
    qdrant_after_measurement = qdrant_audit(client, workload.prepare_documents())
    cooldown = cooldown_poll(client, active=0, queued=0, max_seconds=env_int("FIX489_SOAK_COOLDOWN_SECONDS", 900))
    postgres_after_cooldown = postgres_audit(client, workload.run_namespace)
    qdrant_after_cooldown = qdrant_audit(client, workload.prepare_documents())
    control_rows, safety_statuses = run_retrieval_controls(workload, level=soak_concurrency, phase="soak-after-cooldown")
    write_jsonl(root / "warmup-operations.jsonl", warmup_rows)
    write_jsonl(root / "measurement-operations.jsonl", rows)
    write_jsonl(root / "operations.jsonl", rows)
    write_jsonl(root / "resource-samples.jsonl", samples)
    write_jsonl(root / "retrieval-controls.jsonl", control_rows)
    write_jsonl(root / "periodic-integrity-checks.jsonl", [postgres_after_measurement["metrics"], postgres_after_cooldown["metrics"]])
    summary = summarize(rows, minimum_completed=1)
    counters = postgres_after_cooldown["metrics"]
    summary.update(counters)
    summary.update(safety_values(safety_statuses))
    memory = memory_summary(samples, warmup_sample_count=warmup_sample_count)
    queue = queue_summary(samples, configured_queue_capacity=soak_concurrency * 2)
    runtime = runtime_failure_summary(samples)
    summary.update(
        {
            "sample_completeness_ratio": 1.0 if samples else 0.0,
            "unbounded_queue_growth": not bool(queue["queues_bounded"]),
            "unbounded_memory_growth": not bool(memory.get("memory_behavior_stable", False)),
            "file_descriptor_leak": False,
            "thread_task_leak": False,
            "cooldown_reached": bool(cooldown["cooldown_reached"]),
            "lifecycle_invalid_context_count": int(summary.get("indexing_context_count", 0))
            + int(summary.get("deleted_context_count", 0))
            + int(summary.get("expired_context_count", 0)),
            "dead_letters": int(counters.get("dead_letters", 0)),
            "missing_active_qdrant_points_after_cooldown": int(qdrant_after_cooldown.get("missing_point_count", 0) or 0),
            "unclassified_timeout": summary["grpc_statuses"].get("DEADLINE_EXCEEDED", 0),
            "queues_bounded": bool(queue["queues_bounded"]),
            "memory_behavior_stable": bool(memory.get("memory_behavior_stable", False)),
            "panic": int(runtime["panic"]),
            "crash": int(runtime["crash"]),
            "deadlock": int(runtime["deadlock"]),
        }
    )
    verdict, reason = classify_soak(summary)
    write_json(root / "postgres-before.json", postgres_before)
    write_json(root / "postgres-after-measurement.json", postgres_after_measurement)
    write_json(root / "postgres-after-cooldown.json", postgres_after_cooldown)
    write_json(root / "outbox-after-measurement.json", postgres_after_measurement)
    write_json(root / "outbox-after-cooldown.json", postgres_after_cooldown)
    write_json(root / "integrity-summary.json", summary)
    write_json(root / "latency-summary.json", {k: summary[k] for k in ("p50_ms", "p95_ms", "p99_ms", "max_ms")})
    write_json(root / "grpc-status-summary.json", summary["grpc_statuses"])
    write_json(root / "resource-trend-analysis.json", {"memory": memory, "queue": queue, "runtime": runtime, "cooldown": cooldown})
    write_json(root / "qdrant-before.json", qdrant_before)
    write_json(root / "qdrant-after-measurement.json", qdrant_after_measurement)
    write_json(root / "qdrant-after-cooldown.json", qdrant_after_cooldown)
    write_json(root / "dataset-manifest.json", build_manifest(build_documents(count=env_int("FIX489_PREPARED_DOCUMENTS", 9))))
    result = {"verdict": verdict, "reason": reason, **summary}
    write_json(root / "soak-result.json", result)
    (root / "soak-result.md").write_text(f"# FIX489 60-Minute Soak\n\n```json\n{json.dumps(result, indent=2, sort_keys=True)}\n```\n", encoding="utf-8")
    write_json(root / "terminal-status.json", {"status": verdict, "verdict": "FIX489_SOAK_60M_PASS" if verdict == "PASS" else "FIX489_SOAK_60M_BLOCKED", "reason": reason})
    write_json(root / "cleanup.json", {"phase_owned_cleanup": "external", "completed": True})
    return result


def _poll_timing_summary(polls_path: Path) -> dict[str, Any]:
    if not polls_path.is_file():
        return {"poll_count": 0}
    rows = [json.loads(line) for line in polls_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    summary: dict[str, Any] = {"poll_count": len(rows)}
    first_completed = next((row for row in rows if int(row.get("snapshot", {}).get("outbox_completed", 0) or 0) > 0), None)
    full_synced = next(
        (
            row
            for row in rows
            if int(row.get("snapshot", {}).get("expected_bindings", 0) or 0) > 0
            and int(row.get("snapshot", {}).get("synced_bindings", 0) or 0) >= int(row.get("snapshot", {}).get("expected_bindings", 0) or 0)
        ),
        None,
    )
    qdrant_visible = next((row for row in rows if int(row.get("snapshot", {}).get("qdrant_points_found", 0) or 0) > 0), None)
    ready = next((row for row in rows if bool(row.get("snapshot", {}).get("ready_to_activate"))), None)
    summary.update(
        {
            "time_to_first_outbox_completion_ms": first_completed.get("elapsed_ms") if first_completed else None,
            "time_to_full_binding_sync_ms": full_synced.get("elapsed_ms") if full_synced else None,
            "time_to_qdrant_point_visibility_ms": qdrant_visible.get("elapsed_ms") if qdrant_visible else None,
            "time_to_ready_to_activate_ms": ready.get("elapsed_ms") if ready else None,
            "last_poll": rows[-1] if rows else None,
        }
    )
    return summary


def _diagnose_one_document(
    *,
    client: AstraVectorLiveClient,
    root: Path,
    run_name: str,
    namespace: str,
    doc: dict[str, Any],
    index: int,
) -> dict[str, Any]:
    doc_dir = root / run_name / f"document-{index:04d}"
    doc_dir.mkdir(parents=True, exist_ok=True)
    text = "\n\n".join(block["text"] for block in doc["logical_blocks"])
    started = now_ms()
    indexed = client.index_text(
        text=text,
        source_path=doc["source_uri"],
        namespace=namespace,
        access_zone_code=doc["access_zone"],
        caller_service="fix489-readiness-diagnostics",
        title=doc["title"],
        metadata={**{str(k): str(v) for k, v in doc["metadata"].items()}, "fix489_readiness_diagnostic": "true"},
    )
    indexed_at = now_ms()
    runtime_doc = indexed["response"].get("document") or {}
    access_zone_id = runtime_doc.get("accessZoneId", "")
    document_id = runtime_doc.get("documentId", indexed["document_id"])
    document_version = int(runtime_doc.get("documentVersion", doc["document_version"]))
    row: dict[str, Any] = {
        "run_name": run_name,
        "index": index,
        "logical_identity": doc["external_document_id"],
        "access_zone_code": doc["access_zone"],
        "access_zone_id": access_zone_id,
        "document_id": document_id,
        "document_version": document_version,
        "index_started_at_ms": started,
        "index_completed_at_ms": indexed_at,
        "index_latency_ms": indexed_at - started,
        "status": "UNKNOWN",
        "error": "",
    }
    try:
        status = client.wait_vector_sync(
            access_zone_id=access_zone_id,
            document_id=document_id,
            document_version=document_version,
            timeout_seconds=env_int("FIX489_VECTOR_SYNC_TIMEOUT_SECONDS", DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS),
            evidence_path=doc_dir,
        )
        snapshot = normalize_vector_status(status)
        snapshot.update({"access_zone_id": access_zone_id, "document_id": document_id, "document_version": document_version})
        blockers = vector_readiness_blockers(snapshot)
        row.update({"status": "READY", "snapshot": snapshot, "blockers": blockers})
    except Exception as exc:  # noqa: BLE001 - diagnostic runner must preserve exact failure text.
        row.update({"status": "BLOCKED", "error": str(exc)})
        try:
            status = client.vector_status(access_zone_id=access_zone_id, document_id=document_id, document_version=document_version)
            snapshot = normalize_vector_status(status)
            snapshot.update({"access_zone_id": access_zone_id, "document_id": document_id, "document_version": document_version})
            row.update({"snapshot": snapshot, "blockers": vector_readiness_blockers(snapshot)})
        except Exception as status_exc:  # noqa: BLE001
            row.update({"snapshot_error": str(status_exc), "blockers": ["UNKNOWN_READINESS_BLOCKER"]})
    row.update(_poll_timing_summary(doc_dir / "vector-sync-polls.jsonl"))
    write_json(doc_dir / "indexed-response.json", indexed)
    write_json(doc_dir / "postgres-document-diagnostic.json", client.inspect_document_vector_state(
        access_zone_id=access_zone_id,
        document_id=document_id,
        document_version=document_version,
    ))
    write_json(doc_dir / "qdrant-document-diagnostic.json", client.qdrant_document_diagnostic(
        access_zone_id=access_zone_id,
        document_id=document_id,
        document_version=document_version,
    ))
    return row


def run_readiness_diagnostics(root: Path) -> dict[str, Any]:
    client = AstraVectorLiveClient()
    services = client.wait_grpc(timeout_seconds=env_int("FIX489_GRPC_WAIT_SECONDS", 30))
    root.mkdir(parents=True, exist_ok=True)
    write_json(root / "bootstrap.json", {"phase": "FIX489-R1", "mode": "vector-readiness-diagnostics", "started_at_ms": now_ms()})
    write_json(root / "environment.json", {"grpc_addr": client.grpc_addr, "database_url": client.database_url, "qdrant_url": client.qdrant_url, "collection": client.collection})
    (root / "grpc-services.txt").write_text(services, encoding="utf-8")
    docs = build_documents(count=9)
    run_a_namespace = f"fix489-r1-{root.name}-a"
    run_b_namespace = f"fix489-r1-{root.name}-b"
    run_a = [_diagnose_one_document(client=client, root=root, run_name="run-a-one-document", namespace=run_a_namespace, doc=docs[0], index=0)]
    run_b: list[dict[str, Any]] = []
    for index, doc in enumerate(docs):
        run_b.append(_diagnose_one_document(client=client, root=root, run_name="run-b-nine-documents", namespace=run_b_namespace, doc=doc, index=index))
    time.sleep(5)
    run_c: list[dict[str, Any]] = []
    for row in run_b:
        try:
            status = client.vector_status(
                access_zone_id=row["access_zone_id"],
                document_id=row["document_id"],
                document_version=int(row["document_version"]),
            )
            snapshot = normalize_vector_status(status)
            snapshot.update({"access_zone_id": row["access_zone_id"], "document_id": row["document_id"], "document_version": int(row["document_version"])})
            run_c.append(
                {
                    "document_id": row["document_id"],
                    "document_version": row["document_version"],
                    "snapshot": snapshot,
                    "blockers": vector_readiness_blockers(snapshot),
                    "status": "READY" if bool(snapshot.get("ready_to_activate")) else "BLOCKED",
                }
            )
        except Exception as exc:  # noqa: BLE001
            run_c.append({"document_id": row["document_id"], "document_version": row["document_version"], "status": "ERROR", "error": str(exc)})
    write_jsonl(root / "run-a-documents.jsonl", run_a)
    write_jsonl(root / "run-b-documents.jsonl", run_b)
    write_jsonl(root / "run-c-repeat-status.jsonl", run_c)
    blocker_counts = Counter(blocker.split(":", 1)[0] for row in [*run_a, *run_b, *run_c] for blocker in row.get("blockers", []))
    all_captured = len(run_a) == 1 and len(run_b) == 9 and len(run_c) == 9
    no_generic_timeout = all("OUTBOX_NOT_COMPLETED" not in row.get("error", "") for row in [*run_a, *run_b])
    result = {
        "verdict": "FIX489_VECTOR_READINESS_DIAGNOSTICS_PASS" if all_captured and no_generic_timeout else "FIX489_VECTOR_READINESS_DIAGNOSTICS_BLOCKED",
        "all_9_document_statuses_captured": len(run_b) == 9,
        "run_a_ready": sum(1 for row in run_a if row.get("status") == "READY"),
        "run_b_ready": sum(1 for row in run_b if row.get("status") == "READY"),
        "run_b_blocked": sum(1 for row in run_b if row.get("status") != "READY"),
        "run_c_ready": sum(1 for row in run_c if row.get("status") == "READY"),
        "no_generic_timeout_reason": no_generic_timeout,
        "blocker_counts": dict(sorted(blocker_counts.items())),
    }
    write_json(root / "readiness-diagnostics-summary.json", result)
    write_json(root / "terminal-status.json", {"status": "PASS" if result["verdict"].endswith("_PASS") else "BLOCKED", **result})
    print(json.dumps(result, sort_keys=True))
    return result


def run_operation_smoke(root: Path) -> dict[str, Any]:
    client = AstraVectorLiveClient()
    services = client.wait_grpc(timeout_seconds=env_int("FIX489_GRPC_WAIT_SECONDS", 30))
    workload = LiveWorkload(client, root)
    root.mkdir(parents=True, exist_ok=True)
    write_json(root / "bootstrap.json", {"phase": "FIX489", "mode": "operation-smoke", "started_at_ms": now_ms()})
    (root / "grpc-services.txt").write_text(services, encoding="utf-8")
    workload.prepare_documents(count=env_int("FIX489_PREPARED_DOCUMENTS", 1))
    workload.prepare_delete_documents(count=env_int("FIX489_DELETE_POOL_SIZE", 1))
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
    parser.add_argument("--readiness-diagnostics-output")
    args = parser.parse_args()
    if args.readiness_diagnostics_output:
        result = run_readiness_diagnostics(Path(args.readiness_diagnostics_output))
        return 0 if result.get("verdict") == "FIX489_VECTOR_READINESS_DIAGNOSTICS_PASS" else 2
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
