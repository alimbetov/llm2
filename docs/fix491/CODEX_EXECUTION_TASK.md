# FIX491 Codex Execution Task

Work in repository `alimbetov/llm2` on branch `agent/fix491-persistence-recovery`.

Do not work in `main`. Do not create another branch unless objectively required. Do not merge to `main` automatically.

## Mission

Implement FIX491 as a deterministic persistence recovery and proof mechanism for AstraVector:

1. PostgreSQL canonical schema recovery and audit.
2. PostgreSQL canonical-data integrity audit.
3. Qdrant projection compatibility audit, full rebuild, and consistency audit.
4. Proof that loss of Qdrant does not imply loss of canonical knowledge.
5. Proof that PostgreSQL schema is reproducible from repository migrations.
6. Retrieval parity proof before and after complete Qdrant collection loss and rebuild.

The non-negotiable authority model is:

```text
PostgreSQL = canonical state / source of truth
Qdrant     = rebuildable search/vector projection
```

Qdrant must never become authoritative. Do not add a new service.

## Read before coding

Read completely, in this order:

1. `docs/fix491/TECHNICAL_SPECIFICATION.md`
2. `docs/fix491/ACCEPTANCE_CRITERIA.md`
3. this file
4. `docs/fix491/CODEX_VERIFICATION_PROMPT.md` if present
5. `src/outbox/mod.rs`
6. `src/reconciliation/mod.rs`
7. `src/qdrant/mod.rs`
8. `src/recovery/mod.rs`
9. `src/persistence/**` and current persistence code
10. `src/main.rs`
11. `src/bin/astravector-reconciliation.rs` if present
12. `config/**`
13. all `migrations/**`
14. `tests/e2e_testcontainers.rs`
15. existing recovery/reconciliation/local-demo scripts
16. existing FIX489/FIX490 evidence establishing inherited invariants

Before implementation, understand the actual current contracts for PostgreSQL canonical tables, persisted embeddings, vector bindings, vector outbox, Qdrant point construction, collection creation, reconciliation, lifecycle, TTL, legal hold, deletion fencing, sync statuses, payloads, and retrieval filters.

Do not code from this specification alone when the repository establishes stricter semantics.

## Step 0 — Baseline and scope

Before changes capture:

```bash
git status --short --branch
git branch --show-current
git rev-parse HEAD
git merge-base HEAD agent/rest-boundary-readiness-sync
git diff --stat agent/rest-boundary-readiness-sync...HEAD
```

Record baseline SHA and lineage in evidence. If the named FIX490 base is no longer the correct repository lineage, determine and document the actual inherited base rather than fabricating a result.

Do not mix unrelated changes into FIX491.

## Architectural invariants

Normal Qdrant recovery must be possible from persisted PostgreSQL canonical state without:

- source-document reparsing;
- rechunking;
- BGE-M3/ONNX inference;
- regenerating dense or sparse representations;
- changing representation identities.

Do not change:

- CanonicalTokenizer/tokenizer ownership;
- BGE-M3/ONNX ownership;
- chunking or SOURCE/PARENT/SUB hierarchy;
- dense/sparse/hybrid retrieval semantics;
- fusion/RRF;
- no-answer behavior;
- parent hydration;
- GraphRAG;
- MMR;
- token budget;
- final visibility/access-zone/access-level semantics;
- document/version lifecycle semantics;
- TTL/legal-hold semantics;
- canonical PostgreSQL authority;
- outbox correctness/fencing semantics;
- retrieval ranking.

If correct FIX491 implementation objectively requires changing one of these invariants, stop and report:

```text
FIX491_BLOCKED_BY_ARCHITECTURE_CHANGE
```

Do not hide such a dependency inside opportunistic refactoring.

## Step 1 — Eliminate projection divergence first

The current code constructs Qdrant payload/points in normal publication and reconciliation. Do not add a third FIX491-specific builder.

Identify the exact existing projection contract and extract one shared canonical projection builder/function used by:

```text
PostgreSQL canonical projection input
              ↓
   CanonicalProjectionBuilder
              ↓
          QdrantPoint
        /      |       \
       /       |        \
   Outbox  Reconcile   Rebuild
```

