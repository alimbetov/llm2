use crate::query_processing::planner::{QueryPlan, QueryProcessingMode};

#[derive(Debug, Clone)]
pub struct QueryPlanDiagnostics {
    pub mode: QueryProcessingMode,
    pub original_token_count: usize,
    pub segment_count: usize,
    pub query_was_truncated: bool,
    pub segment_sha256: Vec<String>,
}

impl QueryPlanDiagnostics {
    pub fn from_plan(plan: &QueryPlan) -> Self {
        Self {
            mode: plan.mode,
            original_token_count: plan.original_token_count,
            segment_count: plan.segments.len(),
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
}
