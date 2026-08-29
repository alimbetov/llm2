use std::fs;

fn grpc_source() -> String {
    fs::read_to_string(format!("{}/src/grpc/mod.rs", env!("CARGO_MANIFEST_DIR")))
        .expect("read src/grpc/mod.rs")
}

fn finalize_logical_document_ingestion_body(source: &str) -> &str {
    let start = source
        .find("async fn finalize_logical_document_ingestion")
        .expect("finalize_logical_document_ingestion exists");
    let end = source[start..]
        .find("async fn abort_logical_document_ingestion")
        .expect("abort_logical_document_ingestion follows finalize");
    &source[start..start + end]
}

#[test]
fn session_finalize_uses_manual_server_owned_activation_policy_contract() {
    let source = grpc_source();
    let finalize = finalize_logical_document_ingestion_body(&source);

    assert!(
        finalize.contains("activation_policy: session_finalize_activation_policy() as i32"),
        "session finalize must use the server-owned activation policy helper"
    );
    assert!(
        !finalize.contains("ActivationPolicy::AutoWhenReady as i32"),
        "session finalize must not select unsupported AUTO_WHEN_READY internally"
    );
}
