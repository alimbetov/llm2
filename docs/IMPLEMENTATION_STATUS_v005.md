# AstraVector v005 implementation status

This branch is an incremental implementation over `AstraVector_v004_dense_sparse_hybrid_patch` according to `TECHNICAL_SPEC_v005.md`.

## Implemented in this patch

- Proto foundation:
  - `EmbeddingModeV005`
  - `SearchModeV005`
  - `PublishModeV005`
  - `PreviewEmbedding`
  - `ExplainSearch`
  - `DebugDocumentState`
  - `RetryVectorOutbox`
  - `matched_text` in `SearchResultV004`
  - `force_activate` / `force_reason` in `ActivateDocumentVersionRequest`
- Runtime:
  - `PreviewEmbedding` endpoint with dense preview, sparse preview and tokenization preview.
  - strict sparse precondition for `DENSE_SPARSE_REQUIRED`.
- Index:
  - `CreateMultiGranularityChunks` accepts `embedding_mode` and `publish_mode` fields.
  - sparse-required mode fails fast if sparse output is unavailable.
  - response includes `IndexSummaryV005`.
- Publisher/Qdrant:
  - publisher ensures dense+sparse Qdrant collection before upsert.
  - Qdrant collection validation checks dense dimension and sparse vector presence.
  - Qdrant payload includes model/tokenizer/dense/sparse/chunking versions.
- Search:
  - explicit `SearchModeV005`: DENSE / SPARSE / HYBRID.
  - HYBRID keeps dense+sparse RRF fusion.
  - `SearchResultV004.matched_text` is loaded from PostgreSQL by matched chunk id.
- Explain:
  - `ExplainSearch` returns query embedding summary, top sparse tokens, dense candidates, sparse candidates, fusion ranking and applied filters.
- Debug:
  - `DebugDocumentState` returns document/chunk/vector/outbox/Qdrant status summary without manual SQL.
- Operations:
  - `RetryVectorOutbox` can reset FAILED/RETRY_PENDING outbox rows, optionally filtered by operation/status.
  - `force_activate` path exists with required `force_reason` and minimum data checks.

## Known gaps to validate before production

- The archive was patched in a sandbox without local Rust toolchain, so `cargo check` was not executed here.
- `DebugDocumentState.qdrant.points_found` is initialized conservatively; full Qdrant scroll/count reconciliation should be completed in a follow-up if strict point diff is required.
- Search version filters are present in proto but require additional Qdrant filter wiring for full model-version isolation.
- Admin authorization for Debug/Explain/Retry/ForceActivate depends on the existing API-key/interceptor layer and should be hardened according to deployment policy.

## Required local validation

```bash
cd /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004
cargo fmt
cargo check --all-features
cargo test --all-features
```

Then run the v005 E2E smoke defined in `docs/TECHNICAL_SPEC_v005.md`.
