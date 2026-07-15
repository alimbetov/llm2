use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const QUALITY_ROOT: &str = "benchmarks/quality";

#[derive(Default)]
struct QualityIndex {
    document_ids: HashSet<String>,
    block_ids: HashSet<String>,
    block_keys: HashSet<(String, String)>,
    access_zones: HashSet<String>,
    access_levels: HashSet<String>,
    documents: usize,
    blocks: usize,
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(idx, line)| {
            serde_json::from_str::<Value>(line).unwrap_or_else(|e| {
                panic!("invalid JSONL in {} line {}: {e}", path.display(), idx + 1)
            })
        })
        .collect()
}

fn required_str<'a>(v: &'a Value, key: &str, source: &Path) -> &'a str {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{} missing string field `{key}`", source.display()))
}

fn jsonl_files_under(dir: &Path, file_name: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in
        fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to list {}: {e}", dir.display()))
    {
        let path = entry.unwrap().path().join(file_name);
        if path.exists() {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn query_files() -> Vec<PathBuf> {
    let dir = Path::new(QUALITY_ROOT).join("queries");
    let mut files = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to list {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn profile_query_files(profile: &str) -> Vec<PathBuf> {
    let path = Path::new(QUALITY_ROOT).join(format!("profiles/{profile}.json"));
    let value: Value = serde_json::from_slice(
        &fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display())),
    )
    .unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()));
    value
        .get("queries")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} missing queries array", path.display()))
        .iter()
        .map(|name| {
            Path::new(QUALITY_ROOT).join(format!(
                "queries/{}.jsonl",
                name.as_str()
                    .unwrap_or_else(|| panic!("{} query entry is not a string", path.display()))
            ))
        })
        .collect()
}

fn document_files() -> Vec<PathBuf> {
    jsonl_files_under(&Path::new(QUALITY_ROOT).join("corpora"), "documents.jsonl")
}

fn relation_files() -> Vec<PathBuf> {
    jsonl_files_under(&Path::new(QUALITY_ROOT).join("corpora"), "relations.jsonl")
}

fn array_strings<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn build_index() -> QualityIndex {
    let mut index = QualityIndex::default();
    for file in document_files() {
        for doc in read_jsonl(&file) {
            assert_eq!(required_str(&doc, "schema_version", &file), "1.0");
            let document_id = required_str(&doc, "document_id", &file).to_string();
            assert!(
                index.document_ids.insert(document_id.clone()),
                "duplicate document_id: {document_id}"
            );
            assert!(!required_str(&doc, "title", &file).is_empty());
            assert!(!required_str(&doc, "corpus", &file).is_empty());
            index
                .access_zones
                .insert(required_str(&doc, "access_zone_code", &file).to_string());
            index
                .access_levels
                .insert(required_str(&doc, "access_level", &file).to_string());

            if required_str(&doc, "corpus", &file) == "legal-mini" {
                let metadata = doc.get("metadata").unwrap_or_else(|| {
                    panic!("legal-mini document {document_id} missing metadata")
                });
                assert_eq!(
                    metadata.get("synthetic").and_then(Value::as_bool),
                    Some(true),
                    "legal-mini document {document_id} must be synthetic"
                );
                assert_eq!(
                    metadata.get("not_real_law").and_then(Value::as_bool),
                    Some(true),
                    "legal-mini document {document_id} must be marked not_real_law"
                );
            }

            let blocks = doc
                .get("blocks")
                .and_then(Value::as_array)
                .unwrap_or_else(|| {
                    panic!(
                        "{} document {document_id} missing blocks array",
                        file.display()
                    )
                });
            assert!(!blocks.is_empty(), "document {document_id} has no blocks");
            let mut local_block_ids = HashSet::new();
            for block in blocks {
                let block_id = required_str(block, "block_id", &file).to_string();
                assert!(
                    local_block_ids.insert(block_id.clone()),
                    "duplicate block_id {block_id} inside document {document_id}"
                );
                assert!(
                    index.block_ids.insert(block_id.clone()),
                    "duplicate global block_id {block_id}"
                );
                assert!(
                    index
                        .block_keys
                        .insert((document_id.clone(), block_id.clone())),
                    "duplicate global block key {document_id}/{block_id}"
                );
                assert!(!required_str(block, "type", &file).is_empty());
                assert!(!required_str(block, "text", &file).is_empty());
                index.blocks += 1;
            }
            index.documents += 1;
        }
    }
    index
}

