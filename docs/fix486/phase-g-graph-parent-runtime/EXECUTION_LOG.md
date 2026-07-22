# FIX486G Execution Log

## Identity

```text
branch: codex/fix486g-graph-parent-proof
starting SHA: d1f49c13749e19727b4369add3cf7e98fe4461e7
base Phase F verdict: FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
mandatory bank: 1.0.0 / FROZEN / cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

## Completed stages

1. All Phase G specifications, frozen inputs, supplemental inputs, Issue #28 and
   PR #29 review context were read.
2. `capability-audit.md` mapped the production path and ended with
   `UNKNOWN_MATERIAL_CAPABILITIES = 0`.
3. `DESIGN_REVIEW.md` approved focused contract implementation without ranking
   tuning.
4. Red baseline at source SHA `d1f49c1` produced `7 PASS / 5 FAIL` and was
   preserved externally under
   `astravector-evidence/fix486g/red-baseline-d1f49c1/contract-test.log`.
5. Minimal production repairs added canonical synced-binding validation,
   stable edge identity, complete Graph provenance, self-edge rejection and a
   bounded pre-hydration reserve.
6. Relation ingestion was made granularity-aware without changing the behavior
   of older relations that omit endpoint granularity fields.
7. The expanded focused runner/bank suite passed `17/17`.
8. Supplemental bank review findings were resolved independently of runtime:
   71 unique queries, 71 materialized qrel assignments, corrected language
   metadata and immutable `1.0.0 / FROZEN` hashes.
9. `cargo fmt`, locked all-target check, clippy with denied warnings and the
   complete locked all-target test suite passed, including the 50-concurrent
   Testcontainers retrieval smoke.
10. The first outside-sandbox runtime attempt reached production ingestion and
    activation, then preserved BLOCKED evidence when its canonical audit mixed
    non-CHUNK semantic edges into CHUNK endpoint invariants. The audit was
    narrowed to typed `CHUNK -> CHUNK` edges without changing runtime behavior.
11. The next clean run passed the repaired audit and exposed first loss at
    parent deduplication: hydrated child identities were removed before Graph
    seed selection. A red contract and immutable BLOCKED evidence were saved,
    then seed discovery was separated from final parent-context deduplication.
12. Runtime replay showed the preserved children still competed with their own
    parent representatives for the bounded seed cap. Seed construction now
    chooses the best canonical child of each admitted parent group, preventing
    structural parent edges from displacing relation-bearing child endpoints.
13. A subsequent replay proved the terminal identity overwrite:
    `graph_seed_chunk_id` preferred `parent_chunk_id` even for a hydrated child.
    The generic seed contract now preserves `matched_chunk_id` and uses parent
    identity only as a malformed/missing matched-ID fallback.

## Current implementation stage

The phase-owned Compose profile, runtime config, SQL audit, Python verifier and
shell runner are being completed. No overall Phase G verdict has been issued.

## Required remaining evidence

- locked full static, SQLx and prior-phase gates;
- clean-SHA isolated runtime campaign;
- healthy Search/RetrieveContext proof and parity;
- all fault controls and state restoration;
- warm/restart repeatability;
- 71-query statistical campaign and confidence intervals;
- fail-closed manifest and cleanup;
- compact repository result package;
- PR #29 evidence comment while the PR remains draft.
