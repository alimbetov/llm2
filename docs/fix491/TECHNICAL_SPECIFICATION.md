# FIX491 — PostgreSQL Canonical Recovery + Qdrant Projection Rebuild

## 1. Goal

Implement and prove a deterministic recovery path for AstraVector persistence without changing retrieval, chunking, embedding, ranking, GraphRAG, MMR, access-zone, lifecycle, or outbox semantics.

FIX491 has two recovery responsibilities with different authority models:

```text
PostgreSQL = canonical state
Qdrant     = rebuildable vector/search projection
```

The implementation MUST preserve this asymmetry.

This is a recovery and reproducibility task, not a storage redesign.

## 2. Scope

### 2.1 PostgreSQL recovery

Provide an explicit, operator-invokable mechanism that can:

1. bootstrap an empty PostgreSQL database from repository migrations;
2. verify migration history and migration checksums;
3. verify the required `astravector` schema objects exist after bootstrap;
4. verify a migrated clean database is structurally equivalent to the expected repository schema;
5. audit an existing working PostgreSQL database for schema drift versus a fresh migration-built database;
6. fail closed on unknown/failed migration history or material schema drift;
7. produce machine-readable and human-readable evidence;
8. validate restored canonical data after operator-provided backup/PITR restore.

This mechanism is about schema/canonical-state recoverability. It MUST NOT fabricate lost production rows from migrations.

```text
empty DB + migrations     -> schema recovery
backup/PITR + migrations  -> canonical data recovery
```

### 2.2 Qdrant recovery

Provide an explicit, operator-invokable mechanism that can rebuild an empty or lost Qdrant collection from PostgreSQL canonical state.

The rebuild MUST derive searchable projection data only from PostgreSQL and existing AstraVector representation metadata/embeddings. Qdrant is not allowed to become a second source of truth.

The implementation MUST support:

1. detecting a missing target collection;
2. creating the collection with the exact expected vector/sparse/index configuration;
3. rebuilding eligible active bindings/points from PostgreSQL;
4. restoring required payload fields and payload indexes;
5. excluding deleted/expired/inactive/illegal projection rows according to current lifecycle semantics;
6. reconciling PostgreSQL bindings/outbox state with rebuilt Qdrant state without corrupting canonical history;
7. producing a consistency report;
8. proving retrieval parity before and after Qdrant destruction/rebuild;
9. interruption-safe/resumable rebuild;
10. bounded batch processing with cancellation, progress counters and failure reporting.

## 3. Existing implementation that FIX491 MUST reuse

The current project already has important recovery-related building blocks. FIX491 MUST extend/reuse them instead of adding parallel implementations:

- `Repository` / PostgreSQL canonical persistence;
- SQLx migrations in `migrations/`;
- `QdrantClient` collection, point and payload-index operations;
- `outbox` publisher;
- `Reconciler` and `astravector-reconciliation` binary;
- lifecycle/TTL/delete fencing;
- existing testcontainers and local-demo E2E paths.

### 3.1 Critical projection-builder finding

Current code constructs Qdrant points/payloads independently in at least:

```text
outbox::process_event
reconciliation::reconcile_binding
```

These paths are required to represent the same retrieval payload contract, but maintaining multiple handwritten builders creates drift risk. FIX491 MUST NOT create a third recovery-only payload builder.

Before implementing rebuild, extract or introduce one transport-neutral/caller-neutral canonical projection builder used by:

```text
outbox publisher
reconciliation repair
FIX491 full rebuild
```

Conceptually:

```text
PostgreSQL canonical binding + persisted embedding
                 |
                 v
       CanonicalProjectionBuilder
                 |
                 v
             QdrantPoint
        vectors + canonical payload
```

This extraction is permitted because it removes duplication without changing semantics. Existing outbox behavior is the reference contract unless a concrete defect is proven.

If this cannot be done without changing protected retrieval/lifecycle semantics, stop with `FIX491_BLOCKED_BY_ARCHITECTURE_CHANGE`.

## 4. Non-goals

FIX491 MUST NOT:

- redesign PostgreSQL schema;
- rewrite existing migrations unless a concrete defect is proven;
- introduce an ORM;
- make Qdrant authoritative;
- add a new persistence microservice;
- change embeddings, tokenizer, chunking, retrieval profiles, fusion/RRF, GraphRAG, MMR, token budget, lifecycle, access-zone semantics or final visibility;
- add Kafka/event infrastructure;
- add a new backup product;
- require Qdrant snapshots for correctness recovery;
- reparse documents through AstraIndexator;
- regenerate embeddings merely because Qdrant was lost;
- create a second reconciliation/rebuild engine beside the existing reconciler.

