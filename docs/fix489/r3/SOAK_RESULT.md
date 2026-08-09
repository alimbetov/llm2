# FIX489-R3 60-Minute Soak Result

```text
status=PASS
verdict=FIX489_R3_SOAK_60M_PASS
tested_sha=623c75b65146d1ee9bda3ecd66636d9019accfce
run_id=fix489-r3-soak-20260809T111138Z
evidence=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489-r3-soak/fix489-r3-soak-20260809T111138Z
evidence_manifest_sha256=02ce12a365169b857ff5d963db6b6ed66d14b56ca3c8c1e9f8a9d7de11a94c55
soak_result_sha256=45aba6eaf63e73afcfeea445309bac307b4847ebe4bce193cfe0c845f03b4245
soak_concurrency=1
measurement_duration_seconds=3600
completed_operations=7381
success_rate=1.0
grpc_statuses.OK=7381
UNKNOWN=0
p50_ms=2475
p95_ms=3577
p99_ms=3836
max_ms=31798
sample_completeness_ratio=1.0
memory_behavior_stable=true
queues_bounded=true
```

Hard-gate evidence:

```text
cross_zone_leakage_count=0
access_level_violation_count=0
wrong_version_count=0
deleted_context_count=0
expired_context_count=0
indexing_context_count=0
lifecycle_invalid_context_count=0
missing_active_qdrant_points_after_cooldown=0
duplicate_canonical_identity_count=0
failed_outbox=0
dead_letters=0
orphan_binding_count=0
orphan_outbox_count=0
unexpected_INTERNAL=0
unclassified_timeout=0
panic=0
crash=0
deadlock=0
```

The first soak attempt on `d454de2e1ca1f85727ec1b587c7cf60c70fdd313`
correctly failed with `DELETE_POOL_EXHAUSTED`. The repair at
`623c75b65146d1ee9bda3ecd66636d9019accfce` sized the soak delete pool from
observed stable-floor throughput. The official repeat run above completed with
`UNKNOWN=0`.
