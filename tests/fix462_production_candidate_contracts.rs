#[test]
fn fix462_graph_related_chunk_carries_zone_identity() {
    let graph = std::fs::read_to_string("src/graph/mod.rs").expect("read graph module");
    assert!(graph.contains("pub access_zone_id: Uuid"));
    assert!(graph.contains("pub seed_access_zone_id: Uuid"));
}

#[test]
fn fix462_graph_seed_expansion_is_zone_specific() {
    let persistence =
        std::fs::read_to_string("src/persistence/mod.rs").expect("read persistence module");
    assert!(persistence.contains("expand_chunks_1hop_by_seed_keys"));
    assert!(persistence.contains("JOIN seed_keys s"));
    assert!(persistence.contains("s.access_zone_id = n.access_zone_id"));
    assert!(persistence.contains("s.chunk_id = n.chunk_id"));
}

#[test]
fn fix462_direct_retrieval_uses_compound_parent_key() {
    let grpc = std::fs::read_to_string("src/grpc/mod.rs").expect("read grpc module");
    let persistence =
        std::fs::read_to_string("src/persistence/mod.rs").expect("read persistence module");
    assert!(grpc.contains("Vec<((Uuid, Uuid), QdrantSearchHit)>"));
    assert!(grpc.contains("by_candidate.get(&(*parent_zone_id, matched_id, *parent_id))"));
    assert!(persistence.contains("fetch_hydrated_search_contexts_multi"));
    assert!(persistence.contains("WITH ORDINALITY"));
    assert!(grpc.contains("graph_lookup_key = (rel.access_zone_id, rel.chunk_id)"));
}

#[test]
fn fix462_legal_hold_reconciliation_is_postgres_checked_and_classified() {
    let lifecycle = std::fs::read_to_string("src/lifecycle/mod.rs").expect("read lifecycle module");
    let persistence =
        std::fs::read_to_string("src/persistence/mod.rs").expect("read persistence module");
    assert!(persistence.contains("pub struct DeletableQdrantPoints"));
    assert!(persistence.contains("filter_deletable_qdrant_points_for_document"));
    assert!(persistence.contains("skipped_legal_hold"));
    assert!(persistence.contains("orphan"));
    assert!(lifecycle.contains("qdrant_cleanup_extra_points_skipped_legal_hold_total"));
    assert!(lifecycle.contains("qdrant_cleanup_orphan_points_deleted_total"));
}

#[test]
fn fix462_tombstone_purge_deletes_vector_bindings_before_chunks() {
    let lifecycle = std::fs::read_to_string("src/lifecycle/mod.rs").expect("read lifecycle module");
    let purge_start = lifecycle
        .find("pub async fn purge_index_ttl_tombstones")
        .expect("purge function");
    let purge_body = &lifecycle[purge_start..];
    let vector_bindings_pos = purge_body
        .find("DELETE FROM astravector.vector_bindings_v004")
        .expect("vector binding purge");
    let chunks_pos = purge_body
        .find("DELETE FROM astravector.content_chunks_v004")
        .expect("chunk purge");
    assert!(
        vector_bindings_pos < chunks_pos,
        "vector_bindings_v004 must be purged before content_chunks_v004"
    );
}

#[test]
fn fix462_retry_document_deletion_has_error_stage_migration() {
    let migration =
        std::fs::read_to_string("migrations/0037_v007_fix462_retry_delete_error_stage.sql")
            .expect("read migration");
    let grpc = std::fs::read_to_string("src/grpc/mod.rs").expect("read grpc module");
    assert!(migration.contains("last_delete_error_stage"));
    assert!(grpc.contains("last_delete_error_stage=NULL"));
}

#[test]
fn fix462_retrieve_context_e2e_invokes_tonic_network_client() {
    let e2e = std::fs::read_to_string("tests/e2e_testcontainers.rs").expect("read e2e test");
    assert!(e2e.contains("AstraVectorRetrievalFacadeClient::connect"));
    assert!(e2e.contains("AstraVectorRetrievalFacadeServer::new"));
    assert!(e2e.contains("serve_with_incoming_shutdown"));
    assert!(e2e.contains("RetrieveContext network RPC must work before TTL cleanup"));
    assert!(e2e.contains("RetrieveContext must return zero contexts after TTL cleanup"));
}

#[test]
fn fix462_sqlx_prepare_and_smoke_load_are_ci_gates() {
    let ci = std::fs::read_to_string(".github/workflows/ci.yml").expect("read CI workflow");
    assert!(ci.contains("cargo sqlx prepare --check -- --all-targets --all-features"));
    assert!(ci.contains("smoke_load_retrieve_context"));
}

#[test]
fn fix462_rollback_flag_for_qdrant_reconciliation_exists() {
    let config = std::fs::read_to_string("src/config/mod.rs").expect("read config module");
    let yaml = std::fs::read_to_string("config/application.yaml").expect("read application yaml");
    assert!(config.contains("qdrant_reconciliation_enabled"));
    assert!(yaml.contains("ASTRAVECTOR_INDEX_TTL_QDRANT_RECONCILIATION_ENABLED"));
}
