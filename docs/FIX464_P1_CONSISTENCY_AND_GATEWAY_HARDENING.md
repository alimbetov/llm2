# AstraVector v007/fix464 — P1 consistency and gateway hardening

This patch closes the P1 issues found in the deep fix463 audit without adding new retrieval features.

## Scope

1. Atomic DB claim before Qdrant `DELETE_POINT`.
2. `mark_synced rows=0` is now a fenced error, not a successful completion.
3. Runtime graceful shutdown cancels background workers before the gRPC server exits.
4. `recovery` and `retention` workers accept `CancellationToken`.
5. Recovery errors are logged and metered instead of ignored.
6. Forwarded identity headers require a trusted gateway proof and restrictive `NetworkPolicy`.
7. E2E coverage now includes real tonic `IndexLogicalDocument` ingestion facade + activation.

## Outbox DELETE_POINT fencing

`DELETE_POINT` no longer performs a Qdrant delete after only an in-memory stale check. It first claims the binding in PostgreSQL:

```sql
UPDATE astravector.vector_bindings_v004
SET qdrant_sync_status='DELETE_IN_PROGRESS', updated_at=now()
WHERE access_zone_id=$1
  AND id=$2
  AND ttl_generation=$3
  AND legal_hold=false
  AND qdrant_sync_status IN ('DELETE_PENDING','DELETION_PENDING','DELETE_IN_PROGRESS')
RETURNING qdrant_point_id;
```

Only the returned point id is deleted from Qdrant. Final DB transition to `DELETED/SOFT_DELETED` is also fenced by `DELETE_IN_PROGRESS`, `ttl_generation`, and `legal_hold=false`.

## mark_synced fencing

`mark_synced` returning `rows_affected != 1` now returns `AstraError::OwnershipLost`. This keeps the outbox event retryable instead of marking it `COMPLETED` while binding state remains unconfirmed.

## Shutdown model

On SIGTERM/ctrl-c:

1. readiness is set to false;
2. the shared `CancellationToken` is cancelled;
3. the process waits `shutdown.drain_timeout_seconds` before server shutdown completes.

Recovery and retention loops use `tokio::select!` with `shutdown.cancelled()`.

## Gateway trust model

`x-astravector-role` and access-level identity headers are trusted only if:

- `security.enabled=true`;
- `security.trust_forwarded_identity_headers=true`;
- request includes configured `security.gateway_trust_header` with `security.gateway_trust_token`;
- Kubernetes `NetworkPolicy` allows gRPC ingress only from pods labelled `app=astravector-gateway`.

The gateway must strip user-supplied `x-astravector-*` headers and inject trusted internal identity headers.

## Tests

Added static hardening guard tests in:

```text
tests/fix464_p1_hardening_contracts.rs
```

Added a real tonic ingestion facade E2E test in:

```text
tests/e2e_testcontainers.rs
```

The new E2E calls:

```text
AstraVectorIngestionFacade.IndexLogicalDocument
AstraVectorV004Control.ActivateDocumentVersion
```

through generated tonic clients over TCP.

## Required verification

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
cargo sqlx prepare --check -- --all-targets --all-features
kubectl apply --dry-run=server -f k8s/
```
