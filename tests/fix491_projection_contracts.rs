use astravector_runtime::{
    inference::EmbeddingResult, projection::CanonicalProjectionInput,
    qdrant::qdrant_collection_compatibility_from_info,
};
use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

fn projection_input() -> CanonicalProjectionInput {
    CanonicalProjectionInput {
        access_zone_id: Uuid::parse_str("00000000-0000-0000-0000-000000004862").unwrap(),
        access_zone_code: "4862".to_string(),
        binding_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        qdrant_point_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
        document_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
        document_version: 7,
        root_chunk_id: Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap(),
        source_chunk_id: Uuid::parse_str("55555555-5555-5555-5555-555555555555").unwrap(),
        parent_chunk_id: Some(Uuid::parse_str("66666666-6666-6666-6666-666666666666").unwrap()),
        chunk_id: Uuid::parse_str("77777777-7777-7777-7777-777777777777").unwrap(),
        chunk_granularity: "SUB_180".to_string(),
        representation_type: "ORIGINAL".to_string(),
        access_level: 2,
        lifecycle_status: "ACTIVE".to_string(),
        expires_at: Some(Utc.with_ymd_and_hms(2027, 1, 2, 3, 4, 5).unwrap()),
        legal_hold: false,
        payload_version: 3,
        model_version: "bge-m3-onnx".to_string(),
        tokenizer_version: "bge-m3-tokenizer".to_string(),
        dense_version: Some("dense-v1".to_string()),
        sparse_version: Some("sparse-v1".to_string()),
        metadata: json!({
            "chunking_profile_version": "chunk-v1",
            "source_block_id": "logical-block-a",
            "trace_quality": "CANONICAL",
            "trace_relation_type": "DIRECT",
            "quality_run_id": "fix491-test",
            "quality_runtime_bench": "projection-contract"
        }),
    }
}

