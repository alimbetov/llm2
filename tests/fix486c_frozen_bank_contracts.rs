use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const ROOT: &str = "benchmarks/hierarchical/fix486";
const VERIFIER: &str = "scripts/fix486c_verify_frozen_bank.py";

fn run_verifier(root: &Path, dry_run: bool) -> std::process::Output {
    let mut command = Command::new("python3");
    command.arg(VERIFIER).arg("--root").arg(root);
    if dry_run {
        command.arg("--dry-run");
    }
    command.output().expect("run frozen bank verifier")
}

fn temporary_copy() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("fix486c-bank-{nonce}"));
    for relative in [
        "corpus/hierarchical-fixture-v1.json",
        "queries/hierarchical-queries-v1.jsonl",
        "qrels/hierarchical-qrels-v1.jsonl",
        "graph-relations/hierarchical-graph-v1.json",
        "lifecycle/hierarchical-lifecycle-v1.json",
        "bank-manifest.json",
    ] {
        let source = Path::new(ROOT).join(relative);
        let destination = root.join(relative);
        fs::create_dir_all(destination.parent().expect("parent")).expect("create parent");
        fs::copy(source, destination).expect("copy frozen bank file");
    }
    root
}

#[test]
fn frozen_bank_verifies_and_dry_run_schedules_every_query() {
    let output = run_verifier(Path::new(ROOT), true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("dry-run JSON");
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["bank_version"], "1.0.0");
    assert_eq!(report["scheduled_queries"], 11);
    assert!(report["plans"]
        .as_array()
        .is_some_and(|plans| plans.iter().all(|plan| plan["status"] == "PASS")));
}

#[test]
fn verifier_fails_closed_for_byte_mutation_missing_file_and_extra_file() {
    let root = temporary_copy();
    let query = root.join("queries/hierarchical-queries-v1.jsonl");
    fs::write(
        &query,
        format!("{} ", fs::read_to_string(&query).expect("read query")),
    )
    .expect("mutate query");
    assert!(!run_verifier(&root, false).status.success());

    fs::remove_file(&query).expect("remove payload");
    assert!(!run_verifier(&root, false).status.success());

    fs::copy(
        Path::new(ROOT).join("queries/hierarchical-queries-v1.jsonl"),
        &query,
    )
    .expect("restore payload");
    fs::write(root.join("unexpected.json"), "{}\n").expect("add untracked file");
    assert!(!run_verifier(&root, false).status.success());
    fs::remove_dir_all(root).expect("remove temporary bank");
}

#[test]
fn runtime_runner_uses_portable_json_loading_and_waits_for_activation() {
    let runner = fs::read_to_string("scripts/fix486c-frozen-bank.sh").expect("read runner");
    assert!(runner.contains("wait_for_activation"));
    assert!(runner.contains("OUTBOX_NOT_FINALIZED"));
    assert!(
        !runner.contains("--argfile"),
        "macOS jq does not support --argfile"
    );
}

#[test]
fn generated_large_parent_plan_is_split_into_chunkable_paragraphs() {
    let output = Command::new("python3")
        .arg(VERIFIER)
        .arg("--emit-ingestion-plans")
        .output()
        .expect("emit ingestion plans");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("ingestion plan JSON");
    let large = report["ingestion_plans"]
        .as_array()
        .expect("plans")
        .iter()
        .flat_map(|plan| plan["request"]["blocks"].as_array().expect("blocks"))
        .find(|block| block["blockId"] == "parent-large")
        .expect("large parent block");
    assert!(large["text"]
        .as_str()
        .expect("large parent text")
        .split("\n\n")
        .all(|paragraph| paragraph.split_whitespace().count() <= 256));
}
