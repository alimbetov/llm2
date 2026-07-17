# FIX486A analysis and repair result

## 1. Identity

```text
SOURCE_BRANCH=
BASELINE_SOURCE_SHA=
FINAL_CANDIDATE_SHA=
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

## 2. Baseline before fixes

| Gate | Status | Evidence | Failure code |
|---|---|---|---|
| fmt | | | |
| check locked all targets/features | | | |
| tests locked all targets/features | | | |
| clippy locked all targets/features | | | |
| targeted hierarchical tests | | | |
| integration/Testcontainers | | | |
| affected model-backed gates | | | |

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

## 5. Defect register

| ID | Severity | Category | Scenario | Root cause | Before evidence | Regression test | Fix commit | After evidence | Status |
|---|---|---|---|---|---|---|---|---|---|

## 6. Before/after comparison

For every repaired defect include:

```text
DEFECT_ID=
BANK_VERSION=
BANK_SHA256=
QUERY_OR_SCENARIO_ID=
EXPECTED_RESULT=
BEFORE_SOURCE_SHA=
BEFORE_ACTUAL_RESULT=
BEFORE_EVIDENCE=
REGRESSION_TEST=
ROOT_CAUSE=
FIX_COMMIT=
AFTER_SOURCE_SHA=
AFTER_ACTUAL_RESULT=
AFTER_EVIDENCE=
QUERIES_UNCHANGED=
QRELS_UNCHANGED=
REMAINING_RISK=
```

## 7. Testability and observability gaps

| ID | Gap | Required change | Production impact | Priority | Resolution/status |
|---|---|---|---|---|---|

## 8. Bank feasibility

```text
SCHEMAS_VALID=
LOGICAL_IDENTITIES_FEASIBLE=
CROSS_ZONE_SCENARIO_FEASIBLE=
LIFECYCLE_SCENARIOS_FEASIBLE=
GRAPH_SCENARIO_FEASIBLE=
BANK_1_0_0_FREEZE_READY=
```

## 9. Mandatory final rerun

| Gate | Status | Evidence | Failure code |
|---|---|---|---|
| cargo fmt --all --check | | | |
| cargo check --locked --all-targets --all-features | | | |
| cargo test --locked --all-targets --all-features | | | |
| cargo clippy --locked --all-targets --all-features -- -D warnings | | | |
| all new regression tests | | | |
| relevant integration/Testcontainers tests | | | |
| affected model-backed gates | | | |
| executable bank scenarios | | | |

## 10. Unresolved defects

| ID | Severity | Reason unresolved | Risk | Target phase | Final blocker |
|---|---|---|---|---|---|

Any reproducible in-scope unresolved P0/P1 defect must set `Final blocker = YES`.

## 11. Mac methodology readiness

TBD

## 12. Implementation backlog summary

TBD

## 13. Remaining blockers

TBD

## 14. Final verdict

`FIX486_ANALYSIS_READY` is allowed only when:

```text
UNRESOLVED_IN_SCOPE_P0=0
UNRESOLVED_IN_SCOPE_P1=0
REPAIRED_DEFECTS_WITHOUT_REGRESSION_TEST=0
REPAIRED_DEFECTS_WITHOUT_BEFORE_AFTER_EVIDENCE=0
MANDATORY_FINAL_FAILED=0
MANDATORY_FINAL_BLOCKED=0
MANDATORY_FINAL_SKIPPED=0
IDENTITY_MISMATCH=0
```

Exactly one:

```text
FIX486_ANALYSIS_READY
```

or:

```text
FIX486_ANALYSIS_BLOCKED
```