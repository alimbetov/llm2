use serde_json::Value;
use std::{fs, process::Command};

fn runner() -> String {
    fs::read_to_string("scripts/fix486e-isolation-lifecycle-runtime-proof.sh").unwrap()
}

#[test]
fn phase_e_selects_exact_frozen_campaign() {
    let output = tempfile::NamedTempFile::new().unwrap();
    let status = Command::new("python3")
        .args([
            "scripts/fix486e_proof.py",
            "select",
            "--bank",
            "benchmarks/hierarchical/fix486",
            "--output",
            output.path().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let selected: Value =
        serde_json::from_str(&fs::read_to_string(output.path()).unwrap()).unwrap();
    let ids: Vec<_> = selected
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["query"]["query_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["q-zone-a", "q-zone-b", "q-active-version"]);
}

#[test]
fn phase_e_runner_is_phase_owned_bounded_and_fail_closed() {
    let runner = runner();
    for required in [
        "FIX486E_RUN_ID",
        "docker-compose.fix486e.yml",
        "application-fix486e.yaml",
        "FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS",
        "bootstrap.json",
        "terminal-result.json",
        "manifest-verification.json",
        "cleanup/summary.json",
        "DOCUMENT_DEADLINE_MS >= 1000 && DOCUMENT_DEADLINE_MS <= 600000",
        "INGESTION_DOCUMENT_DEADLINE_RESOLVED",
        "grpc.health.v1.Health/Check",
        "--execute-all",
    ] {
        assert!(runner.contains(required), "missing {required}");
    }
    assert!(runner.contains("58432"));
    assert!(runner.contains("50587"));
    assert!(!runner.contains("docker-compose.fix486d.yml"));
}

#[test]
fn phase_e_contract_covers_isolation_lifecycle_and_repeatability() {
    let runner = runner();
    let helper = fs::read_to_string("scripts/fix486e_proof.py").unwrap();
    let audit = fs::read_to_string("scripts/fix486e-isolation-lifecycle-audit.sql").unwrap();
    for required in [
        "q-zone-a",
        "q-zone-b",
        "q-active-version",
        "opposite-zone-results.jsonl",
        "ASTRA_INACTIVE_VERSION_TRAP",
        "ASTRA_DELETED_PARENT_TRAP",
        "ASTRA_EXPIRED_PARENT_TRAP",
        "warm_repeat",
        "restart_repeat",
        "legal-hold/audit.json",
        "isolation/hard-gates.json",
        "zone-ttl-policy.json",
        "RESOLVED_PER_ACCESS_ZONE_CODE",
    ] {
        assert!(
            runner.contains(required) || helper.contains(required),
            "missing {required}"
        );
    }
    for required in [
        "cross_zone_bindings",
        "wrong_version_results",
        "inactive_version_results",
        "deleted_version_results",
        "expired_version_results",
        "legal_hold_visibility_bypasses",
        "cleanup_selector",
        "default_ttl_days",
    ] {
        assert!(
            runner.contains(required) || audit.contains(required) || helper.contains(required),
            "missing {required}"
        );
    }
    for required in [
        "access_zone_code IN ('4862','4863')",
        "ACCESS_ZONE_POLICY",
        "EXPLICIT_TEST_CLOCK_OVERRIDE",
        "ttl_days",
    ] {
        assert!(runner.contains(required), "missing TTL contract {required}");
    }
}

#[test]
fn phase_e_identity_is_composite_and_rejects_cross_zone_collisions() {
    let helper = fs::read_to_string("scripts/fix486e_proof.py").unwrap();
    for required in [
        "runtime_access_zone_id",
        "runtime_document_id",
        "runtime_chunk_id",
        "CROSS_ZONE_PHYSICAL_ID_COLLISION",
        "zone-a",
        "zone-b",
    ] {
        assert!(helper.contains(required));
    }
}

#[test]
fn phase_e_identity_requirements_come_from_frozen_zone_hierarchy() {
    let helper = fs::read_to_string("scripts/fix486e_proof.py").unwrap();
    assert!(helper.contains("frozen_children = frozen_child_lookup(bank)"));
    assert!(helper.contains("common_logical_ids"));
    assert!(!helper.contains(r#"for logical in ("parent-a1", "child-a1-180", "child-a1-260")"#));
}

#[test]
fn phase_e_ingestion_assigns_zone_scoped_document_ids() {
    let runner = runner();
    assert!(runner.contains(r#"f"fix486e:{sys.argv[1]}:{sys.argv[2]}""#));
    assert!(runner.contains(".request.document.documentId=$document_id"));
    assert!(runner.contains("documentId:$document_id,documentVersion:$version"));
}

#[test]
fn phase_e_make_targets_share_official_execute_path() {
    let makefile = fs::read_to_string("Makefile").unwrap();
    assert!(makefile.contains("verify-fix486e-isolation-lifecycle-runtime:"));
    assert!(makefile.contains("verify-fix486e-isolation-lifecycle-runtime-proof:"));
    assert_eq!(
        makefile
            .matches("./scripts/fix486e-isolation-lifecycle-runtime-proof.sh --execute-all")
            .count(),
        1
    );
}

#[test]
fn phase_e_lifecycle_metadata_matches_protobuf_string_map() {
    let runner = runner();
    assert!(runner.contains(r#"metadata:{fix486e_lifecycle_trap:"true"}"#));
    assert!(!runner.contains("metadata:{fix486e_lifecycle_trap:true}"));
}
