# FIX489 Result

Status:

```text
LIVE_CLIENT_SMOKE_PASS
```

Implemented in this phase:

- `scripts/astravector_live_client.py` reusable production-path client;
- FIX488 local demo now delegates shared model/gRPC/block/audit helpers to the reusable client;
- `scripts/fix489_live_capacity.py` live mixed-load executor;
- capacity shell entrypoint no longer ends in `LIVE_CAPACITY_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN`;
- soak shell entrypoint no longer ends in `LIVE_SOAK_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN`;
- contract tests for the shared client and live capacity wiring.

Required final verdicts remain pending until the official live campaign is executed:
Live mixed-load client smoke:

```text
verdict: FIX489_LIVE_MIXED_LOAD_CLIENT_PASS
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489/operation-smoke-20260804T152510Z
completed_operations: 7
grpc_statuses.OK: 7
success_rate: 1.0
operation_types_observed:
- SEARCH
- RETRIEVE_CONTEXT
- GRAPH_RETRIEVE_CONTEXT
- INGEST_VERSION
- DELETE_OR_EXPIRE
- SYNC_STATUS
- LIFECYCLE_STATUS
UNKNOWN: 0
unexpected_INTERNAL: 0
orphan_binding_count: 0
orphan_outbox_count: 0
failed_outbox: 0
p50_ms: 254.0
p95_ms: 1508.0
p99_ms: 1508.0
```

Latest post-repair live mixed-load client smoke:

```text
verdict: FIX489_LIVE_MIXED_LOAD_CLIENT_PASS
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489/operation-smoke-20260804T172206Z
completed_operations: 7
grpc_statuses.OK: 7
success_rate: 1.0
operation_types_observed:
- SEARCH
- RETRIEVE_CONTEXT
- GRAPH_RETRIEVE_CONTEXT
- INGEST_VERSION
- DELETE_OR_EXPIRE
- SYNC_STATUS
- LIFECYCLE_STATUS
UNKNOWN: 0
unexpected_INTERNAL: 0
orphan_binding_count: 0
orphan_outbox_count: 0
failed_outbox: 0
p50_ms: 292.0
p95_ms: 1528.0
p99_ms: 1528.0
```

Short developer capacity slice:

```text
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489/dev-live-slice-20260804T152125Z
concurrency: 2
completed_operations: 5
grpc_statuses.OK: 5
success_rate: 1.0
classification: STABLE
```

Required final verdicts remain pending until the official 25/50/100/200 capacity campaign and 60-minute soak are executed:

```text
FIX489_CAPACITY_CAMPAIGN_PASS
FIX489_SOAK_60M_PASS
```

Interrupted official capacity attempt:

```text
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix487bc/fix487bc-20260804T153216Z
concurrency-25: FAILED before repair
concurrency-50: FAILED before repair
observed cause: grpcurl returned camel-case Code: DeadlineExceeded, but the harness classified only DEADLINE_EXCEEDED and therefore counted controlled CPU saturation as UNKNOWN hard failure.
repair: normalize grpcurl status spellings to canonical gRPC status codes and run FIX489 capacity/soak with the bounded fix489-capacity operational deadline profile.
production retrieval semantics changed: no
```

Second blocked attempt:

```text
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489/operation-smoke-20260804T171252Z
observed cause: repeated runs reused static deterministic namespace fix489, so a stale REGISTERED document version with zero chunks could be selected again by the harness.
repair: make FIX489 live workload document namespaces run-scoped from the evidence directory name.
production retrieval semantics changed: no
```

Third blocked attempt:

```text
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix487bc/fix487bc-20260804T172529Z
tested_sha: 3829e9dd8f033c9d5437ea803d29ee3d87b81f15
concurrency-25: SATURATED_CONTROLLED
concurrency-25_completed_operations: 3757
concurrency-25_UNKNOWN: 0
concurrency-25_safety_counters: 0
concurrency-50: FAILED
concurrency-50_completed_operations: 1250
concurrency-50_UNKNOWN: 3
observed cause: measured DELETE_OR_EXPIRE created, embedded, activated and then deleted a fresh document inside the measured operation path; under concurrency 50 the hidden ingest/projection wait produced OUTBOX_NOT_COMPLETED tail failures.
repair: prepare phase-owned delete-control documents before measurement; measured DELETE_OR_EXPIRE now calls only the production delete API for a prepared active document.
production retrieval semantics changed: no
```

Post delete-pool repair targeted evidence:

```text
operation_smoke: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489/operation-smoke-delete-pool-20260804T200203Z
operation_smoke_verdict: FIX489_LIVE_MIXED_LOAD_CLIENT_PASS
operation_smoke_UNKNOWN: 0
operation_smoke_success_rate: 1.0

capacity_50_slice: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489/capacity-50-delete-pool-20260804T200322Z
capacity_50_slice_verdict: SATURATED_CONTROLLED
capacity_50_completed_operations: 1425
capacity_50_UNKNOWN: 0
capacity_50_OUTBOX_NOT_COMPLETED: 0
capacity_50_DELETE_OR_EXPIRE: 70 OK / DELETE_SCHEDULED
capacity_50_resource_exhausted_rate: 0.018947368421052633
capacity_50_safety_counters: 0
```

Post 50-percent operational budget targeted evidence:

```text
runtime_profile: fix489-capacity
query_deadline_ms: 67500
postgres_statement_timeout_ms: 45000
qdrant_timeout_ms: 22500
query_max_queue_age_ms: 750
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489/capacity-25-budget67500-delete-pool-20260805T014427Z
concurrency: 25
completed_operations: 1743
grpc_statuses.OK: 1730
grpc_statuses.RESOURCE_EXHAUSTED: 13
UNKNOWN: 0
unexpected_INTERNAL: 0
resource_exhausted_rate: 0.007458405048766495
success_rate: 0.9925415949512335
p50_ms: 5808.0
p95_ms: 7215.0
p99_ms: 9524.0
safety_counters: 0
verdict: SATURATED_CONTROLLED
classification: local Mac CPU stable capacity remains below the fixed official first level of 25, or the load generator needs explicit request pacing before 25 can be used as a stable floor.
```

FIX489-R1 per-document vector readiness diagnostics:

```text
tested_sha: b86e41c373338d2743256b563ed45e25f9c1998a
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489-r1/readiness-20260806T160619Z
verdict: FIX489_VECTOR_READINESS_DIAGNOSTICS_PASS
run_a_ready: 1/1
run_b_ready: 9/9
run_b_blocked: 0
run_c_ready: 9/9
blocker_counts: {}
no_generic_timeout_reason: true
poll_timeline_captured: true
postgres_per_document_diagnostics_captured: true
qdrant_per_document_diagnostics_captured: true
admin_debug_document_attempted: true
retrieval_freeze: PASS
production_retrieval_semantics_changed: no
capacity_ladder_executed: no
decision: previous prepare_documents OUTBOX_NOT_COMPLETED blocker did not reproduce on the isolated R1 diagnostic route; continue with official capacity only from a clean runtime and preserve the new per-document diagnostics if it recurs.
```
