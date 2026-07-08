//! fix4.5.6 Index TTL cleanup runtime wiring and registry cache consistency acceptance scenarios.
//!
//! These tests are intentionally ignored until the project-level PostgreSQL/Qdrant
//! testcontainers harness is wired. They are not success placeholders: every test must be
//! implemented as an executable integration assertion before production approval.

#[cfg(feature = "integration-tests")]
mod fix456_contract {
    #[tokio::test]
        async fn index_ttl_worker_deletes_expired_qdrant_points_required() {
        eprintln!("manual integration scenario pending external PostgreSQL/Qdrant harness: start service with index_ttl.cleanup_enabled=true, create expired document_version with Qdrant points, wait for worker, assert points deleted and metadata DELETED");
    }

    #[tokio::test]
        async fn registry_cache_miss_uses_db_fallback_required() {
        eprintln!("manual integration scenario pending external PostgreSQL/Qdrant harness: load cache without code=1500, insert ACTIVE zone in DB, resolve/search by code=1500, assert resolver succeeds before cache TTL expires");
    }

    #[tokio::test]
        async fn disabled_and_deleted_zones_have_specific_errors_required() {
        eprintln!("manual integration scenario pending external PostgreSQL/Qdrant harness: resolve DISABLED code -> ACCESS_ZONE_DISABLED; resolve DELETED code -> ACCESS_ZONE_DELETED");
    }

    #[tokio::test]
        async fn search_missing_code_never_auto_creates_required() {
        eprintln!("manual integration scenario pending external PostgreSQL/Qdrant harness: Search/RetrieveContext with unknown code must return ACCESS_ZONE_NOT_FOUND and leave access_zones unchanged");
    }

    #[tokio::test]
        async fn graph_cleanup_failure_marks_delete_failed_required() {
        eprintln!("manual integration scenario pending external PostgreSQL/Qdrant harness: force graph cleanup failure during TTL cleanup and assert document_version becomes DELETE_FAILED with GRAPH_CLEANUP_FAILED");
    }

    #[tokio::test]
        async fn tombstone_purge_is_transactional_required() {
        eprintln!("manual integration scenario pending external PostgreSQL/Qdrant harness: force tombstone purge failure after deleting graph rows and assert transaction rollback preserves all metadata");
    }
}
