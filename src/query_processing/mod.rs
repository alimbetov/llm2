pub mod classification;
pub mod coverage;
pub mod diagnostics;
pub mod fusion;
pub mod intent;
pub mod planner;
pub mod profile;
pub mod segmenter;

pub use classification::{
    classify_query_segment, has_question_form, has_technical_identifier, QuerySegmentKind,
};
pub use intent::{extract_query_intents, QueryIntentKind, QueryIntentUnit};
pub use planner::{
    build_query_plan, QueryPlan, QueryPlanningError, QueryProcessingMode, QuerySegment,
    QueryTokenCounter,
};
pub use profile::{EffectiveQueryProcessingLimits, QueryProcessingTier};
pub use segmenter::segment_query;
