use astravector_runtime::config::QueryProcessingConfig;
use astravector_runtime::query_processing::{
    build_query_plan, normalize_query, QueryProcessingTier, QueryTokenCounter,
};
use astravector_runtime::tokenizer::TokenOffset;

struct WhitespaceCounter;

impl QueryTokenCounter for WhitespaceCounter {
    fn count_tokens(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<usize, String> {
        let count = text.split_whitespace().count();
        if count > max_length && !allow_truncation {
            return Err(format!("{count} tokens exceeds max_length={max_length}"));
        }
        Ok(count.min(max_length))
    }
}

#[test]
fn crlf_blank_lines_and_trailing_whitespace_share_one_mapping() {
    let original = "  Пожалуйста, объясните PostgreSQL.  \r\n\r\n\r\n\r\nSELECT * FROM graph_relations;  \r\nМаған API /v1/search түсіндіріңіз.   ";
    let normalized = normalize_query(original, &WhitespaceCounter).expect("normalize query");

    assert!(!normalized.normalized_text.contains('\r'));
    assert!(!normalized.normalized_text.starts_with(char::is_whitespace));
    assert!(!normalized.normalized_text.ends_with(char::is_whitespace));
    assert!(!normalized.normalized_text.contains("\n\n\n\n"));
    assert!(normalized
        .normalized_text
        .contains("SELECT * FROM graph_relations;"));

    let start = normalized
        .normalized_text
        .find("Маған")
        .expect("Kazakh text retained");
    let end = start + "Маған".len();
    let (original_start, original_end) = normalized
        .original_byte_range(start, end)
        .expect("mapped byte range");
    assert_eq!(&original[original_start..original_end], "Маған");
    assert!(normalized.token_offsets.iter().all(|offset| normalized
        .normalized_text
        .is_char_boundary(offset.start_byte)
        && normalized.normalized_text.is_char_boundary(offset.end_byte)));
}

#[test]
fn segments_and_intents_use_normalized_offsets_with_original_ranges() {
    let query =
        "Почему PostgreSQL является source of truth?  \r\n\r\nКак legal hold влияет на TTL?   ";
    let config = QueryProcessingConfig {
        enabled: true,
        extended_enabled: true,
        ..Default::default()
    };
    let plan = build_query_plan(query, &WhitespaceCounter, &config, 4).expect("build query plan");

    assert_eq!(plan.tier, QueryProcessingTier::SegmentedStandard);
    assert!(
        plan.intent_units
            .iter()
            .filter(|intent| intent.required)
            .count()
            >= 2
    );
    for segment in &plan.segments {
        assert_eq!(
            segment.text,
            plan.normalized_query.normalized_text
                [segment.source_byte_start..segment.source_byte_end]
        );
        assert!(query.is_char_boundary(segment.original_byte_start));
        assert!(query.is_char_boundary(segment.original_byte_end));
    }
    for intent in &plan.intent_units {
        let normalized_text = &plan.normalized_query.normalized_text
            [intent.normalized_byte_start..intent.normalized_byte_end];
        let original_text = &query[intent.original_byte_start..intent.original_byte_end];
        assert_eq!(
            normalized_text.trim().replace('\n', " "),
            original_text.trim().replace("\r\n", " ")
        );
        assert!(intent.source_segment_indices.iter().all(|index| {
            let segment = &plan.segments[*index];
            segment.source_byte_start < intent.normalized_byte_end
                && intent.normalized_byte_start < segment.source_byte_end
        }));
    }
}

#[test]
fn fenced_code_stack_trace_sql_and_urls_remain_utf8_safe() {
    let query = "```sql\nselect * from content_chunks_v004;\n```\njava.lang.IllegalStateException\n at api.Search.run(Search.java:12)\nТексеру: https://example.kz/api/v1/search?q=құжат";
    let normalized = normalize_query(query, &WhitespaceCounter).expect("normalize technical query");
    assert!(normalized.normalized_text.contains("```sql"));
    assert!(normalized.normalized_text.contains("content_chunks_v004"));
    assert!(normalized.normalized_text.contains("Search.java:12"));
    assert!(normalized.normalized_text.contains("q=құжат"));
    assert_eq!(
        normalized.normalized_to_original_byte_map.len(),
        normalized.normalized_text.len() + 1
    );
}

#[test]
fn invalid_tokenizer_byte_offsets_fail_closed() {
    struct InvalidOffsetCounter;
    impl QueryTokenCounter for InvalidOffsetCounter {
        fn count_tokens(
            &self,
            _text: &str,
            _max_length: usize,
            _allow_truncation: bool,
        ) -> Result<usize, String> {
            Ok(1)
        }

        fn token_offsets(&self, _text: &str) -> Result<Vec<TokenOffset>, String> {
            Ok(vec![TokenOffset {
                token_index: 0,
                start_byte: 1,
                end_byte: 2,
            }])
        }
    }

    let error = normalize_query("Қ", &InvalidOffsetCounter).expect_err("unsafe offset rejected");
    assert!(error.contains("invalid UTF-8 byte range"));
}
