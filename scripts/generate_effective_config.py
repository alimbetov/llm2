#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path


def integer(name: str, default: int) -> int:
    return int(os.environ.get(name, default))


def number(name: str, default: float) -> float:
    return float(os.environ.get(name, default))


def boolean(name: str, default: bool) -> bool:
    value = os.environ.get(name)
    return default if value is None else value.lower() in {"1", "true", "yes", "on"}


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
            "dense_weight": number("ASTRAVECTOR_HYBRID_DENSE_WEIGHT", 0.6),
            "sparse_weight": number("ASTRAVECTOR_HYBRID_SPARSE_WEIGHT", 0.4),
            "min_strong_lexical_candidates": integer(
                "ASTRAVECTOR_FUSION_MIN_STRONG_LEXICAL_CANDIDATES", 1
            ),
            "lexical": {
                "candidate_limit": integer("ASTRAVECTOR_LEXICAL_SEARCH_CANDIDATE_LIMIT", 50),
                "max_candidate_limit": integer("ASTRAVECTOR_LEXICAL_SEARCH_MAX_CANDIDATE_LIMIT", 100),
                "min_remaining_budget_ms": integer("ASTRAVECTOR_LEXICAL_SEARCH_MIN_REMAINING_BUDGET_MS", 150),
                "statement_timeout_ms": integer("ASTRAVECTOR_LEXICAL_SEARCH_STATEMENT_TIMEOUT_MS", 150),
                "weight": number("ASTRAVECTOR_LEXICAL_SEARCH_RRF_WEIGHT", 0.2),
            },
            "ranking_trace": {
                "enabled": boolean("ASTRAVECTOR_RANKING_TRACE_ENABLED", False),
                "max_candidates": integer("ASTRAVECTOR_RANKING_TRACE_MAX_CANDIDATES", 100),
                "max_stages_per_candidate": integer("ASTRAVECTOR_RANKING_TRACE_MAX_STAGES", 32),
                "include_text_preview": False,
            },
            "no_answer": {
                "enabled": boolean("ASTRAVECTOR_RETRIEVAL_NO_ANSWER_ENABLED", True),
                "min_dense_score": number("ASTRAVECTOR_RETRIEVAL_NO_ANSWER_MIN_DENSE_SCORE", 0.25),
                "min_sparse_score": number("ASTRAVECTOR_RETRIEVAL_NO_ANSWER_MIN_SPARSE_SCORE", 0.10),
                "min_hybrid_score": number("ASTRAVECTOR_RETRIEVAL_NO_ANSWER_MIN_HYBRID_SCORE", 0.30),
            },
        },
        "graph_rag": {
            "max_seed_chunks": integer("ASTRAVECTOR_GRAPH_MAX_SEED_CHUNKS", 5),
            "max_related_chunks": integer("ASTRAVECTOR_GRAPH_MAX_RELATED_CHUNKS", 3),
            "graph_min_score": number("ASTRAVECTOR_GRAPH_MIN_SCORE", 0.05),
            "direct_score_weight": number("ASTRAVECTOR_GRAPH_DIRECT_SCORE_WEIGHT", 1.0),
            "graph_score_weight": number("ASTRAVECTOR_GRAPH_GRAPH_SCORE_WEIGHT", 0.85),
            "min_direct_contexts": integer("ASTRAVECTOR_GRAPH_MIN_DIRECT_CONTEXTS", 1),
            "max_graph_fraction": number("ASTRAVECTOR_GRAPH_MAX_GRAPH_FRACTION", 0.5),
            "mmr_enabled": boolean("ASTRAVECTOR_GRAPH_MMR_ENABLED", True),
            "mmr_lambda": number("ASTRAVECTOR_GRAPH_MMR_LAMBDA", 0.75),
            "mmr_lambda_direct": number("ASTRAVECTOR_GRAPH_MMR_LAMBDA_DIRECT", 0.80),
            "mmr_lambda_graph": number("ASTRAVECTOR_GRAPH_MMR_LAMBDA_GRAPH", 0.65),
        },
        "rag_context": {
            "max_context_tokens": integer("ASTRAVECTOR_RAG_MAX_CONTEXT_TOKENS", 6000),
            "reserved_answer_tokens": integer("ASTRAVECTOR_RAG_RESERVED_ANSWER_TOKENS", 1000),
            "min_direct_token_fraction": number("ASTRAVECTOR_RAG_MIN_DIRECT_TOKEN_FRACTION", 0.5),
            "max_graph_token_fraction": number("ASTRAVECTOR_RAG_MAX_GRAPH_TOKEN_FRACTION", 0.4),
        },
        "model": {"sha256": file_sha(args.model)},
        "tokenizer": {"sha256": file_sha(args.tokenizer)},
    }
    canonical = json.dumps(config, sort_keys=True, separators=(",", ":"))
    digest = hashlib.sha256(canonical.encode()).hexdigest()
    config["effective_config_sha256"] = digest
    atomic_write(args.output, json.dumps(config, indent=2, sort_keys=True) + "\n")
    atomic_write(args.output.with_suffix(".sha256"), digest + "\n")
    print(digest)


if __name__ == "__main__":
    main()
