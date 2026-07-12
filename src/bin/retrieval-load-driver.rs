use astravector_runtime::pb;
use astravector_runtime::pb::astra_vector_retrieval_facade_client::AstraVectorRetrievalFacadeClient;
use astravector_runtime::retrieval::stability::{top1_matches, top_k_jaccard, ResultIdentity};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tonic::Request;

#[derive(Debug)]
struct Args {
    endpoint: String,
    query_bank: PathBuf,
    target_rps: u64,
    concurrency: usize,
    duration: Duration,
    seed: u64,
    output: PathBuf,
    summary: PathBuf,
    baseline: Option<PathBuf>,
    corpus_snapshot_id: String,
    effective_config_sha256: String,
}

fn required(values: &BTreeMap<String, String>, key: &str) -> anyhow::Result<String> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing required argument --{key}"))
}

fn parse_duration(value: &str) -> anyhow::Result<Duration> {
    let (number, multiplier) = if let Some(value) = value.strip_suffix("ms") {
        (value, Duration::from_millis(1))
    } else if let Some(value) = value.strip_suffix('s') {
        (value, Duration::from_secs(1))
    } else if let Some(value) = value.strip_suffix('m') {
        (value, Duration::from_secs(60))
    } else if let Some(value) = value.strip_suffix('h') {
        (value, Duration::from_secs(3600))
    } else {
        (value, Duration::from_secs(1))
    };
    let count = number.parse::<u32>()?;
    Ok(multiplier.saturating_mul(count))
}

fn parse_args() -> anyhow::Result<Args> {
    let raw = env::args().skip(1).collect::<Vec<_>>();
    let mut values = BTreeMap::new();
    let mut index = 0;
    while index < raw.len() {
        let key = raw[index]
            .strip_prefix("--")
            .ok_or_else(|| anyhow::anyhow!("unexpected argument {}", raw[index]))?;
        let value = raw
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("missing value for --{key}"))?;
        values.insert(key.to_string(), value.clone());
        index += 2;
    }
    let target_rps = required(&values, "target-rps")?.parse()?;
    let concurrency = required(&values, "concurrency")?.parse()?;
    anyhow::ensure!(target_rps > 0, "target-rps must be positive");
    anyhow::ensure!(concurrency > 0, "concurrency must be positive");
    Ok(Args {
        endpoint: required(&values, "endpoint")?,
        query_bank: required(&values, "query-bank")?.into(),
        target_rps,
        concurrency,
        duration: parse_duration(&required(&values, "duration")?)?,
        seed: required(&values, "seed")?.parse()?,
        output: required(&values, "output")?.into(),
        summary: required(&values, "summary")?.into(),
        baseline: values.get("baseline").map(PathBuf::from),
        corpus_snapshot_id: required(&values, "corpus-snapshot-id")?,
        effective_config_sha256: required(&values, "effective-config-sha256")?,
    })
}

fn read_jsonl(path: &Path) -> anyhow::Result<Vec<Value>> {
    let values = fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<Value>, _>>()?;
    anyhow::ensure!(!values.is_empty(), "query bank is empty");
    Ok(values)
}

fn access_level(value: &str) -> i32 {
    match value {
        "INTERNAL" => pb::AccessLevel::Internal as i32,
        "CONFIDENTIAL" => pb::AccessLevel::Confidential as i32,
        "RESTRICTED" => pb::AccessLevel::Restricted as i32,
        _ => pb::AccessLevel::Public as i32,
    }
}

fn profile(value: &str) -> i32 {
    match value {
        "LEGAL" => pb::RetrievalProfile::Legal as i32,
        "TECHNICAL" => pb::RetrievalProfile::Technical as i32,
        "SEMANTIC" => pb::RetrievalProfile::Semantic as i32,
        "LEXICAL_STRICT" => pb::RetrievalProfile::LexicalStrict as i32,
        _ => pb::RetrievalProfile::Balanced as i32,
    }
}

