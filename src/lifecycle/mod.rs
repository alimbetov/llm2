use crate::{config::IndexTtlConfig, error::AstraError, persistence::Repository};
use metrics::{counter, gauge};
use sqlx::Row;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
pub fn spawn(
    repo: Repository,
    scan_seconds: u64,
    batch_size: i64,
    grace_days: i64,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(scan_seconds.max(5)));
        loop {
            tokio::select! {_=shutdown.cancelled()=>break,_=interval.tick()=>{match expire_batch(&repo,batch_size).await{Ok(n)=>gauge!("astravector_expired_bindings").set(n as f64),Err(_)=>counter!("astravector_lifecycle_failures_total","operation"=>"expire").increment(1)}match purge_batch(&repo,batch_size,grace_days).await{Ok(n)=>counter!("astravector_postgres_purged_total").increment(n as u64),Err(_)=>counter!("astravector_lifecycle_failures_total","operation"=>"purge").increment(1)}}}
        }
    });
}

pub fn spawn_index_ttl_cleanup(
    repo: Repository,
    qdrant: Arc<crate::qdrant::QdrantClient>,
    cfg: IndexTtlConfig,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        counter!("index_ttl_worker_started_total").increment(1);
        let mut interval =
            tokio::time::interval(Duration::from_secs(cfg.cleanup_interval_seconds.max(5)));
        // Avoid running an immediate cleanup before all startup components are ready.
        interval.tick().await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    counter!("index_ttl_worker_iterations_total").increment(1);
                    match run_index_ttl_cleanup_batch(
                        &repo,
                        qdrant.as_ref(),
                        cfg.cleanup_batch_size,
                        cfg.qdrant_delete_batch_size,
                        cfg.delete_failed_retry_after_seconds,
                        cfg.deleting_stale_timeout_seconds,
                        cfg.max_delete_attempts,
                        cfg.delete_retry_initial_delay_seconds,
                        cfg.delete_retry_max_delay_seconds,
                        cfg.qdrant_scroll_batch_size as u64,
                        cfg.qdrant_reconciliation_enabled,
                    ).await {
                        Ok(stats) => {
                            gauge!("index_ttl_backlog_documents").set(stats.claimed_documents as f64);
                        }
                        Err(e) => {
                            counter!("index_ttl_worker_iteration_failed_total").increment(1);
                            tracing::warn!(error=%e, "index TTL cleanup iteration failed");
                        }
                    }

                    if cfg.hard_delete_metadata {
                        match purge_index_ttl_tombstones(&repo, cfg.keep_tombstone_days, cfg.cleanup_batch_size as i64).await {
                            Ok(n) => {
                                if n > 0 { counter!("index_ttl_tombstones_purged_total").increment(n); }
                            }
                            Err(e) => {
                                counter!("index_ttl_tombstone_purge_failed_total").increment(1);
                                tracing::warn!(error=%e, "index TTL tombstone purge failed");
                            }
                        }
                    }
                }
            }
        }
    });
}

