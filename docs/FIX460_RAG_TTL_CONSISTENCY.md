# AstraVector v007/fix460 — RAG/GraphRAG/TTL consistency hardening

## Scope

`fix460` targets the remaining RAG/TTL consistency issues found after `fix459`:

- real production-path E2E testcontainers lifecycle;
- document lifecycle visibility in GraphRAG context fetch;
- document lifecycle visibility in matched text/trace fetch;
- multi-zone MMR embedding fetch;
- TTL-safe MMR embedding fetch;
- final PostgreSQL visibility recheck before `RetrieveContext` response;
- safer TTL cleanup using PostgreSQL vector bindings as the deletion source of truth;
- `rows_affected` fencing before child cleanup;
- Qdrant search active/available metrics;
- real TTL backlog/dead-letter gauges;
- configurable hybrid fusion knobs.

## Production lifecycle proof

The integration test `test_e2e_ingestion_outbox_retrieve_ttl_cleanup` must exercise:

```text
document registration
→ production persistence path
→ embedding cache
→ vector bindings
→ vector outbox
→ outbox publisher
→ Qdrant point
→ search/visibility fetch
→ TTL expiration
→ cleanup
→ PostgreSQL DELETED
→ Qdrant point absent
→ visibility/text fetch absent
```

Direct `qdrant.upsert` is forbidden in the E2E proof except through the outbox publisher.

## Visibility rule

Every final context must satisfy all predicates at the same time:

```sql
chunk.access_zone_id IN requested zones
chunk.access_level <= caller_access_level
chunk.lifecycle_status = 'ACTIVE'
chunk.expires_at IS NULL OR chunk.expires_at > now()
chunk.deleted_at IS NULL
document.status = 'ACTIVE'
document.lifecycle_status = 'ACTIVE'
document.expires_at IS NULL OR document.expires_at > now()
```

This rule now applies to:

- GraphRAG related context fetch;
- direct chunk text fetch;
- direct chunk trace fetch;
- MMR embedding fetch;
- final `RetrieveContext` visibility recheck.

## TTL cleanup rule

PostgreSQL remains the source of truth. TTL cleanup first loads expected `qdrant_point_id` values from `vector_bindings_v004`, then reconciles any extra Qdrant points found by scroll.

Before child rows are marked deleted, `document_versions` must be successfully updated from `DELETING` to `DELETED` with `rows_affected == 1`.

## Metrics added/updated

- `retrieve_context_final_visibility_recheck_total`
- `retrieve_context_final_visibility_dropped_total`
- `retrieved_contexts_total`
- `retrieved_contexts_empty_total`
- `retrieved_contexts_by_source_total`
- `retrieve_context_final_token_count`
- `qdrant_search_concurrent_active`
- `qdrant_search_permits_available`
- `index_ttl_backlog_documents_total`
- `index_ttl_oldest_expired_age_seconds`
- `index_ttl_delete_permanently_failed_documents`
- `qdrant_cleanup_extra_points_detected_total`
- `index_ttl_cleanup_concurrent_state_change_total`
- `hybrid_fusion_applied_total`

## Required validation

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```
