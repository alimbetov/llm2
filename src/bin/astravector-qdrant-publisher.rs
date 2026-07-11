use astravector_runtime::{
    adaptive::AdaptiveRuntime, config::AppConfig, outbox, persistence::Repository,
    qdrant::QdrantClient,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();
    let cfg = AppConfig::load()?;
    cfg.validate()?;
    let adaptive = Arc::new(AdaptiveRuntime::new(cfg.adaptive.clone()));
    let repo = Repository::connect(&cfg.postgres).await?;
    let client = Arc::new(QdrantClient::new(
        cfg.qdrant.url.clone(),
        (!cfg.qdrant.api_key.is_empty()).then_some(cfg.qdrant.api_key.clone()),
        cfg.qdrant.collection.clone(),
        cfg.qdrant.timeout_ms,
        cfg.qdrant.scroll_page_size,
        cfg.qdrant.scroll_max_pages,
        cfg.qdrant.scroll_max_points,
        cfg.qdrant.scroll_timeout_secs,
        cfg.qdrant.scroll_max_concurrency,
        cfg.limits.max_concurrent_qdrant_search,
        cfg.limits.backpressure_acquire_timeout_ms,
        Some(adaptive.clone()),
        cfg.resilience.qdrant_retry.publisher.clone(),
    )?);
    client.ensure_collection(cfg.dense.dimension).await?;
    let shutdown = CancellationToken::new();
    outbox::spawn(
        repo,
        client,
        cfg.service.instance_id.clone(),
        cfg.qdrant.publisher.batch_size,
        cfg.qdrant.publisher.poll_interval_ms,
        cfg.qdrant.publisher.max_attempts,
        Some(adaptive.clone()),
        shutdown.clone(),
    );
    tokio::signal::ctrl_c().await?;
    shutdown.cancel();
    Ok(())
}
