use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CandidateIntentEvidenceReason {
    ExactTechnicalMatch,
    SparseEvidence,
    DenseEvidence,
    LexicalEvidence,
    GraphOriginEvidence,
    InsufficientEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateIntentEvidence {
    pub intent_id: usize,
    pub dense_score: Option<f32>,
    pub sparse_score: Option<f32>,
    pub lexical_score: Option<f32>,
    pub matched_term_count: usize,
    pub exact_technical_match_count: usize,
    pub evidence_passed: bool,
    pub reason_code: CandidateIntentEvidenceReason,
}

impl CandidateIntentEvidence {
    #[allow(clippy::too_many_arguments)]
    pub fn direct(
        intent_id: usize,
        dense_score: Option<f32>,
        sparse_score: Option<f32>,
        lexical_score: Option<f32>,
        matched_term_count: usize,
        exact_technical_match_count: usize,
        evidence_passed: bool,
    ) -> Self {
        let reason_code = if !evidence_passed {
            CandidateIntentEvidenceReason::InsufficientEvidence
        } else if exact_technical_match_count > 0 {
            CandidateIntentEvidenceReason::ExactTechnicalMatch
        } else if sparse_score.unwrap_or_default() >= dense_score.unwrap_or_default()
            && sparse_score.unwrap_or_default() > 0.0
        {
            CandidateIntentEvidenceReason::SparseEvidence
        } else if dense_score.unwrap_or_default() > 0.0 {
            CandidateIntentEvidenceReason::DenseEvidence
        } else {
            CandidateIntentEvidenceReason::LexicalEvidence
        };
        Self {
            intent_id,
            dense_score,
            sparse_score,
            lexical_score,
            matched_term_count,
            exact_technical_match_count,
            evidence_passed,
            reason_code,
        }
    }

    pub fn graph_origin(intent_id: usize) -> Self {
        Self {
            intent_id,
            dense_score: None,
            sparse_score: None,
            lexical_score: None,
            matched_term_count: 0,
            exact_technical_match_count: 0,
            evidence_passed: true,
            reason_code: CandidateIntentEvidenceReason::GraphOriginEvidence,
        }
    }
}
