# FIX486A defect policy

## Categories

```text
CORRECTNESS_DEFECT
SECURITY_DEFECT
LIFECYCLE_DEFECT
DEGRADATION_DEFECT
GRAPH_PROVENANCE_DEFECT
MMR_DEFECT
TOKEN_BUDGET_DEFECT
MULTI_INTENT_DEFECT
PERFORMANCE_DEFECT
OBSERVABILITY_DEFECT
TESTABILITY_DEFECT
FIXTURE_DEFECT
```

## Severity

```text
P0 leakage, wrong version, deleted/expired content, false FOUND
P1 wrong parent, false no-answer, lost required intent, Graph provenance corruption
P2 duplicate context, latency, resource leak, incomplete diagnostics
P3 documentation or ergonomics
```

## Mandatory repair scope

Every reproducible in-scope P0/P1 defect discovered during fix486a must be repaired in the same branch before `FIX486_ANALYSIS_READY` may be declared.

A reproducible P2 correctness or resource-leak defect should be repaired when the change is local, low-risk and does not broaden the phase beyond hierarchical retrieval. Otherwise it must be recorded in the implementation backlog with an explicit reason, risk and target phase.

P3 defects remain backlog work.

## Before/after rule

Codex must use this sequence for every repaired defect:

1. freeze source and bank identities;
2. execute the unchanged input;
3. preserve failing evidence;
4. add a regression test that fails before the fix;
5. document root cause;
6. implement the smallest production fix;
7. keep queries, qrels and expected identities unchanged;
8. rerun the same input;
9. preserve after-fix evidence;
10. publish a direct comparison;
11. rerun all affected integration and model-backed gates;
12. update Proof Matrix and final report.

## Commit separation

Do not mix in one commit:

- data-bank changes;
- qrel changes;
- regression tests;
- production fixes;
- threshold/ranking changes;
- evidence reports.

Recommended sequence:

```text
analysis documentation
data-bank validation
failing regression test
production fix
runtime proof
evidence report
```

## Prohibited remediation

- changing the expected parent to the returned parent;
- deleting a hard negative;
- weakening access/lifecycle filters;
- adding fixture IDs or anchors to production code;
- tuning weights without same-bank A/B evidence;
- treating timeout as no evidence;
- treating BLOCKED or SKIPPED as PASS;
- rewriting history to remove failing proof;
- replacing a real production path with a test-only shortcut;
- suppressing a failure without correcting its root cause.

## Defect record

Every defect must record:

```text
ID
category
severity
source SHA
candidate SHA
bank version and SHA
query/scenario ID
expected result
actual result
root cause
files/functions
regression test
production fix commit
before evidence
after evidence
before/after comparison
affected phases
remaining risk
resolution status
```

## READY blocker

`FIX486_ANALYSIS_READY` is forbidden while any reproducible in-scope P0/P1 defect remains unresolved, while a repaired defect lacks a regression test or before/after evidence, or while a mandatory final gate is FAIL, BLOCKED or SKIPPED.

All remaining P2/P3 defects must be placed in `IMPLEMENTATION_BACKLOG.md` for the relevant later phase.