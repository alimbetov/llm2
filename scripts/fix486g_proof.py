#!/usr/bin/env python3
"""Fail-closed normalization and aggregation for the fix486g proof.

The runner deliberately keeps logical expectations outside the runtime.  This
module only validates identifiers and evidence captured by public APIs and
read-only audits; it never writes fixture state.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


REQUIRED_QUERIES = {
    "q-graph-repair": "FIX486-08",
}
REQUIRED_ENTRY_POINTS = {"Search", "RetrieveContext"}
REQUIRED_IDENTITY_FIELDS = {
    "logical_zone_id", "runtime_access_zone_id", "logical_document_id",
    "runtime_document_id", "logical_version", "runtime_chunk_id",
    "chunk_role", "granularity", "source_block_id", "content_sha256",
}
GRAPH_PROVENANCE_FIELDS = (
    "graph_seed_access_zone_id", "graph_seed_document_id",
    "graph_seed_document_version", "graph_seed_chunk_id",
    "graph_seed_parent_chunk_id", "graph_relation_id", "graph_edge_id",
    "graph_relation_type", "graph_relation_score",
    "graph_related_access_zone_id", "graph_related_document_id",
    "graph_related_document_version", "graph_related_chunk_id",
    "graph_related_parent_chunk_id", "graph_hop_distance",
)
MANDATORY_PASS_EVIDENCE = frozenset({
    "aggregate.json", "stage-results.json", "query-results.jsonl",
    "identity-map/logical-to-runtime.json", "graph-disabled/results.jsonl",
    "graph-audit/graph-identity-chain.json",
    "graph-audit/graph-provenance-trace.json",
    "canonical-audit/integrity-summary.json",
    "qdrant-audit/payload-consistency.json",
    "comparisons/entry-point-parity.json", "comparisons/warm-repeat.json",
    "restart/pre-post-restart.json", "cleanup/summary.json",
    "statistical/statistical-report.json",
    "statistical/statistical-report.md",
    "statistical/per-query-results.jsonl",
    "statistical/per-slice-metrics.json",
    "statistical/latency-distribution.json",
    "statistical/safety-hard-gates.json",
    "statistical/confidence-intervals.json",
    "defect-register.json",
})
MANIFEST_EXCLUDED_NAMES = {"manifest.json", "manifest-verification.json"}


def fail(code: str, detail: str) -> None:
    raise ValueError(f"{code}: {detail}")


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl(path: Path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


SUPPLEMENTAL_PAYLOADS = [
    "queries/graph-parent-queries-v1.jsonl",
    "qrels/qrel-profiles-v1.json",
    "qrels/query-qrel-assignments-v1.jsonl",
    "faults/graph-fault-plans-v1.json",
]


def supplemental_aggregate(hashes: dict[str, str]) -> str:
    canonical = "".join(f"{path}\0{hashes[path]}\n" for path in sorted(hashes))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def verify_supplemental(bank: Path) -> dict:
    manifest = read_json(bank / "bank-manifest.json")
    queries = read_jsonl(bank / SUPPLEMENTAL_PAYLOADS[0])
    profiles_doc = read_json(bank / SUPPLEMENTAL_PAYLOADS[1])
    assignments = read_jsonl(bank / SUPPLEMENTAL_PAYLOADS[2])
    faults = read_json(bank / SUPPLEMENTAL_PAYLOADS[3])
    if manifest.get("version") != "1.0.0" or manifest.get("status") != "FROZEN":
        fail("SUPPLEMENTAL_BANK_NOT_FROZEN", "manifest identity")
    if profiles_doc.get("version") != "1.0.0" or profiles_doc.get("status") != "FROZEN":
        fail("SUPPLEMENTAL_BANK_NOT_FROZEN", "qrel profiles")
    if faults.get("version") != "1.0.0" or faults.get("status") != "FROZEN":
        fail("SUPPLEMENTAL_BANK_NOT_FROZEN", "fault plans")
    ids = [row.get("query_id") for row in queries]
    if len(ids) != 71 or len(set(ids)) != 71 or any(not value for value in ids):
        fail("SUPPLEMENTAL_QUERY_SET_INVALID", f"rows={len(ids)} unique={len(set(ids))}")
    families: dict[str, int] = {}
    languages: dict[str, int] = {}
    for row in queries:
        families[row["query_family"]] = families.get(row["query_family"], 0) + 1
        languages[row["language"]] = languages.get(row["language"], 0) + 1
    positive = sum(count for family, count in families.items()
                   if family not in {"negative", "graph-disabled"} and not family.startswith("adversarial-"))
    adversarial = sum(count for family, count in families.items() if family.startswith("adversarial-"))
    if (positive, families.get("negative"), families.get("graph-disabled"), adversarial) != (30, 15, 6, 20):
        fail("SUPPLEMENTAL_DISTRIBUTION_INVALID", str(families))
    by_query: dict[str, list[str]] = {}
    known_profiles = set(profiles_doc.get("profiles", {}))
    for row in assignments:
        by_query.setdefault(row.get("query_id", ""), []).append(row.get("qrel_profile", ""))
    if set(by_query) != set(ids) or any(len(values) != 1 for values in by_query.values()):
        fail("SUPPLEMENTAL_QREL_ASSIGNMENT_INVALID", "every query needs exactly one assignment")
    if any(values[0] not in known_profiles for values in by_query.values()):
        fail("SUPPLEMENTAL_QREL_PROFILE_UNKNOWN", "assignment references unknown profile")
    expected_fault_languages = {
        suffix: language
        for suffix, language in {
            "01": "RU", "02": "KZ", "03": "EN", "04": "RU",
        }.items()
    }
    for row in queries:
        if row["query_family"] in {
            "adversarial-cross-zone", "adversarial-lifecycle",
            "adversarial-hop-limit", "adversarial-cycle",
        }:
            expected = expected_fault_languages[row["query_id"].rsplit("-", 1)[1]]
            if row["language"] != expected:
                fail("SUPPLEMENTAL_LANGUAGE_METADATA_INVALID", row["query_id"])
    hashes = {path: sha256(bank / path) for path in SUPPLEMENTAL_PAYLOADS}
    expected_hashes = manifest.get("hashes", {}).get("files", {})
    if hashes != expected_hashes:
        fail("SUPPLEMENTAL_HASH_MISMATCH", "payload hashes")
    aggregate = supplemental_aggregate(hashes)
    if aggregate != manifest.get("hashes", {}).get("aggregate_sha256"):
        fail("SUPPLEMENTAL_HASH_MISMATCH", "aggregate")
    return {
        "status": "PASS", "bank_id": manifest.get("bank_id"),
        "version": "1.0.0", "bank_status": "FROZEN",
        "query_count": len(queries), "qrel_assignment_count": len(assignments),
        "profiles": sorted(known_profiles), "families": families,
        "languages": languages, "aggregate_sha256": aggregate,
    }


def protobuf_positive_int(value, field: str) -> int:
    """Accept protobuf JSON int64 strings while rejecting lossy/coerced values."""
    if isinstance(value, bool):
        fail("CANONICAL_BINDING_INVALID", f"{field} is boolean")
    if isinstance(value, int):
        parsed = value
    elif isinstance(value, str) and value.isascii() and value.isdigit():
        parsed = int(value)
    else:
        fail("CANONICAL_BINDING_INVALID", f"{field} is not a positive integer")
    if parsed <= 0:
        fail("CANONICAL_BINDING_INVALID", f"{field} is not positive")
    return parsed


def select_frozen_queries(bank: Path) -> list[dict]:
    queries = {q["query_id"]: q for q in read_jsonl(bank / "queries/hierarchical-queries-v1.jsonl")}
    qrels_by_query: dict[str, list[dict]] = {}
    for qrel in read_jsonl(bank / "qrels/hierarchical-qrels-v1.jsonl"):
        qrels_by_query.setdefault(qrel["query_id"], []).append(qrel)
    selected = []
    for query_id, case_id in REQUIRED_QUERIES.items():
        query = queries.get(query_id)
        qrels = qrels_by_query.get(query_id, [])
        if query is None or query.get("case_id") != case_id:
            fail("MANDATORY_QUERY_SKIPPED", query_id)
        if len(qrels) != 1 or qrels[0].get("case_id") != case_id:
            fail("MANDATORY_QUERY_QREL_INVALID", query_id)
        selected.append({"query": query, "qrel": qrels[0]})
    return selected


def validate_identity_map(rows: list[dict]) -> None:
    needed = {
        "parent-a1", "parent-a3", "child-a1-180", "child-a1-260",
        "child-a3-180", "child-a3-260",
    }
    seen: dict[str, int] = {}
    for row in rows:
        missing = REQUIRED_IDENTITY_FIELDS - row.keys()
        if missing:
            fail("IDENTITY_MAP_INCOMPLETE", f"missing {sorted(missing)}")
        if any(not row[field] for field in REQUIRED_IDENTITY_FIELDS - {"logical_version"}):
            fail("IDENTITY_MAP_INCOMPLETE", "empty required value")
        if row["logical_zone_id"] != "zone-a" or row["logical_document_id"] != "doc-hierarchy":
            continue
        logical = row.get("logical_chunk_id")
        if logical in needed:
            seen[logical] = seen.get(logical, 0) + 1
            if logical in {"parent-a1", "parent-a3"}:
                if row["chunk_role"] != "PARENT" or row["granularity"] != "PARENT":
                    fail("IDENTITY_MAP_INCOMPLETE", "parent-a1 is not PARENT")
            elif row["chunk_role"] != "CHILD" or row["granularity"] not in {"SUB_180", "SUB_260"}:
                fail("IDENTITY_MAP_INCOMPLETE", f"{logical} is not searchable child")
    missing = needed - seen.keys()
    if missing:
        fail("IDENTITY_MAP_INCOMPLETE", f"missing logical rows {sorted(missing)}")
    duplicate = sorted(logical for logical, count in seen.items() if count != 1)
    if duplicate:
        fail("AMBIGUOUS_LOGICAL_CHILD_ID", f"non-unique logical rows {duplicate}")


def frozen_child_lookup(bank: Path) -> dict[tuple[str, str, int, str, str], str]:
    """Build child identities from immutable corpus hierarchy, never qrels/results."""
    lookup: dict[tuple[str, str, int, str, str], str] = {}
    corpus = read_json(bank / "corpus/hierarchical-fixture-v1.json")
    for zone in corpus["zones"]:
        for document in zone["documents"]:
            for version in document["versions"]:
                for block in version["blocks"]:
                    for child in block.get("expected_hierarchy", {}).get("children", []):
                        key = (zone["logical_zone_id"], document["logical_document_id"], version["document_version"], block["block_id"], child["granularity"])
                        if key in lookup:
                            fail("AMBIGUOUS_LOGICAL_CHILD_ID", str(key))
                        lookup[key] = child["logical_child_id"]
    return lookup


def apply_frozen_child_identities(rows: list[dict], bank: Path) -> list[dict]:
    lookup = frozen_child_lookup(bank)
    for row in rows:
        if row.get("chunk_role") != "CHILD":
            row["identity_role"] = "PARENT" if row.get("chunk_role") == "PARENT" else "SOURCE_CONTAINER"
            continue
        key = (row["logical_zone_id"], row["logical_document_id"], row["logical_version"], row["source_block_id"], row["granularity"])
        logical = lookup.get(key)
        if logical is None:
            # Production segmentation may create valid source/container descendants
            # that are outside the immutable proof hierarchy. Keep them auditable,
            # but never let them satisfy a frozen logical-child assertion.
            row["logical_chunk_id"] = None
            row["identity_role"] = "AUXILIARY_CHILD"
        else:
            row["logical_chunk_id"] = logical
            row["identity_role"] = "PROOF_CHILD"
    return rows


def response_contexts(response: dict, entry_point: str) -> list[dict]:
    return response.get("results", []) if entry_point == "Search" else response.get("contexts", [])


def trace_candidates(response: dict) -> list[dict]:
    return response.get("diagnostics", {}).get("rankingTrace", {}).get("candidates", [])


def result_metadata(result: dict) -> dict:
    citation = result.get("citation") or {}
    return citation.get("metadata") or result.get("metadata") or {}


def is_primary_graph(metadata: dict) -> bool:
    return metadata.get("retrieval_source") == "GRAPH_EXPANDED"


def has_graph_provenance(metadata: dict) -> bool:
    return is_primary_graph(metadata) or (
        metadata.get("graph_secondary_provenance") == "true"
        and bool(metadata.get("graph_edge_id"))
        and bool(metadata.get("graph_related_chunk_id"))
    )


def stage_is_present(candidate: dict, name: str) -> bool:
    return any(stage.get("stage") == name and stage.get("present") for stage in candidate.get("stages", []))


def normalize(query: dict, qrel: dict, entry_point: str, response: dict,
              identity_by_runtime: dict[str, dict], expect_graph: bool) -> dict:
    contexts = response_contexts(response, entry_point)
    if not contexts:
        fail("MATCHED_CHILD_NOT_PRESERVED", f"{query['query_id']} {entry_point} no contexts")
    failures: list[str] = []
    normalized = []
    forbidden = []
    for result in contexts:
        matched = result.get("matchedChunkId", "")
        parent = result.get("parentChunkId", "")
        matched_identity = identity_by_runtime.get(matched, {})
        parent_identity = identity_by_runtime.get(parent, {})
        metadata = result_metadata(result)
        graph_origin = is_primary_graph(metadata)
        graph_provenance = has_graph_provenance(metadata)
        document_version = protobuf_positive_int(result.get("documentVersion"), "documentVersion")
        if result.get("accessZoneId") != matched_identity.get("runtime_access_zone_id"):
            failures.append("CANONICAL_BINDING_INVALID")
        if document_version != 1 or not matched or not parent:
            failures.append("CANONICAL_BINDING_INVALID")
        combined = result.get("matchedText", "") + "\n" + result.get("parentText", "")
        forbidden.extend(a for a in [
            "ZONE_B_SECRET_PARENT_A1", "ZONE_B_PRIVATE_SOURCE",
            "ASTRA_INACTIVE_VERSION_TRAP", "ASTRA_DELETED_PARENT_TRAP",
            "ASTRA_EXPIRED_PARENT_TRAP",
        ] if a in combined)
        normalized.append({
            "matched": matched,
            "parent": parent,
            "matched_logical": matched_identity.get("logical_chunk_id"),
            "parent_logical": parent_identity.get("logical_chunk_id"),
            "graph_origin": graph_origin,
            "graph_provenance": graph_provenance,
            "metadata": metadata,
            "matched_text": result.get("matchedText", ""),
            "parent_text": result.get("parentText", ""),
            "document_version": document_version,
        })
    if forbidden:
        failures.append("FORBIDDEN_GRAPH_CONTEXT")
    direct = [row for row in normalized if not row["graph_origin"] and row["parent_logical"] == qrel.get("expected_direct_parent")]
    required_relations = set(qrel.get("required_graph_relation_any", []))
    graph = [
        row for row in normalized
        if row["graph_provenance"]
        and row["metadata"].get("graph_relation_type") in required_relations
    ]
    if not direct:
        failures.append("DIRECT_PARENT_MISSING")
    if not expect_graph:
        if graph:
            failures.append("GRAPH_DISABLED_FALSE_ATTRIBUTION")
    else:
        expected_children = set(qrel.get("expected_graph_child_any", []))
        valid_graph = []
        for row in graph:
            context_valid = True
            metadata = row["metadata"]
            if row["parent_logical"] != qrel.get("expected_graph_parent"):
                failures.append("GRAPH_WRONG_PARENT")
                context_valid = False
            related_chunk = metadata.get("graph_related_chunk_id")
            related_parent = metadata.get("graph_related_parent_chunk_id")
            related_chunk_identity = identity_by_runtime.get(related_chunk, {})
            if expected_children and related_chunk_identity.get("logical_chunk_id") not in expected_children:
                failures.append("GRAPH_UNEXPECTED_CHILD")
                context_valid = False
            if any(not metadata.get(field) for field in GRAPH_PROVENANCE_FIELDS):
                failures.append("GRAPH_PROVENANCE_MISSING")
                context_valid = False
            if metadata.get("graph_seed_parent_chunk_id") == metadata.get("graph_related_parent_chunk_id"):
                failures.append("GRAPH_SEED_PARENT_REUSE")
                context_valid = False
            if related_parent != row["parent"]:
                failures.append("GRAPH_WRONG_PARENT")
                context_valid = False
            if metadata.get("graph_relation_type") not in qrel.get("required_graph_relation_any", []):
                failures.append("GRAPH_EDGE_MISSING")
                context_valid = False
            if metadata.get("graph_hop_distance") != "1":
                failures.append("GRAPH_HOP_LIMIT_REJECTED")
                context_valid = False
            if "ASTRA_RECONCILIATION_A3" not in row["parent_text"]:
                failures.append("GRAPH_PARENT_CONTENT_INVALID")
                context_valid = False
            if context_valid:
                valid_graph.append(row)
        if not valid_graph:
            failures.append("GRAPH_PARENT_MISSING")
    trace = trace_candidates(response)
    protected_provenance = [{
        "matched_chunk_id": row["metadata"].get("graph_related_chunk_id"),
        "parent_chunk_id": row["parent"],
        **{field: row["metadata"].get(field) for field in GRAPH_PROVENANCE_FIELDS},
    } for row in graph]
    protected_provenance.sort(
        key=lambda row: json.dumps(row, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    )
    return {
        "schema_version": 1,
        "phase": "fix486g",
        "query_id": query["query_id"],
        "case_id": query["case_id"],
        "entry_point": entry_point,
        "status": "PASS" if not failures else "FAIL",
        "reason": None if not failures else failures[0],
        "expect_graph": expect_graph,
        "logical_identity": {
            "zone": "zone-a", "document": "doc-hierarchy", "version": 1,
            "direct_parents": sorted({row["parent_logical"] for row in direct}),
            "graph_children": sorted({
                identity_by_runtime.get(row["metadata"].get("graph_related_chunk_id"), {}).get("logical_chunk_id")
                for row in graph
            }),
            "graph_parents": sorted({row["parent_logical"] for row in graph}),
        },
        "runtime_identity": [{
            "matched_chunk_id": row["matched"], "parent_chunk_id": row["parent"],
            "graph_origin": row["graph_origin"],
        } for row in normalized],
        "protected_provenance": protected_provenance,
        "assertions": {
            "direct_parent_present": bool(direct),
            "graph_context_count": len(graph),
            "graph_parent_present": any(row["parent_logical"] == qrel.get("expected_graph_parent") for row in graph),
            "forbidden_anchors_found": sorted(set(forbidden)),
            "trace_candidate_count": len(trace),
            "final_unique_parent_count": len({row["parent"] for row in normalized}),
        },
        "failure_codes": sorted(set(failures)),
    }


def validate_control(entry_point: str, response: dict, identity_by_runtime: dict[str, dict],
                     graph_expectation: str, forbidden_chunk_id: str | None,
                     forbidden_scope: str = "any") -> dict:
    contexts = response_contexts(response, entry_point)
    normalized = []
    failures = []
    for context in contexts:
        matched = context.get("matchedChunkId", "")
        parent = context.get("parentChunkId", "")
        matched_identity = identity_by_runtime.get(matched, {})
        parent_identity = identity_by_runtime.get(parent, {})
        metadata = result_metadata(context)
        graph_origin = is_primary_graph(metadata)
        graph_provenance = has_graph_provenance(metadata)
        combined = context.get("matchedText", "") + "\n" + context.get("parentText", "")
        if matched_identity.get("logical_zone_id") != "zone-a":
            failures.append("GRAPH_CROSS_ZONE_RESULT")
        if any(anchor in combined for anchor in [
            "ZONE_B_SECRET_PARENT_A1", "ZONE_B_PRIVATE_SOURCE",
            "ASTRA_INACTIVE_VERSION_TRAP", "ASTRA_DELETED_PARENT_TRAP",
            "ASTRA_EXPIRED_PARENT_TRAP",
        ]):
            failures.append("FORBIDDEN_GRAPH_CONTEXT")
        forbidden_identity_present = forbidden_chunk_id and forbidden_chunk_id in {
            matched,
            metadata.get("graph_related_chunk_id"),
        }
        forbidden_provenance_present = graph_origin or graph_provenance
        if forbidden_identity_present and (
            forbidden_scope == "any" or forbidden_provenance_present
        ):
            failures.append("FAULT_TARGET_RETURNED")
        normalized.append({
            "matched_chunk_id": matched,
            "parent_chunk_id": parent,
            "matched_logical": matched_identity.get("logical_chunk_id"),
            "parent_logical": parent_identity.get("logical_chunk_id"),
            "graph_origin": graph_origin,
            "graph_provenance": graph_provenance,
            "graph_relation_type": metadata.get("graph_relation_type"),
        })
    direct = [row for row in normalized if not row["graph_origin"] and row["parent_logical"] == "parent-a1"]
    graph = [
        row for row in normalized
        if row["graph_provenance"]
        and row["graph_relation_type"] in {"REPAIRED_BY", "RELATED_TO"}
    ]
    if not direct:
        failures.append("VALID_DIRECT_SURVIVOR_LOST")
    if graph_expectation == "present" and not any(row["parent_logical"] == "parent-a3" for row in graph):
        failures.append("VALID_GRAPH_SURVIVOR_LOST")
    if graph_expectation == "absent" and graph:
        failures.append("INVALID_GRAPH_CONTEXT_RETURNED")
    if len({(row["matched_chunk_id"], row["parent_chunk_id"]) for row in graph}) != len(graph):
        failures.append("DUPLICATE_GRAPH_CREDIT")
    return {
        "status": "PASS" if not failures else "FAIL",
        "entry_point": entry_point,
        "graph_expectation": graph_expectation,
        "direct_survivor_present": bool(direct),
        "graph_parent_a3_present": any(row["parent_logical"] == "parent-a3" for row in graph),
        "graph_context_count": len(graph),
        "forbidden_chunk_id": forbidden_chunk_id,
        "forbidden_scope": forbidden_scope,
        "contexts": normalized,
        "failure_codes": sorted(set(failures)),
    }


def stable_result(result: dict, include_entry_point: bool = True) -> dict:
    assertions = dict(result.get("assertions") or {})
    # Search and RetrieveContext intentionally use different candidate limits.
    # Candidate trace size is diagnostic, not part of logical result parity.
    assertions.pop("trace_candidate_count", None)
    stable = {
        "query_id": result.get("query_id"),
        "status": result.get("status"),
        "logical_identity": result.get("logical_identity"),
        "runtime_identity": sorted(
            result.get("runtime_identity") or [],
            key=lambda row: (row.get("graph_origin", False), row.get("parent_chunk_id", ""), row.get("matched_chunk_id", "")),
        ),
        "protected_provenance": sorted(
            result.get("protected_provenance") or [],
            key=lambda row: json.dumps(row, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
        ),
        "assertions": assertions,
        "failure_codes": result.get("failure_codes"),
    }
    if include_entry_point:
        stable["entry_point"] = result.get("entry_point")
    return stable


def compare_result_sets(left_path: Path, right_path: Path, parity: bool) -> dict:
    left = read_jsonl(left_path)
    right = read_jsonl(right_path)
    if parity:
        left_by_query = {row["query_id"]: stable_result(row, False) for row in left if row.get("entry_point") == "Search"}
        right_by_query = {row["query_id"]: stable_result(row, False) for row in right if row.get("entry_point") == "RetrieveContext"}
    else:
        left_by_query = {(row["query_id"], row["entry_point"]): stable_result(row) for row in left}
        right_by_query = {(row["query_id"], row["entry_point"]): stable_result(row) for row in right}
    differences = sorted(str(key) for key in left_by_query.keys() | right_by_query.keys()
                         if left_by_query.get(key) != right_by_query.get(key))
    return {"status": "PASS" if not differences else "FAIL", "differences": differences,
            "left_count": len(left_by_query), "right_count": len(right_by_query)}


def build_manifest(run: Path) -> dict:
    records = []
    for path in sorted(run.rglob("*")):
        if not path.is_file() or path.name in MANIFEST_EXCLUDED_NAMES:
            continue
        relative = path.relative_to(run).as_posix()
        records.append({
            "path": relative,
            "size_bytes": path.stat().st_size,
            "sha256": sha256(path),
            "artifact_class": relative.split("/", 1)[0],
            "mandatory": relative in MANDATORY_PASS_EVIDENCE,
        })
    canonical = json.dumps(
        records, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode()
    return {"schema_version": 1, "records": records,
            "file_count": len(records), "aggregate_sha256": hashlib.sha256(canonical).hexdigest()}


def verify_manifest(run: Path, manifest: dict) -> dict:
    failures = []
    seen = set()
    records = manifest.get("records", [])
    if not isinstance(records, list):
        records = []
        failures.append("MANIFEST_RECORDS_INVALID")
    if manifest.get("file_count") != len(records):
        failures.append("MANIFEST_FILE_COUNT_MISMATCH")
    canonical = json.dumps(
        records, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode()
    if manifest.get("aggregate_sha256") != hashlib.sha256(canonical).hexdigest():
        failures.append("MANIFEST_AGGREGATE_MISMATCH")
    for record in records:
        if not isinstance(record, dict):
            failures.append("MANIFEST_RECORD_INVALID")
            continue
        relative = record.get("path", "")
        if not isinstance(relative, str) or not relative:
            failures.append("MANIFEST_PATH_INVALID")
            continue
        if relative in seen:
            failures.append("DUPLICATE_MANIFEST_PATH")
            continue
        seen.add(relative)
        path = (run / relative).resolve()
        try:
            path.relative_to(run.resolve())
        except ValueError:
            failures.append("ARTIFACT_OUTSIDE_RUN_ROOT")
            continue
        if not path.is_file():
            failures.append(f"MISSING:{relative}")
        elif sha256(path) != record.get("sha256") or path.stat().st_size != record.get("size_bytes"):
            failures.append(f"HASH_MISMATCH:{relative}")
    actual_files = {
        path.relative_to(run).as_posix()
        for path in run.rglob("*")
        if path.is_file() and path.name not in MANIFEST_EXCLUDED_NAMES
    }
    if seen != actual_files:
        failures.append("MANIFEST_FILE_SET_MISMATCH")
    mandatory = {
        record.get("path") for record in records
        if isinstance(record, dict) and record.get("mandatory")
    }
    expected_present = MANDATORY_PASS_EVIDENCE & seen
    if mandatory != expected_present:
        failures.append("MANDATORY_MANIFEST_SET_INVALID")
    aggregate_path = run / "aggregate.json"
    if aggregate_path.is_file():
        aggregate = read_json(aggregate_path)
        if aggregate.get("verdict") == "FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS":
            if not MANDATORY_PASS_EVIDENCE <= seen:
                failures.append("PASS_MANIFEST_MISSING_MANDATORY_ARTIFACT")
            for relative in sorted(MANDATORY_PASS_EVIDENCE & seen):
                path = run / relative
                if path.is_file() and path.stat().st_size == 0:
                    failures.append(f"EMPTY_MANDATORY_ARTIFACT:{relative}")
    return {"status": "PASS" if not failures else "FAIL", "failure_codes": failures,
            "verified_files": len(seen)}


def aggregate(run: Path) -> dict:
    results_path = run / "query-results.jsonl"
    if not results_path.is_file():
        return {
            "verdict": "FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED",
            "failure_codes": ["EVIDENCE_INCOMPLETE"],
            "primary_result_count": 0,
        }
    results = read_jsonl(results_path)
    expected = {(query, point) for query in REQUIRED_QUERIES for point in REQUIRED_ENTRY_POINTS}
    actual = {(item.get("query_id"), item.get("entry_point")) for item in results}
    failures = []
    if actual != expected or len(results) != len(expected):
        failures.append("MANDATORY_QUERY_SKIPPED")
    if any(item.get("status") != "PASS" for item in results):
        failures.append("MANDATORY_QUERY_FAILED")
    for required in [
        "identity-map/logical-to-runtime.json", "canonical-audit/integrity-summary.json",
        "qdrant-audit/payload-consistency.json", "comparisons/entry-point-parity.json",
        "comparisons/warm-repeat.json", "restart/pre-post-restart.json",
        "cleanup/summary.json", "stage-results.json", "defect-register.json",
    ]:
        if not (run / required).is_file():
            failures.append("EVIDENCE_INCOMPLETE")
    audit = read_json(run / "canonical-audit/integrity-summary.json") if (run / "canonical-audit/integrity-summary.json").is_file() else {}
    for key in ["orphan_children", "cross_document_bindings", "cross_version_bindings", "cross_zone_bindings", "duplicate_chunk_ids", "duplicate_source_provenance_rows"]:
        if audit.get(key) != 0:
            failures.append("CANONICAL_BINDING_INVALID")
    qdrant = read_json(run / "qdrant-audit/payload-consistency.json") if (run / "qdrant-audit/payload-consistency.json").is_file() else {}
    if qdrant.get("status") != "PASS" or not qdrant.get("count_match"):
        failures.append("QDRANT_AUDIT_INVALID")
    for relative in ["comparisons/entry-point-parity.json", "comparisons/warm-repeat.json", "restart/pre-post-restart.json", "cleanup/summary.json"]:
        path = run / relative
        if path.is_file() and read_json(path).get("status") != "PASS":
            failures.append("EVIDENCE_ASSERTION_FAILED")
    defects = read_json(run / "defect-register.json") if (run / "defect-register.json").is_file() else {}
    if defects.get("unresolved_in_scope_p0") != 0 or defects.get("unresolved_in_scope_p1") != 0:
        failures.append("UNRESOLVED_DEFECTS")
    stages = read_json(run / "stage-results.json") if (run / "stage-results.json").is_file() else {}
    if any(stage.get("status") != "PASS" for stage in stages.get("stages", [])):
        failures.append("MANDATORY_STAGE_FAILED")
    return {"verdict": "FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS" if not failures else "FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED", "failure_codes": sorted(set(failures)), "primary_result_count": len(results)}


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    p_select = sub.add_parser("select")
    p_select.add_argument("--bank", type=Path, required=True)
    p_select.add_argument("--output", type=Path, required=True)
    p_supplemental = sub.add_parser("verify-supplemental")
    p_supplemental.add_argument("--bank", type=Path, required=True)
    p_supplemental.add_argument("--output", type=Path, required=True)
    p_identity = sub.add_parser("validate-identity")
    p_identity.add_argument("--input", type=Path, required=True)
    p_identity.add_argument("--bank", type=Path)
    p_identity.add_argument("--classified-output", type=Path)
    p_norm = sub.add_parser("normalize")
    p_norm.add_argument("--query", type=Path, required=True)
    p_norm.add_argument("--qrel", type=Path, required=True)
    p_norm.add_argument("--entry-point", required=True, choices=REQUIRED_ENTRY_POINTS)
    p_norm.add_argument("--response", type=Path, required=True)
    p_norm.add_argument("--identity-map", type=Path, required=True)
    p_norm.add_argument("--bank", type=Path, required=True)
    p_norm.add_argument("--output", type=Path, required=True)
    p_norm.add_argument("--expect-graph", required=True, choices=["true", "false"])
    p_control = sub.add_parser("validate-control")
    p_control.add_argument("--entry-point", required=True, choices=REQUIRED_ENTRY_POINTS)
    p_control.add_argument("--response", type=Path, required=True)
    p_control.add_argument("--identity-map", type=Path, required=True)
    p_control.add_argument("--bank", type=Path, required=True)
    p_control.add_argument("--graph-expectation", required=True, choices=["present", "absent"])
    p_control.add_argument("--forbidden-chunk-id")
    p_control.add_argument("--forbidden-scope", choices=["any", "graph"], default="any")
    p_control.add_argument("--output", type=Path, required=True)
    p_agg = sub.add_parser("aggregate")
    p_agg.add_argument("--run", type=Path, required=True)
    p_agg.add_argument("--output", type=Path, required=True)
    p_compare = sub.add_parser("compare")
    p_compare.add_argument("--left", type=Path, required=True)
    p_compare.add_argument("--right", type=Path, required=True)
    p_compare.add_argument("--parity", action="store_true")
    p_compare.add_argument("--output", type=Path, required=True)
    p_manifest = sub.add_parser("manifest")
    p_manifest.add_argument("--run", type=Path, required=True)
    p_manifest.add_argument("--output", type=Path, required=True)
    p_verify_manifest = sub.add_parser("verify-manifest")
    p_verify_manifest.add_argument("--run", type=Path, required=True)
    p_verify_manifest.add_argument("--manifest", type=Path, required=True)
    p_verify_manifest.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "select":
            payload = select_frozen_queries(args.bank)
        elif args.command == "verify-supplemental":
            payload = verify_supplemental(args.bank)
        elif args.command == "validate-identity":
            payload = read_json(args.input)
            rows = payload["rows"] if isinstance(payload, dict) else payload
            if args.bank:
                rows = apply_frozen_child_identities(rows, args.bank)
            validate_identity_map(rows)
            roles: dict[str, int] = {}
            for row in rows:
                role = row.get("identity_role", "UNCLASSIFIED")
                roles[role] = roles.get(role, 0) + 1
            if args.classified_output:
                args.classified_output.parent.mkdir(parents=True, exist_ok=True)
                args.classified_output.write_text(
                    json.dumps({"rows": rows}, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8",
                )
            payload = {"status": "PASS", "identity_roles": roles}
        elif args.command == "normalize":
            identity = apply_frozen_child_identities(read_json(args.identity_map)["rows"], args.bank)
            by_runtime = {row["runtime_chunk_id"]: row for row in identity}
            payload = normalize(
                read_json(args.query), read_json(args.qrel), args.entry_point,
                read_json(args.response), by_runtime, args.expect_graph == "true",
            )
        elif args.command == "validate-control":
            identity = apply_frozen_child_identities(read_json(args.identity_map)["rows"], args.bank)
            by_runtime = {row["runtime_chunk_id"]: row for row in identity}
            payload = validate_control(
                args.entry_point, read_json(args.response), by_runtime,
                args.graph_expectation, args.forbidden_chunk_id, args.forbidden_scope,
            )
        elif args.command == "aggregate":
            payload = aggregate(args.run)
        elif args.command == "compare":
            payload = compare_result_sets(args.left, args.right, args.parity)
        elif args.command == "manifest":
            payload = build_manifest(args.run)
        else:
            payload = verify_manifest(args.run, read_json(args.manifest))
        output = getattr(args, "output", None)
        if output is not None:
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(payload, ensure_ascii=False))
        if isinstance(payload, list):
            return 0
        return 0 if payload.get("status", "PASS") == "PASS" and payload.get("verdict", "FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS") == "FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS" else 1
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"FIX486G_ERROR={error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
