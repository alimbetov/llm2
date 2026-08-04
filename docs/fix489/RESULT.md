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
