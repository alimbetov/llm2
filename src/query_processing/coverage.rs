use crate::query_processing::planner::QuerySegment;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryEvidenceStatus {
    Found,
    Degraded,
    Insufficient,
}

#[derive(Debug, Clone)]
pub struct QueryCoverage {
    pub required_total: usize,
    pub required_covered: usize,
    pub ratio: f32,
    pub status: QueryEvidenceStatus,
    pub uncovered_required_segment_indices: Vec<usize>,
}

pub fn evaluate_required_coverage(
    segments: &[QuerySegment],
    covered_segment_indices: &std::collections::HashSet<usize>,
) -> QueryCoverage {
    let required = segments
        .iter()
        .filter(|segment| segment.required_for_coverage)
        .map(|segment| segment.index)
        .collect::<Vec<_>>();
    let required_total = required.len();
    let required_covered = required
        .iter()
        .filter(|idx| covered_segment_indices.contains(idx))
        .count();
    let uncovered_required_segment_indices = required
        .iter()
        .filter(|idx| !covered_segment_indices.contains(idx))
        .copied()
        .collect();
    let ratio = if required_total == 0 {
        0.0
    } else {
        required_covered as f32 / required_total as f32
    };
    let status = match (required_total, required_covered) {
        (0, _) | (_, 0) => QueryEvidenceStatus::Insufficient,
        (total, covered) if covered == total => QueryEvidenceStatus::Found,
        _ => QueryEvidenceStatus::Degraded,
    };
    QueryCoverage {
        required_total,
        required_covered,
        ratio,
        status,
        uncovered_required_segment_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::QueryProcessingConfig;
    use crate::query_processing::planner::build_segment;

    fn segments() -> Vec<QuerySegment> {
        let cfg = QueryProcessingConfig::default();
        vec![
            build_segment(0, "context only", 2, &cfg, false),
            build_segment(1, "What happens?", 2, &cfg, true),
            build_segment(2, "Which ORA-00904 fixture?", 2, &cfg, true),
        ]
    }

    #[test]
    fn zero_required_coverage_is_insufficient() {
        let coverage = evaluate_required_coverage(&segments(), &Default::default());
        assert_eq!(coverage.status, QueryEvidenceStatus::Insufficient);
    }

    #[test]
    fn partial_required_coverage_is_degraded() {
        let coverage = evaluate_required_coverage(&segments(), &[1].into_iter().collect());
        assert_eq!(coverage.status, QueryEvidenceStatus::Degraded);
        assert_eq!(coverage.uncovered_required_segment_indices, vec![2]);
    }

    #[test]
    fn full_required_coverage_is_found() {
        let coverage = evaluate_required_coverage(&segments(), &[1, 2].into_iter().collect());
        assert_eq!(coverage.status, QueryEvidenceStatus::Found);
        assert_eq!(coverage.ratio, 1.0);
    }
}
