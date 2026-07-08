# AstraVector / GraphRAG Lite fix4 — Mathematical Retrieval Optimization Patch

## Scope

This patch applies the consolidated fix4 technical specification on top of `fix3 Reranking & Diversity Safety`.

Implemented optimization areas:

1. Embedding-aware MMR path with token fallback.
2. Top-K heap semantic edge selection instead of full per-source sorting.
3. Direct/graph score calibration.
4. Dedicated Rayon pool support when `semantic_parallelism > 0`.
5. Secondary source metadata preservation during merge deduplication.
6. Strict `FAIL_INDEXING` semantic large-document policy.
7. Explicit `GRAPH_AS_CONTEXT_APPEND` direct/graph budgets.
8. Validation additions for MMR, score normalization and graph append budgets.

## Retrieval changes

### MMR

MMR now prefers `DENSE_EMBEDDING` similarity when each candidate contains an internal normalized embedding in citation metadata under one of:

- `embedding_normalized_json`
- `dense_embedding_normalized_json`

Embeddings are not exposed as a public API contract; they are treated as internal retrieval metadata. If any candidate lacks an embedding, MMR falls back to precomputed token Jaccard and records:

- `graph_mmr_embedding_missing_total`
- `graph_mmr_token_fallback_total`

Candidate metadata includes:

- `mmr_score`
- `mmr_lambda`
- `mmr_max_similarity_to_selected`
- `mmr_similarity_source`

### Score calibration

Direct and graph scores are calibrated before merge/MMR:

```text
Direct: calibrated = raw_score * direct_score_weight
Graph:  calibrated = raw_score * graph_score_weight + graph_score_bias
```

Default config:

```yaml
graph_rag:
  scoring:
    direct_score_weight: 1.0
    graph_score_weight: 0.85
    graph_score_bias: 0.0
    score_normalization: NONE
```

`MIN_MAX` normalization is explicitly reserved for a future patch and is rejected by config validation.

### Merge strategies

`DIRECT_FIRST`:

- Direct candidates have priority.
- Graph candidates fill only remaining final slots.
- Graph candidates do not evict direct candidates.

`GRAPH_AS_CONTEXT_APPEND`:

- Direct candidates are selected up to `direct_context_limit`.
- Graph candidates are selected up to `graph_context_append_limit`.
- Unused direct budget is not reassigned to graph budget.

Duplicate candidates preserve secondary retrieval metadata:

- `retrieval_sources`
- `graph_relations`

## Semantic graph build changes

The semantic edge builder now uses a min-heap of size `semantic_top_k_per_chunk` per source chunk.

Previous complexity:

```text
O(N² log N)
```

New complexity:

```text
O(N² log K)
```

For default `K=3`, this reduces sorting overhead substantially while preserving top-K semantics.

## Parallel execution

Semantic graph build behavior is now explicit:

```text
semantic_parallel_enabled=false:
  sequential build

semantic_parallel_enabled=true, semantic_parallelism=0:
  Rayon global pool

semantic_parallel_enabled=true, semantic_parallelism>0:
  dedicated Rayon ThreadPoolBuilder
```

Metrics:

- `graph_semantic_parallel_mode_total{mode="sequential"}`
- `graph_semantic_parallel_mode_total{mode="global_pool"}`
- `graph_semantic_parallel_mode_total{mode="dedicated_pool"}`
- `graph_semantic_dedicated_rayon_pool_used_total`

## Large document policy

`semantic_large_document_policy=FAIL_INDEXING` now returns an error instead of a warning-only skip.

Supported policies:

- `SKIP_SEMANTIC`
- `STRUCTURAL_ONLY`
- `FAIL_INDEXING`
- `QDRANT_BACKEND` reserved/warning path

## Required local validation

```bash
cargo fmt
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```

This environment did not include Rust toolchain, so compile validation must be performed locally.
