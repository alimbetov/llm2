use crate::query_processing::classification::{has_question_form, has_technical_identifier};
use crate::query_processing::planner::QuerySegment;
use crate::query_processing::NormalizedQuery;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryIntentKind {
    ExplicitQuestion,
    ImperativeRequest,
    Constraint,
    TechnicalEvidence,
    Context,
    ImplicitSearchIntent,
}

#[derive(Debug, Clone)]
pub struct QueryIntentUnit {
    pub id: usize,
    pub kind: QueryIntentKind,
    pub text: String,
    pub source_segment_indices: Vec<usize>,
    pub source_token_start: usize,
    pub source_token_end: usize,
    pub normalized_byte_start: usize,
    pub normalized_byte_end: usize,
    pub original_byte_start: usize,
    pub original_byte_end: usize,
    pub required: bool,
    pub searchable: bool,
    pub weight: f32,
    pub normalized_sha256: String,
}

pub fn extract_query_intents(query: &str, segments: &[QuerySegment]) -> Vec<QueryIntentUnit> {
    let normalized = NormalizedQuery {
        original_text: query.to_owned(),
        normalized_text: query.to_owned(),
        normalized_to_original_byte_map: (0..=query.len()).collect(),
        token_offsets: Vec::new(),
    };
    extract_query_intents_normalized(&normalized, segments)
}

pub fn extract_query_intents_normalized(
    normalized: &NormalizedQuery,
    segments: &[QuerySegment],
) -> Vec<QueryIntentUnit> {
    let query = normalized.normalized_text.as_str();
    let mut units = Vec::new();
    for sentence in split_intent_sentences(query) {
        let trimmed = sentence.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (kind, required, weight) = classify_intent(trimmed);
        let source_segment_indices = segments
            .iter()
            .filter(|segment| {
                ranges_overlap(
                    segment.source_byte_start,
                    segment.source_byte_end,
                    sentence.start,
                    sentence.end,
                )
            })
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        if source_segment_indices.is_empty() {
            continue;
        }
        let source_token_start = source_segment_indices
            .iter()
            .filter_map(|index| segments.get(*index))
            .map(|segment| segment.source_token_start)
            .min()
            .unwrap_or(0);
        let source_token_end = source_segment_indices
            .iter()
            .filter_map(|index| segments.get(*index))
            .map(|segment| segment.source_token_end)
            .max()
            .unwrap_or(source_token_start);
        let Some((original_byte_start, original_byte_end)) =
            normalized.original_byte_range(sentence.start, sentence.end)
        else {
            continue;
        };
        units.push(QueryIntentUnit {
            id: units.len(),
            kind,
            text: trimmed.to_owned(),
            source_segment_indices,
            source_token_start,
            source_token_end,
            normalized_byte_start: sentence.start,
            normalized_byte_end: sentence.end,
            original_byte_start,
            original_byte_end,
            required,
            searchable: true,
            weight,
            normalized_sha256: normalized_hash(trimmed),
        });
    }

    if units.iter().all(|unit| !unit.required) {
        let searchable_segments = segments
            .iter()
            .filter(|segment| segment.searchable)
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        let (original_byte_start, original_byte_end) = normalized
            .original_byte_range(0, query.len())
            .unwrap_or((0, normalized.original_text.len()));
        units.push(QueryIntentUnit {
            id: units.len(),
            kind: QueryIntentKind::ImplicitSearchIntent,
            text: query.trim().to_owned(),
            source_segment_indices: searchable_segments,
            source_token_start: 0,
            source_token_end: segments
                .last()
                .map(|segment| segment.source_token_end)
                .unwrap_or(0),
            normalized_byte_start: 0,
            normalized_byte_end: query.len(),
            original_byte_start,
            original_byte_end,
            required: true,
            searchable: true,
            weight: 1.0,
            normalized_sha256: normalized_hash(query),
        });
    }

    units
}

#[derive(Debug, Clone)]
struct IntentSentence<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn split_intent_sentences(text: &str) -> Vec<IntentSentence<'_>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_code = false;
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if text[index..].starts_with("```") {
            in_code = !in_code;
            index += 3;
            continue;
        }
        let ch = text[index..].chars().next().expect("valid char boundary");
        let end = index + ch.len_utf8();
        let boundary = !in_code
            && (matches!(ch, '?' | '!' | '\n')
                || (ch == '.' && text[end..].chars().next().is_none_or(char::is_whitespace)));
        if boundary {
            let piece = &text[start..end];
            if !piece.trim().is_empty() {
                out.push(IntentSentence {
                    text: piece,
                    start,
                    end,
                });
            }
            start = end;
        }
        index = end;
    }
    if start < text.len() {
        let piece = &text[start..];
        if !piece.trim().is_empty() {
            out.push(IntentSentence {
                text: piece,
                start,
                end: text.len(),
            });
        }
    }
    out
}

