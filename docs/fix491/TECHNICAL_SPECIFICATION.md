# FIX491 — PostgreSQL Canonical Recovery + Qdrant Projection Rebuild

## 1. Goal

Implement and prove a deterministic recovery path for AstraVector persistence without changing retrieval, chunking, embedding, ranking, GraphRAG, MMR, access-zone, lifecycle, or outbox semantics.

FIX491 has two recovery responsibilities with different authority models:

```text
PostgreSQL = canonical state
Qdrant     = rebuildable vector/search projection
```

The implementation MUST preserve this asymmetry.

## 2. Scope

### 2.1 PostgreSQL recovery

Provide an explicit, operator-invokable mechanism that can:

1. bootstrap an empty PostgreSQL database from repository migrations;
2. verify migration history and migration checksums;
3. verify the required `astravector` schema objects exist after bootstrap;
4. verify a migrated clean database is structurally equivalent to the expected repository schema;
5. audit an existing working PostgreSQL database for schema drift versus a fresh migration-built database;
6. fail closed on unknown/failed migration history or material schema drift;
7. produce machine-readable and human-readable evidence.

This mechanism is about schema/canonical-state recoverability. It MUST NOT fabricate lost production rows from migrations. Recovery of canonical data after physical data loss requires a PostgreSQL backup/PITR restore source. FIX491 MUST document and test the boundary:

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
8. proving retrieval parity before and after Qdrant destruction/rebuild.

## 3. Non-goals

FIX491 MUST NOT:

- redesign PostgreSQL schema;
- rewrite existing migrations unless a concrete defect is proven;
- introduce an ORM;
- make Qdrant authoritative;
- add a new persistence microservice;
- change embeddings, tokenizer, chunking, retrieval profiles, fusion/RRF, GraphRAG, MMR, token budget, lifecycle, access-zone semantics or final visibility;
- add Kafka/event infrastructure;
- add a new backup product;
- require Qdrant snapshots for correctness recovery.

Qdrant snapshots may remain an optional RTO optimization, not the correctness source.

## 4. Required operator entry points

Implement explicit CLI/runtime commands. Exact Rust organization is implementation detail, but externally the following capabilities MUST exist:

```text
astravector-runtime migrate
astravector-runtime recovery postgres-audit
astravector-runtime recovery postgres-bootstrap-proof
astravector-runtime recovery qdrant-rebuild
astravector-runtime recovery qdrant-audit
astravector-runtime recovery full-proof
```

If preserving the existing CLI parser requires equivalent command names, document them and keep the semantic surface equally small.

Commands MUST be non-interactive, scriptable and return non-zero on failure.

## 5. PostgreSQL migration-history proof

The audit MUST inspect SQLx `_sqlx_migrations` and compare it with repository migrations.

Minimum checks:

- every repository migration expected at HEAD is present;
- all applied migrations are successful;
- no unknown applied migration version exists;
- checksums match SQLx/repository migration checksums;
- ordering is valid;
- no pending migration remains for a database claimed as current.

The report MUST never treat presence of a version number alone as proof.

## 6. PostgreSQL fresh-bootstrap proof

Use PostgreSQL 16 with pgvector, matching current integration-test assumptions.

The proof MUST start from a database with no `astravector` schema objects and apply the complete `migrations/` chain.

After migration, verify at least:

- schema `astravector`;
- extension `vector` where required;
- parent and partition tables;
- partition keys and child partitions;
- columns, types, nullability and defaults;
- PK/FK/UNIQUE/CHECK constraints;
- indexes including partial indexes;
- required sequences/functions/triggers if any;
- required tables used by runtime, including current canonical document/chunk/embedding/binding/outbox/access-zone/graph state.

Do not hard-code only the four tables used by the local tutorial. Build the inventory from repository migrations and/or PostgreSQL catalogs.

## 7. Schema-drift comparison

Implement deterministic structural comparison between:

```text
A = existing working/current PostgreSQL
B = clean PostgreSQL produced only by repository migrations at the tested SHA
```

Comparison MUST be semantic/catalog based where possible. A raw `pg_dump diff` may be retained as secondary evidence but MUST NOT be the only comparator because dump ordering and environment-specific noise can produce false differences.

Normalize or catalog-compare at least:

- extensions;
- schemas;
- tables and partitioned tables;
- columns/types/defaults/nullability;
- partition keys and partition children;
- constraints;
- indexes and index predicates;
- sequences;
- views/functions/triggers if present.

Classification:

```text
NO_DRIFT
BENIGN_DRIFT
MATERIAL_DRIFT
BLOCKED
```

`MATERIAL_DRIFT` MUST fail the proof. Do not auto-repair an existing working database during an audit.

## 8. PostgreSQL data-loss boundary

FIX491 MUST explicitly document that migrations do not restore lost canonical rows.

Implement a proof/runbook path for:

```text
new PostgreSQL instance
-> restore operator-provided backup/PITR result
-> run pending repository migrations
-> run postgres-audit
-> run integrity checks
-> allow AstraVector startup
```

Do not implement a vendor-specific backup engine in AstraVector.

## 9. Qdrant rebuild source

The rebuild MUST use PostgreSQL canonical state and existing stored representations. It MUST NOT require re-parsing source documents or calling AstraIndexator.

