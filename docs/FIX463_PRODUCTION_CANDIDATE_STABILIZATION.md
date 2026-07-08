# AstraVector v007/fix463 — Production Candidate Stabilization

## Scope

fix463 closes the consolidated P0/P1 backlog from the three audits of `fix462 enhanced`:

- full network RetrieveContext proof;
- outbox operation-version fencing;
- binding TTL generation safety;
- reconciliation payload and legal-hold safety;
- finalizing heartbeat during long indexing;
- tombstone purge/live-outbox safety;
- Kubernetes/runtime manifest correctness;
- gRPC timeout contract;
- SQLx/K8s/Docker production gates.

## Production-candidate gates

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
docker build -t astravector-runtime:0.4.1-fix465-p2-production-hardening .
kubectl apply --dry-run=server -f k8s/
```

## Status

This patch is a source-level stabilization patch. It still requires Rust CI, SQLx online validation, Docker build and testcontainers execution before it can be declared `PRODUCTION CANDIDATE`.
