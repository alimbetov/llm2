# AstraVector v007 fix4.5.8 — Critical production defect remediation

This patch addresses the release-blocking defects identified for internal high-load AstraVector deployments.

## Security deployment assumption

AstraVector remains an internal service. Direct external access is forbidden. The upstream gateway is responsible for authentication, caller authorization, `caller_access_level` derivation, access-zone authorization and sanitization of dangerous headers such as `x-astravector-role` and `x-astravector-access-level`.

## Fixed areas

1. **DEF-005 — ingestion session start race**
   - `StartLogicalDocumentIngestion` now performs idempotency lookup, limit checks and session insert in one PostgreSQL transaction.
   - Transaction-level advisory locks serialize global and per-document session start checks.

2. **DEF-006 — concurrent indexing of the same document**
   - `document_versions` now supports `processing_owner_id`, `processing_started_at` and `processing_heartbeat_at`.
   - Indexing can be claimed only from `REGISTERED` or `FAILED`; concurrent `INDEXING` claims are rejected.
   - Recovery worker moves stale `INDEXING` owners to `FAILED`.

3. **DEF-011 — eternal DELETE_FAILED**
   - `DELETE_FAILED` now has bounded retries using `max_delete_attempts` and `next_delete_attempt_at`.
   - Permanent failures move to `DELETE_PERMANENTLY_FAILED` and require manual intervention.

4. **DEF-021 — stale access-zone registry cache**
   - Cache TTL default reduced to 5 seconds.
   - Cached ACTIVE entries are rechecked against PostgreSQL after `active_recheck_interval_ms`.
   - DISABLED/DELETED recheck invalidates cache and rejects the request.

5. **DEF-017 — GraphRAG parent text leakage**
   - Graph context parent chunk joins now require matching `access_zone_id`, `ACTIVE` lifecycle, access-level visibility, non-expired TTL and non-deleted parent chunk.

6. **DEF-026 — Qdrant 404 during delete**
   - Qdrant delete operations now treat `404 Not Found` as idempotent success.

7. **DEF-027 — retrieval/Qdrant backpressure**
   - Added semaphores for `RetrieveContext`, Qdrant dense/sparse search, GraphRAG expansion and MMR embedding fetch.
   - Overload returns `RESOURCE_EXHAUSTED` or degrades optional graph/MMR work with warnings.

8. **DEF-036 — testcontainers proof**
   - Added testcontainers dev dependencies and a mandatory CI job for `--features integration-tests`.
   - Added bootstrap test that starts PostgreSQL and Qdrant containers instead of silently skipping.

9. **DEF-043 — Kubernetes probes**
   - Kubernetes deployment now uses gRPC probes instead of TCP socket probes.
   - gRPC health status is updated from runtime readiness checks.

10. **DEF-009 — embedding cache model versioning**
   - Document embedding cache ids and cache keys now include tokenizer/model/dense/sparse versions.

## New migration

`migrations/0033_v007_fix458_critical_production_defects.sql`

Adds:

- processing owner columns;
- `next_delete_attempt_at`;
- terminal lifecycle status support;
- indexes for stale indexing recovery and delete retry scheduling;
- access-zone status version field;
- version lookup index for embedding cache.

## Required validation

Run before release:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

The current patch was prepared in an environment without `cargo`; CI must be the final release gate.
