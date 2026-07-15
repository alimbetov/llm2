#!/usr/bin/env python3
"""Prepare and finalize rank-blind fix481 relevance judgments."""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import sys
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
QUALITY = ROOT / "benchmarks" / "quality"
POOL_STAGES = {
    "dense": ("DENSE_RETRIEVAL",),
    "sparse": ("SPARSE_RETRIEVAL",),
    "postgres_fts": ("LEXICAL_RETRIEVAL",),
    "hybrid": ("FUSION_ADMISSION",),
    "hybrid_graph": ("GRAPH_MERGE",),
}


def read_jsonl(path: Path) -> list[dict]:
    values = []
    with path.open("r", encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            if line.strip():
                try:
                    values.append(json.loads(line))
                except json.JSONDecodeError as error:
                    raise SystemExit(f"invalid JSON at {path}:{number}: {error}") from error
    return values


def write_jsonl(path: Path, values: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for value in values:
            handle.write(json.dumps(value, ensure_ascii=False, sort_keys=True) + "\n")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def profile_inputs(profile_name: str) -> tuple[dict, list[dict], dict[str, dict]]:
    profile_path = QUALITY / "profiles" / f"{profile_name}.json"
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    queries = []
    for name in profile["queries"]:
        queries.extend(read_jsonl(QUALITY / "queries" / f"{name}.jsonl"))
    blocks = {}
    for corpus in profile["corpora"]:
        for document in read_jsonl(QUALITY / "corpora" / corpus / "documents.jsonl"):
            document_id = document.get("document_id", "")
            for block in document.get("logical_blocks", document.get("blocks", [])):
                block_id = block.get("block_id")
                if block_id:
                    blocks[block_id] = {
                        "document_id": document_id,
                        "heading": block.get("heading", ""),
                        "text": block.get("text", ""),
                    }
    return profile, queries, blocks


def prepare(args: argparse.Namespace) -> None:
    profile, queries, blocks = profile_inputs(args.profile)
    traces = args.report_dir / "ranking-traces"
    missing = [
        f"{source}/{query['id']}"
        for source in POOL_STAGES
        for query in queries
        if not (traces / source / f"{query['id']}.json").is_file()
    ]
    if missing:
        raise SystemExit("ranking traces missing for: " + ", ".join(missing))

    pools = []
    blind_rows = []
    identities = []
    manifest_queries = []
    for query in queries:
        query_id = query["id"]
        candidates = {}
        source_counts = defaultdict(int)
        for source, accepted_stages in POOL_STAGES.items():
            trace = json.loads(
                (traces / source / f"{query_id}.json").read_text(encoding="utf-8")
            )
            if trace.get("truncated"):
                raise SystemExit(f"trace is truncated for {source}/{query_id}")
            for candidate in trace.get("candidates", []):
                identity = candidate.get("identity") or {}
                block_id = identity.get("source_block_id")
                if not block_id or block_id not in blocks:
                    continue
                ranks = [
                    int(stage.get("rank") or 0)
                    for stage in candidate.get("stages", [])
                    if stage.get("stage") in accepted_stages and stage.get("present")
                ]
                ranks = [rank for rank in ranks if 0 < rank <= args.depth]
                if not ranks:
                    continue
                entry = candidates.setdefault(
                    block_id,
                    {
                        "query_id": query_id,
                        "source_block_id": block_id,
                        "document_id": blocks[block_id]["document_id"],
                        "heading": blocks[block_id]["heading"],
                        "candidate_text": blocks[block_id]["text"],
                        "pool_sources": set(),
                        "source_ranks": {},
                    },
                )
                entry["pool_sources"].add(source)
                entry["source_ranks"][source] = min(ranks)
        pooled = [entry for entry in candidates.values() if entry["pool_sources"]]
        pooled.sort(key=lambda item: (min(item["source_ranks"].values()), item["source_block_id"]))
        sources = sorted({source for item in pooled for source in item["pool_sources"]})
        for source in sources:
            source_counts[source] = sum(source in item["pool_sources"] for item in pooled)
        for item in pooled:
            item["pool_sources"] = sorted(item["pool_sources"])
            blind_id = hashlib.sha256(
                f"fix481:{args.profile}:{query_id}:{item['source_block_id']}".encode()
            ).hexdigest()[:20]
            pools.append(item)
            identities.append(
                {
                    "blind_candidate_id": blind_id,
                    "query_id": query_id,
                    "document_id": item["document_id"],
                    "source_block_id": item["source_block_id"],
                }
            )
            blind_rows.append(
                {
                    "schema_version": 1,
                    "query_id": query_id,
                    "question": query["question"],
                    "blind_candidate_id": blind_id,
                    "heading": item["heading"],
                    "candidate_text": item["candidate_text"],
                    "relevance": None,
                }
            )
        manifest_queries.append(
            {
                "query_id": query_id,
                "requested_pool_depth": args.depth,
                "candidate_count": len(pooled),
                "pool_source_count": len(sources),
                "pool_sources": sources,
                "source_candidate_counts": dict(sorted(source_counts.items())),
            }
        )

    rng = random.Random(f"fix481:{args.profile}:blind-v1")
    rng.shuffle(blind_rows)
    pool_path = args.output_root / "candidate-pools" / f"{args.profile}.jsonl"
    blind_path = args.output_root / "blind-judgments" / f"{args.profile}.jsonl"
    identity_path = args.output_root / "manifests" / f"{args.profile}-identity-map.jsonl"
    manifest_path = args.output_root / "manifests" / f"{args.profile}.json"
    write_jsonl(pool_path, pools)
    write_jsonl(identity_path, identities)
    if blind_path.exists() and any(row.get("relevance") is not None for row in read_jsonl(blind_path)):
        raise SystemExit(f"refusing to overwrite judgments in {blind_path}")
    write_jsonl(blind_path, blind_rows)
    manifest = {
        "schema_version": 1,
        "profile": args.profile,
        "status": "AWAITING_BLIND_JUDGMENT",
        "qrels_complete": False,
        "requested_pool_depth": args.depth,
        "minimum_pool_source_count": args.min_sources,
        "queries_total": len(queries),
        "judged_candidates_total": 0,
        "unjudged_candidates_total": len(blind_rows),
        "queries": manifest_queries,
        "candidate_pool_sha256": sha256(pool_path),
        "blind_judgments_sha256": sha256(blind_path),
        "identity_map_sha256": sha256(identity_path),
        "profile_sha256": sha256(QUALITY / "profiles" / f"{args.profile}.json"),
    }
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2, sort_keys=True))


