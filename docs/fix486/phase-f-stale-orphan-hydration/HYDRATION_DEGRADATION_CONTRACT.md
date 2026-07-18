# FIX486F Hydration Degradation Contract

## 1. Contract objective

This contract proves truthful, bounded and recoverable behavior when Qdrant candidate retrieval succeeds but canonical parent hydration in PostgreSQL partially or totally exceeds its deadline.

Primary frozen case:

```text
FIX486-06 / q-hydration-timeout
```

The proof must distinguish infrastructure degradation from semantic no-answer.

## 2. Frozen lifecycle scenarios

The lifecycle seed defines two mandatory hydration modes.

### Partial timeout

```text
affected_parent = parent-a3
unaffected_parent = parent-a1
expected = SUCCESS_DEGRADED_WITH_SURVIVING_CONTEXT
forbidden = SUCCESS_NO_EVIDENCE
```

### Total timeout

```text
affected_parents = parent-a1,parent-a2,parent-a3
expected_any = UNAVAILABLE, DEADLINE_EXCEEDED, DEGRADED_WITHOUT_FALSE_FOUND
forbidden_any = SUCCESS_NO_EVIDENCE, FOUND_WITH_EMPTY_PARENT
```

## 3. Healthy baseline

Before any failpoint activation, execute the same hydration query through Search and RetrieveContext.

Required baseline:

```text
status = FOUND
contexts > 0
warnings = []
coverage_class = FULL
dropped_parent_count = 0
```

Capture:

- candidate parents;
- hydrated parents;
- logical parent IDs;
- required intents;
- required anchors;
- final order;
- context hashes;
- request deadline;
- observed latency.

The baseline proves that the query and fixture are capable of returning the expected evidence before fault injection.

## 4. Partial timeout semantics

### 4.1 Setup

The request must require at least two independently hydratable parents.

Target setup:

```text
parent-a1 -> normal hydration
parent-a3 -> deterministic timeout
```

The failpoint must be request-scoped and parent-scoped.

### 4.2 Required result

```text
status = DEGRADED
contexts > 0
retryable = true
coverage_class = PARTIAL
surviving_parent_ids contains parent-a1
dropped_parent_ids contains parent-a3
warning contains PARENT_HYDRATION_TIMEOUT
```

The response or normalized proof record must contain an explicit dropped-parent structure.

### 4.3 Semantic integrity

The final context must preserve evidence from surviving parents and must not contain evidence attributed only to the timed-out parent.

Required checks:

- surviving required intents remain represented;
- dropped required intents are explicit;
- no false `FULL_COVERAGE` claim;
- surviving anchors remain present;
- dropped-parent anchors are absent;
- forbidden anchors remain absent.

### 4.4 Forbidden result

```text
FOUND
SUCCESS_NO_EVIDENCE
FOUND_WITH_EMPTY_CONTEXT
INSUFFICIENT_INFORMATION
NO_RELEVANT_CONTEXT
```

Partial infrastructure failure must not become semantic no-answer.

### 4.5 Surviving-context preservation

Hard gates:

```text
partial_surviving_contexts_lost = 0
partial_false_no_answer = 0
partial_false_full_coverage = 0
partial_warning_missing = 0
partial_dropped_parent_missing = 0
```

## 5. Total timeout semantics

### 5.1 Setup

All parents required by the request exceed the hydration deadline.

Target parents:

```text
parent-a1
parent-a2
parent-a3
```

### 5.2 Preferred result

```text
status = UNAVAILABLE or DEADLINE_EXCEEDED
contexts = 0
retryable = true
reason = PARENT_HYDRATION_TIMEOUT
full_hydration_failure = true
```

### 5.3 Compatible degraded result

If the transport cannot represent unavailable/deadline directly, the only acceptable degraded result is:

```text
status = DEGRADED
infrastructure_failure = true
full_hydration_failure = true
contexts = 0
retryable = true
reason = PARENT_HYDRATION_TIMEOUT
```

### 5.4 Content prohibition

During total timeout, the response must not contain:

- hydrated parent text;
- child text substituted for parent text;
- generated answer;
- metadata-derived answer;
- placeholder context;
- empty context object presented as success.

