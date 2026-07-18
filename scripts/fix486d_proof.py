#!/usr/bin/env python3
"""Fail-closed normalization and aggregation for the fix486d proof.

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
    "q-child-parent-exact": "FIX486-01",
    "q-parent-dedup": "FIX486-02",
    "q-exact-identifier": "FIX486-07",
}
REQUIRED_ENTRY_POINTS = {"Search", "RetrieveContext"}
REQUIRED_IDENTITY_FIELDS = {
    "logical_zone_id", "runtime_access_zone_id", "logical_document_id",
    "runtime_document_id", "logical_version", "runtime_chunk_id",
    "chunk_role", "granularity", "source_block_id", "content_sha256",
}


def fail(code: str, detail: str) -> None:
    raise ValueError(f"{code}: {detail}")


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl(path: Path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


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
    needed = {"parent-a1", "child-a1-180", "child-a1-260"}
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
            if logical == "parent-a1":
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


def stage_is_present(candidate: dict, name: str) -> bool:
    return any(stage.get("stage") == name and stage.get("present") for stage in candidate.get("stages", []))


def normalize(query: dict, qrel: dict, entry_point: str, response: dict, identity_by_runtime: dict[str, dict]) -> dict:
    contexts = response_contexts(response, entry_point)
    if not contexts:
        fail("MATCHED_CHILD_NOT_PRESERVED", f"{query['query_id']} {entry_point} no contexts")
    result = contexts[0]
    matched = result.get("matchedChunkId", "")
    parent = result.get("parentChunkId", "")
    matched_logical = identity_by_runtime.get(matched, {}).get("logical_chunk_id")
    parent_logical = identity_by_runtime.get(parent, {}).get("logical_chunk_id")
    matched_text = result.get("matchedText", "")
    parent_text = result.get("parentText", "")
    document_version = protobuf_positive_int(result.get("documentVersion"), "documentVersion")
    failures: list[str] = []
    if result.get("accessZoneId") != identity_by_runtime.get(matched, {}).get("runtime_access_zone_id"):
        failures.append("CANONICAL_BINDING_INVALID")
    if document_version != 1:
        failures.append("CANONICAL_BINDING_INVALID")
    if parent_logical != qrel.get("expected_parent"):
        failures.append("PARENT_HYDRATION_INVALID")
    if qrel.get("expected_child_any") and matched_logical not in qrel["expected_child_any"]:
        failures.append("MATCHED_CHILD_NOT_PRESERVED")
    if not matched or not parent or matched == parent:
        failures.append("CANONICAL_BINDING_INVALID")
    for anchor in qrel.get("required_anchors_in_matched_text", []):
        if anchor not in matched_text:
            failures.append("MATCHED_CHILD_NOT_PRESERVED")
    for anchor in qrel.get("required_anchors_in_parent_text", []):
        if anchor not in parent_text:
            failures.append("PARENT_HYDRATION_INVALID")
    combined = matched_text + "\n" + parent_text
    forbidden = [a for a in qrel.get("forbidden_anchors", []) if a in combined]
    if forbidden:
        failures.append("CANONICAL_BINDING_INVALID")
    trace = trace_candidates(response)
    matching_trace = next((c for c in trace if c.get("identity", {}).get("matchedChunkId") == matched), {})
    if query["query_id"] == "q-exact-identifier":
        if not matching_trace.get("exactTechnicalMatch", False):
            failures.append("EXACT_IDENTIFIER_EVIDENCE_LOST")
        stages = matching_trace.get("stages", [])
        if not any(float(s.get("sparseScore", 0)) > 0 or float(s.get("lexicalScore", 0)) > 0 for s in stages):
            failures.append("EXACT_IDENTIFIER_EVIDENCE_LOST")
    if query["query_id"] == "q-parent-dedup":
        eligible = [c for c in trace if c.get("identity", {}).get("parentChunkId") == parent and stage_is_present(c, "POST_FUSION_DEDUP")]
        if len(eligible) < 2:
            failures.append("PARENT_DEDUP_FAILED")
        final_parents = [c.get("parentChunkId") for c in contexts]
        if final_parents.count(parent) != 1 or len(final_parents) != len(set(final_parents)):
            failures.append("PARENT_DEDUP_FAILED")
    return {
        "schema_version": 1,
        "phase": "fix486d",
        "query_id": query["query_id"],
        "case_id": query["case_id"],
        "entry_point": entry_point,
        "status": "PASS" if not failures else "FAIL",
        "reason": None if not failures else failures[0],
        "runtime_identity": {
            "access_zone_id": result.get("accessZoneId"), "document_id": result.get("documentId"),
            "document_version": document_version, "matched_chunk_id": matched,
            "parent_chunk_id": parent, "matched_source_block_id": identity_by_runtime.get(matched, {}).get("source_block_id"),
            "parent_source_block_id": identity_by_runtime.get(parent, {}).get("source_block_id"),
        },
        "logical_identity": {"zone": "zone-a", "document": "doc-hierarchy", "version": 1,
                             "matched_child": matched_logical, "parent": parent_logical},
        "assertions": {"forbidden_anchors_found": forbidden, "trace_candidate_count": len(trace)},
        "failure_codes": sorted(set(failures)),
    }


def aggregate(run: Path) -> dict:
    results_path = run / "query-results.jsonl"
    if not results_path.is_file():
        return {
            "verdict": "FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED",
            "failure_codes": ["EVIDENCE_INCOMPLETE"],
            "primary_result_count": 0,
        }
    results = read_jsonl(results_path)
    expected = {(query, point) for query in REQUIRED_QUERIES for point in REQUIRED_ENTRY_POINTS}
    actual = {(item.get("query_id"), item.get("entry_point")) for item in results}
    failures = []
    if actual != expected:
        failures.append("MANDATORY_QUERY_SKIPPED")
    if any(item.get("status") != "PASS" for item in results):
        failures.append("MANDATORY_QUERY_FAILED")
    for required in ["identity-map/logical-to-runtime.json", "canonical-audit/integrity-summary.json", "qdrant-audit/summary.json"]:
        if not (run / required).is_file():
            failures.append("EVIDENCE_INCOMPLETE")
    audit = read_json(run / "canonical-audit/integrity-summary.json") if (run / "canonical-audit/integrity-summary.json").is_file() else {}
    for key in ["orphan_children", "cross_document_bindings", "cross_version_bindings", "cross_zone_bindings", "duplicate_chunk_ids", "duplicate_source_provenance_rows"]:
        if audit.get(key) != 0:
            failures.append("CANONICAL_BINDING_INVALID")
    return {"verdict": "FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS" if not failures else "FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED", "failure_codes": sorted(set(failures)), "primary_result_count": len(results)}


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    p_select = sub.add_parser("select")
    p_select.add_argument("--bank", type=Path, required=True)
    p_select.add_argument("--output", type=Path, required=True)
    p_identity = sub.add_parser("validate-identity")
    p_identity.add_argument("--input", type=Path, required=True)
    p_identity.add_argument("--bank", type=Path)
    p_norm = sub.add_parser("normalize")
    p_norm.add_argument("--query", type=Path, required=True)
    p_norm.add_argument("--qrel", type=Path, required=True)
    p_norm.add_argument("--entry-point", required=True, choices=REQUIRED_ENTRY_POINTS)
    p_norm.add_argument("--response", type=Path, required=True)
    p_norm.add_argument("--identity-map", type=Path, required=True)
    p_norm.add_argument("--bank", type=Path, required=True)
    p_norm.add_argument("--output", type=Path, required=True)
    p_agg = sub.add_parser("aggregate")
    p_agg.add_argument("--run", type=Path, required=True)
    p_agg.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "select":
            payload = select_frozen_queries(args.bank)
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
            payload = {"status": "PASS", "identity_roles": roles}
        elif args.command == "normalize":
            identity = apply_frozen_child_identities(read_json(args.identity_map)["rows"], args.bank)
            by_runtime = {row["runtime_chunk_id"]: row for row in identity}
            payload = normalize(read_json(args.query), read_json(args.qrel), args.entry_point, read_json(args.response), by_runtime)
        else:
            payload = aggregate(args.run)
        output = getattr(args, "output", None)
        if output is not None:
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(payload, ensure_ascii=False))
        if isinstance(payload, list):
            return 0
        return 0 if payload.get("status", "PASS") == "PASS" and payload.get("verdict", "FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS") == "FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS" else 1
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"FIX486D_ERROR={error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