fn classify_intent(text: &str) -> (QueryIntentKind, bool, f32) {
    let lower = text.trim().to_lowercase();
    if looks_like_technical_evidence(text) {
        return (QueryIntentKind::TechnicalEvidence, false, 0.65);
    }
    if text.trim().ends_with('?') {
        return (QueryIntentKind::ExplicitQuestion, true, 1.0);
    }
    if has_imperative_prefix(&lower) || has_question_form(text) {
        return (QueryIntentKind::ImperativeRequest, true, 1.0);
    }
    if has_constraint_prefix(&lower) {
        return (QueryIntentKind::Constraint, true, 0.9);
    }
    if has_technical_identifier(text) {
        return (QueryIntentKind::TechnicalEvidence, false, 0.65);
    }
    (QueryIntentKind::Context, false, 0.4)
}

fn has_imperative_prefix(lower: &str) -> bool {
    [
        "explain ",
        "describe ",
        "show ",
        "find ",
        "compare ",
        "summarize ",
        "please explain ",
        "could you compare ",
        "could you explain ",
        "объясни ",
        "опиши ",
        "покажи ",
        "найди ",
        "сравни ",
        "проанализируй ",
        "пожалуйста, объясните ",
        "пожалуйста, объясни ",
        "подскажите, почему ",
        "мне нужно понять ",
        "можно ли сравнить ",
        "нужно определить ",
        "түсіндір ",
        "сипатта ",
        "көрсет ",
        "тап ",
        "салыстыр ",
        "маған түсіндіріңіз ",
        "маған түсіндір ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn has_constraint_prefix(lower: &str) -> bool {
    [
        "only ",
        "must ",
        "without ",
        "using only ",
        "учитывай ",
        "только ",
        "не используй ",
        "должен ",
        "тек ",
        "қолданба ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn looks_like_technical_evidence(text: &str) -> bool {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    trimmed.starts_with("```")
        || lower.starts_with("select ")
        || lower.starts_with("insert ")
        || lower.starts_with("update ")
        || lower.starts_with("delete ")
        || lower.starts_with("with ") && lower.contains(" select ")
        || lower.starts_with("must not be null")
        || trimmed.contains("Exception:")
        || trimmed.contains("Exception at ")
        || trimmed.contains("Caused by:")
        || trimmed
            .lines()
            .filter(|line| line.trim_start().starts_with("at "))
            .count()
            >= 2
}

fn ranges_overlap(
    left_start: usize,
    left_end: usize,
    right_start: usize,
    right_end: usize,
) -> bool {
    left_start < right_end && right_start < left_end
}

fn normalized_hash(text: &str) -> String {
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_processing::planner::QuerySegment;
    use crate::query_processing::QuerySegmentKind;

    fn segment(text: &str) -> QuerySegment {
        QuerySegment {
            index: 0,
            text: text.to_owned(),
            token_count: text.split_whitespace().count(),
            source_token_start: 0,
            source_token_end: text.split_whitespace().count(),
            source_byte_start: 0,
            source_byte_end: text.len(),
            original_byte_start: 0,
            original_byte_end: text.len(),
            kind: QuerySegmentKind::Context,
            has_question_form: false,
            has_technical_identifier: false,
            searchable: true,
            weight: 1.0,
            required_for_coverage: false,
            intent_unit_ids: Vec::new(),
            sha256: String::new(),
        }
    }

    fn normalized(text: &str) -> NormalizedQuery {
        NormalizedQuery {
            original_text: text.to_owned(),
            normalized_text: text.to_owned(),
            normalized_to_original_byte_map: (0..=text.len()).collect(),
            token_offsets: Vec::new(),
        }
    }

    #[test]
    fn technical_log_is_not_explicitly_required() {
        let query = "Caused by: java.lang.IllegalStateException\n at service.run(Service.java:12)";
        let units = extract_query_intents_normalized(&normalized(query), &[segment(query)]);
        assert!(units
            .iter()
            .any(|unit| unit.kind == QueryIntentKind::ImplicitSearchIntent));
    }

    #[test]
    fn question_is_required() {
        let query = "Why does Sparse miss the document?";
        let units = extract_query_intents_normalized(&normalized(query), &[segment(query)]);
        assert!(units.iter().any(|unit| unit.required));
    }

    #[test]
    fn multilingual_polite_requests_are_required() {
        for query in [
            "Пожалуйста, объясните порядок активации",
            "Подскажите, почему версия не активна",
            "Мне нужно понять правила legal hold",
            "Можно ли сравнить dense и sparse",
            "Нужно определить source of truth",
            "Please explain document activation",
            "Could you compare dense and sparse",
            "Маған түсіндіріңіз құжатты белсендіру тәртібін",
        ] {
            let units = extract_query_intents_normalized(&normalized(query), &[segment(query)]);
            assert!(units.iter().any(|unit| unit.required), "query={query}");
        }
    }

    #[test]
    fn technical_sql_and_nullability_logs_precede_constraint_classification() {
        for query in [
            "select * from content_chunks_v004",
            "must not be null: parent_chunk_id",
            "```sql\nselect * from graph_relations\n```",
        ] {
            let units = extract_query_intents_normalized(&normalized(query), &[segment(query)]);
            assert!(units
                .iter()
                .any(|unit| unit.kind == QueryIntentKind::TechnicalEvidence));
            assert!(!units
                .iter()
                .any(|unit| unit.kind == QueryIntentKind::Constraint));
        }
    }
}
