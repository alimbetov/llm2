use astravector_runtime::config::{AppConfig, QueryProcessingConfig};
use astravector_runtime::query_processing::{
    build_query_plan, QueryPlanningError, QueryProcessingTier, QueryTokenCounter,
};
use astravector_runtime::tokenizer::{CanonicalTokenizer, TokenOffset};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct ModelBackedCounter {
    tokenizer: CanonicalTokenizer,
}

impl QueryTokenCounter for ModelBackedCounter {
    fn count_tokens(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<usize, String> {
        self.tokenizer
            .count_canonical_tokens(text, max_length, allow_truncation)
            .map_err(|error| error.to_string())
    }

    fn token_offsets(&self, text: &str) -> Result<Vec<TokenOffset>, String> {
        self.tokenizer
            .token_offsets(text)
            .map_err(|error| error.to_string())
    }
}

fn tokenizer_path() -> PathBuf {
    if let Some(path) = std::env::var_os("ASTRAVECTOR_TOKENIZER_PATH") {
        return PathBuf::from(path);
    }
    let workspace_models = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("worktree has parent")
        .join("models/bge-m3/tokenizer.json");
    if workspace_models.is_file() {
        return workspace_models;
    }
    PathBuf::from("/models/bge-m3/tokenizer.json")
}

fn counter() -> ModelBackedCounter {
    let path = tokenizer_path();
    assert!(
        path.is_file(),
        "MODEL_FILES_NOT_FOUND: configured BGE-M3 tokenizer is missing at {}",
        path.display()
    );
    let mut config = AppConfig::load().expect("load application config");
    config.tokenizer.path = path.to_string_lossy().into_owned();
    ModelBackedCounter {
        tokenizer: CanonicalTokenizer::load(&config).expect("load configured canonical tokenizer"),
    }
}

fn exact_token_text(counter: &ModelBackedCounter, prefix: &str, target: usize) -> String {
    let mut text = prefix.to_owned();
    let mut count = counter
        .count_tokens(&text, target, false)
        .expect("count category prefix");
    assert!(count <= target, "category prefix exceeds target boundary");
    while count < target {
        text.push_str(" test");
        let next = counter
            .count_tokens(&text, target, false)
            .expect("append one-token deterministic filler");
        assert_eq!(next, count + 1, "BGE-M3 filler must add exactly one token");
        count = next;
    }
    assert_eq!(counter.count_tokens(&text, target, false).unwrap(), target);
    text
}

fn extended_config() -> QueryProcessingConfig {
    QueryProcessingConfig {
        enabled: true,
        extended_enabled: true,
        ..Default::default()
    }
}

#[test]
fn real_bge_m3_tokenizer_selects_exact_boundaries_without_truncation() {
    let counter = counter();
    let cases = [
        (
            256,
            "Объясните порядок активации документа.",
            QueryProcessingTier::Single,
        ),
        (
            257,
            "Құжатты белсендіру тәртібін түсіндіріңіз.",
            QueryProcessingTier::SegmentedStandard,
        ),
        (
            1_024,
            "Explain the PostgreSQL source of truth.",
            QueryProcessingTier::SegmentedStandard,
        ),
        (
            1_025,
            "Сравните dense sparse және PostgreSQL FTS.",
            QueryProcessingTier::SegmentedExtended,
        ),
        (
            2_048,
            "content_chunks_v004 /api/v1/search SELECT parent_chunk_id FROM graph_relations.",
            QueryProcessingTier::SegmentedExtended,
        ),
    ];
    for (target, prefix, expected_tier) in cases {
        let special_token_overhead = counter
            .count_tokens(prefix, target, false)
            .expect("count model input tokens")
            .saturating_sub(
                counter
                    .token_offsets(prefix)
                    .expect("count source-backed tokens")
                    .len(),
            );
        assert!(special_token_overhead > 0);
        let query = exact_token_text(&counter, prefix, target);
        let plan = build_query_plan(&query, &counter, &extended_config(), 256)
            .unwrap_or_else(|error| panic!("target={target}: {error}"));
        assert_eq!(plan.original_token_count, target);
        assert_eq!(plan.tier, expected_tier);
        assert_eq!(
            plan.normalized_query.token_offsets.len(),
            target - special_token_overhead
        );
        assert_eq!(plan.segments.first().unwrap().source_token_start, 0);
        assert_eq!(
            plan.segments.last().unwrap().source_token_end,
            plan.normalized_query.token_offsets.len()
        );
        assert!(plan
            .segments
            .iter()
            .all(|segment| segment.token_count <= plan.limits.segment_max_tokens));
        let tail = plan
            .normalized_query
            .token_offsets
            .last()
            .expect("token tail");
        assert_eq!(
            plan.normalized_query.normalized_text[tail.start_byte..tail.end_byte].trim(),
            "test"
        );
    }
}

#[test]
fn token_2049_is_rejected_before_any_retrieval_backend_call() {
    let counter = counter();
    let query = exact_token_text(
        &counter,
        "java.lang.IllegalStateException at api.Search.run CJK 安全 тексеру",
        2_049,
    );
    let embedding_calls = AtomicUsize::new(0);
    let qdrant_calls = AtomicUsize::new(0);
    let fts_calls = AtomicUsize::new(0);
    let graph_calls = AtomicUsize::new(0);

    let plan = build_query_plan(&query, &counter, &extended_config(), 256);
    if plan.is_ok() {
        embedding_calls.fetch_add(1, Ordering::SeqCst);
        qdrant_calls.fetch_add(1, Ordering::SeqCst);
        fts_calls.fetch_add(1, Ordering::SeqCst);
        graph_calls.fetch_add(1, Ordering::SeqCst);
    }

    assert!(matches!(plan, Err(QueryPlanningError::TokenLimitExceeded)));
    assert_eq!(embedding_calls.load(Ordering::SeqCst), 0);
    assert_eq!(qdrant_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fts_calls.load(Ordering::SeqCst), 0);
    assert_eq!(graph_calls.load(Ordering::SeqCst), 0);
}
