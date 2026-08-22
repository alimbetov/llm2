# FIX491 Qdrant Recovery Result

Verdict: `QDRANT_PROJECTION_RECOVERY_PASS`

Evidence:

```text
run_id = fix491-20260811-003559
qdrant_compatibility = docs/fix491/evidence/fix491-20260811-003559/qdrant-compatibility.stdout
qdrant_audit = docs/fix491/evidence/fix491-20260811-003559/qdrant-audit.stdout
qdrant_rebuild = docs/fix491/evidence/fix491-20260811-003559/qdrant-rebuild.stdout
```

## Collection Compatibility

```text
expected_dense_dimension = 1024
actual_dense_dimension   = 1024
dense_distance           = Cosine
sparse_vector_present    = true
required_payload_indexes = 16
missing_payload_indexes  = 0
mismatched_payload_indexes = 0
verdict = QDRANT_COLLECTION_COMPATIBLE
```

## Projection Audit

```text
expected_eligible_bindings = 19776
actual_points              = 19776
missing_points             = 0
orphan_points              = 0
payload_mismatches         = 0
pages_scanned              = 20
points_scanned             = 19776
scan_completed             = true
verdict                    = QDRANT_PROJECTION_CONSISTENT
```

## Rebuild

```text
expected_eligible_bindings = 19776
batches_scanned            = 40
points_upserted            = 19776
failed_points              = 0
batch_size                 = 500
replace_existing           = false
used_inference_fallback    = false
verdict                    = QDRANT_REBUILD_COMPLETED
```

The rebuild path uses persisted PostgreSQL embeddings and the shared canonical projection builder. It does not call ONNX inference and does not create a recovery-only payload builder.

## Fixed Defects

```text
QDRANT_RECOVERY_FENCE_IDLE_TRANSACTION_TIMEOUT
QDRANT_COLLECTION_PAYLOAD_INDEX_COMPATIBILITY_NOT_AUDITED
```

Recovery fencing now uses a session-level exclusive advisory lock on a dedicated PostgreSQL connection with explicit release.
