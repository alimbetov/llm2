use astravector_runtime::{
    adaptive::AdaptiveRuntime,
    cache::L1Cache,
    checksum,
    config::AppConfig,
    grpc::{AstraVectorService, AstraVectorV004ControlService},
    health::Readiness,
    inference::{InferenceEngine, OnnxBgeM3Engine},
    ingestion_cleanup, lifecycle, metrics, outbox,
    pb::{
        astra_vector_admin_facade_server::AstraVectorAdminFacadeServer,
        astra_vector_ingestion_facade_server::AstraVectorIngestionFacadeServer,
        astra_vector_retrieval_facade_server::AstraVectorRetrievalFacadeServer,
        astra_vector_runtime_server::AstraVectorRuntimeServer,
        astra_vector_v004_control_server::AstraVectorV004ControlServer,
    },
    persistence::Repository,
    provider,
    qdrant::QdrantClient,
    recovery, retention,
    scheduler::Scheduler,
    security::ApiKeyAuth,
    tokenizer::CanonicalTokenizer,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{error, info, warn};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();
    let mut loaded = AppConfig::load()?;
    if std::env::args().nth(1).as_deref() == Some("migrate") {
        loaded.postgres.auto_migrate = true;
        let _ = Repository::connect(&loaded.postgres).await?;
        info!("AstraVector migrations completed");
        return Ok(());
    }
    let cfg = Arc::new(loaded);
    cfg.validate()?;
    let prod = cfg.service.environment.eq_ignore_ascii_case("production");
    checksum::verify(&cfg.model.path, &cfg.model.checksum, prod).await?;
    checksum::verify(&cfg.tokenizer.path, &cfg.tokenizer.checksum, prod).await?;
    let metrics_addr: SocketAddr = format!("{}:{}", cfg.metrics.host, cfg.metrics.port).parse()?;
    metrics::install(metrics_addr)?;
    if cfg.limits.allow_dangerous_limit_override {
        ::metrics::gauge!("config_dangerous_override_enabled").set(1.0);
        warn!("dangerous limit override is enabled");
    } else {
        ::metrics::gauge!("config_dangerous_override_enabled").set(0.0);
    }
    let adaptive = Arc::new(AdaptiveRuntime::new(cfg.adaptive.clone()));
    info!(mode=%adaptive.mode().as_str(), "AstraVector adaptive runtime initialized");
    let readiness = Readiness::default();
    readiness.set(false);
    let tokenizer = CanonicalTokenizer::load(&cfg)?;
    let mut selected = None;
    let mut engine = None;
    for candidate in provider::candidates(&cfg.inference.provider)? {
        match OnnxBgeM3Engine::load(cfg.clone(), tokenizer.clone(), &candidate.name) {
            Ok(e) => {
                let a: Arc<dyn InferenceEngine> = Arc::new(e);
                match a.self_test().await {
                    Ok(()) => {
                        selected = Some(candidate);
                        engine = Some(a);
                        break;
                    }
                    Err(x) => warn!(provider=%candidate.name,error=%x,"provider self-test failed"),
                }
            }
            Err(x) => warn!(provider=%candidate.name,error=%x,"provider initialization failed"),
        }
    }
    let selected = selected.ok_or_else(|| anyhow::anyhow!("no ONNX provider passed self-test"))?;
    let engine = engine.ok_or_else(|| {
        anyhow::anyhow!("provider selected but inference engine was not initialized")
    })?;
    let shutdown = CancellationToken::new();
    let repo = if cfg.postgres.enabled {
        match Repository::connect(&cfg.postgres).await {
            Ok(r) => Some(r),
            Err(e) if !cfg.postgres.required_on_startup => {
                warn!(error=%e,"PostgreSQL unavailable; starting degraded");
                None
            }
            Err(e) => return Err(e.into()),
        }
    } else {
        None
    };
    let qdrant = if cfg.qdrant.enabled {
        Some(Arc::new(QdrantClient::new(
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
            cfg.resilience.qdrant_retry.query.clone(),
        )?))
    } else {
        None
    };
    if let Some(r) = repo.clone() {
        if cfg.recovery.enabled {
            recovery::spawn(r.clone(), cfg.recovery.clone(), shutdown.child_token())
        }
        retention::spawn(r.clone(), cfg.retention.clone(), shutdown.child_token());
        if cfg.lifecycle.enabled {
            lifecycle::spawn(
                r.clone(),
                cfg.lifecycle.scan_interval_seconds,
                cfg.lifecycle.batch_size,
                cfg.lifecycle.soft_delete_grace_days,
                shutdown.child_token(),
            );
        }
        if cfg.index_ttl.enabled && cfg.index_ttl.cleanup_enabled {
            let q = qdrant.clone().ok_or_else(|| {
                anyhow::anyhow!("index_ttl.cleanup_enabled=true requires qdrant.enabled=true")
            })?;
            lifecycle::spawn_index_ttl_cleanup(
                r.clone(),
                q,
                cfg.index_ttl.clone(),
                shutdown.child_token(),
            );
        }
        ingestion_cleanup::spawn(r.clone(), cfg.ingestion.clone(), shutdown.child_token());
        if let Some(q) = qdrant.clone().filter(|_| cfg.qdrant.publisher.enabled) {
            outbox::spawn(
                r,
                q,
                cfg.service.instance_id.clone(),
                cfg.qdrant.publisher.batch_size,
                cfg.qdrant.publisher.poll_interval_ms,
                cfg.qdrant.publisher.max_attempts,
                Some(adaptive.clone()),
                shutdown.child_token(),
            );
        }
    }
    let l1 = L1Cache::new(
        cfg.cache.l1.enabled,
        cfg.cache.l1.max_entries,
        cfg.cache.l1.ttl_minutes,
        cfg.cache.l1.idle_timeout_minutes,
    );
    let scheduler = Scheduler::start(
        engine.clone(),
        cfg.batching.clone(),
        cfg.scheduler.max_consecutive_query_batches,
        cfg.resilience.inference_retry.clone(),
    );
    let service = AstraVectorService::new(
        cfg.clone(),
        scheduler.clone(),
        engine.clone(),
        l1,
        repo.clone(),
        qdrant.clone(),
        selected.clone(),
        readiness.clone(),
        shutdown.clone(),
    );
    let auth = ApiKeyAuth::new(&cfg.security);
    let mut grpc = AstraVectorRuntimeServer::new(service)
        .max_decoding_message_size(cfg.grpc.max_request_message_mb * 1024 * 1024)
        .max_encoding_message_size(cfg.grpc.max_response_message_mb * 1024 * 1024);
    let control_impl = AstraVectorV004ControlService::new(
        cfg.clone(),
        scheduler.clone(),
        repo.clone(),
        qdrant.clone(),
        engine.clone(),
        shutdown.clone(),
    );
    let mut control = AstraVectorV004ControlServer::new(control_impl.clone())
        .max_decoding_message_size(cfg.grpc.max_request_message_mb * 1024 * 1024)
        .max_encoding_message_size(cfg.grpc.max_response_message_mb * 1024 * 1024);
    let mut ingestion = AstraVectorIngestionFacadeServer::new(control_impl.clone())
        .max_decoding_message_size(cfg.grpc.max_request_message_mb * 1024 * 1024)
        .max_encoding_message_size(cfg.grpc.max_response_message_mb * 1024 * 1024);
    let mut retrieval = AstraVectorRetrievalFacadeServer::new(control_impl.clone())
        .max_decoding_message_size(cfg.grpc.max_request_message_mb * 1024 * 1024)
        .max_encoding_message_size(cfg.grpc.max_response_message_mb * 1024 * 1024);
    let mut admin = AstraVectorAdminFacadeServer::new(control_impl)
        .max_decoding_message_size(cfg.grpc.max_request_message_mb * 1024 * 1024)
        .max_encoding_message_size(cfg.grpc.max_response_message_mb * 1024 * 1024);
    if cfg.grpc.compression.enabled {
        grpc = grpc
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip);
        control = control
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip);
        ingestion = ingestion
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip);
        retrieval = retrieval
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip);
        admin = admin
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip);
    }
    let control_auth = auth.clone();
    let ingestion_auth = auth.clone();
    let retrieval_auth = auth.clone();
    let admin_auth = auth.clone();
    #[allow(clippy::result_large_err)]
    let grpc =
        tonic::service::interceptor::InterceptedService::new(grpc, move |r| auth.interceptor(r));
    #[allow(clippy::result_large_err)]
    let control = tonic::service::interceptor::InterceptedService::new(control, move |r| {
        control_auth.interceptor(r)
    });
    #[allow(clippy::result_large_err)]
    let ingestion = tonic::service::interceptor::InterceptedService::new(ingestion, move |r| {
        ingestion_auth.interceptor(r)
    });
    #[allow(clippy::result_large_err)]
    let retrieval = tonic::service::interceptor::InterceptedService::new(retrieval, move |r| {
        retrieval_auth.interceptor(r)
    });
    #[allow(clippy::result_large_err)]
    let admin = tonic::service::interceptor::InterceptedService::new(admin, move |r| {
        admin_auth.interceptor(r)
    });
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_not_serving::<AstraVectorRuntimeServer<AstraVectorService>>()
        .await;
    health_reporter
        .set_not_serving::<AstraVectorV004ControlServer<AstraVectorV004ControlService>>()
        .await;
    health_reporter
        .set_not_serving::<AstraVectorIngestionFacadeServer<AstraVectorV004ControlService>>()
        .await;
    health_reporter
        .set_not_serving::<AstraVectorRetrievalFacadeServer<AstraVectorV004ControlService>>()
        .await;
    health_reporter
        .set_not_serving::<AstraVectorAdminFacadeServer<AstraVectorV004ControlService>>()
        .await;
    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(astravector_runtime::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
        .build_v1()?;
    // fix463: start as NOT_SERVING until PostgreSQL/Qdrant have been checked once.
    let mut initial_ready = scheduler.healthy();
    if cfg.postgres.required_for_readiness {
        initial_ready &= match &repo {
            Some(r) => r.ping().await.is_ok(),
            None => false,
        };
    }
    if cfg.qdrant.enabled {
        initial_ready &= match &qdrant {
            Some(q) => q.collection_exists().await.unwrap_or(false),
            None => false,
        };
    }
    readiness.set(initial_ready);
    if initial_ready {
        health_reporter
            .set_serving::<AstraVectorRuntimeServer<AstraVectorService>>()
            .await;
        health_reporter
            .set_serving::<AstraVectorV004ControlServer<AstraVectorV004ControlService>>()
            .await;
        health_reporter
            .set_serving::<AstraVectorIngestionFacadeServer<AstraVectorV004ControlService>>()
            .await;
        health_reporter
            .set_serving::<AstraVectorRetrievalFacadeServer<AstraVectorV004ControlService>>()
            .await;
        health_reporter
            .set_serving::<AstraVectorAdminFacadeServer<AstraVectorV004ControlService>>()
            .await;
    }
    let monitor_ready = readiness.clone();
    let monitor_scheduler = scheduler.clone();
    let monitor_repo = repo.clone();
    let monitor_qdrant = qdrant.clone();
    let required = cfg.postgres.required_for_readiness;
    let qdrant_required = cfg.qdrant.enabled;
    let monitor_shutdown = shutdown.clone();
    let mut health_reporter_monitor = health_reporter.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = monitor_shutdown.cancelled() => break,
                _ = interval.tick() => {
                    let mut ok = monitor_scheduler.healthy();
                    if required {
                        ok &= match &monitor_repo { Some(r) => r.ping().await.is_ok(), None => false };
                    }
                    if qdrant_required {
                        ok &= match &monitor_qdrant { Some(q) => q.collection_exists().await.unwrap_or(false), None => false };
                    }
                    monitor_ready.set(ok);
                    if ok {
                        health_reporter_monitor.set_serving::<AstraVectorRuntimeServer<AstraVectorService>>().await;
                        health_reporter_monitor.set_serving::<AstraVectorV004ControlServer<AstraVectorV004ControlService>>().await;
                        health_reporter_monitor.set_serving::<AstraVectorIngestionFacadeServer<AstraVectorV004ControlService>>().await;
                        health_reporter_monitor.set_serving::<AstraVectorRetrievalFacadeServer<AstraVectorV004ControlService>>().await;
                        health_reporter_monitor.set_serving::<AstraVectorAdminFacadeServer<AstraVectorV004ControlService>>().await;
                    } else {
                        health_reporter_monitor.set_not_serving::<AstraVectorRuntimeServer<AstraVectorService>>().await;
                        health_reporter_monitor.set_not_serving::<AstraVectorV004ControlServer<AstraVectorV004ControlService>>().await;
                        health_reporter_monitor.set_not_serving::<AstraVectorIngestionFacadeServer<AstraVectorV004ControlService>>().await;
                        health_reporter_monitor.set_not_serving::<AstraVectorRetrievalFacadeServer<AstraVectorV004ControlService>>().await;
                        health_reporter_monitor.set_not_serving::<AstraVectorAdminFacadeServer<AstraVectorV004ControlService>>().await;
                    }
                }
            }
        }
    });
    let addr: SocketAddr = format!("{}:{}", cfg.grpc.host, cfg.grpc.port).parse()?;
    info!(%addr,provider=%selected.name,"AstraVector v0.4.0 fix463 starting");
    let sd = shutdown.clone();
    let rd = readiness.clone();
    let drain = cfg.shutdown.drain_timeout_seconds;
    Server::builder()
        .add_service(health_service)
        .add_service(reflection)
        .add_service(grpc)
        .add_service(control)
        .add_service(ingestion)
        .add_service(retrieval)
        .add_service(admin)
        .serve_with_shutdown(addr, async move {
            let _ = tokio::signal::ctrl_c().await;
            rd.set(false);
            sd.cancel();
            tokio::time::sleep(Duration::from_secs(drain)).await;
        })
        .await
        .map_err(|e| {
            error!(error=%e,"gRPC server stopped");
            e
        })?;
    Ok(())
}
