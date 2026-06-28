use crate::{
    error::AstraError,
    inference::EmbeddingResult,
    persistence::Repository,
    qdrant::{QdrantClient, QdrantPoint},
};
use chrono::{DateTime, SecondsFormat, Utc};
use metrics::{counter, gauge, histogram};
use serde_json::json;
use sqlx::Row;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};
use uuid::Uuid;
#[derive(Debug, Clone)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub access_zone_id: Uuid,
    pub binding_id: Uuid,
    pub operation: String,
    pub operation_version: i64,
    pub attempt_count: i32,
    pub locked_by: String,
    pub lock_generation: i64,
}
pub fn spawn(
    repo: Repository,
    qdrant: Arc<QdrantClient>,
    instance_id: String,
    batch_size: i64,
    poll_ms: u64,
    max_attempts: i32,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(poll_ms.max(100)));
        loop {
            tokio::select! {_=shutdown.cancelled()=>break,_=interval.tick()=>match claim_batch(&repo,&instance_id,batch_size,60).await{Ok(events)=>{gauge!("astravector_qdrant_outbox_claimed").set(events.len() as f64);for event in events{let started=std::time::Instant::now();match process_event(&repo,&qdrant,&event).await{Ok(())=>{counter!("astravector_qdrant_events_total","operation"=>event.operation.clone(),"result"=>"success").increment(1);if let Err(e)=complete(&repo,&event).await{error!(event_id=%event.id,error=%e,"outbox complete update failed; event remains reclaimable");counter!("astravector_outbox_complete_failures_total").increment(1)}},Err(e)=>{counter!("astravector_qdrant_failures_total","operation"=>event.operation.clone()).increment(1);if let Err(db_error)=fail(&repo,&event,e.to_string(),max_attempts).await{error!(event_id=%event.id,error=%db_error,"outbox failure state update failed; event remains reclaimable");counter!("astravector_outbox_fail_update_failures_total").increment(1)}}}histogram!("astravector_qdrant_sync_duration_seconds").record(started.elapsed().as_secs_f64());}},Err(e)=>{warn!(error=%e,"outbox claim failed");counter!("astravector_qdrant_failures_total","operation"=>"CLAIM").increment(1)}}}
        }
    });
}
pub async fn claim_batch(
    repo: &Repository,
    instance: &str,
    limit: i64,
    lock_seconds: i64,
) -> Result<Vec<OutboxEvent>, AstraError> {
    let mut tx = repo.pool.begin().await.map_err(db)?;
    let rows=sqlx::query(r#"WITH candidates AS (SELECT id FROM astravector.vector_outbox WHERE ((status IN('PENDING','RETRY_PENDING') AND next_attempt_at<=now()) OR (status='PROCESSING' AND locked_until<now())) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE astravector.vector_outbox o SET status='PROCESSING',locked_by=$2,locked_until=now()+($3*interval '1 second'),lock_generation=lock_generation+1,attempt_count=attempt_count+1,reclaim_count=reclaim_count+CASE WHEN o.status='PROCESSING' THEN 1 ELSE 0 END,last_started_at=now(),updated_at=now() FROM candidates c WHERE o.id=c.id RETURNING o.id,o.binding_access_zone_id,o.binding_id,o.operation,o.operation_version,o.attempt_count,o.locked_by,o.lock_generation"#).bind(limit).bind(instance).bind(lock_seconds).fetch_all(&mut*tx).await.map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(rows
        .into_iter()
        .map(|r| OutboxEvent {
            id: r.get("id"),
            access_zone_id: r.get("binding_access_zone_id"),
            binding_id: r.get("binding_id"),
            operation: r.get("operation"),
            operation_version: r.get("operation_version"),
            attempt_count: r.get("attempt_count"),
            locked_by: r.get("locked_by"),
            lock_generation: r.get("lock_generation"),
        })
        .collect())
}
async fn process_event(
    repo: &Repository,
    q: &QdrantClient,
    event: &OutboxEvent,
) -> Result<(), AstraError> {
    let row=sqlx::query("SELECT b.qdrant_point_id,b.document_id,b.document_version,b.root_chunk_id,b.source_chunk_id,b.parent_chunk_id,b.chunk_id,b.chunk_granularity,b.representation_type,b.access_level,b.expires_at,b.legal_hold,b.lifecycle_status,b.payload_version,b.qdrant_sync_status,b.metadata,c.cache_key,c.model_version,c.tokenizer_version FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_cache_entries c ON c.id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.id=$2").bind(event.access_zone_id).bind(event.binding_id).fetch_optional(&repo.pool).await.map_err(db)?.ok_or_else(||AstraError::FailedPrecondition("binding not found".into()))?;
    let point_id: Uuid = row.get("qdrant_point_id");
    let lifecycle: String = row.get("lifecycle_status");
    let metadata: serde_json::Value = row.get("metadata");
    let chunking_profile_version = metadata
        .get("chunking_profile_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let expires_at = row
        .try_get::<Option<DateTime<Utc>>, _>("expires_at")
        .ok()
        .flatten()
        .map(|x| x.to_rfc3339_opts(SecondsFormat::Secs, true));
    let payload = json!({"access_zone_id":event.access_zone_id,"binding_id":event.binding_id,"document_id":row.get::<Uuid,_>("document_id"),"document_version":row.get::<i64,_>("document_version"),"root_chunk_id":row.get::<Uuid,_>("root_chunk_id"),"source_chunk_id":row.get::<Uuid,_>("source_chunk_id"),"parent_chunk_id":row.try_get::<Option<Uuid>,_>("parent_chunk_id").ok().flatten(),"chunk_id":row.get::<Uuid,_>("chunk_id"),"chunk_granularity":row.get::<String,_>("chunk_granularity"),"representation_type":row.get::<String,_>("representation_type"),"access_level":row.get::<i16,_>("access_level"),"lifecycle_status":lifecycle,"expires_at":expires_at,"legal_hold":row.get::<bool,_>("legal_hold"),"payload_version":row.get::<i64,_>("payload_version"),"model_version":row.get::<String,_>("model_version"),"tokenizer_version":row.get::<String,_>("tokenizer_version"),"chunking_profile_version":chunking_profile_version});
    match event.operation.as_str() {
        "UPSERT_POINT" => {
            upsert_from_cache(repo, q, &row, point_id, payload).await?;
            mark_synced(repo, event).await?
        }
        "UPDATE_PAYLOAD" => {
            if q.point_exists(point_id).await? {
                q.update_payload(point_id, payload).await?
            } else if lifecycle == "ACTIVE" {
                counter!("astravector_qdrant_update_fallback_total").increment(1);
                upsert_from_cache(repo, q, &row, point_id, payload).await?
            } else {
                q.delete(point_id).await?
            }
            mark_synced(repo, event).await?
        }
        "DELETE_POINT" => {
            q.delete(point_id).await?;
            sqlx::query("UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='DELETED',lifecycle_status='SOFT_DELETED',deleted_at=now(),updated_at=now() WHERE access_zone_id=$1 AND id=$2").bind(event.access_zone_id).bind(event.binding_id).execute(&repo.pool).await.map_err(db)?;
        }
        "QUARANTINE_POINT" => {
            let mut p = payload;
            p["lifecycle_status"] = json!("ORPHAN_QUARANTINED");
            q.update_payload(point_id, p).await?
        }
        _ => {
            return Err(AstraError::InvalidArgument(
                "unknown outbox operation".into(),
            ))
        }
    }
    Ok(())
}
async fn upsert_from_cache(
    repo: &Repository,
    q: &QdrantClient,
    row: &sqlx::postgres::PgRow,
    point_id: Uuid,
    payload: serde_json::Value,
) -> Result<(), AstraError> {
    let key: String = row.get("cache_key");
    let r: EmbeddingResult = repo
        .load_completed(&key)
        .await?
        .ok_or_else(|| AstraError::Internal("canonical vector missing".into()))?;
    q.upsert(&QdrantPoint {
        id: point_id,
        dense: r.dense,
        sparse_indices: r.sparse_indices,
        sparse_values: r.sparse_values,
        payload,
    })
    .await
}
async fn mark_synced(repo: &Repository, event: &OutboxEvent) -> Result<(), AstraError> {
    sqlx::query("UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='SYNCED',last_qdrant_sync_version=payload_version,updated_at=now() WHERE access_zone_id=$1 AND id=$2").bind(event.access_zone_id).bind(event.binding_id).execute(&repo.pool).await.map_err(db)?;
    Ok(())
}
async fn complete(repo: &Repository, e: &OutboxEvent) -> Result<(), AstraError> {
    let rows = sqlx::query("UPDATE astravector.vector_outbox SET status='COMPLETED',completed_at=now(),last_finished_at=now(),locked_by=NULL,locked_until=NULL,updated_at=now() WHERE id=$1 AND status='PROCESSING' AND locked_by=$2 AND lock_generation=$3")
        .bind(e.id)
        .bind(&e.locked_by)
        .bind(e.lock_generation)
        .execute(&repo.pool)
        .await
        .map_err(db)?
        .rows_affected();
    if rows != 1 {
        return Err(AstraError::OwnershipLost(
            "outbox fencing complete rejected".into(),
        ));
    }
    Ok(())
}
async fn fail(
    repo: &Repository,
    e: &OutboxEvent,
    message: String,
    max: i32,
) -> Result<(), AstraError> {
    let dead = e.attempt_count >= max;
    let rows = sqlx::query("UPDATE astravector.vector_outbox SET status=$2,next_attempt_at=now()+make_interval(secs=>LEAST(300,POWER(2,LEAST(attempt_count,8))::int)),last_error_code='QDRANT_ERROR',last_error_message=$3,error_code='QDRANT_ERROR',error_message=$3,locked_by=NULL,locked_until=NULL,updated_at=now() WHERE id=$1 AND status='PROCESSING' AND locked_by=$4 AND lock_generation=$5")
        .bind(e.id)
        .bind(if dead { "DEAD_LETTER" } else { "RETRY_PENDING" })
        .bind(message)
        .bind(&e.locked_by)
        .bind(e.lock_generation)
        .execute(&repo.pool)
        .await
        .map_err(db)?
        .rows_affected();
    if rows != 1 {
        return Err(AstraError::OwnershipLost(
            "outbox fencing fail rejected".into(),
        ));
    }
    Ok(())
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}
