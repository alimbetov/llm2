# AstraVector v007 fix12 / GraphRAG Lite fix4.5.1
# Finalize Concurrency, Streaming Memory Safety & Testcontainers Proof Patch

## Scope

This patch is a stabilization patch on top of fix4.5. It targets the remaining production blockers found in the fix4.5 audit:

- race-prone `FinalizeLogicalDocumentIngestion`;
- unsafe failure path leaving sessions in `FINALIZING`;
- full `fetch_all` staging reads during Finalize;
- Start idempotency replay being blocked by session limits;
- cleanup deleting completed `result_response_json` too early;
- Append replay being blocked by document block limits;
- Abort success for missing sessions;
- Qdrant 4xx errors mapped as `Unavailable`;
- missing `DROPPED_CHUNK_IDS_TRUNCATED` diagnostics;
- skip-only load smoke tests.

## Main changes

### Finalize ownership

`FinalizeLogicalDocumentIngestion` now uses atomic ownership acquisition:

```sql
UPDATE astravector.ingestion_sessions_v004
SET status='FINALIZING', updated_at=now()
WHERE ingestion_session_id=$1
  AND status='ACTIVE'
  AND expires_at >= now()
RETURNING ...;
```

Only a caller that receives the returned row may run indexing. Existing `COMPLETED`, `FINALIZING`, `FAILED`, `ABORTED` and `EXPIRED` states are handled explicitly.

### Finalize failure path

If the internal indexing call fails, the session is marked as:

```text
status = FAILED
error_code = INDEXING_FAILED
error_message = <controlled status message>
```

The session is no longer left in `FINALIZING` after an indexing error.

### Bounded staged block reading

Finalize now reads staged blocks in pages controlled by:

```yaml
ingestion:
  finalize_read_batch_size: 1000
  finalize_max_in_memory_blocks: 5000
  finalize_streaming_required_above_blocks: 5000
```

The current implementation is a bounded paged implementation. It avoids a single unbounded `fetch_all` and rejects documents above the configured memory guard instead of risking OOM. A fully streaming chunking pipeline remains a future optimization.

### Start idempotency order

`StartLogicalDocumentIngestion` now checks the existing `(access_zone_id, idempotency_key)` first. If the fingerprint matches, it returns the existing session before enforcing active-session limits.

### Append replay order

`AppendLogicalDocumentBlocks` checks the existing batch first. Existing batch with the same hash returns idempotent success without checking `max_blocks_per_document` and without changing counters.

### Cleanup retention split

Completed sessions now have separate retention for staging rows and result replay:

```yaml
ingestion:
  staging_completed_blocks_retention_seconds: 86400
  completed_session_result_retention_seconds: 604800
  failed_session_retention_seconds: 86400
```

The cleanup worker deletes completed blocks/batches first, keeps the completed session and `result_response_json`, and deletes the completed session only after result retention expires.

### Qdrant error mapping

Non-retryable Qdrant HTTP statuses are mapped to domain errors:

- `400`, `422` -> `InvalidArgument`;
- `401` -> `Unauthenticated`;
- `403` -> `PermissionDenied`;
- `404` -> `NotFound`;
- `409` -> `FailedPrecondition`;
- `429` -> `ResourceExhausted`;
- `5xx` -> `Unavailable`.

### Token-budget diagnostics

If more than 50 chunk ids are dropped by token-budget truncation, diagnostics now include:

```text
DROPPED_CHUNK_IDS_TRUNCATED
```

while `context_chunks_dropped_by_token_budget` keeps the full count.

### Load smoke

`tests/load/test_fix451_grpc_load_smoke.py` imports safely without generated stubs, skips when `ASTRA_VECTOR_TEST_ENDPOINT` is absent, and can run real gRPC indexing requests when generated protobuf modules are available.

## Known limitations

- Rust toolchain was not available in the patching environment, so `cargo fmt/check/test/bench` must be run locally.
- Finalize is bounded-paged, not fully streaming through the entire chunking engine. Documents above `finalize_max_in_memory_blocks` are rejected safely instead of being processed unboundedly.
- Testcontainers scenarios are documented by the test names and load scaffold, but full Rust integration tests require local toolchain and CI setup.

## Required local verification

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```