#[test]
fn fix491_projection_payload_contains_complete_retrieval_contract() {
    let payload = projection_input().payload();

    assert_eq!(
        payload["access_zone_id"],
        json!("00000000-0000-0000-0000-000000004862")
    );
    assert_eq!(payload["access_zone_code"], json!("4862"));
    assert_eq!(
        payload["binding_id"],
        json!("11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(
        payload["qdrant_point_id"],
        json!("22222222-2222-2222-2222-222222222222")
    );
    assert_eq!(
        payload["document_id"],
        json!("33333333-3333-3333-3333-333333333333")
    );
    assert_eq!(payload["document_version"], json!(7));
    assert_eq!(
        payload["root_chunk_id"],
        json!("44444444-4444-4444-4444-444444444444")
    );
    assert_eq!(
        payload["source_chunk_id"],
        json!("55555555-5555-5555-5555-555555555555")
    );
    assert_eq!(
        payload["parent_chunk_id"],
        json!("66666666-6666-6666-6666-666666666666")
    );
    assert_eq!(
        payload["chunk_id"],
        json!("77777777-7777-7777-7777-777777777777")
    );
    assert_eq!(payload["chunk_granularity"], json!("SUB_180"));
    assert_eq!(payload["representation_type"], json!("ORIGINAL"));
    assert_eq!(payload["access_level"], json!(2));
    assert_eq!(payload["lifecycle_status"], json!("ACTIVE"));
    assert_eq!(payload["expires_at"], json!("2027-01-02T03:04:05Z"));
    assert_eq!(payload["expires_at_epoch"], json!(1_798_859_045_i64));
    assert_eq!(payload["legal_hold"], json!(false));
    assert_eq!(payload["payload_version"], json!(3));
    assert_eq!(payload["model_version"], json!("bge-m3-onnx"));
    assert_eq!(payload["tokenizer_version"], json!("bge-m3-tokenizer"));
    assert_eq!(payload["dense_version"], json!("dense-v1"));
    assert_eq!(payload["sparse_version"], json!("sparse-v1"));
    assert_eq!(payload["chunking_profile_version"], json!("chunk-v1"));
    assert_eq!(payload["source_block_id"], json!("logical-block-a"));
    assert_eq!(payload["trace_quality"], json!("CANONICAL"));
    assert_eq!(payload["trace_relation_type"], json!("DIRECT"));
    assert_eq!(payload["quality_run_id"], json!("fix491-test"));
    assert_eq!(
        payload["quality_runtime_bench"],
        json!("projection-contract")
    );
    assert_eq!(payload["quarantined"], json!(false));
}

#[test]
fn fix491_projection_point_reuses_persisted_vectors_without_inference_fallback() {
    let input = projection_input();
    let point = input.point(EmbeddingResult {
        dense: Some(vec![0.1, 0.2, 0.3]),
        sparse_indices: Some(vec![10, 20]),
        sparse_values: Some(vec![0.7, 0.8]),
        token_count: 42,
        truncated: false,
    });

    assert_eq!(
        point.id,
        Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()
    );
    assert_eq!(point.dense, Some(vec![0.1, 0.2, 0.3]));
    assert_eq!(point.sparse_indices, Some(vec![10, 20]));
    assert_eq!(point.sparse_values, Some(vec![0.7, 0.8]));
    assert_eq!(
        point.payload["binding_id"],
        json!("11111111-1111-1111-1111-111111111111")
    );
}

fn qdrant_collection_info(payload_schema: serde_json::Value) -> serde_json::Value {
    json!({
        "result": {
            "config": {
                "params": {
                    "vectors": {
                        "dense": {
                            "size": 1024,
                            "distance": "Cosine"
                        }
                    },
                    "sparse_vectors": {
                        "sparse": {
                            "index": {
                                "on_disk": false
                            }
                        }
                    }
                }
            },
            "payload_schema": payload_schema
        }
    })
}

fn full_payload_schema() -> serde_json::Value {
    json!({
        "access_zone_id": {"data_type": "keyword"},
        "lifecycle_status": {"data_type": "keyword"},
        "chunk_granularity": {"data_type": "keyword"},
        "document_id": {"data_type": "keyword"},
        "document_version": {"data_type": "integer"},
        "access_level": {"data_type": "integer"},
        "expires_at_epoch": {"data_type": "integer"},
        "quarantined": {"data_type": "bool"},
        "model_version": {"data_type": "keyword"},
        "tokenizer_version": {"data_type": "keyword"},
        "dense_version": {"data_type": "keyword"},
        "sparse_version": {"data_type": "keyword"},
        "chunking_profile_version": {"data_type": "keyword"},
        "quality_run_id": {"data_type": "keyword"},
        "binding_id": {"data_type": "keyword"},
        "qdrant_point_id": {"data_type": "keyword"}
    })
}

#[test]
fn fix491_qdrant_collection_compatibility_accepts_full_payload_schema() {
    let summary = qdrant_collection_compatibility_from_info(
        &qdrant_collection_info(full_payload_schema()),
        1024,
    );

    assert_eq!(summary.verdict, "QDRANT_COLLECTION_COMPATIBLE");
    assert!(summary.missing_payload_indexes.is_empty());
    assert!(summary.mismatched_payload_indexes.is_empty());
}

#[test]
fn fix491_qdrant_collection_compatibility_rejects_missing_payload_index() {
    let mut schema = full_payload_schema();
    schema
        .as_object_mut()
        .expect("payload schema object")
        .remove("access_zone_id");

    let summary = qdrant_collection_compatibility_from_info(&qdrant_collection_info(schema), 1024);

    assert_eq!(summary.verdict, "QDRANT_COLLECTION_SCHEMA_MISMATCH");
    assert_eq!(summary.missing_payload_indexes, vec!["access_zone_id"]);
}

#[test]
fn fix491_qdrant_collection_compatibility_rejects_mismatched_payload_index_type() {
    let mut schema = full_payload_schema();
    schema["access_level"] = json!({"data_type": "keyword"});

    let summary = qdrant_collection_compatibility_from_info(&qdrant_collection_info(schema), 1024);

    assert_eq!(summary.verdict, "QDRANT_COLLECTION_SCHEMA_MISMATCH");
    assert_eq!(
        summary.mismatched_payload_indexes,
        vec!["access_level: expected=integer, actual=keyword"]
    );
}
