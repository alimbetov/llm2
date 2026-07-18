# FIX486F — Stale/Orphan and Hydration Degradation Runtime Proof

## 1. Purpose

Phase F proves that AstraVector remains safe and semantically truthful when searchable Qdrant candidates no longer have a valid canonical parent in PostgreSQL, and when canonical parent hydration partially or totally exceeds its deadline.

The phase covers frozen cases:

- `FIX486-05 / q-orphan-child`;
- `FIX486-06 / q-hydration-timeout`.

Allowed verdicts:

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED
```

This specification defines proof requirements only. Runtime implementation, failpoints, runners and evidence generation must not be added until document review is complete.

## 2. Lineage

Phase F starts from `main` after merged Phase E:

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
```

Expected lineage anchor at planning time:

```text
f989eae11176d3f9137b0d3d4fb5418159b90713
```

The actual base SHA used by implementation must be recorded in the future bootstrap and manifest artifacts.

Frozen bank identity remains:

```text
version: 1.0.0
status: FROZEN
aggregate SHA-256:
cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Frozen corpus, queries, qrels, graph and lifecycle payloads must not be modified by Phase F.

## 3. Architectural statement

Qdrant is a searchable projection. PostgreSQL is the canonical source of:

- document lifecycle;
- visibility;
- parent existence;
- version identity;
- access-zone authorization;
- child-to-parent bindings.

Therefore:

```text
Qdrant candidate != authorized final context
```

A candidate can become a final context only after successful canonical validation and parent hydration.

Infrastructure degradation must never be presented as semantic no-answer.

## 4. Scope

### 4.1 FIX486F-A — stale/orphan safety

Mandatory scenarios:

1. stale child point remains while canonical parent/version is deleted or invisible;
2. orphan child point references a non-existent canonical parent;
3. stale/orphan candidates are explicitly classified and rejected;
4. rejected candidates do not displace valid surviving candidates;
5. Search and RetrieveContext expose equivalent semantics.

### 4.2 FIX486F-B — hydration degradation

Mandatory scenarios:

1. healthy baseline hydration;
2. partial timeout: one parent fails, at least one parent survives;
3. total timeout: all required parents fail;
4. recovery after failpoint removal without runtime restart;
5. deterministic repeat after runtime restart;
6. bounded deadline and request-scoped failpoint behavior.

### 4.3 FIX486F-C — semantic integrity and observability

Mandatory controls:

- surviving intent coverage;
- explicit dropped-parent diagnostics;
- response/trace/metric reason consistency;
- concurrency isolation;
- absence of sticky negative/degraded cache;
- empty-parent invariant or controlled runtime scenario;
- fail-closed evidence integrity.

## 5. Out of scope

The following remain independent later phases:

- `FIX486-08` graph relation correctness;
- `FIX486-09` multi-intent/MMR token-budget behavior;
- `FIX486-10` large-parent pressure;
- whole-PostgreSQL outage;
- whole-Qdrant outage;
- general network-partition chaos;
- outbox replay chaos;
- ranking-weight tuning;
- tokenizer changes;
- generated-answer faithfulness.

Phase F validates retrieval and hydration evidence. It does not require a generated LLM answer.

## 6. Frozen scenario expectations

### 6.1 FIX486-05

Frozen query:

```text
q-orphan-child
```

Expected behavior:

- final context count is zero;
- deleted or missing parent content is absent;
- stale child never becomes final evidence;
- drop reason is one of:
  - `HYDRATION_MISSING`;
  - `VISIBILITY_REJECTED`;
  - `DELETED_PARENT`;
- `stale_child_contexts = 0`.

### 6.2 FIX486-06

Frozen query:

```text
q-hydration-timeout
```

Allowed degraded transport states:

```text
DEGRADED
UNAVAILABLE
DEADLINE_EXCEEDED
```

Forbidden states:

```text
FOUND_WITH_EMPTY_CONTEXT
SUCCESS_NO_EVIDENCE
```

Partial timeout must preserve surviving context. Total timeout must not return `FOUND`, semantic no-answer or any content.

## 7. Required response semantics

### 7.1 Healthy baseline

```text
status = FOUND
contexts > 0
warnings = []
coverage_class = FULL
```

### 7.2 Partial hydration timeout

```text
status = DEGRADED
contexts > 0
retryable = true
coverage_class = PARTIAL
warning contains PARENT_HYDRATION_TIMEOUT
surviving_parent_count > 0
dropped_parent_count > 0
```

The response or normalized proof record must identify dropped parents by safe zone-scoped logical identity or equivalent opaque public identifier.

### 7.3 Total hydration timeout

Preferred response:

```text
status = UNAVAILABLE or DEADLINE_EXCEEDED
contexts = 0
retryable = true
reason = PARENT_HYDRATION_TIMEOUT
```

If the transport supports only `DEGRADED`, it must include:

```text
infrastructure_failure = true
full_hydration_failure = true
contexts = 0
retryable = true
```

Forbidden:

- parent content;
- child content as parent substitute;
- metadata-derived answer;
- generated answer;
- context placeholder;
- empty context object;
- `FOUND`;
- `SUCCESS`;
- `INSUFFICIENT_INFORMATION`;
- `NO_RELEVANT_CONTEXT`.

### 7.4 Deleted parent

Deleted or invisible canonical state is not a transient infrastructure failure.

Expected normalized semantics:

```text
final_context_count = 0
retryable = false
reason = DELETED_PARENT or VISIBILITY_REJECTED
```

### 7.5 Missing parent

Missing canonical parent is a consistency/degradation condition.

Expected normalized semantics:

```text
final_context_count = 0
retryable = true
reason = HYDRATION_MISSING
```

It must not be classified as ordinary semantic no-answer.

## 8. Dropped-parent diagnostics

Partial and total degradation must expose, at minimum:

```text
degraded
degradation_class
retryable
dropped_parent_count
dropped_parent_ids
drop_reason
rejection_stage
```

Internal trace additionally records:

```text
request_id
entry_point
access_zone_code
logical_document_id
document_version
logical_parent_id
physical_parent_id
candidate_id
reason_code
rejection_stage
timestamp_utc
elapsed_ms
```

Physical UUIDs must not become high-cardinality metric labels.

## 9. Semantic integrity

The mandatory semantic gate is deterministic evidence coverage, not embedding similarity.

Required artifact:

```text
semantic-integrity.json
```

It must compare healthy and faulted states by:

- required intents;
- surviving intents;
- dropped intents;
- surviving logical parents;
- dropped logical parents;
- required anchor coverage;
- forbidden anchor leakage;
- coverage class.

For partial timeout:

```text
coverage_class = PARTIAL
```

The system must not claim full coverage when a required parent or intent is missing.

Optional diagnostic:

```text
semantic-similarity-diagnostic.json
```

Any embedding-based similarity is non-gating and must not replace deterministic intent/anchor assertions.

## 10. Ranking non-interference

The proof must compare:

1. clean control without stale/orphan point;
2. otherwise identical state with injected stale/orphan point.

After removing the rejected candidate from analysis, the surviving result must remain equivalent by:

- logical parent set;
- content hashes;
- required intent coverage;
- final context count;
- relative order of surviving contexts.

Hard gates:

```text
valid_contexts_displaced_by_stale = 0
required_intents_lost_by_stale = 0
surviving_parent_set_changed = 0
stale_candidate_promoted = 0
```

If stale candidates consume top-k capacity and displace valid candidates, Phase F is blocked. Production remediation may require candidate surplus, canonical filtering before final selection, or refill after rejection.

## 11. Empty-parent policy

Phase F first audits whether blank parent text is impossible by schema and ingestion invariants.

### 11.1 Proven invariant path

If blank content is impossible, evidence must include:

```text
empty-parent-invariant.json
```

It records:

- schema constraint;
- ingestion validation;
- regression test;
- proof that production API cannot create blank parent content.

Result:

```text
EMPTY_PARENT_RUNTIME_SCENARIO = NOT_APPLICABLE_BY_PROVEN_INVARIANT
```

### 11.2 Runtime scenario path

If blank content is possible, run a controlled scenario.

Expected behavior:

```text
drop_reason = EMPTY_CONTEXT or NO_CONTENT
```

Rules:

- empty parent is not hydration timeout;
- empty parent never becomes final context;
- if other contexts survive, result is `DEGRADED`;
- if none survive, result is explicit insufficient evidence, never `FOUND`;
- empty context object is forbidden.

## 12. Fault injection requirements

The failpoint boundary is between candidate selection and canonical parent hydration.

Supported modes:

```text
NONE
RETURN_NOT_FOUND_SELECTED
TIMEOUT_SELECTED_PARENTS
TIMEOUT_ALL_PARENTS
EMPTY_CONTENT_SELECTED   # only when schema permits
```

Required scoping fields:

```text
run_id
request_id
entry_point
access_zone_code
logical_parent_ids
physical_parent_ids
max_activations
hydration_deadline_ms
delay_margin_ms
```

Timeout must be deterministic:

```text
failpoint_delay_ms = hydration_deadline_ms + fixed_margin_ms
```

The future evidence must record configured deadline, configured delay, actual elapsed time and cancellation timestamp.

Forbidden implementation:

- global sleep;
- public production API for failpoint activation;
- detached background fault;
- unbounded activations;
- persistent fault state after cleanup;
- failure of tokenizer, Qdrant retrieval, ingestion or entire gRPC transport.

## 13. Fault-state provenance

Every injected Qdrant point must originate from a production-ingested point.

Before mutation, capture:

```text
point_id
vector hash
payload hash
child physical ID
parent physical ID
logical child ID
logical parent ID
zone
document
version
content hash
```

### 13.1 Stale deleted-parent scenario

1. Production ingestion and projection completion.
2. Capture original point and canonical identities.
3. Delete or make parent/version invisible through production lifecycle path.
4. Confirm canonical deletion/invisibility.
5. Reinsert captured child point into phase-owned Qdrant collection.
6. Execute query and prove rejection.
7. Remove injected point.

### 13.2 Orphan missing-parent scenario

Preferred method:

1. Copy a production point.
2. Assign a phase-owned non-existent `parent_chunk_id`.
3. Preserve remaining provenance fields.
4. Insert only into phase-owned Qdrant collection.
5. Execute query and prove `HYDRATION_MISSING`.
6. Remove injected point.

Do not violate PostgreSQL foreign-key integrity by deleting canonical rows directly unless production schema explicitly supports that test path.

## 14. Concurrency proof

This is a bounded semantic isolation control, not a load test.

### 14.1 Same-parent request isolation

Run concurrently:

```text
Request A: failpoint active, expected timeout
Request B: failpoint inactive, expected success
```

Hard gates:

```text
cross_request_failpoint_leak = 0
healthy_request_blocked_by_faulted_request = 0
negative_cache_poisoning = 0
shared_future_poisoning = 0
global_timeout_contamination = 0
```

### 14.2 Partial multi-parent request

One request requires:

```text
parent-a1 -> success
parent-a3 -> timeout
```

Expected:

```text
DEGRADED
parent-a1 survives
parent-a3 is explicitly dropped
```

Observed latency must satisfy:

```text
observed_latency <= request_deadline + allowed_jitter
```

Per-parent retry behavior must not multiply the overall request deadline.

## 15. Recovery proof

Restart alone is insufficient.

Mandatory sequence:

1. healthy baseline;
2. activate timeout failpoint;
3. observe expected degraded/unavailable response;
4. disable failpoint;
5. do not restart runtime;
6. repeat identical request;
7. require full `FOUND` result;
8. verify no sticky degraded or negative cache;
9. only then perform runtime restart repeat.

Hard gates:

```text
post_fault_status = FOUND
post_fault_full_contexts_restored = true
sticky_degraded_cache = 0
sticky_negative_cache = 0
circuit_breaker_stuck_open = 0
faultpoint_residual_state = 0
```

## 16. Observability

Required semantic metric families, using project naming conventions:

```text
parent_hydration_requests_total{entry_point,outcome}
parent_hydration_duration_seconds{entry_point,outcome}
candidate_rejections_total{entry_point,reason}
degraded_requests_total{entry_point,reason}
stale_candidate_rejections_total{entry_point,reason}
hydration_timeouts_total{entry_point,scope}
```

The implementation may use different names, but must publish:

```text
metrics-contract-map.json
```

Forbidden labels:

- parent UUID;
- document UUID;
- request ID;
- user ID.

Request-level metric deltas must equal expected request counts. Attempt-level metrics must match the configured retry policy.

Response, trace and metrics must use consistent reason categories.

## 17. Search/RetrieveContext parity

For each scenario compare:

- status class;
- infrastructure versus semantic classification;
- context count;
- surviving parents;
- dropped parents;
- drop reasons;
- retryable flag;
- warning codes;
- required intent coverage;
- forbidden anchor leakage.

Non-semantic fields may differ:

- request IDs;
- trace IDs;
- timestamps;
- latency;
- floating-point scores.

Hard gate:

```text
entry_point_semantic_mismatches = 0
```

## 18. Runtime matrix

### Tier 1 — frozen and baseline

| Scenario | Search | RetrieveContext | Rows |
|---|---:|---:|---:|
| clean stale-query baseline | 1 | 1 | 2 |
| healthy hydration baseline | 1 | 1 | 2 |
| stale child to deleted parent | 1 | 1 | 2 |
| orphan child to missing parent | 1 | 1 | 2 |
| partial hydration timeout | 1 | 1 | 2 |
| total hydration timeout | 1 | 1 | 2 |

Tier 1 requires `12/12` normalized rows.

### Tier 2 — mandatory controls

| Control | Search | RetrieveContext | Rows |
|---|---:|---:|---:|
| ranking clean control | 1 | 1 | 2 |
| ranking stale control | 1 | 1 | 2 |
| recovery without restart | 1 | 1 | 2 |
| concurrent healthy/faulted isolation | 2 | 2 | 4 |
| empty parent | 1 | 1 | 2 or proven invariant |

### Tier 3 — repeatability

Repeat critical stale, orphan, partial timeout, total timeout, recovery and concurrency semantics through:

- warm repeat;
- runtime restart repeat.

## 19. Hard gates

### Stale/orphan

```text
stale_final_contexts = 0
orphan_final_contexts = 0
deleted_parent_contexts = 0
missing_parent_contexts = 0
stale_candidate_promoted = 0
unclassified_stale_drops = 0
valid_contexts_displaced_by_stale = 0
required_intents_lost_by_stale = 0
```

### Partial timeout

```text
partial_surviving_contexts_lost = 0
partial_status_not_degraded = 0
partial_warning_missing = 0
partial_dropped_parent_missing = 0
partial_false_no_answer = 0
partial_false_full_coverage = 0
```

### Total timeout

```text
total_false_found = 0
found_with_empty_context = 0
success_no_evidence = 0
false_semantic_no_answer = 0
empty_parent_contexts = 0
timeout_without_explicit_status = 0
content_returned_during_total_timeout = 0
```

### Recovery and concurrency

```text
post_fault_recovery_failures = 0
sticky_degraded_cache = 0
sticky_negative_cache = 0
circuit_breaker_stuck_open = 0
faultpoint_residual_state = 0
cross_request_failpoint_leaks = 0
healthy_request_blocked = 0
shared_cache_poisoning = 0
deadline_multiplication = 0
```

### General

```text
cross_zone_results = 0
wrong_version_results = 0
dead_letters = 0
unexpected_outbox_failures = 0
unknown_failure_classifications = 0
telemetry_reason_mismatches = 0
cleanup_leaks = 0
```

## 20. Evidence root

Future official evidence root:

```text
astravector-evidence/fix486f/<run-id>/
```

Required evidence is specified in `EXECUTION_AND_EVIDENCE_CONTRACT.md`.

## 21. Production capability audit

Before runtime implementation, Codex must document:

- current response schema;
- status and warning enums;
- hydration repository boundary;
- retry and deadline policy;
- concurrency model;
- negative cache and circuit-breaker behavior;
- metrics naming conventions;
- blank-content invariant;
- production deletion path;
- Qdrant payload identity.

Artifact:

```text
capability-audit.md
```

No implementation decision should be based on assumptions that contradict the current production path.

## 22. Completion boundary

Phase F PASS proves:

- canonical authority over stale vector candidates;
- explicit stale/orphan rejection;
- no stale ranking interference;
- truthful partial and total hydration degradation;
- surviving evidence preservation;
- explicit dropped-parent diagnostics;
- bounded deadlines;
- request-scoped concurrency isolation;
- observability consistency;
- recovery without restart;
- warm and restart repeatability.

Phase F does not declare the entire project production-ready.