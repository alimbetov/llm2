# FIX486F Contract RED Baseline

## Identity

```text
source SHA: 491ab56a75f88cb99514efd5cd5fcd12c4a2793d
branch: codex/fix486f-stale-orphan-hydration-proof
command: cargo test --locked --test fix486f_failure_semantics_contracts -- --nocapture
expected: 11 focused contracts compile, execute and fail on current capability gaps
```

## Baseline

The observed failure details are filled from the first execution before production
runtime changes. A failure caused by compilation, missing dependency or unrelated
test setup invalidates this baseline.

| Contract | Expected current failure | Observed failure | Classification |
|---|---|---|---|
| Binding-backed parent validation | canonical binding join absent | persistence source lacks binding key/join, child-parent relation and `BINDING_INVALID` classification | `FIX486F-P0-001` |
| Exhaustive outcomes | terminal outcome model absent | hydration module and exhaustive ordinal invariant absent | `FIX486F-HYDR-OUTCOME-001` |
| Missing versus invalid binding | reasons not distinguished | `BindingInvalid` and `HydrationMissing` terminal variants absent | `FIX486F-ORPHAN-SEMANTICS-001` |
| Partial timeout | no surviving/dropped degradation model | timeout outcome, partial coverage, survivors and dropped-parent model absent | `FIX486F-HYDR-001/003` |
| Total timeout | no structured total-timeout helper | deadline status/details and no-body contract absent | `FIX486F-HYDR-002` |
| Rejection reserve | bounded reserve absent | search configuration has no bounded hydration rejection reserve | `FIX486F-STALE-002` |
| Request-scoped failpoint | immutable bounded plan absent | correlation-scoped plan, modes and activation bound absent | `FIX486F-CONC-001` |
| Empty parent | hydration guard absent | trim-based PARENT rejection and `EMPTY_CONTEXT` outcome absent | `FIX486F-CONTENT-001` |
| Metrics | semantic bounded-label metrics absent | outcome/reason/scope hydration metric families absent | `FIX486F-OBS-001` |
| Entry-point parity | shared normalizer absent | shared `normalize_hydration_outcomes` path absent | `FIX486F-PARITY-001` |
| One batch/no N+1 | binding-backed ordinal batch absent | existing ordinality query lacks exhaustive batch outcome API and input ordinal | `FIX486F-BATCH-001` |

Observed command result:

```text
test result: FAILED. 0 passed; 11 failed; 0 ignored
exit code: 101
compile: PASS
execution: PASS
unrelated setup failures: 0
```

## Gate

```text
RED_BASELINE_CAPTURED = true
PRODUCTION_FIXES_ALLOWED = true
```
