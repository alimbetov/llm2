#!/usr/bin/env python3
"""Capture one sequential FIX486G statistical full pass for offline evaluation."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


EXPECTED_QUERY_COUNT = 71
ENTRY_POINTS = ("Search", "RetrieveContext")
METHODS = {
    "Search": "astravector.embedding.v1.AstraVectorV004Control/Search",
    "RetrieveContext": "astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext",
}
RESOURCE_FIELDS = (
    "sql_statement_count",
    "qdrant_request_count",
    "graph_relation_query_count",
    "n_plus_one_sql_hydration",
)


class CaptureError(ValueError):
    pass


def fail(code: str, detail: str) -> None:
    raise CaptureError(f"{code}: {detail}")


def read_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"{label}_INVALID", f"{path}: {error}")


def read_queries(bank: Path) -> list[dict[str, Any]]:
    manifest_path = bank / "bank-manifest.json"
    manifest = read_json(manifest_path, "BANK")
    if not isinstance(manifest, dict):
        fail("BANK_MANIFEST_INVALID", "JSON object required")
    relative = manifest.get("files", {}).get("queries")
    if manifest.get("status") != "FROZEN" or manifest.get("query_count") != EXPECTED_QUERY_COUNT:
        fail("BANK_IDENTITY_INVALID", "expected a frozen bank containing 71 queries")
    if not isinstance(relative, str) or not relative:
        fail("BANK_MANIFEST_INVALID", "files.queries must name the supplemental JSONL")
    query_path = bank / relative
    try:
        lines = query_path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        fail("BANK_QUERIES_INVALID", f"{query_path}: {error}")
    queries: list[dict[str, Any]] = []
    for line_number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            fail("BANK_QUERIES_INVALID", f"{query_path}:{line_number}: {error}")
        if not isinstance(row, dict):
            fail("BANK_QUERIES_INVALID", f"{query_path}:{line_number}: object required")
        queries.append(row)
    query_ids = [query.get("query_id") for query in queries]
    if (
        len(queries) != EXPECTED_QUERY_COUNT
        or any(not isinstance(query_id, str) or not query_id for query_id in query_ids)
        or len(set(query_ids)) != EXPECTED_QUERY_COUNT
    ):
        fail("BANK_QUERY_SET_INVALID", "exactly 71 unique non-empty query_id values are required")
    return queries


def identity_zones(path: Path) -> dict[str, str]:
    payload = read_json(path, "IDENTITY_MAP")
    rows = payload.get("rows") if isinstance(payload, dict) else payload
    if not isinstance(rows, list) or not rows:
        fail("IDENTITY_MAP_INVALID", "expected a non-empty array or {rows:[...]}")
    zones: dict[str, str] = {}
    for row in rows:
        if not isinstance(row, dict):
            fail("IDENTITY_MAP_INVALID", "every row must be an object")
        logical = row.get("logical_zone_id")
        runtime = row.get("runtime_access_zone_id")
        if not isinstance(logical, str) or not logical or not isinstance(runtime, str) or not runtime:
            fail("IDENTITY_MAP_INVALID", "logical_zone_id and runtime_access_zone_id are required")
        previous = zones.setdefault(logical, runtime)
        if previous != runtime:
            fail("IDENTITY_MAP_INVALID", f"conflicting runtime zones for {logical}")
    return zones


def evidence_source(payload: dict[str, Any]) -> str:
    for key in ("source", "telemetry_source", "formula_source"):
        value = payload.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    fail("RESOURCE_EVIDENCE_INVALID", "a non-empty factual telemetry/formula source is required")


def load_resource_evidence(path: Path) -> tuple[dict[str, Any], str, str]:
    payload = read_json(path, "RESOURCE_EVIDENCE")
    if not isinstance(payload, dict):
        fail("RESOURCE_EVIDENCE_INVALID", "JSON object required")
    source = evidence_source(payload)
    telemetry = payload.get("telemetry")
    if not isinstance(telemetry, dict):
        fail("RESOURCE_EVIDENCE_INVALID", "telemetry object with bounded counters is required")
    for field in RESOURCE_FIELDS:
        if field not in telemetry:
            fail("RESOURCE_EVIDENCE_INCOMPLETE", f"telemetry.{field} is required")
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    return payload, source, digest


def overlay_resource_telemetry(
    evidence: dict[str, Any], query_id: str, entry_point: str
) -> dict[str, Any]:
    merged = dict(evidence["telemetry"])
    for container_name, key in (
        ("by_entry_point", entry_point),
        ("by_query", query_id),
        ("observations", f"{query_id}:{entry_point}"),
    ):
        container = evidence.get(container_name)
        if isinstance(container, dict) and isinstance(container.get(key), dict):
            merged.update(container[key])
    return merged


def resource_value(spec: Any, field: str, graph_enabled: bool, fallback_source: str) -> tuple[Any, str]:
    source = fallback_source
    value = spec
    if isinstance(spec, dict):
        own_source = spec.get("source") or spec.get("formula_source")
        if isinstance(own_source, str) and own_source.strip():
            source = own_source.strip()
        if "value" in spec:
            value = spec["value"]
        elif graph_enabled and "enabled_value" in spec:
            value = spec["enabled_value"]
        elif not graph_enabled and "disabled_value" in spec:
            value = spec["disabled_value"]
        else:
            fail("RESOURCE_EVIDENCE_INVALID", f"telemetry.{field} has no applicable value")
        bound = spec.get("upper_bound")
        if bound is not None:
            if isinstance(bound, bool) or not isinstance(bound, (int, float)) or not math.isfinite(bound):
                fail("RESOURCE_EVIDENCE_INVALID", f"telemetry.{field}.upper_bound must be finite")
            if isinstance(value, (int, float)) and not isinstance(value, bool) and value > bound:
                fail("RESOURCE_EVIDENCE_OUT_OF_BOUNDS", f"telemetry.{field} exceeds upper_bound")
    if not source:
        fail("RESOURCE_EVIDENCE_INVALID", f"telemetry.{field} has no factual source")
    return value, source


def camel_or_snake(mapping: dict[str, Any], *names: str) -> tuple[Any, str] | tuple[None, None]:
    for name in names:
        if name in mapping:
            return mapping[name], f"response.diagnostics.{name}"
    return None, None


def number(value: Any, source: str, *, integer: bool = False) -> int | float:
    if isinstance(value, str) and value.isascii() and value.isdigit():
        value = int(value)
    valid_type = isinstance(value, int) if integer else isinstance(value, (int, float))
    if isinstance(value, bool) or not valid_type or not math.isfinite(value) or value < 0:
        kind = "non-negative integer" if integer else "finite non-negative number"
        fail("DIAGNOSTIC_INVALID", f"{source} must be a {kind}")
    return int(value) if integer else value


def response_contexts(response: dict[str, Any], entry_point: str) -> list[dict[str, Any]]:
    key = "results" if entry_point == "Search" else "contexts"
    value = response.get(key)
    if not isinstance(value, list) or any(not isinstance(item, dict) for item in value):
        fail("GRPC_RESPONSE_INVALID", f"response.{key} must be an array of objects")
    return value


def context_metadata(context: dict[str, Any]) -> dict[str, Any]:
    citation = context.get("citation")
    if isinstance(citation, dict) and isinstance(citation.get("metadata"), dict):
        return citation["metadata"]
    metadata = context.get("metadata")
    return metadata if isinstance(metadata, dict) else {}


def response_hop_count(
    response: dict[str, Any],
    entry_point: str,
    diagnostics: dict[str, Any],
    graph_enabled: bool,
) -> tuple[int, str]:
    value, source = camel_or_snake(diagnostics, "hopCount", "hop_count")
    if source is not None:
        return number(value, source, integer=True), source
    hops: list[int] = []
    for context in response_contexts(response, entry_point):
        metadata = context_metadata(context)
        raw = metadata.get("graph_hop_distance", metadata.get("graphHopDistance"))
        if isinstance(raw, str) and raw.isascii() and raw.isdigit():
            raw = int(raw)
        if raw is not None:
            hops.append(number(raw, "response context graph_hop_distance", integer=True))
    if hops:
        return max(hops), "max(response contexts metadata.graph_hop_distance)"
    graph_count, graph_source = camel_or_snake(
        diagnostics, "graphCandidatesCount", "graph_candidates_count"
    )
    if graph_source is not None and number(graph_count, graph_source, integer=True) == 0:
        return 0, f"{graph_source} == 0"
    if not graph_enabled:
        return 0, "bank query disables graph; proto3 JSON omits zero hop/count scalars"
    fail("DIAGNOSTIC_INCOMPLETE", "hop count is absent from diagnostics and graph context metadata")


def telemetry_from_response(
    response: dict[str, Any],
    entry_point: str,
    query: dict[str, Any],
    evidence: dict[str, Any],
    evidence_source_name: str,
) -> tuple[dict[str, Any], dict[str, str]]:
    diagnostics = response.get("diagnostics")
    if not isinstance(diagnostics, dict):
        fail("DIAGNOSTIC_INCOMPLETE", "response.diagnostics object is required")
    graph_enabled = query.get("enable_graph_expansion")
    hop_max = query.get("graph_max_hops")
    if not isinstance(graph_enabled, bool):
        fail("BANK_QUERY_INVALID", f"{query.get('query_id')}: enable_graph_expansion must be boolean")
    if isinstance(hop_max, bool) or not isinstance(hop_max, int) or hop_max < 0:
        fail("BANK_QUERY_INVALID", f"{query.get('query_id')}: graph_max_hops must be non-negative")
    telemetry: dict[str, Any] = {}
    sources: dict[str, str] = {}
    returned_context_count = len(response_contexts(response, entry_point))
    diagnostic_fields = {
        "graph_expansion_ms": ("graphExpansionDurationMs", "graph_expansion_duration_ms", "graphMs", "graph_ms"),
        "canonical_graph_hydration_ms": (
            "canonicalGraphHydrationMs",
            "canonical_graph_hydration_ms",
            "postgresHydrationMs",
            "postgres_hydration_ms",
        ),
        "candidates_before_validation": (
            "candidateCount",
            "candidate_count",
            "mergedCandidatesCount",
            "merged_candidates_count",
        ),
        "candidates_after_validation": (
            "finalCandidateCount",
            "final_candidate_count",
            "finalCandidatesCount",
            "final_candidates_count",
        ),
    }
    for field, names in diagnostic_fields.items():
        value, source = camel_or_snake(diagnostics, *names)
        if source is None:
            if field == "graph_expansion_ms" and not graph_enabled:
                telemetry[field] = 0
                sources[field] = (
                    "bank query disables graph; proto3 JSON omits zero graph duration scalars"
                )
                continue
            if field == "candidates_after_validation" and returned_context_count == 0:
                telemetry[field] = 0
                sources[field] = (
                    "empty response cardinality; proto3 JSON omits zero final candidate scalar"
                )
                continue
            fail("DIAGNOSTIC_INCOMPLETE", f"response diagnostics do not provide {field}")
        telemetry[field] = number(value, source, integer=field.startswith("candidates_"))
        sources[field] = source

    telemetry["candidate_max"] = 64
    sources["candidate_max"] = "Search request.candidateLimit / RetrieveContext bounded profile contract"
    telemetry["hop_count"], sources["hop_count"] = response_hop_count(
        response, entry_point, diagnostics, graph_enabled
    )
    telemetry["hop_max"] = hop_max
    sources["hop_max"] = "bank query.graph_max_hops and request.graphMaxHops"
    telemetry["graph_executed"] = graph_enabled
    sources["graph_executed"] = "bank query.enable_graph_expansion and request.enableGraphExpansion"

    resource_specs = overlay_resource_telemetry(evidence, query["query_id"], entry_point)
    for field in RESOURCE_FIELDS:
        value, source = resource_value(resource_specs[field], field, graph_enabled, evidence_source_name)
        if field == "n_plus_one_sql_hydration":
            if not isinstance(value, bool):
                fail("RESOURCE_EVIDENCE_INVALID", f"telemetry.{field} must be boolean")
        else:
            value = number(value, f"resource evidence telemetry.{field}", integer=True)
        telemetry[field] = value
        sources[field] = source
    return telemetry, sources


def request_for(
    query: dict[str, Any],
    entry_point: str,
    runtime_zone: str,
    run_kind: str,
    run_identity: str,
    deadline_ms: int,
) -> dict[str, Any]:
    query_id = query["query_id"]
    question = query.get("question")
    max_contexts = query.get("max_contexts")
    graph_enabled = query.get("enable_graph_expansion")
    graph_hops = query.get("graph_max_hops")
    if not isinstance(question, str) or not question or isinstance(max_contexts, bool) or not isinstance(max_contexts, int) or max_contexts < 1:
        fail("BANK_QUERY_INVALID", f"{query_id}: question and positive max_contexts are required")
    correlation = f"fix486g-statistical-{run_kind}-{run_identity}-{query_id}-{entry_point.lower()}"
    if entry_point == "Search":
        return {
            "correlationId": correlation,
            "accessZoneId": runtime_zone,
            "callerAccessLevel": "INTERNAL",
            "query": question,
            "topK": max_contexts,
            "candidateLimit": 64,
            "parentLimit": max_contexts,
            "timeoutMs": deadline_ms,
            "searchMode": "SEARCH_MODE_V005_HYBRID",
            "embeddingMode": "EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE",
            "includeDebug": True,
            "enableGraphExpansion": graph_enabled,
            "graphMaxHops": graph_hops,
            "graphMaxRelatedContexts": max_contexts,
        }
    return {
        "context": {
            "correlationId": correlation,
            "callerService": "fix486g-statistical-capture",
            "callerUserId": "fix486g-statistical-capture",
            "callerAccessLevel": "INTERNAL",
        },
        "accessZoneId": runtime_zone,
        "question": question,
        "profile": "RETRIEVAL_PROFILE_BALANCED",
        "maxContexts": max_contexts,
        "responseDetail": "RESPONSE_DETAIL_DEBUG",
        "enableGraphExpansion": graph_enabled,
        "graphMaxHops": graph_hops,
        "graphMaxRelatedContexts": max_contexts,
    }


def normalized_status(response: dict[str, Any], entry_point: str) -> tuple[str, str]:
    status = response.get("status")
    if isinstance(status, str) and status:
        return status, "response.status"
    if entry_point == "RetrieveContext":
        summary = response.get("summary")
        raw = summary.get("evidenceStatus") if isinstance(summary, dict) else None
        if isinstance(raw, str) and raw:
            return raw.removeprefix("EVIDENCE_STATUS_"), "response.summary.evidenceStatus"
    contexts = response_contexts(response, entry_point)
    return ("FOUND" if contexts else "NO_ANSWER"), "response result cardinality"


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def invoke(
    grpcurl_bin: str,
    endpoint: str,
    method: str,
    request: dict[str, Any],
    deadline_ms: int,
    jitter_allowance_ms: int,
) -> tuple[dict[str, Any], str, float, int, int]:
    encoded = json.dumps(request, ensure_ascii=False, separators=(",", ":"))
    started_unix_ns = time.time_ns()
    started = time.perf_counter_ns()
    try:
        completed = subprocess.run(
            [grpcurl_bin, "-plaintext", "-emit-defaults", "-d", "@", endpoint, method],
            input=encoded,
            text=True,
            capture_output=True,
            timeout=(deadline_ms + jitter_allowance_ms) / 1000.0,
            check=False,
        )
    except subprocess.TimeoutExpired:
        elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
        fail(
            "GRPC_DEADLINE_EXCEEDED",
            f"{method} exceeded deadline+jitter={deadline_ms + jitter_allowance_ms} ms "
            f"(wall={elapsed_ms:.3f} ms)",
        )
    except OSError as error:
        fail("GRPCURL_EXEC_FAILED", f"{grpcurl_bin}: {error}")
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    finished_unix_ns = time.time_ns()
    if completed.returncode != 0:
        fail("GRPC_CALL_FAILED", f"{method}: exit={completed.returncode}: {completed.stderr.strip()}")
    try:
        response = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        fail("GRPC_RESPONSE_INVALID", f"{method}: {error}")
    if not isinstance(response, dict):
        fail("GRPC_RESPONSE_INVALID", f"{method}: JSON object required")
    return response, completed.stderr, elapsed_ms, started_unix_ns, finished_unix_ns


def observation_key(row: dict[str, Any]) -> tuple[Any, ...]:
    return (
        row.get("run_kind"),
        row.get("run_index"),
        row.get("pair_id"),
        row.get("query_id"),
        row.get("entry_point"),
    )


def append_rows(output: Path, rows: list[dict[str, Any]]) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        try:
            existing = [json.loads(line) for line in output.read_text(encoding="utf-8").splitlines() if line.strip()]
        except (OSError, json.JSONDecodeError) as error:
            fail("OUTPUT_INVALID", f"cannot validate existing JSONL {output}: {error}")
        new_keys = {observation_key(row) for row in rows}
        for row in existing:
            key = observation_key(row)
            if key in new_keys:
                fail("OUTPUT_DUPLICATE", f"observation already exists: {key}")
    with output.open("a", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n")


def select_queries(queries: list[dict[str, Any]], args: argparse.Namespace) -> list[dict[str, Any]]:
    selected = queries
    if args.query_id:
        requested = set(args.query_id)
        known = {query["query_id"] for query in queries}
        unknown = sorted(requested - known)
        if unknown:
            fail("QUERY_SELECTION_INVALID", f"unknown query IDs: {unknown}")
        selected = [query for query in queries if query["query_id"] in requested]
    elif args.fault_setup:
        selected = [query for query in queries if query.get("fault_setup") == args.fault_setup]
    elif args.exclude_faults:
        selected = [query for query in queries if not query.get("fault_setup")]
    if not selected:
        fail("QUERY_SELECTION_EMPTY", "selection produced no queries")
    return selected


def degradation_evidence(path: Path | None) -> tuple[dict[str, Any] | None, dict[str, Any] | None]:
    if path is None:
        return None, None
    payload = read_json(path, "DEGRADATION_EVIDENCE")
    if not isinstance(payload, dict) or payload.get("schema_version") != 1:
        fail("DEGRADATION_EVIDENCE_INVALID", "schema_version=1 object required")
    source = payload.get("source")
    degradation = payload.get("degradation")
    if not isinstance(source, str) or not source.strip() or not isinstance(degradation, dict):
        fail("DEGRADATION_EVIDENCE_INVALID", "source and degradation object are required")
    identity = {
        "path": str(path),
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "source": source.strip(),
    }
    return degradation, identity


def capture(args: argparse.Namespace) -> int:
    queries = select_queries(read_queries(args.bank), args)
    zones = identity_zones(args.identity_map)
    evidence, source, evidence_sha256 = load_resource_evidence(args.resource_evidence)
    degradation, degradation_identity = degradation_evidence(args.degradation_evidence)
    selected_faults = {query.get("fault_setup") for query in queries if query.get("fault_setup")}
    if degradation is not None:
        declared_setup = read_json(args.degradation_evidence, "DEGRADATION_EVIDENCE").get("fault_setup")
        if selected_faults and selected_faults != {declared_setup}:
            fail(
                "DEGRADATION_EVIDENCE_MISMATCH",
                f"selected fault setups {sorted(selected_faults)} do not match {declared_setup!r}",
            )
    for query in queries:
        logical_zone = query.get("access_zone")
        if logical_zone not in zones:
            fail("IDENTITY_MAP_INCOMPLETE", f"no runtime access zone for {logical_zone!r}")

    concurrent = args.run_kind.startswith("concurrent_")
    run_identity = args.pair_id if concurrent else f"{args.run_index:03d}"
    entry_points = (args.entry_point,) if args.entry_point else ENTRY_POINTS
    raw_root = args.output.parent / f"{args.output.stem}.raw" / args.run_kind / run_identity
    rows: list[dict[str, Any]] = []
    for query in queries:
        for entry_point in entry_points:
            request = request_for(
                query,
                entry_point,
                zones[query["access_zone"]],
                args.run_kind,
                run_identity,
                args.deadline_ms,
            )
            call_dir = raw_root / query["query_id"] / entry_point.lower()
            write_json(call_dir / "request.json", request)
            response, stderr, latency_ms, started_unix_ns, finished_unix_ns = invoke(
                args.grpcurl_bin,
                args.endpoint,
                METHODS[entry_point],
                request,
                args.deadline_ms,
                args.jitter_allowance_ms,
            )
            write_json(call_dir / "response.json", response)
            (call_dir / "stderr.txt").write_text(stderr, encoding="utf-8")
            telemetry, telemetry_sources = telemetry_from_response(
                response, entry_point, query, evidence, source
            )
            evaluator_response = dict(response)
            status, status_source = normalized_status(response, entry_point)
            evaluator_response.setdefault("status", status)
            row: dict[str, Any] = {
                "schema_version": 1,
                "query_id": query["query_id"],
                "entry_point": entry_point,
                "run_kind": args.run_kind,
                "latency_ms": latency_ms,
                "started_at_unix_ns": started_unix_ns,
                "finished_at_unix_ns": finished_unix_ns,
                "deadline_ms": args.deadline_ms,
                "jitter_allowance_ms": float(args.jitter_allowance_ms),
                "telemetry": telemetry,
                "telemetry_sources": telemetry_sources,
                "resource_evidence": {
                    "path": str(args.resource_evidence),
                    "sha256": evidence_sha256,
                    "source": source,
                },
                "response_status_source": status_source,
                "raw_request_path": str(call_dir / "request.json"),
                "raw_response_path": str(call_dir / "response.json"),
                "response": evaluator_response,
            }
            if concurrent:
                row["pair_id"] = args.pair_id
            else:
                row["run_index"] = args.run_index
            if degradation is not None:
                row["degradation"] = degradation
                row["degradation_evidence"] = degradation_identity
            elif isinstance(response.get("degradation"), dict):
                row["degradation"] = response["degradation"]
            rows.append(row)

    expected = len(queries) * len(entry_points)
    if len(rows) != expected:
        fail("CAPTURE_INCOMPLETE", f"expected {expected} rows, got {len(rows)}")
    append_rows(args.output, rows)
    return len(rows)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--endpoint", required=True)
    result.add_argument("--bank", type=Path, required=True)
    result.add_argument("--identity-map", type=Path, required=True)
    result.add_argument(
        "--run-kind",
        choices=("warm", "restart", "concurrent_fault", "concurrent_healthy"),
        required=True,
    )
    result.add_argument("--run-index", type=int)
    result.add_argument("--pair-id")
    result.add_argument("--entry-point", choices=ENTRY_POINTS)
    selection = result.add_mutually_exclusive_group()
    selection.add_argument("--query-id", action="append")
    selection.add_argument("--fault-setup")
    selection.add_argument("--exclude-faults", action="store_true")
    result.add_argument("--output", type=Path, required=True)
    result.add_argument("--deadline-ms", type=int, required=True)
    result.add_argument("--jitter-allowance-ms", type=int, default=1000)
    result.add_argument("--resource-evidence", type=Path, required=True)
    result.add_argument("--degradation-evidence", type=Path)
    result.add_argument("--grpcurl-bin", default="grpcurl")
    return result


def main() -> int:
    args = parser().parse_args()
    concurrent = args.run_kind.startswith("concurrent_")
    if concurrent:
        if not args.pair_id or args.run_index is not None or not args.entry_point or not args.query_id or len(args.query_id) != 1:
            fail(
                "ARGUMENT_INVALID",
                "concurrent capture requires --pair-id, --entry-point and exactly one --query-id, without --run-index",
            )
        if args.degradation_evidence is None:
            fail("ARGUMENT_INVALID", "concurrent capture requires --degradation-evidence")
    elif args.run_index is None or args.run_index < 1 or args.pair_id is not None:
        fail("ARGUMENT_INVALID", "warm/restart capture requires --run-index >= 1 and no --pair-id")
    if args.deadline_ms < 1:
        fail("ARGUMENT_INVALID", "--deadline-ms must be at least 1")
    if args.jitter_allowance_ms < 0:
        fail("ARGUMENT_INVALID", "--jitter-allowance-ms must be non-negative")
    observation_count = capture(args)
    print(
        json.dumps(
            {
                "status": "PASS",
                "observations_appended": observation_count,
                "output": str(args.output),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except CaptureError as error:
        print(f"FIX486G_STATISTICAL_CAPTURE_FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
