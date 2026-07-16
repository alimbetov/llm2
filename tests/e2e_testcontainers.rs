#![cfg(feature = "integration-tests")]

use astravector_runtime::pb::{
    astra_vector_ingestion_facade_client::AstraVectorIngestionFacadeClient,
    astra_vector_ingestion_facade_server::AstraVectorIngestionFacadeServer,
    astra_vector_retrieval_facade_client::AstraVectorRetrievalFacadeClient,
    astra_vector_retrieval_facade_server::AstraVectorRetrievalFacadeServer,
    astra_vector_v004_control_client::AstraVectorV004ControlClient,
    astra_vector_v004_control_server::AstraVectorV004ControlServer,
};
use astravector_runtime::{
    chunking::{GeneratedChunk, Granularity},
    config::{AppConfig, RetryPolicyConfig},
    error::AstraError,
    graph::GraphBuildLimits,
    grpc::AstraVectorV004ControlService,
    inference::{EmbeddingResult, InferenceEngine, InferenceInput},
    lifecycle::run_index_ttl_cleanup_batch,
    outbox, pb,
    persistence::{PreparedV004IndexEmbedding, Repository},
    qdrant::QdrantClient,
    scheduler::Scheduler,
};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::{sync::Arc, time::Duration};
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

struct FixedE2eEngine;

const E2E_DENSE_DIMENSION: usize = 1024;
const E2E_DENSE_COMPONENT: f32 = 0.03125;

fn e2e_seed_dense() -> Vec<f32> {
    vec![E2E_DENSE_COMPONENT; E2E_DENSE_DIMENSION]
}

fn e2e_graph_related_dense() -> Vec<f32> {
    (0..E2E_DENSE_DIMENSION)
        .map(|idx| {
            if idx % 2 == 0 {
                E2E_DENSE_COMPONENT
            } else {
                -E2E_DENSE_COMPONENT
            }
        })
        .collect()
}

fn e2e_dense_for_text(text: &str) -> Vec<f32> {
    if text.contains("graph expanded production evidence") {
        e2e_graph_related_dense()
    } else {
        e2e_seed_dense()
    }
}

