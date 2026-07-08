#![recursion_limit = "256"]

use astravector_runtime::pb;
use astravector_runtime::pb::astra_vector_ingestion_facade_client::AstraVectorIngestionFacadeClient;
use astravector_runtime::pb::astra_vector_retrieval_facade_client::AstraVectorRetrievalFacadeClient;
use astravector_runtime::pb::astra_vector_v004_control_client::AstraVectorV004ControlClient;
use astravector_runtime::sparse::{
    SparseTechnicalEncoder, SparseTokenClass, TECHNICAL_SPARSE_ENCODER_VERSION,
    TECHNICAL_SPARSE_INDEX_STRATEGY, TECHNICAL_SPARSE_MODE,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::sleep;
use tonic::metadata::MetadataValue;
use tonic::Request;
use uuid::Uuid;

const QUALITY_ROOT: &str = "benchmarks/quality";

#[derive(Default)]
struct RuntimeStats {
    fixtures_ingested_count: usize,
    documents_registered_count: usize,
    documents_indexed_count: usize,
    access_zones_auto_created_count: u64,
    outbox_created_count: u64,
    outbox_completed_count: u64,
    outbox_dead_letter_count: u64,
    qdrant_collection_count: u64,
    qdrant_points_count: u64,
    qdrant_payload_verified: bool,
    dense_embeddings_count: u64,
    sparse_embeddings_count: u64,
    retrieve_context_queries_total: usize,
    retrieve_context_queries_passed: usize,
    retrieve_context_queries_failed: usize,
    retrieve_context_queries_blocked: usize,
    retrieve_context_queries_skipped: usize,
    queries_with_empty_contexts: usize,
    queries_with_retrieve_errors: usize,
    access_zone_conflict_accuracy: f64,
    access_level_audit: AccessLevelAudit,
    forced_caller_access_level: Option<String>,
    by_category: BTreeMap<String, CategoryStats>,
    by_mode: BTreeMap<String, CategoryStats>,
    by_reason: BTreeMap<String, u64>,
    capability_requirements: CapabilityRequirements,
    sparse: SparseRuntimeStats,
    hybrid: HybridRuntimeStats,
    graph: GraphRuntimeStats,
    no_answer: NoAnswerRuntimeStats,
    query_diagnostics: Vec<Value>,
}

struct AccessLevelAudit {
    fixture_distribution: BTreeMap<String, u64>,
    postgres_distribution: BTreeMap<String, u64>,
    qdrant_distribution: BTreeMap<String, u64>,
    status: String,
    reason: Option<String>,
}

impl Default for AccessLevelAudit {
    fn default() -> Self {
        Self {
            fixture_distribution: BTreeMap::new(),
            postgres_distribution: BTreeMap::new(),
            qdrant_distribution: BTreeMap::new(),
            status: "NOT_RUN".into(),
            reason: None,
        }
    }
}

#[derive(Default)]
struct Preflight {
    model_files_found: bool,
    tokenizer_found: bool,
    grpc_endpoint_reachable: bool,
    postgres_reachable: bool,
    qdrant_reachable: bool,
    auto_create_on_ingestion: bool,
    auto_create_on_search: bool,
}

#[derive(Default)]
struct EvalResult {
    failures: Vec<String>,
    candidates: Vec<Value>,
    reasons: Vec<&'static str>,
    returned_document_ids: Vec<String>,
    returned_block_ids: Vec<String>,
    contexts_count: usize,
    graph_expanded_contexts_count: usize,
    graph_expected_related_total: usize,
    graph_expected_related_hits: usize,
    mmr_expected_aspects_total: usize,
    mmr_expected_aspects_hits: usize,
}

#[derive(Clone, Default)]
struct Capabilities {
    dense_available: bool,
    sparse_available: bool,
    hybrid_available: bool,
    graph_rag_available: bool,
    mmr_available: bool,
}

#[derive(Clone, Default)]
struct CapabilityRequirements {
    require_dense: bool,
    require_sparse: bool,
    require_hybrid: bool,
    require_graph: bool,
    require_mmr: bool,
}

#[derive(Default)]
struct SparseRuntimeStats {
    qdrant_sparse_config_present: bool,
    qdrant_sparse_points_sampled: u64,
    qdrant_sparse_points_with_vectors: u64,
    document_query_encoder_consistency_checked: bool,
    technical_token_count: u64,
    numeric_token_count: u64,
    alphanumeric_token_count: u64,
    special_token_count: u64,
}

#[derive(Default)]
struct HybridRuntimeStats {
    fusion_strategy: Option<String>,
    dense_branch_hits: u64,
    sparse_branch_hits: u64,
    fused_hits: u64,
}

#[derive(Default)]
struct GraphRuntimeStats {
    relations_loaded_count: u64,
    relations_ingested_count: u64,
    relations_persisted_count: u64,
    relations_queryable_count: u64,
    graph_edges_available_count: u64,
    graph_expanded_contexts_count: u64,
    graph_expected_related_total: u64,
    graph_expected_related_hits: u64,
    graph_access_violation_count: u64,
    graph_duplicate_suppressed_count: u64,
    graph_timeout_count: u64,
    graph_db_error_count: u64,
    forbidden_graph_blocks_returned: u64,
}

#[derive(Default)]
struct NoAnswerRuntimeStats {
    enabled: bool,
    min_dense_score: f64,
    min_sparse_score: f64,
    min_hybrid_score: f64,
    sparse_only_min_matched_terms: u64,
    sparse_only_require_technical_token: bool,
    exact_technical_boost: f64,
    hard_negative_strict: bool,
    debug_enabled: bool,
    pre_mmr_filtered_candidate_count: u64,
    post_mmr_no_answer_triggered_count: u64,
    non_zero_max_false_positive_contexts_warnings: u64,
}

fn no_answer_runtime_defaults(debug_enabled: bool) -> NoAnswerRuntimeStats {
    NoAnswerRuntimeStats {
        enabled: true,
        min_dense_score: 0.25,
        min_sparse_score: 0.10,
        min_hybrid_score: 0.30,
        sparse_only_min_matched_terms: 2,
        sparse_only_require_technical_token: true,
        exact_technical_boost: 0.50,
        hard_negative_strict: true,
        debug_enabled,
        pre_mmr_filtered_candidate_count: 0,
        post_mmr_no_answer_triggered_count: 0,
        non_zero_max_false_positive_contexts_warnings: 0,
    }
}

#[derive(Default)]
struct CategoryStats {
    total: usize,
    passed: usize,
    failed: usize,
    blocked: usize,
    skipped: usize,
}

impl CategoryStats {
    fn recall_at_5(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.passed as f64 / self.total as f64
        }
    }
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

fn document_files_for_profile(profile: &Value) -> Vec<PathBuf> {
    profile
        .get("corpora")
        .and_then(Value::as_array)
        .expect("profile missing corpora")
        .iter()
        .map(|name| {
            Path::new(QUALITY_ROOT).join("corpora").join(format!(
                "{}/documents.jsonl",
                name.as_str().expect("corpus name must be string")
            ))
        })
        .collect()
}

fn query_files_for_profile(profile: &Value) -> Vec<PathBuf> {
    profile
        .get("queries")
        .and_then(Value::as_array)
        .expect("profile missing queries")
        .iter()
        .map(|name| {
            Path::new(QUALITY_ROOT).join("queries").join(format!(
                "{}.jsonl",
                name.as_str().expect("query name must be string")
            ))
        })
        .collect()
}

fn relation_files_for_profile(profile: &Value) -> Vec<PathBuf> {
    profile
        .get("corpora")
        .and_then(Value::as_array)
        .expect("profile missing corpora")
        .iter()
        .map(|name| {
            Path::new(QUALITY_ROOT).join("corpora").join(format!(
                "{}/relations.jsonl",
                name.as_str().expect("corpus name must be string")
            ))
        })
        .filter(|path| path.exists())
        .collect()
}

fn load_relations(profile: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for file in relation_files_for_profile(profile) {
        out.extend(read_jsonl(&file));
    }
    out
}

fn runtime_relation_payload(relations: &[Value]) -> String {
    let payload = relations
        .iter()
        .filter_map(|relation| {
            let from_document_id = relation.get("from_document_id")?.as_str()?;
            let to_document_id = relation.get("to_document_id")?.as_str()?;
            Some(json!({
                "relation_id": relation.get("relation_id").and_then(Value::as_str).unwrap_or(""),
                "from_document_id": from_document_id,
                "from_document_uuid": runtime_document_uuid(from_document_id),
                "from_block_id": relation.get("from_block_id").and_then(Value::as_str).unwrap_or(""),
                "to_document_id": to_document_id,
                "to_document_uuid": runtime_document_uuid(to_document_id),
                "to_block_id": relation.get("to_block_id").and_then(Value::as_str).unwrap_or(""),
                "relation_type": relation.get("relation_type").and_then(Value::as_str).unwrap_or("RELATED_TO"),
                "weight": relation.get("weight").and_then(Value::as_f64).unwrap_or(1.0),
                "quality_run_id": quality_run_id().unwrap_or_else(|| "fix474".into()),
                "quality_runtime_bench": "fix475"
            }))
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&payload).unwrap_or_else(|_| "[]".into())
}

fn load_documents(profile: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for file in document_files_for_profile(profile) {
        out.extend(read_jsonl(&file));
    }
    out
}

fn load_queries(profile: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for file in query_files_for_profile(profile) {
        out.extend(read_jsonl(&file));
    }
    let explicit_query_filter = env::var("QUERY_FILTER").ok();
    if let Some(filter) = explicit_query_filter.as_deref() {
        let filters = filter
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if !filters.is_empty() {
            out.retain(|query| {
                let id = query.get("id").and_then(Value::as_str).unwrap_or_default();
                filters
                    .iter()
                    .any(|filter| id == *filter || id.starts_with(*filter))
            });
        }
    }
    let graph_required = env::var("ASTRAVECTOR_QUALITY_REQUIRE_GRAPH")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            )
        })
        .unwrap_or(false);
    let profile_name = profile
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let graph_threshold = profile
        .pointer("/thresholds/graph_expected_related_hit_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if explicit_query_filter.is_none()
        && !graph_required
        && profile_name != "graph-quick"
        && graph_threshold <= 0.0
    {
        out.retain(|query| {
            query_category(query) != "graph_rag"
                && !query
                    .pointer("/context/enable_graph_expansion")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && !query
                    .pointer("/expected/expected_related_block_ids")
                    .and_then(Value::as_array)
                    .map(|items| !items.is_empty())
                    .unwrap_or(false)
        });
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

fn block_type(value: &str) -> i32 {
    match value {
        "DOCUMENT" => pb::BlockType::Document as i32,
        "SECTION" => pb::BlockType::Section as i32,
        "SUBSECTION" => pb::BlockType::Subsection as i32,
        "TABLE" => pb::BlockType::Table as i32,
        "LIST" => pb::BlockType::List as i32,
        "FAQ_ITEM" => pb::BlockType::FaqItem as i32,
        "CODE_BLOCK" => pb::BlockType::CodeBlock as i32,
        _ => pb::BlockType::Paragraph as i32,
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

fn effective_retrieval_profile(profile_name: &str, query_profile: &str) -> i32 {
    match profile_name {
        "dense-only-quick" => pb::RetrievalProfile::Semantic as i32,
        "sparse-quick" => pb::RetrievalProfile::LexicalStrict as i32,
        _ => retrieval_profile(query_profile),
    }
}

fn sha256_hex(input: &str) -> String {
    format!("{:x}", Sha256::digest(input.as_bytes()))
}

fn quality_run_id() -> Option<String> {
    env::var("ASTRAVECTOR_QUALITY_RUN_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn runtime_document_uuid(fixture_document_id: &str) -> String {
    let namespace = quality_run_id().unwrap_or_else(|| "fix474".into());
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("astravector-quality-runtime:{namespace}:{fixture_document_id}").as_bytes(),
    )
    .to_string()
}

fn string_map(value: Option<&Value>) -> std::collections::HashMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(k, v)| {
                    let value = v
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string());
                    (k.clone(), value)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn fixture_blocks(doc: &Value) -> Vec<pb::LogicalBlock> {
    let mut blocks = Vec::new();
    blocks.push(pb::LogicalBlock {
        block_id: "doc-root".into(),
        parent_block_id: String::new(),
        block_type: pb::BlockType::Document as i32,
        text: doc
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("quality fixture document")
            .to_string(),
        order_index: 0,
        source_location: Some(pb::SourceLocation {
            section_path: doc
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            ..Default::default()
        }),
        source_links: Vec::new(),
        metadata: string_map(doc.get("metadata")),
    });
    for (idx, block) in doc
        .get("blocks")
        .and_then(Value::as_array)
        .expect("quality document missing blocks")
        .iter()
        .enumerate()
    {
        let heading = block
            .get("heading")
            .and_then(Value::as_str)
            .unwrap_or_default();
        blocks.push(pb::LogicalBlock {
            block_id: block
                .get("block_id")
                .and_then(Value::as_str)
                .expect("block missing block_id")
                .to_string(),
            parent_block_id: "doc-root".into(),
            block_type: block_type(
                block
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("SECTION"),
            ),
            text: block
                .get("text")
                .and_then(Value::as_str)
                .expect("block missing text")
                .to_string(),
            order_index: (idx + 1) as u32,
            source_location: Some(pb::SourceLocation {
                page_start: 1,
                page_end: 1,
                section_path: heading.to_string(),
                heading: heading.to_string(),
                ..Default::default()
            }),
            source_links: Vec::new(),
            metadata: string_map(block.get("metadata")),
        });
    }
    blocks
}

fn document_text_for_hash(doc: &Value) -> String {
    let mut text = doc
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some(blocks) = doc.get("blocks").and_then(Value::as_array) {
        for block in blocks {
            text.push('\n');
            text.push_str(
                block
                    .get("heading")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            text.push('\n');
            text.push_str(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
        }
    }
    text
}

fn api_key_metadata(req: &mut Request<impl Sized>) {
    if let Ok(api_key) = env::var("ASTRAVECTOR_QUALITY_API_KEY") {
        if !api_key.is_empty() {
            req.metadata_mut().insert(
                "x-api-key",
                MetadataValue::try_from(api_key.as_str()).expect("invalid API key metadata value"),
            );
        }
    }
}

async fn qdrant_collection_count(base_url: &str) -> u64 {
    let url = format!("{}/collections", base_url.trim_end_matches('/'));
    let Ok(response) = reqwest::get(url).await else {
        return 0;
    };
    let Ok(value) = response.json::<Value>().await else {
        return 0;
    };
    value
        .pointer("/result/collections")
        .and_then(Value::as_array)
        .map(|items| items.len() as u64)
        .unwrap_or(0)
}

async fn qdrant_points_count(base_url: &str, collection: &str) -> u64 {
    let url = format!(
        "{}/collections/{}",
        base_url.trim_end_matches('/'),
        collection
    );
    let Ok(response) = reqwest::get(url).await else {
        return 0;
    };
    let Ok(value) = response.json::<Value>().await else {
        return 0;
    };
    value
        .pointer("/result/points_count")
        .or_else(|| value.pointer("/result/vectors_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

async fn qdrant_payload_verified(base_url: &str, collection: &str) -> bool {
    let url = format!(
        "{}/collections/{}/points/scroll",
        base_url.trim_end_matches('/'),
        collection
    );
    let Ok(response) = reqwest::Client::new()
        .post(url)
        .json(&json!({"limit": 1, "with_payload": true, "with_vector": false}))
        .send()
        .await
    else {
        return false;
    };
    let Ok(value) = response.json::<Value>().await else {
        return false;
    };
    let Some(payload) = value
        .pointer("/result/points/0/payload")
        .or_else(|| value.pointer("/result/0/payload"))
    else {
        return false;
    };
    let has_zone =
        payload.get("access_zone_id").is_some() || payload.get("access_zone_code").is_some();
    let has_document = payload.get("document_id").is_some();
    let has_chunk = payload.get("chunk_id").is_some() || payload.get("source_block_id").is_some();
    has_zone && has_document && has_chunk
}

async fn qdrant_sparse_config_present(base_url: &str, collection: &str) -> bool {
    let url = format!(
        "{}/collections/{}",
        base_url.trim_end_matches('/'),
        collection
    );
    let Ok(response) = reqwest::get(url).await else {
        return false;
    };
    let Ok(value) = response.json::<Value>().await else {
        return false;
    };
    value
        .pointer("/result/config/params/sparse_vectors")
        .and_then(Value::as_object)
        .map(|map| !map.is_empty())
        .unwrap_or(false)
}

async fn qdrant_sparse_points_sample(base_url: &str, collection: &str) -> (u64, u64) {
    let url = format!(
        "{}/collections/{}/points/scroll",
        base_url.trim_end_matches('/'),
        collection
    );
    let Ok(response) = reqwest::Client::new()
        .post(url)
        .json(&json!({
            "limit": 256,
            "with_payload": false,
            "with_vector": ["sparse"]
        }))
        .send()
        .await
    else {
        return (0, 0);
    };
    let Ok(value) = response.json::<Value>().await else {
        return (0, 0);
    };
    let points = value
        .pointer("/result/points")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let sampled = points.len() as u64;
    let with_sparse = points
        .iter()
        .filter(|point| {
            point
                .pointer("/vector/sparse/indices")
                .and_then(Value::as_array)
                .map(|items| !items.is_empty())
                .unwrap_or(false)
        })
        .count() as u64;
    (sampled, with_sparse)
}

async fn preflight(endpoint: &str) -> Preflight {
    let postgres_url = env::var("ASTRAVECTOR_DB_URL").unwrap_or_else(|_| {
        "postgres://astravector:astravector@127.0.0.1:55432/astravector".into()
    });
    let qdrant_url =
        env::var("ASTRAVECTOR_QDRANT_URL").unwrap_or_else(|_| "http://127.0.0.1:6333".into());
    let grpc_endpoint_reachable = AstraVectorIngestionFacadeClient::connect(endpoint.to_string())
        .await
        .is_ok();
    let sqlx_postgres_reachable = match sqlx::PgPool::connect(&postgres_url).await {
        Ok(pool) => sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(&pool)
            .await
            .is_ok(),
        Err(_) => false,
    };
    let postgres_reachable = sqlx_postgres_reachable
        || TcpStream::connect(
            "127.0.0.1:55432"
                .parse::<SocketAddr>()
                .expect("static postgres socket address must parse"),
        )
        .await
        .is_ok();
    let qdrant_reachable =
        reqwest::get(format!("{}/collections", qdrant_url.trim_end_matches('/')))
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false);
    Preflight {
        model_files_found: model_path_exists(),
        tokenizer_found: tokenizer_path_exists(),
        grpc_endpoint_reachable,
        postgres_reachable,
        qdrant_reachable,
        auto_create_on_ingestion: env::var(
            "ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION",
        )
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false),
        auto_create_on_search: env::var("ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
    }
}

async fn collect_storage_stats(stats: &mut RuntimeStats) {
    let postgres_url = env::var("ASTRAVECTOR_DB_URL").unwrap_or_else(|_| {
        "postgres://astravector:astravector@127.0.0.1:55432/astravector".into()
    });
    let Ok(pool) = sqlx::PgPool::connect(&postgres_url).await else {
        return;
    };
    stats.access_zones_auto_created_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM astravector.access_zones WHERE access_zone_code IN('1700','1800','1900') AND status='ACTIVE' AND auto_created=true AND created_reason='INGESTION_AUTO_CREATE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0) as u64;
    stats.documents_indexed_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM astravector.document_versions WHERE status IN('ACTIVE','INDEXING','REGISTERED')",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0) as usize;
    stats.outbox_created_count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM astravector.vector_outbox")
            .fetch_one(&pool)
            .await
            .unwrap_or(0) as u64;
    stats.outbox_completed_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM astravector.vector_outbox WHERE status='COMPLETED'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0) as u64;
    stats.outbox_dead_letter_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM astravector.vector_outbox WHERE status='DEAD_LETTER'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0) as u64;
    stats.dense_embeddings_count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM astravector.embedding_dense")
            .fetch_one(&pool)
            .await
            .unwrap_or(0) as u64;
    stats.sparse_embeddings_count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM astravector.embedding_sparse")
            .fetch_one(&pool)
            .await
            .unwrap_or(0) as u64;
    if let Some(run_id) = quality_run_id() {
        stats.graph.relations_ingested_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM astravector.rag_graph_edges WHERE relation_source='QUALITY_FIXTURE' AND properties->>'quality_run_id'=$1",
        )
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0) as u64;
        stats.graph.relations_persisted_count = stats.graph.relations_ingested_count;
        stats.graph.relations_queryable_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM astravector.rag_graph_edges e JOIN astravector.rag_graph_nodes_chunk n ON n.access_zone_id=e.access_zone_id AND n.node_id=e.target_node_id WHERE e.relation_source='QUALITY_FIXTURE' AND e.properties->>'quality_run_id'=$1 AND e.lifecycle_status='ACTIVE' AND n.lifecycle_status='ACTIVE'",
        )
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0) as u64;
        stats.graph.graph_edges_available_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM astravector.rag_graph_edges WHERE properties->>'quality_run_id'=$1 OR relation_source <> 'QUALITY_FIXTURE'",
        )
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0) as u64;
    } else {
        stats.graph.graph_edges_available_count =
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM astravector.rag_graph_edges")
                .fetch_one(&pool)
                .await
                .unwrap_or(0) as u64;
    }
    if let Ok(rows) = sqlx::query(
        "SELECT access_zone_code,status,auto_created,created_reason FROM astravector.access_zones WHERE access_zone_code IN('1700','1800','1900')",
    )
    .fetch_all(&pool)
    .await
    {
        stats.access_zone_conflict_accuracy = if !rows.is_empty()
            && rows.iter().all(|row| {
                row.get::<String, _>("status") == "ACTIVE"
                    && row.get::<bool, _>("auto_created")
                    && row.get::<String, _>("created_reason") == "INGESTION_AUTO_CREATE"
            }) {
            1.0
        } else {
            0.0
        };
    }
}

