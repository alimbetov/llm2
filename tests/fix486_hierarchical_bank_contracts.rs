use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn bank_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks/hierarchical/fix486")
}

fn read_json(path: &Path) -> Value {
    let raw =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    let raw =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line).unwrap_or_else(|error| {
                panic!("parse {} line {}: {error}", path.display(), index + 1)
            })
        })
        .collect()
}

fn required_string<'a>(value: &'a Value, field: &str) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("required non-empty string field {field}: {value}"))
}

#[test]
fn fix486_seed_bank_has_complete_query_qrel_structure() {
    let root = bank_root();
    let manifest = read_json(&root.join("bank-manifest.json"));
    assert_eq!(manifest["bank_id"], "fix486-hierarchical-bank");
    assert_eq!(manifest["bank_version"], "1.0.0");
    assert_eq!(manifest["status"], "FROZEN");

    let queries = read_jsonl(&root.join(required_string(&manifest["files"], "queries")));
    let qrels = read_jsonl(&root.join(required_string(&manifest["files"], "qrels")));
    assert_eq!(
        queries.len(),
        11,
        "10 cases include two zone-isolation queries"
    );
    assert_eq!(qrels.len(), queries.len());

    let mut query_by_id = BTreeMap::new();
    let mut covered_cases = BTreeSet::new();
    for query in &queries {
        let query_id = required_string(query, "query_id");
        let case_id = required_string(query, "case_id");
        assert!(
            query_by_id.insert(query_id, query).is_none(),
            "duplicate query_id {query_id}"
        );
        assert!(case_id.starts_with("FIX486-"));
        required_string(query, "access_zone");
        required_string(query, "question");
        required_string(query, "profile");
        assert!(query["max_contexts"]
            .as_u64()
            .is_some_and(|value| value > 0));
        assert!(query["required_intents"]
            .as_array()
            .is_some_and(|values| !values.is_empty()));
        covered_cases.insert(case_id);
    }

    let expected_cases = (1..=10)
        .map(|number| format!("FIX486-{number:02}"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        covered_cases,
        expected_cases.iter().map(String::as_str).collect()
    );

    let mut qrel_ids = BTreeSet::new();
    for qrel in &qrels {
        let query_id = required_string(qrel, "query_id");
        let query = query_by_id
            .get(query_id)
            .unwrap_or_else(|| panic!("qrel without query {query_id}"));
        assert!(qrel_ids.insert(query_id), "duplicate qrel {query_id}");
        assert_eq!(required_string(qrel, "case_id"), query["case_id"]);
        assert_eq!(required_string(qrel, "expected_zone"), query["access_zone"]);
        assert!(qrel.get("expected_status").is_some() || qrel.get("expected_status_any").is_some());
    }
    assert_eq!(qrel_ids, query_by_id.keys().copied().collect());
}

#[test]
fn fix486_seed_bank_logical_identities_are_resolvable_and_zone_scoped() {
    let root = bank_root();
    let corpus = read_json(&root.join("corpus/hierarchical-fixture-v1.json"));
    let qrels = read_jsonl(&root.join("qrels/hierarchical-qrels-v1.jsonl"));

    let mut parents_by_zone: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut children_by_zone: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for zone in corpus["zones"].as_array().expect("zones array") {
        let zone_id = required_string(zone, "logical_zone_id");
        let parents = parents_by_zone.entry(zone_id).or_default();
        let children = children_by_zone.entry(zone_id).or_default();
        for document in zone["documents"].as_array().expect("documents array") {
            for version in document["versions"].as_array().expect("versions array") {
                for block in version["blocks"].as_array().expect("blocks array") {
                    if let Some(hierarchy) = block.get("expected_hierarchy") {
                        parents.insert(required_string(hierarchy, "logical_parent_id"));
                        for child in hierarchy["children"].as_array().expect("children array") {
                            children.insert(required_string(child, "logical_child_id"));
                            assert!(matches!(
                                required_string(child, "granularity"),
                                "SUB_180" | "SUB_260"
                            ));
                        }
                    }
                }
            }
        }
    }

    assert!(parents_by_zone["zone-a"].contains("parent-a1"));
    assert!(parents_by_zone["zone-b"].contains("parent-a1"));
    assert!(children_by_zone["zone-a"].contains("child-a1-180"));
    assert!(children_by_zone["zone-b"].contains("child-a1-180"));

    for qrel in &qrels {
        let zone = required_string(qrel, "expected_zone");
        assert!(
            parents_by_zone.contains_key(zone),
            "unknown qrel zone {zone}"
        );
        if let Some(parent) = qrel.get("expected_parent").and_then(Value::as_str) {
            assert!(
                parents_by_zone[zone].contains(parent),
                "qrel parent {parent} is absent in {zone}"
            );
        }
        if let Some(children) = qrel.get("expected_child_any").and_then(Value::as_array) {
            for child in children {
                let child = child.as_str().expect("expected_child_any string");
                assert!(
                    children_by_zone[zone].contains(child),
                    "qrel child {child} is absent in {zone}"
                );
            }
        }
    }
}

#[test]
fn fix486_failure_and_graph_fixtures_cover_declared_cases() {
    let root = bank_root();
    let graph = read_json(&root.join("graph-relations/hierarchical-graph-v1.json"));
    let lifecycle = read_json(&root.join("lifecycle/hierarchical-lifecycle-v1.json"));

    assert!(graph["relations"].as_array().is_some_and(|relations| {
        relations.iter().any(|relation| {
            relation["from_logical_child"] == "child-a1-180"
                && relation["to_logical_child"] == "child-a3-180"
                && matches!(
                    relation["relation_type"].as_str(),
                    Some("REPAIRED_BY" | "RELATED_TO")
                )
        })
    }));

    let lifecycle_scenarios = lifecycle["scenarios"]
        .as_array()
        .expect("lifecycle scenarios")
        .iter()
        .map(|scenario| required_string(scenario, "scenario_id"))
        .collect::<BTreeSet<_>>();
    for required in [
        "inactive-v2-higher-score",
        "deleted-v3-stale-qdrant-child",
        "parent-hydration-timeout-partial",
        "parent-hydration-timeout-total",
    ] {
        assert!(
            lifecycle_scenarios.contains(required),
            "missing lifecycle scenario {required}"
        );
    }
}
