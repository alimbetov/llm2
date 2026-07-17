# v011/fix486b — Reproducible Runtime Baseline

## 1. Document status

```text
DOCUMENT_TYPE=IMPLEMENTATION_AND_PROOF_SPECIFICATION
PHASE=FIX486B_REPRODUCIBLE_RUNTIME_BASELINE
BASE_BRANCH=epic/fix486-hierarchical-retrieval-validation
WORK_BRANCH=codex/fix486b-reproducible-runtime-baseline
EXPECTED_BASE_SHA=e590cee8a7783b93084fb76c8eabc01e40d226bf
LOCAL_PROJECT=/Users/ruslanalimbetov/Documents/llm2/astravector
WORKTREE=/Users/ruslanalimbetov/Documents/llm2/astravector-fix486b
EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486b
BANK_VERSION=0.1.0-analysis-seed
BANK_FREEZE_AUTHORIZED=false
PRODUCTION_STATUS=BLOCKED
```

The actual branch and commit identities must be resolved at execution time. The expected SHA is a lineage check, not permission to ignore a newer approved epic SHA.

## 2. Preconditions

Phase A completed with:

```text
FIX486_ANALYSIS_READY
```

Established facts:

- `Search` is the authoritative production retrieval pipeline;
- `RetrieveContext` delegates to `Search`;
- PostgreSQL is canonical and Qdrant is a projection;
- direct parent hydration is zone-scoped and batched;
- Graph parent fallback defect `FIX486A-P1-001` was repaired;
- the hierarchical seed bank is structurally feasible but is not frozen as 1.0.0;
- model-backed execution of all hierarchical bank queries belongs to later phases.

Phase B must preserve these boundaries.

## 3. Objective

Create a reproducible, evidence-backed runtime baseline that proves the same source, lockfile, configuration, model, tokenizer, container images and release binary can repeatedly perform the following lifecycle:

```text
clean environment
→ clean PostgreSQL and Qdrant
→ migrations
→ model/tokenizer validation
→ release build
→ runtime startup
→ readiness
→ production ingestion of a control fixture
→ Search and RetrieveContext control probes
→ restart and persistence proof
→ dependency-loss readiness proof
→ complete shutdown with no leaked processes
```

The phase answers:

> Can AstraVector be started and exercised reproducibly from a clean state before the immutable hierarchical bank is frozen and before functional hierarchy certification begins?

## 4. Allowed final verdicts

Exactly one:

```text
FIX486_RUNTIME_BASELINE_PASS
```

or:

```text
FIX486_RUNTIME_BASELINE_BLOCKED
```

`PASS` is an operational reproducibility verdict only. It does not mean:

```text
FIX486_HIERARCHICAL_RETRIEVAL_PASS
HYBRID_BETTER_THAN_DENSE
BANK_1_0_0_FROZEN
MAC_LOAD_PASS
PRODUCTION_READY
```

## 5. Scope

### In scope

- source and environment identity;
- clean worktree enforcement;
- locked static gates;
- SQLx metadata check;
- PostgreSQL and Qdrant clean startup;
- actual container image IDs/digests;
- migrations from an empty database;
- migration idempotency and migration-head proof;
- schema integrity audit;
- real BGE-M3 model and tokenizer identity and load;
- dense dimension and sparse capability recording;
- release build and binary checksum;
- resolved runtime configuration checksum;
- gRPC reflection and health;
- metrics endpoint availability;
- production ingestion of a small baseline control fixture;
- idempotent repeated ingestion;
- `Search` control probe;
- `RetrieveContext` control probe;
- matched-child and non-empty-parent response sanity;
- runtime restart without data loss;
- readiness failure and recovery when a required dependency is removed;
- repeated clean-run comparison;
- evidence manifest and machine-readable stage results;
- repair of reproducible in-scope P0/P1 runtime-baseline defects.

### Out of scope