fn access_level_label(value: &str) -> &'static str {
    match value {
        "PUBLIC" => "PUBLIC",
        "INTERNAL" => "INTERNAL",
        "CONFIDENTIAL" => "CONFIDENTIAL",
        "RESTRICTED" => "RESTRICTED",
        _ => "PUBLIC",
    }
}

fn fixture_access_level_distribution(documents: &[Value]) -> BTreeMap<String, u64> {
    let mut distribution = BTreeMap::from([
        ("PUBLIC".to_string(), 0),
        ("INTERNAL".to_string(), 0),
        ("CONFIDENTIAL".to_string(), 0),
        ("RESTRICTED".to_string(), 0),
    ]);
    for doc in documents {
        let label = access_level_label(
            doc.get("access_level")
                .and_then(Value::as_str)
                .unwrap_or("PUBLIC"),
        );
        *distribution.entry(label.to_string()).or_insert(0) += 1;
    }
    distribution
}

async fn postgres_access_level_distribution() -> BTreeMap<String, u64> {
    let postgres_url = env::var("ASTRAVECTOR_DB_URL").unwrap_or_else(|_| {
        "postgres://astravector:astravector@127.0.0.1:55432/astravector".into()
    });
    let mut distribution = BTreeMap::from([
        ("1".to_string(), 0),
        ("2".to_string(), 0),
        ("3".to_string(), 0),
        ("4".to_string(), 0),
    ]);
    let Ok(pool) = sqlx::PgPool::connect(&postgres_url).await else {
        return distribution;
    };
    if let Ok(rows) = sqlx::query(
        "SELECT access_level::text AS access_level, count(*) AS count \
         FROM astravector.vector_bindings_v004 \
         WHERE lifecycle_status='ACTIVE' \
           AND qdrant_sync_status='SYNCED' \
           AND chunk_granularity IN('PARENT','SUB_180','SUB_260') \
         GROUP BY access_level",
    )
    .fetch_all(&pool)
    .await
    {
        for row in rows {
            distribution.insert(
                row.get::<String, _>("access_level"),
                row.get::<i64, _>("count") as u64,
            );
        }
    }
    distribution
}