Requirements:

- one canonical builder;
- one payload contract;
- one persisted-representation-to-Qdrant mapping;
- outbox, reconciliation, and rebuild all use it;
- existing outbox behavior is the reference unless a concrete defect is demonstrated;
- do not silently change production point semantics.

Add parity tests proving the same canonical input produces a semantically identical `QdrantPoint` through all callers.

## Step 2 — Keep one repair engine

Reuse and extend existing `Reconciler` / `astravector-reconciliation` rather than creating a competing recovery repair subsystem.

Target architecture:

```text
                  Projection Core
                        │
              ┌─────────┴─────────┐
              ▼                   ▼
           Outbox             Reconciler
                                   │
                          ┌────────┴────────┐
                          ▼                 ▼
                    incremental        full rebuild
```

Recovery CLI may orchestrate the existing engine. Do not create parallel `QdrantRepairService`, `RecoveryRepairEngine`, or equivalent unless repository architecture proves this unavoidable.

## Step 3 — Required operator capabilities

Provide scriptable semantics equivalent to:

```text
astravector-runtime migrate
astravector-runtime recovery postgres-audit
astravector-runtime recovery postgres-bootstrap-proof
astravector-runtime recovery qdrant-rebuild
astravector-runtime recovery qdrant-audit
astravector-runtime recovery full-proof
```

Exact CLI spelling may follow existing CLI architecture, but all capabilities must exist and be documented.

Commands must be non-interactive/scriptable, emit a human-readable summary plus a final machine-readable verdict, never log secrets, and return non-zero for failure or blocked states.

`qdrant-rebuild`/`qdrant-audit` may delegate to extended reconciliation. Keep one repair engine.

## Step 4 — PostgreSQL migration recovery

Reuse SQLx migrations in `migrations/`.

`postgres-bootstrap-proof` must start from disposable PostgreSQL 16 + pgvector with no AstraVector schema and:

1. apply the complete repository migration chain;
2. inspect `_sqlx_migrations`;
3. verify known versions and checksums;
4. detect failed, pending, and unknown migration versions;
5. build semantic schema inventory;
6. run canonical-data/schema checks appropriate to an empty bootstrap;
7. fail non-zero on material inconsistency.

Do not equate `cargo sqlx migrate run` with a complete recovery proof.

## Step 5 — PostgreSQL semantic schema inventory

Do not rely only on raw `pg_dump` text diff. Use PostgreSQL catalogs as the primary semantic comparison.

Cover at minimum:

- extensions and versions;
- schemas;
- ordinary and partitioned tables;
- partition keys and children;
- columns and types;
- identity/generation/default expressions;
- nullability;
- PK/FK/UNIQUE/CHECK constraints;
- indexes and partial-index predicates;
- sequences;
- views/materialized views when present;
- functions;
- triggers;
- runtime-material ownership/privileges when required by runtime.

Use deterministic classifications:

```text
NO_DRIFT
BENIGN_DRIFT
MATERIAL_DRIFT
BLOCKED
```

Unknown differences are not silently benign. `MATERIAL_DRIFT` is a failing/non-zero result.

`postgres-audit` must be read-only and must not auto-repair the audited working database.

## Step 6 — PostgreSQL canonical-data integrity

Schema equality is insufficient. Add a read-only canonical-data integrity audit using the real current schema and lifecycle semantics.

Cover relevant relationships, including where present:

- vector bindings → content chunks;
- vector bindings → persisted embedding/cache representation;
- chunks → document versions;
- parent/source chunk relationships;
- access-zone references;
- duplicate logical binding identities;
- invalid lifecycle combinations;
- active/searchable bindings missing required persisted vectors;
- failed/dead outbox rows;
- stale/illegal delete states;
- deletion-fenced or in-progress states;
- orphan canonical relationships.

Do not invent generic integrity rules that contradict current domain semantics. Report deterministic counters and a machine-readable verdict.

## Step 7 — PostgreSQL recovery boundary

Do not imply that migrations restore production data.

Document explicitly:

```text
EMPTY PostgreSQL + repository migrations
= schema recovery

operator-provided PostgreSQL backup/PITR
+ pending migrations
+ postgres-audit
= canonical data disaster recovery
```

