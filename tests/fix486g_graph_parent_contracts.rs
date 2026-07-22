use serde_json::Value;
use std::{fs, process::Command};

const GRAPH: &str = "src/graph/mod.rs";
const GRPC: &str = "src/grpc/mod.rs";
const PERSISTENCE: &str = "src/persistence/mod.rs";
const PROTO: &str = "proto/astravector_embedding.proto";

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn require_all(path: &str, required: &[&str], defect: &str) {
    let text = source(path);
    let missing = required
        .iter()
        .filter(|needle| !text.contains(**needle))
        .copied()
        .collect::<Vec<_>>();
    assert!(missing.is_empty(), "{defect}: {path} missing {missing:?}");
}

#[test]
fn related_child_hydrates_its_own_canonical_parent_in_one_batch() {
    require_all(
        PERSISTENCE,
        &[
            "p.id=COALESCE(c.parent_chunk_id,c.id)",
            "p.document_id=c.document_id",
            "p.document_version=c.document_version",
            "fetch_contexts_for_graph_related_chunks_multi",
            "c.id=ANY($2::uuid[])",
        ],
        "FIX486G-PARENT-001",
    );
}

#[test]
fn canonical_graph_hydration_requires_a_synced_binding() {
    require_all(
        PERSISTENCE,
        &[
            "JOIN astravector.vector_bindings_v004 b",
            "b.qdrant_sync_status='SYNCED'",
            "b.document_id=c.document_id",
            "b.document_version=c.document_version",
            "b.parent_chunk_id IS NOT DISTINCT FROM c.parent_chunk_id",
        ],
        "FIX486G-P0-001",
    );
}

#[test]
fn fixture_relation_ingestion_can_scope_exact_child_granularity() {
    require_all(
        PERSISTENCE,
        &[
            "from_granularity",
            "to_granularity",
            "s_chunk.granularity=$7",
            "t_chunk.granularity=$8",
        ],
        "FIX486G-P1-004",
    );
}

#[test]
fn graph_expansion_preserves_stable_edge_and_endpoint_identity() {
    require_all(
        GRAPH,
        &[
            "pub edge_id: Uuid",
            "pub relation_identity: String",
            "pub relation_source: String",
            "pub related_document_id: Uuid",
            "pub related_document_version: i64",
        ],
        "FIX486G-P1-001",
    );
    require_all(
        PERSISTENCE,
        &[
            "expanded.edge_id",
            "expanded.relation_identity",
            "n.document_id",
            "n.document_version",
        ],
        "FIX486G-P1-001",
    );
}

#[test]
fn graph_result_exposes_complete_protected_provenance() {
    require_all(
        GRPC,
        &[
            "graph_seed_parent_chunk_id",
            "graph_relation_id",
            "graph_edge_id",
            "graph_related_chunk_id",
            "graph_related_parent_chunk_id",
            "graph_related_document_id",
            "graph_related_document_version",
            "graph_hop_distance",
            "GRAPH_EXPANDED",
        ],
        "FIX486G-PROVENANCE-001",
    );
}

#[test]
fn self_edges_are_rejected_before_graph_attribution() {
    require_all(
        PERSISTENCE,
        &["n.chunk_id <> expanded.seed_chunk_id"],
        "FIX486G-P1-003",
    );
}

#[test]
fn invalid_graph_candidate_cannot_exhaust_the_final_window() {
    require_all(
        GRPC,
        &[
            "graph_hydration_fetch_limit",
            "hydration_rejection_reserve",
            "graph_related_contexts_max",
            "graph_expansion_result_limit",
        ],
        "FIX486G-P1-002",
    );
}

#[test]
fn graph_expansion_is_zone_scoped_at_edge_hydration_and_final_visibility() {
    require_all(
        PERSISTENCE,
        &[
            "s.access_zone_id=e.access_zone_id",
            "n.access_zone_id=expanded.access_zone_id",
            "c.access_zone_id=ANY($1::uuid[])",
        ],
        "FIX486G-ZONE-001",
    );
    require_all(
        GRPC,
        &[
            "filter_visible_chunk_ids_multi",
            "visible.contains(&(zone_id, chunk_id))",
        ],
        "FIX486G-ZONE-001",
    );
}

#[test]
fn inactive_deleted_expired_and_quarantined_targets_are_filtered() {
    require_all(
        PERSISTENCE,
        &[
            "n.lifecycle_status='ACTIVE'",
            "n.quarantined=false",
            "c.lifecycle_status='ACTIVE'",
            "c.deleted_at IS NULL",
            "d.status='ACTIVE'",
            "d.lifecycle_status='ACTIVE'",
            "d.expires_at IS NULL OR d.expires_at > now()",
        ],
        "FIX486G-LIFECYCLE-001",
    );
}

#[test]
fn graph_disabled_request_cannot_enter_expansion_branch() {
    require_all(
        GRPC,
        &[
            "r.enable_graph_expansion",
            "self.cfg.graph_rag.enabled",
            "GRAPH_EXPANSION_CALL",
        ],
        "FIX486G-DISABLED-001",
    );
}

#[test]
fn graph_path_is_one_hop_and_non_recursive() {
    require_all(
        PERSISTENCE,
        &["expand_chunks_1hop_by_seed_keys", "hop_distance: 1"],
        "FIX486G-HOP-001",
    );
    let persistence = source(PERSISTENCE);
    assert!(
        !persistence.contains("WITH RECURSIVE seed_keys"),
        "FIX486G-HOP-001: production one-hop expansion became recursive"
    );
}