### 5.5 Forbidden classifications

```text
FOUND
SUCCESS
SUCCESS_NO_EVIDENCE
FOUND_WITH_EMPTY_CONTEXT
FOUND_WITH_EMPTY_PARENT
INSUFFICIENT_INFORMATION
NO_RELEVANT_CONTEXT
```

### 5.6 Hard gates

```text
total_false_found = 0
found_with_empty_context = 0
success_no_evidence = 0
false_semantic_no_answer = 0
empty_parent_contexts = 0
timeout_without_explicit_status = 0
content_returned_during_total_timeout = 0
```

## 6. Dropped-parent response contract

A degraded response must expose equivalent semantics to:

```json
{
  "degradation": {
    "degraded": true,
    "class": "PARTIAL_HYDRATION_TIMEOUT",
    "retryable": true,
    "dropped_parent_count": 1,
    "dropped_parents": [
      {
        "logical_parent_id": "parent-a3",
        "reason": "PARENT_HYDRATION_TIMEOUT",
        "stage": "CANONICAL_PARENT_HYDRATION"
      }
    ]
  }
}
```

Field names may differ if existing protobuf/API conventions provide equivalent information.

Public diagnostics must use safe zone-scoped logical or opaque identities. Internal trace may contain physical UUIDs.

## 7. Failpoint boundary

The failpoint must execute after candidate selection and before/during canonical parent hydration.

It must not fail:

- Qdrant search;
- tokenizer;
- ingestion;
- entire PostgreSQL service;
- whole gRPC transport;
- ranking-weight calculation.

Supported modes:

```text
NONE
TIMEOUT_SELECTED_PARENTS
TIMEOUT_ALL_PARENTS
RETURN_NOT_FOUND_SELECTED
```

## 8. Deterministic deadline behavior

Configured timeout delay:

```text
failpoint_delay_ms = hydration_deadline_ms + fixed_margin_ms
```

Evidence must record:

```text
request_deadline_ms
hydration_deadline_ms
failpoint_delay_ms
fixed_margin_ms
actual_elapsed_ms
cancellation_timestamp_utc
retry_count
```

Retries must not multiply the overall request deadline.

Hard gate:

```text
observed_latency <= request_deadline + allowed_jitter
```

Any deadline multiplication blocks PASS.

## 9. Request-scoped activation

Failpoint matching must include sufficient scope:

```text
run_id
request_id
entry_point
access_zone_code
logical_parent_ids
physical_parent_ids
max_activations
```

Global failpoint activation is forbidden.

A timeout intended for Request A must not affect Request B unless Request B independently matches the activation contract.

## 10. Concurrency isolation

### 10.1 Same-parent parallel requests

Run concurrently:

```text
Request A: same parent, failpoint active, expected timeout
Request B: same parent, failpoint inactive, expected success
```

Required outcomes:

- A receives expected degraded/unavailable semantics;
- B receives healthy `FOUND` semantics;
- B is not blocked until A's failpoint delay expires;
- no negative cache or shared future carries A's failure into B.

Hard gates:

```text
cross_request_failpoint_leak = 0
healthy_request_blocked_by_faulted_request = 0
negative_cache_poisoning = 0
shared_future_poisoning = 0
global_timeout_contamination = 0
```

### 10.2 Multi-parent partial request

Within one request:

```text
parent-a1 -> success
parent-a3 -> timeout
```

The successful parent must be returned without waiting for serial multiplication of all possible parent deadlines.

## 11. Recovery without restart

Mandatory sequence:

1. healthy baseline;
2. activate timeout failpoint;
3. execute partial and/or total timeout request;
4. verify expected fault response;
5. disable failpoint;
6. keep runtime running;
7. execute identical request;
8. require full healthy result.

Required post-fault result:

```text
status = FOUND
coverage_class = FULL
warnings = []
dropped_parent_count = 0
full contexts restored
```

Hard gates:

```text
post_fault_recovery_failures = 0
sticky_degraded_cache = 0
sticky_negative_cache = 0
circuit_breaker_stuck_open = 0
faultpoint_residual_state = 0
```

