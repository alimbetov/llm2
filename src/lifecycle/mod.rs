use crate::{error::AstraError, persistence::Repository};
use metrics::{counter, gauge};
use sqlx::Row;
use std::time::Duration;
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
pub async fn expire_batch(repo: &Repository, limit: i64) -> Result<usize, AstraError> {
    let mut tx = repo.pool.begin().await.map_err(db)?;
    let rows=sqlx::query("SELECT access_zone_id,id,ttl_generation FROM astravector.vector_bindings_v004 WHERE lifecycle_status='ACTIVE' AND expires_at<=now() AND legal_hold=false ORDER BY expires_at FOR UPDATE SKIP LOCKED LIMIT $1").bind(limit).fetch_all(&mut*tx).await.map_err(db)?;
    let mut affected = 0usize;
    for r in rows {
        let zone: Uuid = r.get("access_zone_id");
        let id: Uuid = r.get("id");
        let generation: i64 = r.get("ttl_generation");
        let n=sqlx::query("UPDATE astravector.vector_bindings_v004 SET lifecycle_status='DELETION_PENDING',qdrant_sync_status='DELETE_PENDING',expired_at=now(),updated_at=now() WHERE access_zone_id=$1 AND id=$2 AND ttl_generation=$3 AND legal_hold=false AND lifecycle_status='ACTIVE'").bind(zone).bind(id).bind(generation).execute(&mut*tx).await.map_err(db)?.rows_affected();
        if n == 1 {
            sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'DELETE_POINT',$4,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING").bind(Uuid::new_v4()).bind(zone).bind(id).bind(generation+1).execute(&mut*tx).await.map_err(db)?;
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
    let rows=sqlx::query("UPDATE astravector.vector_bindings_v004 SET ttl_days=$3,expires_at=CASE WHEN $3 IS NULL THEN NULL ELSE now()+($3*interval '1 day') END,ttl_generation=ttl_generation+1,payload_version=payload_version+1,qdrant_sync_status='UPDATE_PENDING',updated_at=now() WHERE access_zone_id=$1 AND root_chunk_id=$2 RETURNING id,payload_version").bind(zone).bind(root).bind(ttl).fetch_all(&mut*tx).await.map_err(db)?;
    for r in &rows {
        sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'UPDATE_PAYLOAD',$4,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING").bind(Uuid::new_v4()).bind(zone).bind(r.get::<Uuid,_>("id")).bind(r.get::<i64,_>("payload_version")).execute(&mut*tx).await.map_err(db)?;
    }
    tx.commit().await.map_err(db)?;
    Ok(rows.len() as u64)
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}