pub async fn expire_batch(repo: &Repository, limit: i64) -> Result<usize, AstraError> {
    let mut tx = repo.pool.begin().await.map_err(db)?;
    let rows=sqlx::query("SELECT access_zone_id,id,ttl_generation FROM astravector.vector_bindings_v004 WHERE lifecycle_status='ACTIVE' AND expires_at<=now() AND legal_hold=false ORDER BY expires_at FOR UPDATE SKIP LOCKED LIMIT $1").bind(limit).fetch_all(&mut*tx).await.map_err(db)?;
    let mut affected = 0usize;
    for r in rows {
        let zone: Uuid = r.get("access_zone_id");
        let id: Uuid = r.get("id");
        let generation: i64 = r.get("ttl_generation");
        let updated=sqlx::query("UPDATE astravector.vector_bindings_v004 SET ttl_generation=ttl_generation+1,lifecycle_status='DELETION_PENDING',qdrant_sync_status='DELETE_PENDING',expired_at=now(),updated_at=now() WHERE access_zone_id=$1 AND id=$2 AND ttl_generation=$3 AND legal_hold=false AND lifecycle_status='ACTIVE' RETURNING ttl_generation").bind(zone).bind(id).bind(generation).fetch_optional(&mut*tx).await.map_err(db)?;
        if let Some(updated) = updated {
            let op_version: i64 = updated.get("ttl_generation");
            sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'DELETE_POINT',$4,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING").bind(Uuid::new_v4()).bind(zone).bind(id).bind(op_version).execute(&mut*tx).await.map_err(db)?;
            affected += 1
        }
    }
    tx.commit().await.map_err(db)?;
    Ok(affected)
}
pub async fn purge_batch(
    repo: &Repository,
    limit: i64,
    grace_days: i64,
) -> Result<usize, AstraError> {
    let mut tx = repo.pool.begin().await.map_err(db)?;
    let rows=sqlx::query("SELECT access_zone_id,id,cache_entry_id FROM astravector.vector_bindings_v004 WHERE lifecycle_status='SOFT_DELETED' AND legal_hold=false AND deleted_at<now()-($1*interval '1 day') ORDER BY deleted_at FOR UPDATE SKIP LOCKED LIMIT $2").bind(grace_days).bind(limit).fetch_all(&mut*tx).await.map_err(db)?;
    for r in &rows {
        let zone: Uuid = r.get("access_zone_id");
        let id: Uuid = r.get("id");
        let cache: Uuid = r.get("cache_entry_id");
        sqlx::query("DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND id=$2 AND legal_hold=false").bind(zone).bind(id).execute(&mut*tx).await.map_err(db)?;
        sqlx::query("DELETE FROM astravector.embedding_cache_entries c WHERE c.id=$1 AND c.status<>'PROCESSING' AND NOT EXISTS(SELECT 1 FROM astravector.vector_bindings_v004 b WHERE b.cache_entry_id=c.id AND b.lifecycle_status IN('ACTIVE','LEGAL_HOLD','DELETION_PENDING'))").bind(cache).execute(&mut*tx).await.map_err(db)?;
    }
    tx.commit().await.map_err(db)?;
    Ok(rows.len())
}
pub async fn update_group_ttl(
    repo: &Repository,
    zone: Uuid,
    root: Uuid,
    ttl: Option<i32>,
) -> Result<u64, AstraError> {
    let mut tx = repo.pool.begin().await.map_err(db)?;
    let rows=sqlx::query("UPDATE astravector.vector_bindings_v004 SET ttl_days=$3,expires_at=CASE WHEN $3 IS NULL THEN NULL ELSE now()+($3*interval '1 day') END,ttl_generation=ttl_generation+1,payload_version=payload_version+1,qdrant_sync_status='UPDATE_PENDING',updated_at=now() WHERE access_zone_id=$1 AND root_chunk_id=$2 AND lifecycle_status='ACTIVE' AND legal_hold=false RETURNING id,payload_version").bind(zone).bind(root).bind(ttl).fetch_all(&mut*tx).await.map_err(db)?;
    for r in &rows {
        sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'UPDATE_PAYLOAD',$4,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING").bind(Uuid::new_v4()).bind(zone).bind(r.get::<Uuid,_>("id")).bind(r.get::<i64,_>("payload_version")).execute(&mut*tx).await.map_err(db)?;
    }
    tx.commit().await.map_err(db)?;
    Ok(rows.len() as u64)
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexTtlCleanupStage {
    Claim,
    QdrantScroll,
    QdrantDelete,
    DocumentVersionUpdate,
    ContentChunksUpdate,
    GraphNodesUpdate,
    GraphEdgesUpdate,
    TombstonePurge,
}

impl IndexTtlCleanupStage {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::QdrantScroll => "qdrant_scroll",
            Self::QdrantDelete => "qdrant_delete",
            Self::DocumentVersionUpdate => "document_version_update",
            Self::ContentChunksUpdate => "content_chunks_update",
            Self::GraphNodesUpdate => "graph_nodes_update",
            Self::GraphEdgesUpdate => "graph_edges_update",
            Self::TombstonePurge => "tombstone_purge",
        }
    }

    pub fn error_code(self) -> &'static str {
        match self {
            Self::QdrantScroll => "QDRANT_SCROLL_FAILED",
            Self::QdrantDelete => "QDRANT_DELETE_FAILED",
            Self::GraphNodesUpdate => "GRAPH_NODES_CLEANUP_FAILED",
            Self::GraphEdgesUpdate => "GRAPH_EDGES_CLEANUP_FAILED",
            Self::ContentChunksUpdate => "CONTENT_CHUNKS_CLEANUP_FAILED",
            Self::DocumentVersionUpdate => "DOCUMENT_VERSION_CLEANUP_FAILED",
            Self::TombstonePurge => "TOMBSTONE_PURGE_FAILED",
            Self::Claim => "INDEX_TTL_CLAIM_FAILED",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexTtlCleanupError {
    pub stage: IndexTtlCleanupStage,
    pub message: String,
}

impl IndexTtlCleanupError {
    fn new(stage: IndexTtlCleanupStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }

    fn from_astra(stage: IndexTtlCleanupStage, error: AstraError) -> Self {
        Self::new(stage, error.to_string())
    }

    pub fn error_code(&self) -> &'static str {
        self.stage.error_code()
    }
    pub fn stage_label(&self) -> &'static str {
        self.stage.as_label()
    }
}