#[test]
fn enriched_quality_bench_structure_exists() {
    for path in [
        "benchmarks/quality/README.md",
        "benchmarks/quality/corpora/synthetic-mini/documents.jsonl",
        "benchmarks/quality/corpora/synthetic-mini/relations.jsonl",
        "benchmarks/quality/corpora/access-zone-mini/documents.jsonl",
        "benchmarks/quality/corpora/graph-rag-mini/documents.jsonl",
        "benchmarks/quality/corpora/graph-rag-mini/relations.jsonl",
        "benchmarks/quality/corpora/mmr-diversity-mini/documents.jsonl",
        "benchmarks/quality/corpora/technical-mini/documents.jsonl",
        "benchmarks/quality/corpora/technical-mini/relations.jsonl",
        "benchmarks/quality/corpora/legal-mini/documents.jsonl",
        "benchmarks/quality/corpora/legal-mini/relations.jsonl",
        "benchmarks/quality/corpora/distractor-mini/documents.jsonl",
        "benchmarks/quality/corpora/long-doc-mini/documents.jsonl",
        "benchmarks/quality/corpora/long-doc-mini/relations.jsonl",
        "benchmarks/quality/corpora/ttl-legal-hold-mini/documents.jsonl",
        "benchmarks/quality/corpora/ttl-legal-hold-mini/relations.jsonl",
        "benchmarks/quality/queries/technical-golden.jsonl",
        "benchmarks/quality/queries/legal-golden.jsonl",
        "benchmarks/quality/queries/distractor-golden.jsonl",
        "benchmarks/quality/queries/long-document-golden.jsonl",
        "benchmarks/quality/queries/ttl-legal-hold-golden.jsonl",
        "benchmarks/quality/profiles/quick.json",
        "benchmarks/quality/profiles/production-candidate.json",
        "benchmarks/quality/schemas/document.schema.json",
        "benchmarks/quality/schemas/relation.schema.json",
        "benchmarks/quality/schemas/query.schema.json",
        "benchmarks/quality/schemas/profile.schema.json",
        "benchmarks/quality/schemas/report.schema.json",
        "docs/QUALITY_BENCH.md",
    ] {
        assert!(
            Path::new(path).exists(),
            "missing quality bench path: {path}"
        );
    }
}

#[test]
fn enriched_quality_document_bank_has_required_scale_and_metadata() {
    let index = build_index();
    assert!(
        index.documents >= 35,
        "expected at least 35 documents, got {}",
        index.documents
    );
    assert!(
        index.blocks >= 140,
        "expected at least 140 blocks, got {}",
        index.blocks
    );
    assert!(
        index.access_zones.len() >= 3,
        "expected at least 3 access zones, got {:?}",
        index.access_zones
    );
    assert!(index.access_levels.contains("PUBLIC"));
    assert!(index.access_levels.contains("INTERNAL"));
    assert!(index.access_levels.contains("RESTRICTED"));
}

