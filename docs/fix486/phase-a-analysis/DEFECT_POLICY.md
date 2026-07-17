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

## Before/after rule

Codex may fix a P0/P1 defect that blocks truthful analysis only in this sequence:

1. freeze source and bank identities;
2. execute the unchanged input;
3. preserve failing evidence;
4. add a regression test;
5. document root cause;
6. implement the smallest production fix;
7. keep queries and qrels unchanged;
8. rerun the same input;
9. preserve after-fix evidence;
10. publish a direct comparison.

## Commit separation

Do not mix in one commit:

- data-bank changes;
- qrel changes;
- production fixes;
- threshold/ranking changes;
- evidence reports.

Recommended sequence:

```text
analysis documentation
data-bank seed
failing regression test
production fix
runtime proof
evidence report
```

## Prohibited remediation

- changing the expected parent to the returned parent;
- deleting a hard negative;
- weakening access/lifecycle filters;
- adding fixture IDs to production code;
- tuning weights without same-bank A/B evidence;
- treating timeout as no evidence;
- treating BLOCKED or SKIPPED as PASS;
- rewriting history to remove failing proof.

## Defect record

Every defect must record:

```text
ID
category
severity
source SHA
bank version and SHA
query/scenario ID
expected result
actual result
root cause
files/functions
regression test
production fix commit
before evidence
before/after evidence
affected phases
remaining risk
```

All non-blocking defects found in fix486a must be placed in `IMPLEMENTATION_BACKLOG.md` for the relevant later phase.