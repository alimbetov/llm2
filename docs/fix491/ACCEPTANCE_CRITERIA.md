# FIX491 Acceptance Criteria

FIX491 is accepted only when all mandatory criteria below are proven on the tested SHA.

## PostgreSQL

- Clean PostgreSQL 16 + pgvector starts with no AstraVector schema objects.
- Full repository migration chain applies successfully.
- `_sqlx_migrations` exactly matches repository migration history/checksums for the tested SHA.
- Required `astravector` schema objects exist after bootstrap.
- Partitioned parents, partition children and partition keys are correct.
- Columns/types/defaults/nullability match expected migrated schema.
- PK/FK/UNIQUE/CHECK constraints match.
- Indexes and partial-index predicates match.
- Extensions, including `vector` where required, are present and compatible.
- Current working database versus clean migration-built database has zero MATERIAL_DRIFT.
- Audit does not mutate the current working database.
- PostgreSQL backup/PITR recovery boundary is documented: migrations recover schema, not lost canonical rows.

## Qdrant

- Missing collection is recreated using shared AstraVector Qdrant configuration semantics.
- Empty compatible collection can be rebuilt from PostgreSQL canonical state.
- Non-empty collection is never destructively replaced without explicit destructive opt-in.
- Dense/sparse representations are reused from PostgreSQL where persisted; rebuild does not silently re-embed with a different representation version.
- Eligible canonical bindings are projected.
- Deleted/expired/inactive/non-searchable canonical rows are not projected as searchable points.
- Existing canonical binding/point identity is preserved where required by current model.
- Historical completed outbox rows are not rewritten to fake recovery.
- Qdrant payload/index configuration required by retrieval is restored.
- `missing eligible points = 0` after clean rebuild.
- `orphan Qdrant points = 0` after clean rebuild.
- representation/version/payload mismatch count = 0.

## Retrieval recovery proof

- A frozen query set is executed before Qdrant destruction and baseline is captured.
- Qdrant target collection is removed while PostgreSQL canonical state remains unchanged.
- Qdrant is rebuilt solely from PostgreSQL/AstraVector canonical representation state.
- Same frozen query set executes after rebuild.
- Context identity and visibility semantics match baseline.
- Ordering/scores match exactly where deterministic or within documented tolerance.
- No access-zone/access-level/version/lifecycle correctness regression occurs.
- Graph/degradation provenance remains equivalent where applicable.

## Full isolated proof

- Disposable PostgreSQL and Qdrant infrastructure is used.
- Empty PostgreSQL -> migrations -> ingestion -> projection -> retrieval succeeds.
- Qdrant destruction -> rebuild -> retrieval parity succeeds.
- PostgreSQL schema audit succeeds after the same run.
- Qdrant projection audit succeeds after the same run.

## Regression gates

Mandatory:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Focused FIX491 tests must cover:

- clean migration bootstrap;
- migration checksum/history mismatch detection;
- material schema-drift detection;
- Qdrant missing-collection rebuild;
- Qdrant incompatible/non-empty destructive refusal;
- orphan/missing point audit;
- before/after retrieval parity.

## Forbidden acceptance shortcuts

Do not claim PASS based only on:

- `cargo sqlx migrate run` succeeding;
- table counts;
- Qdrant collection existence;
- point count equality alone;
- endpoint HTTP/gRPC success;
- manual inspection without machine-readable evidence.

## Required evidence

Checked-in final summaries:

```text
docs/fix491/POSTGRES_RECOVERY_RESULT.md
docs/fix491/QDRANT_RECOVERY_RESULT.md
docs/fix491/PERSISTENCE_RECOVERY_RESULT.md
```

Each must include tested SHA, environment identity, commands, actual counters, failures/blocks and exact verdict.

## Final verdict

Only:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

PASS requires PostgreSQL reproducibility, zero material schema drift, Qdrant rebuild consistency, retrieval parity and all mandatory regression gates.