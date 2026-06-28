# AstraVector_v004 Hardening Roadmap Phase 1 Report

## Verdict

`SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS` preserved.

This report covers Phase 0 baseline verification and Phase 1 safe fixes from `AstraVector_v004 Hardening Roadmap.pdf`.

## Scope Completed

- Removed `legal_hold=false` from read visibility filters in parent context and chunk group fetches.
- Replaced sparse inference `unwrap()` with a typed internal error.
- Documented current `UNIMPLEMENTED` proto methods.
- Documented that `SearchRequestV004.filters` is future extension surface, not a proven arbitrary caller-filter contract.

## Scope Not Changed

- BM25/Sparse/Hybrid retrieval was not implemented.
- Lifecycle/delete/TTL/legal hold runtime behavior was not implemented.
- Recovery/reconciliation repair redesign was not implemented.
- Worker supervision, load/backpressure, and observability hardening were not implemented.

## Validation Commands

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
./smoke-tests/v004/scripts/run-full-smoke.sh --profile reliability-closing --keep-running
./smoke-tests/v004/scripts/run-full-smoke.sh --only bm25-hybrid-retrieval --keep-running
```

## Final Validation Results

```text
cargo fmt --check: PASS
cargo check --all-targets --all-features: PASS
cargo test --all-targets --all-features: PASS
cargo clippy --all-targets --all-features -- -D warnings: PASS
reliability-closing: PASS 9 / FAIL 0 / BLOCKED 0 / SKIPPED 0
bm25-hybrid-retrieval: BM25_RETRIEVAL_BLOCKED
```

## Reliability Closing Step Results

```text
build: PASS
migrations: PASS
full-power-wave1: PASS
access-security: PASS
consistency: PASS
atomicity-failpoints: PASS
outbox-fencing: PASS
dead-letter-qdrant-failure: PASS
data-integrity-audit: PASS
```

## BM25 / Hybrid Result

`BM25_RETRIEVAL_BLOCKED` is the correct result for this phase because the production Search API still has no BM25-only or hybrid execution path, no query sparse request path, no sparse/BM25 Qdrant search path, and no hybrid fusion path.

See:

- `smoke-tests/v004/reports/BM25_HYBRID_RETRIEVAL_REPORT.md`
- `smoke-tests/v004/reports/bm25-hybrid-results.json`
- `smoke-tests/v004/reports/bm25-hybrid-candidates.jsonl`

## Remaining Blockers

- Lifecycle
- Recovery/Reconciliation
- Load/Backpressure
- Observability
- Security hardening beyond access zones
- BM25/Sparse/Hybrid retrieval

