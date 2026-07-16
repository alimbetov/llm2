use astravector_runtime::query_processing::{
    no_answer_is_eligible, summarize_retrieval_statuses, RetrievalBranchStatus,
    SegmentRetrievalStatus,
};

#[test]
fn successful_empty_retrieval_is_the_only_no_evidence_case() {
    let statuses = [
        RetrievalBranchStatus::SuccessNoEvidence,
        RetrievalBranchStatus::SuccessNoEvidence,
    ];
    assert!(no_answer_is_eligible(&statuses));
    assert_eq!(
        summarize_retrieval_statuses(statuses),
        SegmentRetrievalStatus::Success
    );
}

#[test]
fn partial_backend_failure_is_degraded_not_no_answer() {
    for failure in [
        RetrievalBranchStatus::Timeout,
        RetrievalBranchStatus::BackendUnavailable,
        RetrievalBranchStatus::Cancelled,
    ] {
        let statuses = [RetrievalBranchStatus::SuccessNoEvidence, failure];
        assert!(!no_answer_is_eligible(&statuses));
        assert_eq!(
            summarize_retrieval_statuses(statuses),
            SegmentRetrievalStatus::PartialFailure
        );
    }
}

#[test]
fn all_infrastructure_failures_are_failed() {
    let statuses = [
        RetrievalBranchStatus::Timeout,
        RetrievalBranchStatus::BackendUnavailable,
    ];
    assert!(!no_answer_is_eligible(&statuses));
    assert_eq!(
        summarize_retrieval_statuses(statuses),
        SegmentRetrievalStatus::Failed
    );
}

#[test]
fn budget_skip_is_explicit_and_never_no_answer() {
    let statuses = [RetrievalBranchStatus::SkippedBudget];
    assert!(!no_answer_is_eligible(&statuses));
    assert_eq!(
        summarize_retrieval_statuses(statuses),
        SegmentRetrievalStatus::Skipped
    );
}