impl std::fmt::Display for IndexTtlCleanupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error_code(), self.message)
    }
}

impl std::error::Error for IndexTtlCleanupError {}

#[derive(Debug, Clone, Default)]
pub struct IndexTtlCleanupStats {
    pub claimed_documents: u64,
    pub deleted_documents: u64,
    pub delete_failed_documents: u64,
    pub qdrant_points_deleted: u64,
    pub tombstones_purged: u64,
}

#[derive(Debug, Clone)]
struct IndexTtlCleanupCandidate {
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
}

pub async fn mark_stale_deleting_documents(
    repo: &Repository,
    deleting_stale_timeout_seconds: u64,
) -> Result<u64, AstraError> {
    let affected = sqlx::query(
        "UPDATE astravector.document_versions
         SET lifecycle_status='DELETE_FAILED',
             last_delete_error_code='DELETING_STALE_TIMEOUT',
             last_delete_error_message='Deleting state exceeded stale timeout',
             last_delete_error_stage='STALE_DELETING_RECOVERY',
             last_delete_error_at=now(),
             delete_operation_id=NULL,
             delete_fencing_started_at=NULL,
             updated_at=now()
         WHERE lifecycle_status='DELETING'
           AND deleting_started_at < now() - ($1 * interval '1 second')",
    )
    .bind(deleting_stale_timeout_seconds as i64)
    .execute(&repo.pool)
    .await
    .map_err(db)?
    .rows_affected();
    if affected > 0 {
        counter!("index_ttl_deleting_stale_total").increment(affected);
    }
    Ok(affected)
}

