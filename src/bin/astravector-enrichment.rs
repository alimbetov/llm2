use astravector_runtime::enrichment::{DisabledEnrichmentProvider, EnrichmentProvider};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _provider: Box<dyn EnrichmentProvider> = Box::new(DisabledEnrichmentProvider);
    tracing_subscriber::fmt().json().init();
    tracing::info!("AstraVector enrichment worker initialized");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
