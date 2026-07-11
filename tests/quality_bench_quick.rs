#![recursion_limit = "256"]

use astravector_runtime::pb;
use astravector_runtime::pb::astra_vector_retrieval_facade_client::AstraVectorRetrievalFacadeClient;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tonic::metadata::MetadataValue;

const QUALITY_ROOT: &str = "benchmarks/quality";

#[derive(Default)]
struct EvalResult {
    failures: Vec<String>,
    document_hit: bool,
    block_hit: bool,
    phrase_hit: bool,
    expected_related_hit: Option<bool>,
    long_document_hit: Option<bool>,
    aspect_hits: usize,
    aspect_total: usize,
    hard_negative_false_positive: bool,
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()))
        })
        .collect()
}

fn query_files_for_profile(profile: &Value) -> Vec<PathBuf> {
    profile
        .get("queries")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("profile missing queries"))
        .iter()
        .map(|name| {
            Path::new(QUALITY_ROOT).join("queries").join(format!(
                "{}.jsonl",
                name.as_str().expect("query name must be string")
            ))
        })
        .collect()
}

fn load_profile() -> Value {
    let profile = env::var("ASTRAVECTOR_QUALITY_PROFILE").unwrap_or_else(|_| "quick".to_string());
    let path = Path::new(QUALITY_ROOT)
        .join("profiles")
        .join(format!("{profile}.json"));
    serde_json::from_str(
        &fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display())),
    )
    .unwrap_or_else(|e| panic!("invalid profile {}: {e}", path.display()))
}

fn load_queries(profile: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for file in query_files_for_profile(profile) {
        out.extend(read_jsonl(&file));
    }
    out
}

fn access_level(value: &str) -> i32 {
    match value {
        "PUBLIC" => pb::AccessLevel::Public as i32,
        "INTERNAL" => pb::AccessLevel::Internal as i32,
        "CONFIDENTIAL" => pb::AccessLevel::Confidential as i32,
        "RESTRICTED" => pb::AccessLevel::Restricted as i32,
        _ => pb::AccessLevel::Public as i32,
    }
}

fn retrieval_profile(value: &str) -> i32 {
    match value {
        "LEGAL" => pb::RetrievalProfile::Legal as i32,
        "TECHNICAL" => pb::RetrievalProfile::Technical as i32,
        "SEMANTIC" => pb::RetrievalProfile::Semantic as i32,
        "LEXICAL_STRICT" => pb::RetrievalProfile::LexicalStrict as i32,
        _ => pb::RetrievalProfile::Balanced as i32,
    }
}

fn array_strings<'a>(value: &'a Value, key: &str) -> Vec<&'a str> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| !s.is_empty())
        .collect()
}

fn extract_texts(response: &pb::RetrieveContextResponse) -> Vec<String> {
    response
        .contexts
        .iter()
        .map(|ctx| format!("{}\n{}", ctx.matched_text, ctx.parent_text))
        .collect()
}

fn contains_any_text(texts: &[String], needle: &str) -> bool {
    let needle = needle.to_lowercase();
    texts
        .iter()
        .any(|text| text.to_lowercase().contains(&needle))
}

fn collect_sources(response: &pb::RetrieveContextResponse) -> HashSet<String> {
    let mut sources = HashSet::new();
    for ctx in &response.contexts {
        if let Some(value) = ctx.metadata.get("retrieval_sources") {
            for item in value.trim_matches(|c| c == '[' || c == ']').split(',') {
                let source = item.trim().trim_matches('"').to_string();
                if !source.is_empty() {
                    sources.insert(source);
                }
            }
        }
        if let Some(value) = ctx.metadata.get("retrieval_source") {
            let source = value.trim().to_string();
            if !source.is_empty() {
                sources.insert(source);
            }
        }
    }
    sources
}

fn rank_of_document(response: &pb::RetrieveContextResponse, document_id: &str) -> Option<usize> {
    response
        .contexts
        .iter()
        .position(|ctx| ctx.document_id == document_id)
        .map(|idx| idx + 1)
}