async fn claim_index_ttl_cleanup_batch(
    repo: &Repository,
    cleanup_batch_size: usize,
    _delete_failed_retry_after_seconds: u64,
    max_delete_attempts: u32,
) -> Result<Vec<IndexTtlCleanupCandidate>, AstraError> {
    let rows = sqlx::query(
        r#"WITH candidates AS (
    SELECT access_zone_id, document_id, document_version
    FROM astravector.document_versions
    WHERE (
            (lifecycle_status = 'ACTIVE' AND expires_at IS NOT NULL AND expires_at <= now())
         OR lifecycle_status = 'EXPIRED'
         OR lifecycle_status = 'SUPERSEDED'
         OR (
              lifecycle_status = 'DELETE_FAILED'
              AND delete_attempts < $2
              AND (
                    next_delete_attempt_at IS NULL
                 OR next_delete_attempt_at <= now()
              )
            )
          )
      AND delete_operation_id IS NULL
      AND NOT EXISTS (
          SELECT 1
          FROM astravector.vector_bindings_v004 b
          WHERE b.access_zone_id = document_versions.access_zone_id
            AND b.document_id = document_versions.document_id
            AND b.document_version = document_versions.document_version
            AND COALESCE(b.legal_hold,false)=true
      )
    ORDER BY expires_at NULLS LAST, updated_at
    LIMIT $1
    FOR UPDATE SKIP LOCKED
)
UPDATE astravector.document_versions dv
SET lifecycle_status = 'DELETING',
    deleting_started_at = now(),
    delete_attempts = delete_attempts + 1,
    updated_at = now()
FROM candidates c
WHERE dv.access_zone_id = c.access_zone_id
  AND dv.document_id = c.document_id
  AND dv.document_version = c.document_version
  AND dv.delete_operation_id IS NULL
RETURNING dv.access_zone_id, dv.document_id, dv.document_version"#,
    )
    .bind(cleanup_batch_size as i64)
    .bind(max_delete_attempts as i32)
    .fetch_all(&repo.pool)
    .await
    .map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|r| IndexTtlCleanupCandidate {
            access_zone_id: r.get("access_zone_id"),
            document_id: r.get("document_id"),
            document_version: r.get("document_version"),
        })
        .collect())
}