Qdrant snapshots may remain an optional RTO optimization, not the correctness source.

## 5. Required operator entry points

Implement explicit CLI/runtime capabilities. Exact Rust CLI organization is implementation detail, but all semantics below MUST exist and be scriptable/non-interactive:

```text
astravector-runtime migrate
astravector-runtime recovery postgres-audit
astravector-runtime recovery postgres-bootstrap-proof
astravector-runtime recovery qdrant-rebuild
astravector-runtime recovery qdrant-audit
astravector-runtime recovery full-proof
```

It is also acceptable to extend `astravector-reconciliation` for the Qdrant scan/repair engine and make `astravector-runtime recovery ...` a thin orchestrator. There MUST NOT be two competing repair implementations.

Every command MUST return non-zero on failure and emit a final machine-readable verdict.

## 6. PostgreSQL migration-history proof

The audit MUST inspect SQLx `_sqlx_migrations` and compare it with repository migrations.

Minimum checks:

- every repository migration expected at tested HEAD is present;
- all applied migrations are successful;
- no unknown applied migration version exists;
- checksums match repository/SQLx migration checksums;
- ordering is valid;
- no pending migration remains for a database claimed as current;
- migration table itself is readable and structurally expected.

The report MUST never treat presence of a version number alone as proof.

## 7. PostgreSQL fresh-bootstrap proof

Use PostgreSQL 16 with pgvector, matching current integration-test assumptions.

The proof MUST start from a database with no AstraVector schema objects and apply the complete `migrations/` chain.

After migration, verify at least:

- schema `astravector`;
- extension `vector` and extension version compatibility;
- parent and partition tables;
- partition keys and child partitions;
- columns, types, nullability and defaults;
- generated/identity attributes where present;
- PK/FK/UNIQUE/CHECK constraints;
- indexes including partial indexes and predicates;
- sequences/functions/triggers/views/materialized views if any;
- required runtime tables used by current code, not only tutorial tables;
- schema/object ownership assumptions required at runtime, where material;
- required privileges for the configured runtime user, where material.

Do not hard-code only a small table allow-list. Build inventory from PostgreSQL catalogs and repository migrations.

## 8. PostgreSQL schema-drift comparison

Implement deterministic structural comparison between:

```text
A = existing working/current PostgreSQL
B = clean PostgreSQL produced only by repository migrations at the tested SHA
```

Comparison MUST be semantic/catalog based where possible. Raw `pg_dump --schema-only` diff may be secondary evidence but MUST NOT be the only comparator.

Normalize/compare at least:

- extensions and versions;
- schemas;
- tables and partitioned tables;
- columns/types/defaults/nullability/identity;
- partition keys and partition children;
- constraints;
- indexes and index predicates;
- sequences;
- views/functions/triggers if present;
- runtime-relevant ownership/privilege differences.

Classification:

```text
NO_DRIFT
BENIGN_DRIFT
MATERIAL_DRIFT
BLOCKED
```

The implementation MUST document exact rules for BENIGN vs MATERIAL. Unknown differences default to MATERIAL or BLOCKED, never silently BENIGN.

`MATERIAL_DRIFT` MUST fail the proof. Audit mode MUST be strictly read-only against the audited working DB.

## 9. PostgreSQL canonical-data integrity after restore

Schema equality is insufficient after backup/PITR restore. Add read-only integrity checks for canonical rows used by runtime.

At minimum validate current invariants already represented in project audits/retrieval paths, including where applicable:

- bindings reference existing chunks/documents/cache entries;
- parent/source chunk references are valid;
- searchable bindings are unique by canonical logical identity;
- active/searchable chunks have valid document versions;
- completed/synced projection state is internally consistent;
- access zones referenced by searchable data exist and have valid status;
- invalid lifecycle combinations are reported;
- failed/dead outbox rows are counted and surfaced;
- deletion in-progress/fenced states are surfaced rather than silently repaired.

Do not mutate canonical data during `postgres-audit`.

## 10. PostgreSQL data-loss boundary and runbook

FIX491 MUST explicitly document that migrations do not restore lost canonical rows.

Required operator path:

```text
new PostgreSQL instance
-> operator restores backup/PITR
-> run pending repository migrations
-> run postgres-audit
-> run canonical-data integrity checks
-> only then permit recovery continuation/Qdrant rebuild
```

Do not implement a vendor-specific backup engine in AstraVector.

For production documentation, distinguish:

