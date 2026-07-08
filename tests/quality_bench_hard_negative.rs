use serde_json::{json, Value};

#[derive(Debug, Default)]
struct MockContext {
    document_id: String,
    block_id: String,
    text: String,
}

#[derive(Debug, Default)]
struct MockEval {
    passed: bool,
    reasons: Vec<&'static str>,
}

fn max_false_positive_contexts(expected: &Value) -> u64 {
    expected
        .get("max_false_positive_contexts")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn string_list<'a>(expected: &'a Value, key: &str) -> Vec<&'a str> {
    expected
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn evaluate_hard_negative(expected: &Value, contexts: &[MockContext]) -> MockEval {
    let mut eval = MockEval {
        passed: true,
        reasons: Vec::new(),
    };
    if contexts.len() as u64 > max_false_positive_contexts(expected) {
        eval.passed = false;
        eval.reasons.push("QUALITY_GATES_FAILED");
    }
    for forbidden in string_list(expected, "forbidden_document_ids") {
        if contexts.iter().any(|ctx| ctx.document_id == forbidden) {
            eval.passed = false;
            eval.reasons.push("FORBIDDEN_DOCUMENT_RETURNED");
        }
    }
    for forbidden in string_list(expected, "forbidden_block_ids") {
        if contexts.iter().any(|ctx| ctx.block_id == forbidden) {
            eval.passed = false;
            eval.reasons.push("FORBIDDEN_BLOCK_RETURNED");
        }
    }
    let joined = contexts
        .iter()
        .map(|ctx| ctx.text.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in string_list(expected, "forbidden_phrases") {
        if joined.contains(&forbidden.to_lowercase()) {
            eval.passed = false;
            eval.reasons.push("FORBIDDEN_PHRASE_RETURNED");
        }
    }
    eval.reasons.sort_unstable();
    eval.reasons.dedup();
    eval
}

#[test]
fn hard_negative_empty_contexts_pass() {
    let expected = json!({
        "forbidden_document_ids": ["secret-doc"],
        "forbidden_phrases": ["secret phrase"]
    });
    let eval = evaluate_hard_negative(&expected, &[]);
    assert!(eval.passed);
    assert!(eval.reasons.is_empty());
}

#[test]
fn hard_negative_forbidden_document_fails() {
    let expected = json!({ "forbidden_document_ids": ["secret-doc"] });
    let contexts = vec![MockContext {
        document_id: "secret-doc".into(),
        block_id: "public-block".into(),
        text: "benign text".into(),
    }];
    let eval = evaluate_hard_negative(&expected, &contexts);
    assert!(!eval.passed);
    assert!(eval.reasons.contains(&"FORBIDDEN_DOCUMENT_RETURNED"));
}

#[test]
fn hard_negative_forbidden_phrase_fails() {
    let expected = json!({ "forbidden_phrases": ["classified project zephyr"] });
    let contexts = vec![MockContext {
        document_id: "public-doc".into(),
        block_id: "public-block".into(),
        text: "This mentions Classified Project Zephyr.".into(),
    }];
    let eval = evaluate_hard_negative(&expected, &contexts);
    assert!(!eval.passed);
    assert!(eval.reasons.contains(&"FORBIDDEN_PHRASE_RETURNED"));
}

#[test]
fn hard_negative_forbidden_block_fails() {
    let expected = json!({ "forbidden_block_ids": ["secret-block"] });
    let contexts = vec![MockContext {
        document_id: "public-doc".into(),
        block_id: "secret-block".into(),
        text: "benign text".into(),
    }];
    let eval = evaluate_hard_negative(&expected, &contexts);
    assert!(!eval.passed);
    assert!(eval.reasons.contains(&"FORBIDDEN_BLOCK_RETURNED"));
}

#[test]
fn default_max_false_positive_contexts_is_zero() {
    let expected = json!({});
    assert_eq!(max_false_positive_contexts(&expected), 0);
    let contexts = vec![MockContext {
        document_id: "any-doc".into(),
        block_id: "any-block".into(),
        text: "any returned context is a false positive by default".into(),
    }];
    let eval = evaluate_hard_negative(&expected, &contexts);
    assert!(!eval.passed);
    assert!(eval.reasons.contains(&"QUALITY_GATES_FAILED"));
}

#[test]
fn no_answer_triggered_hard_negative_is_counted_passed() {
    let expected = json!({ "max_false_positive_contexts": 0 });
    let eval = evaluate_hard_negative(&expected, &[]);
    assert!(eval.passed);
}

#[test]
fn hard_negative_semantics_do_not_need_query_id_exception() {
    let expected = json!({ "forbidden_phrases": ["do not return this"] });
    let query_ids = ["hard-negative-001", "any-other-id", ""];
    for _query_id in query_ids {
        let eval = evaluate_hard_negative(&expected, &[]);
        assert!(eval.passed);
    }
}
