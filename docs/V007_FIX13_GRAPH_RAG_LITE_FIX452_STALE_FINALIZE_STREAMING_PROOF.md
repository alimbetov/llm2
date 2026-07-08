# V007 fix13 / GraphRAG Lite fix4.5.2

## Finalize Stale-State Safety, Streaming/Bouded-Memory Contract & Integration Proof Patch

This patch is built on top of fix4.5.1 and addresses the remaining production blockers:

- FINALIZING sessions are no longer expired by the normal `expires_at` cleanup path.
- FINALIZING stale detection is controlled by `finalizing_stale_timeout_seconds` and heartbeat fields.
- Finalize completion checks `rows_affected` and returns `INGESTION_FINALIZE_LOST_OWNERSHIP` when ownership was lost.
- Bounded finalize memory guard is non-terminal: it records `last_error_*` and returns the session to ACTIVE instead of marking it FAILED.
- Append validates `batch_content_hash` server-side before storing the batch.
- Finalize validates stored batch hash against staged `block_json`.
- Qdrant collection lifecycle calls use retry-safe request builders.
- `TRUNCATE_LAST_CHUNK` accounts for tokens already consumed by previous chunks.
- Load smoke has a CI-ready Python gRPC stub generation path.
- Migration `0028_v007_fix452_stale_finalize_streaming_integration_proof.sql` adds heartbeat and non-terminal last-error columns.

## Operational notes

`finalize_mode=BOUNDED_IN_MEMORY` remains the default. In this mode AstraVector guarantees bounded failure rather than OOM for documents exceeding `finalize_max_in_memory_blocks`. Such failures are recorded in `last_error_code` and the session remains retryable.

`finalize_mode=TRUE_STREAMING` is reserved and rejected by config validation in fix4.5.2 until the chunking/indexing pipeline is refactored into a true page/section streaming flow.

## Required release checks

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo test --features integration-tests
cargo bench
```
