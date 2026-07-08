# AstraVector v007/fix465 — P2 production hardening

## Scope

`fix465` closes the P2 production-hardening backlog on top of `fix464`:

1. Qdrant payload indexes for retrieval filters.
2. Metrics by all `retrieval_sources`, not only the primary source.
3. Safe checksum error messages.
4. PostgreSQL timeout setup through `set_config` bind parameters.
5. Blocking self-contained smoke-load test with Testcontainers.
6. Version alignment to `0.4.1-fix465-p2-production-hardening`.
7. `astravector-enrichment` removed from production image scope.
8. Grafana dashboard JSONs for retrieval, consistency, TTL, runtime and overview.

## Validation

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
cargo test --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture
./scripts/check-version-alignment.sh
kubectl apply --dry-run=server -f k8s/
```

`fix465` is not a new RAG feature release; it is a production-hardening release.
