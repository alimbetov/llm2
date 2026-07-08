# AstraVector v007 fix4.5.7 — Production Readiness, Testcontainers Proof & Cleanup Recovery Hardening

## Scope

This patch hardens the fix4.5.6 line for internal production readiness. It does not add a new retrieval feature; it adds proof, recovery, and observability around the existing TTL/access-zone lifecycle.

## Key changes

1. Added typed `IndexTtlCleanupStage` and `IndexTtlCleanupError` so TTL cleanup failure classification no longer depends on string matching.
2. Preserved the UUID-backed access-zone invariant:
   - external contract may use `access_zone_code`;
   - internal PostgreSQL/Qdrant/Search/GraphRAG/TTL operations use UUID `access_zone_id`.
3. Documented and tested the Code Matrix TTL boundary: `1500–1999` maps to `365` days, while `1000–1499` maps to `182` days.
4. Added an idempotency counter for already-absent Qdrant points during TTL cleanup:
   `index_ttl_cleanup_qdrant_points_already_absent_total`.
5. Added `access_zone_search_large_zone_set_total` for large multi-zone search requests.
6. Added additive migration `0032_v007_fix457_production_readiness_hardening.sql` with indexes for registry lookup and TTL cleanup retry/stale paths.
7. Replaced panic/TODO integration placeholders with executable contract tests and environment-gated PostgreSQL/Qdrant harness checks.

## Runtime guarantees

### TTL cleanup

The cleanup path treats an empty Qdrant point list as idempotent success. This supports the crash-recovery model:

```text
Qdrant delete succeeded
PostgreSQL update/commit did not finish
DELETING becomes stale
worker marks DELETE_FAILED
retry finds zero Qdrant points
PostgreSQL lifecycle progresses to DELETED
```

### Error classification

Cleanup failures are classified by stage:

| Stage | Error code |
|---|---|
| `QdrantScroll` | `QDRANT_SCROLL_FAILED` |
| `QdrantDelete` | `QDRANT_DELETE_FAILED` |
| `GraphNodesUpdate` | `GRAPH_NODES_CLEANUP_FAILED` |
| `GraphEdgesUpdate` | `GRAPH_EDGES_CLEANUP_FAILED` |
| `ContentChunksUpdate` | `CONTENT_CHUNKS_CLEANUP_FAILED` |
| `DocumentVersionUpdate` | `DOCUMENT_VERSION_CLEANUP_FAILED` |
| `TombstonePurge` | `TOMBSTONE_PURGE_FAILED` |

## Observability

### TTL cleanup metrics

```text
index_ttl_worker_started_total
index_ttl_worker_iterations_total
index_ttl_worker_iteration_failed_total
index_ttl_cleanup_batches_total
index_ttl_cleanup_documents_deleted_total
index_ttl_cleanup_qdrant_points_deleted_total
index_ttl_cleanup_qdrant_points_already_absent_total
index_ttl_cleanup_stage_failed_total{stage,error_code}
index_ttl_cleanup_delete_failed_total
index_ttl_cleanup_duration_ms
index_ttl_deleting_stale_total
index_ttl_backlog_documents
index_ttl_oldest_expired_age_seconds
index_ttl_graph_cleanup_failed_total
index_ttl_tombstone_purge_failed_total
```

### Access-zone metrics

```text
access_zone_registry_db_fallback_total
access_zone_registry_db_fallback_found_total
access_zone_registry_db_fallback_not_found_total
access_zone_registry_db_fallback_disabled_total
access_zone_registry_db_fallback_deleted_total
access_zone_registry_cache_stale_miss_total
access_zone_search_zones_count
access_zone_search_large_zone_set_total
```

## Recommended alert rules

```text
ALERT IndexTtlCleanupBacklogHigh:
index_ttl_backlog_documents > threshold

ALERT IndexTtlOldestExpiredTooOld:
index_ttl_oldest_expired_age_seconds > threshold

ALERT IndexTtlCleanupFailures:
increase(index_ttl_cleanup_delete_failed_total[10m]) > 0

ALERT QdrantDeleteFailures:
increase(qdrant_points_delete_failed_total[10m]) > 0

ALERT RegistryFallbackFailures:
increase(access_zone_registry_db_fallback_not_found_total[10m]) > threshold

ALERT AutoCreatedZonesSpike:
increase(access_zone_auto_created_total[1h]) > threshold

ALERT LargeMultiZoneSearchSpike:
increase(access_zone_search_large_zone_set_total[10m]) > threshold
```

## Test execution

Pure contract tests run without external services. PostgreSQL/Qdrant side-effect checks are environment-gated:

```bash
ASTRAVECTOR_TEST_DATABASE_URL=postgres://... cargo test --test fix457_production_readiness -- --nocapture
ASTRAVECTOR_TEST_QDRANT_URL=http://localhost:6333 cargo test --test fix457_production_readiness -- --nocapture
```

Full release gate remains:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
```

## Remaining limitation

This patch adds executable proof scaffolding and pure contract tests, but the current environment used to prepare the patch did not provide a Rust toolchain or Docker-backed testcontainers. Final production approval still requires running the release gate in CI.
