use crate::config::QueryProcessingConfig;
use crate::query_processing::classification::{
    classify_query_segment, has_question_form, has_technical_identifier, QuerySegmentKind,
};
use crate::query_processing::intent::{extract_query_intents, QueryIntentUnit};
use crate::query_processing::profile::{
    EffectiveQueryProcessingLimits, QueryProcessingTier, QUERY_PROCESSING_PROFILE_VERSION,
};
use crate::query_processing::segmenter::segment_query;
use crate::tokenizer::TokenOffset;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryProcessingMode {
    Single,
    Segmented,
}

#[derive(Debug, Clone)]
pub struct QuerySegment {
    pub index: usize,
    pub text: String,
    pub token_count: usize,
    pub source_token_start: usize,
    pub source_token_end: usize,
    pub source_byte_start: usize,
    pub source_byte_end: usize,
    pub kind: QuerySegmentKind,
    pub has_question_form: bool,
    pub has_technical_identifier: bool,
    pub searchable: bool,
    pub weight: f32,
    /// Compatibility marker for the v008 runtime. It represents one physical
    /// segment selected as the representative of a required logical intent.
    pub required_for_coverage: bool,
    pub intent_unit_ids: Vec<usize>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct QueryPlan {
    pub original_query: String,
    pub original_token_count: usize,
    pub mode: QueryProcessingMode,
    pub tier: QueryProcessingTier,
    pub profile_version: String,
    pub limits: EffectiveQueryProcessingLimits,
    pub segments: Vec<QuerySegment>,
    pub intent_units: Vec<QueryIntentUnit>,
}

#[derive(Debug, Error)]
pub enum QueryPlanningError {
    #[error("query must not be empty")]
    Empty,
    #[error("LONG_QUERY_BYTE_LIMIT_EXCEEDED")]
    ByteLimitExceeded,
    #[error("LONG_QUERY_TOO_LARGE")]
    TokenLimitExceeded,
    #[error("LONG_QUERY_NOT_SUPPORTED")]
    LongQueryNotSupported,
    #[error("LONG_QUERY_EXTENDED_NOT_ENABLED")]
    ExtendedQueryNotEnabled,
    #[error("QUERY_SEGMENTATION_INVARIANT_FAILED: {0}")]
    SegmentationInvariant(String),
    #[error("QUERY_INTENT_EXTRACTION_FAILED: {0}")]
    IntentExtraction(String),
    #[error("tokenization failed: {0}")]
    Tokenization(String),
}

pub trait QueryTokenCounter {
    fn count_tokens(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<usize, String>;

    fn token_offsets(&self, text: &str) -> Result<Vec<TokenOffset>, String> {
        let mut offsets = Vec::new();
        let mut search_from = 0usize;
        for (token_index, token) in text.split_whitespace().enumerate() {
            let relative = text[search_from..]
                .find(token)
                .ok_or_else(|| "token offset fallback failed".to_string())?;
            let start_byte = search_from + relative;
            let end_byte = start_byte + token.len();
            offsets.push(TokenOffset {
                token_index,
                start_byte,
                end_byte,
            });
            search_from = end_byte;
        }
        Ok(offsets)
    }
}

pub fn build_query_plan(
    query: &str,
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
    single_query_max_tokens: usize,
) -> Result<QueryPlan, QueryPlanningError> {
    if query.len() > config.absolute_max_bytes {
        return Err(QueryPlanningError::ByteLimitExceeded);
    }
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(QueryPlanningError::Empty);
    }

    let hard_max = config.extended.max_tokens;
    let original_token_count = token_counter
        .count_tokens(trimmed, hard_max, false)
        .map_err(map_tokenization_error)?;

