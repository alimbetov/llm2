use crate::{config::RecoveryConfig, error::AstraError, persistence::Repository};
use metrics::counter;
use sqlx::{pool::PoolConnection, PgPool, Postgres, Transaction};
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

pub mod postgres;

const QDRANT_PROJECTION_FENCE_CLASS_ID: i32 = 491;
const QDRANT_PROJECTION_FENCE_OBJECT_ID: i32 = 1;

pub async fn acquire_qdrant_projection_write_fence<'a>(
    pool: &'a PgPool,
) -> Result<Transaction<'a, Postgres>, AstraError> {
    let mut tx = pool.begin().await.map_err(db)?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock_shared($1,$2)")
        .bind(QDRANT_PROJECTION_FENCE_CLASS_ID)
        .bind(QDRANT_PROJECTION_FENCE_OBJECT_ID)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    if !acquired {
        return Err(AstraError::ResourceExhausted(
            "QDRANT_RECOVERY_FENCE_ACTIVE: projection write rejected while recovery holds fence"
                .into(),
        ));
    }
    Ok(tx)
}

pub async fn acquire_qdrant_recovery_exclusive_fence(
    pool: &PgPool,
) -> Result<QdrantRecoveryFence, AstraError> {
    let mut conn = pool.acquire().await.map_err(db)?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1,$2)")
        .bind(QDRANT_PROJECTION_FENCE_CLASS_ID)
        .bind(QDRANT_PROJECTION_FENCE_OBJECT_ID)
        .fetch_one(&mut *conn)
        .await
        .map_err(db)?;
    if !acquired {
        return Err(AstraError::ResourceExhausted(
            "QDRANT_RECOVERY_FENCE_BUSY: another projection writer or recovery is active".into(),
        ));
    }
    Ok(QdrantRecoveryFence { conn: Some(conn) })
}

pub struct QdrantRecoveryFence {
    conn: Option<PoolConnection<Postgres>>,
}

impl QdrantRecoveryFence {
    pub async fn release(mut self) -> Result<(), AstraError> {
        let Some(mut conn) = self.conn.take() else {
            return Ok(());
        };
        let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1,$2)")
            .bind(QDRANT_PROJECTION_FENCE_CLASS_ID)
            .bind(QDRANT_PROJECTION_FENCE_OBJECT_ID)
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
        if released {
            Ok(())
        } else {
            Err(AstraError::FailedPrecondition(
                "QDRANT_RECOVERY_FENCE_RELEASE_FAILED: advisory lock was not held".into(),
            ))
        }
    }
}

pub fn spawn(repo: Repository, cfg: RecoveryConfig, shutdown: CancellationToken) {
    tokio::spawn(async move {
        let mut t = interval(Duration::from_secs(cfg.interval_seconds));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("recovery worker shutdown requested");
                    break;
                }
                _ = t.tick() => {
                    match sqlx::query(
                        "WITH stale AS(SELECT id FROM astravector.embedding_cache_entries WHERE status='PROCESSING' AND lease_expires_at<now() ORDER BY lease_expires_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE astravector.embedding_cache_entries c SET status='FAILED',error_code='LEASE_EXPIRED',error_message='Processing lease expired' FROM stale WHERE c.id=stale.id"
                    )
                    .bind(cfg.batch_size)
                    .execute(&repo.pool)
                    .await
                    {
                        Ok(done) if done.rows_affected() > 0 => {
                            counter!("embedding_cache_stale_recovered_total").increment(done.rows_affected());
                        }
                        Ok(_) => {}
                        Err(error) => {
                            counter!("embedding_cache_recovery_errors_total").increment(1);
                            tracing::warn!(%error, "embedding cache recovery failed");
                        }
                    }

                    match sqlx::query(
                        "UPDATE astravector.document_versions
                         SET status='FAILED',
                             processing_owner_id=NULL,
                             processing_heartbeat_at=NULL,
                             updated_at=now()
                         WHERE status='INDEXING'
                           AND processing_owner_id IS NOT NULL
                           AND processing_heartbeat_at < now() - ($1 * interval '1 second')
                           AND delete_operation_id IS NULL"
                    )
                    .bind(cfg.processing_timeout_seconds)
                    .execute(&repo.pool)
                    .await
                    {
                        Ok(done) if done.rows_affected() > 0 => {
                            counter!("document_indexing_stale_recovered_total").increment(done.rows_affected());
                        }
                        Ok(_) => {}
                        Err(error) => {
                            counter!("document_indexing_recovery_errors_total").increment(1);
                            tracing::warn!(%error, "document indexing recovery failed");
                        }
                    }
                }
            }
        }
    });
}

fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}
