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
| quality contracts without local model/tokenizer files | PASS | `ci-parity-quality-contracts.log` |

The first sandboxed all-target run and SQLx run were blocked by OS `EPERM` for Docker/network access.
Their diagnostic logs are preserved as `*-sandbox-blocked.log`; both commands were rerun outside the
sandbox and passed. No blocked run is counted as PASS.

The tracked `rag-quality-bank-v1` judgment bundle was regenerated with the repository's official
deterministic preparation script after the contract suite proved its stored access-zone identities
were stale. The generated status remains `AWAITING_BLIND_JUDGMENT`; no production ranking output or
expected-label input was used.

The first GitHub Actions run exposed that the candidate-pool self-test coupled selection
determinism to publication-only runtime identity files. The self-test now permits only the
`RUNTIME_IDENTITY_INCOMPLETE` prerequisite while still rejecting every pool/source/depth
failure. Publication remains fail-closed. CI parity was verified with explicitly missing
model/tokenizer paths: 18 passed, 0 failed.
