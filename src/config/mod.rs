use anyhow::{Context, Result};
use serde::Deserialize;
use std::{env, path::Path};

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
    pub lifecycle: LifecycleConfig,
    pub enrichment: EnrichmentConfig,
    pub relevance: RelevanceConfig,
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
        let path =
            env::var("ASTRAVECTOR_CONFIG").unwrap_or_else(|_| "config/application.yaml".into());
        let raw = std::fs::read_to_string(&path).with_context(|| format!("read config {path}"))?;
        serde_yaml::from_str(&expand_env(&raw)).context("parse AstraVector configuration")
    }
    pub fn validate(&self) -> Result<()> {
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
        Ok(())
    }
}
fn expand_env(input: &str) -> String {
    let mut out = input.to_owned();
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
