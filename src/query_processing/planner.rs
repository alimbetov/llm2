use crate::config::QueryProcessingConfig;
use crate::query_processing::classification::{
    classify_query_segment, has_question_form, has_technical_identifier, QuerySegmentKind,
};
use crate::query_processing::segmenter::segment_query;
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
    pub kind: QuerySegmentKind,
    pub has_question_form: bool,
    pub has_technical_identifier: bool,
    pub weight: f32,
    pub required_for_coverage: bool,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct QueryPlan {
    pub original_query: String,
    pub original_token_count: usize,
    pub mode: QueryProcessingMode,
    pub segments: Vec<QuerySegment>,
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
    #[error("QUERY_SEGMENTATION_INVARIANT_FAILED: {0}")]
    SegmentationInvariant(String),
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
    let original_token_count = token_counter
        .count_tokens(trimmed, config.absolute_max_tokens, false)
        .map_err(map_tokenization_error)?;
    if original_token_count <= single_query_max_tokens {
        return Ok(QueryPlan {
            original_query: trimmed.to_owned(),
            original_token_count,
            mode: QueryProcessingMode::Single,
            segments: vec![build_segment(
                0,
                trimmed,
                original_token_count,
                config,
                true,
            )],
        });
    }
    if !config.enabled {
        return Err(QueryPlanningError::LongQueryNotSupported);
    }
    if original_token_count > config.absolute_max_tokens {
        return Err(QueryPlanningError::TokenLimitExceeded);
    }
    let segments = segment_query(trimmed, token_counter, config)?;
    validate_plan_segments(&segments, original_token_count, config)?;
    Ok(QueryPlan {
        original_query: trimmed.to_owned(),
        original_token_count,
        mode: QueryProcessingMode::Segmented,
        segments,
    })
}

pub(crate) fn build_segment(
    index: usize,
    text: &str,
    token_count: usize,
    config: &QueryProcessingConfig,
    required_for_coverage: bool,
) -> QuerySegment {
    let kind = classify_query_segment(text);
    let has_question = has_question_form(text);
    let has_technical = has_technical_identifier(text);
    let mut weight = match kind {
        QuerySegmentKind::Question => config.question_segment_weight,
        QuerySegmentKind::Technical => config.technical_segment_weight,
        QuerySegmentKind::Context => config.context_segment_weight,
    };
    if has_question && has_technical {
        weight = weight
            .max(config.question_segment_weight)
            .max(config.technical_segment_weight);
    }
    QuerySegment {
        index,
        text: text.to_owned(),
        token_count,
        kind,
        has_question_form: has_question,
        has_technical_identifier: has_technical,
        weight,
        required_for_coverage,
        sha256: hex::encode(Sha256::digest(text.as_bytes())),
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
    config: &QueryProcessingConfig,
) -> Result<(), QueryPlanningError> {
    if segments.is_empty() {
        return Err(QueryPlanningError::SegmentationInvariant(
            "segmented query produced no segments".into(),
        ));
    }
    if segments.len() > config.max_segments {
        return Err(QueryPlanningError::SegmentationInvariant(format!(
            "segment_count={} exceeds max_segments={}",
            segments.len(),
            config.max_segments
        )));
    }
    if !segments.iter().any(|segment| segment.required_for_coverage) {
        return Err(QueryPlanningError::SegmentationInvariant(
            "no required coverage segments".into(),
        ));
    }
    for (expected, segment) in segments.iter().enumerate() {
        if segment.index != expected {
            return Err(QueryPlanningError::SegmentationInvariant(
                "segment indexes are not contiguous".into(),
            ));
        }
        if segment.text.trim().is_empty() {
            return Err(QueryPlanningError::SegmentationInvariant(
                "empty segment".into(),
            ));
        }
        if segment.token_count == 0 {
            return Err(QueryPlanningError::SegmentationInvariant(
                "zero-token segment".into(),
            ));
        }
        if segment.token_count > config.segment_max_tokens {
            return Err(QueryPlanningError::SegmentationInvariant(format!(
                "segment {} has {} tokens, max {}",
                segment.index, segment.token_count, config.segment_max_tokens
            )));
        }
    }
    let segment_token_sum = segments
        .iter()
        .map(|segment| segment.token_count)
        .sum::<usize>();
    if segment_token_sum < original_token_count / 2 {
        return Err(QueryPlanningError::SegmentationInvariant(
            "segments cover too little of original query".into(),
        ));
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

    fn config() -> QueryProcessingConfig {
        QueryProcessingConfig {
            segment_target_tokens: 10,
            segment_max_tokens: 12,
            segment_overlap_tokens: 2,
            max_segments: 6,
            ..Default::default()
        }
    }

    #[test]
    fn query_at_single_limit_uses_single() {
        let query = (0..8)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let plan = build_query_plan(&query, &WhitespaceCounter, &config(), 8).unwrap();
        assert_eq!(plan.mode, QueryProcessingMode::Single);
        assert_eq!(plan.segments.len(), 1);
    }

    #[test]
    fn query_above_single_limit_uses_segmented() {
        let query = (0..20)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let plan = build_query_plan(&query, &WhitespaceCounter, &config(), 8).unwrap();
        assert_eq!(plan.mode, QueryProcessingMode::Segmented);
        assert!(plan.segments.len() >= 2);
    }

    #[test]
    fn empty_query_is_rejected() {
        assert!(matches!(
            build_query_plan(" ", &WhitespaceCounter, &config(), 8),
            Err(QueryPlanningError::Empty)
        ));
    }

    #[test]
    fn byte_limit_is_checked_before_tokenizer() {
        let cfg = QueryProcessingConfig {
            absolute_max_bytes: 4,
            ..config()
        };
        assert!(matches!(
            build_query_plan("abcdef", &WhitespaceCounter, &cfg, 8),
            Err(QueryPlanningError::ByteLimitExceeded)
        ));
    }

    #[test]
    fn query_above_absolute_limit_is_rejected() {
        let query = (0..1100)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(matches!(
            build_query_plan(&query, &WhitespaceCounter, &config(), 8),
            Err(QueryPlanningError::TokenLimitExceeded)
        ));
    }

    #[test]
    fn disabled_processing_rejects_long_query_without_truncating() {
        let query = (0..20)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let cfg = QueryProcessingConfig {
            enabled: false,
            ..config()
        };
        assert!(matches!(
            build_query_plan(&query, &WhitespaceCounter, &cfg, 8),
            Err(QueryPlanningError::LongQueryNotSupported)
        ));
    }

    #[test]
    fn single_plan_contains_one_segment() {
        let plan = build_query_plan(
            "How does legal hold work?",
            &WhitespaceCounter,
            &config(),
            8,
        )
        .unwrap();
        assert_eq!(plan.segments.len(), 1);
        assert_eq!(plan.segments[0].weight, 1.0);
        assert!(plan.segments[0].required_for_coverage);
    }
}
