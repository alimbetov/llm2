use crate::{config::IngestionConfig, persistence::Repository};
use metrics::{counter, gauge, histogram};
use sqlx::Row;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub fn spawn(repo: Repository, cfg: IngestionConfig, shutdown: CancellationToken) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            cfg.staging_cleanup_interval_seconds.max(1),
        ));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    let started = std::time::Instant::now();
                    match run_once(&repo, &cfg).await {
                        Ok(stats) => {
                            counter!("ingestion_staging_cleanup_deleted_sessions_total").increment(stats.deleted_sessions);
                            counter!("ingestion_staging_cleanup_deleted_blocks_total").increment(stats.deleted_blocks);
                            counter!("ingestion_chunked_session_expired_total").increment(stats.expired_sessions);
                            gauge!("ingestion_staging_rows_total").set(stats.remaining_blocks as f64);
                            gauge!("ingestion_staging_bytes_total").set(stats.remaining_bytes as f64);
                            histogram!("ingestion_staging_cleanup_duration_ms").record(started.elapsed().as_millis() as f64);
                            if stats.remaining_bytes as u64 > cfg.staging_max_bytes {
                                tracing::warn!(remaining_bytes=stats.remaining_bytes, staging_max_bytes=cfg.staging_max_bytes, "ingestion staging size above configured limit");
                            }
                        }
                        Err(error) => tracing::error!(%error, "ingestion cleanup failed"),
                    }
                }
            }
        }
    });
}

#[derive(Default)]
struct CleanupStats {
    expired_sessions: u64,
    deleted_sessions: u64,
    deleted_blocks: u64,
    remaining_blocks: i64,
    remaining_bytes: i64,
}

async fn run_once(repo: &Repository, cfg: &IngestionConfig) -> Result<CleanupStats, sqlx::Error> {
    // fix4.5.2: ACTIVE sessions still expire by expires_at, but active FINALIZING sessions
    // must not be expired by the normal TTL. They are handled by stale-finalize timeout below.
    let expired = sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET status='EXPIRED', updated_at=now() WHERE status='ACTIVE' AND expires_at < now()")
        .execute(&repo.pool).await?.rows_affected();
    counter!("ingestion_cleanup_active_expired_total").increment(expired);

    let stale_finalizing = sqlx::query(
        "UPDATE astravector.ingestion_sessions_v004
         SET status='FAILED',
             error_code='FINALIZE_STALE_TIMEOUT',
             error_message='finalize exceeded finalizing_stale_timeout_seconds',
             updated_at=now()
         WHERE status='FINALIZING'
           AND COALESCE(finalizing_heartbeat_at, finalizing_started_at, updated_at)
               < now() - ($1 * interval '1 second')",
    )
    .bind(cfg.finalizing_stale_timeout_seconds as i64)
    .execute(&repo.pool)
    .await?
    .rows_affected();
    counter!("ingestion_finalizing_stale_failed_total").increment(stale_finalizing);
    counter!("ingestion_cleanup_finalizing_stale_total").increment(stale_finalizing);
    if stale_finalizing == 0 {
        counter!("ingestion_cleanup_finalizing_skipped_total").increment(1);
    }

    // Completed sessions: remove heavy staging rows first, but keep session/result_response_json
    // for idempotent Finalize replay until completed_session_result_retention_seconds expires.
    let completed_block_rows = sqlx::query(
        r#"
        SELECT ingestion_session_id
        FROM astravector.ingestion_sessions_v004
        WHERE status='COMPLETED'
          AND completed_blocks_cleaned_at IS NULL
          AND finalized_at < now() - ($1 * interval '1 second')
    "#,
    )
    .bind(cfg.staging_completed_blocks_retention_seconds as i64)
    .fetch_all(&repo.pool)
    .await?;
    let completed_block_ids: Vec<uuid::Uuid> = completed_block_rows
        .iter()
        .map(|r| r.get("ingestion_session_id"))
        .collect();
    let mut deleted_blocks = 0u64;
    if !completed_block_ids.is_empty() {
        deleted_blocks += sqlx::query("DELETE FROM astravector.ingestion_session_blocks_v004 WHERE ingestion_session_id = ANY($1)")
            .bind(&completed_block_ids).execute(&repo.pool).await?.rows_affected();
        let _ = sqlx::query("DELETE FROM astravector.ingestion_session_batches_v004 WHERE ingestion_session_id = ANY($1)")
            .bind(&completed_block_ids).execute(&repo.pool).await;
        let _ = sqlx::query("UPDATE astravector.ingestion_sessions_v004 SET completed_blocks_cleaned_at=now(), updated_at=now() WHERE ingestion_session_id = ANY($1)")
            .bind(&completed_block_ids).execute(&repo.pool).await?;
        counter!("ingestion_cleanup_completed_blocks_deleted_total")
            .increment(completed_block_ids.len() as u64);
    }

    let terminal_rows = sqlx::query(r#"
        SELECT ingestion_session_id
        FROM astravector.ingestion_sessions_v004
        WHERE (status='COMPLETED' AND COALESCE(result_expires_at, finalized_at + ($1 * interval '1 second')) < now())
           OR (status='ABORTED' AND updated_at < now() - ($2 * interval '1 second'))
           OR (status='EXPIRED' AND updated_at < now() - ($3 * interval '1 second'))
           OR (status='FAILED' AND updated_at < now() - ($4 * interval '1 second'))
    "#)
        .bind(cfg.completed_session_result_retention_seconds as i64)
        .bind(cfg.staging_aborted_retention_seconds as i64)
        .bind(cfg.staging_expired_retention_seconds as i64)
        .bind(cfg.failed_session_retention_seconds as i64)
        .fetch_all(&repo.pool).await?;
    let terminal_ids: Vec<uuid::Uuid> = terminal_rows
        .iter()
        .map(|r| r.get("ingestion_session_id"))
        .collect();
    if !terminal_ids.is_empty() {
        deleted_blocks += sqlx::query("DELETE FROM astravector.ingestion_session_blocks_v004 WHERE ingestion_session_id = ANY($1)")
            .bind(&terminal_ids).execute(&repo.pool).await?.rows_affected();
        let _ = sqlx::query("DELETE FROM astravector.ingestion_session_batches_v004 WHERE ingestion_session_id = ANY($1)")
            .bind(&terminal_ids).execute(&repo.pool).await;
        sqlx::query(
            "DELETE FROM astravector.ingestion_sessions_v004 WHERE ingestion_session_id = ANY($1)",
        )
        .bind(&terminal_ids)
        .execute(&repo.pool)
        .await?;
        counter!("ingestion_cleanup_completed_sessions_deleted_total")
            .increment(terminal_ids.len() as u64);
    }

    let remaining = sqlx::query("SELECT count(*) AS rows_count, COALESCE(sum(block_size_bytes),0) AS bytes_count FROM astravector.ingestion_session_blocks_v004")
        .fetch_one(&repo.pool).await?;
    let retained_results: i64 = sqlx::query_scalar("SELECT count(*) FROM astravector.ingestion_sessions_v004 WHERE status='COMPLETED' AND result_response_json IS NOT NULL")
        .fetch_one(&repo.pool).await?;
    gauge!("ingestion_cleanup_result_rows_retained_total").set(retained_results as f64);
    Ok(CleanupStats {
        expired_sessions: expired,
        deleted_sessions: terminal_ids.len() as u64,
        deleted_blocks,
        remaining_blocks: remaining.get("rows_count"),
        remaining_bytes: remaining.get("bytes_count"),
    })
}
