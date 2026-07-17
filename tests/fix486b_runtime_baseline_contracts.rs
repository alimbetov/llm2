use serde_json::Value;
use std::fs;

const FIXTURE: &str = "benchmarks/hierarchical/fix486/runtime-baseline-control-v1.json";

#[test]
fn control_fixture_has_no_physical_identity_and_exercises_production_hierarchy() {
    let fixture: Value = serde_json::from_str(&fs::read_to_string(FIXTURE).expect("read fixture"))
        .expect("parse fixture");
    let request = &fixture["request"];
    assert_eq!(fixture["fixture_id"], "fix486b-runtime-control-v1");
    assert_eq!(fixture["access_zone_code"], "4861");
    assert_eq!(request["document"]["documentId"], Value::Null);
    assert_eq!(request["accessZoneId"], Value::Null);
    assert_eq!(
        request["indexingOptions"]["publishMode"],
        "PUBLISH_MODE_V005_OUTBOX"
    );
    assert!(request["chunkingOptions"]["createParentContext"]
        .as_bool()
        .unwrap());
    let rendered = request["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("ASTRA_FIX486B_RUNTIME_CONTROL"));
    assert!(
        rendered.contains("PostgreSQL is the canonical state and Qdrant is a search projection.")
    );
    assert!(rendered.split_whitespace().count() >= 180);
}

#[test]
fn phase_runner_contains_every_mandatory_gate_and_runtime_stage() {
    let runner = fs::read_to_string("scripts/fix486b-runtime-baseline.sh").expect("read runner");
    for required in [
        "cargo fmt --all --check",
        "cargo check --locked --all-targets --all-features",
        "cargo test --locked --all-targets --all-features",
        "cargo clippy --locked --all-targets --all-features -- -D warnings",
        "cargo sqlx prepare --check -- --all-targets --all-features",
        "--test e2e_testcontainers",
        "--test smoke_load_retrieve_context_testcontainers",
        "--test fix486_hierarchical_bank_contracts",
        "AstraVectorIngestionFacade/IndexLogicalDocument",
        "AstraVectorV004Control/Search",
        "AstraVectorRetrievalFacade/RetrieveContext",
        "run_clean R1",
        "run_clean R2",
        "run_r3",
        "r1-r2-normalized.json",
        "FIX486_RUNTIME_BASELINE_PASS",
        "FIX486_RUNTIME_BASELINE_BLOCKED",
    ] {
        assert!(runner.contains(required), "runner missing {required}");
    }
    assert!(!runner.contains("BANK_VERSION=1.0.0"));
}

#[test]
fn phase_infrastructure_is_isolated_from_default_project_state() {
    let compose = fs::read_to_string("docker-compose.fix486b.yml").expect("read compose");
    let runner = fs::read_to_string("scripts/fix486b-runtime-baseline.sh").expect("read runner");
    assert!(compose.contains("${FIX486B_POSTGRES_PORT:-56432}"));
    assert!(compose.contains("${FIX486B_QDRANT_HTTP_PORT:-6433}"));
    assert!(runner.contains("docker compose -p \"$ACTIVE_PROJECT\""));
    assert!(runner.contains("compose down -v"));
    assert!(!runner.contains("docker volume prune"));
    assert!(!runner.contains("docker system prune"));
    assert!(
        !runner.contains("${run,,}"),
        "runner must remain compatible with macOS Bash 3.2"
    );
}
