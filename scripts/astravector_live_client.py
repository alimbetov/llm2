#!/usr/bin/env python3
"""Reusable live AstraVector client for local demos and operational harnesses.

The module intentionally shells out to grpcurl/psql/curl-compatible stdlib
interfaces instead of linking generated Python stubs. That keeps the local
operator path close to the commands documented for manual troubleshooting.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import socket
import subprocess
import time
import urllib.request
import uuid
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DATABASE_URL = "postgres://astravector:astravector@127.0.0.1:55432/astravector"
DEFAULT_QDRANT_URL = "http://127.0.0.1:6333"
DEFAULT_GRPC_ADDR = "127.0.0.1:50051"
DEFAULT_COLLECTION = "astravector_local_demo"


def env(name: str, default: str = "") -> str:
    return os.environ.get(name, default)


def sha256_file(path: str | Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def write_json(path: str | Path, value: Any) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run_command(
    args: list[str],
    *,
    cwd: Path = ROOT,
    input_text: str | None = None,
    check: bool = True,
    capture: bool = True,
    env_overlay: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    merged_env = os.environ.copy()
    if env_overlay:
        merged_env.update(env_overlay)
    result = subprocess.run(
        args,
        input=input_text,
        text=True,
        cwd=cwd,
        env=merged_env,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
        check=False,
    )
    if check and result.returncode != 0:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(args)}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def tool_version(cmd: list[str]) -> dict[str, Any]:
    if shutil.which(cmd[0]) is None:
        return {"available": False, "command": cmd[0], "version": ""}
    result = run_command(cmd, check=False)
    return {
        "available": result.returncode == 0,
        "command": " ".join(cmd),
        "version": (result.stdout or result.stderr).strip().splitlines()[:3],
    }


def qdrant_collections_response_is_ready(value: Any) -> bool:
    if not isinstance(value, dict):
        return False
    if isinstance(value.get("collections"), list):
        return True
    result = value.get("result")
    return isinstance(result, dict) and isinstance(result.get("collections"), list)


def vector_sync_is_complete(sync: dict[str, Any]) -> bool:
    if "readyToActivate" in sync:
        return bool(sync.get("readyToActivate"))
    expected = int(sync.get("expectedBindings", 0) or 0)
    return (
        expected > 0
        and int(sync.get("syncedBindings", 0) or 0) >= expected
        and int(sync.get("outboxCompleted", 0) or 0) >= expected
        and int(sync.get("outboxFailed", 0) or 0) == 0
        and int(sync.get("qdrantPointsFound", 0) or 0) >= expected
    )


def document_vector_status_ready(response: dict[str, Any]) -> bool:
    status = response.get("status") or {}
    state = str(status.get("state") or "")
    if state:
        return state == "OPERATION_STATE_READY_TO_ACTIVATE" or bool(
            status.get("readyToActivate") or status.get("searchable")
        )
    if "readyToActivate" in status or "searchable" in status:
        return bool(status.get("readyToActivate") or status.get("searchable"))
    return vector_sync_is_complete(status.get("sync") or {})


def _first_present(mapping: dict[str, Any], *names: str, default: Any = None) -> Any:
    for name in names:
        if name in mapping:
            return mapping.get(name)
    return default


def _int_value(value: Any, default: int = 0) -> int:
    if value is None or value == "":
        return default
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _bool_value(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.lower() in ("true", "t", "1", "yes")
    return bool(value)


def _list_value(value: Any) -> list[Any]:
    return value if isinstance(value, list) else []


def normalize_vector_status(response: dict[str, Any]) -> dict[str, Any]:
    status = response.get("status") or {}
    sync = status.get("sync") or response.get("sync") or {}
    document = response.get("document") or status.get("document") or {}
    qdrant = status.get("qdrant") or response.get("qdrant") or {}
    snapshot = {
        "timestamp": int(time.time() * 1000),
        "access_zone_id": _first_present(document, "accessZoneId", "access_zone_id", default=""),
        "document_id": _first_present(document, "documentId", "document_id", default=""),
        "document_version": _int_value(_first_present(document, "documentVersion", "document_version", default=0)),
        "state": str(status.get("state") or ""),
        "document_status": str(_first_present(status, "documentStatus", "document_status", default="")),
        "message": str(status.get("message") or ""),
        "searchable": _bool_value(status.get("searchable")),
        "ready_to_activate": _bool_value(_first_present(status, "readyToActivate", "ready_to_activate", default=False)),
        "progress_percent": _int_value(_first_present(status, "progressPercent", "progress_percent", default=0)),
        "expected_bindings": _int_value(_first_present(sync, "expectedBindings", "expected_bindings", default=0)),
        "synced_bindings": _int_value(_first_present(sync, "syncedBindings", "synced_bindings", default=0)),
        "pending_bindings": _int_value(_first_present(sync, "pendingBindings", "pending_bindings", default=0)),
        "failed_bindings": _int_value(_first_present(sync, "failedBindings", "failed_bindings", default=0)),
        "deleted_bindings": _int_value(_first_present(sync, "deletedBindings", "deleted_bindings", default=0)),
        "dense_vectors_expected": _int_value(_first_present(sync, "denseVectorsExpected", "dense_vectors_expected", default=0)),
        "dense_vectors_found": _int_value(_first_present(sync, "denseVectorsFound", "dense_vectors_found", default=0)),
        "sparse_vectors_expected": _int_value(_first_present(sync, "sparseVectorsExpected", "sparse_vectors_expected", default=0)),
        "sparse_vectors_found": _int_value(_first_present(sync, "sparseVectorsFound", "sparse_vectors_found", default=0)),
        "outbox_pending": _int_value(_first_present(sync, "outboxPending", "outbox_pending", default=0)),
        "outbox_retry_pending": _int_value(_first_present(sync, "outboxRetryPending", "outbox_retry_pending", default=0)),
        "outbox_completed": _int_value(_first_present(sync, "outboxCompleted", "outbox_completed", default=0)),
        "outbox_failed": _int_value(_first_present(sync, "outboxFailed", "outbox_failed", default=0)),
        "qdrant_collection": str(_first_present(qdrant, "collection", "qdrantCollection", "qdrant_collection", default="")),
        "qdrant_collection_exists": _bool_value(_first_present(qdrant, "collectionExists", "qdrant_collection_exists", default=True)),
        "qdrant_points_expected": _int_value(_first_present(sync, "qdrantPointsExpected", "qdrant_points_expected", "expectedBindings", default=0)),
        "qdrant_points_found": _int_value(_first_present(sync, "qdrantPointsFound", "qdrant_points_found", default=0)),
        "qdrant_points_missing": _int_value(_first_present(sync, "qdrantPointsMissing", "qdrant_points_missing", default=0)),
        "qdrant_points_extra": _int_value(_first_present(sync, "qdrantPointsExtra", "qdrant_points_extra", default=0)),
        "last_sync_attempt_at": str(_first_present(sync, "lastSyncAttemptAt", "last_sync_attempt_at", default="")),
        "last_sync_error_code": str(_first_present(sync, "lastSyncErrorCode", "last_sync_error_code", default="")),
        "last_sync_error_message": str(_first_present(sync, "lastSyncErrorMessage", "last_sync_error_message", default="")),
        "warnings": _list_value(status.get("warnings")),
    }
    if not snapshot["ready_to_activate"]:
        snapshot["ready_to_activate"] = document_vector_status_ready(response)
    return snapshot


def vector_readiness_blockers(snapshot: dict[str, Any], sparse_required: bool = False) -> list[str]:
    blockers: list[str] = []
    expected = _int_value(snapshot.get("expected_bindings"))
    synced = _int_value(snapshot.get("synced_bindings"))
    deleted = _int_value(snapshot.get("deleted_bindings"))
    pending = _int_value(snapshot.get("pending_bindings"))
    failed = _int_value(snapshot.get("failed_bindings"))
    outbox_pending = _int_value(snapshot.get("outbox_pending"))
    outbox_retry = _int_value(snapshot.get("outbox_retry_pending"))
    outbox_completed = _int_value(snapshot.get("outbox_completed"))
    outbox_failed = _int_value(snapshot.get("outbox_failed"))
    dense_expected = _int_value(snapshot.get("dense_vectors_expected"))
    dense_found = _int_value(snapshot.get("dense_vectors_found"))
    sparse_expected = _int_value(snapshot.get("sparse_vectors_expected"))
    sparse_found = _int_value(snapshot.get("sparse_vectors_found"))
    qdrant_expected = _int_value(snapshot.get("qdrant_points_expected"))
    qdrant_found = _int_value(snapshot.get("qdrant_points_found"))
    qdrant_missing = _int_value(snapshot.get("qdrant_points_missing"))
    qdrant_extra = _int_value(snapshot.get("qdrant_points_extra"))
    state = str(snapshot.get("state") or "")
    if state in ("OPERATION_STATE_NOT_FOUND", "DOCUMENT_NOT_FOUND"):
        blockers.append("DOCUMENT_NOT_FOUND:state=%s" % state)
    if expected == 0:
        blockers.append("EXPECTED_BINDINGS_ZERO:expected_bindings=0")
    if synced < expected:
        blockers.append(f"BINDINGS_NOT_SYNCED:expected_bindings={expected},synced_bindings={synced},deleted_bindings={deleted}")
    if pending > 0:
        blockers.append(f"PENDING_BINDINGS_PRESENT:pending_bindings={pending}")
    if failed > 0:
        blockers.append(f"FAILED_BINDINGS_PRESENT:failed_bindings={failed}")
    if deleted > 0:
        blockers.append(f"DELETED_BINDINGS_INCLUDED:deleted_bindings={deleted},expected_bindings={expected}")
    if dense_expected > 0 and dense_found < dense_expected:
        blockers.append(f"DENSE_VECTOR_COUNT_MISMATCH:dense_vectors_expected={dense_expected},dense_vectors_found={dense_found}")
    if sparse_required and sparse_expected > 0 and sparse_found < sparse_expected:
        blockers.append(f"SPARSE_VECTOR_COUNT_MISMATCH:sparse_vectors_expected={sparse_expected},sparse_vectors_found={sparse_found}")
    if outbox_pending > 0:
        blockers.append(f"OUTBOX_PENDING:outbox_pending={outbox_pending}")
    if outbox_retry > 0:
        blockers.append(f"OUTBOX_RETRY_PENDING:outbox_retry_pending={outbox_retry}")
    if outbox_failed > 0:
        blockers.append(f"OUTBOX_FAILED:outbox_failed={outbox_failed}")
    if expected > 0 and outbox_completed < expected:
        blockers.append(f"OUTBOX_COMPLETED_BELOW_EXPECTED:expected_bindings={expected},outbox_completed={outbox_completed}")
    if not _bool_value(snapshot.get("qdrant_collection_exists")):
        blockers.append(f"QDRANT_COLLECTION_MISSING:qdrant_collection={snapshot.get('qdrant_collection')}")
    if qdrant_missing > 0:
        blockers.append(f"QDRANT_POINTS_MISSING:qdrant_points_missing={qdrant_missing}")
    if qdrant_expected > 0 and qdrant_found < qdrant_expected:
        blockers.append(f"QDRANT_POINT_COUNT_BELOW_EXPECTED:qdrant_points_expected={qdrant_expected},qdrant_points_found={qdrant_found}")
    if qdrant_extra > 0:
        blockers.append(f"QDRANT_POINTS_EXTRA:qdrant_points_extra={qdrant_extra}")
    if not _bool_value(snapshot.get("ready_to_activate")):
        blockers.append(f"SERVER_READY_FLAG_FALSE:ready_to_activate=false,state={state}")
    if not blockers and not _bool_value(snapshot.get("ready_to_activate")):
        blockers.append("UNKNOWN_READINESS_BLOCKER")
    return blockers


def _quote_sql(value: str) -> str:
    return "'" + str(value).replace("'", "''") + "'"


def _snapshot_key(snapshot: dict[str, Any]) -> tuple[Any, ...]:
    return tuple(snapshot.get(key) for key in (
        "state",
        "document_status",
        "ready_to_activate",
        "expected_bindings",
        "synced_bindings",
        "pending_bindings",
        "failed_bindings",
        "deleted_bindings",
        "dense_vectors_found",
        "sparse_vectors_found",
        "outbox_pending",
        "outbox_retry_pending",
        "outbox_completed",
        "outbox_failed",
        "qdrant_points_found",
        "qdrant_points_missing",
        "qdrant_points_extra",
        "message",
    ))


def deterministic_document_id(namespace: str, path: str | Path, content_hash: str) -> str:
    return str(uuid.uuid5(uuid.NAMESPACE_URL, f"{namespace}:{Path(path)}:{content_hash}"))


def make_logical_blocks(
    text: str,
    *,
    namespace: str,
    section_path: str,
    heading: str,
    root_text: str,
    metadata_prefix: str,
) -> list[dict[str, Any]]:
    root_id = str(uuid.uuid5(uuid.NAMESPACE_URL, f"{namespace}:root:{sha256_text(text)}"))
    blocks: list[dict[str, Any]] = [
        {
            "blockId": root_id,
            "blockType": "BLOCK_TYPE_DOCUMENT",
            "text": root_text,
            "orderIndex": 0,
            "sourceLocation": {
                "charStart": 0,
                "charEnd": len(text),
                "sectionPath": section_path,
                "heading": heading,
            },
            "metadata": {f"{metadata_prefix}_role": "root"},
        }
    ]
    offset = 0
    order = 1
    for paragraph in [p.strip() for p in text.split("\n\n") if p.strip()]:
        start = text.find(paragraph, offset)
        end = start + len(paragraph)
        blocks.append(
            {
                "blockId": str(uuid.uuid5(uuid.NAMESPACE_URL, f"{namespace}:block:{order}:{sha256_text(paragraph)}")),
                "parentBlockId": root_id,
                "blockType": "BLOCK_TYPE_PARAGRAPH",
                "text": paragraph,
                "orderIndex": order,
                "sourceLocation": {
                    "charStart": start,
                    "charEnd": end,
                    "sectionPath": section_path,
                    "heading": heading,
                },
                "metadata": {f"{metadata_prefix}_order": str(order)},
            }
        )
        offset = end
        order += 1
    return blocks


class AstraVectorLiveClient:
    def __init__(
        self,
        *,
        grpc_addr: str | None = None,
        database_url: str | None = None,
        qdrant_url: str | None = None,
        collection: str | None = None,
    ) -> None:
        self.grpc_addr = grpc_addr or env("ASTRAVECTOR_LOCAL_DEMO_GRPC_ADDR", env("ASTRAVECTOR_GRPC_ADDR", DEFAULT_GRPC_ADDR))
        self.database_url = database_url or env("DATABASE_URL", DEFAULT_DATABASE_URL)
        self.qdrant_url = (qdrant_url or env("ASTRAVECTOR_QDRANT_URL", DEFAULT_QDRANT_URL)).rstrip("/")
        self.collection = collection or env("ASTRAVECTOR_QDRANT_COLLECTION", DEFAULT_COLLECTION)

    def model_identity(self) -> dict[str, str]:
        model = env("ASTRAVECTOR_MODEL_PATH")
        tokenizer = env("ASTRAVECTOR_TOKENIZER_PATH")
        if not model or not tokenizer:
            raise RuntimeError("MODEL_OR_TOKENIZER_NOT_CONFIGURED")
        model_path = Path(model).expanduser().resolve()
        tokenizer_path = Path(tokenizer).expanduser().resolve()
        if not model_path.is_file():
            raise RuntimeError(f"MODEL_NOT_AVAILABLE path={model_path}")
        if not tokenizer_path.is_file():
            raise RuntimeError(f"TOKENIZER_NOT_AVAILABLE path={tokenizer_path}")
        json.loads(tokenizer_path.read_text(encoding="utf-8"))
        return {
            "model_path": str(model_path),
            "model_sha256": sha256_file(model_path),
            "tokenizer_path": str(tokenizer_path),
            "tokenizer_sha256": sha256_file(tokenizer_path),
        }

    def grpcurl(self, method: str, payload: dict[str, Any], *, headers: dict[str, str] | None = None) -> dict[str, Any]:
        args = ["grpcurl", "-plaintext"]
        for key, value in (headers or {}).items():
            args.extend(["-H", f"{key}: {value}"])
        args.extend(["-d", json.dumps(payload, ensure_ascii=False), self.grpc_addr, method])
        result = run_command(args, check=False)
        if result.returncode != 0:
            raise RuntimeError(f"grpcurl failed for {method}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")
        return json.loads(result.stdout or "{}")

    def grpc_list(self) -> str:
        result = run_command(["grpcurl", "-plaintext", self.grpc_addr, "list"], check=False)
        if result.returncode != 0:
            raise RuntimeError(result.stderr)
        return result.stdout

    def wait_grpc(self, timeout_seconds: int = 120) -> str:
        deadline = time.time() + timeout_seconds
        last = ""
        while time.time() < deadline:
            result = run_command(["grpcurl", "-plaintext", self.grpc_addr, "list"], check=False)
            last = (result.stdout or result.stderr).strip()
            if result.returncode == 0 and "AstraVectorV004Control" in result.stdout:
                return result.stdout
            time.sleep(1)
        raise RuntimeError(f"gRPC endpoint not ready: {last}")

    def http_json(self, path_or_url: str, method: str = "GET", body: dict[str, Any] | None = None) -> dict[str, Any]:
        url = path_or_url if path_or_url.startswith("http") else f"{self.qdrant_url}{path_or_url}"
        data = None
        headers = {}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        with urllib.request.urlopen(req, timeout=10) as response:
            raw = response.read().decode("utf-8")
            return json.loads(raw) if raw else {}

    def psql_json(self, sql: str) -> dict[str, Any]:
        result = run_command(["psql", self.database_url, "-t", "-A", "-c", sql])
        raw = result.stdout.strip()
        return json.loads(raw) if raw else {}

    def psql_rows(self, sql: str) -> list[dict[str, Any]]:
        wrapped = f"SELECT COALESCE(json_agg(row_to_json(q)), '[]'::json) FROM ({sql}) q;"
        result = run_command(["psql", self.database_url, "-t", "-A", "-c", wrapped])
        raw = result.stdout.strip()
        return json.loads(raw) if raw else []

    def index_text(
        self,
        *,
        text: str,
        source_path: str,
        namespace: str,
        access_zone_code: str,
        caller_service: str,
        title: str,
        source_type: str = "PLAIN_TEXT",
        metadata: dict[str, str] | None = None,
        document_version: int = 1,
        embedding_mode: str = "EMBEDDING_MODE_V005_DENSE_ONLY",
    ) -> dict[str, Any]:
        content_hash = sha256_text(text)
        document_id = deterministic_document_id(namespace, source_path, content_hash)
        blocks = make_logical_blocks(
            text,
            namespace=namespace,
            section_path=namespace,
            heading=title,
            root_text=f"{title} document root.",
            metadata_prefix=namespace.replace("-", "_"),
        )
        request = {
            "context": {
                "correlationId": f"{namespace}-load-{int(time.time() * 1000)}",
                "idempotencyKey": f"{namespace}-load:{document_id}:{document_version}:{content_hash}",
                "callerService": caller_service,
                "callerUserId": "local-operator",
                "callerAccessLevel": "PUBLIC",
            },
            "accessZoneCode": access_zone_code,
            "document": {
                "externalDocumentId": f"{namespace}:{Path(source_path).name}",
                "documentId": document_id,
                "documentVersion": document_version,
                "title": title,
                "sourceUri": source_path,
                "sourceType": source_type,
                "mimeType": "text/plain; charset=utf-8",
                "contentHash": content_hash,
            },
            "blocks": blocks,
            "chunkingOptions": {
                "profile": "CHUNKING_PROFILE_DEFAULT",
                "preserveBlockBoundaries": True,
                "createParentContext": True,
            },
            "indexingOptions": {
                "activationPolicy": "ACTIVATION_POLICY_MANUAL",
                "embeddingMode": embedding_mode,
                "publishMode": "PUBLISH_MODE_V005_OUTBOX",
                "ttlPolicy": {"mode": "TTL_MODE_NONE"},
                "replaceExistingVersion": True,
            },
            "metadata": metadata or {},
        }
        response = self.grpcurl("astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument", request)
        return {"request": request, "response": response, "content_hash": content_hash, "document_id": document_id, "blocks": blocks}

    def vector_status(self, *, access_zone_id: str, document_id: str, document_version: int, include_qdrant: bool = True) -> dict[str, Any]:
        return self.grpcurl(
            "astravector.embedding.v1.AstraVectorIngestionFacade/GetDocumentVectorStatus",
            {
                "context": {
                    "correlationId": "live-client-vector-status",
                    "callerService": "astravector-live-client",
                    "callerAccessLevel": "PUBLIC",
                },
                "document": {
                    "accessZoneId": access_zone_id,
                    "documentId": document_id,
                    "documentVersion": document_version,
                },
                "includeQdrant": include_qdrant,
            },
        )

    def inspect_document_vector_state(self, *, access_zone_id: str, document_id: str, document_version: int) -> dict[str, Any]:
        zone = _quote_sql(access_zone_id)
        doc = _quote_sql(document_id)
        ver = int(document_version)
        binding_rows = self.psql_rows(
            f"""