Do not implement a backup engine. Document operator backup/PITR assumptions and the RPO/RTO boundary.

## Step 8 — Qdrant collection compatibility

Reuse `QdrantClient::ensure_collection` or extract shared collection configuration logic. Do not duplicate collection schema in a recovery-only implementation.

`collection_exists` is insufficient. Audit at minimum:

- collection name;
- dense vector names/configuration;
- dense dimension;
- distance metric;
- sparse vector configuration;
- required payload indexes and their types;
- Qdrant server/version/config identity available to the client.

Fail closed for incompatible existing collections.

Default destructive policy:

```text
missing collection       -> create and rebuild
empty compatible         -> rebuild
non-empty consistent     -> audit/no-op by default
non-empty drifted        -> refuse destructive replacement
incompatible             -> refuse
```

Destructive replacement requires explicit `--replace-existing` or an equivalent opt-in.

Before destructive action log redacted target identity/config/counts and the explicit opt-in. Never log database passwords, Qdrant API keys, tokens, or connection secrets.

## Step 9 — Qdrant rebuild source

Rebuild only from PostgreSQL canonical bindings/chunks/persisted embeddings/representation metadata.

Reuse persisted dense and sparse vectors. Normal recovery must not call inference.

If an eligible binding requires a persisted representation that is missing, classify and fail with a canonical-integrity/recovery error. Do not silently re-embed because the currently deployed model/tokenizer may differ from the original indexing version.

Preserve binding identities and Qdrant point IDs.

## Step 10 — One eligibility/searchability contract

Recovery must not implement its own handwritten approximation of searchability.

Reuse/extract one eligibility rule for the expected Qdrant projection set across normal projection, reconciliation, rebuild, and audit.

It must preserve current semantics for all relevant state, including:

- document/version lifecycle;
- binding state;
- access zones and access levels;
- expiration/TTL;
- deleted state;
- legal hold;
- representation type/version;
- Qdrant sync semantics;
- current final visibility/searchability rules.

Deleted, expired, inactive, or otherwise non-searchable canonical data must not be rebuilt as active searchable points.

## Step 11 — Preserve outbox history

Do not rewrite historical completed `vector_outbox` rows to `PENDING` merely to force republishing.

Prefer:

```text
PostgreSQL canonical state
        ↓
shared projection builder
        ↓
deterministic direct batched upsert
        ↓
reconciliation / final audit
```

Historical completed outbox rows remain historical completed rows. If recovery-specific operations must be persisted, keep them explicitly identifiable and justify why they are required. Avoid this complexity when deterministic direct rebuild is sufficient.

## Step 12 — Recovery fencing

Full/destructive rebuild must not race:

- the normal outbox publisher;
- another recovery process;
- destructive reconciliation activity.

Implement a detectable guard using the smallest architecture-compatible mechanism, such as PostgreSQL advisory lock, DB-backed recovery lease/generation, or an offline mode with an actual technical check that conflicting publication cannot proceed.

Documentation alone saying "stop the service" is not a sufficient correctness guard when concurrent writes can invalidate the proof.

Add tests proving:

```text
Recovery A holds fence
Recovery B starts
→ rejected or serialized
```

and exercise the relevant rebuild-vs-outbox UPSERT/DELETE conflict path so race corruption cannot be silently accepted.

## Step 13 — Batching, bounded memory, interruption, resume

Do not load the complete projection into memory.

Required properties:

- deterministic stable scan order;
- configurable bounded batch size;
- bounded memory;
- idempotent upsert;
- safe rerun/restart after interruption;
- cancellation support;
- bounded retries using existing reconciliation workload policy where possible;
- progress counters;
- per-batch failure counters;
- mandatory final audit.

Prefer stateless resumability based on stable scans and deterministic point identity. Add persistent recovery-session state only if correctness requires it.

Test interruption after one or more batches followed by restart/rerun and a clean final audit.

## Step 14 — Qdrant full audit

Provide read-only Qdrant audit reporting at minimum:

```text
expected_eligible_bindings
actual_points
missing_points
orphan_points
payload_mismatches
dense_representation_mismatches
sparse_representation_mismatches
representation_version_mismatches
collection_schema_mismatches
pages_scanned
points_scanned
scan_completed
```

