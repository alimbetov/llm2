use astravector_runtime::config::QueryProcessingConfig;
use astravector_runtime::query_processing::coverage::{
    evaluate_required_coverage, QueryEvidenceStatus,
};
use astravector_runtime::query_processing::fusion::{
    cross_segment_rrf, GlobalCandidateIdentity, SegmentCandidate,
};
use astravector_runtime::query_processing::{
    build_query_plan, extract_query_intents, QueryIntentKind, QueryPlanningError,
    QueryProcessingMode, QueryProcessingTier, QueryTokenCounter,
};

struct WhitespaceCounter;

impl QueryTokenCounter for WhitespaceCounter {
    fn count_tokens(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<usize, String> {
        let count = text.split_whitespace().count();
        if count > max_length && !allow_truncation {
            return Err(format!("{count} tokens exceeds max_length={max_length}"));
        }
        Ok(count.min(max_length))
    }
}

fn test_config() -> QueryProcessingConfig {
    QueryProcessingConfig {
        absolute_max_tokens: 64,
        absolute_max_bytes: 4096,
        segment_target_tokens: 8,
        segment_max_tokens: 10,
        segment_overlap_tokens: 2,
        max_segments: 6,
        max_parallel_segments: 3,
        per_segment_candidate_limit: 4,
        global_candidate_limit: 12,
        ..Default::default()
    }
}

fn words(count: usize) -> String {
    (0..count)
        .map(|i| format!("token{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn identity(chunk: &str) -> GlobalCandidateIdentity {
    GlobalCandidateIdentity {
        access_zone_id: "zone-a".into(),
        document_id: "doc-a".into(),
        document_version: 1,
        matched_chunk_id: chunk.into(),
        parent_chunk_id: chunk.into(),
        source_block_id: chunk.into(),
        representation_type: "ORIGINAL".into(),
        qdrant_point_id: Some(chunk.into()),
    }
}

#[test]
fn one_question_input_uses_automatic_modes() {
    let cfg = test_config();
    let single = build_query_plan("What does legal hold do?", &WhitespaceCounter, &cfg, 8).unwrap();
    assert_eq!(single.mode, QueryProcessingMode::Single);
    assert_eq!(single.segments.len(), 1);

    let segmented = build_query_plan(&words(24), &WhitespaceCounter, &cfg, 8).unwrap();
    assert_eq!(segmented.mode, QueryProcessingMode::Segmented);
    assert!(segmented.segments.len() > 1);
}

#[test]
fn exact_tier_boundaries_are_fail_closed() {
    let cfg = QueryProcessingConfig {
        extended_enabled: true,
        ..Default::default()
    };
    assert!(matches!(
        build_query_plan("   ", &WhitespaceCounter, &cfg, 256),
        Err(QueryPlanningError::Empty)
    ));
    for (count, expected) in [
        (1, QueryProcessingTier::Single),
        (256, QueryProcessingTier::Single),
        (257, QueryProcessingTier::SegmentedStandard),
        (1_024, QueryProcessingTier::SegmentedStandard),
        (1_025, QueryProcessingTier::SegmentedExtended),
        (2_048, QueryProcessingTier::SegmentedExtended),
    ] {
        let plan = build_query_plan(&words(count), &WhitespaceCounter, &cfg, 256).unwrap();
        assert_eq!(plan.tier, expected, "boundary {count}");
        assert_eq!(plan.original_token_count, count);
    }
    assert!(matches!(
        build_query_plan(&words(2_049), &WhitespaceCounter, &cfg, 256),
        Err(QueryPlanningError::TokenLimitExceeded)
    ));
}

#[test]
fn effective_limits_are_frozen_into_the_selected_plan() {
    let cfg = QueryProcessingConfig {
        extended_enabled: true,
        ..Default::default()
    };
    let standard = build_query_plan(&words(257), &WhitespaceCounter, &cfg, 256).unwrap();
    let extended = build_query_plan(&words(1_025), &WhitespaceCounter, &cfg, 256).unwrap();
    assert_eq!(standard.limits.max_segments, 7);
    assert_eq!(standard.limits.local_fused_candidate_limit, 18);
    assert_eq!(standard.limits.max_graph_seeds, 8);
    assert_eq!(standard.limits.admission_weight, 3);
    assert_eq!(extended.limits.max_segments, 14);
    assert_eq!(extended.limits.local_fused_candidate_limit, 10);
    assert_eq!(extended.limits.max_graph_seeds, 10);
    assert_eq!(extended.limits.admission_weight, 6);
}

#[test]
fn long_query_disabled_fails_closed_without_truncation() {
    let cfg = QueryProcessingConfig {
        enabled: false,
        ..test_config()
    };
    let error = build_query_plan(&words(24), &WhitespaceCounter, &cfg, 8).unwrap_err();
    assert!(matches!(error, QueryPlanningError::LongQueryNotSupported));
}

#[test]
fn too_large_query_is_out_of_range_contract() {
    let cfg = test_config();
    let error = build_query_plan(&words(65), &WhitespaceCounter, &cfg, 8).unwrap_err();
    assert!(matches!(error, QueryPlanningError::TokenLimitExceeded));
}

#[test]
fn absolute_limit_cannot_be_bypassed_by_a_larger_extended_tier() {
    let cfg = QueryProcessingConfig {
        absolute_max_tokens: 32,
        extended_enabled: true,
        ..test_config()
    };
    let error = build_query_plan(&words(33), &WhitespaceCounter, &cfg, 8).unwrap_err();
    assert!(matches!(error, QueryPlanningError::TokenLimitExceeded));
}

#[test]
fn successful_plan_never_truncates_segments() {
    let cfg = test_config();
    let plan = build_query_plan(&words(32), &WhitespaceCounter, &cfg, 8).unwrap();
    assert_eq!(plan.mode, QueryProcessingMode::Segmented);
    assert!(plan
        .segments
        .iter()
        .all(|segment| segment.token_count <= cfg.segment_max_tokens));
}

#[test]
fn global_rrf_deduplicates_full_identity() {
    let fused = cross_segment_rrf(
        [
            SegmentCandidate {
                identity: identity("chunk-a"),
                segment_index: 0,
                rank: 1,
                score: 0.9,
                segment_weight: 1.0,
                intent_unit_ids: vec![0],
            },
            SegmentCandidate {
                identity: identity("chunk-a"),
                segment_index: 1,
                rank: 1,
                score: 0.8,
                segment_weight: 1.0,
                intent_unit_ids: vec![0],
            },
            SegmentCandidate {
                identity: identity("chunk-b"),
                segment_index: 1,
                rank: 2,
                score: 0.7,
                segment_weight: 0.5,
                intent_unit_ids: vec![1],
            },
        ],
        60.0,
        10,
    );
    assert_eq!(fused.len(), 2);
    assert_eq!(fused[0].identity.matched_chunk_id, "chunk-a");
    assert_eq!(fused[0].matched_segments, vec![0, 1]);
}

#[test]
fn overlap_fusion_uses_best_contribution_per_logical_intent() {
    let fused = cross_segment_rrf(
        [
            SegmentCandidate {
                identity: identity("chunk-a"),
                segment_index: 0,
                rank: 1,
                score: 0.9,
                segment_weight: 1.0,
                intent_unit_ids: vec![42],
            },
            SegmentCandidate {
                identity: identity("chunk-a"),
                segment_index: 1,
                rank: 2,
                score: 0.8,
                segment_weight: 1.0,
                intent_unit_ids: vec![42],
            },
        ],
        60.0,
        10,
    );
    assert_eq!(fused[0].score, 1.0 / 61.0);
    assert_eq!(fused[0].matched_segments, vec![0, 1]);
}

#[test]
fn retrieval_pipeline_has_one_graph_and_one_mmr_stage() {
    let grpc_source = include_str!("../src/grpc/mod.rs");

    assert_eq!(
        grpc_source
            .matches("self.repo()?.expand_chunks_1hop_by_seed_keys(")
            .count(),
        1,
        "Search must invoke GraphRAG expansion at most once per request"
    );
    assert_eq!(
        grpc_source
            .matches("let selection_result = select_results_with_strategy_aware_mmr(")
            .count(),
        1,
        "Search must invoke final MMR selection at most once per request"
    );
}

#[test]
fn production_profile_keeps_extended_tier_disabled() {
    let production_config = include_str!("../config/application-prod.yaml");

    assert!(
        production_config
            .contains("extended_enabled: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_ENABLED:-false}"),
        "Extended tier must remain opt-in in production"
    );
}

#[test]
fn physical_segments_preserve_tail_and_bounded_overlap() {
    let cfg = QueryProcessingConfig {
        extended_enabled: true,
        ..Default::default()
    };
    let plan = build_query_plan(&words(2_048), &WhitespaceCounter, &cfg, 256).unwrap();
    assert!(plan.segments.len() <= 14);
    assert_eq!(plan.segments.first().unwrap().source_token_start, 0);
    assert_eq!(plan.segments.last().unwrap().source_token_end, 2_048);
    assert!(plan
        .segments
        .iter()
        .all(|segment| !segment.text.is_empty() && segment.token_count <= 220));
    for pair in plan.segments.windows(2) {
        let overlap = pair[0]
            .source_token_end
            .saturating_sub(pair[1].source_token_start);
        assert!(overlap <= cfg.segment_overlap_tokens);
        assert!(pair[1].source_token_start < pair[1].source_token_end);
    }
}

#[test]
fn logical_intents_are_independent_from_physical_tail_segments() {
    let cfg = test_config();
    let query = format!(
        "Explain legal hold cleanup. SELECT * FROM audit_log {}",
        words(20)
    );
    let plan = build_query_plan(&query, &WhitespaceCounter, &cfg, 4).unwrap();
    let intents = extract_query_intents(&query, &plan.segments);
    assert!(intents
        .iter()
        .any(|intent| intent.kind == QueryIntentKind::ImperativeRequest && intent.required));
    assert!(intents
        .iter()
        .any(|intent| intent.kind == QueryIntentKind::TechnicalEvidence && !intent.required));
    assert!(!plan.segments.last().unwrap().required_for_coverage);
}

#[test]
fn segment_aware_coverage_reports_partial() {
    let cfg = test_config();
    let plan = build_query_plan(
        "Context only. What does legal hold do? Which ORA-00904 fixture applies?",
        &WhitespaceCounter,
        &cfg,
        4,
    )
    .unwrap();
    let required = plan
        .segments
        .iter()
        .filter(|segment| segment.required_for_coverage)
        .map(|segment| segment.index)
        .collect::<Vec<_>>();
    assert!(!required.is_empty());
    let coverage = evaluate_required_coverage(&plan.segments, &[required[0]].into_iter().collect());
    assert!(matches!(
        coverage.status,
        QueryEvidenceStatus::Found | QueryEvidenceStatus::Degraded
    ));
}