#[test]
fn enriched_quality_relations_reference_existing_documents_and_blocks() {
    let index = build_index();
    let mut relations = 0usize;
    let mut relation_types = HashSet::new();
    for file in relation_files() {
        for rel in read_jsonl(&file) {
            assert_eq!(required_str(&rel, "schema_version", &file), "1.0");
            let from_doc = required_str(&rel, "from_document_id", &file).to_string();
            let from_block = required_str(&rel, "from_block_id", &file).to_string();
            let to_doc = required_str(&rel, "to_document_id", &file).to_string();
            let to_block = required_str(&rel, "to_block_id", &file).to_string();
            assert!(
                index.document_ids.contains(&from_doc),
                "relation references missing from_document_id {from_doc}"
            );
            assert!(
                index.document_ids.contains(&to_doc),
                "relation references missing to_document_id {to_doc}"
            );
            assert!(
                index
                    .block_keys
                    .contains(&(from_doc.clone(), from_block.clone())),
                "relation references missing from block {from_doc}/{from_block}"
            );
            assert!(
                index
                    .block_keys
                    .contains(&(to_doc.clone(), to_block.clone())),
                "relation references missing to block {to_doc}/{to_block}"
            );
            relation_types.insert(required_str(&rel, "relation_type", &file).to_string());
            relations += 1;
        }
    }
    assert!(
        relations >= 20,
        "expected at least 20 relation edges, got {relations}"
    );
    assert!(
        relation_types.len() >= 5,
        "expected at least 5 relation types, got {relation_types:?}"
    );
}

#[test]
fn enriched_quality_queries_are_valid_and_reference_existing_ids() {
    let index = build_index();
    let mut query_count = 0usize;
    let mut categories: HashMap<String, usize> = HashMap::new();

    for file in query_files() {
        for query in read_jsonl(&file) {
            assert_eq!(required_str(&query, "schema_version", &file), "1.0");
            assert!(!required_str(&query, "id", &file).is_empty());
            assert!(!required_str(&query, "question", &file).is_empty());
            let category = required_str(&query, "category", &file).to_string();
            *categories.entry(category.clone()).or_insert(0) += 1;
            assert!(
                query.get("context").is_some(),
                "query missing context: {query:?}"
            );
            let expected = query
                .get("expected")
                .unwrap_or_else(|| panic!("query missing expected: {query:?}"));
            assert!(expected.is_object(), "expected must be object: {query:?}");

            for key in [
                "must_contain_document_ids",
                "forbidden_document_ids",
                "allowed_document_ids",
            ] {
                for doc in array_strings(expected, key) {
                    assert!(
                        index.document_ids.contains(doc),
                        "query {} references missing document {doc} in {key}",
                        required_str(&query, "id", &file)
                    );
                }
            }
            for key in [
                "must_contain_block_ids",
                "forbidden_block_ids",
                "expected_related_block_ids",
            ] {
                for block in array_strings(expected, key) {
                    assert!(
                        index.block_ids.contains(block),
                        "query {} references missing block {block} in {key}",
                        required_str(&query, "id", &file)
                    );
                }
            }
            for item in expected
                .get("required_ranked_before")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(doc) = item.get("document_id").and_then(Value::as_str) {
                    assert!(
                        index.document_ids.contains(doc),
                        "required_ranked_before references missing document {doc}"
                    );
                }
                if let Some(block) = item.get("block_id").and_then(Value::as_str) {
                    assert!(
                        index.block_ids.contains(block),
                        "required_ranked_before references missing block {block}"
                    );
                }
            }
            query_count += 1;
        }
    }

    assert!(
        query_count >= 100,
        "expected at least 100 golden queries, got {query_count}"
    );
    for required in [
        "exact_lookup",
        "paraphrase",
        "lexical_sparse",
        "hybrid",
        "graph_rag",
        "mmr_diversity",
        "access_isolation",
        "hard_negative",
        "ttl_legal_hold",
        "long_document",
    ] {
        assert!(
            categories.contains_key(required),
            "missing query category {required}; categories={categories:?}"
        );
    }
    assert!(
        *categories.get("hard_negative").unwrap_or(&0) >= 15,
        "expected at least 15 hard negative queries, got {:?}",
        categories.get("hard_negative")
    );
    assert!(
        *categories.get("access_isolation").unwrap_or(&0) >= 15,
        "expected at least 15 access isolation queries, got {:?}",
        categories.get("access_isolation")
    );
    assert!(
        *categories.get("graph_rag").unwrap_or(&0) >= 10,
        "expected at least 10 GraphRAG queries, got {:?}",
        categories.get("graph_rag")
    );
    assert!(
        *categories.get("mmr_diversity").unwrap_or(&0) >= 10,
        "expected at least 10 MMR queries, got {:?}",
        categories.get("mmr_diversity")
    );
    assert!(
        *categories.get("long_document").unwrap_or(&0) >= 10,
        "expected at least 10 long-document queries, got {:?}",
        categories.get("long_document")
    );
}

