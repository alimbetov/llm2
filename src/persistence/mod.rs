use crate::{
    chunking::GeneratedChunk,
    config::PostgresConfig,
    error::AstraError,
    graph::{
        self, ChunkEmbeddingForGraph, GraphBuildLimits, GraphEdge, GraphNode, GraphRelationType,
        RelatedChunk,
    },
    inference::EmbeddingResult,
    pb, smoke_failpoints,
};
use chrono::{DateTime, Utc};
use metrics::counter;
use pgvector::Vector;
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::{PgPool, PgPoolOptions},
    Postgres, QueryBuilder, Row, Transaction,
};
use std::time::Duration;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository {
    pub pool: PgPool,
}
#[derive(Debug, Clone)]
pub enum ClaimResult {
    Acquired {
        cache_entry_id: Uuid,
        lease_token: i64,
    },
    Completed {
        cache_entry_id: Uuid,
        result: EmbeddingResult,
    },
    ProcessingByOther {
        cache_entry_id: Uuid,
        lease_expires_at: Option<DateTime<Utc>>,
    },
    RetryAcquired {
        cache_entry_id: Uuid,
        lease_token: i64,
    },
}
#[derive(Debug, Clone)]
pub struct RequestRecord {
    pub id: Uuid,
    pub request_hash: String,
    pub status: String,
}
#[derive(Debug, Clone)]
pub struct DocumentVersionRecord {
    pub document_id: Uuid,
    pub document_version: i64,
    pub status: String,
}
#[derive(Debug, Clone)]
pub struct StoredChunkRecord {
    pub id: Uuid,
    pub root_id: Uuid,
    pub source_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub granularity: String,
    pub sequence_no: i32,
    pub token_count: i32,
    pub content_hash: String,
}
#[derive(Debug, Clone)]
pub struct IdempotentChunkReplay {
    pub fingerprint: String,
    pub complete: bool,
    pub chunks: Vec<StoredChunkRecord>,
}
#[derive(Debug, Clone)]
pub struct V004ChunkForEmbedding {
    pub access_zone_id: Uuid,
    pub document_id: Uuid,
    pub document_version: i64,
    pub root_chunk_id: Uuid,
    pub source_chunk_id: Uuid,
    pub parent_chunk_id: Option<Uuid>,
    pub chunk_id: Uuid,
    pub granularity: String,
    pub sequence_no: i32,
    pub token_count: i32,
    pub content_hash: String,
    pub content: String,
    pub access_level: i16,
    pub ttl_days: Option<i32>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct PreparedV004IndexEmbedding {
    pub chunk: GeneratedChunk,
    pub embedding: EmbeddingResult,
}

#[derive(Debug, Clone)]
pub struct V004IndexPersistenceSummary {
    pub chunks: Vec<StoredChunkRecord>,
    pub dense_vectors: u32,
    pub sparse_vectors: u32,
    pub bindings: u32,
    pub outbox_created: u32,
    pub graph_nodes: u32,
    pub graph_edges: u32,
    pub graph_warnings: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct ParentContextRecord {
    pub access_zone_id: Uuid,
    pub id: Uuid,
    pub document_id: Uuid,
    pub document_version: i64,
    pub root_chunk_id: Uuid,
    pub source_chunk_id: Uuid,
    pub access_level: i16,
    pub content: String,
    pub content_hash: String,
    pub token_count: i32,
    pub sequence_no: i32,
    pub source_block_id: Option<String>,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone)]
pub struct LexicalParentCandidate {
    pub parent: ParentContextRecord,
    pub lexical_score: f32,
    pub exact_match: bool,
    pub matched_terms: u32,
    pub matched_technical_terms: u32,
}
#[derive(Debug, Clone)]
pub struct HydratedSearchContext {
    pub access_zone_id: Uuid,
    pub matched_chunk_id: Uuid,
    pub parent_chunk_id: Uuid,
    pub document_id: Uuid,
    pub document_version: i64,
    pub root_chunk_id: Uuid,
    pub source_chunk_id: Uuid,
    pub matched_text: String,
    pub parent_text: String,
    pub parent_content_hash: String,
    pub parent_token_count: i32,
    pub parent_sequence_no: i32,
    pub access_level: i16,
    pub source_block_id: Option<String>,
    pub source_location: serde_json::Value,
    pub source_links: serde_json::Value,
    pub metadata: serde_json::Value,
    pub parent_metadata: serde_json::Value,
}
#[derive(Debug, Clone)]
pub struct ChunkTraceRecord {
    pub id: Uuid,
    pub source_block_id: Option<String>,
    pub source_location: serde_json::Value,
    pub source_links: serde_json::Value,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct DeletableQdrantPoints {
    pub deletable: Vec<Uuid>,
    pub skipped_legal_hold: Vec<Uuid>,
    pub orphan: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct GraphChunkContextRecord {
    pub chunk_id: Uuid,
    pub parent_chunk_id: Option<Uuid>,
    pub parent_record: ParentContextRecord,
    pub matched_text: String,
    pub trace: Option<ChunkTraceRecord>,
    pub qdrant_point_id: Option<Uuid>,
    pub representation_type: Option<String>,
    pub dense_version: Option<String>,
    pub model_version: Option<String>,
    pub payload_version: Option<i64>,
    pub source_chunk_granularity: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GraphSummaryRecord {
    pub total_nodes: u32,
    pub total_edges: u32,
    pub nodes_by_type: serde_json::Value,
    pub edges_by_relation_type: serde_json::Value,
    pub semantic_edges_count: u32,
    pub semantic_avg_weight: Option<f32>,
    pub semantic_min_weight: Option<f32>,
    pub semantic_max_weight: Option<f32>,
}

pub struct ChunkContentRecord {
    pub id: Uuid,
    pub root_chunk_id: Uuid,
    pub source_chunk_id: Uuid,
    pub parent_chunk_id: Option<Uuid>,
    pub granularity: String,
    pub sequence_no: i32,
    pub token_count: i32,
    pub content_hash: String,
    pub content: String,
}
impl Repository {
    pub async fn fetch_dense_embeddings_for_points(
        &self,
        access_zone_id: Uuid,
        qdrant_point_ids: &[Uuid],
        dense_representation_name: &str,
    ) -> Result<std::collections::HashMap<Uuid, Vec<f32>>, AstraError> {
        let mut result = std::collections::HashMap::new();
        if qdrant_point_ids.is_empty() {
            return Ok(result);
        }
        let rows = sqlx::query(
            "SELECT b.qdrant_point_id, ed.vector_value \
             FROM astravector.vector_bindings_v004 b \
             JOIN astravector.embedding_dense ed ON ed.cache_entry_id = b.cache_entry_id \
             JOIN astravector.embedding_cache_entries ce ON ce.id = b.cache_entry_id \
             JOIN astravector.content_chunks_v004 c ON c.access_zone_id=b.access_zone_id AND c.id=b.chunk_id \
             JOIN astravector.document_versions dv ON dv.access_zone_id=b.access_zone_id AND dv.document_id=b.document_id AND dv.document_version=b.document_version \
             WHERE b.access_zone_id = $1 \
               AND b.qdrant_point_id = ANY($2) \
               AND b.lifecycle_status = 'ACTIVE' \
               AND (b.expires_at IS NULL OR b.expires_at > now()) \
               AND c.lifecycle_status='ACTIVE' \
               AND c.deleted_at IS NULL \
               AND (c.expires_at IS NULL OR c.expires_at > now()) \
               AND dv.status='ACTIVE' AND dv.lifecycle_status='ACTIVE' \
               AND (dv.expires_at IS NULL OR dv.expires_at > now()) \
               AND ed.representation_name = $3 \
               AND ed.representation_version = COALESCE(ce.dense_version, ed.representation_version)"
        )
        .bind(access_zone_id)
        .bind(qdrant_point_ids)
        .bind(dense_representation_name)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for row in rows {
            let point_id: Uuid = row.get("qdrant_point_id");
            let vector: Vector = row.get("vector_value");
            result.insert(point_id, vector.to_vec());
        }
        Ok(result)
    }

    pub async fn fetch_dense_embeddings_for_chunks(
        &self,
        access_zone_id: Uuid,
        chunk_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Vec<f32>>, AstraError> {
        let mut result = std::collections::HashMap::new();
        if chunk_ids.is_empty() {
            return Ok(result);
        }
        let rows = sqlx::query(
            "SELECT DISTINCT ON (b.chunk_id) b.chunk_id, ed.vector_value \
             FROM astravector.vector_bindings_v004 b \
             JOIN astravector.embedding_dense ed ON ed.cache_entry_id = b.cache_entry_id \
             JOIN astravector.content_chunks_v004 c ON c.access_zone_id=b.access_zone_id AND c.id=b.chunk_id \
             JOIN astravector.document_versions dv ON dv.access_zone_id=b.access_zone_id AND dv.document_id=b.document_id AND dv.document_version=b.document_version \
             WHERE b.access_zone_id = $1 \
               AND b.chunk_id = ANY($2) \
               AND b.lifecycle_status = 'ACTIVE' \
               AND (b.expires_at IS NULL OR b.expires_at > now()) \
               AND c.lifecycle_status='ACTIVE' \
               AND c.deleted_at IS NULL \
               AND (c.expires_at IS NULL OR c.expires_at > now()) \
               AND dv.status='ACTIVE' AND dv.lifecycle_status='ACTIVE' \
               AND (dv.expires_at IS NULL OR dv.expires_at > now()) \
             ORDER BY b.chunk_id, b.updated_at DESC"
        )
        .bind(access_zone_id)
        .bind(chunk_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for row in rows {
            let chunk_id: Uuid = row.get("chunk_id");
            let vector: Vector = row.get("vector_value");
            result.insert(chunk_id, vector.to_vec());
        }
        Ok(result)
    }
    pub async fn fetch_dense_embeddings_for_points_multi(
        &self,
        access_zone_ids: &[Uuid],
        qdrant_point_ids: &[Uuid],
        dense_representation_name: &str,
    ) -> Result<std::collections::HashMap<(Uuid, Uuid), Vec<f32>>, AstraError> {
        let mut result = std::collections::HashMap::new();
        if access_zone_ids.is_empty() || qdrant_point_ids.is_empty() {
            return Ok(result);
        }
        let rows = sqlx::query(
            "SELECT b.access_zone_id, b.qdrant_point_id, ed.vector_value \
             FROM astravector.vector_bindings_v004 b \
             JOIN astravector.embedding_dense ed ON ed.cache_entry_id = b.cache_entry_id \
             JOIN astravector.embedding_cache_entries ce ON ce.id = b.cache_entry_id \
             JOIN astravector.content_chunks_v004 c ON c.access_zone_id=b.access_zone_id AND c.id=b.chunk_id \
             JOIN astravector.document_versions dv ON dv.access_zone_id=b.access_zone_id AND dv.document_id=b.document_id AND dv.document_version=b.document_version \
             WHERE b.access_zone_id = ANY($1::uuid[]) \
               AND b.qdrant_point_id = ANY($2::uuid[]) \
               AND b.lifecycle_status = 'ACTIVE' \
               AND (b.expires_at IS NULL OR b.expires_at > now()) \
               AND c.lifecycle_status='ACTIVE' AND c.deleted_at IS NULL \
               AND (c.expires_at IS NULL OR c.expires_at > now()) \
               AND dv.status='ACTIVE' AND dv.lifecycle_status='ACTIVE' \
               AND (dv.expires_at IS NULL OR dv.expires_at > now()) \
               AND ed.representation_name = $3 \
               AND ed.representation_version = COALESCE(ce.dense_version, ed.representation_version)"
        )
        .bind(access_zone_ids)
        .bind(qdrant_point_ids)
        .bind(dense_representation_name)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for row in rows {
            let zone: Uuid = row.get("access_zone_id");
            let point_id: Uuid = row.get("qdrant_point_id");
            let vector: Vector = row.get("vector_value");
            result.insert((zone, point_id), vector.to_vec());
        }
        Ok(result)
    }

    pub async fn fetch_dense_embeddings_for_chunks_multi(
        &self,
        access_zone_ids: &[Uuid],
        chunk_ids: &[Uuid],
        dense_representation_name: &str,
        dense_version: Option<&str>,
    ) -> Result<std::collections::HashMap<(Uuid, Uuid), Vec<f32>>, AstraError> {
        let mut result = std::collections::HashMap::new();
        if access_zone_ids.is_empty() || chunk_ids.is_empty() {
            return Ok(result);
        }
        let rows = sqlx::query(
            "SELECT DISTINCT ON (b.access_zone_id, b.chunk_id) b.access_zone_id, b.chunk_id, ed.vector_value \
             FROM astravector.vector_bindings_v004 b \
             JOIN astravector.embedding_dense ed ON ed.cache_entry_id = b.cache_entry_id \
             JOIN astravector.embedding_cache_entries ce ON ce.id = b.cache_entry_id \
             JOIN astravector.content_chunks_v004 c ON c.access_zone_id=b.access_zone_id AND c.id=b.chunk_id \
             JOIN astravector.document_versions dv ON dv.access_zone_id=b.access_zone_id AND dv.document_id=b.document_id AND dv.document_version=b.document_version \
             WHERE b.access_zone_id = ANY($1::uuid[]) \
               AND b.chunk_id = ANY($2::uuid[]) \
               AND b.lifecycle_status = 'ACTIVE' \
               AND (b.expires_at IS NULL OR b.expires_at > now()) \
               AND c.lifecycle_status='ACTIVE' AND c.deleted_at IS NULL \
               AND (c.expires_at IS NULL OR c.expires_at > now()) \
               AND dv.status='ACTIVE' AND dv.lifecycle_status='ACTIVE' \
               AND (dv.expires_at IS NULL OR dv.expires_at > now()) \
               AND ed.representation_name = $3 \
               AND ($4::text IS NULL OR ed.representation_version = $4::text OR ce.dense_version = $4::text) \
             ORDER BY b.access_zone_id, b.chunk_id, b.updated_at DESC"
        )
        .bind(access_zone_ids)
        .bind(chunk_ids)
        .bind(dense_representation_name)
        .bind(dense_version)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for row in rows {
            let zone: Uuid = row.get("access_zone_id");
            let chunk_id: Uuid = row.get("chunk_id");
            let vector: Vector = row.get("vector_value");
            result.insert((zone, chunk_id), vector.to_vec());
        }
        Ok(result)
    }

    pub async fn filter_visible_chunk_ids_multi(
        &self,
        access_zone_ids: &[Uuid],
        chunk_ids: &[Uuid],
        max_access_level: i16,
    ) -> Result<std::collections::HashSet<(Uuid, Uuid)>, AstraError> {
        let mut result = std::collections::HashSet::new();
        if access_zone_ids.is_empty() || chunk_ids.is_empty() {
            return Ok(result);
        }
        let rows = sqlx::query(
            r#"SELECT c.access_zone_id, c.id
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.id=ANY($2::uuid[])
  AND c.access_level <= $3
  AND c.lifecycle_status='ACTIVE'
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND c.deleted_at IS NULL
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())"#,
        )
        .bind(access_zone_ids)
        .bind(chunk_ids)
        .bind(max_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for row in rows {
            result.insert((
                row.get::<Uuid, _>("access_zone_id"),
                row.get::<Uuid, _>("id"),
            ));
        }
        Ok(result)
    }

    pub async fn fetch_qdrant_point_ids_for_document_deletion(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<Vec<Uuid>, AstraError> {
        let rows = sqlx::query(
            "SELECT qdrant_point_id \
             FROM astravector.vector_bindings_v004 \
             WHERE access_zone_id=$1 \
               AND document_id=$2 \
               AND document_version=$3 \
               AND lifecycle_status IN ('ACTIVE','EXPIRED','SUPERSEDED','DELETING','DELETE_FAILED') \
               AND COALESCE(legal_hold,false) = false \
               AND qdrant_point_id IS NOT NULL"
        )
        .bind(access_zone_id)
        .bind(document_id)
        .bind(document_version)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<Uuid, _>("qdrant_point_id"))
            .collect())
    }

    /// fix462: classify Qdrant reconciliation points against PostgreSQL bindings before deletion.
    /// A Qdrant point may be deleted only when it is an orphan projection or it belongs to a non-legal-hold binding
    /// that is already in a delete-compatible lifecycle. Legal-hold points are never returned here.
    pub async fn filter_deletable_qdrant_points_for_document(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        qdrant_point_ids: &[Uuid],
    ) -> Result<DeletableQdrantPoints, AstraError> {
        if qdrant_point_ids.is_empty() {
            return Ok(DeletableQdrantPoints::default());
        }
        let rows = sqlx::query(
            r#"WITH candidate(point_id) AS (
    SELECT unnest($4::uuid[])
), binding_state AS (
    SELECT c.point_id,
           b.legal_hold,
           b.lifecycle_status
    FROM candidate c
    LEFT JOIN astravector.vector_bindings_v004 b
      ON b.access_zone_id=$1
     AND b.document_id=$2
     AND b.document_version=$3
     AND b.qdrant_point_id=c.point_id
)
SELECT point_id,
       legal_hold,
       lifecycle_status
FROM binding_state"#,
        )
        .bind(access_zone_id)
        .bind(document_id)
        .bind(document_version)
        .bind(qdrant_point_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut result = DeletableQdrantPoints::default();
        for row in rows {
            let point_id: Uuid = row.get("point_id");
            let legal_hold: Option<bool> = row.get("legal_hold");
            let lifecycle_status: Option<String> = row.get("lifecycle_status");
            match (legal_hold.unwrap_or(false), lifecycle_status.as_deref()) {
                (true, _) => result.skipped_legal_hold.push(point_id),
                (_, None) => {
                    result.orphan.push(point_id);
                    result.deletable.push(point_id);
                }
                (
                    false,
                    Some("DELETING" | "DELETED" | "EXPIRED" | "SUPERSEDED" | "DELETE_FAILED"),
                ) => {
                    result.deletable.push(point_id);
                }
                _ => {}
            }
        }
        Ok(result)
    }

    pub async fn connect(c: &PostgresConfig) -> Result<Self, AstraError> {
        let (st, lt, idle) = (
            c.statement_timeout_ms,
            c.lock_timeout_ms,
            c.idle_in_transaction_session_timeout_ms,
        );
        let pool = PgPoolOptions::new()
            .max_connections(c.max_connections)
            .min_connections(c.min_connections)
            .acquire_timeout(Duration::from_millis(c.acquire_timeout_ms))
            .after_connect(move |conn, _| {
                Box::pin(async move {
                    sqlx::query("SELECT set_config('statement_timeout', $1, false)")
                        .bind(format!("{st}ms"))
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("SELECT set_config('lock_timeout', $1, false)")
                        .bind(format!("{lt}ms"))
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query(
                        "SELECT set_config('idle_in_transaction_session_timeout', $1, false)",
                    )
                    .bind(format!("{idle}ms"))
                    .execute(&mut *conn)
                    .await?;
                    Ok(())
                })
            })
            .connect(&c.url)
            .await
            .map_err(db)?;
        if c.auto_migrate {
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .map_err(|e| AstraError::Unavailable(format!("migrations: {e}")))?
        }
        Ok(Self { pool })
    }
    pub async fn ping(&self) -> Result<(), AstraError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
    pub async fn register_document_version(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        content_hash: &str,
        activation_policy: &str,
    ) -> Result<DocumentVersionRecord, AstraError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let status = "REGISTERED";
        let inserted = sqlx::query("INSERT INTO astravector.document_versions(access_zone_id,document_id,document_version,content_hash,status,activation_policy) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(access_zone_id,document_id,document_version) DO NOTHING RETURNING status")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .bind(content_hash)
            .bind(status)
            .bind(activation_policy)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if let Some(row) = inserted {
            smoke_failpoints::hit("required_after_document_version_update")?;
            tx.commit().await.map_err(db)?;
            return Ok(DocumentVersionRecord {
                document_id,
                document_version,
                status: row.get("status"),
            });
        }
        let row = sqlx::query("SELECT content_hash,status FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        let existing_hash: String = row.get("content_hash");
        let status: String = row.get("status");
        if !existing_hash.eq_ignore_ascii_case(content_hash) {
            return Err(AstraError::AlreadyExists(
                "document version already registered with different content_hash".into(),
            ));
        }
        smoke_failpoints::hit("required_after_document_version_update")?;
        tx.commit().await.map_err(db)?;
        Ok(DocumentVersionRecord {
            document_id,
            document_version,
            status,
        })
    }

    pub async fn mark_registered_document_version_failed(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        content_hash: &str,
        failure_code: &str,
        failure_message: &str,
    ) -> Result<(), AstraError> {
        sqlx::query(
            "UPDATE astravector.document_versions \
             SET status='FAILED', updated_at=now(), \
                 metadata=COALESCE(metadata,'{}'::jsonb) || jsonb_build_object( \
                   'indexing_failure', jsonb_build_object('code',$5,'message',$6,'recorded_at',now()::text) \
                 ) \
             WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 \
               AND content_hash=$4 AND status='REGISTERED'",
        )
        .bind(access_zone_id)
        .bind(document_id)
        .bind(document_version)
        .bind(content_hash)
        .bind(failure_code)
        .bind(failure_message)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    pub async fn force_activate_document_version(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        force_reason: &str,
    ) -> Result<DocumentVersionRecord, AstraError> {
        if force_reason.trim().is_empty() {
            return Err(AstraError::InvalidArgument(
                "force_reason is required".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        let exists = sqlx::query("SELECT 1 FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 FOR UPDATE")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .is_some();
        if !exists {
            return Err(AstraError::FailedPrecondition(
                "document version not found".into(),
            ));
        }
        let counts = sqlx::query(
            r#"SELECT
              (SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3) AS chunks,
              (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3) AS bindings"#,
        )
        .bind(access_zone_id)
        .bind(document_id)
        .bind(document_version)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if counts.get::<i64, _>("chunks") == 0 || counts.get::<i64, _>("bindings") == 0 {
            return Err(AstraError::FailedPrecondition(
                "force activation requires chunks and at least one binding".into(),
            ));
        }
        let updated = sqlx::query("UPDATE astravector.document_versions SET status='ACTIVE', lifecycle_status='ACTIVE', indexed_at=COALESCE(indexed_at, now()), activated_at=COALESCE(activated_at,now()), updated_at=now(),metadata=COALESCE(metadata,'{}'::jsonb) || jsonb_build_object('force_activation_reason',$4) WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND delete_operation_id IS NULL RETURNING document_id,document_version,status")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .bind(force_reason)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let Some(updated) = updated else {
            counter!("document_lifecycle_update_blocked_by_delete_operation_total", "operation" => "force_activate").increment(1);
            return Err(AstraError::FailedPrecondition(
                "force activation blocked by active delete_operation_id".into(),
            ));
        };
        tx.commit().await.map_err(db)?;
        Ok(DocumentVersionRecord {
            document_id: updated.get("document_id"),
            document_version: updated.get("document_version"),
            status: updated.get("status"),
        })
    }

    pub async fn activate_document_version(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<DocumentVersionRecord, AstraError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query("SELECT status,activation_policy FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 FOR UPDATE")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AstraError::FailedPrecondition("document version not found".into()))?;
        let status: String = row.get("status");
        if status != "REGISTERED" && status != "INDEXING" && status != "ACTIVE" {
            return Err(AstraError::FailedPrecondition(format!(
                "document version cannot be activated from status {status}"
            )));
        }
        let counts = sqlx::query(
            r#"SELECT
  (SELECT count(*) FROM astravector.content_chunks_v004 c WHERE c.access_zone_id=$1 AND c.document_id=$2 AND c.document_version=$3 AND c.granularity IN('PARENT','SUB_180','SUB_260') AND c.lifecycle_status='ACTIVE') AS chunks,
  (SELECT count(*) FROM astravector.embedding_cache_entries e JOIN astravector.vector_bindings_v004 b ON b.cache_entry_id=e.id WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND e.status='COMPLETED') AS embeddings,
  (SELECT count(*) FROM astravector.vector_bindings_v004 b WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND b.chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS bindings,
  (SELECT count(*) FROM astravector.vector_bindings_v004 b JOIN astravector.vector_outbox o ON o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND o.operation='UPSERT_POINT' AND o.operation_version=b.payload_version AND o.status='COMPLETED' WHERE b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND b.chunk_granularity IN('PARENT','SUB_180','SUB_260')) AS completed_outbox"#,
        )
        .bind(access_zone_id)
        .bind(document_id)
        .bind(document_version)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let chunks: i64 = counts.get("chunks");
        let embeddings: i64 = counts.get("embeddings");
        let bindings: i64 = counts.get("bindings");
        let completed_outbox: i64 = counts.get("completed_outbox");
        if chunks == 0 {
            return Err(AstraError::FailedPrecondition(
                "activation requires chunks".into(),
            ));
        }
        if embeddings < chunks {
            return Err(AstraError::FailedPrecondition(
                "activation requires completed embeddings for all chunks".into(),
            ));
        }
        if bindings < chunks {
            return Err(AstraError::FailedPrecondition(
                "activation requires vector bindings for all chunks".into(),
            ));
        }
        if completed_outbox < bindings {
            return Err(AstraError::FailedPrecondition(
                "activation requires completed outbox for all bindings".into(),
            ));
        }
        let activation_policy: String = row.get("activation_policy");
        if activation_policy == "ACTIVE_LATEST_ONLY" {
            sqlx::query("UPDATE astravector.document_versions SET status='SUPERSEDED', lifecycle_status='SUPERSEDED', expires_at=now(), updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version<>$3 AND status='ACTIVE' AND delete_operation_id IS NULL")
                .bind(access_zone_id)
                .bind(document_id)
                .bind(document_version)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let updated = sqlx::query("UPDATE astravector.document_versions SET status='ACTIVE', lifecycle_status='ACTIVE', indexed_at=COALESCE(indexed_at, now()), activated_at=COALESCE(activated_at,now()), updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND delete_operation_id IS NULL RETURNING document_id,document_version,status")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let Some(updated) = updated else {
            counter!("document_lifecycle_update_blocked_by_delete_operation_total", "operation" => "activate").increment(1);
            return Err(AstraError::FailedPrecondition(
                "activation blocked by active delete_operation_id".into(),
            ));
        };
        tx.commit().await.map_err(db)?;
        Ok(DocumentVersionRecord {
            document_id: updated.get("document_id"),
            document_version: updated.get("document_version"),
            status: updated.get("status"),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn store_v004_chunks(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        chunks: &[GeneratedChunk],
        tokenizer_version: &str,
        chunking_profile_version: &str,
        access_level: i16,
        ttl_days: Option<i32>,
        metadata: serde_json::Value,
    ) -> Result<Vec<StoredChunkRecord>, AstraError> {
        if chunks.is_empty() {
            return Err(AstraError::InvalidArgument("no chunks generated".into()));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        let processing_owner_id = format!(
            "{}:{}",
            std::env::var("HOSTNAME").unwrap_or_else(|_| "local".into()),
            Uuid::new_v4()
        );
        let n=sqlx::query("UPDATE astravector.document_versions SET status='INDEXING',processing_owner_id=$4,processing_started_at=COALESCE(processing_started_at,now()),processing_heartbeat_at=now(),updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND status IN('REGISTERED','FAILED') AND delete_operation_id IS NULL").bind(access_zone_id).bind(document_id).bind(document_version).bind(&processing_owner_id).execute(&mut*tx).await.map_err(db)?.rows_affected();
        if n != 1 {
            counter!("document_indexing_conflict_total").increment(1);
            return Err(AstraError::FailedPrecondition(
                "document version must be REGISTERED/FAILED and not already INDEXING".into(),
            ));
        }
        for chunk in chunks {
            let rows=sqlx::query("INSERT INTO astravector.content_chunks_v004(access_zone_id,id,root_chunk_id,source_chunk_id,parent_chunk_id,document_id,document_version,granularity,representation_type,sequence_no,target_token_count,actual_token_count,content,content_hash,tokenizer_version,chunking_profile_version,access_level,ttl_days,expires_at,metadata,source_block_id,source_location,source_links) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'ORIGINAL',$9,$10,$10,$11,$12,$13,$14,$15,$16,CASE WHEN $16 IS NULL THEN NULL ELSE now()+($16*interval '1 day') END,$17,$18,$19,$20) ON CONFLICT(access_zone_id,document_id,document_version,root_chunk_id,parent_chunk_id,granularity,representation_type,sequence_no) DO UPDATE SET content=EXCLUDED.content,content_hash=EXCLUDED.content_hash,actual_token_count=EXCLUDED.actual_token_count,tokenizer_version=EXCLUDED.tokenizer_version,chunking_profile_version=EXCLUDED.chunking_profile_version,access_level=EXCLUDED.access_level,ttl_days=EXCLUDED.ttl_days,expires_at=EXCLUDED.expires_at,metadata=EXCLUDED.metadata,source_block_id=EXCLUDED.source_block_id,source_location=EXCLUDED.source_location,source_links=EXCLUDED.source_links,updated_at=now()")
                .bind(access_zone_id)
                .bind(chunk.id)
                .bind(chunk.root_id)
                .bind(chunk.source_id)
                .bind(chunk.parent_id)
                .bind(document_id)
                .bind(document_version)
                .bind(chunk.granularity.as_db_str())
                .bind(chunk.sequence_no as i32)
                .bind(chunk.token_count as i32)
                .bind(&chunk.content)
                .bind(&chunk.content_hash)
                .bind(tokenizer_version)
                .bind(chunking_profile_version)
                .bind(access_level)
                .bind(ttl_days)
                .bind(metadata.clone())
                .bind(chunk.source_block_id.as_deref())
                .bind(chunk.source_location.clone())
                .bind(chunk.source_links.clone())
                .execute(&mut*tx).await.map_err(db)?.rows_affected();
            if rows != 1 {
                return Err(AstraError::Internal(
                    "chunk upsert did not affect exactly one row".into(),
                ));
            }
            for block_id in chunk.source_block_ids.iter() {
                sqlx::query("INSERT INTO astravector.logical_block_chunk_mapping(access_zone_id,document_id,document_version,block_id,chunk_id,relation_type,source_location,source_links) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(access_zone_id,document_id,document_version,block_id,chunk_id) DO UPDATE SET relation_type=EXCLUDED.relation_type,source_location=EXCLUDED.source_location,source_links=EXCLUDED.source_links")
                    .bind(access_zone_id)
                    .bind(document_id)
                    .bind(document_version)
                    .bind(block_id)
                    .bind(chunk.id)
                    .bind(&chunk.trace_relation_type)
                    .bind(chunk.source_location.clone())
                    .bind(chunk.source_links.clone())
                    .execute(&mut*tx)
                    .await
                    .map_err(db)?;
            }
            smoke_failpoints::hit("required_after_chunk_insert")?;
        }
        sqlx::query("UPDATE astravector.document_versions SET processing_owner_id=NULL,processing_started_at=NULL,processing_heartbeat_at=NULL,updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND processing_owner_id=$4")
            .bind(access_zone_id).bind(document_id).bind(document_version).bind(&processing_owner_id)
            .execute(&mut *tx).await.map_err(db)?;
        let rows=sqlx::query("SELECT id,root_chunk_id,source_chunk_id,parent_chunk_id,granularity,sequence_no,actual_token_count,content_hash FROM astravector.content_chunks_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND root_chunk_id=$4 AND representation_type='ORIGINAL' ORDER BY CASE granularity WHEN 'SOURCE' THEN 0 WHEN 'PARENT' THEN 1 WHEN 'SUB_180' THEN 2 WHEN 'SUB_260' THEN 3 ELSE 9 END,sequence_no")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .bind(chunks[0].root_id)
            .fetch_all(&mut*tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| StoredChunkRecord {
                id: r.get("id"),
                root_id: r.get("root_chunk_id"),
                source_id: r.get("source_chunk_id"),
                parent_id: r.try_get("parent_chunk_id").ok(),
                granularity: r.get("granularity"),
                sequence_no: r.get("sequence_no"),
                token_count: r.get("actual_token_count"),
                content_hash: r.get("content_hash"),
            })
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn persist_v004_index_transactionally(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        chunks: &[GeneratedChunk],
        embeddings: &[PreparedV004IndexEmbedding],
        tokenizer_version: &str,
        chunking_profile_version: &str,
        access_level: i16,
        ttl_days: Option<i32>,
        metadata: serde_json::Value,
        tenant: &str,
        workspace: &str,
        model_version: &str,
        dense_name: &str,
        dense_version: &str,
        sparse_name: &str,
        sparse_version: &str,
        min_weight: f32,
        max_non_zero: i32,
        qdrant_collection: &str,
        publish_outbox: bool,
        graph_enabled: bool,
        graph_limits: Option<GraphBuildLimits>,
        graph_bulk_insert_batch_size: usize,
        graph_failure_warn_and_continue: bool,
    ) -> Result<V004IndexPersistenceSummary, AstraError> {
        if chunks.is_empty() {
            return Err(AstraError::InvalidArgument("no chunks generated".into()));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        let processing_owner_id = format!(
            "{}:{}",
            std::env::var("HOSTNAME").unwrap_or_else(|_| "local".into()),
            Uuid::new_v4()
        );
        let n = sqlx::query("UPDATE astravector.document_versions SET status='INDEXING',processing_owner_id=$4,processing_started_at=COALESCE(processing_started_at,now()),processing_heartbeat_at=now(),updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND status IN('REGISTERED','FAILED') AND delete_operation_id IS NULL")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .bind(&processing_owner_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        if n != 1 {
            counter!("document_indexing_conflict_total").increment(1);
            return Err(AstraError::FailedPrecondition(
                "document version must be REGISTERED/FAILED and not already INDEXING".into(),
            ));
        }
        for chunk in chunks {
            let rows = sqlx::query("INSERT INTO astravector.content_chunks_v004(access_zone_id,id,root_chunk_id,source_chunk_id,parent_chunk_id,document_id,document_version,granularity,representation_type,sequence_no,target_token_count,actual_token_count,content,content_hash,tokenizer_version,chunking_profile_version,access_level,ttl_days,expires_at,metadata,source_block_id,source_location,source_links) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'ORIGINAL',$9,$10,$10,$11,$12,$13,$14,$15,$16,CASE WHEN $16 IS NULL THEN NULL ELSE now()+($16*interval '1 day') END,$17,$18,$19,$20) ON CONFLICT(access_zone_id,document_id,document_version,root_chunk_id,parent_chunk_id,granularity,representation_type,sequence_no) DO UPDATE SET content=EXCLUDED.content,content_hash=EXCLUDED.content_hash,actual_token_count=EXCLUDED.actual_token_count,tokenizer_version=EXCLUDED.tokenizer_version,chunking_profile_version=EXCLUDED.chunking_profile_version,access_level=EXCLUDED.access_level,ttl_days=EXCLUDED.ttl_days,expires_at=EXCLUDED.expires_at,metadata=EXCLUDED.metadata,source_block_id=EXCLUDED.source_block_id,source_location=EXCLUDED.source_location,source_links=EXCLUDED.source_links,updated_at=now()")
                .bind(access_zone_id)
                .bind(chunk.id)
                .bind(chunk.root_id)
                .bind(chunk.source_id)
                .bind(chunk.parent_id)
                .bind(document_id)
                .bind(document_version)
                .bind(chunk.granularity.as_db_str())
                .bind(chunk.sequence_no as i32)
                .bind(chunk.token_count as i32)
                .bind(&chunk.content)
                .bind(&chunk.content_hash)
                .bind(tokenizer_version)
                .bind(chunking_profile_version)
                .bind(access_level)
                .bind(ttl_days)
                .bind(metadata.clone())
                .bind(chunk.source_block_id.as_deref())
                .bind(chunk.source_location.clone())
                .bind(chunk.source_links.clone())
                .execute(&mut *tx)
                .await
                .map_err(db)?
                .rows_affected();
            if rows != 1 {
                return Err(AstraError::Internal(
                    "chunk upsert did not affect exactly one row".into(),
                ));
            }
            for block_id in chunk.source_block_ids.iter() {
                sqlx::query("INSERT INTO astravector.logical_block_chunk_mapping(access_zone_id,document_id,document_version,block_id,chunk_id,relation_type,source_location,source_links) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(access_zone_id,document_id,document_version,block_id,chunk_id) DO UPDATE SET relation_type=EXCLUDED.relation_type,source_location=EXCLUDED.source_location,source_links=EXCLUDED.source_links")
                    .bind(access_zone_id)
                    .bind(document_id)
                    .bind(document_version)
                    .bind(block_id)
                    .bind(chunk.id)
                    .bind(&chunk.trace_relation_type)
                    .bind(chunk.source_location.clone())
                    .bind(chunk.source_links.clone())
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
            }
            smoke_failpoints::hit("required_after_chunk_insert")?;
        }

        let mut dense_vectors = 0_u32;
        let mut sparse_vectors = 0_u32;
        let mut bindings = 0_u32;
        let mut outbox_created = 0_u32;

        for prepared in embeddings {
            let chunk = &prepared.chunk;
            let result = &prepared.embedding;
            let cache_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!(
                    "v004-cache:{}:{}:{}:{}:{}:{}:{}",
                    access_zone_id,
                    chunk.id,
                    chunk.content_hash,
                    model_version,
                    dense_version,
                    sparse_version,
                    tokenizer_version
                )
                .as_bytes(),
            );
            let binding_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("v004-binding:{}:{}", access_zone_id, chunk.id).as_bytes(),
            );
            let point_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("v004-qdrant-point:{access_zone_id}:{binding_id}").as_bytes(),
            );
            let mut hasher = Sha256::new();
            hasher.update(format!(
                "v004-cache-key:{tenant}:{workspace}:{}:{}:{}:{}:{}:{}:{}",
                access_zone_id,
                chunk.id,
                chunk.content_hash,
                model_version,
                dense_version,
                sparse_version,
                tokenizer_version
            ));
            let cache_key = format!("{:x}", hasher.finalize());
            let rows = sqlx::query("INSERT INTO astravector.embedding_cache_entries(id,tenant_id,workspace_id,cache_key,text_hash,purpose,chunk_type,tokenizer_version,model_version,dense_version,sparse_version,status,model_input_token_count,truncated,completed_at,last_accessed_at) VALUES($1,$2,$3,$4,$5,'DOCUMENT_CHUNK',2,$6,$7,$8,$9,'COMPLETED',$10,$11,now(),now()) ON CONFLICT(cache_key) DO UPDATE SET status='COMPLETED',model_input_token_count=EXCLUDED.model_input_token_count,truncated=EXCLUDED.truncated,completed_at=now(),last_accessed_at=now()")
                .bind(cache_id)
                .bind(tenant)
                .bind(workspace)
                .bind(&cache_key)
                .bind(&chunk.content_hash)
                .bind(tokenizer_version)
                .bind(model_version)
                .bind(dense_version)
                .bind(sparse_version)
                .bind(result.token_count as i32)
                .bind(result.truncated)
                .execute(&mut *tx)
                .await
                .map_err(db)?
                .rows_affected();
            if rows != 1 {
                return Err(AstraError::Internal(
                    "cache upsert did not affect one row".into(),
                ));
            }
            smoke_failpoints::hit("required_after_embedding_cache_insert")?;
            if let Some(v) = &result.dense {
                let rows = sqlx::query("INSERT INTO astravector.embedding_dense(id,cache_entry_id,representation_name,representation_version,dimension,normalized,distance,vector_value) VALUES($1,$2,$3,$4,$5,true,'COSINE',$6) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET vector_value=EXCLUDED.vector_value,created_at=now()")
                    .bind(Uuid::new_v4())
                    .bind(cache_id)
                    .bind(dense_name)
                    .bind(dense_version)
                    .bind(v.len() as i32)
                    .bind(Vector::from(v.clone()))
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?
                    .rows_affected();
                if rows != 1 {
                    return Err(AstraError::Internal(
                        "dense upsert did not affect one row".into(),
                    ));
                }
                dense_vectors += 1;
                smoke_failpoints::hit("required_after_dense_insert")?;
            }
            if let (Some(indices), Some(values)) = (&result.sparse_indices, &result.sparse_values) {
                if !indices.is_empty() && !values.is_empty() {
                    let idx: Vec<i32> = indices.iter().map(|x| *x as i32).collect();
                    let rows = sqlx::query("INSERT INTO astravector.embedding_sparse(id,cache_entry_id,representation_name,representation_version,indices,values,non_zero_count,min_weight,max_non_zero) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET indices=EXCLUDED.indices,values=EXCLUDED.values,non_zero_count=EXCLUDED.non_zero_count,created_at=now()")
                        .bind(Uuid::new_v4())
                        .bind(cache_id)
                        .bind(sparse_name)
                        .bind(sparse_version)
                        .bind(idx)
                        .bind(values)
                        .bind(indices.len() as i32)
                        .bind(min_weight)
                        .bind(max_non_zero)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?
                        .rows_affected();
                    if rows != 1 {
                        return Err(AstraError::Internal(
                            "sparse upsert did not affect one row".into(),
                        ));
                    }
                    sparse_vectors += 1;
                }
            }
            let mut binding_metadata = metadata.clone();
            if let Some(object) = binding_metadata.as_object_mut() {
                object.insert(
                    "chunking_profile_version".to_string(),
                    serde_json::Value::String(chunking_profile_version.to_string()),
                );
                if let Some(source_block_id) = &chunk.source_block_id {
                    object.insert(
                        "source_block_id".to_string(),
                        serde_json::Value::String(source_block_id.clone()),
                    );
                }
                object.insert(
                    "source_block_ids".to_string(),
                    serde_json::json!(chunk.source_block_ids),
                );
                object.insert("source_location".to_string(), chunk.source_location.clone());
                object.insert("source_links".to_string(), chunk.source_links.clone());
                object.insert(
                    "trace_relation_type".to_string(),
                    serde_json::Value::String(chunk.trace_relation_type.clone()),
                );
                object.insert(
                    "trace_quality".to_string(),
                    serde_json::Value::String(chunk.trace_quality.clone()),
                );
            }
            let binding_row = sqlx::query(r#"INSERT INTO astravector.vector_bindings_v004(access_zone_id,id,document_id,document_version,root_chunk_id,source_chunk_id,parent_chunk_id,chunk_id,chunk_granularity,representation_type,chunk_sequence_no,token_count,cache_entry_id,access_level,ttl_days,expires_at,qdrant_collection,qdrant_point_id,metadata)
VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'ORIGINAL',$10,$11,$12,$13,$14,CASE WHEN $14 IS NULL THEN NULL ELSE now()+($14*interval '1 day') END,$15,$16,$17)
ON CONFLICT(access_zone_id,document_id,document_version,chunk_id,representation_type) DO UPDATE SET
  cache_entry_id=EXCLUDED.cache_entry_id,
  token_count=EXCLUDED.token_count,
  access_level=EXCLUDED.access_level,
  ttl_days=EXCLUDED.ttl_days,
  expires_at=EXCLUDED.expires_at,
  qdrant_collection=EXCLUDED.qdrant_collection,
  qdrant_sync_status=CASE WHEN (
    astravector.vector_bindings_v004.cache_entry_id,
    astravector.vector_bindings_v004.token_count,
    astravector.vector_bindings_v004.access_level,
    astravector.vector_bindings_v004.ttl_days,
    astravector.vector_bindings_v004.qdrant_collection,
    astravector.vector_bindings_v004.metadata
  ) IS DISTINCT FROM (
    EXCLUDED.cache_entry_id,
    EXCLUDED.token_count,
    EXCLUDED.access_level,
    EXCLUDED.ttl_days,
    EXCLUDED.qdrant_collection,
    EXCLUDED.metadata
  ) THEN 'PENDING' ELSE astravector.vector_bindings_v004.qdrant_sync_status END,
  payload_version=CASE WHEN (
    astravector.vector_bindings_v004.cache_entry_id,
    astravector.vector_bindings_v004.token_count,
    astravector.vector_bindings_v004.access_level,
    astravector.vector_bindings_v004.ttl_days,
    astravector.vector_bindings_v004.qdrant_collection,
    astravector.vector_bindings_v004.metadata
  ) IS DISTINCT FROM (
    EXCLUDED.cache_entry_id,
    EXCLUDED.token_count,
    EXCLUDED.access_level,
    EXCLUDED.ttl_days,
    EXCLUDED.qdrant_collection,
    EXCLUDED.metadata
  ) THEN astravector.vector_bindings_v004.payload_version+1 ELSE astravector.vector_bindings_v004.payload_version END,
  metadata=EXCLUDED.metadata,
  updated_at=now()
RETURNING payload_version,qdrant_sync_status"#)
                .bind(access_zone_id)
                .bind(binding_id)
                .bind(document_id)
                .bind(document_version)
                .bind(chunk.root_id)
                .bind(chunk.source_id)
                .bind(chunk.parent_id)
                .bind(chunk.id)
                .bind(chunk.granularity.as_db_str())
                .bind(chunk.sequence_no as i32)
                .bind(chunk.token_count as i32)
                .bind(cache_id)
                .bind(access_level)
                .bind(ttl_days)
                .bind(qdrant_collection)
                .bind(point_id)
                .bind(binding_metadata)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
            bindings += 1;
            let payload_version: i64 = binding_row.get("payload_version");
            let qdrant_sync_status: String = binding_row.get("qdrant_sync_status");
            smoke_failpoints::hit("required_after_binding_insert")?;
            if publish_outbox
                && (qdrant_sync_status == "PENDING" || qdrant_sync_status == "UPDATE_PENDING")
            {
                let rows = sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'UPSERT_POINT',$4,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING")
                    .bind(Uuid::new_v4())
                    .bind(access_zone_id)
                    .bind(binding_id)
                    .bind(payload_version)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?
                    .rows_affected();
                if rows > 1 {
                    return Err(AstraError::Internal(
                        "outbox insert affected too many rows".into(),
                    ));
                }
                outbox_created += rows as u32;
                smoke_failpoints::hit("required_after_outbox_insert")?;
            }
        }
        let mut graph_nodes = 0_u32;
        let mut graph_edges = 0_u32;
        let mut graph_warnings = Vec::new();
        if graph_enabled {
            let limits = graph_limits.unwrap_or_default();
            sqlx::query("SAVEPOINT graph_build")
                .execute(&mut *tx)
                .await
                .map_err(db)?;

            let graph_rebuild_future = async {
                tracing::info!(
                    document_id = %document_id,
                    document_version = document_version,
                    access_zone_id = %access_zone_id,
                    "GRAPH_REBUILD_STARTED"
                );

                if limits.semantic_edges_enabled {
                    metrics::counter!("graph_semantic_parallel_enabled_total", "enabled" => limits.semantic_parallel_enabled.to_string()).increment(1);
                }

                self.cleanup_document_graph_tx(
                    &mut tx,
                    access_zone_id,
                    document_id,
                    document_version,
                )
                .await?;

                let mut build = graph::build_limited_structural_graph(
                    access_zone_id,
                    document_id,
                    document_version,
                    &metadata,
                    chunks,
                    access_level,
                    ttl_days,
                    &limits,
                );

                let semantic_inputs: Vec<ChunkEmbeddingForGraph> = embeddings
                    .iter()
                    .filter_map(|prepared| {
                        prepared
                            .embedding
                            .dense
                            .as_ref()
                            .map(|dense| ChunkEmbeddingForGraph {
                                chunk_id: prepared.chunk.id,
                                embedding: dense.clone(),
                            })
                    })
                    .collect();

                tracing::info!(
                    document_id = %document_id,
                    document_version = document_version,
                    access_zone_id = %access_zone_id,
                    chunk_count = semantic_inputs.len(),
                    parallel_enabled = limits.semantic_parallel_enabled,
                    "SEMANTIC_GRAPH_BUILD_STARTED"
                );

                let (semantic_edges, semantic_summary) = graph::build_semantic_edges_in_memory(
                    access_zone_id,
                    document_id,
                    document_version,
                    &build.nodes,
                    &semantic_inputs,
                    access_level,
                    ttl_days,
                    &limits,
                )
                .map_err(|e| AstraError::Internal(format!("graph semantic build failed: {e}")))?;

                metrics::counter!("graph_semantic_edges_created_total")
                    .increment(semantic_summary.semantic_edges_created as u64);
                metrics::counter!("graph_semantic_edges_skipped_by_score_total")
                    .increment(semantic_summary.semantic_edges_skipped_by_score as u64);
                metrics::counter!("graph_semantic_edges_skipped_by_limit_total")
                    .increment(semantic_summary.semantic_edges_skipped_by_limit as u64);
                metrics::counter!("graph_semantic_edges_skipped_duplicate_total")
                    .increment(semantic_summary.semantic_edges_skipped_duplicate as u64);
                metrics::histogram!("graph_semantic_build_duration_ms")
                    .record(semantic_summary.semantic_build_duration_ms as f64);
                if !semantic_summary.warnings.is_empty() {
                    metrics::counter!("graph_semantic_build_warnings_total")
                        .increment(semantic_summary.warnings.len() as u64);
                }

                tracing::info!(
                    document_id = %document_id,
                    document_version = document_version,
                    access_zone_id = %access_zone_id,
                    semantic_edges_created = semantic_summary.semantic_edges_created,
                    skipped_by_score = semantic_summary.semantic_edges_skipped_by_score,
                    skipped_by_limit = semantic_summary.semantic_edges_skipped_by_limit,
                    skipped_duplicate = semantic_summary.semantic_edges_skipped_duplicate,
                    duration_ms = semantic_summary.semantic_build_duration_ms as u64,
                    "SEMANTIC_GRAPH_BUILD_COMPLETED"
                );

                build.warnings.extend(semantic_summary.warnings.clone());
                build.warnings.push(format!(
                    "SEMANTIC_GRAPH_EDGES_CREATED:{}",
                    semantic_summary.semantic_edges_created
                ));
                build.edges.extend(semantic_edges);

                self.save_graph_nodes_edges_batch_tx(
                    &mut tx,
                    &build.nodes,
                    &build.edges,
                    graph_bulk_insert_batch_size.max(1),
                )
                .await?;
                let fixture_edges = self
                    .save_quality_fixture_relation_edges_tx(
                        &mut tx,
                        access_zone_id,
                        ttl_days,
                        graph_bulk_insert_batch_size.max(1),
                        &metadata,
                    )
                    .await?;
                if fixture_edges > 0 {
                    build.warnings.push(format!(
                        "QUALITY_FIXTURE_RELATION_EDGES_CREATED:{fixture_edges}"
                    ));
                }

                tracing::info!(
                    document_id = %document_id,
                    document_version = document_version,
                    access_zone_id = %access_zone_id,
                    graph_nodes = build.nodes.len(),
                    graph_edges = build.edges.len() + fixture_edges,
                    "GRAPH_REBUILD_COMPLETED"
                );

                Ok::<(u32, u32, Vec<String>), AstraError>((
                    build.nodes.len() as u32,
                    (build.edges.len() + fixture_edges) as u32,
                    build.warnings,
                ))
            };

            let strict_fail_indexing_policy = limits
                .semantic_large_document_policy
                .eq_ignore_ascii_case("FAIL_INDEXING");
            let warn_and_continue_for_graph_error =
                graph_failure_warn_and_continue && !strict_fail_indexing_policy;
            let graph_rebuild = tokio::time::timeout(
                Duration::from_millis(limits.semantic_rebuild_timeout_ms),
                graph_rebuild_future,
            )
            .await;
            match graph_rebuild {
                Ok(Ok((nodes, edges, warnings))) => {
                    graph_nodes = nodes;
                    graph_edges = edges;
                    graph_warnings.extend(warnings);
                    metrics::counter!("graph_rebuild_total").increment(1);
                    sqlx::query("RELEASE SAVEPOINT graph_build")
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                }
                Ok(Err(e)) if warn_and_continue_for_graph_error => {
                    metrics::counter!("graph_rebuild_failed_total").increment(1);
                    tracing::warn!(
                        document_id = %document_id,
                        document_version = document_version,
                        access_zone_id = %access_zone_id,
                        error = %e,
                        "GRAPH_REBUILD_FAILED"
                    );
                    sqlx::query("ROLLBACK TO SAVEPOINT graph_build")
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    sqlx::query("RELEASE SAVEPOINT graph_build")
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    graph_warnings.push(format!("GRAPH_BUILD_FAILED: {e}"));
                }
                Err(_) if warn_and_continue_for_graph_error => {
                    metrics::counter!("graph_rebuild_timeout_total").increment(1);
                    tracing::warn!(
                        document_id = %document_id,
                        document_version = document_version,
                        access_zone_id = %access_zone_id,
                        timeout_ms = limits.semantic_rebuild_timeout_ms,
                        "GRAPH_REBUILD_TIMEOUT"
                    );
                    sqlx::query("ROLLBACK TO SAVEPOINT graph_build")
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    sqlx::query("RELEASE SAVEPOINT graph_build")
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    graph_warnings.push("GRAPH_REBUILD_TIMEOUT".into());
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(AstraError::DeadlineExceeded(
                        "graph rebuild timed out".into(),
                    ))
                }
            }
        }
        smoke_failpoints::hit("required_before_commit")?;
        sqlx::query("UPDATE astravector.document_versions SET processing_owner_id=NULL,processing_started_at=NULL,processing_heartbeat_at=NULL,updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND processing_owner_id=$4")
            .bind(access_zone_id).bind(document_id).bind(document_version).bind(&processing_owner_id)
            .execute(&mut *tx).await.map_err(db)?;
        let rows = sqlx::query("SELECT id,root_chunk_id,source_chunk_id,parent_chunk_id,granularity,sequence_no,actual_token_count,content_hash FROM astravector.content_chunks_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND root_chunk_id=$4 AND representation_type='ORIGINAL' ORDER BY CASE granularity WHEN 'SOURCE' THEN 0 WHEN 'PARENT' THEN 1 WHEN 'SUB_180' THEN 2 WHEN 'SUB_260' THEN 3 ELSE 9 END,sequence_no")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .bind(chunks[0].root_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        smoke_failpoints::hit("required_after_commit_before_response")?;
        Ok(V004IndexPersistenceSummary {
            chunks: rows
                .into_iter()
                .map(|r| StoredChunkRecord {
                    id: r.get("id"),
                    root_id: r.get("root_chunk_id"),
                    source_id: r.get("source_chunk_id"),
                    parent_id: r.try_get("parent_chunk_id").ok(),
                    granularity: r.get("granularity"),
                    sequence_no: r.get("sequence_no"),
                    token_count: r.get("actual_token_count"),
                    content_hash: r.get("content_hash"),
                })
                .collect(),
            dense_vectors,
            sparse_vectors,
            bindings,
            outbox_created,
            graph_nodes,
            graph_edges,
            graph_warnings,
        })
    }

    pub async fn cleanup_document_graph_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<(), AstraError> {
        sqlx::query("DELETE FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        sqlx::query("DELETE FROM astravector.rag_graph_nodes WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        Ok(())
    }

    pub async fn save_graph_nodes_edges_batch_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
        batch_size: usize,
    ) -> Result<(), AstraError> {
        for batch in nodes.chunks(batch_size.max(1)) {
            let mut qb = QueryBuilder::new("INSERT INTO astravector.rag_graph_nodes(access_zone_id,node_id,node_type,external_id,document_id,document_version,chunk_id,block_id,label,properties,lifecycle_status,expires_at,quarantined,access_level) ");
            qb.push_values(batch, |mut b, n| {
                b.push_bind(n.access_zone_id)
                    .push_bind(n.node_id)
                    .push_bind(n.node_type.as_str())
                    .push_bind(&n.external_id)
                    .push_bind(n.document_id)
                    .push_bind(n.document_version)
                    .push_bind(n.chunk_id)
                    .push_bind(n.block_id.as_deref())
                    .push_bind(n.label.as_deref())
                    .push_bind(n.properties.clone())
                    .push_bind(&n.lifecycle_status)
                    .push_bind(n.expires_at)
                    .push_bind(n.quarantined)
                    .push_bind(n.access_level);
            });
            qb.push(" ON CONFLICT DO NOTHING");
            qb.build().execute(&mut **tx).await.map_err(db)?;
        }
        let started = std::time::Instant::now();
        for batch in edges.chunks(batch_size.max(1)) {
            let mut qb = QueryBuilder::new("INSERT INTO astravector.rag_graph_edges(access_zone_id,edge_id,source_node_type,source_node_id,target_node_type,target_node_id,relation_type,relation_score,relation_source,relation_rank,document_id,document_version,lifecycle_status,expires_at,quarantined,properties) ");
            qb.push_values(batch, |mut b, e| {
                b.push_bind(e.access_zone_id)
                    .push_bind(e.edge_id)
                    .push_bind(e.source_node_type.as_str())
                    .push_bind(e.source_node_id)
                    .push_bind(e.target_node_type.as_str())
                    .push_bind(e.target_node_id)
                    .push_bind(e.relation_type.as_str())
                    .push_bind(e.relation_score)
                    .push_bind(&e.relation_source)
                    .push_bind(e.relation_rank)
                    .push_bind(e.document_id)
                    .push_bind(e.document_version)
                    .push_bind(&e.lifecycle_status)
                    .push_bind(e.expires_at)
                    .push_bind(e.quarantined)
                    .push_bind(e.properties.clone());
            });
            qb.push(" ON CONFLICT (access_zone_id,relation_type,document_id,document_version,source_node_id,target_node_id) DO UPDATE SET relation_score=EXCLUDED.relation_score, relation_source=EXCLUDED.relation_source, relation_rank=EXCLUDED.relation_rank, properties=EXCLUDED.properties");
            qb.build().execute(&mut **tx).await.map_err(db)?;
        }
        metrics::counter!("graph_edges_batch_insert_total").increment(edges.len() as u64);
        metrics::histogram!("graph_edges_batch_insert_duration_ms")
            .record(started.elapsed().as_millis() as f64);
        for edge in edges {
            metrics::counter!("graph_edges_by_relation_persisted_total", "relation_type" => edge.relation_type.as_str().to_string()).increment(1);
        }
        Ok(())
    }

    async fn save_quality_fixture_relation_edges_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        access_zone_id: Uuid,
        ttl_days: Option<i32>,
        batch_size: usize,
        metadata: &serde_json::Value,
    ) -> Result<usize, AstraError> {
        let Some(relations_json) = metadata
            .get("quality_fixture_relations_json")
            .and_then(|value| value.as_str())
        else {
            return Ok(0);
        };
        let Ok(relations) = serde_json::from_str::<serde_json::Value>(relations_json) else {
            return Ok(0);
        };
        let Some(relations) = relations.as_array() else {
            return Ok(0);
        };

        let expires_at = ttl_days.and_then(|d| {
            if d > 0 {
                Some(Utc::now() + chrono::Duration::days(i64::from(d)))
            } else {
                None
            }
        });
        let mut edges = Vec::new();
        for relation in relations {
            let Some(relation_id) = relation.get("relation_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let raw_relation_type = relation
                .get("relation_type")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    AstraError::InvalidArgument(format!(
                        "quality fixture relation {relation_id} is missing relation_type"
                    ))
                })?;
            let relation_type =
                raw_relation_type
                    .parse::<GraphRelationType>()
                    .map_err(|error| {
                        metrics::counter!(
                            "graph_relation_type_rejected_total",
                            "relation_type" => raw_relation_type.to_string()
                        )
                        .increment(1);
                        AstraError::InvalidArgument(format!(
                            "quality fixture relation {relation_id}: {error}"
                        ))
                    })?;
            let quality_run_id = relation
                .get("quality_run_id")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AstraError::InvalidArgument(format!(
                        "quality fixture relation {relation_id} is missing quality_run_id"
                    ))
                })?;
            let Some(from_document_id) = relation
                .get("from_document_uuid")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
            else {
                continue;
            };
            let Some(to_document_id) = relation
                .get("to_document_uuid")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
            else {
                continue;
            };
            let Some(from_block_id) = relation.get("from_block_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(to_block_id) = relation.get("to_block_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let weight = relation
                .get("weight")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0)
                .clamp(0.0, 1.0) as f32;

            let pairs = sqlx::query(
                r#"
SELECT
  s_nodes.node_id AS source_node_id,
  t_nodes.node_id AS target_node_id,
  s_map.chunk_id AS source_chunk_id,
  t_map.chunk_id AS target_chunk_id
FROM astravector.logical_block_chunk_mapping s_map
JOIN astravector.content_chunks_v004 s_chunk
  ON s_chunk.access_zone_id=s_map.access_zone_id
 AND s_chunk.id=s_map.chunk_id
 AND s_chunk.lifecycle_status='ACTIVE'
 AND s_chunk.deleted_at IS NULL
 AND s_chunk.granularity IN ('PARENT','SUB_180','SUB_260')
 AND COALESCE(s_chunk.metadata->>'quality_run_id','')=$6
JOIN astravector.rag_graph_nodes_chunk s_nodes
  ON s_nodes.access_zone_id=s_map.access_zone_id
 AND s_nodes.chunk_id=s_map.chunk_id
 AND s_nodes.lifecycle_status='ACTIVE'
 AND s_nodes.quarantined=false
JOIN astravector.logical_block_chunk_mapping t_map
  ON t_map.access_zone_id=s_map.access_zone_id
 AND t_map.document_id=$4
 AND t_map.block_id=$5
JOIN astravector.content_chunks_v004 t_chunk
  ON t_chunk.access_zone_id=t_map.access_zone_id
 AND t_chunk.id=t_map.chunk_id
 AND t_chunk.lifecycle_status='ACTIVE'
 AND t_chunk.deleted_at IS NULL
 AND t_chunk.granularity IN ('PARENT','SUB_180','SUB_260')
 AND COALESCE(t_chunk.metadata->>'quality_run_id','')=$6
JOIN astravector.rag_graph_nodes_chunk t_nodes
  ON t_nodes.access_zone_id=t_map.access_zone_id
 AND t_nodes.chunk_id=t_map.chunk_id
 AND t_nodes.lifecycle_status='ACTIVE'
 AND t_nodes.quarantined=false
WHERE s_map.access_zone_id=$1
  AND s_map.document_id=$2
  AND s_map.block_id=$3
  AND s_map.document_version=1
  AND t_map.document_version=1
LIMIT 64
"#,
            )
            .bind(access_zone_id)
            .bind(from_document_id)
            .bind(from_block_id)
            .bind(to_document_id)
            .bind(to_block_id)
            .bind(quality_run_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(db)?;

            for (rank, row) in pairs.into_iter().enumerate() {
                let source_node_id: Uuid = row.get("source_node_id");
                let target_node_id: Uuid = row.get("target_node_id");
                let source_chunk_id: Uuid = row.get("source_chunk_id");
                let target_chunk_id: Uuid = row.get("target_chunk_id");
                if source_node_id == target_node_id {
                    continue;
                }
                let edge_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "astravector-quality-fixture-relation:{access_zone_id}:{relation_id}:{source_chunk_id}:{target_chunk_id}:{}",
                        relation_type.as_str()
                    )
                    .as_bytes(),
                );
                edges.push(GraphEdge {
                    access_zone_id,
                    edge_id,
                    source_node_type: crate::graph::GraphNodeType::Chunk,
                    source_node_id,
                    target_node_type: crate::graph::GraphNodeType::Chunk,
                    target_node_id,
                    relation_type,
                    relation_score: weight,
                    relation_source: "QUALITY_FIXTURE".into(),
                    relation_rank: Some(rank as i32 + 1),
                    document_id: Some(from_document_id),
                    document_version: Some(1),
                    lifecycle_status: "ACTIVE".into(),
                    expires_at,
                    quarantined: false,
                    properties: serde_json::json!({
                        "relation_id": relation_id,
                        "fixture_relation_type": relation_type.as_str(),
                        "from_document_id": relation.get("from_document_id").and_then(|v| v.as_str()).unwrap_or_default(),
                        "from_block_id": from_block_id,
                        "to_document_id": relation.get("to_document_id").and_then(|v| v.as_str()).unwrap_or_default(),
                        "to_block_id": to_block_id,
                        "source_chunk_id": source_chunk_id,
                        "target_chunk_id": target_chunk_id,
                        "quality_run_id": quality_run_id,
                        "quality_runtime_bench": relation.get("quality_runtime_bench").and_then(|v| v.as_str()).unwrap_or("fix475")
                    }),
                });
            }
        }

        let count = edges.len();
        if count > 0 {
            self.save_graph_nodes_edges_batch_tx(tx, &[], &edges, batch_size)
                .await?;
            metrics::counter!("graph_quality_fixture_relation_edges_persisted_total")
                .increment(count as u64);
        }
        Ok(count)
    }

    pub async fn expand_chunks_1hop(
        &self,
        access_zone_id: Uuid,
        seed_chunk_ids: &[Uuid],
        caller_access_level: i16,
        max_related_chunks: u32,
        max_seed_chunks: usize,
        max_edges_visited: usize,
        allowed_relations: &[String],
        quality_run_id: Option<&str>,
    ) -> Result<Vec<RelatedChunk>, AstraError> {
        if seed_chunk_ids.is_empty() || max_related_chunks == 0 {
            return Ok(Vec::new());
        }
        let started = std::time::Instant::now();
        let seed_chunk_ids = seed_chunk_ids
            .iter()
            .copied()
            .take(max_seed_chunks.max(1))
            .collect::<Vec<_>>();
        let allowed_relations = allowed_relations
            .iter()
            .map(|r| r.to_uppercase())
            .collect::<Vec<_>>();
        let rows = sqlx::query(r#"
WITH seed_input AS (
    SELECT input.chunk_id, input.seed_rank
    FROM UNNEST($2::uuid[]) WITH ORDINALITY AS input(chunk_id, seed_rank)
), seed_node_ids AS (
    SELECT n.access_zone_id, n.node_id, n.chunk_id AS seed_chunk_id, s.seed_rank
    FROM astravector.rag_graph_nodes_chunk n
    JOIN seed_input s ON s.chunk_id=n.chunk_id
    WHERE n.access_zone_id = $1
      AND n.lifecycle_status = 'ACTIVE'
      AND n.quarantined = false
      AND (n.expires_at IS NULL OR n.expires_at > now())
), edge_candidates AS (
    SELECT s.seed_rank,
           s.access_zone_id AS seed_access_zone_id,
           s.seed_chunk_id,
           e.access_zone_id,
           e.source_node_id,
           e.target_node_id,
           e.target_node_id AS related_node_id,
           e.relation_type,
           e.relation_score,
           e.relation_rank,
           e.relation_source
    FROM astravector.rag_graph_edges e
    JOIN seed_node_ids s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id
    WHERE e.access_zone_id=$1
      AND e.relation_type = ANY($5::text[])
      AND e.lifecycle_status='ACTIVE'
      AND e.quarantined=false
      AND (e.expires_at IS NULL OR e.expires_at > now())
      AND ($7::text IS NULL OR e.properties->>'quality_run_id'=$7)
    UNION ALL
    SELECT s.seed_rank,
           s.access_zone_id AS seed_access_zone_id,
           s.seed_chunk_id,
           e.access_zone_id,
           e.source_node_id,
           e.target_node_id,
           e.source_node_id AS related_node_id,
           e.relation_type,
           e.relation_score,
           e.relation_rank,
           e.relation_source
    FROM astravector.rag_graph_edges e
    JOIN seed_node_ids s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.target_node_id
    WHERE e.access_zone_id=$1
      AND e.relation_type = ANY($5::text[])
      AND e.lifecycle_status='ACTIVE'
      AND e.quarantined=false
      AND (e.expires_at IS NULL OR e.expires_at > now())
      AND ($7::text IS NULL OR e.properties->>'quality_run_id'=$7)
), expanded AS (
    SELECT *
    FROM edge_candidates
    ORDER BY seed_rank ASC,
             CASE WHEN relation_source='QUALITY_FIXTURE' THEN 0 ELSE 1 END,
             relation_score DESC,
             relation_rank NULLS LAST
    LIMIT $6
)
SELECT n.access_zone_id AS access_zone_id, n.chunk_id, expanded.seed_access_zone_id, expanded.seed_chunk_id, expanded.relation_type, expanded.relation_score, expanded.relation_rank
FROM expanded
JOIN astravector.rag_graph_nodes_chunk n ON n.access_zone_id=expanded.access_zone_id AND n.node_id=expanded.related_node_id
JOIN astravector.content_chunks_v004 c ON c.access_zone_id=n.access_zone_id AND c.id=n.chunk_id
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE n.lifecycle_status='ACTIVE'
  AND n.quarantined=false
  AND (n.expires_at IS NULL OR n.expires_at > now())
  AND c.lifecycle_status='ACTIVE'
  AND c.access_level <= $3
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND c.deleted_at IS NULL
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND COALESCE((c.metadata->>'quarantined')::boolean, false) = false
ORDER BY expanded.seed_rank ASC,
         CASE WHEN expanded.relation_source='QUALITY_FIXTURE' THEN 0 ELSE 1 END,
         expanded.relation_score DESC,
         expanded.relation_rank NULLS LAST
LIMIT $4
"#)
            .bind(access_zone_id)
            .bind(&seed_chunk_ids)
            .bind(caller_access_level)
            .bind(max_related_chunks as i64)
            .bind(&allowed_relations)
            .bind(max_edges_visited.max(max_related_chunks as usize) as i64)
            .bind(quality_run_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        tracing::debug!(
            seed_keys_count = seed_chunk_ids.len(),
            rows_count = rows.len(),
            quality_run_id = quality_run_id.unwrap_or(""),
            allowed_relations = ?allowed_relations,
            max_related_chunks,
            max_edges_visited,
            "GRAPH_EXPANSION_SQL_ROWS"
        );
        metrics::counter!("graph_expansion_requests_total").increment(1);
        metrics::counter!("graph_expansion_seed_chunks_total")
            .increment(seed_chunk_ids.len() as u64);
        metrics::counter!("graph_expansion_candidates_total").increment(rows.len() as u64);
        metrics::histogram!("graph_expansion_duration_ms")
            .record(started.elapsed().as_millis() as f64);
        let mut out = Vec::new();
        for row in rows {
            let raw_relation_type: String = row.get("relation_type");
            let relation = match raw_relation_type.parse::<GraphRelationType>() {
                Ok(relation) => relation,
                Err(error) => {
                    tracing::warn!(relation_type = %raw_relation_type, %error, "GRAPH_RELATION_TYPE_REJECTED");
                    metrics::counter!(
                        "graph_relation_type_rejected_total",
                        "relation_type" => raw_relation_type
                    )
                    .increment(1);
                    continue;
                }
            };
            out.push(RelatedChunk {
                access_zone_id: row.get("access_zone_id"),
                chunk_id: row.get("chunk_id"),
                seed_access_zone_id: row.get("seed_access_zone_id"),
                seed_chunk_id: row.get("seed_chunk_id"),
                relation_type: relation,
                relation_score: row.get::<f32, _>("relation_score"),
                relation_rank: row
                    .try_get::<Option<i32>, _>("relation_rank")
                    .ok()
                    .flatten(),
                hop_distance: 1,
            });
        }
        Ok(out)
    }

    pub async fn expand_chunks_1hop_multi(
        &self,
        access_zone_ids: &[Uuid],
        seed_chunk_ids: &[Uuid],
        caller_access_level: i16,
        max_related_chunks: u32,
        max_seed_chunks: usize,
        max_edges_visited: usize,
        allowed_relations: &[String],
        quality_run_id: Option<&str>,
    ) -> Result<Vec<RelatedChunk>, AstraError> {
        // Backward-compatible wrapper. Production retrieval should prefer
        // expand_chunks_1hop_by_seed_keys to avoid cartesian zone/chunk seed expansion.
        let seed_keys = access_zone_ids
            .iter()
            .flat_map(|zone| seed_chunk_ids.iter().map(move |chunk| (*zone, *chunk)))
            .collect::<Vec<_>>();
        self.expand_chunks_1hop_by_seed_keys(
            &seed_keys,
            caller_access_level,
            max_related_chunks,
            max_seed_chunks,
            max_edges_visited,
            allowed_relations,
            quality_run_id,
        )
        .await
    }

    pub async fn expand_chunks_1hop_by_seed_keys(
        &self,
        seed_keys: &[(Uuid, Uuid)],
        caller_access_level: i16,
        max_related_chunks: u32,
        max_seed_chunks: usize,
        max_edges_visited: usize,
        allowed_relations: &[String],
        quality_run_id: Option<&str>,
    ) -> Result<Vec<RelatedChunk>, AstraError> {
        if seed_keys.is_empty() || max_related_chunks == 0 {
            return Ok(Vec::new());
        }
        let started = std::time::Instant::now();
        let seed_keys = seed_keys
            .iter()
            .copied()
            .take(max_seed_chunks.max(1))
            .collect::<Vec<_>>();
        let seed_zone_ids = seed_keys.iter().map(|(zone, _)| *zone).collect::<Vec<_>>();
        let seed_chunk_ids = seed_keys
            .iter()
            .map(|(_, chunk)| *chunk)
            .collect::<Vec<_>>();
        let allowed_relations = allowed_relations
            .iter()
            .map(|r| r.to_uppercase())
            .collect::<Vec<_>>();
        let rows = sqlx::query(r#"
WITH seed_keys(access_zone_id, chunk_id, seed_rank) AS (
    SELECT input.access_zone_id, input.chunk_id, input.seed_rank
    FROM UNNEST($1::uuid[], $2::uuid[]) WITH ORDINALITY AS input(access_zone_id, chunk_id, seed_rank)
), seed_node_ids AS (
    SELECT n.access_zone_id, n.node_id, n.chunk_id AS seed_chunk_id, s.seed_rank
    FROM astravector.rag_graph_nodes_chunk n
    JOIN seed_keys s
      ON s.access_zone_id = n.access_zone_id
     AND s.chunk_id = n.chunk_id
    WHERE n.lifecycle_status = 'ACTIVE'
      AND n.quarantined = false
      AND (n.expires_at IS NULL OR n.expires_at > now())
), edge_candidates AS (
    SELECT s.seed_rank,
           s.access_zone_id AS seed_access_zone_id,
           s.seed_chunk_id,
           e.access_zone_id,
           e.source_node_id,
           e.target_node_id,
           e.target_node_id AS related_node_id,
           e.relation_type,
           e.relation_score,
           e.relation_rank,
           e.relation_source
    FROM astravector.rag_graph_edges e
    JOIN seed_node_ids s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id
    WHERE e.relation_type = ANY($5::text[])
      AND e.lifecycle_status='ACTIVE'
      AND e.quarantined=false
      AND (e.expires_at IS NULL OR e.expires_at > now())
      AND ($7::text IS NULL OR e.properties->>'quality_run_id'=$7)
    UNION ALL
    SELECT s.seed_rank,
           s.access_zone_id AS seed_access_zone_id,
           s.seed_chunk_id,
           e.access_zone_id,
           e.source_node_id,
           e.target_node_id,
           e.source_node_id AS related_node_id,
           e.relation_type,
           e.relation_score,
           e.relation_rank,
           e.relation_source
    FROM astravector.rag_graph_edges e
    JOIN seed_node_ids s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.target_node_id
    WHERE e.relation_type = ANY($5::text[])
      AND e.lifecycle_status='ACTIVE'
      AND e.quarantined=false
      AND (e.expires_at IS NULL OR e.expires_at > now())
      AND ($7::text IS NULL OR e.properties->>'quality_run_id'=$7)
), expanded AS (
    SELECT *
    FROM edge_candidates
    ORDER BY seed_rank ASC,
             CASE WHEN relation_source='QUALITY_FIXTURE' THEN 0 ELSE 1 END,
             relation_score DESC,
             relation_rank NULLS LAST
    LIMIT $6
)
SELECT n.access_zone_id AS access_zone_id,
       n.chunk_id,
       expanded.seed_access_zone_id,
       expanded.seed_chunk_id,
       expanded.relation_type,
       expanded.relation_score,
       expanded.relation_rank
FROM expanded
JOIN astravector.rag_graph_nodes_chunk n ON n.access_zone_id=expanded.access_zone_id AND n.node_id=expanded.related_node_id
JOIN astravector.content_chunks_v004 c ON c.access_zone_id=n.access_zone_id AND c.id=n.chunk_id
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE n.lifecycle_status='ACTIVE'
  AND n.quarantined=false
  AND (n.expires_at IS NULL OR n.expires_at > now())
  AND c.lifecycle_status='ACTIVE'
  AND c.access_level <= $3
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND c.deleted_at IS NULL
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND COALESCE((c.metadata->>'quarantined')::boolean, false) = false
ORDER BY expanded.seed_rank ASC,
         CASE WHEN expanded.relation_source='QUALITY_FIXTURE' THEN 0 ELSE 1 END,
         expanded.relation_score DESC,
         expanded.relation_rank NULLS LAST
LIMIT $4
"#)
            .bind(&seed_zone_ids)
            .bind(&seed_chunk_ids)
            .bind(caller_access_level)
            .bind(max_related_chunks as i64)
            .bind(&allowed_relations)
            .bind(max_edges_visited.max(max_related_chunks as usize) as i64)
            .bind(quality_run_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        tracing::debug!(
            seed_keys_count = seed_keys.len(),
            rows_count = rows.len(),
            quality_run_id = quality_run_id.unwrap_or(""),
            allowed_relations = ?allowed_relations,
            max_related_chunks,
            max_edges_visited,
            "GRAPH_EXPANSION_SQL_ROWS"
        );
        metrics::counter!("graph_expansion_requests_total").increment(1);
        metrics::counter!("graph_expansion_seed_chunks_total").increment(seed_keys.len() as u64);
        metrics::counter!("graph_expansion_candidates_total").increment(rows.len() as u64);
        metrics::histogram!("graph_expansion_duration_ms")
            .record(started.elapsed().as_millis() as f64);
        let mut out = Vec::new();
        for row in rows {
            let raw_relation_type: String = row.get("relation_type");
            let relation = match raw_relation_type.parse::<GraphRelationType>() {
                Ok(relation) => relation,
                Err(error) => {
                    tracing::warn!(relation_type = %raw_relation_type, %error, "GRAPH_RELATION_TYPE_REJECTED");
                    metrics::counter!(
                        "graph_relation_type_rejected_total",
                        "relation_type" => raw_relation_type
                    )
                    .increment(1);
                    continue;
                }
            };
            out.push(RelatedChunk {
                access_zone_id: row.get("access_zone_id"),
                chunk_id: row.get("chunk_id"),
                seed_access_zone_id: row.get("seed_access_zone_id"),
                seed_chunk_id: row.get("seed_chunk_id"),
                relation_type: relation,
                relation_score: row.get::<f32, _>("relation_score"),
                relation_rank: row
                    .try_get::<Option<i32>, _>("relation_rank")
                    .ok()
                    .flatten(),
                hop_distance: 1,
            });
        }
        Ok(out)
    }

    pub async fn fetch_contexts_for_graph_related_chunks(
        &self,
        access_zone_id: Uuid,
        chunk_ids: &[Uuid],
        caller_access_level: i16,
    ) -> Result<Vec<GraphChunkContextRecord>, AstraError> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"
SELECT DISTINCT ON (c.access_zone_id, c.id)
  c.id AS chunk_id,
  c.parent_chunk_id,
  c.content AS matched_text,
  c.source_block_id,
  c.source_location,
  c.source_links,
  c.metadata AS chunk_metadata,
  c.granularity AS source_chunk_granularity,
  b.qdrant_point_id,
  b.representation_type,
  ce.dense_version,
  ce.model_version,
  b.payload_version,
  p.access_zone_id AS p_access_zone_id,
  p.id AS p_id,
  p.document_id AS p_document_id,
  p.document_version AS p_document_version,
  p.root_chunk_id AS p_root_chunk_id,
  p.source_chunk_id AS p_source_chunk_id,
  p.access_level AS p_access_level,
  p.content AS p_content,
  p.content_hash AS p_content_hash,
  p.actual_token_count AS p_token_count,
  p.sequence_no AS p_sequence_no,
  p.source_block_id AS p_source_block_id,
  p.metadata AS p_metadata
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
 AND d.status='ACTIVE'
 AND d.lifecycle_status='ACTIVE'
 AND (d.expires_at IS NULL OR d.expires_at > now())
JOIN astravector.content_chunks_v004 p
  ON p.access_zone_id=c.access_zone_id
 AND p.id=COALESCE(c.parent_chunk_id,c.id)
 AND p.document_id=c.document_id
 AND p.document_version=c.document_version
 AND p.granularity='PARENT'
 AND p.lifecycle_status='ACTIVE'
 AND p.access_level <= $3
 AND (p.expires_at IS NULL OR p.expires_at > now())
 AND p.deleted_at IS NULL
LEFT JOIN astravector.vector_bindings_v004 b
  ON b.access_zone_id=c.access_zone_id
 AND b.chunk_id=c.id
 AND b.lifecycle_status IN ('ACTIVE','LEGAL_HOLD')
LEFT JOIN astravector.embedding_cache_entries ce
  ON ce.id=b.cache_entry_id
WHERE c.access_zone_id=$1
  AND c.id=ANY($2::uuid[])
  AND c.lifecycle_status='ACTIVE'
  AND c.access_level <= $3
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND c.deleted_at IS NULL
ORDER BY c.access_zone_id, c.id,
  CASE WHEN b.representation_type='ORIGINAL' THEN 0
       WHEN b.representation_type='SUMMARY' THEN 1
       WHEN b.representation_type='KEY_FACT' THEN 2
       WHEN b.representation_type='FAQ' THEN 3
       WHEN b.representation_type='SYNTHETIC_QUESTION' THEN 4
       ELSE 9 END,
  b.updated_at DESC NULLS LAST
"#,
        )
        .bind(access_zone_id)
        .bind(chunk_ids)
        .bind(caller_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut out = Vec::new();
        for r in rows {
            let trace = ChunkTraceRecord {
                id: r.get("chunk_id"),
                source_block_id: r
                    .try_get::<Option<String>, _>("source_block_id")
                    .ok()
                    .flatten(),
                source_location: r
                    .try_get("source_location")
                    .unwrap_or_else(|_| serde_json::json!({})),
                source_links: r
                    .try_get("source_links")
                    .unwrap_or_else(|_| serde_json::json!([])),
                metadata: r
                    .try_get("chunk_metadata")
                    .unwrap_or_else(|_| serde_json::json!({})),
            };
            out.push(GraphChunkContextRecord {
                chunk_id: r.get("chunk_id"),
                parent_chunk_id: r
                    .try_get::<Option<Uuid>, _>("parent_chunk_id")
                    .ok()
                    .flatten(),
                matched_text: r.get("matched_text"),
                trace: Some(trace),
                qdrant_point_id: r
                    .try_get::<Option<Uuid>, _>("qdrant_point_id")
                    .ok()
                    .flatten(),
                representation_type: r
                    .try_get::<Option<String>, _>("representation_type")
                    .ok()
                    .flatten(),
                dense_version: r
                    .try_get::<Option<String>, _>("dense_version")
                    .ok()
                    .flatten(),
                model_version: r
                    .try_get::<Option<String>, _>("model_version")
                    .ok()
                    .flatten(),
                payload_version: r
                    .try_get::<Option<i64>, _>("payload_version")
                    .ok()
                    .flatten(),
                source_chunk_granularity: r
                    .try_get::<Option<String>, _>("source_chunk_granularity")
                    .ok()
                    .flatten(),
                parent_record: ParentContextRecord {
                    access_zone_id: r.get("p_access_zone_id"),
                    id: r.get("p_id"),
                    document_id: r.get("p_document_id"),
                    document_version: r.get("p_document_version"),
                    root_chunk_id: r.get("p_root_chunk_id"),
                    source_chunk_id: r.get("p_source_chunk_id"),
                    access_level: r.get("p_access_level"),
                    content: r.get("p_content"),
                    content_hash: r.get("p_content_hash"),
                    token_count: r.get("p_token_count"),
                    sequence_no: r
                        .try_get::<i32, _>("p_sequence_no")
                        .ok()
                        .unwrap_or_default(),
                    source_block_id: r
                        .try_get::<Option<String>, _>("p_source_block_id")
                        .ok()
                        .flatten(),
                    metadata: r.get("p_metadata"),
                },
            });
        }
        Ok(out)
    }

    pub async fn fetch_contexts_for_graph_related_chunks_multi(
        &self,
        access_zone_ids: &[Uuid],
        chunk_ids: &[Uuid],
        caller_access_level: i16,
    ) -> Result<Vec<GraphChunkContextRecord>, AstraError> {
        if chunk_ids.is_empty() || access_zone_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"
SELECT DISTINCT ON (c.access_zone_id, c.id)
  c.id AS chunk_id,
  c.parent_chunk_id,
  c.content AS matched_text,
  c.source_block_id,
  c.source_location,
  c.source_links,
  c.metadata AS chunk_metadata,
  c.granularity AS source_chunk_granularity,
  b.qdrant_point_id,
  b.representation_type,
  ce.dense_version,
  ce.model_version,
  b.payload_version,
  p.access_zone_id AS p_access_zone_id,
  p.id AS p_id,
  p.document_id AS p_document_id,
  p.document_version AS p_document_version,
  p.root_chunk_id AS p_root_chunk_id,
  p.source_chunk_id AS p_source_chunk_id,
  p.access_level AS p_access_level,
  p.content AS p_content,
  p.content_hash AS p_content_hash,
  p.actual_token_count AS p_token_count,
  p.sequence_no AS p_sequence_no,
  p.source_block_id AS p_source_block_id,
  p.metadata AS p_metadata
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
 AND d.status='ACTIVE'
 AND d.lifecycle_status='ACTIVE'
 AND (d.expires_at IS NULL OR d.expires_at > now())
JOIN astravector.content_chunks_v004 p
  ON p.access_zone_id=c.access_zone_id
 AND p.id=COALESCE(c.parent_chunk_id,c.id)
 AND p.document_id=c.document_id
 AND p.document_version=c.document_version
 AND p.granularity='PARENT'
 AND p.lifecycle_status='ACTIVE'
 AND p.access_level <= $3
 AND (p.expires_at IS NULL OR p.expires_at > now())
 AND p.deleted_at IS NULL
LEFT JOIN astravector.vector_bindings_v004 b
  ON b.access_zone_id=c.access_zone_id
 AND b.chunk_id=c.id
 AND b.lifecycle_status IN ('ACTIVE','LEGAL_HOLD')
LEFT JOIN astravector.embedding_cache_entries ce
  ON ce.id=b.cache_entry_id
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.id=ANY($2::uuid[])
  AND c.lifecycle_status='ACTIVE'
  AND c.access_level <= $3
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND c.deleted_at IS NULL
ORDER BY c.access_zone_id, c.id,
  CASE WHEN b.representation_type='ORIGINAL' THEN 0
       WHEN b.representation_type='SUMMARY' THEN 1
       WHEN b.representation_type='KEY_FACT' THEN 2
       WHEN b.representation_type='FAQ' THEN 3
       WHEN b.representation_type='SYNTHETIC_QUESTION' THEN 4
       ELSE 9 END,
  b.updated_at DESC NULLS LAST
"#,
        )
        .bind(access_zone_ids)
        .bind(chunk_ids)
        .bind(caller_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut out = Vec::new();
        for r in rows {
            let trace = ChunkTraceRecord {
                id: r.get("chunk_id"),
                source_block_id: r
                    .try_get::<Option<String>, _>("source_block_id")
                    .ok()
                    .flatten(),
                source_location: r
                    .try_get("source_location")
                    .unwrap_or_else(|_| serde_json::json!({})),
                source_links: r
                    .try_get("source_links")
                    .unwrap_or_else(|_| serde_json::json!([])),
                metadata: r
                    .try_get("chunk_metadata")
                    .unwrap_or_else(|_| serde_json::json!({})),
            };
            out.push(GraphChunkContextRecord {
                chunk_id: r.get("chunk_id"),
                parent_chunk_id: r
                    .try_get::<Option<Uuid>, _>("parent_chunk_id")
                    .ok()
                    .flatten(),
                matched_text: r.get("matched_text"),
                trace: Some(trace),
                qdrant_point_id: r
                    .try_get::<Option<Uuid>, _>("qdrant_point_id")
                    .ok()
                    .flatten(),
                representation_type: r
                    .try_get::<Option<String>, _>("representation_type")
                    .ok()
                    .flatten(),
                dense_version: r
                    .try_get::<Option<String>, _>("dense_version")
                    .ok()
                    .flatten(),
                model_version: r
                    .try_get::<Option<String>, _>("model_version")
                    .ok()
                    .flatten(),
                payload_version: r
                    .try_get::<Option<i64>, _>("payload_version")
                    .ok()
                    .flatten(),
                source_chunk_granularity: r
                    .try_get::<Option<String>, _>("source_chunk_granularity")
                    .ok()
                    .flatten(),
                parent_record: ParentContextRecord {
                    access_zone_id: r.get("p_access_zone_id"),
                    id: r.get("p_id"),
                    document_id: r.get("p_document_id"),
                    document_version: r.get("p_document_version"),
                    root_chunk_id: r.get("p_root_chunk_id"),
                    source_chunk_id: r.get("p_source_chunk_id"),
                    access_level: r.get("p_access_level"),
                    content: r.get("p_content"),
                    content_hash: r.get("p_content_hash"),
                    token_count: r.get("p_token_count"),
                    sequence_no: r
                        .try_get::<i32, _>("p_sequence_no")
                        .ok()
                        .unwrap_or_default(),
                    source_block_id: r
                        .try_get::<Option<String>, _>("p_source_block_id")
                        .ok()
                        .flatten(),
                    metadata: r.get("p_metadata"),
                },
            });
        }
        Ok(out)
    }

    pub async fn fetch_graph_summary(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<GraphSummaryRecord, AstraError> {
        let row = sqlx::query(r#"
SELECT
  (SELECT count(*) FROM astravector.rag_graph_nodes WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3) AS total_nodes,
  (SELECT count(*) FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3) AS total_edges,
  COALESCE((SELECT jsonb_object_agg(node_type, cnt) FROM (SELECT node_type, count(*) cnt FROM astravector.rag_graph_nodes WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 GROUP BY node_type) s),'{}'::jsonb) AS nodes_by_type,
  COALESCE((SELECT jsonb_object_agg(relation_type, cnt) FROM (SELECT relation_type, count(*) cnt FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 GROUP BY relation_type) s),'{}'::jsonb) AS edges_by_relation_type,
  (SELECT count(*) FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND relation_type='CHUNK_SEMANTIC_SIMILAR') AS semantic_edges_count,
  (SELECT avg(relation_score) FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND relation_type='CHUNK_SEMANTIC_SIMILAR') AS semantic_avg_weight,
  (SELECT min(relation_score) FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND relation_type='CHUNK_SEMANTIC_SIMILAR') AS semantic_min_weight,
  (SELECT max(relation_score) FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND relation_type='CHUNK_SEMANTIC_SIMILAR') AS semantic_max_weight
"#)
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(GraphSummaryRecord {
            total_nodes: row.get::<i64, _>("total_nodes") as u32,
            total_edges: row.get::<i64, _>("total_edges") as u32,
            nodes_by_type: row.get("nodes_by_type"),
            edges_by_relation_type: row.get("edges_by_relation_type"),
            semantic_edges_count: row.get::<i64, _>("semantic_edges_count") as u32,
            semantic_avg_weight: row
                .try_get::<Option<f32>, _>("semantic_avg_weight")
                .ok()
                .flatten(),
            semantic_min_weight: row
                .try_get::<Option<f32>, _>("semantic_min_weight")
                .ok()
                .flatten(),
            semantic_max_weight: row
                .try_get::<Option<f32>, _>("semantic_max_weight")
                .ok()
                .flatten(),
        })
    }

    pub async fn fetch_v004_chunks_by_idempotency_key(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
        idempotency_key: &str,
    ) -> Result<Option<IdempotentChunkReplay>, AstraError> {
        let rows=sqlx::query("SELECT id,root_chunk_id,source_chunk_id,parent_chunk_id,granularity,sequence_no,actual_token_count,content_hash,metadata->>'idempotency_fingerprint' AS fingerprint FROM astravector.content_chunks_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND metadata->>'idempotency_key'=$4 AND representation_type='ORIGINAL' ORDER BY CASE granularity WHEN 'SOURCE' THEN 0 WHEN 'PARENT' THEN 1 WHEN 'SUB_180' THEN 2 WHEN 'SUB_260' THEN 3 ELSE 9 END,sequence_no")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .bind(idempotency_key)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let fingerprint: String = rows[0].get("fingerprint");
        let searchable_chunks = rows
            .iter()
            .filter(|r| {
                let granularity: String = r.get("granularity");
                granularity != "SOURCE"
            })
            .count() as i64;
        let binding_count = sqlx::query("SELECT count(*) AS count FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND chunk_id=ANY($4) AND representation_type='ORIGINAL'")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .bind(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect::<Vec<_>>())
            .fetch_one(&self.pool)
            .await
            .map_err(db)?
            .get::<i64, _>("count");
        Ok(Some(IdempotentChunkReplay {
            fingerprint,
            complete: searchable_chunks > 0 && binding_count >= searchable_chunks,
            chunks: rows
                .into_iter()
                .map(|r| StoredChunkRecord {
                    id: r.get("id"),
                    root_id: r.get("root_chunk_id"),
                    source_id: r.get("source_chunk_id"),
                    parent_id: r.try_get("parent_chunk_id").ok(),
                    granularity: r.get("granularity"),
                    sequence_no: r.get("sequence_no"),
                    token_count: r.get("actual_token_count"),
                    content_hash: r.get("content_hash"),
                })
                .collect(),
        }))
    }

    pub async fn fetch_parent_contexts(
        &self,
        access_zone_id: Uuid,
        parent_ids: &[Uuid],
        caller_access_level: i16,
    ) -> Result<Vec<ParentContextRecord>, AstraError> {
        if parent_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"SELECT c.access_zone_id,c.id,c.document_id,c.document_version,c.root_chunk_id,c.source_chunk_id,c.access_level,c.content,c.content_hash,c.actual_token_count,c.sequence_no,c.source_block_id,c.metadata
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=$1
  AND c.id=ANY($2)
  AND c.granularity='PARENT'
  AND c.representation_type='ORIGINAL'
  AND c.access_level <= $3
  AND c.lifecycle_status='ACTIVE'
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND (c.expires_at IS NULL OR c.expires_at > now())"#,
        )
        .bind(access_zone_id)
        .bind(parent_ids)
        .bind(caller_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| ParentContextRecord {
                access_zone_id: r.get("access_zone_id"),
                id: r.get("id"),
                document_id: r.get("document_id"),
                document_version: r.get("document_version"),
                root_chunk_id: r.get("root_chunk_id"),
                source_chunk_id: r.get("source_chunk_id"),
                access_level: r.get("access_level"),
                content: r.get("content"),
                content_hash: r.get("content_hash"),
                token_count: r.get("actual_token_count"),
                sequence_no: r.get("sequence_no"),
                source_block_id: r
                    .try_get::<Option<String>, _>("source_block_id")
                    .ok()
                    .flatten(),
                metadata: r.get("metadata"),
            })
            .collect())
    }

    pub async fn fetch_parent_contexts_multi(
        &self,
        access_zone_ids: &[Uuid],
        parent_ids: &[Uuid],
        caller_access_level: i16,
    ) -> Result<Vec<ParentContextRecord>, AstraError> {
        if parent_ids.is_empty() || access_zone_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"SELECT c.access_zone_id,c.id,c.document_id,c.document_version,c.root_chunk_id,c.source_chunk_id,c.access_level,c.content,c.content_hash,c.actual_token_count,c.sequence_no,c.source_block_id,c.metadata
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.id=ANY($2::uuid[])
  AND c.granularity='PARENT'
  AND c.representation_type='ORIGINAL'
  AND c.access_level <= $3
  AND c.lifecycle_status='ACTIVE'
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND (c.expires_at IS NULL OR c.expires_at > now())"#,
        )
        .bind(access_zone_ids)
        .bind(parent_ids)
        .bind(caller_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| ParentContextRecord {
                access_zone_id: r.get("access_zone_id"),
                id: r.get("id"),
                document_id: r.get("document_id"),
                document_version: r.get("document_version"),
                root_chunk_id: r.get("root_chunk_id"),
                source_chunk_id: r.get("source_chunk_id"),
                access_level: r.get("access_level"),
                content: r.get("content"),
                content_hash: r.get("content_hash"),
                token_count: r.get("actual_token_count"),
                sequence_no: r.get("sequence_no"),
                source_block_id: r
                    .try_get::<Option<String>, _>("source_block_id")
                    .ok()
                    .flatten(),
                metadata: r.get("metadata"),
            })
            .collect())
    }

    pub async fn fetch_parent_contexts_multi_with_timeout(
        &self,
        access_zone_ids: &[Uuid],
        parent_ids: &[Uuid],
        caller_access_level: i16,
        statement_timeout_ms: u64,
    ) -> Result<Vec<ParentContextRecord>, AstraError> {
        if parent_ids.is_empty() || access_zone_ids.is_empty() {
            return Ok(Vec::new());
        }
        if statement_timeout_ms == 0 {
            return Err(AstraError::DeadlineExceeded(
                "insufficient_postgres_budget".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(postgres_error)?;
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(format!("{statement_timeout_ms}ms"))
            .execute(&mut *tx)
            .await
            .map_err(postgres_error)?;
        let rows = sqlx::query(
            r#"SELECT c.access_zone_id,c.id,c.document_id,c.document_version,c.root_chunk_id,c.source_chunk_id,c.access_level,c.content,c.content_hash,c.actual_token_count,c.sequence_no,c.source_block_id,c.metadata
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.id=ANY($2::uuid[])
  AND c.granularity='PARENT'
  AND c.representation_type='ORIGINAL'
  AND c.access_level <= $3
  AND c.lifecycle_status='ACTIVE'
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND (c.expires_at IS NULL OR c.expires_at > now())"#,
        )
        .bind(access_zone_ids)
        .bind(parent_ids)
        .bind(caller_access_level)
        .fetch_all(&mut *tx)
        .await
        .map_err(postgres_error)?;
        tx.commit().await.map_err(postgres_error)?;
        Ok(rows
            .into_iter()
            .map(|r| ParentContextRecord {
                access_zone_id: r.get("access_zone_id"),
                id: r.get("id"),
                document_id: r.get("document_id"),
                document_version: r.get("document_version"),
                root_chunk_id: r.get("root_chunk_id"),
                source_chunk_id: r.get("source_chunk_id"),
                access_level: r.get("access_level"),
                content: r.get("content"),
                content_hash: r.get("content_hash"),
                token_count: r.get("actual_token_count"),
                sequence_no: r.get("sequence_no"),
                source_block_id: r
                    .try_get::<Option<String>, _>("source_block_id")
                    .ok()
                    .flatten(),
                metadata: r.get("metadata"),
            })
            .collect())
    }

    pub async fn fetch_hydrated_search_contexts_multi(
        &self,
        candidates: &[(Uuid, Uuid, Uuid)],
        caller_access_level: i16,
        statement_timeout_ms: u64,
    ) -> Result<Vec<HydratedSearchContext>, AstraError> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        if statement_timeout_ms == 0 {
            return Err(AstraError::DeadlineExceeded(
                "insufficient_postgres_hydration_budget".into(),
            ));
        }
        let zones = candidates.iter().map(|value| value.0).collect::<Vec<_>>();
        let matched = candidates.iter().map(|value| value.1).collect::<Vec<_>>();
        let parents = candidates.iter().map(|value| value.2).collect::<Vec<_>>();
        let mut tx = self.pool.begin().await.map_err(postgres_error)?;
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(format!("{statement_timeout_ms}ms"))
            .execute(&mut *tx)
            .await
            .map_err(postgres_error)?;
        let rows = sqlx::query(
            r#"WITH candidate_keys AS (
  SELECT * FROM unnest($1::uuid[], $2::uuid[], $3::uuid[])
    WITH ORDINALITY AS keys(access_zone_id, matched_chunk_id, parent_chunk_id, rank)
)
SELECT keys.rank, p.access_zone_id, m.id AS matched_chunk_id, p.id AS parent_chunk_id,
       p.document_id, p.document_version, p.root_chunk_id, p.source_chunk_id,
       m.content AS matched_text, p.content AS parent_text,
       p.content_hash AS parent_content_hash, p.actual_token_count AS parent_token_count,
       p.sequence_no AS parent_sequence_no, p.access_level,
       COALESCE(m.source_block_id,p.source_block_id) AS source_block_id,
       COALESCE(m.source_location,'{}'::jsonb) AS source_location,
       COALESCE(m.source_links,'[]'::jsonb) AS source_links,
       m.metadata, p.metadata AS parent_metadata
FROM candidate_keys keys
JOIN astravector.content_chunks_v004 m
  ON m.access_zone_id=keys.access_zone_id AND m.id=keys.matched_chunk_id
JOIN astravector.content_chunks_v004 p
  ON p.access_zone_id=keys.access_zone_id AND p.id=keys.parent_chunk_id
JOIN astravector.document_versions d
  ON d.access_zone_id=p.access_zone_id
 AND d.document_id=p.document_id
 AND d.document_version=p.document_version
WHERE m.document_id=p.document_id
  AND m.document_version=p.document_version
  AND p.granularity='PARENT'
  AND p.representation_type='ORIGINAL'
  AND m.access_level <= $4 AND p.access_level <= $4
  AND m.lifecycle_status='ACTIVE' AND p.lifecycle_status='ACTIVE'
  AND m.deleted_at IS NULL AND p.deleted_at IS NULL
  AND (m.expires_at IS NULL OR m.expires_at > now())
  AND (p.expires_at IS NULL OR p.expires_at > now())
  AND d.status='ACTIVE' AND d.lifecycle_status='ACTIVE'
  AND d.delete_operation_id IS NULL
  AND (d.expires_at IS NULL OR d.expires_at > now())
ORDER BY keys.rank"#,
        )
        .bind(zones)
        .bind(matched)
        .bind(parents)
        .bind(caller_access_level)
        .fetch_all(&mut *tx)
        .await
        .map_err(postgres_error)?;
        tx.commit().await.map_err(postgres_error)?;
        Ok(rows
            .into_iter()
            .map(|r| HydratedSearchContext {
                access_zone_id: r.get("access_zone_id"),
                matched_chunk_id: r.get("matched_chunk_id"),
                parent_chunk_id: r.get("parent_chunk_id"),
                document_id: r.get("document_id"),
                document_version: r.get("document_version"),
                root_chunk_id: r.get("root_chunk_id"),
                source_chunk_id: r.get("source_chunk_id"),
                matched_text: r.get("matched_text"),
                parent_text: r.get("parent_text"),
                parent_content_hash: r.get("parent_content_hash"),
                parent_token_count: r.get("parent_token_count"),
                parent_sequence_no: r.get("parent_sequence_no"),
                access_level: r.get("access_level"),
                source_block_id: r.try_get("source_block_id").ok(),
                source_location: r.get("source_location"),
                source_links: r.get("source_links"),
                metadata: r.get("metadata"),
                parent_metadata: r.get("parent_metadata"),
            })
            .collect())
    }

    pub async fn search_active_parent_contexts_lexical_multi(
        &self,
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        query: &str,
        quality_run_id: Option<&str>,
        limit: i64,
        statement_timeout_ms: u64,
    ) -> Result<Vec<LexicalParentCandidate>, AstraError> {
        if access_zone_ids.is_empty() || limit <= 0 || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        if statement_timeout_ms == 0 {
            return Err(AstraError::DeadlineExceeded(
                "insufficient_postgres_lexical_budget".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(postgres_error)?;
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(format!("{statement_timeout_ms}ms"))
            .execute(&mut *tx)
            .await
            .map_err(postgres_error)?;
        let rows = sqlx::query(
            r#"WITH query_input AS (
  SELECT to_tsquery(
    'simple',
    array_to_string(tsvector_to_array(to_tsvector('simple', $3)), ' | ')
  ) AS query
)
SELECT c.access_zone_id,c.id,c.document_id,c.document_version,c.root_chunk_id,
       c.source_chunk_id,c.access_level,c.content,c.content_hash,c.actual_token_count,
       c.sequence_no,c.source_block_id,c.metadata,
       ts_rank_cd(c.search_vector_simple, query_input.query)::real AS lexical_score,
       (position(lower($3) in lower(c.content)) > 0) AS exact_match,
       numnode(query_input.query)::bigint AS matched_terms
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
CROSS JOIN query_input
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.access_level <= $2
  AND c.granularity='PARENT'
  AND c.representation_type='ORIGINAL'
  AND c.lifecycle_status='ACTIVE'
  AND c.deleted_at IS NULL
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND d.delete_operation_id IS NULL
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND ($4::text IS NULL OR c.metadata->>'quality_run_id'=$4)
  AND c.search_vector_simple @@ query_input.query
ORDER BY lexical_score DESC,c.access_zone_id ASC,c.document_id ASC,c.id ASC
LIMIT $5"#,
        )
        .bind(access_zone_ids)
        .bind(caller_access_level)
        .bind(query.trim())
        .bind(quality_run_id)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(postgres_error)?;
        tx.commit().await.map_err(postgres_error)?;
        Ok(rows
            .into_iter()
            .map(|r| LexicalParentCandidate {
                lexical_score: r.get("lexical_score"),
                exact_match: r.get("exact_match"),
                matched_terms: r.get::<i64, _>("matched_terms").max(0) as u32,
                matched_technical_terms: 0,
                parent: ParentContextRecord {
                    access_zone_id: r.get("access_zone_id"),
                    id: r.get("id"),
                    document_id: r.get("document_id"),
                    document_version: r.get("document_version"),
                    root_chunk_id: r.get("root_chunk_id"),
                    source_chunk_id: r.get("source_chunk_id"),
                    access_level: r.get("access_level"),
                    content: r.get("content"),
                    content_hash: r.get("content_hash"),
                    token_count: r.get("actual_token_count"),
                    sequence_no: r.get("sequence_no"),
                    source_block_id: r.try_get("source_block_id").ok(),
                    metadata: r.get("metadata"),
                },
            })
            .collect())
    }

    #[deprecated(note = "offline diagnostics only; online retrieval uses indexed lexical search")]
    pub async fn fetch_active_parent_context_candidates_multi(
        &self,
        access_zone_ids: &[Uuid],
        caller_access_level: i16,
        quality_run_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ParentContextRecord>, AstraError> {
        if access_zone_ids.is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"SELECT c.access_zone_id,c.id,c.document_id,c.document_version,c.root_chunk_id,c.source_chunk_id,c.access_level,c.content,c.content_hash,c.actual_token_count,c.sequence_no,c.source_block_id,c.metadata
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.granularity='PARENT'
  AND c.representation_type='ORIGINAL'
  AND c.access_level <= $2
  AND c.lifecycle_status='ACTIVE'
  AND c.deleted_at IS NULL
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND ($3::text IS NULL OR c.metadata->>'quality_run_id'=$3)
ORDER BY c.updated_at DESC, c.sequence_no ASC
LIMIT $4"#,
        )
        .bind(access_zone_ids)
        .bind(caller_access_level)
        .bind(quality_run_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| ParentContextRecord {
                access_zone_id: r.get("access_zone_id"),
                id: r.get("id"),
                document_id: r.get("document_id"),
                document_version: r.get("document_version"),
                root_chunk_id: r.get("root_chunk_id"),
                source_chunk_id: r.get("source_chunk_id"),
                access_level: r.get("access_level"),
                content: r.get("content"),
                content_hash: r.get("content_hash"),
                token_count: r.get("actual_token_count"),
                sequence_no: r.get("sequence_no"),
                source_block_id: r
                    .try_get::<Option<String>, _>("source_block_id")
                    .ok()
                    .flatten(),
                metadata: r.get("metadata"),
            })
            .collect())
    }

    pub async fn fetch_chunk_texts_by_ids(
        &self,
        access_zone_id: Uuid,
        chunk_ids: &[Uuid],
        max_access_level: i16,
    ) -> Result<std::collections::HashMap<Uuid, String>, AstraError> {
        if chunk_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows = sqlx::query(
            r#"SELECT id, content
FROM astravector.content_chunks_v004
WHERE access_zone_id=$1
  AND id = ANY($2)
  AND access_level <= $3
  AND lifecycle_status='ACTIVE'"#,
        )
        .bind(access_zone_id)
        .bind(chunk_ids)
        .bind(max_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("content")))
            .collect())
    }

    pub async fn fetch_chunk_traces_by_ids(
        &self,
        access_zone_id: Uuid,
        chunk_ids: &[Uuid],
        max_access_level: i16,
    ) -> Result<std::collections::HashMap<Uuid, ChunkTraceRecord>, AstraError> {
        if chunk_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows = sqlx::query(
            r#"SELECT id, source_block_id, source_location, source_links, metadata
FROM astravector.content_chunks_v004
WHERE access_zone_id=$1
  AND id = ANY($2)
  AND access_level <= $3
  AND lifecycle_status='ACTIVE'"#,
        )
        .bind(access_zone_id)
        .bind(chunk_ids)
        .bind(max_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                (
                    id,
                    ChunkTraceRecord {
                        id,
                        source_block_id: r
                            .try_get::<Option<String>, _>("source_block_id")
                            .ok()
                            .flatten(),
                        source_location: r
                            .try_get("source_location")
                            .unwrap_or_else(|_| serde_json::json!({})),
                        source_links: r
                            .try_get("source_links")
                            .unwrap_or_else(|_| serde_json::json!([])),
                        metadata: r
                            .try_get("metadata")
                            .unwrap_or_else(|_| serde_json::json!({})),
                    },
                )
            })
            .collect())
    }

    pub async fn fetch_chunk_texts_by_ids_multi(
        &self,
        access_zone_ids: &[Uuid],
        chunk_ids: &[Uuid],
        max_access_level: i16,
    ) -> Result<std::collections::HashMap<(Uuid, Uuid), String>, AstraError> {
        if chunk_ids.is_empty() || access_zone_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows = sqlx::query(
            r#"SELECT c.access_zone_id, c.id, c.content
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.id = ANY($2::uuid[])
  AND c.access_level <= $3
  AND c.lifecycle_status='ACTIVE'
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND c.deleted_at IS NULL
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())"#,
        )
        .bind(access_zone_ids)
        .bind(chunk_ids)
        .bind(max_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    (r.get::<Uuid, _>("access_zone_id"), r.get::<Uuid, _>("id")),
                    r.get::<String, _>("content"),
                )
            })
            .collect())
    }

    pub async fn fetch_chunk_traces_by_ids_multi(
        &self,
        access_zone_ids: &[Uuid],
        chunk_ids: &[Uuid],
        max_access_level: i16,
    ) -> Result<std::collections::HashMap<(Uuid, Uuid), ChunkTraceRecord>, AstraError> {
        if chunk_ids.is_empty() || access_zone_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows = sqlx::query(
            r#"SELECT c.access_zone_id, c.id, c.source_block_id, c.source_location, c.source_links, c.metadata
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=ANY($1::uuid[])
  AND c.id = ANY($2::uuid[])
  AND c.access_level <= $3
  AND c.lifecycle_status='ACTIVE'
  AND (c.expires_at IS NULL OR c.expires_at > now())
  AND c.deleted_at IS NULL
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())"#,
        )
        .bind(access_zone_ids)
        .bind(chunk_ids)
        .bind(max_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let zone: Uuid = r.get("access_zone_id");
                let id: Uuid = r.get("id");
                (
                    (zone, id),
                    ChunkTraceRecord {
                        id,
                        source_block_id: r
                            .try_get::<Option<String>, _>("source_block_id")
                            .ok()
                            .flatten(),
                        source_location: r
                            .try_get("source_location")
                            .unwrap_or_else(|_| serde_json::json!({})),
                        source_links: r
                            .try_get("source_links")
                            .unwrap_or_else(|_| serde_json::json!([])),
                        metadata: r
                            .try_get("metadata")
                            .unwrap_or_else(|_| serde_json::json!({})),
                    },
                )
            })
            .collect())
    }

    pub async fn fetch_chunk_group(
        &self,
        access_zone_id: Uuid,
        root_chunk_id: Uuid,
        caller_access_level: i16,
    ) -> Result<Vec<ChunkContentRecord>, AstraError> {
        let rows = sqlx::query(
            r#"SELECT c.id,c.root_chunk_id,c.source_chunk_id,c.parent_chunk_id,c.granularity,c.sequence_no,c.actual_token_count,c.content_hash,c.content
FROM astravector.content_chunks_v004 c
JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
WHERE c.access_zone_id=$1
  AND c.root_chunk_id=$2
  AND c.representation_type='ORIGINAL'
  AND c.access_level <= $3
  AND c.lifecycle_status='ACTIVE'
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND (c.expires_at IS NULL OR c.expires_at > now())
ORDER BY CASE c.granularity WHEN 'SOURCE' THEN 0 WHEN 'PARENT' THEN 1 WHEN 'SUB_180' THEN 2 WHEN 'SUB_260' THEN 3 ELSE 9 END,c.sequence_no"#,
        )
        .bind(access_zone_id)
        .bind(root_chunk_id)
        .bind(caller_access_level)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|r| ChunkContentRecord {
                id: r.get("id"),
                root_chunk_id: r.get("root_chunk_id"),
                source_chunk_id: r.get("source_chunk_id"),
                parent_chunk_id: r
                    .try_get::<Option<Uuid>, _>("parent_chunk_id")
                    .ok()
                    .flatten(),
                granularity: r.get("granularity"),
                sequence_no: r.get("sequence_no"),
                token_count: r.get("actual_token_count"),
                content_hash: r.get("content_hash"),
                content: r.get("content"),
            })
            .collect())
    }

