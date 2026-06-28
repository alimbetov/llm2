use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceScores {
    pub dense_score: f32,
    pub sparse_score: f32,
    pub lexical_score: f32,
    pub fused_score: f32,
    pub consistency_score: f32,
    pub final_score: f32,
    pub decision: String,
}
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (dot, na, nb) = a.iter().zip(b).fold((0.0, 0.0, 0.0), |(d, x, y), (u, v)| {
        (d + u * v, x + u * u, y + v * v)
    });
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0)
    }
}
pub fn sparse_dot(ai: &[u32], av: &[f32], bi: &[u32], bv: &[f32]) -> f32 {
    let mut i = 0;
    let mut j = 0;
    let mut s = 0.0;
    while i < ai.len() && j < bi.len() {
        if ai[i] == bi[j] {
            s += av[i] * bv[j];
            i += 1;
            j += 1
        } else if ai[i] < bi[j] {
            i += 1
        } else {
            j += 1
        }
    }
    s
}
pub fn lexical_overlap(q: &str, c: &str) -> f32 {
    let q: HashSet<_> = q.split_whitespace().map(|s| s.to_lowercase()).collect();
    let c: HashSet<_> = c.split_whitespace().map(|s| s.to_lowercase()).collect();
    if q.is_empty() {
        0.0
    } else {
        q.intersection(&c).count() as f32 / q.len() as f32
    }
}
pub fn combine(dense: f32, sparse: f32, lexical: f32, consistency: Option<f32>) -> RelevanceScores {
    let fused =
        (0.55 * dense.max(0.0) + 0.25 * sparse.clamp(0.0, 1.0) + 0.20 * lexical).clamp(0.0, 1.0);
    let consistency = consistency.unwrap_or(0.0);
    let final_score = if consistency > 0.0 {
        (0.8 * fused + 0.2 * consistency).clamp(0.0, 1.0)
    } else {
        fused
    };
    let decision = if final_score >= 0.85 {
        "RELEVANT"
    } else if final_score >= 0.65 {
        "PARTIALLY_RELEVANT"
    } else if final_score >= 0.45 {
        "UNCERTAIN"
    } else {
        "NOT_RELEVANT"
    }
    .to_string();
    RelevanceScores {
        dense_score: dense,
        sparse_score: sparse,
        lexical_score: lexical,
        fused_score: fused,
        consistency_score: consistency,
        final_score,
        decision,
    }
}
