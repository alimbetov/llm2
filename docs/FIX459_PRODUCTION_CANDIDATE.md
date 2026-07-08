# AstraVector v007 fix459 — production-candidate hardening

This patch is based on the static audit of `fix458` and targets the remaining P0/P1 issues before production-candidate status.

## Implemented changes

### P0

- Replaced the bootstrap-only `e2e_testcontainers` check with a real PostgreSQL + Qdrant lifecycle test:
  - applies migrations;
  - creates an access zone and document/chunk state;
  - creates a Qdrant collection and point;
  - verifies Qdrant point visibility;
  - expires the document;
  - runs the index TTL cleanup batch;
  - asserts PostgreSQL `DELETED` and Qdrant point absence.

### P1

- GraphRAG expansion joins now carry `access_zone_id` through seed/edge/target joins and bind by `(access_zone_id, node_id)`.
- GraphRAG parent context fetch uses `LEFT JOIN` with visibility predicates in `ON`, preserving accessible child chunks when parent context is unavailable.
- Direct parent fetch paths now check `document_versions.lifecycle_status='ACTIVE'` and `document_versions.expires_at`.
- Ingestion access-zone resolution can force strict DB recheck for cached `ACTIVE` zones via `access_zone_registry.always_recheck_on_ingestion`.
- Access-zone auto-create now distinguishes conflicts with existing `ACTIVE`, `DISABLED`, `DELETED`, and other non-active rows.
- Added `RetryDocumentDeletion` admin gRPC method to move `DELETE_PERMANENTLY_FAILED` / `DELETE_FAILED` documents back into retry flow.
- TTL cleanup now uses `next_delete_attempt_at` as the authoritative retry schedule for `DELETE_FAILED` rows.
- Kubernetes gRPC probes now use `astravector.embedding.v1.AstraVectorRuntime`.
- Added migration `0034_v007_fix459_production_candidate_cleanup.sql` for `document_versions.metadata` and GraphRAG/TTL indexes.

## Required validation

The archive was produced in an environment without Rust tooling. Before merge, run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

## Status

After successful CI and testcontainers validation, this version may be treated as `production-candidate` for internal high-load use. It is not yet final `production-ready`; load, chaos, and soak tests remain mandatory.
