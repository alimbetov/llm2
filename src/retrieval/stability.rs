use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ResultIdentity {
    pub access_zone_id: String,
    pub document_id: String,
    pub document_version: u64,
    pub matched_chunk_id: String,
    pub source_block_id: String,
}

pub fn top1_matches(baseline: &[ResultIdentity], loaded: &[ResultIdentity]) -> bool {
    baseline.first() == loaded.first()
}

pub fn top_k_jaccard(baseline: &[ResultIdentity], loaded: &[ResultIdentity], limit: usize) -> f64 {
    let baseline = baseline.iter().take(limit).collect::<BTreeSet<_>>();
    let loaded = loaded.iter().take(limit).collect::<BTreeSet<_>>();
    let union = baseline.union(&loaded).count();
    if union == 0 {
        1.0
    } else {
        baseline.intersection(&loaded).count() as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(id: &str) -> ResultIdentity {
        ResultIdentity {
            access_zone_id: "zone".into(),
            document_id: "document".into(),
            document_version: 1,
            matched_chunk_id: id.into(),
            source_block_id: id.into(),
        }
    }

    #[test]
    fn empty_results_are_stable() {
        assert!(top1_matches(&[], &[]));
        assert_eq!(top_k_jaccard(&[], &[], 5), 1.0);
    }

    #[test]
    fn order_changes_top1_but_not_set_similarity() {
        let baseline = vec![identity("a"), identity("b")];
        let loaded = vec![identity("b"), identity("a")];
        assert!(!top1_matches(&baseline, &loaded));
        assert_eq!(top_k_jaccard(&baseline, &loaded, 5), 1.0);
    }
}
