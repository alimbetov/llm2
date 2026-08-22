use crate::{
    adaptive::AdaptiveRuntime, error::AstraError, persistence::Repository,
    projection::CanonicalProjectionInput, qdrant::QdrantClient, recovery,
};
use metrics::{counter, gauge, histogram};
use serde_json::json;
use sqlx::Row;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
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
    adaptive: Option<Arc<AdaptiveRuntime>>,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            let effective_poll_ms = adaptive
                .as_ref()
                .map(|a| a.get_u64("outbox.poll_interval_ms", poll_ms.max(100)))
                .unwrap_or(poll_ms.max(100))
                .max(100);
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_millis(effective_poll_ms)) => {
                    let effective_batch_size = adaptive
                        .as_ref()
                        .map(|a| a.get_i64("publisher.batch_size", batch_size.max(1)))
                        .unwrap_or(batch_size.max(1))
                        .max(1);
                    match claim_batch(&repo, &instance_id, effective_batch_size, 60).await {
                        Ok(events) => {
                            gauge!("astravector_qdrant_outbox_claimed").set(events.len() as f64);
                            if let Some(a) = &adaptive {
                                a.observe_outbox_claim(events.len(), batch_size.max(1));
                            }
                            for event in events {
                                let started = std::time::Instant::now();
                                match process_event(&repo, &qdrant, &event).await {
                                    Ok(()) => {
                                        counter!("astravector_qdrant_events_total", "operation" => event.operation.clone(), "result" => "success").increment(1);
                                        if let Err(e) = complete(&repo, &event).await {
                                            error!(event_id=%event.id,error=%e,"outbox complete update failed; event remains reclaimable");
                                            counter!("astravector_outbox_complete_failures_total").increment(1)
                                        }
                                    }
                                    Err(e) => {
                                        counter!("astravector_qdrant_failures_total", "operation" => event.operation.clone()).increment(1);
                                        if let Some(a) = &adaptive {
                                            a.observe_outbox_error(batch_size.max(1));
                                        }
                                        if let Err(db_error) = fail(&repo, &event, e.to_string(), max_attempts).await {
                                            error!(event_id=%event.id,error=%db_error,"outbox failure state update failed; event remains reclaimable");
                                            counter!("astravector_outbox_fail_update_failures_total").increment(1)
                                        }
                                    }
                                }
                                histogram!("astravector_qdrant_sync_duration_seconds").record(started.elapsed().as_secs_f64());
                            }
                        }
                        Err(e) => {
                            warn!(error=%e,"outbox claim failed");
                            if let Some(a) = &adaptive {
                                a.observe_outbox_error(batch_size.max(1));
                            }
                            counter!("astravector_qdrant_failures_total", "operation" => "CLAIM").increment(1)
                        }
                    }
                }
            }
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
    let row=sqlx::query("SELECT b.qdrant_point_id,b.document_id,b.document_version,b.root_chunk_id,b.source_chunk_id,b.parent_chunk_id,b.chunk_id,b.chunk_granularity,b.representation_type,b.access_level,b.expires_at,b.legal_hold,b.lifecycle_status,b.payload_version,b.ttl_generation,b.qdrant_sync_status,b.metadata,c.cache_key,c.model_version,c.tokenizer_version,c.dense_version,c.sparse_version,az.access_zone_code FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_cache_entries c ON c.id=b.cache_entry_id LEFT JOIN astravector.access_zones az ON az.access_zone_id=b.access_zone_id WHERE b.access_zone_id=$1 AND b.id=$2").bind(event.access_zone_id).bind(event.binding_id).fetch_optional(&repo.pool).await.map_err(db)?.ok_or_else(||AstraError::FailedPrecondition("binding not found".into()))?;
    let projection =
        CanonicalProjectionInput::from_pg_row(&row, event.access_zone_id, event.binding_id);
    let point_id = projection.qdrant_point_id;
    let lifecycle: String = row.get("lifecycle_status");
    let current_payload_version: i64 = row.get("payload_version");
    let current_ttl_generation: i64 = row.get("ttl_generation");
    let qdrant_sync_status: String = row.get("qdrant_sync_status");
    let legal_hold: bool = row.get("legal_hold");
    let payload = projection.payload();
    match event.operation.as_str() {
        "UPSERT_POINT" => {
            if lifecycle != "ACTIVE"
                || !matches!(qdrant_sync_status.as_str(), "PENDING" | "UPDATE_PENDING")
                || current_payload_version != event.operation_version
            {
                counter!("vector_outbox_stale_event_skipped_total", "operation" => "UPSERT_POINT")
                    .increment(1);
                counter!("vector_outbox_binding_version_mismatch_total", "operation" => "UPSERT_POINT").increment((current_payload_version != event.operation_version) as u64);
                counter!("vector_outbox_binding_lifecycle_mismatch_total", "operation" => "UPSERT_POINT", "lifecycle" => lifecycle.clone()).increment((lifecycle != "ACTIVE") as u64);
                return Ok(());
            }
            let fence = recovery::acquire_qdrant_projection_write_fence(&repo.pool).await?;
            upsert_from_cache(repo, q, &row, &projection).await?;
            mark_synced(repo, event).await?;
            fence.commit().await.map_err(db)?
        }
        "UPDATE_PAYLOAD" => {
            if lifecycle != "ACTIVE" || current_payload_version != event.operation_version {
                counter!("vector_outbox_stale_event_skipped_total", "operation" => "UPDATE_PAYLOAD").increment(1);
                counter!("vector_outbox_binding_version_mismatch_total", "operation" => "UPDATE_PAYLOAD").increment((current_payload_version != event.operation_version) as u64);
                return Ok(());
            }
            let fence = recovery::acquire_qdrant_projection_write_fence(&repo.pool).await?;
            if q.point_exists(point_id).await? {
                q.update_payload(point_id, payload).await?
            } else {
                counter!("astravector_qdrant_update_fallback_total").increment(1);
                upsert_from_cache(repo, q, &row, &projection).await?
            }
            mark_synced(repo, event).await?;
            fence.commit().await.map_err(db)?
        }
        "DELETE_POINT" => {
            if legal_hold {
                counter!("vector_outbox_stale_event_skipped_total", "operation" => "DELETE_POINT", "reason" => "legal_hold").increment(1);
                info!(event_id=%event.id, binding_id=%event.binding_id, access_zone_id=%event.access_zone_id, operation=%event.operation, event_operation_version=event.operation_version, current_ttl_generation, qdrant_sync_status=%qdrant_sync_status, legal_hold, "OUTBOX_STALE_DELETE_SKIPPED_LEGAL_HOLD");
                return Ok(());
            }
            if current_ttl_generation != event.operation_version
                || !matches!(
                    qdrant_sync_status.as_str(),
                    "DELETE_PENDING" | "DELETION_PENDING" | "DELETE_IN_PROGRESS"
                )
            {
                counter!("vector_outbox_stale_event_skipped_total", "operation" => "DELETE_POINT")
                    .increment(1);
                counter!("vector_outbox_binding_version_mismatch_total", "operation" => "DELETE_POINT").increment((current_ttl_generation != event.operation_version) as u64);
                counter!("vector_outbox_binding_lifecycle_mismatch_total", "operation" => "DELETE_POINT", "lifecycle" => lifecycle.clone()).increment(1);
                info!(event_id=%event.id, binding_id=%event.binding_id, access_zone_id=%event.access_zone_id, operation=%event.operation, event_operation_version=event.operation_version, current_payload_version, current_ttl_generation, lifecycle=%lifecycle, qdrant_sync_status=%qdrant_sync_status, legal_hold, "OUTBOX_STALE_DELETE_SKIPPED_BY_BINDING_FENCE");
                return Ok(());
            }
            let fence = recovery::acquire_qdrant_projection_write_fence(&repo.pool).await?;
            let claimed = sqlx::query("UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='DELETE_IN_PROGRESS',updated_at=now() WHERE access_zone_id=$1 AND id=$2 AND ttl_generation=$3 AND legal_hold=false AND qdrant_sync_status IN('DELETE_PENDING','DELETION_PENDING','DELETE_IN_PROGRESS') RETURNING qdrant_point_id")
                .bind(event.access_zone_id)
                .bind(event.binding_id)
                .bind(event.operation_version)
                .fetch_optional(&repo.pool)
                .await
                .map_err(db)?;
            let Some(claimed) = claimed else {
                counter!("vector_outbox_stale_event_skipped_total", "operation" => "DELETE_POINT", "reason" => "claim_rejected").increment(1);
                info!(event_id=%event.id, binding_id=%event.binding_id, access_zone_id=%event.access_zone_id, event_operation_version=event.operation_version, "OUTBOX_DELETE_POINT_CLAIM_REJECTED");
                return Err(AstraError::OwnershipLost(
                    "DELETE_POINT binding claim rejected by current DB state".into(),
                ));
            };
            let claimed_point_id: Uuid = claimed.get("qdrant_point_id");
            q.delete(claimed_point_id).await?;
            let rows = sqlx::query("UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='DELETED',lifecycle_status='SOFT_DELETED',deleted_at=now(),updated_at=now() WHERE access_zone_id=$1 AND id=$2 AND ttl_generation=$3 AND qdrant_sync_status='DELETE_IN_PROGRESS' AND legal_hold=false")
                .bind(event.access_zone_id)
                .bind(event.binding_id)
                .bind(event.operation_version)
                .execute(&repo.pool)
                .await
                .map_err(db)?
                .rows_affected();
            if rows != 1 {
                counter!("vector_outbox_binding_version_mismatch_total", "operation" => "DELETE_POINT_FINALIZE").increment(1);
                return Err(AstraError::OwnershipLost(
                    "DELETE_POINT final DB update rejected by binding fence".into(),
                ));
            }
            fence.commit().await.map_err(db)?
        }
        "QUARANTINE_POINT" => {
            let fence = recovery::acquire_qdrant_projection_write_fence(&repo.pool).await?;
            let mut p = payload;
            p["lifecycle_status"] = json!("ORPHAN_QUARANTINED");
            q.update_payload(point_id, p).await?;
            fence.commit().await.map_err(db)?
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
    projection: &CanonicalProjectionInput,
) -> Result<(), AstraError> {
    let key: String = row.get("cache_key");
    let r = repo
        .load_completed(&key)
        .await?
        .ok_or_else(|| AstraError::Internal("canonical vector missing".into()))?;
    if let Some(dense) = r.dense.as_ref() {
        q.ensure_collection(dense.len()).await?;
    }
    q.upsert(&projection.point(r)).await
}
async fn mark_synced(repo: &Repository, event: &OutboxEvent) -> Result<(), AstraError> {
    let rows = sqlx::query("UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='SYNCED',last_qdrant_sync_version=payload_version,updated_at=now() WHERE access_zone_id=$1 AND id=$2 AND payload_version=$3 AND lifecycle_status='ACTIVE'")
        .bind(event.access_zone_id)
        .bind(event.binding_id)
        .bind(event.operation_version)
        .execute(&repo.pool)
        .await
        .map_err(db)?
        .rows_affected();
    if rows != 1 {
        counter!("vector_outbox_binding_version_mismatch_total", "operation" => event.operation.clone()).increment(1);
        counter!("vector_outbox_mark_synced_rejected_total", "operation" => event.operation.clone()).increment(1);
        return Err(AstraError::OwnershipLost(
            "mark_synced rejected by binding version/lifecycle fence".into(),
        ));
    }
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
