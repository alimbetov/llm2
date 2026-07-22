use crate::persistence::HydratedSearchContext;
use metrics::{counter, histogram};
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tonic::{codegen::Bytes, Code, Status};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HydrationCandidateIdentity {
    pub access_zone_id: Uuid,
    pub binding_id: Uuid,
    pub matched_chunk_id: Uuid,
    pub parent_chunk_id: Uuid,
    pub granularity: String,
    pub raw_rank: usize,
    pub input_ordinal: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HydrationRejectionReason {
    BindingInvalid,
    VisibilityRejected,
    HydrationMissing,
    ParentHydrationTimeout,
    EmptyContext,
}

impl HydrationRejectionReason {
    pub const fn code(self) -> &'static str {
        match self {
            Self::BindingInvalid => "BINDING_INVALID",
            Self::VisibilityRejected => "VISIBILITY_REJECTED",
            Self::HydrationMissing => "HYDRATION_MISSING",
            Self::ParentHydrationTimeout => "PARENT_HYDRATION_TIMEOUT",
            Self::EmptyContext => "EMPTY_CONTEXT",
        }
    }

    pub const fn retryable(self) -> bool {
        matches!(self, Self::HydrationMissing | Self::ParentHydrationTimeout)
    }

    pub const fn is_parent_scoped(self) -> bool {
        matches!(
            self,
            Self::HydrationMissing | Self::ParentHydrationTimeout | Self::EmptyContext
        )
    }
}

#[derive(Debug, Clone)]
pub struct RejectedHydrationCandidate {
    pub candidate: HydrationCandidateIdentity,
    pub reason: HydrationRejectionReason,
    pub stage: &'static str,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub enum HydrationTerminalOutcome {
    Hydrated {
        candidate: HydrationCandidateIdentity,
        context: Box<HydratedSearchContext>,
        elapsed: Duration,
    },
    BindingInvalid(RejectedHydrationCandidate),
    VisibilityRejected(RejectedHydrationCandidate),
    HydrationMissing(RejectedHydrationCandidate),
    ParentHydrationTimeout(RejectedHydrationCandidate),
    EmptyContext(RejectedHydrationCandidate),
}

impl HydrationTerminalOutcome {
    pub fn candidate(&self) -> &HydrationCandidateIdentity {
        match self {
            Self::Hydrated { candidate, .. } => candidate,
            Self::BindingInvalid(value)
            | Self::VisibilityRejected(value)
            | Self::HydrationMissing(value)
            | Self::ParentHydrationTimeout(value)
            | Self::EmptyContext(value) => &value.candidate,
        }
    }

    pub fn reason(&self) -> Option<HydrationRejectionReason> {
        match self {
            Self::Hydrated { .. } => None,
            Self::BindingInvalid(_) => Some(HydrationRejectionReason::BindingInvalid),
            Self::VisibilityRejected(_) => Some(HydrationRejectionReason::VisibilityRejected),
            Self::HydrationMissing(_) => Some(HydrationRejectionReason::HydrationMissing),
            Self::ParentHydrationTimeout(_) => {
                Some(HydrationRejectionReason::ParentHydrationTimeout)
            }
            Self::EmptyContext(_) => Some(HydrationRejectionReason::EmptyContext),
        }
    }

