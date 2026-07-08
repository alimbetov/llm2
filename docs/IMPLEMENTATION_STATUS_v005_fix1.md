# AstraVector v005-hardening-statistical-fix1

## Scope

This patch applies targeted hardening fixes after the audit of `AstraVector_v005_hardening_statistical.zip`.

## Implemented changes

- Declared and consistently used `EmbeddingModeV005` sparse flags in `create_multi_granularity_chunks`.
- Added strict sparse precheck before any PostgreSQL write for `DENSE_SPARSE_REQUIRED`.
- Reworked v004 indexing path to precompute all searchable chunk embeddings before persistence.
- Added `persist_v004_index_transactionally(...)` to persist chunks, cache entries, dense/sparse vectors, bindings and optional outbox in one PostgreSQL transaction.
- Kept `PublishMode.NONE` outbox-free inside the transaction.
- Extended sync status proto with `qdrant_points_missing` and `qdrant_points_extra`.
- Replaced count-only ready logic with exact expected-vs-actual Qdrant point id reconciliation.
- Treats `DEAD_LETTER` as failed outbox status in sync/debug status and default retry.
- Added version filters to `ExplainSearch`.
- Added `warnings` to `SearchResponseV004` and returns `SPARSE_UNAVAILABLE_DENSE_FALLBACK` for allowed IF_AVAILABLE fallback.
- Added metric increments for sparse unavailable, dense fallback warning, and sync mismatch.

## Required local verification

This environment does not contain Rust toolchain, PostgreSQL, Qdrant or ONNX Runtime. Run locally:

```bash
cargo fmt
cargo check --all-features
cargo test --all-features
```

Then run full smoke with PostgreSQL + Qdrant + ONNX + publisher.

## Known remaining verification risk

The patch is static-code generated in an environment without `cargo check`; generated prost enum/field names must be verified locally.
