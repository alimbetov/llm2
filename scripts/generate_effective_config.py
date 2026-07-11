#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path


def integer(name: str, default: int) -> int:
    return int(os.environ.get(name, default))


def file_sha(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def atomic_write(path: Path, body: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(body, encoding="utf-8")
    temporary.replace(path)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", required=True)
    parser.add_argument("--tokenizer", required=True)
    args = parser.parse_args()
    config = {
        "schema_version": "1.0",
        "profile": os.environ.get("ASTRAVECTOR_PROFILE", "load-m2"),
        "grpc": {"query_deadline_ms": integer("ASTRAVECTOR_QUERY_DEADLINE_MS", 1000)},
        "batching": {
            "query_queue_capacity": integer("ASTRAVECTOR_QUERY_QUEUE_CAPACITY", 32),
            "max_queue_age_ms": integer("ASTRAVECTOR_QUERY_MAX_QUEUE_AGE_MS", 250),
            "min_inference_budget_ms": integer("ASTRAVECTOR_QUERY_MIN_INFERENCE_BUDGET_MS", 250),
            "max_deadline_skew_ms": integer("ASTRAVECTOR_QUERY_MAX_DEADLINE_SKEW_MS", 250),
        },
        "limits": {
            "retrieve_context": integer("ASTRAVECTOR_MAX_CONCURRENT_RETRIEVE_CONTEXT", 8),
            "qdrant_search": integer("ASTRAVECTOR_MAX_CONCURRENT_QDRANT_SEARCH", 16),
            "graph_expansion": integer("ASTRAVECTOR_MAX_CONCURRENT_GRAPH_EXPANSION", 4),
            "mmr_fetch": integer("ASTRAVECTOR_MAX_CONCURRENT_MMR_FETCH", 8),
            "admission_timeout_ms": integer("ASTRAVECTOR_BACKPRESSURE_ACQUIRE_TIMEOUT_MS", 20),
        },
        "search": {
            "candidate_limit": integer("ASTRAVECTOR_SEARCH_CANDIDATE_LIMIT", 50),
            "parent_limit": integer("ASTRAVECTOR_SEARCH_PARENT_LIMIT", 5),
            "fusion_method": os.environ.get("ASTRAVECTOR_HYBRID_FUSION_METHOD", "RRF"),
            "rrf_k": float(os.environ.get("ASTRAVECTOR_SEARCH_RRF_K", 60)),
        },
        "model": {"sha256": file_sha(args.model)},
        "tokenizer": {"sha256": file_sha(args.tokenizer)},
        "environment_overrides": {
            key: value for key, value in sorted(os.environ.items()) if key.startswith("ASTRAVECTOR_")
        },
    }
    canonical = json.dumps(config, sort_keys=True, separators=(",", ":"))
    digest = hashlib.sha256(canonical.encode()).hexdigest()
    config["effective_config_sha256"] = digest
    atomic_write(args.output, json.dumps(config, indent=2, sort_keys=True) + "\n")
    atomic_write(args.output.with_suffix(".sha256"), digest + "\n")
    print(digest)


if __name__ == "__main__":
    main()
