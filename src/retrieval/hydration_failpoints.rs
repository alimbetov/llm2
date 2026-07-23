use crate::{
    error::AstraError,
    retrieval::hydration::{
        HydrationBatchOutcomes, HydrationRejectionReason, HydrationTerminalOutcome,
        RejectedHydrationCandidate,
    },
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HydrationFailpointMode {
    None,
    ReturnNotFoundSelected,
    TimeoutSelectedParents,
    TimeoutAllParents,
    EmptyContentSelected,
}

impl HydrationFailpointMode {
    pub const NONE: Self = Self::None;
    pub const RETURN_NOT_FOUND_SELECTED: Self = Self::ReturnNotFoundSelected;
    pub const TIMEOUT_SELECTED_PARENTS: Self = Self::TimeoutSelectedParents;
    pub const TIMEOUT_ALL_PARENTS: Self = Self::TimeoutAllParents;
    pub const EMPTY_CONTENT_SELECTED: Self = Self::EmptyContentSelected;
}

#[derive(Debug, Deserialize)]
struct HydrationFailpointRuleConfig {
    run_id: String,
    #[serde(alias = "correlation_id")]
    request_id: String,
    entry_point: String,
    access_zone_code: String,
    logical_parent_ids: Vec<String>,
    #[serde(alias = "parent_ids")]
    physical_parent_ids: Vec<String>,
    mode: HydrationFailpointMode,
    max_activations: usize,
    #[serde(default)]
    hydration_deadline_ms: u64,
    #[serde(default)]
    delay_margin_ms: u64,
}

#[derive(Debug)]
struct HydrationFailpointRule {
    config: HydrationFailpointRuleConfig,
    activations: AtomicUsize,
}

#[derive(Debug, Default)]
pub struct HydrationFailpointPlan {
    pub non_production_enabled: bool,
    run_id: String,
    rules: Vec<Arc<HydrationFailpointRule>>,
}

impl HydrationFailpointPlan {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn load(
        non_production_enabled: bool,
        path: &str,
        run_id: &str,
    ) -> Result<Self, AstraError> {
        if !non_production_enabled {
            if !path.trim().is_empty() || !run_id.trim().is_empty() {
                return Err(AstraError::FailedPrecondition(
                    "hydration failpoint plan requires explicit non-production enable flag".into(),
                ));
            }
            return Ok(Self::disabled());
        }
        if path.trim().is_empty() || run_id.trim().is_empty() {
            return Err(AstraError::FailedPrecondition(
                "enabled hydration failpoints require plan_path and run_id".into(),
            ));
        }
        let bytes = fs::read(Path::new(path)).map_err(|error| {
            AstraError::FailedPrecondition(format!(
                "cannot read hydration failpoint plan {path}: {error}"
            ))
        })?;
        let configs: Vec<HydrationFailpointRuleConfig> =
            serde_json::from_slice(&bytes).map_err(|error| {
                AstraError::FailedPrecondition(format!(
                    "invalid hydration failpoint plan {path}: {error}"
                ))
            })?;
        let mut rules = Vec::with_capacity(configs.len());
        for config in configs {
            if config.run_id != run_id
                || config.request_id.trim().is_empty()
                || config.entry_point.trim().is_empty()
                || config.access_zone_code.trim().is_empty()
                || config.physical_parent_ids.is_empty()
                || config.logical_parent_ids.len() != config.physical_parent_ids.len()
                || config.max_activations == 0
            {
                return Err(AstraError::FailedPrecondition(
                    "hydration failpoint rules require matching run_id, request_id, entry_point, access_zone_code, paired logical/physical parent IDs and max_activations > 0"
                        .into(),
                ));
            }
            if matches!(
                config.mode,
                HydrationFailpointMode::TimeoutSelectedParents
                    | HydrationFailpointMode::TimeoutAllParents
            ) && (config.hydration_deadline_ms == 0
                || config.delay_margin_ms > 60_000
                || config
                    .hydration_deadline_ms
                    .saturating_add(config.delay_margin_ms)
                    > 600_000)
            {
                return Err(AstraError::FailedPrecondition(
                    "timeout failpoints require a positive deadline and a bounded total delay <= 600000ms"
                        .into(),
                ));
            }
            rules.push(Arc::new(HydrationFailpointRule {
                config,
                activations: AtomicUsize::new(0),
            }));
        }
        Ok(Self {
            non_production_enabled,
            run_id: run_id.to_owned(),
            rules,
        })
    }

    pub async fn apply(
        &self,
        correlation_id: &str,
        entry_point: &str,
        access_zone_codes: &HashMap<Uuid, String>,
        outcomes: HydrationBatchOutcomes,
    ) -> HydrationBatchOutcomes {
        if !self.non_production_enabled {
            return outcomes;
        }
        let Some(rule) = self.rules.iter().find(|rule| {
            rule.config.run_id == self.run_id
                && rule.config.request_id == correlation_id
                && rule.config.entry_point.eq_ignore_ascii_case(entry_point)
                && outcomes.outcomes.iter().any(|outcome| {
                    if !matches!(outcome, HydrationTerminalOutcome::Hydrated { .. }) {
                        return false;
                    }
                    let candidate = outcome.candidate();
                    access_zone_codes
                        .get(&candidate.access_zone_id)
                        .is_some_and(|zone| zone == &rule.config.access_zone_code)
                        && (rule.config.mode == HydrationFailpointMode::TimeoutAllParents
                            || rule
                                .config
                                .physical_parent_ids
                                .iter()
                                .any(|parent| parent == &candidate.parent_chunk_id.to_string()))
                })
                && reserve_activation(rule)
        }) else {
            return outcomes;
        };
        if rule.config.mode == HydrationFailpointMode::None {
            return outcomes;
        }

        let target_all = rule.config.mode == HydrationFailpointMode::TimeoutAllParents;
        let fault_started = Instant::now();
        let failpoint_delay_ms = if matches!(
            rule.config.mode,
            HydrationFailpointMode::TimeoutSelectedParents
                | HydrationFailpointMode::TimeoutAllParents
        ) {
            rule.config
                .hydration_deadline_ms
                .saturating_add(rule.config.delay_margin_ms)
        } else {
            0
        };
        if failpoint_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(failpoint_delay_ms)).await;
        }

        let mut changed = Vec::with_capacity(outcomes.outcomes.len());
        for outcome in outcomes.outcomes {
            if !matches!(outcome, HydrationTerminalOutcome::Hydrated { .. }) {
                changed.push(outcome);
                continue;
            }
            let candidate = outcome.candidate().clone();
            let zone_matches = access_zone_codes
                .get(&candidate.access_zone_id)
                .is_some_and(|zone| zone == &rule.config.access_zone_code);
            let parent_matches = target_all
                || rule
                    .config
                    .physical_parent_ids
                    .iter()
                    .any(|parent| parent == &candidate.parent_chunk_id.to_string());
            if !zone_matches || !parent_matches {
                changed.push(outcome);
                continue;
            }
            let reason = match rule.config.mode {
                HydrationFailpointMode::None => unreachable!(),
                HydrationFailpointMode::ReturnNotFoundSelected => {
                    HydrationRejectionReason::HydrationMissing
                }
                HydrationFailpointMode::TimeoutSelectedParents
                | HydrationFailpointMode::TimeoutAllParents => {
                    HydrationRejectionReason::ParentHydrationTimeout
                }
                HydrationFailpointMode::EmptyContentSelected => {
                    HydrationRejectionReason::EmptyContext
                }
            };
            let rejected = RejectedHydrationCandidate {
                candidate,
                reason,
                stage: "CANONICAL_PARENT_HYDRATION",
                elapsed: fault_started.elapsed(),
            };
            changed.push(match reason {
                HydrationRejectionReason::HydrationMissing => {
                    HydrationTerminalOutcome::HydrationMissing(rejected)
                }
                HydrationRejectionReason::ParentHydrationTimeout => {
                    HydrationTerminalOutcome::ParentHydrationTimeout(rejected)
                }
                HydrationRejectionReason::EmptyContext => {
                    HydrationTerminalOutcome::EmptyContext(rejected)
                }
                HydrationRejectionReason::BindingInvalid
                | HydrationRejectionReason::VisibilityRejected => unreachable!(),
            });
        }
        HydrationBatchOutcomes::new(changed.len(), changed)
    }
}

