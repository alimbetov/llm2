# AstraVector v007 fix4.4-rev2 — Load Hardening, Large Document Ingestion & Runtime Resilience

## Scope implemented in this archive

This patch adds the production-hardening skeleton and safe runtime improvements for fix4.4-rev2:

1. Config sections for `ingestion`, `embedding`, `resilience`, `rag_context`, `limits`, and `embedding_cache`.
2. Configurable single-request document size limit.
3. Chunked ingestion proto API and PostgreSQL staging migrations.
4. Basic Start/Append/GetStatus/Abort staging handlers. Finalize is intentionally guarded until full staging JSON rehydration integration tests are enabled.
5. Bounded concurrent `Scheduler::submit_many` for document embeddings.
6. Qdrant retry/backoff wrapper for upsert, dense search, and sparse search.
7. Token-budget diagnostics and a conservative post-MMR drop-lowest-score truncation helper.
8. Additive migrations for durable staging sessions.
9. Updated operational metrics names for ingestion, retry, token budget, and concurrent embedding.

## Compatibility

Existing APIs remain available:

- `CreateMultiGranularityChunks`
- `IndexLogicalDocument`
- `Search`
- `ExplainSearch`
- `DebugDocumentState`

Large single-request documents are rejected with a controlled `OUT_OF_RANGE` message that instructs callers to use chunked ingestion.

## Important limitation

`FinalizeLogicalDocumentIngestion` is scaffolded and currently returns `UNIMPLEMENTED` after validating the session lifecycle. This is deliberate: full block rehydration from staging JSON must be completed together with PostgreSQL testcontainers coverage before production enablement.

## Required local verification

```bash
cargo fmt
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```

## Next mandatory work before production

1. Implement Finalize rehydration from `ingestion_session_blocks_v004` into `IndexLogicalDocumentRequest`.
2. Add testcontainers PostgreSQL tests for Start/Append/Finalize/idempotency/hash mismatch.
3. Add toxiproxy/mock Qdrant tests for retry 503→503→200 and 400 no retry.
4. Add gRPC load tests for 10 concurrent documents and 10,000 small documents.
5. Wire SOURCE chunk `METADATA_ONLY` into chunk persistence behavior.
