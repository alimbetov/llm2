use std::fs;

#[test]
fn outbox_uses_operation_version_fencing() {
    let src = fs::read_to_string("src/outbox/mod.rs").unwrap();
    assert!(src.contains("event.operation_version"));
    assert!(src.contains("vector_outbox_stale_event_skipped_total"));
    assert!(src.contains("payload_version=$3"));
    assert!(src.contains("ttl_generation=$3"));
}

#[test]
fn reconciliation_is_not_noop_and_preserves_payload_contract() {
    let bin = fs::read_to_string("src/bin/astravector-reconciliation.rs").unwrap();
    let rec = fs::read_to_string("src/reconciliation/mod.rs").unwrap();
    let projection = fs::read_to_string("src/projection.rs").unwrap();
    assert!(bin.contains("--full"));
    assert!(bin.contains("reconcile_unsynced_batch"));
    assert!(rec.contains("CanonicalProjectionInput"));
    for required in [
        "access_level",
        "expires_at_epoch",
        "chunk_granularity",
        "dense_version",
        "tokenizer_version",
        "quarantined",
    ] {
        assert!(
            projection.contains(required),
            "canonical projection payload must contain {required}"
        );
    }
    assert!(rec.contains("reconciliation_skipped_legal_hold_total"));
}

#[test]
fn k8s_runtime_uses_qdrant_and_consistent_image() {
    let cm = fs::read_to_string("k8s/configmap.yaml").unwrap();
    let np = fs::read_to_string("k8s/networkpolicy.yaml").unwrap();
    let migration = fs::read_to_string("k8s/migration-job.yaml").unwrap();
    assert!(cm.contains("ASTRAVECTOR_QDRANT_ENABLED"));
    assert!(np.contains("port: 6333"));
    assert!(np.contains("port: 6334"));
    assert!(migration.contains("/usr/local/bin/astravector-runtime"));
    for file in [
        "k8s/deployment.yaml",
        "k8s/migration-job.yaml",
        "k8s/lifecycle-cronjob.yaml",
        "k8s/qdrant-publisher-deployment.yaml",
    ] {
        let src = fs::read_to_string(file).unwrap();
        assert!(
            src.contains("0.4.1-fix465-p2-production-hardening"),
            "{file} must use aligned fix465 image tag"
        );
    }
}

#[test]
fn grpc_timeout_parser_was_fixed() {
    let src = fs::read_to_string("src/grpc/mod.rs").unwrap();
    assert!(src.contains("let (num, unit) = s.split_at"));
    assert!(!src.contains("let (unit, num) = s.split_at"));
    assert!(src.contains("test_parse_grpc_timeout_100m"));
}
