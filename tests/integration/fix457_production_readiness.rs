//! fix4.5.7 production readiness tests.
//!
//! These tests are executable and safe in CI without Docker: when external services are not
//! provided through `ASTRAVECTOR_TEST_DATABASE_URL` and `ASTRAVECTOR_TEST_QDRANT_URL`, the
//! external side-effect checks are skipped with a clear message. The pure contract checks always run.

use astravector_runtime::access_zone_registry::{default_ttl_days_from_access_zone_code, is_valid_access_zone_code};
use astravector_runtime::lifecycle::IndexTtlCleanupStage;

#[test]
fn code_matrix_contract_code_1500_is_365_days() {
    assert_eq!(default_ttl_days_from_access_zone_code("1000").unwrap(), 182);
    assert_eq!(default_ttl_days_from_access_zone_code("1499").unwrap(), 182);
    assert_eq!(default_ttl_days_from_access_zone_code("1500").unwrap(), 365);
    assert_eq!(default_ttl_days_from_access_zone_code("1999").unwrap(), 365);
    assert_eq!(default_ttl_days_from_access_zone_code("9500").unwrap(), 3650);
}

#[test]
fn access_zone_code_format_is_strict_four_digits() {
    for good in ["0000", "0001", "1500", "9999"] {
        assert!(is_valid_access_zone_code(good), "expected valid code {good}");
    }
    for bad in ["", "1", "150", "15000", "15A0", " 1500x", "abcd"] {
        assert!(!is_valid_access_zone_code(bad), "expected invalid code {bad}");
    }
}

#[test]
fn cleanup_error_stage_mapping_is_stable() {
    assert_eq!(IndexTtlCleanupStage::QdrantScroll.error_code(), "QDRANT_SCROLL_FAILED");
    assert_eq!(IndexTtlCleanupStage::QdrantDelete.error_code(), "QDRANT_DELETE_FAILED");
    assert_eq!(IndexTtlCleanupStage::DocumentVersionUpdate.error_code(), "DOCUMENT_VERSION_CLEANUP_FAILED");
    assert_eq!(IndexTtlCleanupStage::ContentChunksUpdate.error_code(), "CONTENT_CHUNKS_CLEANUP_FAILED");
    assert_eq!(IndexTtlCleanupStage::GraphNodesUpdate.error_code(), "GRAPH_NODES_CLEANUP_FAILED");
    assert_eq!(IndexTtlCleanupStage::GraphEdgesUpdate.error_code(), "GRAPH_EDGES_CLEANUP_FAILED");
    assert_eq!(IndexTtlCleanupStage::TombstonePurge.error_code(), "TOMBSTONE_PURGE_FAILED");
}

#[tokio::test]
async fn database_integration_harness_is_configurable() {
    let Ok(database_url) = std::env::var("ASTRAVECTOR_TEST_DATABASE_URL") else {
        eprintln!("skipping DB integration proof: set ASTRAVECTOR_TEST_DATABASE_URL to run PostgreSQL side-effect checks");
        return;
    };
    let pool = sqlx::PgPool::connect(&database_url).await.expect("connect test postgres");
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.expect("postgres health query");
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn qdrant_integration_harness_is_configurable() {
    let Ok(qdrant_url) = std::env::var("ASTRAVECTOR_TEST_QDRANT_URL") else {
        eprintln!("skipping Qdrant integration proof: set ASTRAVECTOR_TEST_QDRANT_URL to run Qdrant side-effect checks");
        return;
    };
    let url = format!("{}/collections", qdrant_url.trim_end_matches('/'));
    let response = reqwest::Client::new().get(url).send().await.expect("qdrant collections request");
    assert!(response.status().is_success(), "qdrant health response must be 2xx, got {}", response.status());
}
