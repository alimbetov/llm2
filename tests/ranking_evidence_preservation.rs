use astravector_runtime::grpc::{apply_mmr_rerank, select_results_with_strategy_aware_mmr};
use astravector_runtime::pb;
use std::collections::HashMap;

fn candidate(
    id: &str,
    block: &str,
    score: f32,
    source: &str,
    protected: bool,
) -> pb::SearchResultV004 {
    let mut metadata = HashMap::from([
        ("source_block_id".into(), block.into()),
        ("retrieval_source".into(), source.into()),
        ("retrieval_sources".into(), format!("[\"{source}\"]")),
    ]);
    if protected {
        metadata.insert(
            "ranking_protection".into(),
            "PRIMARY_DIRECT,STRONG_LEXICAL,UNIQUE_SOURCE_BLOCK".into(),
        );
        metadata.insert("strong_lexical_evidence".into(), "true".into());
    }
    pb::SearchResultV004 {
        document_id: format!("doc-{id}"),
        document_version: 1,
        parent_chunk_id: format!("parent-{id}"),
        matched_chunk_id: id.into(),
        parent_text: format!("evidence for {block}"),
        matched_text: format!("evidence for {block}"),
        scores: Some(pb::SearchScoresV004 {
            dense_score: score,
            sparse_score: 0.0,
            fusion_score: score,
            final_score: score,
        }),
        citation: Some(pb::SearchCitationV004 { metadata }),
        ..Default::default()
    }
}

#[test]
fn primary_direct_evidence_survives_graph_expansion() {
    let direct = vec![candidate("primary", "primary", 0.2, "POSTGRES_FTS", true)];
    let graph = (0..5)
        .map(|idx| {
            candidate(
                &format!("graph-{idx}"),
                &format!("g-{idx}"),
                0.9,
                "GRAPH_EXPANDED",
                false,
            )
        })
        .collect();
    let selected = select_results_with_strategy_aware_mmr(
        direct,
        graph,
        3,
        "SCORE_THEN_TRUNCATE",
        2,
        1,
        true,
        0.75,
        0.8,
        0.65,
        20,
        "TOKEN_JACCARD",
        "TOKEN_JACCARD",
        true,
        true,
        5,
    );
    assert!(selected
        .results
        .iter()
        .any(|item| item.matched_chunk_id == "primary"));
}

#[test]
fn lexical_rank_2_technical_candidate_survives_fusion_admission() {
    let mut candidates = (0..20)
        .map(|idx| {
            candidate(
                &format!("dense-{idx}"),
                "distractor",
                1.0 - idx as f32 / 100.0,
                "VECTOR_DIRECT",
                false,
            )
        })
        .collect::<Vec<_>>();
    candidates.push(candidate(
        "lexical-rank-2",
        "technical",
        0.01,
        "POSTGRES_FTS",
        true,
    ));
    let selected = apply_mmr_rerank(
        candidates,
        5,
        false,
        0.8,
        10,
        "TOKEN_JACCARD",
        "TOKEN_JACCARD",
    );
    assert!(selected
        .results
        .iter()
        .any(|item| item.matched_chunk_id == "lexical-rank-2"));
}

#[test]
fn mmr_preserves_unique_primary_evidence() {
    let candidates = vec![
        candidate("distractor", "shared", 0.81, "VECTOR_DIRECT", false),
        candidate("primary", "unique-primary", 0.80, "POSTGRES_FTS", true),
        candidate("other", "other", 0.79, "VECTOR_DIRECT", false),
    ];
    let selected = apply_mmr_rerank(
        candidates,
        2,
        true,
        0.5,
        10,
        "TOKEN_JACCARD",
        "TOKEN_JACCARD",
    );
    assert!(selected
        .results
        .iter()
        .any(|item| item.matched_chunk_id == "primary"));
}

#[test]
fn expected_graph_edge_survives_final_token_budget() {
    let direct = vec![candidate("direct", "primary", 0.7, "VECTOR_DIRECT", true)];
    let graph = vec![candidate(
        "related",
        "unique-related",
        0.8,
        "GRAPH_EXPANDED",
        false,
    )];
    let selected = select_results_with_strategy_aware_mmr(
        direct,
        graph,
        2,
        "GRAPH_AS_CONTEXT_APPEND",
        1,
        1,
        false,
        0.75,
        0.8,
        0.65,
        10,
        "TOKEN_JACCARD",
        "TOKEN_JACCARD",
        true,
        true,
        5,
    );
    assert!(selected
        .results
        .iter()
        .any(|item| item.matched_chunk_id == "related"));
}

#[test]
fn graph_candidate_cannot_displace_all_direct_evidence() {
    let direct = vec![candidate("direct", "primary", 0.1, "VECTOR_DIRECT", true)];
    let graph = (0..5)
        .map(|idx| {
            candidate(
                &format!("graph-{idx}"),
                "related",
                0.9,
                "GRAPH_EXPANDED",
                false,
            )
        })
        .collect();
    let selected = select_results_with_strategy_aware_mmr(
        direct,
        graph,
        2,
        "SCORE_THEN_TRUNCATE",
        1,
        1,
        false,
        0.75,
        0.8,
        0.65,
        10,
        "TOKEN_JACCARD",
        "TOKEN_JACCARD",
        true,
        true,
        5,
    );
    assert!(selected
        .results
        .iter()
        .any(|item| item.matched_chunk_id == "direct"));
}

#[test]
fn protection_does_not_bypass_final_capacity() {
    let selected = apply_mmr_rerank(
        vec![
            candidate("protected-a", "a", 0.2, "POSTGRES_FTS", true),
            candidate("protected-b", "b", 0.1, "POSTGRES_FTS", true),
        ],
        1,
        false,
        0.8,
        10,
        "TOKEN_JACCARD",
        "TOKEN_JACCARD",
    );
    assert_eq!(selected.results.len(), 1);
}
