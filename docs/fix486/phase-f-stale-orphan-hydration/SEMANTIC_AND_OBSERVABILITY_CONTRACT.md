# FIX486F Semantic Integrity and Observability Contract

## 1. Objective

This contract proves that degraded retrieval remains semantically honest, client-diagnosable and operationally observable.

It does not evaluate generated LLM answers. The mandatory proof surface is retrieval evidence returned by Search and RetrieveContext.

## 2. Deterministic semantic integrity

The primary semantic gate is based on intents, parent coverage, content hashes and anchors.

Required artifact:

```text
semantic-integrity.json
```

Required fields:

```text
scenario
entry_point
healthy_required_intents
healthy_parent_ids
healthy_content_hashes
healthy_required_anchors
surviving_intents
dropped_intents
surviving_parent_ids
dropped_parent_ids
surviving_content_hashes
required_anchor_coverage
forbidden_anchor_leakage
coverage_class
false_full_coverage
```

### Healthy

```text
coverage_class = FULL
false_full_coverage = false
dropped_intents = []
```

### Partial hydration timeout

```text
coverage_class = PARTIAL
surviving_intents is non-empty
dropped_intents is non-empty
false_full_coverage = false
```

### Total hydration timeout

```text
coverage_class = NONE_DUE_TO_INFRASTRUCTURE_FAILURE
surviving_intents = []
contexts = 0
```

It must not use ordinary semantic insufficient/no-answer classification.

## 3. Non-gating embedding diagnostic

Optional artifact:

```text
semantic-similarity-diagnostic.json
```

It may compare embeddings for healthy and partial context sets.

Rules:

- diagnostic only;
- model identity and hash recorded;
- no PASS override;
- no replacement for intent/anchor assertions;
- absence does not block PASS unless implementation explicitly declares it mandatory later.

## 4. Dropped-parent client contract

A degraded result must expose equivalent semantics to:

```text
degraded = true
degradation_class
retryable
dropped_parent_count
dropped_parent_ids
drop_reasons
rejection_stages
```

Public IDs must be safe zone-scoped logical identifiers or opaque identifiers.

Internal UUIDs are allowed only in protected trace/evidence.

## 5. Reason taxonomy

Closed reason enum for Phase F:

```text
DELETED_PARENT
VISIBILITY_REJECTED
HYDRATION_MISSING
PARENT_HYDRATION_TIMEOUT
EMPTY_CONTEXT
NO_CONTENT
BINDING_INVALID
VERSION_INVISIBLE
```

Every rejected/timed-out candidate must resolve to one of these or a documented production-equivalent enum mapped in:

```text
reason-contract-map.json
```

`UNKNOWN`, free-form reason text without enum and missing reason are forbidden.

## 6. Response/trace/metric consistency

For every scenario, normalized evidence must compare:

```text
response_reason
trace_reason
metric_reason
response_retryable
trace_retryable
response_stage
trace_stage
```

Required artifact:

```text
diagnostic-propagation-audit.json
```

Hard gates:

```text
response_trace_reason_mismatches = 0
trace_metric_reason_mismatches = 0
retryable_mismatches = 0
rejection_stage_mismatches = 0
```

## 7. Structured trace requirements

Each rejection or timeout trace records:

```text
request_id
entry_point
run_id
access_zone_code
logical_document_id
document_version
logical_child_id
logical_parent_id
candidate_id
physical_parent_id
reason_code
rejection_stage
retryable
timestamp_utc
elapsed_ms
```

Privacy and cardinality rules:

- foreign-zone data must not leak;
- parent content is not required in rejection logs;
- request/parent/document UUIDs must not be metric labels;
- physical IDs stay internal;
- timestamps use UTC.

## 8. Metric semantics

The project may retain its naming conventions, but must expose metrics semantically equivalent to:

```text
parent_hydration_requests_total{entry_point,outcome}
parent_hydration_duration_seconds{entry_point,outcome}
hydration_timeouts_total{entry_point,scope}
candidate_rejections_total{entry_point,reason}
stale_candidate_rejections_total{entry_point,reason}
degraded_requests_total{entry_point,reason}
```