Where the Qdrant API cannot economically prove a vector-level property, document the exact limitation rather than claiming it was checked.

The existing bounded scroll states are correctness-significant. If scroll ends due to timeout, page/point limit, loop detection, Qdrant error, cancellation, or any incomplete condition, final audit must not report consistency.

Only a complete scan may produce a consistent verdict.

Read-only audit must not delete or quarantine orphan points.

Suggested machine verdicts include:

```text
QDRANT_PROJECTION_CONSISTENT
QDRANT_PROJECTION_DRIFT
QDRANT_REBUILD_FAILED
QDRANT_AUDIT_INCOMPLETE
```

## Step 15 — PostgreSQL fingerprint around Qdrant loss

Capture a deterministic read-only fingerprint/counters before deleting Qdrant and compare after loss and after rebuild.

Cover recovery-relevant canonical semantic state, including where present:

- document versions;
- chunks;
- embedding cache/representations;
- vector bindings;
- outbox semantic history;
- graph canonical state;
- access zones;
- lifecycle state.

Exclude only explicitly documented operational timestamps/counters that recovery is legitimately allowed to change.

Proof sequence:

```text
PG fingerprint A
      ↓
DELETE ONLY Qdrant collection
      ↓
PG fingerprint B
      ↓
Qdrant rebuild
      ↓
PG fingerprint C
```

For canonical semantic state require:

```text
A == B == C
```

Unexpected canonical PostgreSQL history mutation is a failure.

## Step 16 — Startup/readiness corner case

Explicitly investigate `qdrant.auto_create_collection=true` or equivalent behavior.

A missing collection followed by runtime auto-creation can produce an existing but empty collection. Therefore:

```text
collection exists != projection restored
```

Do not silently redesign readiness semantics. Prove and document current behavior. If current readiness can report healthy/ready while canonical eligible bindings exist and Qdrant is an empty auto-created projection, record a concrete correctness finding and make only the smallest FIX491-local change necessary for a safe recovery contract.

## Step 17 — Full disaster proof

Run an isolated proof on disposable infrastructure:

```text
EMPTY PostgreSQL
      ↓
all migrations
      ↓
schema audit
      ↓
ingestion fixture
      ↓
PostgreSQL canonical state
      ↓
Qdrant projection
      ↓
activation
      ↓
retrieval baseline
      ↓
PostgreSQL fingerprint A
      ↓
DELETE ONLY Qdrant collection
      ↓
PostgreSQL fingerprint B
      ↓
Qdrant rebuild from PostgreSQL persisted state
      ↓
PostgreSQL fingerprint C
      ↓
Qdrant full audit
      ↓
retrieval parity
      ↓
final persistence verdict
```

Prefer extending `tests/e2e_testcontainers.rs`, existing local-demo infrastructure, and reconciliation helpers. Do not create a parallel integration framework without necessity.

## Step 18 — Retrieval parity bank

Use a frozen before/after query set. When supported by the fixture include:

1. dense;
2. sparse;
3. hybrid;
4. access-zone/visibility case;
5. no-answer/hard-negative;
6. Graph-enabled query.

Compare the semantic result, not merely HTTP/command success. Include relevant fields such as:

- classification / FOUND / no-answer / degraded state;
- context count and order;
- document and version identity;
- matched/parent chunk identity;
- access-zone identity;
- matched and parent text identity where appropriate;
- representation identity;
- dense/sparse/fusion/final scores;
- Graph provenance;
- degradation codes.

Document minimal deterministic floating-point tolerances and use the same tolerances before and after recovery.

## Step 19 — Required focused negative and recovery tests

PostgreSQL coverage:

- clean migration bootstrap;
- checksum mismatch;
- unknown migration;
- pending migration;
- material column drift;
- material index drift;
- partition drift where partitioning exists;
- read-only audit behavior;
- canonical-data relationship corruption.

Qdrant coverage:

