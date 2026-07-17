# FIX486A analysis result

## 1. Identity

```text
SOURCE_BRANCH=
SOURCE_SHA=
ORIGIN_MAIN_SHA=
EPIC_SHA=
CARGO_LOCK_SHA256=
MODEL_SHA256=
TOKENIZER_SHA256=
CONFIG_SHA256=
BANK_ID=fix486-hierarchical-bank
BANK_VERSION=0.1.0-analysis-seed
BANK_SHA256=
EVIDENCE_PATH=
EVIDENCE_MANIFEST_SHA256=
```

## 2. Baseline

| Gate | Status | Evidence | Failure code |
|---|---|---|---|
| fmt | | | |
| check locked all targets/features | | | |
| tests locked all targets/features | | | |
| clippy locked all targets/features | | | |

## 3. Architecture findings

### Ingestion hierarchy

TBD

### Child retrieval and parent hydration

TBD

### Parent grouping and deduplication

TBD

### Isolation and lifecycle

TBD

### GraphRAG parent resolution

TBD

### MMR, token budget and final coverage

TBD

## 4. Critical proof readiness

| Case | Status | Current proof | Missing proof | Blocking work |
|---|---|---|---|---|
| FIX486-01 | | | | |
| FIX486-02 | | | | |
| FIX486-03 | | | | |
| FIX486-04 | | | | |
| FIX486-05 | | | | |
| FIX486-06 | | | | |
| FIX486-07 | | | | |
| FIX486-08 | | | | |
| FIX486-09 | | | | |
| FIX486-10 | | | | |

## 5. Defects

| ID | Severity | Category | Root cause | Required phase |
|---|---|---|---|---|

## 6. Testability and observability gaps

| ID | Gap | Required change | Production impact | Priority |
|---|---|---|---|---|

## 7. Bank feasibility

```text
SCHEMAS_VALID=
LOGICAL_IDENTITIES_FEASIBLE=
CROSS_ZONE_SCENARIO_FEASIBLE=
LIFECYCLE_SCENARIOS_FEASIBLE=
GRAPH_SCENARIO_FEASIBLE=
BANK_1_0_0_FREEZE_READY=
```

## 8. Mac methodology readiness

TBD

## 9. Implementation backlog summary

TBD

## 10. Remaining blockers

TBD

## 11. Final verdict

Exactly one:

```text
FIX486_ANALYSIS_READY
```

or:

```text
FIX486_ANALYSIS_BLOCKED
```