Required mapping artifact:

```text
metrics-contract-map.json
```

## 9. Metric cardinality

Allowed labels include bounded enums:

```text
entry_point
outcome
reason
scope
```

Forbidden high-cardinality labels:

```text
request_id
user_id
parent_uuid
document_uuid
chunk_uuid
point_id
```

Hard gate:

```text
high_cardinality_metric_labels = 0
```

## 10. Metric evidence

Required snapshots:

```text
metrics-before.txt
metrics-after-stale.txt
metrics-after-orphan.txt
metrics-after-partial-timeout.txt
metrics-after-total-timeout.txt
metrics-after-recovery.txt
metrics-delta.json
```

`metrics-delta.json` must state whether each metric is request-level or attempt-level.

Request-level exact deltas must equal executed request counts.

Attempt-level deltas must equal documented retry behavior.

Unexpected retries block PASS when they multiply deadlines or evidence counts.

## 11. Ranking semantic integrity

Presence of stale/orphan points must not alter valid surviving evidence.

Required comparison:

```text
clean parent set == faulted surviving parent set
clean content hashes == faulted surviving content hashes
clean required intent coverage == faulted surviving required intent coverage
clean final count == faulted surviving final count
clean relative order == faulted surviving relative order
```

Score tolerance may be documented, but structural and coverage assertions are exact.

Artifact:

```text
ranking-non-interference.json
```

## 12. Partial degradation honesty

Partial timeout must not claim full coverage.

Required:

```text
status = DEGRADED
coverage_class = PARTIAL
dropped_parent_count > 0
dropped_intents > 0
```

Hard gates:

```text
partial_false_full_coverage = 0
partial_false_no_answer = 0
partial_surviving_contexts_lost = 0
```

## 13. Total degradation honesty

Total timeout is an infrastructure result.

Required:

```text
contexts = 0
infrastructure_failure = true
retryable = true
```

Forbidden:

```text
FOUND
SUCCESS
semantic no-answer
content placeholder
generated answer
```

Hard gates:

```text
false_semantic_no_answer = 0
content_returned_during_total_timeout = 0
total_false_found = 0
```

## 14. Recovery observability

After failpoint removal without restart:

- healthy result returns;
- degraded request counters stop incrementing;
- successful hydration counters increment;
- faultpoint-active gauge or equivalent returns to zero if exposed;
- no sticky circuit-breaker/negative-cache state remains.

Required artifacts:

```text
recovery-without-restart.json
metrics-after-recovery.txt
```

## 15. Concurrency observability

Parallel healthy and faulted requests must be independently identifiable by request ID in traces, without request ID labels in metrics.

Required:

- one trace shows timeout;
- one trace shows success;
- no reason leakage between traces;
- metric deltas show one timeout and one success;
- healthy latency is not bounded by fault delay.

Artifact:

```text
concurrency-isolation.json
```

## 16. Search/RetrieveContext semantic parity

Both entry points must agree on:

```text
status class
coverage class
surviving parent set
dropped parent set
reason classes
retryable class
context count
forbidden leakage
```

Hard gate:

```text
entry_point_semantic_mismatches = 0
```

## 17. Evidence leak scan

Evidence and logs must not expose:

- parent content from total timeout;
- deleted parent content;
- foreign-zone identities or text;
- unredacted secrets;
- internal physical IDs in public response artifacts.

Artifact:

```text
evidence-leak-scan.json
```

Hard gate:

```text
evidence_leaks = 0
```

## 18. Contract completion

Semantic and observability proof passes only when:

```text
semantic integrity = PASS
false full coverage = 0
response/trace/metric mismatches = 0
high-cardinality metric labels = 0
ranking interference = 0
recovery observability = PASS
concurrency observability = PASS
entry-point semantic mismatches = 0
evidence leaks = 0
```

Any violation blocks Phase F.