#[test]
fn enriched_quality_profiles_and_schemas_are_valid_json() {
    for path in [
        "benchmarks/quality/profiles/quick.json",
        "benchmarks/quality/profiles/production-candidate.json",
        "benchmarks/quality/schemas/document.schema.json",
        "benchmarks/quality/schemas/relation.schema.json",
        "benchmarks/quality/schemas/query.schema.json",
        "benchmarks/quality/schemas/profile.schema.json",
        "benchmarks/quality/schemas/report.schema.json",
    ] {
        let value: Value = serde_json::from_str(&fs::read_to_string(path).unwrap())
            .unwrap_or_else(|e| panic!("invalid JSON in {path}: {e}"));
        assert!(value.is_object(), "{path} must contain a JSON object");
    }

    let quick: Value = serde_json::from_str(
        &fs::read_to_string("benchmarks/quality/profiles/quick.json").unwrap(),
    )
    .unwrap();
    let quick_corpora: HashSet<_> = quick
        .get("corpora")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(quick_corpora.contains("technical-mini"));
    assert!(quick_corpora.contains("distractor-mini"));

    let pc: Value = serde_json::from_str(
        &fs::read_to_string("benchmarks/quality/profiles/production-candidate.json").unwrap(),
    )
    .unwrap();
    let pc_corpora: HashSet<_> = pc
        .get("corpora")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for required in [
        "technical-mini",
        "legal-mini",
        "distractor-mini",
        "long-doc-mini",
        "ttl-legal-hold-mini",
    ] {
        assert!(
            pc_corpora.contains(required),
            "production-candidate profile missing corpus {required}"
        );
    }
}

#[test]
fn fix480_quality_splits_are_disjoint_and_have_structural_qrel_coverage() {
    let mut all = HashSet::new();
    let mut counts = Vec::new();
    for split in ["tuning", "validation", "holdout"] {
        let queries =
            read_jsonl(&Path::new(QUALITY_ROOT).join(format!("queries/fix480-{split}.jsonl")));
        counts.push(queries.len());
        for query in queries {
            let id = query["id"].as_str().unwrap().to_string();
            assert!(
                all.insert(id),
                "query appears in more than one fix480 split"
            );
        }
    }
    assert_eq!(counts.iter().sum::<usize>(), 97);
    assert!((57..=60).contains(&counts[0]));
    assert!((18..=21).contains(&counts[1]));
    assert!((18..=21).contains(&counts[2]));
    let qrels = read_jsonl(&Path::new(QUALITY_ROOT).join("qrels/qrels.jsonl"));
    let qrel_ids = qrels
        .iter()
        .map(|value| value["query_id"].as_str().unwrap())
        .collect::<HashSet<_>>();
    assert_eq!(qrel_ids.len(), 97);
    assert!(all.iter().all(|id| qrel_ids.contains(id.as_str())));
}

#[test]
fn fix481_validation_pool_distinguishes_prepared_from_adjudicated_qrels() {
    let root = Path::new(QUALITY_ROOT).join("judgments");
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("manifests/validation.json"))
            .expect("fix481 validation judgment manifest"),
    )
    .expect("valid fix481 judgment manifest JSON");
    assert_eq!(manifest["requested_pool_depth"], 20);
    assert!(manifest["queries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|query| query["pool_source_count"].as_u64().unwrap_or(0) >= 4));

    let complete = manifest["qrels_complete"].as_bool().unwrap_or(false);
    let adjudicated = root.join("adjudicated/validation.jsonl");
    if complete {
        assert_eq!(manifest["status"], "ADJUDICATED");
        let rows = read_jsonl(&adjudicated);
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|row| {
            row["judgment_status"] == "ADJUDICATED"
                && row["relevance"].as_u64().is_some_and(|value| value <= 3)
        }));
    } else {
        assert_eq!(manifest["status"], "AWAITING_BLIND_JUDGMENT");
        assert!(!adjudicated.exists());
        assert!(manifest["unjudged_candidates_total"]
            .as_u64()
            .is_some_and(|value| value > 0));
    }
}