```text
RPO = determined by PostgreSQL backup/PITR policy
RTO = restore + migrate + audit + Qdrant projection rebuild/snapshot restore
```

FIX491 tests need not implement a production backup system, but the runbook MUST define the boundary and verification steps.

## 11. Qdrant rebuild source of truth

Rebuild MUST use PostgreSQL canonical state and existing stored representations. It MUST NOT require re-parsing source documents or calling AstraIndexator.

Where dense/sparse vectors are persisted in PostgreSQL, reuse those exact persisted representations. Do not call inference as fallback in normal recovery.

If an eligible binding lacks the persisted representation required to rebuild its point, classify it as canonical inconsistency and fail/partial-block the rebuild according to documented policy; do not silently re-embed.

Preserve relevant identity/version fields already stored by AstraVector, including current model/tokenizer/dense/sparse/payload/chunking versions.

## 12. Canonical Qdrant projection builder

Introduce one shared code path that builds a Qdrant point from canonical PostgreSQL state.

The builder MUST own canonical construction of:

- point ID;
- dense vector;
- sparse indices/values;
- access-zone fields;
- binding/point identity;
- document/version identity;
- root/source/parent/chunk identity;
- source/provenance fields currently expected in payload;
- chunk granularity;
- representation type;
- access level;
- lifecycle status;
- expiry fields;
- legal-hold payload where currently part of contract;
- payload version;
- model/tokenizer/dense/sparse/chunking versions;
- quality/debug payload fields that are part of current projection contract;
- quarantine flag/status semantics.

Outbox, reconciliation and full rebuild MUST use this same builder or an equally strong shared function. Add unit/integration tests that compare generated payloads across all three callers.

## 13. Qdrant collection recreation and schema validation

Collection creation MUST go through existing `QdrantClient::ensure_collection` semantics or a shared implementation extracted from it.

Do not duplicate collection configuration in scripts.

Validate both creation and an already-existing collection:

- collection name;
- dense dimension;
- dense vector/name configuration;
- distance metric;
- sparse vector configuration;
- payload indexes including expected type per field;
- any current optimizer/on-disk/vector configuration that materially affects compatibility;
- Qdrant server version recorded in evidence.

Important: `collection_exists=true` is NOT compatibility proof. Add an explicit collection-schema compatibility audit.

If collection exists but is incompatible, fail closed by default.

## 14. Destructive safety and target identity

`qdrant-rebuild` MUST NOT silently delete an existing collection.

Required behavior:

```text
collection missing                    -> create and rebuild
collection empty + compatible         -> rebuild
collection non-empty + consistent     -> audit/no-op unless explicitly requested
collection non-empty + drift          -> refuse destructive replacement by default
collection incompatible               -> refuse by default
```

For intentionally destructive replacement require explicit opt-in such as:

```text
--replace-existing
```

Before destructive action log and include in evidence:

- tested Git SHA;
- PostgreSQL host/database identity (redacted credentials);
- PostgreSQL server version;
- Qdrant URL identity (redacted credentials);
- collection name;
- Qdrant server version;
- current collection point count/config hash;
- expected eligible point count;
- explicit replace flag.

Never log passwords/API keys/tokens.

## 15. Searchable eligibility MUST be canonical and shared

Rebuild eligibility MUST reuse/extract current canonical lifecycle/searchability rules rather than copy them into a new handwritten SQL rule set.

The same effective rule must govern:

```text
normal projection
reconciliation
full rebuild
qdrant audit expected-set calculation
```

At minimum account for:

- binding lifecycle;
- document lifecycle/status;
- chunk lifecycle/deleted/expiry state;
- Qdrant sync/deletion state where relevant;
- TTL/expiry;
- legal hold semantics;
- representation type;
- current access-zone validity where part of searchable eligibility.

Legal hold MUST NOT be simplified to “always searchable”. Preserve current lifecycle/delete semantics exactly.

## 16. Outbox, binding and recovery-generation semantics

Rebuild MUST not falsify historical outbox completion and MUST not rewrite completed events as if they had just run.

Preferred direction based on current code:

```text
canonical PostgreSQL scan
-> shared projection builder
-> direct deterministic Qdrant upsert in bounded batches
-> audit/reconciliation
```

Historical outbox rows remain historical.

However, direct rebuild introduces concurrency risk with the normal publisher. FIX491 MUST therefore implement a recovery fence.

At minimum one of these safe designs is required:

1. offline operator mode requiring normal runtime/outbox publisher stopped; or
2. explicit DB-backed recovery lease/lock/generation that excludes conflicting projection writes; or
3. another proven fencing mechanism compatible with current outbox fencing.

