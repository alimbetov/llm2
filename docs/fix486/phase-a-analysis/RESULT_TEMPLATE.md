# FIX486A final analysis result

## Identity

```text
BASELINE_SOURCE_SHA=bb6fd6781623cbe0a84f91a204f59da2a32e5c55
RUNTIME_FIX_CANDIDATE_SHA=f160a76c78fc775e633d5d17760219eb8af8f40b
EPIC_SHA=cfa01b2d615582ac736f1ef844d8fc79280e3ff1
BANK_VERSION=0.1.0-analysis-seed
BANK_FILES_SHA256=cb5b80c25f30f20a2e68a70952be2223905ad9bb2c731ab58603a670d57e4933
MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b
TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
FINAL_EVIDENCE=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486a/fix486a-final-20260717T150000Z
```

## Baseline and final gates

| Gate | Baseline | Final | Assertion |
|---|---|---|---|
| `cargo fmt --all --check` | PASS | PASS | exit 0 |
| `cargo check --locked --all-targets --all-features` | PASS | PASS | exit 0 |
| `cargo test --locked --all-targets --all-features` | PASS | PASS | exit 0; real assertions and Testcontainers executed |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS | PASS | exit 0 |
| FIX486 bank contracts | not present | PASS | 3 passed, 0 failed; 11 queries/qrels, 10 cases |
| Graph parent regression | FAIL before repair | PASS | 1 passed, 0 failed after repair |
| affected model-backed gate | PASS | PASS | real BGE-M3 tokenizer boundary tests 2 passed |

The two repository tests marked ignored by the all-target command are pre-existing opt-in full
production-candidate/live-endpoint suites and are not used as FIX486 Phase A evidence. No mandatory
Phase A stage is represented as PASS because of an ignored test.

## Critical proof readiness

| Case | Status | Blocking future runtime proof |
|---|---|---|
| FIX486-01 | IMPLEMENTED_PARTIALLY_PROVEN | immutable-bank exact child/parent run |
| FIX486-02 | IMPLEMENTED_PARTIALLY_PROVEN | two-child winner/dedup trace |
| FIX486-03 | IMPLEMENTED_PARTIALLY_PROVEN | executable Zone A/B collision run |
| FIX486-04 | IMPLEMENTED_PARTIALLY_PROVEN | higher-attraction inactive version run |
| FIX486-05 | IMPLEMENTED_PARTIALLY_PROVEN | stale Qdrant projection drop trace |
| FIX486-06 | IMPLEMENTED_NOT_PROVEN | deterministic hydration failpoints |
| FIX486-07 | IMPLEMENTED_PARTIALLY_PROVEN | exact sparse/FTS child assertion |
| FIX486-08 | IMPLEMENTED_PARTIALLY_PROVEN | bank REPAIRED_BY identity run |
| FIX486-09 | IMPLEMENTED_PARTIALLY_PROVEN | production-token constrained budget run |
| FIX486-10 | IMPLEMENTED_NOT_PROVEN | generated 900-token parent selection run |

## Defect closeout

```text
DEFECT_ID=FIX486A-P1-001
SEVERITY=P1
BEFORE=FAIL, stale graph child substituted for deleted parent
REGRESSION=tests/e2e_testcontainers.rs
ROOT_CAUSE=LEFT JOIN plus COALESCE parent-to-child fallback
FIX_COMMIT=388d7fd
AFTER=PASS, stale child hydration returns zero contexts
QUERIES_UNCHANGED=true
QRELS_UNCHANGED=true
UNRESOLVED_IN_SCOPE_P0=0
UNRESOLVED_IN_SCOPE_P1=0
```

## Bank feasibility

```text
SCHEMAS_STRUCTURALLY_VALID=true
QUERY_QREL_RECORDS_LOADED=11/11
CRITICAL_CASES_COVERED=10/10
LOGICAL_IDENTITIES_FEASIBLE=true
CROSS_ZONE_SCENARIO_FEASIBLE=true
LIFECYCLE_SCENARIOS_FEASIBLE=true
GRAPH_SCENARIO_FEASIBLE=true
BANK_1_0_0_FREEZE_READY=false
FREEZE_BLOCKERS=generated parent-large text, production token counts, physical identity map, manifest hashes
```

## Scope boundary

`FIX486_ANALYSIS_READY` means the production path is mapped, the seed bank is feasible, all ten
cases have executable proof designs, the discovered P1 is fixed with regression evidence, and all
mandatory Phase A gates pass. It does not mean the complete hierarchical bank, model-backed
quality program or Mac load certification has passed.

## Verdict

```text
FIX486_ANALYSIS_READY
```
