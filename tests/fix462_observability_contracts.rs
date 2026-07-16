#[test]
fn fix462_alerts_reference_existing_metrics_and_runbooks() {
    let alerts = std::fs::read_to_string("docs/ALERTS.md").expect("read alerts doc");
    let lifecycle = std::fs::read_to_string("src/lifecycle/mod.rs").expect("read lifecycle");
    let grpc = std::fs::read_to_string("src/grpc/mod.rs").expect("read grpc");
    let qdrant = std::fs::read_to_string("src/qdrant/mod.rs").expect("read qdrant");
    let docs = format!("{alerts}\n{lifecycle}\n{grpc}\n{qdrant}");
    for metric in [
        "qdrant_cleanup_extra_points_detected_total",
        "qdrant_cleanup_extra_points_deleted_total",
        "qdrant_cleanup_extra_points_skipped_legal_hold_total",
        "qdrant_cleanup_orphan_points_deleted_total",
        "index_ttl_cleanup_concurrent_state_change_total",
        "index_ttl_delete_operation_conflict_total",
        "document_lifecycle_update_blocked_by_delete_operation_total",
        "retrieve_context_final_visibility_dropped_total",
        "qdrant_search_rejected_total",
        "graph_mmr_token_fallback_total",
    ] {
        assert!(
            docs.contains(metric),
            "metric {metric} must exist in code/docs"
        );
        assert!(alerts.contains(metric), "ALERTS.md must mention {metric}");
    }
    assert!(
        alerts.contains("Runbook"),
        "ALERTS.md must include runbook guidance"
    );
}
