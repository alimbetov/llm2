use crate::{
    config::AppConfig,
    grpc::AstraVectorV004ControlService,
    health::Readiness,
    pb::{self, astra_vector_v004_control_server::AstraVectorV004Control},
};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use metrics::{counter, histogram};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc, time::Instant};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tonic::{Code, Request, Status};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct InternalHttpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub max_request_body_bytes: usize,
}

impl InternalHttpConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            enabled: env_bool("ASTRAVECTOR_HTTP_ENABLED", true)?,
            host: std::env::var("ASTRAVECTOR_HTTP_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env_parse("ASTRAVECTOR_HTTP_PORT", 8080_u16)?,
            max_request_body_bytes: env_parse(
                "ASTRAVECTOR_HTTP_MAX_REQUEST_BODY_BYTES",
                65_536_usize,
            )?,
        })
    }

    pub fn validate(&self, grpc_port: u16, metrics_port: u16) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        anyhow::ensure!(self.port != grpc_port, "HTTP port collides with gRPC port");
        anyhow::ensure!(self.port != metrics_port, "HTTP port collides with metrics port");
        anyhow::ensure!(
            self.max_request_body_bytes > 0,
            "HTTP max_request_body_bytes must be > 0"
        );
        Ok(())
    }
}

fn env_bool(name: &str, default: bool) -> anyhow::Result<bool> {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => anyhow::bail!("{name} must be a boolean"),
        },
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

fn env_parse<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match std::env::var(name) {
        Ok(value) => Ok(value.parse::<T>()?),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone)]
pub struct InternalHttpState {
    control: AstraVectorV004ControlService,
    readiness: Readiness,
    cfg: Arc<AppConfig>,
}

impl InternalHttpState {
    pub fn new(
        control: AstraVectorV004ControlService,
        readiness: Readiness,
        cfg: Arc<AppConfig>,
    ) -> Self {
        Self {
            control,
            readiness,
            cfg,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestRetrieveRequest {
    pub question: String,
    #[serde(default)]
    pub access_zone_id: String,
    #[serde(default)]
    pub access_zone_ids: Vec<String>,
    #[serde(default)]
    pub access_zone_code: String,
    #[serde(default)]
    pub access_zone_codes: Vec<String>,
    #[serde(default = "default_access_level")]
    pub caller_access_level: String,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default)]
    pub max_contexts: u32,
    #[serde(default)]
    pub filters: Vec<RestFilter>,
    #[serde(default)]
    pub enable_graph_expansion: bool,
    #[serde(default)]
    pub graph_max_hops: u32,
    #[serde(default)]
    pub graph_max_related_contexts: u32,
    #[serde(default)]
    pub correlation_id: String,
}

fn default_access_level() -> String {
    "INTERNAL".into()
}

fn default_profile() -> String {
    "BALANCED".into()
}

#[derive(Debug, Deserialize)]
pub struct RestFilter {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestErrorBody {
    code: String,
    message: String,
    correlation_id: String,
}

pub async fn serve(
    config: InternalHttpConfig,
    state: InternalHttpState,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let router = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/retrieve", post(retrieve))
        .layer(DefaultBodyLimit::max(config.max_request_body_bytes))
        .with_state(state);
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "AstraVector internal REST boundary starting");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status":"SERVING"})))
}

async fn ready(State(state): State<InternalHttpState>) -> Response {
    if state.readiness.is_ready() {
        (StatusCode::OK, Json(json!({"status":"READY","ready":true}))).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status":"NOT_READY","ready":false})),
        )
            .into_response()
    }
}

