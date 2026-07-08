#[test]
fn fix461_ttl_cleanup_uses_delete_operation_fencing_and_vector_binding_tombstone() {
    let lifecycle = include_str!("../src/lifecycle/mod.rs");
    assert!(
        lifecycle.contains("delete_operation_id"),
        "TTL cleanup must fence Qdrant delete with delete_operation_id"
    );
    assert!(
        lifecycle.contains("qdrant_sync_status='DELETED'"),
        "TTL cleanup must tombstone vector_bindings_v004 after deleting Qdrant projection"
    );
    assert!(
        lifecycle.contains("COALESCE(legal_hold,false)=false"),
        "TTL cleanup must not delete legal-hold bindings"
    );
}

#[test]
fn fix461_multi_zone_retrieval_uses_compound_zone_chunk_keys() {
    let persistence = include_str!("../src/persistence/mod.rs");
    let grpc = include_str!("../src/grpc/mod.rs");
    assert!(
        persistence.contains("HashSet<(Uuid, Uuid)>"),
        "final visibility must return (access_zone_id, chunk_id) keys"
    );
    assert!(
        persistence.contains("HashMap<(Uuid, Uuid), String>"),
        "matched text map must use compound keys"
    );
    assert!(
        persistence.contains("HashMap<(Uuid, Uuid), ChunkTraceRecord>"),
        "trace map must use compound keys"
    );
    assert!(
        grpc.contains("result_identity_key"),
        "direct/graph merge and dedup must use access_zone_id + chunk_id identity"
    );
}

#[test]
fn fix461_mmr_and_hybrid_contracts_are_hardened() {
    let persistence = include_str!("../src/persistence/mod.rs");
    let grpc = include_str!("../src/grpc/mod.rs");
    assert!(
        persistence.contains("ed.representation_name = $3"),
        "MMR chunk fallback must filter dense representation name"
    );
    assert!(
        persistence.contains("ed.representation_version = $4::text")
            || persistence.contains("ce.dense_version = $4::text"),
        "MMR chunk fallback must filter dense version"
    );
    assert!(
        grpc.contains("normalize_scores_for_fusion"),
        "NORMALIZED_WEIGHTED_SCORE must normalize scores"
    );
    assert!(
        grpc.contains("docv:{}:payload:{}:model:{}:dense:{}"),
        "MMR point cache key must include document/payload/model/dense version"
    );
}