#[test]
fn fix482_rag_quality_bank_v1_loads_exactly_42_queries() {
    let files = profile_query_files("rag-quality-bank-v1");
    let expected_counts = HashMap::from([
        ("rag-quality-bank-v1-access.jsonl", 8usize),
        ("rag-quality-bank-v1-semantic.jsonl", 10),
        ("rag-quality-bank-v1-lexical.jsonl", 4),
        ("rag-quality-bank-v1-graph.jsonl", 3),
        ("rag-quality-bank-v1-mmr.jsonl", 3),
        ("rag-quality-bank-v1-long.jsonl", 4),
        ("rag-quality-bank-v1-distractor.jsonl", 4),
        ("rag-quality-bank-v1-negative.jsonl", 6),
    ]);
    assert_eq!(files.len(), expected_counts.len());

    let mut total = 0usize;
    let mut ids = HashSet::new();
    for file in files {
        let file_name = file.file_name().and_then(|name| name.to_str()).unwrap();
        let expected = expected_counts
            .get(file_name)
            .unwrap_or_else(|| panic!("unexpected rag-quality-bank-v1 query file {file_name}"));
        let rows = read_jsonl(&file);
        assert_eq!(
            rows.len(),
            *expected,
            "{} loaded unexpected query count",
            file.display()
        );
        for row in &rows {
            assert_eq!(required_str(row, "schema_version", &file), "1.0");
            assert!(
                ids.insert(required_str(row, "id", &file).to_string()),
                "duplicate query id in rag-quality-bank-v1"
            );
            assert!(!required_str(row, "question", &file).is_empty());
        }
        total += rows.len();
    }

    assert_eq!(total, 42, "rag-quality-bank-v1 must load 42/42 queries");
}

#[test]
fn fix482_rag_quality_bank_v1_queries_reference_existing_fixture_ids() {
    let index = build_index();
    let mut positive_queries = 0usize;
    let mut negative_queries = 0usize;

    for file in profile_query_files("rag-quality-bank-v1") {
        for query in read_jsonl(&file) {
            let query_id = required_str(&query, "id", &file);
            let context = query
                .get("context")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{query_id} missing context object"));
            for key in ["access_zone_code", "caller_access_level", "search_mode"] {
                assert!(
                    context.get(key).and_then(Value::as_str).is_some(),
                    "{query_id} missing context.{key}"
                );
            }
            let expected = query
                .get("expected")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{query_id} missing expected object"));
            for key in [
                "must_contain_document_ids",
                "forbidden_document_ids",
                "allowed_document_ids",
            ] {
                for doc in array_strings(&Value::Object(expected.clone()), key) {
                    assert!(
                        index.document_ids.contains(doc),
                        "{query_id} references missing document {doc} in {key}"
                    );
                }
            }
            for key in [
                "must_contain_block_ids",
                "expected_related_block_ids",
                "forbidden_block_ids",
            ] {
                for block in array_strings(&Value::Object(expected.clone()), key) {
                    assert!(
                        index.block_ids.contains(block),
                        "{query_id} references missing block {block} in {key}"
                    );
                }
            }
            let hard_negative = expected
                .get("hard_negative")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if hard_negative {
                negative_queries += 1;
                assert_eq!(
                    expected.get("expected_empty").and_then(Value::as_bool),
                    Some(true),
                    "{query_id} hard_negative must be expected_empty"
                );
            } else {
                positive_queries += 1;
                assert!(
                    !array_strings(&Value::Object(expected.clone()), "must_contain_block_ids")
                        .collect::<Vec<_>>()
                        .is_empty(),
                    "{query_id} positive query must name at least one expected block"
                );
            }
        }
    }

    assert_eq!(positive_queries, 36);
    assert_eq!(negative_queries, 6);
}

