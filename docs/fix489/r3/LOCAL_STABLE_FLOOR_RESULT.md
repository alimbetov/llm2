# FIX489-R3 Local Stable Floor Result

```text
status=PASS
verdict=FIX489_R3_LOCAL_STABLE_FLOOR_PASS
tested_sha=00eeafaa465ea848c69dea9d7b70bd38aa75b785
run_id=fix489-r3-20260809T062307Z
evidence=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489-r3/fix489-r3-20260809T062307Z
evidence_manifest_sha256=5d2ecabd84e1c0051e5232a01e49a0e07fae74be8a7cdf2a1ffc44dd3700ff4a
capacity_summary_sha256=2af3725e27f7fa0659ef2cdd55470ddc1ea738a76737cc9c978433d54e506dc7
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
maximum_stable_concurrency=2
first_controlled_saturation_concurrency=3
recommended_operating_concurrency=1
```

| Concurrency | Verdict | Ops | OK % | RE % | p50 | p95 | p99 | CPU | Peak RSS | Cooldown |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- | --- |
| 1 | STABLE | 613 | 100.00 | 0.00 | 2444 ms | 3482 ms | 5626 ms | MEASURED | MEASURED | PASS |
| 2 | STABLE | 881 | 100.00 | 0.00 | 2926 ms | 3777 ms | 5808 ms | MEASURED | MEASURED | PASS |
| 3 | SATURATED_CONTROLLED | 982 | 98.17 | 1.83 | 3219 ms | 6432 ms | 7265 ms | MEASURED | MEASURED | PASS |
| 4 | SATURATED_CONTROLLED | 1176 | 95.49 | 4.51 | 3267 ms | 6772 ms | 7201 ms | MEASURED | MEASURED | PASS |

Hard-gate evidence:

```text
cross_zone_leakage_count=0
access_level_violation_count=0
wrong_version_count=0
deleted_context_count=0
expired_context_count=0
indexing_context_count=0
missing_active_qdrant_points_after_cooldown=0
duplicate_canonical_identity_count=0
failed_outbox=0
orphan_binding_count=0
orphan_outbox_count=0
panic=0
crash=0
deadlock=0
memory_behavior_stable=true
queues_bounded=true
```

Historical reference only:

```text
C5  = SATURATED_CONTROLLED
C10 = SATURATED_CONTROLLED
```
