use crate::error::AstraError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentRequest {
    pub access_zone_id: String,
    pub root_chunk_id: String,
    pub source_chunk_id: String,
    pub source_text: String,
    pub max_questions: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedRepresentation {
    pub representation_type: String,
    pub text: String,
    pub status: String,
    pub confidence: f32,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnrichmentResult {
    pub representations: Vec<GeneratedRepresentation>,
}
#[async_trait]
pub trait EnrichmentProvider: Send + Sync {
    async fn enrich(&self, request: EnrichmentRequest) -> Result<EnrichmentResult, AstraError>;
}
#[derive(Default)]
pub struct DisabledEnrichmentProvider;
#[async_trait]
impl EnrichmentProvider for DisabledEnrichmentProvider {
    async fn enrich(&self, _: EnrichmentRequest) -> Result<EnrichmentResult, AstraError> {
        Ok(EnrichmentResult::default())
    }
}
pub trait EnrichmentValidator: Send + Sync {
    fn validate(&self, source: &str, r: &GeneratedRepresentation) -> GeneratedRepresentation;
}
#[derive(Default)]
pub struct RuleBasedValidator;
impl EnrichmentValidator for RuleBasedValidator {
    fn validate(&self, source: &str, r: &GeneratedRepresentation) -> GeneratedRepresentation {
        let mut out = r.clone();
        let text = out.text.trim();
        let source_terms: HashSet<_> = source
            .split_whitespace()
            .map(|x| x.to_lowercase())
            .collect();
        let terms: HashSet<_> = text.split_whitespace().map(|x| x.to_lowercase()).collect();
        let overlap = if terms.is_empty() {
            0.0
        } else {
            terms.intersection(&source_terms).count() as f32 / terms.len() as f32
        };
        out.status = if text.is_empty() {
            "REJECTED"
        } else if overlap >= 0.35 {
            "VALIDATED"
        } else {
            "UNCERTAIN"
        }
        .into();
        out.confidence = overlap;
        out.reasons = vec![format!("source_term_overlap={overlap:.3}")];
        out
    }
}
pub fn validate_and_deduplicate(
    source: &str,
    result: EnrichmentResult,
    max_len: usize,
    v: &dyn EnrichmentValidator,
) -> EnrichmentResult {
    let mut seen = HashSet::new();
    let representations = result
        .representations
        .into_iter()
        .filter_map(|r| {
            let text = r.text.trim().to_string();
            if text.is_empty()
                || text.len() > max_len
                || text == source.trim()
                || !seen.insert(text.to_lowercase())
            {
                return None;
            }
            let mut rr = r;
            rr.text = text;
            Some(v.validate(source, &rr))
        })
        .collect();
    EnrichmentResult { representations }
}
