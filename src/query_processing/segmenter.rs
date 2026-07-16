use crate::query_processing::planner::{
    build_segment, QueryPlanningError, QuerySegment, QueryTokenCounter,
};
use crate::query_processing::profile::EffectiveQueryProcessingLimits;

pub fn segment_query(
    query: &str,
    token_counter: &dyn QueryTokenCounter,
    limits: &EffectiveQueryProcessingLimits,
    question_weight: f32,
    technical_weight: f32,
    context_weight: f32,
) -> Result<Vec<QuerySegment>, QueryPlanningError> {
    let normalized = normalize_query_text(query);
    let offsets = token_counter
        .token_offsets(&normalized)
        .map_err(QueryPlanningError::Tokenization)?;
    if offsets.is_empty() {
        return Err(QueryPlanningError::SegmentationInvariant(
            "query tokenizer produced no offsets".into(),
        ));
    }

    let mut ranges = build_ranges(&normalized, &offsets, limits);
    if ranges.len() > limits.max_segments {
        ranges = compact_ranges(offsets.len(), limits)?;
    }

    let mut segments = Vec::with_capacity(ranges.len());
    for (index, (start, end)) in ranges.into_iter().enumerate() {
        if start >= end || end > offsets.len() {
            return Err(QueryPlanningError::SegmentationInvariant(
                "invalid token range produced".into(),
            ));
        }
        let byte_start = offsets[start].start_byte;
        let byte_end = offsets[end - 1].end_byte;
        let text = normalized[byte_start..byte_end].trim();
        let token_count = token_counter
            .count_tokens(text, limits.segment_max_tokens, false)
            .map_err(QueryPlanningError::Tokenization)?;
        segments.push(build_segment(
            index,
            text,
            token_count,
            start,
            end,
            byte_start,
            byte_end,
            question_weight,
            technical_weight,
            context_weight,
            false,
        ));
    }

    validate_token_coverage(&segments, offsets.len())?;
    Ok(segments)
}

fn normalize_query_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut empty_lines = 0usize;
    for line in text.trim().lines() {
        if line.trim().is_empty() {
            empty_lines += 1;
            if empty_lines <= 2 {
                out.push('\n');
            }
        } else {
            empty_lines = 0;
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.trim().to_owned()
}

fn build_ranges(
    text: &str,
    offsets: &[crate::tokenizer::TokenOffset],
    limits: &EffectiveQueryProcessingLimits,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0usize;
    while start < offsets.len() {
        let remaining = offsets.len() - start;
        let end = if remaining <= limits.segment_max_tokens {
            offsets.len()
        } else {
            choose_soft_boundary(text, offsets, start, limits)
        };
        ranges.push((start, end));
        if end == offsets.len() {
            break;
        }
        let next = end.saturating_sub(limits.segment_overlap_tokens);
        start = next.max(start + 1);
    }
    ranges
}

fn choose_soft_boundary(
    text: &str,
    offsets: &[crate::tokenizer::TokenOffset],
    start: usize,
    limits: &EffectiveQueryProcessingLimits,
) -> usize {
    let target = (start + limits.segment_target_tokens).min(offsets.len());
    let hard = (start + limits.segment_max_tokens).min(offsets.len());
    let minimum = (start + limits.segment_target_tokens.saturating_sub(24)).min(hard);
    let mut best = target.max(start + 1);
    for end in minimum.max(start + 1)..=hard {
        let left = offsets[end - 1].end_byte;
        let right = offsets
            .get(end)
            .map(|offset| offset.start_byte)
            .unwrap_or(text.len());
        let separator = &text[left..right];
        if separator.contains("\n\n") {
            best = end;
        } else if separator.contains('\n')
            || separator.contains(". ")
            || separator.contains("? ")
            || separator.contains("! ")
        {
            if end >= target {
                return end;
            }
            best = end;
        }
    }
    best.min(hard).max(start + 1)
}

fn compact_ranges(
    token_count: usize,
    limits: &EffectiveQueryProcessingLimits,
) -> Result<Vec<(usize, usize)>, QueryPlanningError> {
    let window = limits.segment_max_tokens.max(1);
    let overlap = limits
        .segment_overlap_tokens
        .min(window.saturating_sub(1));
    let capacity = limits
        .max_segments
        .saturating_mul(window)
        .saturating_sub(
            limits
                .max_segments
                .saturating_sub(1)
                .saturating_mul(overlap),
        );
    if token_count > capacity {
        return Err(QueryPlanningError::SegmentationInvariant(format!(
            "query requires more than {} segments at max_tokens={} overlap={}",
            limits.max_segments, window, overlap
        )));
    }
    let mut ranges = Vec::new();
    let mut start = 0usize;
    while start < token_count {
        let end = (start + window).min(token_count);
        ranges.push((start, end));
        if end == token_count {
            break;
        }
        start = end.saturating_sub(overlap).max(start + 1);
    }
    if ranges.len() > limits.max_segments {
        return Err(QueryPlanningError::SegmentationInvariant(
            "compaction exceeded max_segments".into(),
        ));
    }
    Ok(ranges)
}

fn validate_token_coverage(
    segments: &[QuerySegment],
    original_token_count: usize,
) -> Result<(), QueryPlanningError> {
    let mut covered = vec![false; original_token_count];
    for segment in segments {
        for index in segment.source_token_start..segment.source_token_end.min(original_token_count) {
            covered[index] = true;
        }
    }
    if let Some(missing) = covered.iter().position(|covered| !covered) {
        return Err(QueryPlanningError::SegmentationInvariant(format!(
            "original token {missing} is not covered by any segment"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::TokenOffset;

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

        fn token_offsets(&self, text: &str) -> Result<Vec<TokenOffset>, String> {
            let mut offsets = Vec::new();
            let mut from = 0usize;
            for (index, token) in text.split_whitespace().enumerate() {
                let relative = text[from..].find(token).ok_or("token missing")?;
                let start = from + relative;
                let end = start + token.len();
                offsets.push(TokenOffset {
                    token_index: index,
                    start_byte: start,
                    end_byte: end,
                });
                from = end;
            }
            Ok(offsets)
        }
    }

    fn limits(max_tokens: usize, max_segments: usize) -> EffectiveQueryProcessingLimits {
        EffectiveQueryProcessingLimits {
            max_query_tokens: 2_048,
            max_segments,
            segment_target_tokens: 180.min(max_tokens),
            segment_max_tokens: max_tokens,
            segment_overlap_tokens: 24.min(max_tokens.saturating_sub(1)),
            dense_candidate_limit: 10,
            sparse_candidate_limit: 10,
            lexical_candidate_limit: 8,
            local_fused_candidate_limit: 10,
            global_fused_candidate_limit: 140,
            max_parallel_segments: 3,
            max_parallel_lexical_segments: 2,
            deadline_ms: 6_000,
            max_graph_seeds: 10,
            admission_weight: 6,
        }
    }

    fn words(count: usize) -> String {
        (0..count)
            .map(|index| format!("token{index}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn extended_2048_fits_in_14_segments_without_gaps() {
        let segments = segment_query(
            &words(2_048),
            &WhitespaceCounter,
            &limits(220, 14),
            1.0,
            1.0,
            0.5,
        )
        .unwrap();
        assert!(segments.len() <= 14);
        assert_eq!(segments.first().unwrap().source_token_start, 0);
        assert_eq!(segments.last().unwrap().source_token_end, 2_048);
        assert!(segments.iter().all(|segment| segment.token_count <= 220));
    }
}