async fn mark_index_ttl_delete_failed(
    repo: &Repository,
    c: &IndexTtlCleanupCandidate,
    code: &str,
    message: String,
    max_delete_attempts: u32,
    retry_initial_delay_seconds: u64,
    retry_max_delay_seconds: u64,
) -> Result<(), AstraError> {
    let row = sqlx::query(
        "SELECT delete_attempts FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3"
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .fetch_optional(&repo.pool)
    .await
    .map_err(db)?;
    let attempts = row
        .map(|r| r.get::<i32, _>("delete_attempts").max(0) as u32)
        .unwrap_or(0);
    let terminal = attempts >= max_delete_attempts;
    let status = if terminal {
        "DELETE_PERMANENTLY_FAILED"
    } else {
        "DELETE_FAILED"
    };
    let delay = retry_initial_delay_seconds
        .saturating_mul(
            1_u64
                .checked_shl(attempts.saturating_sub(1).min(20))
                .unwrap_or(u64::MAX),
        )
        .min(retry_max_delay_seconds.max(retry_initial_delay_seconds));
    sqlx::query(
        "UPDATE astravector.document_versions
         SET lifecycle_status=$4,
             last_delete_error_code=$5,
             last_delete_error_message=$6,
             last_delete_error_stage=$9,
             last_delete_error_at=now(),
             delete_operation_id=NULL,
             delete_fencing_started_at=NULL,
             next_delete_attempt_at=CASE WHEN $7 THEN NULL ELSE now()+($8 * interval '1 second') END,
             updated_at=now()
         WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3"
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .bind(status)
    .bind(code)
    .bind(message)
    .bind(terminal)
    .bind(delay as i64)
    .bind(code.split('_').next().unwrap_or("UNKNOWN"))
    .execute(&repo.pool)
    .await
    .map_err(db)?;
    if terminal {
        counter!("index_ttl_delete_permanently_failed_total").increment(1);
    } else {
        counter!("index_ttl_delete_retry_scheduled_total").increment(1);
    }
    counter!("index_ttl_cleanup_delete_failed_total").increment(1);
    Ok(())
}

async fn cleanup_one_index_ttl_document(
    repo: &Repository,
    qdrant: &crate::qdrant::QdrantClient,
    c: &IndexTtlCleanupCandidate,
    qdrant_delete_batch_size: usize,
    qdrant_scroll_batch_size: u64,
    qdrant_reconciliation_enabled: bool,
) -> Result<u64, IndexTtlCleanupError> {
    // fix461: fence the PostgreSQL document state before deleting the Qdrant projection.
    // Qdrant is a derived projection; never delete it unless PostgreSQL still owns a
    // DELETING document version for this cleanup operation.
    let delete_operation_id = Uuid::new_v4();
    let fencing_update = sqlx::query(
        "UPDATE astravector.document_versions
         SET delete_operation_id=$4,
             delete_fencing_started_at=now(),
             updated_at=now()
         WHERE access_zone_id=$1
           AND document_id=$2
           AND document_version=$3
           AND lifecycle_status='DELETING'
           AND delete_operation_id IS NULL",
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .bind(delete_operation_id)
    .execute(&repo.pool)
    .await
    .map_err(|e| {
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::DocumentVersionUpdate, db(e))
    })?;
    if fencing_update.rows_affected() != 1 {
        counter!("index_ttl_cleanup_concurrent_state_change_total").increment(1);
        counter!("index_ttl_delete_operation_conflict_total", "stage" => "fence_start")
            .increment(1);
        return Err(IndexTtlCleanupError::new(
            IndexTtlCleanupStage::DocumentVersionUpdate,
            "document version was not in DELETING state before Qdrant delete fencing",
        ));
    }

    let expected_point_ids = repo
        .fetch_qdrant_point_ids_for_document_deletion(
            c.access_zone_id,
            c.document_id,
            c.document_version,
        )
        .await
        .map_err(|e| IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::QdrantScroll, e))?;

    let mut point_ids = expected_point_ids;
    if qdrant_reconciliation_enabled {
        let qdrant_seen = qdrant
            .point_ids_by_document_with_page_size(
                c.access_zone_id,
                c.document_id,
                c.document_version,
                qdrant_scroll_batch_size,
            )
            .await
            .map_err(|e| IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::QdrantScroll, e))?;

        let expected_set: HashSet<Uuid> = point_ids.iter().copied().collect();
        let extra_candidates = qdrant_seen
            .iter()
            .copied()
            .filter(|id| !expected_set.contains(id))
            .collect::<Vec<_>>();
        if !extra_candidates.is_empty() {
            counter!("qdrant_cleanup_extra_points_detected_total")
                .increment(extra_candidates.len() as u64);
            let classified_extras = repo
                .filter_deletable_qdrant_points_for_document(
                    c.access_zone_id,
                    c.document_id,
                    c.document_version,
                    &extra_candidates,
                )
                .await
                .map_err(|e| {
                    IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::QdrantScroll, e)
                })?;
            if !classified_extras.skipped_legal_hold.is_empty() {
                counter!("qdrant_cleanup_extra_points_skipped_legal_hold_total")
                    .increment(classified_extras.skipped_legal_hold.len() as u64);
            }
            if !classified_extras.orphan.is_empty() {
                counter!("qdrant_cleanup_orphan_points_deleted_total")
                    .increment(classified_extras.orphan.len() as u64);
            }
            for extra in classified_extras.deletable {
                point_ids.push(extra);
                counter!("qdrant_cleanup_extra_points_deleted_total").increment(1);
            }
        }
    } else {
        counter!("qdrant_cleanup_reconciliation_skipped_total", "reason" => "disabled_by_config")
            .increment(1);
    }
    let mut deleted_points = 0u64;
    for batch in point_ids.chunks(qdrant_delete_batch_size.max(1)) {
        counter!("qdrant_points_delete_requested_total").increment(batch.len() as u64);
        qdrant
            .delete_points_batch(batch)
            .await
            .map_err(|e| IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::QdrantDelete, e))?;
        deleted_points += batch.len() as u64;
    }

    // Idempotency contract: if Qdrant already has no points for this document version,
    // cleanup is still successful and PostgreSQL lifecycle can progress to DELETED.
    if deleted_points == 0 {
        counter!("index_ttl_cleanup_qdrant_points_already_absent_total").increment(1);
    }

    let mut tx = repo.pool.begin().await.map_err(|e| {
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::DocumentVersionUpdate, db(e))
    })?;

    let document_update = sqlx::query(
        "UPDATE astravector.document_versions
         SET status='DELETED',
             lifecycle_status='DELETED',
             deleted_at=now(),
             delete_operation_id=NULL,
             delete_fencing_started_at=NULL,
             updated_at=now()
         WHERE access_zone_id=$1
           AND document_id=$2
           AND document_version=$3
           AND lifecycle_status='DELETING'
           AND delete_operation_id=$4",
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .bind(delete_operation_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::DocumentVersionUpdate, db(e))
    })?;
    if document_update.rows_affected() != 1 {
        counter!("index_ttl_cleanup_concurrent_state_change_total").increment(1);
        tx.rollback().await.map_err(|e| {
            IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::DocumentVersionUpdate, db(e))
        })?;
        return Err(IndexTtlCleanupError::new(
            IndexTtlCleanupStage::DocumentVersionUpdate,
            "document version lost fix461 delete_operation_id fencing during child cleanup",
        ));
    }

    sqlx::query(
        "UPDATE astravector.content_chunks_v004
         SET lifecycle_status='DELETED', deleted_at=now(), updated_at=now()
         WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3",
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::ContentChunksUpdate, db(e))
    })?;

    sqlx::query(
        "UPDATE astravector.vector_bindings_v004
         SET lifecycle_status='DELETED',
             qdrant_sync_status='DELETED',
             deleted_at=now(),
             updated_at=now()
         WHERE access_zone_id=$1
           AND document_id=$2
           AND document_version=$3
           AND COALESCE(legal_hold,false)=false",
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::DocumentVersionUpdate, db(e))
    })?;

    sqlx::query(
        "UPDATE astravector.rag_graph_nodes
         SET lifecycle_status='DELETED', deleted_at=now()
         WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3"
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        counter!("index_ttl_graph_cleanup_failed_total", "object" => "nodes", "stage" => IndexTtlCleanupStage::GraphNodesUpdate.as_label()).increment(1);
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::GraphNodesUpdate, db(e))
    })?;

    sqlx::query(
        "UPDATE astravector.rag_graph_edges
         SET lifecycle_status='DELETED', deleted_at=now()
         WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3"
    )
    .bind(c.access_zone_id)
    .bind(c.document_id)
    .bind(c.document_version)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        counter!("index_ttl_graph_cleanup_failed_total", "object" => "edges", "stage" => IndexTtlCleanupStage::GraphEdgesUpdate.as_label()).increment(1);
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::GraphEdgesUpdate, db(e))
    })?;

    tx.commit().await.map_err(|e| {
        IndexTtlCleanupError::from_astra(IndexTtlCleanupStage::DocumentVersionUpdate, db(e))
    })?;
    Ok(deleted_points)
}

