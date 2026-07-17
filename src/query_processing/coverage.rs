use crate::query_processing::intent::QueryIntentUnit;
use crate::query_processing::planner::QuerySegment;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryEvidenceStatus {
    Found,
    Degraded,
    Insufficient,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct QueryCoverage {
    pub required_total: usize,
    pub required_covered: usize,
    pub ratio: f32,
    pub status: QueryEvidenceStatus,
    pub uncovered_required_segment_indices: Vec<usize>,
    pub uncovered_required_intent_ids: Vec<usize>,
}

pub fn evaluate_intent_coverage(
    intents: &[QueryIntentUnit],
    covered_intent_ids: &HashSet<usize>,
) -> QueryCoverage {
    let required = intents
        .iter()
        .filter(|intent| intent.required)
        .collect::<Vec<_>>();
    let total_weight = required.iter().map(|intent| intent.weight).sum::<f32>();
    let covered_weight = required
        .iter()
        .filter(|intent| covered_intent_ids.contains(&intent.id))
        .map(|intent| intent.weight)
        .sum::<f32>();
    let required_total = required.len();
    let required_covered = required
        .iter()
        .filter(|intent| covered_intent_ids.contains(&intent.id))
        .count();
    let uncovered_required_intent_ids = required
        .iter()
        .filter(|intent| !covered_intent_ids.contains(&intent.id))
        .map(|intent| intent.id)
        .collect::<Vec<_>>();
    let ratio = if total_weight <= f32::EPSILON {
        0.0
    } else {
        covered_weight / total_weight
    };
    QueryCoverage {
        required_total,
        required_covered,
        ratio,
        status: evidence_status(required_total, required_covered),
        uncovered_required_segment_indices: Vec::new(),
        uncovered_required_intent_ids,
    }
}

/// v008 compatibility adapter. Required markers represent one physical segment
/// for each required logical intent, rather than every technical segment.
pub fn evaluate_required_coverage(
    segments: &[QuerySegment],
    covered_segment_indices: &HashSet<usize>,
) -> QueryCoverage {
    let required = segments
        .iter()
        .filter(|segment| segment.required_for_coverage)
        .map(|segment| segment.index)
        .collect::<Vec<_>>();
    let required_total = required.len();
    let required_covered = required
        .iter()
        .filter(|index| covered_segment_indices.contains(index))
        .count();
    let uncovered_required_segment_indices = required
        .iter()
        .filter(|index| !covered_segment_indices.contains(index))
        .copied()
        .collect::<Vec<_>>();
    let ratio = if required_total == 0 {
        0.0
    } else {
        required_covered as f32 / required_total as f32
    };
    QueryCoverage {
        required_total,
        required_covered,
        ratio,
        status: evidence_status(required_total, required_covered),
        uncovered_required_segment_indices,
        uncovered_required_intent_ids: Vec::new(),
    }
}

fn evidence_status(required_total: usize, required_covered: usize) -> QueryEvidenceStatus {
    match (required_total, required_covered) {
        (0, _) | (_, 0) => QueryEvidenceStatus::Insufficient,
        (total, covered) if total == covered => QueryEvidenceStatus::Found,
        _ => QueryEvidenceStatus::Degraded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_processing::{QueryIntentKind, QueryIntentUnit};

    fn intent(id: usize, required: bool, weight: f32) -> QueryIntentUnit {
        QueryIntentUnit {
            id,
            kind: QueryIntentKind::ExplicitQuestion,
            text: format!("intent-{id}"),
            source_segment_indices: vec![id],
            source_token_start: id,
            source_token_end: id + 1,
            normalized_byte_start: id,
            normalized_byte_end: id + 1,
            original_byte_start: id,
            original_byte_end: id + 1,
            required,
            searchable: true,
            weight,
            normalized_sha256: String::new(),
        }
    }

    #[test]
    fn weighted_partial_intent_coverage_is_degraded() {
        let coverage = evaluate_intent_coverage(
            &[intent(0, true, 1.0), intent(1, true, 1.0)],
            &[0].into_iter().collect(),
        );
        assert_eq!(coverage.status, QueryEvidenceStatus::Degraded);
        assert_eq!(coverage.ratio, 0.5);
        assert_eq!(coverage.uncovered_required_intent_ids, vec![1]);
    }
}