    pub fn into_context(self) -> Option<HydratedSearchContext> {
        match self {
            Self::Hydrated { context, .. } => Some(*context),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct HydrationBatchOutcomes {
    pub outcomes: Vec<HydrationTerminalOutcome>,
}

impl HydrationBatchOutcomes {
    pub fn new(requested_candidates: usize, mut outcomes: Vec<HydrationTerminalOutcome>) -> Self {
        outcomes.sort_by_key(|outcome| outcome.candidate().input_ordinal);
        assert_exhaustive_outcomes(requested_candidates, &outcomes);
        Self { outcomes }
    }

    pub fn hydrated_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|value| matches!(value, HydrationTerminalOutcome::Hydrated { .. }))
            .count()
    }

    pub fn rejected_count(&self) -> usize {
        self.outcomes.len().saturating_sub(self.hydrated_count())
    }
}

pub fn assert_exhaustive_outcomes(
    requested_candidates: usize,
    outcomes: &[HydrationTerminalOutcome],
) {
    let hydrated_outcomes = outcomes
        .iter()
        .filter(|value| matches!(value, HydrationTerminalOutcome::Hydrated { .. }))
        .count();
    let rejected_outcomes = outcomes.len().saturating_sub(hydrated_outcomes);
    assert_eq!(
        requested_candidates,
        hydrated_outcomes + rejected_outcomes,
        "requested_candidates == hydrated_outcomes + rejected_outcomes"
    );
    for (expected, outcome) in outcomes.iter().enumerate() {
        assert_eq!(expected, outcome.candidate().input_ordinal);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageClass {
    Full,
    Partial,
    NoneDueToInfrastructureFailure,
}

impl CoverageClass {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::Partial => "PARTIAL",
            Self::NoneDueToInfrastructureFailure => "NONE_DUE_TO_INFRASTRUCTURE_FAILURE",
        }
    }
}

pub struct NormalizedHydration {
    pub surviving_contexts: Vec<HydratedSearchContext>,
    pub dropped_parents: Vec<RejectedHydrationCandidate>,
    pub rejected_parent_keys: HashSet<(Uuid, Uuid)>,
    pub coverage_class: CoverageClass,
    pub retryable: bool,
}

impl NormalizedHydration {
    pub fn has_total_timeout(&self) -> bool {
        self.surviving_contexts.is_empty()
            && !self.dropped_parents.is_empty()
            && self
                .dropped_parents
                .iter()
                .all(|value| value.reason == HydrationRejectionReason::ParentHydrationTimeout)
    }