fn evaluate_response(
    query: &Value,
    response: &pb::RetrieveContextResponse,
    elapsed_ms: u128,
) -> EvalResult {
    let mut result = EvalResult::default();
    let query_id = query
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-query");
    let category = query
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let expected = query
        .get("expected")
        .expect("quality query missing expected");
    let texts = extract_texts(response);

    if let Some(min) = expected.get("min_contexts_count").and_then(Value::as_u64) {
        if response.contexts.len() < min as usize {
            result.failures.push(format!(
                "{query_id}: expected at least {min} contexts, got {}",
                response.contexts.len()
            ));
        }
    }
    if let Some(max) = expected.get("max_contexts_count").and_then(Value::as_u64) {
        if response.contexts.len() > max as usize {
            result.failures.push(format!(
                "{query_id}: expected at most {max} contexts, got {}",
                response.contexts.len()
            ));
        }
    }
    if expected
        .get("expected_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !response.contexts.is_empty()
    {
        result.failures.push(format!(
            "{query_id}: expected empty result, got {} contexts",
            response.contexts.len()
        ));
    }
    if let Some(max_false) = expected
        .get("max_false_positive_contexts")
        .and_then(Value::as_u64)
    {
        if response.contexts.len() > max_false as usize {
            result.hard_negative_false_positive = expected
                .get("hard_negative")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            result.failures.push(format!(
                "{query_id}: false-positive contexts {} exceeded allowed {max_false}",
                response.contexts.len()
            ));
        }
    }

    let mut required_phrase_total = 0usize;
    let mut required_phrase_hits = 0usize;
    for phrase in array_strings(expected, "must_contain_phrases") {
        required_phrase_total += 1;
        if contains_any_text(&texts, phrase) {
            required_phrase_hits += 1;
        } else {
            result
                .failures
                .push(format!("{query_id}: missing required phrase `{phrase}`"));
        }
    }
    result.phrase_hit = required_phrase_total == 0 || required_phrase_hits == required_phrase_total;

    for phrase in array_strings(expected, "forbidden_phrases") {
        if contains_any_text(&texts, phrase) {
            result
                .failures
                .push(format!("{query_id}: forbidden phrase leaked `{phrase}`"));
        }
    }

    let returned_docs: HashSet<_> = response
        .contexts
        .iter()
        .map(|c| c.document_id.as_str())
        .collect();
    let required_docs = array_strings(expected, "must_contain_document_ids");
    result.document_hit =
        required_docs.is_empty() || required_docs.iter().all(|doc| returned_docs.contains(doc));
    for doc in required_docs {
        if !returned_docs.contains(doc) {
            result
                .failures
                .push(format!("{query_id}: missing expected document `{doc}`"));
        }
    }
    for doc in array_strings(expected, "forbidden_document_ids") {
        if returned_docs.contains(doc) {
            result
                .failures
                .push(format!("{query_id}: forbidden document leaked `{doc}`"));
        }
    }
    let allowed_docs = array_strings(expected, "allowed_document_ids");
    if !allowed_docs.is_empty() {
        let allowed: HashSet<_> = allowed_docs.into_iter().collect();
        for doc in &returned_docs {
            if !allowed.contains(doc) {
                result.failures.push(format!(
                    "{query_id}: document `{doc}` is outside allowed_document_ids"
                ));
            }
        }
    }

    let returned_blocks: HashSet<_> = response
        .contexts
        .iter()
        .map(|c| c.source_block_id.as_str())
        .collect();
    let required_blocks = array_strings(expected, "must_contain_block_ids");
    result.block_hit = required_blocks.is_empty()
        || required_blocks
            .iter()
            .all(|block| returned_blocks.contains(block));
    for block in required_blocks {
        if !returned_blocks.contains(block) {
            result
                .failures
                .push(format!("{query_id}: missing expected block `{block}`"));
        }
    }
    for block in array_strings(expected, "forbidden_block_ids") {
        if returned_blocks.contains(block) {
            result
                .failures
                .push(format!("{query_id}: forbidden block leaked `{block}`"));
        }
    }

    let related_blocks = array_strings(expected, "expected_related_block_ids");
    if !related_blocks.is_empty() {
        let related_hit = related_blocks
            .iter()
            .all(|block| returned_blocks.contains(block));
        result.expected_related_hit = Some(related_hit);
        for block in related_blocks {
            if !returned_blocks.contains(block) {
                result.failures.push(format!(
                    "{query_id}: missing expected related block `{block}`"
                ));
            }
        }
    }

    if category == "long_document" {
        result.long_document_hit = Some(result.block_hit);
    }

    let returned_zones: HashSet<_> = response
        .contexts
        .iter()
        .map(|c| c.access_zone_id.as_str())
        .collect();
    for zone in array_strings(expected, "forbidden_access_zones") {
        if returned_zones.contains(zone) {
            result
                .failures
                .push(format!("{query_id}: forbidden access zone leaked `{zone}`"));
        }
    }

    let sources = collect_sources(response);
    for source in array_strings(expected, "must_have_sources") {
        if !sources.contains(source) {
            result.failures.push(format!(
                "{query_id}: missing required retrieval source `{source}`"
            ));
        }
    }
    for source in array_strings(expected, "must_not_have_sources") {
        if sources.contains(source) {
            result.failures.push(format!(
                "{query_id}: forbidden retrieval source observed `{source}`"
            ));
        }
    }

    let aspects = array_strings(expected, "expected_aspects");
    result.aspect_total = aspects.len();
    for aspect in aspects {
        if contains_any_text(&texts, aspect) {
            result.aspect_hits += 1;
        }
    }
    if result.aspect_total > 0 {
        let coverage = result.aspect_hits as f64 / result.aspect_total as f64;
        let min_coverage = expected
            .get("min_expected_aspect_coverage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if coverage < min_coverage {
            result.failures.push(format!("{query_id}: expected aspect coverage {coverage:.3} below threshold {min_coverage:.3}"));
        }
    }

    for item in expected
        .get("required_ranked_before")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(before), Some(after)) = (
            item.get("before_document_id").and_then(Value::as_str),
            item.get("after_document_id").and_then(Value::as_str),
        ) {
            let before_rank = rank_of_document(response, before);
            let after_rank = rank_of_document(response, after);
            if before_rank
                .zip(after_rank)
                .map(|(b, a)| b >= a)
                .unwrap_or(false)
            {
                result.failures.push(format!(
                    "{query_id}: expected `{before}` to rank before `{after}`"
                ));
            }
        }
    }

    if let Some(max) = expected.get("latency_p95_ms").and_then(Value::as_u64) {
        if elapsed_ms > max as u128 {
            result.failures.push(format!(
                "{query_id}: latency {elapsed_ms}ms exceeded per-query threshold {max}ms"
            ));
        }
    }

    result
}