def finalize(args: argparse.Namespace) -> None:
    blind_path = args.output_root / "blind-judgments" / f"{args.profile}.jsonl"
    identity_path = args.output_root / "manifests" / f"{args.profile}-identity-map.jsonl"
    manifest_path = args.output_root / "manifests" / f"{args.profile}.json"
    blind = read_jsonl(blind_path)
    identities = {row["blind_candidate_id"]: row for row in read_jsonl(identity_path)}
    adjudicated = []
    missing = []
    for row in blind:
        relevance = row.get("relevance")
        blind_id = row.get("blind_candidate_id")
        if type(relevance) is not int or relevance not in range(4):
            missing.append(blind_id)
            continue
        identity = identities.get(blind_id)
        if identity is None or identity["query_id"] != row.get("query_id"):
            raise SystemExit(f"identity mismatch for blind candidate {blind_id}")
        adjudicated.append(
            {
                "schema_version": 1,
                "query_id": identity["query_id"],
                "document_id": identity["document_id"],
                "source_block_id": identity["source_block_id"],
                "relevance": relevance,
                "judgment_status": "ADJUDICATED",
            }
        )
    if missing:
        raise SystemExit(f"{len(missing)} candidates remain unjudged")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    shallow = [
        item["query_id"]
        for item in manifest["queries"]
        if item["requested_pool_depth"] < manifest["requested_pool_depth"]
        or item["pool_source_count"] < manifest["minimum_pool_source_count"]
    ]
    if shallow:
        raise SystemExit("pool contract incomplete for: " + ", ".join(shallow))
    output = args.output_root / "adjudicated" / f"{args.profile}.jsonl"
    write_jsonl(output, adjudicated)
    manifest.update(
        {
            "status": "ADJUDICATED",
            "qrels_complete": True,
            "judged_candidates_total": len(adjudicated),
            "unjudged_candidates_total": 0,
            "adjudicated_qrels_sha256": sha256(output),
            "blind_judgments_sha256": sha256(blind_path),
        }
    )
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2, sort_keys=True))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("prepare", "finalize"))
    parser.add_argument("--profile", required=True)
    parser.add_argument("--report-dir", type=Path)
    parser.add_argument(
        "--output-root", type=Path, default=QUALITY / "judgments"
    )
    parser.add_argument("--depth", type=int, default=20)
    parser.add_argument("--min-sources", type=int, default=4)
    args = parser.parse_args()
    if args.command == "prepare":
        if args.report_dir is None:
            parser.error("prepare requires --report-dir")
        prepare(args)
    else:
        finalize(args)


if __name__ == "__main__":
    try:
        main()
    except OSError as error:
        print(f"judgment pool I/O failure: {error}", file=sys.stderr)
        raise SystemExit(1) from error