Where dense/sparse vectors are already persisted in PostgreSQL, reuse those persisted representations rather than regenerating them through a potentially different model/tokenizer version.

The rebuild MUST respect representation identity fields already stored by AstraVector, including current relevant model/tokenizer/dense/sparse/payload/chunking versions.

## 10. Qdrant collection recreation

Collection creation MUST go through the existing AstraVector Qdrant configuration/`ensure_collection` semantics or a shared implementation extracted from it.

Do not duplicate collection configuration in an unrelated script.

Validate:

- dense dimension;
- dense vector/name configuration;
- sparse configuration;
- distance metric;
- payload indexes;
- collection name;
- versioned payload fields required by retrieval/filtering.

If an existing collection has incompatible configuration, rebuild MUST fail closed unless an explicit destructive flag is supplied.

## 11. Destructive safety

`qdrant-rebuild` MUST NOT silently delete an existing collection.

Required behavior:

```text
collection missing -> create and rebuild
collection empty and compatible -> rebuild
collection non-empty -> audit first; refuse destructive replacement by default
```

For intentionally destructive replacement require an explicit option such as:

```text
--replace-existing
```

and log collection identity and target PostgreSQL database identity before action.

## 12. Searchable eligibility

Rebuild eligibility MUST reuse current canonical lifecycle/visibility semantics. Do not create a second handwritten rule set that may diverge from retrieval.

At minimum exclude objects that are not expected to be searchable due to current canonical state, including deleted/expired/non-active rows and invalid bindings according to current implementation.

Legal-hold behavior must remain consistent with current deletion/reconciliation semantics.

## 13. Outbox and binding semantics

The rebuild MUST not falsify historical outbox completion.

Choose and document one safe strategy based on existing code:

- direct deterministic projection rebuild from canonical bindings/embeddings followed by reconciliation; or
- generation of new recovery-specific outbox operations without rewriting historical completed events.

The selected implementation MUST preserve binding IDs / Qdrant point IDs where those IDs are canonical and already persisted.

After rebuild:

```text
expected searchable PostgreSQL bindings == Qdrant projected points
or every difference is explicitly classified and justified
```

No orphan Qdrant point may remain after a clean rebuild proof.

## 14. Qdrant consistency audit

Produce machine-readable JSON and Markdown evidence containing at least:

- PostgreSQL database identity;
- Qdrant collection identity;
- tested Git SHA;
- expected searchable bindings;
- actual Qdrant points;
- missing points;
- orphan points;
- payload mismatches;
- dense representation mismatches;
- sparse representation mismatches;
- document/version/access-zone/access-level/lifecycle mismatches;
- representation-version mismatches;
- final verdict.

Required verdicts:

```text
QDRANT_PROJECTION_CONSISTENT
QDRANT_PROJECTION_DRIFT
QDRANT_REBUILD_FAILED
```

## 15. Retrieval parity proof

The strongest recovery proof is functional.

Required scenario:

```text
1. start from known-good PostgreSQL + Qdrant
2. execute frozen retrieval query set and save baseline
3. destroy only the target Qdrant collection
4. verify PostgreSQL canonical rows remain unchanged
5. run qdrant-rebuild
6. run qdrant-audit
7. rerun identical retrieval query set
8. compare results
```

Compare at least:

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

The proof MUST fail on safety/correctness changes even if the endpoint still returns HTTP/gRPC success.

## 16. Fresh total-persistence proof

`full-proof` MUST exercise the complete persistence chain on isolated disposable infrastructure:

```text
EMPTY PostgreSQL
    -> all migrations
    -> runtime/ingestion fixture
    -> canonical PostgreSQL state
    -> Qdrant projection
    -> activation/retrieval baseline
    -> destroy Qdrant collection
    -> rebuild Qdrant from PostgreSQL
    -> retrieval parity
    -> PostgreSQL schema audit
    -> Qdrant consistency audit
```

Prefer extending existing testcontainers/local-demo infrastructure rather than creating a second test framework.

## 17. Evidence files

Add a dedicated FIX491 evidence directory. Generated evidence may live outside Git for raw/high-volume output, but checked-in summary templates/runbooks MUST exist.

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

Machine-readable counterparts SHOULD be JSON.

## 18. Required regression gates

At minimum:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Add focused FIX491 integration tests for clean migration, drift detection, Qdrant rebuild and retrieval parity.

Do not rerun unrelated long soak campaigns unless FIX491 changes runtime retrieval semantics (which it should not).

## 19. Safety invariants

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

## 20. Final acceptance verdict

Only these final verdicts are allowed:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

PASS requires all of the following:

```text
PostgreSQL fresh bootstrap             PASS
SQLx migration history/checksums       PASS
PostgreSQL material schema drift       0
PostgreSQL runtime integrity            PASS
Qdrant clean collection rebuild         PASS
Qdrant projection drift                 0
Qdrant orphan points                    0
Qdrant missing eligible points          0
Retrieval before/after rebuild parity   PASS
Regression gates                        PASS
```

A PASS does not claim that migrations can restore deleted PostgreSQL business data. Production canonical-data disaster recovery still requires a verified PostgreSQL backup/PITR source.