## 12. Restart repeat

After successful recovery without restart:

1. restart only AstraVector runtime;
2. preserve PostgreSQL and Qdrant;
3. verify failpoint is disabled by default;
4. execute healthy baseline;
5. explicitly reactivate partial failpoint;
6. repeat partial semantics;
7. explicitly reactivate total failpoint;
8. repeat total semantics;
9. disable failpoint and verify recovery.

Restart must not persist a hidden fault state.

## 13. Empty-parent capability gate

Before runtime fault work, audit whether blank canonical parent content is possible.

### Proven invariant

If impossible, provide:

```text
empty-parent-invariant.json
```

It must identify schema and ingestion enforcement.

### Runtime scenario

If possible, run:

```text
parent exists
visibility valid
content empty or whitespace-only
```

Expected classification:

```text
EMPTY_CONTEXT or NO_CONTENT
```

It must not be classified as hydration timeout.

Hard gates:

```text
empty_parent_final_contexts = 0
empty_context_false_success = 0
empty_context_misclassified_as_timeout = 0
```

## 14. Semantic integrity artifact

Required file:

```text
semantic-integrity.json
```

Required structure:

```text
healthy_required_intents
healthy_parent_coverage
surviving_intents
dropped_intents
surviving_parent_ids
dropped_parent_ids
required_anchor_coverage
forbidden_anchor_leakage
coverage_class
```

Embedding similarity may be stored in:

```text
semantic-similarity-diagnostic.json
```

It is diagnostic only and cannot override deterministic proof results.

## 15. Observability contract

The implementation must expose metrics equivalent to:

```text
parent_hydration_requests_total{entry_point,outcome}
parent_hydration_duration_seconds{entry_point,outcome}
hydration_timeouts_total{entry_point,scope}
degraded_requests_total{entry_point,reason}
```

Required evidence snapshots:

```text
metrics-before.txt
metrics-after-partial-timeout.txt
metrics-after-total-timeout.txt
metrics-after-recovery.txt
metrics-delta.json
```

Metric labels must not contain parent/document/request UUIDs.

## 16. Diagnostic propagation

For each timed-out parent, internal trace records:

```text
request_id
entry_point
access_zone_code
logical_document_id
document_version
logical_parent_id
physical_parent_id
reason_code
retryable
timestamp_utc
elapsed_ms
```

Response, trace and metric reason categories must agree.

Hard gate:

```text
telemetry_reason_mismatches = 0
```

## 17. Search/RetrieveContext parity

Both entry points must agree on:

- healthy status and coverage;
- partial degraded status;
- total infrastructure failure status;
- surviving parents;
- dropped parents;
- reason codes;
- retryable flag;
- warning codes;
- context count;
- semantic versus infrastructure classification.

Hard gate:

```text
hydration_entry_point_mismatches = 0
```

## 18. Warm repeat

Without re-ingestion, repeat:

- healthy baseline;
- partial timeout;
- total timeout;
- recovery without restart.

Required stability:

- same semantic status classes;
- same surviving/dropped parent sets;
- bounded latency;
- no duplicate canonical or Qdrant state;
- deterministic metric deltas.

## 19. Evidence artifacts

Minimum hydration evidence:

```text
hydration-baseline.json
fault-plan.json
fault-activation.json
hydration-partial-timeout.json
hydration-total-timeout.json
hydration-deadline-audit.json
surviving-context-proof.json
semantic-integrity.json
semantic-similarity-diagnostic.json
concurrency-isolation.json
recovery-without-restart.json
restart-recovery.json
metrics-delta.json
diagnostic-propagation-audit.json
fault-cleanup.json
```

## 20. Contract completion

Hydration proof passes only when:

```text
healthy baseline = PASS
partial surviving contexts > 0
partial status = DEGRADED
partial dropped parents explicit
total false FOUND = 0
total content returned = 0
false semantic no-answer = 0
deadline multiplication = 0
cross-request fault leakage = 0
recovery without restart = PASS
restart repeat = PASS
Search/RetrieveContext parity = PASS
telemetry consistency = PASS
cleanup leaks = 0
```

Any violation produces the phase-level BLOCKED verdict.