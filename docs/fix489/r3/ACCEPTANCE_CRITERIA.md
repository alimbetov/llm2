# FIX489-R3 Acceptance Criteria

## Discovery

`FIX489_R3_LOCAL_STABLE_FLOOR_PASS` requires:

```text
at least one STABLE level
all executed levels are STABLE or SATURATED_CONTROLLED
no FAILED level
hard_gate_not_measured_count = 0
capacity curve produced
evidence manifest complete
```

Stable requires:

```text
completed_operations >= minimum_completed_operations
success_rate >= 0.995
RESOURCE_EXHAUSTED + DEADLINE_EXCEEDED + UNAVAILABLE <= 0.005
UNKNOWN = 0
unexpected_INTERNAL = 0
panic/crash/deadlock = 0
all hard safety counters = 0
cooldown_reached = true
queues_bounded = true
memory_behavior_stable = true
outbox drained
Qdrant consistency preserved
```

## Soak

The 60-minute soak may start only after discovery PASS and
`maximum_stable_concurrency >= 1`.

The selected soak concurrency is:

```text
max(1, int(maximum_stable_concurrency * 0.75))
```

Soak PASS requires:

```text
measurement_seconds = 3600
success_rate >= 0.995
sample_completeness_ratio >= 0.98
hard safety counters = 0
cooldown_reached = true
no sustained unbounded memory/queue/FD growth
```
