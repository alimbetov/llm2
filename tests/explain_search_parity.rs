use astravector_runtime::qdrant::{QdrantClient, QdrantVersionFilters};
use uuid::Uuid;

#[test]
fn canonical_qdrant_filter_is_fail_closed_for_search_and_explain() {
    let zone = Uuid::new_v4();
    let filter = QdrantClient::canonical_search_filter(
        &[zone],
        1,
        Some(&QdrantVersionFilters {
            model_version: Some("bge-m3".into()),
            ..Default::default()
        }),
    );
    let must = filter
        .get("must")
        .and_then(serde_json::Value::as_array)
        .expect("canonical filter has must clauses");
    let serialized = serde_json::to_string(must).unwrap();
    for required in [
        "access_zone_id",
        "access_level",
        "lifecycle_status",
        "expires_at_epoch",
        "chunk_granularity",
        "model_version",
    ] {
        assert!(serialized.contains(required), "missing filter {required}");
    }
    assert!(serialized.contains(&zone.to_string()));
    assert!(serialized.contains("ACTIVE"));
    assert!(serialized.contains("\"lte\":1"));
}

#[test]
fn explain_uses_the_same_qdrant_search_methods_as_search() {
    let source = include_str!("../src/grpc/mod.rs");
    let explain = source
        .split("async fn explain_search")
        .nth(1)
        .and_then(|tail| tail.split("async fn").next())
        .expect("explain_search implementation");
    assert!(explain.contains(".search_dense"));
    assert!(explain.contains(".search_sparse"));
    assert!(explain.contains("caller_access_level"));
    assert!(explain.contains("access_zone_id"));
}