async fn retrieve(
    State(state): State<InternalHttpState>,
    headers: HeaderMap,
    payload: Result<Json<RestRetrieveRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let started = Instant::now();
    let payload = match payload {
        Ok(Json(payload)) => payload,
        Err(error) => {
            let status = if matches!(
                error,
                axum::extract::rejection::JsonRejection::MissingJsonContentType(_)
            ) {
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            } else {
                StatusCode::BAD_REQUEST
            };
            return (
                status,
                Json(json!({
                    "code": if status == StatusCode::UNSUPPORTED_MEDIA_TYPE {"UNSUPPORTED_MEDIA_TYPE"} else {"INVALID_JSON"},
                    "message": error.body_text(),
                    "correlationId": correlation_id_from_headers(&headers),
                })),
            )
                .into_response();
        }
    };
    let correlation_id = if payload.correlation_id.trim().is_empty() {
        correlation_id_from_headers(&headers)
    } else {
        payload.correlation_id.trim().to_owned()
    };

    let result = execute_retrieve(&state, payload, &correlation_id).await;
    let elapsed = started.elapsed().as_secs_f64();
    histogram!("astravector_http_request_duration_seconds", "route" => "/api/v1/retrieve", "method" => "POST")
        .record(elapsed);

    match result {
        Ok(body) => {
            counter!("astravector_http_requests_total", "route" => "/api/v1/retrieve", "method" => "POST", "status_class" => "2xx").increment(1);
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(status) => {
            let http_status = http_status_from_tonic(status.code());
            counter!("astravector_http_requests_total", "route" => "/api/v1/retrieve", "method" => "POST", "status_class" => status_class(http_status)).increment(1);
            let body = RestErrorBody {
                code: tonic_code_name(status.code()).into(),
                message: status.message().to_owned(),
                correlation_id,
            };
            (http_status, Json(body)).into_response()
        }
    }
}

async fn execute_retrieve(
    state: &InternalHttpState,
    request: RestRetrieveRequest,
    correlation_id: &str,
) -> Result<Value, Status> {
    let question = request.question.trim();
    if question.is_empty() {
        return Err(Status::invalid_argument("question is required"));
    }
    let access_level = parse_access_level(&request.caller_access_level)?;
    let profile = parse_profile(&request.profile)?;
    let max_contexts = if request.max_contexts == 0 {
        5
    } else {
        request.max_contexts.min(state.cfg.limits.search_top_k_max)
    };
    let mut inner = Request::new(pb::SearchRequestV004 {
        correlation_id: correlation_id.to_owned(),
        access_zone_id: request.access_zone_id,
        caller_access_level: access_level as i32,
        query: request.question,
        top_k: max_contexts,
        candidate_limit: retrieval_candidate_limit(profile),
        parent_limit: max_contexts,
        filters: request
            .filters
            .into_iter()
            .map(|filter| pb::SearchFilterV004 {
                key: filter.key,
                value: filter.value,
            })
            .collect(),
        timeout_ms: state.cfg.grpc.deadlines.query_ms as u32,
        search_mode: retrieval_search_mode(profile) as i32,
        include_debug: false,
        include_vectors: false,
        embedding_mode: retrieval_embedding_mode(profile) as i32,
        model_version: None,
        tokenizer_version: None,
        dense_version: None,
        sparse_version: None,
        chunking_version: None,
        enable_graph_expansion: request.enable_graph_expansion
            || state.cfg.graph_rag.retrieval.enabled_by_default,
        graph_max_hops: if request.graph_max_hops == 0 {
            1
        } else {
            request.graph_max_hops.min(1)
        },
        graph_max_related_contexts: if request.graph_max_related_contexts == 0 {
            state
                .cfg
                .graph_rag
                .retrieval
                .max_related_chunks
                .min(state.cfg.limits.graph_related_contexts_max) as u32
        } else {
            request
                .graph_max_related_contexts
                .min(state.cfg.limits.graph_related_contexts_max as u32)
        },
        access_zone_ids: request.access_zone_ids,
        access_zone_code: request.access_zone_code,
        access_zone_codes: request.access_zone_codes,
    });
    // No authentication metadata is attached. This is an internal service boundary.
    // access level is retrieval visibility semantics, not HTTP authentication.
    inner.metadata_mut().clear();
    let search = <AstraVectorV004ControlService as AstraVectorV004Control>::search(
        &state.control,
        inner,
    )
    .await?
    .into_inner();
    Ok(rest_response_from_search(search, profile))
}

fn parse_access_level(raw: &str) -> Result<pb::AccessLevel, Status> {
    match raw.trim().trim_start_matches("ACCESS_LEVEL_").to_ascii_uppercase().as_str() {
        "1" | "PUBLIC" => Ok(pb::AccessLevel::Public),
        "2" | "INTERNAL" => Ok(pb::AccessLevel::Internal),
        "3" | "CONFIDENTIAL" => Ok(pb::AccessLevel::Confidential),
        "4" | "RESTRICTED" => Ok(pb::AccessLevel::Restricted),
        _ => Err(Status::invalid_argument(
            "callerAccessLevel must be PUBLIC, INTERNAL, CONFIDENTIAL, or RESTRICTED",
        )),
    }
}

fn parse_profile(raw: &str) -> Result<pb::RetrievalProfile, Status> {
    match raw
        .trim()
        .trim_start_matches("RETRIEVAL_PROFILE_")
        .to_ascii_uppercase()
        .as_str()
    {
        "" | "BALANCED" => Ok(pb::RetrievalProfile::Balanced),
        "LEGAL" => Ok(pb::RetrievalProfile::Legal),
        "TECHNICAL" => Ok(pb::RetrievalProfile::Technical),
        "SEMANTIC" => Ok(pb::RetrievalProfile::Semantic),
        "LEXICAL_STRICT" | "LEXICALSTRICT" => Ok(pb::RetrievalProfile::LexicalStrict),
        _ => Err(Status::invalid_argument("unsupported retrieval profile")),
    }
}

fn retrieval_search_mode(profile: pb::RetrievalProfile) -> pb::SearchModeV005 {
    match profile {
        pb::RetrievalProfile::Semantic => pb::SearchModeV005::Dense,
        pb::RetrievalProfile::LexicalStrict => pb::SearchModeV005::Sparse,
        _ => pb::SearchModeV005::Hybrid,
    }
}

fn retrieval_embedding_mode(profile: pb::RetrievalProfile) -> pb::EmbeddingModeV005 {
    match profile {
        pb::RetrievalProfile::Legal | pb::RetrievalProfile::LexicalStrict => {
            pb::EmbeddingModeV005::DenseSparseRequired
        }
        pb::RetrievalProfile::Semantic => pb::EmbeddingModeV005::DenseOnly,
        _ => pb::EmbeddingModeV005::DenseSparseIfAvailable,
    }
}

fn retrieval_candidate_limit(profile: pb::RetrievalProfile) -> u32 {
    match profile {
        pb::RetrievalProfile::Legal => 120,
        pb::RetrievalProfile::Technical => 100,
        pb::RetrievalProfile::LexicalStrict => 80,
        pb::RetrievalProfile::Semantic => 60,
        _ => 80,
    }
}

fn rest_response_from_search(search: pb::SearchResponseV004, profile: pb::RetrievalProfile) -> Value {
    let diagnostics = search.diagnostics.clone().unwrap_or_default();
    let typed_degradation = search.degradation.clone();
    let total_candidates = if diagnostics.candidate_count == 0 {
        search.results.len() as u32
    } else {
        diagnostics.candidate_count
    };
    let mut degradation_codes = search
        .warnings
        .iter()
        .map(|warning| warning.code.clone())
        .filter(|code| {
            matches!(
                code.as_str(),
                "GRAPH_EXPANSION_BACKPRESSURE"
                    | "GRAPH_EXPANSION_TIMEOUT"
                    | "MMR_FETCH_BACKPRESSURE"
                    | "MMR_FETCH_TIMEOUT"
                    | "TOKEN_SIMILARITY_FALLBACK"
                    | "DEADLINE_BUDGET_DEGRADATION"
                    | "LONG_QUERY_PARTIAL_COVERAGE"
                    | "LONG_QUERY_CONTEXT_BUDGET_INSUFFICIENT"
                    | "LONG_QUERY_COVERAGE_EXCEEDS_CONTEXT_LIMIT"
                    | "LONG_QUERY_COVERAGE_REDUCED_BY_VISIBILITY_RECHECK"
                    | "QUERY_SEGMENT_SKIPPED_INSUFFICIENT_BUDGET"
                    | "QUERY_SEGMENT_RETRIEVAL_DEGRADED"
                    | "BINDING_INVALID"
                    | "VISIBILITY_REJECTED"
                    | "HYDRATION_MISSING"
                    | "PARENT_HYDRATION_TIMEOUT"
                    | "EMPTY_CONTEXT"
            )
        })
        .collect::<Vec<_>>();
    if let Some(degradation) = typed_degradation.as_ref() {
        degradation_codes.extend(
            degradation
                .dropped_parents
                .iter()
                .map(|dropped| dropped.reason.clone()),
        );
    }
    degradation_codes.sort();
    degradation_codes.dedup();
    let degraded = typed_degradation
        .as_ref()
        .is_some_and(|degradation| degradation.degraded)
        || !degradation_codes.is_empty();
    let contexts = search
        .results
        .into_iter()
        .map(rest_context_from_search_result)
        .collect::<Vec<_>>();
    let evidence_status = if degraded {
        "DEGRADED"
    } else if contexts.is_empty() {
        "INSUFFICIENT"
    } else {
        "FOUND"
    };
    json!({
        "summary": {
            "totalCandidates": total_candidates,
            "returnedContexts": contexts.len(),
            "profile": profile_name(profile),
            "evidenceStatus": evidence_status,
            "degraded": degraded,
            "degradationCodes": degradation_codes,
            "denseBranchExecuted": diagnostics.dense_branch_executed,
            "sparseBranchExecuted": diagnostics.sparse_branch_executed,
            "fusionExecuted": diagnostics.fusion_executed,
            "denseBranchCandidateCount": diagnostics.dense_branch_candidate_count,
            "sparseBranchCandidateCount": diagnostics.sparse_branch_candidate_count,
            "fusionCandidateCount": diagnostics.fusion_candidate_count
        },
        "contexts": contexts,
        "warnings": search.warnings.into_iter().map(|w| json!({"code":w.code,"message":w.message})).collect::<Vec<_>>(),
        "degradation": typed_degradation.map(|d| json!({
            "degraded": d.degraded,
            "degradationClass": d.degradation_class,
            "retryable": d.retryable,
            "coverageClass": d.coverage_class,
            "infrastructureFailure": d.infrastructure_failure,
            "fullHydrationFailure": d.full_hydration_failure,
            "droppedParents": d.dropped_parents.into_iter().map(|p| json!({
                "parentId": p.parent_id,
                "reason": p.reason,
                "rejectionStage": p.rejection_stage,
                "retryable": p.retryable,
                "inputOrdinal": p.input_ordinal
            })).collect::<Vec<_>>()
        })),
        "diagnostics": {
            "queryEmbeddingMs": diagnostics.query_embedding_ms,
            "qdrantSearchMs": diagnostics.qdrant_search_ms,
            "parentFetchMs": diagnostics.parent_fetch_ms,
            "totalMs": diagnostics.total_ms,
            "candidateCount": diagnostics.candidate_count,
            "finalCandidateCount": diagnostics.final_candidate_count,
            "graphExpansionDurationMs": diagnostics.graph_expansion_duration_ms,
            "graphMergeDurationMs": diagnostics.graph_merge_duration_ms,
            "mmrEnabled": diagnostics.mmr_enabled,
            "mmrDurationMs": diagnostics.mmr_duration_ms,
            "tokenBudgetEnabled": diagnostics.token_budget_enabled,
            "estimatedContextTokensAfter": diagnostics.estimated_context_tokens_after,
            "queryProcessingMode": diagnostics.query_processing_mode,
            "queryOriginalTokenCount": diagnostics.query_original_token_count,
            "querySegmentCount": diagnostics.query_segment_count,
            "queryCoverageRatio": diagnostics.query_coverage_ratio,
            "effectiveQueryTimeoutMs": diagnostics.effective_query_timeout_ms
        }
    })
}

fn rest_context_from_search_result(result: pb::SearchResultV004) -> Value {
    let mut metadata = result
        .citation
        .as_ref()
        .map(|citation| citation.metadata.clone())
        .unwrap_or_default();
    metadata
        .entry("access_zone_id".into())
        .or_insert_with(|| result.access_zone_id.clone());
    metadata
        .entry("document_id".into())
        .or_insert_with(|| result.document_id.clone());
    metadata
        .entry("document_version".into())
        .or_insert_with(|| result.document_version.to_string());
    metadata
        .entry("matched_chunk_id".into())
        .or_insert_with(|| result.matched_chunk_id.clone());
    metadata
        .entry("parent_chunk_id".into())
        .or_insert_with(|| result.parent_chunk_id.clone());
    let scores = result.scores.unwrap_or_default();
    let source_block_id = metadata.get("source_block_id").cloned().unwrap_or_default();
    json!({
        "matchedText": result.matched_text,
        "parentText": result.parent_text,
        "documentId": result.document_id,
        "documentVersion": result.document_version,
        "sourceBlockId": source_block_id,
        "matchedChunkId": result.matched_chunk_id,
        "parentChunkId": result.parent_chunk_id,
        "accessZoneId": metadata.get("access_zone_id").cloned().unwrap_or_default(),
        "citation": {
            "documentId": metadata.get("document_id").cloned().unwrap_or_default(),
            "documentVersion": metadata.get("document_version").and_then(|v| v.parse::<u64>().ok()).unwrap_or_default(),
            "sourceUri": metadata.get("source_uri").cloned().unwrap_or_default(),
            "title": metadata.get("document_title").cloned().unwrap_or_default(),
            "pageStart": metadata.get("page_start").and_then(|v| v.parse::<u32>().ok()).unwrap_or_default(),
            "pageEnd": metadata.get("page_end").and_then(|v| v.parse::<u32>().ok()).unwrap_or_default(),
            "sectionPath": metadata.get("section_path").cloned().unwrap_or_default(),
            "heading": metadata.get("heading").cloned().unwrap_or_default(),
            "matchedChunkId": metadata.get("matched_chunk_id").cloned().unwrap_or_default(),
            "parentChunkId": metadata.get("parent_chunk_id").cloned().unwrap_or_default(),
            "sourceBlockId": metadata.get("source_block_id").cloned().unwrap_or_default()
        },
        "scores": {
            "denseScore": scores.dense_score,
            "sparseScore": scores.sparse_score,
            "fusionScore": scores.fusion_score,
            "finalScore": scores.final_score
        },
        "metadata": metadata
    })
}

fn profile_name(profile: pb::RetrievalProfile) -> &'static str {
    match profile {
        pb::RetrievalProfile::Legal => "LEGAL",
        pb::RetrievalProfile::Technical => "TECHNICAL",
        pb::RetrievalProfile::Semantic => "SEMANTIC",
        pb::RetrievalProfile::LexicalStrict => "LEXICAL_STRICT",
        _ => "BALANCED",
    }
}