#[test]
fn fix482_rag_quality_bank_v1_blind_judgment_template_is_pending() {
    let root = Path::new(QUALITY_ROOT).join("judgments");
    let manifest_path = root.join("manifests/rag-quality-bank-v1.json");
    if !manifest_path.exists() {
        return;
    }

    let manifest: Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", manifest_path.display())),
    )
    .unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", manifest_path.display()));
    assert_eq!(manifest["profile"], "rag-quality-bank-v1");
    assert_eq!(manifest["queries_total"], 42);
    assert_eq!(manifest["status"], "AWAITING_BLIND_JUDGMENT");
    assert_eq!(manifest["qrels_complete"], false);
    assert_eq!(manifest["judged_candidates_total"], 0);
    assert!(manifest["unjudged_candidates_total"]
        .as_u64()
        .is_some_and(|value| value > 0));

    let blind_path = root.join("blind-judgments/rag-quality-bank-v1.jsonl");
    let blind_rows = read_jsonl(&blind_path);
    assert!(!blind_rows.is_empty());
    assert!(blind_rows.iter().all(|row| row["relevance"].is_null()));
    assert!(blind_rows.iter().all(|row| row.get("document_id").is_none()
        && row.get("source_block_id").is_none()
        && row.get("pool_reasons").is_none()));
}

#[test]
fn candidate_pool_does_not_read_expected_labels() {
    let root = Path::new(QUALITY_ROOT).join("judgments");
    let manifest_path = root.join("manifests/rag-quality-bank-v1.json");
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap())
        .expect("valid rag-quality-bank-v1 manifest");
    assert_eq!(
        manifest["candidate_selection"]["uses_expected_labels_for_selection"],
        false
    );

    let pool = read_jsonl(&root.join("candidate-pools/rag-quality-bank-v1.jsonl"));
    assert!(!pool.is_empty());
    for row in pool {
        assert!(row.get("pool_reasons").is_none());
        assert!(row.get("expected").is_none());
        assert!(row.get("relevance").is_none());
        assert!(row["pool_sources"].as_array().is_some_and(|sources| {
            !sources.is_empty()
                && sources.iter().all(|source| {
                    source.get("source").and_then(Value::as_str).is_some()
                        && source.get("rank").and_then(Value::as_u64).is_some()
                        && source.get("score").and_then(Value::as_f64).is_some()
                        && source.get("run_id").and_then(Value::as_str).is_some()
                })
        }));
    }
}

