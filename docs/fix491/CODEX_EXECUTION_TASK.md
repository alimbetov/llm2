# FIX491 Codex Execution Task

Work in repository `alimbetov/llm2` on branch `agent/fix491-persistence-recovery`.

Read first, in this order:

1. `docs/fix491/TECHNICAL_SPECIFICATION.md`
2. `docs/fix491/ACCEPTANCE_CRITERIA.md`
3. current persistence/Qdrant/recovery/outbox code and migrations
4. existing FIX489/FIX490 evidence that establishes inherited invariants

## Mission

Implement FIX491 exactly as specified: deterministic PostgreSQL canonical recovery/audit plus deterministic Qdrant projection rebuild/audit, without changing retrieval semantics.

## Required implementation direction

Preserve:

```text
PostgreSQL = canonical state
Qdrant     = rebuildable projection
```

Do not make Qdrant authoritative and do not add a new service.

Implement the smallest transport/operator surface necessary for:

```text
astravector-runtime migrate
astravector-runtime recovery postgres-audit
astravector-runtime recovery postgres-bootstrap-proof
astravector-runtime recovery qdrant-rebuild
astravector-runtime recovery qdrant-audit
astravector-runtime recovery full-proof
```

Equivalent minimal CLI naming is acceptable only if documented and scriptable.

## PostgreSQL implementation requirements

- Reuse SQLx migrations in `migrations/`.
- Inspect `_sqlx_migrations` including checksums and unknown/pending versions.
- Build semantic schema inventory from PostgreSQL catalogs; do not rely only on raw `pg_dump diff`.
- Cover extensions, schemas, tables/partitioned tables, partition keys/children, columns, defaults, nullability, constraints, indexes/predicates, sequences and functions/triggers/views if present.
- Add clean-DB bootstrap proof using disposable PostgreSQL 16 + pgvector infrastructure.
- Add comparison/audit against an existing DB without mutating it.
- Classify `NO_DRIFT`, `BENIGN_DRIFT`, `MATERIAL_DRIFT`, `BLOCKED`.
- Never auto-repair material drift in audit mode.
- Keep PostgreSQL data-loss recovery boundary explicit: schema from migrations; canonical rows from operator-provided backup/PITR.

## Qdrant implementation requirements

- Reuse existing Qdrant client/collection creation semantics; do not duplicate vector config in scripts.
- Rebuild from PostgreSQL canonical chunks/bindings/embedding representations.
- Reuse persisted dense/sparse vectors when available; do not silently re-embed.
- Preserve canonical binding IDs / Qdrant point IDs where current schema makes them canonical.
- Restore required payload and payload indexes.
- Reuse current lifecycle/searchability semantics; avoid a second divergent rule set.
- Default behavior must refuse destructive replacement of a non-empty collection.
- Require explicit destructive opt-in for replacement.
- Do not rewrite historical completed outbox rows to fake recovery.
- Run reconciliation after rebuild and classify missing/orphan/mismatched projection state.

## Proof requirements

Add focused integration tests and/or deterministic scripts for:

1. clean migration bootstrap;
2. SQLx checksum/history mismatch detection;
3. material schema drift detection;
4. missing Qdrant collection rebuild;
5. non-empty collection destructive refusal;
6. projection audit with orphan/missing detection;
7. before/after retrieval parity after deleting only Qdrant collection;
8. full isolated persistence proof.

Prefer extending existing `tests/e2e_testcontainers.rs`, local-demo and existing recovery/reconciliation helpers rather than adding a parallel framework.

## Safety

Do not change:

- tokenizer/BGE-M3 ownership;
- chunking;
- dense/sparse/hybrid ranking;
- fusion/RRF;
- parent hydration;
- GraphRAG;
- MMR;
- token budget;
- visibility/access-zone/access-level semantics;
- lifecycle/version semantics;
- canonical PostgreSQL authority;
- outbox correctness semantics.

If a change to one of these is required, stop and report:

`FIX491_BLOCKED_BY_ARCHITECTURE_CHANGE`

## Mandatory gates

Run:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Then execute the FIX491 focused proof on disposable infrastructure.

## Evidence

Create/update:

```text
docs/fix491/RECOVERY_RUNBOOK.md
docs/fix491/POSTGRES_RECOVERY_RESULT.md
docs/fix491/QDRANT_RECOVERY_RESULT.md
docs/fix491/PERSISTENCE_RECOVERY_RESULT.md
```

Include tested SHA, exact commands, environment/database/collection identity, counters, drift classification, retrieval parity, regression gates and final verdict.

Do not claim PASS for checks that were not executed.

Final verdict must be one of:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

Do not merge to `main` automatically.