# fix484v Regression Report

| Gate | Result | Evidence |
|---|---:|---|
| `cargo fmt --all --check` | PASS | `cargo-fmt.log` |
| locked all-target check | PASS | `cargo-check.log` |
| query-processing unit tests | PASS | `unit-tests.log` |
| query-processing contracts (14) | PASS | `contract-tests.log` |
| locked all-target/all-feature tests | PASS | `all-tests.log` |
| clippy with warnings denied | PASS | `clippy.log` |
| SQLx metadata against local PostgreSQL | PASS | `sqlx-check.log` |
| 50-concurrent testcontainers smoke | PASS | `integration-tests.log` |
| Docker release build under Rust 1.88 | PASS | `docker-image-inspect.json` |

The first sandboxed all-target run and SQLx run were blocked by OS `EPERM` for Docker/network access.
Their diagnostic logs are preserved as `*-sandbox-blocked.log`; both commands were rerun outside the
sandbox and passed. No blocked run is counted as PASS.

The tracked `rag-quality-bank-v1` judgment bundle was regenerated with the repository's official
deterministic preparation script after the contract suite proved its stored access-zone identities
were stale. The generated status remains `AWAITING_BLIND_JUDGMENT`; no production ranking output or
expected-label input was used.
