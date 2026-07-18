#!/usr/bin/env python3
"""Fail-closed verification and planning for the FIX486C frozen bank."""

import argparse
import hashlib
import json
import sys
from pathlib import Path


PAYLOADS = (
    ("corpus", "corpus/hierarchical-fixture-v1.json"),
    ("queries", "queries/hierarchical-queries-v1.jsonl"),
    ("qrels", "qrels/hierarchical-qrels-v1.jsonl"),
    ("graph", "graph-relations/hierarchical-graph-v1.json"),
    ("lifecycle", "lifecycle/hierarchical-lifecycle-v1.json"),
)
ALLOWED_FILES = {"bank-manifest.json", *(path for _, path in PAYLOADS)}
SHA256_LENGTH = 64


class VerificationError(Exception):
    pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_canonical(path: Path) -> bytes:
    data = path.read_bytes()
    if data.startswith(b"\xef\xbb\xbf"):
        raise VerificationError(f"{path}: UTF-8 BOM is forbidden")
    if b"\x00" in data:
        raise VerificationError(f"{path}: NUL byte is forbidden")
    if b"\r" in data:
        raise VerificationError(f"{path}: LF line endings are required")
    if not data.endswith(b"\n") or data.endswith(b"\n\n"):
        raise VerificationError(f"{path}: must end with exactly one LF")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise VerificationError(f"{path}: invalid UTF-8: {error}") from error
    if any(line.rstrip(" \t") != line for line in text.splitlines()):
        raise VerificationError(f"{path}: trailing whitespace is forbidden")
    return data


def parse_json(path: Path) -> object:
    try:
        return json.loads(read_canonical(path))
    except json.JSONDecodeError as error:
        raise VerificationError(f"{path}: invalid JSON: {error}") from error


def parse_jsonl(path: Path) -> list[dict]:
    rows = []
    for number, line in enumerate(read_canonical(path).decode("utf-8").splitlines(), 1):
        if not line:
            raise VerificationError(f"{path}:{number}: blank JSONL row is forbidden")
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            raise VerificationError(f"{path}:{number}: invalid JSONL row: {error}") from error
        if not isinstance(row, dict):
            raise VerificationError(f"{path}:{number}: JSONL row must be an object")
        rows.append(row)
    return rows


def required_string(value: dict, field: str, context: str) -> str:
    result = value.get(field)
    if not isinstance(result, str) or not result:
        raise VerificationError(f"{context}: required non-empty string {field}")
    return result


def aggregate_hash(per_file: dict[str, str]) -> str:
    payload = "".join(f"{path}\t{per_file[path]}\n" for _, path in PAYLOADS)
    return sha256(payload.encode("utf-8"))


def assert_exact_file_set(root: Path) -> None:
    found = {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()}
    missing = ALLOWED_FILES - found
    extra = found - ALLOWED_FILES
    if missing:
        raise VerificationError(f"missing frozen bank files: {sorted(missing)}")
    if extra:
        raise VerificationError(f"untracked frozen bank files: {sorted(extra)}")


def collect_logical_identities(corpus: dict) -> tuple[set[str], set[tuple[str, str]], set[tuple[str, str]], set[tuple[str, str, int]]]:
    zones: set[str] = set()
    parents: set[tuple[str, str]] = set()
    children: set[tuple[str, str]] = set()
    versions: set[tuple[str, str, int]] = set()
    for zone in corpus.get("zones", []):
        zone_id = required_string(zone, "logical_zone_id", "corpus zone")
        zones.add(zone_id)
        for document in zone.get("documents", []):
            document_id = required_string(document, "logical_document_id", f"zone {zone_id}")
            for version in document.get("versions", []):
                version_number = version.get("document_version")
                if not isinstance(version_number, int) or version_number < 1:
                    raise VerificationError(f"{zone_id}/{document_id}: invalid document_version")
                versions.add((zone_id, document_id, version_number))
                for block in version.get("blocks", []):
                    hierarchy = block.get("expected_hierarchy")
                    if not hierarchy:
                        continue
                    parent = required_string(hierarchy, "logical_parent_id", "expected_hierarchy")
                    parents.add((zone_id, parent))
                    for child in hierarchy.get("children", []):
                        child_id = required_string(child, "logical_child_id", "expected_hierarchy child")
                        if child.get("granularity") not in {"SUB_180", "SUB_260"}:
                            raise VerificationError(f"{zone_id}/{child_id}: unsupported child granularity")
                        children.add((zone_id, child_id))
    return zones, parents, children, versions