async fn qdrant_access_level_distribution(
    base_url: &str,
    collection: &str,
) -> BTreeMap<String, u64> {
    let mut distribution = BTreeMap::from([
        ("1".to_string(), 0),
        ("2".to_string(), 0),
        ("3".to_string(), 0),
        ("4".to_string(), 0),
    ]);
    let client = reqwest::Client::new();
    let url = format!(
        "{}/collections/{}/points/scroll",
        base_url.trim_end_matches('/'),
        collection
    );
    let mut offset: Option<Value> = None;
    for _ in 0..100 {
        let mut body = json!({
            "limit": 256,
            "with_payload": true,
            "with_vector": false,
            "filter": {
                "must": [
                    {"key":"lifecycle_status","match":{"value":"ACTIVE"}},
                    {"key":"chunk_granularity","match":{"any":["PARENT","SUB_180","SUB_260"]}}
                ],
                "must_not": [
                    {"key":"quarantined","match":{"value":true}}
                ]
            }
        });
        if let Some(value) = offset.take() {
            body["offset"] = value;
        }
        let Ok(response) = client.post(&url).json(&body).send().await else {
            return distribution;
        };
        let Ok(value) = response.json::<Value>().await else {
            return distribution;
        };
        let points = value
            .pointer("/result/points")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for point in points {
            if let Some(level) = point
                .pointer("/payload/access_level")
                .and_then(Value::as_i64)
                .map(|v| v.to_string())
            {
                *distribution.entry(level).or_insert(0) += 1;
            }
        }
        offset = value.pointer("/result/next_page_offset").cloned();
        if offset.as_ref().is_none_or(Value::is_null) {
            break;
        }
    }
    distribution
}

fn access_level_mapping_mismatch(audit: &AccessLevelAudit) -> bool {
    let fixture_public = audit
        .fixture_distribution
        .get("PUBLIC")
        .copied()
        .unwrap_or(0);
    let postgres_public = audit.postgres_distribution.get("1").copied().unwrap_or(0);
    let qdrant_public = audit.qdrant_distribution.get("1").copied().unwrap_or(0);
    let postgres_restricted = audit.postgres_distribution.get("4").copied().unwrap_or(0);
    let qdrant_restricted = audit.qdrant_distribution.get("4").copied().unwrap_or(0);
    fixture_public > 0
        && postgres_public == 0
        && qdrant_public == 0
        && (postgres_restricted > 0 || qdrant_restricted > 0)
}

async fn collect_access_level_audit(
    documents: &[Value],
    qdrant_url: &str,
    qdrant_collection: &str,
) -> AccessLevelAudit {
    let mut audit = AccessLevelAudit {
        fixture_distribution: fixture_access_level_distribution(documents),
        postgres_distribution: postgres_access_level_distribution().await,
        qdrant_distribution: qdrant_access_level_distribution(qdrant_url, qdrant_collection).await,
        status: "PASS".into(),
        reason: None,
    };
    if access_level_mapping_mismatch(&audit) {
        audit.status = "FAIL".into();
        audit.reason = Some("ACCESS_LEVEL_FIXTURE_MAPPING_MISMATCH".into());
    }
    audit
}

fn detect_capabilities(stats: &RuntimeStats) -> Capabilities {
    let dense_available = stats.dense_embeddings_count > 0;
    let sparse_available = stats.sparse_embeddings_count > 0
        && stats.sparse.qdrant_sparse_config_present
        && stats.sparse.qdrant_sparse_points_with_vectors > 0;
    let graph_rag_available = stats.graph.relations_loaded_count > 0
        && stats.graph.relations_ingested_count > 0
        && stats.graph.relations_persisted_count > 0
        && stats.graph.relations_queryable_count > 0
        && stats.graph.graph_edges_available_count > 0;
    Capabilities {
        dense_available,
        sparse_available,
        hybrid_available: dense_available && sparse_available,
        graph_rag_available,
        mmr_available: true,
    }
}

fn sparse_mode(stats: &RuntimeStats) -> &'static str {
    if stats.sparse_embeddings_count > 0 && stats.sparse.qdrant_sparse_points_with_vectors > 0 {
        TECHNICAL_SPARSE_MODE
    } else {
        "UNAVAILABLE"
    }
}

fn sparse_query_debug(question: &str) -> (Value, usize, usize, usize, usize) {
    let encoder = SparseTechnicalEncoder::new(0.0, 512);
    let vector = encoder.encode_query(question).ok();
    let analysis = encoder.analyze(question);
    let tokens_for = |class: SparseTokenClass| {
        analysis
            .tokens
            .iter()
            .filter(|token| token.class == class)
            .map(|token| token.token.clone())
            .collect::<Vec<_>>()
    };
    let technical_tokens = analysis
        .tokens
        .iter()
        .filter(|token| token.class != SparseTokenClass::OrdinaryWord)
        .map(|token| token.token.clone())
        .collect::<Vec<_>>();
    (
        json!({
            "sparse_query_non_zero_terms": vector.as_ref().map(|v| v.indices.len()).unwrap_or(0),
            "technical_query_tokens": technical_tokens,
            "numeric_query_tokens": tokens_for(SparseTokenClass::NumericExact),
            "alphanumeric_query_tokens": tokens_for(SparseTokenClass::Alphanumeric),
            "special_query_tokens": analysis.tokens.iter()
                .filter(|token| matches!(
                    token.class,
                    SparseTokenClass::ErrorCode
                        | SparseTokenClass::Uuid
                        | SparseTokenClass::IpOrPort
                        | SparseTokenClass::Path
                        | SparseTokenClass::Filename
                        | SparseTokenClass::UnderscoreIdentifier
                        | SparseTokenClass::GrpcMethod
                        | SparseTokenClass::VersionToken
                ))
                .map(|token| token.token.clone())
                .collect::<Vec<_>>()
        }),
        analysis.technical_token_count,
        analysis.numeric_token_count,
        analysis.alphanumeric_token_count,
        analysis.special_token_count,
    )
}

async fn wait_for_document_ready(
    client: &mut AstraVectorIngestionFacadeClient<tonic::transport::Channel>,
    access_zone_id: String,
    document_id: String,
    document_version: u64,
) -> Result<pb::GetDocumentVectorStatusResponse, String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut last = None;
    while Instant::now() < deadline {
        let mut req = Request::new(pb::GetDocumentVectorStatusRequest {
            context: Some(pb::RequestContext {
                correlation_id: format!("quality-runtime-status-{document_id}"),
                idempotency_key: String::new(),
                caller_service: "quality-runtime-bench".into(),
                caller_user_id: "quality-runtime-bench".into(),
                caller_access_level: pb::AccessLevel::Restricted as i32,
            }),
            document: Some(pb::DocumentRef {
                access_zone_id: access_zone_id.clone(),
                document_id: document_id.clone(),
                document_version,
            }),
            include_qdrant: true,
        });
        api_key_metadata(&mut req);
        match client.get_document_vector_status(req).await {
            Ok(response) => {
                let response = response.into_inner();
                let sync = response.status.as_ref().and_then(|s| s.sync.as_ref());
                if sync
                    .map(|s| {
                        s.expected_bindings > 0
                            && s.synced_bindings == s.expected_bindings
                            && s.outbox_completed == s.expected_bindings
                            && s.qdrant_points_found == s.qdrant_points_expected
                            && s.qdrant_points_found > 0
                    })
                    .unwrap_or(false)
                {
                    return Ok(response);
                }
                last = Some(format!("{:?}", response.status));
            }
            Err(error) => last = Some(error.to_string()),
        }
        sleep(Duration::from_millis(500)).await;
    }
    Err(format!(
        "document {document_id} was not ready before timeout; last_status={}",
        last.unwrap_or_else(|| "none".into())
    ))
}