Do NOT run destructive/full rebuild concurrently with normal publisher without a proven fence.

Document the selected strategy and add a test showing conflicting concurrent rebuild is rejected or safely serialized.

Preserve canonical binding IDs and Qdrant point IDs.

## 17. Qdrant rebuild resumability and batching

Production rebuild may involve many points. It MUST NOT require loading all canonical vectors/points into memory.

Required properties:

- deterministic stable scan order;
- bounded batch size configurable with a safe default;
- bounded memory use;
- progress counters/checkpoint information;
- idempotent point upsert;
- safe restart after interruption;
- cancellation support;
- retry policy compatible with current reconciliation workload;
- per-batch failure accounting;
- no unbounded retry loop;
- final audit required after resume/completion.

A persistent recovery-session table is NOT mandatory unless required for correctness. Prefer deriving resumability from idempotent deterministic scans and Qdrant point identity where possible.

## 18. Qdrant consistency audit

Produce machine-readable JSON and Markdown evidence containing at least:

- PostgreSQL database identity;
- PostgreSQL server/pgvector versions;
- Qdrant collection/server identity;
- tested Git SHA;
- expected eligible bindings;
- actual points;
- missing points;
- orphan points;
- payload mismatches;
- dense representation mismatches where practical;
- sparse representation mismatches where practical;
- document/version/access-zone/access-level/lifecycle mismatches;
- representation-version mismatches;
- collection-schema mismatch count/details;
- incomplete/limit/timeout status for any Qdrant scroll;
- scan pages/points read;
- final verdict.

Qdrant audit MUST fail closed if its scroll is incomplete, timed out, loop-detected or exceeded configured limits. A partial scan cannot produce `CONSISTENT`.

Required verdicts:

```text
QDRANT_PROJECTION_CONSISTENT
QDRANT_PROJECTION_DRIFT
QDRANT_AUDIT_INCOMPLETE
QDRANT_REBUILD_FAILED
```

## 19. Qdrant orphan policy

After a clean destructive rebuild, orphan points MUST be zero.

For non-destructive audit/repair against an existing collection, discovered Qdrant points without canonical PostgreSQL ownership MUST be classified explicitly. Do not automatically delete or quarantine them during read-only audit.

Any repair/quarantine/delete mode must reuse current reconciliation safety rules and require explicit operator intent where destructive.

## 20. Retrieval parity proof

Required scenario:

```text
1. start from known-good PostgreSQL + Qdrant
2. execute frozen retrieval query set and save baseline
3. capture PostgreSQL canonical fingerprint/counters
4. destroy only the target Qdrant collection
5. prove PostgreSQL fingerprint/counters unchanged
6. run qdrant-rebuild
7. run qdrant-audit
8. rerun identical retrieval query set
9. compare results
```

Compare at least:

- response classification (`FOUND`/no-answer/degraded as applicable);
- context count;
- order where deterministic;
- document_id;
- document_version;
- matched_chunk_id;
- parent_chunk_id;
- access_zone_id;
- matched/parent text identity;
- representation identity;
- scores with documented floating-point tolerance where exact equality is not guaranteed;
- Graph provenance/degradation where relevant.

The query set MUST contain at least:

- dense retrieval;
- sparse retrieval when supported by fixture;
- hybrid retrieval when supported;
- access-zone/visibility case;
- no-answer/hard-negative case;
- Graph-enabled case if Graph is enabled in the tested profile.

## 21. PostgreSQL canonical fingerprint around Qdrant loss

Before deleting Qdrant, capture a deterministic read-only fingerprint over recovery-relevant PostgreSQL state. After deletion and rebuild, confirm canonical PostgreSQL state was not unintentionally rewritten.

Fingerprint SHOULD cover stable canonical identities/counts/hashes for:

- document versions;
- chunks;
- embedding/cache representation rows;
- vector bindings;
- outbox historical rows/statuses relevant to the fixture;
- graph state where retrieval proof depends on it.

Exclude legitimately mutable operational timestamps/metrics from strict equality, or normalize them explicitly.

## 22. Fresh total-persistence proof

`full-proof` MUST exercise the complete persistence chain on isolated disposable infrastructure:

```text
EMPTY PostgreSQL
    -> all migrations
    -> schema/migration audit
    -> runtime/ingestion fixture
    -> canonical PostgreSQL state
    -> Qdrant projection
    -> activation/retrieval baseline
    -> canonical PostgreSQL fingerprint
    -> destroy Qdrant collection
    -> rebuild Qdrant from PostgreSQL
    -> qdrant audit
    -> retrieval parity
    -> PostgreSQL fingerprint unchanged
    -> PostgreSQL schema/data-integrity audit
```

