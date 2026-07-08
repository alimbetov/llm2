# V007 fix4.5.6 — Index TTL Cleanup Runtime Wiring & Registry Cache Consistency

Base: `AstraVector_v007_interface_simplification_fix16_graph_rag_lite_fix455_access_zone_auto_provisioning.zip`.

## Scope

This patch closes the remaining lifecycle and cross-pod consistency gaps after fix4.5.5:

1. Starts `IndexTtlCleanupWorker` as a real background worker when `index_ttl.enabled && index_ttl.cleanup_enabled`.
2. Adds direct PostgreSQL fallback lookup in Access Zone Registry when the local cache snapshot misses a zone.
3. Separates `ACCESS_ZONE_NOT_FOUND`, `ACCESS_ZONE_DISABLED`, and `ACCESS_ZONE_DELETED` diagnostics.
4. Makes graph lifecycle cleanup failures visible and moves the document version to `DELETE_FAILED` instead of silently ignoring errors.
5. Makes tombstone purge transactional.
6. Adds fix4.5.6 integration-test targets for testcontainers wiring.

## Runtime TTL cleanup

`main.rs` now starts `lifecycle::spawn_index_ttl_cleanup(...)` when TTL cleanup is enabled. The worker periodically executes:

- `mark_stale_deleting_documents(...)`
- `run_index_ttl_cleanup_batch(...)`
- `purge_index_ttl_tombstones(...)` when `hard_delete_metadata=true`

If `index_ttl.cleanup_enabled=true` but Qdrant is disabled, startup fails because physical vector deletion cannot be performed safely.

## Registry DB fallback

Resolver behavior on cache miss:

1. Check cache snapshot.
2. If absent, query PostgreSQL directly by `access_zone_code` or `access_zone_id`.
3. If `ACTIVE`, return `ResolvedAccessZone` immediately and invalidate cache.
4. If `DISABLED`, return `FAILED_PRECONDITION: ACCESS_ZONE_DISABLED`.
5. If `DELETED`, return `FAILED_PRECONDITION: ACCESS_ZONE_DELETED`.
6. If not found, return `ACCESS_ZONE_NOT_FOUND` or auto-create only in the ingestion path when configured.

Search/RetrieveContext still never auto-create zones.

## Metrics

New/updated metric names used by this patch include:

- `index_ttl_worker_started_total`
- `index_ttl_worker_iterations_total`
- `index_ttl_worker_iteration_failed_total`
- `index_ttl_graph_cleanup_failed_total`
- `index_ttl_tombstone_purge_failed_total`
- `access_zone_registry_db_fallback_total`
- `access_zone_registry_db_fallback_found_total`
- `access_zone_registry_db_fallback_not_found_total`
- `access_zone_registry_db_fallback_disabled_total`
- `access_zone_registry_db_fallback_deleted_total`
- `access_zone_registry_cache_stale_miss_total`

## Limitations

Rust toolchain was not available in the patching environment; `cargo fmt/check/test` must be run in CI or locally before release approval.

Testcontainers tests are scaffolded as ignored acceptance targets and still require real PostgreSQL/Qdrant harness implementation for final production proof.
