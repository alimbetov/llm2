use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::net::SocketAddr;

pub fn install(addr: SocketAddr) -> anyhow::Result<PrometheusHandle> {
    let builder = PrometheusBuilder::new().with_http_listener(addr);
    Ok(builder.install_recorder()?)
}
