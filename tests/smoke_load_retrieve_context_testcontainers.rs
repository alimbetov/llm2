#![cfg(feature = "integration-tests")]

use astravector_runtime::{
    chunking::{GeneratedChunk, Granularity},
    config::{AppConfig, RetryPolicyConfig},
    error::AstraError,
    graph::GraphBuildLimits,
    grpc::AstraVectorV004ControlService,
    inference::{EmbeddingResult, InferenceEngine, InferenceInput},
    outbox, pb,
    pb::{
        astra_vector_retrieval_facade_client::AstraVectorRetrievalFacadeClient,
        astra_vector_retrieval_facade_server::AstraVectorRetrievalFacadeServer,
    },
    persistence::{PreparedV004IndexEmbedding, Repository},
    qdrant::QdrantClient,
    scheduler::Scheduler,
};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_util::sync::CancellationToken;
use tonic::{transport::Server, Request};
use uuid::Uuid;

const CONCURRENCY: usize = 50;
const P95_LIMIT_MS: u128 = 5_000;

struct FixedSmokeEngine;

#[async_trait]
impl InferenceEngine for FixedSmokeEngine {
    async fn encode_batch(
        &self,
        inputs: Vec<InferenceInput>,
    ) -> Result<Vec<EmbeddingResult>, AstraError> {
        Ok(inputs
            .into_iter()
            .map(|_| EmbeddingResult {
                dense: Some(vec![0.03125; 1024]),
                sparse_indices: Some(vec![1, 2, 3]),
                sparse_values: Some(vec![0.9, 0.7, 0.5]),
                token_count: 4,
                truncated: false,
            })
            .collect())
    }
    fn dense_available(&self) -> bool {
        true
    }
    fn sparse_available(&self) -> bool {
        true
    }
    fn count_tokens(
        &self,
        text: &str,
        _max_length: usize,
        _allow_truncation: bool,
    ) -> Result<usize, AstraError> {
        Ok(text.split_whitespace().count().max(1))
    }
    async fn self_test(&self) -> Result<(), AstraError> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_smoke_load_retrieve_context_50_concurrent_testcontainers() {
    let postgres = GenericImage::new("pgvector/pgvector", "pg16")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_DB", "astravector")
        .with_env_var("POSTGRES_USER", "astravector")
        .with_env_var("POSTGRES_PASSWORD", "astravector")
        .start()
        .await
        .expect("PostgreSQL testcontainer must start");
    let pg_port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("PostgreSQL mapped port");
    let database_url = format!(
        "postgres://astravector:astravector@127.0.0.1:{pg_port}/astravector?sslmode=disable"
    );
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect PostgreSQL testcontainer");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("all migrations must apply on a clean DB");

    let qdrant = GenericImage::new("qdrant/qdrant", "v1.11.5")
        .with_exposed_port(6333.tcp())
        .with_wait_for(WaitFor::seconds(3))
        .start()
        .await
        .expect("Qdrant testcontainer must start");
    let qdrant_port = qdrant
        .get_host_port_ipv4(6333)
        .await
        .expect("Qdrant mapped port");
    let qdrant_url = format!("http://127.0.0.1:{qdrant_port}");
    let collection = format!("astravector_smoke_{}", Uuid::new_v4().simple());
    let qdrant_client = QdrantClient::new(
        qdrant_url,
        None,
        collection.clone(),
        5_000,
        100,
        10,
        10_000,
        10,
        2,
        64,
        1_000,
        None,
        RetryPolicyConfig::default(),
    )
    .expect("qdrant client");
    qdrant_client
        .ensure_collection(1024)
        .await
        .expect("create qdrant collection and payload indexes");

    let repo = Repository { pool: pool.clone() };
    let access_zone_id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let document_version = 1_i64;
    let content_hash = "a".repeat(64);

    sqlx::query(
        "INSERT INTO astravector.access_zones(access_zone_id, access_zone_code, access_zone_name, status, default_ttl_days, ttl_policy_source, allow_never_expire)
         VALUES ($1,'1700','smoke-load-zone','ACTIVE',365,'CODE_MATRIX',false)"
    )
    .bind(access_zone_id)
    .execute(&pool)
    .await
    .expect("insert access zone");

    repo.register_document_version(
        access_zone_id,
        document_id,
        document_version,
        &content_hash,
        "MANUAL",
    )
    .await
    .expect("register document version");

    let mut chunks = Vec::new();
    let mut prepared = Vec::new();
    for i in 0..8 {
        let chunk_id = Uuid::new_v4();
        let chunk = GeneratedChunk {
            id: chunk_id,
            root_id: chunk_id,
            source_id: chunk_id,
            parent_id: None,
            granularity: Granularity::Parent,
            sequence_no: i,
            token_count: 8,
            content: format!("smoke load production candidate context chunk {i}"),
            content_hash: format!("{:064x}", i + 1),
            source_block_id: Some(format!("block-{i}")),
            source_block_ids: vec![format!("block-{i}")],
            source_location: json!({"page": i + 1}),
            source_links: json!([]),
            trace_relation_type: "EXACT".to_string(),
            trace_quality: "EXACT".to_string(),
        };
        prepared.push(PreparedV004IndexEmbedding {
            chunk: chunk.clone(),
            embedding: EmbeddingResult {
                dense: Some(vec![0.03125; 1024]),
                sparse_indices: Some(vec![1, 2, 3]),
                sparse_values: Some(vec![0.9, 0.7, 0.5]),
                token_count: 4,
                truncated: false,
            },
        });
        chunks.push(chunk);
    }

    let summary = repo
        .persist_v004_index_transactionally(
            access_zone_id,
            document_id,
            document_version,
            &chunks,
            &prepared,
            "test-tokenizer",
            "test-chunker",
            2,
            Some(1),
            json!({"test": true, "smoke": true}),
            "tenant-smoke",
            "workspace-smoke",
            "model-v1",
            "dense",
            "dense-v1",
            "sparse",
            "sparse-v1",
            0.0,
            64,
            &collection,
            true,
            false,
            Some(GraphBuildLimits::default()),
            100,
            true,
        )
        .await
        .expect("persist smoke chunks, embeddings, bindings and outbox");
    assert_eq!(summary.bindings, chunks.len() as u32);

    sqlx::query(
        "UPDATE astravector.document_versions
         SET status='ACTIVE', lifecycle_status='ACTIVE', expires_at=now() + interval '1 hour', access_zone_code='1700', metadata='{}'::jsonb
         WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3"
    )
    .bind(access_zone_id)
    .bind(document_id)
    .bind(document_version)
    .execute(&pool)
    .await
    .expect("activate document version for smoke retrieval");

    let outbox_shutdown = CancellationToken::new();
    outbox::spawn(
        repo.clone(),
        Arc::new(qdrant_client.clone()),
        "smoke-publisher".into(),
        20,
        100,
        5,
        None,
        outbox_shutdown.clone(),
    );
    let mut synced = false;
    for _ in 0..80 {
        let row = sqlx::query("SELECT count(*) AS cnt FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status='SYNCED'")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version)
            .fetch_one(&pool)
            .await
            .expect("check Qdrant sync status");
        if row.get::<i64, _>("cnt") >= chunks.len() as i64 {
            synced = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    outbox_shutdown.cancel();
    assert!(synced, "smoke setup must publish all bindings to Qdrant");

    let mut cfg = AppConfig::load().expect("load application config for smoke server");
    cfg.qdrant.enabled = true;
    cfg.qdrant.url = format!("http://127.0.0.1:{qdrant_port}");
    cfg.qdrant.collection = collection.clone();
    cfg.dense.dimension = 1024;
    cfg.sparse.required = false;
    cfg.graph_rag.retrieval.enabled_by_default = true;
    cfg.graph_rag.rerank.mmr_enabled = true;
    cfg.limits.max_concurrent_qdrant_search = 64;
    cfg.limits.max_concurrent_retrieve_context = 64;
    cfg.limits.backpressure_acquire_timeout_ms = 1_000;
    let cfg = Arc::new(cfg);
    let engine = Arc::new(FixedSmokeEngine);
    let scheduler = Scheduler::start(
        engine.clone(),
        cfg.batching.clone(),
        cfg.scheduler.max_consecutive_query_batches,
        cfg.resilience.inference_retry.clone(),
    );
    let service = AstraVectorV004ControlService::new(
        cfg,
        scheduler,
        Some(repo.clone()),
        Some(Arc::new(qdrant_client.clone())),
        engine,
        CancellationToken::new(),
    );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random smoke tonic port");
    let grpc_addr = listener.local_addr().expect("read tonic local addr");
    let grpc_shutdown = CancellationToken::new();
    let grpc_shutdown_for_server = grpc_shutdown.clone();
    let server_handle = tokio::spawn(async move {
        Server::builder()
            .add_service(AstraVectorRetrievalFacadeServer::new(service))
            .serve_with_incoming_shutdown(
                TcpListenerStream::new(listener),
                grpc_shutdown_for_server.cancelled(),
            )
            .await
            .expect("tonic retrieval smoke server must run");
    });

    let client = AstraVectorRetrievalFacadeClient::connect(format!("http://{grpc_addr}"))
        .await
        .expect("connect smoke retrieval client");

    let started = Instant::now();
    let mut handles = Vec::new();
    for i in 0..CONCURRENCY {
        let mut client = client.clone();
        let zone = access_zone_id.to_string();
        handles.push(tokio::spawn(async move {
            let mut req = Request::new(pb::RetrieveContextRequest {
                context: Some(pb::RequestContext {
                    correlation_id: format!("fix465-smoke-{i}"),
                    idempotency_key: String::new(),
                    caller_service: "smoke-load".into(),
                    caller_user_id: "smoke".into(),
                    caller_access_level: pb::AccessLevel::Restricted as i32,
                }),
                access_zone_id: zone.clone(),
                question: "smoke load production candidate context".into(),
                access_zone_ids: vec![zone],
                access_zone_code: "1700".into(),
                access_zone_codes: vec!["1700".into()],
                profile: pb::RetrievalProfile::Balanced as i32,
                max_contexts: 5,
                filters: Vec::new(),
                response_detail: pb::ResponseDetail::Debug as i32,
                enable_graph_expansion: true,
                graph_max_hops: 1,
                graph_max_related_contexts: 3,
            });
            req.set_timeout(Duration::from_secs(5));
            let started = Instant::now();
            let response = client.retrieve_context(req).await;
            let elapsed = started.elapsed().as_millis();
            let ok = response
                .map(|r| !r.into_inner().contexts.is_empty())
                .unwrap_or(false);
            (ok, elapsed)
        }));
    }

    let mut successes = 0usize;
    let mut latencies = Vec::new();
    for handle in handles {
        let (ok, elapsed) = handle.await.expect("smoke task must join");
        if ok {
            successes += 1;
        }
        latencies.push(elapsed);
    }
    grpc_shutdown.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;

    latencies.sort_unstable();
    let p95_index = ((latencies.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(latencies.len() - 1);
    let p95 = latencies[p95_index];
    assert!(
        successes * 100 >= CONCURRENCY * 99,
        "smoke success_rate must be >= 99%, successes={successes}/{CONCURRENCY}"
    );
    assert!(
        p95 <= P95_LIMIT_MS,
        "smoke p95 latency must be <= {P95_LIMIT_MS}ms, p95={p95}ms total_elapsed={}ms",
        started.elapsed().as_millis()
    );
}