#[async_trait]
impl InferenceEngine for FixedE2eEngine {
    async fn encode_batch(
        &self,
        inputs: Vec<InferenceInput>,
    ) -> Result<Vec<EmbeddingResult>, AstraError> {
        Ok(inputs
            .into_iter()
            .map(|input| EmbeddingResult {
                dense: Some(e2e_dense_for_text(&input.text)),
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

#[tokio::test]
async fn test_e2e_index_logical_document_via_tonic_ingestion_facade_and_activate() {
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
    let database_url =
        format!("postgres://astravector:astravector@127.0.0.1:{pg_port}/astravector");
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
    let collection = format!("astravector_ingest_e2e_{}", Uuid::new_v4().simple());
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
        4,
        50,
        None,
        RetryPolicyConfig::default(),
    )
    .expect("qdrant client");
    qdrant_client
        .ensure_collection(1024)
        .await
        .expect("create qdrant collection");

    let repo = Repository { pool: pool.clone() };
    let access_zone_id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let document_version = 1_u64;

    sqlx::query(
        "INSERT INTO astravector.access_zones(access_zone_id, access_zone_code, access_zone_name, status, default_ttl_days, ttl_policy_source, allow_never_expire)
         VALUES ($1,'1600','ingestion-e2e-zone','ACTIVE',365,'CODE_MATRIX',false)"
    )
    .bind(access_zone_id)
    .execute(&pool)
    .await
    .expect("insert access zone");

    let mut cfg = AppConfig::load().expect("load application config for ingestion facade E2E");
    cfg.qdrant.enabled = true;
    cfg.qdrant.url = format!("http://127.0.0.1:{qdrant_port}");
    cfg.qdrant.collection = collection.clone();
    cfg.dense.dimension = 1024;
    cfg.sparse.required = false;
    let cfg = Arc::new(cfg);
    let engine = Arc::new(FixedE2eEngine);
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
        .expect("bind random tonic port");
    let grpc_addr = listener.local_addr().expect("read tonic local addr");
    let grpc_shutdown = CancellationToken::new();
    let grpc_shutdown_for_server = grpc_shutdown.clone();
    let server_service = service.clone();
    let server_handle = tokio::spawn(async move {
        Server::builder()
            .add_service(AstraVectorIngestionFacadeServer::new(
                server_service.clone(),
            ))
            .add_service(AstraVectorV004ControlServer::new(server_service))
            .serve_with_incoming_shutdown(
                TcpListenerStream::new(listener),
                grpc_shutdown_for_server.cancelled(),
            )
            .await
            .expect("tonic ingestion/control server must run");
    });

    let mut ingestion_client =
        AstraVectorIngestionFacadeClient::connect(format!("http://{grpc_addr}"))
            .await
            .expect("generated tonic ingestion client must connect");
    let mut control_client = AstraVectorV004ControlClient::connect(format!("http://{grpc_addr}"))
        .await
        .expect("generated tonic control client must connect");

    let index_response = ingestion_client
        .index_logical_document(Request::new(pb::IndexLogicalDocumentRequest {
            context: Some(pb::RequestContext {
                correlation_id: "fix464-ingestion-e2e".into(),
                idempotency_key: format!("fix464-ingestion:{document_id}"),
                caller_service: "e2e-ingestion-client".into(),
                caller_user_id: "e2e".into(),
                caller_access_level: pb::AccessLevel::Restricted as i32,
            }),
            access_zone_id: access_zone_id.to_string(),
            access_zone_code: "1600".into(),
            document: Some(pb::DocumentIdentity {
                external_document_id: "external-fix464-ingestion".into(),
                document_id: document_id.to_string(),
                document_version,
                title: "fix464 real gRPC ingestion E2E".into(),
                source_uri: "internal://fix464/ingestion-e2e".into(),
                source_type: "TEST".into(),
                mime_type: "text/plain".into(),
                content_hash: String::new(),
                source_links: Vec::new(),
            }),
            blocks: vec![
                pb::LogicalBlock {
                    block_id: "doc-root".into(),
                    parent_block_id: String::new(),
                    block_type: pb::BlockType::Document as i32,
                    text: "fix464 real ingestion root document".into(),
                    order_index: 0,
                    source_location: Some(pb::SourceLocation {
                        page_start: 1,
                        page_end: 2,
                        ..Default::default()
                    }),
                    source_links: Vec::new(),
                    metadata: Default::default(),
                },
                pb::LogicalBlock {
                    block_id: "block-1".into(),
                    parent_block_id: "doc-root".into(),
                    block_type: pb::BlockType::Paragraph as i32,
                    text: "hello production candidate via real grpc ingestion".into(),
                    order_index: 1,
                    source_location: Some(pb::SourceLocation {
                        page_start: 1,
                        page_end: 1,
                        ..Default::default()
                    }),
                    source_links: Vec::new(),
                    metadata: Default::default(),
                },
                pb::LogicalBlock {
                    block_id: "block-2".into(),
                    parent_block_id: "doc-root".into(),
                    block_type: pb::BlockType::Paragraph as i32,
                    text: "graph expanded production evidence through ingestion facade".into(),
                    order_index: 2,
                    source_location: Some(pb::SourceLocation {
                        page_start: 2,
                        page_end: 2,
                        ..Default::default()
                    }),
                    source_links: Vec::new(),
                    metadata: Default::default(),
                },
            ],
            chunking_options: Some(pb::TokenAwareChunkingOptions {
                profile: pb::ChunkingProfile::Default as i32,
                parent_target_tokens: 64,
                parent_max_tokens: 128,
                child_target_tokens: 32,
                child_max_tokens: 64,
                child_overlap_tokens: 8,
                min_chunk_tokens: 1,
                preserve_block_boundaries: true,
                allow_split_inside_paragraph: true,
                allow_split_inside_table: false,
                create_parent_context: true,
            }),
            indexing_options: Some(pb::VectorIndexingOptions {
                activation_policy: pb::ActivationPolicy::Manual as i32,
                embedding_mode: pb::EmbeddingModeV005::DenseSparseIfAvailable as i32,
                publish_mode: pb::PublishModeV005::Outbox as i32,
                ttl_policy: Some(pb::TtlPolicy {
                    mode: pb::TtlMode::Relative as i32,
                    ttl_seconds: 3600,
                    expires_at: String::new(),
                    delete_from_qdrant_on_expire: true,
                    keep_metadata_after_expire: false,
                }),
                replace_existing_version: true,
            }),
            metadata: Default::default(),
        }))
        .await
        .expect("IndexLogicalDocument must work through real gRPC ingestion facade")
        .into_inner();
    let summary = index_response.summary.expect("indexing summary");
    assert!(
        summary.blocks_accepted >= 2,
        "ingestion facade must accept logical blocks"
    );
    assert!(
        summary.chunks_created > 0,
        "ingestion facade must create chunks"
    );
    assert!(
        summary.qdrant_points_scheduled > 0,
        "ingestion facade must schedule outbox events"
    );

    let outbox_shutdown = CancellationToken::new();
    outbox::spawn(
        repo.clone(),
        Arc::new(qdrant_client.clone()),
        "ingestion-e2e-publisher".into(),
        10,
        100,
        5,
        None,
        outbox_shutdown.clone(),
    );
    let expected_synced = i64::from(summary.qdrant_points_scheduled);
    let mut synced = false;
    for _ in 0..50 {
        let row = sqlx::query("SELECT count(*) AS cnt FROM astravector.vector_bindings_v004 WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status='SYNCED'")
            .bind(access_zone_id)
            .bind(document_id)
            .bind(document_version as i64)
            .fetch_one(&pool)
            .await
            .expect("check sync status after ingestion facade");
        if row.get::<i64, _>("cnt") >= expected_synced {
            synced = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    outbox_shutdown.cancel();
    assert!(
        synced,
        "real gRPC ingestion must sync all scheduled outbox events before activation"
    );

    let activation = control_client
        .activate_document_version(Request::new(pb::ActivateDocumentVersionRequest {
            access_zone_id: access_zone_id.to_string(),
            document_id: document_id.to_string(),
            document_version,
            force_activate: false,
            force_reason: String::new(),
        }))
        .await
        .expect("ActivateDocumentVersion must work through real gRPC control facade")
        .into_inner();
    assert_eq!(activation.status, "ACTIVE");

    grpc_shutdown.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_e2e_retrieve_context_full_rag_lifecycle_over_tonic_network() {
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
    let database_url =
        format!("postgres://astravector:astravector@127.0.0.1:{pg_port}/astravector");
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

    let collection = format!("astravector_e2e_{}", Uuid::new_v4().simple());
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
        4,
        50,
        None,
        RetryPolicyConfig::default(),
    )
    .expect("qdrant client");
    qdrant_client
        .ensure_collection(1024)
        .await
        .expect("create qdrant collection");

    let repo = Repository { pool: pool.clone() };
    let access_zone_id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let document_version = 1_i64;
    let chunk_id = Uuid::new_v4();
    let seed_chunk_id = Uuid::new_v4();
    let related_parent_id = Uuid::new_v4();
    let related_chunk_id = Uuid::new_v4();
    let content_hash = "a".repeat(64);
    let chunk_hash = "b".repeat(64);
    let seed_chunk_hash = "d".repeat(64);
    let related_parent_hash = "e".repeat(64);
    let related_chunk_hash = "c".repeat(64);

    sqlx::query(
        "INSERT INTO astravector.access_zones(access_zone_id, access_zone_code, access_zone_name, status, default_ttl_days, ttl_policy_source, allow_never_expire)
         VALUES ($1,'1500','e2e-zone','ACTIVE',365,'CODE_MATRIX',false)"
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
    .expect("register document version through production repository path");

    let chunk = GeneratedChunk {
        id: chunk_id,
        root_id: chunk_id,
        source_id: chunk_id,
        parent_id: None,
        granularity: Granularity::Parent,
        sequence_no: 0,
        token_count: 4,
        content: "hello production candidate".to_string(),
        content_hash: chunk_hash.clone(),
        source_block_id: Some("block-1".to_string()),
        source_block_ids: vec!["block-1".to_string()],
        source_location: json!({"page": 1}),
        source_links: json!([]),
        trace_relation_type: "EXACT".to_string(),
        trace_quality: "EXACT".to_string(),
    };
    let seed_chunk = GeneratedChunk {
        id: seed_chunk_id,
        root_id: chunk_id,
        source_id: chunk_id,
        parent_id: Some(chunk_id),
        granularity: Granularity::Sub180,
        sequence_no: 0,
        token_count: 4,
        content: "hello production candidate".to_string(),
        content_hash: seed_chunk_hash.clone(),
        source_block_id: Some("block-1".to_string()),
        source_block_ids: vec!["block-1".to_string()],
        source_location: json!({"page": 1}),
        source_links: json!([]),
        trace_relation_type: "EXACT".to_string(),
        trace_quality: "EXACT".to_string(),
    };
    let related_chunk = GeneratedChunk {
        id: related_chunk_id,
        root_id: related_parent_id,
        source_id: related_parent_id,
        parent_id: Some(related_parent_id),
        granularity: Granularity::Sub180,
        sequence_no: 1,
        token_count: 4,
        content: "graph expanded production evidence".to_string(),
        content_hash: related_chunk_hash.clone(),
        source_block_id: Some("block-2".to_string()),
        source_block_ids: vec!["block-2".to_string()],
        source_location: json!({"page": 2}),
        source_links: json!([]),
        trace_relation_type: "EXACT".to_string(),
        trace_quality: "EXACT".to_string(),
    };
    let related_parent = GeneratedChunk {
        id: related_parent_id,
        root_id: related_parent_id,
        source_id: related_parent_id,
        parent_id: None,
        granularity: Granularity::Parent,
        sequence_no: 1,
        token_count: 4,
        content: "graph expanded production evidence".to_string(),
        content_hash: related_parent_hash.clone(),
        source_block_id: Some("block-2".to_string()),
        source_block_ids: vec!["block-2".to_string()],
        source_location: json!({"page": 2}),
        source_links: json!([]),
        trace_relation_type: "EXACT".to_string(),
        trace_quality: "EXACT".to_string(),
    };
    let prepared = PreparedV004IndexEmbedding {
        chunk: chunk.clone(),
        embedding: EmbeddingResult {
            dense: Some(e2e_seed_dense()),
            sparse_indices: Some(vec![1, 2, 3]),
            sparse_values: Some(vec![0.9, 0.7, 0.5]),
            token_count: 4,
            truncated: false,
        },
    };
    let seed_prepared = PreparedV004IndexEmbedding {
        chunk: seed_chunk.clone(),
        embedding: EmbeddingResult {
            dense: Some(e2e_seed_dense()),
            sparse_indices: Some(vec![1, 2, 3]),
            sparse_values: Some(vec![0.9, 0.7, 0.5]),
            token_count: 4,
            truncated: false,
        },
    };
    let related_prepared = PreparedV004IndexEmbedding {
        chunk: related_chunk.clone(),
        embedding: EmbeddingResult {
            dense: Some(e2e_graph_related_dense()),
            sparse_indices: Some(vec![1, 2, 3]),
            sparse_values: Some(vec![0.8, 0.6, 0.4]),
            token_count: 4,
            truncated: false,
        },
    };

    let summary = repo
        .persist_v004_index_transactionally(
            access_zone_id,
            document_id,
            document_version,
            &[chunk, seed_chunk, related_parent, related_chunk],
            &[prepared, seed_prepared, related_prepared],
            "test-tokenizer",
            "test-chunker",
            2,
            Some(1),
            json!({
                "test": true,
                "quality_run_id": "fix476-e2e",
                "quality_fixture_relations_json": serde_json::to_string(&json!([{
                    "relation_id": "e2e-graph-block-1-to-block-2",
                    "from_document_uuid": document_id.to_string(),
                    "to_document_uuid": document_id.to_string(),
                    "from_block_id": "block-1",
                    "to_block_id": "block-2",
                    "relation_type": "RELATED_TO",
                    "weight": 1.0,
                    "quality_run_id": "fix476-e2e",
                    "quality_runtime_bench": "mega-validation"
                }])).expect("serialize quality fixture relation")
            }),
            "tenant-e2e",
            "workspace-e2e",
            "model-v1",
            "dense",
            "dense-v1",
            "sparse",
            "sparse-v1",
            0.0,
            64,
            &collection,
            true,
            true,
            Some(GraphBuildLimits::default()),
            100,
            true,
        )
        .await
        .expect("persist chunks, embeddings, bindings and outbox through production path");
    assert_eq!(summary.bindings, 3);
    assert_eq!(summary.outbox_created, 3);
    assert!(
        summary.graph_edges > 0,
        "graph persistence must create at least one relation edge for graph expansion; summary={summary:?}"
    );
    // Activate document for retrieval visibility after successful indexing path.
    sqlx::query(
        "UPDATE astravector.document_versions
         SET status='ACTIVE', lifecycle_status='ACTIVE', expires_at=now() + interval '1 hour', access_zone_code='1500', metadata='{}'::jsonb
         WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3"
    )
    .bind(access_zone_id)
    .bind(document_id)
    .bind(document_version)
    .execute(&pool)
    .await
    .expect("activate document version for retrieval");
    let expanded = repo
        .expand_chunks_1hop_by_seed_keys(
            &[(access_zone_id, seed_chunk_id)],
            2,
            3,
            5,
            200,
            &[
                "CHUNK_HAS_PARENT".to_string(),
                "CHUNK_PREVIOUS_SIBLING".to_string(),
                "CHUNK_NEXT_SIBLING".to_string(),
                "CHUNK_SAME_TABLE".to_string(),
                "CHUNK_SEMANTIC_SIMILAR".to_string(),
                "RELATED_TO".to_string(),
            ],
            None,
        )
        .await
        .expect("repository graph expansion must execute before RetrieveContext");
    assert!(
        expanded.iter().any(|related| related.chunk_id == related_chunk_id),
        "repository graph expansion must include the adjacent related chunk; expanded={expanded:?}; summary={summary:?}"
    );

    let shutdown = CancellationToken::new();
    outbox::spawn(
        repo.clone(),
        Arc::new(qdrant_client.clone()),
        "e2e-publisher".to_string(),
        10,
        100,
        5,
        None,
        shutdown.clone(),
    );

    let mut synced = false;
    for _ in 0..50 {
        let row = sqlx::query(
            "SELECT count(*) AS cnt
             FROM astravector.vector_bindings_v004
             WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3 AND qdrant_sync_status='SYNCED'"
        )
        .bind(access_zone_id)
        .bind(document_id)
        .bind(document_version)
        .fetch_one(&pool)
        .await
        .expect("check sync status");
        if row.get::<i64, _>("cnt") >= 3 {
            synced = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    shutdown.cancel();
    assert!(
        synced,
        "outbox publisher must publish binding to Qdrant and mark it SYNCED"
    );

    let hits = qdrant_client
        .search_dense(&e2e_seed_dense(), &[access_zone_id], 2, 5, None)
        .await
        .expect("qdrant dense search must work after outbox publish");
    assert!(
        !hits.is_empty(),
        "Qdrant search must find the outbox-published point"
    );

    let visible = repo
        .filter_visible_chunk_ids_multi(&[access_zone_id], &[chunk_id], 2)
        .await
        .expect("final visibility lookup before TTL");
    assert!(
        visible.contains(&(access_zone_id, chunk_id)),
        "chunk must be visible before TTL cleanup"
    );
    let texts = repo
        .fetch_chunk_texts_by_ids_multi(&[access_zone_id], &[chunk_id], 2)
        .await
        .expect("text fetch before TTL");
    assert_eq!(
        texts.get(&(access_zone_id, chunk_id)).map(String::as_str),
        Some("hello production candidate")
    );

    // fix462 P0: prove the public retrieval path, not just Qdrant+Repository helpers.
    // This invokes the actual RetrieveContext RPC implementation through its tonic trait.
    let mut cfg =
        AppConfig::load().expect("load application config for RetrieveContext service harness");
    cfg.qdrant.enabled = true;
    cfg.qdrant.url = format!("http://127.0.0.1:{qdrant_port}");
    cfg.qdrant.collection = collection.clone();
    cfg.dense.dimension = 1024;
    cfg.graph_rag.retrieval.enabled_by_default = true;
    cfg.graph_rag.retrieval.max_related_chunks = 5;
    cfg.graph_rag.retrieval.graph_expansion_result_limit = 10;
    cfg.graph_rag.retrieval.final_context_limit = 5;
    cfg.graph_rag.retrieval.graph_merge_strategy = "GRAPH_AS_CONTEXT_APPEND".to_string();
    cfg.graph_rag.retrieval.direct_context_limit = 2;
    cfg.graph_rag.retrieval.graph_context_append_limit = 3;
    cfg.graph_rag.scoring.graph_min_score = 0.0;
    cfg.graph_rag.rerank.mmr_enabled = true;
    cfg.sparse.required = false;
    let cfg = Arc::new(cfg);
    let engine = Arc::new(FixedE2eEngine);
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

    // fix462 enhanced gate: this is a network-level tonic E2E, not a trait-only service harness.
    // It verifies generated client/server serialization, tonic transport and request mapping.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random local tonic port");
    let grpc_addr = listener.local_addr().expect("read tonic local addr");
    let grpc_shutdown = CancellationToken::new();
    let grpc_shutdown_for_server = grpc_shutdown.clone();
    let server_service = service.clone();
    let server_handle = tokio::spawn(async move {
        Server::builder()
            .add_service(AstraVectorRetrievalFacadeServer::new(server_service))
            .serve_with_incoming_shutdown(
                TcpListenerStream::new(listener),
                grpc_shutdown_for_server.cancelled(),
            )
            .await
            .expect("tonic retrieval facade server must run");
    });
    let mut retrieval_client =
        AstraVectorRetrievalFacadeClient::connect(format!("http://{grpc_addr}"))
            .await
            .expect("generated tonic retrieval client must connect");

    let mut request_before = Request::new(pb::RetrieveContextRequest {
        context: Some(pb::RequestContext {
            correlation_id: "fix462-e2e-before".into(),
            idempotency_key: String::new(),
            caller_service: "e2e".into(),
            caller_user_id: "e2e".into(),
            caller_access_level: pb::AccessLevel::Restricted as i32,
        }),
        access_zone_id: access_zone_id.to_string(),
        question: "hello production candidate".into(),
        access_zone_ids: vec![access_zone_id.to_string()],
        access_zone_code: "1500".into(),
        access_zone_codes: vec!["1500".into()],
        profile: pb::RetrievalProfile::Balanced as i32,
        max_contexts: 5,
        filters: Vec::new(),
        response_detail: pb::ResponseDetail::Debug as i32,
        enable_graph_expansion: true,
        graph_max_hops: 1,
        graph_max_related_contexts: 3,
    });
    request_before.set_timeout(Duration::from_secs(5));
    let retrieve_before = retrieval_client
        .retrieve_context(request_before)
        .await
        .expect("RetrieveContext network RPC must work before TTL cleanup")
        .into_inner();
    let retrieve_before_summary = retrieve_before.summary;
    let retrieve_before_warnings = retrieve_before
        .warnings
        .iter()
        .map(|w| format!("{}:{}", w.code, w.message))
        .collect::<Vec<_>>();
    assert!(
        !retrieve_before.contexts.is_empty(),
        "RetrieveContext must return at least one context before TTL cleanup; summary={retrieve_before_summary:?}; warnings={retrieve_before_warnings:?}"
    );
    assert!(
        retrieve_before
            .contexts
            .iter()
            .any(|c| c.matched_text.contains("hello production candidate")),
        "RetrieveContext must return expected text before TTL cleanup"
    );
    assert!(
        retrieve_before
            .contexts
            .iter()
            .all(|c| !c.access_zone_id.is_empty() || c.metadata.contains_key("access_zone_id")),
        "RetrieveContext contexts must preserve access_zone_id lineage"
    );
    assert!(
        retrieve_before.contexts.iter().any(|c| c
            .metadata
            .get("retrieval_source")
            .map(|v| v == "VECTOR_DIRECT")
            .unwrap_or(false)
            || c.metadata
                .get("retrieval_sources")
                .map(|v| v.contains("VECTOR_DIRECT"))
                .unwrap_or(false)),
        "RetrieveContext must prove VECTOR_DIRECT source; contexts={:?}",
        retrieve_before
            .contexts
            .iter()
            .map(|c| (&c.matched_text, &c.metadata))
            .collect::<Vec<_>>()
    );
    assert!(
        retrieve_before.contexts.iter().any(|c| c
            .metadata
            .get("retrieval_source")
            .map(|v| v == "GRAPH_EXPANDED")
            .unwrap_or(false)
            || c.metadata
                .get("retrieval_sources")
                .map(|v| v.contains("GRAPH_EXPANDED"))
                .unwrap_or(false)),
        "RetrieveContext must prove GRAPH_EXPANDED source when graph expansion is enabled; contexts={:?}",
        retrieve_before
            .contexts
            .iter()
            .map(|c| (&c.matched_text, &c.metadata))
            .collect::<Vec<_>>()
    );
    assert!(
        retrieve_before.contexts.iter().any(|c| c
            .metadata
            .get("mmr_similarity_source")
            .map(|v| v.contains("dense") || v.contains("DENSE"))
            .unwrap_or(false)
            || c.metadata.contains_key("embedding_identity_key")),
        "RetrieveContext must prove MMR dense embedding mode when dense embeddings exist; contexts={:?}",
        retrieve_before
            .contexts
            .iter()
            .map(|c| (&c.matched_text, &c.metadata))
            .collect::<Vec<_>>()
    );

    sqlx::query(
        "UPDATE astravector.document_versions
         SET lifecycle_status='ACTIVE', expires_at=now() - interval '1 minute'
         WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3",
    )
    .bind(access_zone_id)
    .bind(document_id)
    .bind(document_version)
    .execute(&pool)
    .await
    .expect("expire document version");

    let stats = run_index_ttl_cleanup_batch(
        &repo,
        &qdrant_client,
        10,
        10,
        900,
        3600,
        10,
        30,
        3600,
        100,
        true,
    )
    .await
    .expect("ttl cleanup must succeed");
    assert_eq!(stats.claimed_documents, 1);

    let status_row = sqlx::query(
        "SELECT lifecycle_status FROM astravector.document_versions WHERE access_zone_id=$1 AND document_id=$2 AND document_version=$3"
    )
    .bind(access_zone_id)
    .bind(document_id)
    .bind(document_version)
    .fetch_one(&pool)
    .await
    .expect("read final document status");
    assert_eq!(status_row.get::<String, _>("lifecycle_status"), "DELETED");

    let found_after = qdrant_client
        .point_ids_by_document(access_zone_id, document_id, document_version)
        .await
        .expect("scroll qdrant after cleanup");
    assert!(
        found_after.is_empty(),
        "Qdrant points must be absent after TTL cleanup"
    );

    let visible_after = repo
        .filter_visible_chunk_ids_multi(&[access_zone_id], &[chunk_id], 2)
        .await
        .expect("final visibility lookup after TTL");
    assert!(
        visible_after.is_empty(),
        "visibility recheck must reject deleted/expired chunk after TTL cleanup"
    );
    let texts_after = repo
        .fetch_chunk_texts_by_ids_multi(&[access_zone_id], &[chunk_id], 2)
        .await
        .expect("text fetch after TTL");
    assert!(
        texts_after.is_empty(),
        "text fetch must not return stale content after TTL cleanup"
    );

    let mut request_after = Request::new(pb::RetrieveContextRequest {
        context: Some(pb::RequestContext {
            correlation_id: "fix462-e2e-after".into(),
            idempotency_key: String::new(),
            caller_service: "e2e".into(),
            caller_user_id: "e2e".into(),
            caller_access_level: pb::AccessLevel::Restricted as i32,
        }),
        access_zone_id: access_zone_id.to_string(),
        question: "hello production candidate".into(),
        access_zone_ids: vec![access_zone_id.to_string()],
        access_zone_code: "1500".into(),
        access_zone_codes: vec!["1500".into()],
        profile: pb::RetrievalProfile::Balanced as i32,
        max_contexts: 5,
        filters: Vec::new(),
        response_detail: pb::ResponseDetail::Debug as i32,
        enable_graph_expansion: true,
        graph_max_hops: 1,
        graph_max_related_contexts: 3,
    });
    request_after.set_timeout(Duration::from_secs(5));
    let retrieve_after = retrieval_client
        .retrieve_context(request_after)
        .await
        .expect("RetrieveContext network RPC must remain callable after TTL cleanup")
        .into_inner();
    assert!(
        retrieve_after.contexts.is_empty(),
        "RetrieveContext must return zero contexts after TTL cleanup"
    );
    grpc_shutdown.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
}
