pub mod classification;
pub mod coverage;
pub mod diagnostics;
pub mod evidence;
pub mod fusion;
pub mod intent;
pub mod normalization;
pub mod planner;
pub mod profile;
pub mod segmenter;
pub mod status;

pub use classification::{
    classify_query_segment, has_question_form, has_technical_identifier, QuerySegmentKind,
};
pub use evidence::{CandidateIntentEvidence, CandidateIntentEvidenceReason};
pub use intent::{
    extract_query_intents, extract_query_intents_normalized, QueryIntentKind, QueryIntentUnit,
};
pub use normalization::{normalize_query, NormalizedQuery};
pub use planner::{
    build_query_plan, QueryPlan, QueryPlanningError, QueryProcessingMode, QuerySegment,
    QueryTokenCounter,
};
pub use profile::{EffectiveQueryProcessingLimits, QueryProcessingTier};
pub use segmenter::segment_query;
pub use status::{
    no_answer_is_eligible, summarize_retrieval_statuses, RetrievalBranchStatus,
    SegmentRetrievalStatus,
};