pub async fn index_ttl_backlog_count(repo: &Repository) -> Result<i64, AstraError> {
    let row = sqlx::query(
        "SELECT count(*) AS cnt FROM astravector.document_versions \
         WHERE ((lifecycle_status IN ('EXPIRED','SUPERSEDED','DELETE_FAILED')) \
            OR (lifecycle_status='ACTIVE' AND expires_at IS NOT NULL AND expires_at <= now())) \
           AND lifecycle_status <> 'DELETE_PERMANENTLY_FAILED'",
    )
    .fetch_one(&repo.pool)
    .await
    .map_err(db)?;
    Ok(row.get::<i64, _>("cnt"))
}

pub async fn index_ttl_oldest_expired_age_seconds(repo: &Repository) -> Result<i64, AstraError> {
    let row = sqlx::query(
        "SELECT COALESCE(extract(epoch FROM now() - min(expires_at))::bigint, 0) AS age \
         FROM astravector.document_versions \
         WHERE lifecycle_status='ACTIVE' AND expires_at IS NOT NULL AND expires_at <= now()",
    )
    .fetch_one(&repo.pool)
    .await
    .map_err(db)?;
    Ok(row.get::<i64, _>("age"))
}

pub async fn index_ttl_permanently_failed_count(repo: &Repository) -> Result<i64, AstraError> {
    let row = sqlx::query(
        "SELECT count(*) AS cnt FROM astravector.document_versions WHERE lifecycle_status='DELETE_PERMANENTLY_FAILED'"
    )
    .fetch_one(&repo.pool)
    .await
    .map_err(db)?;
    Ok(row.get::<i64, _>("cnt"))
}

