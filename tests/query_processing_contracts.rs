use astravector_runtime::config::QueryProcessingConfig;
use astravector_runtime::query_processing::coverage::{
    evaluate_required_coverage, QueryEvidenceStatus,
};
use astravector_runtime::query_processing::fusion::{
    cross_segment_rrf, GlobalCandidateIdentity, SegmentCandidate,
};
use astravector_runtime::query_processing::{
    build_query_plan, QueryPlanningError, QueryProcessingMode, QueryTokenCounter,
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
            },
            SegmentCandidate {
                identity: identity("chunk-a"),
                segment_index: 1,
                rank: 1,
                score: 0.8,
                segment_weight: 1.0,
            },
            SegmentCandidate {
                identity: identity("chunk-b"),
                segment_index: 1,
                rank: 2,
                score: 0.7,
                segment_weight: 0.5,
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
