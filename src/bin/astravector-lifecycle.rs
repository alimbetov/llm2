use astravector_runtime::{config::AppConfig, lifecycle, persistence::Repository};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();
    let cfg = AppConfig::load()?;
    cfg.validate()?;
    let repo = Repository::connect(&cfg.postgres).await?;
    let expired = lifecycle::expire_batch(&repo, cfg.lifecycle.batch_size).await?;
    let purged = lifecycle::purge_batch(
        &repo,
        cfg.lifecycle.batch_size,
        cfg.lifecycle.soft_delete_grace_days,
    )
    .await?;
    tracing::info!(expired, purged, "lifecycle run completed");
    Ok(())
}