def validate_semantics(corpus: dict, queries: list[dict], qrels: list[dict], graph: dict, lifecycle: dict) -> None:
    zones, parents, children, versions = collect_logical_identities(corpus)
    query_by_id: dict[str, dict] = {}
    cases: set[str] = set()
    for query in queries:
        query_id = required_string(query, "query_id", "query")
        if query_id in query_by_id:
            raise VerificationError(f"duplicate query_id: {query_id}")
        zone = required_string(query, "access_zone", f"query {query_id}")
        if zone not in zones:
            raise VerificationError(f"query {query_id}: unknown access zone {zone}")
        required_string(query, "case_id", f"query {query_id}")
        required_string(query, "question", f"query {query_id}")
        required_string(query, "profile", f"query {query_id}")
        if not isinstance(query.get("max_contexts"), int) or query["max_contexts"] <= 0:
            raise VerificationError(f"query {query_id}: max_contexts must be positive")
        if "max_context_tokens" in query and (not isinstance(query["max_context_tokens"], int) or query["max_context_tokens"] <= 0):
            raise VerificationError(f"query {query_id}: max_context_tokens must be positive")
        if "graph_max_hops" in query and (not isinstance(query["graph_max_hops"], int) or query["graph_max_hops"] < 0):
            raise VerificationError(f"query {query_id}: graph_max_hops must be non-negative")
        query_by_id[query_id] = query
        cases.add(query["case_id"])
    if len(queries) != 11 or len(cases) != 10:
        raise VerificationError(f"expected 11 queries across 10 cases, got {len(queries)} queries across {len(cases)} cases")

    qrel_by_id: dict[str, dict] = {}
    for qrel in qrels:
        query_id = required_string(qrel, "query_id", "qrel")
        if query_id in qrel_by_id:
            raise VerificationError(f"duplicate qrel query_id: {query_id}")
        query = query_by_id.get(query_id)
        if query is None:
            raise VerificationError(f"orphan qrel: {query_id}")
        if qrel.get("case_id") != query.get("case_id") or qrel.get("expected_zone") != query.get("access_zone"):
            raise VerificationError(f"qrel {query_id}: query identity mismatch")
        zone = qrel["expected_zone"]
        parent = qrel.get("expected_parent")
        if parent is not None and (zone, parent) not in parents:
            raise VerificationError(f"qrel {query_id}: unresolved logical parent {parent} in {zone}")
        for child in qrel.get("expected_child_any", []):
            if (zone, child) not in children:
                raise VerificationError(f"qrel {query_id}: unresolved logical child {child} in {zone}")
        qrel_by_id[query_id] = qrel
    if len(qrels) != 11 or set(qrel_by_id) != set(query_by_id):
        raise VerificationError("every query must have exactly one qrel")

    for relation in graph.get("relations", []):
        zone = required_string(relation, "access_zone", "graph relation")
        for field in ("from_logical_child", "to_logical_child"):
            if (zone, required_string(relation, field, "graph relation")) not in children:
                raise VerificationError(f"graph relation has unknown endpoint {field}")
        for prefix in ("from", "to"):
            if (zone, relation.get(f"{prefix}_document"), relation.get(f"{prefix}_version")) not in versions:
                raise VerificationError(f"graph relation has unknown {prefix} document/version")

    scenario_ids: set[str] = set()
    for scenario in lifecycle.get("scenarios", []):
        scenario_id = required_string(scenario, "scenario_id", "lifecycle scenario")
        if scenario_id in scenario_ids:
            raise VerificationError(f"duplicate lifecycle scenario {scenario_id}")
        scenario_ids.add(scenario_id)
        zone = required_string(scenario, "access_zone", f"scenario {scenario_id}")
        if zone not in zones:
            raise VerificationError(f"scenario {scenario_id}: unknown access zone")
        if "document" in scenario and "version" in scenario and (zone, scenario["document"], scenario["version"]) not in versions:
            raise VerificationError(f"scenario {scenario_id}: unknown document/version")


