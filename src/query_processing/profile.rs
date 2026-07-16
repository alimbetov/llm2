use crate::config::{QueryProcessingConfig, QueryProcessingTierConfig};

pub const QUERY_PROCESSING_PROFILE_VERSION: &str = "tiered-query-v1";
pub const HARD_MAX_QUERY_TOKENS: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryProcessingTier {
    Single,
    SegmentedStandard,
    SegmentedExtended,
}

impl QueryProcessingTier {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Single => "SINGLE",
            Self::SegmentedStandard => "SEGMENTED_STANDARD",
            Self::SegmentedExtended => "SEGMENTED_EXTENDED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveQueryProcessingLimits {
    pub max_query_tokens: usize,
    pub max_segments: usize,
    pub segment_target_tokens: usize,
    pub segment_max_tokens: usize,
    pub segment_overlap_tokens: usize,
    pub dense_candidate_limit: u32,
    pub sparse_candidate_limit: u32,
    pub lexical_candidate_limit: u32,
    pub local_fused_candidate_limit: u32,
    pub global_fused_candidate_limit: u32,
    pub max_parallel_segments: usize,
    pub max_parallel_lexical_segments: usize,
    pub deadline_ms: u64,
    pub max_graph_seeds: usize,
    pub admission_weight: u32,
}

impl EffectiveQueryProcessingLimits {
    pub fn for_single(config: &QueryProcessingConfig, single_max_tokens: usize) -> Self {
        Self {
            max_query_tokens: single_max_tokens,
            max_segments: 1,
            segment_target_tokens: single_max_tokens,
            segment_max_tokens: single_max_tokens,
            segment_overlap_tokens: 0,
            dense_candidate_limit: config.standard.dense_candidate_limit,
            sparse_candidate_limit: config.standard.sparse_candidate_limit,
            lexical_candidate_limit: config.standard.lexical_candidate_limit,
            local_fused_candidate_limit: config.standard.local_fused_candidate_limit,
            global_fused_candidate_limit: config.standard.global_fused_candidate_limit,
            max_parallel_segments: 1,
            max_parallel_lexical_segments: 1,
            deadline_ms: config.single_deadline_ms,
            max_graph_seeds: config.single_graph_seeds,
            admission_weight: config.single_admission_weight,
        }
    }

    pub fn for_segmented(
        config: &QueryProcessingConfig,
        profile: &QueryProcessingTierConfig,
    ) -> Self {
        Self {
            max_query_tokens: profile.max_tokens,
            max_segments: profile.max_segments,
            segment_target_tokens: config.segment_target_tokens,
            segment_max_tokens: config.segment_max_tokens,
            segment_overlap_tokens: config.segment_overlap_tokens,
            dense_candidate_limit: profile.dense_candidate_limit,
            sparse_candidate_limit: profile.sparse_candidate_limit,
            lexical_candidate_limit: profile.lexical_candidate_limit,
            local_fused_candidate_limit: profile.local_fused_candidate_limit,
            global_fused_candidate_limit: profile.global_fused_candidate_limit,
            max_parallel_segments: profile.max_parallel_segments,
            max_parallel_lexical_segments: profile.max_parallel_lexical_segments,
            deadline_ms: profile.deadline_ms,
            max_graph_seeds: profile.max_graph_seeds,
            admission_weight: profile.admission_weight,
        }
    }
}
