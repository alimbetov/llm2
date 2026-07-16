use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetrievalBranchStatus {
    SuccessWithEvidence,
    SuccessNoEvidence,
    Timeout,
    BackendUnavailable,
    Cancelled,
    SkippedBudget,
}

impl RetrievalBranchStatus {
    pub fn is_success(self) -> bool {
        matches!(self, Self::SuccessWithEvidence | Self::SuccessNoEvidence)
    }

    pub fn is_infrastructure_failure(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::BackendUnavailable | Self::Cancelled
        )
    }

    pub fn metric_label(self) -> &'static str {
        match self {
            Self::SuccessWithEvidence => "success_with_evidence",
            Self::SuccessNoEvidence => "success_no_evidence",
            Self::Timeout => "timeout",
            Self::BackendUnavailable => "backend_unavailable",
            Self::Cancelled => "cancelled",
            Self::SkippedBudget => "skipped_budget",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SegmentRetrievalStatus {
    Success,
    PartialFailure,
    Failed,
    Skipped,
}

pub fn summarize_retrieval_statuses(
    statuses: impl IntoIterator<Item = RetrievalBranchStatus>,
) -> SegmentRetrievalStatus {
    let statuses = statuses.into_iter().collect::<Vec<_>>();
    if statuses.is_empty()
        || statuses
            .iter()
            .all(|status| *status == RetrievalBranchStatus::SkippedBudget)
    {
        return SegmentRetrievalStatus::Skipped;
    }
    let success_count = statuses.iter().filter(|status| status.is_success()).count();
    let failure_count = statuses
        .iter()
        .filter(|status| status.is_infrastructure_failure())
        .count();
    match (success_count, failure_count) {
        (_, 0) => SegmentRetrievalStatus::Success,
        (0, _) => SegmentRetrievalStatus::Failed,
        _ => SegmentRetrievalStatus::PartialFailure,
    }
}

pub fn no_answer_is_eligible(statuses: &[RetrievalBranchStatus]) -> bool {
    !statuses.is_empty() && statuses.iter().all(|status| status.is_success())
}
