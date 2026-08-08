# FIX489 Capacity Result

Current status:

```text
FIX489_LOCAL_CAPACITY_CAMPAIGN_BLOCKED
```

Latest tested code:

```text
branch: agent/fix489-live-capacity-soak
sha: 41a23dea956c4e3b90e23ccf654be10abce15b3b
```

Validated gates:

```text
cargo fmt --all --check: PASS
cargo check --locked --all-targets --all-features: PASS
cargo clippy --locked --all-targets --all-features -- -D warnings: PASS
cargo test --locked --all-targets --all-features: PASS
make verify-fix489-live-capacity-contracts: PASS
focused FIX489 Python contracts: 40/40 PASS
```

Latest targeted live evidence:

```text
evidence: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489-capacity/fix489-capacity-20260808T035516Z
terminal_status: BLOCKED
terminal_reason: LIVE_CAPACITY_RUN_FAILED
campaign_reason: NO_STABLE_LEVEL_ON_LOCAL_HARDWARE
```

Observed level results:

```text
concurrency=5:
  verdict: SATURATED_CONTROLLED
  completed_operations: 430
  grpc.OK: 401
  grpc.RESOURCE_EXHAUSTED: 29
  success_rate: 0.9325581395348838
  p95_ms: 6341.0
  p99_ms: 6630.0

concurrency=10:
  verdict: SATURATED_CONTROLLED
  completed_operations: 648
  grpc.OK: 609
  grpc.RESOURCE_EXHAUSTED: 39
  success_rate: 0.9398148148148148
  p95_ms: 6981.0
  p99_ms: 9399.0
```

Safety counters:

```text
cross_zone_leakage_count: 0
access_level_violation_count: 0
wrong_version_count: 0
deleted_context_count: 0
expired_context_count: 0
indexing_context_count: 0
missing_active_qdrant_points_after_cooldown: 0
dead_letters: 0
orphan_binding_count: 0
orphan_outbox_count: 0
duplicate_canonical_identity_count: 0
panic: 0
crash: 0
deadlock: 0
UNKNOWN: 0
```

Harness repair completed in this phase:

```text
defect: repeated INGEST_VERSION operation_id reused deterministic document identity across phases/levels.
repair: live INGEST_VERSION now uses a unique run-local invocation id for namespace/source identity.
additional guard: pending ingest finalization is deduplicated by runtime identity.
production retrieval semantics changed: no
```

Verdict:

```text
FIX489_RUNTIME_SAFETY_GATES_PASS
FIX489_HARNESS_IDENTITY_REPAIR_PASS
FIX489_LOCAL_CAPACITY_STABLE_FLOOR_BLOCKED
```

Next step:

```text
Add a local discovery ladder for concurrency=1,2,3,4 before the official 5,10,15,20,25,50 campaign. Run the 60-minute soak only after a stable operating level is established.
```