SELECT qdrant_sync_status,
       lifecycle_status,
       chunk_granularity,
       representation_type,
       COUNT(*)::bigint AS binding_count
FROM astravector.vector_bindings_v004
WHERE access_zone_id = {zone}::uuid
  AND document_id = {doc}::uuid
  AND document_version = {ver}
GROUP BY qdrant_sync_status, lifecycle_status, chunk_granularity, representation_type
ORDER BY qdrant_sync_status, lifecycle_status, chunk_granularity, representation_type
"""
        )
        outbox_rows = self.psql_rows(
            f"""
SELECT o.status,
       COUNT(*)::bigint AS outbox_count
FROM astravector.vector_outbox o
JOIN astravector.vector_bindings_v004 b
  ON b.id = o.binding_id
 AND b.access_zone_id = o.binding_access_zone_id
WHERE b.access_zone_id = {zone}::uuid
  AND b.document_id = {doc}::uuid
  AND b.document_version = {ver}
GROUP BY o.status
ORDER BY o.status
"""
        )
        embedding_rows = self.psql_rows(
            f"""
SELECT COUNT(*)::bigint AS expected_bindings,
       COUNT(ed.id)::bigint AS dense_vectors_found,
       COUNT(es.id)::bigint AS sparse_vectors_found
FROM astravector.vector_bindings_v004 b
LEFT JOIN astravector.embedding_dense ed ON ed.cache_entry_id = b.cache_entry_id
LEFT JOIN astravector.embedding_sparse es ON es.cache_entry_id = b.cache_entry_id
WHERE b.access_zone_id = {zone}::uuid
  AND b.document_id = {doc}::uuid
  AND b.document_version = {ver}
