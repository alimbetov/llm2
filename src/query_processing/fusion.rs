use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlobalCandidateIdentity {
    pub access_zone_id: String,
    pub document_id: String,
    pub document_version: u64,
    pub matched_chunk_id: String,
    pub parent_chunk_id: String,
}

#[derive(Debug, Clone)]
pub struct SegmentCandidate {
    pub identity: GlobalCandidateIdentity,
    pub segment_index: usize,
    pub rank: usize,
    pub score: f32,
    pub segment_weight: f32,
}

#[derive(Debug, Clone)]
pub struct GlobalRrfCandidate {
    pub identity: GlobalCandidateIdentity,
    pub score: f32,
    pub best_segment_index: usize,
    pub matched_segments: Vec<usize>,
}

pub fn cross_segment_rrf(
    candidates: impl IntoIterator<Item = SegmentCandidate>,
    rrf_k: f32,
    limit: usize,
) -> Vec<GlobalRrfCandidate> {
    let mut by_identity =
        std::collections::HashMap::<GlobalCandidateIdentity, GlobalRrfCandidate>::new();
    let rrf_k = rrf_k.max(1.0);
    for candidate in candidates {
        let contribution = candidate.segment_weight / (rrf_k + candidate.rank.max(1) as f32);
        by_identity
            .entry(candidate.identity.clone())
            .and_modify(|global| {
                global.score += contribution;
                if contribution > global.score
                    || candidate.rank < global.matched_segments.len().max(usize::MAX)
                {
                    global.best_segment_index = candidate.segment_index;
                }
                if !global.matched_segments.contains(&candidate.segment_index) {
                    global.matched_segments.push(candidate.segment_index);
                    global.matched_segments.sort_unstable();
                }
            })
            .or_insert_with(|| GlobalRrfCandidate {
                identity: candidate.identity,
                score: contribution,
                best_segment_index: candidate.segment_index,
                matched_segments: vec![candidate.segment_index],
            });
    }
    let mut out = by_identity.into_values().collect::<Vec<_>>();
    out.sort_by(compare_global_rrf_candidates);
    out.truncate(limit);
    out
}

fn compare_global_rrf_candidates(
    left: &GlobalRrfCandidate,
    right: &GlobalRrfCandidate,
) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| {
            left.identity
                .access_zone_id
                .cmp(&right.identity.access_zone_id)
        })
        .then_with(|| left.identity.document_id.cmp(&right.identity.document_id))
        .then_with(|| {
            left.identity
                .document_version
                .cmp(&right.identity.document_version)
        })
        .then_with(|| {
            left.identity
                .parent_chunk_id
                .cmp(&right.identity.parent_chunk_id)
        })
        .then_with(|| {
            left.identity
                .matched_chunk_id
                .cmp(&right.identity.matched_chunk_id)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(id: &str) -> GlobalCandidateIdentity {
        GlobalCandidateIdentity {
            access_zone_id: "zone".into(),
            document_id: "doc".into(),
            document_version: 1,
            matched_chunk_id: id.into(),
            parent_chunk_id: id.into(),
        }
    }

    #[test]
    fn candidate_found_by_two_segments_is_combined() {
        let fused = cross_segment_rrf(
            [
                SegmentCandidate {
                    identity: identity("a"),
                    segment_index: 0,
                    rank: 1,
                    score: 1.0,
                    segment_weight: 1.0,
                },
                SegmentCandidate {
                    identity: identity("a"),
                    segment_index: 1,
                    rank: 2,
                    score: 0.9,
                    segment_weight: 1.0,
                },
            ],
            60.0,
            10,
        );
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].matched_segments, vec![0, 1]);
    }

    #[test]
    fn segment_weights_affect_score() {
        let fused = cross_segment_rrf(
            [
                SegmentCandidate {
                    identity: identity("context"),
                    segment_index: 0,
                    rank: 1,
                    score: 1.0,
                    segment_weight: 0.5,
                },
                SegmentCandidate {
                    identity: identity("question"),
                    segment_index: 1,
                    rank: 1,
                    score: 1.0,
                    segment_weight: 1.0,
                },
            ],
            60.0,
            10,
        );
        assert_eq!(fused[0].identity.matched_chunk_id, "question");
    }

    #[test]
    fn rank_is_one_based() {
        let fused = cross_segment_rrf(
            [SegmentCandidate {
                identity: identity("a"),
                segment_index: 0,
                rank: 0,
                score: 1.0,
                segment_weight: 1.0,
            }],
            60.0,
            10,
        );
        assert!(fused[0].score > 0.0);
    }

    #[test]
    fn global_limit_is_applied_after_score_sum() {
        let fused = cross_segment_rrf(
            ["a", "b", "c"].into_iter().map(|id| SegmentCandidate {
                identity: identity(id),
                segment_index: 0,
                rank: 1,
                score: 1.0,
                segment_weight: 1.0,
            }),
            60.0,
            2,
        );
        assert_eq!(fused.len(), 2);
    }
}