    pub fn to_proto(&self) -> crate::pb::RetrievalDegradationV005 {
        let coverage_class = match self.coverage_class {
            CoverageClass::Full => {
                crate::pb::RetrievalCoverageClassV005::RetrievalCoverageClassFull
            }
            CoverageClass::Partial => {
                crate::pb::RetrievalCoverageClassV005::RetrievalCoverageClassPartial
            }
            CoverageClass::NoneDueToInfrastructureFailure => crate::pb::RetrievalCoverageClassV005::RetrievalCoverageClassNoneDueToInfrastructureFailure,
        };
        let infrastructure_failure = self.dropped_parents.iter().any(|value| {
            matches!(
                value.reason,
                HydrationRejectionReason::HydrationMissing
                    | HydrationRejectionReason::ParentHydrationTimeout
            )
        });
        crate::pb::RetrievalDegradationV005 {
            degraded: !self.dropped_parents.is_empty(),
            degradation_class: self.coverage_class.code().into(),
            retryable: self.retryable,
            coverage_class: coverage_class as i32,
            dropped_parents: self
                .dropped_parents
                .iter()
                .map(|dropped| crate::pb::DroppedParentSummaryV005 {
                    parent_id: opaque_parent_id(&dropped.candidate),
                    reason: dropped.reason.code().into(),
                    rejection_stage: dropped.stage.into(),
                    retryable: dropped.reason.retryable(),
                    input_ordinal: dropped.candidate.input_ordinal as u32,
                })
                .collect(),
            infrastructure_failure,
            full_hydration_failure: self.surviving_contexts.is_empty()
                && !self.dropped_parents.is_empty(),
        }
    }
}

fn opaque_parent_id(candidate: &HydrationCandidateIdentity) -> String {
    let mut hasher = Sha256::new();
    hasher.update(candidate.access_zone_id.as_bytes());
    hasher.update(candidate.parent_chunk_id.as_bytes());
    format!("parent:{}", &hex::encode(hasher.finalize())[..16])
}

pub fn normalize_hydration_outcomes(
    entry_point: &'static str,
    batch: HydrationBatchOutcomes,
) -> NormalizedHydration {
    let rejected_parent_keys = batch
        .outcomes
        .iter()
        .filter_map(|outcome| {
            outcome
                .reason()
                .filter(|reason| reason.is_parent_scoped())
                .map(|_| {
                    let candidate = outcome.candidate();
                    (candidate.access_zone_id, candidate.parent_chunk_id)
                })
        })
        .collect::<HashSet<_>>();
    let surviving_parent_exists = batch.outcomes.iter().any(|outcome| {
        matches!(outcome, HydrationTerminalOutcome::Hydrated { .. }) && {
            let candidate = outcome.candidate();
            let key = (candidate.access_zone_id, candidate.parent_chunk_id);
            !rejected_parent_keys.contains(&key)
        }
    });
    let total_timeout = !surviving_parent_exists
        && !rejected_parent_keys.is_empty()
        && batch.outcomes.iter().all(|outcome| {
            outcome.reason().is_none()
                || outcome.reason() == Some(HydrationRejectionReason::ParentHydrationTimeout)
        });
    let mut surviving_contexts = Vec::new();
    let mut dropped_parents = Vec::new();
    let mut recorded_parent_rejections = HashMap::new();
    for outcome in batch.outcomes {
        match outcome {
            HydrationTerminalOutcome::Hydrated {
                candidate,
                context,
                elapsed,
            } => {
                let parent_key = (candidate.access_zone_id, candidate.parent_chunk_id);
                if rejected_parent_keys.contains(&parent_key) {
                    counter!(
                        "candidate_rejections_total",
                        "entry_point" => entry_point,
                        "reason" => "PARENT_SCOPED_REJECTION"
                    )
                    .increment(1);
                    continue;
                }
                counter!(
                    "parent_hydration_requests_total",
                    "entry_point" => entry_point,
                    "outcome" => "hydrated"
                )
                .increment(1);
                histogram!(
                    "parent_hydration_duration_seconds",
                    "entry_point" => entry_point,
                    "outcome" => "HYDRATED"
                )
                .record(elapsed.as_secs_f64());
                surviving_contexts.push(*context);
            }
            rejected => {
                let reason = rejected.reason().expect("rejected outcome has reason");
                let rejected = match rejected {
                    HydrationTerminalOutcome::BindingInvalid(value)
                    | HydrationTerminalOutcome::VisibilityRejected(value)
                    | HydrationTerminalOutcome::HydrationMissing(value)
                    | HydrationTerminalOutcome::ParentHydrationTimeout(value)
                    | HydrationTerminalOutcome::EmptyContext(value) => value,
                    HydrationTerminalOutcome::Hydrated { .. } => unreachable!(),
                };
                let parent_key = (
                    rejected.candidate.access_zone_id,
                    rejected.candidate.parent_chunk_id,
                );
                counter!(
                    "candidate_rejections_total",
                    "entry_point" => entry_point,
                    "reason" => reason.code()
                )
                .increment(1);
                counter!(
                    "parent_hydration_requests_total",
                    "entry_point" => entry_point,
                    "outcome" => reason.code()
                )
                .increment(1);
                if matches!(
                    reason,
                    HydrationRejectionReason::BindingInvalid
                        | HydrationRejectionReason::VisibilityRejected
                ) {
                    counter!(
                        "stale_candidate_rejections_total",
                        "entry_point" => entry_point,
                        "reason" => reason.code()
                    )
                    .increment(1);
                }
                if reason == HydrationRejectionReason::ParentHydrationTimeout {
                    counter!(
                        "hydration_timeouts_total",
                        "entry_point" => entry_point,
                        "scope" => if total_timeout { "total" } else { "selected" }
                    )
                    .increment(1);
                }
                histogram!(
                    "parent_hydration_duration_seconds",
                    "entry_point" => entry_point,
                    "outcome" => reason.code()
                )
                .record(rejected.elapsed.as_secs_f64());
                if !reason.is_parent_scoped()
                    || recorded_parent_rejections
                        .insert(parent_key, reason)
                        .is_none()
                {
                    dropped_parents.push(rejected);
                }
            }
        }
    }
    let has_infrastructure_failure = dropped_parents.iter().any(|value| {
        matches!(
            value.reason,
            HydrationRejectionReason::HydrationMissing
                | HydrationRejectionReason::ParentHydrationTimeout
        )
    });
    let coverage_class = if dropped_parents.is_empty() {
        CoverageClass::Full
    } else if !surviving_contexts.is_empty() || !has_infrastructure_failure {
        CoverageClass::Partial
    } else {
        CoverageClass::NoneDueToInfrastructureFailure
    };
    let retryable = dropped_parents.iter().any(|value| value.reason.retryable());
    if !dropped_parents.is_empty() {
        counter!(
            "degraded_requests_total",
            "entry_point" => entry_point,
            "reason" => if coverage_class == CoverageClass::Partial {
                "partial_hydration"
            } else {
                "total_hydration"
            }
        )
        .increment(1);
    }
    NormalizedHydration {
        surviving_contexts,
        dropped_parents,
        rejected_parent_keys,
        coverage_class,
        retryable,
    }
}

pub fn total_hydration_timeout_status(degradation: &crate::pb::RetrievalDegradationV005) -> Status {
    let mut structured_status_details = Vec::new();
    degradation
        .encode(&mut structured_status_details)
        .expect("protobuf degradation encoding");
    let normal_response_body_absent = true;
    debug_assert!(normal_response_body_absent);
    Status::with_details(
        Code::DeadlineExceeded,
        "PARENT_HYDRATION_TIMEOUT: all canonical parents exceeded the hydration deadline",
        Bytes::from(structured_status_details),
    )
}

pub fn bounded_hydration_fetch_window(
    requested_final_count: u32,
    requested_candidate_limit: u32,
    hydration_rejection_reserve: u32,
    hydration_rejection_reserve_max: u32,
    candidate_limit_max: u32,
) -> u32 {
    requested_candidate_limit
        .max(
            requested_final_count
                .saturating_add(hydration_rejection_reserve.min(hydration_rejection_reserve_max)),
        )
        .min(candidate_limit_max.max(requested_final_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn candidate(input_ordinal: usize) -> HydrationCandidateIdentity {
        HydrationCandidateIdentity {
            access_zone_id: Uuid::new_v4(),
            binding_id: Uuid::new_v4(),
            matched_chunk_id: Uuid::new_v4(),
            parent_chunk_id: Uuid::new_v4(),
            granularity: "SUB_180".into(),
            raw_rank: input_ordinal,
            input_ordinal,
        }
    }

    fn context(candidate: &HydrationCandidateIdentity) -> HydratedSearchContext {
        HydratedSearchContext {
            access_zone_id: candidate.access_zone_id,
            matched_chunk_id: candidate.matched_chunk_id,
            parent_chunk_id: candidate.parent_chunk_id,
            document_id: Uuid::new_v4(),
            document_version: 1,
            root_chunk_id: Uuid::new_v4(),
            source_chunk_id: Uuid::new_v4(),
            matched_text: "matched child".into(),
            parent_text: "canonical parent".into(),
            parent_content_hash: "hash".into(),
            parent_token_count: 2,
            parent_sequence_no: 0,
            access_level: 1,
            source_block_id: Some("block".into()),
            source_location: json!({}),
            source_links: json!([]),
            metadata: json!({}),
            parent_metadata: json!({}),
        }
    }

    fn rejected(
        candidate: HydrationCandidateIdentity,
        reason: HydrationRejectionReason,
    ) -> RejectedHydrationCandidate {
        RejectedHydrationCandidate {
            candidate,
            reason,
            stage: "CANONICAL_PARENT_HYDRATION",
            elapsed: Duration::from_millis(1),
        }
    }

    #[test]
    fn partial_timeout_preserves_survivor_and_reports_retryable_drop() {
        let first = candidate(0);
        let second = candidate(1);
        let batch = HydrationBatchOutcomes::new(
            2,
            vec![
                HydrationTerminalOutcome::Hydrated {
                    candidate: first.clone(),
                    context: Box::new(context(&first)),
                    elapsed: Duration::from_millis(1),
                },
                HydrationTerminalOutcome::ParentHydrationTimeout(rejected(
                    second,
                    HydrationRejectionReason::ParentHydrationTimeout,
                )),
            ],
        );
        let normalized = normalize_hydration_outcomes("Search", batch);
        assert_eq!(normalized.surviving_contexts.len(), 1);
        assert_eq!(normalized.dropped_parents.len(), 1);
        assert_eq!(normalized.coverage_class, CoverageClass::Partial);
        assert!(normalized.retryable);
        assert!(!normalized.has_total_timeout());
        let degradation = normalized.to_proto();
        assert!(degradation.degraded);
        assert_eq!(
            degradation.dropped_parents[0].reason,
            "PARENT_HYDRATION_TIMEOUT"
        );
        assert!(degradation.dropped_parents[0].retryable);
        assert!(!degradation.dropped_parents[0].parent_id.contains(
            &normalized.dropped_parents[0]
                .candidate
                .parent_chunk_id
                .to_string()
        ));
    }

    #[test]
    fn total_timeout_is_deadline_exceeded_with_structured_details() {
        let only = candidate(0);
        let batch = HydrationBatchOutcomes::new(
            1,
            vec![HydrationTerminalOutcome::ParentHydrationTimeout(rejected(
                only,
                HydrationRejectionReason::ParentHydrationTimeout,
            ))],
        );
        let normalized = normalize_hydration_outcomes("RetrieveContext", batch);
        assert!(normalized.has_total_timeout());
        let status = total_hydration_timeout_status(&normalized.to_proto());
        assert_eq!(status.code(), Code::DeadlineExceeded);
        assert!(!status.details().is_empty());
    }

    #[test]
    fn parent_scoped_rejection_suppresses_hydrated_sibling_candidate() {
        let parent = Uuid::new_v4();
        let mut first = candidate(0);
        first.parent_chunk_id = parent;
        let mut second = candidate(1);
        second.access_zone_id = first.access_zone_id;
        second.parent_chunk_id = parent;
        let batch = HydrationBatchOutcomes::new(
            2,
            vec![
                HydrationTerminalOutcome::Hydrated {
                    candidate: first.clone(),
                    context: Box::new(context(&first)),
                    elapsed: Duration::from_millis(1),
                },
                HydrationTerminalOutcome::HydrationMissing(rejected(
                    second,
                    HydrationRejectionReason::HydrationMissing,
                )),
            ],
        );

        let normalized = normalize_hydration_outcomes("Search", batch);

        assert!(normalized.surviving_contexts.is_empty());
        assert_eq!(normalized.dropped_parents.len(), 1);
        assert!(normalized
            .rejected_parent_keys
            .contains(&(first.access_zone_id, parent)));
    }

    #[test]
    fn candidate_scoped_rejection_does_not_suppress_healthy_sibling() {
        let parent = Uuid::new_v4();
        let mut invalid = candidate(0);
        invalid.parent_chunk_id = parent;
        let mut healthy = candidate(1);
        healthy.access_zone_id = invalid.access_zone_id;
        healthy.parent_chunk_id = parent;
        let batch = HydrationBatchOutcomes::new(
            2,
            vec![
                HydrationTerminalOutcome::BindingInvalid(rejected(
                    invalid,
                    HydrationRejectionReason::BindingInvalid,
                )),
                HydrationTerminalOutcome::Hydrated {
                    candidate: healthy.clone(),
                    context: Box::new(context(&healthy)),
                    elapsed: Duration::from_millis(1),
                },
            ],
        );

        let normalized = normalize_hydration_outcomes("Search", batch);

        assert_eq!(normalized.surviving_contexts.len(), 1);
        assert!(normalized.rejected_parent_keys.is_empty());
    }

    #[test]
    #[should_panic(expected = "requested_candidates == hydrated_outcomes + rejected_outcomes")]
    fn missing_terminal_outcome_is_rejected() {
        HydrationBatchOutcomes::new(1, Vec::new());
    }

    #[test]
    fn rejection_reserve_is_bounded_by_both_caps() {
        assert_eq!(bounded_hydration_fetch_window(5, 5, 4, 16, 100), 9);
        assert_eq!(bounded_hydration_fetch_window(5, 5, 40, 16, 100), 21);
        assert_eq!(bounded_hydration_fetch_window(5, 5, 40, 16, 12), 12);
    }
}
