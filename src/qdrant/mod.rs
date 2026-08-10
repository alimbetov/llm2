use crate::{
    adaptive::AdaptiveRuntime,
    config::RetryPolicyConfig,
    error::AstraError,
    reliability::{OperationBudget, WorkloadKind},
    smoke_failpoints,
};
use metrics::{counter, gauge, histogram};
use rand::Rng;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantPoint {
    pub id: Uuid,
    pub dense: Option<Vec<f32>>,
    pub sparse_indices: Option<Vec<u32>>,
    pub sparse_values: Option<Vec<f32>>,
    pub payload: Value,
}

fn qdrant_status_error(operation_name: &str, status: StatusCode) -> AstraError {
    let message = format!("qdrant {operation_name} status={status}");
    match status.as_u16() {
        400 | 422 => AstraError::InvalidArgument(message),
        401 => AstraError::Unauthenticated(message),
        403 => AstraError::PermissionDenied(message),
        404 => AstraError::NotFound(message),
        409 => AstraError::FailedPrecondition(message),
        429 => AstraError::ResourceExhausted(message),
        500..=599 => AstraError::Unavailable(message),
        _ => AstraError::Unavailable(message),
    }
}

fn is_delete_operation(operation_name: &str) -> bool {
    matches!(
        operation_name,
        "delete"
            | "delete_point"
            | "delete_points"
            | "delete_points_batch"
            | "delete_document_points"
    )
}

#[derive(Clone)]
pub struct QdrantClient {
    http: Client,
    base_url: String,
    api_key: Option<String>,
    collection: String,
    scroll_page_size: u64,
    scroll_max_pages: u64,
    scroll_max_points: u64,
    scroll_timeout_secs: u64,
    scroll_semaphore: Arc<Semaphore>,
    search_semaphore: Arc<Semaphore>,
    search_max_concurrency: usize,
    search_acquire_timeout_ms: u64,
    adaptive: Option<Arc<AdaptiveRuntime>>,
    retry_policy: RetryPolicyConfig,
}

#[derive(Debug, Clone)]
pub enum QdrantScrollStatus {
    Completed,
    Timeout,
    LimitExceeded,
    LoopDetected,
    QdrantError,
}

#[derive(Debug, Clone)]
pub struct QdrantScrollPointIdsResult {
    pub point_ids: HashSet<Uuid>,
    pub pages_read: u64,
    pub points_read: u64,
    pub completed: bool,
    pub status: QdrantScrollStatus,
}

#[derive(Debug, Clone)]
pub struct QdrantScrollPointsResult {
    pub payloads: HashMap<Uuid, Value>,
    pub pages_read: u64,
    pub points_read: u64,
    pub completed: bool,
    pub status: QdrantScrollStatus,
}

#[derive(Debug, Clone)]
pub struct QdrantSearchHit {
    pub id: Uuid,
    pub score: f32,
    pub dense_score: f32,
    pub sparse_score: f32,
    pub fusion_score: f32,
    pub dense_rank: Option<u32>,
    pub sparse_rank: Option<u32>,
    pub payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct QdrantVersionFilters {
    pub model_version: Option<String>,
    pub tokenizer_version: Option<String>,
    pub dense_version: Option<String>,
    pub sparse_version: Option<String>,
    pub chunking_version: Option<String>,
    pub payload_filters: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantCollectionCompatibilitySummary {
    pub expected_dense_dimension: usize,
    pub actual_dense_dimension: usize,
    pub dense_distance: String,
    pub sparse_vector_present: bool,
    pub required_payload_indexes: usize,
    pub missing_payload_indexes: Vec<String>,
    pub mismatched_payload_indexes: Vec<String>,
    pub verdict: String,
}

const RETRIEVAL_PAYLOAD_INDEXES: &[(&str, &str)] = &[
    ("access_zone_id", "keyword"),
    ("lifecycle_status", "keyword"),
    ("chunk_granularity", "keyword"),
    ("document_id", "keyword"),
    ("document_version", "integer"),
    ("access_level", "integer"),
    ("expires_at_epoch", "integer"),
    ("quarantined", "bool"),
    ("model_version", "keyword"),
    ("tokenizer_version", "keyword"),
    ("dense_version", "keyword"),
    ("sparse_version", "keyword"),
    ("chunking_profile_version", "keyword"),
    ("quality_run_id", "keyword"),
    ("binding_id", "keyword"),
    ("qdrant_point_id", "keyword"),
];

pub fn qdrant_collection_compatibility_from_info(
    body: &Value,
    expected_dimension: usize,
) -> QdrantCollectionCompatibilitySummary {
    let actual_dense_dimension = body
        .pointer("/result/config/params/vectors/dense/size")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let dense_distance = body
        .pointer("/result/config/params/vectors/dense/distance")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let sparse_vector_present = body
        .pointer("/result/config/params/sparse_vectors/sparse")
        .is_some();
    let payload_schema = body
        .pointer("/result/payload_schema")
        .and_then(Value::as_object);
    let mut missing_payload_indexes = Vec::new();
    let mut mismatched_payload_indexes = Vec::new();
    for (field, expected_type) in RETRIEVAL_PAYLOAD_INDEXES {
        let Some(schema) = payload_schema.and_then(|schema| schema.get(*field)) else {
            missing_payload_indexes.push((*field).to_string());
            continue;
        };
        let actual_type = schema
            .get("data_type")
            .or_else(|| schema.get("type"))
            .and_then(Value::as_str)
            .or_else(|| schema.as_str())
            .unwrap_or("");
        if !qdrant_payload_schema_type_matches(expected_type, actual_type) {
            mismatched_payload_indexes.push(format!(
                "{field}: expected={expected_type}, actual={actual_type}"
            ));
        }
    }
    let compatible = actual_dense_dimension == expected_dimension
        && dense_distance.eq_ignore_ascii_case("Cosine")
        && sparse_vector_present
        && missing_payload_indexes.is_empty()
        && mismatched_payload_indexes.is_empty();
    QdrantCollectionCompatibilitySummary {
        expected_dense_dimension: expected_dimension,
        actual_dense_dimension,
        dense_distance,
        sparse_vector_present,
        required_payload_indexes: RETRIEVAL_PAYLOAD_INDEXES.len(),
        missing_payload_indexes,
        mismatched_payload_indexes,
        verdict: if compatible {
            "QDRANT_COLLECTION_COMPATIBLE".to_string()
        } else {
            "QDRANT_COLLECTION_SCHEMA_MISMATCH".to_string()
        },
    }
}

fn qdrant_payload_schema_type_matches(expected: &str, actual: &str) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    let actual = actual.trim().to_ascii_lowercase();
    expected == actual || (expected == "bool" && actual == "boolean")
}

fn qdrant_payload_indexes_enabled() -> bool {
    env::var("ASTRAVECTOR_QDRANT_ENSURE_PAYLOAD_INDEXES")
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

fn retry_delay_ms(policy: &RetryPolicyConfig, attempt: u32) -> u64 {
    let exp = 1u64
        .checked_shl(attempt.saturating_sub(1))
        .unwrap_or(u64::MAX);
    let base = policy
        .base_delay_ms
        .saturating_mul(exp)
        .min(policy.max_delay_ms);
    if policy.jitter_enabled {
        let jitter_max = policy.base_delay_ms.clamp(1, 1_000);
        base.saturating_add(rand::thread_rng().gen_range(0..=jitter_max))
    } else {
        base
    }
}

fn retryable_qdrant_status(
    policy: &RetryPolicyConfig,
    workload: WorkloadKind,
    status: StatusCode,
) -> bool {
    if workload == WorkloadKind::Query && status == StatusCode::TOO_MANY_REQUESTS {
        return false;
    }
    policy.retry_on_statuses.contains(&status.as_u16())
}

impl QdrantClient {
    pub fn new(
        base_url: String,
        api_key: Option<String>,
        collection: String,
        timeout_ms: u64,
        scroll_page_size: u64,
        scroll_max_pages: u64,
        scroll_max_points: u64,
        scroll_timeout_secs: u64,
        scroll_max_concurrency: usize,
        search_max_concurrency: usize,
        search_acquire_timeout_ms: u64,
        adaptive: Option<Arc<AdaptiveRuntime>>,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, AstraError> {
        let http = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| AstraError::Internal(format!("qdrant client: {e}")))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').into(),
            api_key,
            collection,
            scroll_page_size: scroll_page_size.max(1),
            scroll_max_pages: scroll_max_pages.max(1),
            scroll_max_points: scroll_max_points.max(1),
            scroll_timeout_secs: scroll_timeout_secs.max(1),
            scroll_semaphore: Arc::new(Semaphore::new(scroll_max_concurrency.max(1))),
            search_semaphore: Arc::new(Semaphore::new(search_max_concurrency.max(1))),
            search_max_concurrency: search_max_concurrency.max(1),
            search_acquire_timeout_ms: search_acquire_timeout_ms.max(1),
            adaptive,
            retry_policy,
        })
    }
    fn effective_scroll_page_size(&self) -> u64 {
        self.adaptive
            .as_ref()
            .map(|a| a.get_u64("qdrant.scroll_page_size", self.scroll_page_size))
            .unwrap_or(self.scroll_page_size)
            .max(1)
    }

