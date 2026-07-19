use crate::graph::GraphRelationType;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::{env, path::Path};

pub const MIN_INGESTION_DOCUMENT_DEADLINE_MS: u64 = 1_000;
pub const MAX_INGESTION_DOCUMENT_DEADLINE_MS: u64 = 600_000;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub service: ServiceConfig,
    pub grpc: GrpcConfig,
    pub security: SecurityConfig,
    pub shutdown: ShutdownConfig,
    pub retry: RetryConfig,
    pub model: ModelConfig,
    pub tokenizer: TokenizerConfig,
    pub inference: InferenceConfig,
    pub dense: DenseConfig,
    pub sparse: SparseConfig,
    pub tokenization: TokenizationConfig,
    pub cache: CacheConfig,
    pub batching: BatchingConfig,
    pub scheduler: SchedulerConfig,
    pub postgres: PostgresConfig,
    pub recovery: RecoveryConfig,
    pub retention: RetentionConfig,
    pub metrics: MetricsConfig,
    pub qdrant: QdrantConfig,
    pub search: SearchConfig,
    pub explain: ExplainConfig,
    pub lifecycle: LifecycleConfig,
    pub enrichment: EnrichmentConfig,
    pub relevance: RelevanceConfig,
    pub graph_rag: GraphRagConfig,
    pub adaptive: AdaptiveConfig,
    #[serde(default)]
    pub ingestion: IngestionConfig,
    #[serde(default)]
    pub embedding: EmbeddingRuntimeConfig,
    #[serde(default)]
    pub resilience: ResilienceConfig,
    #[serde(default)]
    pub rag_context: RagContextConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub embedding_cache: EmbeddingCachePolicyConfig,
    #[serde(default)]
    pub chunking: ChunkingRuntimeConfig,
    #[serde(default)]
    pub index_ttl: IndexTtlConfig,
    #[serde(default)]
    pub access_zones: AccessZonesConfig,
    #[serde(default)]
    pub access_zone_registry: AccessZoneRegistryConfig,
    #[serde(default)]
    pub access_zone_codes: AccessZoneCodesConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexTtlConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_index_ttl_default_ttl_days")]
    pub default_ttl_days: u32,
    #[serde(default = "default_index_ttl_min_ttl_days")]
    pub min_ttl_days: u32,
    #[serde(default = "default_index_ttl_max_ttl_days")]
    pub max_ttl_days: u32,
    #[serde(default = "default_true")]
    pub allow_never_expire: bool,
    #[serde(default = "default_index_ttl_never_expire_epoch_seconds")]
    pub never_expire_epoch_seconds: i64,
    #[serde(default = "default_true")]
    pub cleanup_enabled: bool,
    #[serde(default = "default_index_ttl_cleanup_interval_seconds")]
    pub cleanup_interval_seconds: u64,
    #[serde(default = "default_index_ttl_cleanup_batch_size")]
    pub cleanup_batch_size: usize,
    #[serde(default = "default_index_ttl_qdrant_delete_batch_size")]
    pub qdrant_delete_batch_size: usize,
    #[serde(default = "default_index_ttl_qdrant_scroll_batch_size")]
    pub qdrant_scroll_batch_size: usize,
    /// Rollback flag: disables Qdrant scroll reconciliation of extra points while keeping expected binding deletes.
    #[serde(default = "default_true")]
    pub qdrant_reconciliation_enabled: bool,
    #[serde(default = "default_index_ttl_delete_failed_retry_after_seconds")]
    pub delete_failed_retry_after_seconds: u64,
    #[serde(default = "default_index_ttl_max_delete_attempts")]
    pub max_delete_attempts: u32,
    #[serde(default = "default_index_ttl_delete_retry_initial_delay_seconds")]
    pub delete_retry_initial_delay_seconds: u64,
    #[serde(default = "default_index_ttl_delete_retry_max_delay_seconds")]
    pub delete_retry_max_delay_seconds: u64,
    #[serde(default = "default_index_ttl_deleting_stale_timeout_seconds")]
    pub deleting_stale_timeout_seconds: u64,
    #[serde(default)]
    pub hard_delete_metadata: bool,
    #[serde(default = "default_index_ttl_keep_tombstone_days")]
    pub keep_tombstone_days: u32,
}
impl Default for IndexTtlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_ttl_days: default_index_ttl_default_ttl_days(),
            min_ttl_days: default_index_ttl_min_ttl_days(),
            max_ttl_days: default_index_ttl_max_ttl_days(),
            allow_never_expire: true,
            never_expire_epoch_seconds: default_index_ttl_never_expire_epoch_seconds(),
            cleanup_enabled: true,
            cleanup_interval_seconds: default_index_ttl_cleanup_interval_seconds(),
            cleanup_batch_size: default_index_ttl_cleanup_batch_size(),
            qdrant_delete_batch_size: default_index_ttl_qdrant_delete_batch_size(),
            qdrant_scroll_batch_size: default_index_ttl_qdrant_scroll_batch_size(),
            qdrant_reconciliation_enabled: true,
            delete_failed_retry_after_seconds: default_index_ttl_delete_failed_retry_after_seconds(
            ),
            max_delete_attempts: default_index_ttl_max_delete_attempts(),
            delete_retry_initial_delay_seconds:
                default_index_ttl_delete_retry_initial_delay_seconds(),
            delete_retry_max_delay_seconds: default_index_ttl_delete_retry_max_delay_seconds(),
            deleting_stale_timeout_seconds: default_index_ttl_deleting_stale_timeout_seconds(),
            hard_delete_metadata: false,
            keep_tombstone_days: default_index_ttl_keep_tombstone_days(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessZonesConfig {
    #[serde(default = "default_true")]
    pub require_on_ingestion: bool,
    #[serde(default = "default_true")]
    pub allow_multi_zone_search: bool,
    #[serde(default = "default_access_zones_max_search_access_zones")]
    pub max_search_access_zones: usize,
    #[serde(default = "default_access_zones_max_access_zone_id_length")]
    pub max_access_zone_id_length: usize,
    #[serde(default = "default_access_zone_id_regex")]
    pub access_zone_id_regex: String,
}
impl Default for AccessZonesConfig {
    fn default() -> Self {
        Self {
            require_on_ingestion: true,
            allow_multi_zone_search: true,
            max_search_access_zones: default_access_zones_max_search_access_zones(),
            max_access_zone_id_length: default_access_zones_max_access_zone_id_length(),
            access_zone_id_regex: default_access_zone_id_regex(),
        }
    }
}

fn default_index_ttl_default_ttl_days() -> u32 {
    90
}
fn default_index_ttl_min_ttl_days() -> u32 {
    1
}
fn default_index_ttl_max_ttl_days() -> u32 {
    3650
}
fn default_index_ttl_never_expire_epoch_seconds() -> i64 {
    253_402_300_799
}
fn default_index_ttl_cleanup_interval_seconds() -> u64 {
    300
}
fn default_index_ttl_cleanup_batch_size() -> usize {
    100
}
fn default_index_ttl_qdrant_delete_batch_size() -> usize {
    500
}
fn default_index_ttl_qdrant_scroll_batch_size() -> usize {
    1000
}
fn default_index_ttl_delete_failed_retry_after_seconds() -> u64 {
    900
}
fn default_index_ttl_max_delete_attempts() -> u32 {
    10
}
fn default_index_ttl_delete_retry_initial_delay_seconds() -> u64 {
    30
}
fn default_index_ttl_delete_retry_max_delay_seconds() -> u64 {
    3600
}
fn default_index_ttl_deleting_stale_timeout_seconds() -> u64 {
    3600
}
fn default_index_ttl_keep_tombstone_days() -> u32 {
    30
}
fn default_access_zones_max_search_access_zones() -> usize {
    50
}
fn default_access_zones_max_access_zone_id_length() -> usize {
    256
}
fn default_access_zone_id_regex() -> String {
    "^[A-Za-z0-9][A-Za-z0-9._:/-]{0,255}$".into()
}
fn default_access_zone_registry_cache_ttl_seconds() -> u64 {
    5
}
fn default_access_zone_registry_active_recheck_interval_ms() -> u64 {
    1000
}
fn default_access_zone_registry_always_recheck_on_ingestion() -> bool {
    true
}
fn default_access_zone_code_regex() -> String {
    "^[0-9]{4}$".into()
}
fn default_access_zone_codes_max_code() -> u32 {
    9999
}
fn default_access_zone_codes_never_expire_end() -> u32 {
    999
}
fn default_access_zone_codes_step_start() -> u32 {
    1000
}
fn default_access_zone_codes_step_codes() -> u32 {
    500
}
fn default_access_zone_codes_step_months() -> u32 {
    6
}
fn default_access_zone_codes_special_max_code_start() -> u32 {
    9500
}
fn default_access_zone_codes_special_max_ttl_days() -> u32 {
    3650
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessZoneRegistryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_access_zone_registry_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,
    #[serde(default = "default_access_zone_registry_active_recheck_interval_ms")]
    pub active_recheck_interval_ms: u64,
    #[serde(default = "default_access_zone_registry_always_recheck_on_ingestion")]
    pub always_recheck_on_ingestion: bool,
    #[serde(default = "default_true")]
    pub fail_if_zone_missing: bool,
    #[serde(default = "default_false")]
    pub auto_create_on_ingestion: bool,
    #[serde(default = "default_false")]
    pub auto_create_on_search: bool,
    #[serde(default = "default_access_zone_registry_auto_create_default_status")]
    pub auto_create_default_status: String,
    #[serde(default = "default_true")]
    pub auto_create_require_internal_auth: bool,
}
fn default_access_zone_registry_auto_create_default_status() -> String {
    "ACTIVE".into()
}
impl Default for AccessZoneRegistryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_ttl_seconds: default_access_zone_registry_cache_ttl_seconds(),
            active_recheck_interval_ms: default_access_zone_registry_active_recheck_interval_ms(),
            always_recheck_on_ingestion: default_access_zone_registry_always_recheck_on_ingestion(),
            fail_if_zone_missing: true,
            auto_create_on_ingestion: false,
            auto_create_on_search: false,
            auto_create_default_status: default_access_zone_registry_auto_create_default_status(),
            auto_create_require_internal_auth: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessZoneCodesConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_access_zone_code_regex")]
    pub code_regex: String,
    #[serde(default)]
    pub min_code: u32,
    #[serde(default = "default_access_zone_codes_max_code")]
    pub max_code: u32,
    #[serde(default = "default_true")]
    pub code_matrix_enabled: bool,
    #[serde(default)]
    pub never_expire_start: u32,
    #[serde(default = "default_access_zone_codes_never_expire_end")]
    pub never_expire_end: u32,
    #[serde(default = "default_access_zone_codes_step_start")]
    pub step_start: u32,
    #[serde(default = "default_access_zone_codes_step_codes")]
    pub step_codes: u32,
    #[serde(default = "default_access_zone_codes_step_months")]
    pub step_months: u32,
    #[serde(default = "default_access_zone_codes_special_max_code_start")]
    pub special_max_code_start: u32,
    #[serde(default = "default_access_zone_codes_special_max_ttl_days")]
    pub special_max_ttl_days: u32,
}
impl Default for AccessZoneCodesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            code_regex: default_access_zone_code_regex(),
            min_code: 0,
            max_code: default_access_zone_codes_max_code(),
            code_matrix_enabled: true,
            never_expire_start: 0,
            never_expire_end: default_access_zone_codes_never_expire_end(),
            step_start: default_access_zone_codes_step_start(),
            step_codes: default_access_zone_codes_step_codes(),
            step_months: default_access_zone_codes_step_months(),
            special_max_code_start: default_access_zone_codes_special_max_code_start(),
            special_max_ttl_days: default_access_zone_codes_special_max_ttl_days(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphRagConfig {
    pub enabled: bool,
    pub build: GraphRagBuildConfig,
    pub retrieval: GraphRagRetrievalConfig,
    #[serde(default)]
    pub scoring: GraphRagScoringConfig,
    #[serde(default)]
    pub rerank: GraphRagRerankConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphRagBuildConfig {
    pub max_document_graph_nodes: usize,
    pub max_document_graph_edges: usize,
    pub max_block_nodes: usize,
    pub max_chunk_nodes: usize,
    pub max_tag_nodes: usize,
    pub max_children_per_block: usize,
    pub max_same_parent_edges: usize,
    pub max_same_table_edges: usize,
    #[serde(default)]
    pub semantic_edges_enabled: bool,
    #[serde(default = "default_semantic_backend")]
    pub semantic_backend: String,
    #[serde(default = "default_true")]
    pub semantic_same_document_only: bool,
    #[serde(default = "default_semantic_top_k_per_chunk")]
    pub semantic_top_k_per_chunk: usize,
    #[serde(default = "default_semantic_min_score")]
    pub semantic_min_score: f32,
    #[serde(default = "default_semantic_max_edges_per_document")]
    pub semantic_max_edges_per_document: usize,
    #[serde(default = "default_semantic_max_chunks_for_in_memory")]
    pub semantic_max_chunks_for_in_memory: usize,
    #[serde(default = "default_semantic_large_document_policy")]
    pub semantic_large_document_policy: String,
    #[serde(default = "default_semantic_batch_size")]
    pub semantic_batch_size: usize,
    #[serde(default = "default_true")]
    pub semantic_normalize_embeddings: bool,
    #[serde(default)]
    pub semantic_parallel_enabled: bool,
    #[serde(default)]
    pub semantic_parallelism: usize,
    #[serde(default = "default_semantic_rebuild_timeout_ms")]
    pub semantic_rebuild_timeout_ms: u64,
    #[serde(default = "default_semantic_warn_build_time_ms")]
    pub semantic_warn_build_time_ms: u64,
    pub cleanup_old_graph_on_reindex: bool,
    pub bulk_insert_batch_size: usize,
    pub failure_mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphRagRetrievalConfig {
    pub enabled_by_default: bool,
    pub max_seed_chunks: usize,
    pub max_hops: u32,
    pub max_nodes_visited: usize,
    pub max_edges_visited: usize,
    pub max_related_chunks: usize,
    #[serde(default = "default_direct_result_limit")]
    pub direct_result_limit: usize,
    #[serde(default = "default_graph_expansion_result_limit")]
    pub graph_expansion_result_limit: usize,
    #[serde(default = "default_final_context_limit")]
    pub final_context_limit: usize,
    #[serde(default = "default_graph_merge_strategy")]
    pub graph_merge_strategy: String,
    #[serde(default = "default_final_context_limit_mode")]
    pub final_context_limit_mode: String,
    #[serde(default = "default_direct_context_limit")]
    pub direct_context_limit: usize,
    #[serde(default = "default_graph_context_append_limit")]
    pub graph_context_append_limit: usize,
    #[serde(default = "default_min_direct_contexts")]
    pub min_direct_contexts: usize,
    #[serde(default = "default_max_graph_fraction")]
    pub max_graph_fraction: f32,
    #[serde(default = "default_max_graph_relations_debug_per_candidate")]
    pub max_graph_relations_debug_per_candidate: usize,
    pub timeout_ms: u64,
    #[serde(default = "default_graph_min_useful_budget_ms")]
    pub min_useful_budget_ms: u64,
    #[serde(default = "default_graph_response_reserve_ms")]
    pub response_reserve_ms: u64,
    pub allow_partial_dense_sparse_fallback: bool,
    pub allowed_relations: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphRagScoringConfig {
    #[serde(default = "default_relation_weights")]
    pub relation_weights: HashMap<String, f32>,
    #[serde(default = "default_structural_relation_weight")]
    pub default_structural_relation_weight: f32,
    #[serde(default = "default_semantic_relation_weight")]
    pub default_semantic_relation_weight: f32,
    #[serde(default = "default_graph_hop_penalty")]
    pub graph_hop_penalty: HashMap<String, f32>,
    #[serde(default = "default_graph_min_score")]
    pub graph_min_score: f32,
    #[serde(default = "default_structural_seed_score_floor")]
    pub structural_seed_score_floor: f32,
    #[serde(default = "default_semantic_power")]
    pub semantic_power: f32,
    #[serde(default = "default_direct_score_weight")]
    pub direct_score_weight: f32,
    #[serde(default = "default_graph_score_weight")]
    pub graph_score_weight: f32,
    #[serde(default)]
    pub graph_score_bias: f32,
    #[serde(default = "default_score_normalization")]
    pub score_normalization: String,
}

impl Default for GraphRagScoringConfig {
    fn default() -> Self {
        Self {
            relation_weights: default_relation_weights(),
            default_structural_relation_weight: default_structural_relation_weight(),
            default_semantic_relation_weight: default_semantic_relation_weight(),
            graph_hop_penalty: default_graph_hop_penalty(),
            graph_min_score: default_graph_min_score(),
            structural_seed_score_floor: default_structural_seed_score_floor(),
            semantic_power: default_semantic_power(),
            direct_score_weight: default_direct_score_weight(),
            graph_score_weight: default_graph_score_weight(),
            graph_score_bias: 0.0,
            score_normalization: default_score_normalization(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphRagRerankConfig {
    #[serde(default)]
    pub mmr_enabled: bool,
    #[serde(default = "default_mmr_lambda")]
    pub mmr_lambda: f32,
    #[serde(default = "default_mmr_lambda_direct")]
    pub mmr_lambda_direct: f32,
    #[serde(default = "default_mmr_lambda_graph")]
    pub mmr_lambda_graph: f32,
    #[serde(default = "default_mmr_candidate_limit")]
    pub mmr_candidate_limit: usize,
    #[serde(default = "default_mmr_similarity_source")]
    pub mmr_similarity_source: String,
    #[serde(default = "default_mmr_fallback_similarity_source")]
    pub mmr_fallback_similarity_source: String,
    #[serde(default = "default_true")]
    pub mmr_allow_graph_candidates: bool,
    #[serde(default = "default_true")]
    pub mmr_allow_direct_candidates: bool,
    #[serde(default = "default_true")]
    pub embedding_fetch_enabled: bool,
    #[serde(default = "default_embedding_fetch_timeout_ms")]
    pub embedding_fetch_timeout_ms: u64,
    #[serde(default = "default_mmr_embedding_min_useful_budget_ms")]
    pub embedding_fetch_min_useful_budget_ms: u64,
    #[serde(default = "default_mmr_response_reserve_ms")]
    pub response_reserve_ms: u64,
    #[serde(default = "default_embedding_fetch_warn_threshold_ms")]
    pub embedding_fetch_warn_threshold_ms: u64,
    #[serde(default)]
    pub embedding_fetch_min_candidates: usize,
    #[serde(default = "default_embedding_fetch_identity_mode")]
    pub embedding_fetch_identity_mode: String,
    #[serde(default)]
    pub embedding_fetch_allow_chunk_fallback: bool,
    #[serde(default = "default_embedding_dense_representation_name")]
    pub embedding_dense_representation_name: String,
    #[serde(default = "default_true")]
    pub embedding_cache_enabled: bool,
    #[serde(default = "default_embedding_cache_max_entries")]
    pub embedding_cache_max_entries: usize,
    #[serde(default = "default_embedding_cache_ttl_seconds")]
    pub embedding_cache_ttl_seconds: u64,

    #[serde(default)]
    pub learned_reranker_enabled: bool,
    #[serde(default = "default_learned_reranker_provider")]
    pub learned_reranker_provider: String,
    #[serde(default = "default_learned_reranker_top_n")]
    pub learned_reranker_top_n: usize,
    #[serde(default = "default_learned_reranker_timeout_ms")]
    pub learned_reranker_timeout_ms: u64,
    #[serde(default = "default_learned_reranker_weight")]
    pub learned_reranker_weight: f32,
    #[serde(default = "default_retrieval_score_weight")]
    pub retrieval_score_weight: f32,
}

impl Default for GraphRagRerankConfig {
    fn default() -> Self {
        Self {
            mmr_enabled: false,
            mmr_lambda: default_mmr_lambda(),
            mmr_lambda_direct: default_mmr_lambda_direct(),
            mmr_lambda_graph: default_mmr_lambda_graph(),
            mmr_candidate_limit: default_mmr_candidate_limit(),
            mmr_similarity_source: default_mmr_similarity_source(),
            mmr_fallback_similarity_source: default_mmr_fallback_similarity_source(),
            mmr_allow_graph_candidates: true,
            mmr_allow_direct_candidates: true,
            embedding_fetch_enabled: true,
            embedding_fetch_timeout_ms: default_embedding_fetch_timeout_ms(),
            embedding_fetch_min_useful_budget_ms: default_mmr_embedding_min_useful_budget_ms(),
            response_reserve_ms: default_mmr_response_reserve_ms(),
            embedding_fetch_warn_threshold_ms: default_embedding_fetch_warn_threshold_ms(),
            embedding_fetch_min_candidates: 0,
            embedding_fetch_identity_mode: default_embedding_fetch_identity_mode(),
            embedding_fetch_allow_chunk_fallback: false,
            embedding_dense_representation_name: default_embedding_dense_representation_name(),
            embedding_cache_enabled: true,
            embedding_cache_max_entries: default_embedding_cache_max_entries(),
            embedding_cache_ttl_seconds: default_embedding_cache_ttl_seconds(),
            learned_reranker_enabled: false,
            learned_reranker_provider: default_learned_reranker_provider(),
            learned_reranker_top_n: default_learned_reranker_top_n(),
            learned_reranker_timeout_ms: default_learned_reranker_timeout_ms(),
            learned_reranker_weight: default_learned_reranker_weight(),
            retrieval_score_weight: default_retrieval_score_weight(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_semantic_backend() -> String {
    "IN_MEMORY".into()
}
fn default_semantic_top_k_per_chunk() -> usize {
    3
}
fn default_semantic_min_score() -> f32 {
    0.70
}
fn default_semantic_max_edges_per_document() -> usize {
    3000
}
fn default_semantic_max_chunks_for_in_memory() -> usize {
    500
}
fn default_semantic_large_document_policy() -> String {
    "SKIP_SEMANTIC".into()
}
fn default_semantic_batch_size() -> usize {
    256
}
fn default_semantic_rebuild_timeout_ms() -> u64 {
    30_000
}
fn default_semantic_warn_build_time_ms() -> u64 {
    3_000
}
fn default_direct_result_limit() -> usize {
    10
}
fn default_graph_expansion_result_limit() -> usize {
    10
}
fn default_final_context_limit() -> usize {
    8
}
fn default_graph_merge_strategy() -> String {
    "SCORE_THEN_TRUNCATE".into()
}
fn default_final_context_limit_mode() -> String {
    "AT_LEAST_TOP_K".into()
}
fn default_direct_context_limit() -> usize {
    6
}
fn default_graph_context_append_limit() -> usize {
    2
}
fn default_min_direct_contexts() -> usize {
    1
}
fn default_max_graph_fraction() -> f32 {
    0.50
}
fn default_max_graph_relations_debug_per_candidate() -> usize {
    5
}
fn default_structural_relation_weight() -> f32 {
    0.90
}
fn default_semantic_relation_weight() -> f32 {
    0.60
}
fn default_graph_min_score() -> f32 {
    0.05
}
fn default_structural_seed_score_floor() -> f32 {
    0.10
}
fn default_semantic_power() -> f32 {
    1.0
}
fn default_direct_score_weight() -> f32 {
    1.0
}
fn default_graph_score_weight() -> f32 {
    0.85
}
fn default_score_normalization() -> String {
    "NONE".into()
}
fn default_mmr_lambda() -> f32 {
    0.75
}
fn default_mmr_lambda_direct() -> f32 {
    0.80
}
fn default_mmr_lambda_graph() -> f32 {
    0.60
}
fn default_mmr_candidate_limit() -> usize {
    80
}
fn default_graph_min_useful_budget_ms() -> u64 {
    80
}
fn default_graph_response_reserve_ms() -> u64 {
    100
}
fn default_embedding_fetch_timeout_ms() -> u64 {
    250
}
fn default_mmr_embedding_min_useful_budget_ms() -> u64 {
    50
}
fn default_mmr_response_reserve_ms() -> u64 {
    75
}
fn default_embedding_fetch_warn_threshold_ms() -> u64 {
    250
}
fn default_embedding_cache_max_entries() -> usize {
    10_000
}
fn default_embedding_cache_ttl_seconds() -> u64 {
    3_600
}
fn default_embedding_fetch_identity_mode() -> String {
    "QDRANT_POINT_ID".into()
}
fn default_embedding_dense_representation_name() -> String {
    "dense".into()
}
fn default_mmr_similarity_source() -> String {
    "DENSE_EMBEDDING".into()
}
fn default_mmr_fallback_similarity_source() -> String {
    "TOKEN_JACCARD".into()
}
fn default_learned_reranker_provider() -> String {
    "NONE".into()
}
fn default_learned_reranker_top_n() -> usize {
    30
}
fn default_learned_reranker_timeout_ms() -> u64 {
    300
}
fn default_learned_reranker_weight() -> f32 {
    0.60
}
fn default_retrieval_score_weight() -> f32 {
    0.40
}
fn default_relation_weights() -> HashMap<String, f32> {
    HashMap::from([
        ("CHUNK_HAS_PARENT".into(), 0.95),
        ("CHUNK_PREVIOUS_SIBLING".into(), 0.90),
        ("CHUNK_NEXT_SIBLING".into(), 0.90),
        ("CHUNK_SAME_TABLE".into(), 0.85),
        ("CHUNK_SEMANTIC_SIMILAR".into(), 0.60),
        ("EXPLAINS".into(), 0.75),
        ("RELATED_TO".into(), 0.65),
        ("REPAIRED_BY".into(), 0.80),
        ("OBSERVED_BY".into(), 0.70),
        ("CONSTRAINED_BY".into(), 0.75),
        ("PRODUCES".into(), 0.75),
        ("CONSTRAINS".into(), 0.75),
        ("PROTECTED_BY".into(), 0.75),
        ("REQUIRES".into(), 0.75),
        ("PUBLISHES_TO".into(), 0.75),
    ])
}
fn default_graph_hop_penalty() -> HashMap<String, f32> {
    HashMap::from([
        ("hop_1".into(), 1.00),
        ("hop_2".into(), 0.70),
        ("hop_3".into(), 0.50),
    ])
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdaptiveConfig {
    pub mode: String,
    pub window_secs: u64,
    pub default_outbox_poll_interval_ms: u64,
    pub policies: AdaptivePoliciesConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdaptivePoliciesConfig {
    pub qdrant_scroll_page_size: AdaptivePolicyConfig,
    pub qdrant_scroll_max_concurrency: AdaptivePolicyConfig,
    pub publisher_batch_size: AdaptivePolicyConfig,
    pub outbox_poll_interval_ms: AdaptivePolicyConfig,
    pub embedding_batch_size: AdaptivePolicyConfig,
    pub qdrant_timeout_ms: AdaptivePolicyConfig,
    pub max_concurrent_search: AdaptivePolicyConfig,
    pub max_concurrent_indexing: AdaptivePolicyConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdaptivePolicyConfig {
    pub enabled: bool,
    pub min: u64,
    pub max: u64,
    pub step: u64,
    pub cooldown_secs: u64,
    pub ttl_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestionConfig {
    #[serde(default = "default_single_request_max_bytes")]
    pub single_request_max_bytes: usize,
    #[serde(default = "default_large_document_mode")]
    pub large_document_mode: String,
    #[serde(default = "default_true")]
    pub chunked_ingestion_enabled: bool,
    #[serde(default = "default_chunked_ingestion_max_batch_bytes")]
    pub chunked_ingestion_max_batch_bytes: usize,
    #[serde(default = "default_chunked_ingestion_max_blocks_per_batch")]
    pub chunked_ingestion_max_blocks_per_batch: usize,
    #[serde(default = "default_chunked_ingestion_session_ttl_seconds")]
    pub chunked_ingestion_session_ttl_seconds: u64,
    #[serde(default = "default_max_concurrent_ingestion_sessions")]
    pub max_concurrent_ingestion_sessions: usize,
    #[serde(default = "default_max_sessions_per_access_zone")]
    pub max_sessions_per_access_zone: usize,
    #[serde(default = "default_max_sessions_per_document")]
    pub max_sessions_per_document: usize,
    #[serde(default = "default_max_blocks_per_document")]
    pub max_blocks_per_document: usize,
    #[serde(default = "default_max_chunks_per_document")]
    pub max_chunks_per_document: usize,
    #[serde(default = "default_max_embeddings_per_request")]
    pub max_embeddings_per_request: usize,
    #[serde(default = "default_staging_cleanup_interval_seconds")]
    pub staging_cleanup_interval_seconds: u64,
    #[serde(default = "default_staging_completed_retention_seconds")]
    pub staging_completed_retention_seconds: u64,
    #[serde(default = "default_staging_aborted_retention_seconds")]
    pub staging_aborted_retention_seconds: u64,
    #[serde(default = "default_staging_expired_retention_seconds")]
    pub staging_expired_retention_seconds: u64,
    #[serde(default = "default_staging_max_bytes")]
    pub staging_max_bytes: u64,
    #[serde(default = "default_finalize_read_batch_size")]
    pub finalize_read_batch_size: usize,
    #[serde(default = "default_finalize_max_in_memory_blocks")]
    pub finalize_max_in_memory_blocks: usize,
    #[serde(default = "default_finalize_streaming_required_above_blocks")]
    pub finalize_streaming_required_above_blocks: usize,
    #[serde(default = "default_staging_completed_blocks_retention_seconds")]
    pub staging_completed_blocks_retention_seconds: u64,
    #[serde(default = "default_completed_session_result_retention_seconds")]
    pub completed_session_result_retention_seconds: u64,
    #[serde(default = "default_failed_session_retention_seconds")]
    pub failed_session_retention_seconds: u64,
    #[serde(default = "default_finalizing_stale_timeout_seconds")]
    pub finalizing_stale_timeout_seconds: u64,
    #[serde(default = "default_true")]
    pub finalizing_heartbeat_enabled: bool,
    #[serde(default = "default_finalizing_heartbeat_interval_seconds")]
    pub finalizing_heartbeat_interval_seconds: u64,
    #[serde(default = "default_finalize_mode")]
    pub finalize_mode: String,
    #[serde(default = "default_finalize_memory_guard_exceeded_status")]
    pub finalize_memory_guard_exceeded_status: String,
    #[serde(default = "default_staging_cleanup_batch_size")]
    pub staging_cleanup_batch_size: usize,
}
impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            single_request_max_bytes: default_single_request_max_bytes(),
            large_document_mode: default_large_document_mode(),
            chunked_ingestion_enabled: true,
            chunked_ingestion_max_batch_bytes: default_chunked_ingestion_max_batch_bytes(),
            chunked_ingestion_max_blocks_per_batch: default_chunked_ingestion_max_blocks_per_batch(
            ),
            chunked_ingestion_session_ttl_seconds: default_chunked_ingestion_session_ttl_seconds(),
            max_concurrent_ingestion_sessions: default_max_concurrent_ingestion_sessions(),
            max_sessions_per_access_zone: default_max_sessions_per_access_zone(),
            max_sessions_per_document: default_max_sessions_per_document(),
            max_blocks_per_document: default_max_blocks_per_document(),
            max_chunks_per_document: default_max_chunks_per_document(),
            max_embeddings_per_request: default_max_embeddings_per_request(),
            staging_cleanup_interval_seconds: default_staging_cleanup_interval_seconds(),
            staging_completed_retention_seconds: default_staging_completed_retention_seconds(),
            staging_aborted_retention_seconds: default_staging_aborted_retention_seconds(),
            staging_expired_retention_seconds: default_staging_expired_retention_seconds(),
            staging_max_bytes: default_staging_max_bytes(),
            finalize_read_batch_size: default_finalize_read_batch_size(),
            finalize_max_in_memory_blocks: default_finalize_max_in_memory_blocks(),
            finalize_streaming_required_above_blocks:
                default_finalize_streaming_required_above_blocks(),
            staging_completed_blocks_retention_seconds:
                default_staging_completed_blocks_retention_seconds(),
            completed_session_result_retention_seconds:
                default_completed_session_result_retention_seconds(),
            failed_session_retention_seconds: default_failed_session_retention_seconds(),
            finalizing_stale_timeout_seconds: default_finalizing_stale_timeout_seconds(),
            finalizing_heartbeat_enabled: true,
            finalizing_heartbeat_interval_seconds: default_finalizing_heartbeat_interval_seconds(),
            finalize_mode: default_finalize_mode(),
            finalize_memory_guard_exceeded_status: default_finalize_memory_guard_exceeded_status(),
            staging_cleanup_batch_size: default_staging_cleanup_batch_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingRuntimeConfig {
    #[serde(default = "default_document_submit_mode")]
    pub document_submit_mode: String,
    #[serde(default = "default_document_max_in_flight_chunks")]
    pub document_max_in_flight_chunks: usize,
    #[serde(default = "default_true")]
    pub document_preserve_order: bool,
    #[serde(default = "default_true")]
    pub cancel_on_error: bool,
    #[serde(default = "default_partial_embedding_failure_mode")]
    pub partial_embedding_failure_mode: String,
}
impl Default for EmbeddingRuntimeConfig {
    fn default() -> Self {
        Self {
            document_submit_mode: default_document_submit_mode(),
            document_max_in_flight_chunks: default_document_max_in_flight_chunks(),
            document_preserve_order: true,
            cancel_on_error: true,
            partial_embedding_failure_mode: default_partial_embedding_failure_mode(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ResilienceConfig {
    #[serde(default)]
    pub inference_retry: InferenceRetryConfig,
    #[serde(default)]
    pub qdrant_retry: QdrantRetryConfig,
}
#[derive(Debug, Clone, Deserialize)]
pub struct QdrantRetryConfig {
    #[serde(default = "default_qdrant_query_retry_policy")]
    pub query: RetryPolicyConfig,
    #[serde(default = "default_qdrant_background_retry_policy")]
    pub publisher: RetryPolicyConfig,
    #[serde(default = "default_qdrant_background_retry_policy")]
    pub reconciliation: RetryPolicyConfig,
}
impl Default for QdrantRetryConfig {
    fn default() -> Self {
        Self {
            query: default_qdrant_query_retry_policy(),
            publisher: default_qdrant_background_retry_policy(),
            reconciliation: default_qdrant_background_retry_policy(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct InferenceRetryConfig {
    #[serde(default = "default_query_retry_policy")]
    pub query: RetryPolicyConfig,
    #[serde(default = "default_document_retry_policy")]
    pub document: RetryPolicyConfig,
}
impl Default for InferenceRetryConfig {
    fn default() -> Self {
        Self {
            query: default_query_retry_policy(),
            document: default_document_retry_policy(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct RetryPolicyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_retry_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_retry_base_delay_ms")]
    pub base_delay_ms: u64,
    #[serde(default = "default_retry_max_delay_ms")]
    pub max_delay_ms: u64,
    #[serde(default = "default_true")]
    pub jitter_enabled: bool,
    #[serde(default)]
    pub retry_on_statuses: Vec<u16>,
    #[serde(default = "default_true")]
    pub retry_on_timeout: bool,
    #[serde(default = "default_true")]
    pub retry_on_unavailable: bool,
    #[serde(default = "default_true")]
    pub retry_on_connect: bool,
    #[serde(default = "default_retry_min_remaining_budget_ms")]
    pub min_remaining_budget_ms: u64,
    #[serde(default = "default_qdrant_min_operation_budget_ms")]
    pub min_operation_budget_ms: u64,
    #[serde(default = "default_qdrant_safety_margin_ms")]
    pub safety_margin_ms: u64,
}
impl Default for RetryPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: default_retry_max_attempts(),
            base_delay_ms: default_retry_base_delay_ms(),
            max_delay_ms: default_retry_max_delay_ms(),
            jitter_enabled: true,
            retry_on_statuses: vec![429, 502, 503, 504],
            retry_on_timeout: true,
            retry_on_unavailable: true,
            retry_on_connect: true,
            min_remaining_budget_ms: default_retry_min_remaining_budget_ms(),
            min_operation_budget_ms: default_qdrant_min_operation_budget_ms(),
            safety_margin_ms: default_qdrant_safety_margin_ms(),
        }
    }
}

fn default_qdrant_query_retry_policy() -> RetryPolicyConfig {
    RetryPolicyConfig {
        max_attempts: 2,
        retry_on_statuses: vec![502, 503, 504],
        retry_on_timeout: false,
        retry_on_connect: true,
        min_operation_budget_ms: 150,
        safety_margin_ms: 50,
        ..RetryPolicyConfig::default()
    }
}

fn default_qdrant_background_retry_policy() -> RetryPolicyConfig {
    RetryPolicyConfig {
        max_attempts: 5,
        retry_on_statuses: vec![429, 502, 503, 504],
        retry_on_timeout: true,
        retry_on_connect: true,
        ..RetryPolicyConfig::default()
    }
}

fn default_qdrant_min_operation_budget_ms() -> u64 {
    150
}

fn default_qdrant_safety_margin_ms() -> u64 {
    50
}

fn default_query_retry_policy() -> RetryPolicyConfig {
    RetryPolicyConfig {
        max_attempts: 2,
        base_delay_ms: 50,
        max_delay_ms: 100,
        retry_on_timeout: false,
        min_remaining_budget_ms: 350,
        ..RetryPolicyConfig::default()
    }
}

fn default_document_retry_policy() -> RetryPolicyConfig {
    RetryPolicyConfig {
        max_attempts: 3,
        base_delay_ms: 100,
        max_delay_ms: 1000,
        retry_on_timeout: true,
        min_remaining_budget_ms: 1000,
        ..RetryPolicyConfig::default()
    }
}

fn default_retry_min_remaining_budget_ms() -> u64 {
    350
}

#[derive(Debug, Clone, Deserialize)]
pub struct RagContextConfig {
    #[serde(default = "default_true")]
    pub token_budget_enabled: bool,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
    #[serde(default = "default_reserved_answer_tokens")]
    pub reserved_answer_tokens: usize,
    #[serde(default = "default_rag_tokenizer")]
    pub tokenizer: String,
    #[serde(default = "default_chars_per_token")]
    pub chars_per_token: usize,
    #[serde(default = "default_tokenizer_safety_margin_percent")]
    pub tokenizer_safety_margin_percent: usize,
    #[serde(default = "default_truncation_strategy")]
    pub truncation_strategy: String,
    #[serde(default = "default_huge_chunk_strategy")]
    pub huge_chunk_strategy: String,
    #[serde(default)]
    pub allow_chunk_text_truncation: bool,
    #[serde(default = "default_min_direct_token_fraction")]
    pub min_direct_token_fraction: f32,
    #[serde(default = "default_max_graph_token_fraction")]
    pub max_graph_token_fraction: f32,
}
impl Default for RagContextConfig {
    fn default() -> Self {
        Self {
            token_budget_enabled: true,
            max_context_tokens: default_max_context_tokens(),
            reserved_answer_tokens: default_reserved_answer_tokens(),
            tokenizer: default_rag_tokenizer(),
            chars_per_token: default_chars_per_token(),
            tokenizer_safety_margin_percent: default_tokenizer_safety_margin_percent(),
            truncation_strategy: default_truncation_strategy(),
            huge_chunk_strategy: default_huge_chunk_strategy(),
            allow_chunk_text_truncation: false,
            min_direct_token_fraction: default_min_direct_token_fraction(),
            max_graph_token_fraction: default_max_graph_token_fraction(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LimitsConfig {
    #[serde(default = "default_single_request_max_bytes")]
    pub source_text_max_bytes: usize,
    #[serde(default = "default_source_text_absolute_max_bytes")]
    pub source_text_absolute_max_bytes: usize,
    #[serde(default = "default_search_top_k_max")]
    pub search_top_k_max: u32,
    #[serde(default = "default_search_candidate_limit_max")]
    pub search_candidate_limit_max: u32,
    #[serde(default = "default_graph_related_contexts_max")]
    pub graph_related_contexts_max: usize,
    #[serde(default = "default_max_chunks_per_document")]
    pub max_chunks_per_document: usize,
    #[serde(default = "default_max_embeddings_per_request")]
    pub max_embeddings_per_request: usize,
    #[serde(default = "default_max_concurrent_retrieve_context")]
    pub max_concurrent_retrieve_context: usize,
    #[serde(default = "default_max_concurrent_qdrant_search")]
    pub max_concurrent_qdrant_search: usize,
    #[serde(default = "default_max_concurrent_graph_expansion")]
    pub max_concurrent_graph_expansion: usize,
    #[serde(default = "default_max_concurrent_mmr_fetch")]
    pub max_concurrent_mmr_fetch: usize,
    #[serde(default = "default_backpressure_acquire_timeout_ms")]
    pub backpressure_acquire_timeout_ms: u64,
    #[serde(default)]
    pub allow_dangerous_limit_override: bool,
}
impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            source_text_max_bytes: default_single_request_max_bytes(),
            source_text_absolute_max_bytes: default_source_text_absolute_max_bytes(),
            search_top_k_max: default_search_top_k_max(),
            search_candidate_limit_max: default_search_candidate_limit_max(),
            graph_related_contexts_max: default_graph_related_contexts_max(),
            max_chunks_per_document: default_max_chunks_per_document(),
            max_embeddings_per_request: default_max_embeddings_per_request(),
            max_concurrent_retrieve_context: default_max_concurrent_retrieve_context(),
            max_concurrent_qdrant_search: default_max_concurrent_qdrant_search(),
            max_concurrent_graph_expansion: default_max_concurrent_graph_expansion(),
            max_concurrent_mmr_fetch: default_max_concurrent_mmr_fetch(),
            backpressure_acquire_timeout_ms: default_backpressure_acquire_timeout_ms(),
            allow_dangerous_limit_override: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingCachePolicyConfig {
    #[serde(default = "default_reindex_behavior")]
    pub reindex_behavior: String,
    #[serde(default)]
    pub invalidate_memory_cache_on_reindex: bool,
}
impl Default for EmbeddingCachePolicyConfig {
    fn default() -> Self {
        Self {
            reindex_behavior: default_reindex_behavior(),
            invalidate_memory_cache_on_reindex: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkingRuntimeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_chunking_profile_version")]
    pub profile_version: String,
    #[serde(default)]
    pub parent: Option<serde_yaml::Value>,
    #[serde(default, rename = "sub_180")]
    pub sub_180: Option<serde_yaml::Value>,
    #[serde(default, rename = "sub_260")]
    pub sub_260: Option<serde_yaml::Value>,
    #[serde(default = "default_source_chunk_storage_mode")]
    pub source_chunk_storage_mode: String,
}
impl Default for ChunkingRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            profile_version: default_chunking_profile_version(),
            parent: None,
            sub_180: None,
            sub_260: None,
            source_chunk_storage_mode: default_source_chunk_storage_mode(),
        }
    }
}

fn default_hybrid_fusion_method() -> String {
    "RRF".into()
}
fn default_hybrid_dense_weight() -> f32 {
    0.6
}
fn default_hybrid_sparse_weight() -> f32 {
    0.4
}
fn default_lexical_backend() -> String {
    "POSTGRES_FTS".into()
}
fn default_lexical_candidate_limit() -> u32 {
    50
}
fn default_lexical_max_candidate_limit() -> u32 {
    100
}
fn default_lexical_min_remaining_budget_ms() -> u64 {
    150
}
fn default_lexical_statement_timeout_ms() -> u64 {
    150
}
fn default_lexical_response_reserve_ms() -> u64 {
    75
}
fn default_lexical_trigram_min_similarity() -> f32 {
    0.70
}
fn default_lexical_sparse_candidate_floor() -> usize {
    10
}
fn default_lexical_sparse_score_floor() -> f32 {
    0.10
}
fn default_lexical_weight() -> f32 {
    0.20
}
fn default_no_answer_enabled() -> bool {
    true
}
fn default_no_answer_min_dense_score() -> f32 {
    0.25
}
fn default_no_answer_min_sparse_score() -> f32 {
    0.10
}
fn default_no_answer_min_hybrid_score() -> f32 {
    0.30
}
fn default_no_answer_sparse_only_min_matched_terms() -> usize {
    2
}
fn default_no_answer_sparse_only_require_technical_token() -> bool {
    true
}
fn default_no_answer_exact_technical_boost() -> f32 {
    0.50
}
fn default_no_answer_hard_negative_strict() -> bool {
    true
}
fn default_no_answer_debug_candidates() -> bool {
    false
}
fn default_single_request_max_bytes() -> usize {
    2 * 1024 * 1024
}
fn default_large_document_mode() -> String {
    "REQUIRE_CHUNKED".into()
}
fn default_chunked_ingestion_max_batch_bytes() -> usize {
    1024 * 1024
}
fn default_chunked_ingestion_max_blocks_per_batch() -> usize {
    500
}
fn default_chunked_ingestion_session_ttl_seconds() -> u64 {
    3600
}
fn default_max_concurrent_ingestion_sessions() -> usize {
    1000
}
fn default_max_sessions_per_access_zone() -> usize {
    100
}
fn default_max_sessions_per_document() -> usize {
    3
}
fn default_max_blocks_per_document() -> usize {
    100_000
}
fn default_max_chunks_per_document() -> usize {
    50_000
}
fn default_max_embeddings_per_request() -> usize {
    50_000
}
fn default_staging_cleanup_interval_seconds() -> u64 {
    300
}
fn default_staging_completed_retention_seconds() -> u64 {
    86_400
}
fn default_staging_aborted_retention_seconds() -> u64 {
    3_600
}
fn default_staging_expired_retention_seconds() -> u64 {
    3_600
}
fn default_staging_max_bytes() -> u64 {
    10 * 1024 * 1024 * 1024
}
fn default_finalize_read_batch_size() -> usize {
    1_000
}
fn default_finalize_max_in_memory_blocks() -> usize {
    5_000
}
fn default_finalize_streaming_required_above_blocks() -> usize {
    5_000
}
fn default_staging_completed_blocks_retention_seconds() -> u64 {
    86_400
}
fn default_completed_session_result_retention_seconds() -> u64 {
    604_800
}
fn default_failed_session_retention_seconds() -> u64 {
    86_400
}
fn default_finalizing_stale_timeout_seconds() -> u64 {
    7_200
}
fn default_finalizing_heartbeat_interval_seconds() -> u64 {
    30
}
fn default_finalize_mode() -> String {
    "BOUNDED_IN_MEMORY".into()
}
fn default_finalize_memory_guard_exceeded_status() -> String {
    "RETURN_TO_ACTIVE".into()
}
fn default_staging_cleanup_batch_size() -> usize {
    1_000
}
fn default_document_submit_mode() -> String {
    "BOUNDED_CONCURRENT".into()
}
fn default_document_max_in_flight_chunks() -> usize {
    32
}
fn default_partial_embedding_failure_mode() -> String {
    "FAIL_DOCUMENT".into()
}
fn default_retry_max_attempts() -> u32 {
    3
}
fn default_retry_base_delay_ms() -> u64 {
    100
}
fn default_retry_max_delay_ms() -> u64 {
    1000
}
fn default_max_context_tokens() -> usize {
    6000
}
fn default_reserved_answer_tokens() -> usize {
    1000
}
fn default_min_direct_token_fraction() -> f32 {
    0.50
}
fn default_max_graph_token_fraction() -> f32 {
    0.40
}
fn default_rag_tokenizer() -> String {
    "APPROX_CHARS".into()
}
fn default_chars_per_token() -> usize {
    3
}
fn default_tokenizer_safety_margin_percent() -> usize {
    20
}
fn default_truncation_strategy() -> String {
    "DROP_LOWEST_MMR_SCORE_CHUNKS".into()
}
fn default_huge_chunk_strategy() -> String {
    "DROP_ONE_HUGE_CHUNK".into()
}
fn default_source_text_absolute_max_bytes() -> usize {
    50 * 1024 * 1024
}
fn default_search_top_k_max() -> u32 {
    50
}
fn default_max_concurrent_retrieve_context() -> usize {
    128
}
fn default_max_concurrent_qdrant_search() -> usize {
    256
}
fn default_max_concurrent_graph_expansion() -> usize {
    32
}
fn default_max_concurrent_mmr_fetch() -> usize {
    32
}
fn default_backpressure_acquire_timeout_ms() -> u64 {
    50
}
fn default_search_candidate_limit_max() -> u32 {
    200
}
fn default_hydration_rejection_reserve() -> u32 {
    4
}
fn default_hydration_rejection_reserve_max() -> u32 {
    16
}
fn default_graph_related_contexts_max() -> usize {
    20
}
fn default_reindex_behavior() -> String {
    "CREATE_NEW_POINT_ID".into()
}
fn default_chunking_profile_version() -> String {
    "multi-granularity-v1".into()
}
fn default_source_chunk_storage_mode() -> String {
    "METADATA_ONLY".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub instance_id: String,
    pub environment: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct GrpcConfig {
    pub host: String,
    pub port: u16,
    pub max_request_message_mb: usize,
    pub max_response_message_mb: usize,
    pub max_items_per_batch: usize,
    pub deadlines: DeadlineConfig,
    pub compression: CompressionConfig,
}
#[derive(Debug, Clone, Deserialize)]
pub struct DeadlineConfig {
    pub query_ms: u64,
    pub document_batch_ms: u64,
    pub contract_ms: u64,
    pub health_ms: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CompressionConfig {
    pub enabled: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub enabled: bool,
    pub api_key: String,
    pub protect_health: bool,
    #[serde(default)]
    pub trust_forwarded_identity_headers: bool,
    #[serde(default = "default_gateway_trust_header")]
    pub gateway_trust_header: String,
    #[serde(default)]
    pub gateway_trust_token: String,
}
fn default_gateway_trust_header() -> String {
    "x-astravector-gateway-trust".into()
}
#[derive(Debug, Clone, Deserialize)]
pub struct ShutdownConfig {
    pub drain_timeout_seconds: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub multiplier: f64,
    pub max_delay_ms: u64,
    pub jitter: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub format: String,
    pub precision: String,
    pub path: String,
    pub checksum: String,
    pub version: String,
    pub dense_output_name: String,
    pub token_output_name: String,
    pub sparse_output_name: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct TokenizerConfig {
    pub path: String,
    pub checksum: String,
    pub version: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct InferenceConfig {
    pub provider: ProviderConfig,
    pub warmup_batches: usize,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub mode: String,
    pub preference: Vec<String>,
    pub fallback_to_cpu: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct DenseConfig {
    pub enabled: bool,
    pub name: String,
    pub dimension: usize,
    pub pooling: String,
    pub normalize: bool,
    pub distance: String,
    pub version: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct SparseConfig {
    pub enabled: bool,
    pub required: bool,
    pub name: String,
    pub min_weight: f32,
    pub max_non_zero: usize,
    pub duplicate_merge: String,
    pub version: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct TokenizationConfig {
    pub query: TokenProfile,
    pub child: TokenProfile,
    pub parent: ParentTokenProfile,
}
#[derive(Debug, Clone, Deserialize)]
pub struct TokenProfile {
    pub max_length: usize,
    pub truncation_allowed: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ParentTokenProfile {
    pub enabled: bool,
    pub max_length: usize,
    pub truncation_allowed: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub scope: String,
    pub l1: L1Config,
    pub l2: L2Config,
}
#[derive(Debug, Clone, Deserialize)]
pub struct L1Config {
    pub enabled: bool,
    pub max_entries: u64,
    pub ttl_minutes: u64,
    pub idle_timeout_minutes: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct L2Config {
    pub enabled: bool,
    pub lease_duration_seconds: u64,
    pub processing_poll_interval_ms: u64,
    pub processing_poll_max_interval_ms: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct BatchingConfig {
    pub query: BatchProfile,
    pub document: BatchProfile,
    pub buckets: Vec<usize>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct BatchProfile {
    pub max_wait_ms: u64,
    pub max_items: usize,
    pub max_padded_tokens: usize,
    pub queue_capacity: usize,
    #[serde(default)]
    pub max_queue_age_ms: u64,
    #[serde(default)]
    pub min_inference_budget_ms: u64,
    #[serde(default)]
    pub max_deadline_skew_ms: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    pub max_consecutive_query_batches: usize,
}
#[derive(Debug, Clone, Deserialize)]
pub struct PostgresConfig {
    pub enabled: bool,
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_ms: u64,
    pub statement_timeout_ms: u64,
    pub lock_timeout_ms: u64,
    pub idle_in_transaction_session_timeout_ms: u64,
    pub auto_migrate: bool,
    pub required_on_startup: bool,
    pub required_for_readiness: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RecoveryConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub processing_timeout_seconds: i64,
    pub batch_size: i64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    pub query_requests_days: i64,
    pub document_requests_days: i64,
    pub failed_requests_days: i64,
    pub cache_unused_days: i64,
    pub delete_batch_size: i64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub host: String,
    pub port: u16,
}
impl AppConfig {
    pub fn load() -> Result<Self> {
        let base_path =
            env::var("ASTRAVECTOR_CONFIG").unwrap_or_else(|_| "config/application.yaml".into());
        let profile = active_profile();
        let mut merged = read_yaml_value(&base_path)?;

        if let Some(profile_path) = profile_config_path(&base_path, &profile) {
            if profile_path.exists() {
                let profile_value = read_yaml_value(profile_path.to_string_lossy().as_ref())?;
                merge_yaml(&mut merged, profile_value);
            } else if env::var("ASTRAVECTOR_PROFILE_CONFIG").is_ok() {
                anyhow::bail!("profile config not found: {}", profile_path.display());
            }
        }

        apply_query_processing_compatibility(&mut merged)?;
        let cfg: AppConfig =
            serde_yaml::from_value(merged).context("parse AstraVector configuration")?;
        Ok(cfg)
    }
    pub fn active_profile_name() -> String {
        active_profile()
    }
    pub fn validate(&self) -> Result<()> {
        if Self::active_profile_name() == "search-production-candidate" {
            anyhow::ensure!(
                self.batching.query.queue_capacity <= 256
                    && self.batching.query.max_queue_age_ms > 0,
                "INVALID_QUERY_QUEUE_CONFIGURATION"
            );
            anyhow::ensure!(
                self.batching.query.min_inference_budget_ms > 0
                    && self.batching.query.max_deadline_skew_ms > 0
                    && self.grpc.deadlines.query_ms > self.batching.query.min_inference_budget_ms,
                "INVALID_QUERY_BUDGET_CONFIGURATION"
            );
            anyhow::ensure!(
                self.limits.backpressure_acquire_timeout_ms < self.grpc.deadlines.query_ms,
                "INVALID_BACKPRESSURE_CONFIGURATION"
            );
        }
        if self.security.enabled
            && self.security.trust_forwarded_identity_headers
            && self.security.gateway_trust_token.trim().is_empty()
        {
            anyhow::bail!("security.trust_forwarded_identity_headers=true requires security.gateway_trust_token");
        }
        anyhow::ensure!(
            self.postgres.statement_timeout_ms > 0,
            "postgres.statement_timeout_ms must be positive"
        );
        anyhow::ensure!(
            self.postgres.lock_timeout_ms > 0,
            "postgres.lock_timeout_ms must be positive"
        );
        anyhow::ensure!(
            self.postgres.idle_in_transaction_session_timeout_ms > 0,
            "postgres.idle_in_transaction_session_timeout_ms must be positive"
        );
        anyhow::ensure!(self.dense.dimension == 1024, "dense dimension must be 1024");
        anyhow::ensure!(
            self.grpc.max_items_per_batch > 0,
            "max_items_per_batch must be positive"
        );
        anyhow::ensure!(
            Path::new(&self.tokenizer.path).exists(),
            "tokenizer missing: {}",
            self.tokenizer.path
        );
        anyhow::ensure!(
            Path::new(&self.model.path).exists(),
            "model missing: {}",
            self.model.path
        );
        if self.service.environment.eq_ignore_ascii_case("production") {
            anyhow::ensure!(
                !self.model.checksum.is_empty(),
                "model checksum required in production"
            );
            anyhow::ensure!(
                !self.tokenizer.checksum.is_empty(),
                "tokenizer checksum required in production"
            );
            anyhow::ensure!(
                self.security.enabled,
                "authentication must be enabled in production"
            );
            anyhow::ensure!(
                !self.security.api_key.is_empty(),
                "api key required in production"
            );
            if self.security.trust_forwarded_identity_headers {
                anyhow::ensure!(
                    !self.security.gateway_trust_token.trim().is_empty(),
                    "gateway trust token required when forwarded identity headers are trusted in production"
                );
            }
            if self.qdrant.enabled {
                anyhow::ensure!(!self.qdrant.url.is_empty(), "qdrant url required");
                anyhow::ensure!(
                    !self.qdrant.collection.is_empty(),
                    "qdrant collection required"
                );
            }
        }
        anyhow::ensure!(
            self.lifecycle.min_ttl_days > 0,
            "min_ttl_days must be positive"
        );
        anyhow::ensure!(
            self.lifecycle.max_ttl_days >= self.lifecycle.min_ttl_days,
            "invalid ttl range"
        );

        anyhow::ensure!(
            self.graph_rag.build.max_document_graph_nodes > 0,
            "graph_rag.build.max_document_graph_nodes must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.build.bulk_insert_batch_size > 0,
            "graph_rag.build.bulk_insert_batch_size must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.retrieval.max_hops <= 1,
            "fix3_graph_lite supports graph_rag.retrieval.max_hops <= 1"
        );
        anyhow::ensure!(
            self.graph_rag.retrieval.timeout_ms > 0,
            "graph_rag.retrieval.timeout_ms must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.retrieval.min_useful_budget_ms > 0
                && self.graph_rag.retrieval.response_reserve_ms > 0,
            "graph_rag retrieval stage budgets must be positive"
        );
        let mut allowed_graph_relations = std::collections::HashSet::new();
        for relation in &self.graph_rag.retrieval.allowed_relations {
            let canonical = relation.trim().to_ascii_uppercase();
            anyhow::ensure!(
                !canonical.is_empty(),
                "Invalid GraphRAG configuration: allowed relation type must not be empty"
            );
            canonical.parse::<GraphRelationType>().map_err(|_| {
                anyhow::anyhow!(
                    "Invalid GraphRAG configuration: unsupported allowed relation type `{canonical}`"
                )
            })?;
            anyhow::ensure!(
                allowed_graph_relations.insert(canonical.clone()),
                "Invalid GraphRAG configuration: duplicate allowed relation type `{canonical}`"
            );
        }
        for (relation, weight) in &self.graph_rag.scoring.relation_weights {
            let canonical = relation.trim().to_ascii_uppercase();
            canonical.parse::<GraphRelationType>().map_err(|_| {
                anyhow::anyhow!(
                    "Invalid GraphRAG configuration: unsupported relation weight key `{canonical}`"
                )
            })?;
            anyhow::ensure!(
                weight.is_finite() && *weight > 0.0 && *weight <= 1.0,
                "Invalid GraphRAG configuration: relation weight `{canonical}` must be finite and in (0, 1]"
            );
        }
        anyhow::ensure!(
            self.graph_rag.retrieval.final_context_limit > 0,
            "graph_rag.retrieval.final_context_limit must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.retrieval.min_direct_contexts
                <= self.graph_rag.retrieval.final_context_limit,
            "graph_rag.retrieval.min_direct_contexts must not exceed final_context_limit"
        );
        anyhow::ensure!(
            self.graph_rag.retrieval.max_graph_fraction.is_finite()
                && (0.0..=1.0).contains(&self.graph_rag.retrieval.max_graph_fraction),
            "graph_rag.retrieval.max_graph_fraction must be finite and in [0, 1]"
        );
        anyhow::ensure!(
            self.rag_context.min_direct_token_fraction.is_finite()
                && (0.0..=1.0).contains(&self.rag_context.min_direct_token_fraction),
            "rag_context.min_direct_token_fraction must be finite and in [0, 1]"
        );
        anyhow::ensure!(
            self.rag_context.max_graph_token_fraction.is_finite()
                && (0.0..=1.0).contains(&self.rag_context.max_graph_token_fraction),
            "rag_context.max_graph_token_fraction must be finite and in [0, 1]"
        );
        anyhow::ensure!(
            self.search.ranking_trace.max_candidates > 0
                && self.search.ranking_trace.max_stages_per_candidate > 0,
            "search.ranking_trace limits must be positive"
        );
        anyhow::ensure!(
            matches!(
                self.graph_rag.retrieval.final_context_limit_mode.as_str(),
                "STRICT" | "AT_LEAST_TOP_K"
            ),
            "graph_rag.retrieval.final_context_limit_mode must be STRICT or AT_LEAST_TOP_K"
        );
        anyhow::ensure!(
            matches!(self.graph_rag.retrieval.graph_merge_strategy.as_str(), "SCORE_THEN_TRUNCATE" | "DIRECT_FIRST" | "GRAPH_AS_CONTEXT_APPEND"),
            "graph_rag.retrieval.graph_merge_strategy must be SCORE_THEN_TRUNCATE, DIRECT_FIRST, or GRAPH_AS_CONTEXT_APPEND"
        );
        if self.graph_rag.retrieval.graph_merge_strategy == "GRAPH_AS_CONTEXT_APPEND" {
            anyhow::ensure!(
                self.graph_rag.retrieval.direct_context_limit > 0
                    || self.graph_rag.retrieval.graph_context_append_limit > 0,
                "GRAPH_AS_CONTEXT_APPEND requires at least one positive direct or graph budget"
            );
        }
        anyhow::ensure!(
            (0.25..=2.0).contains(&self.graph_rag.scoring.semantic_power),
            "graph_rag.scoring.semantic_power must be in range [0.25, 2.0]"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.graph_rag.scoring.graph_min_score),
            "graph_rag.scoring.graph_min_score must be in range [0.0, 1.0]"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.graph_rag.scoring.structural_seed_score_floor),
            "graph_rag.scoring.structural_seed_score_floor must be in range [0.0, 1.0]"
        );
        anyhow::ensure!(
            self.graph_rag.scoring.direct_score_weight >= 0.0,
            "graph_rag.scoring.direct_score_weight must be >= 0.0"
        );
        anyhow::ensure!(
            self.graph_rag.scoring.graph_score_weight >= 0.0,
            "graph_rag.scoring.graph_score_weight must be >= 0.0"
        );
        anyhow::ensure!(
            self.graph_rag.scoring.score_normalization == "NONE",
            "graph_rag.scoring.score_normalization currently supports only NONE; MIN_MAX is reserved for a future fix"
        );
        anyhow::ensure!(
            self.graph_rag.build.semantic_max_chunks_for_in_memory > 0,
            "graph_rag.build.semantic_max_chunks_for_in_memory must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.build.semantic_top_k_per_chunk > 0,
            "graph_rag.build.semantic_top_k_per_chunk must be positive"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.graph_rag.build.semantic_min_score),
            "graph_rag.build.semantic_min_score must be in range [0.0, 1.0]"
        );
        anyhow::ensure!(
            matches!(self.graph_rag.build.semantic_large_document_policy.as_str(), "SKIP_SEMANTIC" | "STRUCTURAL_ONLY" | "FAIL_INDEXING" | "QDRANT_BACKEND"),
            "graph_rag.build.semantic_large_document_policy must be SKIP_SEMANTIC, STRUCTURAL_ONLY, FAIL_INDEXING, or QDRANT_BACKEND"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.graph_rag.rerank.mmr_lambda),
            "graph_rag.rerank.mmr_lambda must be in range [0.0, 1.0]"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.graph_rag.rerank.mmr_lambda_direct),
            "graph_rag.rerank.mmr_lambda_direct must be in range [0.0, 1.0]"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.graph_rag.rerank.mmr_lambda_graph),
            "graph_rag.rerank.mmr_lambda_graph must be in range [0.0, 1.0]"
        );
        anyhow::ensure!(
            self.graph_rag.rerank.mmr_candidate_limit > 0,
            "graph_rag.rerank.mmr_candidate_limit must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.rerank.embedding_fetch_timeout_ms > 0,
            "graph_rag.rerank.embedding_fetch_timeout_ms must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.rerank.embedding_fetch_min_useful_budget_ms > 0
                && self.graph_rag.rerank.response_reserve_ms > 0,
            "graph_rag rerank stage budgets must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.rerank.embedding_fetch_warn_threshold_ms > 0,
            "graph_rag.rerank.embedding_fetch_warn_threshold_ms must be positive"
        );
        anyhow::ensure!(
            self.graph_rag.rerank.embedding_fetch_identity_mode.as_str() == "QDRANT_POINT_ID",
            "graph_rag.rerank.embedding_fetch_identity_mode must be QDRANT_POINT_ID in fix4.3; BINDING_ID and CHUNK_REPRESENTATION_VERSION are reserved"
        );
        anyhow::ensure!(
            !self
                .graph_rag
                .rerank
                .embedding_dense_representation_name
                .trim()
                .is_empty(),
            "graph_rag.rerank.embedding_dense_representation_name must not be empty"
        );
        if self.graph_rag.rerank.embedding_cache_enabled {
            anyhow::ensure!(
                self.graph_rag.rerank.embedding_cache_max_entries > 0,
                "graph_rag.rerank.embedding_cache_max_entries must be positive when cache is enabled"
            );
            anyhow::ensure!(
                self.graph_rag.rerank.embedding_cache_ttl_seconds > 0,
                "graph_rag.rerank.embedding_cache_ttl_seconds must be positive when cache is enabled"
            );
        }
        anyhow::ensure!(
            self.graph_rag
                .retrieval
                .max_graph_relations_debug_per_candidate
                > 0,
            "graph_rag.retrieval.max_graph_relations_debug_per_candidate must be positive"
        );
        anyhow::ensure!(
            !self.graph_rag.rerank.learned_reranker_enabled,
            "graph_rag.rerank.learned_reranker_enabled is reserved and must remain false in fix4"
        );
        anyhow::ensure!(
            matches!(
                self.graph_rag.rerank.learned_reranker_provider.as_str(),
                "NONE" | "ONNX" | "EXTERNAL_HTTP"
            ),
            "graph_rag.rerank.learned_reranker_provider must be NONE, ONNX, or EXTERNAL_HTTP"
        );

        anyhow::ensure!(
            matches!(
                self.search.hybrid_fusion_method.as_str(),
                "RRF" | "WEIGHTED_SCORE" | "NORMALIZED_WEIGHTED_SCORE"
            ),
            "search.hybrid_fusion_method must be RRF, WEIGHTED_SCORE, or NORMALIZED_WEIGHTED_SCORE"
        );
        anyhow::ensure!(
            self.search.hybrid_dense_weight >= 0.0,
            "search.hybrid_dense_weight must be >= 0.0"
        );
        anyhow::ensure!(
            self.search.hybrid_sparse_weight >= 0.0,
            "search.hybrid_sparse_weight must be >= 0.0"
        );
        anyhow::ensure!(
            self.search.hybrid_dense_weight + self.search.hybrid_sparse_weight > 0.0,
            "hybrid fusion weights must have positive sum"
        );
        anyhow::ensure!(self.search.rrf_k > 0.0, "search.rrf_k must be positive");
        anyhow::ensure!(
            self.search.hydration_rejection_reserve <= self.search.hydration_rejection_reserve_max,
            "search.hydration_rejection_reserve must not exceed hydration_rejection_reserve_max"
        );
        anyhow::ensure!(
            self.search.hydration_rejection_reserve_max <= self.limits.search_candidate_limit_max,
            "search.hydration_rejection_reserve_max must not exceed search candidate limit max"
        );
        anyhow::ensure!(
            if self.search.hydration_failpoints.non_production_enabled {
                !self.search.hydration_failpoints.plan_path.trim().is_empty()
                    && !self.search.hydration_failpoints.run_id.trim().is_empty()
            } else {
                self.search.hydration_failpoints.plan_path.trim().is_empty()
                    && self.search.hydration_failpoints.run_id.trim().is_empty()
            },
            "hydration failpoints require enabled + plan_path + run_id together"
        );
        anyhow::ensure!(
            self.search.lexical.backend == "POSTGRES_FTS",
            "search.lexical.backend must be POSTGRES_FTS"
        );
        anyhow::ensure!(
            self.search.lexical.candidate_limit > 0
                && self.search.lexical.candidate_limit <= self.search.lexical.max_candidate_limit,
            "search.lexical candidate limits are invalid"
        );
        anyhow::ensure!(
            self.search.lexical.min_remaining_budget_ms > 0
                && self.search.lexical.statement_timeout_ms > 0
                && self.search.lexical.response_reserve_ms > 0,
            "search.lexical budgets must be positive"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.search.lexical.trigram_min_similarity),
            "search.lexical.trigram_min_similarity must be within [0,1]"
        );
        anyhow::ensure!(
            self.search.lexical.lexical_weight >= 0.0,
            "search.lexical.lexical_weight must be >= 0"
        );
        anyhow::ensure!(
            self.search.no_answer.min_dense_score >= 0.0,
            "search.no_answer.min_dense_score must be >= 0.0"
        );
        anyhow::ensure!(
            self.search.no_answer.min_sparse_score >= 0.0,
            "search.no_answer.min_sparse_score must be >= 0.0"
        );
        anyhow::ensure!(
            self.search.no_answer.min_hybrid_score >= 0.0,
            "search.no_answer.min_hybrid_score must be >= 0.0"
        );
        anyhow::ensure!(
            self.search.no_answer.exact_technical_boost >= 0.0,
            "search.no_answer.exact_technical_boost must be >= 0.0"
        );

        anyhow::ensure!(
            self.ingestion.single_request_max_bytes > 0,
            "ingestion.single_request_max_bytes must be positive"
        );
        anyhow::ensure!(matches!(self.ingestion.large_document_mode.as_str(), "REJECT" | "REQUIRE_CHUNKED"), "ingestion.large_document_mode must be REJECT or REQUIRE_CHUNKED; ACCEPT_WITH_WARNING is reserved until true streaming single-request ingestion is implemented");
        anyhow::ensure!(
            self.ingestion.chunked_ingestion_max_batch_bytes > 0,
            "ingestion.chunked_ingestion_max_batch_bytes must be positive"
        );
        anyhow::ensure!(
            self.ingestion.chunked_ingestion_max_blocks_per_batch > 0,
            "ingestion.chunked_ingestion_max_blocks_per_batch must be positive"
        );
        anyhow::ensure!(
            self.ingestion.max_concurrent_ingestion_sessions > 0,
            "ingestion.max_concurrent_ingestion_sessions must be positive"
        );
        anyhow::ensure!(
            self.ingestion.max_sessions_per_access_zone > 0,
            "ingestion.max_sessions_per_access_zone must be positive"
        );
        anyhow::ensure!(
            self.ingestion.max_sessions_per_document > 0,
            "ingestion.max_sessions_per_document must be positive"
        );
        anyhow::ensure!(
            self.ingestion.staging_max_bytes > 0,
            "ingestion.staging_max_bytes must be positive"
        );
        anyhow::ensure!(
            self.ingestion.finalize_read_batch_size > 0,
            "ingestion.finalize_read_batch_size must be positive"
        );
        anyhow::ensure!(
            self.ingestion.finalize_max_in_memory_blocks >= self.ingestion.finalize_read_batch_size,
            "ingestion.finalize_max_in_memory_blocks must be >= finalize_read_batch_size"
        );
        anyhow::ensure!(self.ingestion.finalize_streaming_required_above_blocks >= self.ingestion.finalize_read_batch_size, "ingestion.finalize_streaming_required_above_blocks must be >= finalize_read_batch_size");
        anyhow::ensure!(self.ingestion.completed_session_result_retention_seconds >= self.ingestion.staging_completed_blocks_retention_seconds, "completed_session_result_retention_seconds must be >= staging_completed_blocks_retention_seconds");
        anyhow::ensure!(
            self.ingestion.failed_session_retention_seconds > 0,
            "ingestion.failed_session_retention_seconds must be positive"
        );
        anyhow::ensure!(
            self.ingestion.finalizing_stale_timeout_seconds > 0,
            "ingestion.finalizing_stale_timeout_seconds must be positive"
        );
        anyhow::ensure!(
            self.ingestion.finalizing_heartbeat_interval_seconds > 0,
            "ingestion.finalizing_heartbeat_interval_seconds must be positive"
        );
        anyhow::ensure!(self.ingestion.finalizing_stale_timeout_seconds >= self.ingestion.finalizing_heartbeat_interval_seconds.saturating_mul(3), "ingestion.finalizing_stale_timeout_seconds must be at least 3x finalizing_heartbeat_interval_seconds");
        anyhow::ensure!(self.ingestion.finalize_mode.as_str() == "BOUNDED_IN_MEMORY", "ingestion.finalize_mode must be BOUNDED_IN_MEMORY in fix4.5.2; TRUE_STREAMING is reserved for a later indexing-pipeline refactor");
        anyhow::ensure!(matches!(self.ingestion.finalize_memory_guard_exceeded_status.as_str(), "RETURN_TO_ACTIVE" | "KEEP_ACTIVE_WITH_LAST_ERROR"), "ingestion.finalize_memory_guard_exceeded_status must be RETURN_TO_ACTIVE or KEEP_ACTIVE_WITH_LAST_ERROR");
        anyhow::ensure!(
            self.ingestion.staging_cleanup_batch_size > 0,
            "ingestion.staging_cleanup_batch_size must be positive"
        );
        anyhow::ensure!(
            self.index_ttl.default_ttl_days == 0
                || self.index_ttl.default_ttl_days >= self.index_ttl.min_ttl_days,
            "index_ttl.default_ttl_days must be 0 or >= min_ttl_days"
        );
        anyhow::ensure!(
            self.index_ttl.default_ttl_days <= self.index_ttl.max_ttl_days,
            "index_ttl.default_ttl_days must be <= max_ttl_days"
        );
        anyhow::ensure!(
            self.index_ttl.min_ttl_days <= self.index_ttl.max_ttl_days,
            "index_ttl.min_ttl_days must be <= max_ttl_days"
        );
        anyhow::ensure!(
            self.index_ttl.default_ttl_days != 0 || self.index_ttl.allow_never_expire,
            "index_ttl.default_ttl_days=0 requires allow_never_expire=true"
        );
        anyhow::ensure!(
            self.index_ttl.never_expire_epoch_seconds > 4_102_444_800,
            "index_ttl.never_expire_epoch_seconds must be a far-future unix epoch"
        );
        anyhow::ensure!(
            self.index_ttl.cleanup_interval_seconds > 0,
            "index_ttl.cleanup_interval_seconds must be positive"
        );
        anyhow::ensure!(
            self.index_ttl.cleanup_batch_size > 0,
            "index_ttl.cleanup_batch_size must be positive"
        );
        anyhow::ensure!(
            self.index_ttl.qdrant_delete_batch_size > 0,
            "index_ttl.qdrant_delete_batch_size must be positive"
        );
        anyhow::ensure!(
            self.index_ttl.qdrant_scroll_batch_size > 0,
            "index_ttl.qdrant_scroll_batch_size must be positive"
        );
        anyhow::ensure!(
            !(self.index_ttl.enabled && self.index_ttl.cleanup_enabled) || self.qdrant.enabled,
            "index_ttl.cleanup_enabled=true requires qdrant.enabled=true"
        );
        anyhow::ensure!(
            self.index_ttl.delete_failed_retry_after_seconds > 0,
            "index_ttl.delete_failed_retry_after_seconds must be positive"
        );
        anyhow::ensure!(
            self.index_ttl.max_delete_attempts > 0,
            "index_ttl.max_delete_attempts must be positive"
        );
        anyhow::ensure!(
            self.index_ttl.delete_retry_initial_delay_seconds > 0,
            "index_ttl.delete_retry_initial_delay_seconds must be positive"
        );
        anyhow::ensure!(
            self.index_ttl.delete_retry_max_delay_seconds
                >= self.index_ttl.delete_retry_initial_delay_seconds,
            "index_ttl.delete_retry_max_delay_seconds must be >= initial delay"
        );
        anyhow::ensure!(self.index_ttl.deleting_stale_timeout_seconds > self.index_ttl.delete_failed_retry_after_seconds, "index_ttl.deleting_stale_timeout_seconds must be greater than delete_failed_retry_after_seconds");
        anyhow::ensure!(
            self.access_zones.max_search_access_zones > 0,
            "access_zones.max_search_access_zones must be positive"
        );
        anyhow::ensure!(
            self.access_zones.max_access_zone_id_length > 0,
            "access_zones.max_access_zone_id_length must be positive"
        );
        anyhow::ensure!(
            self.access_zones.access_zone_id_regex == default_access_zone_id_regex(),
            "access_zones.access_zone_id_regex must remain the supported safe regex in fix4.5.3"
        );
        anyhow::ensure!(
            self.access_zone_registry.cache_ttl_seconds > 0,
            "access_zone_registry.cache_ttl_seconds must be positive"
        );
        anyhow::ensure!(self.access_zone_registry.active_recheck_interval_ms <= 86_400_000, "access_zone_registry.active_recheck_interval_ms must be <= 1 day; 0 means always recheck");
        anyhow::ensure!(!self.access_zone_registry.auto_create_on_search, "access_zone_registry.auto_create_on_search must remain false; Search/RetrieveContext must never create access zones");
        anyhow::ensure!(
            matches!(
                self.access_zone_registry
                    .auto_create_default_status
                    .as_str(),
                "ACTIVE" | "DISABLED"
            ),
            "access_zone_registry.auto_create_default_status must be ACTIVE or DISABLED"
        );
        anyhow::ensure!(
            self.access_zone_codes.code_regex == default_access_zone_code_regex(),
            "access_zone_codes.code_regex must remain ^[0-9]{{4}}$ in fix4.5.4"
        );
        anyhow::ensure!(
            self.access_zone_codes.max_code == 9999,
            "access_zone_codes.max_code must be 9999"
        );
        anyhow::ensure!(
            self.access_zone_codes.never_expire_start == 0,
            "access_zone_codes.never_expire_start must be 0"
        );
        anyhow::ensure!(
            self.access_zone_codes.never_expire_end < self.access_zone_codes.step_start,
            "never-expire range must end before step_start"
        );
        anyhow::ensure!(
            self.access_zone_codes.step_start == 1000,
            "access_zone_codes.step_start must be 1000"
        );
        anyhow::ensure!(
            self.access_zone_codes.step_codes == 500,
            "access_zone_codes.step_codes must be 500"
        );
        anyhow::ensure!(
            self.access_zone_codes.step_months == 6,
            "access_zone_codes.step_months must be 6"
        );
        anyhow::ensure!(
            self.access_zone_codes.special_max_code_start == 9500,
            "special max-retention bucket must start at 9500"
        );
        anyhow::ensure!(
            self.access_zone_codes.special_max_ttl_days == 3650,
            "special max-retention TTL must be 3650 days"
        );
        let qp = &self.search.query_processing;
        let query_max = self.tokenization.query.max_length;
        anyhow::ensure!(
            qp.absolute_max_tokens > query_max,
            "search.query_processing.absolute_max_tokens must exceed tokenization.query.max_length"
        );
        anyhow::ensure!(
            qp.segment_target_tokens > 0,
            "search.query_processing.segment_target_tokens must be positive"
        );
        anyhow::ensure!(
            qp.segment_target_tokens <= qp.segment_max_tokens,
            "search.query_processing.segment_target_tokens must not exceed segment_max_tokens"
        );
        anyhow::ensure!(
            qp.segment_max_tokens <= query_max,
            "search.query_processing.segment_max_tokens must not exceed query max_length"
        );
        anyhow::ensure!(
            qp.segment_overlap_tokens < qp.segment_target_tokens,
            "search.query_processing.segment_overlap_tokens must be smaller than segment_target_tokens"
        );
        anyhow::ensure!(
            qp.max_segments >= 2,
            "search.query_processing.max_segments must be at least 2"
        );
        anyhow::ensure!(
            qp.max_segments <= self.batching.query.max_items,
            "search.query_processing.max_segments must not exceed batching.query.max_items"
        );
        anyhow::ensure!(
            qp.max_parallel_segments > 0 && qp.max_parallel_segments <= qp.max_segments,
            "search.query_processing.max_parallel_segments must be between 1 and max_segments"
        );
        anyhow::ensure!(
            qp.max_parallel_lexical_segments > 0
                && qp.max_parallel_lexical_segments <= qp.max_segments,
            "search.query_processing.max_parallel_lexical_segments must be between 1 and max_segments"
        );
        anyhow::ensure!(
            qp.per_segment_candidate_limit > 0,
            "search.query_processing.per_segment_candidate_limit must be positive"
        );
        anyhow::ensure!(
            qp.global_candidate_limit >= qp.per_segment_candidate_limit,
            "search.query_processing.global_candidate_limit must be >= per_segment_candidate_limit"
        );
        anyhow::ensure!(
            qp.global_candidate_limit <= self.limits.search_candidate_limit_max,
            "search.query_processing.global_candidate_limit exceeds system search candidate limit"
        );
        anyhow::ensure!(
            qp.segment_rrf_k.is_finite() && qp.segment_rrf_k >= 1.0,
            "search.query_processing.segment_rrf_k must be finite and >= 1"
        );
        for (name, value) in [
            ("question_segment_weight", qp.question_segment_weight),
            ("technical_segment_weight", qp.technical_segment_weight),
            ("context_segment_weight", qp.context_segment_weight),
        ] {
            anyhow::ensure!(
                value.is_finite() && value > 0.0,
                "search.query_processing.{name} must be finite and positive"
            );
        }
        anyhow::ensure!(
            qp.long_query_deadline_ms >= self.grpc.deadlines.query_ms,
            "search.query_processing.long_query_deadline_ms must be >= normal query deadline"
        );
        anyhow::ensure!(
            qp.standard.max_tokens > query_max
                && qp.standard.max_tokens < qp.extended.max_tokens
                && qp.extended.max_tokens <= 2_048,
            "query processing tiers must satisfy single < standard < extended <= 2048"
        );
        for (name, tier) in [("standard", &qp.standard), ("extended", &qp.extended)] {
            anyhow::ensure!(
                tier.max_segments >= 2 && tier.max_segments <= 16,
                "search.query_processing.{name}.max_segments must be between 2 and 16"
            );
            anyhow::ensure!(
                tier.max_parallel_segments > 0 && tier.max_parallel_segments <= tier.max_segments,
                "search.query_processing.{name}.max_parallel_segments is invalid"
            );
            anyhow::ensure!(
                tier.max_parallel_lexical_segments > 0
                    && tier.max_parallel_lexical_segments <= tier.max_segments,
                "search.query_processing.{name}.max_parallel_lexical_segments is invalid"
            );
            anyhow::ensure!(
                tier.local_fused_candidate_limit > 0
                    && tier.global_fused_candidate_limit >= tier.local_fused_candidate_limit
                    && tier.global_fused_candidate_limit <= self.limits.search_candidate_limit_max,
                "search.query_processing.{name} candidate limits are invalid"
            );
            anyhow::ensure!(
                tier.deadline_ms >= self.grpc.deadlines.query_ms,
                "search.query_processing.{name}.deadline_ms must be >= normal query deadline"
            );
            anyhow::ensure!(
                tier.admission_weight > 0,
                "search.query_processing.{name}.admission_weight must be positive"
            );
            anyhow::ensure!(
                tier.admission_weight as usize
                    <= self.limits.max_concurrent_retrieve_context,
                "search.query_processing.{name}.admission_weight exceeds retrieval admission capacity"
            );
            anyhow::ensure!(
                tier.max_graph_seeds > 0 && tier.max_graph_seeds <= 12,
                "search.query_processing.{name}.max_graph_seeds must be between 1 and 12"
            );
        }

        anyhow::ensure!(
            self.ingestion.max_blocks_per_document > 0,
            "ingestion.max_blocks_per_document must be positive"
        );
        anyhow::ensure!(
            self.ingestion.max_chunks_per_document > 0,
            "ingestion.max_chunks_per_document must be positive"
        );
        anyhow::ensure!(
            (MIN_INGESTION_DOCUMENT_DEADLINE_MS..=MAX_INGESTION_DOCUMENT_DEADLINE_MS)
                .contains(&self.grpc.deadlines.document_batch_ms),
            "grpc.deadlines.document_batch_ms must be between {MIN_INGESTION_DOCUMENT_DEADLINE_MS} and {MAX_INGESTION_DOCUMENT_DEADLINE_MS} ms"
        );
        anyhow::ensure!(
            self.embedding.document_max_in_flight_chunks >= 1,
            "embedding.document_max_in_flight_chunks must be >= 1"
        );
        anyhow::ensure!(
            matches!(
                self.embedding.document_submit_mode.as_str(),
                "SEQUENTIAL" | "BOUNDED_CONCURRENT"
            ),
            "embedding.document_submit_mode must be SEQUENTIAL or BOUNDED_CONCURRENT"
        );
        anyhow::ensure!(
            self.embedding.partial_embedding_failure_mode == "FAIL_DOCUMENT",
            "embedding.partial_embedding_failure_mode must be FAIL_DOCUMENT in fix4.4"
        );
        for (workload, policy) in [
            ("query", &self.resilience.qdrant_retry.query),
            ("publisher", &self.resilience.qdrant_retry.publisher),
            (
                "reconciliation",
                &self.resilience.qdrant_retry.reconciliation,
            ),
        ] {
            anyhow::ensure!(
                policy.max_attempts >= 1,
                "resilience.qdrant_retry.{workload}.max_attempts must be >= 1"
            );
            anyhow::ensure!(
                policy.max_delay_ms >= policy.base_delay_ms,
                "qdrant {workload} retry max_delay must be >= base_delay"
            );
        }
        anyhow::ensure!(
            self.resilience.inference_retry.query.max_attempts >= 1
                && self.resilience.inference_retry.document.max_attempts >= 1,
            "inference retry max_attempts must be >= 1"
        );
        anyhow::ensure!(
            self.resilience.inference_retry.query.max_delay_ms
                >= self.resilience.inference_retry.query.base_delay_ms
                && self.resilience.inference_retry.document.max_delay_ms
                    >= self.resilience.inference_retry.document.base_delay_ms,
            "inference retry max_delay must be >= base_delay for each workload"
        );
        anyhow::ensure!(
            self.rag_context.max_context_tokens > self.rag_context.reserved_answer_tokens,
            "rag_context.max_context_tokens must be > reserved_answer_tokens"
        );
        anyhow::ensure!(
            self.rag_context.chars_per_token > 0,
            "rag_context.chars_per_token must be positive"
        );
        anyhow::ensure!(
            matches!(self.rag_context.tokenizer.as_str(), "APPROX_CHARS"),
            "rag_context.tokenizer supports only APPROX_CHARS in fix4.4"
        );
        anyhow::ensure!(
            matches!(
                self.rag_context.truncation_strategy.as_str(),
                "DROP_LOWEST_SCORE_CHUNKS" | "DROP_LOWEST_MMR_SCORE_CHUNKS" | "TRUNCATE_LAST_CHUNK"
            ),
            "invalid rag_context.truncation_strategy"
        );
        anyhow::ensure!(
            matches!(
                self.rag_context.huge_chunk_strategy.as_str(),
                "DROP_ONE_HUGE_CHUNK" | "TRUNCATE_ONE_HUGE_CHUNK"
            ),
            "invalid rag_context.huge_chunk_strategy"
        );
        anyhow::ensure!(
            self.limits.source_text_max_bytes <= self.limits.source_text_absolute_max_bytes,
            "limits.source_text_max_bytes must be <= source_text_absolute_max_bytes"
        );
        anyhow::ensure!(
            self.limits.search_candidate_limit_max >= self.limits.search_top_k_max,
            "limits.search_candidate_limit_max must be >= search_top_k_max"
        );
        anyhow::ensure!(
            self.limits.max_concurrent_retrieve_context > 0,
            "limits.max_concurrent_retrieve_context must be positive"
        );
        anyhow::ensure!(
            self.limits.max_concurrent_qdrant_search > 0,
            "limits.max_concurrent_qdrant_search must be positive"
        );
        anyhow::ensure!(
            self.limits.max_concurrent_graph_expansion > 0,
            "limits.max_concurrent_graph_expansion must be positive"
        );
        anyhow::ensure!(
            self.limits.max_concurrent_mmr_fetch > 0,
            "limits.max_concurrent_mmr_fetch must be positive"
        );
        anyhow::ensure!(
            self.limits.backpressure_acquire_timeout_ms > 0,
            "limits.backpressure_acquire_timeout_ms must be positive"
        );
        if !self.limits.allow_dangerous_limit_override {
            anyhow::ensure!(
                self.limits.search_top_k_max <= 200,
                "limits.search_top_k_max > 200 requires dangerous override"
            );
            anyhow::ensure!(
                self.limits.graph_related_contexts_max <= 100,
                "limits.graph_related_contexts_max > 100 requires dangerous override"
            );
        }
        anyhow::ensure!(self.ingestion.single_request_max_bytes == self.limits.source_text_max_bytes, "limits.source_text_max_bytes is a deprecated alias and must equal ingestion.single_request_max_bytes");
        anyhow::ensure!(
            self.rag_context.tokenizer_safety_margin_percent <= 80,
            "rag_context.tokenizer_safety_margin_percent must be <= 80"
        );
        anyhow::ensure!(matches!(self.chunking.source_chunk_storage_mode.as_str(), "FULL_TEXT" | "METADATA_ONLY" | "DISABLED"), "chunking.source_chunk_storage_mode must be FULL_TEXT, METADATA_ONLY, or DISABLED; COMPRESSED_TEXT is reserved");
        anyhow::ensure!(self.embedding_cache.reindex_behavior.as_str() == "CREATE_NEW_POINT_ID", "embedding_cache.reindex_behavior must be CREATE_NEW_POINT_ID in fix4.5; REUSE_POINT_ID_WITH_INVALIDATION is reserved until cache invalidation by document is implemented");

        for (name, policy) in [
            (
                "qdrant_scroll_page_size",
                &self.adaptive.policies.qdrant_scroll_page_size,
            ),
            (
                "qdrant_scroll_max_concurrency",
                &self.adaptive.policies.qdrant_scroll_max_concurrency,
            ),
            (
                "publisher_batch_size",
                &self.adaptive.policies.publisher_batch_size,
            ),
            (
                "outbox_poll_interval_ms",
                &self.adaptive.policies.outbox_poll_interval_ms,
            ),
            (
                "embedding_batch_size",
                &self.adaptive.policies.embedding_batch_size,
            ),
            (
                "qdrant_timeout_ms",
                &self.adaptive.policies.qdrant_timeout_ms,
            ),
            (
                "max_concurrent_search",
                &self.adaptive.policies.max_concurrent_search,
            ),
            (
                "max_concurrent_indexing",
                &self.adaptive.policies.max_concurrent_indexing,
            ),
        ] {
            anyhow::ensure!(
                policy.min <= policy.max,
                "adaptive policy {name} has min > max"
            );
            anyhow::ensure!(
                policy.step > 0,
                "adaptive policy {name} step must be positive"
            );
        }
        Ok(())
    }
}
fn active_profile() -> String {
    let raw = env::var("ASTRAVECTOR_PROFILE")
        .or_else(|_| env::var("ASTRAVECTOR_ENV"))
        .unwrap_or_else(|_| "dev".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "development" | "local" => "dev".to_string(),
        "testing" | "tests" => "test".to_string(),
        "production" => "prod".to_string(),
        "" => "dev".to_string(),
        other => other.to_string(),
    }
}

fn profile_config_path(base_path: &str, profile: &str) -> Option<std::path::PathBuf> {
    if let Ok(explicit) = env::var("ASTRAVECTOR_PROFILE_CONFIG") {
        return Some(std::path::PathBuf::from(explicit));
    }

    let path = Path::new(base_path);
    let stem = path.file_stem()?.to_string_lossy();
    if stem != "application" {
        // Explicit non-default files such as smoke-tests/v004/config/application-smoke.yaml
        // are treated as complete configs unless ASTRAVECTOR_PROFILE_CONFIG is provided.
        return None;
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    Some(dir.join(format!("application-{profile}.yaml")))
}

fn read_yaml_value(path: &str) -> Result<Value> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read config {path}"))?;
    serde_yaml::from_str(&expand_env(&raw)).with_context(|| format!("parse config {path}"))
}

fn merge_yaml(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Mapping(base_map), Value::Mapping(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => merge_yaml(base_value, overlay_value),
                    None => {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value;
        }
    }
}

fn apply_query_processing_compatibility(config: &mut Value) -> Result<()> {
    let Some(query_processing) = config
        .get_mut("search")
        .and_then(|search| search.get_mut("query_processing"))
        .and_then(Value::as_mapping_mut)
    else {
        return Ok(());
    };
    let legacy_mappings = [
        ("max_segments", &["max_segments"][..]),
        ("max_parallel_segments", &["max_parallel_segments"]),
        (
            "max_parallel_lexical_segments",
            &["max_parallel_lexical_segments"],
        ),
        (
            "per_segment_candidate_limit",
            &[
                "dense_candidate_limit",
                "sparse_candidate_limit",
                "local_fused_candidate_limit",
            ],
        ),
        ("global_candidate_limit", &["global_fused_candidate_limit"]),
        ("long_query_deadline_ms", &["deadline_ms"]),
    ];
    if !legacy_mappings
        .iter()
        .any(|(legacy, _)| query_processing.contains_key(Value::String((*legacy).to_string())))
    {
        return Ok(());
    }

    let standard_key = Value::String("standard".into());
    let explicit_standard_keys = query_processing
        .get(&standard_key)
        .and_then(Value::as_mapping)
        .map(|mapping| {
            mapping
                .keys()
                .cloned()
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    if !query_processing.contains_key(&standard_key) {
        query_processing.insert(
            standard_key.clone(),
            serde_yaml::to_value(QueryProcessingTierConfig::standard())?,
        );
    }
    let legacy_values = legacy_mappings
        .iter()
        .filter_map(|(legacy, targets)| {
            query_processing
                .get(Value::String((*legacy).to_string()))
                .cloned()
                .map(|value| (*legacy, *targets, value))
        })
        .collect::<Vec<_>>();
    let standard = query_processing
        .get_mut(&standard_key)
        .and_then(Value::as_mapping_mut)
        .context("search.query_processing.standard must be a mapping")?;
    for (legacy, targets, value) in legacy_values {
        let mut applied = false;
        for target in targets {
            let target_key = Value::String((*target).to_string());
            if !explicit_standard_keys.contains(&target_key) {
                standard.insert(target_key, value.clone());
                applied = true;
            }
        }
        if applied {
            tracing::warn!(
                legacy_key = legacy,
                "LEGACY_QUERY_PROCESSING_KEY_DEPRECATED"
            );
        }
    }
    Ok(())
}

fn expand_env(input: &str) -> String {
    let mut out = input.to_owned();
    for (new_key, legacy_key) in [
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_MAX_SEGMENTS",
            "ASTRAVECTOR_LONG_QUERY_MAX_SEGMENTS",
        ),
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_PARALLEL_SEGMENTS",
            "ASTRAVECTOR_LONG_QUERY_MAX_PARALLEL_SEGMENTS",
        ),
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_PARALLEL_FTS",
            "ASTRAVECTOR_LONG_QUERY_MAX_PARALLEL_FTS_SEGMENTS",
        ),
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_DENSE_LIMIT",
            "ASTRAVECTOR_LONG_QUERY_CANDIDATE_LIMIT",
        ),
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_SPARSE_LIMIT",
            "ASTRAVECTOR_LONG_QUERY_CANDIDATE_LIMIT",
        ),
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_LOCAL_FUSED_LIMIT",
            "ASTRAVECTOR_LONG_QUERY_CANDIDATE_LIMIT",
        ),
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_GLOBAL_FUSED_LIMIT",
            "ASTRAVECTOR_LONG_QUERY_GLOBAL_CANDIDATE_LIMIT",
        ),
        (
            "ASTRAVECTOR_LONG_QUERY_STANDARD_DEADLINE_MS",
            "ASTRAVECTOR_LONG_QUERY_DEADLINE_MS",
        ),
    ] {
        if env::var(new_key).is_err() {
            if let Ok(value) = env::var(legacy_key) {
                let marker = format!("${{{new_key}:-");
                while let Some(start) = out.find(&marker) {
                    let Some(relative_end) = out[start..].find('}') else {
                        break;
                    };
                    out.replace_range(start..start + relative_end + 1, &value);
                }
                tracing::warn!(
                    legacy_key,
                    new_key,
                    "LEGACY_QUERY_PROCESSING_ENV_DEPRECATED"
                );
            }
        }
    }
    for (k, v) in env::vars() {
        out = out.replace(&format!("${{{k}}}"), &v);
        let marker = format!("${{{k}:-");
        while let Some(start) = out.find(&marker) {
            if let Some(rel) = out[start..].find('}') {
                out.replace_range(start..start + rel + 1, &v)
            } else {
                break;
            }
        }
    }
    while let Some(start) = out.find("${") {
        let Some(rel) = out[start..].find('}') else {
            break;
        };
        let replacement = out[start + 2..start + rel]
            .split_once(":-")
            .map(|(_, d)| d.to_owned())
            .unwrap_or_default();
        out.replace_range(start..start + rel + 1, &replacement)
    }
    out
}

#[derive(Debug, Clone, Deserialize)]
pub struct QdrantConfig {
    pub enabled: bool,
    pub url: String,
    pub api_key: String,
    pub collection: String,
    pub timeout_ms: u64,
    pub auto_create_collection: bool,
    pub validate_collection_schema: bool,
    pub ensure_payload_indexes: bool,
    pub dense_vector_name: String,
    pub sparse_vector_name: String,
    pub scroll_page_size: u64,
    pub scroll_max_pages: u64,
    pub scroll_max_points: u64,
    pub scroll_timeout_secs: u64,
    pub scroll_max_concurrency: usize,
    pub publisher: QdrantPublisherConfig,
}
#[derive(Debug, Clone, Deserialize)]
pub struct QdrantPublisherConfig {
    pub enabled: bool,
    pub batch_size: i64,
    pub poll_interval_ms: u64,
    pub max_attempts: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    pub default_mode: String,
    pub candidate_limit: u32,
    pub parent_limit: u32,
    pub rrf_k: f32,
    #[serde(default = "default_hydration_rejection_reserve")]
    pub hydration_rejection_reserve: u32,
    #[serde(default = "default_hydration_rejection_reserve_max")]
    pub hydration_rejection_reserve_max: u32,
    #[serde(default)]
    pub hydration_failpoints: HydrationFailpointConfig,
    #[serde(default)]
    pub query_processing: QueryProcessingConfig,
    #[serde(default = "default_hybrid_fusion_method")]
    pub hybrid_fusion_method: String,
    #[serde(default = "default_hybrid_dense_weight")]
    pub hybrid_dense_weight: f32,
    #[serde(default = "default_hybrid_sparse_weight")]
    pub hybrid_sparse_weight: f32,
    #[serde(default)]
    pub lexical: LexicalSearchConfig,
    #[serde(default)]
    pub fusion: FusionSearchConfig,
    #[serde(default)]
    pub ranking_trace: RankingTraceConfig,
    #[serde(default)]
    pub no_answer: NoAnswerConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HydrationFailpointConfig {
    #[serde(default)]
    pub non_production_enabled: bool,
    #[serde(default)]
    pub plan_path: String,
    #[serde(default)]
    pub run_id: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryProcessingTierConfig {
    pub max_tokens: usize,
    pub max_segments: usize,
    pub dense_candidate_limit: u32,
    pub sparse_candidate_limit: u32,
    pub lexical_candidate_limit: u32,
    pub local_fused_candidate_limit: u32,
    pub global_fused_candidate_limit: u32,
    pub max_parallel_segments: usize,
    pub max_parallel_lexical_segments: usize,
    pub deadline_ms: u64,
    pub max_graph_seeds: usize,
    pub admission_weight: u32,
}

impl QueryProcessingTierConfig {
    fn standard() -> Self {
        Self {
            max_tokens: 1_024,
            max_segments: 7,
            dense_candidate_limit: 18,
            sparse_candidate_limit: 18,
            lexical_candidate_limit: 12,
            local_fused_candidate_limit: 18,
            global_fused_candidate_limit: 100,
            max_parallel_segments: 3,
            max_parallel_lexical_segments: 2,
            deadline_ms: 3_000,
            max_graph_seeds: 8,
            admission_weight: 3,
        }
    }

    fn extended() -> Self {
        Self {
            max_tokens: 2_048,
            max_segments: 14,
            dense_candidate_limit: 10,
            sparse_candidate_limit: 10,
            lexical_candidate_limit: 8,
            local_fused_candidate_limit: 10,
            global_fused_candidate_limit: 140,
            max_parallel_segments: 3,
            max_parallel_lexical_segments: 2,
            deadline_ms: 6_000,
            max_graph_seeds: 10,
            admission_weight: 6,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryProcessingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub extended_enabled: bool,
    #[serde(default = "default_query_profile_version")]
    pub profile_version: String,
    #[serde(default = "default_long_query_absolute_max_tokens")]
    pub absolute_max_tokens: usize,
    #[serde(default = "default_long_query_absolute_max_bytes")]
    pub absolute_max_bytes: usize,
    #[serde(default = "default_query_segment_target_tokens")]
    pub segment_target_tokens: usize,
    #[serde(default = "default_query_segment_max_tokens")]
    pub segment_max_tokens: usize,
    #[serde(default = "default_query_segment_overlap_tokens")]
    pub segment_overlap_tokens: usize,
    // v008 compatibility aliases. Runtime limits resolve from standard/extended.
    #[serde(default = "default_query_max_segments")]
    pub max_segments: usize,
    #[serde(default = "default_query_max_parallel_segments")]
    pub max_parallel_segments: usize,
    #[serde(default = "default_query_max_parallel_lexical_segments")]
    pub max_parallel_lexical_segments: usize,
    #[serde(default = "default_query_per_segment_candidate_limit")]
    pub per_segment_candidate_limit: u32,
    #[serde(default = "default_query_global_candidate_limit")]
    pub global_candidate_limit: u32,
    #[serde(default = "default_query_segment_rrf_k")]
    pub segment_rrf_k: f32,
    #[serde(default = "default_question_segment_weight")]
    pub question_segment_weight: f32,
    #[serde(default = "default_technical_segment_weight")]
    pub technical_segment_weight: f32,
    #[serde(default = "default_context_segment_weight")]
    pub context_segment_weight: f32,
    #[serde(default = "default_long_query_deadline_ms")]
    pub long_query_deadline_ms: u64,
    #[serde(default = "default_single_query_deadline_ms")]
    pub single_deadline_ms: u64,
    #[serde(default = "default_single_graph_seeds")]
    pub single_graph_seeds: usize,
    #[serde(default = "default_single_admission_weight")]
    pub single_admission_weight: u32,
    #[serde(default = "QueryProcessingTierConfig::standard")]
    pub standard: QueryProcessingTierConfig,
    #[serde(default = "QueryProcessingTierConfig::extended")]
    pub extended: QueryProcessingTierConfig,
}

impl Default for QueryProcessingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extended_enabled: false,
            profile_version: default_query_profile_version(),
            absolute_max_tokens: default_long_query_absolute_max_tokens(),
            absolute_max_bytes: default_long_query_absolute_max_bytes(),
            segment_target_tokens: default_query_segment_target_tokens(),
            segment_max_tokens: default_query_segment_max_tokens(),
            segment_overlap_tokens: default_query_segment_overlap_tokens(),
            max_segments: default_query_max_segments(),
            max_parallel_segments: default_query_max_parallel_segments(),
            max_parallel_lexical_segments: default_query_max_parallel_lexical_segments(),
            per_segment_candidate_limit: default_query_per_segment_candidate_limit(),
            global_candidate_limit: default_query_global_candidate_limit(),
            segment_rrf_k: default_query_segment_rrf_k(),
            question_segment_weight: default_question_segment_weight(),
            technical_segment_weight: default_technical_segment_weight(),
            context_segment_weight: default_context_segment_weight(),
            long_query_deadline_ms: default_long_query_deadline_ms(),
            single_deadline_ms: default_single_query_deadline_ms(),
            single_graph_seeds: default_single_graph_seeds(),
            single_admission_weight: default_single_admission_weight(),
            standard: QueryProcessingTierConfig::standard(),
            extended: QueryProcessingTierConfig::extended(),
        }
    }
}

fn default_query_profile_version() -> String {
    "tiered-query-v1".into()
}
fn default_long_query_absolute_max_tokens() -> usize {
    2_048
}
fn default_long_query_absolute_max_bytes() -> usize {
    65_536
}
fn default_query_segment_target_tokens() -> usize {
    180
}
fn default_query_segment_max_tokens() -> usize {
    220
}
fn default_query_segment_overlap_tokens() -> usize {
    24
}
fn default_query_max_segments() -> usize {
    7
}
fn default_query_max_parallel_segments() -> usize {
    3
}
fn default_query_max_parallel_lexical_segments() -> usize {
    2
}
fn default_query_per_segment_candidate_limit() -> u32 {
    18
}
fn default_query_global_candidate_limit() -> u32 {
    100
}
fn default_query_segment_rrf_k() -> f32 {
    60.0
}
fn default_question_segment_weight() -> f32 {
    1.0
}
fn default_technical_segment_weight() -> f32 {
    1.0
}
fn default_context_segment_weight() -> f32 {
    0.5
}
fn default_long_query_deadline_ms() -> u64 {
    3_000
}
fn default_single_query_deadline_ms() -> u64 {
    1_000
}
fn default_single_graph_seeds() -> usize {
    5
}
fn default_single_admission_weight() -> u32 {
    1
}
#[derive(Debug, Clone, Deserialize)]
pub struct FusionSearchConfig {
    #[serde(default = "default_min_strong_lexical_candidates")]
    pub min_strong_lexical_candidates: usize,
}
impl Default for FusionSearchConfig {
    fn default() -> Self {
        Self {
            min_strong_lexical_candidates: default_min_strong_lexical_candidates(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct RankingTraceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ranking_trace_max_candidates")]
    pub max_candidates: usize,
    #[serde(default = "default_ranking_trace_max_stages")]
    pub max_stages_per_candidate: usize,
    #[serde(default)]
    pub include_text_preview: bool,
}
impl Default for RankingTraceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_candidates: default_ranking_trace_max_candidates(),
            max_stages_per_candidate: default_ranking_trace_max_stages(),
            include_text_preview: false,
        }
    }
}
fn default_min_strong_lexical_candidates() -> usize {
    1
}
fn default_ranking_trace_max_candidates() -> usize {
    100
}
fn default_ranking_trace_max_stages() -> usize {
    32
}
#[derive(Debug, Clone, Deserialize)]
pub struct LexicalSearchConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_lexical_backend")]
    pub backend: String,
    #[serde(default = "default_lexical_candidate_limit")]
    pub candidate_limit: u32,
    #[serde(default = "default_lexical_max_candidate_limit")]
    pub max_candidate_limit: u32,
    #[serde(default = "default_lexical_min_remaining_budget_ms")]
    pub min_remaining_budget_ms: u64,
    #[serde(default = "default_lexical_statement_timeout_ms")]
    pub statement_timeout_ms: u64,
    #[serde(default = "default_lexical_response_reserve_ms")]
    pub response_reserve_ms: u64,
    #[serde(default = "default_true")]
    pub exact_technical_enabled: bool,
    #[serde(default)]
    pub trigram_enabled: bool,
    #[serde(default = "default_lexical_trigram_min_similarity")]
    pub trigram_min_similarity: f32,
    #[serde(default = "default_lexical_sparse_candidate_floor")]
    pub run_when_sparse_candidates_below: usize,
    #[serde(default = "default_lexical_sparse_score_floor")]
    pub run_when_sparse_top_score_below: f32,
    #[serde(default = "default_lexical_weight")]
    pub lexical_weight: f32,
}
impl Default for LexicalSearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: default_lexical_backend(),
            candidate_limit: default_lexical_candidate_limit(),
            max_candidate_limit: default_lexical_max_candidate_limit(),
            min_remaining_budget_ms: default_lexical_min_remaining_budget_ms(),
            statement_timeout_ms: default_lexical_statement_timeout_ms(),
            response_reserve_ms: default_lexical_response_reserve_ms(),
            exact_technical_enabled: true,
            trigram_enabled: false,
            trigram_min_similarity: default_lexical_trigram_min_similarity(),
            run_when_sparse_candidates_below: default_lexical_sparse_candidate_floor(),
            run_when_sparse_top_score_below: default_lexical_sparse_score_floor(),
            lexical_weight: default_lexical_weight(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct NoAnswerConfig {
    #[serde(default = "default_no_answer_enabled")]
    pub enabled: bool,
    #[serde(default = "default_no_answer_min_dense_score")]
    pub min_dense_score: f32,
    #[serde(default = "default_no_answer_min_sparse_score")]
    pub min_sparse_score: f32,
    #[serde(default = "default_no_answer_min_hybrid_score")]
    pub min_hybrid_score: f32,
    #[serde(default = "default_no_answer_sparse_only_min_matched_terms")]
    pub sparse_only_min_matched_terms: usize,
    #[serde(default = "default_no_answer_sparse_only_require_technical_token")]
    pub sparse_only_require_technical_token: bool,
    #[serde(default = "default_no_answer_exact_technical_boost")]
    pub exact_technical_boost: f32,
    #[serde(default = "default_no_answer_hard_negative_strict")]
    pub hard_negative_strict: bool,
    #[serde(default = "default_no_answer_debug_candidates")]
    pub debug_candidates: bool,
}
impl Default for NoAnswerConfig {
    fn default() -> Self {
        Self {
            enabled: default_no_answer_enabled(),
            min_dense_score: default_no_answer_min_dense_score(),
            min_sparse_score: default_no_answer_min_sparse_score(),
            min_hybrid_score: default_no_answer_min_hybrid_score(),
            sparse_only_min_matched_terms: default_no_answer_sparse_only_min_matched_terms(),
            sparse_only_require_technical_token:
                default_no_answer_sparse_only_require_technical_token(),
            exact_technical_boost: default_no_answer_exact_technical_boost(),
            hard_negative_strict: default_no_answer_hard_negative_strict(),
            debug_candidates: default_no_answer_debug_candidates(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct ExplainConfig {
    pub top_sparse_tokens: u32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct LifecycleConfig {
    pub enabled: bool,
    pub scan_interval_seconds: u64,
    pub batch_size: i64,
    pub soft_delete_grace_days: i64,
    pub min_ttl_days: u32,
    pub max_ttl_days: u32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct EnrichmentConfig {
    pub enabled: bool,
    pub max_representation_length: usize,
    pub summary_enabled: bool,
    pub synthetic_questions_enabled: bool,
    pub synthetic_questions_max_count: usize,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RelevanceConfig {
    pub enabled: bool,
    pub min_candidate_score: f32,
    pub min_reuse_score: f32,
}

#[cfg(test)]
mod query_processing_compatibility_tests {
    use super::*;

    #[test]
    fn grpc_query_deadline_keeps_default_and_supports_runtime_override() {
        let application = include_str!("../../config/application.yaml");
        assert!(application.contains("query_ms: ${ASTRAVECTOR_GRPC_QUERY_DEADLINE_MS:-1000}"));
    }

    #[test]
    fn document_embedding_deadline_is_bounded_and_runtime_overridable() {
        let application = include_str!("../../config/application.yaml");
        assert!(application
            .contains("document_batch_ms: ${ASTRAVECTOR_INGESTION_DOCUMENT_DEADLINE_MS:-60000}"));
        assert!(
            (MIN_INGESTION_DOCUMENT_DEADLINE_MS..=MAX_INGESTION_DOCUMENT_DEADLINE_MS)
                .contains(&60_000)
        );
        assert!(
            !(MIN_INGESTION_DOCUMENT_DEADLINE_MS..=MAX_INGESTION_DOCUMENT_DEADLINE_MS).contains(&0)
        );
        assert!(
            !(MIN_INGESTION_DOCUMENT_DEADLINE_MS..=MAX_INGESTION_DOCUMENT_DEADLINE_MS)
                .contains(&600_001)
        );
    }

    #[test]
    fn legacy_keys_populate_standard_when_new_tier_is_absent() {
        let mut value: Value = serde_yaml::from_str(
            "search:\n  query_processing:\n    max_segments: 5\n    per_segment_candidate_limit: 9\n    global_candidate_limit: 70\n    long_query_deadline_ms: 2500\n",
        )
        .unwrap();
        apply_query_processing_compatibility(&mut value).unwrap();
        let standard = value["search"]["query_processing"]["standard"]
            .as_mapping()
            .unwrap();
        assert_eq!(standard["max_segments"].as_u64(), Some(5));
        assert_eq!(standard["dense_candidate_limit"].as_u64(), Some(9));
        assert_eq!(standard["sparse_candidate_limit"].as_u64(), Some(9));
        assert_eq!(standard["local_fused_candidate_limit"].as_u64(), Some(9));
        assert_eq!(standard["global_fused_candidate_limit"].as_u64(), Some(70));
        assert_eq!(standard["deadline_ms"].as_u64(), Some(2500));
    }

    #[test]
    fn explicit_new_keys_take_precedence_over_legacy_keys() {
        let mut value: Value = serde_yaml::from_str(
            "search:\n  query_processing:\n    max_segments: 5\n    standard:\n      max_segments: 7\n      dense_candidate_limit: 18\n      sparse_candidate_limit: 18\n      lexical_candidate_limit: 12\n      local_fused_candidate_limit: 18\n      global_fused_candidate_limit: 100\n      max_parallel_segments: 3\n      max_parallel_lexical_segments: 2\n      deadline_ms: 3000\n      max_tokens: 1024\n      max_graph_seeds: 8\n      admission_weight: 3\n",
        )
        .unwrap();
        apply_query_processing_compatibility(&mut value).unwrap();
        assert_eq!(
            value["search"]["query_processing"]["standard"]["max_segments"].as_u64(),
            Some(7)
        );
    }
}
