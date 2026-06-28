use astravector_runtime::{
    config::AppConfig, persistence::Repository, qdrant::QdrantClient, reconciliation::Reconciler,
};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();
    let cfg = AppConfig::load()?;
    cfg.validate()?;
    let repo = Repository::connect(&cfg.postgres).await?;
    let q = QdrantClient::new(
        cfg.qdrant.url.clone(),
        (!cfg.qdrant.api_key.is_empty()).then_some(cfg.qdrant.api_key.clone()),
        cfg.qdrant.collection.clone(),
        cfg.qdrant.timeout_ms,
    )?;
    let _r = Reconciler { repo, qdrant: q };
    tracing::info!("AstraVector reconciliation worker initialized");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