"""
        )
        return {"binding_rows": binding_rows, "outbox_rows": outbox_rows, "embedding_rows": embedding_rows}

    def qdrant_document_diagnostic(self, *, access_zone_id: str, document_id: str, document_version: int) -> dict[str, Any]:
        body = {
            "limit": 256,
            "with_payload": True,
            "with_vector": False,
            "filter": {
                "must": [
                    {"key": "access_zone_id", "match": {"value": access_zone_id}},
                    {"key": "document_id", "match": {"value": document_id}},
                    {"key": "document_version", "match": {"value": int(document_version)}},
                ]
            },
        }
        try:
            response = self.http_json(f"/collections/{self.collection}/points/scroll", method="POST", body=body)
            points = (response.get("result") or {}).get("points") or []
            return {
                "status": "MEASURED",
                "collection": self.collection,
                "point_count": len(points),
                "points": [
                    {
                        "point_id": point.get("id"),
                        "chunk_id": (point.get("payload") or {}).get("chunk_id"),
                        "binding_id": (point.get("payload") or {}).get("binding_id"),
                        "lifecycle_status": (point.get("payload") or {}).get("lifecycle_status"),
                        "qdrant_point_id": (point.get("payload") or {}).get("qdrant_point_id"),
                        "access_zone_id": (point.get("payload") or {}).get("access_zone_id"),
                        "document_id": (point.get("payload") or {}).get("document_id"),
                        "document_version": (point.get("payload") or {}).get("document_version"),
                    }
                    for point in points
                ],
            }
        except Exception as exc:  # noqa: BLE001 - diagnostic must preserve the underlying failure text.
            return {"status": "ERROR", "collection": self.collection, "error": str(exc)}

    def debug_document(
        self,
        *,
        access_zone_id: str,
        document_id: str,
        document_version: int,
        include_chunks: bool = True,
        include_vectors: bool = True,
        include_outbox: bool = True,
        include_qdrant: bool = True,
    ) -> dict[str, Any]:
        payload = {
            "document": {
                "accessZoneId": access_zone_id,
                "documentId": document_id,
                "documentVersion": int(document_version),
            },
            "includeChunks": include_chunks,
            "includeVectors": include_vectors,
            "includeOutbox": include_outbox,
            "includeQdrant": include_qdrant,
        }
        try:
            return self.grpcurl("astravector.embedding.v1.AstraVectorAdminFacade/DebugDocument", payload)
        except Exception as exc:  # noqa: BLE001 - optional admin RPC may be unavailable locally.
            return {"status": "UNAVAILABLE", "error": str(exc)}

    def wait_vector_sync(
        self,
        *,
        access_zone_id: str,
        document_id: str,
        document_version: int,
        timeout_seconds: int = 120,
        evidence_path: Path | None = None,
        sparse_required: bool = False,
    ) -> dict[str, Any]:
        evidence_dir = Path(evidence_path) if evidence_path is not None else None
        polls_path = evidence_dir / "vector-sync-polls.jsonl" if evidence_dir is not None else None
        if evidence_dir is not None:
            evidence_dir.mkdir(parents=True, exist_ok=True)
        started = time.time()
        deadline = time.time() + timeout_seconds
        last: dict[str, Any] = {}
        last_snapshot: dict[str, Any] = {}
        last_key: tuple[Any, ...] | None = None

        def record(snapshot: dict[str, Any], blockers: list[str], event: str) -> None:
            if polls_path is None:
                return
            row = {
                "event": event,
                "elapsed_ms": int((time.time() - started) * 1000),
                "snapshot": snapshot,
                "blockers": blockers,
            }
            with polls_path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")

        while time.time() < deadline:
            last = self.vector_status(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
                include_qdrant=True,
            )
            last_snapshot = normalize_vector_status(last)
            last_snapshot.update({"access_zone_id": access_zone_id, "document_id": document_id, "document_version": int(document_version)})
            blockers = vector_readiness_blockers(last_snapshot, sparse_required=sparse_required)
            key = _snapshot_key(last_snapshot)
            if last_key is None or key != last_key:
                record(last_snapshot, blockers, "poll")
                last_key = key
            if any(blocker.startswith("OUTBOX_FAILED") or blocker.startswith("FAILED_BINDINGS_PRESENT") for blocker in blockers):
                record(last_snapshot, blockers, "failed")
                raise RuntimeError(
                    "VECTOR_SYNC_OUTBOX_FAILED "
                    f"access_zone_id={access_zone_id} document_id={document_id} document_version={document_version} "
                    f"blockers={blockers} evidence_path={evidence_dir}"
                )
            if any(blocker.startswith("DOCUMENT_NOT_FOUND") for blocker in blockers):
                record(last_snapshot, blockers, "document_not_found")
                raise RuntimeError(
                    "VECTOR_SYNC_DOCUMENT_NOT_FOUND "
                    f"access_zone_id={access_zone_id} document_id={document_id} document_version={document_version} "
                    f"blockers={blockers} evidence_path={evidence_dir}"
                )
            if document_vector_status_ready(last):
                record(last_snapshot, blockers, "ready")
                return last
            time.sleep(1)
        final = self.vector_status(
            access_zone_id=access_zone_id,
            document_id=document_id,
            document_version=document_version,
            include_qdrant=True,
        )
        final_snapshot = normalize_vector_status(final)
        final_snapshot.update({"access_zone_id": access_zone_id, "document_id": document_id, "document_version": int(document_version)})
        final_blockers = vector_readiness_blockers(final_snapshot, sparse_required=sparse_required)
        if document_vector_status_ready(final):
            record(final_snapshot, final_blockers, "READY_AT_DEADLINE_BOUNDARY")
            return final
        record(final_snapshot, final_blockers, "timeout")
        if evidence_dir is not None:
            write_json(evidence_dir / "postgres-document-diagnostic.json", self.inspect_document_vector_state(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
            ))
            write_json(evidence_dir / "qdrant-document-diagnostic.json", self.qdrant_document_diagnostic(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
            ))
            write_json(evidence_dir / "debug-document-response.json", self.debug_document(
                access_zone_id=access_zone_id,
                document_id=document_id,
                document_version=document_version,
            ))
        if any(blocker.startswith("QDRANT_") for blocker in final_blockers):
            code = "VECTOR_SYNC_QDRANT_MISMATCH"
        elif any("BINDING" in blocker or blocker.startswith("DENSE_") or blocker.startswith("SPARSE_") for blocker in final_blockers):
            code = "VECTOR_SYNC_BINDING_MISMATCH"
        elif any(blocker.startswith("OUTBOX_") for blocker in final_blockers):
            code = "OUTBOX_NOT_COMPLETED"
        else:
            code = "VECTOR_SYNC_TIMEOUT"
        raise RuntimeError(
            f"{code} access_zone_id={access_zone_id} document_id={document_id} document_version={document_version} "
            f"elapsed_ms={int((time.time() - started) * 1000)} state={final_snapshot.get('state')} "
            f"message={final_snapshot.get('message')} blockers={final_blockers} evidence_path={evidence_dir}"
        )

    def activate_document(self, *, access_zone_id: str, document_id: str, document_version: int) -> dict[str, Any]:
        return self.grpcurl(
            "astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion",
            {
                "accessZoneId": access_zone_id,
                "documentId": document_id,
                "documentVersion": document_version,
            },
        )

    def delete_document_vectors(
        self,
        *,
        access_zone_id: str,
        document_id: str,
        document_version: int,
        reason: str,
    ) -> dict[str, Any]:
        return self.grpcurl(
            "astravector.embedding.v1.AstraVectorIngestionFacade/DeleteDocumentVectorsFacade",
            {
                "context": {
                    "correlationId": f"live-delete-{int(time.time() * 1000)}",
                    "callerService": "astravector-live-client",
                    "callerUserId": "local-operator",
                    "callerAccessLevel": "PUBLIC",
                },
                "document": {
                    "accessZoneId": access_zone_id,
                    "documentId": document_id,
                    "documentVersion": document_version,
                },
                "reason": reason,
            },
        )

    def search(
        self,
        *,
        access_zone_id: str,
        access_zone_code: str,
        query: str,
        top_k: int = 3,
        candidate_limit: int = 20,
        parent_limit: int = 3,
        timeout_ms: int = 15000,
        include_debug: bool = False,
        enable_graph_expansion: bool = False,
    ) -> dict[str, Any]:
        return self.grpcurl(
            "astravector.embedding.v1.AstraVectorV004Control/Search",
            {
                "correlationId": f"live-search-{int(time.time() * 1000)}",
                "accessZoneId": access_zone_id,
                "callerAccessLevel": "PUBLIC",
                "query": query,
                "topK": top_k,
                "candidateLimit": candidate_limit,
                "parentLimit": parent_limit,
                "timeoutMs": timeout_ms,
                "searchMode": "SEARCH_MODE_V005_DENSE",
                "embeddingMode": "EMBEDDING_MODE_V005_DENSE_ONLY",
                "includeDebug": include_debug,
                "enableGraphExpansion": enable_graph_expansion,
                "graphMaxHops": 1,
                "graphMaxRelatedContexts": 2,
                "accessZoneCode": access_zone_code,
            },
        )

    def retrieve_context(
        self,
        *,
        access_zone_id: str,
        access_zone_code: str,
        question: str,
        max_contexts: int = 3,
        timeout_ms: int = 15000,
        enable_graph_expansion: bool = False,
    ) -> dict[str, Any]:
        return self.grpcurl(
            "astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext",
            {
                "context": {
                    "correlationId": f"live-retrieve-{int(time.time() * 1000)}",
                    "callerService": "astravector-live-client",
                    "callerUserId": "local-operator",
                    "callerAccessLevel": "PUBLIC",
                },
                "accessZoneId": access_zone_id,
                "accessZoneCode": access_zone_code,
                "question": question,
                "profile": "RETRIEVAL_PROFILE_SEMANTIC",
                "maxContexts": max_contexts,
                "responseDetail": "RESPONSE_DETAIL_STANDARD",
                "enableGraphExpansion": enable_graph_expansion,
                "graphMaxHops": 1,
                "graphMaxRelatedContexts": 2,
            },
            headers={"x-astravector-timeout-ms": str(timeout_ms)},
        )

    def inspect_postgres_document(self, *, access_zone_id: str, document_id: str, document_version: int) -> dict[str, Any]:
        sql = f"""
