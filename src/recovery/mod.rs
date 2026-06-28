use crate::{config::RecoveryConfig, persistence::Repository};
use tokio::time::{interval, Duration};
pub fn spawn(repo: Repository, cfg: RecoveryConfig) {
    tokio::spawn(async move {
        let mut t = interval(Duration::from_secs(cfg.interval_seconds));
        loop {
            t.tick().await;
            let _=sqlx::query("WITH stale AS(SELECT id FROM astravector.embedding_cache_entries WHERE status='PROCESSING' AND lease_expires_at<now() ORDER BY lease_expires_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE astravector.embedding_cache_entries c SET status='FAILED',error_code='LEASE_EXPIRED',error_message='Processing lease expired' FROM stale WHERE c.id=stale.id").bind(cfg.batch_size).execute(&repo.pool).await;
        }
    });
}
