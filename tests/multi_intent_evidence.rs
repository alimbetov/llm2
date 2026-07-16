use astravector_runtime::query_processing::coverage::{
    evaluate_intent_coverage, QueryEvidenceStatus,
};
use astravector_runtime::query_processing::{
    CandidateIntentEvidence, CandidateIntentEvidenceReason, QueryIntentKind, QueryIntentUnit,
};
use std::collections::HashSet;

fn intent(id: usize) -> QueryIntentUnit {
    QueryIntentUnit {
        id,
        kind: QueryIntentKind::ExplicitQuestion,
        text: format!("intent-{id}"),
        source_segment_indices: vec![0],
        source_token_start: 0,
        source_token_end: 10,
        normalized_byte_start: 0,
        normalized_byte_end: 10,
        original_byte_start: 0,
        original_byte_end: 10,
        required: true,
        searchable: true,
        weight: 1.0,
        normalized_sha256: format!("hash-{id}"),
    }
}

#[test]
fn candidate_evidence_covers_only_the_intent_that_passed() {
    let evidence = [
        CandidateIntentEvidence::direct(0, Some(0.92), Some(0.8), Some(0.7), 5, 1, true),
        CandidateIntentEvidence::direct(1, Some(0.92), Some(0.0), Some(0.0), 0, 0, false),
    ];
    let covered = evidence
        .iter()
        .filter(|item| item.evidence_passed)
        .map(|item| item.intent_id)
        .collect::<HashSet<_>>();
    let coverage = evaluate_intent_coverage(&[intent(0), intent(1)], &covered);

    assert_eq!(coverage.status, QueryEvidenceStatus::Degraded);
    assert_eq!(coverage.required_covered, 1);
    assert_eq!(coverage.uncovered_required_intent_ids, vec![1]);
    assert_eq!(
        evidence[0].reason_code,
        CandidateIntentEvidenceReason::ExactTechnicalMatch
    );
    assert_eq!(
        evidence[1].reason_code,
        CandidateIntentEvidenceReason::InsufficientEvidence
    );
}

#[test]
fn graph_candidate_inherits_only_proven_origin_intent() {
    let inherited = CandidateIntentEvidence::graph_origin(7);
    assert!(inherited.evidence_passed);
    assert_eq!(inherited.intent_id, 7);
    assert_eq!(
        inherited.reason_code,
        CandidateIntentEvidenceReason::GraphOriginEvidence
    );
}