- shared projection-builder parity across outbox/reconciliation/rebuild;
- missing collection recreation;
- compatible empty collection;
- incompatible collection;
- wrong dense dimension/config;
- missing sparse configuration;
- missing/wrong payload index;
- non-empty destructive refusal;
- explicit replacement on disposable collection;
- missing canonical vector;
- proof that inference fallback is not used;
- bounded batch behavior;
- interruption and restart/rerun;
- concurrent recovery fencing;
- rebuild/outbox concurrency safety;
- incomplete scroll fail-closed;
- missing expected point detection;
- orphan point detection;
- payload mismatch detection;
- representation/version mismatch detection where practical;
- PostgreSQL fingerprint unchanged;
- before/after retrieval parity.

Prefer extending existing tests and helpers rather than building a second framework.

## Step 20 — Mandatory regression gates

Run:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Run all new FIX491 focused integration/proof tests as well.

Do not rerun unrelated long-duration FIX489 soak tests merely because persistence recovery changed, unless FIX491 changes retrieval semantics or the repository's verification contract explicitly requires them.

Do not claim PASS for a gate that was not executed.

## Step 21 — Evidence and runbook

Create/update:

```text
docs/fix491/RECOVERY_RUNBOOK.md
docs/fix491/POSTGRES_RECOVERY_RESULT.md
docs/fix491/QDRANT_RECOVERY_RESULT.md
docs/fix491/PERSISTENCE_RECOVERY_RESULT.md
```

Generate machine-readable JSON counterparts for final proof counters/verdicts.

Evidence must include, as applicable:

- repository and branch;
- baseline/base SHA;
- tested SHA;
- exact commands;
- git diff scope;
- environment identities without secrets;
- PostgreSQL server version;
- pgvector version;
- migration count/history/checksum verdict;
- schema drift classification;
- canonical integrity counters;
- Qdrant server version;
- collection/config compatibility;
- expected and actual point counts;
- missing/orphan/mismatch counters;
- scan completion state;
- recovery fence result;
- batch/retry/interruption/resume counters;
- PostgreSQL fingerprints;
- retrieval parity results and tolerances;
- regression gate results;
- discovered defects/limitations;
- final verdict.

Evidence must distinguish `NOT_RUN`, `BLOCKED`, and `FAIL`; never convert them to PASS.

## Final acceptance gate

The only top-level final verdicts are:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

`FIX491_PERSISTENCE_RECOVERY_PASS` is permitted only when all applicable required conditions are proven, including:

```text
PostgreSQL clean bootstrap              PASS
SQLx migration history/checksums        PASS
PostgreSQL MATERIAL_DRIFT               0
PostgreSQL canonical integrity          PASS

Qdrant collection compatibility         PASS
Qdrant full rebuild                     PASS
Qdrant scan completion                  PASS
missing eligible points                 0
orphan Qdrant points                    0
payload mismatch                        0
required representation mismatch        0

PostgreSQL canonical fingerprint        unchanged
recovery fencing                        PASS
interruption/resume                     PASS
retrieval before/after parity           PASS

cargo fmt                               PASS
cargo check --locked                    PASS
cargo clippy --locked                   PASS
cargo test --locked                     PASS
required integration/FIX491 tests       PASS
```

## Fix policy

You may change production code required for FIX491. Prefer minimal architecture-preserving extraction and reuse.

Do not perform generic refactoring, unrelated renaming, retrieval rewrites, model/chunking/ranking changes, frozen-evidence rewriting, or dependency upgrades without necessity.

If an existing defect directly blocks FIX491, fix it minimally and add a regression test. If fixing it requires an out-of-scope architecture change, return BLOCKED rather than bypassing the invariant.

## Commit and push

After successful implementation and verification:

1. inspect `git status` and final diff;
2. commit only FIX491 changes;
3. push to `agent/fix491-persistence-recovery`;
4. do not merge to `main`;
5. report final local HEAD SHA;
6. verify and report remote branch SHA equals local HEAD.

Final Codex response must report:

1. final FIX491 verdict;
2. local HEAD SHA;
3. remote branch SHA;
4. changed production files;
5. added/changed tests;
6. PostgreSQL recovery/audit result;
7. Qdrant rebuild/audit result;
8. retrieval parity result;
9. regression gates;
10. discovered defects/limitations;
11. remaining blocked items, if any.
