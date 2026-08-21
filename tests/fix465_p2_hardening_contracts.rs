use std::{fs, path::Path};

#[test]
fn test_qdrant_payload_indexes_are_created_for_filter_fields() {
    let src = fs::read_to_string("src/qdrant/mod.rs").expect("read qdrant source");
    for field in [
        "access_zone_id",
        "lifecycle_status",
        "chunk_granularity",
        "document_id",
        "document_version",
        "access_level",
        "expires_at_epoch",
        "quarantined",
        "model_version",
        "tokenizer_version",
        "dense_version",
        "sparse_version",
        "chunking_profile_version",
        "binding_id",
        "qdrant_point_id",
    ] {
        assert!(
            src.contains(field),
            "qdrant payload index contract must include {field}"
        );
    }
    assert!(
        src.contains("ensure_payload_indexes"),
        "ensure_collection must create payload indexes"
    );
    assert!(
        src.contains("qdrant_payload_index_create_total"),
        "payload index creation must be observable"
    );
}

#[test]
fn test_retrieved_contexts_metrics_count_all_sources() {
    let src = fs::read_to_string("src/grpc/mod.rs").expect("read grpc source");
    assert!(
        src.contains("fn extraction_retrieval_sources"),
        "source extraction helper must exist"
    );
    assert!(
        src.contains("retrieval_sources"),
        "merged retrieval_sources metadata must be supported"
    );
    assert!(
        src.contains("for source in extraction_retrieval_sources"),
        "metrics must increment for every source, not only primary source"
    );
}

#[test]
fn test_checksum_errors_are_safe_for_user_facing_status() {
    let src = fs::read_to_string("src/checksum.rs").expect("read checksum source");
    assert!(
        src.contains("model/tokenizer checksum mismatch"),
        "safe checksum mismatch message required"
    );
    assert!(
        !src.contains("expected={expected}"),
        "expected checksum must not be exposed in user-facing error"
    );
    assert!(
        !src.contains("actual={actual}"),
        "actual checksum must not be exposed in user-facing error"
    );
}

#[test]
fn test_postgres_timeouts_use_set_config_with_binds() {
    let src = fs::read_to_string("src/persistence/mod.rs").expect("read persistence source");
    assert!(
        src.contains("set_config('statement_timeout'"),
        "statement timeout must use set_config"
    );
    assert!(
        src.contains("set_config('lock_timeout'"),
        "lock timeout must use set_config"
    );
    assert!(
        src.contains("set_config('idle_in_transaction_session_timeout'"),
        "idle timeout must use set_config"
    );
    assert!(
        !src.contains("format!(\"SET statement_timeout"),
        "dynamic SET statement_timeout SQL must not be used"
    );
}

#[test]
fn test_fix465_version_alignment() {
    let tag = "0.4.1-image-contract";
    let registry_ref = "registry.astrabase.asia/astravector:0.4.1-image-contract";
    let cargo = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    assert!(
        cargo.contains("version = \"0.4.1\""),
        "Cargo version must be 0.4.1"
    );
    for file in [
        ".github/workflows/ci.yml",
        "k8s/deployment.yaml",
        "k8s/lifecycle-cronjob.yaml",
        "k8s/qdrant-publisher-deployment.yaml",
        "k8s/migration-job.yaml",
    ] {
        let src = fs::read_to_string(file).expect("read versioned file");
        assert!(
            src.contains(tag),
            "{file} must use aligned image-contract tag"
        );
        if file.starts_with("k8s/") {
            assert!(
                src.contains(registry_ref),
                "{file} must use the private registry image-contract reference"
            );
            assert!(
                src.contains("astravector-registry-pull"),
                "{file} must reference the private registry pull secret"
            );
        }
        assert!(
            !src.contains("0.4.0-fix463-production-candidate-stabilization"),
            "{file} must not use the old fix463 image tag"
        );
    }
}

#[test]
fn test_enrichment_binary_is_out_of_production_image_scope() {
    let dockerfile = fs::read_to_string("Dockerfile").expect("read Dockerfile");
    assert!(
        !dockerfile.contains("/usr/local/bin/astravector-enrichment"),
        "no-op enrichment worker must not be copied into the production image"
    );
    let backlog =
        fs::read_to_string("docs/FIX465_KNOWN_HARDENING_BACKLOG.md").expect("read fix465 backlog");
    assert!(
        backlog.contains("astravector-enrichment"),
        "enrichment scope must be documented"
    );
}

#[test]
fn test_grafana_dashboards_are_valid_json_and_reference_known_metrics() {
    let dashboard_dir = Path::new("observability/grafana");
    let mut count = 0;
    for entry in fs::read_dir(dashboard_dir).expect("read dashboard dir") {
        let path = entry.expect("dashboard entry").path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        count += 1;
        let src = fs::read_to_string(&path).expect("read dashboard json");
        let json: serde_json::Value =
            serde_json::from_str(&src).expect("dashboard must be valid JSON");
        assert!(
            json.get("title").is_some(),
            "dashboard must have title: {}",
            path.display()
        );
        assert!(
            src.contains("retrieved_contexts")
                || src.contains("qdrant")
                || src.contains("vector_outbox")
                || src.contains("index_ttl")
                || src.contains("reconciliation")
                || src.contains("retention"),
            "dashboard must reference AstraVector metrics: {}",
            path.display()
        );
    }
    assert!(
        count >= 5,
        "fix465 must include overview/retrieval/consistency/ttl/runtime dashboards"
    );
}

#[test]
fn test_blocking_self_contained_smoke_load_gate_exists() {
    let ci = fs::read_to_string(".github/workflows/ci.yml").expect("read ci");
    assert!(
        ci.contains("smoke-load-testcontainers"),
        "blocking self-contained smoke-load job must exist"
    );
    assert!(
        ci.contains("smoke_load_retrieve_context_testcontainers"),
        "CI must run the self-contained smoke-load test"
    );
    let test = fs::read_to_string("tests/smoke_load_retrieve_context_testcontainers.rs")
        .expect("read smoke test");
    assert!(
        test.contains("testcontainers"),
        "smoke test must be self-contained with testcontainers"
    );
    assert!(
        test.contains("CONCURRENCY: usize = 50"),
        "smoke test must use 50 concurrent RetrieveContext requests"
    );
}
