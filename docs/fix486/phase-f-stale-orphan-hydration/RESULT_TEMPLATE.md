# FIX486F Result Template

Use this template for every official Phase F execution.

Only these verdicts are allowed:

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED
```

---

# PASS Template

## Verdict

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
```

## Identity

| Field | Value |
|---|---|
| Repository | `alimbetov/llm2` |
| Branch | `codex/fix486f-stale-orphan-hydration-proof` |
| Base SHA | `<sha>` |
| Tested source SHA | `<sha>` |
| PR head SHA | `<sha>` |
| Run ID | `<run-id>` |
| Start UTC | `<timestamp>` |
| End UTC | `<timestamp>` |
| Worktree clean | `true` |

## Frozen bank

| Field | Value |
|---|---|
| Version | `1.0.0` |
| Status | `FROZEN` |
| Aggregate SHA-256 | `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff` |
| Payload mutations | `0` |

## Evidence

| Field | Value |
|---|---|
| Evidence root | `<path>` |
| Manifest file SHA-256 | `<sha>` |
| Manifest internal aggregate | `<sha>` |
| Verified files | `<count>` |
| Checksum mismatches | `0` |
| Missing mandatory files | `0` |

## Static validation

| Gate | Result |
|---|---:|
| Python proof compile | `PASS` |
| Shell syntax | `PASS` |
| `cargo fmt --all --check` | `PASS` |
| `cargo check --locked --all-targets --all-features` | `PASS` |
| `cargo clippy ... -D warnings` | `PASS` |
| Full all-target/all-feature tests | `PASS` |
| Phase A contracts | `PASS` |
| Phase C contracts | `PASS` |
| Phase D contracts | `PASS` |
| Phase E contracts | `PASS` |
| Phase F contracts | `PASS` |

## Runtime environment

| Assertion | Result |
|---|---:|
| Phase-owned PostgreSQL | `PASS` |
| Phase-owned Qdrant | `PASS` |
| Port ownership | `PASS` |
| Runtime Health | `PASS` |
| Metrics ownership | `PASS` |
| Binary/config/model/tokenizer hashes | `PASS` |
| Foreign runtime contributors | `0` |

## Runtime row completeness

| Row group | Result |
|---|---:|
| Tier 1 frozen/baseline rows | `12/12 PASS` |
| Ranking controls | `4/4 PASS` |
| Recovery controls | `2/2 PASS` |
| Concurrency controls | `4/4 PASS` |
| Empty-parent control | `2/2 PASS` or `PROVEN_INVARIANT` |
| Search/Retrieve parity | `PASS` |

## Stale deleted-parent result

```text
Search: PASS
RetrieveContext: PASS
raw stale candidate present: true
canonical parent visible: false
final contexts: 0
reason: <DELETED_PARENT or VISIBILITY_REJECTED>
retryable: false
```

## Orphan missing-parent result

```text
Search: PASS
RetrieveContext: PASS
raw orphan candidate present: true
canonical parent exists: false
final contexts: 0
reason: HYDRATION_MISSING
retryable: true
```

## Ranking non-interference

| Assertion | Value |
|---|---:|
| Valid contexts displaced by stale | `0` |
| Required intents lost by stale | `0` |
| Surviving parent set changed | `0` |
| Stale candidate promoted | `0` |
| Candidate refill/selection strategy | `<strategy>` |

## Partial hydration timeout

```text
Search: PASS
RetrieveContext: PASS
status: DEGRADED
contexts: <n greater than 0>
coverage_class: PARTIAL
surviving_parents: <ids>
dropped_parents: <ids>
warning: PARENT_HYDRATION_TIMEOUT
retryable: true
false full coverage: 0
```

## Total hydration timeout

```text
Search: PASS
RetrieveContext: PASS
status: <UNAVAILABLE or DEADLINE_EXCEEDED or compatible strict DEGRADED>
contexts: 0
infrastructure_failure: true
full_hydration_failure: true
retryable: true
content returned: false
false semantic no-answer: 0
```

## Deadline and concurrency

| Assertion | Value |
|---|---:|
| Deadline multiplication | `0` |
| Cross-request failpoint leaks | `0` |
| Healthy request blocked | `0` |
| Negative-cache poisoning | `0` |
| Shared-state poisoning | `0` |
| Concurrency proof | `PASS` |

## Recovery

| Assertion | Result |
|---|---:|
| Recovery without restart | `PASS` |
| Post-fault status | `FOUND` |
| Full contexts restored | `true` |
| Sticky degraded cache | `0` |
| Sticky negative cache | `0` |
| Circuit breaker stuck open | `0` |
| Residual failpoint state | `0` |

## Semantic integrity

| Assertion | Result |
|---|---:|
| Healthy coverage | `FULL` |
| Partial coverage | `PARTIAL` |
| Dropped intents explicit | `PASS` |
| Surviving intents explicit | `PASS` |
| Required anchors preserved | `PASS` |
| Dropped-parent anchors absent | `PASS` |
| Forbidden leakage | `0` |
| Embedding diagnostic | `NON_GATING` or `NOT_RUN` |