def verify(root: Path) -> dict:
    root = root.resolve()
    assert_exact_file_set(root)
    manifest = parse_json(root / "bank-manifest.json")
    if manifest.get("bank_id") != "fix486-hierarchical-bank":
        raise VerificationError("manifest bank_id mismatch")
    if manifest.get("bank_version") != "1.0.0" or manifest.get("status") != "FROZEN":
        raise VerificationError("manifest must be exactly 1.0.0/FROZEN")
    if manifest.get("query_count") != 11 or manifest.get("case_count") != 10:
        raise VerificationError("manifest query/case count mismatch")
    hashes = manifest.get("hashes")
    if not isinstance(hashes, dict) or hashes.get("status") != "RESOLVED":
        raise VerificationError("manifest hashes must be RESOLVED")
    per_file = {path: sha256(read_canonical(root / path)) for _, path in PAYLOADS}
    for name, path in PAYLOADS:
        actual = per_file[path]
        expected = hashes.get(f"{name}_sha256")
        if not isinstance(expected, str) or len(expected) != SHA256_LENGTH or expected != expected.lower():
            raise VerificationError(f"manifest {name}_sha256 is malformed")
        if actual != expected:
            raise VerificationError(f"hash mismatch for {path}")
    actual_aggregate = aggregate_hash(per_file)
    if hashes.get("aggregate_sha256") != actual_aggregate:
        raise VerificationError("aggregate hash mismatch")
    corpus = parse_json(root / "corpus/hierarchical-fixture-v1.json")
    queries = parse_jsonl(root / "queries/hierarchical-queries-v1.jsonl")
    qrels = parse_jsonl(root / "qrels/hierarchical-qrels-v1.jsonl")
    graph = parse_json(root / "graph-relations/hierarchical-graph-v1.json")
    lifecycle = parse_json(root / "lifecycle/hierarchical-lifecycle-v1.json")
    validate_semantics(corpus, queries, qrels, graph, lifecycle)
    return {"bank_id": manifest["bank_id"], "bank_version": manifest["bank_version"], "bank_aggregate_sha256": actual_aggregate, "case_count": 10, "query_count": len(queries), "qrel_count": len(qrels), "payload_hashes": {name: per_file[path] for name, path in PAYLOADS}, "queries": queries, "corpus": corpus}


def plans(result: dict) -> list[dict]:
    supported = {"TECHNICAL", "LEXICAL_STRICT", "BALANCED"}
    output = []
    for query in result["queries"]:
        if query["profile"] not in supported:
            raise VerificationError(f"query {query['query_id']}: unsupported profile")
        output.append({"query_id": query["query_id"], "case_id": query["case_id"], "status": "PASS", "logical_access_zone": query["access_zone"], "question": query["question"], "profile": query["profile"], "max_contexts": query["max_contexts"], "enable_graph_expansion": query.get("enable_graph_expansion", False), "graph_max_hops": query.get("graph_max_hops"), "max_context_tokens": query.get("max_context_tokens"), "bank_aggregate_sha256": result["bank_aggregate_sha256"]})
    return output


