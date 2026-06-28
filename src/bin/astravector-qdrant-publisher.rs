use astravector_runtime::{
    config::AppConfig, outbox, persistence::Repository, qdrant::QdrantClient,
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
    let repo = Repository::connect(&cfg.postgres).await?;
    let client = Arc::new(QdrantClient::new(
        cfg.qdrant.url.clone(),
        (!cfg.qdrant.api_key.is_empty()).then_some(cfg.qdrant.api_key.clone()),
        cfg.qdrant.collection.clone(),
        cfg.qdrant.timeout_ms,
    )?);
    let shutdown = CancellationToken::new();
    outbox::spawn(
        repo,
        client,
        cfg.service.instance_id.clone(),
        cfg.qdrant.publisher.batch_size,
        cfg.qdrant.publisher.poll_interval_ms,
        cfg.qdrant.publisher.max_attempts,
        shutdown.clone(),
    );
    tokio::signal::ctrl_c().await?;
    shutdown.cancel();
    Ok(())
}
