use crate::{error::AstraError, smoke_failpoints};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantPoint {
    pub id: Uuid,
    pub dense: Option<Vec<f32>>,
    pub sparse_indices: Option<Vec<u32>>,
    pub sparse_values: Option<Vec<f32>>,
    pub payload: Value,
}

#[derive(Clone)]
pub struct QdrantClient {
    http: Client,
    base_url: String,
    api_key: Option<String>,
    collection: String,
}

#[derive(Debug, Clone)]
pub struct QdrantSearchHit {
    pub id: Uuid,
    pub score: f32,
    pub payload: Value,
}

impl QdrantClient {
    pub fn new(
        base_url: String,
        api_key: Option<String>,
        collection: String,
        timeout_ms: u64,
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
        })
    }
    fn request(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) if !k.is_empty() => rb.header("api-key", k),
            _ => rb,
        }
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
        let r = self
            .request(self.http.put(url).json(&body))
            .send()
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant upsert: {e}")))?;
        if !r.status().is_success() {
            return Err(AstraError::Unavailable(format!(
                "qdrant upsert status={}",
                r.status()
            )));
        }
        Ok(())
    }
    pub async fn update_payload(&self, point_id: Uuid, payload: Value) -> Result<(), AstraError> {
        let url = format!(
            "{}/collections/{}/points/payload?wait=true",
            self.base_url, self.collection
        );
        let body = json!({"payload":payload,"points":[point_id.to_string()]});
        let r = self
            .request(self.http.post(url).json(&body))
            .send()
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant payload: {e}")))?;
        if !r.status().is_success() {
            return Err(AstraError::Unavailable(format!(
                "qdrant payload status={}",
                r.status()
            )));
        }
        Ok(())
    }
    pub async fn point_exists(&self, point_id: Uuid) -> Result<bool, AstraError> {
        let url = format!(
            "{}/collections/{}/points/{}",
            self.base_url, self.collection, point_id
        );
        let r = self
            .request(self.http.get(url))
            .send()
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant point get: {e}")))?;
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if !r.status().is_success() {
            return Err(AstraError::Unavailable(format!(
                "qdrant point get status={}",
                r.status()
            )));
        }
        Ok(true)
    }
    pub async fn validate_collection(&self, expected_dimension: usize) -> Result<(), AstraError> {
        let url = format!("{}/collections/{}", self.base_url, self.collection);
        let r = self
            .request(self.http.get(url))
            .send()
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant collection: {e}")))?;
        if !r.status().is_success() {
            return Err(AstraError::Unavailable(format!(
                "qdrant collection status={}",
                r.status()
            )));
        }
        let body: Value = r
            .json()
            .await
            .map_err(|e| AstraError::Internal(format!("qdrant collection json: {e}")))?;
        let dimension = body
            .pointer("/result/config/params/vectors/dense/size")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if dimension != expected_dimension {
            return Err(AstraError::FailedPrecondition(format!("qdrant dense dimension mismatch: expected={expected_dimension}, actual={dimension}")));
        }
        Ok(())
    }

    pub async fn search_dense(
        &self,
        dense: &[f32],
        access_zone_id: Uuid,
        caller_access_level: i16,
        limit: usize,
    ) -> Result<Vec<QdrantSearchHit>, AstraError> {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let body = json!({
            "vector": {"name": "dense", "vector": dense},
            "limit": limit,
            "with_payload": true,
            "with_vector": false,
            "filter": {
                "must": [
                    {"key":"access_zone_id","match":{"value":access_zone_id.to_string()}},
                    {"key":"access_level","range":{"lte":caller_access_level}},
                    {"key":"lifecycle_status","match":{"value":"ACTIVE"}},
                    {"key":"chunk_granularity","match":{"any":["PARENT","SUB_180","SUB_260"]}}
                ],
                "must_not": [
                    {"key":"quarantined","match":{"value":true}}
                ],
                "should": [
                    {"is_empty":{"key":"expires_at"}},
                    {"key":"expires_at","range":{"gt":now}}
                ]
            }
        });
        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection
        );
        let r = self
            .request(self.http.post(url).json(&body))
            .send()
            .await
            .map_err(|e| AstraError::Unavailable(format!("qdrant search: {e}")))?;
        if !r.status().is_success() {
            return Err(AstraError::Unavailable(format!(
                "qdrant search status={}",
                r.status()
            )));
        }
        let body: Value = r
            .json()
            .await
            .map_err(|e| AstraError::Internal(format!("qdrant search json: {e}")))?;
        let points = body
            .get("result")
            .and_then(Value::as_array)
            .ok_or_else(|| AstraError::Internal("qdrant search result missing".into()))?;
        let mut hits = Vec::with_capacity(points.len());
        for point in points {
            let Some(id) = point
                .get("id")
                .and_then(Value::as_str)
                .and_then(|v| Uuid::parse_str(v).ok())
            else {
                continue;
            };
            hits.push(QdrantSearchHit {
                id,
                score: point.get("score").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                payload: point.get("payload").cloned().unwrap_or(Value::Null),
            });
        }
        Ok(hits)
    }

    pub async fn delete(&self, point_id: Uuid) -> Result<(), AstraError> {
        let url = format!(
            "{}/collections/{}/points/delete?wait=true",
            self.base_url, self.collection
        );
        let body = json!({"points":[point_id.to_string()]});
        let r = self
            .request(self.http.post(url).json(&body))
            .send()
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
