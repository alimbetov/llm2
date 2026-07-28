use std::fs;

const RUNNER: &str = "scripts/fix486g-graph-parent-runtime-proof.sh";
const AUDIT: &str = "scripts/fix486g-graph-parent-audit.sql";

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("cannot read {path}: {error}"))
}

fn require_all(text: &str, required: &[&str], contract: &str) {
    let missing = required
        .iter()
        .filter(|item| !text.contains(**item))
        .copied()
        .collect::<Vec<_>>();
    assert!(missing.is_empty(), "{contract}: missing {missing:?}");
}

#[test]
fn fault_mutations_require_exact_rows_and_verified_activation() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "run_exact_mutation()",
            "WITH affected AS",
            "-v ON_ERROR_STOP=1",
            "expected_rows",
            "actual_rows",
            "verify_sql_count",
            "activation_rows",
            "FAULT_MUTATION_ROW_COUNT_MISMATCH",
            "FAULT_ACTIVATION_MISMATCH",
            "faults/mutations",
        ],
        "FIX486G-RUNNER-MUTATION-001",
    );
    assert!(
        !runner.contains("psql \"$DB\" -Atqc \"UPDATE astravector"),
        "FIX486G-RUNNER-MUTATION-001: direct UPDATE bypasses exact-row helper"
    );
    assert!(
        !runner.contains("psql \"$DB\" -Atqc \"INSERT INTO astravector"),
        "FIX486G-RUNNER-MUTATION-001: direct INSERT bypasses exact-row helper"
    );
}

#[test]
fn every_fault_kind_has_activation_and_restoration_evidence() {
    let runner = source(RUNNER);
    for label in [
        "wrong-parent-activate",
        "wrong-parent-restore",
        "binding-invalid-activate",
        "binding-invalid-restore",
        "inactive-activate",
        "deleted-activate",
        "expired-activate",
        "insert-fault-edge",
        "delete-fault-edge",
    ] {
        assert!(
            runner.contains(label),
            "FIX486G-RUNNER-MUTATION-002: missing mutation evidence label {label}"
        );
    }
}

#[test]
fn cleanup_proves_restoration_before_database_teardown() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "restore_fault_state_before_teardown",
            "cleanup/restoration.json",
            "active_fault_rows",
            "restoration_matches_baseline",
            "FAULT_RESTORATION_FAILED",
            "stage pre-teardown-fault-restoration",
        ],
        "FIX486G-RUNNER-CLEANUP-001",
    );
    let cleanup = runner.find("cleanup() {").expect("cleanup function");
    let restore = runner[cleanup..]
        .find("restore_fault_state_before_teardown")
        .expect("restoration call in cleanup");
    let teardown = runner[cleanup..]
        .find("compose down -v")
        .expect("compose teardown in cleanup");
    assert!(
        restore < teardown,
        "FIX486G-RUNNER-CLEANUP-001: restoration must run before compose teardown"
    );
}

#[test]
fn fault_cleanup_restores_the_exact_baseline_ttl() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "baseline_expires=$(jq -r '.expires_at // empty'",
            "expires_at=$expires_sql",
            "expires_at IS NOT DISTINCT FROM $expires_sql",
            "expires_at_visible",
        ],
        "FIX486G-RUNNER-CLEANUP-002",
    );
    assert!(
        !runner.contains("lifecycle_status='ACTIVE',deleted_at=NULL,expires_at=NULL"),
        "FIX486G-RUNNER-CLEANUP-002: cleanup must not erase a valid baseline TTL"
    );
}

#[test]
fn optional_fault_validator_arguments_are_bash_32_nounset_safe() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "local validate_args=(--identity-map",
            "validate_args+=(--rejection-evidence",
            "\"${validate_args[@]}\" --output",
            "run_rejected_target_pair cycle cycle \"$self\"",
        ],
        "FIX486G-RUNNER-BASH32-001",
    );
    assert!(
        !runner.contains("local extra=()"),
        "FIX486G-RUNNER-BASH32-001: Bash 3.2 with nounset cannot expand an empty optional array"
    );
    assert!(
        !runner.contains("\"${extra[@]}\""),
        "FIX486G-RUNNER-BASH32-001: optional empty array expansion must not terminate the proof"
    );
}

#[test]
fn fault_validation_is_table_driven_and_hop_scope_is_declared_centrally() {
    let runner = source(RUNNER);
    let contract = source("scripts/fix486g_fault_contract.py");
    require_all(
        &runner,
        &[
            "run_rejected_target_pair()",
            "validate-rejected-target",
            "run_rejected_target_pair hop-limit hop-limit \"$target\"",
        ],
        "FIX486G-RUNNER-HOP-SCOPE-001",
    );
    require_all(
        &contract,
        &[
            "\"hop-limit\"",
            "\"survivor_mode\": \"GRAPH\"",
            "\"forbidden_scope\": \"GRAPH\"",
            "\"expected_rejection_reason\": \"HOP_LIMIT_REJECTED\"",
        ],
        "FIX486G-RUNNER-HOP-SCOPE-001",
    );
}

