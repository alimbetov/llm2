use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;

pub fn install(addr: SocketAddr) -> anyhow::Result<()> {
    let builder = PrometheusBuilder::new().with_http_listener(addr);
    builder.install()?;
    Ok(())
}
