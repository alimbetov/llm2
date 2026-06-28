use crate::error::AstraError;
use std::collections::{HashMap, HashSet};

pub fn build_sparse(
    input_ids: &[u32],
    mask: &[u32],
    weights: &[f32],
    special: &HashSet<u32>,
    min_weight: f32,
    max_non_zero: usize,
) -> Result<(Vec<u32>, Vec<f32>), AstraError> {
    if input_ids.len() != mask.len() || mask.len() != weights.len() {
        return Err(AstraError::Internal("sparse tensor lengths differ".into()));
    }
    let mut merged = HashMap::<u32, f32>::new();
    for ((id, m), w) in input_ids.iter().zip(mask).zip(weights) {
        if !w.is_finite() {
            return Err(AstraError::Internal(
                "sparse vector contains NaN/Infinity".into(),
            ));
        }
        if *m == 0 || special.contains(id) || *w <= min_weight {
            continue;
        }
        merged
            .entry(*id)
            .and_modify(|old| *old = old.max(*w))
            .or_insert(*w);
    }
    let mut values: Vec<_> = merged.into_iter().collect();
    values.sort_by(|a, b| b.1.total_cmp(&a.1));
    values.truncate(max_non_zero);
    values.sort_by_key(|x| x.0);
    Ok((
        values.iter().map(|x| x.0).collect(),
        values.iter().map(|x| x.1).collect(),
    ))
}