    fn request(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) if !k.is_empty() => rb.header("api-key", k),
            _ => rb,
        }
    }

    async fn send_with_retry<F>(
        &self,
        operation_name: &'static str,
        request_factory: F,
    ) -> Result<reqwest::Response, AstraError>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        self.send_with_retry_budget(
            operation_name,
            WorkloadKind::DocumentPublisher,
            None,
            request_factory,
        )
        .await
    }

    async fn send_with_retry_budget<F>(
        &self,
        operation_name: &'static str,
        workload: WorkloadKind,
        budget: Option<&OperationBudget>,
        request_factory: F,
    ) -> Result<reqwest::Response, AstraError>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        let workload_label = match workload {
            WorkloadKind::Query => "query",
            WorkloadKind::DocumentPublisher => "publisher",
            WorkloadKind::Reconciliation => "reconciliation",
        };
        let max_attempts = if self.retry_policy.enabled {
            self.retry_policy.max_attempts.max(1)
        } else {
            1
        };
        let mut attempt = 1u32;
        loop {
            if let Some(budget) = budget {
                if budget.cancellation.is_cancelled() {
                    counter!("astravector_retry_skipped_total", "component" => "qdrant", "workload" => workload_label, "reason" => "cancelled").increment(1);
                    return Err(AstraError::Cancelled("qdrant operation cancelled".into()));
                }
                if budget.is_expired() {
                    counter!("astravector_retry_skipped_total", "component" => "qdrant", "workload" => workload_label, "reason" => "insufficient_budget").increment(1);
                    return Err(AstraError::DeadlineExceeded(
                        "caller deadline expired before qdrant request".into(),
                    ));
                }
            }
            let started = Instant::now();
            let request = request_factory();
            let request = match budget {
                Some(operation_budget) => request.timeout(operation_budget.remaining()),
                None => request,
            };
            let result = request.send().await;
            match result {
                Ok(response)
                    if response.status().is_success()
                        || (is_delete_operation(operation_name)
                            && response.status() == StatusCode::NOT_FOUND) =>
                {
                    if is_delete_operation(operation_name)
                        && response.status() == StatusCode::NOT_FOUND
                    {
                        counter!("qdrant_delete_not_found_idempotent_total", "operation" => operation_name).increment(1);
                    }
                    if attempt > 1 {
                        counter!("retry_success_after_retry_total", "operation" => operation_name)
                            .increment(1);
                        counter!("qdrant_retry_success_after_retry_total", "operation" => operation_name).increment(1);
                    }
                    histogram!("qdrant_request_duration_ms", "operation" => operation_name)
                        .record(started.elapsed().as_millis() as f64);
                    return Ok(response);
                }
                Ok(response)
                    if self.retry_policy.enabled
                        && retryable_qdrant_status(
                            &self.retry_policy,
                            workload,
                            response.status(),
                        )
                        && attempt < max_attempts =>
                {
                    counter!("qdrant_retry_attempts_total", "operation" => operation_name)
                        .increment(1);
                    counter!("retry_attempts_total", "operation" => operation_name).increment(1);
                    let delay_ms = retry_delay_ms(&self.retry_policy, attempt);
                    if let Some(budget) = budget {
                        if !budget.allows_retry(
                            Duration::from_millis(delay_ms),
                            Duration::from_millis(self.retry_policy.min_operation_budget_ms),
                            Duration::from_millis(self.retry_policy.safety_margin_ms),
                        ) {
                            counter!("astravector_retry_skipped_total", "component" => "qdrant", "workload" => workload_label, "reason" => "insufficient_budget").increment(1);
                            return Err(AstraError::DeadlineExceeded(
                                "insufficient qdrant retry budget".into(),
                            ));
                        }
                    }
                    histogram!("retry_delay_ms", "operation" => operation_name)
                        .record(delay_ms as f64);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    attempt += 1;
                }
                Ok(response)
                    if retryable_qdrant_status(&self.retry_policy, workload, response.status()) =>
                {
                    counter!("qdrant_retry_exhausted_total", "operation" => operation_name)
                        .increment(1);
                    counter!("retry_exhausted_total", "operation" => operation_name).increment(1);
                    return Err(qdrant_status_error(operation_name, response.status()));
                }
                Ok(response) => {
                    counter!("retry_non_retryable_total", "operation" => operation_name)
                        .increment(1);
                    counter!("qdrant_retry_non_retryable_total", "operation" => operation_name)
                        .increment(1);
                    return Err(qdrant_status_error(operation_name, response.status()));
                }
                Err(err)
                    if self.retry_policy.enabled
                        && attempt < max_attempts
                        && ((err.is_timeout() && self.retry_policy.retry_on_timeout)
                            || (err.is_connect() && self.retry_policy.retry_on_connect)) =>
                {
                    counter!("qdrant_retry_attempts_total", "operation" => operation_name)
                        .increment(1);
                    counter!("retry_attempts_total", "operation" => operation_name).increment(1);
                    let delay_ms = retry_delay_ms(&self.retry_policy, attempt);
                    if let Some(budget) = budget {
                        if !budget.allows_retry(
                            Duration::from_millis(delay_ms),
                            Duration::from_millis(self.retry_policy.min_operation_budget_ms),
                            Duration::from_millis(self.retry_policy.safety_margin_ms),
                        ) {
                            counter!("astravector_retry_skipped_total", "component" => "qdrant", "workload" => workload_label, "reason" => "insufficient_budget").increment(1);
                            return Err(AstraError::DeadlineExceeded(
                                "insufficient qdrant retry budget".into(),
                            ));
                        }
                    }
                    histogram!("retry_delay_ms", "operation" => operation_name)
                        .record(delay_ms as f64);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    attempt += 1;
                }
                Err(err) => {
                    if attempt > 1 {
                        counter!("qdrant_retry_exhausted_total", "operation" => operation_name)
                            .increment(1);
                    } else {
                        counter!("qdrant_request_error_total", "operation" => operation_name, "status" => "network").increment(1);
                    }
                    return Err(AstraError::Unavailable(format!(
                        "qdrant {operation_name}: {err}"
                    )));
                }
            }
        }
    }

    pub async fn collection_exists(&self) -> Result<bool, AstraError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection);
        let r = match self
            .send_with_retry("collection_exists", || {
                self.request(self.http.get(url.clone()))
            })
            .await
        {
            Ok(r) => r,
            Err(AstraError::NotFound(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if !r.status().is_success() {
            return Err(qdrant_status_error("collection_exists", r.status()));
        }
        Ok(true)
    }

    pub async fn delete_collection(&self) -> Result<(), AstraError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection);
        let r = self
            .send_with_retry("delete_collection", || {
                self.request(self.http.delete(url.clone()))
            })
            .await?;
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        if !r.status().is_success() {
            return Err(qdrant_status_error("delete_collection", r.status()));
        }
        Ok(())
    }

    fn base_search_filter(
        access_zone_id: Uuid,
        caller_access_level: i16,
        versions: Option<&QdrantVersionFilters>,
    ) -> Value {
        Self::base_search_filter_multi(&[access_zone_id], caller_access_level, versions)
    }

    fn base_search_filter_multi(
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        versions: Option<&QdrantVersionFilters>,
    ) -> Value {
        let now_epoch = chrono::Utc::now().timestamp();
        let zone_values = access_zone_ids
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>();
        let access_zone_match = if zone_values.len() == 1 {
            json!({"key":"access_zone_id","match":{"value":zone_values[0].clone()}})
        } else {
            json!({"key":"access_zone_id","match":{"any":zone_values}})
        };
        let mut must = vec![
            access_zone_match,
            json!({"key":"access_level","range":{"lte":caller_access_level}}),
            json!({"key":"lifecycle_status","match":{"value":"ACTIVE"}}),
            // fix4.5.3 uses a numeric far-future expires_at_epoch for never-expire documents,
            // so the Qdrant filter remains a single range condition and does not rely on should/OR.
            json!({"key":"expires_at_epoch","range":{"gt":now_epoch}}),
            json!({"key":"chunk_granularity","match":{"any":["PARENT","SUB_180","SUB_260"]}}),
        ];
        if let Some(v) = versions {
            if let Some(x) = &v.model_version {
                if !x.is_empty() {
                    must.push(json!({"key":"model_version","match":{"value":x}}));
                }
            }
            if let Some(x) = &v.tokenizer_version {
                if !x.is_empty() {
                    must.push(json!({"key":"tokenizer_version","match":{"value":x}}));
                }
            }
            if let Some(x) = &v.dense_version {
                if !x.is_empty() {
                    must.push(json!({"key":"dense_version","match":{"value":x}}));
                }
            }
            if let Some(x) = &v.sparse_version {
                if !x.is_empty() {
                    must.push(json!({"key":"sparse_version","match":{"value":x}}));
                }
            }
            if let Some(x) = &v.chunking_version {
                if !x.is_empty() {
                    must.push(json!({"key":"chunking_profile_version","match":{"value":x}}));
                }
            }
            for (key, value) in &v.payload_filters {
                if !key.is_empty() && !value.is_empty() {
                    must.push(json!({"key":key,"match":{"value":value}}));
                }
            }
        }
        json!({
            "must": must,
            "must_not": [
                {"key":"quarantined","match":{"value":true}}
            ]
        })
    }

    pub fn canonical_search_filter(
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        versions: Option<&QdrantVersionFilters>,
    ) -> Value {
        Self::base_search_filter_multi(access_zone_ids, caller_access_level, versions)
    }

    fn document_filter(access_zone_id: Uuid, document_id: Uuid, document_version: i64) -> Value {
        json!({"must":[
            {"key":"access_zone_id","match":{"value":access_zone_id.to_string()}},
            {"key":"document_id","match":{"value":document_id.to_string()}},
            {"key":"document_version","match":{"value":document_version}}
        ]})
    }

    /// Ensures that the collection exists with the named dense vector and named sparse vector
    /// used by AstraVector v004. This makes local smoke runs deterministic and prevents
    /// qdrant upsert 404 errors when the publisher starts before the collection exists.
    pub async fn ensure_collection(&self, dense_dimension: usize) -> Result<(), AstraError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection);
        let get = match self
            .send_with_retry("ensure_collection_get", || {
                self.request(self.http.get(url.clone()))
            })
            .await
        {
            Ok(r) => Some(r),
            Err(AstraError::NotFound(_)) => None,
            Err(e) => return Err(e),
        };

        if let Some(get) = get {
            if get.status().is_success() {
                if qdrant_payload_indexes_enabled() {
                    self.ensure_payload_indexes().await?;
                }
                self.validate_collection(dense_dimension).await?;
                return Ok(());
            }
            if get.status() != StatusCode::NOT_FOUND {
                return Err(qdrant_status_error("ensure_collection_get", get.status()));
            }
        }

        let body = json!({
            "vectors": {
                "dense": {
                    "size": dense_dimension,
                    "distance": "Cosine"
                }
            },
            "sparse_vectors": {
                "sparse": {
                    "index": {
                        "on_disk": false
                    }
                }
            }
        });
        let body_for_retry = body.clone();
        let create = self
            .send_with_retry("ensure_collection_create", || {
                self.request(self.http.put(url.clone()).json(&body_for_retry))
            })
            .await?;
        if !create.status().is_success() {
            return Err(qdrant_status_error(
                "ensure_collection_create",
                create.status(),
            ));
        }
        if qdrant_payload_indexes_enabled() {
            self.ensure_payload_indexes().await?;
        }
        self.validate_collection(dense_dimension).await?;
        Ok(())
    }

    pub async fn ensure_payload_indexes(&self) -> Result<(), AstraError> {
        for (field_name, field_schema) in RETRIEVAL_PAYLOAD_INDEXES {
            self.ensure_payload_index(field_name, field_schema).await?;
        }
        Ok(())
    }

    async fn ensure_payload_index(
        &self,
        field_name: &str,
        field_schema: &str,
    ) -> Result<(), AstraError> {
        let url = format!(
            "{}/collections/{}/index?wait=true",
            self.base_url, self.collection
        );
        let body = json!({"field_name": field_name, "field_schema": field_schema});
        let response = self
            .request(self.http.put(url).json(&body))
            .send()
            .await
            .map_err(|e| {
                AstraError::Unavailable(format!("qdrant ensure payload index {field_name}: {e}"))
            })?;
        let status = response.status();
        if status.is_success() || status == StatusCode::CONFLICT {
            counter!("qdrant_payload_index_create_total", "field" => field_name.to_string(), "result" => "ok").increment(1);
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        let lower = text.to_ascii_lowercase();
        if status == StatusCode::BAD_REQUEST
            && (lower.contains("already") || lower.contains("exist"))
        {
            counter!("qdrant_payload_index_create_total", "field" => field_name.to_string(), "result" => "already_exists").increment(1);
            return Ok(());
        }
        counter!("qdrant_payload_index_create_errors_total", "field" => field_name.to_string())
            .increment(1);
        tracing::warn!(field = field_name, schema = field_schema, status = %status, error = %text, "QDRANT_PAYLOAD_INDEX_CREATE_FAILED");
        Err(qdrant_status_error("ensure_payload_index", status))
    }

    pub async fn upsert(&self, point: &QdrantPoint) -> Result<(), AstraError> {
        smoke_failpoints::qdrant_upsert()?;
        let mut vectors = serde_json::Map::new();
        if let Some(v) = &point.dense {
            vectors.insert("dense".into(), json!(v));
        }
        if let (Some(indices), Some(values)) = (&point.sparse_indices, &point.sparse_values) {
            vectors.insert(
                "sparse".into(),
                json!({"indices": indices, "values": values}),
            );
        }
        let body = json!({"points":[{"id":point.id.to_string(),"vector":vectors,"payload":point.payload}]});
        let url = format!(
            "{}/collections/{}/points?wait=true",
            self.base_url, self.collection
        );
        let body_for_retry = body.clone();
        let url_for_retry = url.clone();
        let _r = self
            .send_with_retry("upsert", || {
                self.request(self.http.put(url_for_retry.clone()).json(&body_for_retry))
            })
            .await?;
        Ok(())
    }
    pub async fn update_payload(&self, point_id: Uuid, payload: Value) -> Result<(), AstraError> {
        let url = format!(
            "{}/collections/{}/points/payload?wait=true",
            self.base_url, self.collection
        );
        let body = json!({"payload":payload,"points":[point_id.to_string()]});
        let body_for_retry = body.clone();
        let r = self
            .send_with_retry("update_payload", || {
                self.request(self.http.post(url.clone()).json(&body_for_retry))
            })
            .await?;
        if !r.status().is_success() {
            return Err(qdrant_status_error("update_payload", r.status()));
        }
        Ok(())
    }
    pub async fn point_exists(&self, point_id: Uuid) -> Result<bool, AstraError> {
        let url = format!(
            "{}/collections/{}/points/{}",
            self.base_url, self.collection, point_id
        );
        let r = match self
            .send_with_retry("point_exists", || self.request(self.http.get(url.clone())))
            .await
        {
            Ok(r) => r,
            Err(AstraError::NotFound(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if !r.status().is_success() {
            return Err(qdrant_status_error("point_exists", r.status()));
        }
        Ok(true)
    }
    pub async fn validate_collection(&self, expected_dimension: usize) -> Result<(), AstraError> {
        let compatibility = self.collection_compatibility(expected_dimension).await?;
        if compatibility.actual_dense_dimension != expected_dimension {
            return Err(AstraError::FailedPrecondition(format!(
                "QDRANT_COLLECTION_SCHEMA_MISMATCH: dense dimension mismatch: expected={}, actual={}",
                compatibility.expected_dense_dimension, compatibility.actual_dense_dimension
            )));
        }
        if !compatibility.dense_distance.eq_ignore_ascii_case("Cosine") {
            return Err(AstraError::FailedPrecondition(format!(
                "QDRANT_COLLECTION_SCHEMA_MISMATCH: dense distance mismatch: expected=Cosine, actual={}",
                compatibility.dense_distance
            )));
        }
        if !compatibility.sparse_vector_present {
            return Err(AstraError::FailedPrecondition(
                "QDRANT_COLLECTION_SCHEMA_MISMATCH: missing sparse vector named sparse".into(),
            ));
        }
        if !compatibility.missing_payload_indexes.is_empty()
            || !compatibility.mismatched_payload_indexes.is_empty()
        {
            return Err(AstraError::FailedPrecondition(format!(
                "QDRANT_COLLECTION_SCHEMA_MISMATCH: payload index mismatch: missing={:?}, mismatched={:?}",
                compatibility.missing_payload_indexes, compatibility.mismatched_payload_indexes
            )));
        }
        Ok(())
    }

    pub async fn collection_compatibility(
        &self,
        expected_dimension: usize,
    ) -> Result<QdrantCollectionCompatibilitySummary, AstraError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection);
        let r = self
            .send_with_retry("collection_compatibility", || {
                self.request(self.http.get(url.clone()))
            })
            .await?;
        if !r.status().is_success() {
            return Err(qdrant_status_error("collection_compatibility", r.status()));
        }
        let body: Value = r
            .json()
            .await
            .map_err(|e| AstraError::Internal(format!("qdrant collection json: {e}")))?;
        Ok(qdrant_collection_compatibility_from_info(
            &body,
            expected_dimension,
        ))
    }

    pub async fn search_dense(
        &self,
        dense: &[f32],
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        limit: usize,
        versions: Option<&QdrantVersionFilters>,
    ) -> Result<Vec<QdrantSearchHit>, AstraError> {
        self.search_dense_with_budget(
            dense,
            access_zone_ids,
            caller_access_level,
            limit,
            versions,
            None,
        )
        .await
    }

    pub async fn search_dense_with_budget(
        &self,
        dense: &[f32],
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        limit: usize,
        versions: Option<&QdrantVersionFilters>,
        budget: Option<&OperationBudget>,
    ) -> Result<Vec<QdrantSearchHit>, AstraError> {
        smoke_failpoints::hit("qdrant_dense_search")?;
        let _permit = tokio::time::timeout(
            Duration::from_millis(self.search_acquire_timeout_ms),
            self.search_semaphore.clone().acquire_owned(),
        )
        .await
        .map_err(|_| {
            counter!("qdrant_search_rejected_total", "operation" => "search_dense", "reason" => "acquire_timeout").increment(1);
            AstraError::ResourceExhausted("qdrant search concurrency limit exceeded".into())
        })?
        .map_err(|_| AstraError::Unavailable("qdrant search semaphore closed".into()))?;
        gauge!("qdrant_search_permits_available")
            .set(self.search_semaphore.available_permits() as f64);
        gauge!("qdrant_search_concurrent_active").set(
            self.search_max_concurrency
                .saturating_sub(self.search_semaphore.available_permits()) as f64,
        );
        let body = json!({
            "vector": {"name": "dense", "vector": dense},
            "limit": limit,
            "with_payload": true,
            "with_vector": false,
            "filter": Self::base_search_filter_multi(access_zone_ids, caller_access_level, versions)
        });
        let filter_summary = body
            .get("filter")
            .map(Value::to_string)
            .unwrap_or_else(|| "{}".to_string());
        tracing::debug!(
            collection = %self.collection,
            vector = "dense",
            dense_dim = dense.len(),
            access_zone_ids = ?access_zone_ids,
            caller_access_level,
            limit,
            filter = %filter_summary,
            "QDRANT_DENSE_SEARCH_REQUEST"
        );
        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection
        );
        let body_for_retry = body.clone();
        let url_for_retry = url.clone();
        let r = self
            .send_with_retry_budget("search_dense", WorkloadKind::Query, budget, || {
                self.request(self.http.post(url_for_retry.clone()).json(&body_for_retry))
            })
            .await?;
        let body: Value = r
            .json()
            .await
            .map_err(|e| AstraError::Internal(format!("qdrant search json: {e}")))?;
        let points = body
            .get("result")
            .and_then(Value::as_array)
            .ok_or_else(|| AstraError::Internal("qdrant search result missing".into()))?;
        tracing::debug!(
            collection = %self.collection,
            vector = "dense",
            raw_hits_count = points.len(),
            "QDRANT_DENSE_SEARCH_RESPONSE"
        );
        let mut hits = Vec::with_capacity(points.len());
        for point in points {
            let Some(id) = point
                .get("id")
                .and_then(Value::as_str)
                .and_then(|v| Uuid::parse_str(v).ok())
            else {
                continue;
            };
            let score = point.get("score").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            hits.push(QdrantSearchHit {
                id,
                score,
                dense_score: score,
                sparse_score: 0.0,
                fusion_score: score,
                dense_rank: Some((hits.len() + 1) as u32),
                sparse_rank: None,
                payload: point.get("payload").cloned().unwrap_or(Value::Null),
            });
        }
        Ok(hits)
    }

    pub async fn search_sparse(
        &self,
        indices: &[u32],
        values: &[f32],
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        limit: usize,
        versions: Option<&QdrantVersionFilters>,
    ) -> Result<Vec<QdrantSearchHit>, AstraError> {
        self.search_sparse_with_budget(
            indices,
            values,
            access_zone_ids,
            caller_access_level,
            limit,
            versions,
            None,
        )
        .await
    }

    pub async fn search_sparse_with_budget(
        &self,
        indices: &[u32],
        values: &[f32],
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        limit: usize,
        versions: Option<&QdrantVersionFilters>,
        budget: Option<&OperationBudget>,
    ) -> Result<Vec<QdrantSearchHit>, AstraError> {
        smoke_failpoints::hit("qdrant_sparse_search")?;
        let _permit = tokio::time::timeout(
            Duration::from_millis(self.search_acquire_timeout_ms),
            self.search_semaphore.clone().acquire_owned(),
        )
        .await
        .map_err(|_| {
            counter!("qdrant_search_rejected_total", "operation" => "search_sparse", "reason" => "acquire_timeout").increment(1);
            AstraError::ResourceExhausted("qdrant search concurrency limit exceeded".into())
        })?
        .map_err(|_| AstraError::Unavailable("qdrant search semaphore closed".into()))?;
        gauge!("qdrant_search_permits_available")
            .set(self.search_semaphore.available_permits() as f64);
        gauge!("qdrant_search_concurrent_active").set(
            self.search_max_concurrency
                .saturating_sub(self.search_semaphore.available_permits()) as f64,
        );
        if indices.is_empty() || values.is_empty() {
            return Ok(Vec::new());
        }
        let body = json!({
            "vector": {"name": "sparse", "vector": {"indices": indices, "values": values}},
            "limit": limit,
            "with_payload": true,
            "with_vector": false,
            "filter": Self::base_search_filter_multi(access_zone_ids, caller_access_level, versions)
        });
        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection
        );
        let body_for_retry = body.clone();
        let url_for_retry = url.clone();
        let r = self
            .send_with_retry_budget("search_sparse", WorkloadKind::Query, budget, || {
                self.request(self.http.post(url_for_retry.clone()).json(&body_for_retry))
            })
            .await?;
        let body: Value = r
            .json()
            .await
            .map_err(|e| AstraError::Internal(format!("qdrant sparse search json: {e}")))?;
        let points = body
            .get("result")
            .and_then(Value::as_array)
            .ok_or_else(|| AstraError::Internal("qdrant sparse search result missing".into()))?;
        let mut hits = Vec::with_capacity(points.len());
        for point in points {
            let Some(id) = point
                .get("id")
                .and_then(Value::as_str)
                .and_then(|v| Uuid::parse_str(v).ok())
            else {
                continue;
            };
            let score = point.get("score").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            hits.push(QdrantSearchHit {
                id,
                score,
                dense_score: 0.0,
                sparse_score: score,
                fusion_score: score,
                dense_rank: None,
                sparse_rank: Some((hits.len() + 1) as u32),
                payload: point.get("payload").cloned().unwrap_or(Value::Null),
            });
        }
        Ok(hits)
    }

    pub async fn count_points_by_document(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<u32, AstraError> {
        if !self.collection_exists().await? {
            return Ok(0);
        }
        let url = format!(
            "{}/collections/{}/points/count",
            self.base_url, self.collection
        );
        let body = json!({"exact": true, "filter": Self::document_filter(access_zone_id, document_id, document_version)});
        let r = self
            .send_with_retry("count_points_by_document", || {
                self.request(self.http.post(url.clone()).json(&body))
            })
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant count: {e}")))?;
        if !r.status().is_success() {
            return Err(AstraError::Unavailable(format!(
                "qdrant count status={}",
                r.status()
            )));
        }
        let body: Value = r
            .json()
            .await
            .map_err(|e| AstraError::Internal(format!("qdrant count json: {e}")))?;
        Ok(body
            .pointer("/result/count")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32)
    }

    pub async fn point_ids_by_document(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<HashSet<Uuid>, AstraError> {
        Ok(self
            .point_ids_by_document_paginated(access_zone_id, document_id, document_version)
            .await?
            .point_ids)
    }

    pub async fn point_ids_by_document_with_page_size(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        page_size: u64,
    ) -> Result<HashSet<Uuid>, AstraError> {
        Ok(self
            .point_ids_by_document_paginated_with_page_size(
                access_zone_id,
                document_id,
                document_version,
                Some(page_size),
            )
            .await?
            .point_ids)
    }

    pub async fn point_ids_by_document_paginated(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<QdrantScrollPointIdsResult, AstraError> {
        self.point_ids_by_document_paginated_with_page_size(
            access_zone_id,
            document_id,
            document_version,
            None,
        )
        .await
    }

    pub async fn point_ids_by_document_paginated_with_page_size(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        page_size_override: Option<u64>,
    ) -> Result<QdrantScrollPointIdsResult, AstraError> {
        counter!("astravector_qdrant_scroll_requests_total").increment(1);
        let started = Instant::now();
        let _permit = self.scroll_semaphore.acquire().await.map_err(|_| {
            AstraError::ResourceExhausted(
                "QDRANT_SCROLL_RESOURCE_EXHAUSTED: qdrant scroll semaphore closed".into(),
            )
        })?;
        gauge!("astravector_qdrant_scroll_concurrent_inflight").increment(1.0);

        let result = async {
            if !self.collection_exists().await? {
                return Ok(QdrantScrollPointIdsResult {
                    point_ids: HashSet::new(),
                    pages_read: 0,
                    points_read: 0,
                    completed: true,
                    status: QdrantScrollStatus::Completed,
                });
            }

            let url = format!("{}/collections/{}/points/scroll", self.base_url, self.collection);
            let mut ids: HashSet<Uuid> = HashSet::new();
            let mut seen_offsets: HashSet<String> = HashSet::new();
            let mut next_page_offset: Option<Value> = None;
            let mut pages_read: u64 = 0;
            let timeout = Duration::from_secs(self.scroll_timeout_secs);

            loop {
                if started.elapsed() > timeout {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "timeout").increment(1);
                    return Err(AstraError::DeadlineExceeded(format!(
                        "QDRANT_SCROLL_TIMEOUT: pages_read={pages_read} points_read={}",
                        ids.len()
                    )));
                }
                if pages_read >= self.scroll_max_pages {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "limit_exceeded").increment(1);
                    counter!("astravector_qdrant_scroll_limit_exceeded_total").increment(1);
                    return Err(AstraError::ResourceExhausted(format!(
                        "QDRANT_SCROLL_LIMIT_EXCEEDED: max_pages={} pages_read={pages_read} points_read={}",
                        self.scroll_max_pages,
                        ids.len()
                    )));
                }
                if ids.len() as u64 >= self.scroll_max_points {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "limit_exceeded").increment(1);
                    counter!("astravector_qdrant_scroll_limit_exceeded_total").increment(1);
                    return Err(AstraError::ResourceExhausted(format!(
                        "QDRANT_SCROLL_LIMIT_EXCEEDED: max_points={} pages_read={pages_read} points_read={}",
                        self.scroll_max_points,
                        ids.len()
                    )));
                }

                let mut body = json!({
                    "filter": Self::document_filter(access_zone_id, document_id, document_version),
                    "limit": page_size_override.unwrap_or_else(|| self.effective_scroll_page_size()).max(1),
                    "with_payload": false,
                    "with_vector": false
                });
                if let Some(offset) = next_page_offset.clone() {
                    body["offset"] = offset;
                }

                let r = self.send_with_retry("point_ids_by_document_scroll", || self.request(self.http.post(url.clone()).json(&body))).await
                    .map_err(|e| {
                        counter!("astravector_qdrant_scroll_errors_total", "reason" => "qdrant_error").increment(1);
                        AstraError::Unavailable(format!("QDRANT_SCROLL_FAILED: qdrant scroll: {e}"))
                    })?;
                if !r.status().is_success() {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "qdrant_error").increment(1);
                    return Err(AstraError::Unavailable(format!("QDRANT_SCROLL_FAILED: qdrant scroll status={}", r.status())));
                }
                let page: Value = r.json().await.map_err(|e| {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "qdrant_error").increment(1);
                    AstraError::Internal(format!("QDRANT_SCROLL_FAILED: qdrant scroll json: {e}"))
                })?;

                pages_read += 1;
                let points = page.pointer("/result/points").and_then(Value::as_array).cloned().unwrap_or_default();
                for point in points {
                    if let Some(id) = point.get("id").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok()) {
                        ids.insert(id);
                    }
                }

                if ids.len() as u64 > self.scroll_max_points {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "limit_exceeded").increment(1);
                    counter!("astravector_qdrant_scroll_limit_exceeded_total").increment(1);
                    return Err(AstraError::ResourceExhausted(format!(
                        "QDRANT_SCROLL_LIMIT_EXCEEDED: max_points={} pages_read={pages_read} points_read={}",
                        self.scroll_max_points,
                        ids.len()
                    )));
                }

                let Some(offset) = page.pointer("/result/next_page_offset").cloned().filter(|v| !v.is_null()) else {
                    let points_read = ids.len() as u64;
                    histogram!("astravector_qdrant_scroll_pages_total").record(pages_read as f64);
                    histogram!("astravector_qdrant_scroll_points_total").record(points_read as f64);
                    return Ok(QdrantScrollPointIdsResult {
                        point_ids: ids,
                        pages_read,
                        points_read,
                        completed: true,
                        status: QdrantScrollStatus::Completed,
                    });
                };

                let offset_key = offset.to_string();
                if !seen_offsets.insert(offset_key) {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "loop").increment(1);
                    return Err(AstraError::Internal(format!(
                        "QDRANT_SCROLL_LOOP: repeated next_page_offset pages_read={pages_read} points_read={}",
                        ids.len()
                    )));
                }
                next_page_offset = Some(offset);
            }
        }
        .await;

        let elapsed = started.elapsed().as_secs_f64();
        histogram!("astravector_qdrant_scroll_latency_seconds").record(elapsed);
        gauge!("astravector_qdrant_scroll_concurrent_inflight").decrement(1.0);
        if let Some(adaptive) = &self.adaptive {
            match &result {
                Ok(r) => adaptive.observe_qdrant_scroll(
                    r.pages_read,
                    elapsed,
                    None,
                    self.scroll_page_size,
                ),
                Err(e) => {
                    let msg = e.to_string();
                    let reason = if msg.contains("QDRANT_SCROLL_TIMEOUT") {
                        "timeout"
                    } else if msg.contains("QDRANT_SCROLL_LIMIT_EXCEEDED") {
                        "limit_exceeded"
                    } else if msg.contains("QDRANT_SCROLL_LOOP") {
                        "loop"
                    } else {
                        "qdrant_error"
                    };
                    adaptive.observe_qdrant_scroll(0, elapsed, Some(reason), self.scroll_page_size);
                }
            }
        }
        result
    }

    pub async fn scroll_all_points_with_payload(
        &self,
    ) -> Result<QdrantScrollPointsResult, AstraError> {
        counter!("astravector_qdrant_scroll_requests_total").increment(1);
        let started = Instant::now();
        let _permit = self.scroll_semaphore.acquire().await.map_err(|_| {
            AstraError::ResourceExhausted(
                "QDRANT_SCROLL_RESOURCE_EXHAUSTED: qdrant scroll semaphore closed".into(),
            )
        })?;
        gauge!("astravector_qdrant_scroll_concurrent_inflight").increment(1.0);

        let result = async {
            if !self.collection_exists().await? {
                return Ok(QdrantScrollPointsResult {
                    payloads: HashMap::new(),
                    pages_read: 0,
                    points_read: 0,
                    completed: true,
                    status: QdrantScrollStatus::Completed,
                });
            }

            let url = format!("{}/collections/{}/points/scroll", self.base_url, self.collection);
            let mut payloads: HashMap<Uuid, Value> = HashMap::new();
            let mut seen_offsets: HashSet<String> = HashSet::new();
            let mut next_page_offset: Option<Value> = None;
            let mut pages_read: u64 = 0;
            let timeout = Duration::from_secs(self.scroll_timeout_secs);

            loop {
                if started.elapsed() > timeout {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "timeout")
                        .increment(1);
                    return Err(AstraError::DeadlineExceeded(format!(
                        "QDRANT_SCROLL_TIMEOUT: pages_read={pages_read} points_read={}",
                        payloads.len()
                    )));
                }
                if pages_read >= self.scroll_max_pages {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "limit_exceeded").increment(1);
                    counter!("astravector_qdrant_scroll_limit_exceeded_total").increment(1);
                    return Err(AstraError::ResourceExhausted(format!(
                        "QDRANT_SCROLL_LIMIT_EXCEEDED: max_pages={} pages_read={pages_read} points_read={}",
                        self.scroll_max_pages,
                        payloads.len()
                    )));
                }
                if payloads.len() as u64 >= self.scroll_max_points {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "limit_exceeded").increment(1);
                    counter!("astravector_qdrant_scroll_limit_exceeded_total").increment(1);
                    return Err(AstraError::ResourceExhausted(format!(
                        "QDRANT_SCROLL_LIMIT_EXCEEDED: max_points={} pages_read={pages_read} points_read={}",
                        self.scroll_max_points,
                        payloads.len()
                    )));
                }

                let mut body = json!({
                    "limit": self.effective_scroll_page_size().max(1),
                    "with_payload": true,
                    "with_vector": false
                });
                if let Some(offset) = next_page_offset.clone() {
                    body["offset"] = offset;
                }

                let r = self
                    .send_with_retry("scroll_all_points", || {
                        self.request(self.http.post(url.clone()).json(&body))
                    })
                    .await?;
                if !r.status().is_success() {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "qdrant_error")
                        .increment(1);
                    return Err(qdrant_status_error("scroll_all_points", r.status()));
                }
                let page: Value = r.json().await.map_err(|e| {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "qdrant_error")
                        .increment(1);
                    AstraError::Internal(format!("QDRANT_SCROLL_FAILED: qdrant scroll json: {e}"))
                })?;

                pages_read += 1;
                let points = page
                    .pointer("/result/points")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for point in points {
                    if let Some(id) = point
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|v| Uuid::parse_str(v).ok())
                    {
                        payloads.insert(id, point.get("payload").cloned().unwrap_or(Value::Null));
                    }
                }

                if payloads.len() as u64 > self.scroll_max_points {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "limit_exceeded").increment(1);
                    counter!("astravector_qdrant_scroll_limit_exceeded_total").increment(1);
                    return Err(AstraError::ResourceExhausted(format!(
                        "QDRANT_SCROLL_LIMIT_EXCEEDED: max_points={} pages_read={pages_read} points_read={}",
                        self.scroll_max_points,
                        payloads.len()
                    )));
                }

                let Some(offset) = page
                    .pointer("/result/next_page_offset")
                    .cloned()
                    .filter(|v| !v.is_null())
                else {
                    let points_read = payloads.len() as u64;
                    histogram!("astravector_qdrant_scroll_pages_total").record(pages_read as f64);
                    histogram!("astravector_qdrant_scroll_points_total").record(points_read as f64);
                    return Ok(QdrantScrollPointsResult {
                        payloads,
                        pages_read,
                        points_read,
                        completed: true,
                        status: QdrantScrollStatus::Completed,
                    });
                };

                let offset_key = offset.to_string();
                if !seen_offsets.insert(offset_key) {
                    counter!("astravector_qdrant_scroll_errors_total", "reason" => "loop")
                        .increment(1);
                    return Err(AstraError::Internal(format!(
                        "QDRANT_SCROLL_LOOP: repeated next_page_offset pages_read={pages_read} points_read={}",
                        payloads.len()
                    )));
                }
                next_page_offset = Some(offset);
            }
        }
        .await;

        gauge!("astravector_qdrant_scroll_concurrent_inflight").decrement(1.0);
        result
    }

    pub async fn delete_points_batch(&self, point_ids: &[Uuid]) -> Result<(), AstraError> {
        if point_ids.is_empty() {
            return Ok(());
        }
        let url = format!(
            "{}/collections/{}/points/delete?wait=true",
            self.base_url, self.collection
        );
        let points = point_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let body = json!({"points": points});
        let r = self
            .send_with_retry("delete_points_batch", || {
                self.request(self.http.post(url.clone()).json(&body))
            })
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant delete_points_batch: {e}")))?;
        if r.status().is_success() || r.status() == StatusCode::NOT_FOUND {
            counter!("qdrant_points_delete_success_total").increment(point_ids.len() as u64);
            counter!("qdrant_points_delete_batches_total").increment(1);
            return Ok(());
        }
        counter!("qdrant_points_delete_failed_total").increment(1);
        Err(AstraError::Unavailable(format!(
            "qdrant delete_points_batch status={}",
            r.status()
        )))
    }

    pub async fn delete(&self, point_id: Uuid) -> Result<(), AstraError> {
        let url = format!(
            "{}/collections/{}/points/delete?wait=true",
            self.base_url, self.collection
        );
        let body = json!({"points":[point_id.to_string()]});
        let r = self
            .send_with_retry("delete", || {
                self.request(self.http.post(url.clone()).json(&body))
            })
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant delete: {e}")))?;
        if r.status().is_success() || r.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(AstraError::Unavailable(format!(
            "qdrant delete status={}",
            r.status()
        )))
    }
}

#[cfg(test)]
mod retry_policy_tests {
    use super::*;

    #[test]
    fn qdrant_429_query_is_not_retried() {
        let policy = RetryPolicyConfig {
            retry_on_statuses: vec![429, 502, 503, 504],
            ..RetryPolicyConfig::default()
        };
        assert!(!retryable_qdrant_status(
            &policy,
            WorkloadKind::Query,
            StatusCode::TOO_MANY_REQUESTS,
        ));
        assert!(matches!(
            qdrant_status_error("search_dense", StatusCode::TOO_MANY_REQUESTS),
            AstraError::ResourceExhausted(_)
        ));
    }

    #[test]
    fn qdrant_429_publisher_can_retry() {
        let policy = RetryPolicyConfig {
            retry_on_statuses: vec![429, 502, 503, 504],
            ..RetryPolicyConfig::default()
        };
        assert!(retryable_qdrant_status(
            &policy,
            WorkloadKind::DocumentPublisher,
            StatusCode::TOO_MANY_REQUESTS,
        ));
    }
}
