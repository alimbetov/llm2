# AstraVector Alerts — fix462 enhanced

This file is part of the fix462 production-candidate gate. Alert expressions must reference metric names that exist in code and every alert must have an operator action.

## TTL lifecycle

- **TTL backlog high**
  - Expr: `index_ttl_backlog_documents_total > 1000 for 10m`
  - Runbook: check PostgreSQL locks, Qdrant health, and cleanup worker logs.
- **Oldest expired document too old**
  - Expr: `index_ttl_oldest_expired_age_seconds > 3600 for 10m`
  - Runbook: verify `index_ttl.cleanup_enabled=true` and cleanup worker scheduling.
- **Stale DELETING documents**
  - Expr: `index_ttl_stale_deleting_documents_total > 0 for 10m`
  - Runbook: inspect `delete_operation_id`, `delete_fencing_started_at`, and recovery logs.
- **Permanent delete failure**
  - Expr: `index_ttl_delete_permanently_failed_documents > 0`
  - Runbook: use `RetryDocumentDeletion` only after checking Qdrant and PostgreSQL state.
- **Cleanup concurrent state changes**
  - Expr: `rate(index_ttl_cleanup_concurrent_state_change_total[10m]) > 0`
  - Runbook: check lifecycle writers and `delete_operation_id` guard coverage.
- **Delete operation conflict**
  - Expr: `rate(index_ttl_delete_operation_conflict_total[10m]) > 0`
  - Runbook: inspect concurrent cleanup workers and document lifecycle writers.
- **Lifecycle update blocked by active delete operation**
  - Expr: `rate(document_lifecycle_update_blocked_by_delete_operation_total[10m]) > 0`
  - Runbook: verify this is expected during TTL cleanup; investigate clients retrying activation/manual retry during active deletion.

## Qdrant projection

- **Qdrant delete errors**
  - Expr: `rate(qdrant_points_delete_failed_total[10m]) > 0`
  - Runbook: check Qdrant availability and collection status.
- **Extra Qdrant projections detected**
  - Expr: `rate(qdrant_cleanup_extra_points_detected_total[10m]) > 0`
  - Runbook: compare `vector_bindings_v004` with Qdrant scroll output.
- **Extra Qdrant projections deleted**
  - Expr: `rate(qdrant_cleanup_extra_points_deleted_total[10m]) > 0`
  - Runbook: verify they are orphan or non-legal-hold points.
- **Legal-hold Qdrant points skipped**
  - Expr: `rate(qdrant_cleanup_extra_points_skipped_legal_hold_total[10m]) > 0`
  - Runbook: verify legal hold records are expected; do not force-delete without business approval.
- **Orphan Qdrant projections deleted**
  - Expr: `rate(qdrant_cleanup_orphan_points_deleted_total[10m]) > 0`
  - Runbook: inspect outbox/reconciliation lag and recent failed publishes.
- **Qdrant search rejected**
  - Expr: `rate(qdrant_search_rejected_total[10m]) > 0`
  - Runbook: check `limits.max_concurrent_qdrant_search`, Qdrant latency, and client timeouts.

## RAG quality

- **Empty RetrieveContext spike**
  - Expr: `rate(retrieved_contexts_empty_total[10m])` spike versus baseline
  - Runbook: check index freshness, access zone filters, TTL cleanup, and Qdrant collection health.
- **Final visibility drops**
  - Expr: `rate(retrieve_context_final_visibility_dropped_total[10m]) > 0`
  - Runbook: inspect stale Qdrant points and PostgreSQL visibility filters.
- **MMR token fallback high**
  - Expr: `rate(graph_mmr_token_fallback_total[10m]) / rate(graph_mmr_candidates_total[10m]) > 0.3`
  - Runbook: verify dense embeddings exist for candidate points and representation versions match.
- **Missing MMR access-zone identity**
  - Expr: `rate(graph_mmr_embedding_identity_missing_access_zone_total[10m]) > 0`
  - Runbook: check Qdrant payload mapping and multi-zone result construction.

## Validation requirement

Before declaring `PRODUCTION CANDIDATE`, run observability checks that scrape `/metrics` and verify fix462 counters are visible after the corresponding test scenario.

## Overload and recovery

- **Retrieve admission rejects sustained**
  - Expr: `rate(astravector_admission_rejected_total{scope="retrieve_context"}[5m]) > 0`
  - Runbook: compare admitted RPS with hardware-specific stable capacity; do not increase queue capacity.
- **Query queue age rejects**
  - Expr: `rate(astravector_queue_rejected_total{queue="query",reason="age_exceeded"}[5m]) > 0`
  - Runbook: inspect inference latency and admission limits.
- **Insufficient inference budget**
  - Expr: `rate(astravector_deadline_rejected_total{stage="inference_queue",reason="insufficient_budget"}[5m]) > 0`
  - Runbook: check caller deadlines and queue wait; do not increase global timeout.
- **Retry amplification risk**
  - Expr: `rate(astravector_retry_attempts_total{workload="query"}[5m]) > rate(astravector_retry_success_total{workload="query"}[5m]) * 2`
  - Runbook: verify retries have sufficient remaining budget.
- **Optional retrieval degradation**
  - Expr: `rate(astravector_degraded_path_total[5m]) > 0`
  - Runbook: distinguish Graph/MMR permit pressure from mandatory retrieval failure.