#[test]
fn official_runner_requires_the_complete_statistical_campaign() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "statistical_campaign()",
            "for index in 1 2 3; do statistical_full_pass warm",
            "for index in 1 2; do",
            "statistical_full_pass restart",
            "for index in $(seq 1 10); do",
            "statistical_concurrent_pair",
            "-eq 730",
            "FIX486G_STATISTICAL_QUALITY_PASS",
            "stage statistical-campaign statistical_campaign",
            "stage post-statistical-canonical-audit canonical_audit",
            "statistical/raw-observations.jsonl",
        ],
        "FIX486G-RUNNER-STATISTICS-001",
    );
}

#[test]
fn signals_have_explicit_fail_closed_terminal_metadata() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "trap 'handle_signal INT 130' INT",
            "trap 'handle_signal TERM 143' TERM",
            "trap 'handle_signal HUP 129' HUP",
            "termination_reason",
            "signal",
            "cleanup_attempted",
            "cleanup_status",
            "UNEXPECTED_EXIT",
        ],
        "FIX486G-RUNNER-SIGNAL-001",
    );
}

#[test]
fn official_identity_gate_requires_an_approved_phase_branch_and_remote_sha() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "branch_is_approved()",
            "codex/fix486g-graph-parent-proof|codex/fix486g-finalize-runtime-evidence|codex/fix486g-post-merge-recovery",
            "branch_is_approved &&",
            "[[ \"$SOURCE_SHA\" == \"$REMOTE_SHA\" ]]",
            "git -C \"$ROOT\" status --porcelain",
        ],
        "FIX486G-RUNNER-IDENTITY-001",
    );
}

#[test]
fn fault_controls_use_a_bounded_raw_window_that_can_hold_attack_and_survivor() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "FAULT_GRAPH_RELATED_CONTEXTS=10",
            "graphMaxRelatedContexts:$graph_limit",
            "run_rejected_target_pair wrong-parent wrong-parent \"$child\"",
            "run_rejected_target_pair binding-invalid binding-status \"$child\"",
            "run_rejected_target_pair \"$kind-target\" \"$kind-target\" \"$child\"",
            "run_rejected_target_pair missing-parent missing-parent \"$child\"",
        ],
        "FIX486G-RUNNER-NON-VACUOUS-001",
    );
    assert!(
        runner.contains("((FAULT_GRAPH_RELATED_CONTEXTS <= 20))"),
        "FIX486G-RUNNER-NON-VACUOUS-001: fault window must remain bounded"
    );
}

#[test]
fn every_rejected_graph_target_uses_one_shared_contract_matrix() {
    let contract = source("scripts/fix486g_fault_contract.py");
    let proof = source("scripts/fix486g_proof.py");
    for scenario in [
        "wrong-parent",
        "binding-status",
        "inactive-target",
        "deleted-target",
        "expired-target",
        "missing-parent",
        "cross-zone",
        "hop-limit",
        "cycle",
        "candidate-non-interference",
    ] {
        assert!(
            contract.contains(&format!("\"{scenario}\"")),
            "missing fault contract for {scenario}"
        );
    }
    require_all(
        &proof,
        &[
            "validate_rejected_graph_target(",
            "forbidden_target_final_contexts",
            "forbidden_graph_provenance_credits",
            "valid_survivor_lost",
            "rejection_reason_mismatch",
            "relevant_hard_gates_zero",
        ],
        "FIX486G-RUNNER-SHARED-VALIDATOR-001",
    );
}

#[test]
fn maintenance_modes_are_explicit_and_cannot_emit_official_pass() {
    let runner = source(RUNNER);
    require_all(
        &runner,
        &[
            "--verify-identities",
            "--verify-contracts",
            "--cleanup-only",
            "--verify-evidence",
            "official_runtime_proof:false",
            "execute_all()",
            "case \"$MODE\" in",
        ],
        "FIX486G-RUNNER-MODES-001",
    );
    assert_eq!(
        runner
            .matches("FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS")
            .count(),
        2,
        "FIX486G-RUNNER-MODES-001: official PASS must remain confined to execute-all verdict construction and final assertion"
    );
}

#[test]
fn canonical_audit_distinguishes_documents_versions_and_relation_corruption() {
    let audit = source(AUDIT);
    let runner = source(RUNNER);
    require_all(
        &audit,
        &[
            "active_versions AS",
            "active_documents AS",
            "SELECT DISTINCT access_zone_id, document_id",
            "duplicate_graph_relations",
            "duplicate_graph_relation_ids",
            "cross_document_graph_relations",
            "cross_version_graph_relations",
            "source_document_version",
            "target_document_version",
        ],
        "FIX486G-AUDIT-HARDENING-001",
    );
    assert_ne!(
        audit
            .find("'active_documents'")
            .and_then(|start| audit[start..].find("FROM active_documents")),
        None,
        "FIX486G-AUDIT-HARDENING-001: active document count is not sourced from distinct documents"
    );
    assert_ne!(
        audit
            .find("'active_versions'")
            .and_then(|start| audit[start..].find("FROM active_versions")),
        None,
        "FIX486G-AUDIT-HARDENING-001: active version count is not sourced from active versions"
    );
    require_all(
        &runner,
        &[
            ".duplicate_graph_relations",
            ".duplicate_graph_relation_ids",
            ".cross_document_graph_relations",
            ".cross_version_graph_relations",
        ],
        "FIX486G-AUDIT-HARDENING-002",
    );
}
