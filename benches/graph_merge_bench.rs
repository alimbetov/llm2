use astravector_runtime::{grpc::merge_search_results_before_truncate, pb};
use criterion::{criterion_group, criterion_main, Criterion};

fn fake_result(id: usize, score: f32) -> pb::SearchResultV004 {
    pb::SearchResultV004 {
        document_id: "doc-bench".to_string(),
        document_version: 1,
        root_chunk_id: "root".to_string(),
        source_chunk_id: format!("source-{id}"),
        parent_chunk_id: format!("parent-{id}"),
        matched_chunk_id: format!("chunk-{id}"),
        matched_granularity: pb::ChunkGranularityV004::Sub260V004 as i32,
        parent_text: format!("parent text {id}"),
        scores: Some(pb::SearchScoresV004 {
            dense_score: score,
            sparse_score: 0.0,
            fusion_score: score,
            final_score: score,
        }),
        citation: Some(pb::SearchCitationV004 {
            metadata: std::collections::HashMap::new(),
        }),
        access_zone_id: "zone".to_string(),
        access_level: pb::AccessLevel::Internal as i32,
        matched_text: format!("matched text {id}"),
    }
}

fn direct_candidates(count: usize) -> Vec<pb::SearchResultV004> {
    (0..count)
        .map(|i| fake_result(i, 1.0 - (i as f32 / count.max(1) as f32) * 0.2))
        .collect()
}

fn graph_candidates(count: usize, offset: usize) -> Vec<pb::SearchResultV004> {
    (0..count)
        .map(|i| fake_result(i + offset, 0.9 - (i as f32 / count.max(1) as f32) * 0.3))
        .collect()
}

fn bench_merge_100_direct_100_graph_overlap(c: &mut Criterion) {
    c.bench_function("merge_100_direct_100_graph_50_percent_overlap", |b| {
        b.iter(|| {
            merge_search_results_before_truncate(
                direct_candidates(100),
                graph_candidates(100, 50),
                20,
                "SCORE_THEN_TRUNCATE",
                6,
                2,
            )
        })
    });
}

fn bench_merge_direct_first(c: &mut Criterion) {
    c.bench_function("merge_direct_first_100_100", |b| {
        b.iter(|| {
            merge_search_results_before_truncate(
                direct_candidates(100),
                graph_candidates(100, 50),
                20,
                "DIRECT_FIRST",
                6,
                2,
            )
        })
    });
}

fn bench_merge_graph_as_context_append(c: &mut Criterion) {
    c.bench_function("merge_graph_as_context_append_100_100", |b| {
        b.iter(|| {
            merge_search_results_before_truncate(
                direct_candidates(100),
                graph_candidates(100, 50),
                20,
                "GRAPH_AS_CONTEXT_APPEND",
                6,
                2,
            )
        })
    });
}

criterion_group!(
    benches,
    bench_merge_100_direct_100_graph_overlap,
    bench_merge_direct_first,
    bench_merge_graph_as_context_append
);
criterion_main!(benches);
