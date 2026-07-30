# FIX487B/C Capacity Acceptance Criteria

## Level Verdicts

Each level is exactly one of:

```text
STABLE
SATURATED_CONTROLLED
FAILED
BLOCKED
```

Level 25 must be `STABLE` or the campaign fails fast.

## STABLE

```text
runtime crash = 0
panic = 0
deadlock = 0
integrity violations = 0
unexpected INTERNAL = 0
UNKNOWN = 0
unclassified timeout = 0
cooldown reached = true
queues bounded = true
memory behavior stable = true
successful operations >= 99.5%
RESOURCE_EXHAUSTED + DEADLINE_EXCEEDED <= 0.5%
```

## SATURATED_CONTROLLED

Expected `RESOURCE_EXHAUSTED`, `DEADLINE_EXCEEDED` or `UNAVAILABLE` are allowed only when classified, bounded and followed by full recovery.

## FAILED Hard Gates

```text
cross-zone leakage
access-level violation
deleted/expired/indexing context returned
data corruption
orphan binding
orphan outbox
duplicate canonical identity
runtime crash
panic
deadlock
unbounded queue
unbounded RSS growth
UNKNOWN
unexplained INTERNAL
cooldown failure
outbox not drained
dead letters
missing active Qdrant points after cooldown
evidence incomplete
```
