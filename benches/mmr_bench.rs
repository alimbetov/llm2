use astravector_runtime::{grpc::apply_mmr_rerank, pb};
use criterion::{criterion_group, criterion_main, Criterion};
use std::collections::HashMap;

fn search_result(id: usize, score: f32, with_embedding: bool) -> pb::SearchResultV004 {
    let mut metadata = HashMap::new();
    if with_embedding {
        let dim = 16usize;
        let mut vector = vec![0.0_f32; dim];
        vector[id % dim] = 1.0;
        metadata.insert(
            "embedding_normalized_json".to_string(),
            serde_json::to_string(&vector).unwrap(),
        );
    }
    pb::SearchResultV004 {
        document_id: "doc".to_string(),
        document_version: 1,
        root_chunk_id: "root".to_string(),
        source_chunk_id: format!("source-{id}"),
        parent_chunk_id: format!("parent-{id}"),
        matched_chunk_id: format!("chunk-{id}"),
        matched_granularity: pb::ChunkGranularityV004::ParentV004 as i32,
        parent_text: format!("parent context variant {id}"),
        scores: Some(pb::SearchScoresV004 {
            dense_score: score,
            sparse_score: 0.0,
            fusion_score: score,
            final_score: score,
        }),
        citation: Some(pb::SearchCitationV004 { metadata }),
        access_zone_id: "zone".to_string(),
        access_level: pb::AccessLevel::Public as i32,
        matched_text: if id % 3 == 0 {
            format!("early loan repayment without commission variant {id}")
        } else if id % 3 == 1 {
            format!("branch address service schedule variant {id}")
        } else {
            format!("card tariff fee condition variant {id}")
        },
    }
}

fn candidates(count: usize, embedding_mode: &str) -> Vec<pb::SearchResultV004> {
    (0..count)
        .map(|i| {
            let with_embedding = match embedding_mode {
                "dense" => true,
                "mixed" => i % 5 != 0,
                _ => false,
            };
            search_result(i, 1.0 - (i as f32 * 0.001), with_embedding)
        })
        .collect()
}

fn bench_mmr_production_dense_30_candidates_final_8(c: &mut Criterion) {
    c.bench_function("mmr_production_dense_30_candidates_final_8", |b| {
        b.iter(|| {
            apply_mmr_rerank(
                candidates(30, "dense"),
                8,
                true,
                0.75,
                30,
                "DENSE_EMBEDDING",
                "TOKEN_JACCARD",
            )
        })
    });
}

fn bench_mmr_production_mixed_30_candidates_final_8(c: &mut Criterion) {
    c.bench_function("mmr_production_mixed_30_candidates_final_8", |b| {
        b.iter(|| {
            apply_mmr_rerank(
                candidates(30, "mixed"),
                8,
                true,
                0.75,
                30,
                "DENSE_EMBEDDING",
                "TOKEN_JACCARD",
            )
        })
    });
}

fn bench_mmr_production_token_30_candidates_final_8(c: &mut Criterion) {
    c.bench_function("mmr_production_token_30_candidates_final_8", |b| {
        b.iter(|| {
            apply_mmr_rerank(
                candidates(30, "token"),
                8,
                true,
                0.75,
                30,
                "DENSE_EMBEDDING",
                "TOKEN_JACCARD",
            )
        })
    });
}

criterion_group!(
    benches,
    bench_mmr_production_dense_30_candidates_final_8,
    bench_mmr_production_mixed_30_candidates_final_8,
    bench_mmr_production_token_30_candidates_final_8
);
criterion_main!(benches);
