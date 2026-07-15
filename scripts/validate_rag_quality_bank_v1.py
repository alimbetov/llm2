#!/usr/bin/env python3
"""Structural validator for the rag-quality-bank-v1 fixture bank.

This validator is intentionally runtime-independent. It proves that the
development bank loads all declared files and all 42 queries before any
model-backed evaluation or blind adjudication is attempted.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
QUALITY = ROOT / "benchmarks" / "quality"
PROFILE_NAME = "rag-quality-bank-v1"
EXPECTED_QUERY_FILES = {
    "rag-quality-bank-v1-access": 8,
    "rag-quality-bank-v1-semantic": 10,
    "rag-quality-bank-v1-lexical": 4,
    "rag-quality-bank-v1-graph": 3,
    "rag-quality-bank-v1-mmr": 3,
    "rag-quality-bank-v1-long": 4,
    "rag-quality-bank-v1-distractor": 4,
    "rag-quality-bank-v1-negative": 6,
}
EXPECTED_TOTAL = 42


def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise SystemExit(f"failed to read {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"invalid JSON in {path}: {error}") from error


def read_jsonl(path: Path) -> list[dict]:
    rows = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise SystemExit(f"failed to read {path}: {error}") from error
    for number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise SystemExit(f"invalid JSONL in {path}:{number}: {error}") from error
        if not isinstance(value, dict):
            raise SystemExit(f"{path}:{number} must contain a JSON object")
        rows.append(value)
    return rows


def require_string(value: dict, key: str, source: Path) -> str:
    item = value.get(key)
    if not isinstance(item, str) or not item:
        raise SystemExit(f"{source} missing non-empty string field {key!r}")
    return item


def load_corpus_index(corpora: list[str]) -> tuple[set[str], set[str], dict[str, dict]]:
    documents: set[str] = set()
    blocks: set[str] = set()
    block_rows: dict[str, dict] = {}
    for corpus in corpora:
        path = QUALITY / "corpora" / corpus / "documents.jsonl"
        for document in read_jsonl(path):
            document_id = require_string(document, "document_id", path)
            if document_id in documents:
                raise SystemExit(f"duplicate document_id {document_id}")
            documents.add(document_id)
            for block in document.get("logical_blocks", document.get("blocks", [])):
                if not isinstance(block, dict):
                    raise SystemExit(f"{path} document {document_id} contains non-object block")
                block_id = require_string(block, "block_id", path)
                if block_id in blocks:
                    raise SystemExit(f"duplicate block_id {block_id}")
                blocks.add(block_id)
                block_rows[block_id] = {
                    "document_id": document_id,
                    "heading": block.get("heading", ""),
                    "text": block.get("text", ""),
                }
    return documents, blocks, block_rows


def validate_relations(corpora: list[str], documents: set[str], blocks: set[str]) -> int:
    relations = 0
    for corpus in corpora:
        path = QUALITY / "corpora" / corpus / "relations.jsonl"
        if not path.exists():
            continue
        for relation in read_jsonl(path):
            for key in ("from_document_id", "to_document_id"):
                document_id = require_string(relation, key, path)
                if document_id not in documents:
                    raise SystemExit(f"{path} references missing {key} {document_id}")
            for key in ("from_block_id", "to_block_id"):
                block_id = require_string(relation, key, path)
                if block_id not in blocks:
                    raise SystemExit(f"{path} references missing {key} {block_id}")
            relations += 1
    return relations


def string_array(value: dict, key: str) -> list[str]:
    item = value.get(key, [])
    if item is None:
        return []
    if not isinstance(item, list) or not all(isinstance(entry, str) for entry in item):
        raise SystemExit(f"expected.{key} must be an array of strings")
    return item


def validate_queries(profile: dict, documents: set[str], blocks: set[str]) -> tuple[list[dict], dict]:
    declared = profile.get("queries")
    if declared != list(EXPECTED_QUERY_FILES):
        raise SystemExit(
            "rag-quality-bank-v1 query files changed or reordered: "
            f"expected {list(EXPECTED_QUERY_FILES)}, got {declared}"
        )

    rows: list[dict] = []
    seen_ids: set[str] = set()
    category_counts: Counter[str] = Counter()
    file_counts: dict[str, int] = {}
    for query_file, expected_count in EXPECTED_QUERY_FILES.items():
        path = QUALITY / "queries" / f"{query_file}.jsonl"
        queries = read_jsonl(path)
        file_counts[query_file] = len(queries)
        if len(queries) != expected_count:
            raise SystemExit(f"{query_file} expected {expected_count} queries, got {len(queries)}")
        for query in queries:
            query_id = require_string(query, "id", path)
            if query_id in seen_ids:
                raise SystemExit(f"duplicate query id {query_id}")
            seen_ids.add(query_id)
            if query.get("schema_version") != "1.0":
                raise SystemExit(f"{query_id} must use schema_version 1.0")
            require_string(query, "question", path)
            category_counts[require_string(query, "category", path)] += 1
            context = query.get("context")
            if not isinstance(context, dict):
                raise SystemExit(f"{query_id} missing context object")
            require_string(context, "access_zone_code", path)
            require_string(context, "caller_access_level", path)
            require_string(context, "search_mode", path)
            expected = query.get("expected")
            if not isinstance(expected, dict):
                raise SystemExit(f"{query_id} missing expected object")
            for key in ("must_contain_document_ids", "forbidden_document_ids", "allowed_document_ids"):
                for document_id in string_array(expected, key):
                    if document_id not in documents:
                        raise SystemExit(f"{query_id} references missing document {document_id} in {key}")
            for key in ("must_contain_block_ids", "expected_related_block_ids", "forbidden_block_ids"):
                for block_id in string_array(expected, key):
                    if block_id not in blocks:
                        raise SystemExit(f"{query_id} references missing block {block_id} in {key}")
            if expected.get("hard_negative") is True and expected.get("expected_empty") is not True:
                raise SystemExit(f"{query_id} hard_negative must also set expected_empty=true")
            rows.append(query)

    if len(rows) != EXPECTED_TOTAL:
        raise SystemExit(f"expected {EXPECTED_TOTAL} queries, got {len(rows)}")
    return rows, {
        "file_counts": file_counts,
        "category_counts": dict(sorted(category_counts.items())),
    }


def write_report(output: Path, report: dict) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "target" / "fix482-evidence" / "rag-quality-bank-v1-structural-report.json",
    )
    args = parser.parse_args()

    profile_path = QUALITY / "profiles" / f"{PROFILE_NAME}.json"
    profile = read_json(profile_path)
    if profile.get("name") != PROFILE_NAME:
        raise SystemExit(f"{profile_path} name must be {PROFILE_NAME}")
    corpora = profile.get("corpora")
    if not isinstance(corpora, list) or not all(isinstance(item, str) for item in corpora):
        raise SystemExit(f"{profile_path} corpora must be an array of strings")

    documents, blocks, block_rows = load_corpus_index(corpora)
    relation_count = validate_relations(corpora, documents, blocks)
    queries, query_stats = validate_queries(profile, documents, blocks)
    report = {
        "schema_version": 1,
        "profile": PROFILE_NAME,
        "status": "PASS",
        "queries_loaded": len(queries),
        "queries_expected": EXPECTED_TOTAL,
        "query_file_counts": query_stats["file_counts"],
        "category_counts": query_stats["category_counts"],
        "corpora_loaded": len(corpora),
        "documents_indexed": len(documents),
        "blocks_indexed": len(block_rows),
        "relations_loaded": relation_count,
    }
    write_report(args.output, report)
    print(json.dumps(report, indent=2, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(1)