## Observability

| Assertion | Result |
|---|---:|
| Metrics contract mapping | `PASS` |
| Reason contract mapping | `PASS` |
| Metric deltas | `PASS` |
| Response/trace reason mismatches | `0` |
| Trace/metric reason mismatches | `0` |
| High-cardinality metric labels | `0` |
| Evidence leaks | `0` |

## Repeatability

| Assertion | Result |
|---|---:|
| Warm repeat | `PASS` |
| Runtime restart | `PASS` |
| Post-restart Health | `PASS` |
| Post-restart healthy baseline | `PASS` |
| Post-restart fault reproduction | `PASS` |
| Post-restart recovery | `PASS` |

## Canonical and Qdrant audits

| Counter | Value |
|---|---:|
| Orphan canonical children | `0` |
| Cross-zone bindings | `0` |
| Cross-document bindings | `0` |
| Cross-version bindings | `0` |
| Duplicate chunks | `0` |
| Duplicate bindings | `0` |
| Failed outbox effects | `0` |
| Dead letters | `0` |
| Unexpected Qdrant points after cleanup | `0` |

## Hard gates

```text
stale_final_contexts = 0
orphan_final_contexts = 0
deleted_parent_contexts = 0
missing_parent_contexts = 0
stale_candidate_promoted = 0
unclassified_stale_drops = 0
valid_contexts_displaced_by_stale = 0
required_intents_lost_by_stale = 0
partial_surviving_contexts_lost = 0
partial_false_no_answer = 0
partial_false_full_coverage = 0
total_false_found = 0
found_with_empty_context = 0
success_no_evidence = 0
false_semantic_no_answer = 0
content_returned_during_total_timeout = 0
deadline_multiplication = 0
cross_request_failpoint_leaks = 0
post_fault_recovery_failures = 0
entry_point_semantic_mismatches = 0
telemetry_reason_mismatches = 0
high_cardinality_metric_labels = 0
evidence_leaks = 0
cleanup_leaks = 0
```

## Cleanup

```text
failpoints active: 0
injected stale points: 0
injected orphan points: 0
phase containers: 0
phase network: removed
phase collection/database: removed according to policy
ports released: true
models preserved: true
evidence preserved: true
```

## Publication

| Field | Value |
|---|---|
| Branch pushed | `true/false` |
| PR updated | `true/false` |
| PR merged | `false unless explicitly approved` |

## Final statement

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
```

This verdict closes Phase F only. It does not declare whole-project production readiness.

---

# BLOCKED Template

## Verdict

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED
```

## Identity

| Field | Value |
|---|---|
| Repository | `alimbetov/llm2` |
| Branch | `codex/fix486f-stale-orphan-hydration-proof` |
| Current SHA | `<sha>` |
| Tested source SHA | `<sha or NOT_REACHED>` |
| Run ID | `<run-id>` |
| Evidence root | `<path>` |

## Failure location

| Field | Value |
|---|---|
| Last completed stage | `<stage>` |
| Failing stage | `<stage>` |
| Failure code | `<code>` |
| Exit code | `<code or UNKNOWN>` |
| Signal | `<signal or NONE>` |
| Primary defect class | `<class>` |

## Partial progress

| Assertion | Value |
|---|---:|
| Static gates completed | `<n>/<expected>` |
| Tier 1 rows completed | `<n>/12` |
| Ranking rows completed | `<n>/4` |
| Recovery rows completed | `<n>/2` |
| Concurrency rows completed | `<n>/4` |
| Search/Retrieve parity | `PASS/FAIL/NOT_REACHED` |
| Warm repeat | `PASS/FAIL/NOT_REACHED` |
| Restart repeat | `PASS/FAIL/NOT_REACHED` |

## Failure details

```text
Scenario: <scenario>
Entry point: <Search/RetrieveContext/both>
Expected: <expected>
Actual: <actual>
Reason: <reason>
```

## Hard-gate deltas

```text
<non-zero or unknown hard gates>
```

## Evidence integrity

| Field | Value |
|---|---|
| Bootstrap present | `true/false` |
| Terminal result present | `true/false` |
| Manifest present | `true/false` |
| Checksums verified | `true/false/not reached` |
| Evidence preserved | `true` |
| Previous evidence overwritten | `false` |

## Cleanup

| Field | Value |
|---|---|
| Failpoint disabled | `true/false` |
| Injected points removed | `true/false` |
| Containers removed | `true/false` |
| Ports released | `true/false` |
| Cleanup failure | `<none/details>` |

## Publication

```text
Branch pushed as PASS: false
PR updated with PASS: false
PR merged: false
```

## Next action

```text
1. Preserve this evidence bundle.
2. Classify the defect.
3. Implement the smallest valid fix.
4. Add a focused regression.
5. Commit separately.
6. Rerun every mandatory gate.
7. Execute the complete official proof on the new clean SHA.
```

## Final statement

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED
```