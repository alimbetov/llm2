# V007 Fix11 / GraphRAG Lite fix4.5 — Chunked Finalize & Resilience Completion Patch

This patch continues fix4.4-rev2 and addresses the audit findings that prevented production-candidate approval.

## Implemented in this archive

- `FinalizeLogicalDocumentIngestion` no longer returns `UNIMPLEMENTED`; it rehydrates staged logical blocks, validates the final content hash, invokes the normal `IndexLogicalDocument` flow, stores a replayable response, and marks the session `COMPLETED`.
- `StartLogicalDocumentIngestion` now stores and validates a request fingerprint so an `idempotency_key` cannot be reused for a different document payload.
- `AppendLogicalDocumentBlocks` now uses `ingestion_session_batches_v004`, validates `batch_content_hash`, enforces batch byte/block limits, and updates counters only when a batch is actually inserted.
- A staging cleanup worker (`src/ingestion_cleanup.rs`) expires old active/finalizing sessions and deletes completed/aborted/expired staging rows according to retention settings.
- `Scheduler::submit_many` now cancels early on the first error when `cancel_on_error=true` instead of waiting for all futures to complete.
- Inference batch retry/backoff is applied around `engine.encode_batch(inputs)` using `resilience.inference_retry`.
- Qdrant retry uses `resilience.qdrant_retry` instead of hardcoded constants and the retry wrapper is reused for retry-safe point/count/scroll paths.
- `SOURCE` chunk storage mode is implemented in `ChunkingEngine`: `FULL_TEXT`, `METADATA_ONLY`, and `DISABLED`.
- Search and graph runtime limits now use `limits.search_top_k_max`, `limits.search_candidate_limit_max`, and `limits.graph_related_contexts_max`.
- Token-budget truncation now applies the configured strategy, safety margin, and huge-chunk handling.
- Added migration `0026_v007_fix45_chunked_finalize_resilience_completion.sql`.
- Added runnable/skip-safe pytest load-smoke scaffold under `tests/load/`.

## Required local validation

This environment does not contain Rust toolchain. Before production approval run:

```bash
cargo fmt
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```

Also run PostgreSQL/testcontainers integration tests for chunked ingestion finalize, append idempotency, retry, token-budget truncation, and SOURCE chunk storage modes.

## Remaining production gates

- Confirm SQLx types against the real migrated PostgreSQL schema.
- Confirm prost regeneration after proto changes from previous fixes.
- Confirm 10 MiB chunked ingestion and concurrent document indexing load smoke.
- Confirm search p95 and indexing p95 SLO under the target cluster profile.