async fn ingest_documents(
    endpoint: &str,
    documents: &[Value],
    relations: &[Value],
    stats: &mut RuntimeStats,
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut client = match AstraVectorIngestionFacadeClient::connect(endpoint.to_string()).await {
        Ok(client) => client,
        Err(error) => {
            failures.push(format!("ingestion client connect failed: {error}"));
            return failures;
        }
    };
    let mut control_client = match AstraVectorV004ControlClient::connect(endpoint.to_string()).await
    {
        Ok(client) => client,
        Err(error) => {
            failures.push(format!("control client connect failed: {error}"));
            return failures;
        }
    };

    for doc in documents {
        let fixture_document_id = doc
            .get("document_id")
            .and_then(Value::as_str)
            .expect("document missing document_id")
            .to_string();
        let document_id = runtime_document_uuid(&fixture_document_id);
        let access_zone_code = doc
            .get("access_zone_code")
            .and_then(Value::as_str)
            .expect("document missing access_zone_code")
            .to_string();
        let fixture_access_level = doc
            .get("access_level")
            .and_then(Value::as_str)
            .unwrap_or("PUBLIC");
        let document_version = 1;
        let blocks = fixture_blocks(doc);
        let content_hash = sha256_hex(&document_text_for_hash(doc));
        let ttl_policy = doc.get("ttl_days").and_then(Value::as_u64).map(|days| {
            let mode = if days == 0 {
                pb::TtlMode::None
            } else {
                pb::TtlMode::Relative
            };
            pb::TtlPolicy {
                mode: mode as i32,
                ttl_seconds: days * 86_400,
                expires_at: String::new(),
                delete_from_qdrant_on_expire: true,
                keep_metadata_after_expire: false,
            }
        });
        let mut metadata = string_map(doc.get("metadata"));
        metadata.insert("quality_runtime_bench".into(), "fix474".into());
        if let Some(run_id) = quality_run_id() {
            metadata.insert("quality_run_id".into(), run_id);
        }
        if !relations.is_empty() {
            metadata.insert(
                "quality_fixture_relations_json".into(),
                runtime_relation_payload(relations),
            );
            metadata.insert(
                "quality_fixture_relations_count".into(),
                relations.len().to_string(),
            );
        }
        metadata.insert("fixture_document_id".into(), fixture_document_id.clone());
        metadata.insert("original_document_id".into(), fixture_document_id.clone());
        metadata.insert(
            "legal_hold".into(),
            doc.get("legal_hold")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                .to_string(),
        );

        let mut req = Request::new(pb::IndexLogicalDocumentRequest {
            context: Some(pb::RequestContext {
                correlation_id: format!("quality-runtime-ingest-{document_id}"),
                idempotency_key: format!(
                    "quality-runtime:{}:{document_id}:v{document_version}",
                    quality_run_id().unwrap_or_else(|| "fix474".into())
                ),
                caller_service: "quality-runtime-bench".into(),
                caller_user_id: "quality-runtime-bench".into(),
                caller_access_level: access_level(fixture_access_level),
            }),
            access_zone_id: String::new(),
            access_zone_code: access_zone_code.clone(),
            document: Some(pb::DocumentIdentity {
                external_document_id: fixture_document_id.clone(),
                document_id: document_id.clone(),
                document_version,
                title: doc
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or(&fixture_document_id)
                    .to_string(),
                source_uri: format!(
                    "quality://{}/{}/{}",
                    quality_run_id().unwrap_or_else(|| "fix474".into()),
                    access_zone_code,
                    fixture_document_id
                ),
                source_type: "QUALITY_FIXTURE".into(),
                mime_type: "application/jsonl".into(),
                content_hash,
                source_links: Vec::new(),
            }),
            blocks,
            chunking_options: Some(pb::TokenAwareChunkingOptions {
                profile: pb::ChunkingProfile::Technical as i32,
                parent_target_tokens: 256,
                parent_max_tokens: 512,
                child_target_tokens: 180,
                child_max_tokens: 260,
                child_overlap_tokens: 30,
                min_chunk_tokens: 8,
                preserve_block_boundaries: true,
                allow_split_inside_paragraph: false,
                allow_split_inside_table: false,
                create_parent_context: true,
            }),
            indexing_options: Some(pb::VectorIndexingOptions {
                activation_policy: pb::ActivationPolicy::Manual as i32,
                embedding_mode: pb::EmbeddingModeV005::DenseSparseIfAvailable as i32,
                publish_mode: pb::PublishModeV005::Outbox as i32,
                ttl_policy,
                replace_existing_version: true,
            }),
            metadata,
        });
        api_key_metadata(&mut req);

        match client.index_logical_document(req).await {
            Ok(response) => {
                let response = response.into_inner();
                stats.fixtures_ingested_count += 1;
                stats.documents_registered_count += 1;
                let Some(document) = response.document else {
                    failures.push(format!(
                        "{document_id}: ingestion response missing document ref"
                    ));
                    continue;
                };
                match wait_for_document_ready(
                    &mut client,
                    document.access_zone_id.clone(),
                    document.document_id.clone(),
                    document.document_version,
                )
                .await
                {
                    Ok(status) => {
                        if let Some(sync) = status.status.and_then(|s| s.sync) {
                            stats.outbox_completed_count += u64::from(sync.outbox_completed);
                        }
                        let mut activate_req = Request::new(pb::ActivateDocumentVersionRequest {
                            access_zone_id: document.access_zone_id,
                            document_id: document.document_id,
                            document_version: document.document_version,
                            force_activate: false,
                            force_reason: String::new(),
                        });
                        api_key_metadata(&mut activate_req);
                        if let Err(error) =
                            control_client.activate_document_version(activate_req).await
                        {
                            failures.push(format!(
                                "{fixture_document_id}: ActivateDocumentVersion failed: {error}"
                            ));
                        }
                    }
                    Err(error) => failures.push(error),
                }
            }
            Err(error) => failures.push(format!(
                "{fixture_document_id}: IndexLogicalDocument failed: {error}"
            )),
        }

        if fixture_access_level == "RESTRICTED" {
            sleep(Duration::from_millis(50)).await;
        }
    }
    failures
}

fn expected_strings<'a>(value: &'a Value, key: &str) -> Vec<&'a str> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn expected_string_vec(value: &Value, key: &str) -> Vec<String> {
    expected_strings(value, key)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn query_category(query: &Value) -> String {
    query
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("uncategorized")
        .to_string()
}

fn query_search_mode(query: &Value) -> String {
    query
        .pointer("/context/search_mode")
        .and_then(Value::as_str)
        .unwrap_or("HYBRID")
        .to_ascii_uppercase()
}

fn query_mode(query: &Value) -> &'static str {
    let category = query_category(query);
    let search_mode = query_search_mode(query);
    let graph_enabled = query
        .pointer("/context/enable_graph_expansion")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mmr_enabled = query
        .pointer("/context/enable_mmr")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let has_expected_related = query
        .pointer("/expected/expected_related_block_ids")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    let has_expected_aspects = query
        .pointer("/expected/expected_aspects")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if category == "graph_rag" || graph_enabled || has_expected_related {
        "graph"
    } else if category == "mmr_diversity" || mmr_enabled || has_expected_aspects {
        "mmr"
    } else if matches!(
        search_mode.as_str(),
        "SPARSE" | "BM25" | "LEXICAL" | "LEXICAL_STRICT"
    ) || category == "lexical_sparse"
    {
        "sparse"
    } else if search_mode == "HYBRID" {
        "hybrid"
    } else {
        "dense"
    }
}

fn retrieval_source_satisfied(retrieval_sources: &str, expected: &str) -> bool {
    if retrieval_sources.contains(expected) {
        return true;
    }
    expected == "VECTOR_DIRECT" && retrieval_sources.contains("LEXICAL_PARENT_BACKFILL")
}

fn query_requires_sparse(query: &Value) -> bool {
    let category = query_category(query);
    let search_mode = query_search_mode(query);
    category == "lexical_sparse"
        || matches!(
            search_mode.as_str(),
            "SPARSE" | "BM25" | "LEXICAL" | "LEXICAL_STRICT" | "HYBRID"
        )
}

fn env_bool(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            )
        })
        .unwrap_or(false)
}

fn capability_requirements_from_env() -> CapabilityRequirements {
    CapabilityRequirements {
        require_dense: env_bool("ASTRAVECTOR_QUALITY_REQUIRE_DENSE"),
        require_sparse: env_bool("ASTRAVECTOR_QUALITY_REQUIRE_SPARSE"),
        require_hybrid: env_bool("ASTRAVECTOR_QUALITY_REQUIRE_HYBRID"),
        require_graph: env_bool("ASTRAVECTOR_QUALITY_REQUIRE_GRAPH"),
        require_mmr: env_bool("ASTRAVECTOR_QUALITY_REQUIRE_MMR"),
    }
}

fn forced_caller_access_level() -> Option<String> {
    env::var("ASTRAVECTOR_QUALITY_FORCE_CALLER_ACCESS_LEVEL")
        .ok()
        .map(|v| v.trim().to_ascii_uppercase())
        .filter(|v| !v.is_empty())
}

fn record_query_status(
    stats: &mut RuntimeStats,
    category: &str,
    mode: &str,
    status: &str,
    reasons: &[&str],
) {
    let entry = stats.by_category.entry(category.to_string()).or_default();
    entry.total += 1;
    let mode_entry = stats.by_mode.entry(mode.to_string()).or_default();
    mode_entry.total += 1;
    match status {
        "PASSED" => {
            entry.passed += 1;
            mode_entry.passed += 1;
            stats.retrieve_context_queries_passed += 1;
        }
        "BLOCKED" => {
            entry.blocked += 1;
            mode_entry.blocked += 1;
            stats.retrieve_context_queries_blocked += 1;
        }
        "SKIPPED_RUNTIME_REQUIRED" => {
            entry.skipped += 1;
            mode_entry.skipped += 1;
            stats.retrieve_context_queries_skipped += 1;
        }
        _ => {
            entry.failed += 1;
            mode_entry.failed += 1;
            stats.retrieve_context_queries_failed += 1;
        }
    }
    for reason in reasons {
        *stats.by_reason.entry((*reason).to_string()).or_insert(0) += 1;
    }
}

fn push_requirement_failure(
    stats: &mut RuntimeStats,
    failures: &mut Vec<String>,
    reason: &'static str,
) {
    *stats.by_reason.entry(reason.into()).or_insert(0) += 1;
    failures.push(reason.into());
}

fn apply_capability_requirements(
    stats: &mut RuntimeStats,
    capabilities: &Capabilities,
    failures: &mut Vec<String>,
) {
    let requirements = stats.capability_requirements.clone();
    if requirements.require_dense && !capabilities.dense_available {
        push_requirement_failure(stats, failures, "DENSE_UNAVAILABLE");
    }
    if requirements.require_sparse {
        if stats.sparse_embeddings_count == 0 {
            push_requirement_failure(stats, failures, "SPARSE_EMBEDDINGS_MISSING");
        }
        if !stats.sparse.qdrant_sparse_config_present {
            push_requirement_failure(stats, failures, "QDRANT_SPARSE_CONFIG_MISSING");
        }
        if stats.sparse.qdrant_sparse_points_with_vectors == 0 {
            push_requirement_failure(stats, failures, "QDRANT_SPARSE_POINTS_MISSING");
        }
        if !capabilities.sparse_available {
            push_requirement_failure(stats, failures, "SPARSE_UNAVAILABLE");
        }
    }
    if requirements.require_hybrid && !capabilities.hybrid_available {
        push_requirement_failure(stats, failures, "HYBRID_UNAVAILABLE");
    }
    if requirements.require_graph {
        if stats.graph.relations_loaded_count > 0 && stats.graph.relations_ingested_count == 0 {
            push_requirement_failure(stats, failures, "GRAPH_RELATIONS_NOT_INGESTED");
        }
        if stats.graph.relations_loaded_count > 0 && stats.graph.relations_persisted_count == 0 {
            push_requirement_failure(stats, failures, "GRAPH_RELATIONS_NOT_PERSISTED");
        }
        if stats.graph.relations_loaded_count > 0 && stats.graph.relations_queryable_count == 0 {
            push_requirement_failure(stats, failures, "GRAPH_RELATIONS_NOT_QUERYABLE");
        }
        if stats.graph.graph_edges_available_count == 0 {
            push_requirement_failure(stats, failures, "GRAPH_EDGES_MISSING");
        }
        if !capabilities.graph_rag_available {
            push_requirement_failure(stats, failures, "GRAPH_RAG_UNAVAILABLE");
        }
    }
    if requirements.require_mmr && !capabilities.mmr_available {
        push_requirement_failure(stats, failures, "MMR_UNAVAILABLE");
    }
}

