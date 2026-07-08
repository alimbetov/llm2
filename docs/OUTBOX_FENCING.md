# Outbox operation-version fencing

Every `vector_outbox` event must be treated as a versioned command. A stale event must not mutate Qdrant.

## Rules

- `UPSERT_POINT`: allowed only for current ACTIVE binding with matching `payload_version` and pending sync state.
- `UPDATE_PAYLOAD`: skipped if `payload_version != operation_version`.
- `DELETE_POINT`: allowed only when `ttl_generation == operation_version`, binding is delete-pending and `legal_hold=false`.
- `mark_synced`: conditional on matching `payload_version` and ACTIVE lifecycle.

## Metrics

- `vector_outbox_stale_event_skipped_total{operation}`
- `vector_outbox_binding_version_mismatch_total{operation}`
- `vector_outbox_binding_lifecycle_mismatch_total{operation,lifecycle}`
