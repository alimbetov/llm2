use astravector_runtime::graph::{
    build_semantic_edges_in_memory, ChunkEmbeddingForGraph, GraphBuildLimits, GraphNode,
    GraphNodeType,
};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use uuid::Uuid;

fn fake_embedding(dim: usize, seed: f32) -> Vec<f32> {
    (0..dim)
        .map(|i| ((i as f32 + seed).sin() + 1.0) / 2.0)
        .collect()
}

fn fake_nodes_and_embeddings(
    count: usize,
    dim: usize,
) -> (Vec<GraphNode>, Vec<ChunkEmbeddingForGraph>) {
    let zone = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let mut nodes = Vec::new();
    let mut embeddings = Vec::new();
    for i in 0..count {
        let chunk_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("chunk-{i}").as_bytes());
        let node_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("node-{i}").as_bytes());
        nodes.push(GraphNode {
            access_zone_id: zone,
            node_id,
            node_type: GraphNodeType::Chunk,
            external_id: chunk_id.to_string(),
            document_id: Some(document_id),
            document_version: Some(1),
            chunk_id: Some(chunk_id),
            block_id: None,
            label: None,
            properties: json!({}),
            lifecycle_status: "ACTIVE".into(),
            expires_at: None,
            quarantined: false,
            access_level: 0,
        });
        embeddings.push(ChunkEmbeddingForGraph {
            chunk_id,
            embedding: fake_embedding(dim, i as f32),
        });
    }
    (nodes, embeddings)
}

fn bench_semantic_edges_100_chunks(c: &mut Criterion) {
    c.bench_function("semantic_edges_100_chunks_1024_dim", |b| {
        b.iter(|| {
            let (nodes, embeddings) = fake_nodes_and_embeddings(100, 1024);
            let _ = build_semantic_edges_in_memory(
                Uuid::new_v4(),
                Uuid::new_v4(),
                1,
                &nodes,
                &embeddings,
                0,
                None,
                &GraphBuildLimits::default(),
            );
        })
    });
}

fn bench_semantic_edges_500_chunks(c: &mut Criterion) {
    c.bench_function("semantic_edges_500_chunks_1024_dim", |b| {
        b.iter(|| {
            let (nodes, embeddings) = fake_nodes_and_embeddings(500, 1024);
            let _ = build_semantic_edges_in_memory(
                Uuid::new_v4(),
                Uuid::new_v4(),
                1,
                &nodes,
                &embeddings,
                0,
                None,
                &GraphBuildLimits::default(),
            );
        })
    });
}

fn bench_semantic_edges_1000_chunks(c: &mut Criterion) {
    c.bench_function("semantic_edges_1000_chunks_1024_dim", |b| {
        b.iter(|| {
            let (nodes, embeddings) = fake_nodes_and_embeddings(1000, 1024);
            let _ = build_semantic_edges_in_memory(
                Uuid::new_v4(),
                Uuid::new_v4(),
                1,
                &nodes,
                &embeddings,
                0,
                None,
                &GraphBuildLimits::default(),
            );
        })
    });
}

fn bench_semantic_edges_500_chunks_parallel(c: &mut Criterion) {
    c.bench_function("semantic_edges_500_chunks_1024_dim_parallel", |b| {
        b.iter(|| {
            let (nodes, embeddings) = fake_nodes_and_embeddings(500, 1024);
            let limits = GraphBuildLimits {
                semantic_parallel_enabled: true,
                ..GraphBuildLimits::default()
            };
            let _ = build_semantic_edges_in_memory(
                Uuid::new_v4(),
                Uuid::new_v4(),
                1,
                &nodes,
                &embeddings,
                0,
                None,
                &limits,
            );
        })
    });
}

criterion_group!(
    benches,
    bench_semantic_edges_100_chunks,
    bench_semantic_edges_500_chunks,
    bench_semantic_edges_1000_chunks,
    bench_semantic_edges_500_chunks_parallel
);
criterion_main!(benches);