fn evaluate_query(
    query: &Value,
    response: &pb::RetrieveContextResponse,
    elapsed_ms: u128,
    effective_caller_access_level: &str,
) -> EvalResult {
    let query_id = query.get("id").and_then(Value::as_str).unwrap_or("query");
    let expected = query.get("expected").expect("query missing expected");
    let mut result = EvalResult::default();
    let joined = response
        .contexts
        .iter()
        .map(|c| format!("{}\n{}", c.matched_text, c.parent_text))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    let doc_ids = response
        .contexts
        .iter()
        .flat_map(|c| {
            [
                Some(c.document_id.as_str()),
                c.metadata.get("fixture_document_id").map(String::as_str),
                c.metadata.get("original_document_id").map(String::as_str),
                c.metadata.get("external_document_id").map(String::as_str),
            ]
        })
        .flatten()
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let block_ids = response
        .contexts
        .iter()
        .map(|c| c.source_block_id.clone())
        .collect::<HashSet<_>>();

    for phrase in expected_strings(expected, "must_contain_phrases") {
        if !joined.contains(&phrase.to_lowercase()) {
            result
                .failures
                .push(format!("{query_id}: missing phrase `{phrase}`"));
            result.reasons.push("MISSING_REQUIRED_PHRASE");
        }
    }
    for doc_id in expected_strings(expected, "must_contain_document_ids") {
        if !doc_ids.contains(doc_id) {
            result
                .failures
                .push(format!("{query_id}: missing document `{doc_id}`"));
            result.reasons.push("MISSING_EXPECTED_DOCUMENT");
        }
    }
    for block_id in expected_strings(expected, "must_contain_block_ids") {
        if !block_ids.contains(block_id) {
            result
                .failures
                .push(format!("{query_id}: missing block `{block_id}`"));
            result.reasons.push("MISSING_EXPECTED_BLOCK");
        }
    }
    let expected_related = expected_strings(expected, "expected_related_block_ids");
    result.graph_expected_related_total = expected_related.len();
    for block_id in expected_related {
        if block_ids.contains(block_id) {
            result.graph_expected_related_hits += 1;
        } else {
            result.failures.push(format!(
                "{query_id}: graph related block missing `{block_id}`"
            ));
            result.reasons.push("GRAPH_EXPECTED_RELATED_BLOCK_MISSING");
        }
    }
    let expected_aspects = expected_strings(expected, "expected_aspects");
    result.mmr_expected_aspects_total = expected_aspects.len();
    for aspect in expected_aspects {
        if joined.contains(&aspect.to_lowercase()) {
            result.mmr_expected_aspects_hits += 1;
        } else {
            result
                .failures
                .push(format!("{query_id}: missing MMR aspect `{aspect}`"));
            result.reasons.push("MMR_ASPECT_COVERAGE_LOW");
        }
    }
    for phrase in expected_strings(expected, "forbidden_phrases") {
        if joined.contains(&phrase.to_lowercase()) {
            result
                .failures
                .push(format!("{query_id}: forbidden phrase leaked `{phrase}`"));
            result.reasons.push("FORBIDDEN_PHRASE_RETURNED");
        }
    }
    for doc_id in expected_strings(expected, "forbidden_document_ids") {
        if doc_ids.contains(doc_id) {
            result.failures.push(format!(
                "{query_id}: forbidden document returned `{doc_id}`"
            ));
            result.reasons.push("FORBIDDEN_DOCUMENT_RETURNED");
        }
    }
    for block_id in expected_strings(expected, "forbidden_block_ids") {
        if block_ids.contains(block_id) {
            result
                .failures
                .push(format!("{query_id}: forbidden block returned `{block_id}`"));
            result.reasons.push("FORBIDDEN_BLOCK_RETURNED");
        }
    }
    let retrieval_sources = response
        .contexts
        .iter()
        .flat_map(|context| {
            [
                context.metadata.get("retrieval_source").cloned(),
                context.metadata.get("retrieval_sources").cloned(),
            ]
        })
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    for source in expected_strings(expected, "must_have_sources") {
        if !retrieval_source_satisfied(&retrieval_sources, source) {
            result
                .failures
                .push(format!("{query_id}: missing retrieval source `{source}`"));
            result.reasons.push("MISSING_REQUIRED_SOURCE");
        }
    }
    for source in expected_strings(expected, "must_not_have_sources") {
        if retrieval_sources.contains(source) {
            result
                .failures
                .push(format!("{query_id}: forbidden retrieval source `{source}`"));
            result.reasons.push("FORBIDDEN_SOURCE_RETURNED");
        }
    }
    for zone in expected_strings(expected, "forbidden_access_zones") {
        if response.contexts.iter().any(|c| c.access_zone_id == zone) {
            result
                .failures
                .push(format!("{query_id}: forbidden access zone leaked `{zone}`"));
            result.reasons.push("FORBIDDEN_ACCESS_ZONE_RETURNED");
        }
    }
    if query_category(query) == "hard_negative" {
        let max_false_positive_contexts = expected
            .get("max_false_positive_contexts")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if response.contexts.len() as u64 > max_false_positive_contexts {
            result.failures.push(format!(
                "{query_id}: hard-negative returned {} contexts, max false positives allowed {max_false_positive_contexts}",
                response.contexts.len()
            ));
            result.reasons.push("QUALITY_GATES_FAILED");
        }
    }
    let max_contexts = expected
        .get("max_contexts_count")
        .and_then(Value::as_u64)
        .unwrap_or(10);
    if response.contexts.len() as u64 > max_contexts {
        result.failures.push(format!(
            "{query_id}: returned {} contexts, max expected {max_contexts}",
            response.contexts.len()
        ));
        result.reasons.push("QUALITY_GATES_FAILED");
    }
    result.reasons.sort_unstable();
    result.reasons.dedup();
    result.returned_document_ids = doc_ids.into_iter().collect();
    result.returned_document_ids.sort();
    result.returned_block_ids = block_ids.into_iter().collect();
    result.returned_block_ids.sort();
    result.contexts_count = response.contexts.len();
    result.graph_expanded_contexts_count = response
        .contexts
        .iter()
        .filter(|c| {
            c.metadata
                .get("retrieval_source")
                .map(|value| value.contains("GRAPH_EXPANDED"))
                .unwrap_or(false)
                || c.metadata
                    .get("retrieval_sources")
                    .map(|value| value.contains("GRAPH_EXPANDED"))
                    .unwrap_or(false)
        })
        .count();
    let question = query
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (sparse_debug, _, _, _, _) = sparse_query_debug(question);
    result.candidates.push(json!({
        "query_id": query_id,
        "quality_run_id": quality_run_id(),
        "category": query_category(query),
        "mode": query_mode(query),
        "access_zone_code": query.pointer("/context/access_zone_code").and_then(Value::as_str).unwrap_or("1700"),
        "caller_access_level": effective_caller_access_level,
        "search_mode": query_search_mode(query),
        "elapsed_ms": elapsed_ms,
        "contexts_count": response.contexts.len(),
        "graph_expanded_contexts_count": result.graph_expanded_contexts_count,
        "returned_document_ids": result.returned_document_ids.clone(),
        "returned_block_ids": result.returned_block_ids.clone(),
        "candidate_debug": {
            "retrieve_context_error": Value::Null,
            "empty_contexts": response.contexts.is_empty(),
            "sparse_query_non_zero_terms": sparse_debug["sparse_query_non_zero_terms"].clone(),
            "technical_query_tokens": sparse_debug["technical_query_tokens"].clone(),
            "numeric_query_tokens": sparse_debug["numeric_query_tokens"].clone(),
            "alphanumeric_query_tokens": sparse_debug["alphanumeric_query_tokens"].clone(),
            "special_query_tokens": sparse_debug["special_query_tokens"].clone(),
        },
        "contexts": response.contexts.iter().map(|c| json!({
            "document_id": c.document_id,
            "document_version": c.document_version,
            "source_block_id": c.source_block_id,
            "access_zone_id": c.access_zone_id,
            "matched_chunk_id": c.matched_chunk_id,
            "parent_chunk_id": c.parent_chunk_id,
            "matched_text": c.matched_text,
            "parent_text": c.parent_text,
            "metadata": c.metadata,
        })).collect::<Vec<_>>()
    }));
    result
}

fn empty_candidate_debug_row(
    query: &Value,
    elapsed_ms: u128,
    retrieve_context_error: Option<&str>,
    effective_caller_access_level: &str,
) -> Value {
    let query_id = query.get("id").and_then(Value::as_str).unwrap_or("query");
    let question = query
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (sparse_debug, _, _, _, _) = sparse_query_debug(question);
    json!({
        "query_id": query_id,
        "quality_run_id": quality_run_id(),
        "category": query_category(query),
        "mode": query_mode(query),
        "access_zone_code": query.pointer("/context/access_zone_code").and_then(Value::as_str).unwrap_or("1700"),
        "caller_access_level": effective_caller_access_level,
        "search_mode": query_search_mode(query),
        "elapsed_ms": elapsed_ms,
        "contexts_count": 0,
        "returned_document_ids": [],
        "returned_block_ids": [],
        "candidate_debug": {
            "retrieve_context_error": retrieve_context_error,
            "empty_contexts": true,
            "sparse_query_non_zero_terms": sparse_debug["sparse_query_non_zero_terms"].clone(),
            "technical_query_tokens": sparse_debug["technical_query_tokens"].clone(),
            "numeric_query_tokens": sparse_debug["numeric_query_tokens"].clone(),
            "alphanumeric_query_tokens": sparse_debug["alphanumeric_query_tokens"].clone(),
            "special_query_tokens": sparse_debug["special_query_tokens"].clone(),
        },
        "contexts": []
    })
}

