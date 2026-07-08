# AstraVector / GraphRAG Lite fix3 — Reranking & Diversity Safety

## Scope

This patch adds the fix3 safety layer on top of GraphRAG Lite Balanced Mode:

1. MMR-style diversity reranking after direct+graph merge.
2. Default in-memory semantic graph chunk limit reduced to 500.
3. Large document semantic policy: `SKIP_SEMANTIC`.
4. Reserved learned reranker config with runtime disabled.
5. MMR diagnostics and metrics.

## Retrieval pipeline

```text
direct retrieval
+ graph expansion
-> merge by graph_merge_strategy
-> MMR diversity selection
-> final_context_limit
```

## MMR

Config:

```yaml
graph_rag:
  rerank:
    mmr_enabled: true
    mmr_lambda: 0.75
    mmr_candidate_limit: 30
    mmr_similarity_source: TEXT_JACCARD_FALLBACK
```

The current implementation uses a safe text-token Jaccard fallback for diversity scoring because `SearchResultV004` does not carry dense vectors. Dense-embedding MMR remains the target implementation once candidate embeddings are available in the retrieval candidate layer.

## Large document policy

Default:

```yaml
graph_rag:
  build:
    semantic_max_chunks_for_in_memory: 500
    semantic_large_document_policy: SKIP_SEMANTIC
```

When a document has more than 500 semantic candidate chunks:

- structural graph is still built;
- semantic graph build is skipped;
- indexing continues;
- warning `SEMANTIC_GRAPH_SKIPPED_TOO_MANY_CHUNKS` is added;
- metric `graph_semantic_documents_skipped_large_total` is incremented.

## Learned reranker

Reserved config only:

```yaml
graph_rag:
  rerank:
    learned_reranker_enabled: false
    learned_reranker_provider: NONE
```

If enabled in fix3, config validation fails intentionally.

## Metrics

- `graph_mmr_enabled_total`
- `graph_mmr_disabled_total`
- `graph_mmr_candidates_total`
- `graph_mmr_selected_total`
- `graph_mmr_duration_ms`
- `graph_semantic_documents_skipped_large_total`

## Diagnostics

`SearchDiagnosticsV004` includes:

- `mmr_enabled`
- `mmr_lambda`
- `mmr_candidate_count`
- `mmr_selected_count`
- `mmr_duration_ms`
- `mmr_similarity_source`
- `learned_reranker_enabled`
- `learned_reranker_provider`

## Local verification

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```