    let (mode, tier, limits) = if original_token_count <= single_query_max_tokens {
        (
            QueryProcessingMode::Single,
            QueryProcessingTier::Single,
            EffectiveQueryProcessingLimits::for_single(config, single_query_max_tokens),
        )
    } else if original_token_count <= config.standard.max_tokens {
        if !config.enabled {
            return Err(QueryPlanningError::LongQueryNotSupported);
        }
        (
            QueryProcessingMode::Segmented,
            QueryProcessingTier::SegmentedStandard,
            EffectiveQueryProcessingLimits::for_segmented(config, &config.standard),
        )
    } else if original_token_count <= config.extended.max_tokens {
        if !config.enabled {
            return Err(QueryPlanningError::LongQueryNotSupported);
        }
        if !config.extended_enabled {
            return Err(QueryPlanningError::ExtendedQueryNotEnabled);
        }
        (
            QueryProcessingMode::Segmented,
            QueryProcessingTier::SegmentedExtended,
            EffectiveQueryProcessingLimits::for_segmented(config, &config.extended),
        )
    } else {
        return Err(QueryPlanningError::TokenLimitExceeded);
    };

    let mut segments = if mode == QueryProcessingMode::Single {
        vec![build_segment(
            0,
            trimmed,
            original_token_count,
            0,
            original_token_count,
            0,
            trimmed.len(),
            config.question_segment_weight,
            config.technical_segment_weight,
            config.context_segment_weight,
            true,
        )]
    } else {
        segment_query(
            trimmed,
            token_counter,
            &limits,
            config.question_segment_weight,
            config.technical_segment_weight,
            config.context_segment_weight,
        )?
    };

    let intent_units = extract_query_intents(trimmed, &segments);
    if intent_units.is_empty() {
        return Err(QueryPlanningError::IntentExtraction(
            "no intent units produced".into(),
        ));
    }
    bind_intents_to_segments(&mut segments, &intent_units);
    validate_plan_segments(&segments, original_token_count, &limits)?;