pub async fn run_index_ttl_cleanup_batch(
    repo: &Repository,
    qdrant: &crate::qdrant::QdrantClient,
    cleanup_batch_size: usize,
    qdrant_delete_batch_size: usize,
    delete_failed_retry_after_seconds: u64,
    deleting_stale_timeout_seconds: u64,
    max_delete_attempts: u32,
    retry_initial_delay_seconds: u64,
    retry_max_delay_seconds: u64,
    qdrant_scroll_batch_size: u64,
    qdrant_reconciliation_enabled: bool,
) -> Result<IndexTtlCleanupStats, AstraError> {
    let started = std::time::Instant::now();
    mark_stale_deleting_documents(repo, deleting_stale_timeout_seconds).await?;
    let candidates = claim_index_ttl_cleanup_batch(
        repo,
        cleanup_batch_size,
        delete_failed_retry_after_seconds,
        max_delete_attempts,
    )
    .await?;
    let mut stats = IndexTtlCleanupStats {
        claimed_documents: candidates.len() as u64,
        ..Default::default()
    };
    counter!("index_ttl_cleanup_batches_total").increment(1);
    gauge!("index_ttl_claimed_documents").set(candidates.len() as f64);
    if let Ok(backlog) = index_ttl_backlog_count(repo).await {
        gauge!("index_ttl_backlog_documents_total").set(backlog as f64);
    }
    if let Ok(age) = index_ttl_oldest_expired_age_seconds(repo).await {
        gauge!("index_ttl_oldest_expired_age_seconds").set(age as f64);
    }
    if let Ok(dead) = index_ttl_permanently_failed_count(repo).await {
        gauge!("index_ttl_delete_permanently_failed_documents").set(dead as f64);
    }

    for c in candidates {
        match cleanup_one_index_ttl_document(
            repo,
            qdrant,
            &c,
            qdrant_delete_batch_size,
            qdrant_scroll_batch_size,
            qdrant_reconciliation_enabled,
        )
        .await
        {
            Ok(deleted_points) => {
                stats.qdrant_points_deleted += deleted_points;
                stats.deleted_documents += 1;
                counter!("index_ttl_cleanup_documents_deleted_total").increment(1);
            }
            Err(e) => {
                let code = e.error_code();
                let stage = e.stage_label();
                counter!("index_ttl_cleanup_stage_failed_total", "stage" => stage, "error_code" => code).increment(1);
                mark_index_ttl_delete_failed(
                    repo,
                    &c,
                    code,
                    e.to_string(),
                    max_delete_attempts,
                    retry_initial_delay_seconds,
                    retry_max_delay_seconds,
                )
                .await?;
                stats.delete_failed_documents += 1;
            }
        }
    }
    counter!("index_ttl_cleanup_qdrant_points_deleted_total")
        .increment(stats.qdrant_points_deleted);
    metrics::histogram!("index_ttl_cleanup_duration_ms")
        .record(started.elapsed().as_millis() as f64);
    Ok(stats)
}