def materialize_block_text(block: dict) -> str:
    if "text" in block:
        return block["text"]
    generation = block.get("text_generation")
    if not isinstance(generation, dict):
        raise VerificationError(f"block {block.get('block_id')}: no executable text")
    target = generation.get("target_canonical_tokens")
    anchor = required_string(generation, "required_anchor", "text_generation")
    if not isinstance(target, int) or target <= 0:
        raise VerificationError("text_generation target_canonical_tokens must be positive")
    sentence = f"{anchor} canonical-state evidence remains bounded and independent."
    words = (sentence.split() * (target // len(sentence.split()) + 1))[:target]
    return " ".join(words)


def ingestion_plans(result: dict) -> list[dict]:
    zone_codes = {"zone-a": "4862", "zone-b": "4863"}
    output = []
    for zone in result["corpus"].get("zones", []):
        logical_zone = zone["logical_zone_id"]
        if logical_zone not in zone_codes:
            raise VerificationError(f"no runtime access-zone code mapping for {logical_zone}")
        for document in zone.get("documents", []):
            for version in document.get("versions", []):
                if version.get("expected_state") != "ACTIVE":
                    continue
                blocks = []
                for block in version.get("blocks", []):
                    source = block.get("source_location", {})
                    blocks.append({
                        "blockId": block["block_id"], "parentBlockId": block.get("parent_block_id", ""),
                        "blockType": "BLOCK_TYPE_DOCUMENT" if block.get("block_type") == "DOCUMENT" else "BLOCK_TYPE_SECTION",
                        "text": materialize_block_text(block), "orderIndex": block["order_index"],
                        "sourceLocation": {"pageStart": source.get("page_start", 0), "pageEnd": source.get("page_end", 0), "sectionPath": source.get("section_path", ""), "heading": source.get("heading", "")},
                    })
                external = document["external_document_id"]
                output.append({
                    "logical_zone_id": logical_zone,
                    "runtime_access_zone_code": zone_codes[logical_zone],
                    "logical_document_id": document["logical_document_id"],
                    "document_version": version["document_version"],
                    "request": {
                        "context": {"correlationId": f"fix486c-{logical_zone}-{external}-v{version['document_version']}", "idempotencyKey": f"fix486c-bank-1-0-0-{logical_zone}-{external}-v{version['document_version']}", "callerService": "fix486c-frozen-bank", "callerUserId": "fix486c-frozen-bank", "callerAccessLevel": "INTERNAL"},
                        "accessZoneCode": zone_codes[logical_zone],
                        "document": {"externalDocumentId": external, "documentVersion": version["document_version"], "title": document["title"], "sourceUri": document["source_uri"], "sourceType": document["source_type"], "mimeType": document["mime_type"], "contentHash": ""},
                        "blocks": blocks,
                        "chunkingOptions": {"profile": "CHUNKING_PROFILE_TECHNICAL", "parentTargetTokens": 256, "parentMaxTokens": 512, "childTargetTokens": 180, "childMaxTokens": 260, "childOverlapTokens": 30, "minChunkTokens": 8, "preserveBlockBoundaries": True, "allowSplitInsideParagraph": False, "allowSplitInsideTable": False, "createParentContext": True},
                        "indexingOptions": {"activationPolicy": "ACTIVATION_POLICY_MANUAL", "embeddingMode": "EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE", "publishMode": "PUBLISH_MODE_V005_OUTBOX", "replaceExistingVersion": True},
                        "metadata": {"fix486c_bank_id": "fix486-hierarchical-bank", "fix486c_bank_version": "1.0.0", "fix486c_logical_zone": logical_zone, "fix486c_logical_document": document["logical_document_id"]},
                    },
                })
    return output


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("benchmarks/hierarchical/fix486"))
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--emit-ingestion-plans", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        result = verify(args.root)
        payload = {key: value for key, value in result.items() if key not in {"queries", "corpus"}}
        payload["status"] = "PASS"
        if args.dry_run:
            payload["plans"] = plans(result)
            payload["scheduled_queries"] = len(payload["plans"])
        if args.emit_ingestion_plans:
            payload["ingestion_plans"] = ingestion_plans(result)
        encoded = json.dumps(payload, ensure_ascii=False, indent=2) + "\n"
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(encoded, encoding="utf-8")
        else:
            sys.stdout.write(encoded)
        return 0
    except VerificationError as error:
        print(f"FIX486C_VERIFY_FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