#[test]
fn candidate_pool_is_independent_of_structural_expectations() {
    let output = Command::new("python3")
        .arg("scripts/prepare_fix482_rag_quality_bank_judgments.py")
        .arg("--self-test")
        .output()
        .expect("run fix482 expectation-independence self-test");
    assert!(
        output.status.success(),
        "self-test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("self-test JSON");
    assert_eq!(
        report["candidate_pool_is_independent_of_structural_expectations"],
        true
    );
}

#[test]
fn candidate_pool_contains_at_least_four_sources() {
    let root = Path::new(QUALITY_ROOT).join("judgments");
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifests/rag-quality-bank-v1.json")).unwrap())
            .expect("valid rag-quality-bank-v1 manifest");
    assert_eq!(manifest["minimum_pool_source_count"], 4);
    for query in manifest["queries"].as_array().unwrap() {
        assert!(
            query["pool_source_count"].as_u64().unwrap_or(0) >= 4,
            "query {} has insufficient pool source count: {}",
            query["query_id"],
            query["pool_source_count"]
        );
    }
}

#[test]
fn candidate_pool_depth_is_at_least_twenty() {
    let root = Path::new(QUALITY_ROOT).join("judgments");
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifests/rag-quality-bank-v1.json")).unwrap())
            .expect("valid rag-quality-bank-v1 manifest");
    assert_eq!(manifest["pool_depth"], 20);
    for query in manifest["queries"].as_array().unwrap() {
        let count = query["candidate_count"].as_u64().unwrap_or(0);
        let exception = query["pool_depth_exception"].as_bool().unwrap_or(false);
        assert!(
            count >= 20 || exception,
            "query {} has candidate_count={} without exception",
            query["query_id"],
            count
        );
        if exception {
            assert_eq!(
                query["reason"],
                "ACCESS_FILTERED_CORPUS_SMALLER_THAN_POOL_DEPTH"
            );
        }
    }
}

#[test]
fn blind_template_hides_identity_rank_and_source() {
    let blind = read_jsonl(
        &Path::new(QUALITY_ROOT).join("judgments/blind-judgments/rag-quality-bank-v1.jsonl"),
    );
    assert!(!blind.is_empty());
    for row in blind {
        for hidden in [
            "document_id",
            "document_version",
            "source_block_id",
            "access_zone_id",
            "pool_sources",
            "source",
            "rank",
            "score",
            "expected",
            "expected_label",
        ] {
            assert!(
                row.get(hidden).is_none(),
                "blind row leaks hidden field {hidden}: {row:?}"
            );
        }
        assert!(row["relevance"].is_null());
    }
}

#[test]
fn pool_manifest_contains_runtime_identity() {
    let manifest: Value = serde_json::from_slice(
        &fs::read(Path::new(QUALITY_ROOT).join("judgments/manifests/rag-quality-bank-v1.json"))
            .unwrap(),
    )
    .expect("valid rag-quality-bank-v1 manifest");
    for field in [
        "git_sha",
        "runtime_binary_sha256",
        "effective_config_sha256",
        "model_sha256",
        "tokenizer_sha256",
        "corpus_sha256",
        "query_bank_sha256",
        "candidate_pool_sha256",
        "blind_template_sha256",
        "identity_map_sha256",
        "dense_run_id",
        "dense_result_sha256",
        "sparse_run_id",
        "sparse_result_sha256",
        "postgres_fts_run_id",
        "postgres_fts_result_sha256",
        "hybrid_run_id",
        "hybrid_result_sha256",
        "hybrid_graph_run_id",
        "hybrid_graph_result_sha256",
    ] {
        let value = manifest[field].as_str().unwrap_or("");
        assert!(
            !value.is_empty() && value != "MISSING" && value != "UNKNOWN",
            "manifest field {field} is incomplete: {value:?}"
        );
    }
}

#[test]
fn pool_generation_is_deterministic() {
    let output = Command::new("python3")
        .arg("scripts/prepare_fix482_rag_quality_bank_judgments.py")
        .arg("--self-test")
        .output()
        .expect("run fix482 deterministic self-test");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("self-test JSON");
    assert_eq!(
        report["original_candidate_pool_total"],
        report["mutated_candidate_pool_total"]
    );
}

#[test]
fn pool_generation_respects_access_filters() {
    let pool = read_jsonl(
        &Path::new(QUALITY_ROOT).join("judgments/candidate-pools/rag-quality-bank-v1.jsonl"),
    );
    let queries = profile_query_files("rag-quality-bank-v1")
        .into_iter()
        .flat_map(|file| read_jsonl(&file))
        .map(|query| {
            let context = query["context"].as_object().unwrap();
            (
                query["id"].as_str().unwrap().to_string(),
                (
                    context["access_zone_code"].as_str().unwrap().to_string(),
                    context["caller_access_level"].as_str().unwrap().to_string(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let order = HashMap::from([("PUBLIC", 0u8), ("INTERNAL", 1), ("RESTRICTED", 2)]);
    for row in pool {
        let (zone, caller_level) = queries
            .get(row["query_id"].as_str().unwrap())
            .expect("candidate query exists");
        assert_eq!(row["access_zone_id"].as_str().unwrap(), zone);
        assert!(
            order[row["access_level"].as_str().unwrap()] <= order[caller_level.as_str()],
            "candidate violates access filter: {row:?}"
        );
        assert_eq!(row["lifecycle_status"], "ACTIVE");
    }
}