    pub async fn cleanup_v004_document_version_index(
        &self,
        access_zone_id: Uuid,
        document_id: Uuid,
        document_version: i64,
    ) -> Result<(), AstraError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let binding_ids: Vec<Uuid> = sqlx::query("SELECT id FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?
            .into_iter()
            .map(|row| row.get::<Uuid, _>("id"))
            .collect();
        for binding_id in binding_ids {
            sqlx::query("DELETE FROM astravector.vector_outbox WHERE binding_access_zone_id=$1 AND binding_id=$2")
                .bind(access_zone_id)
                .bind(binding_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let cache_ids: Vec<Uuid> = sqlx::query("SELECT cache_entry_id FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?
            .into_iter()
            .map(|row| row.get::<Uuid, _>("cache_entry_id"))
            .collect();
        sqlx::query("DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for cache_id in cache_ids {
            sqlx::query("DELETE FROM astravector.embedding_dense WHERE cache_entry_id=$1")
                .bind(cache_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            sqlx::query("DELETE FROM astravector.embedding_sparse WHERE cache_entry_id=$1")
                .bind(cache_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            sqlx::query("DELETE FROM astravector.embedding_cache_entries WHERE id=$1")
                .bind(cache_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("DELETE FROM astravector.content_chunks_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn persist_v004_embedding_binding_outbox(
        &self,
        tenant: &str,
        workspace: &str,
        chunk: &V004ChunkForEmbedding,
        text_hash: &str,
        result: &EmbeddingResult,
        tokenizer_version: &str,
        model_version: &str,
        dense_name: &str,
        dense_version: &str,
        sparse_name: &str,
        sparse_version: &str,
        min_weight: f32,
        max_non_zero: i32,
        qdrant_collection: &str,
        chunking_profile_version: &str,
        publish_outbox: bool,
    ) -> Result<(), AstraError> {
        let cache_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "v004-cache:{}:{}:{}:{}:{}:{}:{}",
                chunk.access_zone_id,
                chunk.chunk_id,
                text_hash,
                model_version,
                dense_version,
                sparse_version,
                tokenizer_version
            )
            .as_bytes(),
        );
        let binding_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("v004-binding:{}:{}", chunk.access_zone_id, chunk.chunk_id).as_bytes(),
        );
        let point_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("v004-qdrant-point:{}:{}", chunk.access_zone_id, binding_id).as_bytes(),
        );
        let mut hasher = Sha256::new();
        hasher.update(format!(
            "v004-cache-key:{tenant}:{workspace}:{}:{}:{}:{}:{}:{}:{}",
            chunk.access_zone_id,
            chunk.chunk_id,
            text_hash,
            model_version,
            dense_version,
            sparse_version,
            tokenizer_version
        ));
        let cache_key = format!("{:x}", hasher.finalize());
        let mut tx = self.pool.begin().await.map_err(db)?;
        let rows=sqlx::query("INSERT INTO astravector.embedding_cache_entries(id,tenant_id,workspace_id,cache_key,text_hash,purpose,chunk_type,tokenizer_version,model_version,dense_version,sparse_version,status,model_input_token_count,truncated,completed_at,last_accessed_at) VALUES($1,$2,$3,$4,$5,'DOCUMENT_CHUNK',2,$6,$7,$8,$9,'COMPLETED',$10,$11,now(),now()) ON CONFLICT(cache_key) DO UPDATE SET status='COMPLETED',model_input_token_count=EXCLUDED.model_input_token_count,truncated=EXCLUDED.truncated,completed_at=now(),last_accessed_at=now()")
            .bind(cache_id)
            .bind(tenant)
            .bind(workspace)
            .bind(&cache_key)
            .bind(text_hash)
            .bind(tokenizer_version)
            .bind(model_version)
            .bind(dense_version)
            .bind(sparse_version)
            .bind(result.token_count as i32)
            .bind(result.truncated)
            .execute(&mut*tx).await.map_err(db)?.rows_affected();
        if rows != 1 {
            return Err(AstraError::Internal(
                "cache upsert did not affect one row".into(),
            ));
        }
        smoke_failpoints::hit("required_after_embedding_cache_insert")?;
        if let Some(v) = &result.dense {
            let rows=sqlx::query("INSERT INTO astravector.embedding_dense(id,cache_entry_id,representation_name,representation_version,dimension,normalized,distance,vector_value) VALUES($1,$2,$3,$4,$5,true,'COSINE',$6) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET vector_value=EXCLUDED.vector_value,created_at=now()")
                .bind(Uuid::new_v4()).bind(cache_id).bind(dense_name).bind(dense_version).bind(v.len() as i32).bind(Vector::from(v.clone())).execute(&mut*tx).await.map_err(db)?.rows_affected();
            if rows != 1 {
                return Err(AstraError::Internal(
                    "dense upsert did not affect one row".into(),
                ));
            }
            smoke_failpoints::hit("required_after_dense_insert")?;
        }
        if let (Some(indices), Some(values)) = (&result.sparse_indices, &result.sparse_values) {
            let idx: Vec<i32> = indices.iter().map(|x| *x as i32).collect();
            let rows=sqlx::query("INSERT INTO astravector.embedding_sparse(id,cache_entry_id,representation_name,representation_version,indices,values,non_zero_count,min_weight,max_non_zero) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET indices=EXCLUDED.indices,values=EXCLUDED.values,non_zero_count=EXCLUDED.non_zero_count,created_at=now()")
                .bind(Uuid::new_v4()).bind(cache_id).bind(sparse_name).bind(sparse_version).bind(idx).bind(values).bind(indices.len() as i32).bind(min_weight).bind(max_non_zero).execute(&mut*tx).await.map_err(db)?.rows_affected();
            if rows != 1 {
                return Err(AstraError::Internal(
                    "sparse upsert did not affect one row".into(),
                ));
            }
        }
        let mut binding_metadata = chunk.metadata.clone();
        if let Some(object) = binding_metadata.as_object_mut() {
            object.insert(
                "chunking_profile_version".to_string(),
                serde_json::Value::String(chunking_profile_version.to_string()),
            );
        }
        let binding_row=sqlx::query(r#"INSERT INTO astravector.vector_bindings_v004(access_zone_id,id,document_id,document_version,root_chunk_id,source_chunk_id,parent_chunk_id,chunk_id,chunk_granularity,representation_type,chunk_sequence_no,token_count,cache_entry_id,access_level,ttl_days,expires_at,qdrant_collection,qdrant_point_id,metadata)
VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'ORIGINAL',$10,$11,$12,$13,$14,CASE WHEN $14 IS NULL THEN NULL ELSE now()+($14*interval '1 day') END,$15,$16,$17)
ON CONFLICT(access_zone_id,document_id,document_version,chunk_id,representation_type) DO UPDATE SET
  cache_entry_id=EXCLUDED.cache_entry_id,
  token_count=EXCLUDED.token_count,
  access_level=EXCLUDED.access_level,
  ttl_days=EXCLUDED.ttl_days,
  expires_at=EXCLUDED.expires_at,
  qdrant_collection=EXCLUDED.qdrant_collection,
  qdrant_sync_status=CASE WHEN (
    astravector.vector_bindings_v004.cache_entry_id,
    astravector.vector_bindings_v004.token_count,
    astravector.vector_bindings_v004.access_level,
    astravector.vector_bindings_v004.ttl_days,
    astravector.vector_bindings_v004.qdrant_collection,
    astravector.vector_bindings_v004.metadata
  ) IS DISTINCT FROM (
    EXCLUDED.cache_entry_id,
    EXCLUDED.token_count,
    EXCLUDED.access_level,
    EXCLUDED.ttl_days,
    EXCLUDED.qdrant_collection,
    EXCLUDED.metadata
  ) THEN 'PENDING' ELSE astravector.vector_bindings_v004.qdrant_sync_status END,
  payload_version=CASE WHEN (
    astravector.vector_bindings_v004.cache_entry_id,
    astravector.vector_bindings_v004.token_count,
    astravector.vector_bindings_v004.access_level,
    astravector.vector_bindings_v004.ttl_days,
    astravector.vector_bindings_v004.qdrant_collection,
    astravector.vector_bindings_v004.metadata
  ) IS DISTINCT FROM (
    EXCLUDED.cache_entry_id,
    EXCLUDED.token_count,
    EXCLUDED.access_level,
    EXCLUDED.ttl_days,
    EXCLUDED.qdrant_collection,
    EXCLUDED.metadata
  ) THEN astravector.vector_bindings_v004.payload_version+1 ELSE astravector.vector_bindings_v004.payload_version END,
  metadata=EXCLUDED.metadata,
  updated_at=now()
RETURNING payload_version,qdrant_sync_status"#)
            .bind(chunk.access_zone_id).bind(binding_id).bind(chunk.document_id).bind(chunk.document_version).bind(chunk.root_chunk_id).bind(chunk.source_chunk_id).bind(chunk.parent_chunk_id).bind(chunk.chunk_id).bind(&chunk.granularity).bind(chunk.sequence_no).bind(chunk.token_count).bind(cache_id).bind(chunk.access_level).bind(chunk.ttl_days).bind(qdrant_collection).bind(point_id).bind(binding_metadata).fetch_one(&mut*tx).await.map_err(db)?;
        let payload_version: i64 = binding_row.get("payload_version");
        let qdrant_sync_status: String = binding_row.get("qdrant_sync_status");
        smoke_failpoints::hit("required_after_binding_insert")?;
        if publish_outbox
            && (qdrant_sync_status == "PENDING" || qdrant_sync_status == "UPDATE_PENDING")
        {
            let rows=sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'UPSERT_POINT',$4,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING")
                .bind(Uuid::new_v4()).bind(chunk.access_zone_id).bind(binding_id).bind(payload_version).execute(&mut*tx).await.map_err(db)?.rows_affected();
            if rows > 1 {
                return Err(AstraError::Internal(
                    "outbox insert affected too many rows".into(),
                ));
            }
            smoke_failpoints::hit("required_after_outbox_insert")?;
        }
        smoke_failpoints::hit("required_before_commit")?;
        tx.commit().await.map_err(db)?;
        smoke_failpoints::hit("required_after_commit_before_response")?;
        Ok(())
    }
    pub async fn find_idempotent(
        &self,
        tenant: &str,
        workspace: &str,
        key: &str,
    ) -> Result<Option<RequestRecord>, AstraError> {
        if key.is_empty() {
            return Ok(None);
        }
        let r=sqlx::query("SELECT id,request_hash,status FROM astravector.embedding_requests WHERE tenant_id=$1 AND workspace_id=$2 AND idempotency_key=$3").bind(tenant).bind(workspace).bind(key).fetch_optional(&self.pool).await.map_err(db)?;
        Ok(r.map(|x| RequestRecord {
            id: x.get("id"),
            request_hash: x.get("request_hash"),
            status: x.get("status"),
        }))
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn create_request(
        &self,
        r: &pb::EncodeBatchRequest,
        hash: &str,
        status: &str,
        contract: &str,
        tokenizer: &str,
        model: &str,
        reps: &[String],
    ) -> Result<Uuid, AstraError> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO astravector.embedding_requests(id,emb_task_id,correlation_id,idempotency_key,tenant_id,workspace_id,caller_service,purpose,access_level,persistence_mode,requested_representations,request_hash,status,item_count,contract_version,tokenizer_version,model_version,started_at) VALUES($1,$2,$3,NULLIF($4,''),$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now())")
 .bind(id).bind(Uuid::parse_str(&r.emb_task_id).map_err(|_|AstraError::InvalidArgument("invalid emb_task_id".into()))?).bind(Uuid::parse_str(&r.correlation_id).map_err(|_|AstraError::InvalidArgument("invalid correlation_id".into()))?).bind(&r.idempotency_key).bind(&r.tenant_id).bind(&r.workspace_id).bind(&r.caller_service).bind(format!("{:?}",pb::EncodingPurpose::try_from(r.purpose).unwrap_or(pb::EncodingPurpose::Unspecified))).bind(format!("{:?}",pb::AccessLevel::try_from(r.access_level).unwrap_or(pb::AccessLevel::Unspecified))).bind(format!("{:?}",pb::PersistenceMode::try_from(r.persistence_mode).unwrap_or(pb::PersistenceMode::Unspecified))).bind(reps).bind(hash).bind(status).bind(r.items.len() as i32).bind(contract).bind(tokenizer).bind(model).execute(&self.pool).await.map_err(db)?;
        Ok(id)
    }
    pub async fn create_items(
        &self,
        request_id: Uuid,
        items: &[(pb::EncodeItem, String)],
    ) -> Result<(), AstraError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for (item, h) in items {
            sqlx::query("INSERT INTO astravector.embedding_items(id,embedding_request_id,chunk_id,chunk_type,parent_chunk_id,text_hash,text_length,status) VALUES($1,$2,$3,$4,$5,$6,$7,'RECEIVED') ON CONFLICT(embedding_request_id,chunk_id) DO NOTHING").bind(Uuid::new_v4()).bind(request_id).bind(Uuid::parse_str(&item.chunk_id).map_err(|_|AstraError::InvalidArgument("invalid chunk_id".into()))?).bind(item.chunk_type as i16).bind(item.parent_chunk_id.as_deref().and_then(|x|Uuid::parse_str(x).ok())).bind(h).bind(item.text.len() as i32).execute(&mut*tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn claim(
        &self,
        tenant: &str,
        workspace: &str,
        key: &str,
        text_hash: &str,
        purpose: &str,
        chunk_type: i16,
        tokenizer: &str,
        model: &str,
        dense: Option<&str>,
        sparse: Option<&str>,
        owner: &str,
        lease_secs: i64,
    ) -> Result<ClaimResult, AstraError> {
        let id = Uuid::new_v4();
        if let Some(r)=sqlx::query("INSERT INTO astravector.embedding_cache_entries(id,tenant_id,workspace_id,cache_key,text_hash,purpose,chunk_type,tokenizer_version,model_version,dense_version,sparse_version,status,owner_instance_id,lease_token,processing_started_at,lease_expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'PROCESSING',$12,1,now(),now()+($13 * interval '1 second')) ON CONFLICT(cache_key) DO NOTHING RETURNING id,lease_token").bind(id).bind(tenant).bind(workspace).bind(key).bind(text_hash).bind(purpose).bind(chunk_type).bind(tokenizer).bind(model).bind(dense).bind(sparse).bind(owner).bind(lease_secs).fetch_optional(&self.pool).await.map_err(db)?{return Ok(ClaimResult::Acquired{cache_entry_id:r.get("id"),lease_token:r.get("lease_token")})}
        self.read_claim(key, owner, lease_secs).await
    }
    async fn read_claim(
        &self,
        key: &str,
        owner: &str,
        lease_secs: i64,
    ) -> Result<ClaimResult, AstraError> {
        let r=sqlx::query("SELECT id,status,lease_expires_at FROM astravector.embedding_cache_entries WHERE cache_key=$1").bind(key).fetch_one(&self.pool).await.map_err(db)?;
        let id: Uuid = r.get("id");
        let status: String = r.get("status");
        if status == "COMPLETED" {
            return Ok(ClaimResult::Completed {
                cache_entry_id: id,
                result: self
                    .load_completed(key)
                    .await?
                    .ok_or_else(|| AstraError::Internal("completed cache has no vectors".into()))?,
            });
        }
        let exp: Option<DateTime<Utc>> = r.try_get("lease_expires_at").ok();
        if status == "FAILED" || exp.map(|x| x < Utc::now()).unwrap_or(true) {
            if let Some(t) = self.takeover(id, owner, lease_secs).await? {
                return Ok(ClaimResult::RetryAcquired {
                    cache_entry_id: id,
                    lease_token: t,
                });
            }
        }
        Ok(ClaimResult::ProcessingByOther {
            cache_entry_id: id,
            lease_expires_at: exp,
        })
    }
    pub async fn takeover(
        &self,
        id: Uuid,
        owner: &str,
        lease_secs: i64,
    ) -> Result<Option<i64>, AstraError> {
        let r=sqlx::query("UPDATE astravector.embedding_cache_entries SET owner_instance_id=$2,lease_token=lease_token+1,processing_started_at=now(),lease_expires_at=now()+($3 * interval '1 second'),retry_count=retry_count+1,status='PROCESSING',error_code=NULL,error_message=NULL WHERE id=$1 AND status IN('PROCESSING','FAILED') AND(status='FAILED' OR lease_expires_at<now()) RETURNING lease_token").bind(id).bind(owner).bind(lease_secs).fetch_optional(&self.pool).await.map_err(db)?;
        Ok(r.map(|x| x.get("lease_token")))
    }
    pub async fn load_completed(&self, key: &str) -> Result<Option<EmbeddingResult>, AstraError> {
        let r=sqlx::query("SELECT d.vector_value,s.indices,s.values,c.model_input_token_count,c.truncated FROM astravector.embedding_cache_entries c LEFT JOIN astravector.embedding_dense d ON d.cache_entry_id=c.id LEFT JOIN astravector.embedding_sparse s ON s.cache_entry_id=c.id WHERE c.cache_key=$1 AND c.status='COMPLETED'").bind(key).fetch_optional(&self.pool).await.map_err(db)?;
        let Some(x) = r else { return Ok(None) };
        let dense: Option<Vector> = x.try_get("vector_value").ok();
        let idx: Option<Vec<i32>> = x.try_get("indices").ok();
        Ok(Some(EmbeddingResult {
            dense: dense.map(|v| v.to_vec()),
            sparse_indices: idx.map(|v| v.into_iter().map(|n| n as u32).collect()),
            sparse_values: x.try_get("values").ok(),
            token_count: x
                .try_get::<Option<i32>, _>("model_input_token_count")
                .ok()
                .flatten()
                .unwrap_or(0) as usize,
            truncated: x.try_get("truncated").unwrap_or(false),
        }))
    }
    pub async fn wait_completed(
        &self,
        key: &str,
        deadline: tokio::time::Instant,
        initial: u64,
        max: u64,
    ) -> Result<EmbeddingResult, AstraError> {
        let mut d = initial.max(10);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(AstraError::DeadlineExceeded(
                    "cache wait deadline exceeded".into(),
                ));
            }
            if let Some(v) = self.load_completed(key).await? {
                return Ok(v);
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + Duration::from_millis(d)).min(deadline),
            )
            .await;
            d = ((d as f64) * 1.5) as u64;
            d = d.min(max)
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn persist_owned(
        &self,
        cache_id: Uuid,
        owner: &str,
        lease: i64,
        result: &EmbeddingResult,
        dense_name: &str,
        dense_version: &str,
        sparse_name: &str,
        sparse_version: &str,
        min_weight: f32,
        max_non_zero: i32,
    ) -> Result<(), AstraError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let owned=sqlx::query("SELECT 1 FROM astravector.embedding_cache_entries WHERE id=$1 AND status='PROCESSING' AND owner_instance_id=$2 AND lease_token=$3 FOR UPDATE").bind(cache_id).bind(owner).bind(lease).fetch_optional(&mut*tx).await.map_err(db)?;
        if owned.is_none() {
            return Err(AstraError::OwnershipLost("lease no longer owned".into()));
        }
        if let Some(v) = &result.dense {
            sqlx::query("INSERT INTO astravector.embedding_dense(id,cache_entry_id,representation_name,representation_version,dimension,normalized,distance,vector_value) VALUES($1,$2,$3,$4,$5,true,'COSINE',$6) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET vector_value=EXCLUDED.vector_value,created_at=now()").bind(Uuid::new_v4()).bind(cache_id).bind(dense_name).bind(dense_version).bind(v.len()as i32).bind(Vector::from(v.clone())).execute(&mut*tx).await.map_err(db)?;
        }
        if let (Some(i), Some(v)) = (&result.sparse_indices, &result.sparse_values) {
            let idx: Vec<i32> = i.iter().map(|x| *x as i32).collect();
            sqlx::query("INSERT INTO astravector.embedding_sparse(id,cache_entry_id,representation_name,representation_version,indices,values,non_zero_count,min_weight,max_non_zero) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET indices=EXCLUDED.indices,values=EXCLUDED.values,non_zero_count=EXCLUDED.non_zero_count,created_at=now()").bind(Uuid::new_v4()).bind(cache_id).bind(sparse_name).bind(sparse_version).bind(idx).bind(v).bind(i.len()as i32).bind(min_weight).bind(max_non_zero).execute(&mut*tx).await.map_err(db)?;
        }
        let n=sqlx::query("UPDATE astravector.embedding_cache_entries SET status='COMPLETED',model_input_token_count=$4,truncated=$5,completed_at=now(),last_accessed_at=now(),lease_expires_at=NULL WHERE id=$1 AND owner_instance_id=$2 AND lease_token=$3 AND status='PROCESSING'").bind(cache_id).bind(owner).bind(lease).bind(result.token_count as i32).bind(result.truncated).execute(&mut*tx).await.map_err(db)?.rows_affected();
        if n != 1 {
            return Err(AstraError::OwnershipLost("fencing update rejected".into()));
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn update_item(
        &self,
        request: Uuid,
        chunk: Uuid,
        cache: Option<Uuid>,
        status: &str,
        result: Option<&EmbeddingResult>,
        code: Option<&str>,
        msg: Option<&str>,
    ) -> Result<(), AstraError> {
        sqlx::query("UPDATE astravector.embedding_items SET cache_entry_id=$3,status=$4,model_input_token_count=$5,truncated=COALESCE($6,false),error_code=$7,error_message=$8,completed_at=CASE WHEN $4 IN('COMPLETED','FAILED','CACHE_HIT') THEN now() ELSE completed_at END WHERE embedding_request_id=$1 AND chunk_id=$2").bind(request).bind(chunk).bind(cache).bind(status).bind(result.map(|r|r.token_count as i32)).bind(result.map(|r|r.truncated)).bind(code).bind(msg).execute(&self.pool).await.map_err(db)?;
        Ok(())
    }
    pub async fn finish_request(
        &self,
        id: Uuid,
        status: &str,
        code: Option<&str>,
        msg: Option<&str>,
    ) -> Result<(), AstraError> {
        sqlx::query("UPDATE astravector.embedding_requests SET status=$2,error_code=$3,error_message=$4,completed_at=now() WHERE id=$1").bind(id).bind(status).bind(code).bind(msg).execute(&self.pool).await.map_err(db)?;
        Ok(())
    }
    pub async fn replay_items(
        &self,
        request_id: Uuid,
    ) -> Result<Vec<(pb::EncodeItem, EmbeddingResult)>, AstraError> {
        let rows=sqlx::query("SELECT i.chunk_id,i.chunk_type,i.parent_chunk_id,c.cache_key FROM astravector.embedding_items i JOIN astravector.embedding_cache_entries c ON c.id=i.cache_entry_id WHERE i.embedding_request_id=$1 ORDER BY i.created_at").bind(request_id).fetch_all(&self.pool).await.map_err(db)?;
        let mut out = Vec::new();
        for r in rows {
            let key: String = r.get("cache_key");
            if let Some(v) = self.load_completed(&key).await? {
                out.push((
                    pb::EncodeItem {
                        chunk_id: r.get::<Uuid, _>("chunk_id").to_string(),
                        chunk_type: r.get::<i16, _>("chunk_type") as i32,
                        text: String::new(),
                        parent_chunk_id: r
                            .try_get::<Option<Uuid>, _>("parent_chunk_id")
                            .ok()
                            .flatten()
                            .map(|x| x.to_string()),
                        content_hash: None,
                        document_id: None,
                        document_version: None,
                        access_level: 0,
                        ttl_days: None,
                        representation_type: 1,
                        source_chunk_id: None,
                        metadata: Default::default(),
                    },
                    v,
                ))
            }
        }
        Ok(out)
    }
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}

fn postgres_error(error: sqlx::Error) -> AstraError {
    match &error {
        sqlx::Error::Database(database) if database.code().as_deref() == Some("57014") => {
            AstraError::DeadlineExceeded(format!("POSTGRES_STATEMENT_TIMEOUT: {error}"))
        }
        sqlx::Error::PoolTimedOut => {
            AstraError::Unavailable(format!("POSTGRES_POOL_TIMEOUT: {error}"))
        }
        sqlx::Error::Io(_) | sqlx::Error::Tls(_) | sqlx::Error::PoolClosed => {
            AstraError::Unavailable(format!("POSTGRES_CONNECTION_UNAVAILABLE: {error}"))
        }
        _ => AstraError::Unavailable(format!("POSTGRES_QUERY_FAILED: {error}")),
    }
}