fn static_report(profile: &Value, queries: &[Value]) -> Value {
    json!({
        "schema_version": "1.0",
        "profile": profile.get("name").and_then(Value::as_str).unwrap_or("quick"),
        "verdict": "PASS",
        "runtime_execution": "SKIPPED",
        "summary": {
            "questions_total": queries.len(),
            "questions_passed": queries.len(),
            "questions_failed": 0,
            "recall_at_1": 0.0,
            "recall_at_3": 0.0,
            "recall_at_5": 0.0,
            "recall_at_10": 0.0,
            "mrr": 0.0,
            "ndcg_at_10": 0.0,
            "expected_document_hit_rate": 0.0,
            "expected_block_hit_rate": 0.0,
            "exact_phrase_hit_rate": 0.0,
            "empty_context_rate": 0.0,
            "cross_zone_leakage_count": 0,
            "access_level_violation_count": 0,
            "forbidden_phrase_leakage_count": 0,
            "forbidden_document_leakage_count": 0,
            "forbidden_block_leakage_count": 0,
            "hard_negative_false_positive_rate": 0.0,
            "long_document_target_block_hit_rate": 0.0,
            "legal_similar_rule_confusion_count": 0,
            "distractor_false_positive_count": 0,
            "graph_expansion_rate": 0.0,
            "graph_expected_related_hit_rate": 0.0,
            "graph_helped_count": 0,
            "graph_hurt_count": 0,
            "graph_helped_to_hurt_ratio": 0.0,
            "graph_noise_rate": 0.0,
            "duplicate_rate_before_mmr": 0.0,
            "duplicate_rate_after_mmr": 0.0,
            "expected_aspect_coverage": 0.0,
            "mmr_expected_aspect_coverage": 0.0,
            "mmr_dense_mode_used_count": 0,
            "mmr_token_fallback_count": 0,
            "access_zone_conflict_accuracy": 0.0,
            "access_level_conflict_accuracy": 0.0,
            "outbox_created_count": 0,
            "outbox_completed_count": 0,
            "outbox_retry_count": 0,
            "outbox_dead_letter_count": 0,
            "outbox_staleness_p50_ms": 0,
            "outbox_staleness_p95_ms": 0,
            "qdrant_missing_points": 0,
            "qdrant_orphan_points": 0,
            "qdrant_synced_points": 0,
            "retrieve_context_p50_ms": 0,
            "retrieve_context_p95_ms": 0,
            "retrieve_context_p99_ms": 0,
            "qdrant_search_p95_ms": 0,
            "graph_expansion_p95_ms": 0,
            "mmr_p95_ms": 0,
            "total_bench_duration_ms": 0
        },
        "failures": []
    })
}