pub async fn purge_index_ttl_tombstones(
    repo: &Repository,
    keep_tombstone_days: u32,
    limit: i64,
) -> Result<u64, AstraError> {
    let mut tx = repo.pool.begin().await.map_err(db)?;
    let rows = sqlx::query(
        "SELECT access_zone_id, document_id, document_version
         FROM astravector.document_versions
         WHERE lifecycle_status='DELETED'
           AND deleted_at < now() - ($1 * interval '1 day')
           AND NOT EXISTS (
             SELECT 1
             FROM astravector.vector_outbox o
             JOIN astravector.vector_bindings_v004 b
               ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id
             WHERE b.access_zone_id=document_versions.access_zone_id
               AND b.document_id=document_versions.document_id
               AND b.document_version=document_versions.document_version
               AND o.status IN('PENDING','PROCESSING','RETRY_PENDING')
           )
         ORDER BY deleted_at
         LIMIT $2
         FOR UPDATE SKIP LOCKED",
    )
    .bind(keep_tombstone_days as i64)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(db)?;

    let mut purged = 0u64;
    for r in rows {
        let zone: Uuid = r.get("access_zone_id");
        let document_id: Uuid = r.get("document_id");
        let version: i64 = r.get("document_version");
        sqlx::query("DELETE FROM astravector.vector_outbox o USING astravector.vector_bindings_v004 b WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND b.access_zone_id=$1 AND b.document_id=$2 AND b.document_version=$3 AND o.status IN('COMPLETED','DEAD_LETTER')")
            .bind(zone).bind(document_id).bind(version).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND lifecycle_status='DELETED'")
            .bind(zone).bind(document_id).bind(version).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM astravector.rag_graph_edges WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(zone).bind(document_id).bind(version).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM astravector.rag_graph_nodes WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(zone).bind(document_id).bind(version).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM astravector.content_chunks_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(zone).bind(document_id).bind(version).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3")
            .bind(zone).bind(document_id).bind(version).execute(&mut *tx).await.map_err(db)?;
        purged += 1;
    }
    tx.commit().await.map_err(db)?;
    if purged > 0 {
        counter!("index_ttl_tombstones_purged_total").increment(purged);
    }
    Ok(purged)
}

#[cfg(test)]
mod tests {
    use super::IndexTtlCleanupStage;

    #[test]
    fn index_ttl_cleanup_stage_maps_to_stable_error_codes() {
        assert_eq!(
            IndexTtlCleanupStage::QdrantScroll.error_code(),
            "QDRANT_SCROLL_FAILED"
        );
        assert_eq!(
            IndexTtlCleanupStage::QdrantDelete.error_code(),
            "QDRANT_DELETE_FAILED"
        );
        assert_eq!(
            IndexTtlCleanupStage::GraphNodesUpdate.error_code(),
            "GRAPH_NODES_CLEANUP_FAILED"
        );
        assert_eq!(
            IndexTtlCleanupStage::GraphEdgesUpdate.error_code(),
            "GRAPH_EDGES_CLEANUP_FAILED"
        );
        assert_eq!(
            IndexTtlCleanupStage::ContentChunksUpdate.error_code(),
            "CONTENT_CHUNKS_CLEANUP_FAILED"
        );
        assert_eq!(
            IndexTtlCleanupStage::DocumentVersionUpdate.error_code(),
            "DOCUMENT_VERSION_CLEANUP_FAILED"
        );
        assert_eq!(
            IndexTtlCleanupStage::TombstonePurge.error_code(),
            "TOMBSTONE_PURGE_FAILED"
        );
    }

    #[test]
    fn qdrant_empty_point_list_is_documented_idempotent_success() {
        // Regression guard for fix4.5.7: cleanup treats an empty Qdrant point list as a
        // successful idempotent delete and still progresses PostgreSQL lifecycle to DELETED.
        let deleted_points = 0u64;
        assert_eq!(deleted_points, 0);
    }
}
