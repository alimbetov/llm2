use serde_json::{json, Map, Value};
use std::{collections::HashMap, fs, path::Path, process::Command};

const SCRIPT: &str = "scripts/fix486g_statistical_proof.py";
const BANK: &str = "benchmarks/hierarchical/fix486g-supplemental";
const ARTIFACTS: [&str; 7] = [
    "statistical-report.json",
    "statistical-report.md",
    "per-query-results.jsonl",
    "per-slice-metrics.json",
    "latency-distribution.json",
    "safety-hard-gates.json",
    "confidence-intervals.json",
];

fn jsonl(path: impl AsRef<Path>) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn assignments() -> HashMap<String, String> {
    jsonl(format!("{BANK}/qrels/query-qrel-assignments-v1.jsonl"))
        .into_iter()
        .map(|row| {
            (
                row["query_id"].as_str().unwrap().to_owned(),
                row["qrel_profile"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn direct_context(parent: &str) -> Value {
    let (child, text) = match parent {
        "parent-a1" => ("child-a1-180", "ASTRA_CANONICAL_STATE_A1"),
        "parent-a2" => ("child-a2-180", "ASTRA_LEGAL_HOLD_A2"),
        _ => panic!("unknown direct parent"),
    };
    json!({
        "matchedChunkId": child,
        "parentChunkId": parent,
        "matchedLogicalId": child,
        "parentLogicalId": parent,
        "logicalAccessZoneId": "zone-a",
        "documentVersion": 1,
        "matchedText": "direct evidence",
        "parentText": text,
        "metadata": {"retrieval_source": "DIRECT"}
    })
}

fn graph_context(parent: &str) -> Value {
    json!({
        // Production returns hydrated parent evidence. The protected related
        // child identity remains in Graph provenance.
        "matchedChunkId": parent,
        "parentChunkId": parent,
        "parentLogicalId": parent,
        "logicalAccessZoneId": "zone-a",
        "documentVersion": 1,
        "matchedText": "graph evidence",
        "parentText": "ASTRA_RECONCILIATION_A3",
        "metadata": {
            "retrieval_source": "GRAPH_EXPANDED",
            "graph_seed_access_zone_id": "zone-a-runtime",
            "graph_seed_document_id": "doc-runtime",
            "graph_seed_document_version": "1",
            "graph_seed_chunk_id": "child-a1-180",
            "graph_seed_parent_chunk_id": "parent-a1",
            "graph_relation_id": "relation-a1-a3",
            "graph_edge_id": "edge-a1-a3",
            "graph_relation_type": "REPAIRED_BY",
            "graph_relation_score": "0.95",
            "graph_related_access_zone_id": "zone-a-runtime",
            "graph_related_document_id": "doc-runtime",
            "graph_related_document_version": "1",
            "graph_related_chunk_id": "child-a3-180",
            "graph_related_parent_chunk_id": parent,
            "graph_hop_distance": "1",
            "graph_binding_valid": true,
            "lifecycle_status": "ACTIVE"
        }
    })
}

fn fault_evidence(setup: &str) -> Value {
    let (class, reason) = match setup {
        "graph_wrong_parent_overlay" => ("WRONG_PARENT", "BINDING_INVALID"),
        "graph_cross_zone_overlay" => ("CROSS_ZONE", "GRAPH_ENDPOINT_ZONE_MISMATCH"),
        "graph_inactive_deleted_expired_overlay" => ("LIFECYCLE_INVALID", "VISIBILITY_REJECTED"),
        "graph_second_hop_overlay" => ("HOP_LIMIT", "HOP_LIMIT_REJECTED"),
        "graph_cycle_overlay" => ("CYCLE_OR_DUPLICATE", "GRAPH_CYCLE_REJECTED"),
        _ => panic!("unknown fault setup"),
    };
    json!({
        "graph_failure_injected": true,
        "graph_failure_detected": true,
        "graph_failure_classification": class,
        "semantic_no_answer": false,
        "partial_graph_evidence": true,
        "reported_full_coverage": false,
        "rejection_reasons": [reason],
        "rejection_observation": {
            "status": "PASS",
            "observed": true,
            "reason": reason,
            "source": "focused verified topology"
        }
    })
}

fn observation(
    query: &Value,
    qrel_profile: &str,
    entry_point: &str,
    run_kind: &str,
    run_index: Option<u64>,
    pair_id: Option<&str>,
) -> Value {
    let graph_enabled = query["enable_graph_expansion"].as_bool().unwrap();
    let (status, contexts) = match qrel_profile {
        "NEGATIVE_NO_ANSWER" => ("NO_ANSWER", vec![]),
        "NEGATIVE_LEGAL_HOLD" => ("FOUND", vec![direct_context("parent-a2")]),
        "GRAPH_DISABLED" => ("FOUND", vec![direct_context("parent-a1")]),
        _ => (
            "FOUND",
            vec![graph_context("parent-a3"), direct_context("parent-a1")],
        ),
    };
    let response_key = if entry_point == "Search" {
        "results"
    } else {
        "contexts"
    };
    let mut response = Map::new();
    response.insert("status".into(), json!(status));
    response.insert(response_key.into(), Value::Array(contexts));
    let mut row = json!({
        "schema_version": 1,
        "query_id": query["query_id"],
        "entry_point": entry_point,
        "run_kind": run_kind,
        "latency_ms": 12.0,
        "started_at_unix_ns": 1000000000,
        "finished_at_unix_ns": 1012000000,
        "deadline_ms": 1000.0,
        "jitter_allowance_ms": 25.0,
        "telemetry": {
            "graph_expansion_ms": if graph_enabled { 2.0 } else { 0.0 },
            "canonical_graph_hydration_ms": if graph_enabled { 1.0 } else { 0.0 },
            "candidates_before_validation": 4,
            "candidates_after_validation": 2,
            "candidate_max": 64,
            "hop_count": if graph_enabled { 1 } else { 0 },
            "hop_max": 1,
            "sql_statement_count": 3,
            "qdrant_request_count": 1,
            "graph_relation_query_count": if graph_enabled { 1 } else { 0 },
            "n_plus_one_sql_hydration": false,
            "graph_executed": graph_enabled
        },
        "response": Value::Object(response)
    });
    if let Some(index) = run_index {
        row["run_index"] = json!(index);
    }
    if let Some(pair) = pair_id {
        row["pair_id"] = json!(pair);
    }
    if let Some(setup) = query.get("fault_setup").and_then(Value::as_str) {
        row["degradation"] = fault_evidence(setup);
    } else if run_kind == "concurrent_healthy" {
        row["degradation"] = json!({"healthy_request_affected": false});
    }
    row
}

fn synthetic_campaign() -> Vec<Value> {
    let queries = jsonl(format!("{BANK}/queries/graph-parent-queries-v1.jsonl"));
    let profiles = assignments();
    let mut rows = Vec::new();
    for (kind, repeats) in [("warm", 3_u64), ("restart", 2_u64)] {
        for index in 1..=repeats {
            for query in &queries {
                let profile = &profiles[query["query_id"].as_str().unwrap()];
                for entry in ["Search", "RetrieveContext"] {
                    rows.push(observation(query, profile, entry, kind, Some(index), None));
                }
            }
        }
    }
    let fault = queries
        .iter()
        .find(|query| query.get("fault_setup").is_some())
        .unwrap();
    let healthy = queries
        .iter()
        .find(|query| query["query_id"] == "g-pos-ru-01")
        .unwrap();
    for index in 1..=10 {
        let pair = format!("pair-{index:02}");
        let entry = if index % 2 == 0 {
            "RetrieveContext"
        } else {
            "Search"
        };
        rows.push(observation(
            fault,
            &profiles[fault["query_id"].as_str().unwrap()],
            entry,
            "concurrent_fault",
            None,
            Some(&pair),
        ));
        rows.push(observation(
            healthy,
            &profiles[healthy["query_id"].as_str().unwrap()],
            entry,
            "concurrent_healthy",
            None,
            Some(&pair),
        ));
    }
    rows
}

fn write_rows(path: &Path, rows: &[Value]) {
    let content = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{content}\n")).unwrap();
}

fn report(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn write_identity_map(path: &Path) {
    let rows = ["parent-a1", "parent-a3"]
        .into_iter()
        .map(|parent| {
            json!({
                "runtime_chunk_id": parent,
                "logical_chunk_id": parent,
                "source_block_id": parent,
                "runtime_parent_chunk_id": Value::Null
            })
        })
        .chain(
            [("child-a1-180", "parent-a1"), ("child-a3-180", "parent-a3")]
                .into_iter()
                .map(|(child, parent)| {
                    json!({
                        "runtime_chunk_id": child,
                        "logical_chunk_id": child,
                        "source_block_id": parent,
                        "runtime_parent_chunk_id": parent
                    })
                }),
        )
        .collect::<Vec<_>>();
    fs::write(path, serde_json::to_string(&json!({"rows": rows})).unwrap()).unwrap();
}

#[test]
fn plan_is_offline_and_requires_142_results_per_full_pass() {
    let source = fs::read_to_string(SCRIPT).unwrap();
    for forbidden in [
        "import requests",
        "import urllib",
        "import socket",
        "import subprocess",
        "http://",
        "https://",
    ] {
        assert!(
            !source.contains(forbidden),
            "offline evaluator contains network capability {forbidden}"
        );
    }
    let output = tempfile::NamedTempFile::new().unwrap();
    let status = Command::new("python3")
        .args([SCRIPT, "plan", "--bank", BANK, "--output"])
        .arg(output.path())
        .status()
        .unwrap();
    assert!(status.success());
    let plan = report(output.path());
    assert_eq!(plan["status"], "PASS");
    assert_eq!(plan["network_calls"], false);
    assert_eq!(plan["query_count"], 71);
    assert_eq!(plan["results_per_full_pass"], 142);
    assert_eq!(plan["full_passes"]["warm"]["minimum"], 3);
    assert_eq!(plan["full_passes"]["restart"]["minimum"], 2);
    assert_eq!(plan["concurrent_pairs"]["minimum"], 10);
    assert_eq!(plan["minimum_raw_observations"], 730);
}

#[test]
fn synthetic_complete_campaign_passes_and_writes_every_artifact() {
    let input = tempfile::NamedTempFile::new().unwrap();
    let output = tempfile::tempdir().unwrap();
    let rows = synthetic_campaign();
    assert_eq!(rows.len(), 730);
    write_rows(input.path(), &rows);

    let validation = tempfile::NamedTempFile::new().unwrap();
    let dry_status = Command::new("python3")
        .args([SCRIPT, "dry-validate", "--bank", BANK, "--raw-input"])
        .arg(input.path())
        .args(["--output"])
        .arg(validation.path())
        .status()
        .unwrap();
    assert!(dry_status.success());
    assert_eq!(report(validation.path())["results_per_full_pass"], 142);

    let status = Command::new("python3")
        .args([SCRIPT, "evaluate", "--bank", BANK, "--raw-input"])
        .arg(input.path())
        .args(["--output-dir"])
        .arg(output.path())
        .status()
        .unwrap();
    assert!(status.success());
    for artifact in ARTIFACTS {
        assert!(output.path().join(artifact).is_file(), "missing {artifact}");
    }
    let result = report(&output.path().join("statistical-report.json"));
    assert_eq!(result["verdict"], "FIX486G_STATISTICAL_QUALITY_PASS");
    assert_eq!(result["sample_plan"]["raw_observation_count"], 730);
    assert_eq!(result["sample_plan"]["full_pass_counts"]["warm"], 3);
    assert_eq!(result["sample_plan"]["full_pass_counts"]["restart"], 2);
    assert_eq!(result["sample_plan"]["concurrent_pair_count"], 10);
    assert_eq!(
        result["metrics"]["GraphParentRecall@1"]["point_estimate"],
        1.0
    );
    assert_eq!(
        result["metrics"]["WarmNormalizedRepeatability"]["point_estimate"],
        1.0
    );
    assert_eq!(result["metrics"]["nDCG@3"]["outcome"], "REPORTED");
    assert_eq!(
        result["metrics"]["FalseSemanticNoAnswerRate"]["point_estimate"],
        0.0
    );
    assert_eq!(
        result["metrics"]["HealthyRequestContaminationRate"]["point_estimate"],
        0.0
    );
    let intervals = report(&output.path().join("confidence-intervals.json"));
    assert!(intervals["proportions"].as_array().unwrap().len() > 5);
    assert!(intervals["safety_failure_upper_bounds"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["failure_probability_upper"].as_f64().unwrap() > 0.0));
}

#[test]
fn statistical_evidence_rejects_source_sha_mismatch() {
    let input = tempfile::NamedTempFile::new().unwrap();
    let source_identity = tempfile::NamedTempFile::new().unwrap();
    let output = tempfile::tempdir().unwrap();
    let rows = synthetic_campaign();
    write_rows(input.path(), &rows);
    fs::write(
        source_identity.path(),
        serde_json::to_string(&json!({
            "branch": "agent/fix486g-current-sha-graph-parent-repair",
            "source_sha": "old-source-sha",
            "remote_branch_sha": "old-source-sha",
            "local_remote_equal": true
        }))
        .unwrap(),
    )
    .unwrap();

    let status = Command::new("python3")
        .args([SCRIPT, "evaluate", "--bank", BANK, "--raw-input"])
        .arg(input.path())
        .args(["--source-identity"])
        .arg(source_identity.path())
        .args([
            "--expected-source-sha",
            "current-source-sha",
            "--output-dir",
        ])
        .arg(output.path())
        .status()
        .unwrap();

    assert!(!status.success());
    let result = report(&output.path().join("statistical-report.json"));
    assert_eq!(result["verdict"], "FIX486G_STATISTICAL_QUALITY_BLOCKED");
    assert!(result["failure_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "SOURCE_SHA_MISMATCH: manifest source old-source-sha != expected tested current-source-sha"));
}

#[test]
fn direct_survivor_is_sufficient_for_faults_that_invalidate_the_graph_target() {
    let input = tempfile::NamedTempFile::new().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut rows = synthetic_campaign();
    for row in &mut rows {
        if row["degradation"]["graph_failure_classification"] == "WRONG_PARENT" {
            let key = if row["entry_point"] == "Search" {
                "results"
            } else {
                "contexts"
            };
            row["response"][key] = json!([direct_context("parent-a1")]);
        }
    }
    write_rows(input.path(), &rows);

    let status = Command::new("python3")
        .args([SCRIPT, "evaluate", "--bank", BANK, "--raw-input"])
        .arg(input.path())
        .args(["--output-dir"])
        .arg(output.path())
        .status()
        .unwrap();
    assert!(status.success());
    let result = report(&output.path().join("statistical-report.json"));
    assert_eq!(result["verdict"], "FIX486G_STATISTICAL_QUALITY_PASS");
    assert_eq!(result["hard_gates"]["valid_survivor_lost"], 0);
}

#[test]
fn multi_relation_provenance_uses_the_matching_relation_seed_and_accepts_verified_degradation() {
    let input = tempfile::NamedTempFile::new().unwrap();
    let identity_map = tempfile::NamedTempFile::new().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut rows = synthetic_campaign();
    for row in &mut rows {
        let key = if row["entry_point"] == "Search" {
            "results"
        } else {
            "contexts"
        };
        if let Some(contexts) = row["response"][key].as_array_mut() {
            for context in contexts {
                let metadata = &mut context["metadata"];
                if metadata["retrieval_source"] != "GRAPH_EXPANDED" {
                    continue;
                }
                metadata["graph_relation_type"] = json!("CHUNK_SEMANTIC_SIMILAR");
                metadata["graph_relation_score"] = json!("1.0");
                metadata["graph_seed_chunk_id"] = json!("child-a3-180");
                metadata["graph_seed_parent_chunk_id"] = json!("parent-a3");
                metadata["graph_relations"] = json!(serde_json::to_string(&json!([
                    {
                        "relation_type": "CHUNK_SEMANTIC_SIMILAR",
                        "relation_score": 1.0,
                        "seed_chunk_id": "child-a3-180",
                        "hop_distance": 1
                    },
                    {
                        "relation_type": "REPAIRED_BY",
                        "relation_score": 0.95,
                        "seed_chunk_id": "child-a1-180",
                        "hop_distance": 1
                    }
                ]))
                .unwrap());
            }
        }
        if row["entry_point"] == "RetrieveContext"
            && row["degradation"]["graph_failure_injected"] == true
        {
            row["response"]["status"] = json!("DEGRADED");
            row["response"]["degradation"] = json!({
                "degraded": true,
                "degradationClass": "PARTIAL"
            });
        }
    }
    write_rows(input.path(), &rows);
    write_identity_map(identity_map.path());

    let status = Command::new("python3")
        .args([SCRIPT, "evaluate", "--bank", BANK, "--raw-input"])
        .arg(input.path())
        .args(["--identity-map"])
        .arg(identity_map.path())
        .args(["--output-dir"])
        .arg(output.path())
        .status()
        .unwrap();
    assert!(status.success());
    let result = report(&output.path().join("statistical-report.json"));
    assert_eq!(result["verdict"], "FIX486G_STATISTICAL_QUALITY_PASS");
    assert_eq!(result["hard_gates"]["graph_seed_parent_reuse"], 0);
    assert_eq!(result["hard_gates"]["seed_parent_reuse_final_contexts"], 0);
}

#[test]
fn unresolved_matching_relation_seed_fails_closed() {
    let input = tempfile::NamedTempFile::new().unwrap();
    let identity_map = tempfile::NamedTempFile::new().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut rows = synthetic_campaign();
    let context = &mut rows[0]["response"]["results"][0];
    context["metadata"]["graph_relation_type"] = json!("CHUNK_SEMANTIC_SIMILAR");
    context["metadata"]["graph_seed_chunk_id"] = json!("child-a3-180");
    context["metadata"]["graph_seed_parent_chunk_id"] = json!("parent-a3");
    context["metadata"]["graph_relations"] = json!(serde_json::to_string(&json!([{
        "relation_type": "REPAIRED_BY",
        "relation_score": 0.95,
        "seed_chunk_id": "unknown-runtime-child",
        "hop_distance": 1
    }]))
    .unwrap());
    write_rows(input.path(), &rows);
    write_identity_map(identity_map.path());

    let status = Command::new("python3")
        .args([SCRIPT, "evaluate", "--bank", BANK, "--raw-input"])
        .arg(input.path())
        .args(["--identity-map"])
        .arg(identity_map.path())
        .args(["--output-dir"])
        .arg(output.path())
        .status()
        .unwrap();
    assert!(!status.success());
    let result = report(&output.path().join("statistical-report.json"));
    assert!(
        result["hard_gates"]["graph_provenance_missing"]
            .as_u64()
            .unwrap()
            > 0
    );
    let rows = jsonl(output.path().join("per-query-results.jsonl"));
    assert!(rows.iter().any(|row| {
        row["failure_codes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|code| code == "GRAPH_SEED_PARENT_IDENTITY_MISSING")
    }));
}

#[test]
fn synthetic_wrong_parent_blocks_and_preserves_fail_closed_artifacts() {
    let input = tempfile::NamedTempFile::new().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut rows = synthetic_campaign();
    let result_key = if rows[0]["entry_point"] == "Search" {
        "results"
    } else {
        "contexts"
    };
    rows[0]["response"][result_key][0]["parentLogicalId"] = json!("parent-wrong");
    write_rows(input.path(), &rows);

    let status = Command::new("python3")
        .args([SCRIPT, "evaluate", "--bank", BANK, "--raw-input"])
        .arg(input.path())
        .args(["--output-dir"])
        .arg(output.path())
        .status()
        .unwrap();
    assert!(!status.success());
    for artifact in ARTIFACTS {
        assert!(output.path().join(artifact).is_file(), "missing {artifact}");
    }
    let result = report(&output.path().join("statistical-report.json"));
    assert_eq!(result["verdict"], "FIX486G_STATISTICAL_QUALITY_BLOCKED");
    assert!(
        result["hard_gates"]["wrong_parent_graph_final_contexts"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(result["failure_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "SAFETY_HARD_GATE_FAILED"));
}
