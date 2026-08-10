# FIX491 Acceptance Criteria

FIX491 is accepted only when all mandatory criteria below are proven on the tested SHA.

## PostgreSQL

- Clean PostgreSQL 16 + pgvector starts with no AstraVector schema objects.
- Full repository migration chain applies successfully.
- `_sqlx_migrations` exactly matches repository migration history/checksums for the tested SHA.
- Unknown, failed and pending migrations are detected.
- Required `astravector` schema objects exist after bootstrap.
- Partitioned parents, partition children and partition keys are correct.
- Columns/types/defaults/nullability/identity attributes match expected migrated schema.
- PK/FK/UNIQUE/CHECK constraints match.
- Indexes and partial-index predicates match.
- Extensions, including `vector`, are present and compatible.
- Runtime-material functions/views/triggers/sequences/ownership/privileges are included where applicable.
- Current working database versus clean migration-built database has zero MATERIAL_DRIFT.
- Unknown catalog differences are not silently classified BENIGN.
- Audit does not mutate the current working database.
- Canonical row integrity audit covers document/chunk/cache/binding/outbox/access-zone/lifecycle relationships.
- Failed/dead outbox and deletion-fenced/in-progress states are surfaced.
- PostgreSQL backup/PITR recovery boundary is documented: migrations recover schema, not lost canonical rows.

## Shared Qdrant projection contract

- A single canonical Qdrant projection builder/path is used by outbox, reconciliation and FIX491 rebuild, or equivalent code sharing proves identical construction.
- FIX491 does not introduce a third handwritten payload builder.
- Builder parity tests prove point IDs, dense/sparse vectors and payload fields are identical for the same canonical PostgreSQL input.
- Existing outbox semantics remain the reference unless a concrete defect is separately proven.

## Qdrant collection schema

- Missing collection is recreated using shared `QdrantClient` collection configuration semantics.
- `collection_exists` alone is not accepted as compatibility proof.
- Existing collection config is audited for dense dimension/names, distance metric, sparse config and required payload indexes/types.
- Incompatible collection fails closed by default.
- Qdrant server version and collection config identity/hash are captured in evidence.

## Qdrant rebuild

- Empty compatible collection can be rebuilt from PostgreSQL canonical state.
- Non-empty collection is never destructively replaced without explicit destructive opt-in.
- Non-empty already-consistent collection can be audited/no-op without destructive replacement.
- Dense/sparse representations are reused from PostgreSQL where persisted; rebuild does not silently re-embed.
- Missing required persisted representation is classified as canonical inconsistency/failure, not repaired by inference fallback.
- Eligible canonical bindings are projected using shared eligibility semantics.
- Deleted/expired/inactive/non-searchable canonical rows are not projected as searchable points.
- Existing canonical binding/point identity is preserved.
- Historical completed outbox rows are not rewritten to fake recovery.
- Required payload/index configuration is restored.
- `missing eligible points = 0` after clean rebuild.
- `orphan Qdrant points = 0` after clean destructive rebuild.
- representation/version/payload mismatch count = 0.

## Recovery safety and operability

- Full/destructive rebuild is fenced from conflicting normal publisher/reconciliation writes by an explicitly documented mechanism.
- Concurrent conflicting recovery attempts are rejected or safely serialized.
- Rebuild uses deterministic stable ordering and configurable bounded batches.
- Rebuild does not load the complete projection into memory.
- Upserts are idempotent.
- Interrupted rebuild can be safely resumed/rerun.
- Cancellation and bounded retry behavior are supported.
- Progress/failure counters are emitted.
- Destructive mode logs redacted PostgreSQL/Qdrant target identity before action.
- Secrets/passwords/API keys are never emitted in logs/evidence.

## Qdrant audit completeness

- Audit compares expected canonical projection with actual Qdrant state.
- Audit reports missing, orphan, payload and representation/version mismatches.
- Audit validates collection schema and payload indexes.
- Audit records scroll pages/points/status.
- Timeout/limit/loop-detected/incomplete scroll cannot produce a CONSISTENT verdict.
- Read-only audit does not delete/quarantine orphan points.

## Retrieval recovery proof

- A frozen query set is executed before Qdrant destruction and baseline is captured.
- Query set includes dense, and sparse/hybrid when supported by fixture, plus visibility/no-answer and Graph-enabled coverage where applicable.
- PostgreSQL canonical fingerprint/counters are captured before Qdrant loss.
- Qdrant target collection is removed while PostgreSQL canonical state remains intact.
- Qdrant is rebuilt solely from PostgreSQL/AstraVector persisted representation state.
- Same frozen query set executes after rebuild.
- Context identity and visibility semantics match baseline.
- Ordering/scores match exactly where deterministic or within documented tolerance.
- No access-zone/access-level/version/lifecycle correctness regression occurs.
- Graph/degradation provenance remains equivalent where applicable.
- PostgreSQL canonical fingerprint remains unchanged except explicitly normalized operational fields.

## Startup/readiness proof

- Existing startup semantics are not silently changed.
- Missing Qdrant collection with auto-create may create an empty collection, but this event is not treated as proof that projection recovery completed.
- Recovery evidence distinguishes `collection exists` from `projection consistent`.

## Full isolated proof

- Disposable PostgreSQL and Qdrant infrastructure is used.
- Empty PostgreSQL -> migrations -> migration/schema audit succeeds.
- Ingestion -> canonical state -> projection -> activation/retrieval succeeds.
- Qdrant destruction -> rebuild -> projection audit -> retrieval parity succeeds.
- PostgreSQL schema and canonical-data integrity audit succeeds after the same run.
- Qdrant projection audit succeeds with a complete scan.
- Interruption/resume and concurrency-fence cases are exercised separately if not in the primary full-proof run.

## Regression gates

Mandatory:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Focused FIX491 tests must cover at least:

- clean migration bootstrap;
- migration checksum/history mismatch detection;
- unknown/pending migration detection;
- material schema-drift detection;
- audit read-only behavior;
- canonical-data integrity checks;
- shared projection-builder parity;
- Qdrant missing-collection rebuild;
- collection-schema compatibility/incompatibility;
- payload-index recreation;
- non-empty destructive refusal;
- explicit destructive replace on disposable collection;
- no inference fallback during rebuild;
- bounded batched rebuild;
- interruption/resume;
- concurrent rebuild fencing;
- incomplete-scroll fail-closed behavior;
- orphan/missing/payload/version drift detection;
- before/after retrieval parity;
- PostgreSQL fingerprint unchanged.

## Forbidden acceptance shortcuts

Do not claim PASS based only on:

- `cargo sqlx migrate run` succeeding;
- table counts;
- Qdrant collection existence;
- point count equality alone;
- partial Qdrant scroll;
- endpoint HTTP/gRPC success;
- reconciliation returning without error;
- manual inspection without machine-readable evidence.

## Required evidence

Checked-in final summaries:

```text
docs/fix491/POSTGRES_RECOVERY_RESULT.md
docs/fix491/QDRANT_RECOVERY_RESULT.md
docs/fix491/PERSISTENCE_RECOVERY_RESULT.md
```

Machine-readable JSON counterparts are mandatory for final counters/verdicts.

Each report must include tested SHA, environment identity, commands, actual counters, drift classification, recovery fence, scan completeness, retrieval parity, failures/blocks and exact verdict.

## Final verdict

Only:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

PASS requires PostgreSQL reproducibility and canonical integrity, zero material schema drift, shared projection contract, complete Qdrant rebuild/audit consistency, safe resumable/fenced recovery, PostgreSQL fingerprint preservation, retrieval parity and all mandatory regression gates.