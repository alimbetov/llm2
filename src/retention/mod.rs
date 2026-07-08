use crate::{config::RetentionConfig, persistence::Repository};
use metrics::{counter, gauge};
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

pub fn spawn(repo: Repository, cfg: RetentionConfig, shutdown: CancellationToken) {
    tokio::spawn(async move {
        let mut t = interval(Duration::from_secs(3600));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("retention worker shutdown requested");
                    break;
                }
                _ = t.tick() => {
                    match sqlx::query("DELETE FROM astravector.embedding_requests WHERE id IN(SELECT id FROM astravector.embedding_requests WHERE (purpose='Query' AND created_at<now()-($1 * interval '1 day')) OR (purpose<>'Query' AND created_at<now()-($2 * interval '1 day')) OR (status='FAILED' AND created_at<now()-($3 * interval '1 day')) ORDER BY created_at LIMIT $4)")
                        .bind(cfg.query_requests_days).bind(cfg.document_requests_days).bind(cfg.failed_requests_days).bind(cfg.delete_batch_size).execute(&repo.pool).await {
                            Ok(done) => { counter!("retention_deleted_total", "table" => "embedding_requests").increment(done.rows_affected()); }
                            Err(error) => { counter!("retention_errors_total", "table" => "embedding_requests").increment(1); tracing::warn!(%error, "retention cleanup failed for embedding_requests"); }
                    }
                    match sqlx::query("DELETE FROM astravector.embedding_cache_entries WHERE id IN(SELECT id FROM astravector.embedding_cache_entries WHERE status<>'PROCESSING' AND last_accessed_at<now()-($1 * interval '1 day') ORDER BY last_accessed_at LIMIT $2 FOR UPDATE SKIP LOCKED)")
                        .bind(cfg.cache_unused_days).bind(cfg.delete_batch_size).execute(&repo.pool).await {
                            Ok(done) => { counter!("retention_deleted_total", "table" => "embedding_cache_entries").increment(done.rows_affected()); gauge!("retention_last_cache_entries_deleted").set(done.rows_affected() as f64); }
                            Err(error) => { counter!("retention_errors_total", "table" => "embedding_cache_entries").increment(1); tracing::warn!(%error, "retention cleanup failed for embedding_cache_entries"); }
                    }
                }
            }
        }
    });
}
