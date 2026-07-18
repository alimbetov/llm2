use serde_json::Value;
use std::{path::Path, process::Command};

const BANK: &str = "benchmarks/hierarchical/fix486";
const HELPER: &str = "scripts/fix486d_proof.py";

#[test]
fn phase_d_selects_exactly_the_three_frozen_positive_queries() {
    let output = Command::new("python3")
        .args([
            HELPER,
            "select",
            "--bank",
            BANK,
            "--output",
            "/tmp/fix486d-selected.json",
        ])
        .output()
        .expect("run helper");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let selected: Value =
        serde_json::from_slice(&std::fs::read("/tmp/fix486d-selected.json").expect("selected"))
            .expect("selected JSON");
    assert_eq!(selected.as_array().expect("array").len(), 3);
    let ids = selected
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["query"]["query_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "q-child-parent-exact",
            "q-parent-dedup",
            "q-exact-identifier"
        ]
    );
    let _ = std::fs::remove_file("/tmp/fix486d-selected.json");
}

#[test]
fn phase_d_identity_validator_fails_closed_for_missing_child() {
    let temp = std::env::temp_dir().join("fix486d-invalid-identity.json");
    std::fs::write(&temp, r#"{"rows":[{"logical_zone_id":"zone-a"}]}"#).expect("write");
    let output = Command::new("python3")
        .args([
            HELPER,
            "validate-identity",
            "--input",
            temp.to_str().unwrap(),
        ])
        .output()
        .expect("run helper");
    assert!(!output.status.success());
    let _ = std::fs::remove_file(temp);
}

#[test]
fn phase_d_identity_validator_classifies_auxiliary_children_without_weakening_proof_rows() {
    let temp = std::env::temp_dir().join("fix486d-auxiliary-identity.json");
    let row = |chunk: &str, role: &str, granularity: &str, source: &str, logical: Value| {
        serde_json::json!({
            "logical_zone_id": "zone-a",
            "runtime_access_zone_id": "zone-runtime-a",
            "logical_document_id": "doc-hierarchy",
            "runtime_document_id": "document-runtime-a",
            "logical_version": 1,
            "runtime_chunk_id": chunk,
            "chunk_role": role,
            "granularity": granularity,
            "source_block_id": source,
            "content_sha256": format!("hash-{chunk}"),
            "logical_chunk_id": logical,
            "runtime_parent_chunk_id": "parent-runtime-a"
        })
    };
    let rows = serde_json::json!({"rows": [
        row("parent-runtime-a", "PARENT", "PARENT", "parent-a1", Value::String("parent-a1".into())),
        row("child-runtime-180", "CHILD", "SUB_180", "parent-a1", Value::Null),
        row("child-runtime-260", "CHILD", "SUB_260", "parent-a1", Value::Null),
        row("source-runtime-180", "CHILD", "SUB_180", "source-a", Value::String("source-a-180".into()))
    ]});
    std::fs::write(&temp, serde_json::to_vec(&rows).expect("identity JSON")).expect("write");
    let command = Command::new("python3")
        .args([
            HELPER,
            "validate-identity",
            "--input",
            temp.to_str().unwrap(),
            "--bank",
            BANK,
        ])
        .output()
        .expect("run helper");
    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    let validation: Value = serde_json::from_slice(&command.stdout).expect("validation JSON");
    assert_eq!(validation["status"], "PASS");
    assert_eq!(validation["identity_roles"]["PROOF_CHILD"], 2);
    assert_eq!(validation["identity_roles"]["AUXILIARY_CHILD"], 1);
    let _ = std::fs::remove_file(temp);
}

#[test]
fn phase_d_normalizer_accepts_protobuf_json_int64_version_without_weakening_validation() {
    let directory = std::env::temp_dir().join(format!("fix486d-version-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create temp directory");
    let query = directory.join("query.json");
    let qrel = directory.join("qrel.json");
    let response = directory.join("response.json");
    let identity = directory.join("identity.json");
    let output = directory.join("result.json");
    std::fs::write(
        &query,
        r#"{"query_id":"q-child-parent-exact","case_id":"FIX486-01"}"#,
    )
    .expect("query");
    std::fs::write(
        &qrel,
        r#"{"expected_parent":"parent-a1","expected_child_any":["child-a1-180"],"required_anchors_in_matched_text":["ORA-00904","content_chunks_v004"],"required_anchors_in_parent_text":["ASTRA_CANONICAL_STATE_A1"],"forbidden_anchors":[]}"#,
    )
    .expect("qrel");
    std::fs::write(
        &response,
        r#"{"results":[{"accessZoneId":"zone-runtime-a","documentId":"document-runtime-a","documentVersion":"1","matchedChunkId":"child-runtime-180","parentChunkId":"parent-runtime-a","matchedText":"ORA-00904 content_chunks_v004","parentText":"ASTRA_CANONICAL_STATE_A1"}],"diagnostics":{"rankingTrace":{"candidates":[]}}}"#,
    )
    .expect("response");
    std::fs::write(
        &identity,
        r#"{"rows":[{"logical_zone_id":"zone-a","runtime_access_zone_id":"zone-runtime-a","logical_document_id":"doc-hierarchy","runtime_document_id":"document-runtime-a","logical_version":1,"runtime_chunk_id":"parent-runtime-a","chunk_role":"PARENT","granularity":"PARENT","source_block_id":"parent-a1","content_sha256":"parent-hash","logical_chunk_id":"parent-a1"},{"logical_zone_id":"zone-a","runtime_access_zone_id":"zone-runtime-a","logical_document_id":"doc-hierarchy","runtime_document_id":"document-runtime-a","logical_version":1,"runtime_chunk_id":"child-runtime-180","chunk_role":"CHILD","granularity":"SUB_180","source_block_id":"parent-a1","content_sha256":"child-hash","logical_chunk_id":null}]}"#,
    )
    .expect("identity");
    let command = Command::new("python3")
        .args([
            HELPER,
            "normalize",
            "--query",
            query.to_str().unwrap(),
            "--qrel",
            qrel.to_str().unwrap(),
            "--entry-point",
            "Search",
            "--response",
            response.to_str().unwrap(),
            "--identity-map",
            identity.to_str().unwrap(),
            "--bank",
            BANK,
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("run normalizer");
    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    let result: Value =
        serde_json::from_slice(&std::fs::read(&output).expect("result")).expect("result JSON");
    assert_eq!(result["status"], "PASS");
    assert_eq!(result["runtime_identity"]["document_version"], 1);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn phase_d_runner_and_audit_are_phase_owned_and_fail_closed() {
    let runner =
        std::fs::read_to_string("scripts/fix486d-child-parent-runtime-proof.sh").expect("runner");
    let helper = std::fs::read_to_string(HELPER).expect("helper");
    assert!(helper.contains("frozen_child_lookup"));
    assert!(helper.contains("expected_hierarchy"));
    assert!(!helper.contains("replace(\"parent-\", \"child-\")"));
    assert!(runner.contains("UNKNOWN_FROZEN_PROFILE"));
    assert!(runner.contains("RETRIEVAL_PROFILE_LEXICAL_STRICT"));
    assert!(runner.contains("SEARCH_MODE_V005_SPARSE"));
    assert!(runner.contains(".bank_aggregate_sha256==$sha"));
    assert!(!runner.contains("'.aggregate_sha256==$sha'"));
    assert!(
        runner.contains("pre_dedup_distinct_child_count")
            || std::fs::read_to_string("src/grpc/mod.rs")
                .expect("grpc source")
                .contains("pre_dedup_distinct_child_count")
    );
    assert!(runner.contains("--execute-all"));
    for required_execution in [
        "warm_repeat",
        "restart_repeat",
        "entry-point-parity.json",
        "stage-results.json",
        "manifest-verification.json",
        "cleanup/summary.json",
        "jq -c .",
    ] {
        assert!(
            runner.contains(required_execution),
            "missing executable proof requirement {required_execution}"
        );
    }
    for required in [
        "set -Eeuo pipefail",
        "bootstrap.json",
        "terminal-result.json",
        "PREEXISTING_PORT_OWNER",
        "IDENTITY_MAP_INCOMPLETE",
        "CANONICAL_BINDING_INVALID",
        "FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED",
        "IndexLogicalDocument",
        "'4862' THEN 'zone-a'",
        "'4863' THEN 'zone-b'",
        "c.content_hash content_sha256",
    ] {
        assert!(
            runner.contains(required) || helper.contains(required),
            "missing fail-closed requirement {required}"
        );
    }
    assert!(Path::new("scripts/fix486d-child-parent-audit.sql").is_file());
    assert!(Path::new("docker-compose.fix486d.yml").is_file());
    let makefile = std::fs::read_to_string("Makefile").expect("Makefile");
    assert!(makefile.contains(
        "verify-fix486d-child-parent-runtime-proof: verify-fix486d-child-parent-runtime"
    ));
}
