# FIX491 Codex Execution Task

Work in repository `alimbetov/llm2` on branch `agent/fix491-persistence-recovery`.

Read first, in this order:

1. `docs/fix491/TECHNICAL_SPECIFICATION.md`
2. `docs/fix491/ACCEPTANCE_CRITERIA.md`
3. `src/outbox/mod.rs`
4. `src/reconciliation/mod.rs`
5. `src/qdrant/mod.rs`
6. `src/recovery/mod.rs`
7. current persistence code and all migrations
8. existing FIX489/FIX490 evidence that establishes inherited invariants

## Mission

Implement FIX491 exactly as specified: deterministic PostgreSQL canonical recovery/audit plus deterministic Qdrant projection rebuild/audit, without changing retrieval semantics.

Preserve:

```text
PostgreSQL = canonical state
Qdrant     = rebuildable projection
```

Do not make Qdrant authoritative and do not add a new service.

## Mandatory design direction before coding

The current code already constructs Qdrant payloads in both outbox and reconciliation. Do NOT create a third FIX491-specific payload builder.

First identify the exact current projection contract and extract one shared canonical projection builder/function used by:

```text
outbox publisher
reconciliation repair
FIX491 rebuild
```

Add tests proving the same canonical PostgreSQL input produces the same `QdrantPoint` from all callers.

Existing outbox projection behavior is the reference unless a concrete defect is proven.

Also reuse/extend existing `Reconciler` / `astravector-reconciliation` for scan/repair. Do not create a parallel reconciliation engine.

## Required operator capabilities

Provide scriptable semantics equivalent to:

```text
astravector-runtime migrate
astravector-runtime recovery postgres-audit
astravector-runtime recovery postgres-bootstrap-proof
astravector-runtime recovery qdrant-rebuild
astravector-runtime recovery qdrant-audit
astravector-runtime recovery full-proof
```

It is acceptable for `qdrant-rebuild/audit` to delegate to an extended `astravector-reconciliation` implementation. Keep one repair engine.

Every command must return non-zero on failure and emit a final machine-readable verdict.

## PostgreSQL implementation requirements

- Reuse SQLx migrations in `migrations/`.
- Inspect `_sqlx_migrations` including checksums and unknown/failed/pending versions.
- Build semantic schema inventory from PostgreSQL catalogs; do not rely only on raw `pg_dump diff`.
- Cover extensions/versions, schemas, tables/partitioned tables, partition keys/children, columns, identity/defaults/nullability, constraints, indexes/predicates, sequences, functions/views/triggers and runtime-material ownership/privileges.
- Add clean-DB bootstrap proof using disposable PostgreSQL 16 + pgvector infrastructure.
- Add comparison/audit against an existing DB without mutating it.
- Define deterministic `NO_DRIFT`, `BENIGN_DRIFT`, `MATERIAL_DRIFT`, `BLOCKED` rules. Unknown differences are not silently benign.
- Add read-only canonical-data integrity checks over document/chunk/cache/binding/outbox/access-zone/lifecycle relationships.
- Surface failed/dead outbox and deletion-fenced/in-progress states.
- Keep data-loss boundary explicit: schema from migrations; canonical rows from operator-provided backup/PITR.
- Document RPO/RTO boundary; do not implement a backup engine.

## Qdrant collection requirements

- Reuse `QdrantClient::ensure_collection` or shared extracted config logic.
- Add explicit compatibility audit; `collection_exists` is insufficient.
- Validate collection name, dense dimension/names, distance metric, sparse config and required payload indexes/types.
- Record Qdrant server version/config identity in evidence.
- Incompatible existing collection fails closed by default.

## Qdrant rebuild requirements

- Rebuild only from PostgreSQL canonical bindings/chunks/persisted embeddings/representation metadata.
- Reuse persisted dense/sparse vectors; normal recovery must not call inference.
- If required persisted representation is missing, classify/fail; do not silently re-embed.
- Preserve binding IDs and Qdrant point IDs.
- Reuse one shared searchability/eligibility rule across normal projection/reconciliation/rebuild/audit expected set.
- Preserve current lifecycle/TTL/legal-hold/deletion semantics exactly.
- Do not rewrite historical completed outbox rows.
- Prefer direct deterministic batched upsert through the shared projection builder followed by audit/reconciliation.