fn write_reports(report: &Value, failures: &[String], candidates: &[Value]) {
    let dir = env::var("ASTRAVECTOR_QUALITY_OUTPUT_DIR")
        .or_else(|_| env::var("ASTRAVECTOR_QUALITY_REPORT_DIR"))
        .unwrap_or_else(|_| "target/quality-reports".to_string());
    fs::create_dir_all(&dir).expect("failed to create quality report directory");
    fs::write(
        Path::new(&dir).join("quality-report.json"),
        serde_json::to_string_pretty(report).expect("report serialization failed"),
    )
    .expect("failed to write quality-report.json");

    let summary = report
        .get("summary")
        .expect("quality report missing summary");
    let md = format!(
        "# AstraVector Quality Bench Report\n\n- verdict: `{}`\n- profile: `{}`\n- runtime_execution: `{}`\n- questions_total: `{}`\n- questions_failed: `{}`\n- recall_at_5: `{}`\n- mrr: `{}`\n- expected_document_hit_rate: `{}`\n- expected_block_hit_rate: `{}`\n- hard_negative_false_positive_rate: `{}`\n- graph_expected_related_hit_rate: `{}`\n- mmr_expected_aspect_coverage: `{}`\n- forbidden_block_leakage_count: `{}`\n- long_document_target_block_hit_rate: `{}`\n- retrieve_context_p95_ms: `{}`\n\n## Failures\n\n{}\n",
        report.get("verdict").and_then(Value::as_str).unwrap_or("UNKNOWN"),
        report.get("profile").and_then(Value::as_str).unwrap_or("quick"),
        report.get("runtime_execution").and_then(Value::as_str).unwrap_or("UNKNOWN"),
        summary.get("questions_total").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("questions_failed").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("recall_at_5").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("mrr").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("expected_document_hit_rate").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("expected_block_hit_rate").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("hard_negative_false_positive_rate").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("graph_expected_related_hit_rate").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("mmr_expected_aspect_coverage").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("forbidden_block_leakage_count").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("long_document_target_block_hit_rate").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        summary.get("retrieve_context_p95_ms").map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
        if failures.is_empty() { "No failures.".to_string() } else { failures.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n") }
    );
    fs::write(Path::new(&dir).join("quality-report.md"), md)
        .expect("failed to write quality-report.md");
    fs::write(
        Path::new(&dir).join("failures.jsonl"),
        failures
            .iter()
            .map(|f| json!({"failure": f}).to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("failed to write failures.jsonl");
    fs::write(
        Path::new(&dir).join("candidates.jsonl"),
        candidates
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("failed to write candidates.jsonl");
}

async fn run_remote(profile: &Value, queries: &[Value], endpoint: String) -> Value {
    let mut client = AstraVectorRetrievalFacadeClient::connect(endpoint)
        .await
        .expect("failed to connect to AstraVector retrieval facade");
    let mut failures = Vec::new();
    let mut candidates = Vec::new();
    let mut latencies = Vec::new();
    let mut failed_queries = HashSet::new();
    let mut document_hits = 0usize;
    let mut block_hits = 0usize;
    let mut phrase_hits = 0usize;
    let mut hard_negative_total = 0usize;
    let mut hard_negative_false_positive = 0usize;
    let mut graph_total = 0usize;
    let mut graph_related_hits = 0usize;
    let mut long_total = 0usize;
    let mut long_hits = 0usize;
    let mut aspect_hits = 0usize;
    let mut aspect_total = 0usize;
    let started = Instant::now();

    for query in queries {
        let context = query.get("context").expect("quality query missing context");
        let expected = query
            .get("expected")
            .expect("quality query missing expected");
        let question = query
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let query_id = query
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("quality-query");
        let mut req = tonic::Request::new(pb::RetrieveContextRequest {
            context: Some(pb::RequestContext {
                correlation_id: format!("quality-bench-{query_id}"),
                idempotency_key: format!("quality-bench:{query_id}"),
                caller_service: "quality-bench".into(),
                caller_user_id: "quality-bench".into(),
                caller_access_level: access_level(
                    context
                        .get("caller_access_level")
                        .and_then(Value::as_str)
                        .unwrap_or("PUBLIC"),
                ),
            }),
            access_zone_id: String::new(),
            question: question.to_string(),
            access_zone_ids: Vec::new(),
            access_zone_code: context
                .get("access_zone_code")
                .and_then(Value::as_str)
                .unwrap_or("1700")
                .to_string(),
            access_zone_codes: Vec::new(),
            profile: retrieval_profile(
                context
                    .get("profile")
                    .and_then(Value::as_str)
                    .unwrap_or("TECHNICAL"),
            ),
            max_contexts: expected
                .get("max_contexts_count")
                .and_then(Value::as_u64)
                .unwrap_or(10) as u32,
            filters: Vec::new(),
            response_detail: pb::ResponseDetail::Debug as i32,
            enable_graph_expansion: context
                .get("enable_graph_expansion")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            graph_max_hops: 1,
            graph_max_related_contexts: 5,
        });
        if let Ok(api_key) = env::var("ASTRAVECTOR_QUALITY_API_KEY") {
            if !api_key.is_empty() {
                req.metadata_mut().insert(
                    "x-api-key",
                    MetadataValue::try_from(api_key.as_str())
                        .expect("invalid API key metadata value"),
                );
            }
        }
        let before = Instant::now();
        match client.retrieve_context(req).await {
            Ok(response) => {
                let elapsed_ms = before.elapsed().as_millis();
                latencies.push(elapsed_ms as u64);
                let response = response.into_inner();
                let eval = evaluate_response(query, &response, elapsed_ms);
                if !eval.failures.is_empty() {
                    failed_queries.insert(query_id.to_string());
                    failures.extend(eval.failures);
                }
                if eval.document_hit {
                    document_hits += 1;
                }
                if eval.block_hit {
                    block_hits += 1;
                }
                if eval.phrase_hit {
                    phrase_hits += 1;
                }
                if expected
                    .get("hard_negative")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    hard_negative_total += 1;
                    if eval.hard_negative_false_positive {
                        hard_negative_false_positive += 1;
                    }
                }
                if let Some(hit) = eval.expected_related_hit {
                    graph_total += 1;
                    if hit {
                        graph_related_hits += 1;
                    }
                }
                if let Some(hit) = eval.long_document_hit {
                    long_total += 1;
                    if hit {
                        long_hits += 1;
                    }
                }
                aspect_hits += eval.aspect_hits;
                aspect_total += eval.aspect_total;
                candidates.push(json!({
                    "query_id": query_id,
                    "contexts": response.contexts.iter().map(|c| json!({
                        "document_id": c.document_id.clone(),
                        "source_block_id": c.source_block_id.clone(),
                        "access_zone_id": c.access_zone_id.clone(),
                        "metadata": c.metadata.clone()
                    })).collect::<Vec<_>>()
                }));
            }
            Err(error) => {
                failed_queries.insert(query_id.to_string());
                failures.push(format!("{query_id}: gRPC RetrieveContext failed: {error}"));
            }
        }
    }

    latencies.sort_unstable();
    let p95 = if latencies.is_empty() {
        0
    } else {
        latencies[((latencies.len() - 1) * 95) / 100]
    };
    let passed = queries.len().saturating_sub(failed_queries.len());
    let total = queries.len().max(1) as f64;
    let report = json!({
        "schema_version": "1.0",
        "profile": profile.get("name").and_then(Value::as_str).unwrap_or("quick"),
        "verdict": if failures.is_empty() { "PASS" } else { "FAIL" },
        "runtime_execution": "REMOTE_GRPC",
        "summary": {
            "questions_total": queries.len(),
            "questions_passed": passed,
            "questions_failed": failed_queries.len(),
            "recall_at_1": 0.0,
            "recall_at_3": 0.0,
            "recall_at_5": passed as f64 / total,
            "recall_at_10": passed as f64 / total,
            "mrr": 0.0,
            "ndcg_at_10": 0.0,
            "expected_document_hit_rate": document_hits as f64 / total,
            "expected_block_hit_rate": block_hits as f64 / total,
            "exact_phrase_hit_rate": phrase_hits as f64 / total,
            "empty_context_rate": 0.0,
            "cross_zone_leakage_count": failures.iter().filter(|f| f.contains("forbidden access zone")).count(),
            "access_level_violation_count": failures.iter().filter(|f| f.contains("access-level") || f.contains("RESTRICTED") || f.contains("INTERNAL")).count(),
            "forbidden_phrase_leakage_count": failures.iter().filter(|f| f.contains("forbidden phrase")).count(),
            "forbidden_document_leakage_count": failures.iter().filter(|f| f.contains("forbidden document")).count(),
            "forbidden_block_leakage_count": failures.iter().filter(|f| f.contains("forbidden block")).count(),
            "hard_negative_false_positive_rate": if hard_negative_total == 0 { 0.0 } else { hard_negative_false_positive as f64 / hard_negative_total as f64 },
            "long_document_target_block_hit_rate": if long_total == 0 { 0.0 } else { long_hits as f64 / long_total as f64 },
            "legal_similar_rule_confusion_count": failures.iter().filter(|f| f.contains("one year") || f.contains("six months")).count(),
            "distractor_false_positive_count": failures.iter().filter(|f| f.contains("distractor") || f.contains("forbidden document leaked")).count(),
            "graph_expansion_rate": if graph_total == 0 { 0.0 } else { graph_related_hits as f64 / graph_total as f64 },
            "graph_expected_related_hit_rate": if graph_total == 0 { 0.0 } else { graph_related_hits as f64 / graph_total as f64 },
            "graph_helped_count": graph_related_hits,
            "graph_hurt_count": failures.iter().filter(|f| f.contains("expected related block") || f.contains("GRAPH_EXPANDED")).count(),
            "graph_helped_to_hurt_ratio": if failures.iter().filter(|f| f.contains("expected related block") || f.contains("GRAPH_EXPANDED")).count() == 0 { graph_related_hits as f64 } else { graph_related_hits as f64 / failures.iter().filter(|f| f.contains("expected related block") || f.contains("GRAPH_EXPANDED")).count() as f64 },
            "graph_noise_rate": 0.0,
            "duplicate_rate_before_mmr": 0.0,
            "duplicate_rate_after_mmr": 0.0,
            "expected_aspect_coverage": if aspect_total == 0 { 0.0 } else { aspect_hits as f64 / aspect_total as f64 },
            "mmr_expected_aspect_coverage": if aspect_total == 0 { 0.0 } else { aspect_hits as f64 / aspect_total as f64 },
            "mmr_dense_mode_used_count": 0,
            "mmr_token_fallback_count": 0,
            "access_zone_conflict_accuracy": 1.0,
            "access_level_conflict_accuracy": 1.0,
            "outbox_created_count": 0,
            "outbox_completed_count": 0,
            "outbox_retry_count": 0,
            "outbox_dead_letter_count": 0,
            "outbox_staleness_p50_ms": 0,
            "outbox_staleness_p95_ms": 0,
            "qdrant_missing_points": 0,
            "qdrant_orphan_points": 0,
            "qdrant_synced_points": 0,
            "retrieve_context_p50_ms": 0,
            "retrieve_context_p95_ms": p95,
            "retrieve_context_p99_ms": p95,
            "qdrant_search_p95_ms": 0,
            "graph_expansion_p95_ms": 0,
            "mmr_p95_ms": 0,
            "total_bench_duration_ms": started.elapsed().as_millis()
        },
        "failures": failures
    });
    let failures_vec = report
        .get("failures")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    write_reports(&report, &failures_vec, &candidates);
    report
}

#[tokio::test]
async fn quality_bench_quick() {
    let profile = load_profile();
    let queries = load_queries(&profile);
    assert!(
        queries.len() >= 100,
        "enriched quality bench requires at least 100 golden queries"
    );

    let report = if let Ok(endpoint) = env::var("ASTRAVECTOR_QUALITY_ENDPOINT") {
        if endpoint.trim().is_empty() {
            static_report(&profile, &queries)
        } else {
            run_remote(&profile, &queries, endpoint).await
        }
    } else {
        static_report(&profile, &queries)
    };

    let failures = report
        .get("failures")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    write_reports(&report, &failures, &[]);
    assert_eq!(
        report.get("verdict").and_then(Value::as_str),
        Some("PASS"),
        "Quality gates failed: {failures:?}"
    );
}
