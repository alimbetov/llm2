# fix462 Observability Validation

Before `PRODUCTION CANDIDATE`, verify that `/metrics` exposes fix462 counters and that counters change during test scenarios.

## Required metric names

- `qdrant_cleanup_extra_points_detected_total`
- `qdrant_cleanup_extra_points_deleted_total`
- `qdrant_cleanup_extra_points_skipped_legal_hold_total`
- `qdrant_cleanup_orphan_points_deleted_total`
- `index_ttl_cleanup_concurrent_state_change_total`
- `retrieve_context_final_visibility_dropped_total`
- `qdrant_search_rejected_total`
- `retrieved_contexts_total`
- `retrieved_contexts_empty_total`

## Manual check

```bash
curl -fsS http://127.0.0.1:9090/metrics | grep qdrant_cleanup_extra_points
curl -fsS http://127.0.0.1:9090/metrics | grep retrieve_context
```

## Alert validation

All alert metric names in `docs/ALERTS.md` must exist in code or exported metrics. Every alert must include a runbook action.
