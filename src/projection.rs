use crate::{inference::EmbeddingResult, qdrant::QdrantPoint};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

const NEVER_EXPIRES_EPOCH: i64 = 253_402_300_799_i64;

#[derive(Debug, Clone)]
pub struct CanonicalProjectionInput {
    pub access_zone_id: Uuid,
    pub access_zone_code: String,
    pub binding_id: Uuid,
    pub qdrant_point_id: Uuid,
    pub document_id: Uuid,
    pub document_version: i64,
    pub root_chunk_id: Uuid,
    pub source_chunk_id: Uuid,
    pub parent_chunk_id: Option<Uuid>,
    pub chunk_id: Uuid,
    pub chunk_granularity: String,
    pub representation_type: String,
    pub access_level: i16,
    pub lifecycle_status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub legal_hold: bool,
    pub payload_version: i64,
    pub model_version: String,
    pub tokenizer_version: String,
    pub dense_version: Option<String>,
    pub sparse_version: Option<String>,
    pub metadata: Value,
}

impl CanonicalProjectionInput {
    pub fn from_pg_row(
        row: &sqlx::postgres::PgRow,
        access_zone_id: Uuid,
        binding_id: Uuid,
    ) -> Self {
        Self {
            access_zone_id,
            access_zone_code: row
                .try_get::<Option<String>, _>("access_zone_code")
                .ok()
                .flatten()
                .unwrap_or_default(),
            binding_id,
            qdrant_point_id: row.get("qdrant_point_id"),
            document_id: row.get("document_id"),
            document_version: row.get("document_version"),
            root_chunk_id: row.get("root_chunk_id"),
            source_chunk_id: row.get("source_chunk_id"),
            parent_chunk_id: row
                .try_get::<Option<Uuid>, _>("parent_chunk_id")
                .ok()
                .flatten(),
            chunk_id: row.get("chunk_id"),
            chunk_granularity: row.get("chunk_granularity"),
            representation_type: row.get("representation_type"),
            access_level: row.get("access_level"),
            lifecycle_status: row.get("lifecycle_status"),
            expires_at: row
                .try_get::<Option<DateTime<Utc>>, _>("expires_at")
                .ok()
                .flatten(),
            legal_hold: row.get("legal_hold"),
            payload_version: row.get("payload_version"),
            model_version: row.get("model_version"),
            tokenizer_version: row.get("tokenizer_version"),
            dense_version: row
                .try_get::<Option<String>, _>("dense_version")
                .ok()
                .flatten(),
            sparse_version: row
                .try_get::<Option<String>, _>("sparse_version")
                .ok()
                .flatten(),
            metadata: row.get("metadata"),
        }
    }

    pub fn payload(&self) -> Value {
        let chunking_profile_version = self
            .metadata
            .get("chunking_profile_version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let source_block_id = self
            .metadata
            .get("source_block_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let trace_quality = self
            .metadata
            .get("trace_quality")
            .and_then(|v| v.as_str())
            .unwrap_or("MISSING");
        let trace_relation_type = self
            .metadata
            .get("trace_relation_type")
            .and_then(|v| v.as_str())
            .unwrap_or("SYNTHETIC");
        let quality_run_id = self
            .metadata
            .get("quality_run_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let quality_runtime_bench = self
            .metadata
            .get("quality_runtime_bench")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let expires_at = self
            .expires_at
            .as_ref()
            .map(|x| x.to_rfc3339_opts(SecondsFormat::Secs, true));
        let expires_at_epoch = self
            .expires_at
            .map(|x| x.timestamp())
            // Never-expire legacy or ttl_days=0 points use a far-future epoch so Qdrant filters stay range-only.
            .unwrap_or(NEVER_EXPIRES_EPOCH);

        json!({
            "access_zone_id": self.access_zone_id,
            "access_zone_code": self.access_zone_code,
            "binding_id": self.binding_id,
            "qdrant_point_id": self.qdrant_point_id,
            "document_id": self.document_id,
            "document_version": self.document_version,
            "root_chunk_id": self.root_chunk_id,
            "source_chunk_id": self.source_chunk_id,
            "parent_chunk_id": self.parent_chunk_id,
            "chunk_id": self.chunk_id,
            "source_block_id": source_block_id,
            "trace_quality": trace_quality,
            "trace_relation_type": trace_relation_type,
            "chunk_granularity": self.chunk_granularity,
            "representation_type": self.representation_type,
            "access_level": self.access_level,
            "lifecycle_status": self.lifecycle_status,
            "expires_at": expires_at,
            "expires_at_epoch": expires_at_epoch,
            "legal_hold": self.legal_hold,
            "payload_version": self.payload_version,
            "model_version": self.model_version,
            "tokenizer_version": self.tokenizer_version,
            "dense_version": self.dense_version,
            "sparse_version": self.sparse_version,
            "chunking_profile_version": chunking_profile_version,
            "quality_run_id": quality_run_id,
            "quality_runtime_bench": quality_runtime_bench,
            "quarantined": false
        })
    }

    pub fn point(&self, embedding: EmbeddingResult) -> QdrantPoint {
        QdrantPoint {
            id: self.qdrant_point_id,
            dense: embedding.dense,
            sparse_indices: embedding.sparse_indices,
            sparse_values: embedding.sparse_values,
            payload: self.payload(),
        }
    }
}
