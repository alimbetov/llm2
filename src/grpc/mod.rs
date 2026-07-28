use crate::{
    access_zone_registry,
    cache::L1Cache,
    chunking::{
        AnnotatedTextSegment, ChunkingEngine, ChunkingProfile, ConservativeTokenCounter,
        SizeProfile, SourceChunkStorageMode,
    },
    config::{AppConfig, NoAnswerConfig},
    contract,
    error::AstraError,
    health::Readiness,
    inference::{EmbeddingResult, InferenceEngine, InferenceInput},
    pb::{
        self, astra_vector_admin_facade_server::AstraVectorAdminFacade,
        astra_vector_ingestion_facade_server::AstraVectorIngestionFacade,
        astra_vector_retrieval_facade_server::AstraVectorRetrievalFacade,
        astra_vector_runtime_server::AstraVectorRuntime,
        astra_vector_v004_control_server::AstraVectorV004Control,
    },
    persistence::{
        ChunkContentRecord, ChunkTraceRecord, ClaimResult, FinalVisibilityCandidate,
        LexicalParentCandidate, ParentContextRecord, Repository,
    },
    provider::SelectedProvider,
    qdrant::{QdrantClient, QdrantSearchHit, QdrantVersionFilters},
    query_processing::{
        build_query_plan,
        coverage::{
            evaluate_intent_coverage, evaluate_required_coverage, QueryCoverage,
            QueryEvidenceStatus,
        },
        diagnostics::QueryPlanDiagnostics,
        evidence::CandidateIntentEvidence,
        fusion::{cross_segment_rrf, GlobalCandidateIdentity, SegmentCandidate},
        planner::{QueryPlan, QueryPlanningError, QuerySegment, QueryTokenCounter},
        status::{summarize_retrieval_statuses, RetrievalBranchStatus, SegmentRetrievalStatus},
        QueryProcessingMode, QueryProcessingTier,
    },
    reliability::{resolve_optional_stage_budget, OperationBudget, WorkloadKind},
    retrieval::{
        hydration::{
            bounded_hydration_fetch_window, normalize_hydration_outcomes,
            total_hydration_timeout_status, HydrationCandidateIdentity,
        },
        hydration_failpoints::HydrationFailpointPlan,
    },
    scheduler::{QueueKind, Scheduler, SubmitManyOptions},
    sparse::{SparseTechnicalEncoder, SparseTokenClass},
};
use futures::{future::join_all, stream, StreamExt};
use metrics::{counter, gauge, histogram};
use moka::future::Cache;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{Arc, OnceLock},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tonic::{
    metadata::{MetadataKey, MetadataMap},
    Request, Response, Status,
};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

struct AdmissionPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    scope: &'static str,
    weight: f64,
}

struct RequestCancellationGuard(CancellationToken);

#[derive(Clone, Copy)]
struct RetrievalEntryPoint(&'static str);

#[derive(Clone, Copy)]
struct RequestTiming {
    started: Instant,
    transport_deadline: Option<Instant>,
}

impl RequestTiming {
    fn from_request<T>(request: &Request<T>) -> Self {
        let started = Instant::now();
        Self {
            started,
            transport_deadline: grpc_transport_deadline(request.metadata(), started),
        }
    }
}

impl Drop for RequestCancellationGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct LongQueryInFlightGuard {
    segments: f64,
}

impl Drop for LongQueryInFlightGuard {
    fn drop(&mut self) {
        gauge!("astravector_long_query_segments_in_flight").decrement(self.segments);
    }
}

struct DirectQdrantGeneration {
    hits: Vec<QdrantSearchHit>,
    query_embedding_ms: u64,
    qdrant_search_ms: u64,
    dense_search_ms: u64,
    sparse_search_ms: u64,
    fusion_ms: u64,
    dense_branch_executed: bool,
    sparse_branch_executed: bool,
    fusion_executed: bool,
    dense_branch_candidate_count: u32,
    sparse_branch_candidate_count: u32,
    fusion_candidate_count: u32,
    sparse_top_score: f32,
    dense_failed: bool,
    sparse_failed: bool,
    branch_statuses: Vec<RetrievalBranchStatus>,
    retrieval_status: SegmentRetrievalStatus,
}

struct SegmentQdrantResult {
    segment_index: usize,
    segment_weight: f32,
    intent_unit_ids: Vec<usize>,
    hits: Vec<QdrantSearchHit>,
    dense_executed: bool,
    sparse_executed: bool,
    fusion_executed: bool,
    dense_failed: bool,
    sparse_failed: bool,
    dense_status: Option<RetrievalBranchStatus>,
    sparse_status: Option<RetrievalBranchStatus>,
    dense_candidates: usize,
    sparse_candidates: usize,
    dense_ms: u64,
    sparse_ms: u64,
    fusion_ms: u64,
    sparse_top_score: f32,
    warnings: Vec<pb::DiagnosticWarningV005>,
}

struct SegmentLexicalResult {
    segment_index: usize,
    segment_text: String,
    segment_weight: f32,
    candidates: Vec<LexicalParentCandidate>,
    duration_ms: u64,
    warnings: Vec<pb::DiagnosticWarningV005>,
}

struct EngineQueryTokenCounter<'a> {
    engine: &'a dyn InferenceEngine,
}

impl QueryTokenCounter for EngineQueryTokenCounter<'_> {
    fn count_tokens(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<usize, String> {
        self.engine
            .count_tokens(text, max_length, allow_truncation)
            .map_err(|error| error.to_string())
    }

    fn token_offsets(&self, text: &str) -> Result<Vec<crate::tokenizer::TokenOffset>, String> {
        self.engine
            .token_offsets(text)
            .map_err(|error| error.to_string())
    }
}

fn query_planning_status(error: QueryPlanningError) -> Status {
    match error {
        QueryPlanningError::Empty => Status::invalid_argument("query must not be empty"),
        QueryPlanningError::ByteLimitExceeded => Status::out_of_range(
            "LONG_QUERY_BYTE_LIMIT_EXCEEDED: query exceeds configured absolute_max_bytes",
        ),
        QueryPlanningError::TokenLimitExceeded => Status::out_of_range(
            "LONG_QUERY_TOO_LARGE: query exceeds configured absolute_max_tokens",
        ),
        QueryPlanningError::LongQueryNotSupported => Status::out_of_range(
            "LONG_QUERY_NOT_SUPPORTED: query_processing.enabled=false rejects queries above tokenization.query.max_length",
        ),
        QueryPlanningError::ExtendedQueryNotEnabled => Status::out_of_range(
            "LONG_QUERY_EXTENDED_NOT_ENABLED: enable query_processing.extended_enabled for queries above the Standard tier",
        ),
        QueryPlanningError::IntentExtraction(message) => Status::internal(format!(
            "QUERY_INTENT_EXTRACTION_FAILED: {message}"
        )),
        QueryPlanningError::SegmentationInvariant(message) => Status::internal(format!(
            "QUERY_SEGMENTATION_INVARIANT_FAILED: {message}"
        )),
        QueryPlanningError::Tokenization(message) => {
            Status::invalid_argument(format!("tokenization failed: {message}"))
        }
    }
}

fn postgres_statement_timeout_ms(
    configured_timeout_ms: u64,
    remaining_ms: u64,
    safety_margin_ms: u64,
) -> Result<u64, AstraError> {
    if remaining_ms <= safety_margin_ms {
        return Err(AstraError::DeadlineExceeded(
            "insufficient_postgres_budget".into(),
        ));
    }
    Ok(configured_timeout_ms
        .min(remaining_ms - safety_margin_ms)
        .max(1))
}

fn optional_lexical_failure_can_degrade(status: &Status) -> bool {
    matches!(
        status.code(),
        tonic::Code::DeadlineExceeded | tonic::Code::Unavailable
    )
}

fn optional_lexical_failure_has_fallback(
    successful_lexical_segments: usize,
    direct_evidence_count: usize,
) -> bool {
    successful_lexical_segments > 0 || direct_evidence_count > 0
}

#[cfg(test)]
mod downstream_budget_tests {
    use super::*;

    #[test]
    fn postgres_timeout_respects_deadline_budget() {
        assert_eq!(postgres_statement_timeout_ms(5_000, 800, 50).unwrap(), 750);
        assert_eq!(postgres_statement_timeout_ms(500, 800, 50).unwrap(), 500);
        assert!(postgres_statement_timeout_ms(5_000, 50, 50).is_err());
    }

    #[test]
    fn only_transient_lexical_failures_are_degradable() {
        assert!(optional_lexical_failure_can_degrade(
            &Status::deadline_exceeded("statement timeout")
        ));
        assert!(optional_lexical_failure_can_degrade(&Status::unavailable(
            "postgres unavailable"
        )));
        assert!(!optional_lexical_failure_can_degrade(&Status::cancelled(
            "client cancelled"
        )));
        assert!(!optional_lexical_failure_can_degrade(&Status::internal(
            "query defect"
        )));
        assert!(optional_lexical_failure_has_fallback(0, 1));
        assert!(optional_lexical_failure_has_fallback(1, 0));
        assert!(!optional_lexical_failure_has_fallback(0, 0));
    }

    #[test]
    fn equal_score_order_is_deterministic() {
        let result = |document: &str, version: u64, chunk: &str| pb::SearchResultV004 {
            document_id: document.into(),
            document_version: version,
            matched_chunk_id: chunk.into(),
            scores: Some(pb::SearchScoresV004 {
                final_score: 0.5,
                fusion_score: 0.4,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut values = [
            result("doc-b", 1, "chunk-a"),
            result("doc-a", 1, "chunk-b"),
            result("doc-a", 2, "chunk-c"),
            result("doc-a", 2, "chunk-a"),
        ];
        values.sort_by(stable_result_rank);
        let identity = values
            .iter()
            .map(|value| {
                (
                    value.document_id.as_str(),
                    value.document_version,
                    value.matched_chunk_id.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            identity,
            vec![
                ("doc-a", 2, "chunk-a"),
                ("doc-a", 2, "chunk-c"),
                ("doc-a", 1, "chunk-b"),
                ("doc-b", 1, "chunk-a"),
            ]
        );
    }

    #[test]
    fn equal_qdrant_scores_use_point_id_tiebreak() {
        let hit = |id: &str| QdrantSearchHit {
            id: Uuid::parse_str(id).unwrap(),
            score: 0.5,
            dense_score: 0.5,
            sparse_score: 0.0,
            fusion_score: 0.5,
            dense_rank: None,
            sparse_rank: None,
            payload: Default::default(),
        };
        let mut values = [
            hit("00000000-0000-0000-0000-000000000002"),
            hit("00000000-0000-0000-0000-000000000001"),
        ];
        values.sort_by(stable_qdrant_hit_rank);
        assert_eq!(
            values[0].id,
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
        );
    }
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        gauge!("astravector_admission_in_flight", "scope" => self.scope).decrement(self.weight);
    }
}

#[derive(Clone)]
pub struct AstraVectorV004ControlService {
    cfg: Arc<AppConfig>,
    scheduler: Scheduler,
    repo: Option<Repository>,
    qdrant: Option<Arc<QdrantClient>>,
    engine: Arc<dyn InferenceEngine>,
    shutdown: CancellationToken,
    retrieve_context_semaphore: Arc<Semaphore>,
    graph_expansion_semaphore: Arc<Semaphore>,
    mmr_fetch_semaphore: Arc<Semaphore>,
    hydration_failpoints: Arc<HydrationFailpointPlan>,
}

impl AstraVectorV004ControlService {
    pub fn new(
        cfg: Arc<AppConfig>,
        scheduler: Scheduler,
        repo: Option<Repository>,
        qdrant: Option<Arc<QdrantClient>>,
        engine: Arc<dyn InferenceEngine>,
        shutdown: CancellationToken,
    ) -> Self {
        register_query_observability_metrics();
        let retrieve_context_semaphore = Arc::new(Semaphore::new(
            cfg.limits.max_concurrent_retrieve_context.max(1),
        ));
        let graph_expansion_semaphore = Arc::new(Semaphore::new(
            cfg.limits.max_concurrent_graph_expansion.max(1),
        ));
        let mmr_fetch_semaphore =
            Arc::new(Semaphore::new(cfg.limits.max_concurrent_mmr_fetch.max(1)));
        let hydration_failpoint_plan = HydrationFailpointPlan::load(
            cfg.search.hydration_failpoints.non_production_enabled,
            &cfg.search.hydration_failpoints.plan_path,
            &cfg.search.hydration_failpoints.run_id,
        )
        .unwrap_or_else(|error| panic!("invalid hydration failpoint startup plan: {error}"));
        tracing::info!(
            non_production_enabled = hydration_failpoint_plan.non_production_enabled,
            rule_count = hydration_failpoint_plan.rule_count(),
            run_id = %cfg.search.hydration_failpoints.run_id,
            "HYDRATION_FAILPOINT_PLAN_RESOLVED"
        );
        let hydration_failpoints = Arc::new(hydration_failpoint_plan);
        Self {
            cfg,
            scheduler,
            repo,
            qdrant,
            engine,
            shutdown,
            retrieve_context_semaphore,
            graph_expansion_semaphore,
            mmr_fetch_semaphore,
            hydration_failpoints,
        }
    }

    async fn acquire_backpressure_permit(
        semaphore: Arc<Semaphore>,
        timeout_ms: u64,
        metric_scope: &'static str,
        weight: u32,
        cancellation: &CancellationToken,
    ) -> Result<AdmissionPermit, Status> {
        let started = std::time::Instant::now();
        let weight = weight.max(1);
        let acquire = tokio::time::timeout(
            Duration::from_millis(timeout_ms.max(1)),
            semaphore.acquire_many_owned(weight),
        );
        let result = tokio::select! {
            _ = cancellation.cancelled() => {
                return Err(Status::cancelled(format!("{metric_scope}_admission_cancelled")));
            }
            result = acquire => result.map_err(|_| {
                counter!("backpressure_acquire_timeout_total", "scope" => metric_scope).increment(1);
                counter!("retrieval_rejected_total", "scope" => metric_scope, "reason" => "acquire_timeout").increment(1);
                counter!("astravector_admission_rejected_total", "scope" => metric_scope, "reason" => "admission_timeout").increment(1);
                Status::resource_exhausted(format!("{metric_scope}_admission_timeout"))
            })?
        }.map_err(|_| Status::unavailable(format!("{metric_scope} semaphore closed")));
        histogram!("astravector_admission_wait_seconds", "scope" => metric_scope)
            .record(started.elapsed().as_secs_f64());
        let permit = result?;
        gauge!("astravector_admission_in_flight", "scope" => metric_scope).increment(weight as f64);
        Ok(AdmissionPermit {
            _permit: permit,
            scope: metric_scope,
            weight: weight as f64,
        })
    }

    #[allow(clippy::result_large_err)]
    fn repo(&self) -> Result<&Repository, Status> {
        self.repo
            .as_ref()
            .ok_or_else(|| Status::unavailable("PostgreSQL repository is not configured"))
    }

    #[allow(clippy::result_large_err)]
    fn qdrant(&self) -> Result<&Arc<QdrantClient>, Status> {
        self.qdrant
            .as_ref()
            .ok_or_else(|| Status::unavailable("Qdrant client is not configured"))
    }

    #[allow(clippy::too_many_arguments)]
    async fn generate_direct_qdrant_hits(
        &self,
        plan: &QueryPlan,
        access_zone_ids: &[Uuid],
        caller_access_level: pb::AccessLevel,
        candidate_limit: u32,
        top_k: u32,
        search_mode: pb::SearchModeV005,
        wants_dense: bool,
        wants_sparse: bool,
        sparse_available: bool,
        sparse_required: bool,
        version_filters: &QdrantVersionFilters,
        deadline: Instant,
        qdrant_budget: &OperationBudget,
        request_cancel: CancellationToken,
        warnings: &mut Vec<pb::DiagnosticWarningV005>,
    ) -> Result<DirectQdrantGeneration, Status> {
        match plan.mode {
            QueryProcessingMode::Single => {
                self.generate_single_direct_qdrant_hits(
                    plan,
                    access_zone_ids,
                    caller_access_level,
                    candidate_limit,
                    search_mode,
                    wants_dense,
                    wants_sparse,
                    sparse_available,
                    sparse_required,
                    version_filters,
                    deadline,
                    qdrant_budget,
                    request_cancel,
                    warnings,
                )
                .await
            }
            QueryProcessingMode::Segmented => {
                self.generate_segmented_direct_qdrant_hits(
                    plan,
                    access_zone_ids,
                    caller_access_level,
                    candidate_limit,
                    top_k,
                    search_mode,
                    wants_dense,
                    wants_sparse,
                    sparse_available,
                    sparse_required,
                    version_filters,
                    deadline,
                    qdrant_budget,
                    request_cancel,
                    warnings,
                )
                .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn generate_single_direct_qdrant_hits(
        &self,
        plan: &QueryPlan,
        access_zone_ids: &[Uuid],
        caller_access_level: pb::AccessLevel,
        candidate_limit: u32,
        search_mode: pb::SearchModeV005,
        wants_dense: bool,
        wants_sparse: bool,
        sparse_available: bool,
        sparse_required: bool,
        version_filters: &QdrantVersionFilters,
        deadline: Instant,
        qdrant_budget: &OperationBudget,
        request_cancel: CancellationToken,
        warnings: &mut Vec<pb::DiagnosticWarningV005>,
    ) -> Result<DirectQdrantGeneration, Status> {
        let query = plan.original_query.as_str();
        let emb_started = std::time::Instant::now();
        let embedding = self
            .scheduler
            .submit(
                QueueKind::Query,
                InferenceInput {
                    text: query.to_string(),
                    max_length: self.cfg.tokenization.query.max_length,
                    allow_truncation: false,
                    want_dense: wants_dense,
                    want_sparse: wants_sparse && sparse_available,
                    token_count_hint: plan.original_token_count,
                },
                deadline,
                request_cancel,
            )
            .await
            .map_err(Status::from)?;
        if embedding.truncated {
            return Err(Status::internal(
                "UNEXPECTED_QUERY_TRUNCATION: single query embedding was truncated",
            ));
        }
        let query_embedding_ms = emb_started.elapsed().as_millis() as u64;
        let qdrant_started = std::time::Instant::now();
        let qdrant = self.qdrant()?.clone();
        let dense_vector = embedding.dense.clone();
        let sparse_indices = embedding.sparse_indices.clone();
        let sparse_values = embedding.sparse_values.clone();
        let dense_dim = dense_vector.as_ref().map(|v| v.len()).unwrap_or(0);
        let dense_norm = dense_vector
            .as_ref()
            .map(|v| {
                v.iter()
                    .map(|x| (*x as f64) * (*x as f64))
                    .sum::<f64>()
                    .sqrt()
            })
            .unwrap_or(0.0);
        tracing::debug!(
            query_embedding_ms,
            dense_dim,
            dense_norm,
            sparse_terms = sparse_indices.as_ref().map(|v| v.len()).unwrap_or(0),
            "SEARCH_QUERY_EMBEDDING_READY"
        );
        let dense_access_zone_ids = access_zone_ids.to_vec();
        let sparse_access_zone_ids = access_zone_ids.to_vec();
        let dense_future = {
            let qdrant = qdrant.clone();
            let version_filters = version_filters.clone();
            let budget = qdrant_budget.clone();
            async move {
                if !wants_dense {
                    return Ok::<Option<(Vec<QdrantSearchHit>, u64)>, Status>(None);
                }
                let Some(dense) = dense_vector.as_deref() else {
                    return Err(Status::failed_precondition(
                        "query dense embedding unavailable",
                    ));
                };
                let branch_started = Instant::now();
                let hits = qdrant
                    .search_dense_with_budget(
                        dense,
                        &dense_access_zone_ids,
                        caller_access_level as i16,
                        candidate_limit as usize,
                        Some(&version_filters),
                        Some(&budget),
                    )
                    .await
                    .map_err(Status::from)?;
                Ok(Some((hits, branch_started.elapsed().as_millis() as u64)))
            }
        };
        let sparse_future = {
            let qdrant = qdrant.clone();
            let version_filters = version_filters.clone();
            let budget = qdrant_budget.clone();
            async move {
                if !wants_sparse {
                    return Ok::<Option<(Vec<QdrantSearchHit>, u64)>, Status>(None);
                }
                let branch_started = Instant::now();
                match (sparse_indices.as_deref(), sparse_values.as_deref()) {
                    (Some(indices), Some(values)) if !indices.is_empty() && !values.is_empty() => {
                        qdrant
                            .search_sparse_with_budget(
                                indices,
                                values,
                                &sparse_access_zone_ids,
                                caller_access_level as i16,
                                candidate_limit as usize,
                                Some(&version_filters),
                                Some(&budget),
                            )
                            .await
                            .map(|hits| Some((hits, branch_started.elapsed().as_millis() as u64)))
                            .map_err(Status::from)
                    }
                    _ if sparse_required => Err(Status::failed_precondition(
                        "SPARSE_UNAVAILABLE: query sparse embedding is empty or unavailable",
                    )),
                    _ => Ok(Some((
                        Vec::new(),
                        branch_started.elapsed().as_millis() as u64,
                    ))),
                }
            }
        };
        let (dense_result, sparse_result) = tokio::join!(dense_future, sparse_future);
        let mut dense_failed = false;
        let mut sparse_failed = false;
        let mut dense_status = None;
        let mut sparse_status = None;
        let (mut dense_hits, dense_search_ms) = match dense_result {
            Ok(Some((hits, duration_ms))) => {
                dense_status = Some(success_branch_status(&hits));
                (hits, duration_ms)
            }
            Ok(None) => (Vec::new(), 0),
            Err(e) => {
                dense_failed = true;
                dense_status = Some(failed_branch_status(&e));
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "DENSE_SEARCH_FAILED".into(),
                    message: format!("Dense Qdrant search failed: {}", e.message()),
                });
                (Vec::new(), 0)
            }
        };
        let (mut sparse_hits, sparse_search_ms) = match sparse_result {
            Ok(Some((hits, duration_ms))) => {
                sparse_status = Some(success_branch_status(&hits));
                (hits, duration_ms)
            }
            Ok(None) => (Vec::new(), 0),
            Err(e) if sparse_required => return Err(e),
            Err(e) => {
                sparse_failed = true;
                sparse_status = Some(failed_branch_status(&e));
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "SPARSE_SEARCH_FAILED".into(),
                    message: format!("Sparse Qdrant search failed: {}", e.message()),
                });
                (Vec::new(), 0)
            }
        };
        dense_hits.sort_by(stable_qdrant_hit_rank);
        sparse_hits.sort_by(stable_qdrant_hit_rank);
        let dense_branch_executed = wants_dense && !dense_failed;
        let sparse_branch_executed = wants_sparse && !sparse_failed;
        let fusion_executed = search_mode == pb::SearchModeV005::Hybrid
            && dense_branch_executed
            && sparse_branch_executed;
        let dense_branch_candidate_count = dense_hits.len() as u32;
        let sparse_branch_candidate_count = sparse_hits.len() as u32;
        let sparse_top_score = sparse_hits.first().map(|hit| hit.score).unwrap_or(0.0);
        let fusion_started = Instant::now();
        let hits = Self::select_branch_hits(
            dense_hits,
            sparse_hits,
            candidate_limit,
            search_mode,
            dense_failed,
            sparse_failed,
            warnings,
            &self.cfg,
        )?;
        let fusion_ms = fusion_started.elapsed().as_millis() as u64;
        let qdrant_search_ms = qdrant_started.elapsed().as_millis() as u64;
        let branch_statuses = [dense_status, sparse_status]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let retrieval_status = summarize_retrieval_statuses(branch_statuses.iter().copied());
        Ok(DirectQdrantGeneration {
            fusion_candidate_count: hits.len() as u32,
            hits,
            query_embedding_ms,
            qdrant_search_ms,
            dense_search_ms,
            sparse_search_ms,
            fusion_ms,
            dense_branch_executed,
            sparse_branch_executed,
            fusion_executed,
            dense_branch_candidate_count,
            sparse_branch_candidate_count,
            sparse_top_score,
            dense_failed,
            sparse_failed,
            branch_statuses,
            retrieval_status,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn generate_segmented_direct_qdrant_hits(
        &self,
        plan: &QueryPlan,
        access_zone_ids: &[Uuid],
        caller_access_level: pb::AccessLevel,
        candidate_limit: u32,
        top_k: u32,
        search_mode: pb::SearchModeV005,
        wants_dense: bool,
        wants_sparse: bool,
        sparse_available: bool,
        sparse_required: bool,
        version_filters: &QdrantVersionFilters,
        deadline: Instant,
        qdrant_budget: &OperationBudget,
        request_cancel: CancellationToken,
        warnings: &mut Vec<pb::DiagnosticWarningV005>,
    ) -> Result<DirectQdrantGeneration, Status> {
        warnings.push(pb::DiagnosticWarningV005 {
            code: match plan.tier {
                QueryProcessingTier::SegmentedExtended => "LONG_QUERY_SEGMENTED_EXTENDED",
                _ => "LONG_QUERY_SEGMENTED_STANDARD",
            }
            .into(),
            message: format!(
                "query processed as {} with {} bounded segments",
                plan.tier.code(),
                plan.segments.len()
            ),
        });
        let emb_started = std::time::Instant::now();
        let inputs = plan
            .segments
            .iter()
            .map(|segment| InferenceInput {
                text: segment.text.clone(),
                max_length: plan.limits.segment_max_tokens,
                allow_truncation: false,
                want_dense: wants_dense,
                want_sparse: wants_sparse && sparse_available,
                token_count_hint: segment.token_count,
            })
            .collect::<Vec<_>>();
        let embeddings = self
            .scheduler
            .submit_many(
                QueueKind::Query,
                inputs,
                deadline,
                request_cancel,
                SubmitManyOptions {
                    max_in_flight: plan.limits.max_parallel_segments,
                    preserve_order: true,
                    cancel_on_error: true,
                },
            )
            .await
            .map_err(Status::from)?;
        if embeddings.len() != plan.segments.len() {
            return Err(Status::internal(
                "QUERY_SEGMENT_EMBEDDING_FAILED: embedding count mismatch",
            ));
        }
        if embeddings.iter().any(|embedding| embedding.truncated) {
            return Err(Status::internal(
                "UNEXPECTED_QUERY_TRUNCATION: segmented query embedding was truncated",
            ));
        }
        let query_embedding_ms = emb_started.elapsed().as_millis() as u64;
        let qdrant_started = std::time::Instant::now();
        let per_segment_limit = plan
            .limits
            .local_fused_candidate_limit
            .min(candidate_limit)
            .min(self.cfg.limits.search_candidate_limit_max)
            .max(top_k.min(candidate_limit))
            .max(1) as usize;
        let global_limit = candidate_limit
            .min(plan.limits.global_fused_candidate_limit)
            .max(1) as usize;
        let mut dense_failed = false;
        let mut sparse_failed = false;
        let mut dense_search_ms = 0_u64;
        let mut sparse_search_ms = 0_u64;
        let mut dense_branch_candidate_count = 0_u32;
        let mut sparse_branch_candidate_count = 0_u32;
        let mut sparse_top_score = 0.0_f32;
        let mut all_dense_executed = wants_dense;
        let mut all_sparse_executed = wants_sparse;
        let mut all_fusion_executed = search_mode == pb::SearchModeV005::Hybrid;
        let mut segment_fusion_ms = 0_u64;
        let mut branch_statuses = Vec::new();
        let mut segment_candidates = Vec::new();
        let mut best_hits =
            HashMap::<GlobalCandidateIdentity, (QdrantSearchHit, f32, usize)>::new();
        let fusion_started = Instant::now();
        let segment_inputs = plan
            .segments
            .iter()
            .cloned()
            .zip(embeddings)
            .collect::<Vec<_>>();
        let mut segment_results = stream::iter(segment_inputs)
            .map(|(segment, embedding)| {
                let budget = qdrant_budget.clone();
                async move {
                    self.retrieve_qdrant_for_segment(
                        &segment,
                        &embedding,
                        access_zone_ids,
                        caller_access_level,
                        per_segment_limit,
                        search_mode,
                        wants_dense,
                        wants_sparse,
                        sparse_required,
                        version_filters,
                        &budget,
                    )
                    .await
                }
            })
            .buffer_unordered(plan.limits.max_parallel_segments.max(1))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, Status>>()?;
        segment_results.sort_by_key(|result| result.segment_index);
        for result in segment_results {
            let intent_unit_ids = result.intent_unit_ids.clone();
            branch_statuses.extend(
                [result.dense_status, result.sparse_status]
                    .into_iter()
                    .flatten(),
            );
            dense_failed |= result.dense_failed;
            sparse_failed |= result.sparse_failed;
            all_dense_executed &= result.dense_executed;
            all_sparse_executed &= result.sparse_executed;
            all_fusion_executed &= result.fusion_executed;
            dense_search_ms += result.dense_ms;
            sparse_search_ms += result.sparse_ms;
            segment_fusion_ms += result.fusion_ms;
            histogram!("astravector_long_query_segment_retrieval_duration_seconds")
                .record((result.dense_ms + result.sparse_ms + result.fusion_ms) as f64 / 1_000.0);
            dense_branch_candidate_count += result.dense_candidates as u32;
            sparse_branch_candidate_count += result.sparse_candidates as u32;
            sparse_top_score = sparse_top_score.max(result.sparse_top_score);
            warnings.extend(result.warnings);
            for (rank, hit) in result.hits.into_iter().enumerate() {
                let Some(identity) = qdrant_hit_identity(&hit) else {
                    continue;
                };
                let local_rank = rank + 1;
                let contribution = result.segment_weight
                    / (self.cfg.search.query_processing.segment_rrf_k + rank as f32 + 1.0);
                best_hits
                    .entry(identity.clone())
                    .and_modify(|(best_hit, best_contribution, best_local_rank)| {
                        if contribution > *best_contribution
                            || (contribution == *best_contribution && local_rank < *best_local_rank)
                        {
                            *best_contribution = contribution;
                            *best_local_rank = local_rank;
                            *best_hit = hit.clone();
                        }
                    })
                    .or_insert_with(|| (hit.clone(), contribution, local_rank));
                segment_candidates.push(SegmentCandidate {
                    identity,
                    segment_index: result.segment_index,
                    rank: local_rank,
                    score: hit.score,
                    segment_weight: result.segment_weight,
                    intent_unit_ids: intent_unit_ids.clone(),
                });
            }
        }
        let fused = cross_segment_rrf(
            segment_candidates,
            self.cfg.search.query_processing.segment_rrf_k,
            global_limit,
        );
        let hits = fused
            .into_iter()
            .filter_map(|candidate| {
                let (mut hit, _, _) = best_hits.remove(&candidate.identity)?;
                hit.score = candidate.score;
                hit.fusion_score = candidate.score;
                annotate_segmented_hit(&mut hit, &candidate.matched_segments.into_iter().collect());
                Some(hit)
            })
            .collect::<Vec<_>>();
        let fusion_ms = fusion_started.elapsed().as_millis() as u64 + segment_fusion_ms;
        let qdrant_search_ms = qdrant_started.elapsed().as_millis() as u64;
        let retrieval_status = summarize_retrieval_statuses(branch_statuses.iter().copied());
        Ok(DirectQdrantGeneration {
            fusion_candidate_count: hits.len() as u32,
            hits,
            query_embedding_ms,
            qdrant_search_ms,
            dense_search_ms,
            sparse_search_ms,
            fusion_ms,
            dense_branch_executed: all_dense_executed,
            sparse_branch_executed: all_sparse_executed,
            fusion_executed: all_fusion_executed,
            dense_branch_candidate_count,
            sparse_branch_candidate_count,
            sparse_top_score,
            dense_failed,
            sparse_failed,
            branch_statuses,
            retrieval_status,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn retrieve_qdrant_for_segment(
        &self,
        segment: &QuerySegment,
        embedding: &EmbeddingResult,
        access_zone_ids: &[Uuid],
        caller_access_level: pb::AccessLevel,
        per_segment_limit: usize,
        search_mode: pb::SearchModeV005,
        wants_dense: bool,
        wants_sparse: bool,
        sparse_required: bool,
        version_filters: &QdrantVersionFilters,
        qdrant_budget: &OperationBudget,
    ) -> Result<SegmentQdrantResult, Status> {
        let qdrant = self.qdrant()?.clone();
        let mut warnings = Vec::new();
        let dense_started = Instant::now();
        let dense_result = if wants_dense {
            let dense = embedding
                .dense
                .as_deref()
                .ok_or_else(|| Status::failed_precondition("query dense embedding unavailable"))?;
            Some(
                qdrant
                    .search_dense_with_budget(
                        dense,
                        access_zone_ids,
                        caller_access_level as i16,
                        per_segment_limit,
                        Some(version_filters),
                        Some(qdrant_budget),
                    )
                    .await,
            )
        } else {
            None
        };
        let dense_ms = if wants_dense {
            dense_started.elapsed().as_millis() as u64
        } else {
            0
        };
        let mut dense_failed = false;
        let mut dense_status = None;
        let mut dense_hits = match dense_result {
            Some(Ok(hits)) => {
                dense_status = Some(success_branch_status(&hits));
                hits
            }
            Some(Err(error)) => {
                dense_failed = true;
                dense_status = Some(failed_branch_status(&Status::from(error.clone())));
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "DENSE_SEARCH_FAILED".into(),
                    message: format!(
                        "Dense Qdrant search failed for segment {}: {error}",
                        segment.index
                    ),
                });
                Vec::new()
            }
            None => Vec::new(),
        };

        let sparse_started = Instant::now();
        let sparse_result = if wants_sparse {
            match (
                embedding.sparse_indices.as_deref(),
                embedding.sparse_values.as_deref(),
            ) {
                (Some(indices), Some(values)) if !indices.is_empty() && !values.is_empty() => Some(
                    qdrant
                        .search_sparse_with_budget(
                            indices,
                            values,
                            access_zone_ids,
                            caller_access_level as i16,
                            per_segment_limit,
                            Some(version_filters),
                            Some(qdrant_budget),
                        )
                        .await,
                ),
                _ if sparse_required => {
                    return Err(Status::failed_precondition(
                        "SPARSE_UNAVAILABLE: query sparse embedding is empty or unavailable",
                    ));
                }
                _ => None,
            }
        } else {
            None
        };
        let sparse_ms = if wants_sparse {
            sparse_started.elapsed().as_millis() as u64
        } else {
            0
        };
        let mut sparse_failed = false;
        let mut sparse_status = None;
        let mut sparse_hits = match sparse_result {
            Some(Ok(hits)) => {
                sparse_status = Some(success_branch_status(&hits));
                hits
            }
            Some(Err(error)) if sparse_required => return Err(Status::from(error)),
            Some(Err(error)) => {
                sparse_failed = true;
                sparse_status = Some(failed_branch_status(&Status::from(error.clone())));
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "SPARSE_SEARCH_FAILED".into(),
                    message: format!(
                        "Sparse Qdrant search failed for segment {}: {error}",
                        segment.index
                    ),
                });
                Vec::new()
            }
            None => Vec::new(),
        };
        dense_hits.sort_by(stable_qdrant_hit_rank);
        sparse_hits.sort_by(stable_qdrant_hit_rank);
        let dense_candidates = dense_hits.len();
        let sparse_candidates = sparse_hits.len();
        let sparse_top_score = sparse_hits.first().map(|hit| hit.score).unwrap_or(0.0);
        let fusion_started = Instant::now();
        let hits = Self::select_branch_hits(
            dense_hits,
            sparse_hits,
            per_segment_limit as u32,
            search_mode,
            dense_failed,
            sparse_failed,
            &mut warnings,
            &self.cfg,
        )?;
        Ok(SegmentQdrantResult {
            segment_index: segment.index,
            segment_weight: segment.weight,
            intent_unit_ids: segment.intent_unit_ids.clone(),
            hits,
            dense_executed: wants_dense && !dense_failed,
            sparse_executed: wants_sparse && !sparse_failed,
            fusion_executed: search_mode == pb::SearchModeV005::Hybrid
                && wants_dense
                && wants_sparse
                && !dense_failed
                && !sparse_failed,
            dense_failed,
            sparse_failed,
            dense_status,
            sparse_status,
            dense_candidates,
            sparse_candidates,
            dense_ms,
            sparse_ms,
            fusion_ms: fusion_started.elapsed().as_millis() as u64,
            sparse_top_score,
            warnings,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn retrieve_lexical_for_segment(
        &self,
        segment: &QuerySegment,
        access_zone_ids: &[Uuid],
        caller_access_level: pb::AccessLevel,
        quality_run_id_filter: Option<&str>,
        lexical_limit: i64,
        deadline: Instant,
        request_cancel: CancellationToken,
    ) -> Result<SegmentLexicalResult, Status> {
        let remaining_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        let usable_ms = remaining_ms.saturating_sub(self.cfg.search.lexical.response_reserve_ms);
        if usable_ms < self.cfg.search.lexical.min_remaining_budget_ms {
            return Ok(SegmentLexicalResult {
                segment_index: segment.index,
                segment_text: segment.text.clone(),
                segment_weight: segment.weight,
                candidates: Vec::new(),
                duration_ms: 0,
                warnings: vec![pb::DiagnosticWarningV005 {
                    code: "QUERY_SEGMENT_FTS_SKIPPED".into(),
                    message: format!(
                        "PostgreSQL FTS skipped for segment {} because request budget is insufficient",
                        segment.index
                    ),
                }],
            });
        }
        let statement_timeout_ms = self
            .cfg
            .search
            .lexical
            .statement_timeout_ms
            .min(usable_ms)
            .max(1);
        let started = Instant::now();
        let search = self.repo()?.search_active_parent_contexts_lexical_multi(
            access_zone_ids,
            caller_access_level as i16,
            &segment.text,
            quality_run_id_filter,
            lexical_limit,
            statement_timeout_ms,
        );
        let candidates = tokio::select! {
            _ = request_cancel.cancelled() => {
                return Err(Status::cancelled("query cancelled during PostgreSQL FTS"));
            }
            result = search => result.map_err(Status::from)?,
        };
        Ok(SegmentLexicalResult {
            segment_index: segment.index,
            segment_text: segment.text.clone(),
            segment_weight: segment.weight,
            candidates,
            duration_ms: started.elapsed().as_millis() as u64,
            warnings: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn select_branch_hits(
        dense_hits: Vec<QdrantSearchHit>,
        sparse_hits: Vec<QdrantSearchHit>,
        candidate_limit: u32,
        search_mode: pb::SearchModeV005,
        dense_failed: bool,
        sparse_failed: bool,
        warnings: &mut Vec<pb::DiagnosticWarningV005>,
        cfg: &AppConfig,
    ) -> Result<Vec<QdrantSearchHit>, Status> {
        match search_mode {
            pb::SearchModeV005::Dense => {
                if dense_hits.is_empty() && dense_failed {
                    return Err(Status::unavailable(
                        "QDRANT_SEARCH_UNAVAILABLE: dense search failed",
                    ));
                }
                Ok(dense_hits)
            }
            pb::SearchModeV005::Sparse => {
                if sparse_hits.is_empty() && sparse_failed {
                    return Err(Status::unavailable(
                        "QDRANT_SEARCH_UNAVAILABLE: sparse search failed",
                    ));
                }
                Ok(sparse_hits)
            }
            _ => {
                if dense_hits.is_empty()
                    && sparse_hits.is_empty()
                    && (dense_failed || sparse_failed)
                {
                    return Err(Status::unavailable(
                        "QDRANT_SEARCH_UNAVAILABLE: dense and sparse search unavailable",
                    ));
                }
                if !dense_hits.is_empty() && !sparse_hits.is_empty() {
                    Ok(fuse_qdrant_hits(
                        dense_hits,
                        sparse_hits,
                        candidate_limit as usize,
                        &cfg.search.hybrid_fusion_method,
                        cfg.search.hybrid_dense_weight,
                        cfg.search.hybrid_sparse_weight,
                        cfg.search.rrf_k,
                    ))
                } else if !dense_hits.is_empty() {
                    if sparse_failed {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "SPARSE_SEARCH_FAILED_FALLBACK_TO_DENSE".into(),
                            message: "Sparse search failed; returning dense-only retrieval results"
                                .into(),
                        });
                    }
                    Ok(dense_hits)
                } else {
                    if dense_failed {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "DENSE_SEARCH_FAILED_FALLBACK_TO_SPARSE".into(),
                            message: "Dense search failed; returning sparse-only retrieval results"
                                .into(),
                        });
                    }
                    Ok(sparse_hits)
                }
            }
        }
    }

    fn require_trusted_forwarded_identity_headers(
        &self,
        metadata: &MetadataMap,
    ) -> Result<(), Status> {
        if !self.cfg.security.enabled {
            return Ok(());
        }
        if !self.cfg.security.trust_forwarded_identity_headers {
            counter!("security_forwarded_identity_rejected_total", "reason" => "disabled")
                .increment(1);
            return Err(Status::permission_denied(
                "forwarded identity headers are not trusted by this AstraVector instance",
            ));
        }
        let expected = self.cfg.security.gateway_trust_token.as_bytes();
        if expected.is_empty() {
            counter!("security_forwarded_identity_rejected_total", "reason" => "missing_expected_token").increment(1);
            return Err(Status::permission_denied(
                "trusted gateway token is not configured",
            ));
        }
        let header_name = self.cfg.security.gateway_trust_header.as_str();
        let Ok(header_key) = MetadataKey::from_bytes(header_name.as_bytes()) else {
            counter!("security_forwarded_identity_rejected_total", "reason" => "invalid_gateway_header").increment(1);
            return Err(Status::permission_denied(
                "trusted gateway header name is invalid",
            ));
        };
        let presented = metadata
            .get(&header_key)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .as_bytes()
            .to_vec();
        if presented.as_slice().ct_eq(expected).unwrap_u8() != 1 {
            counter!("security_forwarded_identity_rejected_total", "reason" => "bad_gateway_token")
                .increment(1);
            return Err(Status::permission_denied(
                "trusted gateway identity proof is required",
            ));
        }
        Ok(())
    }

    fn require_internal_or_admin(&self, metadata: &MetadataMap) -> Result<(), Status> {
        if !self.cfg.security.enabled {
            return Ok(());
        }
        self.require_trusted_forwarded_identity_headers(metadata)?;
        let role = metadata
            .get("x-astravector-role")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if role.eq_ignore_ascii_case("admin") || role.eq_ignore_ascii_case("internal") {
            Ok(())
        } else {
            Err(Status::permission_denied("internal/admin role is required"))
        }
    }

    fn require_admin(&self, metadata: &MetadataMap) -> Result<(), Status> {
        if !self.cfg.security.enabled {
            return Ok(());
        }
        self.require_trusted_forwarded_identity_headers(metadata)?;
        let role = metadata
            .get("x-astravector-role")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if role.eq_ignore_ascii_case("admin") {
            Ok(())
        } else {
            Err(Status::permission_denied("admin role is required"))
        }
    }

    fn embedding_mode_requires_sparse(mode: i32, default_required: bool) -> bool {
        match pb::EmbeddingModeV005::try_from(mode).unwrap_or(pb::EmbeddingModeV005::Unspecified) {
            pb::EmbeddingModeV005::DenseSparseRequired => true,
            pb::EmbeddingModeV005::DenseSparseIfAvailable => false,
            pb::EmbeddingModeV005::DenseOnly => false,
            pb::EmbeddingModeV005::Unspecified => default_required,
        }
    }

    fn embedding_mode_wants_sparse(mode: i32, default_enabled: bool) -> bool {
        match pb::EmbeddingModeV005::try_from(mode).unwrap_or(pb::EmbeddingModeV005::Unspecified) {
            pb::EmbeddingModeV005::DenseOnly => false,
            pb::EmbeddingModeV005::DenseSparseIfAvailable
            | pb::EmbeddingModeV005::DenseSparseRequired => true,
            pb::EmbeddingModeV005::Unspecified => default_enabled,
        }
    }

    fn resolve_search_mode(mode: i32, configured_default: &str) -> pb::SearchModeV005 {
        let explicit =
            pb::SearchModeV005::try_from(mode).unwrap_or(pb::SearchModeV005::Unspecified);
        if explicit != pb::SearchModeV005::Unspecified {
            return explicit;
        }
        match configured_default.to_ascii_uppercase().as_str() {
            "DENSE" => pb::SearchModeV005::Dense,
            "SPARSE" => pb::SearchModeV005::Sparse,
            _ => pb::SearchModeV005::Hybrid,
        }
    }

    fn validate_access_zone_id_format(value: &str, max_len: usize) -> bool {
        let bytes = value.as_bytes();
        if bytes.is_empty() || bytes.len() > max_len {
            return false;
        }
        let first = bytes[0] as char;
        if !first.is_ascii_alphanumeric() {
            return false;
        }
        value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/'))
    }

    async fn resolve_search_access_zones(
        &self,
        legacy_access_zone_id: &str,
        access_zone_ids: &[String],
        access_zone_code: &str,
        access_zone_codes: &[String],
    ) -> Result<Vec<access_zone_registry::ResolvedAccessZone>, Status> {
        if !self.cfg.access_zones.allow_multi_zone_search
            && (access_zone_ids.len() > 1 || access_zone_codes.len() > 1)
        {
            counter!("access_zone_search_rejected_total", "reason" => "multi_zone_disabled")
                .increment(1);
            return Err(Status::invalid_argument("multi-zone search is disabled"));
        }
        access_zone_registry::resolve_request_zones(
            &self.repo()?.pool,
            &self.cfg,
            legacy_access_zone_id,
            access_zone_ids,
            access_zone_code,
            access_zone_codes,
        )
        .await
    }

    async fn resolve_ingestion_access_zone(
        &self,
        legacy_access_zone_id: &str,
        access_zone_code: &str,
    ) -> Result<access_zone_registry::ResolvedAccessZone, Status> {
        access_zone_registry::resolve_or_create_ingestion_zone(
            &self.repo()?.pool,
            &self.cfg,
            legacy_access_zone_id,
            access_zone_code,
        )
        .await
    }

    fn version_filters_from_search_request(r: &pb::SearchRequestV004) -> QdrantVersionFilters {
        QdrantVersionFilters {
            model_version: r.model_version.clone().filter(|v| !v.trim().is_empty()),
            tokenizer_version: r.tokenizer_version.clone().filter(|v| !v.trim().is_empty()),
            dense_version: r.dense_version.clone().filter(|v| !v.trim().is_empty()),
            sparse_version: r.sparse_version.clone().filter(|v| !v.trim().is_empty()),
            chunking_version: r.chunking_version.clone().filter(|v| !v.trim().is_empty()),
            payload_filters: Self::payload_filters_from_request(&r.filters),
        }
    }

    fn version_filters_from_explain_request(r: &pb::ExplainSearchRequest) -> QdrantVersionFilters {
        QdrantVersionFilters {
            model_version: r.model_version.clone().filter(|v| !v.trim().is_empty()),
            tokenizer_version: r.tokenizer_version.clone().filter(|v| !v.trim().is_empty()),
            dense_version: r.dense_version.clone().filter(|v| !v.trim().is_empty()),
            sparse_version: r.sparse_version.clone().filter(|v| !v.trim().is_empty()),
            chunking_version: r.chunking_version.clone().filter(|v| !v.trim().is_empty()),
            payload_filters: Vec::new(),
        }
    }

    fn payload_filters_from_request(filters: &[pb::SearchFilterV004]) -> Vec<(String, String)> {
        filters
            .iter()
            .filter_map(|f| {
                let key = f.key.trim();
                let value = f.value.trim();
                if Self::is_safe_payload_filter_key(key) && !value.is_empty() && value.len() <= 256
                {
                    Some((key.to_string(), value.to_string()))
                } else {
                    None
                }
            })
            .collect()
    }

    fn is_safe_payload_filter_key(key: &str) -> bool {
        !key.is_empty()
            && key.len() <= 64
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    }

    fn default_chunking_profile(input: Option<pb::ChunkingProfileV004>) -> ChunkingProfile {
        fn from_pb(v: Option<&pb::ChunkSizeProfileV004>, fallback: SizeProfile) -> SizeProfile {
            let Some(v) = v else { return fallback };
            let candidate = SizeProfile {
                target: v.target_tokens as usize,
                min: v.min_tokens as usize,
                max: v.max_tokens as usize,
                overlap: v.overlap_tokens as usize,
            };
            if candidate.validate().is_ok() {
                candidate
            } else {
                fallback
            }
        }
        let version = input
            .as_ref()
            .map(|p| p.profile_version.trim())
            .filter(|v| !v.is_empty())
            .unwrap_or("v004-smoke-profile-v1")
            .to_string();
        let parent_default = SizeProfile {
            target: 520,
            min: 1,
            max: 700,
            overlap: 0,
        };
        let sub180_default = SizeProfile {
            target: 180,
            min: 1,
            max: 220,
            overlap: 0,
        };
        let sub260_default = SizeProfile {
            target: 260,
            min: 1,
            max: 320,
            overlap: 0,
        };
        let parent = from_pb(
            input.as_ref().and_then(|p| p.parent.as_ref()),
            parent_default,
        );
        let mut sub180 = sub180_default;
        let mut sub260 = sub260_default;
        if let Some(input) = input.as_ref() {
            for g in &input.granularities {
                match pb::ChunkGranularityV004::try_from(g.granularity).ok() {
                    Some(pb::ChunkGranularityV004::Sub180V004) => {
                        sub180 = from_pb(Some(g), sub180.clone())
                    }
                    Some(pb::ChunkGranularityV004::Sub260V004) => {
                        sub260 = from_pb(Some(g), sub260.clone())
                    }
                    _ => {}
                }
            }
        }
        ChunkingProfile {
            version,
            parent,
            sub180,
            sub260,
        }
    }

    async fn compute_document_sync_status(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        include_qdrant: bool,
    ) -> Result<pb::GetVectorSyncStatusResponse, Status> {
        let repo = self.repo()?;
        let row = sqlx::query(r#"SELECT
          COALESCE((SELECT status FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3),'NOT_FOUND') AS document_status,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS expected_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status='SYNCED' AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS synced_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status IN('PENDING','UPDATE_PENDING','DELETE_PENDING') AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS pending_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status IN('FAILED','DEAD_LETTER') AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS failed_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS dense_vectors_expected,
          (SELECT count(*) FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_dense d ON d.cache_entry_id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND b.chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS dense_vectors_found,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS sparse_vectors_expected,
          (SELECT count(*) FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_sparse s ON s.cache_entry_id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND b.chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS sparse_vectors_found,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='PENDING') AS outbox_pending,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='RETRY_PENDING') AS outbox_retry_pending,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='COMPLETED') AS outbox_completed,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status IN('FAILED','DEAD_LETTER')) AS outbox_failed,
          (SELECT COALESCE(max(o.updated_at)::text,'') FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3) AS last_sync_attempt_at,
          (SELECT COALESCE(o.error_code,'') FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.error_code IS NOT NULL ORDER BY o.updated_at DESC LIMIT 1) AS last_sync_error_code,
          (SELECT COALESCE(o.error_message,'') FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.error_message IS NOT NULL ORDER BY o.updated_at DESC LIMIT 1) AS last_sync_error_message
        "#)
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_one(&repo.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres sync status: {e}")))?;
        let expected_bindings: i64 = row.get("expected_bindings");
        let synced_bindings: i64 = row.get("synced_bindings");
        let dense_vectors_expected: i64 = row.get("dense_vectors_expected");
        let dense_vectors_found: i64 = row.get("dense_vectors_found");
        let sparse_vectors_expected: i64 = row.get("sparse_vectors_expected");
        let sparse_vectors_found: i64 = row.get("sparse_vectors_found");
        let outbox_pending: i64 = row.get("outbox_pending");
        let outbox_retry_pending: i64 = row.get("outbox_retry_pending");
        let outbox_completed: i64 = row.get("outbox_completed");
        let outbox_failed: i64 = row.get("outbox_failed");
        let expected_point_ids: std::collections::HashSet<Uuid> = sqlx::query(
            "SELECT qdrant_point_id FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')",
        )
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_all(&repo.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres expected qdrant ids: {e}")))?
            .into_iter()
            .map(|row| row.get::<Uuid, _>("qdrant_point_id"))
            .collect();
        let mut qdrant_collection_exists = false;
        let mut qdrant_points_found = 0_u32;
        let mut qdrant_points_missing = expected_point_ids.len() as u32;
        let mut qdrant_points_extra = 0_u32;
        let mut warnings = Vec::new();
        if include_qdrant {
            if let Some(q) = self.qdrant.as_ref() {
                qdrant_collection_exists = q.collection_exists().await.map_err(Status::from)?;
                if qdrant_collection_exists {
                    let actual_point_ids = q
                        .point_ids_by_document(access_zone_id, document_id, document_version)
                        .await
                        .map_err(Status::from)?;
                    qdrant_points_found = actual_point_ids.len() as u32;
                    qdrant_points_missing =
                        expected_point_ids.difference(&actual_point_ids).count() as u32;
                    qdrant_points_extra =
                        actual_point_ids.difference(&expected_point_ids).count() as u32;
                    if qdrant_points_missing > 0 || qdrant_points_extra > 0 {
                        counter!("astravector_sync_status_consistency_mismatch_total").increment(1);
                    }
                    if qdrant_points_extra > 0 {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "QDRANT_EXTRA_POINTS_FOUND".into(),
                            message: format!("Qdrant contains {qdrant_points_extra} extra point(s) for this document/version"),
                        });
                    }
                }
            }
        } else {
            qdrant_collection_exists = self.qdrant.is_some();
            qdrant_points_found = expected_bindings as u32;
            qdrant_points_missing = 0;
        }
        let ready = expected_bindings > 0
            && synced_bindings == expected_bindings
            && dense_vectors_found == dense_vectors_expected
            && (!self.cfg.sparse.required || sparse_vectors_found == sparse_vectors_expected)
            && outbox_completed >= expected_bindings
            && outbox_pending == 0
            && outbox_retry_pending == 0
            && outbox_failed == 0
            && qdrant_collection_exists
            && qdrant_points_missing == 0
            && qdrant_points_found >= expected_bindings as u32;
        Ok(pb::GetVectorSyncStatusResponse {
            document_status: row.get::<String, _>("document_status"),
            expected_bindings: expected_bindings as u32,
            synced_bindings: synced_bindings as u32,
            pending_bindings: row.get::<i64, _>("pending_bindings") as u32,
            failed_bindings: row.get::<i64, _>("failed_bindings") as u32,
            dense_vectors_expected: dense_vectors_expected as u32,
            dense_vectors_found: dense_vectors_found as u32,
            sparse_vectors_expected: sparse_vectors_expected as u32,
            sparse_vectors_found: sparse_vectors_found as u32,
            outbox_pending: outbox_pending as u32,
            outbox_retry_pending: outbox_retry_pending as u32,
            outbox_completed: outbox_completed as u32,
            outbox_failed: outbox_failed as u32,
            qdrant_collection: self.cfg.qdrant.collection.clone(),
            qdrant_collection_exists,
            qdrant_points_expected: expected_bindings as u32,
            qdrant_points_found,
            qdrant_points_missing,
            qdrant_points_extra,
            ready_to_activate: ready,
            last_sync_attempt_at: row
                .try_get::<Option<String>, _>("last_sync_attempt_at")
                .ok()
                .flatten()
                .unwrap_or_default(),
            last_sync_error_code: row
                .try_get::<Option<String>, _>("last_sync_error_code")
                .ok()
                .flatten()
                .unwrap_or_default(),
            last_sync_error_message: row
                .try_get::<Option<String>, _>("last_sync_error_message")
                .ok()
                .flatten()
                .unwrap_or_default(),
            warnings,
        })
    }
}

fn qdrant_hit_identity(hit: &QdrantSearchHit) -> Option<GlobalCandidateIdentity> {
    let access_zone_id = payload_string(&hit.payload, "access_zone_id")?;
    let document_id = payload_string(&hit.payload, "document_id")?;
    let document_version = payload_u64(&hit.payload, "document_version")?;
    let matched_chunk_id = payload_string(&hit.payload, "chunk_id")?;
    let parent_chunk_id =
        payload_string(&hit.payload, "parent_chunk_id").unwrap_or_else(|| matched_chunk_id.clone());
    Some(GlobalCandidateIdentity {
        access_zone_id,
        document_id,
        document_version,
        matched_chunk_id,
        parent_chunk_id,
        source_block_id: payload_string(&hit.payload, "source_block_id").unwrap_or_default(),
        representation_type: payload_string(&hit.payload, "representation_type")
            .unwrap_or_else(|| "ORIGINAL".into()),
        qdrant_point_id: Some(hit.id.to_string()),
    })
}

fn payload_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn payload_u64(payload: &serde_json::Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
    })
}

fn annotate_segmented_hit(hit: &mut QdrantSearchHit, matched_segments: &BTreeSet<usize>) {
    let Some(object) = hit.payload.as_object_mut() else {
        return;
    };
    object.insert(
        "query_processing_mode".into(),
        serde_json::json!("SEGMENTED"),
    );
    object.insert(
        "query_segment_indices".into(),
        serde_json::json!(matched_segments.iter().copied().collect::<Vec<_>>()),
    );
}

fn result_query_segment_indices(result: &pb::SearchResultV004) -> Vec<usize> {
    result_segment_indices_from_metadata(result, "query_segment_indices")
}

fn result_passed_query_segment_indices(result: &pb::SearchResultV004) -> Vec<usize> {
    result_segment_indices_from_metadata(result, "passed_query_segment_indices")
}

fn result_passed_query_intent_ids(result: &pb::SearchResultV004) -> Vec<usize> {
    result_segment_indices_from_metadata(result, "passed_query_intent_ids")
}

fn result_segment_indices_from_metadata(result: &pb::SearchResultV004, key: &str) -> Vec<usize> {
    let Some(raw) = result
        .citation
        .as_ref()
        .and_then(|citation| citation.metadata.get(key))
    else {
        return Vec::new();
    };
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(items) = value.as_array() {
            return items
                .iter()
                .filter_map(serde_json::Value::as_u64)
                .map(|value| value as usize)
                .collect();
        }
        if let Some(index) = value.as_u64() {
            return vec![index as usize];
        }
    }
    raw.split(',')
        .filter_map(|part| {
            part.trim_matches(|ch: char| !ch.is_ascii_digit())
                .parse::<usize>()
                .ok()
        })
        .collect()
}

fn coverage_for_results(plan: &QueryPlan, results: &[pb::SearchResultV004]) -> QueryCoverage {
    if plan.mode == QueryProcessingMode::Single {
        let covered = usize::from(!results.is_empty());
        return QueryCoverage {
            required_total: 1,
            required_covered: covered,
            ratio: covered as f32,
            status: if covered == 0 {
                QueryEvidenceStatus::Insufficient
            } else {
                QueryEvidenceStatus::Found
            },
            uncovered_required_segment_indices: if covered == 0 { vec![0] } else { Vec::new() },
            uncovered_required_intent_ids: Vec::new(),
        };
    }
    let covered_segments = results
        .iter()
        .flat_map(|result| {
            let passed = result_passed_query_segment_indices(result);
            if passed.is_empty() {
                result_query_segment_indices(result)
            } else {
                passed
            }
        })
        .collect::<HashSet<_>>();
    if plan.intent_units.is_empty() {
        return evaluate_required_coverage(&plan.segments, &covered_segments);
    }
    let explicit_intent_ids = results
        .iter()
        .flat_map(result_passed_query_intent_ids)
        .collect::<HashSet<_>>();
    let covered_intent_ids = if explicit_intent_ids.is_empty() {
        plan.intent_units
            .iter()
            .filter(|intent| {
                intent
                    .source_segment_indices
                    .iter()
                    .any(|index| covered_segments.contains(index))
            })
            .map(|intent| intent.id)
            .collect::<HashSet<_>>()
    } else {
        explicit_intent_ids
    };
    evaluate_intent_coverage(&plan.intent_units, &covered_intent_ids)
}

fn reserve_required_segment_coverage(
    results: &mut [pb::SearchResultV004],
    plan: &QueryPlan,
    final_context_limit: usize,
) -> bool {
    if plan.mode != QueryProcessingMode::Segmented || final_context_limit == 0 {
        return false;
    }
    let required_segment_indices = plan
        .segments
        .iter()
        .filter(|segment| segment.required_for_coverage)
        .map(|segment| segment.index)
        .collect::<Vec<_>>();
    let exceeds_limit = required_segment_indices.len() > final_context_limit;
    let mut selected_result_keys = HashSet::new();
    for segment_index in required_segment_indices
        .into_iter()
        .take(final_context_limit)
    {
        if results.iter().any(|result| {
            selected_result_keys.contains(&result_identity_key(result))
                && result_query_segment_indices(result).contains(&segment_index)
        }) {
            continue;
        }
        let selected = results
            .iter()
            .enumerate()
            .filter(|(_, result)| result_query_segment_indices(result).contains(&segment_index))
            .filter(|(_, result)| !selected_result_keys.contains(&result_identity_key(result)))
            .max_by(|(_, left), (_, right)| {
                score_of(left)
                    .total_cmp(&score_of(right))
                    .then_with(|| result_identity_key(right).cmp(&result_identity_key(left)))
            })
            .map(|(index, _)| index);
        if let Some(index) = selected {
            selected_result_keys.insert(result_identity_key(&results[index]));
            mark_ranking_protection(
                &mut results[index],
                RankingProtection {
                    preserve_required_segment_coverage: true,
                    ..Default::default()
                },
            );
        }
    }
    exceeds_limit
}

fn query_processing_mode_v008(mode: QueryProcessingMode) -> i32 {
    match mode {
        QueryProcessingMode::Single => pb::QueryProcessingModeV008::Single as i32,
        QueryProcessingMode::Segmented => pb::QueryProcessingModeV008::Segmented as i32,
    }
}

fn query_segment_diagnostics(
    plan: &QueryPlan,
    results: &[pb::SearchResultV004],
) -> Vec<pb::QuerySegmentDiagnosticV008> {
    plan.segments
        .iter()
        .map(|segment| {
            let segment_results = results
                .iter()
                .filter(|result| {
                    let passed = result_passed_query_segment_indices(result);
                    if passed.is_empty() {
                        result_query_segment_indices(result).contains(&segment.index)
                    } else {
                        passed.contains(&segment.index)
                    }
                })
                .collect::<Vec<_>>();
            let dense_candidates = segment_results
                .iter()
                .filter(|result| {
                    result
                        .scores
                        .as_ref()
                        .is_some_and(|scores| scores.dense_score > 0.0)
                })
                .count();
            let sparse_candidates = segment_results
                .iter()
                .filter(|result| {
                    result
                        .scores
                        .as_ref()
                        .is_some_and(|scores| scores.sparse_score > 0.0)
                })
                .count();
            let lexical_candidates = segment_results
                .iter()
                .filter(|result| {
                    extraction_retrieval_sources(result)
                        .iter()
                        .any(|source| source == "POSTGRES_FTS")
                })
                .count();
            pb::QuerySegmentDiagnosticV008 {
                segment_index: segment.index as u32,
                token_count: segment.token_count as u32,
                segment_kind: format!("{:?}", segment.kind).to_uppercase(),
                weight: segment.weight,
                required_for_coverage: segment.required_for_coverage,
                retrieval_executed: true,
                evidence_found: !segment_results.is_empty(),
                dense_candidates: dense_candidates as u32,
                sparse_candidates: sparse_candidates as u32,
                lexical_candidates: lexical_candidates as u32,
                final_contexts: segment_results.len() as u32,
                segment_sha256: segment.sha256.clone(),
            }
        })
        .collect()
}

fn query_segment_diagnostics_from_hits(
    plan: &QueryPlan,
    hits: &[QdrantSearchHit],
) -> Vec<pb::QuerySegmentDiagnosticV008> {
    plan.segments
        .iter()
        .map(|segment| {
            let segment_hits = hits
                .iter()
                .filter(|hit| {
                    hit.payload
                        .get("query_segment_indices")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|indices| {
                            indices
                                .iter()
                                .any(|index| index.as_u64() == Some(segment.index as u64))
                        })
                        || plan.mode == QueryProcessingMode::Single
                })
                .collect::<Vec<_>>();
            pb::QuerySegmentDiagnosticV008 {
                segment_index: segment.index as u32,
                token_count: segment.token_count as u32,
                segment_kind: format!("{:?}", segment.kind).to_uppercase(),
                weight: segment.weight,
                required_for_coverage: segment.required_for_coverage,
                retrieval_executed: true,
                evidence_found: !segment_hits.is_empty(),
                dense_candidates: segment_hits
                    .iter()
                    .filter(|hit| hit.dense_score > 0.0)
                    .count() as u32,
                sparse_candidates: segment_hits
                    .iter()
                    .filter(|hit| hit.sparse_score > 0.0)
                    .count() as u32,
                lexical_candidates: 0,
                final_contexts: segment_hits.len() as u32,
                segment_sha256: segment.sha256.clone(),
            }
        })
        .collect()
}

#[tonic::async_trait]
impl AstraVectorV004Control for AstraVectorV004ControlService {
    async fn search(
        &self,
        request: Request<pb::SearchRequestV004>,
    ) -> Result<Response<pb::SearchResponseV004>, Status> {
        let started = std::time::Instant::now();
        let hydration_entry_point = request
            .extensions()
            .get::<RetrievalEntryPoint>()
            .map_or("Search", |value| value.0);
        let request_timing = request
            .extensions()
            .get::<RequestTiming>()
            .copied()
            .unwrap_or_else(|| RequestTiming::from_request(&request));
        let r = request.into_inner();
        let mut ranking_trace = RankingTraceCollector::new(
            r.include_debug && self.cfg.search.ranking_trace.enabled,
            self.cfg.search.ranking_trace.max_candidates,
            self.cfg.search.ranking_trace.max_stages_per_candidate,
        );
        let query = r.query.trim();
        if query.is_empty() {
            return Err(Status::invalid_argument("query must not be empty"));
        }
        let resolved_zones = self
            .resolve_search_access_zones(
                &r.access_zone_id,
                &r.access_zone_ids,
                &r.access_zone_code,
                &r.access_zone_codes,
            )
            .await?;
        let access_zone_ids: Vec<Uuid> = resolved_zones.iter().map(|z| z.access_zone_id).collect();
        let access_zone_codes = resolved_zones
            .iter()
            .map(|zone| (zone.access_zone_id, zone.access_zone_code.clone()))
            .collect::<HashMap<_, _>>();
        let caller_access_level = pb::AccessLevel::try_from(r.caller_access_level)
            .ok()
            .filter(|v| *v != pb::AccessLevel::Unspecified)
            .ok_or_else(|| Status::invalid_argument("caller_access_level is required"))?;
        let top_k_max = self.cfg.limits.search_top_k_max.max(1);
        let top_k = if r.top_k == 0 { 10 } else { r.top_k }.min(top_k_max);
        if r.top_k > top_k_max {
            return Err(Status::invalid_argument(format!(
                "top_k must be <= {top_k_max}"
            )));
        }
        let candidate_limit = if r.candidate_limit == 0 {
            (top_k * 4).max(top_k)
        } else {
            r.candidate_limit
        };
        if candidate_limit < top_k {
            return Err(Status::invalid_argument("candidate_limit must be >= top_k"));
        }
        let candidate_limit =
            candidate_limit.min(self.cfg.limits.search_candidate_limit_max.max(top_k));
        let parent_limit = if r.parent_limit == 0 {
            top_k
        } else {
            r.parent_limit
        };
        if parent_limit == 0 || parent_limit > self.cfg.limits.search_top_k_max {
            return Err(Status::invalid_argument(format!(
                "parent_limit must be between 1 and {}",
                self.cfg.limits.search_top_k_max
            )));
        }
        let search_mode = Self::resolve_search_mode(r.search_mode, &self.cfg.search.default_mode);
        let version_filters = Self::version_filters_from_search_request(&r);
        tracing::debug!(
            correlation_id = %r.correlation_id,
            access_zone_ids = ?access_zone_ids,
            access_zone_code = %r.access_zone_code,
            caller_access_level = ?caller_access_level,
            search_mode = ?search_mode,
            top_k,
            candidate_limit,
            parent_limit,
            query_len = query.chars().count(),
            "SEARCH_REQUEST_RECEIVED"
        );
        let wants_sparse = matches!(
            search_mode,
            pb::SearchModeV005::Sparse | pb::SearchModeV005::Hybrid
        );
        let wants_dense = matches!(
            search_mode,
            pb::SearchModeV005::Dense | pb::SearchModeV005::Hybrid
        );
        let sparse_available = self.engine.sparse_available();
        let sparse_required = wants_sparse
            && Self::embedding_mode_requires_sparse(r.embedding_mode, self.cfg.sparse.required);
        let mut warnings = Vec::new();
        if r.include_vectors {
            warnings.push(pb::DiagnosticWarningV005 {
                code: "INCLUDE_VECTORS_IGNORED".into(),
                message: "include_vectors is ignored for search responses; dense embeddings are internal-only and are stripped before returning results.".into(),
            });
        }
        if search_mode == pb::SearchModeV005::Sparse && !sparse_available {
            counter!("astravector_sparse_unavailable_total").increment(1);
            return Err(Status::failed_precondition(
                "SPARSE_UNAVAILABLE: SPARSE search requested but loaded ONNX artifact has no sparse output",
            ));
        }
        if wants_sparse && sparse_required && !sparse_available {
            counter!("astravector_sparse_unavailable_total").increment(1);
            return Err(Status::failed_precondition(
                "SPARSE_UNAVAILABLE: sparse search requested but loaded ONNX artifact has no sparse output",
            ));
        }
        if search_mode == pb::SearchModeV005::Hybrid
            && wants_sparse
            && !sparse_required
            && !sparse_available
        {
            counter!("astravector_search_dense_fallback_warning_total").increment(1);
            warnings.push(pb::DiagnosticWarningV005 {
                code: "SPARSE_UNAVAILABLE_DENSE_FALLBACK".into(),
                message: "Sparse embedding is unavailable; HYBRID search degraded to DENSE search because embeddingMode is DENSE_SPARSE_IF_AVAILABLE.".into(),
            });
        }
        let query_counter = EngineQueryTokenCounter {
            engine: self.engine.as_ref(),
        };
        let query_planning_started = Instant::now();
        let query_plan = build_query_plan(
            query,
            &query_counter,
            &self.cfg.search.query_processing,
            self.cfg.tokenization.query.max_length,
        )
        .map_err(query_planning_status)?;
        let query_tier = query_plan.tier.code();
        histogram!("astravector_query_planning_duration_seconds", "tier" => query_tier)
            .record(query_planning_started.elapsed().as_secs_f64());
        histogram!("astravector_query_segment_count", "tier" => query_tier)
            .record(query_plan.segments.len() as f64);
        histogram!("astravector_query_intent_count", "tier" => query_tier)
            .record(query_plan.intent_units.len() as f64);
        let query_plan_diagnostics = QueryPlanDiagnostics::from_plan(&query_plan);
        let timeout_ms =
            effective_query_timeout_ms(r.timeout_ms as u64, query_plan.limits.deadline_ms);
        let server_deadline = request_timing.started + Duration::from_millis(timeout_ms);
        let deadline = request_timing
            .transport_deadline
            .map_or(server_deadline, |transport| transport.min(server_deadline));
        if deadline <= Instant::now() {
            return Err(Status::deadline_exceeded(
                "query deadline exhausted during planning",
            ));
        }
        let timeout_ms = deadline
            .saturating_duration_since(request_timing.started)
            .as_millis() as u64;
        let request_cancel = self.shutdown.child_token();
        let _request_cancel_guard = RequestCancellationGuard(request_cancel.clone());
        let admission_timeout_ms = self
            .cfg
            .limits
            .backpressure_acquire_timeout_ms
            .min(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis() as u64,
            )
            .max(1);
        let _retrieve_permit = Self::acquire_backpressure_permit(
            self.retrieve_context_semaphore.clone(),
            admission_timeout_ms,
            "retrieve_context",
            query_plan.limits.admission_weight,
            &request_cancel,
        )
        .await
        .inspect_err(|_status| {
            tracing::warn!(
                correlation_id = %r.correlation_id,
                tier = query_plan.tier.code(),
                admission_weight = query_plan.limits.admission_weight,
                reason = "retrieve_context_admission_timeout",
                "RETRIEVE_CONTEXT_ADMISSION_REJECTED"
            );
        })?;
        gauge!("retrieval_concurrent_active").set(
            (self
                .cfg
                .limits
                .max_concurrent_retrieve_context
                .saturating_sub(self.retrieve_context_semaphore.available_permits()))
                as f64,
        );
        let qdrant_budget = OperationBudget {
            deadline,
            cancellation: request_cancel.clone(),
            workload: WorkloadKind::Query,
        };
        let remaining_budget_after_planning_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        counter!(
            "astravector_query_processing_total",
            "mode" => query_plan_diagnostics.mode_code()
        )
        .increment(1);
        histogram!("astravector_query_original_tokens")
            .record(query_plan.original_token_count as f64);
        histogram!("astravector_query_segment_count").record(query_plan.segments.len() as f64);
        let _long_query_in_flight = if query_plan.mode == QueryProcessingMode::Segmented {
            let segment_count = query_plan.segments.len() as f64;
            gauge!("astravector_long_query_segments_in_flight").increment(segment_count);
            gauge!("astravector_long_query_max_segments_in_flight").set(
                query_plan
                    .segments
                    .len()
                    .min(query_plan.limits.max_parallel_segments) as f64,
            );
            Some(LongQueryInFlightGuard {
                segments: segment_count,
            })
        } else {
            None
        };
        for segment in &query_plan.segments {
            counter!("astravector_query_segments_total", "segment_kind" => format!("{:?}", segment.kind))
                .increment(1);
            histogram!("astravector_query_segment_tokens").record(segment.token_count as f64);
        }
        let candidate_limit = if query_plan.mode == QueryProcessingMode::Segmented {
            candidate_limit
                .min(query_plan.limits.global_fused_candidate_limit)
                .max(top_k)
        } else {
            candidate_limit
        };
        let candidate_limit = bounded_hydration_fetch_window(
            parent_limit,
            candidate_limit,
            self.cfg.search.hydration_rejection_reserve,
            self.cfg.search.hydration_rejection_reserve_max,
            self.cfg.limits.search_candidate_limit_max,
        );
        tracing::info!(
            correlation_id = %r.correlation_id,
            mode = query_plan_diagnostics.mode_code(),
            original_token_count = query_plan.original_token_count,
            segment_count = query_plan.segments.len(),
            "QUERY_PLAN_READY"
        );
        let direct_generation = self
            .generate_direct_qdrant_hits(
                &query_plan,
                &access_zone_ids,
                caller_access_level,
                candidate_limit,
                top_k,
                search_mode,
                wants_dense,
                wants_sparse,
                sparse_available,
                sparse_required,
                &version_filters,
                deadline,
                &qdrant_budget,
                request_cancel.clone(),
                &mut warnings,
            )
            .await?;
        let DirectQdrantGeneration {
            hits,
            query_embedding_ms,
            qdrant_search_ms,
            dense_search_ms,
            sparse_search_ms,
            fusion_ms,
            dense_branch_executed,
            sparse_branch_executed,
            fusion_executed,
            dense_branch_candidate_count,
            sparse_branch_candidate_count,
            fusion_candidate_count,
            sparse_top_score,
            dense_failed,
            sparse_failed,
            branch_statuses,
            retrieval_status,
        } = direct_generation;
        for status in &branch_statuses {
            counter!(
                "astravector_retrieval_branch_total",
                "status" => status.metric_label(),
                "tier" => format!("{:?}", query_plan.tier)
            )
            .increment(1);
        }
        let retrieval_infrastructure_failure = branch_statuses
            .iter()
            .any(|status| status.is_infrastructure_failure());
        match retrieval_status {
            SegmentRetrievalStatus::PartialFailure => {
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "RETRIEVAL_PARTIAL_FAILURE".into(),
                    message: "one retrieval branch failed; successful branches remain eligible for evidence, but an empty result is not a successful no-answer".into(),
                });
                counter!("astravector_query_degraded_total", "reason" => "retrieval_partial_failure")
                    .increment(1);
            }
            SegmentRetrievalStatus::Failed => {
                return Err(Status::unavailable(
                    "RETRIEVAL_BACKENDS_UNAVAILABLE: all requested retrieval branches failed",
                ));
            }
            SegmentRetrievalStatus::Skipped => {
                return Err(Status::deadline_exceeded(
                    "RETRIEVAL_SKIPPED_BUDGET: no retrieval branch had sufficient budget",
                ));
            }
            SegmentRetrievalStatus::Success => {}
        }
        tracing::debug!(
            correlation_id = %r.correlation_id,
            search_mode = ?search_mode,
            qdrant_search_ms,
            raw_hits_count = hits.len(),
            dense_failed,
            sparse_failed,
            "SEARCH_QDRANT_RESULTS_READY"
        );

        // fix462: direct parent grouping is keyed by (access_zone_id, parent_id), not parent_id alone.
        // content_chunks_v004 is keyed by (access_zone_id, id); using only UUID would mix tenants/zones when
        // malformed or imported data contains the same chunk UUID in more than one zone.
        let mut groups: Vec<((Uuid, Uuid), QdrantSearchHit)> = Vec::new();
        let mut seen_candidates = HashSet::new();
        let mut pre_dedup_child_ids = HashMap::<(Uuid, Uuid), HashSet<Uuid>>::new();
        for hit in hits.iter() {
            let Some(hit_access_zone_id) = hit
                .payload
                .get("access_zone_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|v| Uuid::parse_str(v).ok())
            else {
                counter!("retrieval_hit_missing_access_zone_total").increment(1);
                continue;
            };
            let granularity = hit
                .payload
                .get("chunk_granularity")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let parent_id = match granularity {
                "PARENT" => hit
                    .payload
                    .get("chunk_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|v| Uuid::parse_str(v).ok()),
                "SUB_180" | "SUB_260" => hit
                    .payload
                    .get("parent_chunk_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|v| Uuid::parse_str(v).ok())
                    .or_else(|| {
                        hit.payload
                            .get("chunk_id")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|v| Uuid::parse_str(v).ok())
                    }),
                _ => hit
                    .payload
                    .get("chunk_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|v| Uuid::parse_str(v).ok()),
            }
            .unwrap_or_else(Uuid::nil);
            let parent_key = (hit_access_zone_id, parent_id);
            if matches!(granularity, "SUB_180" | "SUB_260") {
                if let Some(chunk_id) = hit
                    .payload
                    .get("chunk_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                {
                    pre_dedup_child_ids
                        .entry(parent_key)
                        .or_default()
                        .insert(chunk_id);
                }
            }
            let matched_chunk_id = hit
                .payload
                .get("chunk_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .unwrap_or_else(Uuid::nil);
            if seen_candidates.insert((parent_key, matched_chunk_id)) {
                groups.push((parent_key, hit.clone()));
            }
            if groups.len() >= candidate_limit as usize {
                break;
            }
        }
        let hydration_keys = groups
            .iter()
            .enumerate()
            .map(|(input_ordinal, ((zone, parent_id), hit))| {
                let matched_chunk_id = hit
                    .payload
                    .get("chunk_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .unwrap_or_else(Uuid::nil);
                let binding_id = hit
                    .payload
                    .get("binding_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .unwrap_or_else(Uuid::nil);
                let granularity = hit
                    .payload
                    .get("chunk_granularity")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                HydrationCandidateIdentity {
                    access_zone_id: *zone,
                    binding_id,
                    matched_chunk_id,
                    parent_chunk_id: *parent_id,
                    granularity,
                    raw_rank: input_ordinal,
                    input_ordinal,
                }
            })
            .collect::<Vec<_>>();
        tracing::debug!(
            correlation_id = %r.correlation_id,
            raw_hits_count = hits.len(),
            parent_group_count = groups.len(),
            parent_ids_count = hydration_keys.len(),
            "SEARCH_PARENT_GROUPS_READY"
        );
        let parent_fetch_started = std::time::Instant::now();
        let postgres_safety_margin_ms = 50u64;
        let remaining_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        let statement_timeout_ms = postgres_statement_timeout_ms(
            self.cfg.postgres.statement_timeout_ms,
            remaining_ms,
            postgres_safety_margin_ms,
        )
        .map_err(|_| {
            counter!("astravector_deadline_rejected_total", "stage" => "postgres_parent_fetch", "reason" => "insufficient_postgres_budget").increment(1);
            Status::deadline_exceeded("insufficient_postgres_budget")
        })?;
        histogram!("astravector_deadline_remaining_seconds", "stage" => "postgres_parent_fetch")
            .record(remaining_ms as f64 / 1000.0);
        let hydration_outcomes = self
            .repo()?
            .fetch_hydration_outcomes_batch(
                &hydration_keys,
                caller_access_level as i16,
                statement_timeout_ms,
            )
            .await
            .map_err(Status::from)?;
        let hydration_outcomes = self
            .hydration_failpoints
            .apply(
                &r.correlation_id,
                hydration_entry_point,
                &access_zone_codes,
                hydration_outcomes,
            )
            .await;
        let normalized_hydration =
            normalize_hydration_outcomes(hydration_entry_point, hydration_outcomes);
        let hydration_degradation = normalized_hydration.to_proto();
        let rejected_parent_keys = normalized_hydration.rejected_parent_keys.clone();
        let retrieval_infrastructure_failure =
            retrieval_infrastructure_failure || hydration_degradation.infrastructure_failure;
        for dropped in &normalized_hydration.dropped_parents {
            tracing::warn!(
                correlation_id = %r.correlation_id,
                entry_point = hydration_entry_point,
                binding_id = %dropped.candidate.binding_id,
                matched_chunk_id = %dropped.candidate.matched_chunk_id,
                parent_chunk_id = %dropped.candidate.parent_chunk_id,
                reason = dropped.reason.code(),
                stage = dropped.stage,
                "CANONICAL_PARENT_HYDRATION_REJECTED"
            );
            if !warnings
                .iter()
                .any(|warning| warning.code == dropped.reason.code())
            {
                warnings.push(pb::DiagnosticWarningV005 {
                    code: dropped.reason.code().into(),
                    message: format!("canonical parent candidate rejected at {}", dropped.stage),
                });
            }
        }
        if normalized_hydration.has_total_timeout() {
            return Err(total_hydration_timeout_status(&hydration_degradation));
        }
        let hydrated = normalized_hydration.surviving_contexts;
        let parent_fetch_ms = parent_fetch_started.elapsed().as_millis() as u64;
        histogram!("astravector_parent_hydration_duration_seconds")
            .record(parent_fetch_ms as f64 / 1000.0);
        counter!("astravector_parent_hydration_candidates_total")
            .increment(hydration_keys.len() as u64);
        counter!("astravector_parent_hydration_missing_total")
            .increment(hydration_keys.len().saturating_sub(hydrated.len()) as u64);
        let fetched_parent_count = hydrated
            .iter()
            .map(|context| (context.access_zone_id, context.parent_chunk_id))
            .collect::<HashSet<_>>()
            .len();
        let by_candidate = hydrated
            .into_iter()
            .map(|context| {
                (
                    (
                        context.access_zone_id,
                        context.matched_chunk_id,
                        context.parent_chunk_id,
                    ),
                    context,
                )
            })
            .collect::<HashMap<_, _>>();
        let mut direct_results = Vec::new();
        let mut pre_parent_dedup_graph_seed_results = Vec::new();
        let mut hydrated_parent_seen = HashSet::new();
        let mut graph_results = Vec::new();
        let mut seed_scores: HashMap<(Uuid, Uuid), f32> = HashMap::new();
        for ((zone, _), hit) in &groups {
            if let Some(seed_id) = hit
                .payload
                .get("chunk_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|v| Uuid::parse_str(v).ok())
            {
                seed_scores.entry((*zone, seed_id)).or_insert(hit.score);
            }
        }
        for ((parent_zone_id, parent_id), hit) in &groups {
            if direct_results.len() >= candidate_limit as usize {
                break;
            }
            let Some(matched_id) = hit
                .payload
                .get("chunk_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
            else {
                continue;
            };
            let Some(context) = by_candidate.get(&(*parent_zone_id, matched_id, *parent_id)) else {
                continue;
            };
            let parent = ParentContextRecord {
                access_zone_id: context.access_zone_id,
                id: context.parent_chunk_id,
                document_id: context.document_id,
                document_version: context.document_version,
                root_chunk_id: context.root_chunk_id,
                source_chunk_id: context.source_chunk_id,
                access_level: context.access_level,
                content: context.parent_text.clone(),
                content_hash: context.parent_content_hash.clone(),
                token_count: context.parent_token_count,
                sequence_no: context.parent_sequence_no,
                source_block_id: context.source_block_id.clone(),
                metadata: context.parent_metadata.clone(),
            };
            let trace = ChunkTraceRecord {
                id: context.matched_chunk_id,
                source_block_id: context.source_block_id.clone(),
                source_location: context.source_location.clone(),
                source_links: context.source_links.clone(),
                metadata: context.metadata.clone(),
            };
            let mut direct =
                search_result_from_hit(&parent, hit, context.matched_text.clone(), Some(&trace));
            if let Some(citation) = direct.citation.as_mut() {
                citation
                    .metadata
                    .insert("retrieval_source".into(), "VECTOR_DIRECT".into());
                citation
                    .metadata
                    .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
                let mut child_ids = pre_dedup_child_ids
                    .get(&(*parent_zone_id, *parent_id))
                    .into_iter()
                    .flatten()
                    .map(Uuid::to_string)
                    .collect::<Vec<_>>();
                child_ids.sort();
                citation.metadata.insert(
                    "pre_dedup_distinct_child_count".into(),
                    child_ids.len().to_string(),
                );
                citation.metadata.insert(
                    "pre_dedup_child_ids".into(),
                    serde_json::to_string(&child_ids).unwrap_or_else(|_| "[]".into()),
                );
            }
            calibrate_result_score(
                &mut direct,
                "VECTOR_DIRECT",
                self.cfg.graph_rag.scoring.direct_score_weight,
                self.cfg.graph_rag.scoring.graph_score_weight,
                self.cfg.graph_rag.scoring.graph_score_bias,
            );
            let granularity = hit
                .payload
                .get("chunk_granularity")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if matches!(granularity, "SUB_180" | "SUB_260") {
                pre_parent_dedup_graph_seed_results.push(direct.clone());
            }
            if !hydrated_parent_seen.insert((*parent_zone_id, *parent_id)) {
                continue;
            }
            direct_results.push(direct);
        }
        let quality_run_id_filter = search_quality_run_id_filter(&r.filters);
        let mut lexical_search_ms = 0_u64;
        let mut lexical_candidate_count = 0_u32;
        if self.cfg.search.lexical.enabled
            && matches!(
                search_mode,
                pb::SearchModeV005::Sparse | pb::SearchModeV005::Hybrid
            )
        {
            let lexical_remaining_ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis() as u64;
            if lexical_remaining_ms < self.cfg.search.lexical.min_remaining_budget_ms {
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "LEXICAL_SKIPPED_INSUFFICIENT_BUDGET".into(),
                    message:
                        "indexed lexical retrieval skipped because request budget is insufficient"
                            .into(),
                });
                counter!("astravector_optional_stage_skipped_total", "stage" => "lexical", "reason" => "insufficient_budget").increment(1);
            } else {
                let lexical_started = Instant::now();
                let lexical_limit = self
                    .cfg
                    .search
                    .lexical
                    .candidate_limit
                    .min(self.cfg.search.lexical.max_candidate_limit)
                    .min(self.cfg.limits.search_candidate_limit_max)
                    .max(candidate_limit) as i64;
                let lexical_inputs = query_plan.segments.clone();
                let segment_lexical_attempts = stream::iter(lexical_inputs)
                    .map(|segment| {
                        let cancellation = request_cancel.clone();
                        let segment_access_zone_ids = access_zone_ids.clone();
                        let segment_quality_run_id = quality_run_id_filter.clone();
                        async move {
                            self.retrieve_lexical_for_segment(
                                &segment,
                                &segment_access_zone_ids,
                                caller_access_level,
                                segment_quality_run_id.as_deref(),
                                lexical_limit,
                                deadline,
                                cancellation,
                            )
                            .await
                        }
                    })
                    .buffer_unordered(
                        self.cfg
                            .search
                            .query_processing
                            .max_parallel_lexical_segments
                            .max(1),
                    )
                    .collect::<Vec<_>>()
                    .await;
                let mut segment_lexical_results = Vec::new();
                let mut segment_lexical_failures = Vec::new();
                for attempt in segment_lexical_attempts {
                    match attempt {
                        Ok(result) => segment_lexical_results.push(result),
                        Err(status) if optional_lexical_failure_can_degrade(&status) => {
                            segment_lexical_failures.push(status)
                        }
                        Err(status) => return Err(status),
                    }
                }
                if !segment_lexical_failures.is_empty() {
                    if !optional_lexical_failure_has_fallback(
                        segment_lexical_results.len(),
                        direct_results.len(),
                    ) {
                        return Err(segment_lexical_failures.remove(0));
                    }
                    for status in segment_lexical_failures {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "QUERY_SEGMENT_FTS_DEGRADED".into(),
                            message: format!(
                                "PostgreSQL FTS degraded after transient backend failure: {}",
                                status.message()
                            ),
                        });
                        counter!("astravector_optional_stage_skipped_total", "stage" => "lexical_segment", "reason" => "backend_failure").increment(1);
                    }
                }
                segment_lexical_results.sort_by_key(|result| result.segment_index);
                let mut lexical_candidates = Vec::new();
                for result in segment_lexical_results {
                    if result.candidates.is_empty()
                        && result
                            .warnings
                            .iter()
                            .any(|warning| warning.code == "QUERY_SEGMENT_FTS_SKIPPED")
                    {
                        counter!("astravector_optional_stage_skipped_total", "stage" => "lexical_segment", "reason" => "insufficient_budget").increment(1);
                    } else {
                        counter!("astravector_segment_lexical_search_total").increment(1);
                    }
                    histogram!("astravector_segment_lexical_search_duration_seconds")
                        .record(result.duration_ms as f64 / 1_000.0);
                    histogram!("astravector_long_query_fts_duration_seconds")
                        .record(result.duration_ms as f64 / 1_000.0);
                    warnings.extend(result.warnings);
                    lexical_candidates.extend(result.candidates.into_iter().map(|candidate| {
                        (
                            candidate,
                            result.segment_index,
                            result.segment_text.clone(),
                            result.segment_weight,
                        )
                    }));
                }
                lexical_search_ms = lexical_started.elapsed().as_millis() as u64;
                lexical_candidate_count = lexical_candidates.len() as u32;
                metrics::histogram!("astravector_lexical_search_duration_seconds")
                    .record(lexical_started.elapsed().as_secs_f64());
                counter!("astravector_lexical_search_total", "backend" => "POSTGRES_FTS")
                    .increment(1);
                counter!("astravector_lexical_candidates_total")
                    .increment(lexical_candidates.len() as u64);
                let parents_by_document = lexical_candidates.iter().fold(
                    HashMap::<(Uuid, Uuid), Vec<&ParentContextRecord>>::new(),
                    |mut acc, (candidate, _, _, _)| {
                        let parent = &candidate.parent;
                        acc.entry((parent.access_zone_id, parent.document_id))
                            .or_default()
                            .push(parent);
                        acc
                    },
                );
                let mut direct_result_index_by_key = direct_results
                    .iter()
                    .enumerate()
                    .map(|(idx, result)| (result_identity_key(result), idx))
                    .collect::<HashMap<_, _>>();
                let mut lexical_results = Vec::new();
                let mut sibling_seed_scores = HashMap::<(Uuid, Uuid), f32>::new();
                let mut sibling_seed_query_by_document = HashMap::<(Uuid, Uuid), String>::new();
                let mut sibling_seed_segment_by_document = HashMap::<(Uuid, Uuid), usize>::new();
                if self.cfg.graph_rag.rerank.mmr_enabled {
                    for result in &direct_results {
                        let Ok(access_zone_id) = Uuid::parse_str(&result.access_zone_id) else {
                            continue;
                        };
                        let Ok(document_id) = Uuid::parse_str(&result.document_id) else {
                            continue;
                        };
                        sibling_seed_scores
                            .entry((access_zone_id, document_id))
                            .and_modify(|existing| *existing = existing.max(score_of(result)))
                            .or_insert_with(|| score_of(result));
                    }
                }
                for (candidate, segment_index, segment_text, segment_weight) in &lexical_candidates
                {
                    let parent = &candidate.parent;
                    if rejected_parent_keys.contains(&(parent.access_zone_id, parent.id)) {
                        counter!(
                            "candidate_rejections_total",
                            "entry_point" => hydration_entry_point,
                            "reason" => "PARENT_SCOPED_REJECTION"
                        )
                        .increment(1);
                        continue;
                    }
                    let mut lexical = search_result_from_lexical_parent(parent, segment_text);
                    if let Some(citation) = lexical.citation.as_mut() {
                        citation.metadata.insert(
                            "query_processing_mode".into(),
                            query_plan_diagnostics.mode_code().into(),
                        );
                        citation
                            .metadata
                            .insert("query_segment_indices".into(), format!("[{segment_index}]"));
                    }
                    let score = candidate.lexical_score * *segment_weight;
                    let exact_phrase_match = candidate.exact_match;
                    let query_terms = query_term_count(segment_text);
                    let matched_terms = matched_term_count(&lexical, segment_text);
                    let matched_discriminating_terms =
                        matched_discriminating_term_count(&lexical, segment_text);
                    let leading_discriminating_match =
                        leading_discriminating_query_term_matches(&lexical, segment_text);
                    let strict_lexical_evidence = exact_phrase_match
                        || strict_lexical_query_match(
                            matched_terms,
                            matched_discriminating_terms,
                            leading_discriminating_match,
                            query_terms,
                        );
                    let document_overview_seed = self.cfg.graph_rag.rerank.mmr_enabled
                        && parent.source_block_id.as_deref() == Some("doc-root")
                        && matched_terms >= 2
                        && matched_discriminating_terms >= 1;
                    if !(strict_lexical_evidence || document_overview_seed) {
                        continue;
                    }
                    if strict_lexical_evidence {
                        if let Some(citation) = lexical.citation.as_mut() {
                            citation
                                .metadata
                                .insert("strong_lexical_evidence".into(), "true".into());
                        }
                    }
                    let document_key = (parent.access_zone_id, parent.document_id);
                    sibling_seed_scores
                        .entry(document_key)
                        .and_modify(|existing| *existing = existing.max(score))
                        .or_insert(score);
                    sibling_seed_query_by_document
                        .entry(document_key)
                        .or_insert_with(|| segment_text.clone());
                    sibling_seed_segment_by_document
                        .entry(document_key)
                        .or_insert(*segment_index);
                    lexical_results.push((
                        lexical,
                        score,
                        matched_discriminating_terms,
                        matched_terms,
                        leading_discriminating_match,
                    ));
                }
                if self.cfg.graph_rag.rerank.mmr_enabled {
                    for (document_key, seed_score) in sibling_seed_scores {
                        let Some(parents) = parents_by_document.get(&document_key) else {
                            continue;
                        };
                        let sibling_query = sibling_seed_query_by_document
                            .get(&document_key)
                            .map(String::as_str)
                            .unwrap_or(query);
                        let sibling_segment_index = sibling_seed_segment_by_document
                            .get(&document_key)
                            .copied()
                            .unwrap_or(0);
                        for parent in parents {
                            if parent.source_block_id.as_deref() == Some("doc-root") {
                                continue;
                            }
                            let mut lexical =
                                search_result_from_lexical_parent(parent, sibling_query);
                            if is_root_container_result(&lexical) {
                                continue;
                            }
                            if let Some(citation) = lexical.citation.as_mut() {
                                citation.metadata.insert(
                                    "query_processing_mode".into(),
                                    query_plan_diagnostics.mode_code().into(),
                                );
                                citation.metadata.insert(
                                    "query_segment_indices".into(),
                                    format!("[{sibling_segment_index}]"),
                                );
                            }
                            let sibling_score = (seed_score * 0.8 + sibling_sequence_bonus(parent))
                                .clamp(0.12, 1.0);
                            if let Some(scores) = lexical.scores.as_mut() {
                                scores.sparse_score = scores.sparse_score.max(sibling_score);
                                scores.fusion_score = scores.fusion_score.max(sibling_score);
                                scores.final_score = scores.final_score.max(sibling_score);
                            }
                            let matched_terms = matched_term_count(&lexical, sibling_query);
                            let matched_discriminating_terms =
                                matched_discriminating_term_count(&lexical, sibling_query);
                            let leading_discriminating_match =
                                leading_discriminating_query_term_matches(&lexical, sibling_query);
                            if !strict_lexical_query_match(
                                matched_terms,
                                matched_discriminating_terms,
                                leading_discriminating_match,
                                query_term_count(sibling_query),
                            ) {
                                continue;
                            }
                            if let Some(citation) = lexical.citation.as_mut() {
                                citation
                                    .metadata
                                    .insert("strong_lexical_evidence".into(), "true".into());
                            }
                            lexical_results.push((
                                lexical,
                                sibling_score,
                                matched_discriminating_terms,
                                matched_terms,
                                leading_discriminating_match,
                            ));
                        }
                    }
                }
                lexical_results.sort_by(
                |(_, left_score, left_discriminating, left_terms, left_leading),
                 (_, right_score, right_discriminating, right_terms, right_leading)| {
                    right_score
                        .partial_cmp(left_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| right_discriminating.cmp(left_discriminating))
                        .then_with(|| right_terms.cmp(left_terms))
                        .then_with(|| right_leading.cmp(left_leading))
                },
            );
                let lexical_take_limit = (candidate_limit as usize)
                    .saturating_mul(3)
                    .max(candidate_limit as usize)
                    .min(self.cfg.limits.search_candidate_limit_max as usize);
                let mut protected_lexical_count = 0usize;
                let lexical_reservation_limit =
                    self.cfg.search.fusion.min_strong_lexical_candidates.max(3);
                for (rank, (mut lexical, lexical_score, _, _, _)) in lexical_results
                    .into_iter()
                    .take(lexical_take_limit)
                    .enumerate()
                {
                    apply_indexed_lexical_rank_score(
                        &mut lexical,
                        lexical_score,
                        rank + 1,
                        self.cfg.search.lexical.lexical_weight,
                        self.cfg.search.rrf_k,
                    );
                    if rank < lexical_reservation_limit
                        && is_strong_lexical_candidate(&lexical)
                        && protected_lexical_count < lexical_reservation_limit
                    {
                        mark_ranking_protection(
                            &mut lexical,
                            RankingProtection {
                                preserve_primary_direct: true,
                                preserve_strong_lexical: true,
                                preserve_unique_source_block: true,
                                preserve_required_segment_coverage: false,
                            },
                        );
                        protected_lexical_count += 1;
                    }
                    let key = result_identity_key(&lexical);
                    if let Some(idx) = direct_result_index_by_key.get(&key).copied() {
                        merge_lexical_backfill_candidate(&mut direct_results[idx], &lexical);
                    } else {
                        direct_result_index_by_key.insert(key, direct_results.len());
                        direct_results.push(lexical);
                    }
                }
                if sparse_branch_candidate_count
                    < self.cfg.search.lexical.run_when_sparse_candidates_below as u32
                    || sparse_top_score < self.cfg.search.lexical.run_when_sparse_top_score_below
                {
                    warnings.push(pb::DiagnosticWarningV005 {
                        code: "LEXICAL_FALLBACK_USED".into(),
                        message:
                            "indexed lexical retrieval supplemented a weak sparse candidate set"
                                .into(),
                    });
                    counter!("astravector_lexical_fallback_total").increment(1);
                }
            }
        }
        tracing::debug!(
            correlation_id = %r.correlation_id,
            parent_fetch_ms,
            fetched_parent_count,
            direct_contexts_count = direct_results.len(),
            "SEARCH_PARENT_FETCH_DONE"
        );
        drop_root_container_results_when_document_has_evidence(&mut direct_results);
        for result in &mut direct_results {
            if let Some(citation) = result.citation.as_mut() {
                citation
                    .metadata
                    .insert("evidence_provenance".into(), "PRIMARY_DIRECT".into());
            }
        }
        let lexical_reservation_limit = self.cfg.search.fusion.min_strong_lexical_candidates.max(3);
        for (fusion_rank, result) in direct_results.iter_mut().enumerate() {
            let lexical_rank = result
                .citation
                .as_ref()
                .and_then(|citation| citation.metadata.get("lexical_rank"))
                .and_then(|rank| rank.parse::<usize>().ok());
            if is_strong_lexical_candidate(result)
                && (fusion_rank < lexical_reservation_limit
                    || lexical_rank.is_some_and(|rank| rank <= lexical_reservation_limit))
            {
                mark_ranking_protection(
                    result,
                    RankingProtection {
                        preserve_primary_direct: true,
                        preserve_strong_lexical: true,
                        preserve_unique_source_block: result_source_block_id(result).is_some(),
                        preserve_required_segment_coverage: false,
                    },
                );
            }
        }
        let dense_trace = if ranking_trace.enabled {
            {
                direct_results
                    .iter()
                    .filter(|result| {
                        result
                            .scores
                            .as_ref()
                            .is_some_and(|scores| scores.dense_score > 0.0)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            }
        } else {
            Vec::new()
        };
        ranking_trace.observe(pb::RankingStageV005::DenseRetrieval, &dense_trace);
        let sparse_trace = if ranking_trace.enabled {
            {
                direct_results
                    .iter()
                    .filter(|result| {
                        result
                            .scores
                            .as_ref()
                            .is_some_and(|scores| scores.sparse_score > 0.0)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            }
        } else {
            Vec::new()
        };
        ranking_trace.observe(pb::RankingStageV005::SparseRetrieval, &sparse_trace);
        let lexical_trace = if ranking_trace.enabled {
            {
                direct_results
                    .iter()
                    .filter(|result| {
                        extraction_retrieval_sources(result)
                            .iter()
                            .any(|source| source == "POSTGRES_FTS")
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            }
        } else {
            Vec::new()
        };
        ranking_trace.observe(pb::RankingStageV005::LexicalRetrieval, &lexical_trace);
        ranking_trace.observe(pb::RankingStageV005::FusionAdmission, &direct_results);
        ranking_trace.observe(pb::RankingStageV005::PostFusionDedup, &direct_results);
        ranking_trace.observe(pb::RankingStageV005::PostgresHydration, &direct_results);
        let mut no_answer_stats = NoAnswerFilterStats::default();
        let no_answer_debug = no_answer_debug_enabled(r.include_debug, &self.cfg.search.no_answer);
        let query_technical_tokens = if self.cfg.search.no_answer.enabled {
            strong_technical_query_tokens(query)
        } else {
            Vec::new()
        };
        let preserve_partial_evidence_for_mmr = self.cfg.graph_rag.rerank.mmr_enabled;
        let pre_mmr_no_answer_started = Instant::now();
        let pre_mmr_before = if ranking_trace.enabled {
            direct_results.clone()
        } else {
            Vec::new()
        };
        no_answer_stats.pre_mmr_filtered_count = if query_plan.mode == QueryProcessingMode::Segmented {
            apply_segmented_pre_mmr_no_answer_filter(
                &mut direct_results,
                &query_plan,
                search_mode,
                &self.cfg.search.no_answer,
                no_answer_debug,
                preserve_partial_evidence_for_mmr,
            )
        } else {
            apply_pre_mmr_no_answer_filter(
                &mut direct_results,
                query,
                &query_technical_tokens,
                search_mode,
                &self.cfg.search.no_answer,
                no_answer_debug,
                preserve_partial_evidence_for_mmr,
            )
        };
        let coverage_after_direct = coverage_for_results(&query_plan, &direct_results);
        if query_plan.mode == QueryProcessingMode::Segmented {
            histogram!("astravector_long_query_coverage_after_direct")
                .record(coverage_after_direct.ratio as f64);
        }
        if query_plan.mode == QueryProcessingMode::Segmented {
            match coverage_after_direct.status {
                QueryEvidenceStatus::Found => {}
                QueryEvidenceStatus::Degraded => {
                    warnings.push(pb::DiagnosticWarningV005 {
                        code: "LONG_QUERY_PARTIAL_COVERAGE".into(),
                        message: format!(
                            "required segment coverage after direct no-answer is {}/{}",
                            coverage_after_direct.required_covered,
                            coverage_after_direct.required_total
                        ),
                    });
                    counter!("astravector_long_query_partial_coverage_total").increment(1);
                }
                QueryEvidenceStatus::Insufficient => {
                    if retrieval_infrastructure_failure {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "LONG_QUERY_COVERAGE_DEGRADED_BY_RETRIEVAL_FAILURE".into(),
                            message: "required intent coverage is unknown because at least one retrieval branch failed".into(),
                        });
                        counter!("astravector_long_query_coverage_unavailable_total").increment(1);
                    } else {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "LONG_QUERY_PARTIAL_COVERAGE".into(),
                            message:
                                "no required long-query intents produced admissible direct evidence"
                                    .into(),
                        });
                        direct_results.clear();
                        counter!("astravector_long_query_partial_coverage_total").increment(1);
                    }
                }
                QueryEvidenceStatus::Unavailable => {
                    warnings.push(pb::DiagnosticWarningV005 {
                        code: "LONG_QUERY_COVERAGE_UNAVAILABLE".into(),
                        message: "required long-query coverage could not be evaluated".into(),
                    });
                    direct_results.clear();
                    counter!("astravector_long_query_coverage_unavailable_total").increment(1);
                }
            }
        }
        ranking_trace.mark_removed(
            pb::RankingStageV005::PreMmrNoAnswer,
            &pre_mmr_before,
            &direct_results,
            pb::CandidateDropReasonV005::NoAnswerFiltered,
            "pre-MMR no-answer policy",
        );
        if no_answer_stats.pre_mmr_filtered_count > 0 {
            counter!("retrieval_no_answer_pre_mmr_filtered_total")
                .increment(no_answer_stats.pre_mmr_filtered_count as u64);
            warnings.push(pb::DiagnosticWarningV005 {
                code: "PRE_MMR_WEAK_CANDIDATE_FILTERED".into(),
                message: format!(
                    "no-answer policy filtered {} weak direct candidates before graph expansion/MMR",
                    no_answer_stats.pre_mmr_filtered_count
                ),
            });
        }
        let pre_mmr_no_answer_ms = pre_mmr_no_answer_started.elapsed().as_millis() as u64;
        let mut graph_expansion_duration_ms = 0_u64;
        let mut graph_candidates_by_relation: HashMap<String, usize> = HashMap::new();
        let mut graph_seed_candidates = Vec::new();
        let mut graph_seed_preview_by_key = HashMap::new();
        let mut graph_seed_source_block_by_key = HashMap::new();
        let mut graph_seed_parent_by_key = HashMap::new();
        let mut graph_seed_document_by_key = HashMap::new();
        let graph_seed_source_results = graph_seed_source_results_for_admitted_parents(
            &direct_results,
            &pre_parent_dedup_graph_seed_results,
        );
        seed_scores.clear();
        for result in graph_seed_source_results {
            let Ok(access_zone_id) = Uuid::parse_str(&result.access_zone_id) else {
                continue;
            };
            let Some(seed_chunk_id) = graph_seed_chunk_id(result) else {
                continue;
            };
            let Ok(parent_chunk_id) = Uuid::parse_str(&result.parent_chunk_id) else {
                continue;
            };
            let key = (access_zone_id, seed_chunk_id);
            let seed_score = graph_seed_score(result);
            let matched_terms = matched_term_count(result, query);
            let matched_discriminating_terms = matched_discriminating_term_count(result, query);
            let strong_lexical_evidence = matched_terms >= 2
                && matched_discriminating_terms >= 1
                && matched_terms.saturating_mul(2) >= query_term_count(query);
            let matched_segment_indices = {
                let passed = result_passed_query_segment_indices(result);
                if passed.is_empty() {
                    result_query_segment_indices(result)
                } else {
                    passed
                }
            };
            let explicit_intent_ids = result_passed_query_intent_ids(result);
            let intent_unit_ids = if explicit_intent_ids.is_empty() {
                query_plan
                    .intent_units
                    .iter()
                    .filter(|intent| {
                        intent
                            .source_segment_indices
                            .iter()
                            .any(|index| matched_segment_indices.contains(index))
                    })
                    .map(|intent| intent.id)
                    .collect()
            } else {
                explicit_intent_ids
            };
            graph_seed_candidates.push(GraphSeedCandidate {
                key,
                parent_key: (access_zone_id, parent_chunk_id),
                score: seed_score,
                matched_terms,
                matched_discriminating_terms,
                strong_lexical_evidence,
                intent_unit_ids,
            });
            seed_scores.entry(key).or_insert(seed_score);
            graph_seed_parent_by_key.insert(key, result.parent_chunk_id.clone());
            graph_seed_document_by_key
                .insert(key, (result.document_id.clone(), result.document_version));
            let source_block_id = result
                .citation
                .as_ref()
                .and_then(|citation| citation.metadata.get("source_block_id"))
                .cloned()
                .unwrap_or_default();
            graph_seed_source_block_by_key.insert(key, source_block_id.clone());
            graph_seed_preview_by_key.entry(key).or_insert_with(|| {
                format!(
                    "{}:{}:{:.3}:{}",
                    seed_chunk_id,
                    source_block_id,
                    seed_score,
                    extraction_retrieval_sources(result).join("+")
                )
            });
        }
        let required_intent_ids = query_plan
            .intent_units
            .iter()
            .filter(|intent| intent.required)
            .map(|intent| intent.id)
            .collect::<Vec<_>>();
        graph_seed_candidates = select_graph_seed_candidates(
            graph_seed_candidates,
            &required_intent_ids,
            query_plan.limits.max_graph_seeds.min(12),
        );
        let graph_seed_intents_by_key = graph_seed_candidates
            .iter()
            .map(|candidate| (candidate.key, candidate.intent_unit_ids.clone()))
            .collect::<HashMap<_, _>>();
        let graph_seed_keys = graph_seed_candidates
            .iter()
            .map(|candidate| candidate.key)
            .collect::<Vec<_>>();
        let graph_seed_preview = graph_seed_candidates
            .iter()
            .take(12)
            .filter_map(|candidate| graph_seed_preview_by_key.get(&candidate.key).cloned())
            .collect::<Vec<_>>()
            .join(",");
        let graph_seed_trace = if ranking_trace.enabled {
            {
                pre_parent_dedup_graph_seed_results
                    .iter()
                    .filter(|result| {
                        graph_seed_chunk_id(result)
                            .zip(Uuid::parse_str(&result.access_zone_id).ok())
                            .is_some_and(|(chunk, zone)| graph_seed_keys.contains(&(zone, chunk)))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            }
        } else {
            Vec::new()
        };
        ranking_trace.observe(pb::RankingStageV005::GraphSeed, &graph_seed_trace);
        let remaining_budget_before_graph_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        let graph_stage_budget = resolve_optional_stage_budget(
            deadline,
            Duration::from_millis(self.cfg.graph_rag.retrieval.timeout_ms),
            Duration::from_millis(self.cfg.graph_rag.retrieval.min_useful_budget_ms),
            Duration::from_millis(self.cfg.graph_rag.retrieval.response_reserve_ms),
        );
        let graph_timeout = graph_stage_budget.unwrap_or_default();
        if r.enable_graph_expansion
            && self.cfg.graph_rag.enabled
            && !graph_seed_keys.is_empty()
            && graph_stage_budget.is_none()
        {
            warnings.push(pb::DiagnosticWarningV005 {
                code: "GRAPH_SKIPPED_INSUFFICIENT_BUDGET".into(),
                message: "Graph expansion skipped to preserve the response deadline reserve".into(),
            });
            counter!("astravector_degraded_path_total", "component" => "graph", "reason" => "insufficient_budget").increment(1);
            counter!("astravector_optional_stage_skipped_total", "stage" => "graph", "reason" => "insufficient_budget").increment(1);
        }
        if r.enable_graph_expansion
            && self.cfg.graph_rag.enabled
            && !graph_seed_keys.is_empty()
            && graph_stage_budget.is_some()
        {
            let maybe_graph_permit = Self::acquire_backpressure_permit(
                self.graph_expansion_semaphore.clone(),
                self.cfg.limits.backpressure_acquire_timeout_ms,
                "graph_expansion",
                1,
                &request_cancel,
            )
            .await;
            if maybe_graph_permit.is_err() {
                if !self
                    .cfg
                    .graph_rag
                    .retrieval
                    .allow_partial_dense_sparse_fallback
                {
                    return Err(Status::resource_exhausted(
                        "graph_expansion_admission_timeout",
                    ));
                }
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "GRAPH_EXPANSION_BACKPRESSURE".into(),
                    message: "Graph expansion skipped because concurrency limit is exceeded".into(),
                });
                counter!("graph_expansion_rejected_total", "reason" => "backpressure").increment(1);
                counter!("astravector_degraded_path_total", "component" => "graph", "reason" => "admission_timeout").increment(1);
                tracing::warn!(correlation_id=%r.correlation_id, reason="admission_timeout", "GRAPH_PATH_DEGRADED_TO_DIRECT_RETRIEVAL");
            }
            if let Ok(_graph_permit) = maybe_graph_permit {
                gauge!("graph_expansion_concurrent_active").set(
                    (self
                        .cfg
                        .limits
                        .max_concurrent_graph_expansion
                        .saturating_sub(self.graph_expansion_semaphore.available_permits()))
                        as f64,
                );
                let graph_expansion_started = std::time::Instant::now();
                let max_related = if r.graph_max_related_contexts == 0 {
                    self.cfg
                        .graph_rag
                        .retrieval
                        .max_related_chunks
                        .min(self.cfg.limits.graph_related_contexts_max) as u32
                } else {
                    r.graph_max_related_contexts
                        .min(self.cfg.limits.graph_related_contexts_max as u32)
                };
                let hydration_rejection_reserve = self
                    .cfg
                    .search
                    .hydration_rejection_reserve
                    .min(self.cfg.search.hydration_rejection_reserve_max);
                let graph_hydration_fetch_limit = max_related
                    .saturating_add(hydration_rejection_reserve)
                    .min(self.cfg.limits.graph_related_contexts_max as u32);
                tracing::debug!(
                    correlation_id = %r.correlation_id,
                    quality_run_id = quality_run_id_filter.as_deref().unwrap_or(""),
                    graph_seed_keys_count = graph_seed_keys.len(),
                    graph_seed_preview = %graph_seed_preview,
                    max_related,
                    graph_hydration_fetch_limit,
                    max_seed_chunks = query_plan.limits.max_graph_seeds.min(12),
                    max_edges_visited = self.cfg.graph_rag.retrieval.max_edges_visited,
                    allowed_relations = ?self.cfg.graph_rag.retrieval.allowed_relations,
                    "GRAPH_EXPANSION_CALL"
                );
                let graph_call = self.repo()?.expand_chunks_1hop_by_seed_keys(
                    &graph_seed_keys,
                    caller_access_level as i16,
                    graph_hydration_fetch_limit,
                    query_plan.limits.max_graph_seeds.min(12),
                    self.cfg.graph_rag.retrieval.max_edges_visited,
                    &self.cfg.graph_rag.retrieval.allowed_relations,
                    quality_run_id_filter.as_deref(),
                );
                let graph_outcome = tokio::select! {
                    _ = request_cancel.cancelled() => {
                        return Err(Status::cancelled("query cancelled during graph expansion"));
                    }
                    outcome = tokio::time::timeout(graph_timeout, graph_call) => outcome,
                };
                match graph_outcome {
                    Ok(Ok(related)) => {
                        let related_preview = related
                            .iter()
                            .take(10)
                            .map(|rel| {
                                format!(
                                    "{}->{}:{}:{:.3}",
                                    rel.seed_chunk_id,
                                    rel.chunk_id,
                                    rel.relation_type.as_str(),
                                    rel.relation_score
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        tracing::debug!(
                            correlation_id = %r.correlation_id,
                            quality_run_id = quality_run_id_filter.as_deref().unwrap_or(""),
                            related_count = related.len(),
                            related_preview = %related_preview,
                            "GRAPH_EXPANSION_RELATED_ROWS"
                        );
                        let related_ids = related.iter().map(|r| r.chunk_id).collect::<Vec<_>>();
                        let contexts = self
                            .repo()?
                            .fetch_contexts_for_graph_related_chunks_multi(
                                &access_zone_ids,
                                &related_ids,
                                caller_access_level as i16,
                            )
                            .await
                            .map_err(Status::from)?;
                        tracing::debug!(
                            correlation_id = %r.correlation_id,
                            related_ids_count = related_ids.len(),
                            graph_contexts_count = contexts.len(),
                            "GRAPH_EXPANSION_CONTEXTS_FETCHED"
                        );
                        let by_chunk: HashMap<
                            (Uuid, Uuid),
                            crate::persistence::GraphChunkContextRecord,
                        > = contexts
                            .into_iter()
                            .map(|c| ((c.parent_record.access_zone_id, c.chunk_id), c))
                            .collect();
                        let scoring = graph_scoring_options_from_config(&self.cfg);
                        let mut filtered_candidates = 0usize;
                        for rel in related {
                            if graph_results.len()
                                >= (max_related as usize).min(
                                    self.cfg
                                        .graph_rag
                                        .retrieval
                                        .graph_expansion_result_limit
                                        .max(1),
                                )
                            {
                                break;
                            }
                            let graph_lookup_key = (rel.access_zone_id, rel.chunk_id);
                            let Some(ctx) = by_chunk.get(&graph_lookup_key) else {
                                continue;
                            };
                            *graph_candidates_by_relation
                                .entry(rel.relation_type.as_str().to_string())
                                .or_insert(0) += 1;
                            metrics::counter!("graph_expansion_candidates_by_relation_total", "relation_type" => rel.relation_type.as_str().to_string()).increment(1);
                            let parent_id = ctx.parent_record.id.to_string();
                            let seed_score = seed_scores
                                .get(&(rel.seed_access_zone_id, rel.seed_chunk_id))
                                .copied()
                                .unwrap_or(0.5);
                            let raw_graph_score = crate::graph::score_graph_candidate_with_options(
                                seed_score,
                                rel.relation_type,
                                rel.relation_score,
                                rel.hop_distance,
                                &scoring,
                            );
                            if raw_graph_score < scoring.graph_min_score {
                                filtered_candidates += 1;
                                metrics::counter!("graph_candidates_filtered_by_relation_total", "relation_type" => rel.relation_type.as_str().to_string()).increment(1);
                                continue;
                            }
                            let relation_weight =
                                crate::graph::relation_weight(&scoring, rel.relation_type);
                            let hop_penalty = crate::graph::hop_penalty(&scoring, rel.hop_distance);
                            let adjusted_edge_weight = if !rel.relation_type.is_structural() {
                                rel.relation_score.powf(scoring.semantic_power)
                            } else {
                                rel.relation_score
                            };
                            let hit = QdrantSearchHit {
                                id: rel.chunk_id,
                                score: raw_graph_score,
                                dense_score: 0.0,
                                sparse_score: 0.0,
                                fusion_score: raw_graph_score,
                                dense_rank: None,
                                sparse_rank: None,
                                payload: serde_json::json!({
                                    "access_zone_id": rel.access_zone_id.to_string(),
                                    "binding_id": ctx.binding_id.to_string(),
                                    "chunk_id": rel.chunk_id.to_string(),
                                    "parent_chunk_id": parent_id,
                                    "source_block_id": ctx.trace.as_ref().and_then(|t| t.source_block_id.clone()).unwrap_or_default(),
                                    "chunk_granularity": "GRAPH_EXPANDED",
                                    "source_chunk_granularity": ctx.source_chunk_granularity.clone().unwrap_or_default(),
                                    "qdrant_point_id": ctx.qdrant_point_id.map(|v| v.to_string()).unwrap_or_default(),
                                    "representation_type": ctx.representation_type.clone().unwrap_or_else(|| "ORIGINAL".into()),
                                    "dense_version": ctx.dense_version.clone().unwrap_or_default(),
                                    "model_version": ctx.model_version.clone().unwrap_or_default(),
                                    "payload_version": ctx.payload_version.unwrap_or_default()
                                }),
                            };
                            let mut graph_result = search_result_from_hit(
                                &ctx.parent_record,
                                &hit,
                                ctx.matched_text.clone(),
                                ctx.trace.as_ref(),
                            );
                            if let Some(citation) = graph_result.citation.as_mut() {
                                citation
                                    .metadata
                                    .insert("retrieval_source".into(), "GRAPH_EXPANDED".into());
                                citation.metadata.insert(
                                    "graph_seed_access_zone_id".into(),
                                    rel.seed_access_zone_id.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_seed_chunk_id".into(),
                                    rel.seed_chunk_id.to_string(),
                                );
                                if let Some(parent_id) = graph_seed_parent_by_key
                                    .get(&(rel.seed_access_zone_id, rel.seed_chunk_id))
                                {
                                    citation.metadata.insert(
                                        "graph_seed_parent_chunk_id".into(),
                                        parent_id.clone(),
                                    );
                                }
                                if let Some((document_id, document_version)) =
                                    graph_seed_document_by_key
                                        .get(&(rel.seed_access_zone_id, rel.seed_chunk_id))
                                {
                                    citation.metadata.insert(
                                        "graph_seed_document_id".into(),
                                        document_id.clone(),
                                    );
                                    citation.metadata.insert(
                                        "graph_seed_document_version".into(),
                                        document_version.to_string(),
                                    );
                                }
                                if let Some(intent_ids) = graph_seed_intents_by_key
                                    .get(&(rel.seed_access_zone_id, rel.seed_chunk_id))
                                {
                                    citation.metadata.insert(
                                        "passed_query_intent_ids".into(),
                                        serde_json::to_string(intent_ids)
                                            .unwrap_or_else(|_| "[]".into()),
                                    );
                                    let inherited = intent_ids
                                        .iter()
                                        .copied()
                                        .map(CandidateIntentEvidence::graph_origin)
                                        .collect::<Vec<_>>();
                                    citation.metadata.insert(
                                        "candidate_intent_evidence".into(),
                                        serde_json::to_string(&inherited)
                                            .unwrap_or_else(|_| "[]".into()),
                                    );
                                }
                                if let Some(source_block_id) = graph_seed_source_block_by_key
                                    .get(&(rel.seed_access_zone_id, rel.seed_chunk_id))
                                {
                                    citation.metadata.insert(
                                        "graph_seed_source_block_id".into(),
                                        source_block_id.clone(),
                                    );
                                }
                                citation.metadata.insert(
                                    "graph_relation_id".into(),
                                    rel.relation_identity.clone(),
                                );
                                citation
                                    .metadata
                                    .insert("graph_edge_id".into(), rel.edge_id.to_string());
                                citation.metadata.insert(
                                    "graph_relation_source".into(),
                                    rel.relation_source.clone(),
                                );
                                citation.metadata.insert(
                                    "graph_relation_type".into(),
                                    rel.relation_type.as_str().into(),
                                );
                                citation.metadata.insert(
                                    "graph_related_access_zone_id".into(),
                                    rel.access_zone_id.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_related_document_id".into(),
                                    rel.related_document_id.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_related_document_version".into(),
                                    rel.related_document_version.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_related_chunk_id".into(),
                                    rel.chunk_id.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_related_parent_chunk_id".into(),
                                    ctx.parent_record.id.to_string(),
                                );
                                citation
                                    .metadata
                                    .insert("graph_binding_id".into(), ctx.binding_id.to_string());
                                citation.metadata.insert(
                                    "graph_relation_score".into(),
                                    rel.relation_score.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_relation_weight".into(),
                                    relation_weight.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_hop_distance".into(),
                                    rel.hop_distance.to_string(),
                                );
                                citation
                                    .metadata
                                    .insert("graph_hop_penalty".into(), hop_penalty.to_string());
                                citation.metadata.insert(
                                    "graph_adjusted_edge_weight".into(),
                                    adjusted_edge_weight.to_string(),
                                );
                                citation.metadata.insert(
                                    "graph_semantic_power".into(),
                                    if !rel.relation_type.is_structural() {
                                        scoring.semantic_power.to_string()
                                    } else {
                                        String::new()
                                    },
                                );
                                citation
                                    .metadata
                                    .insert("graph_score".into(), raw_graph_score.to_string());
                                citation.metadata.insert(
                                    "retrieval_sources".into(),
                                    "[\"GRAPH_EXPANDED\"]".into(),
                                );
                                citation
                                    .metadata
                                    .insert("evidence_provenance".into(), "GRAPH_EXPANDED".into());
                                citation.metadata.insert(
                                    "graph_merge_strategy".into(),
                                    self.cfg.graph_rag.retrieval.graph_merge_strategy.clone(),
                                );
                                if citation
                                    .metadata
                                    .get("qdrant_point_id")
                                    .map(|v| !v.is_empty())
                                    .unwrap_or(false)
                                {
                                    metrics::counter!("graph_candidate_identity_found_total")
                                        .increment(1);
                                    if citation
                                        .metadata
                                        .get("representation_type")
                                        .map(|v| v == "ORIGINAL")
                                        .unwrap_or(false)
                                    {
                                        metrics::counter!(
                                            "graph_candidate_identity_original_selected_total"
                                        )
                                        .increment(1);
                                    } else {
                                        metrics::counter!("graph_candidate_identity_fallback_representation_total").increment(1);
                                    }
                                } else {
                                    metrics::counter!("graph_candidate_identity_missing_total")
                                        .increment(1);
                                }
                            }
                            calibrate_result_score(
                                &mut graph_result,
                                "GRAPH_EXPANDED",
                                self.cfg.graph_rag.scoring.direct_score_weight,
                                self.cfg.graph_rag.scoring.graph_score_weight,
                                self.cfg.graph_rag.scoring.graph_score_bias,
                            );
                            graph_results.push(graph_result);
                        }
                        metrics::counter!("graph_expansion_candidates_filtered_total")
                            .increment(filtered_candidates as u64);
                        if graph_results.is_empty() {
                            tracing::warn!(
                                correlation_id = %r.correlation_id,
                                quality_run_id = quality_run_id_filter.as_deref().unwrap_or(""),
                                filtered_candidates,
                                relations = ?graph_candidates_by_relation,
                                "GRAPH_EXPANSION_ZERO_GRAPH_RESULTS"
                            );
                        }
                    }
                    Ok(Err(e)) => warnings.push(pb::DiagnosticWarningV005 {
                        code: "GRAPH_EXPANSION_FAILED".into(),
                        message: format!(
                            "Graph expansion failed; vector-only results returned: {e}"
                        ),
                    }),
                    Err(_) => warnings.push(pb::DiagnosticWarningV005 {
                        code: "GRAPH_EXPANSION_TIMEOUT".into(),
                        message: "Graph expansion timed out; vector-only results returned".into(),
                    }),
                }
                graph_expansion_duration_ms = graph_expansion_started.elapsed().as_millis() as u64;
            }
            tracing::info!(
                direct_candidates = direct_results.len(),
                graph_candidates = graph_results.len(),
                duration_ms = graph_expansion_duration_ms,
                "GRAPH_EXPANSION_COMPLETED"
            );
        }
        retain_results_outside_rejected_parents(&mut direct_results, &rejected_parent_keys);
        retain_results_outside_rejected_parents(&mut graph_results, &rejected_parent_keys);
        ranking_trace.observe(pb::RankingStageV005::GraphExpansion, &graph_results);
        let merge_started = std::time::Instant::now();
        let direct_count = direct_results.len();
        let graph_count = graph_results.len();
        let final_limit = resolve_final_context_limit(
            self.cfg.graph_rag.retrieval.final_context_limit,
            top_k as usize,
            &self.cfg.graph_rag.retrieval.final_context_limit_mode,
        );
        let embedding_fetch_limit = self
            .cfg
            .graph_rag
            .rerank
            .mmr_candidate_limit
            .max(final_limit);
        let remaining_budget_before_mmr_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        let mmr_stage_budget = resolve_optional_stage_budget(
            deadline,
            Duration::from_millis(self.cfg.graph_rag.rerank.embedding_fetch_timeout_ms),
            Duration::from_millis(
                self.cfg
                    .graph_rag
                    .rerank
                    .embedding_fetch_min_useful_budget_ms,
            ),
            Duration::from_millis(self.cfg.graph_rag.rerank.response_reserve_ms),
        );
        let mmr_fetch_timeout = mmr_stage_budget.unwrap_or_default();
        let embedding_fetch_stats = if mmr_stage_budget.is_none() {
            warnings.push(pb::DiagnosticWarningV005 {
                code: "MMR_EMBEDDING_FETCH_SKIPPED_INSUFFICIENT_BUDGET".into(),
                message:
                    "MMR embedding hydration skipped to preserve the response deadline reserve"
                        .into(),
            });
            counter!("astravector_degraded_path_total", "component" => "mmr", "reason" => "insufficient_budget").increment(1);
            counter!("astravector_optional_stage_skipped_total", "stage" => "mmr_embedding_fetch", "reason" => "insufficient_budget").increment(1);
            MmrEmbeddingFetchStats::skipped()
        } else if let Ok(_mmr_permit) = Self::acquire_backpressure_permit(
            self.mmr_fetch_semaphore.clone(),
            self.cfg.limits.backpressure_acquire_timeout_ms,
            "mmr_fetch",
            1,
            &request_cancel,
        )
        .await
        {
            gauge!("mmr_fetch_concurrent_active").set(
                (self
                    .cfg
                    .limits
                    .max_concurrent_mmr_fetch
                    .saturating_sub(self.mmr_fetch_semaphore.available_permits()))
                    as f64,
            );
            tokio::select! {
                _ = request_cancel.cancelled() => {
                    return Err(Status::cancelled("query cancelled during MMR embedding fetch"));
                }
                stats = enrich_dense_embeddings_for_mmr(
                    self.repo.as_ref(),
                    &access_zone_ids,
                    &mut direct_results,
                    &mut graph_results,
                    &self.cfg,
                    embedding_fetch_limit,
                    mmr_fetch_timeout,
                ) => stats,
            }
        } else {
            if !self
                .cfg
                .graph_rag
                .retrieval
                .allow_partial_dense_sparse_fallback
            {
                return Err(Status::resource_exhausted("mmr_fetch_admission_timeout"));
            }
            counter!("mmr_fetch_rejected_total", "reason" => "backpressure").increment(1);
            counter!("astravector_degraded_path_total", "component" => "mmr", "reason" => "admission_timeout").increment(1);
            tracing::warn!(correlation_id=%r.correlation_id, reason="admission_timeout", "MMR_PATH_DEGRADED_TO_TOKEN_FALLBACK");
            warnings.push(pb::DiagnosticWarningV005 {
                code: "MMR_FETCH_BACKPRESSURE".into(),
                message: "MMR embedding fetch skipped because concurrency limit is exceeded".into(),
            });
            MmrEmbeddingFetchStats::skipped()
        };
        let broad_coverage_candidates =
            if self.cfg.graph_rag.rerank.mmr_enabled && is_broad_coverage_query(query) {
                Some(
                    direct_results
                        .iter()
                        .chain(graph_results.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            };
        for result in direct_results
            .iter_mut()
            .take(self.cfg.graph_rag.retrieval.min_direct_contexts)
        {
            mark_ranking_protection(
                result,
                RankingProtection {
                    preserve_primary_direct: true,
                    preserve_strong_lexical: is_strong_lexical_candidate(result),
                    preserve_unique_source_block: result_source_block_id(result).is_some(),
                    preserve_required_segment_coverage: false,
                },
            );
        }
        if reserve_required_segment_coverage(&mut direct_results, &query_plan, final_limit) {
            warnings.push(pb::DiagnosticWarningV005 {
                code: "LONG_QUERY_COVERAGE_EXCEEDS_CONTEXT_LIMIT".into(),
                message: format!(
                    "required query segment count exceeds final context limit {final_limit}"
                ),
            });
        }
        let max_graph_contexts = ((final_limit as f32)
            * self.cfg.graph_rag.retrieval.max_graph_fraction)
            .floor() as usize;
        let mut mmr_input_trace = if ranking_trace.enabled {
            direct_results.clone()
        } else {
            Vec::new()
        };
        if ranking_trace.enabled {
            mmr_input_trace.extend(graph_results.iter().cloned());
        }
        ranking_trace.observe(pb::RankingStageV005::GraphMerge, &mmr_input_trace);
        ranking_trace.observe(pb::RankingStageV005::MmrInput, &mmr_input_trace);
        let selection_result = select_results_with_strategy_aware_mmr(
            direct_results,
            graph_results,
            final_limit,
            &self.cfg.graph_rag.retrieval.graph_merge_strategy,
            self.cfg
                .graph_rag
                .retrieval
                .direct_context_limit
                .max(self.cfg.graph_rag.retrieval.min_direct_contexts),
            self.cfg
                .graph_rag
                .retrieval
                .graph_context_append_limit
                .min(max_graph_contexts),
            self.cfg.graph_rag.rerank.mmr_enabled,
            self.cfg.graph_rag.rerank.mmr_lambda,
            self.cfg.graph_rag.rerank.mmr_lambda_direct,
            self.cfg.graph_rag.rerank.mmr_lambda_graph,
            self.cfg.graph_rag.rerank.mmr_candidate_limit,
            &self.cfg.graph_rag.rerank.mmr_similarity_source,
            &self.cfg.graph_rag.rerank.mmr_fallback_similarity_source,
            self.cfg.graph_rag.rerank.mmr_allow_direct_candidates,
            self.cfg.graph_rag.rerank.mmr_allow_graph_candidates,
            self.cfg
                .graph_rag
                .retrieval
                .max_graph_relations_debug_per_candidate,
        );
        let merge_duration_ms = merge_started.elapsed().as_millis() as u64;
        let mmr_result = selection_result.mmr.clone();
        let mut results = selection_result.results;
        ranking_trace.mark_removed(
            pb::RankingStageV005::MmrSelected,
            &mmr_input_trace,
            &results,
            pb::CandidateDropReasonV005::MmrLimit,
            "MMR selection or context limit",
        );
        if let Some(candidates) = broad_coverage_candidates.as_deref() {
            reinforce_broad_coverage_results(&mut results, candidates, final_limit);
        }
        let post_mmr_no_answer_started = Instant::now();
        let post_mmr_before = if ranking_trace.enabled {
            results.clone()
        } else {
            Vec::new()
        };
        let post_mmr_individual_filtered = if query_plan.mode == QueryProcessingMode::Single {
            apply_post_mmr_technical_no_answer_filter(
                &mut results,
                query,
                &query_technical_tokens,
                search_mode,
                &self.cfg.search.no_answer,
            )
        } else {
            0
        };
        if post_mmr_individual_filtered > 0 {
            warnings.push(pb::DiagnosticWarningV005 {
                code: "POST_MMR_WEAK_CANDIDATE_FILTERED".into(),
                message: format!(
                    "post-MMR evidence policy removed {post_mmr_individual_filtered} weak technical candidates"
                ),
            });
        }
        let post_mmr_no_answer_triggered = should_clear_post_mmr_results(
            &results,
            Some(&query_plan),
            query,
            &query_technical_tokens,
            search_mode,
            &self.cfg.search.no_answer,
        );
        if post_mmr_no_answer_triggered {
            no_answer_stats.post_mmr_triggered_count = 1;
            counter!("retrieval_no_answer_post_mmr_triggered_total").increment(1);
            warnings.push(pb::DiagnosticWarningV005 {
                code: "POST_MMR_NO_ANSWER_TRIGGERED".into(),
                message: "final no-answer policy returned an empty context set because final evidence was below configured thresholds".into(),
            });
            warnings.push(pb::DiagnosticWarningV005 {
                code: "FINAL_CONTEXT_SET_TOO_WEAK".into(),
                message: "all final contexts were below the no-answer branch threshold after MMR"
                    .into(),
            });
            results.clear();
        }
        let coverage_after_mmr = coverage_for_results(&query_plan, &results);
        if query_plan.mode == QueryProcessingMode::Segmented {
            histogram!("astravector_long_query_coverage_after_mmr")
                .record(coverage_after_mmr.ratio as f64);
        }
        ranking_trace.mark_removed(
            pb::RankingStageV005::PostMmrNoAnswer,
            &post_mmr_before,
            &results,
            pb::CandidateDropReasonV005::NoAnswerFiltered,
            "post-MMR no-answer policy",
        );
        let post_mmr_no_answer_ms = post_mmr_no_answer_started.elapsed().as_millis() as u64;
        let token_budget_before =
            estimate_results_tokens(&results, self.cfg.rag_context.chars_per_token);
        let token_budget_started = Instant::now();
        let token_budget_input = if ranking_trace.enabled {
            results.clone()
        } else {
            Vec::new()
        };
        let (dropped_chunk_ids, token_budget_warning_codes, dropped_chunk_count) =
            apply_token_budget_truncation(&mut results, &self.cfg.rag_context);
        ranking_trace.mark_removed(
            pb::RankingStageV005::TokenBudget,
            &token_budget_input,
            &results,
            pb::CandidateDropReasonV005::TokenBudgetDrop,
            "context token budget",
        );
        let token_budget_ms = token_budget_started.elapsed().as_millis() as u64;
        for code in &token_budget_warning_codes {
            warnings.push(pb::DiagnosticWarningV005 {
                code: code.clone(),
                message: format!("token budget warning: {code}"),
            });
        }
        let token_budget_after =
            estimate_results_tokens(&results, self.cfg.rag_context.chars_per_token);
        let coverage_after_token_budget = coverage_for_results(&query_plan, &results);
        if dropped_chunk_count > 0 {
            counter!("rag_context_token_budget_applied_total").increment(1);
            counter!("rag_context_chunks_dropped_total").increment(dropped_chunk_count as u64);
        }
        let visibility_recheck_started = Instant::now();
        let visibility_input = if ranking_trace.enabled {
            results.clone()
        } else {
            Vec::new()
        };
        let final_visibility_candidates = results
            .iter()
            .enumerate()
            .filter_map(|(ordinal, result)| {
                Some((
                    ordinal,
                    FinalVisibilityCandidate {
                        access_zone_id: Uuid::parse_str(&result.access_zone_id).ok()?,
                        matched_chunk_id: Uuid::parse_str(&result.matched_chunk_id).ok()?,
                        parent_chunk_id: Uuid::parse_str(&result.parent_chunk_id).ok()?,
                        binding_id: result
                            .citation
                            .as_ref()
                            .and_then(|citation| citation.metadata.get("binding_id"))
                            .map(|value| Uuid::parse_str(value))
                            .transpose()
                            .ok()?,
                    },
                ))
            })
            .collect::<Vec<_>>();
        if !results.is_empty() {
            let candidate_result_ordinals = final_visibility_candidates
                .iter()
                .map(|(ordinal, _)| *ordinal)
                .collect::<Vec<_>>();
            let candidates = final_visibility_candidates
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect::<Vec<_>>();
            // Supersedes filter_visible_chunk_ids_multi and
            // visible.contains(&(zone_id, chunk_id)) with full result identity validation.
            let visible_candidate_ordinals = self
                .repo()?
                .filter_visible_search_results_batch(&candidates, caller_access_level as i16)
                .await
                .map_err(Status::from)?;
            let visible_ordinals = visible_candidate_ordinals
                .into_iter()
                .filter_map(|ordinal| candidate_result_ordinals.get(ordinal).copied())
                .collect::<HashSet<_>>();
            let before = results.len();
            let mut ordinal = 0usize;
            results.retain(|_| {
                let visible = visible_ordinals.contains(&ordinal);
                ordinal += 1;
                visible
            });
            let dropped = before.saturating_sub(results.len());
            counter!("retrieve_context_final_visibility_recheck_total").increment(1);
            if dropped > 0 {
                counter!("retrieve_context_final_visibility_dropped_total")
                    .increment(dropped as u64);
                warnings.push(pb::DiagnosticWarningV005 {
                    code: "FINAL_VISIBILITY_RECHECK_DROPPED_CONTEXTS".into(),
                    message: format!(
                        "final PostgreSQL visibility recheck dropped {dropped} stale contexts"
                    ),
                });
            }
        }
        ranking_trace.mark_removed(
            pb::RankingStageV005::VisibilityRecheck,
            &visibility_input,
            &results,
            pb::CandidateDropReasonV005::VisibilityRejected,
            "final PostgreSQL visibility recheck",
        );
        let visibility_recheck_ms = visibility_recheck_started.elapsed().as_millis() as u64;
        let coverage_after_visibility = coverage_for_results(&query_plan, &results);
        if query_plan.mode == QueryProcessingMode::Segmented {
            histogram!("astravector_long_query_coverage_after_visibility")
                .record(coverage_after_visibility.ratio as f64);
        }
        if query_plan.mode == QueryProcessingMode::Segmented
            && coverage_after_visibility.ratio < coverage_after_token_budget.ratio
        {
            warnings.push(pb::DiagnosticWarningV005 {
                code: "LONG_QUERY_COVERAGE_REDUCED_BY_VISIBILITY_RECHECK".into(),
                message: format!(
                    "required segment coverage decreased from {:.3} to {:.3} during final visibility recheck",
                    coverage_after_token_budget.ratio, coverage_after_visibility.ratio
                ),
            });
        }
        if query_plan.mode == QueryProcessingMode::Segmented {
            match coverage_after_visibility.status {
                QueryEvidenceStatus::Found => {}
                QueryEvidenceStatus::Degraded => {
                    if !warnings
                        .iter()
                        .any(|warning| warning.code == "LONG_QUERY_PARTIAL_COVERAGE")
                    {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "LONG_QUERY_PARTIAL_COVERAGE".into(),
                            message: format!(
                                "final required segment coverage is {}/{}",
                                coverage_after_visibility.required_covered,
                                coverage_after_visibility.required_total
                            ),
                        });
                    }
                }
                QueryEvidenceStatus::Insufficient => {
                    warnings.push(pb::DiagnosticWarningV005 {
                        code: if retrieval_infrastructure_failure {
                            "LONG_QUERY_COVERAGE_DEGRADED_BY_RETRIEVAL_FAILURE".into()
                        } else {
                            "LONG_QUERY_INSUFFICIENT_COVERAGE".into()
                        },
                        message: if retrieval_infrastructure_failure {
                            "final intent coverage is unknown because a retrieval branch failed"
                                .into()
                        } else {
                            "final result set covers no required query intents".into()
                        },
                    });
                    results.clear();
                }
                QueryEvidenceStatus::Unavailable => {
                    warnings.push(pb::DiagnosticWarningV005 {
                        code: "LONG_QUERY_COVERAGE_UNAVAILABLE".into(),
                        message: "final required segment coverage could not be evaluated".into(),
                    });
                    results.clear();
                    counter!("astravector_long_query_coverage_unavailable_total").increment(1);
                }
            }
        }
        strip_internal_embedding_metadata(&mut results);
        ranking_trace.observe(pb::RankingStageV005::FinalSelection, &results);
        let ranking_trace = ranking_trace.finish();
        let final_results_count = results.len();
        counter!("retrieved_contexts_total").increment(final_results_count as u64);
        if final_results_count == 0 {
            counter!("retrieved_contexts_empty_total").increment(1);
        }
        for result in &results {
            for source in extraction_retrieval_sources(result) {
                counter!("retrieved_contexts_by_source_total", "source" => source).increment(1);
            }
        }
        gauge!("retrieve_context_final_token_count").set(token_budget_after as f64);
        if mmr_result.enabled {
            metrics::counter!("graph_mmr_enabled_total").increment(1);
        } else {
            metrics::counter!("graph_mmr_disabled_total").increment(1);
        }
        metrics::counter!("graph_mmr_candidates_total").increment(mmr_result.input_count as u64);
        metrics::counter!("graph_mmr_selected_total").increment(mmr_result.selected_count as u64);
        metrics::histogram!("graph_mmr_duration_ms").record(mmr_result.duration_ms as f64);
        metrics::counter!("graph_mmr_embedding_missing_total")
            .increment(mmr_result.embedding_missing_count as u64);
        metrics::counter!("graph_mmr_token_fallback_total")
            .increment(mmr_result.token_fallback_count as u64);
        metrics::counter!("graph_merge_direct_candidates_total").increment(direct_count as u64);
        metrics::counter!("graph_merge_graph_candidates_total").increment(graph_count as u64);
        metrics::counter!("graph_merge_final_candidates_total")
            .increment(final_results_count as u64);
        metrics::counter!("graph_merge_deduplicated_total")
            .increment(selection_result.deduplicated_count as u64);
        for (relation_type, count) in &graph_candidates_by_relation {
            metrics::counter!("graph_merge_candidates_by_relation_total", "relation_type" => relation_type.clone()).increment(*count as u64);
        }
        metrics::histogram!("graph_merge_duration_ms").record(merge_duration_ms as f64);
        tracing::info!(
            direct_candidates = direct_count,
            graph_candidates = graph_count,
            final_candidates = final_results_count,
            deduplicated_candidates = selection_result.deduplicated_count,
            no_answer_pre_mmr_filtered = no_answer_stats.pre_mmr_filtered_count,
            no_answer_post_mmr_triggered = no_answer_stats.post_mmr_triggered_count,
            strategy = self.cfg.graph_rag.retrieval.graph_merge_strategy.as_str(),
            final_context_limit = final_limit,
            duration_ms = merge_duration_ms,
            "GRAPH_MERGE_COMPLETED"
        );
        let query_segments_v008 = query_segment_diagnostics(&query_plan, &results);
        let query_status = if hydration_degradation.degraded
            || retrieval_status == SegmentRetrievalStatus::PartialFailure
            || matches!(
                coverage_after_visibility.status,
                QueryEvidenceStatus::Degraded | QueryEvidenceStatus::Unavailable
            ) {
            "degraded"
        } else {
            "success"
        };
        counter!("astravector_query_total", "tier" => query_tier, "status" => query_status)
            .increment(1);
        histogram!("astravector_query_duration_seconds", "tier" => query_tier)
            .record(started.elapsed().as_secs_f64());
        histogram!("astravector_intent_coverage_ratio", "tier" => query_tier, "stage" => "final_visibility")
            .record(coverage_after_visibility.ratio as f64);
        histogram!("astravector_graph_seed_count", "tier" => query_tier)
            .record(graph_seed_candidates.len() as f64);
        if query_plan.mode == QueryProcessingMode::Segmented {
            histogram!("astravector_long_query_duration_seconds")
                .record(started.elapsed().as_secs_f64());
        }
        Ok(Response::new(pb::SearchResponseV004 {
            results,
            degradation: hydration_degradation
                .degraded
                .then_some(hydration_degradation),
            diagnostics: Some(pb::SearchDiagnosticsV004 {
                query_embedding_ms,
                qdrant_search_ms,
                parent_fetch_ms,
                total_ms: started.elapsed().as_millis() as u64,
                candidate_count: hits.len() as u32,
                parent_group_count: fetched_parent_count as u32,
                direct_candidates_count: direct_count as u32,
                graph_candidates_count: graph_count as u32,
                merged_candidates_count: selection_result.merged_count as u32,
                final_candidates_count: final_results_count as u32,
                deduplicated_candidates_count: selection_result.deduplicated_count as u32,
                graph_candidates_by_relation_json: serde_json::to_string(
                    &graph_candidates_by_relation,
                )
                .unwrap_or_else(|_| "{}".into()),
                graph_expansion_duration_ms,
                graph_merge_duration_ms: merge_duration_ms,
                graph_merge_strategy: self.cfg.graph_rag.retrieval.graph_merge_strategy.clone(),
                final_context_limit: final_limit as u32,
                final_context_limit_mode: self
                    .cfg
                    .graph_rag
                    .retrieval
                    .final_context_limit_mode
                    .clone(),
                graph_min_score: self.cfg.graph_rag.scoring.graph_min_score,
                semantic_power: self.cfg.graph_rag.scoring.semantic_power,
                mmr_enabled: mmr_result.enabled,
                mmr_lambda: self.cfg.graph_rag.rerank.mmr_lambda,
                mmr_candidate_count: mmr_result.input_count as u32,
                mmr_selected_count: mmr_result.selected_count as u32,
                mmr_duration_ms: mmr_result.duration_ms,
                mmr_similarity_source: mmr_result.similarity_source.clone(),
                learned_reranker_enabled: self.cfg.graph_rag.rerank.learned_reranker_enabled,
                learned_reranker_provider: self
                    .cfg
                    .graph_rag
                    .rerank
                    .learned_reranker_provider
                    .clone(),
                direct_context_limit: self.cfg.graph_rag.retrieval.direct_context_limit as u32,
                graph_context_append_limit: self.cfg.graph_rag.retrieval.graph_context_append_limit
                    as u32,
                direct_score_weight: self.cfg.graph_rag.scoring.direct_score_weight,
                graph_score_weight: self.cfg.graph_rag.scoring.graph_score_weight,
                graph_score_bias: self.cfg.graph_rag.scoring.graph_score_bias,
                score_normalization: self.cfg.graph_rag.scoring.score_normalization.clone(),
                mmr_lambda_direct: self.cfg.graph_rag.rerank.mmr_lambda_direct,
                mmr_lambda_graph: self.cfg.graph_rag.rerank.mmr_lambda_graph,
                mmr_embedding_fetch_requested: embedding_fetch_stats.requested as u32,
                mmr_embedding_fetch_found: embedding_fetch_stats.found as u32,
                mmr_embedding_fetch_missing: embedding_fetch_stats.missing as u32,
                mmr_embedding_fetch_duration_ms: embedding_fetch_stats.duration_ms,
                mmr_embedding_cache_hits: embedding_fetch_stats.cache_hits as u32,
                mmr_embedding_cache_misses: embedding_fetch_stats.cache_misses as u32,
                mmr_embedding_fetch_errors: embedding_fetch_stats.errors as u32,
                mmr_embedding_fetch_timeouts: embedding_fetch_stats.timeouts as u32,
                mmr_embedding_fetch_skipped_all_present: embedding_fetch_stats.skipped_all_present,
                mmr_embedding_fetch_skipped_small_pool: embedding_fetch_stats.skipped_small_pool,
                mmr_dense_pair_comparisons: mmr_result.dense_pair_comparisons as u32,
                mmr_token_pair_comparisons: mmr_result.token_pair_comparisons as u32,
                mmr_effective_similarity_source: mmr_result.similarity_source.clone(),
                warning_codes: warnings.iter().map(|w| w.code.clone()).collect(),
                token_budget_enabled: self.cfg.rag_context.token_budget_enabled,
                max_context_tokens: self.cfg.rag_context.max_context_tokens as u32,
                estimated_context_tokens_before: token_budget_before as u32,
                estimated_context_tokens_after: token_budget_after as u32,
                context_chunks_dropped_by_token_budget: dropped_chunk_count,
                token_truncation_strategy: self.cfg.rag_context.truncation_strategy.clone(),
                dropped_chunk_ids: dropped_chunk_ids.iter().take(50).cloned().collect(),
                huge_chunk_strategy: self.cfg.rag_context.huge_chunk_strategy.clone(),
                dense_branch_executed,
                sparse_branch_executed,
                fusion_executed,
                dense_branch_candidate_count,
                sparse_branch_candidate_count,
                fusion_candidate_count,
                dense_search_ms,
                sparse_search_ms,
                lexical_search_ms,
                fusion_ms,
                postgres_hydration_ms: parent_fetch_ms,
                pre_mmr_no_answer_ms,
                graph_ms: graph_expansion_duration_ms,
                mmr_embedding_fetch_ms: embedding_fetch_stats.duration_ms,
                mmr_selection_ms: mmr_result.duration_ms,
                post_mmr_no_answer_ms,
                token_budget_ms,
                visibility_recheck_ms,
                lexical_candidate_count,
                fused_candidate_count: fusion_candidate_count,
                hydrated_candidate_count: fetched_parent_count as u32,
                final_candidate_count: final_results_count as u32,
                remaining_budget_before_graph_ms,
                remaining_budget_before_mmr_ms,
                ranking_trace: Some(ranking_trace),
                query_processing_mode: query_plan_diagnostics.mode_code().into(),
                query_original_token_count: query_plan.original_token_count as u32,
                query_segment_count: query_plan.segments.len() as u32,
                query_was_truncated: query_plan_diagnostics.query_was_truncated,
                query_coverage_ratio: coverage_after_visibility.ratio,
                query_required_segments: coverage_after_visibility.required_total as u32,
                query_covered_required_segments: coverage_after_visibility.required_covered as u32,
                query_segment_sha256: query_plan_diagnostics.segment_sha256,
                effective_query_timeout_ms: timeout_ms,
                remaining_budget_after_planning_ms,
                query_coverage_after_direct: coverage_after_direct.ratio,
                query_coverage_after_mmr: coverage_after_mmr.ratio,
                query_coverage_after_token_budget: coverage_after_token_budget.ratio,
                query_coverage_after_visibility: coverage_after_visibility.ratio,
                uncovered_required_query_segment_ids: coverage_after_visibility
                    .uncovered_required_segment_indices
                    .iter()
                    .map(|index| *index as u32)
                    .collect(),
                query_processing_mode_v008: query_processing_mode_v008(query_plan.mode),
                query_segments_v008,
            }),
            warnings,
        }))
    }

    async fn create_multi_granularity_chunks(
        &self,
        request: Request<pb::CreateMultiGranularityChunksRequest>,
    ) -> Result<Response<pb::CreateMultiGranularityChunksResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let resolved_access_zone = self
            .resolve_ingestion_access_zone(&r.access_zone_id, &r.access_zone_code)
            .await?;
        let access_zone_id = resolved_access_zone.access_zone_id;
        let access_zone_code = resolved_access_zone.access_zone_code.clone();
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be UUID"))?;
        let effective_chunk_ttl_days: Option<i32> = match r.ttl_days {
            Some(0) => {
                if !(self.cfg.index_ttl.allow_never_expire
                    && resolved_access_zone.allow_never_expire)
                {
                    return Err(Status::invalid_argument("ttl_days=0 requires index_ttl.allow_never_expire=true and access zone allow_never_expire=true"));
                }
                None
            }
            Some(v) => {
                if v < self.cfg.index_ttl.min_ttl_days || v > self.cfg.index_ttl.max_ttl_days {
                    return Err(Status::invalid_argument(
                        "ttl_days is outside configured min/max bounds",
                    ));
                }
                Some(v as i32)
            }
            None => {
                let default_ttl = resolved_access_zone.default_ttl_days;
                if default_ttl == 0 {
                    if !(self.cfg.index_ttl.allow_never_expire
                        && resolved_access_zone.allow_never_expire)
                    {
                        return Err(Status::invalid_argument("default ttl_days=0 requires index_ttl.allow_never_expire=true and access zone allow_never_expire=true"));
                    }
                    None
                } else {
                    Some(default_ttl as i32)
                }
            }
        };
        if r.document_version == 0 {
            return Err(Status::invalid_argument(
                "document_version must be greater than zero",
            ));
        }
        if r.source_text.trim().is_empty() {
            return Err(Status::invalid_argument("source_text is required"));
        }
        let from_chunked_finalize = r
            .metadata
            .get("chunked_ingestion_finalize")
            .map(|v| v == "true")
            .unwrap_or(false);
        if !from_chunked_finalize
            && r.source_text.len() > self.cfg.ingestion.single_request_max_bytes
        {
            let message = match self.cfg.ingestion.large_document_mode.as_str() {
                "REQUIRE_CHUNKED" => format!(
                    "source_text exceeds configured single_request_max_bytes={} bytes; use chunked logical ingestion API",
                    self.cfg.ingestion.single_request_max_bytes
                ),
                "ACCEPT_WITH_WARNING" => {
                    tracing::warn!(
                        bytes = r.source_text.len(),
                        limit = self.cfg.ingestion.single_request_max_bytes,
                        "LARGE_DOCUMENT_ACCEPTED_WITH_WARNING is configured, but current single-request path still rejects over-limit payloads"
                    );
                    format!(
                        "source_text exceeds configured single_request_max_bytes={} bytes; ACCEPT_WITH_WARNING is reserved until streaming storage is enabled",
                        self.cfg.ingestion.single_request_max_bytes
                    )
                }
                _ => format!(
                    "source_text exceeds configured single_request_max_bytes={} bytes",
                    self.cfg.ingestion.single_request_max_bytes
                ),
            };
            counter!("ingestion_large_document_rejected_total").increment(1);
            return Err(Status::out_of_range(message));
        }
        if from_chunked_finalize
            && r.source_text.len() > self.cfg.limits.source_text_absolute_max_bytes
        {
            counter!("ingestion_large_document_rejected_total").increment(1);
            return Err(Status::resource_exhausted(format!(
                "chunked source_text exceeds configured source_text_absolute_max_bytes={} bytes",
                self.cfg.limits.source_text_absolute_max_bytes
            )));
        }
        let access_level = pb::AccessLevel::try_from(r.access_level)
            .ok()
            .filter(|v| *v != pb::AccessLevel::Unspecified)
            .ok_or_else(|| Status::invalid_argument("access_level is required"))?;
        let idempotency_key = r.idempotency_key.trim().to_string();
        let profile = Self::default_chunking_profile(r.profile);
        let profile_version = profile.version.clone();
        let source_fingerprint = {
            let mut hasher = Sha256::new();
            hasher.update(r.source_text.as_bytes());
            hasher.update(profile_version.as_bytes());
            hasher.update((access_level as i32).to_string().as_bytes());
            format!("{:x}", hasher.finalize())
        };
        if !idempotency_key.is_empty() {
            if let Some(replay) = self
                .repo()?
                .fetch_v004_chunks_by_idempotency_key(
                    access_zone_id,
                    document_id,
                    r.document_version as i64,
                    &idempotency_key,
                )
                .await
                .map_err(Status::from)?
            {
                if replay.fingerprint != source_fingerprint {
                    return Err(Status::failed_precondition(
                        "idempotency_key reused with different request fingerprint",
                    ));
                }
                if replay.complete {
                    return Ok(Response::new(chunks_response_from_records(
                        replay.chunks,
                        "INDEXING",
                    )));
                }
            }
        }
        let engine = ChunkingEngine::new(ConservativeTokenCounter);
        let annotated_segments = annotated_segments_from_metadata(&r.metadata)?;
        // Every searchable representation is embedded with the same model. A
        // whitespace counter is useful for shaping chunks but cannot enforce the
        // model's BPE limit, so split oversized logical blocks before chunking.
        let annotated_segments = split_annotated_segments_for_model(
            annotated_segments,
            profile
                .parent
                .max
                .min(profile.sub180.max)
                .min(profile.sub260.max)
                .min(self.cfg.tokenization.child.max_length),
            |text, max_length| self.engine.count_tokens(text, max_length, false),
            |text| self.engine.token_offsets(text),
        )?;
        let generated = if annotated_segments.is_empty() {
            engine
                .chunk(
                    access_zone_id,
                    document_id,
                    r.document_version,
                    &r.source_text,
                    &profile,
                    SourceChunkStorageMode::from_config(
                        &self.cfg.chunking.source_chunk_storage_mode,
                    )
                    .map_err(Status::from)?,
                )
                .map_err(Status::from)?
        } else {
            engine
                .chunk_segments(
                    access_zone_id,
                    document_id,
                    r.document_version,
                    &annotated_segments,
                    &profile,
                    SourceChunkStorageMode::from_config(
                        &self.cfg.chunking.source_chunk_storage_mode,
                    )
                    .map_err(Status::from)?,
                )
                .map_err(Status::from)?
        };
        match self.cfg.chunking.source_chunk_storage_mode.as_str() {
            "METADATA_ONLY" => counter!("source_chunk_metadata_only_total").increment(1),
            "DISABLED" => counter!("source_chunk_disabled_total").increment(1),
            _ => counter!("source_chunk_full_text_total").increment(1),
        }
        if generated.len() > self.cfg.ingestion.max_chunks_per_document
            || generated.len() > self.cfg.limits.max_chunks_per_document
        {
            return Err(Status::resource_exhausted(format!(
                "document generated {} chunks, configured max_chunks_per_document is {}",
                generated.len(),
                self.cfg
                    .ingestion
                    .max_chunks_per_document
                    .min(self.cfg.limits.max_chunks_per_document)
            )));
        }
        let mut request_metadata = serde_json::to_value(&r.metadata)
            .map_err(|e| Status::internal(format!("metadata serialization: {e}")))?;
        if let Some(object) = request_metadata.as_object_mut() {
            object.insert(
                "access_zone_code".to_string(),
                serde_json::Value::String(access_zone_code.clone()),
            );
        }
        if let Some(object) = request_metadata.as_object_mut() {
            if !idempotency_key.is_empty() {
                object.insert(
                    "idempotency_key".to_string(),
                    serde_json::Value::String(idempotency_key.clone()),
                );
                object.insert(
                    "idempotency_fingerprint".to_string(),
                    serde_json::Value::String(source_fingerprint.clone()),
                );
            }
        }
        let embedding_mode = pb::EmbeddingModeV005::try_from(r.embedding_mode)
            .unwrap_or(pb::EmbeddingModeV005::DenseSparseRequired);
        let sparse_required = embedding_mode == pb::EmbeddingModeV005::DenseSparseRequired;
        let wants_sparse = matches!(
            embedding_mode,
            pb::EmbeddingModeV005::DenseSparseRequired
                | pb::EmbeddingModeV005::DenseSparseIfAvailable
        );
        let sparse_available = self.engine.sparse_available();
        if sparse_required && !sparse_available {
            counter!("astravector_sparse_unavailable_total").increment(1);
            return Err(Status::failed_precondition(
                "SPARSE_UNAVAILABLE: sparse embedding requested but loaded ONNX artifact has no sparse output",
            ));
        }

        let deadline =
            Instant::now() + Duration::from_millis(self.cfg.grpc.deadlines.document_batch_ms);
        let document_chunks = generated
            .iter()
            .filter(|chunk| chunk.granularity.as_db_str() != "SOURCE")
            .collect::<Vec<_>>();
        if document_chunks.len() > self.cfg.limits.max_embeddings_per_request {
            return Err(Status::resource_exhausted(format!(
                "document requires {} embeddings, configured max_embeddings_per_request is {}",
                document_chunks.len(),
                self.cfg.limits.max_embeddings_per_request
            )));
        }
        let inputs = document_chunks
            .iter()
            .map(|generated_chunk| InferenceInput {
                text: generated_chunk.content.clone(),
                max_length: self.cfg.tokenization.child.max_length,
                allow_truncation: self.cfg.tokenization.child.truncation_allowed,
                want_dense: true,
                want_sparse: wants_sparse && sparse_available,
                token_count_hint: generated_chunk.token_count,
            })
            .collect::<Vec<_>>();
        let embeddings = if self.cfg.embedding.document_submit_mode == "BOUNDED_CONCURRENT" {
            self.scheduler
                .submit_many(
                    QueueKind::Document,
                    inputs,
                    deadline,
                    self.shutdown.child_token(),
                    SubmitManyOptions {
                        max_in_flight: self.cfg.embedding.document_max_in_flight_chunks,
                        preserve_order: self.cfg.embedding.document_preserve_order,
                        cancel_on_error: self.cfg.embedding.cancel_on_error,
                    },
                )
                .await
                .map_err(Status::from)?
        } else {
            let mut out = Vec::with_capacity(inputs.len());
            for input in inputs {
                out.push(
                    self.scheduler
                        .submit(
                            QueueKind::Document,
                            input,
                            deadline,
                            self.shutdown.child_token(),
                        )
                        .await
                        .map_err(Status::from)?,
                );
            }
            out
        };
        let mut prepared_embeddings = Vec::with_capacity(embeddings.len());
        for (generated_chunk, embedding) in document_chunks.into_iter().zip(embeddings) {
            if sparse_required
                && embedding
                    .sparse_indices
                    .as_ref()
                    .map(|v| v.is_empty())
                    .unwrap_or(true)
            {
                counter!("astravector_sparse_unavailable_total").increment(1);
                return Err(Status::failed_precondition(
                    "SPARSE_UNAVAILABLE: document sparse embedding required but produced no sparse indices",
                ));
            }
            prepared_embeddings.push(crate::persistence::PreparedV004IndexEmbedding {
                chunk: generated_chunk.clone(),
                embedding,
            });
        }
        let publish_outbox = r.publish_mode != pb::PublishModeV005::None as i32;
        let summary = self
            .repo()?
            .persist_v004_index_transactionally(
                access_zone_id,
                document_id,
                r.document_version as i64,
                &generated,
                &prepared_embeddings,
                &self.cfg.tokenizer.version,
                &profile_version,
                access_level as i16,
                effective_chunk_ttl_days,
                request_metadata.clone(),
                "v004-control",
                &access_zone_id.to_string(),
                &self.cfg.model.version,
                &self.cfg.dense.name,
                &self.cfg.dense.version,
                &self.cfg.sparse.name,
                &self.cfg.sparse.version,
                self.cfg.sparse.min_weight,
                self.cfg.sparse.max_non_zero as i32,
                &self.cfg.qdrant.collection,
                publish_outbox,
                self.cfg.graph_rag.enabled,
                Some(graph_build_limits_from_config(&self.cfg)),
                self.cfg.graph_rag.build.bulk_insert_batch_size,
                self.cfg
                    .graph_rag
                    .build
                    .failure_mode
                    .eq_ignore_ascii_case("WARN_AND_CONTINUE"),
            )
            .await
            .map_err(Status::from)?;
        let _ = sqlx::query("UPDATE astravector.document_versions SET access_zone_code=$4, updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND delete_operation_id IS NULL")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(r.document_version as i64)
            .bind(&access_zone_code)
            .execute(&self.repo()?.pool)
            .await;
        let _ = sqlx::query("UPDATE astravector.content_chunks_v004 SET access_zone_code=$4, updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(r.document_version as i64)
            .bind(&access_zone_code)
            .execute(&self.repo()?.pool)
            .await;
        let stored = summary.chunks;
        Ok(Response::new(chunks_response_from_records_with_summary(
            stored,
            "INDEXING",
            summary.dense_vectors,
            summary.sparse_vectors,
            summary.bindings,
            summary.outbox_created,
        )))
    }

    async fn get_chunk_group(
        &self,
        request: Request<pb::GetChunkGroupRequest>,
    ) -> Result<Response<pb::GetChunkGroupResponse>, Status> {
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        let root_chunk_id = Uuid::parse_str(r.root_chunk_id.trim())
            .map_err(|_| Status::invalid_argument("root_chunk_id must be a UUID"))?;
        let caller_access_level = pb::AccessLevel::try_from(r.caller_access_level)
            .ok()
            .filter(|v| *v != pb::AccessLevel::Unspecified)
            .unwrap_or(pb::AccessLevel::Restricted);
        let chunks: Vec<_> = self
            .repo()?
            .fetch_chunk_group(access_zone_id, root_chunk_id, caller_access_level as i16)
            .await
            .map_err(Status::from)?
            .into_iter()
            .map(chunk_content_to_pb)
            .collect();
        if chunks.is_empty() {
            return Err(Status::not_found("chunk group not found"));
        }
        Ok(Response::new(pb::GetChunkGroupResponse { chunks }))
    }

    async fn resolve_parent_context(
        &self,
        request: Request<pb::ResolveParentContextRequest>,
    ) -> Result<Response<pb::ResolveParentContextResponse>, Status> {
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        if r.chunk_ids.is_empty() {
            return Err(Status::invalid_argument("chunk_ids must not be empty"));
        }
        let mut ids = Vec::with_capacity(r.chunk_ids.len());
        for id in r.chunk_ids {
            ids.push(
                Uuid::parse_str(id.trim())
                    .map_err(|_| Status::invalid_argument("chunk_ids must be UUIDs"))?,
            );
        }
        let caller_access_level = pb::AccessLevel::try_from(r.caller_access_level)
            .ok()
            .filter(|v| *v != pb::AccessLevel::Unspecified)
            .unwrap_or(pb::AccessLevel::Restricted);
        let contexts: Vec<_> = self
            .repo()?
            .fetch_parent_contexts(access_zone_id, &ids, caller_access_level as i16)
            .await
            .map_err(Status::from)?
            .into_iter()
            .map(parent_context_to_pb)
            .collect();
        if contexts.is_empty() {
            return Err(Status::not_found("parent context not found"));
        }
        Ok(Response::new(pb::ResolveParentContextResponse { contexts }))
    }

    async fn register_document_version(
        &self,
        request: Request<pb::RegisterDocumentVersionRequest>,
    ) -> Result<Response<pb::DocumentVersionResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let resolved_access_zone = self
            .resolve_ingestion_access_zone(&r.access_zone_id, &r.access_zone_code)
            .await?;
        let access_zone_id = resolved_access_zone.access_zone_id;
        let access_zone_code = resolved_access_zone.access_zone_code.clone();
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument(
                "document_version must be greater than zero",
            ));
        }
        let content_hash = r.content_hash.trim().to_ascii_lowercase();
        if content_hash.len() != 64 || !content_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Status::invalid_argument(
                "content_hash must be a 64-character sha256 hex string",
            ));
        }
        let activation_policy = if r.activation_policy.trim().is_empty() {
            "ACTIVE_LATEST_ONLY"
        } else {
            r.activation_policy.trim()
        };
        let record = self
            .repo()?
            .register_document_version(
                access_zone_id,
                document_id,
                r.document_version as i64,
                &content_hash,
                activation_policy,
            )
            .await
            .map_err(Status::from)?;
        let _ = sqlx::query("UPDATE astravector.document_versions SET access_zone_code=$4, updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(r.document_version as i64)
            .bind(&access_zone_code)
            .execute(&self.repo()?.pool)
            .await;
        Ok(Response::new(pb::DocumentVersionResponse {
            document_id: record.document_id.to_string(),
            document_version: record.document_version as u64,
            status: record.status,
        }))
    }

    async fn activate_document_version(
        &self,
        request: Request<pb::ActivateDocumentVersionRequest>,
    ) -> Result<Response<pb::DocumentVersionResponse>, Status> {
        let metadata = request.metadata().clone();
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be a UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument("document_version must be > 0"));
        }
        let record = if r.force_activate {
            self.require_admin(&metadata)?;
            if r.force_reason.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "force_reason is required when force_activate=true",
                ));
            }
            eprintln!("event=FORCE_ACTIVATE_DOCUMENT access_zone_id={} document_id={} document_version={} force_reason={}", access_zone_id, document_id, r.document_version, r.force_reason.replace('\n', " "));
            self.repo()?
                .force_activate_document_version(
                    access_zone_id,
                    document_id,
                    r.document_version as i64,
                    &r.force_reason,
                )
                .await
                .map_err(Status::from)?
        } else {
            self.require_internal_or_admin(&metadata)?;
            let status = self
                .compute_document_sync_status(
                    access_zone_id,
                    document_id,
                    r.document_version as i64,
                    true,
                )
                .await?;
            if !status.ready_to_activate {
                return Err(Status::failed_precondition(format!(
                    "DOCUMENT_NOT_READY_TO_ACTIVATE: expected_bindings={}, synced_bindings={}, outbox_pending={}, outbox_retry_pending={}, outbox_failed={}, qdrant_points_expected={}, qdrant_points_found={}",
                    status.expected_bindings,
                    status.synced_bindings,
                    status.outbox_pending,
                    status.outbox_retry_pending,
                    status.outbox_failed,
                    status.qdrant_points_expected,
                    status.qdrant_points_found
                )));
            }
            self.repo()?
                .activate_document_version(access_zone_id, document_id, r.document_version as i64)
                .await
                .map_err(Status::from)?
        };
        Ok(Response::new(pb::DocumentVersionResponse {
            document_id: record.document_id.to_string(),
            document_version: record.document_version as u64,
            status: record.status,
        }))
    }

    async fn explain_search(
        &self,
        request: Request<pb::ExplainSearchRequest>,
    ) -> Result<Response<pb::ExplainSearchResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let started = std::time::Instant::now();
        let request_timing = request
            .extensions()
            .get::<RequestTiming>()
            .copied()
            .unwrap_or_else(|| RequestTiming::from_request(&request));
        let r = request.into_inner();
        let query = r.query.trim();
        if query.is_empty() {
            return Err(Status::invalid_argument("query must not be empty"));
        }
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        let caller_access_level = pb::AccessLevel::try_from(r.caller_access_level)
            .ok()
            .filter(|v| *v != pb::AccessLevel::Unspecified)
            .ok_or_else(|| Status::invalid_argument("caller_access_level is required"))?;
        let top_k = if r.top_k == 0 { 5 } else { r.top_k }.min(self.cfg.limits.search_top_k_max);
        let candidate_limit = if r.candidate_limit == 0 {
            20
        } else {
            r.candidate_limit
        }
        .min(self.cfg.limits.search_candidate_limit_max);
        let search_mode = Self::resolve_search_mode(r.search_mode, &self.cfg.search.default_mode);
        let version_filters = Self::version_filters_from_explain_request(&r);
        let wants_sparse = matches!(
            search_mode,
            pb::SearchModeV005::Sparse | pb::SearchModeV005::Hybrid
        );
        let wants_dense = matches!(
            search_mode,
            pb::SearchModeV005::Dense | pb::SearchModeV005::Hybrid
        );
        let sparse_required = wants_sparse
            && Self::embedding_mode_requires_sparse(r.embedding_mode, self.cfg.sparse.required);
        if wants_sparse && sparse_required && !self.engine.sparse_available() {
            return Err(Status::failed_precondition("SPARSE_UNAVAILABLE: sparse explain requested but loaded ONNX artifact has no sparse output"));
        }
        let query_counter = EngineQueryTokenCounter {
            engine: self.engine.as_ref(),
        };
        let query_plan = build_query_plan(
            query,
            &query_counter,
            &self.cfg.search.query_processing,
            self.cfg.tokenization.query.max_length,
        )
        .map_err(query_planning_status)?;
        let query_plan_diagnostics = QueryPlanDiagnostics::from_plan(&query_plan);
        let timeout_ms = effective_query_timeout_ms(r.timeout_ms, query_plan.limits.deadline_ms);
        let server_deadline = request_timing.started + Duration::from_millis(timeout_ms);
        let deadline = request_timing
            .transport_deadline
            .map_or(server_deadline, |transport| transport.min(server_deadline));
        if deadline <= Instant::now() {
            return Err(Status::deadline_exceeded(
                "explain deadline exhausted during planning",
            ));
        }
        let timeout_ms = deadline
            .saturating_duration_since(request_timing.started)
            .as_millis() as u64;
        let request_cancel = self.shutdown.child_token();
        let _request_cancel_guard = RequestCancellationGuard(request_cancel.clone());
        let _retrieve_permit = Self::acquire_backpressure_permit(
            self.retrieve_context_semaphore.clone(),
            self.cfg.limits.backpressure_acquire_timeout_ms.min(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis() as u64,
            ),
            "explain_search",
            query_plan.limits.admission_weight,
            &request_cancel,
        )
        .await?;
        let remaining_budget_after_planning_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        if query_plan.mode == QueryProcessingMode::Segmented {
            let qdrant_budget = OperationBudget {
                deadline,
                cancellation: request_cancel.clone(),
                workload: WorkloadKind::Query,
            };
            let mut warnings = Vec::new();
            let direct = self
                .generate_direct_qdrant_hits(
                    &query_plan,
                    &[access_zone_id],
                    caller_access_level,
                    candidate_limit,
                    top_k,
                    search_mode,
                    wants_dense,
                    wants_sparse,
                    self.engine.sparse_available(),
                    sparse_required,
                    &version_filters,
                    deadline,
                    &qdrant_budget,
                    request_cancel.clone(),
                    &mut warnings,
                )
                .await?;
            let fusion = direct
                .hits
                .iter()
                .take(top_k as usize)
                .enumerate()
                .map(|(rank, hit)| pb::ExplainFusionCandidateV005 {
                    rank: (rank + 1) as u32,
                    chunk_id: hit
                        .payload
                        .get("chunk_id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    dense_rank: hit.dense_rank,
                    sparse_rank: hit.sparse_rank,
                    dense_score: hit.dense_score,
                    sparse_score: hit.sparse_score,
                    fusion_score: hit.fusion_score,
                    reason: "SEGMENTED_GLOBAL_RRF".into(),
                })
                .collect::<Vec<_>>();
            let selected_parents = direct
                .hits
                .iter()
                .take(top_k as usize)
                .filter_map(|hit| {
                    let parent = hit
                        .payload
                        .get("parent_chunk_id")
                        .or_else(|| hit.payload.get("chunk_id"))?
                        .as_str()?;
                    Some(pb::ExplainSelectedParentV005 {
                        parent_chunk_id: parent.to_string(),
                        selected_because: "best segmented fused candidate".into(),
                    })
                })
                .collect();
            return Ok(Response::new(pb::ExplainSearchResponse {
                query: query.to_string(),
                query_embedding: None,
                dense_candidates: direct
                    .hits
                    .iter()
                    .filter(|hit| hit.dense_score > 0.0)
                    .take(top_k as usize)
                    .enumerate()
                    .map(|(rank, hit)| explain_candidate(rank, hit))
                    .collect(),
                sparse_candidates: direct
                    .hits
                    .iter()
                    .filter(|hit| hit.sparse_score > 0.0)
                    .take(top_k as usize)
                    .enumerate()
                    .map(|(rank, hit)| explain_candidate(rank, hit))
                    .collect(),
                fusion,
                selected_parents,
                applied_filters: vec![
                    pb::AppliedFilterV005 {
                        key: "access_zone_id".into(),
                        op: "eq".into(),
                        value: access_zone_id.to_string(),
                    },
                    pb::AppliedFilterV005 {
                        key: "access_level".into(),
                        op: "lte".into(),
                        value: (caller_access_level as i32).to_string(),
                    },
                    pb::AppliedFilterV005 {
                        key: "lifecycle_status".into(),
                        op: "eq".into(),
                        value: "ACTIVE".into(),
                    },
                ],
                diagnostics: Some(pb::SearchDiagnosticsV004 {
                    query_embedding_ms: direct.query_embedding_ms,
                    qdrant_search_ms: direct.qdrant_search_ms,
                    total_ms: started.elapsed().as_millis() as u64,
                    candidate_count: direct.hits.len() as u32,
                    parent_group_count: direct.hits.len() as u32,
                    direct_candidates_count: direct.hits.len() as u32,
                    merged_candidates_count: direct.hits.len() as u32,
                    final_candidates_count: direct.hits.len() as u32,
                    dense_branch_executed: direct.dense_branch_executed,
                    sparse_branch_executed: direct.sparse_branch_executed,
                    fusion_executed: direct.fusion_executed,
                    dense_branch_candidate_count: direct.dense_branch_candidate_count,
                    sparse_branch_candidate_count: direct.sparse_branch_candidate_count,
                    fusion_candidate_count: direct.fusion_candidate_count,
                    dense_search_ms: direct.dense_search_ms,
                    sparse_search_ms: direct.sparse_search_ms,
                    fusion_ms: direct.fusion_ms,
                    warning_codes: warnings.into_iter().map(|warning| warning.code).collect(),
                    query_processing_mode: query_plan_diagnostics.mode_code().into(),
                    query_original_token_count: query_plan.original_token_count as u32,
                    query_segment_count: query_plan.segments.len() as u32,
                    query_was_truncated: false,
                    query_coverage_ratio: 0.0,
                    query_required_segments: query_plan
                        .segments
                        .iter()
                        .filter(|segment| segment.required_for_coverage)
                        .count() as u32,
                    query_covered_required_segments: 0,
                    query_segment_sha256: query_plan_diagnostics.segment_sha256,
                    effective_query_timeout_ms: timeout_ms,
                    remaining_budget_after_planning_ms,
                    query_processing_mode_v008: query_processing_mode_v008(query_plan.mode),
                    query_segments_v008: query_segment_diagnostics_from_hits(
                        &query_plan,
                        &direct.hits,
                    ),
                    ..Default::default()
                }),
            }));
        }
        let emb_started = std::time::Instant::now();
        let embedding = self
            .scheduler
            .submit(
                QueueKind::Query,
                InferenceInput {
                    text: query.to_string(),
                    max_length: self.cfg.tokenization.query.max_length,
                    allow_truncation: false,
                    want_dense: wants_dense,
                    want_sparse: wants_sparse && self.engine.sparse_available(),
                    token_count_hint: query_plan.original_token_count,
                },
                deadline,
                request_cancel,
            )
            .await
            .map_err(Status::from)?;
        if embedding.truncated {
            return Err(Status::internal(
                "UNEXPECTED_QUERY_TRUNCATION: explain query embedding was truncated",
            ));
        }
        let query_embedding_ms = emb_started.elapsed().as_millis() as u64;
        let q_started = std::time::Instant::now();
        let qdrant = self.qdrant()?;
        let dense_hits = if wants_dense {
            let dense = embedding
                .dense
                .as_deref()
                .ok_or_else(|| Status::failed_precondition("query dense embedding unavailable"))?;
            qdrant
                .search_dense(
                    dense,
                    &[access_zone_id],
                    caller_access_level as i16,
                    candidate_limit as usize,
                    Some(&version_filters),
                )
                .await
                .map_err(Status::from)?
        } else {
            Vec::new()
        };
        let sparse_hits = if wants_sparse {
            match (
                embedding.sparse_indices.as_deref(),
                embedding.sparse_values.as_deref(),
            ) {
                (Some(indices), Some(values)) if !indices.is_empty() && !values.is_empty() => {
                    qdrant
                        .search_sparse(
                            indices,
                            values,
                            &[access_zone_id],
                            caller_access_level as i16,
                            candidate_limit as usize,
                            Some(&version_filters),
                        )
                        .await
                        .map_err(Status::from)?
                }
                _ if sparse_required => {
                    return Err(Status::failed_precondition(
                        "SPARSE_UNAVAILABLE: query sparse embedding unavailable",
                    ))
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let qdrant_search_ms = q_started.elapsed().as_millis() as u64;
        let fused = fuse_qdrant_hits(
            dense_hits.clone(),
            sparse_hits.clone(),
            candidate_limit as usize,
            &self.cfg.search.hybrid_fusion_method,
            self.cfg.search.hybrid_dense_weight,
            self.cfg.search.hybrid_sparse_weight,
            self.cfg.search.rrf_k,
        );
        let dense_candidates = dense_hits
            .iter()
            .take(top_k as usize)
            .enumerate()
            .map(|(rank, hit)| explain_candidate(rank, hit))
            .collect();
        let sparse_candidates = sparse_hits
            .iter()
            .take(top_k as usize)
            .enumerate()
            .map(|(rank, hit)| explain_candidate(rank, hit))
            .collect();
        let fusion = fused
            .iter()
            .take(top_k as usize)
            .enumerate()
            .map(|(rank, hit)| pb::ExplainFusionCandidateV005 {
                rank: (rank + 1) as u32,
                chunk_id: hit
                    .payload
                    .get("chunk_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                dense_rank: hit.dense_rank,
                sparse_rank: hit.sparse_rank,
                dense_score: hit.dense_score,
                sparse_score: hit.sparse_score,
                fusion_score: hit.fusion_score,
                reason: if hit.dense_rank.is_some() && hit.sparse_rank.is_some() {
                    "MATCHED_BY_DENSE_AND_SPARSE"
                } else if hit.dense_rank.is_some() {
                    "MATCHED_BY_DENSE_ONLY"
                } else {
                    "MATCHED_BY_SPARSE_ONLY"
                }
                .into(),
            })
            .collect();
        let selected_parents = fused
            .iter()
            .take(top_k as usize)
            .filter_map(|hit| {
                let parent = hit
                    .payload
                    .get("parent_chunk_id")
                    .or_else(|| hit.payload.get("chunk_id"))?
                    .as_str()?;
                Some(pb::ExplainSelectedParentV005 {
                    parent_chunk_id: parent.to_string(),
                    selected_because: "best fused candidate".into(),
                })
            })
            .collect();
        let top_sparse_tokens = match (&embedding.sparse_indices, &embedding.sparse_values) {
            (Some(indices), Some(values)) => {
                let mut pairs: Vec<(u32, f32)> = indices
                    .iter()
                    .copied()
                    .zip(values.iter().copied())
                    .collect();
                pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                pairs.truncate(self.cfg.explain.top_sparse_tokens as usize);
                pairs
                    .into_iter()
                    .map(|(token_id, weight)| pb::SparseTokenPreviewV005 { token_id, weight })
                    .collect()
            }
            _ => Vec::new(),
        };
        Ok(Response::new(pb::ExplainSearchResponse {
            query: query.to_string(),
            query_embedding: Some(pb::QueryEmbeddingSummaryV005 {
                dense_dimension: embedding.dense.as_ref().map(|v| v.len()).unwrap_or(0) as u32,
                sparse_non_zero_count: embedding
                    .sparse_indices
                    .as_ref()
                    .map(|v| v.len())
                    .unwrap_or(0) as u32,
                top_sparse_tokens,
            }),
            dense_candidates,
            sparse_candidates,
            fusion,
            selected_parents,
            applied_filters: vec![
                pb::AppliedFilterV005 {
                    key: "access_zone_id".into(),
                    op: "eq".into(),
                    value: access_zone_id.to_string(),
                },
                pb::AppliedFilterV005 {
                    key: "access_level".into(),
                    op: "lte".into(),
                    value: (caller_access_level as i32).to_string(),
                },
                pb::AppliedFilterV005 {
                    key: "lifecycle_status".into(),
                    op: "eq".into(),
                    value: "ACTIVE".into(),
                },
            ],
            diagnostics: Some(pb::SearchDiagnosticsV004 {
                query_embedding_ms,
                qdrant_search_ms,
                parent_fetch_ms: 0,
                total_ms: started.elapsed().as_millis() as u64,
                candidate_count: fused.len() as u32,
                parent_group_count: fused.len() as u32,
                direct_candidates_count: fused.len() as u32,
                graph_candidates_count: 0,
                merged_candidates_count: fused.len() as u32,
                final_candidates_count: fused.len() as u32,
                deduplicated_candidates_count: 0,
                graph_candidates_by_relation_json: "{}".into(),
                graph_expansion_duration_ms: 0,
                graph_merge_duration_ms: 0,
                graph_merge_strategy: "N/A".into(),
                final_context_limit: fused.len() as u32,
                final_context_limit_mode: "N/A".into(),
                graph_min_score: 0.0,
                semantic_power: 1.0,
                mmr_enabled: false,
                mmr_lambda: 0.75,
                mmr_candidate_count: 0,
                mmr_selected_count: 0,
                mmr_duration_ms: 0,
                mmr_similarity_source: "N/A".into(),
                learned_reranker_enabled: false,
                learned_reranker_provider: "NONE".into(),
                direct_context_limit: 0,
                graph_context_append_limit: 0,
                direct_score_weight: 1.0,
                graph_score_weight: 1.0,
                graph_score_bias: 0.0,
                score_normalization: "NONE".into(),
                mmr_lambda_direct: 0.80,
                mmr_lambda_graph: 0.60,
                mmr_embedding_fetch_requested: 0,
                mmr_embedding_fetch_found: 0,
                mmr_embedding_fetch_missing: 0,
                mmr_embedding_fetch_duration_ms: 0,
                mmr_embedding_cache_hits: 0,
                mmr_embedding_cache_misses: 0,
                mmr_embedding_fetch_errors: 0,
                mmr_embedding_fetch_timeouts: 0,
                mmr_embedding_fetch_skipped_all_present: false,
                mmr_embedding_fetch_skipped_small_pool: false,
                mmr_dense_pair_comparisons: 0,
                mmr_token_pair_comparisons: 0,
                mmr_effective_similarity_source: "N/A".into(),
                warning_codes: Vec::new(),
                token_budget_enabled: self.cfg.rag_context.token_budget_enabled,
                max_context_tokens: self.cfg.rag_context.max_context_tokens as u32,
                estimated_context_tokens_before: 0,
                estimated_context_tokens_after: 0,
                context_chunks_dropped_by_token_budget: 0,
                token_truncation_strategy: self.cfg.rag_context.truncation_strategy.clone(),
                dropped_chunk_ids: Vec::new(),
                huge_chunk_strategy: self.cfg.rag_context.huge_chunk_strategy.clone(),
                dense_branch_executed: wants_dense,
                sparse_branch_executed: wants_sparse,
                fusion_executed: wants_dense && wants_sparse,
                dense_branch_candidate_count: dense_hits.len() as u32,
                sparse_branch_candidate_count: sparse_hits.len() as u32,
                fusion_candidate_count: fused.len() as u32,
                query_processing_mode: query_plan_diagnostics.mode_code().into(),
                query_original_token_count: query_plan.original_token_count as u32,
                query_segment_count: query_plan.segments.len() as u32,
                query_was_truncated: false,
                query_coverage_ratio: if fused.is_empty() { 0.0 } else { 1.0 },
                query_required_segments: 1,
                query_covered_required_segments: u32::from(!fused.is_empty()),
                query_segment_sha256: query_plan_diagnostics.segment_sha256,
                effective_query_timeout_ms: timeout_ms,
                remaining_budget_after_planning_ms,
                query_processing_mode_v008: query_processing_mode_v008(query_plan.mode),
                query_segments_v008: query_segment_diagnostics_from_hits(&query_plan, &fused),
                ..Default::default()
            }),
        }))
    }

    async fn debug_document_state(
        &self,
        request: Request<pb::DebugDocumentStateRequest>,
    ) -> Result<Response<pb::DebugDocumentStateResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be a UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument("document_version must be > 0"));
        }
        let repo = self.repo()?;
        let doc = sqlx::query("SELECT status, content_hash FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id).bind(document_id).bind(r.document_version as i64)
            .fetch_optional(&repo.pool).await.map_err(|e| Status::unavailable(format!("postgres: {e}")))?;
        let (status, content_hash) = doc
            .map(|row| {
                (
                    row.get::<String, _>("status"),
                    row.get::<String, _>("content_hash"),
                )
            })
            .unwrap_or_else(|| ("NOT_FOUND".into(), String::new()));
        let chunks_rows = if r.include_chunks {
            sqlx::query("SELECT c.id,c.parent_chunk_id,c.root_chunk_id,c.granularity,c.actual_token_count,c.lifecycle_status,c.source_block_id,c.source_location,c.source_links,COALESCE((SELECT m.relation_type FROM astravector.logical_block_chunk_mapping m WHERE m.access_zone_id=c.access_zone_id AND m.document_id=c.document_id AND m.document_version=c.document_version AND m.chunk_id=c.id LIMIT 1),'') AS trace_relation_type,CASE WHEN c.source_block_id IS NULL THEN 'MISSING' WHEN (SELECT count(*) FROM astravector.logical_block_chunk_mapping m WHERE m.access_zone_id=c.access_zone_id AND m.document_id=c.document_id AND m.document_version=c.document_version AND m.chunk_id=c.id) > 1 THEN 'MERGED' ELSE 'EXACT' END AS trace_quality,COALESCE(array_agg(b.qdrant_point_id::text) FILTER (WHERE b.qdrant_point_id IS NOT NULL),'{}') AS qdrant_point_ids FROM astravector.content_chunks_v004 c LEFT JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=c.access_zone_id AND b.chunk_id=c.id WHERE c.access_zone_id=$1 AND c.document_id=$2 AND c.document_version=$3 GROUP BY c.id,c.parent_chunk_id,c.root_chunk_id,c.granularity,c.actual_token_count,c.lifecycle_status,c.source_block_id,c.source_location,c.source_links,c.metadata ORDER BY c.created_at")
                .bind(access_zone_id).bind(document_id).bind(r.document_version as i64)
                .fetch_all(&repo.pool).await.map_err(|e| Status::unavailable(format!("postgres: {e}")))?
        } else {
            Vec::new()
        };
        let chunks: Vec<pb::DebugChunkInfoV005> = chunks_rows
            .into_iter()
            .map(|row| pb::DebugChunkInfoV005 {
                chunk_id: row.get::<Uuid, _>("id").to_string(),
                parent_chunk_id: row
                    .try_get::<Option<Uuid>, _>("parent_chunk_id")
                    .ok()
                    .flatten()
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                root_chunk_id: row.get::<Uuid, _>("root_chunk_id").to_string(),
                granularity: granularity_from_str(&row.get::<String, _>("granularity")),
                actual_token_count: row.get::<i32, _>("actual_token_count") as u32,
                lifecycle_status: row.get::<String, _>("lifecycle_status"),
                source_block_id: row
                    .try_get::<Option<String>, _>("source_block_id")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                trace_relation_type: row
                    .try_get::<String, _>("trace_relation_type")
                    .unwrap_or_default(),
                trace_quality: row
                    .try_get::<String, _>("trace_quality")
                    .unwrap_or_default(),
                source_location_json: row
                    .try_get::<serde_json::Value, _>("source_location")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                source_links_json: row
                    .try_get::<serde_json::Value, _>("source_links")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                qdrant_point_ids: row
                    .try_get::<Vec<String>, _>("qdrant_point_ids")
                    .unwrap_or_default(),
            })
            .collect();
        let counts = sqlx::query(r#"SELECT
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3) AS bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_dense d ON d.cache_entry_id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3) AS dense_count,
          (SELECT count(*) FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_sparse s ON s.cache_entry_id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3) AS sparse_count,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='PENDING') AS outbox_pending,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='RETRY_PENDING') AS outbox_retry_pending,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='COMPLETED') AS outbox_completed,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status IN('FAILED','DEAD_LETTER')) AS outbox_failed
        "#)
            .bind(access_zone_id).bind(document_id).bind(r.document_version as i64)
            .fetch_one(&repo.pool).await.map_err(|e| Status::unavailable(format!("postgres: {e}")))?;
        let bindings_count: i64 = counts.get("bindings");
        let mut qdrant_collection_exists = self.qdrant.is_some();
        let mut qdrant_point_ids = std::collections::HashSet::new();
        let mut scroll_status = String::from("NOT_REQUESTED");
        let mut scroll_pages_read = 0_u32;
        let mut scroll_points_read = 0_u32;
        if r.include_qdrant {
            if let Some(q) = self.qdrant.as_ref() {
                qdrant_collection_exists = q.collection_exists().await.map_err(Status::from)?;
                if qdrant_collection_exists {
                    let scroll = q
                        .point_ids_by_document_paginated(
                            access_zone_id,
                            document_id,
                            r.document_version as i64,
                        )
                        .await
                        .map_err(Status::from)?;
                    scroll_status = format!("{:?}", scroll.status);
                    scroll_pages_read = scroll.pages_read as u32;
                    scroll_points_read = scroll.points_read as u32;
                    qdrant_point_ids = scroll.point_ids;
                } else {
                    scroll_status = String::from("COLLECTION_MISSING");
                }
            } else {
                qdrant_collection_exists = false;
                scroll_status = String::from("QDRANT_DISABLED");
            }
        }
        let binding_rows = sqlx::query("SELECT b.id,b.chunk_id,b.qdrant_point_id,b.qdrant_sync_status, COALESCE((SELECT o.status FROM astravector.vector_outbox o WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND o.operation='UPSERT_POINT' ORDER BY o.updated_at DESC LIMIT 1),'') AS outbox_status FROM astravector.vector_bindings_v004 b WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3")
            .bind(access_zone_id).bind(document_id).bind(r.document_version as i64)
            .fetch_all(&repo.pool).await.map_err(|e| Status::unavailable(format!("postgres qdrant debug: {e}")))?;
        let mut points_missing = Vec::new();
        for row in &binding_rows {
            let point_id: Uuid = row.get("qdrant_point_id");
            if r.include_qdrant
                && (!qdrant_collection_exists || !qdrant_point_ids.contains(&point_id))
            {
                let sync_status: String = row.get("qdrant_sync_status");
                let outbox_status: String = row.get("outbox_status");
                let reason = if !qdrant_collection_exists {
                    "QDRANT_COLLECTION_MISSING"
                } else if sync_status != "SYNCED" {
                    "BINDING_NOT_SYNCED"
                } else if outbox_status != "COMPLETED" {
                    "OUTBOX_NOT_COMPLETED"
                } else {
                    "QDRANT_POINT_NOT_FOUND"
                };
                points_missing.push(pb::MissingQdrantPointV005 {
                    chunk_id: row.get::<Uuid, _>("chunk_id").to_string(),
                    binding_id: row.get::<Uuid, _>("id").to_string(),
                    qdrant_point_id: point_id.to_string(),
                    reason: reason.to_string(),
                });
            }
        }
        let qdrant_info = pb::DebugQdrantInfoV005 {
            collection: self.cfg.qdrant.collection.clone(),
            collection_exists: qdrant_collection_exists,
            points_expected: bindings_count as u32,
            points_found: if r.include_qdrant {
                qdrant_point_ids.len() as u32
            } else {
                bindings_count as u32
            },
            points_missing,
            scroll_status,
            scroll_pages_read,
            scroll_points_read,
        };
        let trace_counts = sqlx::query(r#"SELECT
          (SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND source_block_id IS NOT NULL) AS traced_chunks,
          (SELECT count(*) FROM astravector.logical_block_chunk_mapping WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3) AS mapping_rows
        "#)
            .bind(access_zone_id)
            .bind(document_id)
            .bind(r.document_version as i64)
            .fetch_one(&repo.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres trace debug: {e}")))?;
        let traced_chunks: i64 = trace_counts.get("traced_chunks");
        let mapping_rows: i64 = trace_counts.get("mapping_rows");
        let mut debug_warnings = Vec::new();
        if r.include_vectors {
            debug_warnings.push(pb::DiagnosticWarningV005 {
                code: "INCLUDE_VECTORS_IGNORED".into(),
                message: "include_vectors is ignored for debug/explain responses; dense embeddings are internal-only and are not returned.".into(),
            });
        }
        debug_warnings.push(pb::DiagnosticWarningV005 {
            code: "LOGICAL_BLOCK_TRACE_SUMMARY".into(),
            message: format!("Debug trace: traced_chunks={traced_chunks}, logical_block_chunk_mapping_rows={mapping_rows}, bindings={bindings_count}"),
        });
        if bindings_count > 0 && traced_chunks == 0 {
            debug_warnings.push(pb::DiagnosticWarningV005 {
                code: "SOURCE_TRACE_MISSING".into(),
                message: "No chunk source trace found for this document; legacy or pre-fix2 indexing path may have been used".into(),
            });
        }
        let graph_summary = match repo
            .fetch_graph_summary(access_zone_id, document_id, r.document_version as i64)
            .await
        {
            Ok(g) => Some(pb::DebugGraphSummaryV005 {
                total_graph_nodes: g.total_nodes,
                total_graph_edges: g.total_edges,
                nodes_by_type_json: g.nodes_by_type.to_string(),
                edges_by_relation_type_json: g.edges_by_relation_type.to_string(),
                graph_partitions_status: "PARTITIONED_BY_NODE_TYPE_AND_RELATION_TYPE".into(),
                semantic_edges_count: g.semantic_edges_count,
                semantic_avg_weight: g.semantic_avg_weight.unwrap_or_default(),
                semantic_min_weight: g.semantic_min_weight.unwrap_or_default(),
                semantic_max_weight: g.semantic_max_weight.unwrap_or_default(),
                allowed_relations_json: serde_json::to_string(
                    &self.cfg.graph_rag.retrieval.allowed_relations,
                )
                .unwrap_or_default(),
                relation_weights_json: serde_json::to_string(
                    &self.cfg.graph_rag.scoring.relation_weights,
                )
                .unwrap_or_default(),
            }),
            Err(e) => {
                debug_warnings.push(pb::DiagnosticWarningV005 {
                    code: "GRAPH_DEBUG_UNAVAILABLE".into(),
                    message: format!("Graph summary unavailable: {e}"),
                });
                None
            }
        };
        let sync = self
            .compute_document_sync_status(
                access_zone_id,
                document_id,
                r.document_version as i64,
                r.include_qdrant,
            )
            .await?;
        let ready = sync.ready_to_activate;
        Ok(Response::new(pb::DebugDocumentStateResponse {
            document: Some(pb::DebugDocumentInfoV005 {
                status,
                content_hash,
                model_version: self.cfg.model.version.clone(),
                tokenizer_version: self.cfg.tokenizer.version.clone(),
                chunking_version: "unknown".into(),
            }),
            chunks,
            vectors: Some(pb::DebugVectorInfoV005 {
                dense_count: counts.get::<i64, _>("dense_count") as u32,
                sparse_count: counts.get::<i64, _>("sparse_count") as u32,
                bindings_count: bindings_count as u32,
            }),
            outbox: Some(pb::DebugOutboxInfoV005 {
                pending: counts.get::<i64, _>("outbox_pending") as u32,
                retry_pending: counts.get::<i64, _>("outbox_retry_pending") as u32,
                completed: counts.get::<i64, _>("outbox_completed") as u32,
                failed: counts.get::<i64, _>("outbox_failed") as u32,
            }),
            qdrant: Some(qdrant_info),
            ready_to_activate: ready,
            warnings: debug_warnings,
            graph: graph_summary,
        }))
    }

    async fn retry_vector_outbox(
        &self,
        request: Request<pb::RetryVectorOutboxRequest>,
    ) -> Result<Response<pb::RetryVectorOutboxResponse>, Status> {
        self.require_admin(request.metadata())?;
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be a UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument("document_version must be > 0"));
        }
        let operation = r.operation.as_deref().unwrap_or("UPSERT_POINT");
        let status_filter = r.status.as_deref();
        let sql = if status_filter.is_some() {
            "UPDATE astravector.vector_outbox o SET status='PENDING', next_attempt_at=now(), locked_by=NULL, locked_until=NULL, error_code=NULL, error_message=NULL, updated_at=now() FROM astravector.vector_bindings_v004 b WHERE b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id AND b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.operation=$4 AND o.status=$5 RETURNING o.id"
        } else {
            "UPDATE astravector.vector_outbox o SET status='PENDING', next_attempt_at=now(), locked_by=NULL, locked_until=NULL, error_code=NULL, error_message=NULL, updated_at=now() FROM astravector.vector_bindings_v004 b WHERE b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id AND b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.operation=$4 AND o.status IN('FAILED','RETRY_PENDING','DEAD_LETTER') RETURNING o.id"
        };
        let mut q = sqlx::query(sql)
            .bind(access_zone_id)
            .bind(document_id)
            .bind(r.document_version as i64)
            .bind(operation);
        if let Some(status) = status_filter {
            q = q.bind(status);
        }
        let rows = q
            .fetch_all(&self.repo()?.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres: {e}")))?;
        let ids: Vec<String> = rows
            .iter()
            .map(|row| row.get::<Uuid, _>("id").to_string())
            .collect();
        Ok(Response::new(pb::RetryVectorOutboxResponse {
            matched: ids.len() as u32,
            reset_to_pending: ids.len() as u32,
            affected_outbox_ids: ids,
        }))
    }
}

#[tonic::async_trait]
impl AstraVectorIngestionFacade for AstraVectorV004ControlService {
    async fn index_logical_document(
        &self,
        request: Request<pb::IndexLogicalDocumentRequest>,
    ) -> Result<Response<pb::IndexLogicalDocumentResponse>, Status> {
        let metadata = request.metadata().clone();
        self.require_internal_or_admin(&metadata)?;
        let r = request.into_inner();
        let ctx = r.context.unwrap_or_default();
        let resolved_access_zone = self
            .resolve_ingestion_access_zone(&r.access_zone_id, &r.access_zone_code)
            .await?;
        let access_zone_id = resolved_access_zone.access_zone_id;
        let access_zone_code = resolved_access_zone.access_zone_code.clone();
        let document = r
            .document
            .ok_or_else(|| Status::invalid_argument("document is required"))?;
        let document_id = resolve_facade_document_id(&document)?;
        validate_source_links("document", &document.source_links)?;
        let document_version = if document.document_version == 0 {
            1
        } else {
            document.document_version
        };
        let logical_blocks = validate_and_sort_logical_blocks(r.blocks)?;
        let source_text = render_logical_blocks_for_chunking(&logical_blocks);
        if source_text.trim().is_empty() {
            return Err(Status::invalid_argument("logical blocks contain no text"));
        }
        let from_chunked_finalize = ctx.caller_service == "chunked-ingestion";
        if !from_chunked_finalize && source_text.len() > self.cfg.ingestion.single_request_max_bytes
        {
            counter!("ingestion_large_document_rejected_total").increment(1);
            return Err(Status::out_of_range(format!(
                "logical document text exceeds configured single_request_max_bytes={} bytes; use chunked logical ingestion API",
                self.cfg.ingestion.single_request_max_bytes
            )));
        }
        if from_chunked_finalize
            && source_text.len() > self.cfg.limits.source_text_absolute_max_bytes
        {
            counter!("ingestion_large_document_rejected_total").increment(1);
            return Err(Status::resource_exhausted(format!(
                "chunked logical document text exceeds configured source_text_absolute_max_bytes={} bytes",
                self.cfg.limits.source_text_absolute_max_bytes
            )));
        }
        let content_hash = normalized_content_hash(&document.content_hash, &source_text)?;
        let indexing_options = r.indexing_options.clone().unwrap_or_default();
        reject_unsupported_activation_policy(indexing_options.activation_policy)?;
        let activation_policy = activation_policy_as_str(indexing_options.activation_policy);
        let _ = self
            .repo()?
            .register_document_version(
                access_zone_id,
                document_id,
                document_version as i64,
                &content_hash,
                activation_policy,
            )
            .await
            .map_err(Status::from)?;

        let mut merged_metadata = r.metadata.clone();
        attach_document_metadata(&mut merged_metadata, &document);
        attach_logical_block_metadata(&mut merged_metadata, &logical_blocks)?;
        merged_metadata.insert("ingestion_facade".into(), "IndexLogicalDocument".into());
        if from_chunked_finalize {
            merged_metadata.insert("chunked_ingestion_finalize".into(), "true".into());
        }
        merged_metadata.insert(
            "logical_blocks_count".into(),
            logical_blocks.len().to_string(),
        );
        merged_metadata.insert("content_hash".into(), content_hash.clone());
        merged_metadata.insert("access_zone_code".into(), access_zone_code.clone());

        let requested_ttl_days = ttl_days_from_policy(indexing_options.ttl_policy.as_ref())?;
        let effective_ttl_days =
            requested_ttl_days.unwrap_or(resolved_access_zone.default_ttl_days);
        if effective_ttl_days == 0
            && !(self.cfg.index_ttl.allow_never_expire && resolved_access_zone.allow_never_expire)
        {
            return Err(Status::invalid_argument("ttl_days=0 requires index_ttl.allow_never_expire=true and access zone allow_never_expire=true"));
        }
        if effective_ttl_days != 0
            && (effective_ttl_days < self.cfg.index_ttl.min_ttl_days
                || effective_ttl_days > self.cfg.index_ttl.max_ttl_days)
        {
            return Err(Status::invalid_argument(
                "ttl_days is outside configured min/max bounds",
            ));
        }
        let chunk_ttl_days = if effective_ttl_days == 0 {
            None
        } else {
            Some(effective_ttl_days)
        };
        let idempotency_key = if !ctx.idempotency_key.trim().is_empty() {
            ctx.idempotency_key.clone()
        } else {
            format!("v007-index-logical:{access_zone_id}:{document_id}:{document_version}")
        };
        let mut inner = Request::new(pb::CreateMultiGranularityChunksRequest {
            access_zone_id: access_zone_id.to_string(),
            document_id: document_id.to_string(),
            document_version,
            source_text,
            access_level: normalized_access_level(ctx.caller_access_level) as i32,
            ttl_days: chunk_ttl_days,
            profile: Some(chunking_profile_v004_from_v007(r.chunking_options.as_ref())),
            metadata: merged_metadata,
            idempotency_key,
            correlation_id: ctx.correlation_id.clone(),
            embedding_mode: normalized_embedding_mode(indexing_options.embedding_mode) as i32,
            publish_mode: normalized_publish_mode(indexing_options.publish_mode) as i32,
            access_zone_code: access_zone_code.clone(),
        });
        *inner.metadata_mut() = metadata;
        let created =
            match <Self as AstraVectorV004Control>::create_multi_granularity_chunks(self, inner)
                .await
            {
                Ok(response) => response.into_inner(),
                Err(status) => {
                    // Registration precedes indexing for concurrency fencing. If the
                    // pre-persistence path fails, leave an explicit retryable FAILED
                    // state instead of a silent REGISTERED document without chunks.
                    if let Err(error) = self
                        .repo()?
                        .mark_registered_document_version_failed(
                            access_zone_id,
                            document_id,
                            document_version as i64,
                            &content_hash,
                            status.code().to_string().as_str(),
                            status.message(),
                        )
                        .await
                    {
                        tracing::error!(
                            access_zone_id = %access_zone_id,
                            document_id = %document_id,
                            document_version,
                            error = %error,
                            "INGESTION_FAILURE_STATE_RECORD_FAILED"
                        );
                    }
                    return Err(status);
                }
            };
        let summary = created.summary.unwrap_or_default();
        // fix4.5.3: document-version TTL/lifecycle metadata is the PostgreSQL source of truth.
        let _ = sqlx::query("UPDATE astravector.document_versions SET indexed_at=COALESCE(indexed_at, now()), ttl_days=$4, expires_at=CASE WHEN $4=0 THEN NULL ELSE COALESCE(indexed_at, now()) + ($4 * interval '1 day') END, lifecycle_status=CASE WHEN status='ACTIVE' THEN 'ACTIVE' ELSE lifecycle_status END, access_zone_code=$5, updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND delete_operation_id IS NULL")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version as i64)
            .bind(effective_ttl_days as i32)
            .bind(&access_zone_code)
            .execute(&self.repo()?.pool)
            .await;
        let state = match pb::ActivationPolicy::try_from(indexing_options.activation_policy)
            .unwrap_or(pb::ActivationPolicy::Manual)
        {
            pb::ActivationPolicy::Skip => pb::OperationState::Indexing,
            pb::ActivationPolicy::AutoWhenReady => pb::OperationState::Publishing,
            _ => pb::OperationState::Indexing,
        };
        Ok(Response::new(pb::IndexLogicalDocumentResponse {
            document: Some(pb::DocumentRef {
                access_zone_id: access_zone_id.to_string(),
                document_id: document_id.to_string(),
                document_version,
            }),
            operation: Some(pb::OperationStatus {
                operation_id: ctx.correlation_id,
                state: state as i32,
                message:
                    "Logical blocks accepted; tokenizer-aware chunks and vector outbox were created"
                        .into(),
                warnings: Vec::new(),
                errors: Vec::new(),
            }),
            summary: Some(pb::IndexingSummary {
                blocks_received: logical_blocks.len() as u32,
                blocks_accepted: logical_blocks.len() as u32,
                blocks_rejected: 0,
                chunks_created: summary.chunks_total,
                parent_chunks_created: summary.parent_chunks,
                child_chunks_created: summary.sub180_chunks + summary.sub260_chunks,
                atomic_chunks_created: 0,
                dense_vectors_created: summary.dense_vectors,
                sparse_vectors_created: summary.sparse_vectors,
                qdrant_points_scheduled: summary.outbox_created,
                already_indexed: false,
            }),
        }))
    }

    async fn start_logical_document_ingestion(
        &self,
        request: Request<pb::StartLogicalDocumentIngestionRequest>,
    ) -> Result<Response<pb::StartLogicalDocumentIngestionResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        if !self.cfg.ingestion.chunked_ingestion_enabled {
            return Err(Status::failed_precondition("chunked ingestion is disabled"));
        }
        let r = request.into_inner();
        let resolved_access_zone = self
            .resolve_ingestion_access_zone(&r.access_zone_id, &r.access_zone_code)
            .await?;
        let access_zone_id = resolved_access_zone.access_zone_id;
        let access_zone_code = resolved_access_zone.access_zone_code.clone();
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument("document_version must be > 0"));
        }
        let normalized_ttl_days = if r.ttl_days == 0 {
            resolved_access_zone.default_ttl_days
        } else {
            r.ttl_days
        };
        if normalized_ttl_days == 0
            && !(self.cfg.index_ttl.allow_never_expire && resolved_access_zone.allow_never_expire)
        {
            return Err(Status::invalid_argument("ttl_days=0 requires index_ttl.allow_never_expire=true and access zone allow_never_expire=true"));
        }
        if normalized_ttl_days != 0
            && (normalized_ttl_days < self.cfg.index_ttl.min_ttl_days
                || normalized_ttl_days > self.cfg.index_ttl.max_ttl_days)
        {
            return Err(Status::invalid_argument(
                "ttl_days is outside configured min/max bounds",
            ));
        }
        if r.total_bytes_estimate as usize > self.cfg.limits.source_text_absolute_max_bytes {
            return Err(Status::resource_exhausted(
                "estimated document size exceeds configured absolute maximum",
            ));
        }
        if r.total_blocks_estimate as usize > self.cfg.ingestion.max_blocks_per_document {
            return Err(Status::resource_exhausted(
                "estimated block count exceeds max_blocks_per_document",
            ));
        }
        let repo = self.repo()?;
        let idempotency_key = if r.idempotency_key.trim().is_empty() {
            format!(
                "v007-chunked:{}:{}:{}",
                access_zone_id, document_id, r.document_version
            )
        } else {
            r.idempotency_key.trim().to_owned()
        };
        let request_fingerprint =
            start_ingestion_fingerprint(&r, access_zone_id, document_id, &idempotency_key)?;
        let mut tx =
            repo.pool.begin().await.map_err(|e| {
                Status::unavailable(format!("postgres ingestion start tx begin: {e}"))
            })?;
        // Serialize start-session limit checks in the same transaction as INSERT.
        // The global lock protects max_concurrent_ingestion_sessions and per-zone limits;
        // the document lock protects max_sessions_per_document and idempotency for one document.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind("ingestion-start:global")
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                Status::unavailable(format!("postgres ingestion global advisory lock: {e}"))
            })?;
        let advisory_key = format!(
            "ingestion-start:{access_zone_id}:{document_id}:{}",
            r.document_version
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&advisory_key)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                Status::unavailable(format!("postgres ingestion document advisory lock: {e}"))
            })?;
        counter!("ingestion_start_advisory_lock_acquired_total").increment(1);

        if let Some(existing) = sqlx::query("SELECT ingestion_session_id, status, expires_at, request_fingerprint FROM astravector.ingestion_sessions_v004 WHERE access_zone_id=$1 AND idempotency_key=$2")
            .bind(access_zone_id).bind(&idempotency_key)
            .fetch_optional(&mut *tx).await.map_err(|e| Status::unavailable(format!("postgres ingestion start lookup: {e}")))? {
            let existing_fp: Option<String> = existing.try_get("request_fingerprint").ok();
            if existing_fp.as_deref() != Some(request_fingerprint.as_str()) {
                counter!("ingestion_start_idempotency_conflict_total").increment(1);
                return Err(Status::failed_precondition("IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_REQUEST"));
            }
            let response = pb::StartLogicalDocumentIngestionResponse {
                ingestion_session_id: existing.get::<Uuid,_>("ingestion_session_id").to_string(),
                status: existing.get::<String,_>("status"),
                expires_at: existing.get::<chrono::DateTime<chrono::Utc>,_>("expires_at").to_rfc3339(),
                warnings: Vec::new(),
            };
            tx.commit().await.map_err(|e| Status::unavailable(format!("postgres ingestion start replay commit: {e}")))?;
            counter!("ingestion_start_idempotent_replay_total").increment(1);
            return Ok(Response::new(response));
        }
        let active_global: i64 = sqlx::query_scalar("SELECT count(*) FROM astravector.ingestion_sessions_v004 WHERE status IN ('ACTIVE','FINALIZING')")
            .fetch_one(&mut *tx).await.map_err(|e| Status::unavailable(format!("postgres ingestion active count: {e}")))?;
        if active_global as usize >= self.cfg.ingestion.max_concurrent_ingestion_sessions {
            counter!("ingestion_limit_rejected_total", "limit" => "max_concurrent_ingestion_sessions").increment(1);
            counter!("ingestion_start_limit_rejected_total", "limit" => "max_concurrent_ingestion_sessions").increment(1);
            return Err(Status::resource_exhausted(
                "max_concurrent_ingestion_sessions exceeded",
            ));
        }
        let active_zone: i64 = sqlx::query_scalar("SELECT count(*) FROM astravector.ingestion_sessions_v004 WHERE access_zone_id=$1 AND status IN ('ACTIVE','FINALIZING')")
            .bind(access_zone_id).fetch_one(&mut *tx).await.map_err(|e| Status::unavailable(format!("postgres ingestion zone count: {e}")))?;
        if active_zone as usize >= self.cfg.ingestion.max_sessions_per_access_zone {
            counter!("ingestion_limit_rejected_total", "limit" => "max_sessions_per_access_zone")
                .increment(1);
            counter!("ingestion_start_limit_rejected_total", "limit" => "max_sessions_per_access_zone").increment(1);
            return Err(Status::resource_exhausted(
                "max_sessions_per_access_zone exceeded",
            ));
        }
        let active_doc: i64 = sqlx::query_scalar("SELECT count(*) FROM astravector.ingestion_sessions_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND status IN ('ACTIVE','FINALIZING')")
            .bind(access_zone_id).bind(document_id).bind(r.document_version as i64)
            .fetch_one(&mut *tx).await.map_err(|e| Status::unavailable(format!("postgres ingestion document count: {e}")))?;
        if active_doc as usize >= self.cfg.ingestion.max_sessions_per_document {
            counter!("ingestion_limit_rejected_total", "limit" => "max_sessions_per_document")
                .increment(1);
            counter!("ingestion_start_limit_rejected_total", "limit" => "max_sessions_per_document").increment(1);
            return Err(Status::resource_exhausted(
                "max_sessions_per_document exceeded",
            ));
        }
        let session_id = Uuid::new_v4();
        let expires_at = chrono::Utc::now()
            + chrono::Duration::seconds(
                self.cfg.ingestion.chunked_ingestion_session_ttl_seconds as i64,
            );
        let row = sqlx::query(
            "INSERT INTO astravector.ingestion_sessions_v004 (ingestion_session_id, access_zone_id, access_zone_code, document_id, document_version, source_uri, file_name, content_hash, idempotency_key, request_fingerprint, status, total_bytes_estimate, total_blocks_estimate, ttl_days, expires_at, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'ACTIVE',$11,$12,$13,$14,now(),now()) RETURNING ingestion_session_id, status, expires_at"
        )
        .bind(session_id)
        .bind(access_zone_id)
        .bind(&access_zone_code)
        .bind(document_id)
        .bind(r.document_version as i64)
        .bind(&r.source_uri)
        .bind(&r.file_name)
        .bind(&r.content_hash)
        .bind(&idempotency_key)
        .bind(&request_fingerprint)
        .bind(r.total_bytes_estimate as i64)
        .bind(r.total_blocks_estimate as i64)
        .bind(normalized_ttl_days as i32)
        .bind(expires_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| Status::unavailable(format!("postgres ingestion start: {e}")))?;
        tx.commit()
            .await
            .map_err(|e| Status::unavailable(format!("postgres ingestion start commit: {e}")))?;
        counter!("ingestion_chunked_sessions_started_total").increment(1);
        Ok(Response::new(pb::StartLogicalDocumentIngestionResponse {
            ingestion_session_id: row.get::<Uuid, _>("ingestion_session_id").to_string(),
            status: row.get::<String, _>("status"),
            expires_at: row
                .get::<chrono::DateTime<chrono::Utc>, _>("expires_at")
                .to_rfc3339(),
            warnings: Vec::new(),
        }))
    }

    async fn append_logical_document_blocks(
        &self,
        request: Request<pb::AppendLogicalDocumentBlocksRequest>,
    ) -> Result<Response<pb::AppendLogicalDocumentBlocksResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let session_id = Uuid::parse_str(r.ingestion_session_id.trim())
            .map_err(|_| Status::invalid_argument("ingestion_session_id must be UUID"))?;
        if r.blocks.is_empty() {
            return Err(Status::invalid_argument("blocks are required"));
        }
        if r.batch_content_hash.trim().is_empty() {
            return Err(Status::invalid_argument("batch_content_hash is required"));
        }
        let client_batch_hash = normalize_sha256_hex(&r.batch_content_hash)
            .map_err(|_| Status::invalid_argument("batch_content_hash must be sha256 hex"))?;
        let server_batch_hash = compute_batch_content_hash(&r.blocks)
            .map_err(|e| Status::internal(format!("batch hash serialization: {e}")))?;
        if client_batch_hash != server_batch_hash {
            counter!("ingestion_append_batch_hash_mismatch_total").increment(1);
            return Err(Status::failed_precondition("BATCH_HASH_MISMATCH"));
        }
        if r.blocks.len() > self.cfg.ingestion.chunked_ingestion_max_blocks_per_batch {
            counter!("ingestion_limit_rejected_total", "limit" => "chunked_ingestion_max_blocks_per_batch").increment(1);
            return Err(Status::resource_exhausted(
                "batch exceeds chunked_ingestion_max_blocks_per_batch",
            ));
        }
        let batch_bytes: usize = r.blocks.iter().map(logical_block_size_bytes).sum();
        if batch_bytes > self.cfg.ingestion.chunked_ingestion_max_batch_bytes {
            counter!("ingestion_append_batch_size_rejected_total").increment(1);
            counter!("ingestion_limit_rejected_total", "limit" => "chunked_ingestion_max_batch_bytes").increment(1);
            return Err(Status::resource_exhausted(
                "batch exceeds chunked_ingestion_max_batch_bytes",
            ));
        }
        let repo = self.repo()?;
        let mut tx = repo
            .pool
            .begin()
            .await
            .map_err(|e| Status::unavailable(format!("postgres ingestion append tx: {e}")))?;
        let session = sqlx::query("SELECT status, expires_at, received_blocks FROM astravector.ingestion_sessions_v004 WHERE ingestion_session_id=$1 FOR UPDATE")
            .bind(session_id).fetch_optional(&mut *tx).await
            .map_err(|e| Status::unavailable(format!("postgres ingestion append lookup: {e}")))?
            .ok_or_else(|| Status::not_found("INGESTION_SESSION_NOT_FOUND"))?;
        let status: String = session.get("status");
        if status != "ACTIVE" {
            return Err(Status::failed_precondition(format!(
                "INGESTION_SESSION_{status}"
            )));
        }
        let expires_at: chrono::DateTime<chrono::Utc> = session.get("expires_at");
        if expires_at < chrono::Utc::now() {
            return Err(Status::failed_precondition("INGESTION_SESSION_EXPIRED"));
        }
        if let Some(batch) = sqlx::query("SELECT batch_content_hash, block_count, batch_size_bytes FROM astravector.ingestion_session_batches_v004 WHERE ingestion_session_id=$1 AND batch_index=$2")
            .bind(session_id).bind(r.batch_index as i32).fetch_optional(&mut *tx).await
            .map_err(|e| Status::unavailable(format!("postgres ingestion append batch lookup: {e}")))? {
            let stored_hash: String = batch.get("batch_content_hash");
            if stored_hash != server_batch_hash {
                counter!("ingestion_append_batch_hash_mismatch_total").increment(1);
                return Err(Status::failed_precondition("BATCH_HASH_MISMATCH"));
            }
            tx.commit().await.map_err(|e| Status::unavailable(format!("postgres ingestion append replay commit: {e}")))?;
            counter!("ingestion_append_batches_replayed_total").increment(1);
            return Ok(Response::new(pb::AppendLogicalDocumentBlocksResponse { ingestion_session_id: session_id.to_string(), status: "ACTIVE".into(), accepted_blocks: batch.get::<i32,_>("block_count") as u32, accepted_batch_index: r.batch_index, warnings: Vec::new() }));
        }
        let received_blocks: i64 = session.get("received_blocks");
        if received_blocks.saturating_add(r.blocks.len() as i64) as usize
            > self.cfg.ingestion.max_blocks_per_document
        {
            counter!("ingestion_limit_rejected_total", "limit" => "max_blocks_per_document")
                .increment(1);
            return Err(Status::resource_exhausted(
                "max_blocks_per_document exceeded",
            ));
        }
        sqlx::query("INSERT INTO astravector.ingestion_session_batches_v004 (ingestion_session_id, batch_index, batch_content_hash, block_count, batch_size_bytes, created_at) VALUES ($1,$2,$3,$4,$5,now())")
            .bind(session_id).bind(r.batch_index as i32).bind(&server_batch_hash).bind(r.blocks.len() as i32).bind(batch_bytes as i64)
            .execute(&mut *tx).await.map_err(|e| Status::unavailable(format!("postgres ingestion append batch insert: {e}")))?;
        for (idx, block) in r.blocks.iter().enumerate() {
            let block_json = logical_block_to_json(block);
            sqlx::query("INSERT INTO astravector.ingestion_session_blocks_v004 (ingestion_session_id, batch_index, block_index, block_json, batch_content_hash, block_size_bytes, created_at) VALUES ($1,$2,$3,$4,$5,$6,now())")
                .bind(session_id).bind(r.batch_index as i32).bind(idx as i32).bind(block_json).bind(&server_batch_hash).bind(logical_block_size_bytes(block) as i32)
                .execute(&mut *tx).await.map_err(|e| Status::unavailable(format!("postgres ingestion append block: {e}")))?;
        }
        sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET received_batches = received_batches + 1, received_blocks = received_blocks + $2, received_bytes = received_bytes + $3, updated_at=now() WHERE ingestion_session_id=$1")
            .bind(session_id).bind(r.blocks.len() as i64).bind(batch_bytes as i64).execute(&mut *tx).await
            .map_err(|e| Status::unavailable(format!("postgres ingestion append update: {e}")))?;
        tx.commit()
            .await
            .map_err(|e| Status::unavailable(format!("postgres ingestion append commit: {e}")))?;
        counter!("ingestion_append_batches_inserted_total").increment(1);
        Ok(Response::new(pb::AppendLogicalDocumentBlocksResponse {
            ingestion_session_id: session_id.to_string(),
            status: "ACTIVE".into(),
            accepted_blocks: r.blocks.len() as u32,
            accepted_batch_index: r.batch_index,
            warnings: Vec::new(),
        }))
    }

    async fn finalize_logical_document_ingestion(
        &self,
        request: Request<pb::FinalizeLogicalDocumentIngestionRequest>,
    ) -> Result<Response<pb::IndexLogicalDocumentResponse>, Status> {
        let metadata = request.metadata().clone();
        self.require_internal_or_admin(&metadata)?;
        let r = request.into_inner();
        let session_id = Uuid::parse_str(r.ingestion_session_id.trim())
            .map_err(|_| Status::invalid_argument("ingestion_session_id must be UUID"))?;
        if r.final_content_hash.trim().is_empty() {
            return Err(Status::invalid_argument("final_content_hash is required"));
        }
        let repo = self.repo()?;
        let started = Instant::now();

        // fix4.5.1: acquire finalize ownership atomically. Only ACTIVE -> FINALIZING is allowed.
        // No indexing call is allowed unless this UPDATE returns the session row.
        let owned = sqlx::query(
            "UPDATE astravector.ingestion_sessions_v004
             SET status='FINALIZING',
                 finalizing_started_at=COALESCE(finalizing_started_at, now()),
                 finalizing_heartbeat_at=now(),
                 updated_at=now()
             WHERE ingestion_session_id=$1
               AND status='ACTIVE'
               AND expires_at >= now()
             RETURNING access_zone_id, document_id, document_version, status, expires_at, source_uri, file_name, content_hash, ttl_days"
        )
        .bind(session_id)
        .fetch_optional(&repo.pool)
        .await
        .map_err(|e| Status::unavailable(format!("postgres ingestion finalize ownership: {e}")))?;

        let Some(session) = owned else {
            let existing = sqlx::query(
                "SELECT status, expires_at, result_response_json, error_code, error_message
                 FROM astravector.ingestion_sessions_v004
                 WHERE ingestion_session_id=$1",
            )
            .bind(session_id)
            .fetch_optional(&repo.pool)
            .await
            .map_err(|e| {
                Status::unavailable(format!("postgres ingestion finalize state lookup: {e}"))
            })?
            .ok_or_else(|| Status::not_found("INGESTION_SESSION_NOT_FOUND"))?;

            let status: String = existing.get("status");
            if status == "COMPLETED" {
                if let Some(value) = existing
                    .try_get::<Option<serde_json::Value>, _>("result_response_json")
                    .ok()
                    .flatten()
                {
                    let response = index_logical_document_response_from_json(value)?;
                    counter!("ingestion_finalize_replayed_completed_total").increment(1);
                    counter!("ingestion_finalize_completed_replay_total").increment(1);
                    return Ok(Response::new(response));
                }
                return Err(Status::data_loss("INGESTION_COMPLETED_RESULT_MISSING"));
            }
            if status == "FINALIZING" {
                counter!("ingestion_finalize_already_finalizing_total").increment(1);
                counter!("ingestion_finalize_concurrent_rejected_total").increment(1);
                return Err(Status::aborted("INGESTION_SESSION_FINALIZING"));
            }
            if status == "FAILED" {
                let code: Option<String> = existing.try_get("error_code").ok();
                let msg: Option<String> = existing.try_get("error_message").ok();
                return Err(Status::failed_precondition(format!(
                    "INGESTION_SESSION_FAILED:{}:{}",
                    code.unwrap_or_default(),
                    msg.unwrap_or_default()
                )));
            }
            if status == "ABORTED" || status == "EXPIRED" {
                return Err(Status::failed_precondition(format!(
                    "INGESTION_SESSION_{status}"
                )));
            }
            let expires_at: chrono::DateTime<chrono::Utc> = existing.get("expires_at");
            if expires_at < chrono::Utc::now() {
                return Err(Status::failed_precondition("INGESTION_SESSION_EXPIRED"));
            }
            return Err(Status::failed_precondition(format!(
                "INGESTION_SESSION_{status}"
            )));
        };
        counter!("ingestion_finalize_ownership_acquired_total").increment(1);

        let block_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM astravector.ingestion_session_blocks_v004 WHERE ingestion_session_id=$1"
        )
        .bind(session_id)
        .fetch_one(&repo.pool)
        .await
        .map_err(|e| Status::unavailable(format!("postgres ingestion finalize block count: {e}")))?;
        if block_count == 0 {
            mark_ingestion_session_failed(
                &repo.pool,
                session_id,
                "INGESTION_STAGING_EMPTY",
                "no staged blocks",
            )
            .await
            .ok();
            return Err(Status::failed_precondition("INGESTION_STAGING_EMPTY"));
        }
        if block_count as usize > self.cfg.ingestion.max_blocks_per_document {
            mark_ingestion_session_failed(
                &repo.pool,
                session_id,
                "MAX_BLOCKS_PER_DOCUMENT_EXCEEDED",
                "staged blocks exceed max_blocks_per_document",
            )
            .await
            .ok();
            counter!("ingestion_limit_rejected_total", "limit" => "max_blocks_per_document")
                .increment(1);
            return Err(Status::resource_exhausted(
                "max_blocks_per_document exceeded",
            ));
        }
        if self.cfg.ingestion.finalize_mode == "BOUNDED_IN_MEMORY"
            && block_count as usize > self.cfg.ingestion.finalize_streaming_required_above_blocks
            && block_count as usize > self.cfg.ingestion.finalize_max_in_memory_blocks
        {
            record_nonterminal_ingestion_error(
                &repo.pool,
                session_id,
                "FINALIZE_MEMORY_GUARD_EXCEEDED",
                "staged document exceeds bounded in-memory finalize guard",
                self.cfg
                    .ingestion
                    .finalize_memory_guard_exceeded_status
                    .as_str(),
            )
            .await
            .ok();
            counter!("ingestion_finalize_max_in_memory_guard_rejected_total").increment(1);
            counter!("ingestion_finalize_memory_guard_exceeded_total").increment(1);
            return Err(Status::resource_exhausted("FINALIZE_MEMORY_GUARD_EXCEEDED: staged document exceeds bounded in-memory finalize guard"));
        }

        let mut blocks = Vec::with_capacity(
            (block_count as usize).min(self.cfg.ingestion.finalize_max_in_memory_blocks),
        );
        let mut cursor: Option<(i32, i32)> = None;
        let page_size = self.cfg.ingestion.finalize_read_batch_size.max(1) as i64;
        let mut read_pages = 0u64;
        loop {
            let rows = if let Some((batch_idx, block_idx)) = cursor {
                sqlx::query(
                    "SELECT batch_index, block_index, block_json
                     FROM astravector.ingestion_session_blocks_v004
                     WHERE ingestion_session_id=$1
                       AND (batch_index, block_index) > ($2, $3)
                     ORDER BY batch_index, block_index
                     LIMIT $4",
                )
                .bind(session_id)
                .bind(batch_idx)
                .bind(block_idx)
                .bind(page_size)
                .fetch_all(&repo.pool)
                .await
            } else {
                sqlx::query(
                    "SELECT batch_index, block_index, block_json
                     FROM astravector.ingestion_session_blocks_v004
                     WHERE ingestion_session_id=$1
                     ORDER BY batch_index, block_index
                     LIMIT $2",
                )
                .bind(session_id)
                .bind(page_size)
                .fetch_all(&repo.pool)
                .await
            }
            .map_err(|e| {
                Status::unavailable(format!("postgres ingestion finalize blocks page: {e}"))
            })?;
            if rows.is_empty() {
                break;
            }
            read_pages += 1;
            if self.cfg.ingestion.finalizing_heartbeat_enabled {
                let _ = sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET finalizing_heartbeat_at=now(), updated_at=now() WHERE ingestion_session_id=$1 AND status='FINALIZING'")
                    .bind(session_id)
                    .execute(&repo.pool)
                    .await;
                counter!("ingestion_finalizing_heartbeat_total").increment(1);
            }
            for row in rows {
                let batch_idx: i32 = row.get("batch_index");
                let block_idx: i32 = row.get("block_index");
                cursor = Some((batch_idx, block_idx));
                let value: serde_json::Value = row.get("block_json");
                blocks.push(logical_block_from_json(&value)?);
                if blocks.len() > self.cfg.ingestion.finalize_max_in_memory_blocks {
                    record_nonterminal_ingestion_error(
                        &repo.pool,
                        session_id,
                        "FINALIZE_MEMORY_GUARD_EXCEEDED",
                        "staged blocks exceed finalize_max_in_memory_blocks",
                        self.cfg
                            .ingestion
                            .finalize_memory_guard_exceeded_status
                            .as_str(),
                    )
                    .await
                    .ok();
                    counter!("ingestion_finalize_max_in_memory_guard_rejected_total").increment(1);
                    counter!("ingestion_finalize_memory_guard_exceeded_total").increment(1);
                    return Err(Status::resource_exhausted(
                        "finalize_max_in_memory_blocks exceeded",
                    ));
                }
            }
        }
        counter!("ingestion_finalize_blocks_streamed_total").increment(blocks.len() as u64);
        counter!("ingestion_finalize_read_pages_total").increment(read_pages);

        validate_staged_batch_consistency(&repo.pool, session_id).await?;

        let staged_text = render_logical_blocks_for_chunking(&blocks);
        let computed_hash = normalized_content_hash("", &staged_text)?;
        if computed_hash
            != r.final_content_hash
                .trim()
                .trim_start_matches("sha256:")
                .to_ascii_lowercase()
        {
            // Hash mismatch is a client-correctable precondition error; keep session FINALIZING unsafe would block retry,
            // so return it to ACTIVE and record the last error fields.
            record_nonterminal_ingestion_error(
                &repo.pool,
                session_id,
                "FINAL_CONTENT_HASH_MISMATCH",
                &format!(
                    "expected {}, computed {}",
                    r.final_content_hash, computed_hash
                ),
                "RETURN_TO_ACTIVE",
            )
            .await
            .ok();
            counter!("ingestion_finalize_hash_mismatch_total").increment(1);
            return Err(Status::failed_precondition("FINAL_CONTENT_HASH_MISMATCH"));
        }
        let access_zone_id: Uuid = session.get("access_zone_id");
        let document_id: Uuid = session.get("document_id");
        let document_version: i64 = session.get("document_version");
        let source_uri: Option<String> = session
            .try_get::<Option<String>, _>("source_uri")
            .ok()
            .flatten();
        let file_name: Option<String> = session
            .try_get::<Option<String>, _>("file_name")
            .ok()
            .flatten();
        let content_hash: Option<String> = session
            .try_get::<Option<String>, _>("content_hash")
            .ok()
            .flatten();
        let ttl_days: Option<i32> = session.try_get::<Option<i32>, _>("ttl_days").ok().flatten();
        let access_zone_code: Option<String> = session
            .try_get::<Option<String>, _>("access_zone_code")
            .ok()
            .flatten();
        let ttl_policy = ttl_days.and_then(|d| {
            if d <= 0 {
                None
            } else {
                Some(pb::TtlPolicy {
                    mode: pb::TtlMode::Relative as i32,
                    ttl_seconds: (d as u64).saturating_mul(86_400),
                    expires_at: String::new(),
                    delete_from_qdrant_on_expire: true,
                    keep_metadata_after_expire: true,
                })
            }
        });
        let mut inner = Request::new(pb::IndexLogicalDocumentRequest {
            context: Some(pb::RequestContext {
                correlation_id: format!("chunked-finalize-{session_id}"),
                idempotency_key: format!("v007-chunked-finalize:{session_id}"),
                caller_service: "chunked-ingestion".into(),
                caller_user_id: String::new(),
                caller_access_level: pb::AccessLevel::Internal as i32,
            }),
            access_zone_id: access_zone_id.to_string(),
            access_zone_code: access_zone_code.unwrap_or_default(),
            document: Some(pb::DocumentIdentity {
                external_document_id: String::new(),
                document_id: document_id.to_string(),
                document_version: document_version as u64,
                title: file_name.clone().unwrap_or_default(),
                source_uri: source_uri.unwrap_or_default(),
                source_type: "CHUNKED_LOGICAL_DOCUMENT".into(),
                mime_type: String::new(),
                content_hash: content_hash.unwrap_or_default(),
                source_links: Vec::new(),
            }),
            blocks,
            chunking_options: None,
            indexing_options: Some(pb::VectorIndexingOptions {
                activation_policy: pb::ActivationPolicy::AutoWhenReady as i32,
                embedding_mode: pb::EmbeddingModeV005::DenseSparseIfAvailable as i32,
                publish_mode: pb::PublishModeV005::Outbox as i32,
                ttl_policy,
                replace_existing_version: true,
            }),
            metadata: std::collections::HashMap::new(),
        });
        *inner.metadata_mut() = metadata;
        if self.cfg.ingestion.finalizing_heartbeat_enabled {
            let _ = sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET finalizing_heartbeat_at=now(), updated_at=now() WHERE ingestion_session_id=$1 AND status='FINALIZING'")
                .bind(session_id)
                .execute(&repo.pool)
                .await;
            counter!("ingestion_finalizing_heartbeat_total").increment(1);
        }
        // fix463: keep FINALIZING session heartbeat fresh while the potentially
        // long index_logical_document call is running, otherwise ingestion_cleanup
        // can mark the session FAILED while indexing commits document/chunks/outbox.
        let finalize_hb_stop = CancellationToken::new();
        let finalize_hb_task = if self.cfg.ingestion.finalizing_heartbeat_enabled {
            let pool = repo.pool.clone();
            let stop = finalize_hb_stop.clone();
            let interval_seconds = self
                .cfg
                .ingestion
                .finalizing_heartbeat_interval_seconds
                .max(1);
            Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
                loop {
                    tokio::select! {
                        _ = stop.cancelled() => break,
                        _ = interval.tick() => {
                            let _ = sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET finalizing_heartbeat_at=now(), updated_at=now() WHERE ingestion_session_id=$1 AND status='FINALIZING'")
                                .bind(session_id)
                                .execute(&pool)
                                .await;
                            counter!("ingestion_finalizing_heartbeat_total").increment(1);
                        }
                    }
                }
            }))
        } else {
            None
        };
        let indexing_result = self.index_logical_document(inner).await;
        finalize_hb_stop.cancel();
        if let Some(task) = finalize_hb_task {
            let _ = task.await;
        }
        let response = match indexing_result {
            Ok(response) => response.into_inner(),
            Err(status) => {
                mark_ingestion_session_failed(
                    &repo.pool,
                    session_id,
                    "INDEXING_FAILED",
                    status.message(),
                )
                .await
                .ok();
                counter!("ingestion_finalize_failed_total", "error_code" => "INDEXING_FAILED")
                    .increment(1);
                counter!("ingestion_finalize_indexing_failed_total").increment(1);
                return Err(status);
            }
        };
        let response_json = index_logical_document_response_to_json(&response);
        let complete_result = sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET status='COMPLETED', final_content_hash=$2, result_response_json=$3, finalized_at=now(), result_expires_at=now() + ($4 * interval '1 second'), finalizing_heartbeat_at=now(), updated_at=now() WHERE ingestion_session_id=$1 AND status='FINALIZING'")
            .bind(session_id)
            .bind(&r.final_content_hash)
            .bind(response_json)
            .bind(self.cfg.ingestion.completed_session_result_retention_seconds as i64)
            .execute(&repo.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres ingestion finalize complete: {e}")))?;
        if complete_result.rows_affected() != 1 {
            counter!("ingestion_finalize_lost_ownership_total").increment(1);
            counter!("ingestion_finalize_complete_update_zero_rows_total").increment(1);
            return Err(Status::aborted("INGESTION_FINALIZE_LOST_OWNERSHIP"));
        }
        counter!("ingestion_chunked_sessions_finalized_total").increment(1);
        histogram!("ingestion_finalize_duration_ms").record(started.elapsed().as_millis() as f64);
        Ok(Response::new(response))
    }

    async fn abort_logical_document_ingestion(
        &self,
        request: Request<pb::AbortLogicalDocumentIngestionRequest>,
    ) -> Result<Response<pb::AbortLogicalDocumentIngestionResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let session_id = Uuid::parse_str(r.ingestion_session_id.trim())
            .map_err(|_| Status::invalid_argument("ingestion_session_id must be UUID"))?;
        let repo = self.repo()?;
        let row = sqlx::query(
            "SELECT status FROM astravector.ingestion_sessions_v004 WHERE ingestion_session_id=$1",
        )
        .bind(session_id)
        .fetch_optional(&repo.pool)
        .await
        .map_err(|e| Status::unavailable(format!("postgres ingestion abort lookup: {e}")))?
        .ok_or_else(|| Status::not_found("INGESTION_SESSION_NOT_FOUND"))?;
        let status: String = row.get("status");
        match status.as_str() {
            "ACTIVE" => {
                let result = sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET status='ABORTED', error_message=$2, updated_at=now() WHERE ingestion_session_id=$1 AND status='ACTIVE'")
                    .bind(session_id)
                    .bind(r.reason)
                    .execute(&repo.pool)
                    .await
                    .map_err(|e| Status::unavailable(format!("postgres ingestion abort: {e}")))?;
                if result.rows_affected() == 0 {
                    return Err(Status::aborted("INGESTION_SESSION_STATE_CHANGED"));
                }
                counter!("ingestion_chunked_sessions_aborted_total").increment(1);
                Ok(Response::new(pb::AbortLogicalDocumentIngestionResponse {
                    ingestion_session_id: session_id.to_string(),
                    status: "ABORTED".into(),
                }))
            }
            "ABORTED" => Ok(Response::new(pb::AbortLogicalDocumentIngestionResponse {
                ingestion_session_id: session_id.to_string(),
                status: "ABORTED".into(),
            })),
            "FINALIZING" => Err(Status::failed_precondition("INGESTION_SESSION_FINALIZING")),
            "COMPLETED" => Err(Status::failed_precondition("INGESTION_SESSION_COMPLETED")),
            "FAILED" => Err(Status::failed_precondition("INGESTION_SESSION_FAILED")),
            "EXPIRED" => Err(Status::failed_precondition("INGESTION_SESSION_EXPIRED")),
            other => Err(Status::failed_precondition(format!(
                "INGESTION_SESSION_{other}"
            ))),
        }
    }

    async fn get_logical_document_ingestion_status(
        &self,
        request: Request<pb::GetLogicalDocumentIngestionStatusRequest>,
    ) -> Result<Response<pb::GetLogicalDocumentIngestionStatusResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let session_id = Uuid::parse_str(r.ingestion_session_id.trim())
            .map_err(|_| Status::invalid_argument("ingestion_session_id must be UUID"))?;
        let row = sqlx::query("SELECT status, received_batches, received_blocks, received_bytes, expires_at, error_code, error_message FROM astravector.ingestion_sessions_v004 WHERE ingestion_session_id=$1")
            .bind(session_id)
            .fetch_optional(&self.repo()?.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres ingestion status: {e}")))?
            .ok_or_else(|| Status::not_found("INGESTION_SESSION_NOT_FOUND"))?;
        Ok(Response::new(
            pb::GetLogicalDocumentIngestionStatusResponse {
                ingestion_session_id: session_id.to_string(),
                status: row.get::<String, _>("status"),
                received_batches: row.get::<i32, _>("received_batches") as u32,
                received_blocks: row.get::<i64, _>("received_blocks") as u64,
                received_bytes: row.get::<i64, _>("received_bytes") as u64,
                expires_at: row
                    .get::<chrono::DateTime<chrono::Utc>, _>("expires_at")
                    .to_rfc3339(),
                error_code: row.try_get::<String, _>("error_code").unwrap_or_default(),
                error_message: row
                    .try_get::<String, _>("error_message")
                    .unwrap_or_default(),
            },
        ))
    }

    async fn get_document_vector_status(
        &self,
        request: Request<pb::GetDocumentVectorStatusRequest>,
    ) -> Result<Response<pb::GetDocumentVectorStatusResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let doc = r
            .document
            .ok_or_else(|| Status::invalid_argument("document is required"))?;
        let access_zone_id = Uuid::parse_str(doc.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("document.access_zone_id must be UUID"))?;
        let document_id = Uuid::parse_str(doc.document_id.trim())
            .map_err(|_| Status::invalid_argument("document.document_id must be UUID"))?;
        if doc.document_version == 0 {
            return Err(Status::invalid_argument(
                "document.document_version must be > 0",
            ));
        }
        let sync = self
            .compute_document_sync_status(
                access_zone_id,
                document_id,
                doc.document_version as i64,
                r.include_qdrant,
            )
            .await?;
        let state = if sync.ready_to_activate {
            pb::OperationState::ReadyToActivate
        } else if sync.failed_bindings > 0 || sync.outbox_failed > 0 {
            pb::OperationState::Failed
        } else if sync.outbox_pending > 0 || sync.outbox_retry_pending > 0 {
            pb::OperationState::Publishing
        } else {
            pb::OperationState::Syncing
        };
        let progress = if sync.expected_bindings == 0 {
            0.0
        } else {
            ((sync.synced_bindings as f32 / sync.expected_bindings as f32) * 100.0).min(100.0)
        };
        Ok(Response::new(pb::GetDocumentVectorStatusResponse {
            document: Some(doc),
            status: Some(pb::DocumentVectorStatus {
                state: state as i32,
                progress_percent: progress,
                searchable: sync.ready_to_activate,
                message: document_status_message(&sync),
                ready_to_activate: sync.ready_to_activate,
                sync: Some(sync),
            }),
        }))
    }

    async fn delete_document_vectors_facade(
        &self,
        request: Request<pb::DeleteDocumentVectorsFacadeRequest>,
    ) -> Result<Response<pb::DeleteDocumentVectorsFacadeResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let doc = r
            .document
            .ok_or_else(|| Status::invalid_argument("document is required"))?;
        let access_zone_id = Uuid::parse_str(doc.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("document.access_zone_id must be UUID"))?;
        let document_id = Uuid::parse_str(doc.document_id.trim())
            .map_err(|_| Status::invalid_argument("document.document_id must be UUID"))?;
        if doc.document_version == 0 {
            return Err(Status::invalid_argument(
                "document.document_version must be > 0",
            ));
        }
        let affected = schedule_v004_delete_document_vectors(
            self.repo()?,
            access_zone_id,
            document_id,
            doc.document_version as i64,
        )
        .await?;
        Ok(Response::new(pb::DeleteDocumentVectorsFacadeResponse {
            document: Some(doc),
            operation: Some(pb::OperationStatus {
                operation_id: r.context.map(|c| c.correlation_id).unwrap_or_default(),
                state: pb::OperationState::DeleteScheduled as i32,
                message: format!("DELETE_POINT events scheduled for {affected} vector bindings; final DELETED state is reported only after publisher/reconciliation confirmation"),
                warnings: Vec::new(),
                errors: Vec::new(),
            }),
        }))
    }
}

#[tonic::async_trait]
impl AstraVectorRetrievalFacade for AstraVectorV004ControlService {
    async fn retrieve_context(
        &self,
        request: Request<pb::RetrieveContextRequest>,
    ) -> Result<Response<pb::RetrieveContextResponse>, Status> {
        let request_timing = RequestTiming::from_request(&request);
        let metadata = request.metadata().clone();
        if self.cfg.security.enabled {
            self.require_trusted_forwarded_identity_headers(&metadata)?;
        }
        let r = request.into_inner();
        let ctx = r.context.unwrap_or_default();
        let effective_access_level =
            effective_retrieve_access_level(&metadata, ctx.caller_access_level)?;
        if r.question.trim().is_empty() {
            return Err(Status::invalid_argument("question is required"));
        }
        let profile =
            pb::RetrievalProfile::try_from(r.profile).unwrap_or(pb::RetrievalProfile::Balanced);
        let max_contexts = if r.max_contexts == 0 {
            5
        } else {
            r.max_contexts.min(self.cfg.limits.search_top_k_max)
        };
        let correlation_id = ctx.correlation_id.clone();
        tracing::debug!(
            correlation_id = %correlation_id,
            access_zone_id = %r.access_zone_id,
            access_zone_code = %r.access_zone_code,
            access_zone_ids_count = r.access_zone_ids.len(),
            access_zone_codes_count = r.access_zone_codes.len(),
            caller_access_level = ?effective_access_level,
            profile = ?profile,
            search_mode = ?retrieval_search_mode(profile),
            max_contexts,
            question_len = r.question.chars().count(),
            "RETRIEVE_CONTEXT_REQUEST_RECEIVED"
        );
        let mut inner = Request::new(pb::SearchRequestV004 {
            correlation_id,
            access_zone_id: r.access_zone_id,
            caller_access_level: effective_access_level as i32,
            query: r.question,
            top_k: max_contexts,
            candidate_limit: retrieval_candidate_limit(profile),
            parent_limit: max_contexts,
            filters: r.filters,
            timeout_ms: self.cfg.grpc.deadlines.query_ms as u32,
            search_mode: retrieval_search_mode(profile) as i32,
            include_debug: r.response_detail == pb::ResponseDetail::Debug as i32,
            include_vectors: false,
            embedding_mode: retrieval_embedding_mode(profile) as i32,
            model_version: None,
            tokenizer_version: None,
            dense_version: None,
            sparse_version: None,
            chunking_version: None,
            enable_graph_expansion: r.enable_graph_expansion
                || self.cfg.graph_rag.retrieval.enabled_by_default,
            graph_max_hops: if r.graph_max_hops == 0 {
                1
            } else {
                r.graph_max_hops.min(1)
            },
            graph_max_related_contexts: if r.graph_max_related_contexts == 0 {
                self.cfg
                    .graph_rag
                    .retrieval
                    .max_related_chunks
                    .min(self.cfg.limits.graph_related_contexts_max) as u32
            } else {
                r.graph_max_related_contexts
                    .min(self.cfg.limits.graph_related_contexts_max as u32)
            },
            access_zone_ids: r.access_zone_ids,
            access_zone_code: r.access_zone_code,
            access_zone_codes: r.access_zone_codes,
        });
        *inner.metadata_mut() = metadata;
        inner.extensions_mut().insert(request_timing);
        inner
            .extensions_mut()
            .insert(RetrievalEntryPoint("RetrieveContext"));
        let search = <Self as AstraVectorV004Control>::search(self, inner)
            .await?
            .into_inner();
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
            .map(retrieved_context_from_search_result)
            .collect::<Vec<_>>();
        tracing::debug!(
            total_candidates,
            returned_contexts = contexts.len(),
            profile = ?profile,
            "RETRIEVE_CONTEXT_RESPONSE_ASSEMBLED"
        );
        Ok(Response::new(pb::RetrieveContextResponse {
            summary: Some(pb::RetrievalSummary {
                total_candidates,
                returned_contexts: contexts.len() as u32,
                profile: profile as i32,
                evidence_status: if degraded {
                    pb::EvidenceStatus::Degraded as i32
                } else if contexts.is_empty() {
                    pb::EvidenceStatus::Insufficient as i32
                } else {
                    pb::EvidenceStatus::Found as i32
                },
                degraded,
                degradation_codes,
                corpus_snapshot_id: String::new(),
                effective_config_sha256: String::new(),
                dense_branch_executed: diagnostics.dense_branch_executed,
                sparse_branch_executed: diagnostics.sparse_branch_executed,
                fusion_executed: diagnostics.fusion_executed,
                dense_branch_candidate_count: diagnostics.dense_branch_candidate_count,
                sparse_branch_candidate_count: diagnostics.sparse_branch_candidate_count,
                fusion_candidate_count: diagnostics.fusion_candidate_count,
            }),
            contexts,
            warnings: search.warnings,
            diagnostics: Some(diagnostics),
            degradation: search.degradation,
        }))
    }

    async fn explain_retrieve(
        &self,
        request: Request<pb::ExplainRetrieveRequest>,
    ) -> Result<Response<pb::ExplainRetrieveResponse>, Status> {
        let request_timing = RequestTiming::from_request(&request);
        let metadata = request.metadata().clone();
        if self.cfg.security.enabled {
            self.require_trusted_forwarded_identity_headers(&metadata)?;
        }
        let r = request.into_inner();
        let ctx = r.context.unwrap_or_default();
        let effective_access_level =
            effective_retrieve_access_level(&metadata, ctx.caller_access_level)?;
        let profile =
            pb::RetrievalProfile::try_from(r.profile).unwrap_or(pb::RetrievalProfile::Balanced);
        let mut inner = Request::new(pb::ExplainSearchRequest {
            correlation_id: ctx.correlation_id,
            access_zone_id: r.access_zone_id,
            caller_access_level: effective_access_level as i32,
            query: r.question,
            search_mode: retrieval_search_mode(profile) as i32,
            embedding_mode: retrieval_embedding_mode(profile) as i32,
            top_k: if r.max_contexts == 0 {
                5
            } else {
                r.max_contexts.min(self.cfg.limits.search_top_k_max)
            },
            candidate_limit: retrieval_candidate_limit(profile),
            timeout_ms: self.cfg.grpc.deadlines.query_ms,
            model_version: None,
            tokenizer_version: None,
            dense_version: None,
            sparse_version: None,
            chunking_version: None,
        });
        *inner.metadata_mut() = metadata;
        inner.extensions_mut().insert(request_timing);
        let explain = <Self as AstraVectorV004Control>::explain_search(self, inner)
            .await?
            .into_inner();
        Ok(Response::new(pb::ExplainRetrieveResponse {
            explain: Some(explain),
        }))
    }
}

#[tonic::async_trait]
impl AstraVectorAdminFacade for AstraVectorV004ControlService {
    async fn debug_document(
        &self,
        request: Request<pb::DebugDocumentRequest>,
    ) -> Result<Response<pb::DebugDocumentResponse>, Status> {
        let metadata = request.metadata().clone();
        let r = request.into_inner();
        let doc = r
            .document
            .ok_or_else(|| Status::invalid_argument("document is required"))?;
        let mut inner = Request::new(pb::DebugDocumentStateRequest {
            access_zone_id: doc.access_zone_id,
            document_id: doc.document_id,
            document_version: doc.document_version,
            include_chunks: r.include_chunks,
            include_vectors: r.include_vectors,
            include_outbox: r.include_outbox,
            include_qdrant: r.include_qdrant,
        });
        *inner.metadata_mut() = metadata;
        let debug = <Self as AstraVectorV004Control>::debug_document_state(self, inner)
            .await?
            .into_inner();
        Ok(Response::new(pb::DebugDocumentResponse {
            debug: Some(debug),
        }))
    }

    async fn retry_outbox(
        &self,
        request: Request<pb::RetryOutboxFacadeRequest>,
    ) -> Result<Response<pb::RetryOutboxFacadeResponse>, Status> {
        let metadata = request.metadata().clone();
        let r = request.into_inner();
        let doc = r
            .document
            .ok_or_else(|| Status::invalid_argument("document is required"))?;
        let mut inner = Request::new(pb::RetryVectorOutboxRequest {
            access_zone_id: doc.access_zone_id,
            document_id: doc.document_id,
            document_version: doc.document_version,
            operation: if r.operation.trim().is_empty() {
                None
            } else {
                Some(r.operation)
            },
            status: if r.status.trim().is_empty() {
                None
            } else {
                Some(r.status)
            },
        });
        *inner.metadata_mut() = metadata;
        let result = <Self as AstraVectorV004Control>::retry_vector_outbox(self, inner)
            .await?
            .into_inner();
        Ok(Response::new(pb::RetryOutboxFacadeResponse {
            result: Some(result),
        }))
    }

    async fn retry_document_deletion(
        &self,
        request: Request<pb::RetryDocumentDeletionRequest>,
    ) -> Result<Response<pb::RetryDocumentDeletionResponse>, Status> {
        self.require_admin(request.metadata())?;
        let r = request.into_inner();
        if r.document_version == 0 {
            return Err(Status::invalid_argument("document_version must be > 0"));
        }
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be a UUID"))?;

        let access_zone_id = if !r.access_zone_id.trim().is_empty() {
            let zone_id = Uuid::parse_str(r.access_zone_id.trim())
                .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
            if !r.access_zone_code.trim().is_empty() {
                let zone = access_zone_registry::resolve_single_code(
                    &self.repo()?.pool,
                    &self.cfg,
                    r.access_zone_code.trim(),
                )
                .await?;
                if zone.access_zone_id != zone_id {
                    counter!("access_zone_code_uuid_mismatch_total").increment(1);
                    return Err(Status::invalid_argument(
                        "access_zone_code/access_zone_id mismatch",
                    ));
                }
            }
            zone_id
        } else if !r.access_zone_code.trim().is_empty() {
            access_zone_registry::resolve_single_code(
                &self.repo()?.pool,
                &self.cfg,
                r.access_zone_code.trim(),
            )
            .await?
            .access_zone_id
        } else {
            return Err(Status::invalid_argument(
                "access_zone_id or access_zone_code is required",
            ));
        };

        let row = sqlx::query(
            "WITH target AS (
                 SELECT access_zone_id, document_id, document_version, lifecycle_status AS previous_lifecycle_status
                 FROM astravector.document_versions
                 WHERE access_zone_id=$1
                   AND document_id=$2
                   AND document_version=$3
                   AND lifecycle_status IN ('DELETE_PERMANENTLY_FAILED','DELETE_FAILED')
                   AND delete_operation_id IS NULL
                 FOR UPDATE
             )
             UPDATE astravector.document_versions dv
             SET lifecycle_status='DELETE_FAILED',
                 next_delete_attempt_at=now(),
                 last_delete_error_code=NULL,
                 last_delete_error_message=NULL,
                 last_delete_error_stage=NULL,
                 delete_operation_id=NULL,
                 delete_fencing_started_at=NULL,
                 delete_attempts=CASE WHEN $4 THEN 0 ELSE dv.delete_attempts END,
                 updated_at=now()
             FROM target
             WHERE dv.access_zone_id=target.access_zone_id
               AND dv.document_id=target.document_id
               AND dv.document_version=target.document_version
             RETURNING target.previous_lifecycle_status,
                       dv.lifecycle_status AS new_lifecycle_status,
                       dv.delete_attempts,
                       dv.next_delete_attempt_at"
        )
        .bind(access_zone_id)
        .bind(document_id)
        .bind(r.document_version as i64)
        .bind(r.reset_attempts)
        .fetch_optional(&self.repo()?.pool)
        .await
        .map_err(|e| Status::unavailable(format!("postgres retry document deletion: {e}")))?;

        let Some(row) = row else {
            counter!("document_lifecycle_update_blocked_by_delete_operation_total", "operation" => "retry_document_deletion").increment(1);
            return Err(Status::failed_precondition("document is not in DELETE_FAILED or DELETE_PERMANENTLY_FAILED, or an active delete_operation_id is present"));
        };
        counter!("index_ttl_retry_document_deletion_manual_total").increment(1);
        let next_attempt: chrono::DateTime<chrono::Utc> = row.get("next_delete_attempt_at");
        Ok(Response::new(pb::RetryDocumentDeletionResponse {
            accepted: true,
            previous_lifecycle_status: row.get::<String, _>("previous_lifecycle_status"),
            new_lifecycle_status: row.get::<String, _>("new_lifecycle_status"),
            delete_attempts: row.get::<i32, _>("delete_attempts").max(0) as u32,
            next_delete_attempt_at: next_attempt.to_rfc3339(),
        }))
    }

    async fn get_runtime_health(
        &self,
        request: Request<pb::GetRuntimeHealthRequest>,
    ) -> Result<Response<pb::GetRuntimeHealthResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let mut status = "SERVING";
        let mut details = Vec::new();

        match &self.repo {
            Some(repo) => match repo.ping().await {
                Ok(()) => details.push("postgres=SERVING".to_string()),
                Err(e) => {
                    status = "NOT_SERVING";
                    details.push(format!("postgres=NOT_SERVING:{e}"));
                }
            },
            None => {
                status = "NOT_SERVING";
                details.push("postgres=NOT_CONFIGURED".to_string());
            }
        }

        if self.cfg.qdrant.enabled {
            match &self.qdrant {
                Some(qdrant) => match qdrant.collection_exists().await {
                    Ok(true) => details.push("qdrant=SERVING".to_string()),
                    Ok(false) => {
                        status = "NOT_SERVING";
                        details.push("qdrant=NOT_SERVING:collection_missing".to_string());
                    }
                    Err(e) => {
                        status = "NOT_SERVING";
                        details.push(format!("qdrant=NOT_SERVING:{e}"));
                    }
                },
                None => {
                    status = "NOT_SERVING";
                    details.push("qdrant=NOT_CONFIGURED".to_string());
                }
            }
        } else {
            details.push("qdrant=DISABLED".to_string());
        }

        details.push(format!("adaptive_mode={:?}", self.cfg.adaptive.mode));

        Ok(Response::new(pb::GetRuntimeHealthResponse {
            status: status.into(),
            message: details.join("; "),
        }))
    }
}

#[derive(Clone)]
pub struct AstraVectorService {
    cfg: Arc<AppConfig>,
    scheduler: Scheduler,
    engine: Arc<dyn InferenceEngine>,
    l1: L1Cache,
    repo: Option<Repository>,
    qdrant: Option<Arc<QdrantClient>>,
    provider: SelectedProvider,
    readiness: Readiness,
    shutdown: CancellationToken,
}
impl AstraVectorService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: Arc<AppConfig>,
        scheduler: Scheduler,
        engine: Arc<dyn InferenceEngine>,
        l1: L1Cache,
        repo: Option<Repository>,
        qdrant: Option<Arc<QdrantClient>>,
        provider: SelectedProvider,
        readiness: Readiness,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            cfg,
            scheduler,
            engine,
            l1,
            repo,
            qdrant,
            provider,
            readiness,
            shutdown,
        }
    }

    fn require_internal_or_admin(&self, metadata: &MetadataMap) -> Result<(), Status> {
        if !self.cfg.security.enabled {
            return Ok(());
        }
        let role = metadata
            .get("x-astravector-role")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if role.eq_ignore_ascii_case("admin") || role.eq_ignore_ascii_case("internal") {
            Ok(())
        } else {
            Err(Status::permission_denied("internal/admin role is required"))
        }
    }

    async fn compute_document_sync_status(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        include_qdrant: bool,
    ) -> Result<pb::GetVectorSyncStatusResponse, Status> {
        let repo = self
            .repo
            .as_ref()
            .ok_or_else(|| Status::unavailable("PostgreSQL unavailable"))?;
        let row = sqlx::query(r#"SELECT
          COALESCE((SELECT status FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3),'NOT_FOUND') AS document_status,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS expected_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status='SYNCED' AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS synced_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status IN('PENDING','UPDATE_PENDING','DELETE_PENDING') AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS pending_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status IN('FAILED','DEAD_LETTER') AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS failed_bindings,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS dense_vectors_expected,
          (SELECT count(*) FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_dense d ON d.cache_entry_id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND b.chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS dense_vectors_found,
          (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS sparse_vectors_expected,
          (SELECT count(*) FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_sparse s ON s.cache_entry_id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND b.chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS sparse_vectors_found,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='PENDING') AS outbox_pending,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='RETRY_PENDING') AS outbox_retry_pending,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status='COMPLETED') AS outbox_completed,
          (SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status IN('FAILED','DEAD_LETTER')) AS outbox_failed,
          (SELECT COALESCE(max(o.updated_at)::text,'') FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3) AS last_sync_attempt_at,
          (SELECT COALESCE(o.error_code,'') FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.error_code IS NOT NULL ORDER BY o.updated_at DESC LIMIT 1) AS last_sync_error_code,
          (SELECT COALESCE(o.error_message,'') FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.id=o.binding_id AND b.access_zone_id=o.binding_access_zone_id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.error_message IS NOT NULL ORDER BY o.updated_at DESC LIMIT 1) AS last_sync_error_message
        "#)
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_one(&repo.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres sync status: {e}")))?;
        let expected_bindings: i64 = row.get("expected_bindings");
        let synced_bindings: i64 = row.get("synced_bindings");
        let dense_vectors_expected: i64 = row.get("dense_vectors_expected");
        let dense_vectors_found: i64 = row.get("dense_vectors_found");
        let sparse_vectors_expected: i64 = row.get("sparse_vectors_expected");
        let sparse_vectors_found: i64 = row.get("sparse_vectors_found");
        let outbox_pending: i64 = row.get("outbox_pending");
        let outbox_retry_pending: i64 = row.get("outbox_retry_pending");
        let outbox_completed: i64 = row.get("outbox_completed");
        let outbox_failed: i64 = row.get("outbox_failed");
        let expected_point_ids: std::collections::HashSet<Uuid> = sqlx::query(
            "SELECT qdrant_point_id FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_granularity IN('PARENT','SUB_180','SUB_260')",
        )
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_all(&repo.pool)
            .await
            .map_err(|e| Status::unavailable(format!("postgres expected qdrant ids: {e}")))?
            .into_iter()
            .map(|row| row.get::<Uuid, _>("qdrant_point_id"))
            .collect();
        let mut qdrant_collection_exists = false;
        let mut qdrant_points_found = 0_u32;
        let mut qdrant_points_missing = expected_point_ids.len() as u32;
        let mut qdrant_points_extra = 0_u32;
        let mut warnings = Vec::new();
        if include_qdrant {
            if let Some(q) = self.qdrant.as_ref() {
                qdrant_collection_exists = q.collection_exists().await.map_err(Status::from)?;
                if qdrant_collection_exists {
                    let actual_point_ids = q
                        .point_ids_by_document(access_zone_id, document_id, document_version)
                        .await
                        .map_err(Status::from)?;
                    qdrant_points_found = actual_point_ids.len() as u32;
                    qdrant_points_missing =
                        expected_point_ids.difference(&actual_point_ids).count() as u32;
                    qdrant_points_extra =
                        actual_point_ids.difference(&expected_point_ids).count() as u32;
                    if qdrant_points_missing > 0 || qdrant_points_extra > 0 {
                        counter!("astravector_sync_status_consistency_mismatch_total").increment(1);
                    }
                    if qdrant_points_extra > 0 {
                        warnings.push(pb::DiagnosticWarningV005 {
                            code: "QDRANT_EXTRA_POINTS_FOUND".into(),
                            message: format!("Qdrant contains {qdrant_points_extra} extra point(s) for this document/version"),
                        });
                    }
                }
            }
        } else {
            qdrant_collection_exists = self.qdrant.is_some();
            qdrant_points_found = expected_bindings as u32;
            qdrant_points_missing = 0;
        }
        let ready = expected_bindings > 0
            && synced_bindings == expected_bindings
            && dense_vectors_found == dense_vectors_expected
            && (!self.cfg.sparse.required || sparse_vectors_found == sparse_vectors_expected)
            && outbox_completed >= expected_bindings
            && outbox_pending == 0
            && outbox_retry_pending == 0
            && outbox_failed == 0
            && qdrant_collection_exists
            && qdrant_points_missing == 0
            && qdrant_points_found >= expected_bindings as u32;
        Ok(pb::GetVectorSyncStatusResponse {
            document_status: row.get::<String, _>("document_status"),
            expected_bindings: expected_bindings as u32,
            synced_bindings: synced_bindings as u32,
            pending_bindings: row.get::<i64, _>("pending_bindings") as u32,
            failed_bindings: row.get::<i64, _>("failed_bindings") as u32,
            dense_vectors_expected: dense_vectors_expected as u32,
            dense_vectors_found: dense_vectors_found as u32,
            sparse_vectors_expected: sparse_vectors_expected as u32,
            sparse_vectors_found: sparse_vectors_found as u32,
            outbox_pending: outbox_pending as u32,
            outbox_retry_pending: outbox_retry_pending as u32,
            outbox_completed: outbox_completed as u32,
            outbox_failed: outbox_failed as u32,
            qdrant_collection: self.cfg.qdrant.collection.clone(),
            qdrant_collection_exists,
            qdrant_points_expected: expected_bindings as u32,
            qdrant_points_found,
            qdrant_points_missing,
            qdrant_points_extra,
            ready_to_activate: ready,
            last_sync_attempt_at: row
                .try_get::<Option<String>, _>("last_sync_attempt_at")
                .ok()
                .flatten()
                .unwrap_or_default(),
            last_sync_error_code: row
                .try_get::<Option<String>, _>("last_sync_error_code")
                .ok()
                .flatten()
                .unwrap_or_default(),
            last_sync_error_message: row
                .try_get::<Option<String>, _>("last_sync_error_message")
                .ok()
                .flatten()
                .unwrap_or_default(),
            warnings,
        })
    }

    fn validate(&self, r: &mut pb::EncodeBatchRequest) -> Result<(), AstraError> {
        for (n, v) in [
            ("emb_task_id", &r.emb_task_id),
            ("correlation_id", &r.correlation_id),
            ("tenant_id", &r.tenant_id),
            ("workspace_id", &r.workspace_id),
            ("caller_service", &r.caller_service),
        ] {
            if v.trim().is_empty() {
                return Err(AstraError::InvalidArgument(format!("{n} is required")));
            }
        }
        Uuid::parse_str(&r.emb_task_id)
            .map_err(|_| AstraError::InvalidArgument("emb_task_id must be UUID".into()))?;
        Uuid::parse_str(&r.correlation_id)
            .map_err(|_| AstraError::InvalidArgument("correlation_id must be UUID".into()))?;
        if r.items.is_empty() || r.items.len() > self.cfg.grpc.max_items_per_batch {
            return Err(AstraError::InvalidArgument("invalid batch size".into()));
        }
        if pb::EncodingPurpose::try_from(r.purpose)
            .ok()
            .filter(|x| *x != pb::EncodingPurpose::Unspecified)
            .is_none()
        {
            return Err(AstraError::InvalidArgument("purpose is required".into()));
        }
        if pb::AccessLevel::try_from(r.access_level)
            .ok()
            .filter(|x| *x != pb::AccessLevel::Unspecified)
            .is_none()
        {
            return Err(AstraError::InvalidArgument(
                "access_level is required".into(),
            ));
        }
        if pb::PersistenceMode::try_from(r.persistence_mode)
            .ok()
            .filter(|x| *x != pb::PersistenceMode::Unspecified)
            .is_none()
        {
            return Err(AstraError::InvalidArgument(
                "persistence_mode is required".into(),
            ));
        }
        if r.expected_contract_version != contract::CONTRACT_VERSION {
            return Err(AstraError::FailedPrecondition(
                "contract version mismatch".into(),
            ));
        }
        if r.expected_tokenizer_version != self.cfg.tokenizer.version {
            return Err(AstraError::FailedPrecondition(
                "tokenizer version mismatch".into(),
            ));
        }
        if r.expected_embedding_version != self.cfg.dense.version {
            return Err(AstraError::FailedPrecondition(
                "embedding version mismatch".into(),
            ));
        }
        let reps: BTreeSet<i32> = r.requested_representations.iter().copied().collect();
        if reps.is_empty()
            || reps.iter().any(|x| {
                pb::RepresentationType::try_from(*x)
                    .ok()
                    .filter(|v| *v != pb::RepresentationType::Unspecified)
                    .is_none()
            })
        {
            return Err(AstraError::InvalidArgument(
                "requested_representations invalid".into(),
            ));
        }
        r.requested_representations = reps.into_iter().collect();
        let mut ids = HashSet::new();
        for i in &r.items {
            if !ids.insert(i.chunk_id.clone()) {
                return Err(AstraError::InvalidArgument(format!(
                    "duplicate chunk_id={}",
                    i.chunk_id
                )));
            }
            Uuid::parse_str(&i.chunk_id).map_err(|_| {
                AstraError::InvalidArgument(format!("invalid chunk_id={}", i.chunk_id))
            })?;
            let ct = pb::ChunkType::try_from(i.chunk_type)
                .map_err(|_| AstraError::InvalidArgument("invalid chunk_type".into()))?;
            if ct == pb::ChunkType::Unspecified {
                return Err(AstraError::InvalidArgument("chunk_type required".into()));
            }
            if ct == pb::ChunkType::Parent && !self.cfg.tokenization.parent.enabled {
                return Err(AstraError::FailedPrecondition(
                    "PARENT_EMBEDDING_DISABLED".into(),
                ));
            }
            if ct == pb::ChunkType::Child && i.parent_chunk_id.as_deref().unwrap_or("").is_empty() {
                return Err(AstraError::InvalidArgument(
                    "child requires parent_chunk_id".into(),
                ));
            }
            if i.text.trim().is_empty() {
                return Err(AstraError::InvalidArgument("text is empty".into()));
            }
            if i.text.len() > 8 * 1024 * 1024 {
                return Err(AstraError::OutOfRange("text byte limit exceeded".into()));
            }
            let h = hash_text(&i.text);
            if let Some(c) = &i.content_hash {
                if !c.eq_ignore_ascii_case(&h) {
                    return Err(AstraError::InvalidArgument(format!(
                        "content_hash mismatch for {}",
                        i.chunk_id
                    )));
                }
            }
        }
        Ok(())
    }
    async fn process(
        &self,
        mut req: pb::EncodeBatchRequest,
        deadline: Instant,
    ) -> Result<pb::EncodeBatchResponse, Status> {
        let started = std::time::Instant::now();
        self.validate(&mut req).map_err(Status::from)?;
        counter!("astravector_requests_total","purpose"=>req.purpose.to_string()).increment(1);
        let mode = pb::PersistenceMode::try_from(req.persistence_mode)
            .map_err(|_| Status::invalid_argument("invalid persistence_mode"))?;
        let want_dense = req
            .requested_representations
            .contains(&(pb::RepresentationType::Dense as i32));
        let want_sparse = req
            .requested_representations
            .contains(&(pb::RepresentationType::BgeLearnedSparse as i32));
        if want_sparse && !self.engine.sparse_available() {
            return Err(Status::failed_precondition(
                "learned sparse unavailable for loaded ONNX artifact",
            ));
        }
        if mode == pb::PersistenceMode::Required && self.repo.is_none() {
            return Err(Status::unavailable("PostgreSQL required"));
        }
        let hashes: Vec<(pb::EncodeItem, String)> = req
            .items
            .iter()
            .cloned()
            .map(|i| {
                let h = hash_text(&i.text);
                (i, h)
            })
            .collect();
        let request_hash = request_hash(&req, &hashes);
        let reps: Vec<String> = req
            .requested_representations
            .iter()
            .map(ToString::to_string)
            .collect();
        let mut request_id = None;
        if let Some(repo) = &self.repo {
            if !req.idempotency_key.is_empty() {
                if let Some(old) = repo
                    .find_idempotent(&req.tenant_id, &req.workspace_id, &req.idempotency_key)
                    .await
                    .map_err(Status::from)?
                {
                    if old.request_hash != request_hash {
                        return Err(Status::failed_precondition(
                            "IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD",
                        ));
                    }
                    if old.status == "COMPLETED" {
                        let rows = repo.replay_items(old.id).await.map_err(Status::from)?;
                        let items = rows
                            .into_iter()
                            .map(|(i, v)| to_pb(&i, &v, false, true, &self.cfg))
                            .collect();
                        return Ok(self.response(&req, items, "REPLAYED", None));
                    }
                    request_id = Some(old.id)
                }
            }
            if request_id.is_none() {
                let id = repo
                    .create_request(
                        &req,
                        &request_hash,
                        "PROCESSING",
                        contract::CONTRACT_VERSION,
                        &self.cfg.tokenizer.version,
                        &self.cfg.model.version,
                        &reps,
                    )
                    .await
                    .map_err(Status::from)?;
                repo.create_items(id, &hashes).await.map_err(Status::from)?;
                request_id = Some(id)
            }
        }
        let futures = hashes
            .into_iter()
            .enumerate()
            .map(|(index, (item, text_hash))| {
                let s = self.clone();
                let r = req.clone();
                async move {
                    let x = s
                        .process_item(
                            &r,
                            item,
                            text_hash,
                            mode,
                            want_dense,
                            want_sparse,
                            request_id,
                            deadline,
                        )
                        .await;
                    (index, x)
                }
            });
        let mut joined = join_all(futures).await;
        joined.sort_by_key(|x| x.0);
        let mut failures = 0;
        let mut persistence_error = None;
        let mut responses = Vec::with_capacity(joined.len());
        for (_, r) in joined {
            match r {
                Ok((x, p)) => {
                    if let Some(e) = p {
                        persistence_error = Some(e)
                    }
                    responses.push(x)
                }
                Err((item, e)) => {
                    failures += 1;
                    responses.push(failed_pb(&item, &e))
                }
            }
        }
        let task = if failures == 0 {
            pb::TaskStatus::TaskCompleted
        } else if failures == responses.len() {
            pb::TaskStatus::TaskFailed
        } else {
            pb::TaskStatus::TaskPartiallyCompleted
        };
        if let (Some(repo), Some(id)) = (&self.repo, request_id) {
            let st = match task {
                pb::TaskStatus::TaskCompleted => "COMPLETED",
                pb::TaskStatus::TaskPartiallyCompleted => "PARTIALLY_COMPLETED",
                _ => "FAILED",
            };
            let _ = repo
                .finish_request(id, st, None, persistence_error.as_deref())
                .await;
        }
        histogram!("astravector_request_duration_seconds").record(started.elapsed().as_secs_f64());
        Ok(pb::EncodeBatchResponse {
            emb_task_id: req.emb_task_id.clone(),
            correlation_id: req.correlation_id.clone(),
            status: task as i32,
            items: responses,
            persistence: Some(pb::PersistenceResult {
                mode: format!("{mode:?}"),
                status: if persistence_error.is_some() {
                    "FAILED".into()
                } else {
                    "PERSISTED_OR_NOT_REQUIRED".into()
                },
                error_code: persistence_error.as_ref().map(|_| "POSTGRES_ERROR".into()),
                error_message: persistence_error,
            }),
            versions: Some(contract::versions(&self.cfg)),
            runtime: Some(contract::runtime_metadata(
                &self.provider.name,
                self.provider.fallback_used,
            )),
        })
    }
    #[allow(clippy::too_many_arguments)]
    async fn process_item(
        &self,
        req: &pb::EncodeBatchRequest,
        item: pb::EncodeItem,
        text_hash: String,
        mode: pb::PersistenceMode,
        want_dense: bool,
        want_sparse: bool,
        request_id: Option<Uuid>,
        deadline: Instant,
    ) -> Result<(pb::EncodeItemResponse, Option<String>), (pb::EncodeItem, AstraError)> {
        if Instant::now() >= deadline {
            return Err((item, AstraError::DeadlineExceeded("request expired".into())));
        }
        let key = cache_key(req, &item, &text_hash, &self.cfg);
        if let Some(v) = self.l1.get(&key).await {
            if mode != pb::PersistenceMode::Required || v.persisted_in_postgres {
                counter!("astravector_l1_cache_hits_total").increment(1);
                return Ok((to_pb(&item, &v.result, true, false, &self.cfg), None));
            }
        }
        counter!("astravector_l1_cache_misses_total").increment(1);
        let query = req.purpose == pb::EncodingPurpose::Query as i32;
        let profile = if query {
            &self.cfg.tokenization.query
        } else if item.chunk_type == pb::ChunkType::Parent as i32 {
            return Err((
                item,
                AstraError::FailedPrecondition(
                    "parent profile adapter not enabled in scheduler".into(),
                ),
            ));
        } else {
            &self.cfg.tokenization.child
        };
        let mut owned: Option<(Uuid, i64)> = None;
        if mode != pb::PersistenceMode::None {
            if let Some(repo) = &self.repo {
                match repo
                    .claim(
                        &req.tenant_id,
                        &req.workspace_id,
                        &key,
                        &text_hash,
                        if query { "QUERY" } else { "DOCUMENT_CHUNK" },
                        item.chunk_type as i16,
                        &self.cfg.tokenizer.version,
                        &self.cfg.model.version,
                        want_dense.then_some(self.cfg.dense.version.as_str()),
                        want_sparse.then_some(self.cfg.sparse.version.as_str()),
                        &self.cfg.service.instance_id,
                        self.cfg.cache.l2.lease_duration_seconds as i64,
                    )
                    .await
                {
                    Ok(ClaimResult::Completed {
                        cache_entry_id,
                        result,
                    }) => {
                        counter!("astravector_l2_cache_hits_total").increment(1);
                        let v = Arc::new(result);
                        self.l1.put(key, v.clone(), true).await;
                        if let (Some(id), Ok(chunk)) = (request_id, Uuid::parse_str(&item.chunk_id))
                        {
                            let _ = repo
                                .update_item(
                                    id,
                                    chunk,
                                    Some(cache_entry_id),
                                    "CACHE_HIT",
                                    Some(&v),
                                    None,
                                    None,
                                )
                                .await;
                        }
                        return Ok((to_pb(&item, &v, false, true, &self.cfg), None));
                    }
                    Ok(ClaimResult::ProcessingByOther { cache_entry_id, .. }) => {
                        counter!("astravector_claim_total","result"=>"wait").increment(1);
                        match repo
                            .wait_completed(
                                &key,
                                deadline,
                                self.cfg.cache.l2.processing_poll_interval_ms,
                                self.cfg.cache.l2.processing_poll_max_interval_ms,
                            )
                            .await
                        {
                            Ok(v) => {
                                let v = Arc::new(v);
                                self.l1.put(key, v.clone(), true).await;
                                if let (Some(id), Ok(chunk)) =
                                    (request_id, Uuid::parse_str(&item.chunk_id))
                                {
                                    let _ = repo
                                        .update_item(
                                            id,
                                            chunk,
                                            Some(cache_entry_id),
                                            "CACHE_HIT",
                                            Some(&v),
                                            None,
                                            None,
                                        )
                                        .await;
                                }
                                return Ok((to_pb(&item, &v, false, true, &self.cfg), None));
                            }
                            Err(e) => return Err((item, e)),
                        }
                    }
                    Ok(ClaimResult::Acquired {
                        cache_entry_id,
                        lease_token,
                    })
                    | Ok(ClaimResult::RetryAcquired {
                        cache_entry_id,
                        lease_token,
                    }) => {
                        counter!("astravector_claim_total","result"=>"owner").increment(1);
                        owned = Some((cache_entry_id, lease_token))
                    }
                    Err(e) => {
                        if mode == pb::PersistenceMode::Required {
                            return Err((item, e));
                        }
                    }
                }
            }
        }
        let kind = if query {
            QueueKind::Query
        } else {
            QueueKind::Document
        };
        let hint = match self.engine.count_tokens(
            &item.text,
            profile.max_length,
            profile.truncation_allowed,
        ) {
            Ok(n) => n,
            Err(e) => return Err((item, e)),
        };
        let input = InferenceInput {
            text: item.text.clone(),
            max_length: profile.max_length,
            allow_truncation: profile.truncation_allowed,
            want_dense,
            want_sparse,
            token_count_hint: hint,
        };
        let computed = match self
            .scheduler
            .submit(kind, input, deadline, self.shutdown.child_token())
            .await
        {
            Ok(v) => Arc::new(v),
            Err(e) => return Err((item, e)),
        };
        let mut persistence_error = None;
        if let (Some(repo), Some((cache_id, lease))) = (&self.repo, owned) {
            let persist = repo
                .persist_owned(
                    cache_id,
                    &self.cfg.service.instance_id,
                    lease,
                    &computed,
                    &self.cfg.dense.name,
                    &self.cfg.dense.version,
                    &self.cfg.sparse.name,
                    &self.cfg.sparse.version,
                    self.cfg.sparse.min_weight,
                    self.cfg.sparse.max_non_zero as i32,
                )
                .await;
            if let Err(e) = persist {
                if mode == pb::PersistenceMode::Required {
                    return Err((item, e));
                }
                persistence_error = Some(e.to_string())
            } else {
                if let Err(e) = repo
                    .upsert_binding_with_outbox(
                        &req.tenant_id,
                        &req.workspace_id,
                        &item,
                        req.access_level,
                        cache_id,
                        &self.cfg.qdrant.collection,
                    )
                    .await
                {
                    if mode == pb::PersistenceMode::Required {
                        return Err((item, e));
                    }
                    persistence_error = Some(e.to_string())
                }
                if let (Some(id), Ok(chunk)) = (request_id, Uuid::parse_str(&item.chunk_id)) {
                    if let Err(e) = repo
                        .update_item(
                            id,
                            chunk,
                            Some(cache_id),
                            "COMPLETED",
                            Some(&computed),
                            None,
                            None,
                        )
                        .await
                    {
                        if mode == pb::PersistenceMode::Required {
                            return Err((item, e));
                        }
                        persistence_error = Some(e.to_string())
                    }
                }
            }
        } else if mode == pb::PersistenceMode::Required {
            return Err((
                item,
                AstraError::Unavailable("REQUIRED persistence not completed".into()),
            ));
        }
        if mode != pb::PersistenceMode::Required || persistence_error.is_none() {
            self.l1
                .put(
                    key,
                    computed.clone(),
                    persistence_error.is_none() && mode != pb::PersistenceMode::None,
                )
                .await
        }
        Ok((
            to_pb(&item, &computed, false, false, &self.cfg),
            persistence_error,
        ))
    }
    fn response(
        &self,
        req: &pb::EncodeBatchRequest,
        items: Vec<pb::EncodeItemResponse>,
        status: &str,
        error: Option<String>,
    ) -> pb::EncodeBatchResponse {
        pb::EncodeBatchResponse {
            emb_task_id: req.emb_task_id.clone(),
            correlation_id: req.correlation_id.clone(),
            status: pb::TaskStatus::TaskCompleted as i32,
            items,
            persistence: Some(pb::PersistenceResult {
                mode: "REPLAY".into(),
                status: status.into(),
                error_code: error.as_ref().map(|_| "ERROR".into()),
                error_message: error,
            }),
            versions: Some(contract::versions(&self.cfg)),
            runtime: Some(contract::runtime_metadata(
                &self.provider.name,
                self.provider.fallback_used,
            )),
        }
    }
}
#[tonic::async_trait]
impl AstraVectorRuntime for AstraVectorService {
    async fn encode(
        &self,
        request: Request<pb::EncodeRequest>,
    ) -> Result<Response<pb::EncodeResponse>, Status> {
        let deadline = deadline_from(request.metadata(), self.cfg.grpc.deadlines.query_ms);
        let r = request.into_inner();
        let item = r
            .item
            .ok_or_else(|| Status::invalid_argument("item required"))?;
        let b = pb::EncodeBatchRequest {
            emb_task_id: r.emb_task_id,
            correlation_id: r.correlation_id,
            idempotency_key: r.idempotency_key,
            tenant_id: r.tenant_id,
            workspace_id: r.workspace_id,
            caller_service: r.caller_service,
            access_level: r.access_level,
            purpose: r.purpose,
            requested_representations: r.requested_representations,
            items: vec![item],
            expected_contract_version: r.expected_contract_version,
            expected_tokenizer_version: r.expected_tokenizer_version,
            expected_embedding_version: r.expected_embedding_version,
            persistence_mode: r.persistence_mode,
        };
        let x = self.process(b, deadline).await?;
        Ok(Response::new(pb::EncodeResponse {
            emb_task_id: x.emb_task_id,
            correlation_id: x.correlation_id,
            item: x.items.into_iter().next(),
            persistence: x.persistence,
            versions: x.versions,
            runtime: x.runtime,
        }))
    }
    async fn encode_batch(
        &self,
        request: Request<pb::EncodeBatchRequest>,
    ) -> Result<Response<pb::EncodeBatchResponse>, Status> {
        let fallback = if request.get_ref().purpose == pb::EncodingPurpose::Query as i32 {
            self.cfg.grpc.deadlines.query_ms
        } else {
            self.cfg.grpc.deadlines.document_batch_ms
        };
        let deadline = deadline_from(request.metadata(), fallback);
        Ok(Response::new(
            self.process(request.into_inner(), deadline).await?,
        ))
    }
    async fn get_contract(
        &self,
        _: Request<pb::GetContractRequest>,
    ) -> Result<Response<pb::GetContractResponse>, Status> {
        Ok(Response::new(pb::GetContractResponse {
            versions: Some(contract::versions(&self.cfg)),
            model_id: self.cfg.model.id.clone(),
            dense_dimension: self.cfg.dense.dimension as u32,
            dense_distance: self.cfg.dense.distance.clone(),
        }))
    }
    async fn get_capabilities(
        &self,
        _: Request<pb::GetCapabilitiesRequest>,
    ) -> Result<Response<pb::GetCapabilitiesResponse>, Status> {
        Ok(Response::new(pb::GetCapabilitiesResponse {
            purposes: vec![1, 2],
            chunk_types: if self.cfg.tokenization.parent.enabled {
                vec![1, 2]
            } else {
                vec![2]
            },
            representations: if self.engine.sparse_available() {
                vec![1, 2]
            } else {
                vec![1]
            },
            dense_dimension: 1024,
            max_query_tokens: self.cfg.tokenization.query.max_length as u32,
            max_child_tokens: self.cfg.tokenization.child.max_length as u32,
            parent_embedding_enabled: self.cfg.tokenization.parent.enabled,
            runtime: Some(contract::runtime_metadata(
                &self.provider.name,
                self.provider.fallback_used,
            )),
        }))
    }
    async fn health(
        &self,
        _: Request<pb::HealthRequest>,
    ) -> Result<Response<pb::HealthResponse>, Status> {
        let ready = self.readiness.is_ready() && self.scheduler.healthy();
        Ok(Response::new(pb::HealthResponse {
            status: if ready {
                "SERVING".into()
            } else {
                "NOT_SERVING".into()
            },
            ready,
            details: self.provider.name.clone(),
        }))
    }
    async fn delete_document_vectors(
        &self,
        request: Request<pb::DeleteDocumentVectorsRequest>,
    ) -> Result<Response<pb::DeleteDocumentVectorsResponse>, Status> {
        let r = request.into_inner();
        let repo = self
            .repo
            .as_ref()
            .ok_or_else(|| Status::unavailable("PostgreSQL unavailable"))?;
        let doc = Uuid::parse_str(&r.document_id)
            .map_err(|_| Status::invalid_argument("invalid document_id"))?;
        let affected = repo
            .delete_document_vectors(
                &r.tenant_id,
                &r.workspace_id,
                doc,
                r.document_version.map(|x| x as i64),
            )
            .await
            .map_err(Status::from)?;
        Ok(Response::new(pb::DeleteDocumentVectorsResponse {
            affected_bindings: affected,
            status: "DELETE_PENDING".into(),
        }))
    }
    async fn update_vector_metadata(
        &self,
        request: Request<pb::UpdateVectorMetadataRequest>,
    ) -> Result<Response<pb::UpdateVectorMetadataResponse>, Status> {
        let r = request.into_inner();
        let repo = self
            .repo
            .as_ref()
            .ok_or_else(|| Status::unavailable("PostgreSQL unavailable"))?;
        let id = Uuid::parse_str(&r.binding_id)
            .map_err(|_| Status::invalid_argument("invalid binding_id"))?;
        repo.update_binding_metadata(
            &r.tenant_id,
            &r.workspace_id,
            id,
            r.access_level,
            r.ttl_days,
            &r.metadata,
        )
        .await
        .map_err(Status::from)?;
        Ok(Response::new(pb::UpdateVectorMetadataResponse {
            binding_id: r.binding_id,
            status: "UPDATE_PENDING".into(),
        }))
    }
    async fn extend_vector_ttl(
        &self,
        request: Request<pb::ExtendVectorTtlRequest>,
    ) -> Result<Response<pb::ExtendVectorTtlResponse>, Status> {
        let r = request.into_inner();
        let repo = self
            .repo
            .as_ref()
            .ok_or_else(|| Status::unavailable("PostgreSQL unavailable"))?;
        if r.ttl_days < self.cfg.lifecycle.min_ttl_days
            || r.ttl_days > self.cfg.lifecycle.max_ttl_days
        {
            return Err(Status::invalid_argument(
                "ttl_days outside configured range",
            ));
        }
        let id = Uuid::parse_str(&r.binding_id)
            .map_err(|_| Status::invalid_argument("invalid binding_id"))?;
        let exp = repo
            .extend_binding_ttl(&r.tenant_id, &r.workspace_id, id, r.ttl_days, r.replace)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(pb::ExtendVectorTtlResponse {
            binding_id: r.binding_id,
            expires_at: exp.to_rfc3339(),
            status: "UPDATE_PENDING".into(),
        }))
    }
    async fn get_vector_sync_status(
        &self,
        request: Request<pb::GetVectorSyncStatusRequest>,
    ) -> Result<Response<pb::GetVectorSyncStatusResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be a UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument("document_version must be > 0"));
        }
        let status = self
            .compute_document_sync_status(
                access_zone_id,
                document_id,
                r.document_version as i64,
                r.include_qdrant,
            )
            .await?;
        Ok(Response::new(status))
    }

    async fn preview_embedding(
        &self,
        request: Request<pb::PreviewEmbeddingRequest>,
    ) -> Result<Response<pb::PreviewEmbeddingResponse>, Status> {
        self.require_internal_or_admin(request.metadata())?;
        let r = request.into_inner();
        let text = r.text.trim();
        if text.is_empty() {
            return Err(Status::invalid_argument("text must not be empty"));
        }
        let purpose =
            pb::EncodingPurpose::try_from(r.purpose).unwrap_or(pb::EncodingPurpose::Unspecified);
        if purpose == pb::EncodingPurpose::Unspecified {
            return Err(Status::invalid_argument("purpose is required"));
        }
        let profile = if purpose == pb::EncodingPurpose::Query {
            &self.cfg.tokenization.query
        } else {
            &self.cfg.tokenization.child
        };
        let sparse_required = match pb::EmbeddingModeV005::try_from(r.embedding_mode)
            .unwrap_or(pb::EmbeddingModeV005::Unspecified)
        {
            pb::EmbeddingModeV005::DenseSparseRequired => true,
            pb::EmbeddingModeV005::Unspecified => self.cfg.sparse.required,
            _ => false,
        };
        let wants_sparse = match pb::EmbeddingModeV005::try_from(r.embedding_mode)
            .unwrap_or(pb::EmbeddingModeV005::Unspecified)
        {
            pb::EmbeddingModeV005::DenseOnly => false,
            pb::EmbeddingModeV005::DenseSparseIfAvailable
            | pb::EmbeddingModeV005::DenseSparseRequired => true,
            pb::EmbeddingModeV005::Unspecified => self.cfg.sparse.enabled,
        };
        if wants_sparse && sparse_required && !self.engine.sparse_available() {
            return Err(Status::failed_precondition(
                "SPARSE_UNAVAILABLE: sparse embedding requested but loaded ONNX artifact has no sparse output",
            ));
        }
        let timeout_ms = effective_query_timeout_ms(r.timeout_ms, self.cfg.grpc.deadlines.query_ms);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let result = self
            .scheduler
            .submit(
                if purpose == pb::EncodingPurpose::Query {
                    QueueKind::Query
                } else {
                    QueueKind::Document
                },
                InferenceInput {
                    text: text.to_string(),
                    max_length: profile.max_length,
                    allow_truncation: profile.truncation_allowed,
                    want_dense: r.include_dense || !r.include_sparse,
                    want_sparse: r.include_sparse && wants_sparse && self.engine.sparse_available(),
                    token_count_hint: 0,
                },
                deadline,
                self.shutdown.child_token(),
            )
            .await
            .map_err(Status::from)?;
        let mut warnings = Vec::new();
        if r.include_sparse && wants_sparse && !self.engine.sparse_available() {
            warnings.push(pb::DiagnosticWarningV005 {
                code: "SPARSE_UNAVAILABLE".into(),
                message: "Sparse output is not available in loaded ONNX artifact".into(),
            });
        }
        let dense = result.dense.as_ref().map(|values| pb::DensePreviewV005 {
            dimension: values.len() as u32,
            norm: values.iter().map(|v| v * v).sum::<f32>().sqrt(),
            preview_values: values.iter().take(8).copied().collect(),
            full_values: if r.include_full_dense {
                values.clone()
            } else {
                Vec::new()
            },
        });
        let sparse = match (&result.sparse_indices, &result.sparse_values) {
            (Some(indices), Some(values)) => {
                let top = if r.top_sparse == 0 {
                    indices.len()
                } else {
                    r.top_sparse as usize
                }
                .min(indices.len());
                Some(pb::SparsePreviewV005 {
                    non_zero_count: indices.len() as u32,
                    indices: indices.iter().take(top).copied().collect(),
                    values: values.iter().take(top).copied().collect(),
                })
            }
            _ => None,
        };
        Ok(Response::new(pb::PreviewEmbeddingResponse {
            model_version: self.cfg.model.version.clone(),
            tokenizer_version: self.cfg.tokenizer.version.clone(),
            dense_version: self.cfg.dense.version.clone(),
            sparse_version: self.cfg.sparse.version.clone(),
            dense,
            sparse,
            tokenization: Some(pb::TokenizationPreviewV005 {
                token_count: result.token_count as u32,
                max_tokens: profile.max_length as u32,
                truncated: result.truncated,
            }),
            warnings,
        }))
    }

    async fn evaluate_relevance(
        &self,
        request: Request<pb::EvaluateRelevanceRequest>,
    ) -> Result<Response<pb::EvaluateRelevanceResponse>, Status> {
        let r = request.into_inner();
        let lexical = crate::relevance::lexical_overlap(&r.question, &r.candidate_text);
        let consistency = if let Some(answer) = &r.answer {
            crate::relevance::lexical_overlap(answer, &r.source_texts.join(" "))
        } else {
            1.0
        };
        let scores = crate::relevance::combine(lexical, lexical, consistency, None);
        Ok(Response::new(pb::EvaluateRelevanceResponse {
            dense_score: scores.dense_score,
            lexical_score: scores.lexical_score,
            consistency_score: scores.consistency_score,
            final_score: scores.final_score,
            decision: scores.decision,
            evaluation_id: Uuid::new_v4().to_string(),
        }))
    }
}
fn granularity_from_str(v: &str) -> i32 {
    match v {
        "SOURCE" => pb::ChunkGranularityV004::SourceV004 as i32,
        "PARENT" => pb::ChunkGranularityV004::ParentV004 as i32,
        "SUB_180" => pb::ChunkGranularityV004::Sub180V004 as i32,
        "SUB_260" => pb::ChunkGranularityV004::Sub260V004 as i32,
        _ => pb::ChunkGranularityV004::Unspecified as i32,
    }
}
fn created_chunk_from_record(c: &crate::persistence::StoredChunkRecord) -> pb::CreatedChunkV004 {
    pb::CreatedChunkV004 {
        chunk_id: c.id.to_string(),
        root_chunk_id: c.root_id.to_string(),
        source_chunk_id: c.source_id.to_string(),
        parent_chunk_id: c.parent_id.map(|id| id.to_string()),
        granularity: granularity_from_str(&c.granularity),
        sequence_no: c.sequence_no as u32,
        token_count: c.token_count as u32,
        content_hash: c.content_hash.clone(),
    }
}
fn chunks_response_from_records(
    stored: Vec<crate::persistence::StoredChunkRecord>,
    status: &str,
) -> pb::CreateMultiGranularityChunksResponse {
    chunks_response_from_records_with_summary(stored, status, 0, 0, 0, 0)
}

fn chunks_response_from_records_with_summary(
    stored: Vec<crate::persistence::StoredChunkRecord>,
    status: &str,
    dense_vectors: u32,
    sparse_vectors: u32,
    bindings: u32,
    outbox_created: u32,
) -> pb::CreateMultiGranularityChunksResponse {
    let root_chunk_id = stored
        .first()
        .map(|c| c.root_id.to_string())
        .unwrap_or_default();
    let parent_chunks = stored
        .iter()
        .filter(|c| c.granularity == "PARENT")
        .map(created_chunk_from_record)
        .collect();
    let sub_chunks_180 = stored
        .iter()
        .filter(|c| c.granularity == "SUB_180")
        .map(created_chunk_from_record)
        .collect();
    let sub_chunks_260 = stored
        .iter()
        .filter(|c| c.granularity == "SUB_260")
        .map(created_chunk_from_record)
        .collect();
    pb::CreateMultiGranularityChunksResponse {
        root_chunk_id,
        parent_chunks,
        sub_chunks_180,
        sub_chunks_260,
        total_chunks: stored.len() as u32,
        status: status.into(),
        summary: Some(pb::IndexSummaryV005 {
            chunks_total: stored.len() as u32,
            source_chunks: stored.iter().filter(|c| c.granularity == "SOURCE").count() as u32,
            parent_chunks: stored.iter().filter(|c| c.granularity == "PARENT").count() as u32,
            sub180_chunks: stored.iter().filter(|c| c.granularity == "SUB_180").count() as u32,
            sub260_chunks: stored.iter().filter(|c| c.granularity == "SUB_260").count() as u32,
            dense_vectors,
            sparse_vectors,
            bindings,
            outbox_created,
            status: status.into(),
        }),
    }
}
fn parent_context_to_pb(parent: ParentContextRecord) -> pb::ChunkContentV004 {
    pb::ChunkContentV004 {
        chunk: Some(pb::CreatedChunkV004 {
            chunk_id: parent.id.to_string(),
            root_chunk_id: parent.root_chunk_id.to_string(),
            source_chunk_id: parent.source_chunk_id.to_string(),
            parent_chunk_id: None,
            granularity: pb::ChunkGranularityV004::ParentV004 as i32,
            sequence_no: 0,
            token_count: parent.token_count as u32,
            content_hash: parent.content_hash,
        }),
        content: parent.content,
        representation_type: pb::SearchRepresentationType::Original as i32,
    }
}
fn chunk_content_to_pb(chunk: ChunkContentRecord) -> pb::ChunkContentV004 {
    pb::ChunkContentV004 {
        chunk: Some(pb::CreatedChunkV004 {
            chunk_id: chunk.id.to_string(),
            root_chunk_id: chunk.root_chunk_id.to_string(),
            source_chunk_id: chunk.source_chunk_id.to_string(),
            parent_chunk_id: chunk.parent_chunk_id.map(|id| id.to_string()),
            granularity: granularity_from_str(&chunk.granularity),
            sequence_no: chunk.sequence_no as u32,
            token_count: chunk.token_count as u32,
            content_hash: chunk.content_hash,
        }),
        content: chunk.content,
        representation_type: pb::SearchRepresentationType::Original as i32,
    }
}

fn explain_candidate(rank: usize, hit: &QdrantSearchHit) -> pb::ExplainCandidateV005 {
    pb::ExplainCandidateV005 {
        rank: (rank + 1) as u32,
        score: hit.score,
        qdrant_point_id: hit.id.to_string(),
        chunk_id: hit
            .payload
            .get("chunk_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        parent_chunk_id: hit
            .payload
            .get("parent_chunk_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        granularity: granularity_from_str(
            hit.payload
                .get("chunk_granularity")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        ),
    }
}

fn normalize_scores_for_fusion(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }
    let min = scores.iter().copied().fold(f32::INFINITY, f32::min);
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !min.is_finite() || !max.is_finite() || (max - min).abs() < f32::EPSILON {
        return vec![1.0; scores.len()];
    }
    scores
        .iter()
        .map(|s| ((*s - min) / (max - min)).clamp(0.0, 1.0))
        .collect()
}

fn fuse_qdrant_hits(
    mut dense_hits: Vec<QdrantSearchHit>,
    mut sparse_hits: Vec<QdrantSearchHit>,
    limit: usize,
    fusion_method: &str,
    dense_weight: f32,
    sparse_weight: f32,
    rrf_k: f32,
) -> Vec<QdrantSearchHit> {
    dense_hits.sort_by(stable_qdrant_hit_rank);
    sparse_hits.sort_by(stable_qdrant_hit_rank);
    let mut by_id: std::collections::HashMap<Uuid, QdrantSearchHit> =
        std::collections::HashMap::new();
    let method = fusion_method.to_ascii_uppercase();
    let use_weighted = method == "WEIGHTED_SCORE" || method == "NORMALIZED_WEIGHTED_SCORE";
    let use_normalized = method == "NORMALIZED_WEIGHTED_SCORE";
    let dense_weight = dense_weight.max(0.0);
    let sparse_weight = sparse_weight.max(0.0);
    let rrf_k = rrf_k.max(1.0);
    let dense_norm = if use_normalized {
        normalize_scores_for_fusion(&dense_hits.iter().map(|h| h.score).collect::<Vec<_>>())
    } else {
        Vec::new()
    };
    let sparse_norm = if use_normalized {
        normalize_scores_for_fusion(&sparse_hits.iter().map(|h| h.score).collect::<Vec<_>>())
    } else {
        Vec::new()
    };

    for (rank, hit) in dense_hits.into_iter().enumerate() {
        let contribution = if use_normalized {
            dense_weight * dense_norm.get(rank).copied().unwrap_or(0.0)
        } else if use_weighted {
            dense_weight * hit.score
        } else {
            1.0_f32 / (rrf_k + rank as f32 + 1.0)
        };
        by_id
            .entry(hit.id)
            .and_modify(|existing| {
                existing.dense_score = hit.score;
                existing.dense_rank = Some((rank + 1) as u32);
                existing.fusion_score += contribution;
                existing.score = existing.fusion_score;
            })
            .or_insert_with(|| QdrantSearchHit {
                id: hit.id,
                score: contribution,
                dense_score: hit.score,
                sparse_score: 0.0,
                fusion_score: contribution,
                dense_rank: Some((rank + 1) as u32),
                sparse_rank: None,
                payload: hit.payload,
            });
    }

    for (rank, hit) in sparse_hits.into_iter().enumerate() {
        let contribution = if use_normalized {
            sparse_weight * sparse_norm.get(rank).copied().unwrap_or(0.0)
        } else if use_weighted {
            sparse_weight * hit.score
        } else {
            1.0_f32 / (rrf_k + rank as f32 + 1.0)
        };
        by_id
            .entry(hit.id)
            .and_modify(|existing| {
                existing.sparse_score = hit.score;
                existing.sparse_rank = Some((rank + 1) as u32);
                existing.fusion_score += contribution;
                existing.score = existing.fusion_score;
            })
            .or_insert_with(|| QdrantSearchHit {
                id: hit.id,
                score: contribution,
                dense_score: 0.0,
                sparse_score: hit.score,
                fusion_score: contribution,
                dense_rank: None,
                sparse_rank: Some((rank + 1) as u32),
                payload: hit.payload,
            });
    }

    metrics::counter!("hybrid_fusion_applied_total", "method" => method.clone()).increment(1);
    let mut hits: Vec<QdrantSearchHit> = by_id.into_values().collect();
    hits.sort_by(stable_qdrant_hit_rank);
    hits.truncate(limit);
    hits
}

fn stable_qdrant_hit_rank(left: &QdrantSearchHit, right: &QdrantSearchHit) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.id.cmp(&right.id))
}

fn effective_retrieve_access_level(
    metadata: &MetadataMap,
    body_value: i32,
) -> Result<pb::AccessLevel, Status> {
    if let Some(level) = access_level_from_metadata(metadata)? {
        return Ok(level);
    }
    let body_level = pb::AccessLevel::try_from(body_value).unwrap_or(pb::AccessLevel::Unspecified);
    if body_level == pb::AccessLevel::Unspecified {
        return Err(Status::permission_denied(
            "ACCESS_LEVEL_REQUIRED: RetrieveContext requires caller access level from trusted metadata or explicit non-UNSPECIFIED request context",
        ));
    }
    Ok(body_level)
}

fn access_level_from_metadata(metadata: &MetadataMap) -> Result<Option<pb::AccessLevel>, Status> {
    let raw = metadata
        .get("x-astravector-access-level")
        .or_else(|| metadata.get("x-astravector-caller-access-level"))
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let Some(raw) = raw else {
        return Ok(None);
    };
    let normalized = raw.trim_start_matches("ACCESS_LEVEL_").to_ascii_uppercase();
    let level = match normalized.as_str() {
        "1" | "PUBLIC" => pb::AccessLevel::Public,
        "2" | "INTERNAL" => pb::AccessLevel::Internal,
        "3" | "CONFIDENTIAL" => pb::AccessLevel::Confidential,
        "4" | "RESTRICTED" => pb::AccessLevel::Restricted,
        "0" | "UNSPECIFIED" => {
            return Err(Status::permission_denied(
                "ACCESS_LEVEL_REQUIRED: metadata access level is UNSPECIFIED",
            ));
        }
        _ => {
            return Err(Status::invalid_argument(format!(
                "INVALID_ACCESS_LEVEL: unsupported x-astravector-access-level value: {raw}"
            )));
        }
    };
    Ok(Some(level))
}

fn reject_unsupported_activation_policy(value: i32) -> Result<(), Status> {
    match pb::ActivationPolicy::try_from(value).unwrap_or(pb::ActivationPolicy::Manual) {
        pb::ActivationPolicy::AutoWhenReady => Err(Status::invalid_argument(
            "UNSUPPORTED_ACTIVATION_POLICY_AUTO_WHEN_READY: AUTO_WHEN_READY requires lifecycle auto-activation worker and is disabled in v007 fix1",
        )),
        _ => Ok(()),
    }
}

fn normalized_access_level(value: i32) -> pb::AccessLevel {
    pb::AccessLevel::try_from(value)
        .ok()
        .filter(|v| *v != pb::AccessLevel::Unspecified)
        .unwrap_or(pb::AccessLevel::Internal)
}

fn normalized_embedding_mode(value: i32) -> pb::EmbeddingModeV005 {
    pb::EmbeddingModeV005::try_from(value)
        .ok()
        .filter(|v| *v != pb::EmbeddingModeV005::Unspecified)
        .unwrap_or(pb::EmbeddingModeV005::DenseSparseRequired)
}

fn normalized_publish_mode(value: i32) -> pb::PublishModeV005 {
    pb::PublishModeV005::try_from(value)
        .ok()
        .filter(|v| *v != pb::PublishModeV005::Unspecified)
        .unwrap_or(pb::PublishModeV005::Outbox)
}

fn resolve_facade_document_id(document: &pb::DocumentIdentity) -> Result<Uuid, Status> {
    if !document.document_id.trim().is_empty() {
        return Uuid::parse_str(document.document_id.trim()).map_err(|_| {
            Status::invalid_argument("document.document_id must be UUID when provided")
        });
    }
    let source = if !document.external_document_id.trim().is_empty() {
        document.external_document_id.trim()
    } else if !document.source_uri.trim().is_empty() {
        document.source_uri.trim()
    } else {
        return Err(Status::invalid_argument(
            "document.document_id or document.external_document_id is required",
        ));
    };
    Ok(Uuid::new_v5(&Uuid::NAMESPACE_URL, source.as_bytes()))
}

fn validate_and_sort_logical_blocks(
    mut blocks: Vec<pb::LogicalBlock>,
) -> Result<Vec<pb::LogicalBlock>, Status> {
    if blocks.is_empty() {
        return Err(Status::invalid_argument(
            "LOGICAL_BLOCKS_EMPTY: blocks must not be empty",
        ));
    }

    let mut by_id: HashMap<String, &pb::LogicalBlock> = HashMap::new();
    let mut root_count = 0_u32;
    for block in &blocks {
        let block_id = block.block_id.trim();
        if block_id.is_empty() {
            return Err(Status::invalid_argument(
                "LOGICAL_BLOCK_ID_REQUIRED: block_id is required for every logical block",
            ));
        }
        if by_id.insert(block_id.to_string(), block).is_some() {
            return Err(Status::invalid_argument(format!(
                "LOGICAL_BLOCK_DUPLICATE_ID: duplicate logical block_id: {block_id}"
            )));
        }
        let block_type =
            pb::BlockType::try_from(block.block_type).unwrap_or(pb::BlockType::Unspecified);
        if block_type == pb::BlockType::Unspecified {
            return Err(Status::invalid_argument(format!(
                "LOGICAL_BLOCK_TYPE_INVALID: block {block_id} has BLOCK_TYPE_UNSPECIFIED"
            )));
        }
        if block_type == pb::BlockType::Document {
            root_count += 1;
        }
        if block.text.trim().is_empty() {
            return Err(Status::invalid_argument(format!(
                "LOGICAL_BLOCK_TEXT_EMPTY: logical block {block_id} has empty text"
            )));
        }
        if block.parent_block_id.trim() == block_id {
            return Err(Status::invalid_argument(format!(
                "LOGICAL_BLOCK_SELF_PARENT: logical block {block_id} cannot reference itself as parent"
            )));
        }
        validate_source_location(block_id, block.source_location.as_ref())?;
        validate_source_links(block_id, &block.source_links)?;
    }

    if root_count != 1 {
        return Err(Status::invalid_argument(format!(
            "LOGICAL_BLOCK_ROOT_INVALID: expected exactly one BLOCK_TYPE_DOCUMENT root, found {root_count}"
        )));
    }

    for block in &blocks {
        let block_id = block.block_id.trim();
        let parent_id = block.parent_block_id.trim();
        if parent_id.is_empty() {
            if pb::BlockType::try_from(block.block_type).unwrap_or(pb::BlockType::Unspecified)
                != pb::BlockType::Document
            {
                return Err(Status::invalid_argument(format!(
                    "LOGICAL_BLOCK_PARENT_REQUIRED: non-root block {block_id} must have parent_block_id"
                )));
            }
            continue;
        }
        let parent = by_id.get(parent_id).ok_or_else(|| {
            Status::invalid_argument(format!(
                "LOGICAL_BLOCK_PARENT_NOT_FOUND: block {block_id} references missing parent_block_id {parent_id}"
            ))
        })?;
        validate_parent_child(block, parent)?;
    }

    for block in &blocks {
        assert_no_logical_block_cycle(block, &by_id)?;
    }

    blocks.sort_by(|a, b| {
        a.order_index
            .cmp(&b.order_index)
            .then_with(|| a.block_id.cmp(&b.block_id))
    });
    Ok(blocks)
}

fn validate_parent_child(
    child: &pb::LogicalBlock,
    parent: &pb::LogicalBlock,
) -> Result<(), Status> {
    let child_type =
        pb::BlockType::try_from(child.block_type).unwrap_or(pb::BlockType::Unspecified);
    let parent_type =
        pb::BlockType::try_from(parent.block_type).unwrap_or(pb::BlockType::Unspecified);
    let allowed = match child_type {
        pb::BlockType::Section => matches!(
            parent_type,
            pb::BlockType::Document | pb::BlockType::Section
        ),
        pb::BlockType::Subsection => matches!(
            parent_type,
            pb::BlockType::Section | pb::BlockType::Subsection
        ),
        pb::BlockType::Paragraph => matches!(
            parent_type,
            pb::BlockType::Document | pb::BlockType::Section | pb::BlockType::Subsection
        ),
        pb::BlockType::Table => matches!(
            parent_type,
            pb::BlockType::Document | pb::BlockType::Section | pb::BlockType::Subsection
        ),
        pb::BlockType::TableRow => parent_type == pb::BlockType::Table,
        pb::BlockType::List => matches!(
            parent_type,
            pb::BlockType::Document
                | pb::BlockType::Section
                | pb::BlockType::Subsection
                | pb::BlockType::Paragraph
        ),
        pb::BlockType::ListItem => parent_type == pb::BlockType::List,
        pb::BlockType::FaqItem => matches!(
            parent_type,
            pb::BlockType::Document | pb::BlockType::Section | pb::BlockType::Subsection
        ),
        pb::BlockType::CodeBlock => matches!(
            parent_type,
            pb::BlockType::Document
                | pb::BlockType::Section
                | pb::BlockType::Subsection
                | pb::BlockType::Paragraph
        ),
        pb::BlockType::Caption => matches!(
            parent_type,
            pb::BlockType::Table | pb::BlockType::Section | pb::BlockType::Subsection
        ),
        pb::BlockType::Document => child.parent_block_id.trim().is_empty(),
        pb::BlockType::Unspecified => false,
    };
    if !allowed {
        return Err(Status::invalid_argument(format!(
            "LOGICAL_BLOCK_PARENT_CHILD_INVALID: block {} type {:?} cannot have parent {} type {:?}",
            child.block_id, child_type, parent.block_id, parent_type
        )));
    }
    Ok(())
}

fn assert_no_logical_block_cycle(
    block: &pb::LogicalBlock,
    by_id: &HashMap<String, &pb::LogicalBlock>,
) -> Result<(), Status> {
    let mut visited = HashSet::new();
    let mut current = block;
    while !current.parent_block_id.trim().is_empty() {
        let current_id = current.block_id.trim().to_string();
        if !visited.insert(current_id.clone()) {
            return Err(Status::invalid_argument(format!(
                "LOGICAL_BLOCK_TREE_CYCLE: cycle detected at block {current_id}"
            )));
        }
        current = by_id.get(current.parent_block_id.trim()).ok_or_else(|| {
            Status::invalid_argument(format!(
                "LOGICAL_BLOCK_PARENT_NOT_FOUND: block {} references missing parent_block_id {}",
                current.block_id, current.parent_block_id
            ))
        })?;
    }
    Ok(())
}

fn validate_source_location(
    block_id: &str,
    location: Option<&pb::SourceLocation>,
) -> Result<(), Status> {
    let Some(location) = location else {
        return Ok(());
    };
    if location.page_end > 0 && location.page_start > 0 && location.page_end < location.page_start {
        return Err(Status::invalid_argument(format!(
            "SOURCE_LOCATION_INVALID: block {block_id} has page_end < page_start"
        )));
    }
    if location.char_end > 0 && location.char_start > 0 && location.char_end < location.char_start {
        return Err(Status::invalid_argument(format!(
            "SOURCE_LOCATION_INVALID: block {block_id} has char_end < char_start"
        )));
    }
    Ok(())
}

fn validate_source_links(owner: &str, links: &[pb::SourceLink]) -> Result<(), Status> {
    for link in links {
        let link_type =
            pb::SourceLinkType::try_from(link.r#type).unwrap_or(pb::SourceLinkType::Unspecified);
        if link_type == pb::SourceLinkType::Unspecified {
            return Err(Status::invalid_argument(format!(
                "SOURCE_LINK_TYPE_INVALID: {owner} contains SOURCE_LINK_TYPE_UNSPECIFIED"
            )));
        }
        let url = link.url.trim();
        if url.is_empty() {
            return Err(Status::invalid_argument(format!(
                "SOURCE_LINK_URL_REQUIRED: {owner} contains source link without url"
            )));
        }
        let lower = url.to_ascii_lowercase();
        if lower.starts_with("javascript:")
            || lower.starts_with("file:")
            || lower.starts_with("data:")
        {
            return Err(Status::invalid_argument(format!(
                "SOURCE_LINK_INVALID_SCHEME: {owner} contains unsafe source link scheme"
            )));
        }
        let allowed = lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("s3://")
            || lower.starts_with("minio://")
            || lower.starts_with("internal://");
        if !allowed {
            return Err(Status::invalid_argument(format!(
                "SOURCE_LINK_INVALID_SCHEME: {owner} contains unsupported source link scheme"
            )));
        }
        if url.len() > 4096 {
            return Err(Status::invalid_argument(format!(
                "SOURCE_LINK_TOO_LONG: {owner} contains source link longer than 4096 characters"
            )));
        }
        if link.label.len() > 512 {
            return Err(Status::invalid_argument(format!(
                "SOURCE_LINK_LABEL_TOO_LONG: {owner} contains source link label longer than 512 characters"
            )));
        }
        if link_type == pb::SourceLinkType::Download
            && (lower.contains("token=")
                || lower.contains("signature=")
                || lower.contains("x-amz-signature"))
            && link.expires_at.trim().is_empty()
        {
            return Err(Status::invalid_argument(format!(
                "SOURCE_LINK_EXPIRES_AT_REQUIRED: {owner} contains signed download link without expires_at"
            )));
        }
    }
    Ok(())
}

fn render_logical_blocks_for_chunking(blocks: &[pb::LogicalBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let Some(location) = block.source_location.as_ref() {
            if !location.heading.trim().is_empty() {
                out.push_str(location.heading.trim());
                out.push('\n');
            }
        }
        out.push_str(block.text.trim());
        out.push_str("\n\n");
    }
    out
}

fn annotated_segments_from_metadata(
    metadata: &std::collections::HashMap<String, String>,
) -> Result<Vec<crate::chunking::AnnotatedTextSegment>, Status> {
    let Some(raw) = metadata.get("logical_blocks") else {
        return Ok(Vec::new());
    };
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        Status::invalid_argument(format!("logical_blocks metadata is not valid JSON: {e}"))
    })?;
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in items {
        let block_id = item
            .get("block_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if block_id.is_empty() {
            continue;
        }
        let block_type = item
            .get("block_type_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| block_type_name_from_json(item.get("block_type")).to_string());
        let parent_block_id = item
            .get("parent_block_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let text = item
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        out.push(crate::chunking::AnnotatedTextSegment {
            block_id,
            parent_block_id,
            block_type,
            text,
            source_location: item
                .get("source_location")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            source_links: item
                .get("source_links")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            metadata: item
                .get("metadata")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            order_index: item
                .get("order_index")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32,
        });
    }
    out.sort_by_key(|s| s.order_index);
    Ok(out)
}

fn split_annotated_segments_for_model<Count, Offsets>(
    segments: Vec<AnnotatedTextSegment>,
    max_model_tokens: usize,
    count_tokens: Count,
    token_offsets: Offsets,
) -> Result<Vec<AnnotatedTextSegment>, Status>
where
    Count: Fn(&str, usize) -> Result<usize, AstraError>,
    Offsets: Fn(&str) -> Result<Vec<crate::tokenizer::TokenOffset>, AstraError>,
{
    if max_model_tokens == 0 {
        return Err(Status::failed_precondition(
            "tokenization.child.max_length must be greater than zero",
        ));
    }
    let mut output = Vec::with_capacity(segments.len());
    for segment in segments {
        match count_tokens(&segment.text, max_model_tokens) {
            Ok(_) => output.push(segment),
            Err(AstraError::OutOfRange(_)) => {
                let offsets = token_offsets(&segment.text).map_err(Status::from)?;
                if offsets.is_empty() {
                    return Err(Status::out_of_range(format!(
                        "logical block {} exceeds model limit but has no token boundaries",
                        segment.block_id
                    )));
                }
                let mut start = 0_usize;
                let mut pieces = Vec::new();
                while start < segment.text.len() {
                    let mut last_fitting_end = None;
                    let mut last_natural_end = None;
                    for offset in offsets.iter().filter(|offset| offset.end_byte > start) {
                        let candidate = segment.text[start..offset.end_byte].trim();
                        if candidate.is_empty() {
                            continue;
                        }
                        match count_tokens(candidate, max_model_tokens) {
                            Ok(_) => {
                                last_fitting_end = Some(offset.end_byte);
                                if is_natural_model_split_boundary(&segment.text, offset.end_byte) {
                                    last_natural_end = Some(offset.end_byte);
                                }
                            }
                            Err(AstraError::OutOfRange(_)) => break,
                            Err(error) => return Err(Status::from(error)),
                        }
                    }
                    let end = last_natural_end.or(last_fitting_end).ok_or_else(|| {
                        Status::out_of_range(format!(
                            "logical block {} contains a token sequence exceeding model limit={max_model_tokens}",
                            segment.block_id
                        ))
                    })?;
                    let text = segment.text[start..end].trim();
                    if text.is_empty() {
                        return Err(Status::internal(
                            "model-aware logical block splitter made no progress",
                        ));
                    }
                    let mut derived = segment.clone();
                    derived.text = text.to_string();
                    pieces.push(derived);
                    start = end;
                }
                let piece_count = pieces.len();
                for (piece_index, mut piece) in pieces.into_iter().enumerate() {
                    if !piece.metadata.is_object() {
                        piece.metadata = serde_json::json!({});
                    }
                    if let Some(metadata) = piece.metadata.as_object_mut() {
                        metadata
                            .insert("model_segment_index".into(), serde_json::json!(piece_index));
                        metadata
                            .insert("model_segment_count".into(), serde_json::json!(piece_count));
                        metadata.insert(
                            "model_segmentation_reason".into(),
                            serde_json::json!("TOKENIZER_LIMIT"),
                        );
                    }
                    output.push(piece);
                }
            }
            Err(error) => return Err(Status::from(error)),
        }
    }
    Ok(output)
}

fn is_natural_model_split_boundary(text: &str, end: usize) -> bool {
    text[end..].chars().next().is_none_or(char::is_whitespace)
        || text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | '!' | '?' | ';' | ':' | ','))
}

#[cfg(test)]
mod model_aware_ingestion_tests {
    use super::*;

    fn segment(text: &str) -> AnnotatedTextSegment {
        AnnotatedTextSegment {
            block_id: "block-a".into(),
            parent_block_id: None,
            block_type: "PARAGRAPH".into(),
            text: text.into(),
            source_location: serde_json::json!({}),
            source_links: serde_json::json!([]),
            metadata: serde_json::json!({}),
            order_index: 1,
        }
    }

    #[test]
    fn oversized_logical_block_is_split_at_canonical_token_boundaries() {
        let parts = split_annotated_segments_for_model(
            vec![segment("aa bb cc dd")],
            2,
            |text, max| {
                let count = text.split_whitespace().count();
                if count > max {
                    Err(AstraError::OutOfRange(format!("{count} > {max}")))
                } else {
                    Ok(count)
                }
            },
            |text| {
                Ok(text
                    .split_whitespace()
                    .scan(0_usize, |start, word| {
                        let offset = text[*start..].find(word)? + *start;
                        *start = offset + word.len();
                        Some(crate::tokenizer::TokenOffset {
                            token_index: offset,
                            start_byte: offset,
                            end_byte: offset + word.len(),
                        })
                    })
                    .collect())
            },
        )
        .expect("split oversized block");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].text, "aa bb");
        assert_eq!(parts[1].text, "cc dd");
        assert!(parts.iter().all(|part| part.block_id == "block-a"));
        assert_eq!(parts[0].metadata["model_segment_index"], 0);
        assert_eq!(parts[1].metadata["model_segment_index"], 1);
        assert_eq!(parts[0].metadata["model_segment_count"], 2);
        assert_eq!(
            parts
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            "aa bb cc dd"
        );
    }
}

fn block_type_name_from_json(value: Option<&serde_json::Value>) -> &'static str {
    let Some(value) = value else {
        return "UNSPECIFIED";
    };
    let code = value.as_i64().unwrap_or(0);
    match pb::BlockType::try_from(code as i32).unwrap_or(pb::BlockType::Unspecified) {
        pb::BlockType::Document => "DOCUMENT",
        pb::BlockType::Section => "SECTION",
        pb::BlockType::Subsection => "SUBSECTION",
        pb::BlockType::Paragraph => "PARAGRAPH",
        pb::BlockType::Table => "TABLE",
        pb::BlockType::TableRow => "TABLE_ROW",
        pb::BlockType::List => "LIST",
        pb::BlockType::ListItem => "LIST_ITEM",
        pb::BlockType::FaqItem => "FAQ_ITEM",
        pb::BlockType::CodeBlock => "CODE_BLOCK",
        pb::BlockType::Caption => "CAPTION",
        pb::BlockType::Unspecified => "UNSPECIFIED",
    }
}

fn normalized_content_hash(user_hash: &str, source_text: &str) -> Result<String, Status> {
    let candidate = user_hash
        .trim()
        .trim_start_matches("sha256:")
        .to_ascii_lowercase();
    if candidate.is_empty() {
        return Ok(hash_text(source_text));
    }
    if candidate.len() == 64 && candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(candidate)
    } else {
        Err(Status::invalid_argument(
            "document.content_hash must be a 64-character sha256 hex string or empty",
        ))
    }
}

fn activation_policy_as_str(value: i32) -> &'static str {
    match pb::ActivationPolicy::try_from(value).unwrap_or(pb::ActivationPolicy::Manual) {
        pb::ActivationPolicy::AutoWhenReady => "AUTO_WHEN_READY",
        pb::ActivationPolicy::Skip => "SKIP",
        _ => "MANUAL",
    }
}

fn attach_document_metadata(
    metadata: &mut std::collections::HashMap<String, String>,
    document: &pb::DocumentIdentity,
) {
    for (k, v) in [
        (
            "external_document_id",
            document.external_document_id.as_str(),
        ),
        ("document_title", document.title.as_str()),
        ("source_uri", document.source_uri.as_str()),
        ("source_type", document.source_type.as_str()),
        ("mime_type", document.mime_type.as_str()),
    ] {
        if !v.trim().is_empty() {
            metadata.insert(k.into(), v.into());
        }
    }
    if !document.source_links.is_empty() {
        if let Ok(json) = serde_json::to_string(&source_links_as_json(&document.source_links)) {
            metadata.insert("document_source_links".into(), json);
        }
    }
}

fn attach_logical_block_metadata(
    metadata: &mut std::collections::HashMap<String, String>,
    blocks: &[pb::LogicalBlock],
) -> Result<(), Status> {
    let block_refs = blocks
        .iter()
        .map(|b| {
            serde_json::json!({
                "block_id": b.block_id,
                "parent_block_id": b.parent_block_id,
                "block_type": b.block_type,
                "block_type_name": block_type_name_for_proto(b.block_type),
                "text": b.text,
                "order_index": b.order_index,
                "source_location": b.source_location.as_ref().map(source_location_as_json),
                "source_links": source_links_as_json(&b.source_links),
                "metadata": b.metadata,
            })
        })
        .collect::<Vec<_>>();
    let json = serde_json::to_string(&block_refs)
        .map_err(|e| Status::internal(format!("logical block metadata serialization: {e}")))?;
    metadata.insert("logical_blocks".into(), json);
    Ok(())
}

fn block_type_name_for_proto(value: i32) -> &'static str {
    match pb::BlockType::try_from(value).unwrap_or(pb::BlockType::Unspecified) {
        pb::BlockType::Document => "DOCUMENT",
        pb::BlockType::Section => "SECTION",
        pb::BlockType::Subsection => "SUBSECTION",
        pb::BlockType::Paragraph => "PARAGRAPH",
        pb::BlockType::Table => "TABLE",
        pb::BlockType::TableRow => "TABLE_ROW",
        pb::BlockType::List => "LIST",
        pb::BlockType::ListItem => "LIST_ITEM",
        pb::BlockType::FaqItem => "FAQ_ITEM",
        pb::BlockType::CodeBlock => "CODE_BLOCK",
        pb::BlockType::Caption => "CAPTION",
        pb::BlockType::Unspecified => "UNSPECIFIED",
    }
}

fn source_location_as_json(location: &pb::SourceLocation) -> serde_json::Value {
    serde_json::json!({
        "page_start": location.page_start,
        "page_end": location.page_end,
        "char_start": location.char_start,
        "char_end": location.char_end,
        "section_path": location.section_path,
        "heading": location.heading,
        "table_id": location.table_id,
        "row_index": location.row_index,
        "column_index": location.column_index,
    })
}

fn source_links_as_json(links: &[pb::SourceLink]) -> Vec<serde_json::Value> {
    links
        .iter()
        .map(|l| {
            serde_json::json!({
                "type": l.r#type,
                "url": &l.url,
                "label": &l.label,
                "mime_type": &l.mime_type,
                "requires_auth": l.requires_auth,
                "expires_at": &l.expires_at,
                "attributes": &l.attributes,
            })
        })
        .collect()
}

fn ttl_days_from_policy(policy: Option<&pb::TtlPolicy>) -> Result<Option<u32>, Status> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    match pb::TtlMode::try_from(policy.mode).unwrap_or(pb::TtlMode::None) {
        pb::TtlMode::None | pb::TtlMode::Unspecified => Ok(None),
        pb::TtlMode::Relative => {
            if policy.ttl_seconds == 0 {
                return Err(Status::invalid_argument(
                    "ttl_seconds must be > 0 for TTL_MODE_RELATIVE",
                ));
            }
            let days = policy.ttl_seconds.div_ceil(86_400).min(u32::MAX as u64) as u32;
            Ok(Some(days.max(1)))
        }
        pb::TtlMode::Absolute => {
            if policy.expires_at.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "expires_at is required for TTL_MODE_ABSOLUTE",
                ));
            }
            Err(Status::invalid_argument(
                "UNSUPPORTED_TTL_MODE_ABSOLUTE: TTL_MODE_ABSOLUTE is not enabled in v007 fix1; use TTL_MODE_RELATIVE or TTL_MODE_NONE",
            ))
        }
    }
}

fn chunking_profile_v004_from_v007(
    input: Option<&pb::TokenAwareChunkingOptions>,
) -> pb::ChunkingProfileV004 {
    let profile = input
        .map(|i| pb::ChunkingProfile::try_from(i.profile).unwrap_or(pb::ChunkingProfile::Default))
        .unwrap_or(pb::ChunkingProfile::Default);
    let (parent_target, parent_max, child_target, child_max, overlap, version) = match profile {
        pb::ChunkingProfile::Legal => (1200, 1400, 220, 280, 60, "v007-legal-token-aware"),
        pb::ChunkingProfile::Technical => (1000, 1200, 260, 340, 50, "v007-technical-token-aware"),
        pb::ChunkingProfile::Faq => (500, 700, 180, 240, 20, "v007-faq-token-aware"),
        pb::ChunkingProfile::TableHeavy => {
            (800, 1000, 200, 260, 30, "v007-table-heavy-token-aware")
        }
        _ => (900, 1100, 260, 320, 40, "v007-default-token-aware"),
    };
    let parent = input
        .and_then(|i| optional_size(i.parent_target_tokens, i.parent_max_tokens, 0))
        .unwrap_or((parent_target, 1, parent_max, 0));
    let child = input
        .and_then(|i| {
            optional_size(
                i.child_target_tokens,
                i.child_max_tokens,
                i.child_overlap_tokens,
            )
        })
        .unwrap_or((child_target, 1, child_max, overlap));
    pb::ChunkingProfileV004 {
        parent: Some(pb::ChunkSizeProfileV004 {
            granularity: pb::ChunkGranularityV004::ParentV004 as i32,
            target_tokens: parent.0,
            min_tokens: parent.1,
            max_tokens: parent.2,
            overlap_tokens: parent.3,
        }),
        granularities: vec![
            pb::ChunkSizeProfileV004 {
                granularity: pb::ChunkGranularityV004::Sub180V004 as i32,
                target_tokens: child.0.min(220),
                min_tokens: child.1,
                max_tokens: child.2.min(280),
                overlap_tokens: child.3,
            },
            pb::ChunkSizeProfileV004 {
                granularity: pb::ChunkGranularityV004::Sub260V004 as i32,
                target_tokens: child.0,
                min_tokens: child.1,
                max_tokens: child.2,
                overlap_tokens: child.3,
            },
        ],
        preserve_headings: input.map(|i| i.preserve_block_boundaries).unwrap_or(true),
        preserve_paragraphs: !input
            .map(|i| i.allow_split_inside_paragraph)
            .unwrap_or(true),
        preserve_sentences: true,
        profile_version: version.into(),
    }
}

fn optional_size(target: u32, max: u32, overlap: u32) -> Option<(u32, u32, u32, u32)> {
    if target == 0 && max == 0 {
        return None;
    }
    let target = if target == 0 { max } else { target };
    let max = if max == 0 { target } else { max };
    Some((
        target,
        1,
        max.max(target),
        overlap.min(target.saturating_sub(1)),
    ))
}

fn start_ingestion_fingerprint(
    req: &pb::StartLogicalDocumentIngestionRequest,
    access_zone_id: Uuid,
    document_id: Uuid,
    idempotency_key: &str,
) -> Result<String, Status> {
    let mut metadata_pairs = req.metadata.iter().collect::<Vec<_>>();
    metadata_pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut hasher = Sha256::new();
    hasher.update(access_zone_id.as_bytes());
    hasher.update(document_id.as_bytes());
    hasher.update(req.document_version.to_le_bytes());
    hasher.update(req.content_hash.as_bytes());
    hasher.update(req.source_uri.as_bytes());
    hasher.update(req.file_name.as_bytes());
    hasher.update(req.ttl_days.to_le_bytes());
    hasher.update(idempotency_key.as_bytes());
    for (k, v) in metadata_pairs {
        hasher.update(k.as_bytes());
        hasher.update([0]);
        hasher.update(v.as_bytes());
        hasher.update([0xff]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn logical_block_size_bytes(block: &pb::LogicalBlock) -> usize {
    block.block_id.len()
        + block.parent_block_id.len()
        + block.text.len()
        + block
            .metadata
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
}

fn logical_block_to_json(block: &pb::LogicalBlock) -> serde_json::Value {
    let source_location = block
        .source_location
        .as_ref()
        .map(|s| {
            serde_json::json!({
                "page_start": s.page_start,
                "page_end": s.page_end,
                "char_start": s.char_start,
                "char_end": s.char_end,
                "section_path": &s.section_path,
                "heading": &s.heading,
                "table_id": &s.table_id,
                "row_index": s.row_index,
                "column_index": s.column_index,
            })
        })
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    let source_links = serde_json::Value::Array(
        block
            .source_links
            .iter()
            .map(|l| {
                serde_json::json!({
                    "type": l.r#type,
                    "url": &l.url,
                    "label": &l.label,
                    "mime_type": &l.mime_type,
                    "requires_auth": l.requires_auth,
                    "expires_at": &l.expires_at,
                    "attributes": &l.attributes,
                })
            })
            .collect(),
    );
    serde_json::json!({
        "block_id": &block.block_id,
        "parent_block_id": &block.parent_block_id,
        "block_type": block.block_type,
        "text": &block.text,
        "order_index": block.order_index,
        "metadata": &block.metadata,
        "source_location": source_location,
        "source_links": source_links,
    })
}

fn normalize_sha256_hex(raw: &str) -> Result<String, ()> {
    let candidate = raw
        .trim()
        .trim_start_matches("sha256:")
        .to_ascii_lowercase();
    if candidate.len() == 64 && candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(candidate)
    } else {
        Err(())
    }
}

fn compute_batch_content_hash(blocks: &[pb::LogicalBlock]) -> Result<String, serde_json::Error> {
    // fix4.5.2: server-side canonical batch hash. serde_json::json! uses a stable object shape
    // because logical_block_to_json constructs fields in a fixed order.
    let values: Vec<serde_json::Value> = blocks.iter().map(logical_block_to_json).collect();
    let bytes = serde_json::to_vec(&values)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn logical_block_from_json(value: &serde_json::Value) -> Result<pb::LogicalBlock, Status> {
    let obj = value.as_object().ok_or_else(|| {
        Status::data_loss("INGESTION_STAGING_CORRUPTED: block_json is not object")
    })?;
    let metadata = obj
        .get("metadata")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let source_location = obj
        .get("source_location")
        .and_then(|v| v.as_object())
        .map(|m| pb::SourceLocation {
            page_start: m
                .get("page_start")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            page_end: m
                .get("page_end")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            char_start: m
                .get("char_start")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            char_end: m
                .get("char_end")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            section_path: m
                .get("section_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            heading: m
                .get("heading")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            table_id: m
                .get("table_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            row_index: m
                .get("row_index")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            column_index: m
                .get("column_index")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
        });
    let source_links = obj
        .get("source_links")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_object())
                .map(|m| pb::SourceLink {
                    r#type: m.get("type").and_then(|v| v.as_i64()).unwrap_or_default() as i32,
                    url: m
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    label: m
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    mime_type: m
                        .get("mime_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    requires_auth: m
                        .get("requires_auth")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    expires_at: m
                        .get("expires_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    attributes: m
                        .get("attributes")
                        .and_then(|v| v.as_object())
                        .map(|mm| {
                            mm.iter()
                                .map(|(k, v)| {
                                    (k.clone(), v.as_str().unwrap_or_default().to_string())
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(pb::LogicalBlock {
        block_id: obj
            .get("block_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        parent_block_id: obj
            .get("parent_block_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        block_type: obj
            .get("block_type")
            .and_then(|v| v.as_i64())
            .unwrap_or_default() as i32,
        text: obj
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        order_index: obj
            .get("order_index")
            .and_then(|v| v.as_u64())
            .unwrap_or_default() as u32,
        source_location,
        source_links,
        metadata,
    })
}

fn index_logical_document_response_to_json(
    response: &pb::IndexLogicalDocumentResponse,
) -> serde_json::Value {
    serde_json::json!({
        "document": response.document.as_ref().map(|d| serde_json::json!({"access_zone_id": &d.access_zone_id, "document_id": &d.document_id, "document_version": d.document_version})),
        "operation": response.operation.as_ref().map(|o| serde_json::json!({"operation_id": &o.operation_id, "state": o.state, "message": &o.message})),
        "summary": response.summary.as_ref().map(|s| serde_json::json!({
            "blocks_received": s.blocks_received,
            "blocks_accepted": s.blocks_accepted,
            "blocks_rejected": s.blocks_rejected,
            "chunks_created": s.chunks_created,
            "parent_chunks_created": s.parent_chunks_created,
            "child_chunks_created": s.child_chunks_created,
            "atomic_chunks_created": s.atomic_chunks_created,
            "dense_vectors_created": s.dense_vectors_created,
            "sparse_vectors_created": s.sparse_vectors_created,
            "qdrant_points_scheduled": s.qdrant_points_scheduled,
            "already_indexed": s.already_indexed,
        })),
    })
}

fn index_logical_document_response_from_json(
    value: serde_json::Value,
) -> Result<pb::IndexLogicalDocumentResponse, Status> {
    let obj = value
        .as_object()
        .ok_or_else(|| Status::data_loss("stored finalize response is not object"))?;
    let document = obj
        .get("document")
        .and_then(|v| v.as_object())
        .map(|d| pb::DocumentRef {
            access_zone_id: d
                .get("access_zone_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            document_id: d
                .get("document_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            document_version: d
                .get("document_version")
                .and_then(|v| v.as_u64())
                .unwrap_or_default(),
        });
    let operation = obj
        .get("operation")
        .and_then(|v| v.as_object())
        .map(|o| pb::OperationStatus {
            operation_id: o
                .get("operation_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            state: o.get("state").and_then(|v| v.as_i64()).unwrap_or_default() as i32,
            message: o
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            warnings: Vec::new(),
            errors: Vec::new(),
        });
    let summary = obj
        .get("summary")
        .and_then(|v| v.as_object())
        .map(|s| pb::IndexingSummary {
            blocks_received: s
                .get("blocks_received")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            blocks_accepted: s
                .get("blocks_accepted")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            blocks_rejected: s
                .get("blocks_rejected")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            chunks_created: s
                .get("chunks_created")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            parent_chunks_created: s
                .get("parent_chunks_created")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            child_chunks_created: s
                .get("child_chunks_created")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            atomic_chunks_created: s
                .get("atomic_chunks_created")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            dense_vectors_created: s
                .get("dense_vectors_created")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            sparse_vectors_created: s
                .get("sparse_vectors_created")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            qdrant_points_scheduled: s
                .get("qdrant_points_scheduled")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            already_indexed: s
                .get("already_indexed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        });
    Ok(pb::IndexLogicalDocumentResponse {
        document,
        operation,
        summary,
    })
}

fn graph_build_limits_from_config(cfg: &AppConfig) -> crate::graph::GraphBuildLimits {
    crate::graph::GraphBuildLimits {
        max_document_graph_nodes: cfg.graph_rag.build.max_document_graph_nodes,
        max_document_graph_edges: cfg.graph_rag.build.max_document_graph_edges,
        max_block_nodes: cfg.graph_rag.build.max_block_nodes,
        max_chunk_nodes: cfg.graph_rag.build.max_chunk_nodes,
        max_children_per_block: cfg.graph_rag.build.max_children_per_block,
        max_same_parent_edges: cfg.graph_rag.build.max_same_parent_edges,
        max_same_table_edges: cfg.graph_rag.build.max_same_table_edges,
        semantic_edges_enabled: cfg.graph_rag.build.semantic_edges_enabled,
        semantic_top_k_per_chunk: cfg.graph_rag.build.semantic_top_k_per_chunk,
        semantic_min_score: cfg.graph_rag.build.semantic_min_score,
        semantic_max_edges_per_document: cfg.graph_rag.build.semantic_max_edges_per_document,
        semantic_max_chunks_for_in_memory: cfg.graph_rag.build.semantic_max_chunks_for_in_memory,
        semantic_large_document_policy: cfg.graph_rag.build.semantic_large_document_policy.clone(),
        semantic_normalize_embeddings: cfg.graph_rag.build.semantic_normalize_embeddings,
        semantic_parallel_enabled: cfg.graph_rag.build.semantic_parallel_enabled,
        semantic_parallelism: cfg.graph_rag.build.semantic_parallelism,
        semantic_warn_build_time_ms: cfg.graph_rag.build.semantic_warn_build_time_ms,
        semantic_rebuild_timeout_ms: cfg.graph_rag.build.semantic_rebuild_timeout_ms,
    }
}

fn graph_scoring_options_from_config(cfg: &AppConfig) -> crate::graph::GraphScoringOptions {
    crate::graph::GraphScoringOptions {
        relation_weights: cfg.graph_rag.scoring.relation_weights.clone(),
        default_structural_relation_weight: cfg
            .graph_rag
            .scoring
            .default_structural_relation_weight,
        default_semantic_relation_weight: cfg.graph_rag.scoring.default_semantic_relation_weight,
        graph_hop_penalty: cfg.graph_rag.scoring.graph_hop_penalty.clone(),
        graph_min_score: cfg.graph_rag.scoring.graph_min_score,
        structural_seed_score_floor: cfg.graph_rag.scoring.structural_seed_score_floor,
        semantic_power: cfg.graph_rag.scoring.semantic_power,
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

fn retrieved_context_from_search_result(result: pb::SearchResultV004) -> pb::RetrievedContext {
    let mut metadata = result
        .citation
        .as_ref()
        .map(|c| c.metadata.clone())
        .unwrap_or_default();
    // fix463: RetrieveContext response must preserve tenant/zone lineage even if
    // the generated proto consumer only inspects metadata/citation.
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
    let source_links = source_links_from_metadata(&metadata);
    let citation = pb::Citation {
        document_id: result.document_id.clone(),
        document_version: result.document_version,
        source_uri: metadata.get("source_uri").cloned().unwrap_or_default(),
        title: metadata.get("document_title").cloned().unwrap_or_default(),
        page_start: metadata
            .get("page_start")
            .and_then(|v| v.parse().ok())
            .unwrap_or_default(),
        page_end: metadata
            .get("page_end")
            .and_then(|v| v.parse().ok())
            .unwrap_or_default(),
        section_path: metadata.get("section_path").cloned().unwrap_or_default(),
        heading: metadata.get("heading").cloned().unwrap_or_default(),
        matched_chunk_id: result.matched_chunk_id.clone(),
        parent_chunk_id: result.parent_chunk_id.clone(),
        source_block_id: metadata.get("source_block_id").cloned().unwrap_or_default(),
    };
    let scores = result.scores.unwrap_or_default();
    pb::RetrievedContext {
        matched_text: result.matched_text,
        parent_text: result.parent_text,
        citation: Some(citation),
        scores: Some(pb::Scores {
            dense_score: scores.dense_score,
            sparse_score: scores.sparse_score,
            fusion_score: scores.fusion_score,
            final_score: scores.final_score,
        }),
        document_id: result.document_id,
        document_version: result.document_version,
        access_zone_id: metadata.get("access_zone_id").cloned().unwrap_or_default(),
        source_block_id: metadata.get("source_block_id").cloned().unwrap_or_default(),
        matched_chunk_id: result.matched_chunk_id,
        parent_chunk_id: result.parent_chunk_id,
        source_links,
        metadata,
    }
}

fn source_links_from_metadata(
    metadata: &std::collections::HashMap<String, String>,
) -> Vec<pb::SourceLink> {
    let mut links = Vec::new();
    for (key, link_type, label) in [
        (
            "preview_url",
            pb::SourceLinkType::Preview,
            "Предпросмотр документа",
        ),
        (
            "download_url",
            pb::SourceLinkType::Download,
            "Скачать документ",
        ),
        (
            "source_url",
            pb::SourceLinkType::OriginalDocument,
            "Открыть оригинальный документ",
        ),
        (
            "source_uri",
            pb::SourceLinkType::OriginalDocument,
            "Источник документа",
        ),
        ("page_url", pb::SourceLinkType::Page, "Открыть страницу"),
        ("section_url", pb::SourceLinkType::Section, "Открыть раздел"),
    ] {
        if let Some(url) = metadata.get(key).filter(|v| !v.trim().is_empty()) {
            links.push(pb::SourceLink {
                r#type: link_type as i32,
                url: url.clone(),
                label: label.into(),
                mime_type: metadata.get("mime_type").cloned().unwrap_or_default(),
                requires_auth: true,
                expires_at: metadata.get("expires_at").cloned().unwrap_or_default(),
                attributes: metadata.clone(),
            });
        }
    }
    for raw_key in ["matched_source_links", "document_source_links"] {
        if let Some(raw) = metadata.get(raw_key) {
            if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(raw) {
                for v in values {
                    if let Some(url) = v.get("url").and_then(serde_json::Value::as_str) {
                        links.push(pb::SourceLink {
                            r#type: v
                                .get("type")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0) as i32,
                            url: url.into(),
                            label: v
                                .get("label")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Источник")
                                .into(),
                            mime_type: v
                                .get("mime_type")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .into(),
                            requires_auth: v
                                .get("requires_auth")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(true),
                            expires_at: v
                                .get("expires_at")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .into(),
                            attributes: std::collections::HashMap::new(),
                        });
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    links.retain(|l| seen.insert(format!("{}|{}", l.r#type, l.url)));
    links
}

fn document_status_message(sync: &pb::GetVectorSyncStatusResponse) -> String {
    if sync.ready_to_activate {
        "Document vectors are synchronized and ready to activate".into()
    } else if sync.failed_bindings > 0 || sync.outbox_failed > 0 {
        "Document vector synchronization has failed bindings or outbox failures".into()
    } else if sync.outbox_pending > 0 || sync.outbox_retry_pending > 0 {
        "Document vectors are waiting for Qdrant publisher".into()
    } else {
        "Document vectors are being synchronized".into()
    }
}

async fn schedule_v004_delete_document_vectors(
    repo: &Repository,
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
) -> Result<u64, Status> {
    let mut tx = repo
        .pool
        .begin()
        .await
        .map_err(|e| Status::unavailable(format!("postgres: {e}")))?;
    let rows = sqlx::query(
        "WITH candidates AS (
             SELECT id
             FROM astravector.vector_bindings_v004
             WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3
               AND legal_hold=false
               AND qdrant_sync_status NOT IN('DELETE_PENDING','DELETE_IN_PROGRESS','DELETED')
             FOR UPDATE
         )
         UPDATE astravector.vector_bindings_v004 b
         SET ttl_generation=b.ttl_generation+1,
             lifecycle_status='DELETION_PENDING',
             qdrant_sync_status='DELETE_PENDING',
             updated_at=now()
         FROM candidates c
         WHERE b.access_zone_id=$1 AND b.id=c.id
         RETURNING b.id,b.ttl_generation",
    )
    .bind(access_zone_id)
    .bind(document_id)
    .bind(document_version)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| Status::unavailable(format!("postgres: {e}")))?;
    for row in &rows {
        let id: Uuid = row.get("id");
        let ttl_generation: i64 = row.get("ttl_generation");
        sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'DELETE_POINT',$4,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING")
            .bind(Uuid::new_v4())
            .bind(access_zone_id)
            .bind(id)
            .bind(ttl_generation)
            .execute(&mut *tx)
            .await
            .map_err(|e| Status::unavailable(format!("postgres: {e}")))?;
    }
    tx.commit()
        .await
        .map_err(|e| Status::unavailable(format!("postgres: {e}")))?;
    Ok(rows.len() as u64)
}

#[derive(Debug)]
pub struct SearchMergeResult {
    pub results: Vec<pb::SearchResultV004>,
    pub merged_count: usize,
    pub deduplicated_count: usize,
}

#[derive(Debug, Clone)]
pub struct SearchMmrResult {
    pub results: Vec<pb::SearchResultV004>,
    pub input_count: usize,
    pub selected_count: usize,
    pub duration_ms: u64,
    pub enabled: bool,
    pub similarity_source: String,
    pub embedding_missing_count: usize,
    pub token_fallback_count: usize,
    pub dense_pair_comparisons: usize,
    pub token_pair_comparisons: usize,
}

#[derive(Debug, Clone)]
pub struct SearchSelectionResult {
    pub results: Vec<pb::SearchResultV004>,
    pub merged_count: usize,
    pub deduplicated_count: usize,
    pub mmr: SearchMmrResult,
}

#[derive(Debug, Default, Clone)]
pub struct MmrEmbeddingFetchStats {
    pub requested: usize,
    pub found: usize,
    pub missing: usize,
    pub errors: usize,
    pub timeouts: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub skipped_all_present: bool,
    pub skipped_small_pool: bool,
    pub duration_ms: u64,
}

impl MmrEmbeddingFetchStats {
    fn skipped() -> Self {
        Self {
            skipped_small_pool: true,
            ..Self::default()
        }
    }
}

static MMR_EMBEDDING_CACHE: OnceLock<Cache<String, Vec<f32>>> = OnceLock::new();

#[derive(Debug, Clone)]
struct EmbeddingIdentity {
    key: String,
    access_zone_id: Uuid,
    qdrant_point_id: Option<Uuid>,
    chunk_id: Option<Uuid>,
}

fn embedding_identity_from_result(
    result: &pb::SearchResultV004,
    _default_access_zone_id: Uuid,
    cfg: &AppConfig,
) -> Option<EmbeddingIdentity> {
    let metadata = result.citation.as_ref().map(|c| &c.metadata)?;
    let access_zone_id = match Uuid::parse_str(&result.access_zone_id) {
        Ok(id) => id,
        Err(_) => {
            metrics::counter!("graph_mmr_embedding_identity_missing_access_zone_total")
                .increment(1);
            return None;
        }
    };
    let dense_version = metadata
        .get("dense_version")
        .cloned()
        .unwrap_or_else(|| cfg.dense.version.clone());
    let model_version = metadata
        .get("model_version")
        .cloned()
        .unwrap_or_else(|| "UNKNOWN".into());
    let payload_version = metadata
        .get("payload_version")
        .cloned()
        .unwrap_or_else(|| "UNKNOWN".into());
    if let Some(raw) = metadata.get("qdrant_point_id") {
        if let Ok(point_id) = Uuid::parse_str(raw) {
            return Some(EmbeddingIdentity {
                key: format!(
                    "{}:point:{}:docv:{}:payload:{}:model:{}:dense:{}",
                    access_zone_id,
                    point_id,
                    result.document_version,
                    payload_version,
                    model_version,
                    dense_version
                ),
                access_zone_id,
                qdrant_point_id: Some(point_id),
                chunk_id: Uuid::parse_str(&result.matched_chunk_id).ok(),
            });
        }
    }
    metrics::counter!("graph_mmr_embedding_identity_missing_total").increment(1);
    if cfg.graph_rag.rerank.embedding_fetch_allow_chunk_fallback {
        if let Ok(chunk_id) = Uuid::parse_str(&result.matched_chunk_id) {
            let representation_type = metadata
                .get("representation_type")
                .cloned()
                .unwrap_or_else(|| "UNKNOWN".into());
            let tokenizer_version = metadata
                .get("tokenizer_version")
                .cloned()
                .unwrap_or_else(|| "UNKNOWN".into());
            let content_hash = metadata
                .get("content_hash")
                .cloned()
                .unwrap_or_else(|| "UNKNOWN".into());
            let required = [
                payload_version.as_str(),
                model_version.as_str(),
                dense_version.as_str(),
                tokenizer_version.as_str(),
                content_hash.as_str(),
            ];
            if required
                .iter()
                .any(|v| *v == "UNKNOWN" || v.trim().is_empty())
            {
                metrics::counter!("graph_mmr_embedding_identity_fallback_uncacheable_total")
                    .increment(1);
                return Some(EmbeddingIdentity {
                    key: format!(
                        "{}:chunk:{}:uncacheable:{}",
                        access_zone_id,
                        chunk_id,
                        uuid::Uuid::new_v4()
                    ),
                    access_zone_id,
                    qdrant_point_id: None,
                    chunk_id: Some(chunk_id),
                });
            }
            metrics::counter!("graph_mmr_embedding_identity_fallback_total").increment(1);
            return Some(EmbeddingIdentity {
                key: format!(
                    "{}:chunk:{}:docv:{}:payload:{}:model:{}:dense:{}:tokenizer:{}:repr:{}:hash:{}",
                    access_zone_id,
                    chunk_id,
                    result.document_version,
                    payload_version,
                    model_version,
                    dense_version,
                    tokenizer_version,
                    representation_type,
                    content_hash
                ),
                access_zone_id,
                qdrant_point_id: None,
                chunk_id: Some(chunk_id),
            });
        }
    }
    None
}

fn mmr_embedding_cache(cfg: &AppConfig) -> &'static Cache<String, Vec<f32>> {
    MMR_EMBEDDING_CACHE.get_or_init(|| {
        Cache::builder()
            .max_capacity(cfg.graph_rag.rerank.embedding_cache_max_entries as u64)
            .time_to_live(Duration::from_secs(
                cfg.graph_rag.rerank.embedding_cache_ttl_seconds,
            ))
            .build()
    })
}

fn prelimit_candidates_for_embedding_fetch(results: &mut Vec<pb::SearchResultV004>, limit: usize) {
    if results.len() <= limit {
        return;
    }
    results.sort_by(|a, b| {
        score_of(b)
            .partial_cmp(&score_of(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    metrics::counter!("graph_mmr_candidates_truncated_total", "stage" => "embedding_fetch_prelimit").increment(1);
    metrics::counter!("graph_mmr_candidates_truncated_by_total", "stage" => "embedding_fetch_prelimit").increment((results.len() - limit) as u64);
    results.truncate(limit);
}

async fn enrich_dense_embeddings_for_mmr(
    repo: Option<&Repository>,
    access_zone_ids: &[Uuid],
    direct_results: &mut [pb::SearchResultV004],
    graph_results: &mut [pb::SearchResultV004],
    cfg: &AppConfig,
    enrichment_limit: usize,
    fetch_timeout: Duration,
) -> MmrEmbeddingFetchStats {
    let started = std::time::Instant::now();
    let mut stats = MmrEmbeddingFetchStats::default();
    if !cfg.graph_rag.rerank.embedding_fetch_enabled {
        return stats;
    }
    let Some(default_access_zone_id) = access_zone_ids.first().copied() else {
        return stats;
    };
    let total_candidates = direct_results.len() + graph_results.len();
    if total_candidates < cfg.graph_rag.rerank.embedding_fetch_min_candidates {
        stats.skipped_small_pool = true;
        metrics::counter!("graph_mmr_embedding_fetch_skipped_small_pool_total").increment(1);
        return stats;
    }

    let mut identities: Vec<EmbeddingIdentity> = Vec::new();
    let mut seen_keys = HashSet::new();
    let mut enrichment_candidates: Vec<&pb::SearchResultV004> =
        direct_results.iter().chain(graph_results.iter()).collect();
    metrics::counter!("graph_mmr_full_candidates_total")
        .increment(enrichment_candidates.len() as u64);
    enrichment_candidates.sort_by(|a, b| {
        score_of(b)
            .partial_cmp(&score_of(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let original_enrichment_count = enrichment_candidates.len();
    enrichment_candidates.truncate(enrichment_limit.max(1));
    metrics::counter!("graph_mmr_enrichment_candidates_total")
        .increment(enrichment_candidates.len() as u64);
    if original_enrichment_count > enrichment_candidates.len() {
        metrics::counter!("graph_mmr_enrichment_candidates_skipped_total")
            .increment((original_enrichment_count - enrichment_candidates.len()) as u64);
    }
    for result in enrichment_candidates {
        if extract_normalized_embedding(result).is_some() {
            continue;
        }
        if let Some(identity) = embedding_identity_from_result(result, default_access_zone_id, cfg)
        {
            if seen_keys.insert(identity.key.clone()) {
                identities.push(identity);
            }
        } else {
            metrics::counter!("graph_mmr_embedding_identity_missing_total").increment(1);
        }
    }
    if identities.is_empty() {
        stats.skipped_all_present = true;
        metrics::counter!("graph_mmr_embedding_fetch_skipped_all_present_total").increment(1);
        return stats;
    }

    let mut embeddings_by_key: HashMap<String, Vec<f32>> = HashMap::new();
    if cfg.graph_rag.rerank.embedding_cache_enabled {
        let cache = mmr_embedding_cache(cfg);
        for identity in &identities {
            if let Some(vector) = cache.get(&identity.key).await {
                embeddings_by_key.insert(identity.key.clone(), vector);
                stats.cache_hits += 1;
                metrics::counter!("graph_mmr_embedding_cache_hit_total").increment(1);
            } else {
                stats.cache_misses += 1;
                metrics::counter!("graph_mmr_embedding_cache_miss_total").increment(1);
            }
        }
    }

    let point_ids_to_fetch = identities
        .iter()
        .filter(|i| !embeddings_by_key.contains_key(&i.key))
        .filter_map(|i| i.qdrant_point_id)
        .collect::<Vec<_>>();
    let chunk_ids_to_fetch = identities
        .iter()
        .filter(|i| !embeddings_by_key.contains_key(&i.key))
        .filter(|i| i.qdrant_point_id.is_none())
        .filter_map(|i| i.chunk_id)
        .collect::<Vec<_>>();

    stats.requested = point_ids_to_fetch.len() + chunk_ids_to_fetch.len();
    if stats.requested > 0 {
        metrics::counter!("graph_mmr_embedding_fetch_requested_total")
            .increment(stats.requested as u64);
    }

    if let Some(repo) = repo {
        if !point_ids_to_fetch.is_empty() {
            metrics::counter!("graph_mmr_embedding_fetch_by_point_total")
                .increment(point_ids_to_fetch.len() as u64);
            let fetch = repo.fetch_dense_embeddings_for_points_multi(
                access_zone_ids,
                &point_ids_to_fetch,
                &cfg.graph_rag.rerank.embedding_dense_representation_name,
            );
            match tokio::time::timeout(fetch_timeout, fetch).await {
                Ok(Ok(fetched)) => {
                    stats.found += fetched.len();
                    metrics::counter!("graph_mmr_embedding_fetch_found_total")
                        .increment(fetched.len() as u64);
                    for ((zone_id, point_id), vector) in fetched {
                        let normalized = normalize_embedding_once(vector);
                        if normalized.is_empty() {
                            metrics::counter!("graph_mmr_embedding_invalid_total").increment(1);
                            continue;
                        }
                        for identity in identities.iter().filter(|i| {
                            i.access_zone_id == zone_id && i.qdrant_point_id == Some(point_id)
                        }) {
                            if cfg.graph_rag.rerank.embedding_cache_enabled {
                                mmr_embedding_cache(cfg)
                                    .insert(identity.key.clone(), normalized.clone())
                                    .await;
                                metrics::counter!("graph_mmr_embedding_cache_insert_total")
                                    .increment(1);
                            }
                            embeddings_by_key.insert(identity.key.clone(), normalized.clone());
                        }
                    }
                }
                Ok(Err(e)) => {
                    stats.errors += 1;
                    metrics::counter!("graph_mmr_embedding_fetch_error_total").increment(1);
                    tracing::warn!(candidate_count = point_ids_to_fetch.len(), error = %e, "MMR_EMBEDDING_FETCH_BY_POINT_FAILED_TOKEN_FALLBACK");
                }
                Err(_) => {
                    stats.errors += 1;
                    stats.timeouts += 1;
                    metrics::counter!("graph_mmr_embedding_fetch_timeout_total").increment(1);
                    tracing::warn!(
                        candidate_count = point_ids_to_fetch.len(),
                        timeout_ms = cfg.graph_rag.rerank.embedding_fetch_timeout_ms,
                        "MMR_EMBEDDING_FETCH_BY_POINT_TIMEOUT_TOKEN_FALLBACK"
                    );
                }
            }
        }
        if !chunk_ids_to_fetch.is_empty()
            && cfg.graph_rag.rerank.embedding_fetch_allow_chunk_fallback
        {
            metrics::counter!("graph_mmr_embedding_fetch_by_chunk_fallback_total")
                .increment(chunk_ids_to_fetch.len() as u64);
            let fetch = repo.fetch_dense_embeddings_for_chunks_multi(
                access_zone_ids,
                &chunk_ids_to_fetch,
                &cfg.graph_rag.rerank.embedding_dense_representation_name,
                Some(cfg.dense.version.as_str()),
            );
            match tokio::time::timeout(fetch_timeout, fetch).await {
                Ok(Ok(fetched)) => {
                    stats.found += fetched.len();
                    metrics::counter!("graph_mmr_embedding_fetch_found_total")
                        .increment(fetched.len() as u64);
                    for ((zone_id, chunk_id), vector) in fetched {
                        let normalized = normalize_embedding_once(vector);
                        if normalized.is_empty() {
                            metrics::counter!("graph_mmr_embedding_invalid_total").increment(1);
                            continue;
                        }
                        for identity in identities.iter().filter(|i| {
                            i.access_zone_id == zone_id
                                && i.chunk_id == Some(chunk_id)
                                && i.qdrant_point_id.is_none()
                        }) {
                            if cfg.graph_rag.rerank.embedding_cache_enabled {
                                mmr_embedding_cache(cfg)
                                    .insert(identity.key.clone(), normalized.clone())
                                    .await;
                                metrics::counter!("graph_mmr_embedding_cache_insert_total")
                                    .increment(1);
                            }
                            embeddings_by_key.insert(identity.key.clone(), normalized.clone());
                        }
                    }
                }
                Ok(Err(e)) => {
                    stats.errors += 1;
                    metrics::counter!("graph_mmr_embedding_fetch_error_total").increment(1);
                    tracing::warn!(candidate_count = chunk_ids_to_fetch.len(), error = %e, "MMR_EMBEDDING_FETCH_BY_CHUNK_FALLBACK_FAILED_TOKEN_FALLBACK");
                }
                Err(_) => {
                    stats.errors += 1;
                    stats.timeouts += 1;
                    metrics::counter!("graph_mmr_embedding_fetch_timeout_total").increment(1);
                    tracing::warn!(
                        candidate_count = chunk_ids_to_fetch.len(),
                        timeout_ms = cfg.graph_rag.rerank.embedding_fetch_timeout_ms,
                        "MMR_EMBEDDING_FETCH_BY_CHUNK_FALLBACK_TIMEOUT_TOKEN_FALLBACK"
                    );
                }
            }
        }
    } else if stats.requested > 0 {
        stats.errors += 1;
        metrics::counter!("graph_mmr_embedding_fetch_error_total", "reason" => "repo_unavailable")
            .increment(1);
    }

    for result in direct_results.iter_mut().chain(graph_results.iter_mut()) {
        if extract_normalized_embedding(result).is_some() {
            continue;
        }
        let Some(identity) = embedding_identity_from_result(result, default_access_zone_id, cfg)
        else {
            continue;
        };
        if let Some(vector) = embeddings_by_key.get(&identity.key) {
            if let Some(citation) = result.citation.as_mut() {
                if let Ok(json) = serde_json::to_string(vector) {
                    citation
                        .metadata
                        .insert("embedding_normalized_json".into(), json);
                    citation
                        .metadata
                        .insert("embedding_identity_key".into(), identity.key);
                    citation
                        .metadata
                        .insert("embedding_internal_only".into(), "true".into());
                }
            }
        }
    }

    stats.duration_ms = started.elapsed().as_millis() as u64;
    stats.missing = identities.len().saturating_sub(embeddings_by_key.len());
    metrics::counter!("graph_mmr_embedding_fetch_missing_total").increment(stats.missing as u64);
    metrics::counter!("graph_mmr_candidate_embedding_missing_total")
        .increment(stats.missing as u64);
    metrics::histogram!("graph_mmr_embedding_fetch_duration_ms").record(stats.duration_ms as f64);
    if stats.duration_ms > cfg.graph_rag.rerank.embedding_fetch_warn_threshold_ms {
        metrics::counter!("graph_mmr_embedding_fetch_slow_total").increment(1);
        tracing::warn!(
            duration_ms = stats.duration_ms,
            threshold_ms = cfg.graph_rag.rerank.embedding_fetch_warn_threshold_ms,
            requested = stats.requested,
            found = stats.found,
            cache_hits = stats.cache_hits,
            "MMR_EMBEDDING_FETCH_SLOW"
        );
    }
    stats
}

fn normalize_embedding_once(mut vector: Vec<f32>) -> Vec<f32> {
    if vector.is_empty() || vector.iter().any(|v| !v.is_finite()) {
        return Vec::new();
    }
    let norm_sq: f32 = vector.iter().map(|v| v * v).sum();
    if (0.99..=1.01).contains(&norm_sq) {
        metrics::counter!("graph_mmr_embedding_already_normalized_total").increment(1);
        return vector;
    }
    let norm = norm_sq.sqrt();
    if norm <= f32::EPSILON {
        metrics::counter!("graph_mmr_embedding_zero_norm_total").increment(1);
        return Vec::new();
    }
    for value in &mut vector {
        *value /= norm;
    }
    metrics::counter!("graph_mmr_embedding_normalized_on_attach_total").increment(1);
    vector
}

fn estimate_results_tokens(results: &[pb::SearchResultV004], chars_per_token: usize) -> usize {
    let divisor = chars_per_token.max(1);
    results
        .iter()
        .map(|r| {
            let chars = r.matched_text.len() + r.parent_text.len();
            chars.div_ceil(divisor)
        })
        .sum()
}

fn estimate_text_tokens(text: &str, chars_per_token: usize) -> usize {
    let divisor = chars_per_token.max(1);
    text.len().div_ceil(divisor)
}

fn apply_token_budget_truncation(
    results: &mut Vec<pb::SearchResultV004>,
    cfg: &crate::config::RagContextConfig,
) -> (Vec<String>, Vec<String>, u32) {
    if !cfg.token_budget_enabled {
        return (Vec::new(), Vec::new(), 0);
    }
    let available = cfg
        .max_context_tokens
        .saturating_sub(cfg.reserved_answer_tokens);
    let effective_available = available
        .saturating_mul(100usize.saturating_sub(cfg.tokenizer_safety_margin_percent.min(80)))
        / 100;
    if effective_available == 0 {
        let dropped = results
            .iter()
            .map(|r| r.matched_chunk_id.clone())
            .collect::<Vec<_>>();
        results.clear();
        let count = dropped.len() as u32;
        return (dropped, Vec::new(), count);
    }
    let mut dropped = Vec::new();
    let chars_per_token = cfg.chars_per_token.max(1);
    let mut huge_to_drop = Vec::new();
    for (idx, result) in results.iter_mut().enumerate() {
        let tokens = estimate_text_tokens(&result.matched_text, chars_per_token)
            + estimate_text_tokens(&result.parent_text, chars_per_token);
        if tokens > effective_available {
            match cfg.huge_chunk_strategy.as_str() {
                "TRUNCATE_ONE_HUGE_CHUNK" if cfg.allow_chunk_text_truncation => {
                    let max_chars = effective_available.saturating_mul(chars_per_token).max(1);
                    result.matched_text = result.matched_text.chars().take(max_chars).collect();
                    counter!("rag_context_huge_chunk_truncated_total").increment(1);
                }
                _ => {
                    huge_to_drop.push(idx);
                    counter!("rag_context_huge_chunk_dropped_total").increment(1);
                }
            }
        }
    }
    for idx in huge_to_drop.into_iter().rev() {
        let removed = results.remove(idx);
        dropped.push(removed.matched_chunk_id);
    }
    let graph_token_limit =
        (effective_available as f32 * cfg.max_graph_token_fraction).floor() as usize;
    while estimate_graph_results_tokens(results, chars_per_token) > graph_token_limit {
        let Some(idx) = results
            .iter()
            .enumerate()
            .filter(|(_, result)| is_graph_expanded_result(result))
            .min_by(|(_, left), (_, right)| {
                token_truncation_score(left, &cfg.truncation_strategy)
                    .partial_cmp(&token_truncation_score(right, &cfg.truncation_strategy))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
        else {
            break;
        };
        let removed = results.remove(idx);
        dropped.push(removed.matched_chunk_id);
        counter!("rag_context_graph_quota_dropped_total").increment(1);
    }
    while estimate_results_tokens(results, chars_per_token) > effective_available
        && !results.is_empty()
    {
        let idx = results
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let protection_order = is_ranking_protected(a).cmp(&is_ranking_protected(b));
                if protection_order != std::cmp::Ordering::Equal {
                    return protection_order;
                }
                let sa = token_truncation_score(a, &cfg.truncation_strategy);
                let sb = token_truncation_score(b, &cfg.truncation_strategy);
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
            .unwrap_or(results.len() - 1);
        if cfg.truncation_strategy == "TRUNCATE_LAST_CHUNK"
            && cfg.allow_chunk_text_truncation
            && !results.is_empty()
        {
            let prefix_tokens = estimate_results_tokens(
                &results[..results.len().saturating_sub(1)],
                chars_per_token,
            );
            if prefix_tokens < effective_available {
                let allowed_for_last = effective_available.saturating_sub(prefix_tokens).max(1);
                if let Some(last) = results.last_mut() {
                    let max_chars = allowed_for_last.saturating_mul(chars_per_token).max(1);
                    last.matched_text = last.matched_text.chars().take(max_chars).collect();
                }
                if estimate_results_tokens(results, chars_per_token) <= effective_available {
                    break;
                }
            }
        }
        let removed = results.remove(idx);
        dropped.push(removed.matched_chunk_id);
    }
    let dropped_count = dropped.len() as u32;
    let mut warnings = Vec::new();
    if dropped.len() > 50 {
        dropped.truncate(50);
        warnings.push("DROPPED_CHUNK_IDS_TRUNCATED".to_string());
        counter!("rag_context_dropped_chunk_ids_truncated_total").increment(1);
    }
    (dropped, warnings, dropped_count)
}

fn is_graph_expanded_result(result: &pb::SearchResultV004) -> bool {
    primary_retrieval_source(result) == Some("GRAPH_EXPANDED")
}

fn primary_retrieval_source(result: &pb::SearchResultV004) -> Option<&str> {
    result
        .citation
        .as_ref()?
        .metadata
        .get("retrieval_source")
        .map(String::as_str)
}

fn duplicate_candidate_should_replace(
    existing: &pb::SearchResultV004,
    candidate: &pb::SearchResultV004,
) -> bool {
    match (
        is_graph_expanded_result(existing),
        is_graph_expanded_result(candidate),
    ) {
        // Graph expansion may enrich direct evidence, but must not relabel it.
        (false, true) => false,
        (true, false) => true,
        _ => score_of(candidate) > score_of(existing),
    }
}

fn estimate_graph_results_tokens(
    results: &[pb::SearchResultV004],
    chars_per_token: usize,
) -> usize {
    results
        .iter()
        .filter(|result| is_graph_expanded_result(result))
        .map(|result| {
            estimate_text_tokens(&result.matched_text, chars_per_token)
                + estimate_text_tokens(&result.parent_text, chars_per_token)
        })
        .sum()
}

fn token_truncation_score(result: &pb::SearchResultV004, strategy: &str) -> f32 {
    let Some(scores) = result.scores.as_ref() else {
        return 0.0;
    };
    match strategy {
        "DROP_LOWEST_SCORE_CHUNKS" => scores.fusion_score,
        "DROP_LOWEST_MMR_SCORE_CHUNKS" => scores.final_score,
        _ => scores.final_score,
    }
}

fn strip_internal_embedding_metadata(results: &mut [pb::SearchResultV004]) {
    for result in results {
        if let Some(citation) = result.citation.as_mut() {
            citation.metadata.remove("embedding_normalized_json");
            citation.metadata.remove("dense_embedding_normalized_json");
            citation.metadata.remove("embedding_internal_only");
        }
    }
}

pub fn resolve_final_context_limit(configured_limit: usize, top_k: usize, mode: &str) -> usize {
    match mode {
        "AT_LEAST_TOP_K" => configured_limit.max(top_k).max(1),
        _ => configured_limit.max(1),
    }
}

pub fn merge_search_results_before_truncate(
    direct_results: Vec<pb::SearchResultV004>,
    graph_results: Vec<pb::SearchResultV004>,
    final_limit: usize,
    strategy: &str,
    direct_context_limit: usize,
    graph_context_append_limit: usize,
) -> SearchMergeResult {
    match strategy {
        "DIRECT_FIRST" => merge_direct_first(direct_results, graph_results, final_limit),
        "GRAPH_AS_CONTEXT_APPEND" => merge_graph_as_context_append(
            direct_results,
            graph_results,
            final_limit,
            direct_context_limit,
            graph_context_append_limit,
        ),
        _ => merge_score_then_truncate(direct_results, graph_results, final_limit),
    }
}

fn merge_score_then_truncate(
    direct_results: Vec<pb::SearchResultV004>,
    graph_results: Vec<pb::SearchResultV004>,
    final_limit: usize,
) -> SearchMergeResult {
    let mut by_chunk: HashMap<String, pb::SearchResultV004> = HashMap::new();
    let mut dedup_count = 0usize;
    for result in direct_results.into_iter().chain(graph_results) {
        let key = result_identity_key(&result);
        if let Some(existing) = by_chunk.get_mut(&key) {
            dedup_count += 1;
            let replace = duplicate_candidate_should_replace(existing, &result);
            merge_secondary_metadata(existing, &result);
            if replace {
                let mut replacement = result;
                merge_secondary_metadata(&mut replacement, existing);
                *existing = replacement;
            }
            continue;
        }
        by_chunk.insert(key, result);
    }
    let merged_count = by_chunk.len();
    let mut merged = by_chunk.into_values().collect::<Vec<_>>();
    merged.sort_by(stable_result_rank);
    merged.truncate(final_limit);
    SearchMergeResult {
        results: merged,
        merged_count,
        deduplicated_count: dedup_count,
    }
}

fn merge_direct_first(
    direct_results: Vec<pb::SearchResultV004>,
    graph_results: Vec<pb::SearchResultV004>,
    final_limit: usize,
) -> SearchMergeResult {
    let mut seen = HashSet::new();
    let mut dedup_count = 0usize;
    let mut merged = Vec::with_capacity(direct_results.len() + graph_results.len());
    for result in direct_results {
        if seen.insert(result_identity_key(&result)) {
            merged.push(result);
        } else {
            dedup_count += 1;
        }
    }
    let mut graph_sorted = graph_results;
    graph_sorted.sort_by(|a, b| {
        score_of(b)
            .partial_cmp(&score_of(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for result in graph_sorted {
        if seen.insert(result_identity_key(&result)) {
            merged.push(result);
        } else {
            dedup_count += 1;
            if let Some(existing) = merged
                .iter_mut()
                .find(|r| result_identity_key(r) == result_identity_key(&result))
            {
                merge_secondary_metadata(existing, &result);
            }
        }
    }
    let merged_count = merged.len();
    merged.truncate(final_limit);
    SearchMergeResult {
        results: merged,
        merged_count,
        deduplicated_count: dedup_count,
    }
}

fn merge_graph_as_context_append(
    direct_results: Vec<pb::SearchResultV004>,
    graph_results: Vec<pb::SearchResultV004>,
    final_limit: usize,
    direct_context_limit: usize,
    graph_context_append_limit: usize,
) -> SearchMergeResult {
    let mut seen = HashSet::new();
    let mut dedup_count = 0usize;
    let mut merged = Vec::with_capacity(final_limit);
    let direct_budget = direct_context_limit.min(final_limit);
    for result in direct_results.into_iter().take(direct_budget) {
        if seen.insert(result_identity_key(&result)) {
            merged.push(result);
        } else {
            dedup_count += 1;
        }
    }
    let graph_budget = graph_context_append_limit.min(final_limit.saturating_sub(merged.len()));
    let mut graph_sorted = graph_results;
    graph_sorted.sort_by(|a, b| {
        score_of(b)
            .partial_cmp(&score_of(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for result in graph_sorted.into_iter().take(graph_budget) {
        if seen.insert(result_identity_key(&result)) {
            merged.push(result);
        } else {
            dedup_count += 1;
            if let Some(existing) = merged
                .iter_mut()
                .find(|r| result_identity_key(r) == result_identity_key(&result))
            {
                merge_secondary_metadata(existing, &result);
            }
        }
    }
    let merged_count = merged.len();
    SearchMergeResult {
        results: merged,
        merged_count,
        deduplicated_count: dedup_count,
    }
}

pub fn select_results_with_strategy_aware_mmr(
    direct_results: Vec<pb::SearchResultV004>,
    graph_results: Vec<pb::SearchResultV004>,
    final_limit: usize,
    strategy: &str,
    direct_context_limit: usize,
    graph_context_append_limit: usize,
    mmr_enabled: bool,
    mmr_lambda: f32,
    mmr_lambda_direct: f32,
    mmr_lambda_graph: f32,
    mmr_candidate_limit: usize,
    similarity_source: &str,
    fallback_similarity_source: &str,
    mmr_allow_direct_candidates: bool,
    mmr_allow_graph_candidates: bool,
    max_graph_relations_debug_per_candidate: usize,
) -> SearchSelectionResult {
    match strategy {
        "DIRECT_FIRST" => select_direct_first_with_group_mmr(
            direct_results,
            graph_results,
            final_limit,
            mmr_enabled,
            mmr_lambda_direct,
            mmr_lambda_graph,
            mmr_candidate_limit,
            similarity_source,
            fallback_similarity_source,
            mmr_allow_direct_candidates,
            mmr_allow_graph_candidates,
            max_graph_relations_debug_per_candidate,
        ),
        "GRAPH_AS_CONTEXT_APPEND" if graph_results.is_empty() => {
            let merge = merge_score_then_truncate(
                direct_results,
                graph_results,
                mmr_candidate_limit.max(final_limit),
            );
            let mmr = apply_mmr_rerank(
                merge.results,
                final_limit,
                mmr_enabled && mmr_allow_direct_candidates,
                mmr_lambda_direct,
                mmr_candidate_limit,
                similarity_source,
                fallback_similarity_source,
            );
            SearchSelectionResult {
                results: mmr.results.clone(),
                merged_count: merge.merged_count,
                deduplicated_count: merge.deduplicated_count,
                mmr,
            }
        }
        "GRAPH_AS_CONTEXT_APPEND" => select_graph_append_with_group_mmr(
            direct_results,
            graph_results,
            final_limit,
            direct_context_limit,
            graph_context_append_limit,
            mmr_enabled,
            mmr_lambda_direct,
            mmr_lambda_graph,
            mmr_candidate_limit,
            similarity_source,
            fallback_similarity_source,
            mmr_allow_direct_candidates,
            mmr_allow_graph_candidates,
            max_graph_relations_debug_per_candidate,
        ),
        _ => {
            let merge = merge_score_then_truncate(
                direct_results,
                graph_results,
                mmr_candidate_limit.max(final_limit),
            );
            let mmr = apply_mmr_rerank(
                merge.results,
                final_limit,
                mmr_enabled && (mmr_allow_direct_candidates || mmr_allow_graph_candidates),
                mmr_lambda,
                mmr_candidate_limit,
                similarity_source,
                fallback_similarity_source,
            );
            SearchSelectionResult {
                results: mmr.results.clone(),
                merged_count: merge.merged_count,
                deduplicated_count: merge.deduplicated_count,
                mmr,
            }
        }
    }
}

fn dedup_results_by_chunk(
    mut results: Vec<pb::SearchResultV004>,
    max_relations: usize,
) -> (Vec<pb::SearchResultV004>, usize) {
    results.sort_by(stable_result_rank);
    let mut by_chunk: HashMap<String, pb::SearchResultV004> = HashMap::new();
    let mut dedup = 0usize;
    for result in results {
        let key = result_identity_key(&result);
        if let Some(existing) = by_chunk.get_mut(&key) {
            dedup += 1;
            let replace = duplicate_candidate_should_replace(existing, &result);
            merge_secondary_metadata_with_limit(existing, &result, max_relations);
            if replace {
                let mut replacement = result;
                merge_secondary_metadata_with_limit(&mut replacement, existing, max_relations);
                *existing = replacement;
            }
        } else {
            by_chunk.insert(key, result);
        }
    }
    let mut out = by_chunk.into_values().collect::<Vec<_>>();
    out.sort_by(stable_result_rank);
    (out, dedup)
}

fn absorb_graph_duplicates_into_direct_pool(
    direct_pool: &mut [pb::SearchResultV004],
    graph_results: Vec<pb::SearchResultV004>,
    max_relations: usize,
) -> (Vec<pb::SearchResultV004>, usize) {
    let direct_by_identity = direct_pool
        .iter()
        .enumerate()
        .map(|(idx, result)| (result_identity_key(result), idx))
        .collect::<HashMap<_, _>>();
    let mut unique_graph = Vec::with_capacity(graph_results.len());
    let mut deduplicated = 0usize;
    for graph in graph_results {
        if let Some(idx) = direct_by_identity
            .get(&result_identity_key(&graph))
            .copied()
        {
            merge_secondary_metadata_with_limit(&mut direct_pool[idx], &graph, max_relations);
            deduplicated += 1;
        } else {
            unique_graph.push(graph);
        }
    }
    (unique_graph, deduplicated)
}

fn combine_group_mmr(
    left: SearchMmrResult,
    right: SearchMmrResult,
    results: Vec<pb::SearchResultV004>,
) -> SearchMmrResult {
    let source = if left.similarity_source == "DENSE_EMBEDDING"
        && right.similarity_source == "DENSE_EMBEDDING"
    {
        "DENSE_EMBEDDING".to_string()
    } else if !left.enabled && !right.enabled {
        "SCORE_ONLY".to_string()
    } else {
        "MIXED_OR_FALLBACK".to_string()
    };
    SearchMmrResult {
        results,
        input_count: left.input_count + right.input_count,
        selected_count: left.selected_count + right.selected_count,
        duration_ms: left.duration_ms + right.duration_ms,
        enabled: left.enabled || right.enabled,
        similarity_source: source,
        embedding_missing_count: left.embedding_missing_count + right.embedding_missing_count,
        token_fallback_count: left.token_fallback_count + right.token_fallback_count,
        dense_pair_comparisons: left.dense_pair_comparisons + right.dense_pair_comparisons,
        token_pair_comparisons: left.token_pair_comparisons + right.token_pair_comparisons,
    }
}

fn select_direct_first_with_group_mmr(
    direct_results: Vec<pb::SearchResultV004>,
    graph_results: Vec<pb::SearchResultV004>,
    final_limit: usize,
    mmr_enabled: bool,
    mmr_lambda_direct: f32,
    mmr_lambda_graph: f32,
    mmr_candidate_limit: usize,
    similarity_source: &str,
    fallback_similarity_source: &str,
    mmr_allow_direct_candidates: bool,
    mmr_allow_graph_candidates: bool,
    max_graph_relations_debug_per_candidate: usize,
) -> SearchSelectionResult {
    let (mut direct_pool, direct_dedup) =
        dedup_results_by_chunk(direct_results, max_graph_relations_debug_per_candidate);
    let (graph_results, cross_source_dedup) = absorb_graph_duplicates_into_direct_pool(
        &mut direct_pool,
        graph_results,
        max_graph_relations_debug_per_candidate,
    );
    metrics::counter!("graph_mmr_group_direct_candidates_total")
        .increment(direct_pool.len() as u64);
    metrics::gauge!("graph_mmr_group_direct_lambda_current").set(mmr_lambda_direct as f64);
    metrics::gauge!("graph_mmr_group_graph_lambda_current").set(mmr_lambda_graph as f64);
    let direct_mmr = apply_mmr_rerank(
        direct_pool,
        final_limit,
        mmr_enabled && mmr_allow_direct_candidates,
        mmr_lambda_direct,
        mmr_candidate_limit,
        similarity_source,
        fallback_similarity_source,
    );
    let mut selected = direct_mmr.results.clone();
    let mut dedup_count = direct_dedup + cross_source_dedup;
    let mut graph_candidates = Vec::new();
    let selected_by_chunk: HashMap<String, usize> = selected
        .iter()
        .enumerate()
        .map(|(idx, r)| (result_identity_key(r), idx))
        .collect();
    for graph in graph_results {
        if let Some(idx) = selected_by_chunk.get(&result_identity_key(&graph)).copied() {
            dedup_count += 1;
            merge_secondary_metadata_with_limit(
                &mut selected[idx],
                &graph,
                max_graph_relations_debug_per_candidate,
            );
        } else {
            graph_candidates.push(graph);
        }
    }
    let remaining = final_limit.saturating_sub(selected.len());
    metrics::counter!("graph_mmr_group_graph_candidates_total")
        .increment(graph_candidates.len() as u64);
    let graph_mmr = apply_mmr_rerank(
        graph_candidates,
        remaining,
        mmr_enabled && mmr_allow_graph_candidates && remaining > 0,
        mmr_lambda_graph,
        mmr_candidate_limit,
        similarity_source,
        fallback_similarity_source,
    );
    selected.extend(graph_mmr.results.clone());
    metrics::counter!("graph_mmr_group_direct_selected_total")
        .increment(direct_mmr.selected_count as u64);
    metrics::counter!("graph_mmr_group_graph_selected_total")
        .increment(graph_mmr.selected_count as u64);
    let mmr = combine_group_mmr(direct_mmr, graph_mmr, selected.clone());
    SearchSelectionResult {
        results: selected,
        merged_count: mmr.input_count,
        deduplicated_count: dedup_count,
        mmr,
    }
}

fn select_graph_append_with_group_mmr(
    direct_results: Vec<pb::SearchResultV004>,
    graph_results: Vec<pb::SearchResultV004>,
    final_limit: usize,
    direct_context_limit: usize,
    graph_context_append_limit: usize,
    mmr_enabled: bool,
    mmr_lambda_direct: f32,
    mmr_lambda_graph: f32,
    mmr_candidate_limit: usize,
    similarity_source: &str,
    fallback_similarity_source: &str,
    mmr_allow_direct_candidates: bool,
    mmr_allow_graph_candidates: bool,
    max_graph_relations_debug_per_candidate: usize,
) -> SearchSelectionResult {
    let graph_seed_chunk_ids = graph_results
        .iter()
        .filter_map(|result| {
            result
                .citation
                .as_ref()
                .and_then(|citation| citation.metadata.get("graph_seed_chunk_id"))
                .cloned()
        })
        .collect::<HashSet<_>>();
    let graph_seed_source_block_ids = graph_results
        .iter()
        .filter_map(|result| {
            result
                .citation
                .as_ref()
                .and_then(|citation| citation.metadata.get("graph_seed_source_block_id"))
                .cloned()
        })
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    let (mut direct_pool, direct_dedup) =
        dedup_results_by_chunk(direct_results, max_graph_relations_debug_per_candidate);
    let (graph_results, cross_source_dedup) = absorb_graph_duplicates_into_direct_pool(
        &mut direct_pool,
        graph_results,
        max_graph_relations_debug_per_candidate,
    );
    metrics::counter!("graph_mmr_group_direct_candidates_total")
        .increment(direct_pool.len() as u64);
    metrics::gauge!("graph_mmr_group_direct_lambda_current").set(mmr_lambda_direct as f64);
    metrics::gauge!("graph_mmr_group_graph_lambda_current").set(mmr_lambda_graph as f64);
    let direct_budget = direct_context_limit.min(final_limit);
    let (mut seed_direct_pool, remaining_direct_pool): (Vec<_>, Vec<_>) =
        direct_pool.drain(..).partition(|result| {
            graph_seed_chunk_ids.contains(&result.matched_chunk_id)
                || result_source_block_id(result)
                    .map(|block_id| graph_seed_source_block_ids.contains(block_id))
                    .unwrap_or(false)
        });
    for seed in &mut seed_direct_pool {
        mark_ranking_protection(
            seed,
            RankingProtection {
                preserve_primary_direct: true,
                preserve_strong_lexical: is_strong_lexical_candidate(seed),
                preserve_unique_source_block: result_source_block_id(seed).is_some(),
                preserve_required_segment_coverage: false,
            },
        );
    }
    let seed_direct_mmr = apply_mmr_rerank(
        seed_direct_pool,
        direct_budget,
        mmr_enabled && mmr_allow_direct_candidates && direct_budget > 0,
        mmr_lambda_direct,
        mmr_candidate_limit,
        similarity_source,
        fallback_similarity_source,
    );
    let remaining_direct_budget = direct_budget.saturating_sub(seed_direct_mmr.results.len());
    let remaining_direct_mmr = apply_mmr_rerank(
        remaining_direct_pool,
        remaining_direct_budget,
        mmr_enabled && mmr_allow_direct_candidates && remaining_direct_budget > 0,
        mmr_lambda_direct,
        mmr_candidate_limit,
        similarity_source,
        fallback_similarity_source,
    );
    let mut direct_selected = seed_direct_mmr.results.clone();
    direct_selected.extend(remaining_direct_mmr.results.clone());
    let direct_mmr = combine_group_mmr(seed_direct_mmr, remaining_direct_mmr, direct_selected);
    let mut selected = direct_mmr.results.clone();
    let mut dedup_count = direct_dedup + cross_source_dedup;
    let selected_by_chunk: HashMap<String, usize> = selected
        .iter()
        .enumerate()
        .map(|(idx, r)| (result_identity_key(r), idx))
        .collect();
    let mut graph_filtered = Vec::new();
    for graph in graph_results {
        if let Some(idx) = selected_by_chunk.get(&result_identity_key(&graph)).copied() {
            dedup_count += 1;
            merge_secondary_metadata_with_limit(
                &mut selected[idx],
                &graph,
                max_graph_relations_debug_per_candidate,
            );
        } else {
            graph_filtered.push(graph);
        }
    }
    let graph_budget = graph_context_append_limit.min(final_limit.saturating_sub(selected.len()));
    metrics::counter!("graph_mmr_group_graph_candidates_total")
        .increment(graph_filtered.len() as u64);
    let graph_mmr = apply_mmr_rerank(
        graph_filtered,
        graph_budget,
        mmr_enabled && mmr_allow_graph_candidates && graph_budget > 0,
        mmr_lambda_graph,
        mmr_candidate_limit,
        similarity_source,
        fallback_similarity_source,
    );
    selected.extend(graph_mmr.results.clone());
    metrics::counter!("graph_mmr_group_direct_selected_total")
        .increment(direct_mmr.selected_count as u64);
    metrics::counter!("graph_mmr_group_graph_selected_total")
        .increment(graph_mmr.selected_count as u64);
    let mmr = combine_group_mmr(direct_mmr, graph_mmr, selected.clone());
    SearchSelectionResult {
        results: selected,
        merged_count: mmr.input_count,
        deduplicated_count: dedup_count,
        mmr,
    }
}

pub fn apply_mmr_rerank(
    mut candidates: Vec<pb::SearchResultV004>,
    final_limit: usize,
    enabled: bool,
    lambda: f32,
    candidate_limit: usize,
    similarity_source: &str,
    fallback_similarity_source: &str,
) -> SearchMmrResult {
    let started = std::time::Instant::now();
    let requested_similarity_source = similarity_source.to_string();
    if !enabled {
        candidates.sort_by(stable_result_rank);
        reserve_protected_candidates_in_prefix(&mut candidates, final_limit);
        candidates.truncate(final_limit);
        let selected_count = candidates.len();
        return SearchMmrResult {
            results: candidates,
            input_count: 0,
            selected_count,
            duration_ms: started.elapsed().as_millis() as u64,
            enabled: false,
            similarity_source: "SCORE_ONLY".into(),
            embedding_missing_count: 0,
            token_fallback_count: 0,
            dense_pair_comparisons: 0,
            token_pair_comparisons: 0,
        };
    }

    let input_count = candidates.len();
    candidates.sort_by(stable_result_rank);
    let truncate_limit = candidate_limit.max(final_limit);
    reserve_protected_candidates_in_prefix(&mut candidates, truncate_limit);
    if candidates.len() > truncate_limit {
        metrics::counter!("graph_mmr_candidates_truncated_total").increment(1);
        metrics::counter!("graph_mmr_candidates_truncated_by_total")
            .increment((candidates.len() - truncate_limit) as u64);
    }
    candidates.truncate(truncate_limit);
    let protected_candidates = candidates
        .iter()
        .filter(|candidate| is_ranking_protected(candidate))
        .cloned()
        .collect::<Vec<_>>();

    let mut prepared: Vec<MmrPreparedCandidate> = candidates
        .into_iter()
        .map(MmrPreparedCandidate::from_result)
        .collect();
    let embedding_missing_count = prepared
        .iter()
        .filter(|c| c.embedding_normalized.is_none())
        .count();
    let mut dense_pair_comparisons = 0usize;
    let mut token_pair_comparisons = 0usize;

    let mut selected: Vec<MmrPreparedCandidate> = Vec::with_capacity(final_limit);
    while !prepared.is_empty() && selected.len() < final_limit {
        let mut best_idx = 0usize;
        let mut best_mmr = f32::MIN;
        let mut best_similarity = 0.0_f32;

        for (idx, candidate) in prepared.iter().enumerate() {
            let relevance = score_of(&candidate.result);
            let max_similarity = selected
                .iter()
                .map(|selected_candidate| {
                    let (similarity, source) = candidate_similarity(
                        candidate,
                        selected_candidate,
                        &requested_similarity_source,
                    );
                    match source {
                        SimilaritySource::DenseEmbedding => dense_pair_comparisons += 1,
                        SimilaritySource::TokenJaccardFallback => token_pair_comparisons += 1,
                    }
                    similarity
                })
                .fold(0.0_f32, f32::max);
            let mmr_score = lambda * relevance - (1.0 - lambda) * max_similarity;
            let best_candidate = &prepared[best_idx];
            let tie_break_wins = (mmr_score - best_mmr).abs() <= f32::EPSILON
                && (relevance > score_of(&best_candidate.result)
                    || ((relevance - score_of(&best_candidate.result)).abs() <= f32::EPSILON
                        && (max_similarity < best_similarity
                            || ((max_similarity - best_similarity).abs() <= f32::EPSILON
                                && result_identity_key(&candidate.result)
                                    < result_identity_key(&best_candidate.result)))));
            if mmr_score > best_mmr || tie_break_wins {
                best_mmr = mmr_score;
                best_idx = idx;
                best_similarity = max_similarity;
            }
        }

        let mut chosen = prepared.remove(best_idx);
        if let Some(citation) = chosen.result.citation.as_mut() {
            citation
                .metadata
                .insert("rerank_stage".into(), "MMR".into());
            citation
                .metadata
                .insert("mmr_score".into(), best_mmr.to_string());
            citation
                .metadata
                .insert("mmr_lambda".into(), lambda.to_string());
            citation.metadata.insert(
                "mmr_max_similarity_to_selected".into(),
                best_similarity.to_string(),
            );
            let effective_source = if dense_pair_comparisons > 0 && token_pair_comparisons > 0 {
                "MIXED"
            } else if dense_pair_comparisons > 0 {
                "DENSE_EMBEDDING"
            } else {
                fallback_similarity_source
            };
            citation
                .metadata
                .insert("mmr_similarity_source".into(), effective_source.to_string());
            citation.metadata.insert(
                "mmr_dense_pair_comparisons".into(),
                dense_pair_comparisons.to_string(),
            );
            citation.metadata.insert(
                "mmr_token_pair_comparisons".into(),
                token_pair_comparisons.to_string(),
            );
        }
        selected.push(chosen);
    }

    let effective_source = if dense_pair_comparisons > 0 && token_pair_comparisons > 0 {
        "MIXED".to_string()
    } else if dense_pair_comparisons > 0 {
        "DENSE_EMBEDDING".to_string()
    } else if enabled {
        fallback_similarity_source.to_string()
    } else {
        "SCORE_ONLY".to_string()
    };
    let token_fallback_count = token_pair_comparisons;
    metrics::counter!("graph_mmr_dense_pair_comparisons_total")
        .increment(dense_pair_comparisons as u64);
    metrics::counter!("graph_mmr_token_pair_comparisons_total")
        .increment(token_pair_comparisons as u64);
    if effective_source == "MIXED" {
        metrics::counter!("graph_mmr_mixed_similarity_sessions_total").increment(1);
    }
    let mut results = selected.into_iter().map(|c| c.result).collect::<Vec<_>>();
    preserve_protected_candidates_in_selection(&mut results, &protected_candidates, final_limit);
    let selected_count = results.len();
    SearchMmrResult {
        results,
        input_count,
        selected_count,
        duration_ms: started.elapsed().as_millis() as u64,
        enabled: true,
        similarity_source: effective_source,
        embedding_missing_count,
        token_fallback_count,
        dense_pair_comparisons,
        token_pair_comparisons,
    }
}

fn reserve_protected_candidates_in_prefix(
    candidates: &mut [pb::SearchResultV004],
    prefix_limit: usize,
) {
    if prefix_limit == 0 || candidates.len() <= prefix_limit {
        return;
    }
    let mut replacement_slots = (0..prefix_limit)
        .rev()
        .filter(|idx| !is_ranking_protected(&candidates[*idx]))
        .collect::<Vec<_>>();
    let protected_after_prefix = (prefix_limit..candidates.len())
        .filter(|idx| is_ranking_protected(&candidates[*idx]))
        .collect::<Vec<_>>();
    for protected_idx in protected_after_prefix {
        let Some(slot) = replacement_slots.pop() else {
            break;
        };
        candidates.swap(slot, protected_idx);
    }
}

fn preserve_protected_candidates_in_selection(
    selected: &mut Vec<pb::SearchResultV004>,
    protected: &[pb::SearchResultV004],
    final_limit: usize,
) {
    if final_limit == 0 {
        return;
    }
    for (protected_rank, candidate) in protected.iter().enumerate() {
        let key = result_identity_key(candidate);
        if selected
            .iter()
            .any(|existing| result_identity_key(existing) == key)
        {
            continue;
        }
        if selected.len() < final_limit {
            selected.push(candidate.clone());
            continue;
        }
        if let Some(idx) = selected
            .iter()
            .rposition(|existing| !is_ranking_protected(existing))
        {
            selected.remove(idx);
            let insertion = protected_rank.min(selected.len());
            selected.insert(insertion, candidate.clone());
        }
    }
}

#[derive(Debug, Clone)]
struct MmrPreparedCandidate {
    result: pb::SearchResultV004,
    embedding_normalized: Option<Vec<f32>>,
    token_set: HashSet<String>,
}

impl MmrPreparedCandidate {
    fn from_result(result: pb::SearchResultV004) -> Self {
        let embedding_normalized = extract_normalized_embedding(&result);
        let token_set = tokenize_result_text(&result);
        Self {
            result,
            embedding_normalized,
            token_set,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SimilaritySource {
    DenseEmbedding,
    TokenJaccardFallback,
}

fn candidate_similarity(
    a: &MmrPreparedCandidate,
    b: &MmrPreparedCandidate,
    requested_similarity_source: &str,
) -> (f32, SimilaritySource) {
    if requested_similarity_source == "DENSE_EMBEDDING" {
        if let (Some(ae), Some(be)) = (
            a.embedding_normalized.as_deref(),
            b.embedding_normalized.as_deref(),
        ) {
            if let Some(dot) = dot_slices(ae, be) {
                metrics::counter!("graph_mmr_embedding_similarity_total").increment(1);
                return (dot.clamp(-1.0, 1.0), SimilaritySource::DenseEmbedding);
            }
            metrics::counter!("graph_mmr_embedding_dimension_mismatch_total").increment(1);
        }
    }
    (
        token_jaccard_sets(&a.token_set, &b.token_set),
        SimilaritySource::TokenJaccardFallback,
    )
}

fn extract_normalized_embedding(result: &pb::SearchResultV004) -> Option<Vec<f32>> {
    let metadata = &result.citation.as_ref()?.metadata;
    let raw = metadata
        .get("embedding_normalized_json")
        .or_else(|| metadata.get("dense_embedding_normalized_json"))?;
    let vector: Vec<f32> = serde_json::from_str(raw).ok()?;
    if vector.is_empty() || vector.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let norm_sq: f32 = vector.iter().map(|v| v * v).sum();
    if !(0.95..=1.05).contains(&norm_sq) {
        return None;
    }
    Some(vector)
}

fn dot_slices(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    Some(a.iter().zip(b.iter()).map(|(x, y)| x * y).sum())
}

fn tokenize_result_text(result: &pb::SearchResultV004) -> HashSet<String> {
    tokenize_text(&candidate_text_for_no_answer(result))
}

fn tokenize_text(text: &str) -> HashSet<String> {
    text.nfkc()
        .collect::<String>()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect()
}

fn result_text_similarity(a: &pb::SearchResultV004, b: &pb::SearchResultV004) -> f32 {
    let a_text = format!("{} {}", a.matched_text, a.parent_text);
    let b_text = format!("{} {}", b.matched_text, b.parent_text);
    token_jaccard_similarity(&a_text, &b_text)
}

fn token_jaccard_similarity(a: &str, b: &str) -> f32 {
    let a_tokens = tokenize_text(a);
    let b_tokens = tokenize_text(b);
    token_jaccard_sets(&a_tokens, &b_tokens)
}

fn token_jaccard_sets(a_tokens: &HashSet<String>, b_tokens: &HashSet<String>) -> f32 {
    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }
    let intersection = a_tokens.intersection(b_tokens).count() as f32;
    let union = a_tokens.union(b_tokens).count() as f32;
    if union <= f32::EPSILON {
        0.0
    } else {
        intersection / union
    }
}

fn calibrate_result_score(
    result: &mut pb::SearchResultV004,
    source: &str,
    direct_score_weight: f32,
    graph_score_weight: f32,
    graph_score_bias: f32,
) {
    let raw_score = score_of(result);
    let calibrated_score = if source == "GRAPH_EXPANDED" {
        raw_score * graph_score_weight + graph_score_bias
    } else {
        raw_score * direct_score_weight
    };
    if let Some(scores) = result.scores.as_mut() {
        scores.final_score = calibrated_score;
    }
    if let Some(citation) = result.citation.as_mut() {
        citation
            .metadata
            .insert("raw_score".into(), raw_score.to_string());
        citation
            .metadata
            .insert("calibrated_score".into(), calibrated_score.to_string());
        citation
            .metadata
            .insert("score_calibration_source".into(), source.into());
    }
    metrics::counter!("graph_score_calibration_applied_total", "source" => source.to_string())
        .increment(1);
}

fn merge_secondary_metadata(primary: &mut pb::SearchResultV004, secondary: &pb::SearchResultV004) {
    merge_secondary_metadata_with_limit(primary, secondary, 5);
}

fn merge_lexical_backfill_candidate(
    primary: &mut pb::SearchResultV004,
    lexical: &pb::SearchResultV004,
) {
    merge_secondary_metadata(primary, lexical);
    match (primary.scores.as_mut(), lexical.scores.as_ref()) {
        (Some(primary_scores), Some(lexical_scores)) => {
            primary_scores.fusion_score += lexical_scores.fusion_score;
            primary_scores.final_score = primary_scores.fusion_score;
        }
        (None, Some(lexical_scores)) => {
            primary.scores = Some(*lexical_scores);
        }
        _ => {}
    }
    if let (Some(primary_citation), Some(lexical_citation)) =
        (primary.citation.as_mut(), lexical.citation.as_ref())
    {
        for key in [
            "source_block_id",
            "section_path",
            "heading",
            "lexical_rank",
            "lexical_score",
            "lexical_backend",
            "strong_lexical_evidence",
            "ranking_protection",
        ] {
            if let Some(value) = lexical_citation.metadata.get(key) {
                if !value.trim().is_empty() {
                    primary_citation
                        .metadata
                        .entry(key.to_string())
                        .or_insert_with(|| value.clone());
                }
            }
        }
    }
}

fn apply_indexed_lexical_rank_score(
    result: &mut pb::SearchResultV004,
    lexical_score: f32,
    lexical_rank: usize,
    lexical_weight: f32,
    rrf_k: f32,
) {
    let contribution = lexical_weight.max(0.0) / (rrf_k.max(1.0) + lexical_rank as f32);
    if let Some(scores) = result.scores.as_mut() {
        scores.dense_score = 0.0;
        scores.sparse_score = 0.0;
        scores.fusion_score = contribution;
        scores.final_score = contribution;
    }
    if let Some(citation) = result.citation.as_mut() {
        citation
            .metadata
            .insert("lexical_rank".into(), lexical_rank.to_string());
        citation
            .metadata
            .insert("lexical_score".into(), lexical_score.to_string());
        citation
            .metadata
            .insert("lexical_backend".into(), "POSTGRES_FTS".into());
    }
}

fn merge_secondary_metadata_with_limit(
    primary: &mut pb::SearchResultV004,
    secondary: &pb::SearchResultV004,
    max_relations: usize,
) {
    let primary_is_graph_expanded = primary_retrieval_source(primary) == Some("GRAPH_EXPANDED");
    let Some(primary_citation) = primary.citation.as_mut() else {
        return;
    };
    let Some(secondary_citation) = secondary.citation.as_ref() else {
        return;
    };
    let primary_source = primary_citation
        .metadata
        .get("retrieval_source")
        .cloned()
        .unwrap_or_else(|| "UNKNOWN".into());
    let secondary_source = secondary_citation
        .metadata
        .get("retrieval_source")
        .cloned()
        .unwrap_or_else(|| "UNKNOWN".into());
    let primary_parent = primary.parent_chunk_id.trim();
    let secondary_graph_seed_parent = secondary_citation
        .metadata
        .get("graph_seed_parent_chunk_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let secondary_graph_related_parent = secondary_citation
        .metadata
        .get("graph_related_parent_chunk_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let allow_graph_secondary_provenance = secondary_source != "GRAPH_EXPANDED"
        || primary_is_graph_expanded
        || (secondary_graph_seed_parent.is_none_or(|value| value == primary_parent)
            && secondary_graph_related_parent.is_none_or(|value| value == primary_parent));

    let mut sources =
        parse_string_array_metadata(primary_citation.metadata.get("retrieval_sources"));
    if sources.is_empty() {
        sources.push(primary_source.clone());
    }
    if (secondary_source != "GRAPH_EXPANDED" || allow_graph_secondary_provenance)
        && !sources.iter().any(|s| s == &secondary_source)
    {
        sources.push(secondary_source.clone());
    }
    if let Ok(json) = serde_json::to_string(&sources) {
        primary_citation
            .metadata
            .insert("retrieval_sources".into(), json);
    }
    primary_citation.metadata.insert(
        "secondary_sources_count".into(),
        sources.len().saturating_sub(1).to_string(),
    );

    if secondary_source == "GRAPH_EXPANDED" && allow_graph_secondary_provenance {
        for key in [
            "graph_seed_access_zone_id",
            "graph_seed_document_id",
            "graph_seed_document_version",
            "graph_seed_chunk_id",
            "graph_seed_parent_chunk_id",
            "graph_seed_source_block_id",
            "graph_relation_id",
            "graph_edge_id",
            "graph_relation_source",
            "graph_relation_type",
            "graph_relation_score",
            "graph_related_access_zone_id",
            "graph_related_document_id",
            "graph_related_document_version",
            "graph_related_chunk_id",
            "graph_related_parent_chunk_id",
            "graph_binding_id",
            "graph_hop_distance",
        ] {
            if let Some(value) = secondary_citation.metadata.get(key) {
                if !value.trim().is_empty() {
                    primary_citation
                        .metadata
                        .entry(key.to_string())
                        .or_insert_with(|| value.clone());
                }
            }
        }
        primary_citation
            .metadata
            .insert("graph_secondary_provenance".into(), "true".into());
    }

    let mut relations = parse_json_array_metadata(primary_citation.metadata.get("graph_relations"));
    if allow_graph_secondary_provenance {
        if let Some(relation) = secondary_citation.metadata.get("graph_relation_type") {
            relations.push(serde_json::json!({
                "relation_type": relation,
                "relation_score": secondary_citation.metadata.get("graph_relation_score").and_then(|v| v.parse::<f32>().ok()).unwrap_or_default(),
                "graph_score": secondary_citation.metadata.get("graph_score").and_then(|v| v.parse::<f32>().ok()).unwrap_or_default(),
                "seed_chunk_id": secondary_citation.metadata.get("graph_seed_chunk_id").cloned().unwrap_or_default(),
                "hop_distance": secondary_citation.metadata.get("graph_hop_distance").and_then(|v| v.parse::<u32>().ok()).unwrap_or_default(),
            }));
        }
    }
    if !relations.is_empty() {
        let mut seen = HashSet::new();
        relations.retain(|value| seen.insert(value.to_string()));
        let limit = max_relations.max(1);
        let truncated = relations.len() > limit;
        relations.truncate(limit);
        if let Ok(json) = serde_json::to_string(&relations) {
            primary_citation
                .metadata
                .insert("graph_relations".into(), json);
        }
        if truncated {
            primary_citation
                .metadata
                .insert("graph_relations_truncated".into(), "true".into());
        }
    }
    metrics::counter!("graph_secondary_sources_merged_total").increment(1);
}

fn parse_string_array_metadata(value: Option<&String>) -> Vec<String> {
    let Some(raw) = value else {
        return Vec::new();
    };
    if let Ok(values) = serde_json::from_str::<Vec<String>>(raw) {
        return values;
    }
    raw.split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn extraction_retrieval_sources(result: &pb::SearchResultV004) -> Vec<String> {
    let Some(citation) = result.citation.as_ref() else {
        return vec!["unknown".to_string()];
    };
    let mut sources = parse_string_array_metadata(citation.metadata.get("retrieval_sources"));
    if sources.is_empty() {
        if let Some(source) = citation.metadata.get("retrieval_source") {
            if !source.trim().is_empty() {
                sources.push(source.trim().to_string());
            }
        }
    }
    sources.sort();
    sources.dedup();
    if sources.is_empty() {
        sources.push("unknown".to_string());
    }
    sources
}

fn has_graph_expanded_evidence(results: &[pb::SearchResultV004]) -> bool {
    results.iter().any(|result| {
        !is_negative_mention_evidence(result)
            && extraction_retrieval_sources(result)
                .iter()
                .any(|source| source == "GRAPH_EXPANDED")
    })
}

fn parse_json_array_metadata(value: Option<&String>) -> Vec<serde_json::Value> {
    value
        .and_then(|raw| serde_json::from_str::<Vec<serde_json::Value>>(raw).ok())
        .unwrap_or_default()
}

fn score_of(result: &pb::SearchResultV004) -> f32 {
    result.scores.as_ref().map(|s| s.final_score).unwrap_or(0.0)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RankingProtection {
    preserve_primary_direct: bool,
    preserve_strong_lexical: bool,
    preserve_unique_source_block: bool,
    preserve_required_segment_coverage: bool,
}

struct RankingTraceCollector {
    enabled: bool,
    max_candidates: usize,
    max_stages: usize,
    order: Vec<String>,
    candidates: HashMap<String, pb::RankingCandidateTraceV005>,
    total_seen: usize,
    truncated: bool,
}

impl RankingTraceCollector {
    fn new(enabled: bool, max_candidates: usize, max_stages: usize) -> Self {
        Self {
            enabled,
            max_candidates: max_candidates.max(1),
            max_stages: max_stages.max(1),
            order: Vec::new(),
            candidates: HashMap::new(),
            total_seen: 0,
            truncated: false,
        }
    }

    fn observe(&mut self, stage: pb::RankingStageV005, results: &[pb::SearchResultV004]) {
        if !self.enabled {
            return;
        }
        for (rank, result) in results.iter().enumerate() {
            self.observe_one(stage, result, rank + 1, true, None, "");
        }
    }

    fn mark_removed(
        &mut self,
        stage: pb::RankingStageV005,
        before: &[pb::SearchResultV004],
        after: &[pb::SearchResultV004],
        reason: pb::CandidateDropReasonV005,
        detail: &str,
    ) {
        if !self.enabled {
            return;
        }
        let retained = after
            .iter()
            .map(result_identity_key)
            .collect::<HashSet<_>>();
        for (rank, result) in before.iter().enumerate() {
            self.observe_one(
                stage,
                result,
                rank + 1,
                retained.contains(&result_identity_key(result)),
                (!retained.contains(&result_identity_key(result))).then_some(reason),
                detail,
            );
        }
    }

    fn observe_one(
        &mut self,
        stage: pb::RankingStageV005,
        result: &pb::SearchResultV004,
        rank: usize,
        present: bool,
        drop_reason: Option<pb::CandidateDropReasonV005>,
        detail: &str,
    ) {
        let key = result_identity_key(result);
        if !self.candidates.contains_key(&key) {
            self.total_seen += 1;
            if self.candidates.len() >= self.max_candidates {
                self.truncated = true;
                return;
            }
            let citation = result.citation.as_ref();
            let source_block_id = citation
                .and_then(|c| c.metadata.get("source_block_id"))
                .cloned()
                .unwrap_or_default();
            let sources = extraction_retrieval_sources(result);
            self.order.push(key.clone());
            self.candidates.insert(
                key.clone(),
                pb::RankingCandidateTraceV005 {
                    identity: Some(pb::RankingCandidateIdentityV005 {
                        access_zone_id: result.access_zone_id.clone(),
                        document_id: result.document_id.clone(),
                        document_version: result.document_version,
                        matched_chunk_id: result.matched_chunk_id.clone(),
                        parent_chunk_id: result.parent_chunk_id.clone(),
                        source_block_id,
                    }),
                    stages: Vec::new(),
                    primary_direct: sources.iter().any(|s| s != "GRAPH_EXPANDED"),
                    graph_expanded: sources.iter().any(|s| s == "GRAPH_EXPANDED"),
                    exact_technical_match: trace_exact_technical_match(citation),
                    strong_lexical_evidence: is_strong_lexical_candidate(result),
                    ranking_protected: is_ranking_protected(result),
                },
            );
        }
        let Some(candidate) = self.candidates.get_mut(&key) else {
            return;
        };
        candidate.exact_technical_match |= trace_exact_technical_match(result.citation.as_ref());
        candidate.strong_lexical_evidence |= is_strong_lexical_candidate(result);
        candidate.ranking_protected |= is_ranking_protected(result);
        if candidate.stages.len() >= self.max_stages {
            self.truncated = true;
            return;
        }
        let scores = result.scores.as_ref().cloned().unwrap_or_default();
        let citation = result.citation.as_ref();
        let metadata_float = |name: &str| {
            citation
                .and_then(|c| c.metadata.get(name))
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or_default()
        };
        let effective_rank = if stage == pb::RankingStageV005::LexicalRetrieval {
            citation
                .and_then(|c| c.metadata.get("lexical_rank"))
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(rank)
        } else {
            rank
        };
        candidate.stages.push(pb::RankingStageTraceV005 {
            stage: stage as i32,
            present,
            rank: effective_rank as u32,
            dense_score: scores.dense_score,
            sparse_score: scores.sparse_score,
            lexical_score: metadata_float("lexical_score"),
            fusion_score: scores.fusion_score,
            graph_score: metadata_float("graph_score"),
            mmr_relevance: scores.final_score,
            mmr_redundancy: metadata_float("mmr_max_similarity_to_selected"),
            final_score: scores.final_score,
            retrieval_sources: extraction_retrieval_sources(result),
            drop_reason: drop_reason.unwrap_or(pb::CandidateDropReasonV005::DropReasonUnspecified)
                as i32,
            reason: detail.to_string(),
        });
    }

    fn finish(self) -> pb::RankingTraceV005 {
        let candidates = self
            .order
            .into_iter()
            .filter_map(|key| self.candidates.get(&key).cloned())
            .collect();
        pb::RankingTraceV005 {
            candidates,
            truncated: self.truncated,
            total_candidates_seen: self.total_seen as u32,
        }
    }
}

fn mark_ranking_protection(result: &mut pb::SearchResultV004, protection: RankingProtection) {
    let Some(citation) = result.citation.as_mut() else {
        return;
    };
    let encoded = [
        protection
            .preserve_primary_direct
            .then_some("PRIMARY_DIRECT"),
        protection
            .preserve_strong_lexical
            .then_some("STRONG_LEXICAL"),
        protection
            .preserve_unique_source_block
            .then_some("UNIQUE_SOURCE_BLOCK"),
        protection
            .preserve_required_segment_coverage
            .then_some("REQUIRED_SEGMENT_COVERAGE"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(",");
    let existing = citation
        .metadata
        .get("ranking_protection")
        .map(String::as_str)
        .unwrap_or("");
    let protections = existing
        .split(',')
        .chain(encoded.split(','))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    citation.metadata.insert(
        "ranking_protection".into(),
        protections.iter().copied().collect::<Vec<_>>().join(","),
    );
}

fn is_ranking_protected(result: &pb::SearchResultV004) -> bool {
    result
        .citation
        .as_ref()
        .and_then(|citation| citation.metadata.get("ranking_protection"))
        .is_some_and(|value| !value.is_empty())
}

fn is_strong_lexical_candidate(result: &pb::SearchResultV004) -> bool {
    result
        .citation
        .as_ref()
        .and_then(|citation| citation.metadata.get("strong_lexical_evidence"))
        .is_some_and(|value| value == "true")
}

fn graph_seed_chunk_id(result: &pb::SearchResultV004) -> Option<Uuid> {
    Uuid::parse_str(&result.matched_chunk_id)
        .ok()
        .or_else(|| Uuid::parse_str(&result.parent_chunk_id).ok())
}

fn graph_seed_score(result: &pb::SearchResultV004) -> f32 {
    result
        .scores
        .as_ref()
        .map(|scores| {
            let branch_score = scores.dense_score.max(scores.sparse_score);
            if branch_score.is_finite() && branch_score > 0.0 {
                branch_score.clamp(0.0, 1.0)
            } else {
                scores.final_score.max(scores.fusion_score).clamp(0.0, 1.0)
            }
        })
        .unwrap_or(0.5)
}

fn graph_seed_source_results_for_admitted_parents<'a>(
    direct_results: &'a [pb::SearchResultV004],
    pre_parent_dedup_results: &'a [pb::SearchResultV004],
) -> Vec<&'a pb::SearchResultV004> {
    let admitted_parents = direct_results
        .iter()
        .map(|result| {
            (
                result.access_zone_id.clone(),
                result.parent_chunk_id.clone(),
            )
        })
        .collect::<HashSet<_>>();
    let mut parents_with_children = HashSet::new();
    let mut seen_children = HashSet::new();
    let mut selected = Vec::new();

    for result in pre_parent_dedup_results {
        let parent_key = (
            result.access_zone_id.clone(),
            result.parent_chunk_id.clone(),
        );
        let child_key = (
            result.access_zone_id.clone(),
            result.matched_chunk_id.clone(),
        );
        if admitted_parents.contains(&parent_key) && seen_children.insert(child_key) {
            parents_with_children.insert(parent_key);
            selected.push(result);
        }
    }
    for result in direct_results {
        let parent_key = (
            result.access_zone_id.clone(),
            result.parent_chunk_id.clone(),
        );
        if !parents_with_children.contains(&parent_key) {
            selected.push(result);
        }
    }
    selected
}

#[derive(Debug, Clone)]
struct GraphSeedCandidate {
    key: (Uuid, Uuid),
    parent_key: (Uuid, Uuid),
    score: f32,
    matched_terms: usize,
    matched_discriminating_terms: usize,
    strong_lexical_evidence: bool,
    intent_unit_ids: Vec<usize>,
}

fn select_graph_seed_candidates(
    candidates: Vec<GraphSeedCandidate>,
    required_intent_ids: &[usize],
    limit: usize,
) -> Vec<GraphSeedCandidate> {
    let mut by_key = HashMap::<(Uuid, Uuid), GraphSeedCandidate>::new();
    for candidate in candidates {
        by_key
            .entry(candidate.key)
            .and_modify(|existing| {
                if compare_graph_seed_candidates(&candidate, existing).is_lt() {
                    existing.score = candidate.score;
                    existing.matched_terms = candidate.matched_terms;
                    existing.matched_discriminating_terms = candidate.matched_discriminating_terms;
                    existing.strong_lexical_evidence = candidate.strong_lexical_evidence;
                }
                for intent_id in &candidate.intent_unit_ids {
                    if !existing.intent_unit_ids.contains(intent_id) {
                        existing.intent_unit_ids.push(*intent_id);
                    }
                }
                existing.intent_unit_ids.sort_unstable();
            })
            .or_insert(candidate);
    }
    let mut ranked = by_key.into_values().collect::<Vec<_>>();
    ranked.sort_by(compare_graph_seed_candidates);
    let mut selected_parent_keys = HashSet::new();
    let mut ordered_parent_keys = Vec::new();
    for intent_id in required_intent_ids {
        if let Some(candidate) = ranked.iter().find(|candidate| {
            candidate.intent_unit_ids.contains(intent_id)
                && !selected_parent_keys.contains(&candidate.parent_key)
        }) {
            selected_parent_keys.insert(candidate.parent_key);
            ordered_parent_keys.push(candidate.parent_key);
        }
    }
    for candidate in &ranked {
        if selected_parent_keys.insert(candidate.parent_key) {
            ordered_parent_keys.push(candidate.parent_key);
        }
    }
    let mut candidates_by_parent = HashMap::<(Uuid, Uuid), Vec<GraphSeedCandidate>>::new();
    for candidate in ranked {
        candidates_by_parent
            .entry(candidate.parent_key)
            .or_default()
            .push(candidate);
    }
    let mut selected = Vec::new();
    for parent_key in ordered_parent_keys {
        if let Some(candidates) = candidates_by_parent.remove(&parent_key) {
            for candidate in candidates {
                selected.push(candidate);
                if selected.len() == limit {
                    return selected;
                }
            }
        }
    }
    selected
}

fn compare_graph_seed_candidates(
    left: &GraphSeedCandidate,
    right: &GraphSeedCandidate,
) -> std::cmp::Ordering {
    right
        .strong_lexical_evidence
        .cmp(&left.strong_lexical_evidence)
        .then_with(|| {
            right
                .matched_discriminating_terms
                .cmp(&left.matched_discriminating_terms)
        })
        .then_with(|| right.matched_terms.cmp(&left.matched_terms))
        .then_with(|| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| left.key.cmp(&right.key))
}

fn stable_result_rank(
    left: &pb::SearchResultV004,
    right: &pb::SearchResultV004,
) -> std::cmp::Ordering {
    let left_scores = left.scores.as_ref().cloned().unwrap_or_default();
    let right_scores = right.scores.as_ref().cloned().unwrap_or_default();
    right_scores
        .final_score
        .total_cmp(&left_scores.final_score)
        .then_with(|| {
            right_scores
                .fusion_score
                .total_cmp(&left_scores.fusion_score)
        })
        .then_with(|| left.document_id.cmp(&right.document_id))
        .then_with(|| right.document_version.cmp(&left.document_version))
        .then_with(|| left.matched_chunk_id.cmp(&right.matched_chunk_id))
}

fn success_branch_status<T>(items: &[T]) -> RetrievalBranchStatus {
    if items.is_empty() {
        RetrievalBranchStatus::SuccessNoEvidence
    } else {
        RetrievalBranchStatus::SuccessWithEvidence
    }
}

fn register_query_observability_metrics() {
    for tier in ["SINGLE", "STANDARD", "EXTENDED"] {
        counter!("astravector_query_total", "tier" => tier, "status" => "success").increment(0);
        counter!("astravector_query_total", "tier" => tier, "status" => "degraded").increment(0);
        counter!("astravector_query_total", "tier" => tier, "status" => "failed").increment(0);
        counter!("astravector_query_degraded_total", "tier" => tier, "reason" => "retrieval_partial_failure")
            .increment(0);
        counter!("astravector_optional_stage_skipped_total", "tier" => tier, "stage" => "graph", "reason" => "insufficient_budget")
            .increment(0);
        counter!("astravector_optional_stage_skipped_total", "tier" => tier, "stage" => "mmr", "reason" => "insufficient_budget")
            .increment(0);
        counter!("astravector_admission_rejected_total", "tier" => tier, "reason" => "admission_timeout")
            .increment(0);
        counter!("astravector_mmr_skipped_total", "tier" => tier, "reason" => "insufficient_budget")
            .increment(0);
        gauge!("astravector_work_units_in_flight", "tier" => tier).set(0.0);
    }
    histogram!("astravector_long_query_coverage_after_direct").record(0.0);
}

fn failed_branch_status(status: &Status) -> RetrievalBranchStatus {
    match status.code() {
        tonic::Code::DeadlineExceeded => RetrievalBranchStatus::Timeout,
        tonic::Code::Cancelled => RetrievalBranchStatus::Cancelled,
        _ => RetrievalBranchStatus::BackendUnavailable,
    }
}

#[derive(Debug, Clone, Default)]
struct NoAnswerFilterStats {
    pre_mmr_filtered_count: usize,
    post_mmr_triggered_count: usize,
}

fn strong_technical_query_tokens(query: &str) -> Vec<String> {
    let encoder = SparseTechnicalEncoder::new(0.0, 512);
    let mut tokens = encoder
        .analyze(query)
        .tokens
        .into_iter()
        .filter(|token| {
            matches!(
                token.class,
                SparseTokenClass::NumericExact
                    | SparseTokenClass::Alphanumeric
                    | SparseTokenClass::ErrorCode
                    | SparseTokenClass::UnderscoreIdentifier
                    | SparseTokenClass::Path
                    | SparseTokenClass::Filename
                    | SparseTokenClass::IpOrPort
                    | SparseTokenClass::Uuid
                    | SparseTokenClass::GrpcMethod
                    | SparseTokenClass::VersionToken
            )
        })
        .map(|token| token.token)
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens
}

fn no_answer_debug_enabled(include_debug: bool, cfg: &NoAnswerConfig) -> bool {
    include_debug
        || cfg.debug_candidates
        || std::env::var("ASTRAVECTOR_RETRIEVAL_DEBUG_CANDIDATES")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
        || std::env::var("ASTRAVECTOR_QUALITY_DEBUG_CANDIDATES")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
}

fn candidate_text_for_no_answer(result: &pb::SearchResultV004) -> String {
    let matched = result.matched_text.trim();
    let body = if !matched.is_empty() {
        matched
    } else {
        result.parent_text.as_str()
    };
    let mut parts = vec![body.to_string()];
    if let Some(citation) = result.citation.as_ref() {
        for key in ["document_title", "title", "heading", "section_path"] {
            if let Some(value) = citation.metadata.get(key) {
                if !value.trim().is_empty() {
                    parts.push(value.clone());
                }
            }
        }
    }
    parts.join("\n").to_lowercase()
}

fn matched_technical_tokens(
    result: &pb::SearchResultV004,
    query_technical_tokens: &[String],
) -> Vec<String> {
    if query_technical_tokens.is_empty() {
        return Vec::new();
    }
    let candidate_text = candidate_text_for_no_answer(result);
    query_technical_tokens
        .iter()
        .filter(|token| candidate_text.contains(&token.to_lowercase()))
        .cloned()
        .collect()
}

fn complete_technical_match(required: &[String], matched: &[String]) -> bool {
    !required.is_empty() && required.iter().all(|token| matched.contains(token))
}

fn trace_exact_technical_match(citation: Option<&pb::SearchCitationV004>) -> bool {
    citation.is_some_and(|citation| {
        [
            "exact_technical_match",
            "candidate_debug.exact_technical_token_match",
        ]
        .iter()
        .any(|key| {
            citation
                .metadata
                .get(*key)
                .is_some_and(|value| value == "true")
        })
    })
}

fn matched_term_count(result: &pb::SearchResultV004, query: &str) -> usize {
    let candidate_text = candidate_text_for_no_answer(result);
    let candidate_terms = lexical_terms(&candidate_text);
    positive_query_terms(query)
        .into_iter()
        .filter(|term| candidate_terms.contains(term))
        .count()
}

fn matched_discriminating_term_count(result: &pb::SearchResultV004, query: &str) -> usize {
    let candidate_text = candidate_text_for_no_answer(result);
    let candidate_terms = lexical_terms(&candidate_text);
    positive_query_terms(query)
        .into_iter()
        .filter(|term| !is_common_retrieval_overlap_term(term))
        .filter(|term| candidate_terms.contains(term))
        .count()
}

fn leading_discriminating_query_term_matches(result: &pb::SearchResultV004, query: &str) -> bool {
    let Some(leading) = ordered_positive_query_terms(query)
        .into_iter()
        .find(|term| !is_common_retrieval_overlap_term(term))
    else {
        return true;
    };
    lexical_terms(&candidate_text_for_no_answer(result)).contains(&leading)
}

fn query_term_count(query: &str) -> usize {
    positive_query_terms(query).len()
}

fn lexical_terms(text: &str) -> HashSet<String> {
    ordered_lexical_terms(text).into_iter().collect()
}

fn ordered_lexical_terms(text: &str) -> Vec<String> {
    let excluded = excluded_query_terms(text);
    ordered_lexical_terms_raw(text)
        .into_iter()
        .filter(|term| !excluded.contains(term))
        .collect()
}

fn positive_query_terms(query: &str) -> HashSet<String> {
    ordered_positive_query_terms(query).into_iter().collect()
}

fn ordered_positive_query_terms(query: &str) -> Vec<String> {
    let excluded = excluded_query_terms(query);
    ordered_lexical_terms_raw(query)
        .into_iter()
        .filter(|term| !excluded.contains(term))
        .collect()
}

fn ordered_lexical_terms_raw(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    for term in text.split_whitespace().filter_map(normalized_lexical_term) {
        for variant in lexical_term_variants(&term) {
            if seen.insert(variant.clone()) {
                terms.push(variant);
            }
        }
    }
    terms
}

fn excluded_query_terms(query: &str) -> HashSet<String> {
    let lowered = query.to_lowercase();
    let mut clauses = Vec::new();
    for marker in [
        "without using",
        "without",
        "excluding",
        "except",
        "не используя",
        "без",
        "кроме",
    ] {
        let mut offset = 0;
        while let Some(index) = lowered[offset..].find(marker) {
            let start = offset + index + marker.len();
            let tail = &lowered[start..];
            let end = tail
                .find(|ch: char| matches!(ch, ',' | ';' | '.' | '?' | '!'))
                .unwrap_or(tail.len());
            clauses.push(tail[..end].trim().to_string());
            offset = start;
        }
    }
    for marker in ["айтпай", "қолданбай"] {
        for clause in lowered.split(|ch: char| matches!(ch, ',' | ';' | '.' | '?' | '!')) {
            if let Some(index) = clause.find(marker) {
                let head = clause[..index].trim();
                if !head.is_empty() {
                    clauses.push(head.to_string());
                }
            }
        }
    }
    clauses
        .into_iter()
        .flat_map(|clause| ordered_lexical_terms_raw(&clause))
        .filter(|term| !matches!(term.as_str(), "without" | "using" | "excluding" | "except"))
        .collect()
}

fn violates_query_exclusion_terms(result: &pb::SearchResultV004, query: &str) -> bool {
    let excluded_terms = excluded_query_terms(query);
    if excluded_terms.is_empty() {
        return false;
    }
    let candidate_terms = lexical_terms(&candidate_text_for_no_answer(result));
    excluded_terms.iter().any(|term| candidate_terms.contains(term))
}

fn lexical_term_variants(term: &str) -> Vec<String> {
    let mut variants = vec![term.to_string()];
    if term.contains('_') || term.contains('-') {
        variants.extend(
            term.split(['_', '-'])
                .filter(|part| part.len() >= 2)
                .map(str::to_string),
        );
    }
    if variants.iter().any(|variant| variant == "tenant") {
        variants.push("access_zone_id".into());
    }
    if term.len() > 5 && term.ends_with("ly") {
        variants.push(term[..term.len() - 2].to_string());
    }
    match term {
        "absent" => variants.push("missing".into()),
        "acknowledge" | "acknowledgement" | "acknowledgment" => variants.push("acknowledge".into()),
        "accelerate" | "accelerates" => variants.push("improve".into()),
        "canonical" => {
            variants.push("source".into());
            variants.push("truth".into());
        }
        "ceiling" => variants.push("limit".into()),
        "confirm" | "confirms" => variants.push("prove".into()),
        "discuss" | "discusses" => variants.push("cover".into()),
        "drift" => variants.push("missing".into()),
        "essential" => variants.push("mandatory".into()),
        "fail" | "failed" | "failure" | "failures" => variants.push("failure".into()),
        "improve" | "improves" => variants.push("accelerate".into()),
        "key" => variants.push("index".into()),
        "bench" => variants.push("benchmark".into()),
        "benchmark" => variants.push("bench".into()),
        "limit" => variants.push("ceiling".into()),
        "missing" => {
            variants.push("absent".into());
            variants.push("drift".into());
        }
        "record" | "records" => variants.push("handles".into()),
        "restored" | "restore" => variants.push("repair".into()),
        "repaired" | "repair" => variants.push("restore".into()),
        "retrieval" => variants.push("search".into()),
        "search" => variants.push("retrieval".into()),
        "security" => {
            variants.push("threat".into());
            variants.push("model".into());
        }
        "store" => variants.push("postgresql".into()),
        "prove" | "proves" => variants.push("confirm".into()),
        "seen" | "saw" => variants.push("acknowledgement".into()),
        "tenant" => variants.push("access_zone_id".into()),
        "gateway" => variants.push("x-astravector".into()),
        _ => {}
    }
    variants
}

fn normalized_lexical_term(term: &str) -> Option<String> {
    let term = term
        .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .to_lowercase();
    if term.len() < 2 || is_lexical_stopword(&term) {
        return None;
    }
    Some(lexical_stem(&term))
}

fn lexical_stem(term: &str) -> String {
    if matches!(term, "binding" | "chess" | "missing" | "ranking") {
        return term.to_string();
    }
    match term {
        "acknowledged" => return "acknowledge".to_string(),
        "fixtures" => return "fixture".to_string(),
        "proves" => return "prove".to_string(),
        "repaired" => return "repair".to_string(),
        "restored" => return "restore".to_string(),
        _ => {}
    }
    if term.len() > 6 && term.ends_with("ated") {
        return format!("{}ate", &term[..term.len() - 4]);
    }
    if term.len() > 5 && term.ends_with("ies") {
        return format!("{}y", &term[..term.len() - 3]);
    }
    if term.len() > 6 && (term.ends_with("ates") || term.ends_with("les")) {
        return term[..term.len() - 1].to_string();
    }
    for suffix in ["ing", "ed", "es", "s"] {
        if term.len() > suffix.len() + 3 && term.ends_with(suffix) {
            return term[..term.len() - suffix.len()].to_string();
        }
    }
    term.to_string()
}

fn is_lexical_stopword(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "did"
            | "do"
            | "does"
            | "answer"
            | "avoid"
            | "avoiding"
            | "behavior"
            | "behaviour"
            | "block"
            | "can"
            | "chunk"
            | "chunks"
            | "contain"
            | "contains"
            | "cover"
            | "covers"
            | "during"
            | "evidence"
            | "explain"
            | "explains"
            | "for"
            | "find"
            | "from"
            | "happen"
            | "happens"
            | "how"
            | "if"
            | "in"
            | "is"
            | "it"
            | "main"
            | "mechanism"
            | "mechanisms"
            | "much"
            | "must"
            | "of"
            | "on"
            | "operator"
            | "or"
            | "quickly"
            | "redundant"
            | "rule"
            | "rules"
            | "running"
            | "return"
            | "section"
            | "should"
            | "state"
            | "that"
            | "the"
            | "to"
            | "distinct"
            | "guidance"
            | "guide"
            | "relevant"
            | "unique"
            | "what"
            | "when"
            | "which"
            | "where"
            | "while"
            | "with"
            | "why"
    )
}

fn is_common_retrieval_overlap_term(term: &str) -> bool {
    matches!(
        term,
        "astravector"
            | "claim"
            | "claims"
            | "dead-letter"
            | "document"
            | "documents"
            | "event"
            | "events"
            | "filter"
            | "filters"
            | "point"
            | "points"
            | "publish"
            | "publishing"
            | "qdrant"
            | "search"
            | "vector"
            | "vectors"
    )
}

fn strong_lexical_match(matched_terms: usize, query_terms: usize, cfg: &NoAnswerConfig) -> bool {
    if query_terms == 0 {
        return false;
    }
    let required_terms = cfg.sparse_only_min_matched_terms.max(2).min(query_terms);
    matched_terms >= required_terms && matched_terms.saturating_mul(2) >= query_terms
}

fn lexical_score_for_no_answer(result: &pb::SearchResultV004) -> f32 {
    result
        .citation
        .as_ref()
        .and_then(|citation| citation.metadata.get("lexical_score"))
        .and_then(|score| score.parse::<f32>().ok())
        .unwrap_or(0.0)
}

fn retrieval_sources_contains(result: &pb::SearchResultV004, needle: &str) -> bool {
    result.citation.as_ref().is_some_and(|citation| {
        citation
            .metadata
            .get("retrieval_source")
            .is_some_and(|value| value == needle)
            || citation
                .metadata
                .get("retrieval_sources")
                .is_some_and(|value| value.contains(needle))
    })
}

fn strict_lexical_query_match(
    matched_terms: usize,
    matched_discriminating_terms: usize,
    leading_discriminating_match: bool,
    query_terms: usize,
) -> bool {
    let strong_coverage = query_terms == 0 || matched_terms.saturating_mul(2) >= query_terms;
    matched_terms >= 2
        && matched_discriminating_terms >= 1
        && leading_discriminating_match
        && strong_coverage
}

fn is_mixed_script_query(query: &str) -> bool {
    let has_ascii_alpha = query.chars().any(|ch| ch.is_ascii_alphabetic());
    let has_cyrillic = query.chars().any(|ch| matches!(ch as u32, 0x0400..=0x04FF | 0x0500..=0x052F));
    has_ascii_alpha && has_cyrillic
}

fn apply_no_answer_exact_technical_boost(
    result: &mut pb::SearchResultV004,
    required_tokens: &[String],
    matched_tokens: &[String],
    cfg: &NoAnswerConfig,
    debug_enabled: bool,
) -> (bool, f32, f32) {
    let sparse_before = result
        .scores
        .as_ref()
        .map(|scores| scores.sparse_score)
        .unwrap_or(0.0);
    let exact_match = complete_technical_match(required_tokens, matched_tokens);
    let mut sparse_after = sparse_before;
    let mut boost_applied = false;
    if exact_match && sparse_before > 0.0 {
        sparse_after = sparse_before * (1.0 + cfg.exact_technical_boost);
        boost_applied = true;
        if let Some(scores) = result.scores.as_mut() {
            scores.sparse_score = sparse_after;
            scores.final_score = scores.final_score.max(sparse_after);
            scores.fusion_score = scores.fusion_score.max(sparse_after);
        }
    }
    if debug_enabled {
        if let Some(citation) = result.citation.as_mut() {
            citation.metadata.insert(
                "candidate_debug.exact_technical_token_match".into(),
                exact_match.to_string(),
            );
            citation
                .metadata
                .insert("exact_technical_match".into(), exact_match.to_string());
            citation.metadata.insert(
                "candidate_debug.required_technical_tokens".into(),
                serde_json::to_string(required_tokens).unwrap_or_else(|_| "[]".into()),
            );
            citation.metadata.insert(
                "candidate_debug.matched_technical_tokens".into(),
                serde_json::to_string(matched_tokens).unwrap_or_else(|_| "[]".into()),
            );
            citation.metadata.insert(
                "candidate_debug.exact_technical_boost_applied".into(),
                boost_applied.to_string(),
            );
            citation.metadata.insert(
                "candidate_debug.sparse_score_before_boost".into(),
                sparse_before.to_string(),
            );
            citation.metadata.insert(
                "candidate_debug.sparse_score_after_boost".into(),
                sparse_after.to_string(),
            );
        }
    }
    (exact_match, sparse_before, sparse_after)
}

fn no_answer_candidate_passes(
    result: &pb::SearchResultV004,
    search_mode: pb::SearchModeV005,
    exact_technical_match: bool,
    sparse_after_boost: f32,
    matched_terms: usize,
    matched_discriminating_terms: usize,
    leading_discriminating_match: bool,
    query_terms: usize,
    mixed_script_query: bool,
    cfg: &NoAnswerConfig,
) -> bool {
    let Some(scores) = result.scores.as_ref() else {
        return false;
    };
    let strong_query_coverage = matched_terms.saturating_mul(5) >= query_terms.saturating_mul(3);
    let exact_sparse_allow =
        exact_technical_match && sparse_after_boost >= cfg.min_sparse_score * 2.0;
    let exact_hybrid_allow = exact_sparse_allow && matched_discriminating_terms >= 1;
    let strong_lexical_match = strong_lexical_match(matched_terms, query_terms, cfg);
    let lexical_signal = lexical_score_for_no_answer(result);
    let has_indexed_lexical_support = retrieval_sources_contains(result, "POSTGRES_FTS")
        || is_strong_lexical_candidate(result)
        || lexical_signal >= cfg.min_sparse_score * 0.75;
    let branch_confidence = scores
        .dense_score
        .max(sparse_after_boost)
        .max(lexical_signal);
    let strong_branch_source = scores.dense_score >= cfg.min_dense_score
        || is_strong_lexical_candidate(result)
        || (sparse_after_boost >= cfg.min_sparse_score && strong_lexical_match)
        || (lexical_signal >= cfg.min_sparse_score && strong_lexical_match);
    let non_container_direct = result.matched_chunk_id != result.parent_chunk_id;
    let branch_confidence_allow = branch_confidence >= cfg.min_sparse_score
        && strong_branch_source
        && matched_terms >= cfg.sparse_only_min_matched_terms.max(2)
        && matched_discriminating_terms >= 1
        && strong_query_coverage
        && (leading_discriminating_match
            || strong_lexical_match
            || is_strong_lexical_candidate(result));
    let semantic_anchor_allow = if has_indexed_lexical_support {
        non_container_direct
            && strong_branch_source
            && matched_terms >= 1
            && matched_discriminating_terms >= 1
            && (leading_discriminating_match
                || sparse_after_boost >= cfg.min_sparse_score * 0.75
                || lexical_signal >= cfg.min_sparse_score * 0.75)
    } else if mixed_script_query {
        non_container_direct
            && scores.dense_score >= cfg.min_dense_score
            && matched_terms >= 1
            && matched_discriminating_terms >= 1
    } else {
        let required_dense_terms = if query_terms <= 1 || mixed_script_query {
            1
        } else {
            cfg.sparse_only_min_matched_terms.max(2)
        };
        non_container_direct
            && scores.dense_score >= cfg.min_dense_score
            && matched_terms >= required_dense_terms
            && matched_discriminating_terms >= 1
            && strong_query_coverage
            && leading_discriminating_match
    };
    let strong_hybrid_lexical_allow = strong_lexical_match
        && strong_branch_source
        && matched_terms >= cfg.sparse_only_min_matched_terms.max(3)
        && matched_discriminating_terms >= 2
        && (leading_discriminating_match || is_strong_lexical_candidate(result))
        && strong_query_coverage;
    match search_mode {
        pb::SearchModeV005::Dense => {
            scores.dense_score >= cfg.min_dense_score || scores.final_score >= cfg.min_dense_score
        }
        pb::SearchModeV005::Sparse => {
            let technical_gate = !cfg.sparse_only_require_technical_token
                || exact_technical_match
                || strong_lexical_match;
            (technical_gate
                && matched_terms >= cfg.sparse_only_min_matched_terms
                && (sparse_after_boost >= cfg.min_sparse_score || strong_lexical_match))
                || exact_sparse_allow
        }
        pb::SearchModeV005::Hybrid | pb::SearchModeV005::Unspecified => {
            ((scores.final_score >= cfg.min_hybrid_score
                || scores.fusion_score >= cfg.min_hybrid_score)
                && matched_terms >= 2
                && matched_discriminating_terms >= 1
                && leading_discriminating_match
                && strong_query_coverage)
                || branch_confidence_allow
                || semantic_anchor_allow
                || strong_hybrid_lexical_allow
                || exact_hybrid_allow
        }
    }
}

fn no_answer_partial_mmr_evidence_passes(
    result: &pb::SearchResultV004,
    search_mode: pb::SearchModeV005,
    exact_technical_match: bool,
    sparse_after_boost: f32,
    matched_terms: usize,
    matched_discriminating_terms: usize,
    cfg: &NoAnswerConfig,
) -> bool {
    let Some(scores) = result.scores.as_ref() else {
        return false;
    };
    let enough_score = match search_mode {
        pb::SearchModeV005::Dense => {
            scores.dense_score >= cfg.min_dense_score || scores.final_score >= cfg.min_dense_score
        }
        pb::SearchModeV005::Sparse => {
            sparse_after_boost >= cfg.min_sparse_score || scores.final_score >= cfg.min_sparse_score
        }
        pb::SearchModeV005::Hybrid | pb::SearchModeV005::Unspecified => {
            scores.final_score >= cfg.min_hybrid_score
                || scores.fusion_score >= cfg.min_hybrid_score
                || (sparse_after_boost >= cfg.min_sparse_score
                    && (scores.dense_score >= cfg.min_dense_score
                        || exact_technical_match
                        || is_strong_lexical_candidate(result)))
        }
    };
    enough_score
        && matched_terms >= 2
        && (matched_discriminating_terms >= 1 || exact_technical_match)
}

fn partial_multi_aspect_candidate_passes(
    result: &pb::SearchResultV004,
    query: &str,
    search_mode: pb::SearchModeV005,
    exact_technical_match: bool,
    sparse_after_boost: f32,
    matched_terms: usize,
    matched_discriminating_terms: usize,
    strongly_seeded_document: bool,
    cfg: &NoAnswerConfig,
) -> bool {
    if !is_multi_aspect_query(query)
        || !strongly_seeded_document
        || is_root_container_result(result)
        || is_negative_mention_evidence(result)
        || matched_terms == 0
        || (matched_discriminating_terms == 0 && !exact_technical_match)
    {
        return false;
    }
    let Some(scores) = result.scores.as_ref() else {
        return false;
    };
    match search_mode {
        pb::SearchModeV005::Dense => {
            scores.dense_score >= cfg.min_dense_score || scores.final_score >= cfg.min_dense_score
        }
        pb::SearchModeV005::Sparse => {
            sparse_after_boost >= cfg.min_sparse_score || scores.final_score >= cfg.min_sparse_score
        }
        pb::SearchModeV005::Hybrid | pb::SearchModeV005::Unspecified => {
            scores.final_score >= cfg.min_hybrid_score
                || scores.fusion_score >= cfg.min_hybrid_score
                || sparse_after_boost >= cfg.min_sparse_score
        }
    }
}

fn is_negative_mention_evidence(result: &pb::SearchResultV004) -> bool {
    let text = format!("{}\n{}", result.matched_text, result.parent_text).to_lowercase();
    [
        "does not mention",
        "do not mention",
        "doesn't mention",
        "does not rebuild",
        "does not prevent",
        "do not prevent",
        "differs from",
        "no mention of",
        "not a ",
        "not mentioned",
        "not prevent",
        "not reference",
        "does not reference",
        "separate from",
        "should not be confused",
        "unrelated to",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn apply_pre_mmr_no_answer_filter(
    results: &mut Vec<pb::SearchResultV004>,
    query: &str,
    query_technical_tokens: &[String],
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
    debug_enabled: bool,
    preserve_partial_evidence_for_mmr: bool,
) -> usize {
    if !cfg.enabled || results.is_empty() {
        return 0;
    }
    let before = results.len();
    let query_terms = query_term_count(query);
    let mixed_script_query = is_mixed_script_query(query);
    for result in results.iter_mut() {
        let matched_tokens = matched_technical_tokens(result, query_technical_tokens);
        apply_no_answer_exact_technical_boost(
            result,
            query_technical_tokens,
            &matched_tokens,
            cfg,
            debug_enabled,
        );
    }
    let strongly_seeded_documents = results
        .iter()
        .filter_map(|result| {
            let matched_tokens = matched_technical_tokens(result, query_technical_tokens);
            let exact_technical_match =
                complete_technical_match(query_technical_tokens, &matched_tokens);
            let sparse_after_boost = result
                .scores
                .as_ref()
                .map(|scores| scores.sparse_score)
                .unwrap_or(0.0);
            let matched_terms = matched_term_count(result, query);
            let matched_discriminating_terms = matched_discriminating_term_count(result, query);
            let leading_discriminating_match =
                leading_discriminating_query_term_matches(result, query);
            (!is_negative_mention_evidence(result)
                && !violates_query_exclusion_terms(result, query)
                && no_answer_candidate_passes(
                    result,
                    search_mode,
                    exact_technical_match,
                    sparse_after_boost,
                    matched_terms,
                    matched_discriminating_terms,
                    leading_discriminating_match,
                    query_terms,
                    mixed_script_query,
                    cfg,
                ))
            .then(|| no_answer_document_key(result))
        })
        .collect::<HashSet<_>>();
    results.retain(|result| {
        let matched_tokens = matched_technical_tokens(result, query_technical_tokens);
        let exact_technical_match =
            complete_technical_match(query_technical_tokens, &matched_tokens);
        let sparse_after_boost = result
            .scores
            .as_ref()
            .map(|scores| scores.sparse_score)
            .unwrap_or(0.0);
        let matched_terms = matched_term_count(result, query);
        let matched_discriminating_terms = matched_discriminating_term_count(result, query);
        let leading_discriminating_match = leading_discriminating_query_term_matches(result, query);
        if is_negative_mention_evidence(result) || violates_query_exclusion_terms(result, query) {
            return false;
        }
        if no_answer_candidate_passes(
            result,
            search_mode,
            exact_technical_match,
            sparse_after_boost,
            matched_terms,
            matched_discriminating_terms,
            leading_discriminating_match,
            query_terms,
            mixed_script_query,
            cfg,
        ) {
            return true;
        }
        preserve_partial_evidence_for_mmr
            && (no_answer_partial_mmr_evidence_passes(
                result,
                search_mode,
                exact_technical_match,
                sparse_after_boost,
                matched_terms,
                matched_discriminating_terms,
                cfg,
            ) || partial_multi_aspect_candidate_passes(
                result,
                query,
                search_mode,
                exact_technical_match,
                sparse_after_boost,
                matched_terms,
                matched_discriminating_terms,
                strongly_seeded_documents.contains(&no_answer_document_key(result)),
                    cfg,
                ))
    });
    let filtered = before.saturating_sub(results.len());
    filtered + prune_same_document_no_answer_siblings(results, query)
}

fn apply_segmented_pre_mmr_no_answer_filter(
    results: &mut Vec<pb::SearchResultV004>,
    plan: &QueryPlan,
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
    debug_enabled: bool,
    preserve_partial_evidence_for_mmr: bool,
) -> usize {
    if !cfg.enabled || results.is_empty() {
        return 0;
    }
    let before = results.len();
    let mut accepted = Vec::with_capacity(results.len());
    for result in results.drain(..) {
        let matched_segment_indices = result_query_segment_indices(&result);
        if plan.intent_units.is_empty() {
            let mut passed_segment_indices = Vec::new();
            let mut accepted_result = None;
            for segment_index in &matched_segment_indices {
                let Some(segment) = plan
                    .segments
                    .iter()
                    .find(|segment| segment.index == *segment_index)
                else {
                    continue;
                };
                let mut candidate = vec![result.clone()];
                let technical_tokens = strong_technical_query_tokens(&segment.text);
                apply_pre_mmr_no_answer_filter(
                    &mut candidate,
                    &segment.text,
                    &technical_tokens,
                    search_mode,
                    cfg,
                    debug_enabled,
                    preserve_partial_evidence_for_mmr,
                );
                if let Some(passed) = candidate.pop() {
                    passed_segment_indices.push(*segment_index);
                    accepted_result.get_or_insert(passed);
                }
            }
            if let Some(mut passed) = accepted_result {
                if let Some(citation) = passed.citation.as_mut() {
                    citation.metadata.insert(
                        "passed_query_segment_indices".into(),
                        serde_json::to_string(&passed_segment_indices)
                            .unwrap_or_else(|_| "[]".into()),
                    );
                }
                accepted.push(passed);
            }
            continue;
        }
        let mut passed_segment_indices = Vec::new();
        let mut passed_intent_ids = Vec::new();
        let mut intent_evidence = Vec::new();
        let mut accepted_result = None;
        for intent in plan.intent_units.iter().filter(|intent| {
            intent
                .source_segment_indices
                .iter()
                .any(|index| matched_segment_indices.contains(index))
        }) {
            let mut candidate = vec![result.clone()];
            let technical_tokens = strong_technical_query_tokens(&intent.text);
            apply_pre_mmr_no_answer_filter(
                &mut candidate,
                &intent.text,
                &technical_tokens,
                search_mode,
                cfg,
                debug_enabled,
                preserve_partial_evidence_for_mmr,
            );
            let scores = result.scores.as_ref();
            let matched_tokens = matched_technical_tokens(&result, &technical_tokens);
            let matched_terms = matched_term_count(&result, &intent.text);
            let matched_discriminating_terms =
                matched_discriminating_term_count(&result, &intent.text);
            let intents_sharing_physical_segment = plan
                .intent_units
                .iter()
                .filter(|other| other.required)
                .filter(|other| {
                    other
                        .source_segment_indices
                        .iter()
                        .any(|index| intent.source_segment_indices.contains(index))
                })
                .count();
            let independent_intent_evidence = intents_sharing_physical_segment <= 1
                || !matched_tokens.is_empty()
                || matched_discriminating_terms > 0;
            let passed = !candidate.is_empty() && independent_intent_evidence;
            if !passed {
                candidate.clear();
            }
            intent_evidence.push(CandidateIntentEvidence::direct(
                intent.id,
                scores.map(|value| value.dense_score),
                scores.map(|value| value.sparse_score),
                Some(lexical_score_for_no_answer(&result)),
                matched_terms,
                matched_tokens.len(),
                passed,
            ));
            if let Some(passed) = candidate.pop() {
                passed_intent_ids.push(intent.id);
                passed_segment_indices.extend(
                    intent
                        .source_segment_indices
                        .iter()
                        .filter(|index| matched_segment_indices.contains(index))
                        .copied(),
                );
                accepted_result.get_or_insert(passed);
            }
        }
        if let Some(mut passed) = accepted_result {
            passed_segment_indices.sort_unstable();
            passed_segment_indices.dedup();
            passed_intent_ids.sort_unstable();
            passed_intent_ids.dedup();
            if let Some(citation) = passed.citation.as_mut() {
                citation.metadata.insert(
                    "passed_query_segment_indices".into(),
                    serde_json::to_string(&passed_segment_indices).unwrap_or_else(|_| "[]".into()),
                );
                citation.metadata.insert(
                    "passed_query_intent_ids".into(),
                    serde_json::to_string(&passed_intent_ids).unwrap_or_else(|_| "[]".into()),
                );
                citation.metadata.insert(
                    "candidate_intent_evidence".into(),
                    serde_json::to_string(&intent_evidence).unwrap_or_else(|_| "[]".into()),
                );
            }
            accepted.push(passed);
        }
    }
    *results = accepted;
    before.saturating_sub(results.len())
}

fn segmented_final_no_answer_should_trigger(
    results: &[pb::SearchResultV004],
    plan: &QueryPlan,
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
) -> bool {
    if !cfg.enabled || results.is_empty() {
        return false;
    }
    !results.iter().any(|result| {
        result_passed_query_segment_indices(result)
            .into_iter()
            .filter_map(|segment_index| {
                plan.segments
                    .iter()
                    .find(|segment| segment.index == segment_index)
            })
            .any(|segment| {
                let technical_tokens = strong_technical_query_tokens(&segment.text);
                !final_no_answer_should_trigger(
                    std::slice::from_ref(result),
                    &segment.text,
                    &technical_tokens,
                    search_mode,
                    cfg,
                )
            })
    })
}

fn should_clear_post_mmr_results(
    results: &[pb::SearchResultV004],
    plan: Option<&QueryPlan>,
    query: &str,
    query_technical_tokens: &[String],
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
) -> bool {
    if plan.is_some_and(|plan| plan.mode == QueryProcessingMode::Segmented) {
        return segmented_final_no_answer_should_trigger(
            results,
            plan.expect("segmented plan should be present"),
            search_mode,
            cfg,
        );
    }
    final_no_answer_should_trigger(results, query, query_technical_tokens, search_mode, cfg)
}

fn aggregate_no_answer_candidate_passes(
    results: &[pb::SearchResultV004],
    query: &str,
    query_technical_tokens: &[String],
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
) -> bool {
    let positive_results = results
        .iter()
        .filter(|result| !is_negative_mention_evidence(result))
        .collect::<Vec<_>>();
    if positive_results.is_empty() {
        return false;
    }
    let mut by_document: HashMap<String, Vec<&pb::SearchResultV004>> = HashMap::new();
    for result in positive_results {
        by_document
            .entry(no_answer_document_key(result))
            .or_default()
            .push(result);
    }
    by_document.values().any(|group| {
        aggregate_no_answer_group_passes(group, query, query_technical_tokens, search_mode, cfg)
    })
}

fn no_answer_document_key(result: &pb::SearchResultV004) -> String {
    result
        .citation
        .as_ref()
        .and_then(|citation| {
            [
                "fixture_document_id",
                "original_document_id",
                "external_document_id",
            ]
            .into_iter()
            .find_map(|key| citation.metadata.get(key).cloned())
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| result.document_id.clone())
}

fn aggregate_no_answer_group_passes(
    results: &[&pb::SearchResultV004],
    query: &str,
    query_technical_tokens: &[String],
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
) -> bool {
    if results.is_empty() {
        return false;
    }
    if results
        .iter()
        .any(|result| violates_query_exclusion_terms(result, query))
    {
        return false;
    }
    let query_terms = query_term_count(query);
    let mixed_script_query = is_mixed_script_query(query);
    if query_terms == 0 {
        return false;
    }
    let combined_text = results
        .iter()
        .map(|result| candidate_text_for_no_answer(result))
        .collect::<Vec<_>>()
        .join("\n");
    let candidate_terms = lexical_terms(&combined_text);
    let ordered_query_terms = ordered_lexical_terms(query);
    let matched_terms = ordered_query_terms
        .iter()
        .filter(|term| candidate_terms.contains(*term))
        .count();
    let matched_discriminating_terms = ordered_query_terms
        .iter()
        .filter(|term| !is_common_retrieval_overlap_term(term))
        .filter(|term| candidate_terms.contains(*term))
        .count();
    let leading_discriminating_match = ordered_query_terms
        .iter()
        .find(|term| !is_common_retrieval_overlap_term(term))
        .map(|term| candidate_terms.contains(term))
        .unwrap_or(true);
    let exact_technical_match = !query_technical_tokens.is_empty()
        && query_technical_tokens
            .iter()
            .all(|token| combined_text.contains(&token.to_lowercase()));
    let sparse_after_boost = results
        .iter()
        .filter_map(|result| result.scores.as_ref().map(|scores| scores.sparse_score))
        .fold(0.0_f32, f32::max);
    let broad_mmr_evidence_passes = is_broad_coverage_query(query)
        && no_answer_broad_mmr_evidence_passes(
            results,
            search_mode,
            matched_terms,
            matched_discriminating_terms,
            cfg,
        );
    let mut aggregate = results
        .iter()
        .max_by(|left, right| {
            score_of(left)
                .partial_cmp(&score_of(right))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|result| (*result).clone())
        .unwrap_or_default();
    aggregate.parent_text = combined_text.clone();
    aggregate.matched_text = combined_text;
    if let Some(scores) = aggregate.scores.as_mut() {
        for result in results {
            if let Some(candidate_scores) = result.scores.as_ref() {
                scores.dense_score = scores.dense_score.max(candidate_scores.dense_score);
                scores.sparse_score = scores.sparse_score.max(candidate_scores.sparse_score);
                scores.fusion_score = scores.fusion_score.max(candidate_scores.fusion_score);
                scores.final_score = scores.final_score.max(candidate_scores.final_score);
            }
        }
    }
    no_answer_candidate_passes(
        &aggregate,
        search_mode,
        exact_technical_match,
        sparse_after_boost,
        matched_terms,
        matched_discriminating_terms,
        leading_discriminating_match,
        query_terms,
        mixed_script_query,
        cfg,
    ) || broad_mmr_evidence_passes
}

fn no_answer_broad_mmr_evidence_passes(
    results: &[&pb::SearchResultV004],
    search_mode: pb::SearchModeV005,
    matched_terms: usize,
    matched_discriminating_terms: usize,
    cfg: &NoAnswerConfig,
) -> bool {
    let mut non_root_blocks = HashSet::new();
    let mut has_mmr_metadata = false;
    let mut enough_score = false;
    for result in results {
        if !is_root_container_result(result) {
            non_root_blocks.insert(result_identity_key(result));
        }
        if result
            .citation
            .as_ref()
            .and_then(|citation| citation.metadata.get("rerank_stage"))
            .map(|stage| stage == "MMR")
            .unwrap_or(false)
        {
            has_mmr_metadata = true;
        }
        if let Some(scores) = result.scores.as_ref() {
            enough_score |= match search_mode {
                pb::SearchModeV005::Dense => {
                    scores.dense_score >= cfg.min_dense_score
                        || scores.fusion_score >= cfg.min_dense_score
                }
                pb::SearchModeV005::Sparse => {
                    scores.sparse_score >= cfg.min_sparse_score
                        || scores.fusion_score >= cfg.min_sparse_score
                }
                pb::SearchModeV005::Hybrid | pb::SearchModeV005::Unspecified => {
                    scores.fusion_score >= cfg.min_hybrid_score
                        || scores.sparse_score >= cfg.min_sparse_score
                }
            };
        }
    }
    has_mmr_metadata
        && enough_score
        && non_root_blocks.len() >= 3
        && matched_terms >= 1
        && matched_discriminating_terms >= 1
}

fn final_no_answer_should_trigger(
    results: &[pb::SearchResultV004],
    query: &str,
    query_technical_tokens: &[String],
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
) -> bool {
    if !cfg.enabled || results.is_empty() {
        return false;
    }
    let any_candidate_passes = results.iter().any(|result| {
        let matched_tokens = matched_technical_tokens(result, query_technical_tokens);
        let exact_technical_match =
            complete_technical_match(query_technical_tokens, &matched_tokens);
        let sparse_after_boost = result
            .scores
            .as_ref()
            .map(|scores| scores.sparse_score)
            .unwrap_or(0.0);
        let matched_terms = matched_term_count(result, query);
        let matched_discriminating_terms = matched_discriminating_term_count(result, query);
        let leading_discriminating_match = leading_discriminating_query_term_matches(result, query);
        let query_terms = query_term_count(query);
        let mixed_script_query = is_mixed_script_query(query);
        if is_negative_mention_evidence(result) || violates_query_exclusion_terms(result, query) {
            return false;
        }
        no_answer_candidate_passes(
            result,
            search_mode,
            exact_technical_match,
            sparse_after_boost,
            matched_terms,
            matched_discriminating_terms,
            leading_discriminating_match,
            query_terms,
            mixed_script_query,
            cfg,
        )
    });
    !(any_candidate_passes
        || aggregate_no_answer_candidate_passes(
            results,
            query,
            query_technical_tokens,
            search_mode,
            cfg,
        ))
}

fn apply_post_mmr_technical_no_answer_filter(
    results: &mut Vec<pb::SearchResultV004>,
    query: &str,
    query_technical_tokens: &[String],
    search_mode: pb::SearchModeV005,
    cfg: &NoAnswerConfig,
) -> usize {
    if !cfg.enabled || query_technical_tokens.is_empty() || results.is_empty() {
        return 0;
    }
    let before = results.len();
    let query_terms = query_term_count(query);
    let mixed_script_query = is_mixed_script_query(query);
    results.retain(|result| {
        if is_negative_mention_evidence(result) || violates_query_exclusion_terms(result, query) {
            return false;
        }
        let matched_tokens = matched_technical_tokens(result, query_technical_tokens);
        let exact_technical_match =
            complete_technical_match(query_technical_tokens, &matched_tokens);
        let sparse_after_boost = result
            .scores
            .as_ref()
            .map(|scores| scores.sparse_score)
            .unwrap_or_default();
        no_answer_candidate_passes(
            result,
            search_mode,
            exact_technical_match,
            sparse_after_boost,
            matched_term_count(result, query),
            matched_discriminating_term_count(result, query),
            leading_discriminating_query_term_matches(result, query),
            query_terms,
            mixed_script_query,
            cfg,
        )
    });
    before.saturating_sub(results.len())
}

fn result_identity_key(result: &pb::SearchResultV004) -> String {
    if let Some(source_block_id) = result_source_block_id(result) {
        return format!(
            "{}:{}:{}",
            result.access_zone_id, result.document_id, source_block_id
        );
    }
    format!("{}:{}", result.access_zone_id, result.matched_chunk_id)
}

fn prune_same_document_no_answer_siblings(
    results: &mut Vec<pb::SearchResultV004>,
    query: &str,
) -> usize {
    if results.len() < 2 || is_multi_aspect_query(query) || is_broad_coverage_query(query) {
        return 0;
    }
    let mut best_index_by_document = HashMap::<String, usize>::new();
    for (idx, result) in results.iter().enumerate() {
        let document_key = no_answer_document_key(result);
        best_index_by_document
            .entry(document_key)
            .and_modify(|existing_idx| {
                let existing = &results[*existing_idx];
                let better = score_of(result)
                    .partial_cmp(&score_of(existing))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        is_strong_lexical_candidate(result)
                            .cmp(&is_strong_lexical_candidate(existing))
                    })
                    .then_with(|| {
                        lexical_score_for_no_answer(result)
                            .partial_cmp(&lexical_score_for_no_answer(existing))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                if better.is_gt() {
                    *existing_idx = idx;
                }
            })
            .or_insert(idx);
    }
    let keep = best_index_by_document
        .into_values()
        .collect::<HashSet<_>>();
    let before = results.len();
    let mut idx = 0usize;
    results.retain(|_| {
        let keep_current = keep.contains(&idx);
        idx += 1;
        keep_current
    });
    before.saturating_sub(results.len())
}

fn retain_results_outside_rejected_parents(
    results: &mut Vec<pb::SearchResultV004>,
    rejected_parent_keys: &HashSet<(Uuid, Uuid)>,
) -> usize {
    let before = results.len();
    results.retain(|result| {
        Uuid::parse_str(&result.access_zone_id)
            .ok()
            .zip(Uuid::parse_str(&result.parent_chunk_id).ok())
            .is_none_or(|key| !rejected_parent_keys.contains(&key))
    });
    before.saturating_sub(results.len())
}

fn result_source_block_id(result: &pb::SearchResultV004) -> Option<&str> {
    result
        .citation
        .as_ref()
        .and_then(|citation| citation.metadata.get("source_block_id"))
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn is_root_container_result(result: &pb::SearchResultV004) -> bool {
    if result_source_block_id(result) == Some("doc-root") {
        return true;
    }
    result
        .citation
        .as_ref()
        .and_then(|citation| citation.metadata.get("section_path"))
        .map(|section_path| section_path.trim().eq_ignore_ascii_case("root"))
        .unwrap_or(false)
}

fn drop_root_container_results_when_document_has_evidence(
    results: &mut Vec<pb::SearchResultV004>,
) -> usize {
    let documents_with_non_root_evidence = results
        .iter()
        .filter(|result| !is_root_container_result(result))
        .map(no_answer_document_key)
        .collect::<HashSet<_>>();
    let before = results.len();
    results.retain(|result| {
        !is_root_container_result(result)
            || !documents_with_non_root_evidence.contains(&no_answer_document_key(result))
    });
    before.saturating_sub(results.len())
}

fn search_quality_run_id_filter(filters: &[pb::SearchFilterV004]) -> Option<String> {
    filters
        .iter()
        .find(|filter| filter.key == "quality_run_id")
        .map(|filter| filter.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn lexical_parent_score(parent: &ParentContextRecord, query: &str) -> f32 {
    let evidence_text = parent_lexical_evidence_text(parent);
    if exact_evidence_phrase_match(&evidence_text, query) {
        return 1.0;
    }
    let candidate_terms = lexical_terms(&evidence_text);
    let query_terms = lexical_terms(query);
    if query_terms.is_empty() {
        return 0.0;
    }
    let matched = query_terms
        .iter()
        .filter(|term| candidate_terms.contains(*term))
        .count();
    (matched as f32 / query_terms.len() as f32).clamp(0.0, 1.0)
}

fn parent_lexical_evidence_text(parent: &ParentContextRecord) -> String {
    let mut parts = vec![parent.content.clone()];
    for key in ["document_title", "title", "heading", "section_path"] {
        if let Some(value) = parent.metadata.get(key).and_then(serde_json::Value::as_str) {
            if !value.trim().is_empty() {
                parts.push(value.to_string());
            }
        }
    }
    if let Some(source_block_id) = parent
        .source_block_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if let Some((heading, section_path, _, _)) =
            logical_block_location(&parent.metadata, source_block_id)
        {
            if !heading.is_empty() {
                parts.push(heading);
            }
            if !section_path.is_empty() {
                parts.push(section_path);
            }
        }
    }
    parts.join("\n")
}

fn sibling_sequence_bonus(parent: &ParentContextRecord) -> f32 {
    if parent.sequence_no <= 0 {
        return 0.0;
    }
    let bounded = parent.sequence_no.clamp(1, 20) as f32;
    ((21.0 - bounded) / 20.0) * 0.12
}

fn is_broad_coverage_query(query: &str) -> bool {
    let query = query.to_lowercase();
    [
        "aspect",
        "aspects",
        "mechanism",
        "mechanisms",
        "checklist",
        "inspect",
        "coverage",
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn is_multi_aspect_query(query: &str) -> bool {
    let normalized = query.to_lowercase();
    let clauses = normalized
        .split([',', ';'])
        .flat_map(|segment| segment.split(" and "))
        .flat_map(|segment| segment.split(" or "))
        .filter(|segment| {
            ordered_lexical_terms(segment)
                .into_iter()
                .any(|term| !is_common_retrieval_overlap_term(&term))
        })
        .count();
    clauses >= 2
        || ["aspect", "aspects", "checklist", "coverage"]
            .iter()
            .any(|needle| normalized.contains(needle))
}

fn reinforce_broad_coverage_results(
    results: &mut Vec<pb::SearchResultV004>,
    candidates: &[pb::SearchResultV004],
    final_limit: usize,
) -> usize {
    if results.is_empty() || candidates.is_empty() || final_limit == 0 {
        return 0;
    }
    let (canonical_candidates, _) = dedup_results_by_chunk(candidates.to_vec(), 5);
    let mut selected = results
        .iter()
        .map(result_identity_key)
        .collect::<HashSet<_>>();
    let mut selected_by_document: HashMap<String, usize> = HashMap::new();
    for result in results.iter() {
        *selected_by_document
            .entry(no_answer_document_key(result))
            .or_default() += 1;
    }
    let mut candidate_groups: HashMap<String, Vec<pb::SearchResultV004>> = HashMap::new();
    for candidate in &canonical_candidates {
        if is_root_container_result(candidate) || is_negative_mention_evidence(candidate) {
            continue;
        }
        let document_key = no_answer_document_key(candidate);
        if selected_by_document
            .get(&document_key)
            .copied()
            .unwrap_or(0)
            < 2
        {
            continue;
        }
        candidate_groups
            .entry(document_key)
            .or_default()
            .push(candidate.clone());
    }
    let mut document_order = selected_by_document
        .into_iter()
        .filter(|(document_key, count)| *count >= 2 && candidate_groups.contains_key(document_key))
        .collect::<Vec<_>>();
    document_order.sort_by(|(left_doc, left_count), (right_doc, right_count)| {
        right_count.cmp(left_count).then_with(|| {
            document_max_score(candidate_groups.get(right_doc).into_iter().flatten())
                .partial_cmp(&document_max_score(
                    candidate_groups.get(left_doc).into_iter().flatten(),
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let mut inserted = 0usize;
    for (document_key, _) in document_order {
        let Some(group) = candidate_groups.get_mut(&document_key) else {
            continue;
        };
        group.sort_by(|left, right| {
            candidate_sequence_no(left)
                .cmp(&candidate_sequence_no(right))
                .then_with(|| {
                    score_of(right)
                        .partial_cmp(&score_of(left))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        for candidate in group.iter().take(6) {
            let key = result_identity_key(candidate);
            if selected.contains(&key) {
                continue;
            }
            if results.len() < final_limit {
                results.push(candidate.clone());
                selected.insert(key);
                inserted += 1;
                continue;
            }
            let Some(replace_idx) = broad_coverage_replacement_index(results, &document_key) else {
                continue;
            };
            let old_key = result_identity_key(&results[replace_idx]);
            selected.remove(&old_key);
            results[replace_idx] = candidate.clone();
            selected.insert(key);
            inserted += 1;
        }
    }
    inserted
}

fn document_max_score<'a>(results: impl Iterator<Item = &'a pb::SearchResultV004>) -> f32 {
    results.map(score_of).fold(0.0_f32, f32::max)
}

fn candidate_sequence_no(result: &pb::SearchResultV004) -> i32 {
    result
        .citation
        .as_ref()
        .and_then(|citation| citation.metadata.get("sequence_no"))
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(i32::MAX)
}

fn broad_coverage_replacement_index(
    results: &[pb::SearchResultV004],
    document_key: &str,
) -> Option<usize> {
    results
        .iter()
        .enumerate()
        .filter(|(_, result)| no_answer_document_key(result) != document_key)
        .min_by(|(_, left), (_, right)| {
            score_of(left)
                .partial_cmp(&score_of(right))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .or_else(|| {
            results
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    candidate_sequence_no(left)
                        .cmp(&candidate_sequence_no(right))
                        .then_with(|| {
                            score_of(right)
                                .partial_cmp(&score_of(left))
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                })
                .map(|(idx, _)| idx)
        })
}

fn exact_evidence_phrase_match(candidate: &str, query: &str) -> bool {
    let candidate = normalized_phrase(candidate);
    let query = normalized_phrase(query);
    if candidate.len() < 24 || query.len() < 24 {
        return false;
    }
    query.contains(&candidate) || candidate.contains(&query)
}

fn normalized_phrase(text: &str) -> String {
    text.split_whitespace()
        .map(|part| {
            part.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
                .to_lowercase()
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn logical_block_location(
    metadata: &serde_json::Value,
    source_block_id: &str,
) -> Option<(String, String, u32, u32)> {
    let blocks = metadata
        .get("logical_blocks")
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())?;
    blocks.as_array()?.iter().find_map(|block| {
        if block.get("block_id")?.as_str()? != source_block_id {
            return None;
        }
        let location = block.get("source_location")?;
        let heading = location
            .get("heading")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let section_path = location
            .get("section_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let page_start = location
            .get("page_start")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as u32;
        let page_end = location
            .get("page_end")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as u32;
        Some((heading, section_path, page_start, page_end))
    })
}

fn search_result_from_lexical_parent(
    parent: &ParentContextRecord,
    query: &str,
) -> pb::SearchResultV004 {
    let mut metadata: std::collections::HashMap<String, String> = parent
        .metadata
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    metadata.insert("retrieval_source".into(), "POSTGRES_FTS".into());
    metadata.insert("retrieval_sources".into(), "[\"POSTGRES_FTS\"]".into());
    metadata.insert("chunk_granularity".into(), "PARENT".into());
    metadata.insert("representation_type".into(), "ORIGINAL".into());
    metadata.insert("sequence_no".into(), parent.sequence_no.to_string());
    let score = lexical_parent_score(parent, query);
    let source_block_id = parent
        .source_block_id
        .clone()
        .or_else(|| metadata.get("source_block_id").cloned())
        .unwrap_or_default();
    let matched_text = parent.content.clone();
    if !source_block_id.is_empty() {
        if let Some((heading, section_path, page_start, page_end)) =
            logical_block_location(&parent.metadata, &source_block_id)
        {
            if !heading.is_empty() {
                metadata.entry("heading".into()).or_insert(heading);
            }
            if !section_path.is_empty() {
                metadata
                    .entry("section_path".into())
                    .or_insert(section_path);
            }
            metadata.insert("page_start".into(), page_start.to_string());
            metadata.insert("page_end".into(), page_end.to_string());
        }
        metadata.insert("source_block_id".into(), source_block_id);
    }
    pb::SearchResultV004 {
        document_id: parent.document_id.to_string(),
        document_version: parent.document_version as u64,
        root_chunk_id: parent.root_chunk_id.to_string(),
        source_chunk_id: parent.source_chunk_id.to_string(),
        parent_chunk_id: parent.id.to_string(),
        matched_chunk_id: parent.id.to_string(),
        matched_granularity: granularity_from_str("PARENT"),
        parent_text: parent.content.clone(),
        scores: Some(pb::SearchScoresV004 {
            dense_score: 0.0,
            sparse_score: score,
            fusion_score: score,
            final_score: score,
        }),
        citation: Some(pb::SearchCitationV004 { metadata }),
        access_zone_id: parent.access_zone_id.to_string(),
        access_level: parent.access_level as i32,
        matched_text,
    }
}

fn search_result_from_hit(
    parent: &ParentContextRecord,
    hit: &QdrantSearchHit,
    matched_text: String,
    trace: Option<&crate::persistence::ChunkTraceRecord>,
) -> pb::SearchResultV004 {
    let matched_chunk_id = hit
        .payload
        .get("chunk_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let matched_granularity = hit
        .payload
        .get("chunk_granularity")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let mut metadata: std::collections::HashMap<String, String> = parent
        .metadata
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    metadata.insert("sequence_no".into(), parent.sequence_no.to_string());
    if let Some(trace) = trace {
        if let Some(source_block_id) = &trace.source_block_id {
            metadata.insert("source_block_id".into(), source_block_id.clone());
        }
        if let Some(obj) = trace.source_location.as_object() {
            for key in [
                "page_start",
                "page_end",
                "char_start",
                "char_end",
                "section_path",
                "heading",
                "table_id",
                "row_index",
                "column_index",
            ] {
                if let Some(value) = obj.get(key) {
                    if let Some(s) = value.as_str() {
                        if !s.is_empty() {
                            metadata.insert(key.into(), s.into());
                        }
                    } else if value.is_number() {
                        metadata.insert(key.into(), value.to_string());
                    }
                }
            }
            if let Some(page) = obj.get("page_start").and_then(serde_json::Value::as_u64) {
                if let Some(source_uri) = metadata
                    .get("source_uri")
                    .cloned()
                    .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
                {
                    metadata
                        .entry("page_url".into())
                        .or_insert_with(|| format!("{source_uri}?page={page}"));
                }
            }
        }
        if let Some(source_links) = trace.source_links.as_array().filter(|a| !a.is_empty()) {
            if let Ok(json) = serde_json::to_string(source_links) {
                metadata.insert("matched_source_links".into(), json);
            }
        }
        if let Some(obj) = trace.metadata.as_object() {
            for (k, v) in obj {
                if k.starts_with("source_") || k == "document_title" || k == "mime_type" {
                    if let Some(s) = v.as_str() {
                        metadata.entry(k.clone()).or_insert_with(|| s.to_string());
                    }
                }
            }
        }
    }
    if let Some(point_id) = hit
        .payload
        .get("qdrant_point_id")
        .and_then(serde_json::Value::as_str)
        .filter(|v| !v.is_empty())
    {
        metadata.insert("qdrant_point_id".into(), point_id.to_string());
    } else if matched_granularity != "GRAPH_EXPANDED" {
        metadata.insert("qdrant_point_id".into(), hit.id.to_string());
    }
    for key in [
        "binding_id",
        "representation_type",
        "dense_version",
        "model_version",
        "payload_version",
        "chunk_granularity",
        "source_chunk_granularity",
        "query_processing_mode",
        "query_segment_indices",
    ] {
        if let Some(value) = hit.payload.get(key) {
            if let Some(s) = value.as_str() {
                if !s.is_empty() {
                    metadata.insert(key.into(), s.into());
                }
            } else if value.is_number() {
                metadata.insert(key.into(), value.to_string());
            }
        }
    }
    if metadata
        .get("source_block_id")
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        if let Some(source_block_id) = hit
            .payload
            .get("source_block_id")
            .and_then(serde_json::Value::as_str)
            .filter(|v| !v.is_empty())
        {
            metadata.insert("source_block_id".into(), source_block_id.into());
        }
    }
    pb::SearchResultV004 {
        document_id: parent.document_id.to_string(),
        document_version: parent.document_version as u64,
        root_chunk_id: parent.root_chunk_id.to_string(),
        source_chunk_id: parent.source_chunk_id.to_string(),
        parent_chunk_id: parent.id.to_string(),
        matched_chunk_id,
        matched_granularity: granularity_from_str(matched_granularity),
        parent_text: parent.content.clone(),
        scores: Some(pb::SearchScoresV004 {
            dense_score: hit.dense_score,
            sparse_score: hit.sparse_score,
            fusion_score: hit.fusion_score,
            final_score: hit.score.max(0.0),
        }),
        citation: Some(pb::SearchCitationV004 { metadata }),
        access_zone_id: parent.access_zone_id.to_string(),
        access_level: parent.access_level as i32,
        matched_text,
    }
}
fn deadline_from(m: &MetadataMap, fallback: u64) -> Instant {
    let d = m
        .get("grpc-timeout")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_timeout)
        .unwrap_or(Duration::from_millis(fallback));
    Instant::now() + d
}

fn grpc_transport_deadline(metadata: &MetadataMap, started: Instant) -> Option<Instant> {
    metadata
        .get("grpc-timeout")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_timeout)
        .map(|timeout| started + timeout)
}
fn parse_timeout(s: &str) -> Option<Duration> {
    // gRPC timeout format is <digits><unit>, e.g. "100m", "2S", "1H".
    // fix463: split into (number, unit). The previous code inverted these and
    // silently ignored valid grpc-timeout headers by falling back to config timeouts.
    let (num, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: std::num::NonZeroU64 = num
        .parse::<u64>()
        .ok()
        .and_then(std::num::NonZeroU64::new)?;
    Some(match unit.chars().next()? {
        'H' => Duration::from_secs(n.get() * 3600),
        'M' => Duration::from_secs(n.get() * 60),
        'S' => Duration::from_secs(n.get()),
        'm' => Duration::from_millis(n.get()),
        'u' => Duration::from_micros(n.get()),
        'n' => Duration::from_nanos(n.get()),
        _ => return None,
    })
}

fn effective_query_timeout_ms(requested: u64, configured_max: u64) -> u64 {
    if requested == 0 {
        configured_max
    } else {
        requested.min(configured_max)
    }
}

#[cfg(test)]
mod fix483p_long_query_hardening_tests {
    use super::*;
    use crate::query_processing::classification::QuerySegmentKind;

    fn segment(index: usize, text: &str) -> QuerySegment {
        QuerySegment {
            index,
            text: text.into(),
            token_count: text.split_whitespace().count(),
            source_token_start: index,
            source_token_end: index + text.split_whitespace().count(),
            source_byte_start: 0,
            source_byte_end: text.len(),
            original_byte_start: 0,
            original_byte_end: text.len(),
            kind: QuerySegmentKind::Question,
            has_question_form: true,
            has_technical_identifier: false,
            searchable: true,
            weight: 1.0,
            required_for_coverage: true,
            intent_unit_ids: Vec::new(),
            sha256: format!("segment-{index}"),
        }
    }

    fn normalized_query(text: &str) -> crate::query_processing::NormalizedQuery {
        crate::query_processing::NormalizedQuery {
            original_text: text.into(),
            normalized_text: text.into(),
            normalized_to_original_byte_map: (0..=text.len()).collect(),
            token_offsets: Vec::new(),
        }
    }

    fn result_for_segment(index: usize, text: &str, score: f32) -> pb::SearchResultV004 {
        let mut metadata = HashMap::new();
        metadata.insert("query_segment_indices".into(), format!("[{index}]"));
        pb::SearchResultV004 {
            access_zone_id: "00000000-0000-0000-0000-000000000001".into(),
            document_id: format!("document-{index}"),
            document_version: 1,
            parent_chunk_id: format!("parent-{index}"),
            matched_chunk_id: format!("chunk-{index}"),
            parent_text: text.into(),
            matched_text: text.into(),
            scores: Some(pb::SearchScoresV004 {
                dense_score: score,
                sparse_score: score,
                fusion_score: score,
                final_score: score,
            }),
            citation: Some(pb::SearchCitationV004 { metadata }),
            ..Default::default()
        }
    }

    fn intent(id: usize, text: &str) -> crate::query_processing::QueryIntentUnit {
        crate::query_processing::QueryIntentUnit {
            id,
            kind: crate::query_processing::QueryIntentKind::ExplicitQuestion,
            text: text.into(),
            source_segment_indices: vec![0],
            source_token_start: 0,
            source_token_end: 12,
            normalized_byte_start: 0,
            normalized_byte_end: text.len(),
            original_byte_start: 0,
            original_byte_end: text.len(),
            required: true,
            searchable: true,
            weight: 1.0,
            normalized_sha256: format!("intent-{id}"),
        }
    }

    #[test]
    fn long_query_deadline_is_selected_and_client_timeout_caps_it() {
        let cfg = AppConfig::load().expect("load test config");
        assert_eq!(
            effective_query_timeout_ms(0, cfg.grpc.deadlines.query_ms),
            cfg.grpc.deadlines.query_ms
        );
        assert_eq!(
            effective_query_timeout_ms(0, cfg.search.query_processing.standard.deadline_ms),
            cfg.search.query_processing.standard.deadline_ms
        );
        assert_eq!(
            effective_query_timeout_ms(25, cfg.search.query_processing.standard.deadline_ms),
            25
        );
    }

    #[test]
    fn pre_and_post_mmr_no_answer_are_segment_aware() {
        let plan = QueryPlan {
            original_query: "background unrelated words and legal hold cleanup".into(),
            normalized_query: normalized_query("background unrelated words and legal hold cleanup"),
            original_token_count: 7,
            mode: QueryProcessingMode::Segmented,
            tier: QueryProcessingTier::SegmentedStandard,
            profile_version: "test".into(),
            limits: crate::query_processing::EffectiveQueryProcessingLimits::for_segmented(
                &crate::config::QueryProcessingConfig::default(),
                &crate::config::QueryProcessingConfig::default().standard,
            ),
            segments: vec![
                segment(0, "background unrelated words"),
                segment(1, "legal hold cleanup"),
            ],
            intent_units: Vec::new(),
        };
        let mut results = vec![result_for_segment(
            1,
            "legal hold cleanup prevents expiration deletion",
            1.0,
        )];
        let cfg = NoAnswerConfig::default();
        let filtered = apply_segmented_pre_mmr_no_answer_filter(
            &mut results,
            &plan,
            pb::SearchModeV005::Dense,
            &cfg,
            false,
            false,
        );
        assert_eq!(filtered, 0);
        assert_eq!(result_passed_query_segment_indices(&results[0]), vec![1]);
        assert!(!segmented_final_no_answer_should_trigger(
            &results,
            &plan,
            pb::SearchModeV005::Dense,
            &cfg,
        ));
    }

    #[test]
    fn one_physical_segment_does_not_credit_unrelated_intent() {
        let query = "Why is PostgreSQL the source of truth? How does legal hold affect TTL?";
        let mut shared_segment = segment(0, query);
        shared_segment.intent_unit_ids = vec![0, 1];
        let plan = QueryPlan {
            original_query: query.into(),
            normalized_query: normalized_query(query),
            original_token_count: query.split_whitespace().count(),
            mode: QueryProcessingMode::Segmented,
            tier: QueryProcessingTier::SegmentedStandard,
            profile_version: "test".into(),
            limits: crate::query_processing::EffectiveQueryProcessingLimits::for_segmented(
                &crate::config::QueryProcessingConfig::default(),
                &crate::config::QueryProcessingConfig::default().standard,
            ),
            segments: vec![shared_segment],
            intent_units: vec![
                intent(0, "Why is PostgreSQL the source of truth?"),
                intent(1, "How does legal hold affect TTL?"),
            ],
        };
        let mut results = vec![result_for_segment(
            0,
            "PostgreSQL is the canonical source of truth for document visibility.",
            1.0,
        )];

        let filtered = apply_segmented_pre_mmr_no_answer_filter(
            &mut results,
            &plan,
            pb::SearchModeV005::Dense,
            &NoAnswerConfig::default(),
            false,
            false,
        );

        assert_eq!(filtered, 0);
        assert_eq!(result_passed_query_intent_ids(&results[0]), vec![0]);
        let evidence = results[0]
            .citation
            .as_ref()
            .and_then(|citation| citation.metadata.get("candidate_intent_evidence"))
            .and_then(|raw| serde_json::from_str::<Vec<CandidateIntentEvidence>>(raw).ok())
            .expect("candidate-to-intent evidence metadata");
        assert!(evidence
            .iter()
            .any(|item| item.intent_id == 0 && item.evidence_passed));
        assert!(evidence
            .iter()
            .any(|item| item.intent_id == 1 && !item.evidence_passed));
        let coverage = coverage_for_results(&plan, &results);
        assert_eq!(coverage.status, QueryEvidenceStatus::Degraded);
        assert_eq!(coverage.required_covered, 1);
        assert_eq!(coverage.uncovered_required_intent_ids, vec![1]);
    }

    #[test]
    fn required_segment_candidate_is_mmr_protected() {
        let plan = QueryPlan {
            original_query: "first question second question".into(),
            normalized_query: normalized_query("first question second question"),
            original_token_count: 4,
            mode: QueryProcessingMode::Segmented,
            tier: QueryProcessingTier::SegmentedStandard,
            profile_version: "test".into(),
            limits: crate::query_processing::EffectiveQueryProcessingLimits::for_segmented(
                &crate::config::QueryProcessingConfig::default(),
                &crate::config::QueryProcessingConfig::default().standard,
            ),
            segments: vec![segment(0, "first question"), segment(1, "second question")],
            intent_units: Vec::new(),
        };
        let mut results = vec![
            result_for_segment(0, "first evidence", 0.9),
            result_for_segment(1, "second evidence", 0.8),
        ];
        assert!(!reserve_required_segment_coverage(&mut results, &plan, 2));
        assert!(results.iter().all(|result| {
            result
                .citation
                .as_ref()
                .and_then(|citation| citation.metadata.get("ranking_protection"))
                .is_some_and(|value| value.contains("REQUIRED_SEGMENT_COVERAGE"))
        }));
    }

    #[tokio::test]
    async fn weighted_admission_permit_is_released_on_drop() {
        let semaphore = Arc::new(Semaphore::new(6));
        let cancellation = CancellationToken::new();
        let permit = AstraVectorV004ControlService::acquire_backpressure_permit(
            semaphore.clone(),
            50,
            "test_weighted_admission",
            3,
            &cancellation,
        )
        .await
        .unwrap();
        assert_eq!(semaphore.available_permits(), 3);
        drop(permit);
        assert_eq!(semaphore.available_permits(), 6);
    }

    #[tokio::test]
    async fn weighted_admission_honors_cancellation_without_leaking() {
        let semaphore = Arc::new(Semaphore::new(1));
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = AstraVectorV004ControlService::acquire_backpressure_permit(
            semaphore.clone(),
            50,
            "test_weighted_admission",
            2,
            &cancellation,
        )
        .await
        .err()
        .expect("cancelled admission must fail");
        assert_eq!(error.code(), tonic::Code::Cancelled);
        assert_eq!(semaphore.available_permits(), 1);
    }

    #[test]
    fn transport_deadline_is_measured_from_request_receipt() {
        let mut metadata = MetadataMap::new();
        metadata.insert("grpc-timeout", "25m".parse().unwrap());
        let started = Instant::now();
        let deadline = grpc_transport_deadline(&metadata, started).unwrap();
        assert_eq!(deadline.duration_since(started), Duration::from_millis(25));
    }
}

fn hash_text(t: &str) -> String {
    let c = t.nfc().collect::<String>().replace("\r\n", "\n");
    hex::encode(Sha256::digest(c.as_bytes()))
}
fn request_hash(r: &pb::EncodeBatchRequest, items: &[(pb::EncodeItem, String)]) -> String {
    let mut h = Sha256::new();
    for x in [
        &r.tenant_id,
        &r.workspace_id,
        &r.purpose.to_string(),
        &r.access_level.to_string(),
        &r.persistence_mode.to_string(),
        &r.expected_contract_version,
    ] {
        h.update(x.as_bytes());
        h.update([0])
    }
    for x in &r.requested_representations {
        h.update(x.to_be_bytes())
    }
    for (i, t) in items {
        for x in [
            &i.chunk_id,
            &i.chunk_type.to_string(),
            i.parent_chunk_id.as_deref().unwrap_or(""),
            t,
        ] {
            h.update(x.as_bytes());
            h.update([0])
        }
    }
    hex::encode(h.finalize())
}
fn cache_key(r: &pb::EncodeBatchRequest, i: &pb::EncodeItem, t: &str, c: &AppConfig) -> String {
    let mut h = Sha256::new();
    for x in [
        &r.tenant_id,
        &r.workspace_id,
        t,
        &r.purpose.to_string(),
        &i.chunk_type.to_string(),
        &c.tokenizer.version,
        &c.model.version,
        &c.dense.version,
        &c.sparse.version,
        "cls_v1",
        "l2_v1",
    ] {
        h.update(x.as_bytes());
        h.update([0])
    }
    for x in &r.requested_representations {
        h.update(x.to_be_bytes())
    }
    hex::encode(h.finalize())
}
fn to_pb(
    i: &pb::EncodeItem,
    r: &EmbeddingResult,
    l1: bool,
    l2: bool,
    c: &AppConfig,
) -> pb::EncodeItemResponse {
    pb::EncodeItemResponse {
        chunk_id: i.chunk_id.clone(),
        chunk_type: i.chunk_type,
        parent_chunk_id: i.parent_chunk_id.clone(),
        status: pb::ItemStatus::ItemCompleted as i32,
        dense: r.dense.as_ref().map(|v| pb::DenseRepresentation {
            name: c.dense.name.clone(),
            values: v.clone(),
            dimension: v.len() as u32,
            normalized: true,
            distance: c.dense.distance.clone(),
            version: c.dense.version.clone(),
        }),
        learned_sparse: match (&r.sparse_indices, &r.sparse_values) {
            (Some(x), Some(v)) => Some(pb::SparseRepresentation {
                name: c.sparse.name.clone(),
                indices: x.clone(),
                values: v.clone(),
                version: c.sparse.version.clone(),
            }),
            _ => None,
        },
        model_input_token_count: r.token_count as u32,
        truncated: r.truncated,
        l1_cache_hit: l1,
        l2_cache_hit: l2,
        error_code: None,
        error_message: None,
    }
}
fn failed_pb(i: &pb::EncodeItem, e: &AstraError) -> pb::EncodeItemResponse {
    pb::EncodeItemResponse {
        chunk_id: i.chunk_id.clone(),
        chunk_type: i.chunk_type,
        parent_chunk_id: i.parent_chunk_id.clone(),
        status: pb::ItemStatus::ItemFailed as i32,
        dense: None,
        learned_sparse: None,
        model_input_token_count: 0,
        truncated: false,
        l1_cache_hit: false,
        l2_cache_hit: false,
        error_code: Some("ITEM_ERROR".into()),
        error_message: Some(e.to_string()),
    }
}

#[cfg(test)]
mod v007_fix1_tests {
    use super::*;

    fn test_result(chunk_id: &str, text: &str, score: f32) -> pb::SearchResultV004 {
        pb::SearchResultV004 {
            document_id: "doc".into(),
            document_version: 1,
            root_chunk_id: chunk_id.into(),
            source_chunk_id: chunk_id.into(),
            parent_chunk_id: chunk_id.into(),
            matched_chunk_id: chunk_id.into(),
            matched_granularity: pb::ChunkGranularityV004::ParentV004 as i32,
            parent_text: text.into(),
            scores: Some(pb::SearchScoresV004 {
                dense_score: score,
                sparse_score: 0.0,
                fusion_score: score,
                final_score: score,
            }),
            citation: Some(pb::SearchCitationV004 {
                metadata: std::collections::HashMap::new(),
            }),
            access_zone_id: "zone".into(),
            access_level: pb::AccessLevel::Public as i32,
            matched_text: text.into(),
        }
    }

    #[test]
    fn graph_expanded_evidence_is_detected_for_no_answer_gate() {
        let mut graph = test_result("g1", "related evidence explains the answer", 0.1);
        graph.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"VECTOR_DIRECT\",\"GRAPH_EXPANDED\"]".into(),
        );
        assert!(has_graph_expanded_evidence(&[graph]));
    }

    #[test]
    fn score_merge_preserves_direct_origin_when_graph_duplicate_scores_higher() {
        let mut direct = test_result("direct-child", "canonical direct evidence", 0.4);
        direct.parent_chunk_id = "shared-parent".into();
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "VECTOR_DIRECT".into());
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "canonical-a1".into());

        let mut graph = test_result("graph-child", "graph duplicate evidence", 0.9);
        graph.parent_chunk_id = "shared-parent".into();
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "GRAPH_EXPANDED".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"GRAPH_EXPANDED\"]".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "canonical-a1".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_relation_type".into(), "CHUNK_HAS_PARENT".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_edge_id".into(), "edge-direct-parent".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_related_chunk_id".into(), "graph-child".into());

        let merged = merge_score_then_truncate(vec![direct], vec![graph], 10);

        assert_eq!(merged.results.len(), 1);
        let result = &merged.results[0];
        assert_eq!(result.matched_chunk_id, "direct-child");
        assert_eq!(score_of(result), 0.4);
        let citation = result.citation.as_ref().unwrap();
        assert_eq!(
            citation
                .metadata
                .get("retrieval_source")
                .map(String::as_str),
            Some("VECTOR_DIRECT")
        );
        assert!(extraction_retrieval_sources(result)
            .iter()
            .any(|source| source == "GRAPH_EXPANDED"));
        assert!(citation.metadata.contains_key("graph_relations"));
        assert_eq!(
            citation
                .metadata
                .get("graph_secondary_provenance")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            citation.metadata.get("graph_edge_id").map(String::as_str),
            Some("edge-direct-parent")
        );
        assert_eq!(
            citation
                .metadata
                .get("graph_related_chunk_id")
                .map(String::as_str),
            Some("graph-child")
        );
        assert!(!is_graph_expanded_result(result));
    }

    #[test]
    fn graph_secondary_provenance_does_not_cross_parent_scope_into_direct_duplicate() {
        let mut direct = test_result("direct-child", "canonical direct evidence", 0.4);
        direct.parent_chunk_id = "parent-a1".into();
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "VECTOR_DIRECT".into());
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "canonical-a1".into());

        let mut graph = test_result("graph-child", "graph duplicate evidence", 0.9);
        graph.parent_chunk_id = "parent-a1".into();
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "GRAPH_EXPANDED".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"GRAPH_EXPANDED\"]".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "canonical-a1".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_relation_type".into(), "REPAIRED_BY".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_seed_parent_chunk_id".into(), "parent-a3".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_related_parent_chunk_id".into(), "parent-a1".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_edge_id".into(), "edge-hop-limit".into());

        let merged = merge_score_then_truncate(vec![direct], vec![graph], 10);

        assert_eq!(merged.results.len(), 1);
        let result = &merged.results[0];
        let citation = result.citation.as_ref().unwrap();
        assert_eq!(
            citation
                .metadata
                .get("retrieval_source")
                .map(String::as_str),
            Some("VECTOR_DIRECT")
        );
        assert_eq!(
            extraction_retrieval_sources(result),
            vec!["VECTOR_DIRECT".to_string()]
        );
        assert!(!citation.metadata.contains_key("graph_secondary_provenance"));
        assert!(!citation.metadata.contains_key("graph_relation_type"));
        assert!(!citation.metadata.contains_key("graph_edge_id"));
        assert!(!citation.metadata.contains_key("graph_relations"));
    }

    #[test]
    fn chunk_dedup_promotes_direct_origin_even_when_graph_is_seen_first() {
        let mut direct = test_result("direct-child", "canonical direct evidence", 0.4);
        let mut graph = test_result("graph-child", "graph duplicate evidence", 0.9);
        for (result, source) in [
            (&mut direct, "VECTOR_DIRECT"),
            (&mut graph, "GRAPH_EXPANDED"),
        ] {
            result
                .citation
                .as_mut()
                .unwrap()
                .metadata
                .insert("retrieval_source".into(), source.into());
            result
                .citation
                .as_mut()
                .unwrap()
                .metadata
                .insert("source_block_id".into(), "canonical-a1".into());
        }

        let (deduplicated, count) = dedup_results_by_chunk(vec![direct, graph], 5);

        assert_eq!(count, 1);
        assert_eq!(deduplicated.len(), 1);
        assert_eq!(deduplicated[0].matched_chunk_id, "direct-child");
        assert_eq!(
            primary_retrieval_source(&deduplicated[0]),
            Some("VECTOR_DIRECT")
        );
    }

    #[test]
    fn graph_append_does_not_reintroduce_unselected_direct_duplicate() {
        let mut selected_direct = test_result("direct-a", "selected direct", 0.9);
        let mut unselected_direct = test_result("direct-b", "unselected direct", 0.4);
        let mut graph_duplicate = test_result("graph-b", "duplicate graph", 0.8);
        let mut unique_graph = test_result("graph-c", "unique graph", 0.7);
        for (result, source, block) in [
            (&mut selected_direct, "VECTOR_DIRECT", "block-a"),
            (&mut unselected_direct, "VECTOR_DIRECT", "block-b"),
            (&mut graph_duplicate, "GRAPH_EXPANDED", "block-b"),
            (&mut unique_graph, "GRAPH_EXPANDED", "block-c"),
        ] {
            result
                .citation
                .as_mut()
                .unwrap()
                .metadata
                .insert("retrieval_source".into(), source.into());
            result
                .citation
                .as_mut()
                .unwrap()
                .metadata
                .insert("source_block_id".into(), block.into());
        }

        let selection = select_graph_append_with_group_mmr(
            vec![selected_direct, unselected_direct],
            vec![graph_duplicate, unique_graph],
            2,
            1,
            1,
            false,
            0.75,
            0.75,
            30,
            "TOKEN_JACCARD",
            "TOKEN_JACCARD",
            true,
            true,
            5,
        );

        let ids = selection
            .results
            .iter()
            .map(|result| result.matched_chunk_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["direct-a", "graph-c"]);
    }

    #[test]
    fn broad_coverage_reinsertion_prefers_direct_duplicate_origin() {
        let mut selected_a = test_result("selected-a", "selected A", 0.9);
        let mut selected_b = test_result("selected-b", "selected B", 0.8);
        let mut direct = test_result("direct-c", "direct C", 0.4);
        let mut graph = test_result("graph-c", "graph duplicate C", 0.9);
        for result in [&mut selected_a, &mut selected_b, &mut direct, &mut graph] {
            result.document_id = "shared-document".into();
        }
        for (result, source) in [
            (&mut direct, "VECTOR_DIRECT"),
            (&mut graph, "GRAPH_EXPANDED"),
        ] {
            result
                .citation
                .as_mut()
                .unwrap()
                .metadata
                .insert("retrieval_source".into(), source.into());
            result
                .citation
                .as_mut()
                .unwrap()
                .metadata
                .insert("source_block_id".into(), "block-c".into());
            result
                .citation
                .as_mut()
                .unwrap()
                .metadata
                .insert("sequence_no".into(), "3".into());
        }
        let mut results = vec![selected_a, selected_b];

        reinforce_broad_coverage_results(&mut results, &[direct, graph], 3);

        assert!(results
            .iter()
            .any(|result| result.matched_chunk_id == "direct-c"));
        assert!(!results
            .iter()
            .any(|result| result.matched_chunk_id == "graph-c"));
    }

    #[test]
    fn negative_graph_expanded_evidence_does_not_bypass_no_answer() {
        let mut graph = test_result("g1", "this does not mention the requested answer", 0.1);
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"GRAPH_EXPANDED\"]".into());
        assert!(!has_graph_expanded_evidence(&[graph]));
    }

    #[test]
    fn separate_from_statement_is_negative_evidence() {
        let result = test_result(
            "audit",
            "Audit evidence uses retention rules separate from vector outbox.",
            0.5,
        );
        assert!(is_negative_mention_evidence(&result));
    }

    #[test]
    fn multi_aspect_query_requires_multiple_meaningful_clauses() {
        assert!(is_multi_aspect_query(
            "How does repair handle missing and orphan points?"
        ));
        assert!(!is_multi_aspect_query(
            "How does repair handle missing points?"
        ));
        assert!(!is_broad_coverage_query(
            "How does repair handle missing and orphan points?"
        ));
    }

    #[test]
    fn multi_aspect_mmr_admission_preserves_strong_document_sibling() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "How does repair handle missing and orphan points?";
        let mut seed = test_result(
            "repair",
            "Repair handles orphan points and missing point recovery.",
            0.8,
        );
        seed.citation.as_mut().unwrap().metadata.insert(
            "fixture_document_id".into(),
            "reconciliation-runbook".into(),
        );
        let mut metrics = test_result(
            "metrics",
            "Reconciliation repair metrics report scanned and repaired counts.",
            0.5,
        );
        metrics.citation.as_mut().unwrap().metadata.insert(
            "fixture_document_id".into(),
            "reconciliation-runbook".into(),
        );
        let mut negative = test_result(
            "negative",
            "This repair note is separate from orphan point reconciliation metrics.",
            0.9,
        );
        negative.citation.as_mut().unwrap().metadata.insert(
            "fixture_document_id".into(),
            "reconciliation-runbook".into(),
        );

        let mut candidates = vec![seed, metrics, negative];
        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            true,
        );

        assert_eq!(filtered, 1);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.matched_chunk_id == "repair"));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.matched_chunk_id == "metrics"));
        assert!(!candidates
            .iter()
            .any(|candidate| candidate.matched_chunk_id == "negative"));
    }

    #[test]
    fn weak_overlap_is_not_preserved_when_request_disables_mmr() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "How is the upstream connection pool repaired after HTTP 502 failures?";
        let mut candidates = vec![
            test_result(
                "pool",
                "Summer membership includes access to a heated swimming pool.",
                0.2,
            ),
            test_result(
                "repair",
                "The service center repairs payment terminals and displays.",
                0.2,
            ),
        ];
        for candidate in &mut candidates {
            let scores = candidate.scores.as_mut().unwrap();
            scores.sparse_score = 0.2;
            scores.fusion_score = 0.2;
            scores.final_score = 0.2;
        }

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            true,
        );

        let remaining = candidates
            .iter()
            .map(|candidate| candidate.parent_chunk_id.clone())
            .collect::<Vec<_>>();
        if filtered != 2 {
            panic!("filtered={filtered} remaining={remaining:?}");
        }
        assert!(candidates.is_empty());
    }

    #[test]
    fn hybrid_no_answer_uses_branch_confidence_not_only_rrf_scale() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "How much can a zone alpha employee transfer internally?";
        let mut candidate = test_result(
            "zone-alpha-transfer-001-b1",
            "Internal transfer limit is 1 000 000 KZT for zone alpha.",
            0.03,
        );
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.75;
            scores.sparse_score = 0.27;
            scores.fusion_score = 0.03;
            scores.final_score = 0.03;
        }
        candidate.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"POSTGRES_FTS\",\"VECTOR_DIRECT\"]".into(),
        );
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.33".into());
        let mut candidates = vec![candidate];

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            false,
        );

        assert_eq!(filtered, 0);
        assert_eq!(candidates.len(), 1);
        assert!(!final_no_answer_should_trigger(
            &candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg
        ));
    }

    #[test]
    fn hybrid_no_answer_preserves_complete_technical_evidence_in_multilingual_query() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query =
            "Как AstraVector обрабатывает ошибку ORA-00904 для таблицы content_chunks_v004?";
        let technical_tokens = strong_technical_query_tokens(query);
        assert!(technical_tokens.len() >= 2);
        let mut candidate = test_result(
            "canonical-child-001",
            "ORA-00904 was recorded for content_chunks_v004 during canonical validation.",
            0.03,
        );
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.59;
            scores.sparse_score = 0.34;
            scores.fusion_score = 0.03;
            scores.final_score = 0.03;
        }
        let mut candidates = vec![candidate];

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &technical_tokens,
            pb::SearchModeV005::Hybrid,
            &cfg,
            true,
            false,
        );

        assert_eq!(filtered, 0);
        assert_eq!(candidates.len(), 1);
        assert!(!final_no_answer_should_trigger(
            &candidates,
            query,
            &technical_tokens,
            pb::SearchModeV005::Hybrid,
            &cfg
        ));
        let metadata = &candidates[0].citation.as_ref().unwrap().metadata;
        assert_eq!(
            metadata.get("exact_technical_match").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn hybrid_no_answer_rejects_partial_technical_match_in_multilingual_query() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query =
            "Как AstraVector обрабатывает ошибку ORA-00904 для таблицы content_chunks_v004?";
        let technical_tokens = strong_technical_query_tokens(query);
        assert!(technical_tokens.len() >= 2);
        let mut candidate = test_result(
            "unrelated-error-child",
            "ORA-00904 appears in an unrelated migration diagnostic.",
            0.04,
        );
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.10;
            scores.sparse_score = 0.60;
            scores.fusion_score = 0.04;
            scores.final_score = 0.04;
        }
        let mut candidates = vec![candidate];

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &technical_tokens,
            pb::SearchModeV005::Hybrid,
            &cfg,
            true,
            false,
        );

        assert_eq!(filtered, 1);
        assert!(candidates.is_empty());
    }

    #[test]
    fn post_mmr_technical_filter_removes_weak_sibling_without_dropping_exact_evidence() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Объясни parent_chunk_id и /api/v1/search при canonical validation.";
        let technical_tokens = strong_technical_query_tokens(query);
        let mut exact = test_result(
            "canonical-child",
            "parent_chunk_id is validated by /api/v1/search against canonical state.",
            0.03,
        );
        let mut weak = test_result(
            "large-parent",
            "A large parent appendix remains bounded and independent.",
            0.03,
        );
        for candidate in [&mut exact, &mut weak] {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.47;
            scores.sparse_score = 0.21;
            scores.fusion_score = 0.03;
            scores.final_score = 0.03;
        }
        let exact_tokens = matched_technical_tokens(&exact, &technical_tokens);
        apply_no_answer_exact_technical_boost(
            &mut exact,
            &technical_tokens,
            &exact_tokens,
            &cfg,
            true,
        );
        let mut candidates = vec![exact, weak];

        let filtered = apply_post_mmr_technical_no_answer_filter(
            &mut candidates,
            query,
            &technical_tokens,
            pb::SearchModeV005::Hybrid,
            &cfg,
        );

        assert_eq!(filtered, 1);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].matched_chunk_id, "canonical-child");
    }

    #[test]
    fn graph_disabled_canonical_query_keeps_specific_parent_and_drops_unrelated_sibling() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Find authoritative document-state evidence without Graph expansion.";

        let mut source = test_result(
            "source-a-parent",
            "AstraVector stores canonical document state in PostgreSQL and uses Qdrant projections.",
            0.035,
        );
        source.parent_chunk_id = "source-a-parent".into();
        source.matched_chunk_id = "source-a-parent".into();
        {
            let scores = source.scores.as_mut().unwrap();
            scores.dense_score = 0.56;
            scores.sparse_score = 0.41;
            scores.fusion_score = 0.035;
            scores.final_score = 0.035;
        }
        source.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"POSTGRES_FTS\",\"VECTOR_DIRECT\"]".into(),
        );
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "source-a".into());
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("ranking_protection".into(), "PRIMARY_DIRECT,UNIQUE_SOURCE_BLOCK".into());
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        let mut canonical = test_result(
            "child-a1-260",
            "ASTRA_CANONICAL_STATE_A1. PostgreSQL is the authoritative canonical state for the Qdrant projection.",
            0.034,
        );
        canonical.parent_chunk_id = "parent-a1".into();
        canonical.matched_chunk_id = "child-a1-260".into();
        {
            let scores = canonical.scores.as_mut().unwrap();
            scores.dense_score = 0.54;
            scores.sparse_score = 0.10;
            scores.fusion_score = 0.034;
            scores.final_score = 0.034;
        }
        canonical.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"POSTGRES_FTS\",\"VECTOR_DIRECT\"]".into(),
        );
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a1".into());
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        let mut reconciliation = test_result(
            "child-a3-180",
            "ASTRA_RECONCILIATION_A3. Missing Qdrant points are detected and republished from canonical bindings.",
            0.033,
        );
        reconciliation.parent_chunk_id = "parent-a3".into();
        reconciliation.matched_chunk_id = "child-a3-180".into();
        {
            let scores = reconciliation.scores.as_mut().unwrap();
            scores.dense_score = 0.48;
            scores.sparse_score = 0.27;
            scores.fusion_score = 0.033;
            scores.final_score = 0.033;
        }
        reconciliation.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"POSTGRES_FTS\",\"VECTOR_DIRECT\"]".into(),
        );
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a3".into());
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        let mut large_parent = test_result(
            "parent-large",
            "ASTRA_LARGE_PARENT_ONLY_A canonical-state evidence remains bounded and independent.",
            0.034,
        );
        large_parent.parent_chunk_id = "parent-large".into();
        large_parent.matched_chunk_id = "parent-large".into();
        {
            let scores = large_parent.scores.as_mut().unwrap();
            scores.dense_score = 0.55;
            scores.sparse_score = 0.11;
            scores.fusion_score = 0.034;
            scores.final_score = 0.034;
        }
        large_parent.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"POSTGRES_FTS\",\"VECTOR_DIRECT\"]".into(),
        );
        large_parent
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-large".into());
        large_parent
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        let mut candidates = vec![source, canonical, reconciliation, large_parent];
        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            true,
        );

        assert_eq!(filtered, 3);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].parent_chunk_id, "parent-a1");
        assert!(!final_no_answer_should_trigger(
            &candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg
        ));
    }

    #[test]
    fn root_section_path_is_treated_as_root_container() {
        let mut source = test_result(
            "source-a-parent",
            "AstraVector stores canonical document state in PostgreSQL and uses Qdrant projections.",
            0.035,
        );
        source.parent_chunk_id = "source-a-parent".into();
        source.matched_chunk_id = "source-a-parent".into();
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "source-a".into());
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "root".into());

        assert!(is_root_container_result(&source));
    }

    #[test]
    fn strict_lexical_support_rejects_weak_sibling_contexts() {
        let query = "Find authoritative document-state evidence without Graph expansion.";
        let query_terms = query_term_count(query);

        let mut lifecycle = test_result(
            "child-a2-180",
            "ASTRA_LEGAL_HOLD_A2. This section is the only authoritative fixture evidence for legal hold and TTL.",
            0.028,
        );
        lifecycle.parent_chunk_id = "parent-a2".into();
        lifecycle.matched_chunk_id = "child-a2-180".into();
        lifecycle
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a2".into());
        lifecycle
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("heading".into(), "Legal hold and TTL".into());
        lifecycle
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "operations/lifecycle".into());

        let mut appendix = test_result(
            "parent-large",
            "ASTRA_LARGE_PARENT_ONLY_A canonical-state evidence remains bounded and independent.",
            0.034,
        );
        appendix.parent_chunk_id = "parent-large".into();
        appendix.matched_chunk_id = "parent-large".into();
        appendix
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-large".into());
        appendix
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("heading".into(), "Large canonical-state appendix".into());
        appendix
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "operations/large-context".into());

        let matched_terms = matched_term_count(&lifecycle, query);
        let matched_discriminating_terms = matched_discriminating_term_count(&lifecycle, query);
        let leading_discriminating_match =
            leading_discriminating_query_term_matches(&lifecycle, query);
        assert!(!strict_lexical_query_match(
            matched_terms,
            matched_discriminating_terms,
            leading_discriminating_match,
            query_terms,
        ));

        let matched_terms = matched_term_count(&appendix, query);
        let matched_discriminating_terms = matched_discriminating_term_count(&appendix, query);
        let leading_discriminating_match =
            leading_discriminating_query_term_matches(&appendix, query);
        assert!(!strict_lexical_query_match(
            matched_terms,
            matched_discriminating_terms,
            leading_discriminating_match,
            query_terms,
        ));
    }

    #[test]
    fn graph_enabled_negative_slice_triggers_no_answer_after_common_overlap_only() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "How do I reset an AstraVector user password?";

        let mut source = test_result(
            "source-a-parent",
            "AstraVector stores canonical document state in PostgreSQL and uses Qdrant projections.",
            0.036,
        );
        source.parent_chunk_id = "source-a-parent".into();
        source.matched_chunk_id = "source-a-parent".into();
        {
            let scores = source.scores.as_mut().unwrap();
            scores.dense_score = 0.53;
            scores.sparse_score = 0.17;
            scores.fusion_score = 0.036;
            scores.final_score = 0.036;
        }
        source.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"GRAPH_EXPANDED\",\"POSTGRES_FTS\",\"VECTOR_DIRECT\"]".into(),
        );
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "source-a".into());
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("ranking_protection".into(), "PRIMARY_DIRECT,UNIQUE_SOURCE_BLOCK".into());
        source
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        let mut reconciliation = test_result(
            "child-a3-180",
            "ASTRA_RECONCILIATION_A3. Missing Qdrant points are detected and republished from canonical bindings.",
            0.031,
        );
        reconciliation.parent_chunk_id = "parent-a3".into();
        reconciliation.matched_chunk_id = "child-a3-180".into();
        {
            let scores = reconciliation.scores.as_mut().unwrap();
            scores.dense_score = 0.44;
            scores.sparse_score = 0.05;
            scores.fusion_score = 0.031;
            scores.final_score = 0.031;
        }
        reconciliation.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"GRAPH_EXPANDED\",\"VECTOR_DIRECT\"]".into(),
        );
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a3".into());
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        let candidates = vec![source, reconciliation];
        assert!(final_no_answer_should_trigger(
            &candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg
        ));
    }

    #[test]
    fn multilingual_graph_disabled_direct_only_keeps_canonical_parent() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Почему PostgreSQL является canonical state для Qdrant projection?";

        let mut canonical = test_result(
            "child-a1-260",
            "ASTRA_CANONICAL_STATE_A1. PostgreSQL is the authoritative canonical state for the Qdrant projection.",
            0.031,
        );
        canonical.parent_chunk_id = "parent-a1".into();
        canonical.matched_chunk_id = "child-a1-260".into();
        {
            let scores = canonical.scores.as_mut().unwrap();
            scores.dense_score = 0.6753512;
            scores.sparse_score = 0.25254804;
            scores.fusion_score = 0.031318814;
            scores.final_score = 0.031318814;
        }
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a1".into());
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("heading".into(), "PostgreSQL canonical state".into());
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "operations/canonical-state".into());

        let mut reconciliation = test_result(
            "child-a3-180",
            "ASTRA_RECONCILIATION_A3. Missing Qdrant points are detected and republished from canonical bindings.",
            0.030,
        );
        reconciliation.parent_chunk_id = "parent-a3".into();
        reconciliation.matched_chunk_id = "child-a3-180".into();
        {
            let scores = reconciliation.scores.as_mut().unwrap();
            scores.dense_score = 0.48065647;
            scores.sparse_score = 0.2681214;
            scores.fusion_score = 0.030550372;
            scores.final_score = 0.030550372;
        }
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a3".into());
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("heading".into(), "Qdrant reconciliation".into());
        reconciliation
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "operations/reconciliation".into());

        let mut appendix = test_result(
            "parent-large",
            "ASTRA_LARGE_PARENT_ONLY_A canonical-state evidence remains bounded and independent.",
            0.027,
        );
        appendix.parent_chunk_id = "parent-large".into();
        appendix.matched_chunk_id = "parent-large".into();
        {
            let scores = appendix.scores.as_mut().unwrap();
            scores.dense_score = 0.32515;
            scores.sparse_score = 0.10712104;
            scores.fusion_score = 0.027984343;
            scores.final_score = 0.027984343;
        }
        appendix
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
        appendix
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-large".into());
        appendix
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("heading".into(), "Large canonical-state appendix".into());
        appendix
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "operations/large-context".into());

        let mut candidates = vec![canonical, reconciliation, appendix];
        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            true,
        );

        let remaining = candidates
            .iter()
            .map(|candidate| candidate.parent_chunk_id.clone())
            .collect::<Vec<_>>();
        if filtered != 2 {
            panic!("filtered={filtered} remaining={remaining:?}");
        }
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].parent_chunk_id, "parent-a1");
    }

    #[test]
    fn mixed_script_dense_only_query_keeps_best_direct_candidate() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Что подтверждает authoritative state документа без Graph expansion?";

        let mut canonical = test_result(
            "child-a1-260",
            "ASTRA_CANONICAL_STATE_A1. PostgreSQL is the authoritative canonical state for the Qdrant projection.",
            0.032,
        );
        canonical.parent_chunk_id = "parent-a1".into();
        canonical.matched_chunk_id = "child-a1-260".into();
        {
            let scores = canonical.scores.as_mut().unwrap();
            scores.dense_score = 0.544541;
            scores.sparse_score = 0.09610293;
            scores.fusion_score = 0.03201844;
            scores.final_score = 0.03201844;
        }
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a1".into());
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("heading".into(), "PostgreSQL canonical state".into());
        canonical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "operations/canonical-state".into());

        let mut lifecycle = test_result(
            "child-a2-180",
            "ASTRA_LEGAL_HOLD_A2. This section is the only authoritative fixture evidence for legal hold and TTL.",
            0.025,
        );
        lifecycle.parent_chunk_id = "parent-a2".into();
        lifecycle.matched_chunk_id = "child-a2-180".into();
        {
            let scores = lifecycle.scores.as_mut().unwrap();
            scores.dense_score = 0.44949526;
            scores.sparse_score = 0.13748428;
            scores.fusion_score = 0.025322013;
            scores.final_score = 0.025322013;
        }
        lifecycle
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());
        lifecycle
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a2".into());
        lifecycle
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("heading".into(), "Legal hold and TTL".into());
        lifecycle
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("section_path".into(), "operations/lifecycle".into());

        let mut candidates = vec![canonical, lifecycle];
        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            true,
        );

        assert_eq!(filtered, 1);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].parent_chunk_id, "parent-a1");
    }

    #[test]
    fn graph_expansion_does_not_bypass_post_mmr_no_answer_gate() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "How do I reset an AstraVector user password?";

        let mut graph = test_result(
            "child-a3-180",
            "ASTRA_RECONCILIATION_A3. Missing Qdrant points are detected and republished from canonical bindings.",
            0.031,
        );
        graph.parent_chunk_id = "parent-a3".into();
        graph.matched_chunk_id = "child-a3-180".into();
        {
            let scores = graph.scores.as_mut().unwrap();
            scores.dense_score = 0.44;
            scores.sparse_score = 0.05;
            scores.fusion_score = 0.031;
            scores.final_score = 0.031;
        }
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "GRAPH_EXPANDED".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_secondary_provenance".into(), "true".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a3".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        assert!(has_graph_expanded_evidence(&[graph.clone()]));
        assert!(should_clear_post_mmr_results(
            &[graph],
            None,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
        ));
    }

    #[test]
    fn document_only_overlap_does_not_satisfy_no_answer_gate() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Does the document contain a Kubernetes backup procedure?";

        let mut candidate = test_result(
            "canonical-parent",
            "ASTRA_CANONICAL_STATE_A1. PostgreSQL is the authoritative canonical state for document versions, content chunks, lifecycle and access visibility.",
            0.031,
        );
        candidate.parent_chunk_id = "parent-a1".into();
        candidate.matched_chunk_id = "child-a1-180".into();
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.48;
            scores.sparse_score = 0.05;
            scores.fusion_score = 0.031;
            scores.final_score = 0.031;
        }
        candidate.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"VECTOR_DIRECT\",\"POSTGRES_FTS\"]".into(),
        );
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a1".into());
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.12".into());

        assert!(should_clear_post_mmr_results(
            &[candidate],
            None,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
        ));
    }

    #[test]
    fn exclusion_terms_are_detected_multilingually() {
        let english = excluded_query_terms(
            "Explain legal hold and TTL without using reconciliation evidence.",
        );
        assert!(english.contains("reconciliation"));

        let russian =
            excluded_query_terms("Опиши legal hold и TTL, не используя информацию о reconciliation.");
        assert!(russian.contains("reconciliation"));

        let kazakh =
            excluded_query_terms("Reconciliation туралы айтпай, legal hold және TTL түсіндір.");
        assert!(kazakh.contains("reconciliation"));
    }

    #[test]
    fn exclusion_clause_rejects_reconciliation_dependent_candidate() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Explain legal hold and TTL without using reconciliation evidence.";

        let mut candidate = test_result(
            "legal-hold-parent",
            "ASTRA_LEGAL_HOLD_A2. TTL normally removes expired searchable representations after lifecycle checks and reconciliation.",
            0.035,
        );
        candidate.parent_chunk_id = "parent-a2".into();
        candidate.matched_chunk_id = "child-a2-180".into();
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.46;
            scores.sparse_score = 0.40;
            scores.fusion_score = 0.035;
            scores.final_score = 0.035;
        }
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\",\"POSTGRES_FTS\"]".into());
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "parent-a2".into());
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "1.5".into());
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("strong_lexical_evidence".into(), "true".into());

        assert!(should_clear_post_mmr_results(
            &[candidate],
            None,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
        ));
    }

    #[test]
    fn hybrid_no_answer_rejects_partial_overlap_hard_negative() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Which dragon passport is required for zone alpha transfers?";
        let mut candidate = test_result(
            "zone-alpha-transfer-001-b1",
            "Internal transfer limit is 1 000 000 KZT for zone alpha.",
            0.04,
        );
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.76;
            scores.sparse_score = 0.30;
            scores.fusion_score = 0.04;
            scores.final_score = 0.04;
        }
        candidate.citation.as_mut().unwrap().metadata.insert(
            "retrieval_sources".into(),
            "[\"POSTGRES_FTS\",\"VECTOR_DIRECT\"]".into(),
        );
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.33".into());
        let mut candidates = vec![candidate];

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            false,
        );

        assert_eq!(filtered, 1);
        assert!(candidates.is_empty());
    }

    #[test]
    fn hybrid_no_answer_preserves_proves_confirms_lexical_evidence() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "What proves that a notification was seen by the user?";
        let mut candidate = test_result(
            "dist-receipt-001",
            "Receipt acknowledgement confirms a user saw a notification.",
            0.03,
        );
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.21;
            scores.sparse_score = 0.30;
            scores.fusion_score = 0.03;
            scores.final_score = 0.03;
        }
        candidate
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("lexical_score".into(), "0.45".into());
        let mut candidates = vec![candidate];

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            false,
        );

        assert_eq!(filtered, 0);
        assert_eq!(candidates.len(), 1);
        assert!(!final_no_answer_should_trigger(
            &candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg
        ));
    }

    #[test]
    fn hybrid_no_answer_preserves_sparse_strong_lexical_evidence_without_metadata() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let query = "Which claim records that a user acknowledged a notification?";
        let mut candidate = test_result(
            "dist-inbox-001",
            "Inbox claim handles user notification acknowledgement and delivery receipts.",
            0.03,
        );
        {
            let scores = candidate.scores.as_mut().unwrap();
            scores.dense_score = 0.05;
            scores.sparse_score = 0.20;
            scores.fusion_score = 0.03;
            scores.final_score = 0.03;
        }
        let mut candidates = vec![candidate];

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            false,
        );

        assert_eq!(filtered, 0);
        assert_eq!(candidates.len(), 1);
        assert!(!final_no_answer_should_trigger(
            &candidates,
            query,
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg
        ));
    }

    #[test]
    fn lexical_terms_normalize_common_retrieval_paraphrases() {
        let restored =
            lexical_terms("missing Qdrant points should be restored from canonical store");
        assert!(restored.contains("repair"));
        assert!(restored.contains("postgresql"));
        assert!(restored.contains("source"));
        assert!(restored.contains("truth"));

        let tenant_key = lexical_terms("tenant-scoped payload key filtering");
        assert!(tenant_key.contains("access_zone_id"));
        assert!(tenant_key.contains("index"));

        let acknowledgement =
            lexical_terms("claim records that a user acknowledged a notification");
        assert!(acknowledgement.contains("handles"));
        assert!(acknowledgement.contains("acknowledge"));

        let fixture = lexical_terms("technical file path identifier fixtures");
        assert!(fixture.contains("fixture"));
        assert!(fixture.contains("file"));
        assert!(fixture.contains("path"));
        assert!(fixture.contains("identifier"));

        let security = lexical_terms("Which gateway runbook block contains the security guidance?");
        assert!(security.contains("gateway"));
        assert!(security.contains("runbook"));
        assert!(security.contains("security"));
        assert!(security.contains("threat"));
        assert!(security.contains("model"));
        assert!(!security.contains("block"));
        assert!(!security.contains("guidance"));

        let duplicate = lexical_terms(
            "Find the unique duplicate-outbox evidence while avoiding redundant chunks.",
        );
        assert!(duplicate.contains("duplicate-outbox"));
        assert!(duplicate.contains("outbox"));
        assert!(!duplicate.contains("unique"));
        assert!(!duplicate.contains("redundant"));
        assert!(!duplicate.contains("chunks"));

        let quality = lexical_terms("quality benchmark");
        assert!(quality.contains("quality"));
        assert!(quality.contains("bench"));
    }

    #[test]
    fn ranking_protection_does_not_bypass_no_answer() {
        let cfg = NoAnswerConfig {
            enabled: true,
            ..Default::default()
        };
        let mut protected = test_result(
            "protected-weak",
            "Summer membership includes access to a heated swimming pool.",
            0.01,
        );
        protected
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("ranking_protection".into(), "PRIMARY_DIRECT".into());
        let mut candidates = vec![protected];

        let filtered = apply_pre_mmr_no_answer_filter(
            &mut candidates,
            "How is the upstream connection pool repaired after HTTP 502 failures?",
            &[],
            pb::SearchModeV005::Hybrid,
            &cfg,
            false,
            true,
        );

        assert_eq!(filtered, 1);
        assert!(candidates.is_empty());
    }

    #[test]
    fn ranking_protection_does_not_bypass_hard_token_budget() {
        let cfg = crate::config::RagContextConfig {
            token_budget_enabled: true,
            max_context_tokens: 20,
            reserved_answer_tokens: 0,
            tokenizer_safety_margin_percent: 0,
            chars_per_token: 1,
            huge_chunk_strategy: "DROP_HUGE_CHUNK".into(),
            ..Default::default()
        };
        let mut protected = test_result("protected-huge", &"x".repeat(100), 0.9);
        protected
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("ranking_protection".into(), "PRIMARY_DIRECT".into());
        let mut results = vec![protected];

        let (dropped, _, count) = apply_token_budget_truncation(&mut results, &cfg);

        assert!(results.is_empty());
        assert_eq!(dropped, vec!["protected-huge"]);
        assert_eq!(count, 1);
    }

    #[test]
    fn graph_token_fraction_is_enforced() {
        let cfg = crate::config::RagContextConfig {
            token_budget_enabled: true,
            max_context_tokens: 200,
            reserved_answer_tokens: 0,
            tokenizer_safety_margin_percent: 0,
            chars_per_token: 1,
            max_graph_token_fraction: 0.25,
            ..Default::default()
        };
        let direct = test_result("direct", "primary", 0.8);
        let mut graph = test_result("graph", &"g".repeat(80), 0.9);
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "GRAPH_EXPANDED".into());
        let mut results = vec![direct, graph];

        apply_token_budget_truncation(&mut results, &cfg);

        assert!(results
            .iter()
            .any(|result| result.matched_chunk_id == "direct"));
        assert!(!results
            .iter()
            .any(|result| result.matched_chunk_id == "graph"));
    }

    #[test]
    fn mmr_disabled_keeps_score_order() {
        let candidates = vec![
            test_result("a", "alpha credit repayment", 0.7),
            test_result("b", "beta branch address", 0.9),
        ];
        let result = apply_mmr_rerank(
            candidates,
            2,
            false,
            0.75,
            30,
            "DENSE_EMBEDDING",
            "TOKEN_JACCARD",
        );
        assert!(!result.enabled);
        assert_eq!(result.results[0].matched_chunk_id, "b");
    }

    #[test]
    fn mmr_enabled_adds_metadata_and_selects_limit() {
        let candidates = vec![
            test_result("a", "early loan repayment without fee", 0.95),
            test_result("b", "early loan repayment no commission", 0.90),
            test_result("c", "branch address and service schedule", 0.80),
        ];
        let result = apply_mmr_rerank(
            candidates,
            2,
            true,
            0.75,
            30,
            "DENSE_EMBEDDING",
            "TOKEN_JACCARD",
        );
        assert!(result.enabled);
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.selected_count, 2);
        assert!(result.results[0]
            .citation
            .as_ref()
            .unwrap()
            .metadata
            .contains_key("mmr_score"));
    }

    #[test]
    fn mmr_tie_breaking_is_deterministic_across_candidate_order() {
        let baseline = vec![
            test_result("d", "delta reconciliation", 0.8),
            test_result("b", "beta reconciliation", 0.8),
            test_result("a", "alpha reconciliation", 0.8),
            test_result("c", "gamma reconciliation", 0.8),
        ];
        let expected = apply_mmr_rerank(
            baseline.clone(),
            3,
            true,
            0.75,
            30,
            "TOKEN_JACCARD",
            "TOKEN_JACCARD",
        )
        .results
        .into_iter()
        .map(|result| result.matched_chunk_id)
        .collect::<Vec<_>>();

        for offset in 0..50 {
            let mut shuffled = baseline.clone();
            let len = shuffled.len();
            shuffled.rotate_left(offset % len);
            let actual = apply_mmr_rerank(
                shuffled,
                3,
                true,
                0.75,
                30,
                "TOKEN_JACCARD",
                "TOKEN_JACCARD",
            )
            .results
            .into_iter()
            .map(|result| result.matched_chunk_id)
            .collect::<Vec<_>>();
            assert_eq!(actual, expected, "candidate rotation {offset}");
        }
    }

    #[test]
    fn graph_seed_ranking_preserves_strong_lexical_evidence() {
        let zone = Uuid::from_u128(1);
        let weak_high_score = GraphSeedCandidate {
            key: (zone, Uuid::from_u128(30)),
            parent_key: (zone, Uuid::from_u128(30)),
            score: 0.95,
            matched_terms: 1,
            matched_discriminating_terms: 0,
            strong_lexical_evidence: false,
            intent_unit_ids: Vec::new(),
        };
        let strong_lower_score = GraphSeedCandidate {
            key: (zone, Uuid::from_u128(20)),
            parent_key: (zone, Uuid::from_u128(20)),
            score: 0.55,
            matched_terms: 3,
            matched_discriminating_terms: 2,
            strong_lexical_evidence: true,
            intent_unit_ids: vec![0],
        };
        let strong_stable_tie = GraphSeedCandidate {
            key: (zone, Uuid::from_u128(10)),
            parent_key: (zone, Uuid::from_u128(10)),
            score: 0.55,
            matched_terms: 3,
            matched_discriminating_terms: 2,
            strong_lexical_evidence: true,
            intent_unit_ids: vec![1],
        };
        let mut candidates = [weak_high_score, strong_lower_score, strong_stable_tie];

        candidates.sort_by(compare_graph_seed_candidates);

        assert_eq!(candidates[0].key.1, Uuid::from_u128(10));
        assert_eq!(candidates[1].key.1, Uuid::from_u128(20));
        assert_eq!(candidates[2].key.1, Uuid::from_u128(30));
    }

    #[test]
    fn graph_seed_selection_reserves_required_intents_and_enforces_cap() {
        let zone = Uuid::from_u128(1);
        let candidates = (0..15)
            .map(|index| GraphSeedCandidate {
                key: (zone, Uuid::from_u128(index + 1)),
                parent_key: (zone, Uuid::from_u128(index + 1)),
                score: 1.0 - index as f32 / 100.0,
                matched_terms: 2,
                matched_discriminating_terms: 1,
                strong_lexical_evidence: true,
                intent_unit_ids: if index == 14 { vec![2] } else { vec![1] },
            })
            .collect();
        let selected = select_graph_seed_candidates(candidates, &[1, 2], 12);
        assert_eq!(selected.len(), 12);
        assert!(selected
            .iter()
            .any(|candidate| candidate.intent_unit_ids.contains(&1)));
        assert!(selected
            .iter()
            .any(|candidate| candidate.intent_unit_ids.contains(&2)));
    }

    #[test]
    fn graph_seed_cap_keeps_sibling_representations_of_selected_parent_group() {
        let zone = Uuid::from_u128(1);
        let target_parent = Uuid::from_u128(100);
        let target_children = [Uuid::from_u128(101), Uuid::from_u128(102)];
        let mut candidates = vec![
            GraphSeedCandidate {
                key: (zone, target_children[0]),
                parent_key: (zone, target_parent),
                score: 1.0,
                matched_terms: 4,
                matched_discriminating_terms: 2,
                strong_lexical_evidence: true,
                intent_unit_ids: vec![0],
            },
            GraphSeedCandidate {
                key: (zone, target_children[1]),
                parent_key: (zone, target_parent),
                score: 0.4,
                matched_terms: 1,
                matched_discriminating_terms: 0,
                strong_lexical_evidence: false,
                intent_unit_ids: vec![0],
            },
        ];
        candidates.extend((0..12).map(|index| GraphSeedCandidate {
            key: (zone, Uuid::from_u128(200 + index)),
            parent_key: (zone, Uuid::from_u128(300 + index)),
            score: 0.9 - index as f32 / 100.0,
            matched_terms: 3,
            matched_discriminating_terms: 1,
            strong_lexical_evidence: true,
            intent_unit_ids: vec![0],
        }));

        let selected = select_graph_seed_candidates(candidates, &[0], 12);
        let selected_keys = selected
            .iter()
            .map(|candidate| candidate.key)
            .collect::<HashSet<_>>();

        assert_eq!(selected.len(), 12);
        assert!(selected_keys.contains(&(zone, target_children[0])));
        assert!(selected_keys.contains(&(zone, target_children[1])));
    }

    #[test]
    fn graph_seed_preserves_matched_child_identity_with_parent_fallback() {
        let parent = Uuid::from_u128(10);
        let matched_subchunk = Uuid::from_u128(20);
        let result = pb::SearchResultV004 {
            parent_chunk_id: parent.to_string(),
            matched_chunk_id: matched_subchunk.to_string(),
            ..Default::default()
        };

        assert_eq!(graph_seed_chunk_id(&result), Some(matched_subchunk));

        let result_without_matched = pb::SearchResultV004 {
            parent_chunk_id: parent.to_string(),
            ..Default::default()
        };
        assert_eq!(graph_seed_chunk_id(&result_without_matched), Some(parent));
    }

    #[test]
    fn graph_seed_sources_keep_all_child_representations_of_admitted_parents() {
        let admitted_parent = Uuid::from_u128(10).to_string();
        let fallback_parent = Uuid::from_u128(11).to_string();
        let excluded_parent = Uuid::from_u128(12).to_string();
        let zone = Uuid::from_u128(1).to_string();
        let direct = vec![
            pb::SearchResultV004 {
                access_zone_id: zone.clone(),
                matched_chunk_id: admitted_parent.clone(),
                parent_chunk_id: admitted_parent.clone(),
                ..Default::default()
            },
            pb::SearchResultV004 {
                access_zone_id: zone.clone(),
                matched_chunk_id: fallback_parent.clone(),
                parent_chunk_id: fallback_parent.clone(),
                ..Default::default()
            },
        ];
        let children = vec![
            pb::SearchResultV004 {
                access_zone_id: zone.clone(),
                matched_chunk_id: Uuid::from_u128(101).to_string(),
                parent_chunk_id: admitted_parent.clone(),
                ..Default::default()
            },
            pb::SearchResultV004 {
                access_zone_id: zone.clone(),
                matched_chunk_id: Uuid::from_u128(102).to_string(),
                parent_chunk_id: admitted_parent,
                ..Default::default()
            },
            pb::SearchResultV004 {
                access_zone_id: zone,
                matched_chunk_id: Uuid::from_u128(103).to_string(),
                parent_chunk_id: excluded_parent,
                ..Default::default()
            },
        ];

        let selected = graph_seed_source_results_for_admitted_parents(&direct, &children);
        let selected_ids = selected
            .iter()
            .map(|result| result.matched_chunk_id.as_str())
            .collect::<HashSet<_>>();

        assert_eq!(selected.len(), 3);
        assert!(selected_ids.contains(Uuid::from_u128(101).to_string().as_str()));
        assert!(selected_ids.contains(Uuid::from_u128(102).to_string().as_str()));
        assert!(selected_ids.contains(fallback_parent.as_str()));
        assert!(!selected_ids.contains(Uuid::from_u128(103).to_string().as_str()));
    }

    #[test]
    fn graph_seed_score_uses_branch_relevance_instead_of_rrf_scale() {
        let result = pb::SearchResultV004 {
            scores: Some(pb::SearchScoresV004 {
                dense_score: 0.72,
                sparse_score: 0.31,
                fusion_score: 0.016,
                final_score: 0.016,
            }),
            ..Default::default()
        };

        assert!((graph_seed_score(&result) - 0.72).abs() < f32::EPSILON);
    }

    #[test]
    fn ranking_trace_records_first_drop_reason_without_document_text() {
        let retained = test_result("retained", "confidential retained text", 0.8);
        let dropped = test_result("dropped", "confidential dropped text", 0.7);
        let mut collector = RankingTraceCollector::new(true, 10, 10);
        collector.observe(
            pb::RankingStageV005::FusionAdmission,
            &[retained.clone(), dropped.clone()],
        );
        collector.mark_removed(
            pb::RankingStageV005::MmrSelected,
            &[retained.clone(), dropped],
            &[retained],
            pb::CandidateDropReasonV005::MmrLimit,
            "MMR selection limit",
        );

        let trace = collector.finish();
        let dropped_trace = trace
            .candidates
            .iter()
            .find(|candidate| {
                candidate
                    .identity
                    .as_ref()
                    .is_some_and(|identity| identity.matched_chunk_id == "dropped")
            })
            .expect("dropped candidate trace");
        let loss = dropped_trace.stages.last().expect("drop stage");
        assert!(!loss.present);
        assert_eq!(
            loss.drop_reason,
            pb::CandidateDropReasonV005::MmrLimit as i32
        );
        assert!(dropped_trace
            .identity
            .as_ref()
            .is_some_and(|identity| !identity.source_block_id.contains("confidential")));
    }

    #[test]
    fn graph_append_preserves_direct_seed_provenance_without_mmr() {
        let mut seed = test_result("seed-parent", "direct relation seed evidence", 0.4);
        seed.citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "block-2".into());
        let direct = vec![
            test_result("high-a", "high score direct evidence", 0.9),
            test_result("high-b", "second high score direct evidence", 0.8),
            seed,
        ];
        let mut graph = test_result("related", "one-hop related evidence", 0.7);
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_seed_chunk_id".into(), "seed-sub".into());
        graph
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("graph_seed_source_block_id".into(), "block-2".into());

        let selected = select_graph_append_with_group_mmr(
            direct,
            vec![graph],
            3,
            2,
            1,
            false,
            0.75,
            0.75,
            30,
            "TOKEN_JACCARD",
            "TOKEN_JACCARD",
            true,
            true,
            8,
        );

        let ids = selected
            .results
            .iter()
            .map(|result| result.matched_chunk_id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"seed-parent"));
        assert!(ids.contains(&"related"));
    }

    #[test]
    fn token_jaccard_similarity_detects_overlap() {
        let same = token_jaccard_similarity("early loan repayment", "loan repayment early");
        let different = token_jaccard_similarity("early loan repayment", "branch address schedule");
        assert!(same > different);
    }

    #[test]
    fn lexical_terms_expand_underscore_identifiers() {
        let result = test_result(
            "access-zone-field",
            "Table CC_HOME_REQUESTS stores requests and payload field access_zone_id is mandatory.",
            0.8,
        );
        assert!(
            matched_term_count(&result, "Filter by access_zone_id") >= 4,
            "underscore identifiers must match both the full token and sub-token variants"
        );
    }

    #[test]
    fn exact_evidence_phrase_match_detects_query_embedded_parent_text() {
        assert!(exact_evidence_phrase_match(
            "Qdrant drift is repaired by reconciliation.",
            "Qdrant drift is repaired by reconciliation. What related context explains the state comparison?"
        ));
        assert!(!exact_evidence_phrase_match(
            "Qdrant projection and payload filters",
            "How should access filtering work?"
        ));
    }

    #[test]
    fn lexical_rrf_fusion_is_deterministic() {
        let mut direct = test_result(
            "recon-001",
            "Qdrant drift is repaired by reconciliation.",
            0.02,
        );
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "VECTOR_DIRECT".into());
        direct
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"VECTOR_DIRECT\"]".into());

        let mut lexical = test_result(
            "recon-001",
            "Qdrant drift is repaired by reconciliation.",
            1.0,
        );
        lexical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_source".into(), "POSTGRES_FTS".into());
        lexical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("retrieval_sources".into(), "[\"POSTGRES_FTS\"]".into());
        lexical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("source_block_id".into(), "recon-001".into());

        apply_indexed_lexical_rank_score(&mut lexical, 42.0, 1, 0.2, 60.0);
        lexical
            .citation
            .as_mut()
            .unwrap()
            .metadata
            .insert("strong_lexical_evidence".into(), "true".into());
        mark_ranking_protection(
            &mut lexical,
            RankingProtection {
                preserve_primary_direct: true,
                preserve_strong_lexical: true,
                preserve_unique_source_block: true,
                preserve_required_segment_coverage: false,
            },
        );
        let lexical_contribution = lexical.scores.as_ref().unwrap().fusion_score;
        let direct_fusion = direct.scores.as_ref().unwrap().fusion_score;
        merge_lexical_backfill_candidate(&mut direct, &lexical);

        assert_eq!(
            direct.scores.as_ref().unwrap().final_score,
            direct_fusion + lexical_contribution
        );
        assert_ne!(direct.scores.as_ref().unwrap().final_score, 42.0);
        let sources = extraction_retrieval_sources(&direct);
        assert!(sources.iter().any(|source| source == "VECTOR_DIRECT"));
        assert!(sources.iter().any(|source| source == "POSTGRES_FTS"));
        assert_eq!(
            direct
                .citation
                .as_ref()
                .unwrap()
                .metadata
                .get("source_block_id")
                .map(String::as_str),
            Some("recon-001")
        );
        let metadata = &direct.citation.as_ref().unwrap().metadata;
        assert_eq!(metadata.get("lexical_rank").map(String::as_str), Some("1"));
        assert!(metadata.contains_key("ranking_protection"));
    }

    fn block(
        id: &str,
        parent: &str,
        kind: pb::BlockType,
        text: &str,
        order: u32,
    ) -> pb::LogicalBlock {
        pb::LogicalBlock {
            block_id: id.to_string(),
            parent_block_id: parent.to_string(),
            block_type: kind as i32,
            text: text.to_string(),
            order_index: order,
            source_location: None,
            source_links: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn validates_valid_logical_block_tree() {
        let blocks = vec![
            block("root", "", pb::BlockType::Document, "Document", 0),
            block("sec", "root", pb::BlockType::Section, "Section", 1),
            block("p", "sec", pb::BlockType::Paragraph, "Paragraph", 2),
        ];
        assert!(validate_and_sort_logical_blocks(blocks).is_ok());
    }

    #[test]
    fn rejects_missing_parent() {
        let blocks = vec![
            block("root", "", pb::BlockType::Document, "Document", 0),
            block("p", "missing", pb::BlockType::Paragraph, "Paragraph", 1),
        ];
        let err = validate_and_sort_logical_blocks(blocks).unwrap_err();
        assert!(err.message().contains("LOGICAL_BLOCK_PARENT_NOT_FOUND"));
    }

    #[test]
    fn rejects_cycle() {
        let blocks = vec![
            block("root", "", pb::BlockType::Document, "Document", 0),
            block("a", "b", pb::BlockType::Section, "A", 1),
            block("b", "a", pb::BlockType::Subsection, "B", 2),
        ];
        let err = validate_and_sort_logical_blocks(blocks).unwrap_err();
        assert!(
            err.message().contains("LOGICAL_BLOCK_TREE_CYCLE")
                || err.message().contains("LOGICAL_BLOCK_PARENT_CHILD_INVALID")
        );
    }

    #[test]
    fn rejects_unsafe_source_link() {
        let mut root = block("root", "", pb::BlockType::Document, "Document", 0);
        root.source_links.push(pb::SourceLink {
            r#type: pb::SourceLinkType::Preview as i32,
            url: "javascript:alert(1)".into(),
            label: "bad".into(),
            mime_type: String::new(),
            requires_auth: true,
            expires_at: String::new(),
            attributes: std::collections::HashMap::new(),
        });
        let err = validate_and_sort_logical_blocks(vec![root]).unwrap_err();
        assert!(err.message().contains("SOURCE_LINK_INVALID_SCHEME"));
    }

    #[test]
    fn rejects_absolute_ttl_until_supported() {
        let policy = pb::TtlPolicy {
            mode: pb::TtlMode::Absolute as i32,
            ttl_seconds: 0,
            expires_at: "2026-07-01T00:00:00Z".into(),
            delete_from_qdrant_on_expire: true,
            keep_metadata_after_expire: true,
        };
        let err = ttl_days_from_policy(Some(&policy)).unwrap_err();
        assert!(err.message().contains("UNSUPPORTED_TTL_MODE_ABSOLUTE"));
    }

    #[test]
    fn rejects_missing_retrieve_access_level() {
        let metadata = MetadataMap::new();
        let err = effective_retrieve_access_level(&metadata, pb::AccessLevel::Unspecified as i32)
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn embedding_mmr_uses_dense_vectors_when_present() {
        let mut a = test_result("a", "loan repayment", 0.95);
        let mut b = test_result("b", "credit closure", 0.90);
        for (r, emb) in [(&mut a, vec![1.0_f32, 0.0]), (&mut b, vec![0.0_f32, 1.0])] {
            r.citation.as_mut().unwrap().metadata.insert(
                "embedding_normalized_json".into(),
                serde_json::to_string(&emb).unwrap(),
            );
        }
        let result = apply_mmr_rerank(
            vec![a, b],
            2,
            true,
            0.75,
            30,
            "DENSE_EMBEDDING",
            "TOKEN_JACCARD",
        );
        assert_eq!(result.similarity_source, "DENSE_EMBEDDING");
        assert_eq!(result.embedding_missing_count, 0);
    }

    #[test]
    fn direct_first_preserves_direct_priority_after_group_mmr() {
        let direct = vec![
            test_result("d1", "direct one", 0.9),
            test_result("d2", "direct two", 0.8),
        ];
        let graph = vec![
            test_result("g1", "graph high", 0.99),
            test_result("g2", "graph second", 0.98),
        ];
        let selected = select_results_with_strategy_aware_mmr(
            direct,
            graph,
            2,
            "DIRECT_FIRST",
            1,
            1,
            true,
            0.75,
            0.80,
            0.60,
            30,
            "DENSE_EMBEDDING",
            "TOKEN_JACCARD",
            true,
            true,
            5,
        );
        assert_eq!(selected.results.len(), 2);
        assert!(selected
            .results
            .iter()
            .all(|r| r.matched_chunk_id.starts_with('d')));
    }

    #[test]
    fn graph_append_preserves_separate_budgets_after_group_mmr() {
        let direct = vec![test_result("d1", "direct one", 0.9)];
        let graph = vec![
            test_result("g1", "graph one", 0.99),
            test_result("g2", "graph two", 0.98),
        ];
        let selected = select_results_with_strategy_aware_mmr(
            direct,
            graph,
            8,
            "GRAPH_AS_CONTEXT_APPEND",
            6,
            1,
            true,
            0.75,
            0.80,
            0.60,
            30,
            "DENSE_EMBEDDING",
            "TOKEN_JACCARD",
            true,
            true,
            5,
        );
        assert_eq!(selected.results.len(), 2);
        assert_eq!(
            selected
                .results
                .iter()
                .filter(|r| r.matched_chunk_id.starts_with('g'))
                .count(),
            1
        );
    }

    #[test]
    fn embedding_mmr_uses_mixed_similarity_when_one_embedding_missing() {
        let mut a = test_result("a", "loan repayment", 0.95);
        let b = test_result("b", "credit closure", 0.90);
        let emb = vec![1.0_f32, 0.0];
        a.citation.as_mut().unwrap().metadata.insert(
            "embedding_normalized_json".into(),
            serde_json::to_string(&emb).unwrap(),
        );
        let result = apply_mmr_rerank(
            vec![a, b],
            2,
            true,
            0.75,
            30,
            "DENSE_EMBEDDING",
            "TOKEN_JACCARD",
        );
        assert_eq!(result.embedding_missing_count, 1);
        assert!(matches!(
            result.similarity_source.as_str(),
            "TOKEN_JACCARD" | "MIXED"
        ));
    }

    #[test]
    fn graph_append_empty_direct_returns_graph_budget() {
        let direct = Vec::new();
        let graph = vec![
            test_result("g1", "graph one", 0.99),
            test_result("g2", "graph two", 0.98),
        ];
        let selected = select_results_with_strategy_aware_mmr(
            direct,
            graph,
            8,
            "GRAPH_AS_CONTEXT_APPEND",
            8,
            2,
            true,
            0.75,
            0.80,
            0.60,
            30,
            "DENSE_EMBEDDING",
            "TOKEN_JACCARD",
            true,
            true,
            5,
        );
        assert_eq!(selected.results.len(), 2);
        assert!(selected
            .results
            .iter()
            .all(|r| r.matched_chunk_id.starts_with('g')));
    }

    #[test]
    fn dimension_mismatch_uses_token_fallback() {
        let mut a = test_result("a", "loan repayment", 0.95);
        let mut b = test_result("b", "credit closure", 0.90);
        a.citation.as_mut().unwrap().metadata.insert(
            "embedding_normalized_json".into(),
            serde_json::to_string(&vec![1.0_f32, 0.0]).unwrap(),
        );
        b.citation.as_mut().unwrap().metadata.insert(
            "embedding_normalized_json".into(),
            serde_json::to_string(&vec![1.0_f32, 0.0, 0.0]).unwrap(),
        );
        let a = MmrPreparedCandidate::from_result(a);
        let b = MmrPreparedCandidate::from_result(b);
        let (_score, source) = candidate_similarity(&a, &b, "DENSE_EMBEDDING");
        assert!(matches!(source, SimilaritySource::TokenJaccardFallback));
    }

    #[test]
    fn search_result_from_graph_hit_preserves_qdrant_point_identity() {
        let parent = ParentContextRecord {
            access_zone_id: uuid::Uuid::nil(),
            id: uuid::Uuid::nil(),
            document_id: uuid::Uuid::nil(),
            document_version: 1,
            root_chunk_id: uuid::Uuid::nil(),
            source_chunk_id: uuid::Uuid::nil(),
            access_level: 1,
            content: "parent".into(),
            content_hash: "hash".into(),
            token_count: 1,
            sequence_no: 1,
            source_block_id: Some("parent-block".into()),
            metadata: serde_json::json!({}),
        };
        let point_id = uuid::Uuid::new_v4();
        let hit = QdrantSearchHit {
            id: uuid::Uuid::new_v4(),
            score: 0.9,
            dense_score: 0.0,
            sparse_score: 0.0,
            fusion_score: 0.9,
            dense_rank: None,
            sparse_rank: None,
            payload: serde_json::json!({
                "chunk_id": uuid::Uuid::new_v4().to_string(),
                "chunk_granularity": "GRAPH_EXPANDED",
                "qdrant_point_id": point_id.to_string(),
                "representation_type": "ORIGINAL",
                "dense_version": "dense-v1"
            }),
        };
        let result = search_result_from_hit(&parent, &hit, "matched".into(), None);
        let metadata = &result.citation.as_ref().unwrap().metadata;
        assert_eq!(
            metadata.get("qdrant_point_id").unwrap(),
            &point_id.to_string()
        );
        assert_eq!(metadata.get("representation_type").unwrap(), "ORIGINAL");
    }
}

async fn mark_ingestion_session_failed(
    pool: &sqlx::PgPool,
    ingestion_session_id: Uuid,
    error_code: &str,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "UPDATE astravector.ingestion_sessions_v004
         SET status='FAILED', error_code=$2, error_message=$3, updated_at=now()
         WHERE ingestion_session_id=$1 AND status='FINALIZING'",
    )
    .bind(ingestion_session_id)
    .bind(error_code)
    .bind(error_message)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        counter!("ingestion_finalize_failed_update_zero_rows_total").increment(1);
    }
    Ok(())
}

async fn record_nonterminal_ingestion_error(
    pool: &sqlx::PgPool,
    ingestion_session_id: Uuid,
    error_code: &str,
    error_message: &str,
    behavior: &str,
) -> Result<(), sqlx::Error> {
    let status_sql = match behavior {
        "KEEP_ACTIVE_WITH_LAST_ERROR" | "RETURN_TO_ACTIVE" => "ACTIVE",
        other => {
            counter!("ingestion_nonterminal_error_unknown_behavior_total").increment(1);
            tracing::warn!(behavior=%other, "unknown nonterminal ingestion failure behavior; returning session to ACTIVE");
            "ACTIVE"
        }
    };
    let result = sqlx::query(
        "UPDATE astravector.ingestion_sessions_v004
         SET status=$2,
             last_error_code=$3,
             last_error_message=$4,
             last_error_at=now(),
             updated_at=now()
         WHERE ingestion_session_id=$1 AND status='FINALIZING'",
    )
    .bind(ingestion_session_id)
    .bind(status_sql)
    .bind(error_code)
    .bind(error_message)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        counter!("ingestion_finalize_lost_ownership_total").increment(1);
    }
    Ok(())
}

async fn validate_staged_batch_consistency(
    pool: &sqlx::PgPool,
    ingestion_session_id: Uuid,
) -> Result<(), Status> {
    let batches = sqlx::query(
        "SELECT b.batch_index, b.batch_content_hash, b.block_count, COUNT(s.block_index) AS actual_count
         FROM astravector.ingestion_session_batches_v004 b
         LEFT JOIN astravector.ingestion_session_blocks_v004 s
           ON s.ingestion_session_id = b.ingestion_session_id
          AND s.batch_index = b.batch_index
         WHERE b.ingestion_session_id=$1
         GROUP BY b.batch_index, b.batch_content_hash, b.block_count
         ORDER BY b.batch_index"
    )
    .bind(ingestion_session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Status::unavailable(format!("postgres ingestion batch consistency: {e}")))?;
    if batches.is_empty() {
        return Err(Status::failed_precondition("INGESTION_STAGING_EMPTY"));
    }
    let mut expected_next: Option<i32> = None;
    for row in batches {
        let batch_index: i32 = row.get("batch_index");
        if let Some(expected) = expected_next {
            if batch_index != expected {
                counter!("ingestion_finalize_batch_gap_total").increment(1);
                return Err(Status::failed_precondition("INGESTION_BATCH_GAP"));
            }
        }
        expected_next = Some(batch_index + 1);
        let block_count: i32 = row.get("block_count");
        let actual_count: i64 = row.get("actual_count");
        if block_count as i64 != actual_count {
            counter!("ingestion_finalize_batch_count_mismatch_total").increment(1);
            counter!("ingestion_staging_corrupted_total", "reason" => "batch_count_mismatch")
                .increment(1);
            return Err(Status::data_loss(
                "INGESTION_STAGING_CORRUPTED: batch block_count mismatch",
            ));
        }

        let expected_hash: String = row.get("batch_content_hash");
        let block_rows = sqlx::query(
            "SELECT block_json
             FROM astravector.ingestion_session_blocks_v004
             WHERE ingestion_session_id=$1 AND batch_index=$2
             ORDER BY block_index",
        )
        .bind(ingestion_session_id)
        .bind(batch_index)
        .fetch_all(pool)
        .await
        .map_err(|e| Status::unavailable(format!("postgres ingestion batch hash rows: {e}")))?;
        let mut blocks = Vec::with_capacity(block_rows.len());
        for block_row in block_rows {
            let value: serde_json::Value = block_row.get("block_json");
            blocks.push(logical_block_from_json(&value)?);
        }
        let actual_hash = compute_batch_content_hash(&blocks).map_err(|e| {
            Status::data_loss(format!(
                "INGESTION_STAGING_CORRUPTED: batch hash serialization: {e}"
            ))
        })?;
        if normalize_sha256_hex(&expected_hash).unwrap_or_default() != actual_hash {
            counter!("ingestion_finalize_batch_hash_mismatch_total").increment(1);
            counter!("ingestion_staging_corrupted_total", "reason" => "batch_hash_mismatch")
                .increment(1);
            return Err(Status::data_loss(
                "INGESTION_STAGING_CORRUPTED_BATCH_HASH_MISMATCH",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod fix463_stabilization_tests {
    use super::*;

    #[test]
    fn test_parse_grpc_timeout_100m() {
        assert_eq!(parse_timeout("100m"), Some(Duration::from_millis(100)));
    }

    #[test]
    fn test_parse_grpc_timeout_2s_and_1h() {
        assert_eq!(parse_timeout("2S"), Some(Duration::from_secs(2)));
        assert_eq!(parse_timeout("1H"), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn retrieved_context_preserves_access_zone_lineage_in_metadata_and_field() {
        let zone = Uuid::new_v4().to_string();
        let result = pb::SearchResultV004 {
            access_zone_id: zone.clone(),
            document_id: Uuid::new_v4().to_string(),
            document_version: 7,
            matched_chunk_id: Uuid::new_v4().to_string(),
            parent_chunk_id: Uuid::new_v4().to_string(),
            matched_text: "matched".into(),
            parent_text: "parent".into(),
            citation: Some(pb::SearchCitationV004 {
                metadata: std::collections::HashMap::new(),
            }),
            scores: None,
            ..Default::default()
        };
        let ctx = retrieved_context_from_search_result(result);
        assert_eq!(ctx.access_zone_id, zone);
        assert_eq!(ctx.metadata.get("access_zone_id"), Some(&zone));
        assert!(ctx.metadata.contains_key("document_id"));
        assert!(ctx.metadata.contains_key("matched_chunk_id"));
    }

    #[test]
    fn rejected_parent_cannot_reenter_from_another_retrieval_branch() {
        let zone = Uuid::new_v4();
        let rejected_parent = Uuid::new_v4();
        let healthy_parent = Uuid::new_v4();
        let result = |parent_chunk_id: Uuid| pb::SearchResultV004 {
            access_zone_id: zone.to_string(),
            document_id: Uuid::new_v4().to_string(),
            document_version: 1,
            matched_chunk_id: Uuid::new_v4().to_string(),
            parent_chunk_id: parent_chunk_id.to_string(),
            parent_text: "canonical parent".into(),
            ..Default::default()
        };
        let mut branch_results = vec![result(rejected_parent), result(healthy_parent)];
        let rejected = HashSet::from([(zone, rejected_parent)]);

        let removed = retain_results_outside_rejected_parents(&mut branch_results, &rejected);

        assert_eq!(removed, 1);
        assert_eq!(branch_results.len(), 1);
        assert_eq!(
            branch_results[0].parent_chunk_id,
            healthy_parent.to_string()
        );
    }
}
