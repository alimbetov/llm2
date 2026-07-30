# FIX487C Soak Acceptance Criteria

## Integrity

```text
cross_zone_leakage_count = 0
access_level_violation_count = 0
lifecycle_invalid_context_count = 0
orphan_binding_count = 0
orphan_outbox_count = 0
duplicate_canonical_identity_count = 0
failed_outbox = 0
dead_letters = 0
missing_active_qdrant_points_after_cooldown = 0
```

## Runtime

```text
crash = 0
panic = 0
deadlock = 0
UNKNOWN = 0
unexpected_INTERNAL = 0
unclassified_timeout = 0
successful operations >= 99.5%
sample completeness >= 98%
unbounded queue growth = false
unbounded memory growth = false
file descriptor leak = false
thread/task growth leak = false
```

## Cooldown

```text
runtime_ready_after_cooldown = true
in_flight_after_cooldown = 0
queue_depth_after_cooldown = 0
outbox_pending_after_cooldown = 0
outbox_retry_pending_after_cooldown = 0
failed_outbox = 0
dead_letters = 0
```
