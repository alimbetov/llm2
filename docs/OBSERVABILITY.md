# AstraVector observability

## Required metric groups

- Retrieval: `retrieved_contexts_total`, `retrieved_contexts_empty_total`, `retrieved_contexts_by_source_total`.
- Qdrant: `qdrant_request_duration_ms`, `qdrant_search_rejected_total`, `qdrant_payload_index_create_total`, `qdrant_payload_index_create_errors_total`.
- Outbox: `vector_outbox_stale_event_skipped_total`, `vector_outbox_mark_synced_rejected_total`, `vector_outbox_binding_version_mismatch_total`.
- Reconciliation: `reconciliation_bindings_repaired_total`, `reconciliation_skipped_legal_hold_total`, `reconciliation_errors_total`.
- TTL: `index_ttl_backlog_documents_total`, `index_ttl_oldest_expired_age_seconds`, `index_ttl_delete_failed_total`.
- Retention: `retention_deleted_total`, `retention_errors_total`.

## Retrieval source accounting

A context may be produced by more than one source, for example `VECTOR_DIRECT` and `GRAPH_EXPANDED`. `fix465` requires `retrieved_contexts_by_source_total` to increment once per source in `retrieval_sources`.
