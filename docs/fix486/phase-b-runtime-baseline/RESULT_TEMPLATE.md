# FIX486B runtime baseline result

## 1. Identity

```text
SOURCE_BRANCH=
BASELINE_SOURCE_SHA=
FINAL_CANDIDATE_SHA=
ORIGIN_MAIN_SHA=
EPIC_SHA=
CARGO_LOCK_SHA256=
CONFIG_SHA256=
BINARY_SHA256=
MODEL_SHA256=
TOKENIZER_SHA256=
CONTROL_FIXTURE_SHA256=
POSTGRES_IMAGE=
POSTGRES_IMAGE_ID=
QDRANT_IMAGE=
QDRANT_IMAGE_ID=
MIGRATION_HEAD=
EVIDENCE_PATH=
EVIDENCE_MANIFEST_SHA256=
```

## 2. Mandatory gate matrix

| Gate | Status | Exit code | Evidence | Failure code |
|---|---|---:|---|---|
| fmt | | | | |
| locked check | | | | |
| locked all-target tests | | | | |
| locked clippy | | | | |
| SQLx prepare check | | | | |
| e2e Testcontainers | | | | |
| concurrency Testcontainers | | | | |
| FIX486 bank contracts | | | | |

## 3. R1 — clean cold start

| Stage | Status | Key assertions | Evidence |
|---|---|---|---|
| source/worktree | | | |
| infrastructure | | | |
| migrations | | | |
| schema audit | | | |
| model/tokenizer | | | |
| release build | | | |
| reflection/health/metrics | | | |
| control ingestion | | | |
| idempotency | | | |
| Search probe | | | |
| RetrieveContext probe | | | |
| shutdown audit | | | |

## 4. R2 — independent clean repetition

| Stage | Status | Key assertions | Evidence |
|---|---|---|---|
| clean recreation | | | |
| repeated migrations | | | |
| repeated runtime startup | | | |
| repeated ingestion | | | |
| repeated retrieval | | | |
| shutdown audit | | | |

## 5. R1/R2 normalized comparison

```text
SOURCE_IDENTITIES_MATCH=
DEPENDENCY_IDENTITIES_MATCH=
MIGRATION_HEAD_MATCH=
HIERARCHY_SHAPE_MATCH=
DETERMINISTIC_PHYSICAL_IDS_MATCH=
SEARCH_LOGICAL_IDENTITY_MATCH=
RETRIEVE_LOGICAL_IDENTITY_MATCH=
STAGE_VERDICTS_MATCH=
DIFF_CLASSIFICATION=
```

## 6. R3 — persistence and dependency recovery

| Stage | Status | Assertion | Evidence |
|---|---|---|---|
| restart without reingestion | | | |
| post-restart Search | | | |
| post-restart RetrieveContext | | | |
| Qdrant down readiness | | | |
| Qdrant recovery | | | |
| PostgreSQL down readiness | | | |
| PostgreSQL recovery | | | |
| final shutdown/process audit | | | |

## 7. Control fixture identity

```text
ACCESS_ZONE_ID=
DOCUMENT_ID=
DOCUMENT_VERSION=1
SOURCE_CHUNK_ID=
PARENT_CHUNK_IDS=
CHILD_CHUNK_IDS=
BINDING_IDS=
QDRANT_POINT_IDS=
ORPHAN_CHILDREN=0
DUPLICATE_DOCUMENTS=0
DUPLICATE_CHUNKS=0
DUPLICATE_BINDINGS=0
```

## 8. Retrieval control results

```text
SEARCH_RESULT_COUNT=
RETRIEVE_CONTEXT_COUNT=
EXPECTED_DOCUMENT_RETURNED=
MATCHED_CHUNK_ID_PRESENT=
PARENT_CHUNK_ID_PRESENT=
MATCHED_TEXT_NONEMPTY=
PARENT_TEXT_NONEMPTY=
SEARCH_RETRIEVE_LOGICAL_IDENTITY_MATCH=
WRONG_ZONE_RESULTS=0
WRONG_VERSION_RESULTS=0
```

## 9. Defect register

| ID | Severity | Category | Stage | Root cause | Regression | Fix commit | Status |
|---|---|---|---|---|---|---|---|

## 10. Before/after repairs

For each repaired P0/P1 record:

```text
DEFECT_ID=
BEFORE_SHA=
BEFORE_STATUS=
BEFORE_EVIDENCE=
CONTROL_INPUT_SHA256=
REGRESSION_TEST=
ROOT_CAUSE=
FIX_COMMIT=
AFTER_SHA=
AFTER_STATUS=
AFTER_EVIDENCE=
```

## 11. Remaining risks and Phase C handoff

```text
UNRESOLVED_IN_SCOPE_P0=
UNRESOLVED_IN_SCOPE_P1=
P2_FIXED=
P2_DEFERRED=
BANK_1_0_0_FROZEN=false
PHASE_C_BLOCKERS=
PHASE_C_REQUIRED_INPUTS=
```

## 12. Scope statement

This phase proves runtime reproducibility and recovery for a production-path control fixture. It does not certify the full hierarchical bank, ranking quality, hybrid superiority, Graph/MMR quality, token-budget quality, Mac SLO or production readiness.

## 13. Final verdict

Exactly one:

```text
FIX486_RUNTIME_BASELINE_PASS
```

or:

```text
FIX486_RUNTIME_BASELINE_BLOCKED
```