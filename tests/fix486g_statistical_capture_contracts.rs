use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const SCRIPT: &str = "scripts/fix486g_statistical_capture.py";
const BANK: &str = "benchmarks/hierarchical/fix486g-supplemental";

fn write_json(path: &Path, value: Value) {
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
    )
    .unwrap();
}

fn fake_grpcurl(dir: &Path) -> PathBuf {
    let path = dir.join("fake-grpcurl.py");
    fs::write(
        &path,
        r#"#!/usr/bin/env python3
import json
import os
import sys

request = json.load(sys.stdin)
with open(os.environ["FIX486G_FAKE_CALL_LOG"], "a", encoding="utf-8") as log:
    log.write(sys.argv[-1] + "\n")
graph = request.get("enableGraphExpansion", False)
diagnostics = {
    "graphExpansionDurationMs": str(2 if graph else 0),
    "postgresHydrationMs": "1",
    "candidateCount": 4,
    "finalCandidateCount": 2,
    "graphCandidatesCount": 1 if graph else 0,
    "hopCount": 1 if graph else 0
}
context = {
    "matchedChunkId": "runtime-child",
    "parentChunkId": "runtime-parent",
    "metadata": {"graph_hop_distance": "1"} if graph else {}
}
if sys.argv[-1].endswith("/Search"):
    print(json.dumps({"results": [context], "diagnostics": diagnostics}))
else:
    print(json.dumps({
        "contexts": [context],
        "summary": {"evidenceStatus": "EVIDENCE_STATUS_FOUND"},
        "diagnostics": diagnostics
    }))
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn fixtures(dir: &Path) -> (PathBuf, PathBuf) {
    let identity = dir.join("identity.json");
    write_json(
        &identity,
        json!({"rows": [{
            "logical_zone_id": "zone-a",
            "runtime_access_zone_id": "runtime-zone-a"
        }]}),
    );
    let resource = dir.join("resource-evidence.json");
    write_json(
        &resource,
        json!({
            "schema_version": 1,
            "source": "fake bounded per-request instrumentation counters",
            "telemetry": {
                "sql_statement_count": {"value": 3, "upper_bound": 3},
                "qdrant_request_count": {"value": 1, "upper_bound": 1},
                "graph_relation_query_count": {
                    "enabled_value": 1,
                    "disabled_value": 0,
                    "upper_bound": 1,
                    "formula_source": "one bounded graph relation query iff request enables graph"
                },
                "n_plus_one_sql_hydration": false
            }
        }),
    );
    (identity, resource)
}

fn base_command(fake: &Path, identity: &Path, output: &Path, call_log: &Path) -> Command {
    let mut command = Command::new("python3");
    command
        .arg(SCRIPT)
        .args(["--endpoint", "127.0.0.1:50588", "--bank", BANK])
        .arg("--identity-map")
        .arg(identity)
        .args(["--run-kind", "warm", "--run-index", "1"])
        .arg("--output")
        .arg(output)
        .args(["--deadline-ms", "1000", "--grpcurl-bin"])
        .arg(fake)
        .env("FIX486G_FAKE_CALL_LOG", call_log);
    command
}

fn jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn response_file_count(path: &Path) -> usize {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| {
            if path.is_dir() {
                response_file_count(&path)
            } else if path.file_name().and_then(|name| name.to_str()) == Some("response.json") {
                1
            } else {
                0
            }
        })
        .sum()
}

#[test]
fn fake_grpcurl_full_pass_makes_exactly_142_calls_and_appends_complete_jsonl() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fake_grpcurl(temp.path());
    let (identity, resource) = fixtures(temp.path());
    let output = temp.path().join("capture.jsonl");
    let call_log = temp.path().join("calls.log");
    let status = base_command(&fake, &identity, &output, &call_log)
        .arg("--resource-evidence")
        .arg(&resource)
        .status()
        .unwrap();
    assert!(status.success());

    let calls = fs::read_to_string(call_log).unwrap();
    assert_eq!(calls.lines().count(), 142);
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.ends_with("/Search"))
            .count(),
        71
    );
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.ends_with("/RetrieveContext"))
            .count(),
        71
    );

    let rows = jsonl(&output);
    assert_eq!(rows.len(), 142);
    assert_eq!(
        rows.iter()
            .map(|row| row["query_id"].as_str().unwrap())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        71
    );
    for row in rows {
        assert_eq!(row["schema_version"], 1);
        assert_eq!(row["run_kind"], "warm");
        assert_eq!(row["run_index"], 1);
        assert!(row["latency_ms"].as_f64().unwrap() >= 0.0);
        assert!(row["response"].is_object());
        assert!(row["resource_evidence"]["sha256"].as_str().unwrap().len() == 64);
        for field in [
            "graph_expansion_ms",
            "canonical_graph_hydration_ms",
            "candidates_before_validation",
            "candidates_after_validation",
            "candidate_max",
            "hop_count",
            "hop_max",
            "sql_statement_count",
            "qdrant_request_count",
            "graph_relation_query_count",
            "n_plus_one_sql_hydration",
            "graph_executed",
        ] {
            assert!(!row["telemetry"][field].is_null(), "missing {field}");
            assert!(
                row["telemetry_sources"][field].is_string(),
                "missing source for {field}"
            );
        }
    }
    assert_eq!(response_file_count(&temp.path().join("capture.raw")), 142);
}

#[test]
fn missing_resource_evidence_fails_before_any_grpc_call() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fake_grpcurl(temp.path());
    let (identity, _) = fixtures(temp.path());
    let output = temp.path().join("capture.jsonl");
    let call_log = temp.path().join("calls.log");
    let result = base_command(&fake, &identity, &output, &call_log)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--resource-evidence"));
    assert!(!output.exists());
    assert!(!call_log.exists());
}
