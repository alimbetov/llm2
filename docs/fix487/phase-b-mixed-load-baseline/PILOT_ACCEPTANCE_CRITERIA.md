# FIX487B Pilot Acceptance Criteria

## Harness

```text
deterministic_dataset = true
deterministic_schedule = true
bounded_worker_count = 5
unbounded_queue = false
raw_operation_evidence_complete = true
resource_sampling_complete = true
postgres_audit_complete = true
qdrant_audit_complete = true
outbox_audit_complete = true
evidence_manifest_complete = true
```

## Workload

```text
measurement_duration_seconds >= 180
completed_measurement_operations >= 100
completed deterministic cycles >= 1
all operation classes executed > 0
```

## Hard Gates

```text
cross_zone_leakage_count = 0
access_level_violation_count = 0
deleted_context_count = 0
expired_context_count = 0
indexing_context_count = 0
orphan_binding_count = 0
orphan_outbox_count = 0
duplicate_canonical_identity_count = 0
cross_zone_binding_anomaly_count = 0
failed_outbox = 0
dead_letters = 0
missing_active_qdrant_points_after_cooldown = 0
```

## Verdicts

```text
FIX487B_MIXED_LOAD_HARNESS_PASS
FIX487B_CONCURRENCY_5_PILOT_PASS
```

Blocked/failed pilot evidence must never be rewritten into PASS.
