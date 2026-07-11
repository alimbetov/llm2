use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn load_profile_has_bounded_env_overrides() {
    let profile = read("config/application-load-m2.yaml");
    for name in [
        "ASTRAVECTOR_QUERY_QUEUE_CAPACITY",
        "ASTRAVECTOR_QUERY_MAX_QUEUE_AGE_MS",
        "ASTRAVECTOR_QUERY_MIN_INFERENCE_BUDGET_MS",
        "ASTRAVECTOR_QUERY_MAX_DEADLINE_SKEW_MS",
        "ASTRAVECTOR_MAX_CONCURRENT_RETRIEVE_CONTEXT",
        "ASTRAVECTOR_MAX_CONCURRENT_QDRANT_SEARCH",
        "ASTRAVECTOR_MAX_CONCURRENT_GRAPH_EXPANSION",
        "ASTRAVECTOR_MAX_CONCURRENT_MMR_FETCH",
        "ASTRAVECTOR_BACKPRESSURE_ACQUIRE_TIMEOUT_MS",
    ] {
        assert!(profile.contains(name), "missing {name}");
    }
    assert!(profile.contains("queue_capacity: ${ASTRAVECTOR_QUERY_QUEUE_CAPACITY:-32}"));
}

#[test]
fn query_and_document_retry_policies_are_separate() {
    let config = read("config/application.yaml");
    let retry = config.split("resilience:").nth(1).unwrap();
    assert!(retry.contains("query:"));
    assert!(retry.contains("retry_on_timeout: false"));
    assert!(retry.contains("document:"));
    assert!(retry.contains("retry_on_timeout: true"));
}

#[test]
fn scheduler_has_queue_age_budget_and_closed_mapping() {
    let scheduler = read("src/scheduler/mod.rs");
    for contract in [
        "enqueued_at: Instant",
        "queue_age_exceeded",
        "insufficient_inference_budget",
        "TrySendError::Full",
        "TrySendError::Closed",
        "astravector_batch_deadline_skew_seconds",
        "astravector_retry_skipped_total",
    ] {
        assert!(scheduler.contains(contract), "missing {contract}");
    }
}

#[test]
fn load_gate_blocks_dirty_tree_before_evidence_creation() {
    let script = read("scripts/macbook-model-backed-load.sh");
    let dirty = script.find("git status --porcelain").unwrap();
    let evidence = script.find("mkdir -p \"$EVIDENCE_DIR\"/").unwrap();
    assert!(dirty < evidence);
    assert!(script.contains("DIRTY_GIT_AT_START"));
    assert!(script.contains("ASTRAVECTOR_EVIDENCE_ROOT"));
}

#[test]
fn recovery_gate_uses_ceil_windows_and_ttr() {
    let script = read("scripts/macbook-model-backed-load.sh");
    assert!(script.contains("(stable_rps * 65 + 99) / 100"));
    assert!(script.contains("window-$(printf '%03d'"));
    assert!(script.contains("consecutive_healthy >= 3"));
    let report = read("scripts/finalize_macbook_load_report.py");
    assert!(report.contains("time_to_recovery_seconds"));
    assert!(report.contains("\"schema_version\": \"3.0\""));
}

#[test]
fn load_gate_binds_evidence_to_expected_release_and_integrity() {
    let script = read("scripts/macbook-model-backed-load.sh");
    let finalizer = read("scripts/finalize_macbook_load_report.py");
    for required in [
        "ASTRAVECTOR_EXPECTED_RELEASE_SHA",
        "RELEASE_SHA_MISMATCH",
        "integrity_snapshot",
        "stabilized-metrics-before.prom",
        "stabilized-metrics-after.prom",
        "ASTRAVECTOR_QDRANT_URL=http://127.0.0.1:6333",
        "postgres_ready=false",
        "qdrant_ready=false",
    ] {
        assert!(
            script.contains(required),
            "missing load-gate contract: {required}"
        );
    }
    for required in [
        "head_matches_expected_release_sha",
        "introduced_violations_total",
        "stabilized_admission_rejects == 0",
        "query_depth_after == 0",
    ] {
        assert!(
            finalizer.contains(required),
            "missing finalizer contract: {required}"
        );
    }
}

#[test]
fn optional_degradation_is_observable_and_guarded() {
    let grpc = read("src/grpc/mod.rs");
    assert!(grpc.contains("allow_partial_dense_sparse_fallback"));
    assert!(grpc.contains("astravector_degraded_path_total"));
    assert!(grpc.contains("GRAPH_PATH_DEGRADED_TO_DIRECT_RETRIEVAL"));
    assert!(grpc.contains("MMR_PATH_DEGRADED_TO_TOKEN_FALLBACK"));
}
