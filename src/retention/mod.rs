use crate::{config::RetentionConfig, persistence::Repository};
use tokio::time::{interval, Duration};
pub fn spawn(repo: Repository, cfg: RetentionConfig) {
    tokio::spawn(async move {
        let mut t = interval(Duration::from_secs(3600));
        loop {
            t.tick().await;
            let _=sqlx::query("DELETE FROM astravector.embedding_requests WHERE id IN(SELECT id FROM astravector.embedding_requests WHERE (purpose='Query' AND created_at<now()-($1 * interval '1 day')) OR (purpose<>'Query' AND created_at<now()-($2 * interval '1 day')) OR (status='FAILED' AND created_at<now()-($3 * interval '1 day')) ORDER BY created_at LIMIT $4)").bind(cfg.query_requests_days).bind(cfg.document_requests_days).bind(cfg.failed_requests_days).bind(cfg.delete_batch_size).execute(&repo.pool).await;
            let _=sqlx::query("DELETE FROM astravector.embedding_cache_entries WHERE id IN(SELECT id FROM astravector.embedding_cache_entries WHERE status<>'PROCESSING' AND last_accessed_at<now()-($1 * interval '1 day') ORDER BY last_accessed_at LIMIT $2 FOR UPDATE SKIP LOCKED)").bind(cfg.cache_unused_days).bind(cfg.delete_batch_size).execute(&repo.pool).await;
        }
    });
}