fn reserve_activation(rule: &HydrationFailpointRule) -> bool {
    rule.activations
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < rule.config.max_activations).then_some(current + 1)
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        persistence::HydratedSearchContext, retrieval::hydration::HydrationCandidateIdentity,
    };
    use serde_json::json;

    fn hydrated(candidate: HydrationCandidateIdentity) -> HydrationTerminalOutcome {
        HydrationTerminalOutcome::Hydrated {
            context: Box::new(HydratedSearchContext {
                access_zone_id: candidate.access_zone_id,
                matched_chunk_id: candidate.matched_chunk_id,
                parent_chunk_id: candidate.parent_chunk_id,
                document_id: Uuid::new_v4(),
                document_version: 1,
                root_chunk_id: Uuid::new_v4(),
                source_chunk_id: Uuid::new_v4(),
                matched_text: "child".into(),
                parent_text: "parent".into(),
                parent_content_hash: "hash".into(),
                parent_token_count: 1,
                parent_sequence_no: 0,
                access_level: 1,
                source_block_id: None,
                source_location: json!({}),
                source_links: json!([]),
                metadata: json!({}),
                parent_metadata: json!({}),
            }),
            candidate,
            elapsed: Duration::from_millis(1),
        }
    }

    fn candidate(zone: Uuid, parent: Uuid, ordinal: usize) -> HydrationCandidateIdentity {
        HydrationCandidateIdentity {
            access_zone_id: zone,
            binding_id: Uuid::new_v4(),
            matched_chunk_id: Uuid::new_v4(),
            parent_chunk_id: parent,
            granularity: "SUB_180".into(),
            raw_rank: ordinal,
            input_ordinal: ordinal,
        }
    }

    #[tokio::test]
    async fn activation_is_request_scoped_selected_and_bounded() {
        let zone = Uuid::new_v4();
        let selected_parent = Uuid::new_v4();
        let healthy_parent = Uuid::new_v4();
        let rule = Arc::new(HydrationFailpointRule {
            config: HydrationFailpointRuleConfig {
                run_id: "run-1".into(),
                request_id: "request-a".into(),
                entry_point: "Search".into(),
                access_zone_code: "4862".into(),
                logical_parent_ids: vec!["logical-selected".into()],
                physical_parent_ids: vec![selected_parent.to_string()],
                mode: HydrationFailpointMode::ReturnNotFoundSelected,
                max_activations: 1,
                hydration_deadline_ms: 0,
                delay_margin_ms: 0,
            },
            activations: AtomicUsize::new(0),
        });
        let plan = HydrationFailpointPlan {
            non_production_enabled: true,
            run_id: "run-1".into(),
            rules: vec![rule.clone()],
        };
        let zones = HashMap::from([(zone, "4862".into())]);
        let batch = || {
            let selected = candidate(zone, selected_parent, 0);
            let healthy = candidate(zone, healthy_parent, 1);
            HydrationBatchOutcomes::new(2, vec![hydrated(selected), hydrated(healthy)])
        };

        let unrelated = plan.apply("request-b", "Search", &zones, batch()).await;
        assert!(unrelated
            .outcomes
            .iter()
            .all(|outcome| matches!(outcome, HydrationTerminalOutcome::Hydrated { .. })));
        assert_eq!(rule.activations.load(Ordering::Acquire), 0);

        let faulted = plan.apply("request-a", "Search", &zones, batch()).await;
        assert!(matches!(
            faulted.outcomes[0],
            HydrationTerminalOutcome::HydrationMissing(_)
        ));
        assert!(matches!(
            faulted.outcomes[1],
            HydrationTerminalOutcome::Hydrated { .. }
        ));

        let exhausted = plan.apply("request-a", "Search", &zones, batch()).await;
        assert!(exhausted
            .outcomes
            .iter()
            .all(|outcome| matches!(outcome, HydrationTerminalOutcome::Hydrated { .. })));
        assert_eq!(rule.activations.load(Ordering::Acquire), 1);
    }

    #[test]
    fn startup_plan_is_fail_closed() {
        assert!(HydrationFailpointPlan::load(false, "plan.json", "").is_err());
        assert!(HydrationFailpointPlan::load(true, "", "run-1").is_err());
    }

    #[test]
    fn startup_plan_parses_the_phase_contract() {
        let parent = Uuid::new_v4();
        let path = std::env::temp_dir().join(format!("fix486f-plan-{}.json", Uuid::new_v4()));
        let plan_json = json!([{
            "run_id": "run-1",
            "request_id": "request-a",
            "entry_point": "RetrieveContext",
            "access_zone_code": "4862",
            "logical_parent_ids": ["logical-parent"],
            "physical_parent_ids": [parent.to_string()],
            "mode": "TIMEOUT_SELECTED_PARENTS",
            "max_activations": 1,
            "hydration_deadline_ms": 10,
            "delay_margin_ms": 2
        }]);
        fs::write(&path, serde_json::to_vec(&plan_json).unwrap()).unwrap();
        let plan = HydrationFailpointPlan::load(true, path.to_str().unwrap(), "run-1").unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(plan.rule_count(), 1);
    }
}