#[test]
fn search_and_retrieve_context_share_graph_pipeline() {
    require_all(
        GRPC,
        &[
            "<Self as AstraVectorV004Control>::search(self, inner)",
            "RetrievalEntryPoint(\"RetrieveContext\")",
            "enable_graph_expansion: r.enable_graph_expansion",
        ],
        "FIX486G-PARITY-001",
    );
}

#[test]
fn graph_proof_does_not_add_public_failpoint_controls() {
    let proto = source(PROTO);
    for forbidden in [
        "graph_fault_plan",
        "graph_failpoint",
        "wrong_parent_overlay",
    ] {
        assert!(
            !proto.contains(forbidden),
            "FIX486G-FAILPOINT-PUBLIC-001: protobuf exposes {forbidden}"
        );
    }
}

#[test]
fn supplemental_bank_is_frozen_complete_and_hash_verified() {
    let output = tempfile::NamedTempFile::new().unwrap();
    let status = Command::new("python3")
        .args([
            "scripts/fix486g_proof.py",
            "verify-supplemental",
            "--bank",
            "benchmarks/hierarchical/fix486g-supplemental",
            "--output",
            output.path().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let report: Value = serde_json::from_str(&fs::read_to_string(output.path()).unwrap()).unwrap();
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["version"], "1.0.0");
    assert_eq!(report["bank_status"], "FROZEN");
    assert_eq!(report["query_count"], 71);
    assert_eq!(report["qrel_assignment_count"], 71);
}

#[test]
fn runner_is_phase_owned_fail_closed_and_graph_specific() {
    let runner = source("scripts/fix486g-graph-parent-runtime-proof.sh");
    for required in [
        "FIX486G_RUN_ID",
        "docker-compose.fix486g.yml",
        "application-fix486g.yaml",
        "FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS",
        "graph-disabled-control",
        "graph-audit",
        "graph-provenance-trace.json",
        "graph-identity-chain.json",
        "enableGraphExpansion:$graph",
        "graphMaxHops:1",
        "cleanup/summary.json",
        "manifest-verification.json",
        "REMOTE_SHA",
        "local_remote_equal",
    ] {
        assert!(runner.contains(required), "runner missing {required}");
    }
    assert!(runner.contains("59432"));
    assert!(runner.contains("50588"));
    assert!(!runner.contains("docker-compose.fix486f.yml"));
    assert!(!runner.contains("/Users/"));
    assert!(runner.contains("WORKSPACE_ROOT"));
}

#[test]
fn graph_faults_preserve_an_independent_valid_parent_survivor() {
    let runner = source("scripts/fix486g-graph-parent-runtime-proof.sh");
    for required in [
        "child-a3-260",
        "child_a3_alt",
        "insert_fault_edge \"$edge\" \"$source\" \"$survivor\" REPAIRED_BY",
        "run_control_pair wrong-parent present \"$child\"",
        "run_control_pair binding-invalid present \"$child\"",
        "run_control_pair \"$kind-target\" present \"$child\"",
    ] {
        assert!(
            runner.contains(required),
            "FIX486G-CANDIDATE-NON-INTERFERENCE: runner missing {required}"
        );
    }
}

#[test]
fn canonical_graph_audit_scopes_chunk_endpoint_invariants_to_chunk_edges() {
    let audit = source("scripts/fix486g-graph-parent-audit.sql");
    require_all(
        "scripts/fix486g-graph-parent-audit.sql",
        &[
            "e.source_node_type='CHUNK'",
            "e.target_node_type='CHUNK'",
            "orphan_graph_endpoints",
            "cross_zone_graph_edges",
        ],
        "FIX486G-AUDIT-001",
    );
    assert_eq!(audit.matches("graph_endpoints AS").count(), 1);
}

#[test]
fn graph_seed_identity_survives_parent_context_deduplication() {
    require_all(
        GRPC,
        &[
            "pre_parent_dedup_graph_seed_results",
            "matches!(granularity, \"SUB_180\" | \"SUB_260\")",
            "graph_seed_source_results_for_admitted_parents(",
            "&pre_parent_dedup_graph_seed_results,",
        ],
        "FIX486G-P0-002",
    );
}

#[test]
fn graph_seed_selection_keeps_all_hydrated_children_of_each_admitted_parent_group() {
    require_all(
        GRPC,
        &[
            "graph_seed_source_results_for_admitted_parents",
            "pre_parent_dedup_results",
            "admitted_parents.contains(&parent_key)",
            "seen_children.insert(child_key)",
            "parents_with_children",
        ],
        "FIX486G-P0-002",
    );
    assert!(
        !GRPC.contains("child_seed_by_parent"),
        "FIX486G-P0-002: one child per parent makes Graph relation discovery depend on a nondeterministic granularity winner"
    );
}

#[test]
fn make_targets_share_one_official_execute_path() {
    let makefile = source("Makefile");
    assert!(makefile.contains("verify-fix486g-graph-parent-runtime:"));
    assert!(makefile.contains("verify-fix486g-graph-parent-runtime-proof:"));
    assert_eq!(
        makefile
            .matches("./scripts/fix486g-graph-parent-runtime-proof.sh --execute-all")
            .count(),
        1
    );
}

#[test]
fn production_source_has_no_phase_fixture_specific_branching() {
    for path in [GRAPH, GRPC, PERSISTENCE] {
        let text = source(path);
        for forbidden in [
            "q-graph-repair",
            "FIX486-08",
            "parent-a3",
            "graph-a1-repaired",
        ] {
            assert!(
                !text.contains(forbidden),
                "{path} contains fixture-specific production token {forbidden}"
            );
        }
    }
}
