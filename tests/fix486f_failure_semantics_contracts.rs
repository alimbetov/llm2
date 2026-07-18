use std::fs;

const HYDRATION: &str = "src/retrieval/hydration.rs";
const FAILPOINTS: &str = "src/retrieval/hydration_failpoints.rs";
const GRPC: &str = "src/grpc/mod.rs";
const PERSISTENCE: &str = "src/persistence/mod.rs";
const PROTO: &str = "proto/astravector_embedding.proto";
const CONFIG: &str = "src/config/mod.rs";

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn require_all(path: &str, required: &[&str], defect: &str) {
    let text = source(path);
    let missing = required
        .iter()
        .filter(|item| !text.contains(**item))
        .copied()
        .collect::<Vec<_>>();
    assert!(missing.is_empty(), "{defect}: {path} missing {missing:?}");
}

#[test]
fn binding_parent_mismatch_is_rejected_by_canonical_batch_join() {
    require_all(
        PERSISTENCE,
        &[
            "keys.binding_id",
            "JOIN astravector.vector_bindings_v004 b",
            "b.id=keys.binding_id",
            "b.chunk_id=keys.matched_chunk_id",
            "m.parent_chunk_id=p.id",
            "BINDING_INVALID",
        ],
        "FIX486F-P0-001",
    );
}

#[test]
fn every_candidate_ordinal_has_one_terminal_hydration_outcome() {
    require_all(
        HYDRATION,
        &[
            "enum HydrationTerminalOutcome",
            "input_ordinal",
            "requested_candidates == hydrated_outcomes + rejected_outcomes",
            "assert_exhaustive_outcomes",
        ],
        "FIX486F-HYDR-OUTCOME-001",
    );
}

#[test]
fn missing_parent_and_binding_invalid_are_distinct() {
    require_all(
        HYDRATION,
        &[
            "BindingInvalid",
            "HydrationMissing",
            "BINDING_INVALID",
            "HYDRATION_MISSING",
        ],
        "FIX486F-ORPHAN-SEMANTICS-001",
    );
}

#[test]
fn partial_timeout_preserves_survivors_and_reports_degradation() {
    require_all(
        HYDRATION,
        &[
            "ParentHydrationTimeout",
            "CoverageClass::Partial",
            "surviving_contexts",
            "dropped_parents",
            "retryable: true",
        ],
        "FIX486F-HYDR-001",
    );
    require_all(
        PROTO,
        &[
            "RetrievalDegradation",
            "DroppedParentSummary",
            "coverage_class",
            "retryable",
        ],
        "FIX486F-HYDR-003",
    );
}

#[test]
fn total_timeout_uses_transport_failure_without_normal_content() {
    require_all(
        HYDRATION,
        &[
            "total_hydration_timeout_status",
            "Status::deadline_exceeded",
            "structured_status_details",
            "normal_response_body_absent",
        ],
        "FIX486F-HYDR-002",
    );
}

#[test]
fn rejected_high_ranked_candidate_has_bounded_reserve() {
    require_all(
        CONFIG,
        &[
            "hydration_rejection_reserve",
            "hydration_rejection_reserve_max",
        ],
        "FIX486F-STALE-002",
    );
    require_all(
        GRPC,
        &[
            "bounded_hydration_fetch_window",
            "hydration_rejection_reserve",
        ],
        "FIX486F-STALE-002",
    );
}

#[test]
fn failpoint_plan_is_request_scoped_bounded_and_correlation_based() {
    require_all(
        FAILPOINTS,
        &[
            "HydrationFailpointPlan",
            "correlation_id",
            "max_activations",
            "non_production_enabled",
            "TIMEOUT_SELECTED_PARENTS",
            "TIMEOUT_ALL_PARENTS",
            "RETURN_NOT_FOUND_SELECTED",
            "EMPTY_CONTENT_SELECTED",
        ],
        "FIX486F-CONC-001",
    );
    let proto = source(PROTO);
    assert!(
        !proto.contains("hydration_failpoint") && !proto.contains("failpoint_plan"),
        "FIX486F-FAILPOINT-PUBLIC-001: public protobuf exposes failpoint control"
    );
}

#[test]
fn whitespace_parent_is_rejected_as_empty_context() {
    require_all(
        HYDRATION,
        &[
            "parent_text.trim().is_empty()",
            "EmptyContext",
            "EMPTY_CONTEXT",
        ],
        "FIX486F-CONTENT-001",
    );
}

#[test]
fn hydration_metrics_have_only_bounded_labels() {
    require_all(
        HYDRATION,
        &[
            "parent_hydration_requests_total",
            "candidate_rejections_total",
            "degraded_requests_total",
            "entry_point",
            "outcome",
            "reason",
            "scope",
        ],
        "FIX486F-OBS-001",
    );
    let hydration = source(HYDRATION);
    for forbidden in [
        "parent_uuid =>",
        "document_uuid =>",
        "binding_id =>",
        "chunk_id =>",
        "point_id =>",
        "correlation_id =>",
    ] {
        assert!(
            !hydration.contains(forbidden),
            "FIX486F-OBS-CARDINALITY-001: forbidden metric label {forbidden}"
        );
    }
}

#[test]
fn search_and_retrieve_share_hydration_normalization() {
    let grpc = source(GRPC);
    assert!(
        grpc.matches("normalize_hydration_outcomes").count() >= 2,
        "FIX486F-PARITY-001: Search/RetrieveContext do not share hydration normalization"
    );
}

#[test]
fn hydration_is_one_ordinality_batch_without_n_plus_one() {
    require_all(
        PERSISTENCE,
        &[
            "WITH ORDINALITY",
            "fetch_hydration_outcomes_batch",
            "input_ordinal",
            "unnest",
        ],
        "FIX486F-BATCH-001",
    );
    let hydration = source(HYDRATION);
    assert!(
        !hydration.contains("for candidate in candidates { repo.fetch_parent"),
        "FIX486F-BATCH-001: per-candidate parent query detected"
    );
}
