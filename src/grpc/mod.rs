use crate::{
    cache::L1Cache,
    chunking::{ChunkingEngine, ChunkingProfile, ConservativeTokenCounter, SizeProfile},
    config::AppConfig,
    contract,
    error::AstraError,
    health::Readiness,
    inference::{EmbeddingResult, InferenceEngine, InferenceInput},
    pb::{
        self, astra_vector_runtime_server::AstraVectorRuntime,
        astra_vector_v004_control_server::AstraVectorV004Control,
    },
    persistence::{ChunkContentRecord, ClaimResult, ParentContextRecord, Repository},
    provider::SelectedProvider,
    qdrant::{QdrantClient, QdrantSearchHit},
    scheduler::{QueueKind, Scheduler},
};
use futures::future::join_all;
use metrics::{counter, histogram};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tonic::{metadata::MetadataMap, Request, Response, Status};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

#[derive(Clone)]
pub struct AstraVectorV004ControlService {
    cfg: Arc<AppConfig>,
    scheduler: Scheduler,
    repo: Option<Repository>,
    qdrant: Option<Arc<QdrantClient>>,
    shutdown: CancellationToken,
}

impl AstraVectorV004ControlService {
    pub fn new(
        cfg: Arc<AppConfig>,
        scheduler: Scheduler,
        repo: Option<Repository>,
        qdrant: Option<Arc<QdrantClient>>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            cfg,
            scheduler,
            repo,
            qdrant,
            shutdown,
        }
    }

    fn not_implemented() -> Status {
        Status::unimplemented("AstraVectorV004Control backend is not implemented yet")
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
}