    Ok(QueryPlan {
        original_query: trimmed.to_owned(),
        original_token_count,
        mode,
        tier,
        profile_version: QUERY_PROCESSING_PROFILE_VERSION.to_owned(),
        limits,
        segments,
        intent_units,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_segment(
    index: usize,
    text: &str,
    token_count: usize,
    source_token_start: usize,
    source_token_end: usize,
    source_byte_start: usize,
    source_byte_end: usize,
    question_weight: f32,
    technical_weight: f32,
    context_weight: f32,
    required_for_coverage: bool,
) -> QuerySegment {
    let kind = classify_query_segment(text);
    let has_question = has_question_form(text);
    let has_technical = has_technical_identifier(text);
    let mut weight = match kind {
        QuerySegmentKind::Question => question_weight,
        QuerySegmentKind::Technical => technical_weight,
        QuerySegmentKind::Context => context_weight,
    };
    if has_question && has_technical {
        weight = weight.max(question_weight).max(technical_weight);
    }
    QuerySegment {
        index,
        text: text.to_owned(),
        token_count,
        source_token_start,
        source_token_end,
        source_byte_start,
        source_byte_end,
        kind,
        has_question_form: has_question,
        has_technical_identifier: has_technical,
        searchable: true,
        weight,
        required_for_coverage,
        intent_unit_ids: Vec::new(),
        sha256: hex::encode(Sha256::digest(text.as_bytes())),
    }
}

fn bind_intents_to_segments(segments: &mut [QuerySegment], intents: &[QueryIntentUnit]) {
    for segment in segments.iter_mut() {
        segment.required_for_coverage = false;
        segment.intent_unit_ids.clear();
    }
    for intent in intents {
        for index in &intent.source_segment_indices {
            if let Some(segment) = segments.get_mut(*index) {
                segment.intent_unit_ids.push(intent.id);
            }
        }
        if intent.required {
            if let Some(index) = intent.source_segment_indices.first() {
                if let Some(segment) = segments.get_mut(*index) {
                    segment.required_for_coverage = true;
                }
            }
        }
    }
    if !segments.iter().any(|segment| segment.required_for_coverage) {
        if let Some(first) = segments.first_mut() {
            first.required_for_coverage = true;
        }
    }
}

fn map_tokenization_error(error: String) -> QueryPlanningError {
    if error.contains("exceeds max_length") || error.contains("OutOfRange") {
        QueryPlanningError::TokenLimitExceeded
    } else {
        QueryPlanningError::Tokenization(error)
    }
}

fn validate_plan_segments(
    segments: &[QuerySegment],
    original_token_count: usize,
    limits: &EffectiveQueryProcessingLimits,
) -> Result<(), QueryPlanningError> {
    if segments.is_empty() {
        return Err(QueryPlanningError::SegmentationInvariant(
            "segmented query produced no segments".into(),
        ));
    }
    if segments.len() > limits.max_segments {
        return Err(QueryPlanningError::SegmentationInvariant(format!(
            "segment_count={} exceeds max_segments={}",
            segments.len(),
            limits.max_segments
        )));
    }
    if !segments.iter().any(|segment| segment.required_for_coverage) {
        return Err(QueryPlanningError::SegmentationInvariant(
            "no required logical intent representative".into(),
        ));
    }
    let mut covered = vec![false; original_token_count];
    for (expected, segment) in segments.iter().enumerate() {
        if segment.index != expected {
            return Err(QueryPlanningError::SegmentationInvariant(
                "segment indexes are not contiguous".into(),
            ));
        }
        if segment.text.trim().is_empty() || segment.token_count == 0 {
            return Err(QueryPlanningError::SegmentationInvariant(
                "empty or zero-token segment".into(),
            ));
        }
        if segment.token_count > limits.segment_max_tokens {
            return Err(QueryPlanningError::SegmentationInvariant(format!(
                "segment {} has {} tokens, max {}",
                segment.index, segment.token_count, limits.segment_max_tokens
            )));
        }
        for index in segment.source_token_start..segment.source_token_end.min(original_token_count)
        {
            covered[index] = true;
        }
    }
    if let Some(index) = covered.iter().position(|covered| !covered) {
        return Err(QueryPlanningError::SegmentationInvariant(format!(
            "original token {index} is not covered"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct WhitespaceCounter;

    impl QueryTokenCounter for WhitespaceCounter {
        fn count_tokens(
            &self,
            text: &str,
            max_length: usize,
            allow_truncation: bool,
        ) -> Result<usize, String> {
            let count = text.split_whitespace().count();
            if count > max_length && !allow_truncation {
                return Err(format!("{count} tokens exceeds max_length={max_length}"));
            }
            Ok(count.min(max_length))
        }
    }

    fn words(count: usize) -> String {
        (0..count)
            .map(|index| format!("token{index}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn boundary_tiers_are_selected() {
        let mut config = QueryProcessingConfig::default();
        config.extended_enabled = true;
        assert_eq!(
            build_query_plan(&words(256), &WhitespaceCounter, &config, 256)
                .unwrap()
                .tier,
            QueryProcessingTier::Single
        );
        assert_eq!(
            build_query_plan(&words(257), &WhitespaceCounter, &config, 256)
                .unwrap()
                .tier,
            QueryProcessingTier::SegmentedStandard
        );
        assert_eq!(
            build_query_plan(&words(1_024), &WhitespaceCounter, &config, 256)
                .unwrap()
                .tier,
            QueryProcessingTier::SegmentedStandard
        );
        assert_eq!(
            build_query_plan(&words(1_025), &WhitespaceCounter, &config, 256)
                .unwrap()
                .tier,
            QueryProcessingTier::SegmentedExtended
        );
        assert_eq!(
            build_query_plan(&words(2_048), &WhitespaceCounter, &config, 256)
                .unwrap()
                .tier,
            QueryProcessingTier::SegmentedExtended
        );
        assert!(matches!(
            build_query_plan(&words(2_049), &WhitespaceCounter, &config, 256),
            Err(QueryPlanningError::TokenLimitExceeded)
        ));
    }

    #[test]
    fn extended_is_fail_closed_by_default() {
        let config = QueryProcessingConfig::default();
        assert!(matches!(
            build_query_plan(&words(1_025), &WhitespaceCounter, &config, 256),
            Err(QueryPlanningError::ExtendedQueryNotEnabled)
        ));
    }
}