## Recovery fencing

A full/destructive rebuild must not race the normal outbox publisher or another recovery execution.

Implement and document a safe fence using the smallest architecture-compatible mechanism, for example:

- explicit offline mode requiring normal runtime/publisher stopped; or
- PostgreSQL advisory/recovery lease/generation; or
- another DB-backed fence compatible with current outbox fencing.

Do not assume operator discipline without a detectable guard if destructive concurrent writes could corrupt proof.

Add a test proving conflicting recovery is rejected or serialized.

## Rebuild scaling/resumability

Do not load the whole projection into memory.

Required:

- deterministic stable scan order;
- configurable bounded batch size;
- bounded memory;
- idempotent upsert;
- safe rerun/resume after interruption;
- cancellation support;
- bounded retries using reconciliation workload policy;
- progress and per-batch failure counters;
- mandatory final audit.

Prefer stateless resumability from deterministic scans/point identity; add persistent recovery-session state only if correctness requires it.

## Qdrant audit requirements

Audit must report:

- expected eligible bindings;
- actual points;
- missing/orphan points;
- payload mismatch;
- dense/sparse/version mismatch where practical;
- collection-schema mismatch;
- scroll pages/points/completion status.

The existing Qdrant client exposes bounded scroll states. If scroll is timeout/limit/loop/error/incomplete, final audit MUST NOT report consistency.

Read-only audit must not delete/quarantine orphan points.

## PostgreSQL fingerprint around Qdrant loss

Capture a deterministic read-only fingerprint/counters before deleting Qdrant and compare after rebuild. Cover recovery-relevant document/chunk/embedding/binding/outbox/graph state while excluding normalized operational timestamps.

The proof must show Qdrant loss/rebuild did not rewrite canonical PostgreSQL history unexpectedly.

## Retrieval proof requirements

Frozen query set must include, when supported by the fixture:

1. dense;
2. sparse;
3. hybrid;
4. access-zone/visibility case;
5. no-answer/hard-negative;
6. Graph-enabled query.

Capture baseline, delete only Qdrant collection, rebuild, audit, rerun same queries and compare classification/context identity/order/scores/provenance with documented tolerances.

## Startup/readiness audit

Do not silently change readiness semantics.

Explicitly prove/document that auto-creating a missing empty Qdrant collection is NOT equivalent to restored projection. `collection exists` cannot substitute for FIX491 recovery proof.

## Destructive safety

Default behavior:

```text
missing collection                -> create/rebuild
empty compatible                  -> rebuild
non-empty consistent              -> audit/no-op by default
non-empty drifted                 -> refuse destructive replacement
incompatible                      -> refuse
```

Require explicit `--replace-existing` (or equivalent) for destructive replacement.

Before destructive action log redacted target identities/config/counts and explicit opt-in. Never log secrets.

## Required focused tests

PostgreSQL:

- clean migration bootstrap;
- checksum mismatch;
- unknown/pending migration;
- material drift;
- read-only audit;
- canonical-data integrity.

Qdrant:

- projection-builder parity between outbox/reconciliation/rebuild;
- missing collection recreation;
- collection compatibility/incompatibility;
- payload-index recreation;
- non-empty destructive refusal;
- explicit replace on disposable collection;
- no inference fallback;
- bounded batch behavior;
- interruption + resume/rerun;
- concurrent recovery fence;
- incomplete scroll fail-closed;
- missing/orphan/payload/version mismatch detection;
- before/after retrieval parity;
- PostgreSQL fingerprint unchanged.

Prefer extending existing testcontainers/local-demo and reconciliation helpers rather than creating a parallel framework.

## Safety invariants

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
- outbox correctness/fencing semantics.

If such a change is required, stop with:

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

Generate machine-readable JSON counterparts for final proof counters/verdicts.

Include tested SHA, exact commands, environment/database/collection identities (no secrets), versions, counters, migration/drift classification, canonical integrity, collection compatibility, recovery fence, interruption/resume, scan completeness, PostgreSQL fingerprint, retrieval parity, regression gates and final verdict.

Do not claim PASS for checks not executed.

Final verdict must be one of:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

Do not merge to `main` automatically.