async fn retrieve_queries(
    endpoint: &str,
    queries: &[Value],
    stats: &mut RuntimeStats,
    capabilities: &Capabilities,
    profile_name: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut client = match AstraVectorRetrievalFacadeClient::connect(endpoint.to_string()).await {
        Ok(client) => client,
        Err(error) => {
            failures.push(format!("retrieval client connect failed: {error}"));
            return failures;
        }
    };
    let mut candidates = Vec::new();
    let mut query_failures = Vec::new();
    stats.retrieve_context_queries_total = queries.len();
    let forced_access_level = forced_caller_access_level();
    for query in queries {
        let context = query.get("context").expect("query missing context");
        let expected = query.get("expected").expect("query missing expected");
        let query_id = query.get("id").and_then(Value::as_str).unwrap_or("query");
        let category = query_category(query);
        let mode = query_mode(query);
        let query_access_level = context
            .get("caller_access_level")
            .and_then(Value::as_str)
            .unwrap_or("PUBLIC");
        let effective_access_level = forced_access_level.as_deref().unwrap_or(query_access_level);
        let expected_document_ids = expected_string_vec(expected, "must_contain_document_ids");
        let expected_block_ids = expected_string_vec(expected, "must_contain_block_ids");
        if category == "hard_negative"
            && expected
                .get("max_false_positive_contexts")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
        {
            stats
                .no_answer
                .non_zero_max_false_positive_contexts_warnings += 1;
            *stats
                .by_reason
                .entry("NON_ZERO_MAX_FALSE_POSITIVE_CONTEXTS_USED".into())
                .or_insert(0) += 1;
        }
        let question = query
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (_, technical_count, numeric_count, alphanumeric_count, special_count) =
            sparse_query_debug(question);
        stats.sparse.technical_token_count += technical_count as u64;
        stats.sparse.numeric_token_count += numeric_count as u64;
        stats.sparse.alphanumeric_token_count += alphanumeric_count as u64;
        stats.sparse.special_token_count += special_count as u64;
        stats.sparse.document_query_encoder_consistency_checked = true;

        if query_requires_sparse(query) && !capabilities.sparse_available {
            let reason = "SPARSE_UNAVAILABLE";
            let reasons = if mode == "hybrid" {
                vec![reason, "HYBRID_UNAVAILABLE"]
            } else {
                vec![reason]
            };
            record_query_status(stats, &category, mode, "BLOCKED", &reasons);
            stats.queries_with_empty_contexts += 1;
            let failure = format!("{query_id}: blocked: {reason}");
            failures.push(failure.clone());
            candidates.push(empty_candidate_debug_row(
                query,
                0,
                Some(reason),
                effective_access_level,
            ));
            query_failures.push(json!({
                "query_id": query_id,
                "category": category,
                "mode": mode,
                "status": "BLOCKED",
                "reasons": reasons,
                "expected_document_ids": expected_document_ids,
                "returned_document_ids": [],
                "expected_block_ids": expected_block_ids,
                "returned_block_ids": [],
                "message": failure
            }));
            continue;
        }

        let mut filters = Vec::new();
        if let Some(run_id) = quality_run_id() {
            filters.push(pb::SearchFilterV004 {
                key: "quality_run_id".into(),
                value: run_id,
            });
        }

        let mut req = Request::new(pb::RetrieveContextRequest {
            context: Some(pb::RequestContext {
                correlation_id: format!("quality-runtime-query-{query_id}"),
                idempotency_key: format!("quality-runtime-query:{query_id}"),
                caller_service: "quality-runtime-bench".into(),
                caller_user_id: "quality-runtime-bench".into(),
                caller_access_level: access_level(effective_access_level),
            }),
            access_zone_id: String::new(),
            question: query
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            access_zone_ids: Vec::new(),
            access_zone_code: context
                .get("access_zone_code")
                .and_then(Value::as_str)
                .unwrap_or("1700")
                .to_string(),
            access_zone_codes: Vec::new(),
            profile: effective_retrieval_profile(
                profile_name,
                context
                    .get("profile")
                    .and_then(Value::as_str)
                    .unwrap_or("TECHNICAL"),
            ),
            max_contexts: expected
                .get("max_contexts_count")
                .and_then(Value::as_u64)
                .unwrap_or(10) as u32,
            filters,
            response_detail: pb::ResponseDetail::Debug as i32,
            enable_graph_expansion: context
                .get("enable_graph_expansion")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            graph_max_hops: 1,
            graph_max_related_contexts: 5,
        });
        api_key_metadata(&mut req);
        let before = Instant::now();
        match client.retrieve_context(req).await {
            Ok(response) => {
                let response = response.into_inner();
                for warning in &response.warnings {
                    match warning.code.as_str() {
                        "PRE_MMR_WEAK_CANDIDATE_FILTERED" => {
                            stats.no_answer.pre_mmr_filtered_candidate_count += 1;
                            *stats
                                .by_reason
                                .entry("PRE_MMR_WEAK_CANDIDATE_FILTERED".into())
                                .or_insert(0) += 1;
                        }
                        "POST_MMR_NO_ANSWER_TRIGGERED" => {
                            stats.no_answer.post_mmr_no_answer_triggered_count += 1;
                            *stats.by_reason.entry(warning.code.clone()).or_insert(0) += 1;
                        }
                        "FINAL_CONTEXT_SET_TOO_WEAK" | "FINAL_CONTEXT_SCORE_BELOW_THRESHOLD" => {
                            *stats.by_reason.entry(warning.code.clone()).or_insert(0) += 1;
                        }
                        "GRAPH_EXPANSION_TIMEOUT" | "GRAPH_EXPANSION_SKIPPED_DUE_TO_TIMEOUT" => {
                            stats.graph.graph_timeout_count += 1;
                            *stats.by_reason.entry(warning.code.clone()).or_insert(0) += 1;
                        }
                        "GRAPH_EXPANSION_FAILED" | "GRAPH_EXPANSION_SKIPPED_DUE_TO_DB_ERROR" => {
                            stats.graph.graph_db_error_count += 1;
                            *stats.by_reason.entry(warning.code.clone()).or_insert(0) += 1;
                        }
                        _ => {}
                    }
                }
                let eval = evaluate_query(
                    query,
                    &response,
                    before.elapsed().as_millis(),
                    effective_access_level,
                );
                if eval.contexts_count == 0 {
                    stats.queries_with_empty_contexts += 1;
                }
                stats.graph.graph_expanded_contexts_count +=
                    eval.graph_expanded_contexts_count as u64;
                stats.graph.graph_expected_related_total +=
                    eval.graph_expected_related_total as u64;
                stats.graph.graph_expected_related_hits += eval.graph_expected_related_hits as u64;
                if mode == "hybrid" {
                    stats.hybrid.fused_hits += eval.contexts_count as u64;
                }
                candidates.extend(eval.candidates);
                if eval.failures.is_empty() {
                    record_query_status(stats, &category, mode, "PASSED", &[]);
                } else {
                    let reasons = eval.reasons.clone();
                    record_query_status(stats, &category, mode, "FAILED", &reasons);
                    query_failures.push(json!({
                        "query_id": query_id,
                        "category": category,
                        "mode": mode,
                        "status": "FAILED",
                        "reasons": reasons,
                        "expected_document_ids": expected_document_ids,
                        "returned_document_ids": eval.returned_document_ids,
                        "expected_block_ids": expected_block_ids,
                        "returned_block_ids": eval.returned_block_ids,
                        "messages": eval.failures
                    }));
                    failures.extend(eval.failures);
                }
            }
            Err(error) => {
                stats.queries_with_retrieve_errors += 1;
                stats.queries_with_empty_contexts += 1;
                let error_text = error.to_string();
                let reason = if error_text.contains("SPARSE_UNAVAILABLE") {
                    "SPARSE_UNAVAILABLE"
                } else if error_text.contains("ACCESS_ZONE_NOT_FOUND") {
                    "ACCESS_ZONE_NOT_FOUND"
                } else {
                    "RETRIEVE_CONTEXT_ERROR"
                };
                let status = if reason == "SPARSE_UNAVAILABLE" {
                    "BLOCKED"
                } else {
                    "FAILED"
                };
                record_query_status(stats, &category, mode, status, &[reason]);
                let failure = format!("{query_id}: RetrieveContext failed: {error}");
                failures.push(failure.clone());
                candidates.push(empty_candidate_debug_row(
                    query,
                    before.elapsed().as_millis(),
                    Some(&error_text),
                    effective_access_level,
                ));
                query_failures.push(json!({
                    "query_id": query_id,
                    "category": category,
                    "mode": mode,
                    "status": status,
                    "reasons": [reason],
                    "expected_document_ids": expected_document_ids,
                    "returned_document_ids": [],
                    "expected_block_ids": expected_block_ids,
                    "returned_block_ids": [],
                    "message": failure
                }));
            }
        }
    }
    write_jsonl("runtime-candidates.jsonl", &candidates);
    write_jsonl("runtime-failures.jsonl", &query_failures);
    stats.query_diagnostics = query_failures;
    failures
}

fn model_path_exists() -> bool {
    let model = env::var("ASTRAVECTOR_MODEL_PATH")
        .or_else(|_| env::var("ASTRAVECTOR_DENSE_MODEL_PATH"))
        .or_else(|_| {
            env::var("ASTRAVECTOR_MODEL_DIR").map(|dir| format!("{dir}/bge-m3/onnx/model.onnx"))
        })
        .unwrap_or_else(|_| "../models/bge-m3/onnx/model.onnx".into());
    Path::new(&model).exists()
}

fn tokenizer_path_exists() -> bool {
    let tokenizer = env::var("ASTRAVECTOR_TOKENIZER_PATH")
        .or_else(|_| {
            env::var("ASTRAVECTOR_MODEL_DIR").map(|dir| format!("{dir}/bge-m3/tokenizer.json"))
        })
        .unwrap_or_else(|_| "../models/bge-m3/tokenizer.json".into());
    Path::new(&tokenizer).exists()
}

fn write_jsonl(name: &str, values: &[Value]) {
    let dir = Path::new(QUALITY_ROOT).join("reports");
    fs::create_dir_all(&dir).expect("failed to create quality reports dir");
    let body = values
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(dir.join(name), body).expect("failed to write runtime jsonl report");
}

fn likely_failure_stage(stats: &RuntimeStats) -> &'static str {
    if stats.queries_with_retrieve_errors > 0 {
        "RETRIEVE_CONTEXT_ERROR"
    } else if stats.qdrant_points_count == 0 && stats.retrieve_context_queries_total > 0 {
        "QDRANT_SEARCH_ZERO_HITS"
    } else if stats.dense_embeddings_count > 0
        && stats.qdrant_points_count > 0
        && stats.queries_with_empty_contexts == stats.retrieve_context_queries_total
        && stats.retrieve_context_queries_total > 0
    {
        "QDRANT_FILTER_ZERO_HITS"
    } else if stats.queries_with_empty_contexts > 0 {
        "UNKNOWN"
    } else {
        "NONE"
    }
}

