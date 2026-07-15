#!/usr/bin/env python3
"""Prepare independent blind judgment inputs for rag-quality-bank-v1.

The generated blind template deliberately contains no production rank, retrieval
source, block id, document id, or expected-label fields. Relevance remains null
until a human blind adjudication pass fills it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import re
import sys
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
QUALITY = ROOT / "benchmarks" / "quality"
PROFILE = "rag-quality-bank-v1"
TOKEN_RE = re.compile(r"[\w-]+", re.UNICODE)


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl(path: Path) -> list[dict]:
    rows = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as error:
            raise SystemExit(f"invalid JSONL in {path}:{number}: {error}") from error
    return rows


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in rows),
        encoding="utf-8",
    )


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def tokens(text: str) -> set[str]:
    return {token.lower() for token in TOKEN_RE.findall(text) if len(token) >= 3}


def profile_inputs() -> tuple[dict, list[dict], dict[str, dict]]:
    profile = read_json(QUALITY / "profiles" / f"{PROFILE}.json")
    queries: list[dict] = []
    for query_file in profile["queries"]:
        queries.extend(read_jsonl(QUALITY / "queries" / f"{query_file}.jsonl"))
    blocks: dict[str, dict] = {}
    for corpus in profile["corpora"]:
        for document in read_jsonl(QUALITY / "corpora" / corpus / "documents.jsonl"):
            document_id = document["document_id"]
            for block in document.get("logical_blocks", document.get("blocks", [])):
                block_id = block["block_id"]
                blocks[block_id] = {
                    "source_block_id": block_id,
                    "document_id": document_id,
                    "heading": block.get("heading", ""),
                    "candidate_text": block.get("text", ""),
                    "access_zone_code": document.get("access_zone_code", ""),
                    "access_level": document.get("access_level", ""),
                    "corpus": document.get("corpus", ""),
                }
    return profile, queries, blocks


def expected_strings(expected: dict, key: str) -> list[str]:
    value = expected.get(key, [])
    return value if isinstance(value, list) else []


def add_candidate(candidates: dict[str, dict], block: dict, reason: str) -> None:
    entry = candidates.setdefault(
        block["source_block_id"],
        {
            "schema_version": 1,
            "profile": PROFILE,
            "query_id": "",
            "document_id": block["document_id"],
            "source_block_id": block["source_block_id"],
            "heading": block["heading"],
            "candidate_text": block["candidate_text"],
            "access_zone_code": block["access_zone_code"],
            "access_level": block["access_level"],
            "corpus": block["corpus"],
            "pool_reasons": [],
        },
    )
    if reason not in entry["pool_reasons"]:
        entry["pool_reasons"].append(reason)


def build_pool(args: argparse.Namespace) -> tuple[list[dict], list[dict], list[dict], dict]:
    _, queries, blocks = profile_inputs()
    all_blocks = list(blocks.values())
    pool_rows: list[dict] = []
    blind_rows: list[dict] = []
    identity_rows: list[dict] = []
    query_manifest: list[dict] = []

    for query in queries:
        query_id = query["id"]
        expected = query.get("expected", {})
        candidates: dict[str, dict] = {}
        for reason, key in (
            ("must_contain_block", "must_contain_block_ids"),
            ("expected_related_block", "expected_related_block_ids"),
            ("forbidden_block", "forbidden_block_ids"),
        ):
            for block_id in expected_strings(expected, key):
                if block_id in blocks:
                    add_candidate(candidates, blocks[block_id], reason)

        referenced_docs = set(
            expected_strings(expected, "must_contain_document_ids")
            + expected_strings(expected, "forbidden_document_ids")
            + expected_strings(expected, "allowed_document_ids")
        )
        for block in all_blocks:
            if block["document_id"] in referenced_docs:
                add_candidate(candidates, block, "referenced_document_block")

        query_tokens = tokens(query.get("question", ""))
        scored = []
        for block in all_blocks:
            overlap = len(query_tokens & tokens(block["candidate_text"] + " " + block["heading"]))
            if overlap:
                scored.append((overlap, block["source_block_id"], block))
        scored.sort(key=lambda item: (-item[0], item[1]))
        for _, _, block in scored[: args.lexical_depth]:
            add_candidate(candidates, block, "lexical_overlap")

        rows = sorted(
            candidates.values(),
            key=lambda item: (min(item["pool_reasons"]), item["source_block_id"]),
        )
        if len(rows) < args.min_candidates:
            raise SystemExit(
                f"{query_id} has only {len(rows)} independent candidates; "
                f"required {args.min_candidates}"
            )
        for row in rows:
            row = dict(row)
            row["query_id"] = query_id
            row["pool_reasons"] = sorted(row["pool_reasons"])
            blind_id = hashlib.sha256(
                f"fix482:{PROFILE}:{query_id}:{row['source_block_id']}".encode("utf-8")
            ).hexdigest()[:20]
            pool_rows.append(row)
            identity_rows.append(
                {
                    "schema_version": 1,
                    "blind_candidate_id": blind_id,
                    "query_id": query_id,
                    "document_id": row["document_id"],
                    "source_block_id": row["source_block_id"],
                }
            )
            blind_rows.append(
                {
                    "schema_version": 1,
                    "profile": PROFILE,
                    "query_id": query_id,
                    "question": query["question"],
                    "blind_candidate_id": blind_id,
                    "heading": row["heading"],
                    "candidate_text": row["candidate_text"],
                    "relevance": None,
                }
            )
        reason_counts = Counter(reason for row in rows for reason in row["pool_reasons"])
        query_manifest.append(
            {
                "query_id": query_id,
                "candidate_count": len(rows),
                "pool_reason_counts": dict(sorted(reason_counts.items())),
            }
        )

    rng = random.Random(f"fix482:{PROFILE}:blind-v1")
    rng.shuffle(blind_rows)
    manifest = {
        "schema_version": 1,
        "profile": PROFILE,
        "status": "AWAITING_BLIND_JUDGMENT",
        "qrels_complete": False,
        "queries_total": len(queries),
        "candidate_pool_total": len(pool_rows),
        "blind_candidates_total": len(blind_rows),
        "judged_candidates_total": 0,
        "unjudged_candidates_total": len(blind_rows),
        "candidate_selection": {
            "runtime_rank_independent": True,
            "uses_production_ranking_output": False,
            "lexical_depth": args.lexical_depth,
            "minimum_candidates_per_query": args.min_candidates,
        },
        "queries": query_manifest,
    }
    return pool_rows, blind_rows, identity_rows, manifest


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-root", type=Path, default=QUALITY / "judgments")
    parser.add_argument("--lexical-depth", type=int, default=12)
    parser.add_argument("--min-candidates", type=int, default=3)
    args = parser.parse_args()

    pool_rows, blind_rows, identity_rows, manifest = build_pool(args)
    pool_path = args.output_root / "candidate-pools" / f"{PROFILE}.jsonl"
    blind_path = args.output_root / "blind-judgments" / f"{PROFILE}.jsonl"
    identity_path = args.output_root / "manifests" / f"{PROFILE}-identity-map.jsonl"
    manifest_path = args.output_root / "manifests" / f"{PROFILE}.json"

    if blind_path.exists():
        existing = read_jsonl(blind_path)
        if any(row.get("relevance") is not None for row in existing):
            raise SystemExit(f"refusing to overwrite adjudicated judgments in {blind_path}")

    write_jsonl(pool_path, pool_rows)
    write_jsonl(blind_path, blind_rows)
    write_jsonl(identity_path, identity_rows)
    manifest.update(
        {
            "candidate_pool_sha256": sha256(pool_path),
            "blind_judgments_sha256": sha256(blind_path),
            "identity_map_sha256": sha256(identity_path),
        }
    )
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(manifest, indent=2, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except OSError as error:
        print(f"fix482 judgment preparation failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
