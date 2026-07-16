# FIX484V Result

## Verdict

`FIX484V_PRODUCTION_BLOCKED`

The current main is buildable, testable, and hardened for the tiered query-processing merge. Every
mandatory local build, static, contract, Docker, SQLx, testcontainers, and concurrent smoke gate run
for fix484v passed. The implementation is suitable for draft PR review and consolidation into local
`main`.

The broader project is not declared production-ready by this verification alone. External live-runtime
quality/holdout adjudication, extended soak/load testing, and deployment security review are outside
this issue's executed gate set. The existing `AWAITING_BLIND_JUDGMENT` quality status is preserved.

## Fixed

- Aligned Cargo, CI, and Docker on locked dependency MSRV Rust 1.88.
- Added locked CI gates and a healthy pgvector PostgreSQL service for SQLx verification.
- Removed temporary fix484 bootstrap workflow/script/evidence after confirming normal source builds.
- Fixed tier boundaries, hard maximum enforcement, segmentation tail/overlap behavior, logical-intent
  coverage, and overlap-safe fusion.
- Added weighted admission, receipt-based deadlines, cancellation propagation, bounded GraphRAG seeds,
  and one-stage GraphRAG/MMR contracts.
- Added backward-compatible legacy configuration migration with new-key precedence.
- Decoupled candidate-selection self-tests from publication-only local model identity while
  retaining fail-closed identity requirements for generated judgment bundles.

## Preservation

- Base `origin/main`: `9817c4b03d401efbc59bcbcdff00583a890f8e55`.
- Production Extended default remains disabled.
- No fixture-specific behavior was added to `src/**`.
- No production ranking weights, RRF k, sealed expectations, or holdout thresholds were changed.
