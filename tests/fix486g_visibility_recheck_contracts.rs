use std::fs;

const GRPC: &str = "src/grpc/mod.rs";
const PERSISTENCE: &str = "src/persistence/mod.rs";

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
fn final_visibility_recheck_uses_complete_typed_result_identity() {
    require_all(
        PERSISTENCE,
        &[
            "pub struct FinalVisibilityCandidate",
            "pub access_zone_id: Uuid",
            "pub matched_chunk_id: Uuid",
            "pub parent_chunk_id: Uuid",
            "pub binding_id: Option<Uuid>",
            "filter_visible_search_results_batch",
        ],
        "FIX486G-VISIBILITY-TOCTOU-001",
    );
}

#[test]
fn final_visibility_recheck_is_one_atomic_ordinality_batch() {
    require_all(
        PERSISTENCE,
        &[
            "WITH candidate_keys AS",
            "unnest($1::uuid[], $2::uuid[], $3::uuid[], $4::uuid[])",
            "WITH ORDINALITY AS keys",
            "result_ordinal",
            "keys.binding_id IS NULL OR",
        ],
        "FIX486G-VISIBILITY-BATCH-001",
    );

    let persistence = source(PERSISTENCE);
    assert!(
        !persistence.contains("for candidate in candidates { self.filter_visible"),
        "FIX486G-VISIBILITY-BATCH-001: final visibility recheck became N+1"
    );
}

#[test]
fn final_visibility_recheck_revalidates_zone_document_child_and_parent() {
    require_all(
        PERSISTENCE,
        &[
            "JOIN astravector.access_zones az",
            "az.status='ACTIVE'",
            "JOIN astravector.content_chunks_v004 m",
            "JOIN astravector.content_chunks_v004 p",
            "p.id=COALESCE(m.parent_chunk_id,m.id)",
            "m.document_id=p.document_id",
            "m.document_version=p.document_version",
            "m.access_level <= $5",
            "p.access_level <= $5",
            "m.lifecycle_status='ACTIVE'",
            "p.lifecycle_status='ACTIVE'",
            "m.deleted_at IS NULL",
            "p.deleted_at IS NULL",
            "m.expires_at IS NULL OR m.expires_at > now()",
            "p.expires_at IS NULL OR p.expires_at > now()",
            "d.status='ACTIVE'",
            "d.lifecycle_status='ACTIVE'",
            "d.delete_operation_id IS NULL",
        ],
        "FIX486G-VISIBILITY-TOCTOU-002",
    );
}

#[test]
fn final_visibility_recheck_requires_carried_binding_to_remain_active_and_synced() {
    require_all(
        PERSISTENCE,
        &[
            "LEFT JOIN astravector.vector_bindings_v004 b",
            "b.id=keys.binding_id",
            "b.chunk_id=m.id",
            "b.parent_chunk_id IS NOT DISTINCT FROM m.parent_chunk_id",
            "b.lifecycle_status='ACTIVE'",
            "b.qdrant_sync_status='SYNCED'",
        ],
        "FIX486G-VISIBILITY-BINDING-001",
    );
}

#[test]
fn grpc_final_recheck_wires_full_identity_and_retains_by_ordinal() {
    require_all(
        GRPC,
        &[
            "FinalVisibilityCandidate",
            "filter_visible_search_results_batch",
            "parent_chunk_id: Uuid::parse_str(&result.parent_chunk_id)",
            "binding_id: result",
            ".metadata.get(\"binding_id\")",
            "visible_ordinals.contains(&ordinal)",
        ],
        "FIX486G-VISIBILITY-WIRING-001",
    );
}