fn expected_strings(expected: &Value, keys: &[&str]) -> Vec<String> {
    let mut result = keys
        .iter()
        .flat_map(|key| {
            expected
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    result.sort();
    result.dedup();
    result
}

fn evidence_expected(query: &Value) -> bool {
    let expected = &query["expected"];
    expected
        .get("evidence_expected")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            query.get("category").and_then(Value::as_str) != Some("hard_negative")
                && (expected
                    .get("min_contexts_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
                    || !expected_strings(
                        expected,
                        &[
                            "must_contain_document_ids",
                            "required_document_ids",
                            "must_contain_block_ids",
                            "required_block_ids",
                            "must_contain_phrases",
                        ],
                    )
                    .is_empty())
        })
}

fn fingerprint(contexts: &[Value]) -> String {
    let canonical = contexts
        .iter()
        .map(|context| {
            format!(
                "{}|{}|{}|{}|{}",
                context["access_zone_id"].as_str().unwrap_or_default(),
                context["document_id"].as_str().unwrap_or_default(),
                context["document_version"].as_u64().unwrap_or_default(),
                context["matched_chunk_id"].as_str().unwrap_or_default(),
                context["source_block_id"].as_str().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn identities(result: &Value) -> anyhow::Result<Vec<ResultIdentity>> {
    result["contexts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|context| {
            Ok(ResultIdentity {
                access_zone_id: context["access_zone_id"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                document_id: context["document_id"].as_str().unwrap_or_default().into(),
                document_version: context["document_version"].as_u64().unwrap_or_default(),
                matched_chunk_id: context["matched_chunk_id"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                source_block_id: context["source_block_id"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
            })
        })
        .collect()
}

fn baseline_by_query(path: &Path, snapshot_id: &str) -> anyhow::Result<BTreeMap<String, Value>> {
    let mut baseline = BTreeMap::new();
    for value in read_jsonl(path)? {
        anyhow::ensure!(
            value["corpus_snapshot_id"].as_str() == Some(snapshot_id),
            "baseline corpus snapshot does not match current corpus snapshot"
        );
        if value["grpc_status"] != "OK" {
            continue;
        }
        let query_id = value["query_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("baseline result is missing query_id"))?;
        baseline.entry(query_id.to_string()).or_insert(value);
    }
    Ok(baseline)
}

fn grpc_status_name(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "OK",
        tonic::Code::Cancelled => "CANCELLED",
        tonic::Code::Unknown => "UNKNOWN",
        tonic::Code::InvalidArgument => "INVALID_ARGUMENT",
        tonic::Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
        tonic::Code::NotFound => "NOT_FOUND",
        tonic::Code::AlreadyExists => "ALREADY_EXISTS",
        tonic::Code::PermissionDenied => "PERMISSION_DENIED",
        tonic::Code::ResourceExhausted => "RESOURCE_EXHAUSTED",
        tonic::Code::FailedPrecondition => "FAILED_PRECONDITION",
        tonic::Code::Aborted => "ABORTED",
        tonic::Code::OutOfRange => "OUT_OF_RANGE",
        tonic::Code::Unimplemented => "UNIMPLEMENTED",
        tonic::Code::Internal => "INTERNAL",
        tonic::Code::Unavailable => "UNAVAILABLE",
        tonic::Code::DataLoss => "DATA_LOSS",
        tonic::Code::Unauthenticated => "UNAUTHENTICATED",
    }
}

async fn execute_request(
    mut client: AstraVectorRetrievalFacadeClient<tonic::transport::Channel>,
    request_no: usize,
    query: Value,
) -> Value {
    let query_id = query.get("id").and_then(Value::as_str).unwrap_or("query");
    let context = &query["context"];
    let started_at = chrono::Utc::now();
    let started = Instant::now();
    let expected = &query["expected"];
    let critical = query
        .get("critical")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let expected_documents = expected_strings(
        expected,
        &["must_contain_document_ids", "required_document_ids"],
    );
    let expected_blocks = expected_strings(
        expected,
        &[
            "must_contain_block_ids",
            "required_block_ids",
            "expected_source_block_ids",
        ],
    );
    let forbidden_documents = expected_strings(expected, &["forbidden_document_ids"]);
    let expected_version = expected
        .get("expected_document_version")
        .and_then(Value::as_u64);
    let mut request = Request::new(pb::RetrieveContextRequest {
        context: Some(pb::RequestContext {
            correlation_id: format!("load-{request_no}"),
            idempotency_key: format!("load:{request_no}"),
            caller_service: "retrieval-load-driver".into(),
            caller_user_id: "local-load".into(),
            caller_access_level: access_level(
                context
                    .get("caller_access_level")
                    .and_then(Value::as_str)
                    .unwrap_or("PUBLIC"),
            ),
        }),
        question: query
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        access_zone_code: context
            .get("access_zone_code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        profile: profile(
            context
                .get("profile")
                .and_then(Value::as_str)
                .unwrap_or("BALANCED"),
        ),
        max_contexts: expected
            .get("max_contexts_count")
            .and_then(Value::as_u64)
            .unwrap_or(10) as u32,
        response_detail: pb::ResponseDetail::Standard as i32,
        enable_graph_expansion: context
            .get("enable_graph_expansion")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        graph_max_hops: 1,
        graph_max_related_contexts: 5,
        ..Default::default()
    });
    if let Ok(api_key) = env::var("ASTRAVECTOR_API_KEY") {
        if let Ok(value) = api_key.parse() {
            request.metadata_mut().insert("x-api-key", value);
        }
    }
    let response = client.retrieve_context(request).await;
    match response {
        Ok(response) => {
            let response = response.into_inner();
            let summary = response.summary.unwrap_or_default();
            let contexts = response
                .contexts
                .iter()
                .enumerate()
                .map(|(rank, context)| {
                    let citation = context.citation.as_ref();
                    let expected_document_match = expected_documents.is_empty()
                        || [
                            Some(context.document_id.as_str()),
                            context.metadata.get("fixture_document_id").map(String::as_str),
                            context.metadata.get("original_document_id").map(String::as_str),
                            context.metadata.get("external_document_id").map(String::as_str),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|document| expected_documents.iter().any(|expected| expected == document));
                    let expected_block_match = expected_blocks.is_empty()
                        || expected_blocks.iter().any(|expected| {
                            expected == &context.source_block_id
                                || context
                                    .metadata
                                    .get("fixture_source_block_id")
                                    .is_some_and(|value| value == expected)
                        });
                    let forbidden_document = forbidden_documents.iter().any(|forbidden| {
                        forbidden == &context.document_id
                            || context
                                .metadata
                                .get("fixture_document_id")
                                .is_some_and(|value| value == forbidden)
                    });
                    json!({
                        "rank": rank + 1,
                        "access_zone_id": context.access_zone_id,
                        "document_id": context.document_id,
                        "document_version": context.document_version,
                        "matched_chunk_id": context.matched_chunk_id,
                        "source_block_id": context.source_block_id,
                        "citation_complete": citation.is_some_and(|value| !value.document_id.is_empty() && !value.source_uri.is_empty() && !value.matched_chunk_id.is_empty() && !value.source_block_id.is_empty()),
                        "citation_grounded": citation.is_some_and(|value| value.document_id == context.document_id && value.document_version == context.document_version && value.matched_chunk_id == context.matched_chunk_id && value.source_block_id == context.source_block_id),
                        "expected_document_match": expected_document_match,
                        "expected_block_match": expected_block_match,
                        "forbidden_document": forbidden_document,
                    })
                })
                .collect::<Vec<_>>();
            let top1_correct = contexts
                .first()
                .is_none_or(|context| context["expected_document_match"] == true);
            let positive_empty = evidence_expected(&query) && contexts.is_empty();
            let missing_expected_document = !expected_documents.is_empty()
                && !contexts
                    .iter()
                    .any(|value| value["expected_document_match"] == true);
            let missing_expected_block = !expected_blocks.is_empty()
                && !contexts
                    .iter()
                    .any(|value| value["expected_block_match"] == true);
            let forbidden_document = contexts
                .iter()
                .any(|value| value["forbidden_document"] == true);
            let wrong_version = expected_version.is_some_and(|expected| {
                contexts.iter().any(|value| {
                    value["expected_document_match"] == true
                        && value["document_version"].as_u64() != Some(expected)
                })
            });
            let result_fingerprint = fingerprint(&contexts);
            json!({
                "request_no": request_no,
                "query_id": query_id,
                "category": query.get("category").and_then(Value::as_str).unwrap_or("general"),
                "started_at": started_at.to_rfc3339(),
                "latency_ms": started.elapsed().as_secs_f64() * 1000.0,
                "grpc_status": "OK",
                "evidence_status": pb::EvidenceStatus::try_from(summary.evidence_status).map(|value| value.as_str_name().to_string()).unwrap_or_else(|_| "EVIDENCE_STATUS_UNSPECIFIED".into()),
                "degraded": summary.degraded,
                "warning_codes": summary.degradation_codes,
                "corpus_snapshot_id": summary.corpus_snapshot_id,
                "effective_config_sha256": summary.effective_config_sha256,
                "hybrid_execution": {"dense":summary.dense_branch_executed,"sparse":summary.sparse_branch_executed,"fusion":summary.fusion_executed},
                "contexts": contexts,
                "result_fingerprint": result_fingerprint,
                "top1_correct": top1_correct,
                "positive_empty": positive_empty,
                "missing_expected_document": missing_expected_document,
                "missing_expected_block": missing_expected_block,
                "forbidden_document": forbidden_document,
                "wrong_version": wrong_version,
                "critical": critical
            })
        }
        Err(status) => json!({
            "request_no": request_no, "query_id": query_id,
            "category": query.get("category").and_then(Value::as_str).unwrap_or("general"),
            "started_at": started_at.to_rfc3339(), "latency_ms": started.elapsed().as_secs_f64() * 1000.0,
            "grpc_status": grpc_status_name(status.code()),
            "error": status.message(), "contexts": [], "top1_correct": false,
            "positive_empty": false, "critical": critical
        }),
    }
}

fn percentile(mut values: Vec<f64>, percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let index = ((values.len() - 1) as f64 * percentile).ceil() as usize;
    values[index]
}

fn latency_summary(values: Vec<f64>) -> Value {
    json!({
        "count": values.len(),
        "p50_ms": percentile(values.clone(), 0.50),
        "p95_ms": percentile(values.clone(), 0.95),
        "p99_ms": percentile(values.clone(), 0.99),
        "max_ms": values.into_iter().max_by(f64::total_cmp).unwrap_or(0.0),
    })
}

fn write_atomic(path: &Path, body: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, body)?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let queries = read_jsonl(&args.query_bank)?;
    let query_bank_sha256 = hex::encode(Sha256::digest(fs::read(&args.query_bank)?));
    let baseline = args
        .baseline
        .as_deref()
        .map(|path| baseline_by_query(path, &args.corpus_snapshot_id))
        .transpose()?;
    let request_count = (args.duration.as_secs_f64() * args.target_rps as f64).ceil() as usize;
    let mut sequence = Vec::with_capacity(request_count);
    let mut rng = StdRng::seed_from_u64(args.seed);
    while sequence.len() < request_count {
        let mut indexes = (0..queries.len()).collect::<Vec<_>>();
        indexes.shuffle(&mut rng);
        sequence.extend(indexes);
    }
    sequence.truncate(request_count);
    let semaphore = Arc::new(Semaphore::new(args.concurrency));
    let client = AstraVectorRetrievalFacadeClient::connect(args.endpoint.clone()).await?;
    let results = Arc::new(Mutex::new(Vec::with_capacity(request_count)));
    let interval = Duration::from_secs_f64(1.0 / args.target_rps as f64);
    let started = Instant::now();
    let mut handles = Vec::with_capacity(request_count);
    let mut started_requests = 0usize;
    for (request_no, query_index) in sequence.into_iter().enumerate() {
        tokio::time::sleep_until(tokio::time::Instant::from_std(
            started + interval.saturating_mul(request_no as u32),
        ))
        .await;
        let permit = match semaphore.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                results.lock().await.push(json!({
                    "request_no": request_no + 1,
                    "query_id": queries[query_index].get("id").and_then(Value::as_str).unwrap_or("query"),
                    "category": queries[query_index].get("category").and_then(Value::as_str).unwrap_or("general"),
                    "started_at": chrono::Utc::now().to_rfc3339(),
                    "latency_ms": 0.0,
                    "grpc_status": "LOCAL_DROPPED",
                    "error": "bounded pending request capacity exhausted",
                    "contexts": [],
                    "top1_correct": false,
                    "positive_empty": false,
                    "critical": queries[query_index].get("critical").and_then(Value::as_bool).unwrap_or(false)
                }));
                continue;
            }
        };
        started_requests += 1;
        let client = client.clone();
        let query = queries[query_index].clone();
        let results = results.clone();
        handles.push(tokio::spawn(async move {
            let result = execute_request(client, request_no + 1, query).await;
            results.lock().await.push(result);
            drop(permit);
        }));
    }
    for handle in handles {
        handle.await?;
    }
    let mut results = Arc::try_unwrap(results)
        .map_err(|_| anyhow::anyhow!("result references remain"))?
        .into_inner();
    results.sort_by_key(|value| value["request_no"].as_u64().unwrap_or_default());
    for result in &mut results {
        result["corpus_snapshot_id"] = Value::String(args.corpus_snapshot_id.clone());
        result["effective_config_sha256"] = Value::String(args.effective_config_sha256.clone());
    }
    let output = results
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write_atomic(&args.output, &output)?;
    let ok = results
        .iter()
        .filter(|value| value["grpc_status"] == "OK")
        .count();
    let positive_empty = results
        .iter()
        .filter(|value| value["positive_empty"] == true)
        .count();
    let critical_wrong = results
        .iter()
        .filter(|value| value["critical"] == true && value["top1_correct"] != true)
        .count();
    let citation_incomplete = results
        .iter()
        .flat_map(|value| value["contexts"].as_array().into_iter().flatten())
        .filter(|context| context["citation_complete"] != true)
        .count();
    let citation_grounding_failures = results
        .iter()
        .flat_map(|value| value["contexts"].as_array().into_iter().flatten())
        .filter(|context| context["citation_grounded"] != true)
        .count();
    let missing_expected_documents = results
        .iter()
        .filter(|value| value["missing_expected_document"] == true)
        .count();
    let missing_expected_blocks = results
        .iter()
        .filter(|value| value["missing_expected_block"] == true)
        .count();
    let forbidden_documents = results
        .iter()
        .filter(|value| value["forbidden_document"] == true)
        .count();
    let wrong_versions = results
        .iter()
        .filter(|value| value["wrong_version"] == true)
        .count();
    let mut stability_compared = 0usize;
    let mut top1_matches_count = 0usize;
    let mut critical_compared = 0usize;
    let mut critical_top1_matches = 0usize;
    let mut top5_jaccard_sum = 0.0;
    if let Some(baseline) = &baseline {
        for result in &results {
            let query_id = result["query_id"].as_str().unwrap_or_default();
            let expected = baseline
                .get(query_id)
                .ok_or_else(|| anyhow::anyhow!("baseline is missing query {query_id}"))?;
            let expected_ids = identities(expected)?;
            let loaded_ids = identities(result)?;
            stability_compared += 1;
            let top1_stable = top1_matches(&expected_ids, &loaded_ids);
            top1_matches_count += usize::from(top1_stable);
            top5_jaccard_sum += top_k_jaccard(&expected_ids, &loaded_ids, 5);
            if result["critical"] == true {
                critical_compared += 1;
                critical_top1_matches += usize::from(top1_stable);
            }
        }
    }
    let top1_stability =
        (stability_compared > 0).then(|| top1_matches_count as f64 / stability_compared as f64);
    let critical_top1_stability =
        (critical_compared > 0).then(|| critical_top1_matches as f64 / critical_compared as f64);
    let top5_mean_jaccard =
        (stability_compared > 0).then(|| top5_jaccard_sum / stability_compared as f64);
    let stability_pass = baseline.is_none()
        || (top1_stability.is_some_and(|value| value >= 0.99)
            && top5_mean_jaccard.is_some_and(|value| value >= 0.95)
            && critical_top1_stability.is_none_or(|value| value == 1.0));
    let latencies = results
        .iter()
        .filter(|value| value["grpc_status"] == "OK")
        .filter_map(|value| value["latency_ms"].as_f64())
        .collect::<Vec<_>>();
    let mut statuses = BTreeMap::new();
    for result in &results {
        *statuses
            .entry(result["grpc_status"].as_str().unwrap_or("UNKNOWN"))
            .or_insert(0usize) += 1;
    }
    let latency_by_status = statuses
        .keys()
        .map(|status| {
            let values = results
                .iter()
                .filter(|value| value["grpc_status"].as_str() == Some(status))
                .filter_map(|value| value["latency_ms"].as_f64())
                .collect::<Vec<_>>();
            ((*status).to_string(), latency_summary(values))
        })
        .collect::<serde_json::Map<_, _>>();
    let achieved_rps = started_requests as f64 / args.duration.as_secs_f64().max(f64::EPSILON);
    let achieved_ratio = achieved_rps / args.target_rps as f64;
    let success_rate = ok as f64 / results.len().max(1) as f64;
    let transport_pass = success_rate >= 0.99;
    let summary = json!({
        "schema_version": "1.0",
        "driver": "retrieval-load-driver",
        "seed": args.seed,
        "query_bank_sha256": query_bank_sha256,
        "corpus_snapshot_id": args.corpus_snapshot_id,
        "effective_config_sha256": args.effective_config_sha256,
        "target_rps": args.target_rps,
        "concurrency": args.concurrency,
        "requested_duration_seconds": args.duration.as_secs_f64(),
        "wall_duration_seconds": started.elapsed().as_secs_f64(),
        "requests_total": results.len(),
        "scheduled_requests": request_count,
        "started_requests": started_requests,
        "completed_requests": results.len(),
        "achieved_rps": achieved_rps,
        "achieved_ratio": achieved_ratio,
        "successful_requests": ok,
        "success_rate": success_rate,
        "error_rate": 1.0 - success_rate,
        "transport_pass": transport_pass,
        "status_distribution": statuses,
        "latency_by_status": latency_by_status,
        "successful_p95_ms": percentile(latencies.clone(), 0.95),
        "successful_p99_ms": percentile(latencies, 0.99),
        "positive_empty_count": positive_empty,
        "critical_wrong_result_count": critical_wrong,
        "citation_incomplete_count": citation_incomplete,
        "citation_grounding_failure_count": citation_grounding_failures,
        "missing_expected_document_count": missing_expected_documents,
        "missing_expected_block_count": missing_expected_blocks,
        "forbidden_document_count": forbidden_documents,
        "wrong_version_count": wrong_versions,
        "stability": {
            "comparison_completed": baseline.is_some(),
            "sample_count": stability_compared,
            "top1_identity_stability": top1_stability,
            "critical_sample_count": critical_compared,
            "critical_top1_identity_stability": critical_top1_stability,
            "top5_mean_jaccard": top5_mean_jaccard,
            "pass": stability_pass
        },
        "verdict": if transport_pass && achieved_ratio >= 0.95 && positive_empty == 0 && critical_wrong == 0 && citation_incomplete == 0 && citation_grounding_failures == 0 && missing_expected_documents == 0 && missing_expected_blocks == 0 && forbidden_documents == 0 && wrong_versions == 0 && stability_pass { "PASS" } else { "FAIL" }
    });
    write_atomic(
        &args.summary,
        &(serde_json::to_string_pretty(&summary)? + "\n"),
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    if summary["verdict"] != "PASS" {
        anyhow::bail!("retrieval load correctness gate failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_by_status_counts_each_request_once() {
        let summary = latency_summary(vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(summary["count"], 4);
        assert_eq!(summary["max_ms"], 4.0);
        assert_eq!(summary["p50_ms"], 3.0);
    }

    #[test]
    fn achieved_ratio_rejects_underdriven_step() {
        let requested_rps = 10.0;
        let started_requests = 80.0;
        let duration_seconds = 10.0;
        let achieved_ratio = (started_requests / duration_seconds) / requested_rps;
        assert!(achieved_ratio < 0.95);
    }
}
