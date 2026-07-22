# FIX486G Supplemental Bank Review

## Current verdict

```text
CHANGES_REQUIRED_BEFORE_FREEZE
```

The current committed monolithic query file is an analysis seed only. It must not be used for an official statistical PASS claim.

## Confirmed strengths

- 30 positive Graph-required paraphrases exist across RU/KZ/EN.
- 15 negative/constrained-negative queries exist across RU/KZ/EN.
- 20 adversarial executions cover wrong parent, cross-zone, lifecycle, hop-limit and cycle classes.
- Fault plans preserve frozen-bank immutability and require phase-owned cleanup.
- Statistical thresholds, hard gates and confidence-interval reporting are specified independently of runtime output.

## Blocking findings

### FIX486G-BANK-P1-001 — Graph-disabled controls absent from v0.1 query file

The reviewed design requires six explicit Graph-disabled controls:

```text
2 RU
2 KZ
2 EN
```

They must prove:

- Graph execution count is zero;
- Graph origin count is zero;
- direct canonical result remains healthy;
- no Graph contribution is credited while Graph is disabled.

### FIX486G-BANK-P1-002 — adversarial language metadata mismatch

Twelve v0.1 adversarial records have a language label that does not match the actual query text. The affected families are:

```text
cross-zone: 3
lifecycle: 3
hop-limit: 3
cycle: 3
```

This does not change retrieval semantics, but it invalidates language-sliced statistics and therefore blocks freeze.

### FIX486G-BANK-P1-003 — per-query qrel materialization pending

The qrel profile definitions exist, but every query must resolve to exactly one reviewed qrel profile before freeze.

Required profiles:

```text
POSITIVE_GRAPH
NEGATIVE_NO_ANSWER
NEGATIVE_LEGAL_HOLD
GRAPH_DISABLED
FAULT_WRONG_PARENT
FAULT_CROSS_ZONE
FAULT_LIFECYCLE
FAULT_HOP_LIMIT
FAULT_CYCLE
```

### FIX486G-BANK-P1-004 — canonical hashes pending

Per-file and aggregate SHA-256 values are not yet frozen. Official execution is prohibited until the final query set, qrel assignments, profiles and fault plans are hashed and marked `1.0.0 / FROZEN`.

## Required corrected candidate

Create a deterministic `0.2.0-reviewed-candidate` containing:

```text
positive Graph-required: 30
negative/constrained-negative: 15
Graph-disabled: 6
adversarial: 20
total: 71
```

Mandatory language distribution for positive queries:

```text
RU: 10
KZ: 10
EN: 10
```

Mandatory negative distribution:

```text
RU: 5
KZ: 5
EN: 5
```

Mandatory Graph-disabled distribution:

```text
RU: 2
KZ: 2
EN: 2
```

Adversarial language distribution must be derived from the actual text and validated automatically.

## Freeze checklist

- [ ] 71 unique query IDs.
- [ ] Every query parses as JSON.
- [ ] Every query resolves to one qrel profile.
- [ ] No orphan qrel assignments.
- [ ] No unused qrel profiles without an explicit reason.
- [ ] Language metadata matches query text review.
- [ ] Frozen source corpus identity remains unchanged.
- [ ] Fault overlays have deterministic setup and cleanup.
- [ ] No expected result is derived from runtime output.
- [ ] Per-file SHA-256 values computed.
- [ ] Aggregate SHA-256 computed.
- [ ] Bank status promoted to `1.0.0 / FROZEN`.

## Allowed next activity

The next permitted activity is materialization and independent review of the corrected `0.2.0-reviewed-candidate`. Production Graph tuning must not use official results from this bank before freeze.