- freezing hierarchical bank 1.0.0;
- changing hierarchical qrels;
- running all 11 hierarchical bank queries as quality proof;
- child/parent correctness certification;
- access-zone collision certification;
- lifecycle and stale-Qdrant certification;
- hydration failpoint implementation;
- Graph parent quality proof;
- MMR/token-budget quality proof;
- hybrid versus dense ablation;
- latency SLO certification;
- 60-minute soak;
- mixed 70/20/10 load;
- packaging/rollback production promotion;
- ranking, RRF, MMR, Graph or no-answer tuning.

## 6. Runtime identity contract

Every official run must record:

```text
source branch and SHA
origin/main SHA
epic SHA
Cargo.lock SHA-256
resolved config SHA-256
release binary SHA-256
model SHA-256
tokenizer SHA-256
control fixture SHA-256
PostgreSQL image reference and image ID/digest
Qdrant image reference and image ID/digest
Rust and Cargo versions
Docker and Compose versions
OS and hardware identity
ports and owning PIDs
migration head
```

Any identity drift during a stage produces:

```text
EVIDENCE_IDENTITY_MISMATCH
```

and blocks the phase.

## 7. Baseline control fixture

Use a small phase-specific control fixture, not the full hierarchical quality bank.

Required logical content:

```text
access zone: FIX486B_BASELINE_ZONE
document: fix486b-runtime-control
version: 1
status: ACTIVE
one SOURCE
at least one PARENT
at least one SUB_180 or SUB_260 generated by the production chunking path
exact anchor: ASTRA_FIX486B_RUNTIME_CONTROL
semantic statement: PostgreSQL is the canonical state and Qdrant is a search projection.
```

Requirements:

1. Ingest through the production public ingestion facade or the same production service path used by it.
2. Do not insert hierarchy rows directly with ad hoc SQL except for read-only audit.
3. Do not precompute physical chunk IDs in the fixture.
4. Capture returned and persisted physical identities.
5. Repeating the same idempotent request must not duplicate documents, chunks, bindings or outbox effects.
6. The fixture must remain separate from `benchmarks/hierarchical/fix486` bank 1.0.0 freeze work.

## 8. Required execution matrix

### Run R1 — clean cold start

1. Verify clean worktree and expected lineage.
2. Prove target ports are unused.
3. Remove only phase-owned containers/volumes.
4. Start clean PostgreSQL and Qdrant.
5. Apply migrations from empty database.
6. Reapply migrations and prove idempotency.
7. Validate schema integrity.
8. Validate model/tokenizer files and hashes.
9. Build release binaries with `--locked`.
10. Start runtime with resolved phase config.
11. Prove reflection, health and metrics.
12. Ingest control fixture.
13. Repeat ingestion with same request identity.
14. Execute Search and RetrieveContext probes.
15. Capture database, Qdrant and response audit.
16. Stop runtime and prove no leaked process owns runtime ports.

### Run R2 — independent clean repetition

Destroy phase-owned PostgreSQL/Qdrant state and repeat R1 from empty infrastructure.

Compare R1 and R2:

```text
same source/lock/model/tokenizer/config identities
same migration head
same logical hierarchy shape
same deterministic physical hierarchy identities where production identity inputs are identical
same Search/RetrieveContext logical result identity
same PASS/FAIL stage set
no duplicate state
```

Timestamps, run IDs, process IDs and latency values may differ.

### Run R3 — persistence and recovery restart

Using a successful R2 data state:

1. Restart AstraVector without reingestion.
2. Prove health returns to SERVING.
3. Repeat Search/RetrieveContext probes and obtain the same logical result.
4. Stop Qdrant and prove readiness is not falsely healthy when Qdrant is required.
5. Restore Qdrant and prove readiness recovers.
6. Stop PostgreSQL and prove readiness is not falsely healthy when PostgreSQL is required.
7. Restore PostgreSQL and prove readiness recovers.
8. Stop runtime cleanly and verify no leaked ports/processes.

