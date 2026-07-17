# FIX486B defect policy

## Categories

```text
RUNTIME_STARTUP_DEFECT
MIGRATION_DEFECT
MODEL_TOKENIZER_DEFECT
CONFIGURATION_DEFECT
INGESTION_IDEMPOTENCY_DEFECT
RETRIEVAL_CONTROL_DEFECT
READINESS_DEFECT
RECOVERY_DEFECT
DETERMINISM_DEFECT
RESOURCE_LEAK_DEFECT
OBSERVABILITY_DEFECT
TESTABILITY_DEFECT
ENVIRONMENT_BLOCKER
```

## Severity

```text
P0 canonical data loss, cross-zone visibility, corrupted migration, false healthy state that can serve unsafe data
P1 restart loses searchable state, infrastructure failure becomes no-answer, deterministic inputs produce divergent identities, idempotent ingestion duplicates state
P2 resource leak, incomplete diagnostics, flaky startup, avoidable manual step
P3 documentation or ergonomics
```

## Required defect record

```text
ID
category
severity
source SHA
control fixture SHA
R1/R2/R3 stage
expected result
actual result
root cause
affected files/functions
regression test
fix commit
before evidence
after evidence
remaining risk
target phase when deferred
```

## Repair sequence

For every reproducible in-scope P0/P1:

1. freeze source, environment and control fixture identities;
2. preserve FAIL evidence;
3. create a failing regression test;
4. document root cause;
5. implement the smallest safe fix in a separate commit;
6. keep Phase A bank queries/qrels and control expectations unchanged;
7. rerun the same failing stage;
8. rerun complete R1, R2 and R3;
9. rerun all mandatory gates;
10. publish before/after comparison.

## Commit separation

Do not combine in one commit:

```text
specification changes
control fixture changes
regression test
production fix
ranking/config tuning
evidence report
```

## Prohibited remediation

- changing control expectations to match a defect;
- bypassing production ingestion with direct rows or Qdrant points;
- disabling dependency readiness checks;
- accepting empty/no-answer after dependency failure;
- using different model/tokenizer/config between R1 and R2;
- deleting failing evidence;
- weakening lifecycle/access/no-answer rules;
- freezing bank 1.0.0;
- broad ranking or Graph/MMR redesign.

## P2 handling

Fix P2 defects when the change is local, low-risk and required for reproducibility. Otherwise record:

```text
reason for deferral
operational risk
workaround
target phase
required proof
```

## Final rule

Any unresolved reproducible in-scope P0/P1 blocks:

```text
FIX486_RUNTIME_BASELINE_PASS
```