use astravector_runtime::query_processing::{
    inherit_proven_graph_intents, CandidateIntentEvidence, CandidateIntentEvidenceReason,
};

#[test]
fn graph_inherits_only_evidence_passed_origin_intents() {
    let direct = vec![
        CandidateIntentEvidence::direct(3, Some(0.9), None, None, 3, 0, true),
        CandidateIntentEvidence::direct(4, Some(0.9), None, None, 0, 0, false),
        CandidateIntentEvidence::direct(3, None, Some(0.8), None, 2, 1, true),
    ];

    let inherited = inherit_proven_graph_intents(&direct);
    assert_eq!(inherited.len(), 1);
    assert_eq!(inherited[0].intent_id, 3);
    assert!(inherited[0].evidence_passed);
    assert_eq!(
        inherited[0].reason_code,
        CandidateIntentEvidenceReason::GraphOriginEvidence
    );
}

#[test]
fn graph_without_direct_evidence_cannot_create_coverage() {
    let direct = vec![CandidateIntentEvidence::direct(
        9,
        Some(0.99),
        None,
        None,
        0,
        0,
        false,
    )];
    assert!(inherit_proven_graph_intents(&direct).is_empty());
}
