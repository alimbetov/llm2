use astravector_runtime::{
    adaptive::AdaptiveRuntime, config::AppConfig, persistence::Repository, qdrant::QdrantClient,
    reconciliation::Reconciler,
};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

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
    let q = QdrantClient::new(
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
        cfg.resilience.qdrant_retry.clone(),
    )?;
    let reconciler = Reconciler { repo, qdrant: q };

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--full") {
        let batch_size = read_arg_i64(&args, "--batch-size").unwrap_or(500);
        let interval_secs = read_arg_u64(&args, "--interval-seconds").unwrap_or(30);
        tracing::info!(
            batch_size,
            interval_secs,
            "AstraVector reconciliation worker started in full loop mode"
        );
        loop {
            match reconciler.reconcile_unsynced_batch(batch_size).await {
                Ok(summary) => tracing::info!(?summary, "reconciliation batch completed"),
                Err(error) => tracing::warn!(%error, "reconciliation batch failed"),
            }
            tokio::time::sleep(Duration::from_secs(interval_secs.max(1))).await;
        }
    }

    if let Some(binding_spec) = read_arg_string(&args, "--binding") {
        let (zone, binding) = parse_zone_uuid_pair(&binding_spec)?;
        let summary = reconciler.reconcile_binding(zone, binding).await?;
        tracing::info!(?summary, %zone, %binding, "single binding reconciliation completed");
        return Ok(());
    }

    anyhow::bail!(
        "unsupported reconciliation mode; use --full or --binding <zone_uuid>:<binding_uuid>"
    )
}

fn read_arg_string(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}
fn read_arg_i64(args: &[String], name: &str) -> Option<i64> {
    read_arg_string(args, name)?.parse().ok()
}
fn read_arg_u64(args: &[String], name: &str) -> Option<u64> {
    read_arg_string(args, name)?.parse().ok()
}
fn parse_zone_uuid_pair(raw: &str) -> anyhow::Result<(Uuid, Uuid)> {
    let (zone, binding) = raw
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected <zone_uuid>:<binding_uuid>"))?;
    Ok((Uuid::parse_str(zone)?, Uuid::parse_str(binding)?))
}
