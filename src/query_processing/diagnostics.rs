use crate::query_processing::planner::{QueryPlan, QueryProcessingMode};
use crate::query_processing::profile::QueryProcessingTier;

#[derive(Debug, Clone)]
pub struct QueryPlanDiagnostics {
    pub mode: QueryProcessingMode,
    pub tier: QueryProcessingTier,
    pub profile_version: String,
    pub original_token_count: usize,
    pub segment_count: usize,
    pub intent_count: usize,
    pub query_was_truncated: bool,
    pub segment_sha256: Vec<String>,
}

impl QueryPlanDiagnostics {
    pub fn from_plan(plan: &QueryPlan) -> Self {
        Self {
            mode: plan.mode,
            tier: plan.tier,
            profile_version: plan.profile_version.clone(),
            original_token_count: plan.original_token_count,
            segment_count: plan.segments.len(),
            intent_count: plan.intent_units.len(),
            query_was_truncated: false,
            segment_sha256: plan
                .segments
                .iter()
                .map(|segment| segment.sha256.clone())
                .collect(),
        }
    }

    pub fn mode_code(&self) -> &'static str {
        match self.mode {
            QueryProcessingMode::Single => "SINGLE",
            QueryProcessingMode::Segmented => "SEGMENTED",
        }
    }

    pub fn tier_code(&self) -> &'static str {
        self.tier.code()
    }
}
