use crate::{
    chunking::GeneratedChunk, config::PostgresConfig, error::AstraError,
    inference::EmbeddingResult, pb, smoke_failpoints,
};
use chrono::{DateTime, Utc};
use pgvector::Vector;
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::{PgPool, PgPoolOptions},
    Row,
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
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone)]
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
                    sqlx::query(&format!("SET statement_timeout='{st}ms'"))
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query(&format!("SET lock_timeout='{lt}ms'"))
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query(&format!(
                        "SET idle_in_transaction_session_timeout='{idle}ms'"
                    ))
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
            sqlx::query("UPDATE astravector.document_versions SET status='SUPERSEDED',updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version<>$3 AND status='ACTIVE'")
                .bind(access_zone_id)
                .bind(document_id)
                .bind(document_version)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let updated = sqlx::query("UPDATE astravector.document_versions SET status='ACTIVE',activated_at=COALESCE(activated_at,now()),updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 RETURNING document_id,document_version,status")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
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
        let n=sqlx::query("UPDATE astravector.document_versions SET status='INDEXING',updated_at=now() WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND status IN('REGISTERED','INDEXING','FAILED')").bind(access_zone_id).bind(document_id).bind(document_version).execute(&mut*tx).await.map_err(db)?.rows_affected();
        if n != 1 {
            return Err(AstraError::FailedPrecondition(
                "document version must exist and be REGISTERED/INDEXING/FAILED".into(),
            ));
        }
        for chunk in chunks {
            let rows=sqlx::query("INSERT INTO astravector.content_chunks_v004(access_zone_id,id,root_chunk_id,source_chunk_id,parent_chunk_id,document_id,document_version,granularity,representation_type,sequence_no,target_token_count,actual_token_count,content,content_hash,tokenizer_version,chunking_profile_version,access_level,ttl_days,expires_at,metadata) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'ORIGINAL',$9,$10,$10,$11,$12,$13,$14,$15,$16,CASE WHEN $16 IS NULL THEN NULL ELSE now()+($16*interval '1 day') END,$17) ON CONFLICT(access_zone_id,document_id,document_version,root_chunk_id,parent_chunk_id,granularity,representation_type,sequence_no) DO UPDATE SET content=EXCLUDED.content,content_hash=EXCLUDED.content_hash,actual_token_count=EXCLUDED.actual_token_count,tokenizer_version=EXCLUDED.tokenizer_version,chunking_profile_version=EXCLUDED.chunking_profile_version,access_level=EXCLUDED.access_level,ttl_days=EXCLUDED.ttl_days,expires_at=EXCLUDED.expires_at,metadata=EXCLUDED.metadata,updated_at=now()")
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
                .execute(&mut*tx).await.map_err(db)?.rows_affected();
            if rows != 1 {
                return Err(AstraError::Internal(
                    "chunk upsert did not affect exactly one row".into(),
                ));
            }
            smoke_failpoints::hit("required_after_chunk_insert")?;
        }
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
            r#"SELECT c.access_zone_id,c.id,c.document_id,c.document_version,c.root_chunk_id,c.source_chunk_id,c.access_level,c.content,c.content_hash,c.actual_token_count,c.metadata
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
                metadata: r.get("metadata"),
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
                parent_chunk_id: r.try_get("parent_chunk_id").ok(),
                granularity: r.get("granularity"),
                sequence_no: r.get("sequence_no"),
                token_count: r.get("actual_token_count"),
                content_hash: r.get("content_hash"),
                content: r.get("content"),
            })
            .collect())
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
    ) -> Result<(), AstraError> {
        let cache_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "v004-cache:{}:{}:{}:{}",
                chunk.access_zone_id, chunk.chunk_id, text_hash, dense_version
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
            "v004-cache-key:{tenant}:{workspace}:{}:{}:{}",
            chunk.access_zone_id, chunk.chunk_id, text_hash
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
        if qdrant_sync_status == "PENDING" || qdrant_sync_status == "UPDATE_PENDING" {
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