Dependency removal must not be represented as ordinary no-answer evidence.

## 9. Mandatory gates

Run with `set -o pipefail` and preserve exact exit codes:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --locked --features integration-tests --test e2e_testcontainers -- --nocapture
cargo test --locked --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
```

The concurrency Testcontainers test is a startup/concurrency sanity gate only. It does not produce a Phase I performance verdict.

## 10. Runtime assertions

### Infrastructure

```text
POSTGRES_ACCEPTS_CONNECTIONS=true
QDRANT_READYZ=true
PREEXISTING_RUNTIME_PORT_OWNER=false
ACTUAL_IMAGE_IDENTITIES_RECORDED=true
```

### Migrations

```text
CLEAN_MIGRATION_PASS=true
MIGRATION_REAPPLY_PASS=true
MIGRATION_HEAD_MATCH=true
FAILED_SQLX_MIGRATIONS=0
SCHEMA_INTEGRITY_VIOLATIONS=0
```

### Model and tokenizer

```text
MODEL_PRESENT=true
TOKENIZER_PRESENT=true
MODEL_HASH_RECORDED=true
TOKENIZER_HASH_RECORDED=true
TOKENIZER_OFFSETS_PASS=true
MODEL_WARMUP_PASS=true
DENSE_DIMENSION=1024
SPARSE_CAPABILITY_RECORDED=true
```

### Runtime

```text
RELEASE_BUILD_PASS=true
RUNTIME_ALIVE=true
GRPC_REFLECTION_PASS=true
HEALTH_SERVING=true
METRICS_ENDPOINT_PASS=true
EXPECTED_SERVICES_PRESENT=true
```

### Control ingestion

```text
DOCUMENTS_CREATED=1
DUPLICATE_DOCUMENTS=0
DUPLICATE_CHUNKS=0
DUPLICATE_BINDINGS=0
DUPLICATE_OUTBOX_EFFECTS=0
PARENT_COUNT>=1
CHILD_COUNT>=1
ORPHAN_CHILDREN=0
```

### Control retrieval

```text
SEARCH_RESULT_COUNT>=1
RETRIEVE_CONTEXT_COUNT>=1
EXPECTED_DOCUMENT_RETURNED=true
MATCHED_CHUNK_ID_PRESENT=true
PARENT_CHUNK_ID_PRESENT=true
MATCHED_TEXT_NONEMPTY=true
PARENT_TEXT_NONEMPTY=true
SEARCH_RETRIEVE_LOGICAL_IDENTITY_MATCH=true
CROSS_ZONE_RESULTS=0
WRONG_VERSION_RESULTS=0
```

This is a control-probe assertion, not a ranking-quality judgment.

### Restart and recovery

```text
RESTART_WITHOUT_REINGEST_PASS=true
POST_RESTART_LOGICAL_RESULT_STABLE=true
READINESS_FAILS_WITHOUT_QDRANT=true
READINESS_RECOVERS_WITH_QDRANT=true
READINESS_FAILS_WITHOUT_POSTGRES=true
READINESS_RECOVERS_WITH_POSTGRES=true
LEAKED_RUNTIME_PROCESSES=0
LEAKED_PORT_OWNERS=0
```

## 11. Reproducibility comparison

Produce a normalized comparison excluding volatile fields.

Required stable fields:

```text
source and dependency hashes
migration head
resolved service list
fixture logical identity
hierarchy counts
physical IDs generated from identical production inputs
Search/RetrieveContext logical result identity
stage verdicts
```

A mismatch must be classified as one of:

```text
SOURCE_DRIFT
CONFIG_DRIFT
DEPENDENCY_DRIFT
NONDETERMINISTIC_IDENTITY
NONDETERMINISTIC_RUNTIME_RESULT
FIXTURE_DRIFT
ENVIRONMENT_BLOCKER
```

## 12. Defect and repair policy

For every reproducible in-scope P0/P1 defect:

1. freeze source and evidence identities;
2. preserve failing runtime evidence;
3. add a failing regression test;
4. document root cause;
5. implement the smallest safe production fix in a separate commit;
6. keep the control input and Phase A bank qrels unchanged;
7. rerun the same stage;
8. rerun R1–R3 and all mandatory gates;
9. publish before/after evidence.

P0 examples:

- wrong-zone control result;
- deleted/inactive data visible in the control path;
- readiness falsely healthy without canonical dependencies;
- migration corrupts or loses canonical state.

P1 examples:

- restart loses searchable state;
- infrastructure failure becomes successful no-answer;
- idempotent ingestion creates duplicates;
- same clean inputs produce different physical hierarchy IDs.

Do not perform broad ranking redesign, Graph redesign, MMR redesign or bank freeze in this phase.

## 13. Evidence layout

```text
<EVIDENCE_ROOT>/<run-id>/
├── environment/
├── source/
├── static/
├── infrastructure/
├── migrations/
├── model-tokenizer/
├── build/
├── runtime/
├── fixture/
├── ingestion/
├── retrieval/
├── restart/
├── dependency-recovery/
├── comparisons/
├── logs/
├── metrics/
├── stage-results.json
├── defect-register.json
├── manifest.json
└── FIX486B-RUNTIME-BASELINE-RESULT.md
```

Large logs and local environment material remain outside Git. Commit only compact summaries, schemas, scripts and manifest hashes.

## 14. Required repository outputs

```text
docs/fix486/phase-b-runtime-baseline/TECHNICAL_SPECIFICATION.md
docs/fix486/phase-b-runtime-baseline/CODEX_EXECUTION_TASK.md
docs/fix486/phase-b-runtime-baseline/CONTROL_FIXTURE_SPECIFICATION.md
docs/fix486/phase-b-runtime-baseline/RUN_MATRIX.md
docs/fix486/phase-b-runtime-baseline/EVIDENCE_CONTRACT.md
docs/fix486/phase-b-runtime-baseline/DEFECT_POLICY.md
docs/fix486/phase-b-runtime-baseline/ACCEPTANCE_CRITERIA.md
docs/fix486/phase-b-runtime-baseline/RESULT_TEMPLATE.md
```

Implementation may add:

```text
phase-owned smoke script
phase-owned resolved config
control fixture loader or test driver
read-only schema/hierarchy audit SQL
Makefile target verify-fix486b-runtime-baseline
CI gate for bank structural contract and targeted hierarchy Testcontainers test
```

Choose the next unused smoke-script numeric prefix after inspecting the repository; do not overwrite an existing script.

## 15. Definition of Done

```text
[ ] Exact source and runtime identities recorded
[ ] Clean R1 completed
[ ] Independent clean R2 completed
[ ] R1/R2 normalized comparison PASS
[ ] Restart/recovery R3 completed
[ ] Clean migrations and idempotent reapply PASS
[ ] Model/tokenizer and warmup PASS
[ ] Release binary and resolved config hashes recorded
[ ] Reflection, health and metrics PASS
[ ] Control ingestion and idempotency PASS
[ ] Search and RetrieveContext control probes PASS
[ ] PostgreSQL/Qdrant readiness failure and recovery PASS
[ ] Static, SQLx, all-target and required Testcontainers gates PASS
[ ] No unresolved in-scope P0/P1
[ ] External evidence bundle complete
[ ] No bank 1.0.0 freeze claimed
[ ] Handoff blockers for fix486c recorded
```

## 16. Final gate

`FIX486_RUNTIME_BASELINE_PASS` requires every mandatory assertion and stage to be `PASS`.

Any mandatory `FAIL`, `BLOCKED`, `SKIPPED`, missing evidence, dirty worktree, identity drift or unresolved in-scope P0/P1 produces:

```text
FIX486_RUNTIME_BASELINE_BLOCKED
```
