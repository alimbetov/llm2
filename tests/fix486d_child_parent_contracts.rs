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
    for mode in [
        "--verify-identities",
        "--prepare",
        "--ingest",
        "--execute-search",
        "--execute-retrieve-context",
        "--repeat",
        "--restart-proof",
        "--execute-all",
    ] {
        assert!(runner.contains(mode), "missing runner mode {mode}");
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