Prefer extending existing testcontainers/local-demo infrastructure rather than creating a second test framework.

## 23. Startup/readiness behavior

FIX491 must document and test expected runtime behavior for persistence loss:

- PostgreSQL required and unavailable -> existing fail/degraded startup semantics remain unchanged;
- required Qdrant collection missing and auto-create enabled -> collection may be created but readiness MUST NOT imply projection is restored merely because collection exists;
- an empty newly-created collection with canonical PostgreSQL data is not equivalent to a recovered service;
- operator recovery/reconciliation must restore projection before a recovery PASS is claimed.

Do not silently change global readiness semantics in FIX491 unless a concrete correctness bug requires it. If a readiness change is proposed, isolate and justify it explicitly.

## 24. Evidence files

Required checked-in documents:

```text
docs/fix491/TECHNICAL_SPECIFICATION.md
docs/fix491/ACCEPTANCE_CRITERIA.md
docs/fix491/CODEX_EXECUTION_TASK.md
docs/fix491/CODEX_VERIFICATION_PROMPT.md
docs/fix491/RECOVERY_RUNBOOK.md
```

Expected generated summaries:

```text
docs/fix491/POSTGRES_RECOVERY_RESULT.md
docs/fix491/QDRANT_RECOVERY_RESULT.md
docs/fix491/PERSISTENCE_RECOVERY_RESULT.md
```

Machine-readable JSON counterparts MUST be generated for final proof counters/verdicts. Raw/high-volume evidence may remain outside Git.

## 25. Required tests

At minimum add focused tests for:

### PostgreSQL

- full clean migration chain;
- migration checksum mismatch;
- unknown migration version;
- pending migration detection;
- material schema drift;
- benign drift classification if supported;
- partition/index/constraint inventory;
- audit read-only behavior;
- canonical row integrity checks.

### Qdrant

- shared projection builder parity for outbox/reconciliation/rebuild;
- missing collection recreation;
- collection schema compatibility audit;
- payload-index recreation;
- empty compatible rebuild;
- incompatible collection refusal;
- non-empty destructive refusal;
- explicit replace path on disposable collection;
- persisted dense/sparse reuse without inference;
- missing canonical embedding causes failure/classification, not re-embedding;
- bounded batched rebuild;
- interrupted rebuild + resume/idempotent rerun;
- concurrent rebuild fence;
- incomplete Qdrant scroll cannot return consistent;
- missing/orphan/payload/version drift detection;
- before/after retrieval parity;
- PostgreSQL canonical fingerprint unchanged.

## 26. Required regression gates

At minimum:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Add focused FIX491 integration tests on disposable PostgreSQL/Qdrant.

Do not rerun unrelated long soak campaigns unless FIX491 changes runtime retrieval semantics, which it should not.

## 27. Safety invariants

The implementation MUST NOT change these already proven invariants:

- PostgreSQL remains canonical;
- Qdrant remains rebuildable projection;
- tokenizer/BGE-M3/chunking ownership remains in AstraVector;
- dense/sparse/hybrid semantics unchanged;
- parent hydration unchanged;
- GraphRAG/MMR/token-budget/final visibility unchanged;
- access-zone/access-level/lifecycle/version rules unchanged;
- outbox/reconciliation safety is not weakened;
- FIX489 and FIX490 evidence remains inherited unless relevant code is modified.

If implementation requires changing any of these, stop with:

```text
FIX491_BLOCKED_BY_ARCHITECTURE_CHANGE
```

## 28. Final acceptance verdict

Only these final verdicts are allowed:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

PASS requires all of the following:

```text
PostgreSQL fresh bootstrap                 PASS
SQLx migration history/checksums           PASS
PostgreSQL material schema drift           0
PostgreSQL canonical-data integrity        PASS
PostgreSQL audit is read-only               PASS
Qdrant collection-schema compatibility     PASS
Qdrant clean collection rebuild             PASS
Qdrant projection drift                     0
Qdrant orphan points                        0
Qdrant missing eligible points              0
Qdrant audit completeness                   PASS
Shared projection-builder parity            PASS
Persisted representation reuse              PASS
Recovery interruption/resume                PASS
Recovery concurrency fence                  PASS
PostgreSQL fingerprint unchanged            PASS
Retrieval before/after rebuild parity       PASS
Regression gates                            PASS
```

A PASS does not claim that migrations can restore deleted PostgreSQL business data. Production canonical-data disaster recovery still requires a verified PostgreSQL backup/PITR source.