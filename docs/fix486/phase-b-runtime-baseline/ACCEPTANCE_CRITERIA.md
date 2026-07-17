# FIX486B acceptance criteria

## Identity and lineage

```text
[ ] Work branch is based on the approved current epic SHA
[ ] Worktree was clean at every official run start
[ ] Source, origin/main and epic SHAs recorded
[ ] Cargo.lock, config, binary, model, tokenizer and fixture hashes recorded
[ ] PostgreSQL and Qdrant image identities recorded
[ ] No identity changed during an official run
```

## Mandatory static and integration gates

```text
[ ] cargo fmt --all --check PASS
[ ] cargo check --locked --all-targets --all-features PASS
[ ] cargo test --locked --all-targets --all-features PASS
[ ] cargo clippy --locked --all-targets --all-features -- -D warnings PASS
[ ] cargo sqlx prepare --check -- --all-targets --all-features PASS
[ ] e2e_testcontainers PASS
[ ] smoke_load_retrieve_context_testcontainers PASS
[ ] fix486_hierarchical_bank_contracts PASS
```

## R1 clean cold start

```text
[ ] No pre-existing runtime port owner
[ ] Clean PostgreSQL and Qdrant started
[ ] Actual image IDs/digests recorded
[ ] Clean migrations PASS
[ ] Migration reapply idempotent
[ ] Migration head matches repository
[ ] Schema integrity violations = 0
[ ] Model/tokenizer checks and warmup PASS
[ ] Dense dimension = 1024
[ ] Sparse capability recorded
[ ] Locked release build PASS
[ ] Reflection, health and metrics PASS
[ ] Control fixture ingested through production path
[ ] Repeated ingestion produced no duplicates
[ ] Search control probe PASS
[ ] RetrieveContext control probe PASS
[ ] Search/RetrieveContext logical identity compatible
[ ] Clean shutdown and port audit PASS
```

## R2 independent repetition

```text
[ ] Phase-owned persistent state destroyed and recreated
[ ] R2 repeats all mandatory R1 stages
[ ] R1/R2 source and dependency identities match
[ ] Migration head matches
[ ] Hierarchy shape matches
[ ] Deterministic physical identities match for identical production inputs
[ ] Search/RetrieveContext logical result identity matches
[ ] Normalized stage verdict set matches
```

## R3 restart and recovery

```text
[ ] Runtime restart requires no reingestion
[ ] Post-restart Search and RetrieveContext PASS
[ ] Logical result identity remains stable
[ ] Readiness fails when Qdrant is unavailable
[ ] Readiness recovers after Qdrant restoration
[ ] Readiness fails when PostgreSQL is unavailable
[ ] Readiness recovers after PostgreSQL restoration
[ ] Dependency loss is not reported as ordinary no-answer
[ ] Leaked processes = 0
[ ] Leaked port owners = 0
```

## Defects and evidence

```text
[ ] Every discovered defect has a complete record
[ ] Every repaired P0/P1 has FAIL evidence
[ ] Every repaired P0/P1 has a failing regression test before repair
[ ] Every repaired P0/P1 has a separate minimal fix commit
[ ] Every repaired P0/P1 has same-input before/after evidence
[ ] Unresolved in-scope P0 = 0
[ ] Unresolved in-scope P1 = 0
[ ] External evidence bundle validates against its manifest
[ ] Compact final result and manifest hash committed
```

## Scope protection

```text
[ ] Phase A hierarchical queries/qrels were not changed for PASS
[ ] Bank 1.0.0 was not frozen
[ ] No hierarchy quality, hybrid superiority or Mac SLO verdict claimed
[ ] fix486c handoff blockers recorded
```

## Final verdict

`FIX486_RUNTIME_BASELINE_PASS` is permitted only when every mandatory item above is satisfied.

Any mandatory `FAIL`, `BLOCKED`, `SKIPPED`, missing evidence, identity mismatch, dirty worktree or unresolved in-scope P0/P1 requires:

```text
FIX486_RUNTIME_BASELINE_BLOCKED
```