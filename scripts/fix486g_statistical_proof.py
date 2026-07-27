#!/usr/bin/env python3
"""Offline, fail-closed evaluator for the frozen FIX486G supplemental bank.

The program never performs network calls.  It consumes JSONL envelopes containing
raw Search/RetrieveContext responses plus capture-time timing/resource evidence.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable

try:
    from scripts.fix486g_fault_contract import FAULT_CONTRACT_BY_SETUP
except ModuleNotFoundError:
    from fix486g_fault_contract import FAULT_CONTRACT_BY_SETUP

PASS = "FIX486G_STATISTICAL_QUALITY_PASS"
BLOCKED = "FIX486G_STATISTICAL_QUALITY_BLOCKED"
ENTRY_POINTS = ("Search", "RetrieveContext")
FULL_RUN_KINDS = ("warm", "restart")
CONCURRENT_KINDS = ("concurrent_fault", "concurrent_healthy")
MIN_WARM_REPEATS = 3
MIN_RESTART_REPEATS = 2
MIN_CONCURRENT_PAIRS = 10
EXPECTED_QUERY_COUNT = 71
ARTIFACTS = (
    "statistical-report.json",
    "statistical-report.md",
    "per-query-results.jsonl",
    "per-slice-metrics.json",
    "latency-distribution.json",
    "safety-hard-gates.json",
    "confidence-intervals.json",
)
PROPORTION_THRESHOLDS = {
    "GraphParentRecall@1": 0.90,
    "GraphParentRecall@3": 0.97,
    "GraphParentRecall@5": 0.99,
    "GraphParentAccuracy": 1.0,
    "GraphEdgePrecision": 1.0,
    "GraphProvenanceCompleteness": 1.0,
    "GraphContributionRate": 0.95,
    "DirectPreservationRate": 1.0,
    "NoAnswerSpecificity": 1.0,
}
MEAN_THRESHOLDS = {"MRR": 0.94, "nDCG@5": 0.95}
REQUIRED_PROVENANCE = (
    "graph_seed_access_zone_id",
    "graph_seed_document_id",
    "graph_seed_document_version",
    "graph_seed_chunk_id",
    "graph_seed_parent_chunk_id",
    "graph_relation_id",
    "graph_edge_id",
    "graph_relation_type",
    "graph_relation_score",
    "graph_related_access_zone_id",
    "graph_related_document_id",
    "graph_related_document_version",
    "graph_related_chunk_id",
    "graph_related_parent_chunk_id",
    "graph_hop_distance",
)
GLOBAL_HARD_GATES = (
    "cross_zone_graph_final_contexts",
    "wrong_parent_graph_final_contexts",
    "seed_parent_reuse_final_contexts",
    "inactive_deleted_expired_graph_final_contexts",
    "binding_invalid_graph_final_contexts",
    "hop_limit_violation_final_contexts",
    "cycle_credit_inflation_events",
    "false_graph_attribution_events",
    "forbidden_anchor_leaks",
    "graph_disabled_execution_count",
    "request_deadline_violations",
    "candidate_bound_violations",
    "n_plus_one_sql_hydration_events",
    "search_retrieve_parity_failures",
)
METRIC_VERSION = "fix486g-statistical-v1"


class EvaluationError(ValueError):
    """An evidence or contract failure that must block evaluation."""


def fail(code: str, detail: str) -> None:
    raise EvaluationError(f"{code}: {detail}")


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail("JSON_READ_FAILED", f"{path}: {error}")


def read_jsonl(paths: Iterable[Path]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for path in paths:
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except OSError as error:
            fail("JSONL_READ_FAILED", f"{path}: {error}")
        for line_number, line in enumerate(lines, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                fail("JSONL_INVALID", f"{path}:{line_number}: {error}")
            if not isinstance(row, dict):
                fail("JSONL_ROW_INVALID", f"{path}:{line_number}: object required")
            row["_source"] = f"{path}:{line_number}"
            rows.append(row)
    return rows


def deep_merge(base: dict[str, Any], overlay: dict[str, Any]) -> dict[str, Any]:
    merged = dict(base)
    for key, value in overlay.items():
        if key == "extends":
            continue
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = deep_merge(merged[key], value)
        else:
            merged[key] = value
    return merged


def verify_and_load_bank(bank: Path) -> dict[str, Any]:
    manifest = read_json(bank / "bank-manifest.json")
    if not isinstance(manifest, dict):
        fail("BANK_MANIFEST_INVALID", "object required")
    if (manifest.get("version"), manifest.get("status"), manifest.get("query_count")) != (
        "1.0.0",
        "FROZEN",
        EXPECTED_QUERY_COUNT,
    ):
        fail("BANK_IDENTITY_INVALID", "expected frozen 1.0.0 bank with 71 queries")
    files = manifest.get("files") or {}
    required = ("queries", "qrel_profiles", "qrel_assignments", "fault_plans")
    if any(not isinstance(files.get(name), str) for name in required):
        fail("BANK_MANIFEST_INVALID", "missing bank file identities")
    expected_hashes = manifest.get("hashes", {}).get("files", {})
    actual_hashes: dict[str, str] = {}
    for relative in files.values():
        path = bank / relative
        if not path.is_file():
            fail("BANK_FILE_MISSING", str(path))
        actual_hashes[relative] = sha256(path)
    if actual_hashes != expected_hashes:
        fail("BANK_HASH_MISMATCH", "frozen payload hashes differ from manifest")
    aggregate_source = "".join(f"{name}\0{actual_hashes[name]}\n" for name in sorted(actual_hashes))
    aggregate = hashlib.sha256(aggregate_source.encode("utf-8")).hexdigest()
    if aggregate != manifest.get("hashes", {}).get("aggregate_sha256"):
        fail("BANK_HASH_MISMATCH", "aggregate hash differs from manifest")

    queries = read_jsonl([bank / files["queries"]])
    for row in queries:
        row.pop("_source", None)
    assignments = read_jsonl([bank / files["qrel_assignments"]])
    for row in assignments:
        row.pop("_source", None)
    profiles_doc = read_json(bank / files["qrel_profiles"])
    fault_doc = read_json(bank / files["fault_plans"])
    if len(queries) != EXPECTED_QUERY_COUNT or len({row.get("query_id") for row in queries}) != EXPECTED_QUERY_COUNT:
        fail("BANK_QUERY_SET_INVALID", "query IDs must be 71 unique non-empty values")
    by_query = {row["query_id"]: row for row in queries}
    assignment_map: dict[str, str] = {}
    for assignment in assignments:
        query_id = assignment.get("query_id")
        if query_id in assignment_map:
            fail("QREL_ASSIGNMENT_DUPLICATE", str(query_id))
        assignment_map[query_id] = assignment.get("qrel_profile")
    if set(assignment_map) != set(by_query):
        fail("QREL_ASSIGNMENT_INCOMPLETE", "every query needs exactly one assignment")

    raw_profiles = profiles_doc.get("profiles") or {}
    resolved_profiles: dict[str, dict[str, Any]] = {}

    def resolve(name: str, stack: tuple[str, ...] = ()) -> dict[str, Any]:
        if name in resolved_profiles:
            return resolved_profiles[name]
        if name in stack or name not in raw_profiles:
            fail("QREL_PROFILE_INVALID", f"unknown/cyclic profile {name}")
        profile = raw_profiles[name]
        parent = resolve(profile["extends"], stack + (name,)) if profile.get("extends") else {}
        resolved_profiles[name] = deep_merge(parent, profile)
        return resolved_profiles[name]

    for name in raw_profiles:
        resolve(name)
    for query_id, profile in assignment_map.items():
        if profile not in resolved_profiles:
            fail("QREL_PROFILE_UNKNOWN", f"{query_id}: {profile}")

    fault_by_setup = {item["fault_setup"]: item for item in fault_doc.get("fault_plans", [])}
    return {
        "manifest": manifest,
        "aggregate_sha256": aggregate,
        "queries": by_query,
        "assignments": assignment_map,
        "profiles": resolved_profiles,
        "fault_by_setup": fault_by_setup,
    }


def build_plan(bank_data: dict[str, Any], warm: int, restart: int, pairs: int) -> dict[str, Any]:
    if warm < MIN_WARM_REPEATS or restart < MIN_RESTART_REPEATS or pairs < MIN_CONCURRENT_PAIRS:
        fail(
            "SAMPLE_PLAN_TOO_SMALL",
            f"warm>={MIN_WARM_REPEATS}, restart>={MIN_RESTART_REPEATS}, concurrent_pairs>={MIN_CONCURRENT_PAIRS}",
        )
    per_pass = EXPECTED_QUERY_COUNT * len(ENTRY_POINTS)
    return {
        "schema_version": 1,
        "status": "PASS",
        "bank_id": bank_data["manifest"]["bank_id"],
        "bank_version": bank_data["manifest"]["version"],
        "bank_aggregate_sha256": bank_data["aggregate_sha256"],
        "network_calls": False,
        "query_count": EXPECTED_QUERY_COUNT,
        "entry_points": list(ENTRY_POINTS),
        "results_per_full_pass": per_pass,
        "full_passes": {
            "warm": {"minimum": MIN_WARM_REPEATS, "planned": warm, "run_indices": list(range(1, warm + 1))},
            "restart": {
                "minimum": MIN_RESTART_REPEATS,
                "planned": restart,
                "run_indices": list(range(1, restart + 1)),
            },
        },
        "concurrent_pairs": {"minimum": MIN_CONCURRENT_PAIRS, "planned": pairs},
        "minimum_raw_observations": per_pass * (warm + restart) + pairs * 2,
        "raw_jsonl_contract": {
            "required": [
                "schema_version=1",
                "query_id",
                "entry_point=Search|RetrieveContext",
                "run_kind=warm|restart|concurrent_fault|concurrent_healthy",
                "run_index (warm/restart) or pair_id (concurrent)",
                "latency_ms",
                "started_at_unix_ns",
                "finished_at_unix_ns",
                "deadline_ms",
                "jitter_allowance_ms",
                "telemetry",
                "response",
            ],
            "telemetry_required": [
                "graph_expansion_ms",
                "canonical_graph_hydration_ms",
                "candidates_before_validation",
                "candidates_after_validation",
                "candidate_max",
                "hop_count",
                "hop_max",
                "sql_statement_count",
                "qdrant_request_count",
                "graph_relation_query_count",
                "n_plus_one_sql_hydration=false",
                "graph_executed",
            ],
            "fault_rows_require_degradation": True,
            "response_shape": "raw Search.results or RetrieveContext.contexts JSON",
        },
        "required_artifacts": list(ARTIFACTS),
    }


def identity_lookup(path: Path | None) -> dict[str, dict[str, Any]]:
    if path is None:
        return {}
    payload = read_json(path)
    rows = payload.get("rows") if isinstance(payload, dict) else payload
    if not isinstance(rows, list):
        fail("IDENTITY_MAP_INVALID", "expected array or {rows:[...]}")
    lookup: dict[str, dict[str, Any]] = {}
    for row in rows:
        runtime = row.get("runtime_chunk_id") if isinstance(row, dict) else None
        if not runtime or runtime in lookup:
            fail("IDENTITY_MAP_INVALID", "runtime_chunk_id must be unique and non-empty")
        lookup[runtime] = row
    return lookup


def finite_number(value: Any, name: str, *, minimum: float = 0.0) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < minimum:
        fail("RAW_FIELD_INVALID", f"{name} must be a finite number >= {minimum}")
    return float(value)


def integer(value: Any, name: str, *, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        fail("RAW_FIELD_INVALID", f"{name} must be an integer >= {minimum}")
    return value


def validate_rows(rows: list[dict[str, Any]], bank_data: dict[str, Any]) -> dict[str, Any]:
    if not rows:
        fail("RAW_INPUT_EMPTY", "at least one JSONL row is required")
    query_ids = set(bank_data["queries"])
    full_keys: set[tuple[str, int, str, str]] = set()
    run_indices: dict[str, set[int]] = defaultdict(set)
    pair_rows: dict[str, list[dict[str, Any]]] = defaultdict(list)
    telemetry_fields = (
        "graph_expansion_ms",
        "canonical_graph_hydration_ms",
        "candidates_before_validation",
        "candidates_after_validation",
        "candidate_max",
        "hop_count",
        "hop_max",
        "sql_statement_count",
        "qdrant_request_count",
        "graph_relation_query_count",
        "n_plus_one_sql_hydration",
        "graph_executed",
    )
    for row in rows:
        source = row.get("_source", "raw row")
        if row.get("schema_version") != 1:
            fail("RAW_SCHEMA_INVALID", f"{source}: schema_version must be 1")
        query_id = row.get("query_id")
        if query_id not in query_ids:
            fail("RAW_QUERY_UNKNOWN", f"{source}: {query_id}")
        entry = row.get("entry_point")
        if entry not in ENTRY_POINTS:
            fail("RAW_ENTRY_POINT_INVALID", f"{source}: {entry}")
        kind = row.get("run_kind")
        if kind not in FULL_RUN_KINDS + CONCURRENT_KINDS:
            fail("RAW_RUN_KIND_INVALID", f"{source}: {kind}")
        finite_number(row.get("latency_ms"), f"{source}.latency_ms")
        started_at = integer(row.get("started_at_unix_ns"), f"{source}.started_at_unix_ns", minimum=1)
        finished_at = integer(row.get("finished_at_unix_ns"), f"{source}.finished_at_unix_ns", minimum=1)
        if finished_at <= started_at:
            fail("RAW_TIME_INTERVAL_INVALID", f"{source}: finished_at_unix_ns must be after start")
        finite_number(row.get("deadline_ms"), f"{source}.deadline_ms", minimum=1)
        finite_number(row.get("jitter_allowance_ms"), f"{source}.jitter_allowance_ms")
        if not isinstance(row.get("response"), dict):
            fail("RAW_RESPONSE_INVALID", f"{source}: response object required")
        telemetry = row.get("telemetry")
        if not isinstance(telemetry, dict) or any(field not in telemetry for field in telemetry_fields):
            fail("RAW_TELEMETRY_INCOMPLETE", f"{source}: required={telemetry_fields}")
        for field in telemetry_fields[:2]:
            finite_number(telemetry[field], f"{source}.telemetry.{field}")
        for field in telemetry_fields[2:10]:
            integer(telemetry[field], f"{source}.telemetry.{field}")
        for field in telemetry_fields[10:]:
            if not isinstance(telemetry[field], bool):
                fail("RAW_FIELD_INVALID", f"{source}.telemetry.{field} must be boolean")
        if kind in FULL_RUN_KINDS:
            index = integer(row.get("run_index"), f"{source}.run_index", minimum=1)
            key = (kind, index, query_id, entry)
            if key in full_keys:
                fail("RAW_OBSERVATION_DUPLICATE", str(key))
            full_keys.add(key)
            run_indices[kind].add(index)
        else:
            pair_id = row.get("pair_id")
            if not isinstance(pair_id, str) or not pair_id:
                fail("CONCURRENT_PAIR_INVALID", f"{source}: pair_id required")
            pair_rows[pair_id].append(row)

    expected_matrix = {(query_id, entry) for query_id in query_ids for entry in ENTRY_POINTS}
    minimums = {"warm": MIN_WARM_REPEATS, "restart": MIN_RESTART_REPEATS}
    pass_counts: dict[str, int] = {}
    for kind, minimum in minimums.items():
        indices = run_indices.get(kind, set())
        if len(indices) < minimum or indices != set(range(1, max(indices, default=0) + 1)):
            fail("FULL_PASS_PLAN_INVALID", f"{kind}: contiguous indices from 1 and at least {minimum} required")
        for index in indices:
            actual = {(qid, entry) for run_kind, run_index, qid, entry in full_keys if run_kind == kind and run_index == index}
            if actual != expected_matrix or len(actual) != EXPECTED_QUERY_COUNT * 2:
                missing = sorted(expected_matrix - actual)[:5]
                extra = sorted(actual - expected_matrix)[:5]
                fail(
                    "FULL_PASS_INCOMPLETE",
                    f"{kind}/{index}: expected=142 actual={len(actual)} missing={missing} extra={extra}",
                )
        pass_counts[kind] = len(indices)

    if len(pair_rows) < MIN_CONCURRENT_PAIRS:
        fail("CONCURRENT_SAMPLE_TOO_SMALL", f"expected at least {MIN_CONCURRENT_PAIRS}, got {len(pair_rows)}")
    for pair_id, pair in pair_rows.items():
        kinds = {row["run_kind"] for row in pair}
        entries = {row["entry_point"] for row in pair}
        if len(pair) != 2 or kinds != set(CONCURRENT_KINDS) or len(entries) != 1:
            fail("CONCURRENT_PAIR_INVALID", f"{pair_id}: exactly one fault and one healthy row for one entry point")
        overlap_start = max(row["started_at_unix_ns"] for row in pair)
        overlap_end = min(row["finished_at_unix_ns"] for row in pair)
        if overlap_start >= overlap_end:
            fail("CONCURRENT_PAIR_NOT_OVERLAPPING", f"{pair_id}: request intervals do not overlap")
        fault = next(row for row in pair if row["run_kind"] == "concurrent_fault")
        if not bank_data["queries"][fault["query_id"]].get("fault_setup"):
            fail("CONCURRENT_PAIR_INVALID", f"{pair_id}: fault row must use an adversarial query")
        healthy = next(row for row in pair if row["run_kind"] == "concurrent_healthy")
        if bank_data["queries"][healthy["query_id"]].get("fault_setup"):
            fail("CONCURRENT_PAIR_INVALID", f"{pair_id}: healthy row cannot use an adversarial query")
        degradation = healthy.get("degradation")
        if not isinstance(degradation, dict) or not isinstance(degradation.get("healthy_request_affected"), bool):
            fail("DEGRADATION_EVIDENCE_INCOMPLETE", f"{healthy['_source']}: healthy_request_affected required")

    return {
        "status": "PASS",
        "raw_observation_count": len(rows),
        "query_count": EXPECTED_QUERY_COUNT,
        "results_per_full_pass": EXPECTED_QUERY_COUNT * 2,
        "full_pass_counts": pass_counts,
        "concurrent_pair_count": len(pair_rows),
    }


def metadata(context: dict[str, Any]) -> dict[str, Any]:
    citation = context.get("citation") or {}
    value = citation.get("metadata") or context.get("metadata") or {}
    return value if isinstance(value, dict) else {}


def contexts(row: dict[str, Any]) -> list[dict[str, Any]]:
    key = "results" if row["entry_point"] == "Search" else "contexts"
    value = row["response"].get(key)
    if not isinstance(value, list) or any(not isinstance(item, dict) for item in value):
        fail("RAW_RESPONSE_INVALID", f"{row['_source']}: response.{key} must be an array of objects")
    return value


def logical_chunk(context: dict[str, Any], runtime_key: str, logical_keys: tuple[str, ...], identities: dict[str, dict[str, Any]]) -> str | None:
    for key in logical_keys:
        value = context.get(key)
        if isinstance(value, str) and value:
            return value
    raw = context.get(runtime_key)
    if isinstance(raw, str) and raw in identities:
        return identities[raw].get("logical_chunk_id")
    if isinstance(raw, str) and raw.startswith(("parent-", "child-")):
        return raw
    return None


def primary_graph_origin(meta: dict[str, Any]) -> bool:
    return (meta.get("retrieval_source") or meta.get("retrievalSource")) == "GRAPH_EXPANDED"


def graph_provenance(meta: dict[str, Any]) -> bool:
    return primary_graph_origin(meta) or (
        str(meta.get("graph_secondary_provenance", "")).lower() == "true"
        and present(meta.get("graph_edge_id"))
        and present(meta.get("graph_related_chunk_id"))
    )


def normalized_context(context: dict[str, Any], identities: dict[str, dict[str, Any]]) -> dict[str, Any]:
    meta = metadata(context)
    matched = context.get("matchedChunkId") or context.get("matched_chunk_id")
    parent = context.get("parentChunkId") or context.get("parent_chunk_id")
    # Graph evidence is hydrated as a parent context, so the response-level
    # matched ID may be that parent. The canonical matched child remains in
    # protected Graph provenance and is the identity the qrel evaluates.
    if graph_provenance(meta):
        related = meta.get("graph_related_chunk_id") or meta.get("graphRelatedChunkId")
        if isinstance(related, str) and related:
            matched = related
    matched_logical = logical_chunk(
        {"matchedChunkId": matched},
        "matchedChunkId",
        (),
        identities,
    )
    parent_logical = logical_chunk(
        context,
        "parentChunkId" if "parentChunkId" in context else "parent_chunk_id",
        ("parentLogicalId", "parent_logical_id"),
        identities,
    )
    identity = identities.get(matched, {}) if isinstance(matched, str) else {}
    zone = context.get("logicalAccessZoneId") or context.get("logical_access_zone_id") or identity.get("logical_zone_id")
    if zone is None and context.get("accessZoneId") in {"zone-a", "zone-b"}:
        zone = context.get("accessZoneId")
    parent_text = context.get("parentText") or context.get("parent_text") or ""
    matched_text = context.get("matchedText") or context.get("matched_text") or ""
    return {
        "matched_runtime": matched,
        "parent_runtime": parent,
        "matched_logical": matched_logical,
        "parent_logical": parent_logical,
        "zone": zone,
        "primary_graph_origin": primary_graph_origin(meta),
        "graph_provenance": graph_provenance(meta),
        "metadata": meta,
        "parent_text": parent_text,
        "matched_text": matched_text,
        "combined_text": f"{matched_text}\n{parent_text}",
        "document_version": as_int(context.get("documentVersion") or context.get("document_version")),
    }


def present(value: Any) -> bool:
    return value is not None and value != "" and value != []


def as_int(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.isascii() and value.isdigit():
        return int(value)
    return None


def add_gate(gates: dict[str, int], *names: str) -> None:
    for name in names:
        gates[name] += 1


def ndcg(grades: list[int], ideal_grades: list[int], k: int) -> float:
    def dcg(values: list[int]) -> float:
        return sum((2 ** max(grade, 0) - 1) / math.log2(index + 2) for index, grade in enumerate(values[:k]))

    ideal = dcg(sorted(ideal_grades, reverse=True))
    return dcg(grades) / ideal if ideal else 0.0


def evaluate_observation(
    row: dict[str, Any], bank_data: dict[str, Any], identities: dict[str, dict[str, Any]], gates: dict[str, int]
) -> dict[str, Any]:
    query = bank_data["queries"][row["query_id"]]
    profile_name = bank_data["assignments"][row["query_id"]]
    qrel = bank_data["profiles"][profile_name]
    normalized = [normalized_context(item, identities) for item in contexts(row)]
    required_relations = set(qrel.get("required_graph_relation_any") or [])
    graph_rows = [
        item for item in normalized
        if item["graph_provenance"]
        and item["metadata"].get("graph_relation_type") in required_relations
    ]
    direct_rows = [item for item in normalized if not item["primary_graph_origin"]]
    expected_graph = qrel.get("expected_graph_parent")
    expected_direct = qrel.get("expected_direct_parent")
    fault_setup = query.get("fault_setup")
    fault_contract = FAULT_CONTRACT_BY_SETUP.get(fault_setup) if fault_setup else None
    if fault_setup and fault_contract is None:
        fail("FAULT_VALIDATION_CONTRACT_UNKNOWN", fault_setup)
    graph_survivor_required = (
        not fault_contract or fault_contract["survivor_mode"] == "GRAPH"
    )
    forbidden_anchors = qrel.get("forbidden_anchors") or []
    failures: list[str] = []
    valid_graph_contexts = 0
    complete_graph_contexts = 0
    edge_valid_contexts = 0
    graph_keys: set[tuple[Any, ...]] = set()

    for item in normalized:
        meta = item["metadata"]
        forbidden = [anchor for anchor in forbidden_anchors if anchor in item["combined_text"]]
        if forbidden:
            add_gate(gates, "forbidden_anchor_leaks")
            failures.append("FORBIDDEN_ANCHOR_LEAK")
        if item in graph_rows:
            if item["zone"] != qrel.get("expected_zone"):
                add_gate(gates, "cross_zone_graph_final_contexts", "graph_cross_zone_results")
                failures.append("CROSS_ZONE_GRAPH_CONTEXT")
            if expected_graph and item["parent_logical"] != expected_graph:
                add_gate(gates, "wrong_parent_graph_final_contexts", "graph_wrong_parent")
                failures.append("WRONG_GRAPH_PARENT")
            seed_parent = meta.get("graph_seed_parent_chunk_id")
            related_parent = meta.get("graph_related_parent_chunk_id")
            if present(seed_parent) and seed_parent == related_parent:
                add_gate(gates, "seed_parent_reuse_final_contexts", "graph_seed_parent_reuse")
                failures.append("GRAPH_SEED_PARENT_REUSE")
            lifecycle = str(meta.get("lifecycle_status", "ACTIVE")).upper()
            if lifecycle != "ACTIVE" or meta.get("deleted") is True or meta.get("expired") is True or meta.get("quarantined") is True:
                add_gate(gates, "inactive_deleted_expired_graph_final_contexts", "inactive_or_deleted_graph_results")
                failures.append("GRAPH_LIFECYCLE_INVALID")
            expected_version = qrel.get("expected_version")
            if expected_version is not None and item["document_version"] != expected_version:
                add_gate(gates, "wrong_version_results")
                failures.append("GRAPH_VERSION_INVALID")
            if meta.get("graph_binding_valid") is False or item["parent_logical"] is None or item["matched_logical"] is None:
                add_gate(gates, "binding_invalid_graph_final_contexts")
                failures.append("GRAPH_BINDING_INVALID")
            expected_children = set(qrel.get("expected_graph_child_any") or [])
            endpoint_mismatch = (
                (expected_children and item["matched_logical"] not in expected_children)
                or (present(meta.get("graph_related_chunk_id")) and meta.get("graph_related_chunk_id") != item["matched_runtime"])
                or (present(meta.get("graph_related_parent_chunk_id")) and meta.get("graph_related_parent_chunk_id") != item["parent_runtime"])
            )
            if endpoint_mismatch:
                add_gate(gates, "binding_invalid_graph_final_contexts")
                failures.append("GRAPH_ENDPOINT_IDENTITY_MISMATCH")
            hop = as_int(meta.get("graph_hop_distance"))
            if hop is None or hop > query.get("graph_max_hops", 1):
                add_gate(
                    gates,
                    "hop_limit_violation_final_contexts",
                    "graph_hop_limit_violations",
                    "second_hop_final_contexts",
                )
                failures.append("GRAPH_HOP_LIMIT_VIOLATION")
            complete = all(present(meta.get(field)) for field in REQUIRED_PROVENANCE)
            if complete:
                complete_graph_contexts += 1
            else:
                add_gate(gates, "graph_provenance_missing")
                failures.append("GRAPH_PROVENANCE_MISSING")
            relation_type = meta.get("graph_relation_type")
            edge_valid = relation_type in (qrel.get("required_graph_relation_any") or [relation_type])
            if edge_valid and complete and hop == qrel.get("required_hop_index", 1):
                edge_valid_contexts += 1
            else:
                failures.append("GRAPH_EDGE_INVALID")
            if not expected_graph or item["parent_logical"] == expected_graph:
                valid_graph_contexts += 1
            key = (
                item["matched_logical"],
                item["parent_logical"],
                meta.get("graph_edge_id"),
                meta.get("graph_relation_id"),
            )
            if key in graph_keys:
                add_gate(
                    gates,
                    "cycle_credit_inflation_events",
                    "graph_cycle_credit_inflation",
                    "duplicate_graph_credit",
                )
                failures.append("DUPLICATE_GRAPH_CREDIT")
            graph_keys.add(key)

    if profile_name in {"NEGATIVE_NO_ANSWER", "NEGATIVE_LEGAL_HOLD", "GRAPH_DISABLED"} and graph_rows:
        add_gate(gates, "false_graph_attribution_events")
        failures.append("FALSE_GRAPH_ATTRIBUTION")
    if profile_name == "GRAPH_DISABLED" and graph_rows:
        add_gate(gates, "graph_disabled_origin_count")
    if profile_name == "NEGATIVE_NO_ANSWER" and normalized:
        add_gate(gates, "false_positive_contexts")
    forbidden_parent = qrel.get("forbidden_graph_parent")
    if forbidden_parent and any(item["parent_logical"] == forbidden_parent for item in normalized):
        add_gate(gates, "false_graph_attribution_events")
        failures.append("FORBIDDEN_PARENT_RETURNED")
    telemetry = row["telemetry"]
    if profile_name == "GRAPH_DISABLED" and telemetry["graph_executed"]:
        add_gate(gates, "graph_disabled_execution_count")
        failures.append("GRAPH_DISABLED_EXECUTED")
    if row["latency_ms"] > row["deadline_ms"] + row["jitter_allowance_ms"]:
        add_gate(gates, "request_deadline_violations")
        failures.append("REQUEST_DEADLINE_EXCEEDED")
    if (
        telemetry["candidates_before_validation"] > telemetry["candidate_max"]
        or telemetry["candidates_after_validation"] > telemetry["candidate_max"]
        or telemetry["hop_count"] > telemetry["hop_max"]
        or telemetry["hop_count"] > query.get("graph_max_hops", 1)
    ):
        add_gate(gates, "candidate_bound_violations")
        failures.append("BOUNDEDNESS_VIOLATION")
    if telemetry["n_plus_one_sql_hydration"]:
        add_gate(gates, "n_plus_one_sql_hydration_events")
        failures.append("N_PLUS_ONE_SQL_HYDRATION")

    response_status = row["response"].get("status")
    expected_status = qrel.get("expected_status")
    expected_statuses = qrel.get("expected_status_any") or ([expected_status] if expected_status else [])
    if expected_statuses and response_status not in expected_statuses:
        failures.append("RESPONSE_STATUS_INVALID")
    if "expected_final_context_count" in qrel and len(normalized) != qrel["expected_final_context_count"]:
        failures.append("FINAL_CONTEXT_COUNT_INVALID")
    expected_parent = qrel.get("expected_parent")
    if expected_parent and not any(item["parent_logical"] == expected_parent for item in direct_rows):
        failures.append("EXPECTED_PARENT_MISSING")
    required_anchors = (
        qrel.get("required_anchors_in_parent_text") or []
        if not fault_contract or fault_contract["survivor_mode"] == "GRAPH"
        else []
    )
    if required_anchors and not any(all(anchor in item["parent_text"] for anchor in required_anchors) for item in normalized):
        failures.append("REQUIRED_PARENT_ANCHOR_MISSING")
    if expected_direct and not any(item["parent_logical"] == expected_direct for item in direct_rows):
        failures.append("DIRECT_PARENT_MISSING")
    graph_rank = next(
        (index for index, item in enumerate(normalized, 1) if item in graph_rows and item["parent_logical"] == expected_graph),
        None,
    )
    if expected_graph and graph_rank is None and graph_survivor_required:
        failures.append("GRAPH_PARENT_MISSING")
    if fault_setup:
        direct_survivor_present = bool(
            expected_direct
            and any(item["parent_logical"] == expected_direct for item in direct_rows)
        )
        graph_survivor_present = graph_rank is not None
        survivor_present = direct_survivor_present and (
            fault_contract["survivor_mode"] != "GRAPH" or graph_survivor_present
        )
        if qrel.get("hard_gate", {}).get("valid_survivor_lost") == 0 and (
            not survivor_present
        ):
            add_gate(gates, "valid_survivor_lost")
            failures.append("VALID_SURVIVOR_LOST")

    degradation = row.get("degradation")
    fault_class = "NONE"
    if fault_setup:
        fault_plan = bank_data["fault_by_setup"].get(fault_setup)
        if not fault_plan:
            fail("FAULT_PLAN_UNKNOWN", fault_setup)
        fault_class = fault_plan["fault_class"]
        required_degradation = (
            "graph_failure_injected",
            "graph_failure_detected",
            "graph_failure_classification",
            "semantic_no_answer",
            "partial_graph_evidence",
            "reported_full_coverage",
            "rejection_observation",
        )
        if not isinstance(degradation, dict) or any(field not in degradation for field in required_degradation):
            fail("DEGRADATION_EVIDENCE_INCOMPLETE", f"{row['_source']}: {required_degradation}")
        if degradation["graph_failure_injected"] is not True:
            failures.append("FAULT_NOT_INJECTED")
        reasons = degradation.get("rejection_reasons") or []
        expected_reason = fault_contract["expected_rejection_reason"]
        allowed_reasons = fault_plan.get("expected_rejection_reason_any") or []
        if expected_reason not in allowed_reasons:
            fail(
                "FAULT_CONTRACT_REASON_NOT_APPROVED",
                f"{fault_setup}: {expected_reason}",
            )
        observation = degradation["rejection_observation"]
        if (
            reasons != [expected_reason]
            or not isinstance(observation, dict)
            or observation.get("status") != "PASS"
            or observation.get("observed") is not True
            or observation.get("reason") != expected_reason
        ):
            failures.append("FAULT_REJECTION_NOT_EVIDENCED")

    graded = qrel.get("graded_relevance") or {}
    grades = [int(graded.get(item["parent_logical"], 0)) for item in normalized]
    ideal_grades = [int(value) for value in graded.values() if int(value) > 0]
    relation_types = sorted({item["metadata"].get("graph_relation_type") for item in graph_rows if present(item["metadata"].get("graph_relation_type"))})
    stable_set = sorted(
        {
            (
                item["parent_logical"] or "UNKNOWN",
                "GRAPH" if item in graph_rows else "DIRECT",
                str(item["metadata"].get("graph_relation_id", "")),
                str(item["metadata"].get("graph_relation_type", "")),
                str(item["metadata"].get("graph_hop_distance", "")),
            )
            for item in normalized
        }
    )
    return {
        "schema_version": 1,
        "query_id": row["query_id"],
        "entry_point": row["entry_point"],
        "run_kind": row["run_kind"],
        "run_index": row.get("run_index"),
        "pair_id": row.get("pair_id"),
        "profile": profile_name,
        "language": query["language"],
        "query_family": query["query_family"],
        "graph_enabled": bool(query["enable_graph_expansion"]),
        "relation_types": relation_types or ["NONE"],
        "fault_class": fault_class,
        "status": "PASS" if not failures else "FAIL",
        "failure_codes": sorted(set(failures)),
        "latency_ms": row["latency_ms"],
        "deadline_ms": row["deadline_ms"],
        "jitter_allowance_ms": row["jitter_allowance_ms"],
        "telemetry": telemetry,
        "degradation": degradation,
        "context_count": len(normalized),
        "graph_context_count": len(graph_rows),
        "valid_graph_context_count": valid_graph_contexts,
        "complete_graph_context_count": complete_graph_contexts,
        "edge_valid_graph_context_count": edge_valid_contexts,
        "direct_expected_present": bool(expected_direct and any(item["parent_logical"] == expected_direct for item in direct_rows)),
        "graph_rank": graph_rank,
        "grades": grades,
        "ideal_grades": ideal_grades,
        "mrr": 1.0 / graph_rank if graph_rank else 0.0,
        "ndcg_at_3": ndcg(grades, ideal_grades, 3),
        "ndcg_at_5": ndcg(grades, ideal_grades, 5),
        "stable_parent_provenance_set": stable_set,
        "normalized_contexts": [
            {
                "matched_logical": item["matched_logical"],
                "parent_logical": item["parent_logical"],
                "graph_origin": item in graph_rows,
                "relation_type": item["metadata"].get("graph_relation_type"),
                "hop": item["metadata"].get("graph_hop_distance"),
            }
            for item in normalized
        ],
    }


def wilson(successes: int, total: int, z: float = 1.959963984540054) -> dict[str, float | int | None]:
    if total == 0:
        return {"successes": successes, "total": total, "point_estimate": None, "lower": None, "upper": None}
    p = successes / total
    denominator = 1 + z * z / total
    center = (p + z * z / (2 * total)) / denominator
    margin = z * math.sqrt(p * (1 - p) / total + z * z / (4 * total * total)) / denominator
    return {
        "successes": successes,
        "total": total,
        "point_estimate": p,
        "lower": max(0.0, center - margin),
        "upper": min(1.0, center + margin),
    }


def proportion_metric(name: str, numerator: int, denominator: int, threshold: float) -> dict[str, Any]:
    point = numerator / denominator if denominator else None
    return {
        "name": name,
        "numerator": numerator,
        "denominator": denominator,
        "point_estimate": point,
        "confidence_interval": {"method": "Wilson two-sided 95%", **wilson(numerator, denominator)},
        "threshold": threshold,
        "outcome": "PASS" if denominator and point is not None and point >= threshold else "BLOCKED",
    }


def mean_metric(name: str, values: list[float], threshold: float) -> dict[str, Any]:
    point = statistics.fmean(values) if values else None
    return {
        "name": name,
        "numerator": sum(values),
        "denominator": len(values),
        "point_estimate": point,
        "confidence_interval": None,
        "threshold": threshold,
        "outcome": "PASS" if point is not None and point >= threshold else "BLOCKED",
    }


def descriptive_mean_metric(name: str, values: list[float]) -> dict[str, Any]:
    return {
        "name": name,
        "numerator": sum(values),
        "denominator": len(values),
        "point_estimate": statistics.fmean(values) if values else None,
        "confidence_interval": None,
        "threshold": None,
        "outcome": "REPORTED" if values else "NOT_APPLICABLE",
    }


def zero_error_metric(name: str, errors: int, denominator: int) -> dict[str, Any]:
    point = errors / denominator if denominator else None
    return {
        "name": name,
        "numerator": errors,
        "denominator": denominator,
        "point_estimate": point,
        "confidence_interval": {"method": "Wilson two-sided 95%", **wilson(errors, denominator)},
        "threshold": 0.0,
        "comparison": "<=",
        "outcome": "PASS" if denominator and errors == 0 else "BLOCKED",
    }


def quality_metrics(results: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    official = [row for row in results if row["run_kind"] in FULL_RUN_KINDS]
    primary = [row for row in official if row["profile"] == "POSITIVE_GRAPH"]
    graph_contexts = sum(row["graph_context_count"] for row in primary)
    fault_rows = [row for row in official if row["fault_class"] != "NONE"]
    no_answer = [row for row in official if row["profile"] == "NEGATIVE_NO_ANSWER"]
    metrics = {
        f"GraphParentRecall@{k}": proportion_metric(
            f"GraphParentRecall@{k}", sum(row["graph_rank"] is not None and row["graph_rank"] <= k for row in primary), len(primary), threshold
        )
        for k, threshold in ((1, 0.90), (3, 0.97), (5, 0.99))
    }
    metrics["MRR"] = mean_metric("MRR", [row["mrr"] for row in primary], MEAN_THRESHOLDS["MRR"])
    metrics["nDCG@3"] = descriptive_mean_metric("nDCG@3", [row["ndcg_at_3"] for row in primary])
    metrics["nDCG@5"] = mean_metric("nDCG@5", [row["ndcg_at_5"] for row in primary], MEAN_THRESHOLDS["nDCG@5"])
    metrics["GraphParentAccuracy"] = proportion_metric(
        "GraphParentAccuracy", sum(row["valid_graph_context_count"] for row in primary), graph_contexts, 1.0
    )
    metrics["GraphEdgePrecision"] = proportion_metric(
        "GraphEdgePrecision", sum(row["edge_valid_graph_context_count"] for row in primary), graph_contexts, 1.0
    )
    metrics["GraphProvenanceCompleteness"] = proportion_metric(
        "GraphProvenanceCompleteness", sum(row["complete_graph_context_count"] for row in primary), graph_contexts, 1.0
    )
    metrics["GraphContributionRate"] = proportion_metric(
        "GraphContributionRate", sum(row["graph_rank"] is not None for row in primary), len(primary), 0.95
    )
    metrics["DirectPreservationRate"] = proportion_metric(
        "DirectPreservationRate", sum(row["direct_expected_present"] for row in fault_rows), len(fault_rows), 1.0
    )
    metrics["NoAnswerSpecificity"] = proportion_metric(
        "NoAnswerSpecificity", sum(row["context_count"] == 0 for row in no_answer), len(no_answer), 1.0
    )
    return metrics


def repeatability_metrics(results: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    output: dict[str, dict[str, Any]] = {}
    for kind, name in (("warm", "WarmNormalizedRepeatability"), ("restart", "RestartNormalizedRepeatability")):
        groups: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
        for row in results:
            if row["run_kind"] == kind:
                groups[(row["query_id"], row["entry_point"])].append(row)
        successes = sum(len({canonical_json(row["stable_parent_provenance_set"]) for row in group}) == 1 for group in groups.values())
        output[name] = proportion_metric(name, successes, len(groups), 1.0)
    return output


def parity_metric(results: list[dict[str, Any]], gates: dict[str, int]) -> dict[str, Any]:
    groups: dict[tuple[str, int, str], dict[str, dict[str, Any]]] = defaultdict(dict)
    for row in results:
        if row["run_kind"] in FULL_RUN_KINDS:
            groups[(row["run_kind"], row["run_index"], row["query_id"])][row["entry_point"]] = row
    failures = []
    for key, pair in groups.items():
        left = pair.get("Search")
        right = pair.get("RetrieveContext")
        if not left or not right or left["stable_parent_provenance_set"] != right["stable_parent_provenance_set"]:
            failures.append(key)
    if failures:
        gates["search_retrieve_parity_failures"] += len(failures)
    return {
        "name": "SearchRetrieveParity",
        "numerator": len(groups) - len(failures),
        "denominator": len(groups),
        "point_estimate": (len(groups) - len(failures)) / len(groups) if groups else None,
        "threshold": 1.0,
        "outcome": "PASS" if groups and not failures else "BLOCKED",
        "differences": [f"{kind}/{index}/{query}" for kind, index, query in failures],
    }


def degradation_metrics(results: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    fault = [row for row in results if row["fault_class"] != "NONE"]
    injected = [row for row in fault if row["degradation"] and row["degradation"].get("graph_failure_injected") is True]
    detected = [row for row in injected if row["degradation"].get("graph_failure_detected") is True]
    partial = [row for row in injected if row["degradation"].get("partial_graph_evidence") is True]
    healthy = [row for row in results if row["run_kind"] == "concurrent_healthy"]
    return {
        "GraphFailureDetectionRecall": proportion_metric("GraphFailureDetectionRecall", len(detected), len(injected), 1.0),
        "GraphFailureClassificationAccuracy": proportion_metric(
            "GraphFailureClassificationAccuracy",
            sum(row["degradation"].get("graph_failure_classification") == row["fault_class"] for row in detected),
            len(detected),
            1.0,
        ),
        "FalseSemanticNoAnswerRate": zero_error_metric(
            "FalseSemanticNoAnswerRate",
            sum(row["degradation"].get("semantic_no_answer") is True for row in injected),
            len(injected),
        ),
        "FalseFullCoverageRate": zero_error_metric(
            "FalseFullCoverageRate",
            sum(row["degradation"].get("reported_full_coverage") is True for row in partial),
            len(partial),
        ),
        "HealthyRequestContaminationRate": zero_error_metric(
            "HealthyRequestContaminationRate",
            sum((row["degradation"] or {}).get("healthy_request_affected") is True for row in healthy),
            len(healthy),
        ),
    }


def percentile(values: list[float], p: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    rank = max(1, math.ceil(p * len(ordered)))
    return ordered[rank - 1]


def latency_distribution(results: list[dict[str, Any]]) -> dict[str, Any]:
    groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for row in results:
        groups["overall"].append(row)
        groups[f"entry_point={row['entry_point']}|graph_enabled={str(row['graph_enabled']).lower()}"].append(row)
    payload = {}
    for name, rows in sorted(groups.items()):
        values = [float(row["latency_ms"]) for row in rows]
        telemetry = [row["telemetry"] for row in rows]
        payload[name] = {
            "sample_count": len(values),
            "percentile_method": "nearest-rank",
            "p50_ms": percentile(values, 0.50),
            "p95_ms": percentile(values, 0.95),
            "p99_ms": percentile(values, 0.99),
            "mean_ms": statistics.fmean(values),
            "max_ms": max(values),
            "resource_metrics": {
                key: {"mean": statistics.fmean(float(item[key]) for item in telemetry), "max": max(item[key] for item in telemetry)}
                for key in (
                    "graph_expansion_ms",
                    "canonical_graph_hydration_ms",
                    "candidates_before_validation",
                    "candidates_after_validation",
                    "sql_statement_count",
                    "qdrant_request_count",
                    "graph_relation_query_count",
                )
            },
        }
    return {"schema_version": 1, "groups": payload}


def slice_metrics(results: list[dict[str, Any]]) -> dict[str, Any]:
    dimensions = {
        "language": lambda row: [row["language"]],
        "query_family": lambda row: [row["query_family"]],
        "profile": lambda row: [row["profile"]],
        "entry_point": lambda row: [row["entry_point"]],
        "graph_enabled": lambda row: [str(row["graph_enabled"]).lower()],
        "relation_type": lambda row: row["relation_types"],
        "fault_class": lambda row: [row["fault_class"]],
        "run_kind": lambda row: [row["run_kind"]],
    }
    slices = []
    blocked = []
    for dimension, values_for in dimensions.items():
        grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for row in results:
            for value in values_for(row):
                grouped[str(value)].append(row)
        for value, subset in sorted(grouped.items()):
            metrics = quality_metrics(subset)
            applicable = {name: metric for name, metric in metrics.items() if metric["denominator"] > 0}
            outcome = "PASS" if all(metric["outcome"] != "BLOCKED" for metric in applicable.values()) else "BLOCKED"
            item = {
                "dimension": dimension,
                "value": value,
                "sample_count": len(subset),
                "metrics": metrics,
                "outcome": outcome,
                "applicability": "APPLICABLE" if applicable else "NOT_APPLICABLE",
            }
            slices.append(item)
            if outcome == "BLOCKED":
                blocked.append(f"{dimension}={value}")
    return {"schema_version": 1, "slices": slices, "blocked_slices": blocked}


def confidence_intervals(
    metric_groups: dict[str, dict[str, Any]], gates: dict[str, int], safety_denominator: int
) -> dict[str, Any]:
    intervals = []
    for group_name, metrics in metric_groups.items():
        for name, metric in metrics.items():
            if metric.get("confidence_interval"):
                intervals.append({"group": group_name, "metric": name, **metric["confidence_interval"]})
    safety = []
    for name, failures in sorted(gates.items()):
        upper = wilson(failures, safety_denominator, z=1.6448536269514722)["upper"]
        safety.append(
            {
                "gate": name,
                "failures": failures,
                "sample_denominator": safety_denominator,
                "sample_definition": "all evaluated raw observations",
                "method": "Wilson one-sided 95% upper bound",
                "failure_probability_upper": upper,
            }
        )
    return {"schema_version": 1, "proportions": intervals, "safety_failure_upper_bounds": safety}


def report_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# FIX486G Statistical Evaluation",
        "",
        f"**Verdict:** `{report['verdict']}`",
        "",
        f"Bank: `{report['bank_id']}@{report['bank_version']}`  ",
        f"Raw observations: {report['sample_plan']['raw_observation_count']}  ",
        f"Warm passes: {report['sample_plan']['full_pass_counts']['warm']}  ",
        f"Restart passes: {report['sample_plan']['full_pass_counts']['restart']}  ",
        f"Concurrent pairs: {report['sample_plan']['concurrent_pair_count']}",
        "",
        "## Metrics",
        "",
        "| Metric | Numerator | Denominator | Estimate | Threshold | Outcome |",
        "|---|---:|---:|---:|---:|---|",
    ]
    for metric in report["metrics"].values():
        estimate = "n/a" if metric["point_estimate"] is None else f"{metric['point_estimate']:.6f}"
        threshold = "reported" if metric["threshold"] is None else f"{metric['threshold']:.2f}"
        lines.append(
            f"| {metric['name']} | {metric['numerator']:.6g} | {metric['denominator']} | {estimate} | {threshold} | {metric['outcome']} |"
        )
    lines.extend(["", "## Gates", ""])
    lines.append(f"Hard-gate violations: {sum(report['hard_gates'].values())}")
    if report["failure_codes"]:
        lines.extend(["", "## Blocking Reasons", ""] + [f"- `{code}`" for code in report["failure_codes"]])
    return "\n".join(lines) + "\n"


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_artifacts(output_dir: Path, payloads: dict[str, Any]) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for name in ARTIFACTS:
        payload = payloads[name]
        if name.endswith(".jsonl"):
            path = output_dir / name
            path.write_text("".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in payload), encoding="utf-8")
        elif name.endswith(".md"):
            (output_dir / name).write_text(str(payload), encoding="utf-8")
        else:
            write_json(output_dir / name, payload)


def blocked_artifacts(output_dir: Path, reason: str) -> None:
    minimal_report = {
        "schema_version": 1,
        "metric_version": METRIC_VERSION,
        "verdict": BLOCKED,
        "failure_codes": [reason],
        "status": "BLOCKED",
        "sample_plan": {"raw_observation_count": 0, "full_pass_counts": {"warm": 0, "restart": 0}, "concurrent_pair_count": 0},
        "metrics": {},
        "hard_gates": {},
    }
    payloads = {
        "statistical-report.json": minimal_report,
        "statistical-report.md": f"# FIX486G Statistical Evaluation\n\n**Verdict:** `{BLOCKED}`\n\n- `{reason}`\n",
        "per-query-results.jsonl": [],
        "per-slice-metrics.json": {"schema_version": 1, "slices": [], "blocked_slices": [reason]},
        "latency-distribution.json": {"schema_version": 1, "groups": {}},
        "safety-hard-gates.json": {"schema_version": 1, "status": "BLOCKED", "hard_gates": {}, "failure_codes": [reason]},
        "confidence-intervals.json": {"schema_version": 1, "proportions": [], "safety_failure_upper_bounds": []},
    }
    write_artifacts(output_dir, payloads)


def evaluate(
    bank: Path, raw_paths: list[Path], identity_path: Path | None, output_dir: Path
) -> dict[str, Any]:
    bank_data = verify_and_load_bank(bank)
    rows = read_jsonl(raw_paths)
    validation = validate_rows(rows, bank_data)
    identities = identity_lookup(identity_path)
    gates: dict[str, int] = defaultdict(int)
    for gate in GLOBAL_HARD_GATES:
        gates[gate] = 0
    for profile in bank_data["profiles"].values():
        for gate in (profile.get("hard_gate") or {}):
            gates[gate] = 0
    results = [evaluate_observation(row, bank_data, identities, gates) for row in rows]
    quality = quality_metrics(results)
    repeatability = repeatability_metrics(results)
    parity = parity_metric(results, gates)
    degradation = degradation_metrics(results)
    all_metrics = {**quality, **repeatability, parity["name"]: parity, **degradation}
    slices = slice_metrics(results)
    latency = latency_distribution(results)
    row_failures = sum(row["status"] != "PASS" for row in results)
    failure_codes = []
    if row_failures:
        failure_codes.append("PER_OBSERVATION_GATES_FAILED")
    if any(value != 0 for value in gates.values()):
        failure_codes.append("SAFETY_HARD_GATE_FAILED")
    if any(metric["outcome"] == "BLOCKED" for metric in all_metrics.values()):
        failure_codes.append("QUALITY_THRESHOLD_FAILED")
    if slices["blocked_slices"]:
        failure_codes.append("CRITICAL_SLICE_FAILED")
    verdict = PASS if not failure_codes else BLOCKED
    input_identities = [{"path": str(path), "sha256": sha256(path)} for path in raw_paths]
    report = {
        "schema_version": 1,
        "metric_version": METRIC_VERSION,
        "status": "PASS" if verdict == PASS else "BLOCKED",
        "verdict": verdict,
        "source_identities": {
            "evaluator": str(Path(__file__).resolve()),
            "evaluator_sha256": sha256(Path(__file__)),
            "raw_inputs": input_identities,
            "identity_map": ({"path": str(identity_path), "sha256": sha256(identity_path)} if identity_path else None),
        },
        "bank_id": bank_data["manifest"]["bank_id"],
        "bank_version": bank_data["manifest"]["version"],
        "bank_status": bank_data["manifest"]["status"],
        "bank_aggregate_sha256": bank_data["aggregate_sha256"],
        "run_identities": sorted(
            {
                (row["run_kind"], row.get("run_index"), row.get("pair_id"))
                for row in rows
            },
            key=lambda value: (value[0], value[1] or 0, value[2] or ""),
        ),
        "metric_definitions": {
            "version": METRIC_VERSION,
            "proportion_interval": "Wilson two-sided 95%",
            "zero_failure_bound": "Wilson one-sided 95% upper bound",
            "latency_percentile": "nearest-rank",
            "thresholds": {**PROPORTION_THRESHOLDS, **MEAN_THRESHOLDS},
        },
        "sample_plan": validation,
        "metrics": all_metrics,
        "per_slice_outcomes": {"blocked": slices["blocked_slices"], "slice_count": len(slices["slices"])},
        "hard_gates": dict(sorted(gates.items())),
        "excluded_or_blocked_rows": [
            {
                "query_id": row["query_id"],
                "entry_point": row["entry_point"],
                "run_kind": row["run_kind"],
                "run_index": row["run_index"],
                "pair_id": row["pair_id"],
                "reasons": row["failure_codes"],
            }
            for row in results
            if row["status"] != "PASS"
        ],
        "failure_codes": failure_codes,
    }
    safety = {
        "schema_version": 1,
        "status": "PASS" if not any(gates.values()) else "BLOCKED",
        "hard_gates": dict(sorted(gates.items())),
        "all_zero_required": True,
    }
    cis = confidence_intervals(
        {"quality": quality, "repeatability": repeatability, "degradation": degradation},
        gates,
        len(results),
    )
    payloads = {
        "statistical-report.json": report,
        "statistical-report.md": report_markdown(report),
        "per-query-results.jsonl": results,
        "per-slice-metrics.json": slices,
        "latency-distribution.json": latency,
        "safety-hard-gates.json": safety,
        "confidence-intervals.json": cis,
    }
    write_artifacts(output_dir, payloads)
    return report


def emit(payload: Any, output: Path | None) -> None:
    if output:
        output.parent.mkdir(parents=True, exist_ok=True)
        write_json(output, payload)
    print(json.dumps(payload, ensure_ascii=False, sort_keys=True))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    plan_parser = sub.add_parser("plan", help="emit the offline capture/evaluation plan")
    plan_parser.add_argument("--bank", type=Path, required=True)
    plan_parser.add_argument("--warm-repeats", type=int, default=MIN_WARM_REPEATS)
    plan_parser.add_argument("--restart-repeats", type=int, default=MIN_RESTART_REPEATS)
    plan_parser.add_argument("--concurrent-pairs", type=int, default=MIN_CONCURRENT_PAIRS)
    plan_parser.add_argument("--output", type=Path)
    for name in ("validate", "dry-validate"):
        validate_parser = sub.add_parser(name, help="validate ready raw JSONL without calculating a verdict")
        validate_parser.add_argument("--bank", type=Path, required=True)
        validate_parser.add_argument("--raw-input", "--input", type=Path, action="append", dest="raw_inputs", required=True)
        validate_parser.add_argument("--identity-map", type=Path)
        validate_parser.add_argument("--output", type=Path)
    evaluate_parser = sub.add_parser("evaluate", help="evaluate raw JSONL and write all mandatory artifacts")
    evaluate_parser.add_argument("--bank", type=Path, required=True)
    evaluate_parser.add_argument("--raw-input", "--input", type=Path, action="append", dest="raw_inputs", required=True)
    evaluate_parser.add_argument("--identity-map", type=Path)
    evaluate_parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "plan":
            payload = build_plan(
                verify_and_load_bank(args.bank), args.warm_repeats, args.restart_repeats, args.concurrent_pairs
            )
            emit(payload, args.output)
            return 0
        if args.command in {"validate", "dry-validate"}:
            bank_data = verify_and_load_bank(args.bank)
            rows = read_jsonl(args.raw_inputs)
            payload = validate_rows(rows, bank_data)
            identity_lookup(args.identity_map)
            payload["bank_aggregate_sha256"] = bank_data["aggregate_sha256"]
            payload["network_calls"] = False
            emit(payload, args.output)
            return 0
        report = evaluate(args.bank, args.raw_inputs, args.identity_map, args.output_dir)
        print(json.dumps({"verdict": report["verdict"], "failure_codes": report["failure_codes"]}, sort_keys=True))
        return 0 if report["verdict"] == PASS else 1
    except (EvaluationError, OSError, KeyError, TypeError) as error:
        message = str(error)
        if args.command == "evaluate":
            try:
                blocked_artifacts(args.output_dir, message)
            except OSError as artifact_error:
                print(f"FIX486G_STATISTICAL_ERROR={artifact_error}", file=sys.stderr)
        print(f"FIX486G_STATISTICAL_ERROR={message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