WITH ids AS (
  SELECT '{access_zone_id}'::uuid AS access_zone_id, '{document_id}'::uuid AS document_id, {document_version}::bigint AS document_version
)
SELECT json_build_object(
  'document_versions', (SELECT json_agg(row_to_json(dv)) FROM (
    SELECT access_zone_id, document_id, document_version, status, lifecycle_status, access_zone_code
    FROM astravector.document_versions
    WHERE access_zone_id=(SELECT access_zone_id FROM ids) AND document_id=(SELECT document_id FROM ids) AND document_version=(SELECT document_version FROM ids)
  ) dv),
  'chunk_count', (SELECT count(*) FROM astravector.content_chunks_v004 c, ids WHERE c.access_zone_id=ids.access_zone_id AND c.document_id=ids.document_id AND c.document_version=ids.document_version),
  'chunks_by_granularity', (SELECT json_object_agg(chunk_granularity, count) FROM (
    SELECT granularity AS chunk_granularity, count(*) FROM astravector.content_chunks_v004 c, ids WHERE c.access_zone_id=ids.access_zone_id AND c.document_id=ids.document_id AND c.document_version=ids.document_version GROUP BY granularity
  ) g),
  'binding_count', (SELECT count(*) FROM astravector.vector_bindings_v004 b, ids WHERE b.access_zone_id=ids.access_zone_id AND b.document_id=ids.document_id AND b.document_version=ids.document_version),
  'bindings_by_sync', (SELECT json_object_agg(qdrant_sync_status, count) FROM (
    SELECT qdrant_sync_status, count(*) FROM astravector.vector_bindings_v004 b, ids WHERE b.access_zone_id=ids.access_zone_id AND b.document_id=ids.document_id AND b.document_version=ids.document_version GROUP BY qdrant_sync_status
  ) s),
  'outbox_by_status', (SELECT json_object_agg(status, count) FROM (
    SELECT o.status, count(*)
    FROM astravector.vector_outbox o
    JOIN astravector.vector_bindings_v004 b
      ON b.id=o.binding_id
     AND b.access_zone_id=o.binding_access_zone_id
    JOIN ids
      ON b.access_zone_id=ids.access_zone_id
     AND b.document_id=ids.document_id
     AND b.document_version=ids.document_version
    GROUP BY o.status
  ) o)
);
"""
        return self.psql_json(sql)

    def inspect_qdrant_document(self, *, access_zone_id: str, document_id: str, document_version: int, limit: int = 5) -> dict[str, Any]:
        info = self.http_json(f"/collections/{self.collection}")
        scroll = self.http_json(
            f"/collections/{self.collection}/points/scroll",
            "POST",
            {
                "limit": limit,
                "with_payload": True,
                "with_vector": False,
                "filter": {
                    "must": [
                        {"key": "access_zone_id", "match": {"value": access_zone_id}},
                        {"key": "document_id", "match": {"value": document_id}},
                        {"key": "document_version", "match": {"value": document_version}},
                    ]
                },
            },
        )
        return {"collection": self.collection, "info": info.get("result", info), "scroll": scroll.get("result", scroll)}

    def integrity_counters(self) -> dict[str, int]:
        rows = self.psql_rows(
            """
SELECT 'orphan_binding_count' AS metric, COUNT(*)::bigint AS value
FROM astravector.vector_bindings_v004 vb
LEFT JOIN astravector.content_chunks_v004 c
  ON c.id = vb.chunk_id
 AND c.access_zone_id = vb.access_zone_id
WHERE c.id IS NULL
UNION ALL
SELECT 'orphan_outbox_count', COUNT(*)::bigint
FROM astravector.vector_outbox vo
LEFT JOIN astravector.vector_bindings_v004 vb
  ON vb.id = vo.binding_id
 AND vb.access_zone_id = vo.binding_access_zone_id
WHERE vb.id IS NULL
UNION ALL
SELECT 'failed_outbox', COUNT(*)::bigint
FROM astravector.vector_outbox
WHERE status IN ('FAILED', 'DEAD_LETTER')
"""
        )
        return {str(row["metric"]): int(row["value"]) for row in rows}


def port_open(host: str, port: int, timeout: float = 1.0) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        return sock.connect_ex((host, int(port))) == 0
