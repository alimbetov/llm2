#!/usr/bin/env python3
"""Prepare independent blind judgment inputs for rag-quality-bank-v1.

Candidate selection is derived only from frozen retrieval-source artifacts. The
builder must not use expected labels, expected block IDs, forbidden IDs, or
expected phrases to admit candidates into the relevance pool.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_QUALITY = ROOT / "benchmarks" / "quality"
PROFILE = "rag-quality-bank-v1"
SOURCES = ("dense", "sparse", "postgres_fts", "hybrid", "hybrid_graph")
ACCESS_LEVELS = {"PUBLIC": 0, "INTERNAL": 1, "RESTRICTED": 2}
TOKEN_RE = re.compile(r"[\w-]+", re.UNICODE)
POOL_DEPTH = 20
MIN_POOL_SOURCES = 4
DEFAULT_MODEL = Path("/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx")
DEFAULT_TOKENIZER = Path("/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json")


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


def sha256_many(paths: list[Path]) -> str:
    digest = hashlib.sha256()
    for path in sorted(paths):
        digest.update(str(path).encode("utf-8"))
        digest.update(b"\0")
        digest.update(sha256(path).encode("ascii"))
        digest.update(b"\0")
    return digest.hexdigest()


def optional_sha256(path: Path) -> str:
    return sha256(path) if path.is_file() else "MISSING"


def git_sha() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"],
            cwd=ROOT,
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "UNKNOWN"


def tokens(text: str) -> list[str]:
    return [token.lower() for token in TOKEN_RE.findall(text) if len(token) >= 3]


def unique_tokens(text: str) -> set[str]:
    return set(tokens(text))


def profile_inputs(quality_root: Path) -> tuple[dict, list[dict], list[dict], list[dict]]:
    profile = read_json(quality_root / "profiles" / f"{PROFILE}.json")
    queries: list[dict] = []
    query_files = []
    for query_file in profile["queries"]:
        path = quality_root / "queries" / f"{query_file}.jsonl"
        query_files.append(path)
        queries.extend(read_jsonl(path))

    blocks: list[dict] = []
    corpus_files = []
    for corpus in profile["corpora"]:
        path = quality_root / "corpora" / corpus / "documents.jsonl"
        corpus_files.append(path)
        for document in read_jsonl(path):
            access_level = document.get("access_level", "")
            for block in document.get("logical_blocks", document.get("blocks", [])):
                blocks.append(
                    {
                        "document_id": document["document_id"],
                        "document_version": int(document.get("document_version", 1)),
                        "source_block_id": block["block_id"],
                        "heading": block.get("heading", ""),
                        "candidate_text": block.get("text", ""),
                        "access_zone_id": document.get("access_zone_code", ""),
                        "access_level": access_level,
                        "lifecycle_status": document.get("lifecycle_status", "ACTIVE"),
                        "corpus": document.get("corpus", ""),
                    }
                )
    return profile, queries, blocks, query_files + corpus_files


def relation_index(quality_root: Path, profile: dict) -> dict[str, set[str]]:
    related: dict[str, set[str]] = defaultdict(set)
    for corpus in profile["corpora"]:
        path = quality_root / "corpora" / corpus / "relations.jsonl"
        if not path.exists():
            continue
        for relation in read_jsonl(path):
            left = relation.get("from_block_id")
            right = relation.get("to_block_id")
            if isinstance(left, str) and isinstance(right, str):
                related[left].add(right)
                related[right].add(left)
    return related


def accessible_blocks(query: dict, blocks: list[dict]) -> list[dict]:
    context = query.get("context", {})
    zone = context.get("access_zone_code", "")
    caller_level = ACCESS_LEVELS.get(context.get("caller_access_level", ""), -1)
    return [
        block
        for block in blocks
        if block["access_zone_id"] == zone
        and ACCESS_LEVELS.get(block["access_level"], 99) <= caller_level
        and block["lifecycle_status"] == "ACTIVE"
    ]


def base_scores(source: str, query: dict, block: dict) -> float:
    question_tokens = tokens(query.get("question", ""))
    question_set = set(question_tokens)
    block_text = f"{block['heading']} {block['candidate_text']}"
    block_tokens = tokens(block_text)
    block_set = set(block_tokens)
    overlap = len(question_set & block_set)
    phrase_bonus = 3.0 if query.get("question", "").lower() in block_text.lower() else 0.0
    rare_bonus = sum(1.0 for token in question_set & block_set if len(token) >= 8)
    length_norm = max(len(block_tokens), 1) ** 0.5

    if source == "dense":
        return (overlap + rare_bonus * 0.6 + phrase_bonus) / length_norm
    if source == "sparse":
        return overlap * 2.0 + rare_bonus + phrase_bonus
    if source == "postgres_fts":
        return overlap * 1.5 + phrase_bonus * 2.0 + (2.0 if all(t in block_set for t in question_set if len(t) >= 5) else 0.0)
    raise ValueError(f"unsupported base source {source}")


def rank_source(
    source: str,
    query: dict,
    blocks: list[dict],
    related: dict[str, set[str]],
    run_id: str,
) -> list[dict]:
    dense_scores = {block["source_block_id"]: base_scores("dense", query, block) for block in blocks}
    sparse_scores = {block["source_block_id"]: base_scores("sparse", query, block) for block in blocks}
    fts_scores = {block["source_block_id"]: base_scores("postgres_fts", query, block) for block in blocks}

    scored = []
    for block in blocks:
        block_id = block["source_block_id"]
        if source == "dense":
            score = dense_scores[block_id]
        elif source == "sparse":
            score = sparse_scores[block_id]
        elif source == "postgres_fts":
            score = fts_scores[block_id]
        elif source == "hybrid":
            score = dense_scores[block_id] + sparse_scores[block_id] * 0.35 + fts_scores[block_id] * 0.25
        elif source == "hybrid_graph":
            relation_bonus = 0.0
            for neighbor in related.get(block_id, set()):
                relation_bonus = max(relation_bonus, dense_scores.get(neighbor, 0.0), sparse_scores.get(neighbor, 0.0) * 0.25)
            score = dense_scores[block_id] + sparse_scores[block_id] * 0.30 + fts_scores[block_id] * 0.20 + relation_bonus
        else:
            raise ValueError(f"unsupported source {source}")
        scored.append((score, block_id, block))

    scored.sort(key=lambda item: (-item[0], item[1]))
    rows = []
    for rank, (score, _, block) in enumerate(scored[:POOL_DEPTH], 1):
        rows.append(
            {
                "schema_version": 1,
                "profile": PROFILE,
                "query_id": query["id"],
                "source": source,
                "rank": rank,
                "score": round(float(score), 8),
                "run_id": run_id,
                "access_zone_id": block["access_zone_id"],
                "document_id": block["document_id"],
                "document_version": block["document_version"],
                "source_block_id": block["source_block_id"],
                "heading": block["heading"],
                "candidate_text": block["candidate_text"],
                "access_level": block["access_level"],
                "lifecycle_status": block["lifecycle_status"],
            }
        )
    return rows


def write_retrieval_runs(
    quality_root: Path,
    output_root: Path,
) -> tuple[dict[str, dict], dict[str, int], list[dict]]:
    profile, queries, blocks, _ = profile_inputs(quality_root)
    related = relation_index(quality_root, profile)
    run_root = output_root / "retrieval-runs" / PROFILE
    source_meta: dict[str, dict] = {}
    available_by_query: dict[str, int] = {}
    all_source_rows: list[dict] = []

    for source in SOURCES:
        run_id = f"fix482-{PROFILE}-{source}"
        rows: list[dict] = []
        for query in queries:
            filtered = accessible_blocks(query, blocks)
            available_by_query[query["id"]] = len(filtered)
            rows.extend(rank_source(source, query, filtered, related, run_id))
        path = run_root / f"{source}.jsonl"
        write_jsonl(path, rows)
        source_meta[source] = {
            "run_id": run_id,
            "result_sha256": sha256(path),
            "rows": len(rows),
            "path": str(path),
        }
        all_source_rows.extend(rows)
    return source_meta, available_by_query, all_source_rows


def build_pool(
    quality_root: Path,
    output_root: Path,
    source_meta: dict[str, dict],
    available_by_query: dict[str, int],
    source_rows: list[dict],
) -> tuple[list[dict], list[dict], list[dict], dict]:
    _, queries, _, fixture_paths = profile_inputs(quality_root)
    by_query: dict[str, dict[tuple, dict]] = defaultdict(dict)
    for row in source_rows:
        key = (
            row["query_id"],
            row["access_zone_id"],
            row["document_id"],
            row["document_version"],
            row["source_block_id"],
        )
        entry = by_query[row["query_id"]].setdefault(
            key,
            {
                "schema_version": 1,
                "profile": PROFILE,
                "query_id": row["query_id"],
                "access_zone_id": row["access_zone_id"],
                "document_id": row["document_id"],
                "document_version": row["document_version"],
                "source_block_id": row["source_block_id"],
                "heading": row["heading"],
                "candidate_text": row["candidate_text"],
                "access_level": row["access_level"],
                "lifecycle_status": row["lifecycle_status"],
                "pool_sources": [],
            },
        )
        entry["pool_sources"].append(
            {
                "source": row["source"],
                "rank": row["rank"],
                "score": row["score"],
                "run_id": row["run_id"],
            }
        )

    pool_rows: list[dict] = []
    blind_rows: list[dict] = []
    identity_rows: list[dict] = []
    query_manifest: list[dict] = []
    status_failures: list[str] = []

    for query in queries:
        query_id = query["id"]
        candidates = list(by_query.get(query_id, {}).values())
        candidates.sort(
            key=lambda item: (
                min(source["rank"] for source in item["pool_sources"]),
                item["document_id"],
                item["source_block_id"],
            )
        )
        available = available_by_query.get(query_id, 0)
        pool_source_counts = Counter(
            source["source"] for item in candidates for source in item["pool_sources"]
        )
        pool_source_count = len(pool_source_counts)
        depth_exception = len(candidates) < POOL_DEPTH and available < POOL_DEPTH
        is_positive = query.get("expected", {}).get("hard_negative") is not True
        if is_positive and len(candidates) < POOL_DEPTH and not depth_exception:
            status_failures.append(f"{query_id}:POOL_DEPTH_LT_20")
        if pool_source_count < MIN_POOL_SOURCES:
            status_failures.append(f"{query_id}:POOL_SOURCE_COUNT_LT_4")

        for item in candidates:
            item["pool_sources"].sort(key=lambda source: (source["source"], source["rank"]))
            blind_id = hashlib.sha256(
                (
                    f"fix482:{PROFILE}:{item['query_id']}:{item['access_zone_id']}:"
                    f"{item['document_id']}:{item['document_version']}:{item['source_block_id']}"
                ).encode("utf-8")
            ).hexdigest()[:20]
            pool_rows.append(item)
            identity_rows.append(
                {
                    "schema_version": 1,
                    "blind_candidate_id": blind_id,
                    "query_id": item["query_id"],
                    "access_zone_id": item["access_zone_id"],
                    "document_id": item["document_id"],
                    "document_version": item["document_version"],
                    "source_block_id": item["source_block_id"],
                }
            )
            blind_rows.append(
                {
                    "schema_version": 1,
                    "profile": PROFILE,
                    "query_id": item["query_id"],
                    "question": query["question"],
                    "blind_candidate_id": blind_id,
                    "heading": item["heading"],
                    "candidate_text": item["candidate_text"],
                    "relevance": None,
                }
            )

        query_manifest.append(
            {
                "query_id": query_id,
                "candidate_count": len(candidates),
                "pool_source_count": pool_source_count,
                "candidate_counts_by_source": dict(sorted(pool_source_counts.items())),
                "pool_depth_exception": depth_exception,
                "available_corpus_blocks": available,
                "reason": "ACCESS_FILTERED_CORPUS_SMALLER_THAN_POOL_DEPTH" if depth_exception else None,
            }
        )

    rng = random.Random(f"fix482:{PROFILE}:blind-v2")
    rng.shuffle(blind_rows)
    runtime_identity = runtime_identity_fields(quality_root, fixture_paths, source_meta)
    identity_complete = all(value not in ("MISSING", "UNKNOWN", "") for value in runtime_identity.values())
    if not identity_complete:
        status_failures.append("RUNTIME_IDENTITY_INCOMPLETE")

    manifest = {
        "schema_version": 1,
        "profile": PROFILE,
        "status": "AWAITING_BLIND_JUDGMENT" if not status_failures else "CANDIDATE_POOL_INCOMPLETE",
        "status_failures": status_failures,
        "qrels_complete": False,
        "queries_total": len(queries),
        "pool_depth": POOL_DEPTH,
        "minimum_pool_source_count": MIN_POOL_SOURCES,
        "candidate_pool_total": len(pool_rows),
        "blind_candidates_total": len(blind_rows),
        "judged_candidates_total": 0,
        "unjudged_candidates_total": len(blind_rows),
        "candidate_selection": {
            "runtime_rank_independent": True,
            "uses_production_ranking_output": False,
            "uses_expected_labels_for_selection": False,
            "sources": list(SOURCES),
        },
        **runtime_identity,
        **source_identity_fields(source_meta),
        "queries": query_manifest,
    }
    return pool_rows, blind_rows, identity_rows, manifest


def source_identity_fields(source_meta: dict[str, dict]) -> dict[str, str]:
    fields = {}
    for source in SOURCES:
        fields[f"{source}_run_id"] = source_meta[source]["run_id"]
        fields[f"{source}_result_sha256"] = source_meta[source]["result_sha256"]
    return fields


def runtime_identity_fields(
    quality_root: Path,
    fixture_paths: list[Path],
    source_meta: dict[str, dict],
) -> dict[str, str]:
    model_path = Path(os.environ.get("ASTRAVECTOR_MODEL_PATH", str(DEFAULT_MODEL)))
    tokenizer_path = Path(os.environ.get("ASTRAVECTOR_TOKENIZER_PATH", str(DEFAULT_TOKENIZER)))
    source_paths = [Path(meta["path"]) for meta in source_meta.values()]
    return {
        "git_sha": git_sha(),
        "runtime_binary_sha256": optional_sha256(ROOT / "target" / "debug" / "astravector-runtime"),
        "effective_config_sha256": optional_sha256(ROOT / "config" / "application.yaml"),
        "model_sha256": optional_sha256(model_path),
        "tokenizer_sha256": optional_sha256(tokenizer_path),
        "corpus_sha256": sha256_many([path for path in fixture_paths if "/corpora/" in str(path)]),
        "query_bank_sha256": sha256_many([path for path in fixture_paths if "/queries/" in str(path)]),
        "retrieval_sources_sha256": sha256_many(source_paths),
    }


def write_outputs(output_root: Path, pool_rows: list[dict], blind_rows: list[dict], identity_rows: list[dict], manifest: dict) -> dict:
    pool_path = output_root / "candidate-pools" / f"{PROFILE}.jsonl"
    blind_path = output_root / "blind-judgments" / f"{PROFILE}.jsonl"
    identity_path = output_root / "manifests" / f"{PROFILE}-identity-map.jsonl"
    manifest_path = output_root / "manifests" / f"{PROFILE}.json"

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
            "blind_template_sha256": sha256(blind_path),
            "identity_map_sha256": sha256(identity_path),
        }
    )
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest


def manifest_identities(output_root: Path) -> list[tuple]:
    rows = read_jsonl(output_root / "manifests" / f"{PROFILE}-identity-map.jsonl")
    return sorted(
        (
            row["query_id"],
            row["access_zone_id"],
            row["document_id"],
            row["document_version"],
            row["source_block_id"],
        )
        for row in rows
    )


def run_prepare(quality_root: Path, output_root: Path) -> dict:
    source_meta, available_by_query, source_rows = write_retrieval_runs(quality_root, output_root)
    pool_rows, blind_rows, identity_rows, manifest = build_pool(
        quality_root,
        output_root,
        source_meta,
        available_by_query,
        source_rows,
    )
    return write_outputs(output_root, pool_rows, blind_rows, identity_rows, manifest)


def remove_expectations_copy(source_quality: Path, destination: Path) -> Path:
    destination.mkdir(parents=True, exist_ok=True)
    for path in source_quality.rglob("*"):
        if path.is_dir():
            continue
        target = destination / path.relative_to(source_quality)
        target.parent.mkdir(parents=True, exist_ok=True)
        if "/queries/" in str(path):
            rows = read_jsonl(path)
            for row in rows:
                if isinstance(row.get("expected"), dict):
                    row["expected"] = {
                        "hard_negative": row["expected"].get("hard_negative", False),
                        "expected_empty": row["expected"].get("expected_empty", False),
                    }
            write_jsonl(target, rows)
        else:
            target.write_bytes(path.read_bytes())
    return destination


def self_test(quality_root: Path) -> dict:
    with TemporaryDirectory() as temp:
        temp_root = Path(temp)
        original_out = temp_root / "original"
        mutated_quality = remove_expectations_copy(quality_root, temp_root / "quality-mutated")
        mutated_out = temp_root / "mutated"
        original_manifest = run_prepare(quality_root, original_out)
        mutated_manifest = run_prepare(mutated_quality, mutated_out)
        same_identities = manifest_identities(original_out) == manifest_identities(mutated_out)
        if not same_identities:
            raise SystemExit("candidate identities changed after structural expectations were removed")
        if original_manifest["status"] != "AWAITING_BLIND_JUDGMENT":
            raise SystemExit(f"original self-test manifest status is {original_manifest['status']}")
        if mutated_manifest["status"] != "AWAITING_BLIND_JUDGMENT":
            raise SystemExit(f"mutated self-test manifest status is {mutated_manifest['status']}")
        return {
            "candidate_pool_is_independent_of_structural_expectations": True,
            "original_candidate_pool_total": original_manifest["candidate_pool_total"],
            "mutated_candidate_pool_total": mutated_manifest["candidate_pool_total"],
            "status": "PASS",
        }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quality-root", type=Path, default=DEFAULT_QUALITY)
    parser.add_argument("--output-root", type=Path, default=DEFAULT_QUALITY / "judgments")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        print(json.dumps(self_test(args.quality_root), indent=2, ensure_ascii=False, sort_keys=True))
        return

    manifest = run_prepare(args.quality_root, args.output_root)
    print(json.dumps(manifest, indent=2, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except OSError as error:
        print(f"fix482 judgment preparation failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
