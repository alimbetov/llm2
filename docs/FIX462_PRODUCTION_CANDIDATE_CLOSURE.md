# AstraVector v007/fix462 — Production Candidate Closure

fix462 is considered complete only after the enhanced production-candidate gates pass.

## Closed consistency areas

- network gRPC RetrieveContext E2E via tonic client/server;
- parent grouping by `(access_zone_id, parent_id)`;
- zone-aware GraphRAG `RelatedChunk` and seed expansion;
- legal-hold-safe Qdrant reconciliation;
- tombstone purge order: `vector_bindings_v004` before `content_chunks_v004`;
- `RetryDocumentDeletion` runtime safety via `last_delete_error_stage` migration;
- `delete_operation_id` guards for lifecycle updates;
- SQLx schema validation;
- observability validation;
- smoke RetrieveContext load test;
- rollback and operator documentation.

## Required commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

## Status rule

If any P0/P1 gate above is not proven by tests, the artifact remains `PATCH CANDIDATE`, not `PRODUCTION CANDIDATE`.