fn correlation_id_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-correlation-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn http_status_from_tonic(code: Code) -> StatusCode {
    match code {
        Code::InvalidArgument | Code::OutOfRange => StatusCode::BAD_REQUEST,
        Code::Unauthenticated => StatusCode::UNAUTHORIZED,
        Code::PermissionDenied => StatusCode::FORBIDDEN,
        Code::NotFound => StatusCode::NOT_FOUND,
        Code::AlreadyExists | Code::FailedPrecondition | Code::Aborted => StatusCode::CONFLICT,
        Code::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
        Code::Cancelled => StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
        Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        Code::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn tonic_code_name(code: Code) -> &'static str {
    match code {
        Code::Ok => "OK",
        Code::Cancelled => "CANCELLED",
        Code::Unknown => "UNKNOWN",
        Code::InvalidArgument => "INVALID_ARGUMENT",
        Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
        Code::NotFound => "NOT_FOUND",
        Code::AlreadyExists => "ALREADY_EXISTS",
        Code::PermissionDenied => "PERMISSION_DENIED",
        Code::ResourceExhausted => "RESOURCE_EXHAUSTED",
        Code::FailedPrecondition => "FAILED_PRECONDITION",
        Code::Aborted => "ABORTED",
        Code::OutOfRange => "OUT_OF_RANGE",
        Code::Unimplemented => "UNIMPLEMENTED",
        Code::Internal => "INTERNAL",
        Code::Unavailable => "UNAVAILABLE",
        Code::DataLoss => "DATA_LOSS",
        Code::Unauthenticated => "UNAUTHENTICATED",
    }
}

fn status_class(status: StatusCode) -> &'static str {
    match status.as_u16() / 100 {
        2 => "2xx",
        4 => "4xx",
        5 => "5xx",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_access_level_defaults_and_parses() {
        assert_eq!(default_access_level(), "INTERNAL");
        assert_eq!(parse_access_level("PUBLIC").unwrap(), pb::AccessLevel::Public);
        assert_eq!(parse_access_level("INTERNAL").unwrap(), pb::AccessLevel::Internal);
        assert!(parse_access_level("UNSPECIFIED").is_err());
    }

    #[test]
    fn retrieval_profile_mapping_matches_grpc_facade() {
        assert_eq!(
            retrieval_search_mode(pb::RetrievalProfile::Semantic),
            pb::SearchModeV005::Dense
        );
        assert_eq!(
            retrieval_search_mode(pb::RetrievalProfile::LexicalStrict),
            pb::SearchModeV005::Sparse
        );
        assert_eq!(
            retrieval_candidate_limit(pb::RetrievalProfile::Technical),
            100
        );
        assert_eq!(
            retrieval_embedding_mode(pb::RetrievalProfile::Legal),
            pb::EmbeddingModeV005::DenseSparseRequired
        );
    }

    #[test]
    fn transport_error_mapping_is_deterministic() {
        assert_eq!(http_status_from_tonic(Code::InvalidArgument), StatusCode::BAD_REQUEST);
        assert_eq!(http_status_from_tonic(Code::ResourceExhausted), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(http_status_from_tonic(Code::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(http_status_from_tonic(Code::DeadlineExceeded), StatusCode::GATEWAY_TIMEOUT);
    }
}