#[tonic::async_trait]
impl AstraVectorV004Control for AstraVectorV004ControlService {
    async fn search(
        &self,
        request: Request<pb::SearchRequestV004>,
    ) -> Result<Response<pb::SearchResponseV004>, Status> {
        let started = std::time::Instant::now();
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
        let top_k = if r.top_k == 0 { 10 } else { r.top_k }.min(50);
        if r.top_k > 50 {
            return Err(Status::invalid_argument("top_k must be <= 50"));
        }
        let candidate_limit = if r.candidate_limit == 0 {
            (top_k * 4).max(top_k)
        } else {
            r.candidate_limit
        };
        if candidate_limit < top_k {
            return Err(Status::invalid_argument("candidate_limit must be >= top_k"));
        }
        let candidate_limit = candidate_limit.min(200);
        let parent_limit = if r.parent_limit == 0 {
            top_k
        } else {
            r.parent_limit
        };
        if parent_limit == 0 || parent_limit > 50 {
            return Err(Status::invalid_argument(
                "parent_limit must be between 1 and 50",
            ));
        }
        let timeout_ms = if r.timeout_ms == 0 {
            self.cfg.grpc.deadlines.query_ms
        } else {
            r.timeout_ms as u64
        }
        .min(self.cfg.grpc.deadlines.query_ms);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        let emb_started = std::time::Instant::now();
        let embedding = self
            .scheduler
            .submit(
                QueueKind::Query,
                InferenceInput {
                    text: query.to_string(),
                    max_length: self.cfg.tokenization.query.max_length,
                    allow_truncation: self.cfg.tokenization.query.truncation_allowed,
                    want_dense: true,
                    want_sparse: false,
                    token_count_hint: 0,
                },
                deadline,
                self.shutdown.child_token(),
            )
            .await
            .map_err(Status::from)?;
        let query_embedding_ms = emb_started.elapsed().as_millis() as u64;
        let dense = embedding
            .dense
            .as_deref()
            .ok_or_else(|| Status::failed_precondition("query dense embedding unavailable"))?;

        let qdrant_started = std::time::Instant::now();
        let hits = self
            .qdrant()?
            .search_dense(
                dense,
                access_zone_id,
                caller_access_level as i16,
                candidate_limit as usize,
            )
            .await
            .map_err(Status::from)?;
        let qdrant_search_ms = qdrant_started.elapsed().as_millis() as u64;

        let mut groups: Vec<(Uuid, QdrantSearchHit)> = Vec::new();
        let mut seen = HashSet::new();
        for hit in hits.iter() {
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
                    .and_then(|v| Uuid::parse_str(v).ok()),
                _ => None,
            };
            if let Some(parent_id) = parent_id {
                if seen.insert(parent_id) {
                    groups.push((parent_id, hit.clone()));
                }
            }
            if groups.len() >= parent_limit as usize {
                break;
            }
        }
        let parent_ids: Vec<Uuid> = groups.iter().map(|(id, _)| *id).collect();
        let parent_fetch_started = std::time::Instant::now();
        let parents = self
            .repo()?
            .fetch_parent_contexts(access_zone_id, &parent_ids, caller_access_level as i16)
            .await
            .map_err(Status::from)?;
        let parent_fetch_ms = parent_fetch_started.elapsed().as_millis() as u64;
        let by_parent: std::collections::HashMap<Uuid, ParentContextRecord> =
            parents.into_iter().map(|p| (p.id, p)).collect();
        let mut results = Vec::new();
        for (parent_id, hit) in groups {
            if results.len() >= top_k as usize {
                break;
            }
            let Some(parent) = by_parent.get(&parent_id) else {
                continue;
            };
            results.push(search_result_from_hit(parent, &hit));
        }
        Ok(Response::new(pb::SearchResponseV004 {
            results,
            diagnostics: Some(pb::SearchDiagnosticsV004 {
                query_embedding_ms,
                qdrant_search_ms,
                parent_fetch_ms,
                total_ms: started.elapsed().as_millis() as u64,
                candidate_count: hits.len() as u32,
                parent_group_count: by_parent.len() as u32,
            }),
        }))
    }

    async fn create_multi_granularity_chunks(
        &self,
        request: Request<pb::CreateMultiGranularityChunksRequest>,
    ) -> Result<Response<pb::CreateMultiGranularityChunksResponse>, Status> {
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be UUID"))?;
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument(
                "document_version must be greater than zero",
            ));
        }
        if r.source_text.trim().is_empty() {
            return Err(Status::invalid_argument("source_text is required"));
        }
        if r.source_text.len() > 2 * 1024 * 1024 {
            return Err(Status::out_of_range(
                "source_text exceeds 2 MiB smoke limit",
            ));
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
        let generated = engine
            .chunk(
                access_zone_id,
                document_id,
                r.document_version,
                &r.source_text,
                &profile,
            )
            .map_err(Status::from)?;
        let mut request_metadata = serde_json::to_value(&r.metadata)
            .map_err(|e| Status::internal(format!("metadata serialization: {e}")))?;
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
        let stored = self
            .repo()?
            .store_v004_chunks(
                access_zone_id,
                document_id,
                r.document_version as i64,
                &generated,
                &self.cfg.tokenizer.version,
                &profile_version,
                access_level as i16,
                r.ttl_days.map(|v| v as i32),
                request_metadata.clone(),
            )
            .await
            .map_err(Status::from)?;
        let deadline =
            Instant::now() + Duration::from_millis(self.cfg.grpc.deadlines.document_batch_ms);
        for stored_chunk in stored.iter().filter(|chunk| chunk.granularity != "SOURCE") {
            let Some(generated_chunk) = generated.iter().find(|chunk| {
                chunk.granularity.as_db_str() == stored_chunk.granularity
                    && chunk.sequence_no as i32 == stored_chunk.sequence_no
                    && chunk.content_hash == stored_chunk.content_hash
            }) else {
                return Err(Status::internal(
                    "stored chunk cannot be matched to generated content",
                ));
            };
            if generated_chunk.granularity.as_db_str() == "SOURCE" {
                continue;
            }
            let input = InferenceInput {
                text: generated_chunk.content.clone(),
                max_length: self.cfg.tokenization.child.max_length,
                allow_truncation: self.cfg.tokenization.child.truncation_allowed,
                want_dense: true,
                want_sparse: false,
                token_count_hint: generated_chunk.token_count,
            };
            let embedding = self
                .scheduler
                .submit(
                    QueueKind::Document,
                    input,
                    deadline,
                    self.shutdown.child_token(),
                )
                .await
                .map_err(Status::from)?;
            let core_chunk = crate::persistence::V004ChunkForEmbedding {
                access_zone_id,
                document_id,
                document_version: r.document_version as i64,
                root_chunk_id: stored_chunk.root_id,
                source_chunk_id: stored_chunk.source_id,
                parent_chunk_id: stored_chunk.parent_id,
                chunk_id: stored_chunk.id,
                granularity: stored_chunk.granularity.clone(),
                sequence_no: stored_chunk.sequence_no,
                token_count: stored_chunk.token_count,
                content_hash: stored_chunk.content_hash.clone(),
                content: generated_chunk.content.clone(),
                access_level: access_level as i16,
                ttl_days: r.ttl_days.map(|v| v as i32),
                metadata: request_metadata.clone(),
            };
            self.repo()?
                .persist_v004_embedding_binding_outbox(
                    "v004-control",
                    &access_zone_id.to_string(),
                    &core_chunk,
                    &stored_chunk.content_hash,
                    &embedding,
                    &self.cfg.tokenizer.version,
                    &self.cfg.model.version,
                    &self.cfg.dense.name,
                    &self.cfg.dense.version,
                    &self.cfg.sparse.name,
                    &self.cfg.sparse.version,
                    self.cfg.sparse.min_weight,
                    self.cfg.sparse.max_non_zero as i32,
                    &self.cfg.qdrant.collection,
                    &profile_version,
                )
                .await
                .map_err(Status::from)?;
        }
        Ok(Response::new(chunks_response_from_records(
            stored, "INDEXING",
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
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be UUID"))?;
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
        let r = request.into_inner();
        let access_zone_id = Uuid::parse_str(r.access_zone_id.trim())
            .map_err(|_| Status::invalid_argument("access_zone_id must be a UUID"))?;
        let document_id = Uuid::parse_str(r.document_id.trim())
            .map_err(|_| Status::invalid_argument("document_id must be a UUID"))?;
        if r.document_version == 0 {
            return Err(Status::invalid_argument("document_version must be > 0"));
        }
        let record = self
            .repo()?
            .activate_document_version(access_zone_id, document_id, r.document_version as i64)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(pb::DocumentVersionResponse {
            document_id: record.document_id.to_string(),
            document_version: record.document_version as u64,
            status: record.status,
        }))
    }

    async fn delete_chunk_group(
        &self,
        _request: Request<pb::DeleteChunkGroupRequest>,
    ) -> Result<Response<pb::DeleteChunkGroupResponse>, Status> {
        Err(Self::not_implemented())
    }

    async fn update_chunk_group_ttl(
        &self,
        _request: Request<pb::UpdateChunkGroupTtlRequest>,
    ) -> Result<Response<pb::UpdateChunkGroupTtlResponse>, Status> {
        Err(Self::not_implemented())
    }

    async fn set_chunk_group_legal_hold(
        &self,
        _request: Request<pb::SetChunkGroupLegalHoldRequest>,
    ) -> Result<Response<pb::SetChunkGroupLegalHoldResponse>, Status> {
        Err(Self::not_implemented())
    }

    async fn get_relevance_evaluation_v004(
        &self,
        _request: Request<pb::GetRelevanceEvaluationRequest>,
    ) -> Result<Response<pb::GetRelevanceEvaluationResponse>, Status> {
        Err(Self::not_implemented())
    }

    async fn submit_relevance_feedback_v004(
        &self,
        _request: Request<pb::SubmitRelevanceFeedbackRequest>,
    ) -> Result<Response<pb::SubmitRelevanceFeedbackResponse>, Status> {
        Err(Self::not_implemented())
    }

    async fn list_quarantined_points(
        &self,
        _request: Request<pb::ListQuarantinedPointsRequest>,
    ) -> Result<Response<pb::ListQuarantinedPointsResponse>, Status> {
        Err(Self::not_implemented())
    }

    async fn resolve_quarantined_point(
        &self,
        _request: Request<pb::ResolveQuarantinedPointRequest>,
    ) -> Result<Response<pb::ResolveQuarantinedPointResponse>, Status> {
        Err(Self::not_implemented())
    }
}
#[derive(Clone)]
pub struct AstraVectorService {
    cfg: Arc<AppConfig>,
    scheduler: Scheduler,
    engine: Arc<dyn InferenceEngine>,
    l1: L1Cache,
    repo: Option<Repository>,
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
            provider,
            readiness,
            shutdown,
        }
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
                mode: format!("{:?}", mode),
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
        let r = request.into_inner();
        let repo = self
            .repo
            .as_ref()
            .ok_or_else(|| Status::unavailable("PostgreSQL unavailable"))?;
        let id = Uuid::parse_str(&r.binding_id)
            .map_err(|_| Status::invalid_argument("invalid binding_id"))?;
        let s = repo
            .binding_status(&r.tenant_id, &r.workspace_id, id)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(pb::GetVectorSyncStatusResponse {
            binding_id: s.id.to_string(),
            lifecycle_status: s.lifecycle_status,
            qdrant_sync_status: s.qdrant_sync_status,
            last_error: s.last_error,
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
fn search_result_from_hit(
    parent: &ParentContextRecord,
    hit: &QdrantSearchHit,
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
    let metadata = parent
        .metadata
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
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
            dense_score: hit.score,
            final_score: hit.score.max(0.0),
        }),
        citation: Some(pb::SearchCitationV004 { metadata }),
        access_zone_id: parent.access_zone_id.to_string(),
        access_level: parent.access_level as i32,
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
fn parse_timeout(s: &str) -> Option<Duration> {
    let (unit, num) = s.split_at(s.len().checked_sub(1)?);
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
