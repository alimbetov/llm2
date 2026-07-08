use crate::{config::RecoveryConfig, persistence::Repository};
use metrics::counter;
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

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
