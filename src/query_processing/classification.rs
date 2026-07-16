use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuerySegmentKind {
    Question,
    Technical,
    Context,
}

pub fn classify_query_segment(text: &str) -> QuerySegmentKind {
    let question = has_question_form(text);
    let technical = has_technical_identifier(text);
    match (question, technical) {
        (true, _) => QuerySegmentKind::Question,
        (false, true) => QuerySegmentKind::Technical,
        (false, false) => QuerySegmentKind::Context,
    }
}

pub fn has_question_form(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.ends_with('?') {
        return true;
    }
    let lower = trimmed.to_lowercase();
    let imperative_prefixes = [
        "what ",
        "why ",
        "how ",
        "which ",
        "when ",
        "where ",
        "who ",
        "explain ",
        "describe ",
        "show ",
        "find ",
        "connect ",
        "compare ",
        "summarize ",
        "объясни ",
        "покажи ",
        "найди ",
        "сравни ",
        "опиши ",
        "как ",
        "почему ",
        "какой ",
        "какие ",
        "где ",
        "когда ",
        "что ",
        "түсіндір ",
        "көрсет ",
        "тап ",
        "салыстыр ",
        "қалай ",
        "неге ",
        "қайда ",
        "қашан ",
        "не ",
    ];
    imperative_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

pub fn has_technical_identifier(text: &str) -> bool {
    static TECHNICAL_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = TECHNICAL_PATTERNS.get_or_init(|| {
        [
            r"(?i)\b[A-Z]{2,}[-_][A-Z0-9]{2,}\b",
            r"(?i)\b[A-Z]{2,}-\d{3,}\b",
            r"(?i)\b[a-z0-9_]+\.(rs|py|java|kt|go|ts|tsx|js|jsx|json|yaml|yml|toml|sql|proto|md)\b",
            r"(?i)\b/[A-Za-z0-9._~:/?#\[\]@!$&'()*+,;=%-]+",
            r"(?i)\b[a-z][a-z0-9]*(_[a-z0-9]+){1,}\b",
            r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b",
            r"(?i)\b(ORA|SQLSTATE|HTTP|ERR|E)[-_]?\d{3,}\b",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("query classification regex must compile"))
        .collect()
    });
    patterns.iter().any(|regex| regex.is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_mark_is_question() {
        assert_eq!(
            classify_query_segment("What happens to legal hold?"),
            QuerySegmentKind::Question
        );
    }

    #[test]
    fn imperative_request_is_question() {
        assert_eq!(
            classify_query_segment("Explain the reconciliation procedure"),
            QuerySegmentKind::Question
        );
    }

    #[test]
    fn technical_path_is_technical() {
        assert_eq!(
            classify_query_segment("The failing endpoint is /api/v1/documents"),
            QuerySegmentKind::Technical
        );
    }

    #[test]
    fn error_code_is_technical() {
        assert!(has_technical_identifier(
            "Database returns ORA-00904 during ingestion"
        ));
    }

    #[test]
    fn plain_context_is_context() {
        assert_eq!(
            classify_query_segment("This is background context without a direct request."),
            QuerySegmentKind::Context
        );
    }

    #[test]
    fn russian_imperative_is_question() {
        assert!(has_question_form("Объясни порядок активации документа"));
    }

    #[test]
    fn kazakh_imperative_is_question() {
        assert!(has_question_form("Түсіндір құжатты белсендіру тәртібін"));
    }
}