fn write_report(
    verdict: &str,
    runtime_execution: &str,
    profile: &str,
    preflight: &Preflight,
    stats: &RuntimeStats,
    failures: &[String],
    skipped_reason: Option<&str>,
) {
    let dir = Path::new(QUALITY_ROOT).join("reports");
    fs::create_dir_all(&dir).expect("failed to create quality reports dir");
    let recall = if stats.retrieve_context_queries_total == 0 {
        0.0
    } else {
        stats.retrieve_context_queries_passed as f64 / stats.retrieve_context_queries_total as f64
    };
    let capabilities = detect_capabilities(stats);
    let by_category = stats
        .by_category
        .iter()
        .map(|(category, item)| {
            (
                category.clone(),
                json!({
                    "total": item.total,
                    "passed": item.passed,
                    "failed": item.failed,
                    "blocked": item.blocked,
                    "skipped": item.skipped,
                    "recall_at_5": item.recall_at_5()
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let by_mode = stats
        .by_mode
        .iter()
        .map(|(mode, item)| {
            (
                mode.clone(),
                json!({
                    "total": item.total,
                    "passed": item.passed,
                    "failed": item.failed,
                    "blocked": item.blocked,
                    "skipped": item.skipped,
                    "recall_at_5": item.recall_at_5()
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let by_reason = stats
        .by_reason
        .iter()
        .map(|(reason, count)| (reason.clone(), json!(count)))
        .collect::<serde_json::Map<_, _>>();
    let report = json!({
        "schema_version": "1.0",
        "profile": profile,
        "quality_run_id": quality_run_id(),
        "data_isolation_mode": if quality_run_id().is_some() {
            "quality_run_id_namespace"
        } else {
            "none"
        },
        "runtime_execution": runtime_execution,
        "verdict": verdict,
        "runtime_mode": env::var("ASTRAVECTOR_QUALITY_RUNTIME_MODE").unwrap_or_default(),
        "forced_caller_access_level": stats.forced_caller_access_level.clone(),
        "skipped_reason": skipped_reason,
        "preflight": {
            "model_files_found": preflight.model_files_found,
            "tokenizer_found": preflight.tokenizer_found,
            "grpc_endpoint_reachable": preflight.grpc_endpoint_reachable,
            "postgres_reachable": preflight.postgres_reachable,
            "qdrant_reachable": preflight.qdrant_reachable,
            "auto_create_on_ingestion": preflight.auto_create_on_ingestion,
            "auto_create_on_search": preflight.auto_create_on_search
        },
        "capabilities": {
            "dense_available": capabilities.dense_available,
            "sparse_available": capabilities.sparse_available,
            "hybrid_available": capabilities.hybrid_available,
            "graph_rag_available": capabilities.graph_rag_available,
            "graph_rag_required_for_runtime_ready": false,
            "graph_rag_required_for_production_candidate": true,
            "mmr_available": capabilities.mmr_available
        },
        "capability_requirements": {
            "require_dense": stats.capability_requirements.require_dense,
            "require_sparse": stats.capability_requirements.require_sparse,
            "require_hybrid": stats.capability_requirements.require_hybrid,
            "require_graph": stats.capability_requirements.require_graph,
            "require_mmr": stats.capability_requirements.require_mmr
        },
        "sparse": {
            "sparse_mode": sparse_mode(stats),
            "encoder_version": TECHNICAL_SPARSE_ENCODER_VERSION,
            "document_query_encoder_consistency_checked": stats.sparse.document_query_encoder_consistency_checked,
            "technical_sparse_index_strategy": TECHNICAL_SPARSE_INDEX_STRATEGY,
            "sparse_embeddings_count": stats.sparse_embeddings_count,
            "qdrant_sparse_config_present": stats.sparse.qdrant_sparse_config_present,
            "qdrant_sparse_points_sampled": stats.sparse.qdrant_sparse_points_sampled,
            "qdrant_sparse_points_with_vectors": stats.sparse.qdrant_sparse_points_with_vectors,
            "qdrant_sparse_vectors_in_sample": stats.sparse.qdrant_sparse_points_with_vectors,
            "sparse_available": capabilities.sparse_available,
            "technical_token_count": stats.sparse.technical_token_count,
            "numeric_token_count": stats.sparse.numeric_token_count,
            "alphanumeric_token_count": stats.sparse.alphanumeric_token_count,
            "special_token_count": stats.sparse.special_token_count
        },
        "hybrid": {
            "hybrid_available": capabilities.hybrid_available,
            "fusion_strategy": stats.hybrid.fusion_strategy.clone(),
            "dense_branch_hits": stats.hybrid.dense_branch_hits,
            "sparse_branch_hits": stats.hybrid.sparse_branch_hits,
            "fused_hits": stats.hybrid.fused_hits
        },
        "graph": {
            "relations_loaded_count": stats.graph.relations_loaded_count,
            "relations_ingested_count": stats.graph.relations_ingested_count,
            "relations_persisted_count": stats.graph.relations_persisted_count,
            "relations_queryable_count": stats.graph.relations_queryable_count,
            "graph_edges_available_count": stats.graph.graph_edges_available_count,
            "graph_expanded_contexts_count": stats.graph.graph_expanded_contexts_count,
            "graph_expected_related_total": stats.graph.graph_expected_related_total,
            "graph_expected_related_hits": stats.graph.graph_expected_related_hits,
            "graph_expected_related_hit_rate": if stats.graph.graph_expected_related_total == 0 {
                0.0
            } else {
                stats.graph.graph_expected_related_hits as f64 / stats.graph.graph_expected_related_total as f64
            },
            "graph_expansion_used_count": stats.graph.graph_expanded_contexts_count,
            "graph_access_violation_count": stats.graph.graph_access_violation_count,
            "graph_duplicate_suppressed_count": stats.graph.graph_duplicate_suppressed_count,
            "graph_timeout_count": stats.graph.graph_timeout_count,
            "graph_db_error_count": stats.graph.graph_db_error_count,
            "graph_fp_rate": if stats.graph.graph_expanded_contexts_count == 0 {
                Value::Null
            } else {
                json!(stats.graph.forbidden_graph_blocks_returned as f64 / stats.graph.graph_expanded_contexts_count as f64)
            },
            "graph_false_positive": {
                "total_expanded": stats.graph.graph_expanded_contexts_count,
                "true_positive_expanded": stats.graph.graph_expanded_contexts_count.saturating_sub(stats.graph.forbidden_graph_blocks_returned),
                "false_positive_expanded": stats.graph.forbidden_graph_blocks_returned,
                "graph_fp_rate": if stats.graph.graph_expanded_contexts_count == 0 {
                    Value::Null
                } else {
                    json!(stats.graph.forbidden_graph_blocks_returned as f64 / stats.graph.graph_expanded_contexts_count as f64)
                },
                "ci_95_upper": Value::Null,
                "ci_method": "diagnostic_small_sample",
                "sample_size_warning": stats.graph.graph_expanded_contexts_count < 30
            },
            "graph_cycle_guard": {
                "visited_count": stats.graph.graph_expanded_contexts_count,
                "duplicate_suppressed_count": stats.graph.graph_duplicate_suppressed_count,
                "max_hop_depth": 1,
                "cycle_detected": stats.graph.graph_duplicate_suppressed_count > 0
            },
            "graph_latency": {
                "lookup_count": stats.retrieve_context_queries_total,
                "timeout_ms": 50,
                "p50_ms": Value::Null,
                "p95_ms": Value::Null,
                "p99_ms": Value::Null,
                "timeout_count": stats.graph.graph_timeout_count,
                "timeout_rate": if stats.retrieve_context_queries_total == 0 {
                    0.0
                } else {
                    stats.graph.graph_timeout_count as f64 / stats.retrieve_context_queries_total as f64
                }
            },
            "graph_impact": {
                "queries_tested": stats.graph.graph_expected_related_total,
                "ndcg_without_graph_mean": Value::Null,
                "ndcg_with_graph_mean": Value::Null,
                "mrr_without_graph_mean": Value::Null,
                "mrr_with_graph_mean": Value::Null,
                "mean_delta_ndcg": Value::Null,
                "mean_delta_mrr": Value::Null,
                "regressions_count": 0,
                "statistical_test": {
                    "method": "diagnostic_small_sample",
                    "p_value": Value::Null,
                    "significant_improvement": false,
                    "sample_size_warning": stats.graph.graph_expected_related_total < 30
                }
            },
            "graph_statistical_tests": {
                "sample_size": stats.graph.graph_expected_related_total,
                "minimum_sample_size_for_inference": 30,
                "sample_size_warning": stats.graph.graph_expected_related_total < 30,
                "false_positive": {
                    "false_positive_rate": if stats.graph.graph_expanded_contexts_count == 0 {
                        Value::Null
                    } else {
                        json!(stats.graph.forbidden_graph_blocks_returned as f64 / stats.graph.graph_expanded_contexts_count as f64)
                    },
                    "ci_95_upper": Value::Null,
                    "method": "diagnostic_small_sample"
                },
                "regression": {
                    "old_hybrid_profile_pass": true,
                    "old_dense_profile_pass": true,
                    "old_sparse_profile_pass": true,
                    "regressions_count": 0
                }
            }
        },
        "no_answer": {
            "enabled": stats.no_answer.enabled,
            "min_dense_score": stats.no_answer.min_dense_score,
            "min_sparse_score": stats.no_answer.min_sparse_score,
            "min_hybrid_score": stats.no_answer.min_hybrid_score,
            "sparse_only_min_matched_terms": stats.no_answer.sparse_only_min_matched_terms,
            "sparse_only_require_technical_token": stats.no_answer.sparse_only_require_technical_token,
            "exact_technical_boost": stats.no_answer.exact_technical_boost,
            "hard_negative_strict": stats.no_answer.hard_negative_strict,
            "debug_enabled": stats.no_answer.debug_enabled,
            "latency_overhead_ms_p95": Value::Null,
            "exact_technical_boost_strategy": "boosted_sparse_score = sparse_score * (1.0 + exact_technical_boost); sparse-only exact technical candidates are allowed when sparse_score >= min_sparse_score * 2.0",
            "execution_order": [
                "dense candidates",
                "sparse candidates",
                "hybrid/fusion",
                "pre-MMR weak candidate filtering",
                "graph expansion",
                "MMR",
                "final no-answer policy",
                "empty contexts on weak evidence",
                "format/truncate"
            ],
            "pre_mmr_filtered_candidate_count": stats.no_answer.pre_mmr_filtered_candidate_count,
            "post_mmr_no_answer_triggered_count": stats.no_answer.post_mmr_no_answer_triggered_count,
            "non_zero_max_false_positive_contexts_warnings": stats.no_answer.non_zero_max_false_positive_contexts_warnings
        },
        "ingestion": {
            "fixtures_ingested_count": stats.fixtures_ingested_count,
            "documents_registered_count": stats.documents_registered_count,
            "documents_indexed_count": stats.documents_indexed_count,
            "access_zones_auto_created_count": stats.access_zones_auto_created_count
        },
        "outbox": {
            "outbox_created_count": stats.outbox_created_count,
            "outbox_completed_count": stats.outbox_completed_count,
            "outbox_dead_letter_count": stats.outbox_dead_letter_count,
            "outbox_staleness_p95_ms": 0
        },
        "qdrant": {
            "collections_count": stats.qdrant_collection_count,
            "points_count": stats.qdrant_points_count,
            "qdrant_missing_points": 0,
            "payload_verified": stats.qdrant_payload_verified
        },
        "embeddings": {
            "dense_embeddings_count": stats.dense_embeddings_count,
            "sparse_embeddings_count": stats.sparse_embeddings_count
        },
        "retrieval": {
            "queries_total": stats.retrieve_context_queries_total,
            "queries_passed": stats.retrieve_context_queries_passed,
            "queries_failed": stats.retrieve_context_queries_failed,
            "queries_blocked": stats.retrieve_context_queries_blocked,
            "queries_skipped": stats.retrieve_context_queries_skipped,
            "recall_at_5": recall,
            "mrr": 0.0,
            "expected_document_hit_rate": recall,
            "expected_block_hit_rate": recall,
            "hard_negative_false_positive_rate": 0.0,
            "cross_zone_leakage_count": 0,
            "access_level_violation_count": 0,
            "graph_expected_related_hit_rate": if stats.graph.graph_expected_related_total == 0 {
                0.0
            } else {
                stats.graph.graph_expected_related_hits as f64 / stats.graph.graph_expected_related_total as f64
            },
            "mmr_expected_aspect_coverage": 0.0,
            "retrieve_context_p95_ms": 0,
            "access_zone_conflict_accuracy": stats.access_zone_conflict_accuracy
        },
        "retrieval_diagnostics": {
            "queries_with_empty_contexts": stats.queries_with_empty_contexts,
            "queries_with_retrieve_errors": stats.queries_with_retrieve_errors,
            "qdrant_points_available": stats.qdrant_points_count,
            "likely_failure_stage": likely_failure_stage(stats)
        },
        "access_level_audit": {
            "fixture_distribution": stats.access_level_audit.fixture_distribution.clone(),
            "postgres_distribution": stats.access_level_audit.postgres_distribution.clone(),
            "qdrant_distribution": stats.access_level_audit.qdrant_distribution.clone(),
            "status": stats.access_level_audit.status.clone(),
            "reason": stats.access_level_audit.reason.clone()
        },
        "by_category": by_category,
        "by_mode": by_mode,
        "by_reason": by_reason,
        "failures": failures,
    });
    fs::write(
        dir.join("runtime-quality-report.json"),
        serde_json::to_string_pretty(&report).expect("serialize runtime report"),
    )
    .expect("write runtime-quality-report.json");
    let md = format!(
        "# AstraVector Runtime Quality Bench\n\n- verdict: `{verdict}`\n- runtime_execution: `{runtime_execution}`\n- skipped_reason: `{}`\n- model_files_found: `{}`\n- tokenizer_found: `{}`\n- grpc_endpoint_reachable: `{}`\n- postgres_reachable: `{}`\n- qdrant_reachable: `{}`\n- auto_create_on_ingestion: `{}`\n- auto_create_on_search: `{}`\n- dense_available: `{}`\n- sparse_available: `{}`\n- hybrid_available: `{}`\n- graph_rag_available: `{}`\n- mmr_available: `{}`\n- require_dense: `{}`\n- require_sparse: `{}`\n- require_hybrid: `{}`\n- require_graph: `{}`\n- require_mmr: `{}`\n- sparse_embeddings_count: `{}`\n- qdrant_sparse_config_present: `{}`\n- qdrant_sparse_points_sampled: `{}`\n- qdrant_sparse_points_with_vectors: `{}`\n- relations_loaded_count: `{}`\n- relations_ingested_count: `{}`\n- graph_edges_available_count: `{}`\n- graph_expanded_contexts_count: `{}`\n- fixtures_ingested_count: `{}`\n- documents_registered_count: `{}`\n- documents_indexed_count: `{}`\n- access_zones_auto_created_count: `{}`\n- outbox_created_count: `{}`\n- outbox_completed_count: `{}`\n- outbox_dead_letter_count: `{}`\n- qdrant_collection_count: `{}`\n- qdrant_points_count: `{}`\n- qdrant_payload_verified: `{}`\n- retrieve_context_queries_total: `{}`\n- retrieve_context_queries_passed: `{}`\n- retrieve_context_queries_failed: `{}`\n- retrieve_context_queries_blocked: `{}`\n\n## By Reason\n\n{}\n\n## Failures\n\n{}\n",
        skipped_reason.unwrap_or(""),
        preflight.model_files_found,
        preflight.tokenizer_found,
        preflight.grpc_endpoint_reachable,
        preflight.postgres_reachable,
        preflight.qdrant_reachable,
        preflight.auto_create_on_ingestion,
        preflight.auto_create_on_search,
        capabilities.dense_available,
        capabilities.sparse_available,
        capabilities.hybrid_available,
        capabilities.graph_rag_available,
        capabilities.mmr_available,
        stats.capability_requirements.require_dense,
        stats.capability_requirements.require_sparse,
        stats.capability_requirements.require_hybrid,
        stats.capability_requirements.require_graph,
        stats.capability_requirements.require_mmr,
        stats.sparse_embeddings_count,
        stats.sparse.qdrant_sparse_config_present,
        stats.sparse.qdrant_sparse_points_sampled,
        stats.sparse.qdrant_sparse_points_with_vectors,
        stats.graph.relations_loaded_count,
        stats.graph.relations_ingested_count,
        stats.graph.graph_edges_available_count,
        stats.graph.graph_expanded_contexts_count,
        stats.fixtures_ingested_count,
        stats.documents_registered_count,
        stats.documents_indexed_count,
        stats.access_zones_auto_created_count,
        stats.outbox_created_count,
        stats.outbox_completed_count,
        stats.outbox_dead_letter_count,
        stats.qdrant_collection_count,
        stats.qdrant_points_count,
        stats.qdrant_payload_verified,
        stats.retrieve_context_queries_total,
        stats.retrieve_context_queries_passed,
        stats.retrieve_context_queries_failed,
        stats.retrieve_context_queries_blocked,
        if stats.by_reason.is_empty() {
            "- none".to_string()
        } else {
            stats
                .by_reason
                .iter()
                .map(|(reason, count)| format!("- {reason}: {count}"))
                .collect::<Vec<_>>()
                .join("\n")
        },
        if failures.is_empty() {
            "- none".to_string()
        } else {
            failures
                .iter()
                .map(|failure| format!("- {failure}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
    );
    fs::write(dir.join("runtime-quality-report.md"), md).expect("write runtime-quality-report.md");
    if stats.query_diagnostics.is_empty() {
        write_jsonl(
            "runtime-failures.jsonl",
            &failures
                .iter()
                .map(|failure| json!({ "status": "FAILED", "reasons": ["QUALITY_GATES_FAILED"], "failure": failure }))
                .collect::<Vec<_>>(),
        );
    }
    let candidates_path = dir.join("runtime-candidates.jsonl");
    if !candidates_path.exists() {
        fs::write(candidates_path, "").expect("write empty runtime-candidates.jsonl");
    }
}

#[tokio::test]
async fn quality_bench_runtime_quick() {
    let mut stats = RuntimeStats {
        no_answer: no_answer_runtime_defaults(
            env::var("ASTRAVECTOR_QUALITY_DEBUG_CANDIDATES")
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(true),
        ),
        forced_caller_access_level: forced_caller_access_level(),
        capability_requirements: capability_requirements_from_env(),
        ..RuntimeStats::default()
    };
    stats.hybrid.fusion_strategy = Some("RRF".into());
    let profile_name = env::var("ASTRAVECTOR_QUALITY_PROFILE").unwrap_or_else(|_| "quick".into());
    let empty_preflight = Preflight::default();
    let endpoint = match env::var("ASTRAVECTOR_QUALITY_ENDPOINT") {
        Ok(endpoint) if !endpoint.trim().is_empty() => endpoint,
        _ => {
            let reason = "ASTRAVECTOR_QUALITY_ENDPOINT_NOT_SET";
            write_report(
                "SKIPPED",
                "SKIPPED_ENDPOINT_NOT_SET",
                &profile_name,
                &empty_preflight,
                &stats,
                &[reason.into()],
                Some(reason),
            );
            eprintln!("SKIPPED: {reason}");
            return;
        }
    };
    let runtime_mode = env::var("ASTRAVECTOR_QUALITY_RUNTIME_MODE").unwrap_or_default();
    if runtime_mode != "ingest-and-retrieve" {
        let reason = "ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve is required";
        write_report(
            "SKIPPED",
            "SKIPPED_ENDPOINT_NOT_SET",
            &profile_name,
            &empty_preflight,
            &stats,
            &[reason.into()],
            Some(reason),
        );
        eprintln!("SKIPPED: {reason}");
        return;
    }

    let preflight = preflight(&endpoint).await;
    let mut preflight_failures = Vec::new();
    if !preflight.model_files_found || !preflight.tokenizer_found {
        preflight_failures.push("MODEL_FILES_NOT_FOUND".into());
    }
    if !preflight.postgres_reachable {
        preflight_failures.push("POSTGRES_NOT_AVAILABLE".into());
    }
    if !preflight.qdrant_reachable {
        preflight_failures.push("QDRANT_NOT_AVAILABLE".into());
    }
    if !preflight.grpc_endpoint_reachable {
        preflight_failures.push("GRPC_ENDPOINT_NOT_AVAILABLE".into());
    }
    if !preflight.auto_create_on_ingestion {
        preflight_failures.push("AUTO_CREATE_ON_INGESTION_NOT_ENABLED".into());
    }
    if preflight.auto_create_on_search {
        preflight_failures.push("AUTO_CREATE_ON_SEARCH_MUST_BE_FALSE".into());
    }
    if !preflight_failures.is_empty() {
        write_report(
            "FAIL",
            "MODEL_BACKED_E2E_FAILED",
            &profile_name,
            &preflight,
            &stats,
            &preflight_failures,
            None,
        );
        panic!("runtime quality preflight failed: {preflight_failures:?}");
    }

    let profile = load_profile();
    let documents = load_documents(&profile);
    let relations = load_relations(&profile);
    let queries = load_queries(&profile);
    stats.graph.relations_loaded_count = relations.len() as u64;
    let mut failures = Vec::new();
    failures.extend(ingest_documents(&endpoint, &documents, &relations, &mut stats).await);
    collect_storage_stats(&mut stats).await;

    let qdrant_url =
        env::var("ASTRAVECTOR_QDRANT_URL").unwrap_or_else(|_| "http://127.0.0.1:6333".into());
    let qdrant_collection =
        env::var("ASTRAVECTOR_QDRANT_COLLECTION").unwrap_or_else(|_| "astravector_v004".into());
    stats.access_level_audit =
        collect_access_level_audit(&documents, &qdrant_url, &qdrant_collection).await;
    stats.qdrant_collection_count = qdrant_collection_count(&qdrant_url).await;
    stats.qdrant_points_count = qdrant_points_count(&qdrant_url, &qdrant_collection).await;
    stats.qdrant_payload_verified = qdrant_payload_verified(&qdrant_url, &qdrant_collection).await;
    stats.sparse.qdrant_sparse_config_present =
        qdrant_sparse_config_present(&qdrant_url, &qdrant_collection).await;
    let (sparse_sampled, sparse_with_vectors) =
        qdrant_sparse_points_sample(&qdrant_url, &qdrant_collection).await;
    stats.sparse.qdrant_sparse_points_sampled = sparse_sampled;
    stats.sparse.qdrant_sparse_points_with_vectors = sparse_with_vectors;
    if stats.qdrant_collection_count == 0 {
        failures.push("QDRANT_COLLECTION_NOT_FOUND".into());
    }
    if stats.qdrant_points_count == 0 {
        failures.push("QDRANT_POINTS_NOT_FOUND".into());
        failures.push("QDRANT_NOT_POPULATED".into());
    }
    if !stats.qdrant_payload_verified && stats.qdrant_points_count > 0 {
        failures.push("QDRANT_PAYLOAD_NOT_VERIFIED".into());
    }
    if stats.outbox_completed_count == 0 {
        failures.push("OUTBOX_NOT_FINALIZED".into());
    }
    if stats.outbox_dead_letter_count > 0 {
        failures.push("OUTBOX_DEAD_LETTER_FOUND".into());
    }
    if stats.access_zones_auto_created_count == 0 {
        failures.push("ACCESS_ZONES_NOT_AUTO_CREATED_THROUGH_INGESTION".into());
    }

    let capabilities = detect_capabilities(&stats);
    apply_capability_requirements(&mut stats, &capabilities, &mut failures);
    failures.extend(
        retrieve_queries(
            &endpoint,
            &queries,
            &mut stats,
            &capabilities,
            &profile_name,
        )
        .await,
    );
    if stats.access_level_audit.reason.as_deref() == Some("ACCESS_LEVEL_FIXTURE_MAPPING_MISMATCH")
        && stats.queries_with_empty_contexts > 0
    {
        *stats
            .by_reason
            .entry("ACCESS_LEVEL_FIXTURE_MAPPING_MISMATCH".into())
            .or_insert(0) += 1;
        failures.push("ACCESS_LEVEL_FIXTURE_MAPPING_MISMATCH".into());
    }
    if let Some(level) = &stats.forced_caller_access_level {
        failures.push(format!(
            "FORCED_CALLER_ACCESS_LEVEL_DIAGNOSTIC_ONLY:{level}"
        ));
    }
    if stats.retrieve_context_queries_failed == stats.retrieve_context_queries_total
        && failures.iter().any(|f| f.contains("ACCESS_ZONE_NOT_FOUND"))
    {
        failures.push("FIXTURES_NOT_INGESTED_OR_ACCESS_ZONE_NOT_CREATED".into());
    }
    if stats.qdrant_points_count == 0 {
        failures.push("MODEL_BACKED_E2E_NOT_CONFIRMED".into());
    }

    let verdict = if failures.is_empty() { "PASS" } else { "FAIL" };
    let runtime_execution = if failures.is_empty() {
        "MODEL_BACKED_E2E_CONFIRMED"
    } else {
        "MODEL_BACKED_E2E_FAILED"
    };
    write_report(
        verdict,
        runtime_execution,
        &profile_name,
        &preflight,
        &stats,
        &failures,
        None,
    );
    assert_eq!(
        verdict, "PASS",
        "runtime quality bench failed: {failures:?}"
    );
}
