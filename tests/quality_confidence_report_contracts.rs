use serde_json::{json, Value};

fn profile_skipped(profile: &Value) -> bool {
    matches!(
        profile.get("runtime_execution").and_then(Value::as_str),
        Some("SKIPPED_ENDPOINT_NOT_SET" | "SKIPPED_RUNTIME_REQUIRED" | "MODEL_BACKED_E2E_SKIPPED")
    ) || profile.get("verdict").and_then(Value::as_str) == Some("SKIPPED")
}

fn confidence_verdict(report: &Value, diagnostic_only: bool) -> (&'static str, Vec<&'static str>) {
    if diagnostic_only {
        return ("DIAGNOSTIC_ONLY", Vec::new());
    }
    let mut reasons = Vec::new();
    if report
        .pointer("/preflight/endpoint_available")
        .and_then(Value::as_bool)
        == Some(false)
    {
        reasons.push("ENDPOINT_UNAVAILABLE");
    }
    for profile in ["dense", "sparse", "hybrid"] {
        let value = report.pointer(&format!("/profiles/{profile}")).unwrap();
        if profile_skipped(value) {
            reasons.push("PROFILE_SKIPPED");
        }
    }
    if report
        .pointer("/profiles/dense/verdict")
        .and_then(Value::as_str)
        != Some("PASS")
    {
        reasons.push("DENSE_PROFILE_NOT_PASS");
    }
    if report
        .pointer("/profiles/sparse/blocked")
        .and_then(Value::as_bool)
        == Some(true)
    {
        reasons.push("SPARSE_PROFILE_BLOCKED");
    }
    if report
        .pointer("/profiles/hybrid/blocked")
        .and_then(Value::as_bool)
        == Some(true)
    {
        reasons.push("HYBRID_PROFILE_BLOCKED");
    }
    if report
        .pointer("/profiles/sparse/sparse_available")
        .and_then(Value::as_bool)
        != Some(true)
    {
        reasons.push("SPARSE_UNAVAILABLE");
    }
    if report
        .pointer("/profiles/hybrid/hybrid_available")
        .and_then(Value::as_bool)
        != Some(true)
    {
        reasons.push("HYBRID_UNAVAILABLE");
    }
    if report
        .pointer("/no_answer/enabled")
        .and_then(Value::as_bool)
        != Some(true)
    {
        reasons.push("NO_ANSWER_DISABLED");
    }
    if report
        .pointer("/hard_negative/false_positive_reduction_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        < report
            .pointer("/hard_negative/target_reduction_rate")
            .and_then(Value::as_f64)
            .unwrap_or(0.5)
    {
        reasons.push("HARD_NEGATIVE_TARGET_NOT_MET");
    }
    if report
        .pointer("/hard_negative/after_forbidden_total")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        > report
            .pointer("/hard_negative/max_allowed_forbidden_total")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    {
        reasons.push("FORBIDDEN_TOTAL_AFTER_NON_ZERO");
    }
    if report
        .pointer("/security/cross_zone_leakage_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        reasons.push("CROSS_ZONE_LEAKAGE_FOUND");
    }
    if report
        .pointer("/security/access_level_violation_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        reasons.push("ACCESS_LEVEL_VIOLATION_FOUND");
    }
    if report
        .pointer("/confidence_gate/timeout_triggered")
        .and_then(Value::as_bool)
        == Some(true)
    {
        reasons.push("CONFIDENCE_GATE_TIMEOUT");
    }
    if report.pointer("/baseline/valid").and_then(Value::as_bool) != Some(true) {
        reasons.push("BASELINE_FILE_INVALID");
    }
    if reasons.is_empty() {
        ("PASS", reasons)
    } else {
        ("FAIL", reasons)
    }
}

fn pass_report() -> Value {
    json!({
        "preflight": {"endpoint_available": true},
        "confidence_gate": {"timeout_triggered": false},
        "baseline": {"valid": true},
        "profiles": {
            "dense": {"verdict": "PASS", "runtime_execution": "MODEL_BACKED_E2E_CONFIRMED", "blocked": false},
            "sparse": {"verdict": "PASS", "runtime_execution": "MODEL_BACKED_E2E_CONFIRMED", "blocked": false, "sparse_available": true},
            "hybrid": {"verdict": "PASS", "runtime_execution": "MODEL_BACKED_E2E_CONFIRMED", "blocked": false, "hybrid_available": true}
        },
        "hard_negative": {
            "false_positive_reduction_rate": 1.0,
            "target_reduction_rate": 0.5,
            "after_forbidden_total": 0,
            "max_allowed_forbidden_total": 0
        },
        "no_answer": {"enabled": true},
        "security": {"cross_zone_leakage_count": 0, "access_level_violation_count": 0}
    })
}

#[test]
fn pass_case() {
    let (verdict, reasons) = confidence_verdict(&pass_report(), false);
    assert_eq!(verdict, "PASS");
    assert!(reasons.is_empty());
}

#[test]
fn fail_endpoint_unavailable() {
    let mut report = pass_report();
    report["preflight"]["endpoint_available"] = json!(false);
    let (verdict, reasons) = confidence_verdict(&report, false);
    assert_eq!(verdict, "FAIL");
    assert!(reasons.contains(&"ENDPOINT_UNAVAILABLE"));
}

#[test]
fn fail_skipped_profile() {
    let mut report = pass_report();
    report["profiles"]["dense"]["runtime_execution"] = json!("SKIPPED_ENDPOINT_NOT_SET");
    let (verdict, reasons) = confidence_verdict(&report, false);
    assert_eq!(verdict, "FAIL");
    assert!(reasons.contains(&"PROFILE_SKIPPED"));
}

#[test]
fn fail_sparse_blocked() {
    let mut report = pass_report();
    report["profiles"]["sparse"]["blocked"] = json!(true);
    let (verdict, reasons) = confidence_verdict(&report, false);
    assert_eq!(verdict, "FAIL");
    assert!(reasons.contains(&"SPARSE_PROFILE_BLOCKED"));
}

#[test]
fn fail_hybrid_blocked() {
    let mut report = pass_report();
    report["profiles"]["hybrid"]["blocked"] = json!(true);
    let (verdict, reasons) = confidence_verdict(&report, false);
    assert_eq!(verdict, "FAIL");
    assert!(reasons.contains(&"HYBRID_PROFILE_BLOCKED"));
}

#[test]
fn fail_hard_negative_target_not_met() {
    let mut report = pass_report();
    report["hard_negative"]["false_positive_reduction_rate"] = json!(0.25);
    let (verdict, reasons) = confidence_verdict(&report, false);
    assert_eq!(verdict, "FAIL");
    assert!(reasons.contains(&"HARD_NEGATIVE_TARGET_NOT_MET"));
}

#[test]
fn fail_forbidden_total_after_non_zero() {
    let mut report = pass_report();
    report["hard_negative"]["after_forbidden_total"] = json!(1);
    let (verdict, reasons) = confidence_verdict(&report, false);
    assert_eq!(verdict, "FAIL");
    assert!(reasons.contains(&"FORBIDDEN_TOTAL_AFTER_NON_ZERO"));
}

#[test]
fn diagnostic_only_case() {
    let mut report = pass_report();
    report["preflight"]["endpoint_available"] = json!(false);
    let (verdict, reasons) = confidence_verdict(&report, true);
    assert_eq!(verdict, "DIAGNOSTIC_ONLY");
    assert!(reasons.is_empty());
}
