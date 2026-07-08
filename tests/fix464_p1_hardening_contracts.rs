#[test]
fn delete_point_uses_atomic_db_claim_before_qdrant_delete() {
    let src = include_str!("../src/outbox/mod.rs");
    assert!(src.contains("qdrant_sync_status='DELETE_IN_PROGRESS'"));
    assert!(src.contains("RETURNING qdrant_point_id"));
    assert!(src.contains("OUTBOX_DELETE_POINT_CLAIM_REJECTED"));
}

#[test]
fn mark_synced_rejection_is_not_completed_as_success() {
    let src = include_str!("../src/outbox/mod.rs");
    assert!(src.contains("vector_outbox_mark_synced_rejected_total"));
    assert!(src.contains("mark_synced rejected by binding version/lifecycle fence"));
    assert!(src.contains("return Err(AstraError::OwnershipLost"));
}

#[test]
fn recovery_and_retention_are_shutdown_aware() {
    let recovery = include_str!("../src/recovery/mod.rs");
    let retention = include_str!("../src/retention/mod.rs");
    assert!(recovery.contains("CancellationToken"));
    assert!(recovery.contains("shutdown.cancelled()"));
    assert!(recovery.contains("embedding_cache_recovery_errors_total"));
    assert!(retention.contains("CancellationToken"));
    assert!(retention.contains("shutdown.cancelled()"));
}

#[test]
fn forwarded_identity_headers_require_gateway_trust_proof() {
    let grpc = include_str!("../src/grpc/mod.rs");
    let cfg = include_str!("../config/application.yaml");
    let netpol = include_str!("../k8s/networkpolicy.yaml");
    assert!(grpc.contains("require_trusted_forwarded_identity_headers"));
    assert!(grpc.contains("security_forwarded_identity_rejected_total"));
    assert!(cfg.contains("trust_forwarded_identity_headers"));
    assert!(cfg.contains("gateway_trust_token"));
    assert!(netpol.contains("app: astravector-gateway"));
}

#[test]
fn e2e_has_real_tonic_ingestion_facade_test() {
    let e2e = include_str!("../tests/e2e_testcontainers.rs");
    assert!(e2e.contains("test_e2e_index_logical_document_via_tonic_ingestion_facade_and_activate"));
    assert!(e2e.contains("AstraVectorIngestionFacadeClient::connect"));
    assert!(e2e.contains("index_logical_document(Request::new"));
    assert!(e2e.contains("activate_document_version(Request::new"));
}
