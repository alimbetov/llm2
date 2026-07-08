# V007 fix7 / GraphRAG Lite fix4.1
# End-to-End MMR & Strategy Semantics Patch

## Purpose

This patch hardens the previous fix4 mathematical optimization by making embedding-based MMR operational end-to-end and by preserving merge strategy semantics.

## Added

- Batch PostgreSQL fetch for dense embeddings by candidate chunk ids.
- Optional in-memory embedding cache keyed by `(access_zone_id, chunk_id)` with TTL and max entries.
- Safe token fallback on embedding fetch error, timeout, missing vector, or unavailable repository.
- Strategy-aware MMR:
  - `SCORE_THEN_TRUNCATE`: global MMR.
  - `DIRECT_FIRST`: direct group MMR first, graph fills remaining slots only.
  - `GRAPH_AS_CONTEXT_APPEND`: separate direct and graph MMR budgets.
- Group-specific MMR lambdas:
  - `mmr_lambda_direct` default `0.80`.
  - `mmr_lambda_graph` default `0.60`.
- Strict `FAIL_INDEXING`: semantic large-document fail policy now overrides `WARN_AND_CONTINUE`.
- JSON explainability metadata for `retrieval_sources` and `graph_relations`.
- Extra diagnostics for score calibration and embedding fetch.
- Additional metrics for embedding fetch, cache, candidate truncation, and group MMR.

## Important behavior

Embeddings are attached to candidate metadata only internally for MMR and are stripped before the gRPC search response is returned.

If embeddings cannot be fetched, search does not fail. MMR falls back to token Jaccard and emits metrics/logs.

## Required local validation

```bash
cargo fmt
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```

This archive was prepared in an environment without Rust toolchain, so compiler validation remains mandatory.
