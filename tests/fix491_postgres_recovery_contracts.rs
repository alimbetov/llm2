use astravector_runtime::recovery::postgres::{
    compare_migration_history, repository_migrations_from_dir, AppliedMigration,
    CanonicalIntegrityAudit, PostgresRecoveryAuditPolicy, RepositoryMigration,
};
use sha2::{Digest, Sha384};

fn migration(version: i64, checksum_hex: &str) -> RepositoryMigration {
    RepositoryMigration {
        version,
        description: format!("migration {version}"),
        checksum_hex: checksum_hex.to_string(),
    }
}

fn applied(version: i64, success: bool, checksum_hex: &str) -> AppliedMigration {
    AppliedMigration {
        version,
        success,
        checksum_hex: checksum_hex.to_string(),
    }
}

#[tokio::test]
async fn fix491_repository_migration_checksum_uses_sqlx_sha384_semantics() {
    let temp = tempfile::tempdir().expect("tempdir");
    let sql = "CREATE TABLE demo(id bigint PRIMARY KEY);\n";
    std::fs::write(temp.path().join("0001_create_demo.sql"), sql).expect("write migration");

    let migrations = repository_migrations_from_dir(temp.path())
        .await
        .expect("resolve migrations");
    let expected = hex::encode(Sha384::digest(sql.as_bytes()));

    assert_eq!(migrations.len(), 1);
    assert_eq!(migrations[0].version, 1);
    assert_eq!(migrations[0].checksum_hex, expected);
    assert_eq!(
        migrations[0].checksum_hex.len(),
        96,
        "SQLx stores SHA-384 checksums as 48 bytes, not SHA-256"
    );
}

#[test]
fn fix491_migration_history_fails_on_checksum_mismatch() {
    let audit = compare_migration_history(
        &[migration(1, "aaaa"), migration(2, "bbbb")],
        &[applied(1, true, "aaaa"), applied(2, true, "cccc")],
    );

    assert_eq!(audit.blocking_total(), 1);
    assert_eq!(audit.checksum_mismatches.len(), 1);
    assert_eq!(audit.checksum_mismatches[0].version, 2);
    assert_eq!(audit.pending_migrations, Vec::<i64>::new());
    assert_eq!(audit.unknown_migrations, Vec::<i64>::new());
}

#[test]
fn fix491_migration_history_fails_on_unknown_pending_and_failed_rows() {
    let audit = compare_migration_history(
        &[migration(1, "aaaa"), migration(2, "bbbb")],
        &[applied(1, false, "aaaa"), applied(99, true, "zzzz")],
    );

    assert_eq!(audit.failed_migrations, 1);
    assert_eq!(audit.unknown_migrations, vec![99]);
    assert_eq!(audit.pending_migrations, vec![2]);
    assert_eq!(audit.blocking_total(), 3);
}

#[test]
fn fix491_canonical_integrity_blocks_any_partial_or_orphan_state() {
    let audit = CanonicalIntegrityAudit {
        partial_active_documents: 1,
        orphan_chunks: 2,
        orphan_bindings: 3,
        orphan_outbox: 4,
        orphan_graph_nodes: 5,
        orphan_graph_edges: 6,
        duplicate_chunks: 7,
        duplicate_bindings: 8,
        duplicate_outbox_events: 9,
        active_searchable_bindings_missing_dense: 10,
        active_searchable_bindings_missing_sparse: 11,
        dead_or_failed_outbox: 12,
    };

    assert_eq!(
        audit.blocking_total(),
        67,
        "optional sparse gaps are diagnostic, not blocking"
    );
    assert_eq!(
        audit.blocking_total_with_policy(PostgresRecoveryAuditPolicy {
            sparse_required: true,
        }),
        78,
        "required sparse profile treats missing sparse vectors as persistence corruption"
    );
}

#[test]
fn fix491_full_proof_is_fail_closed_until_all_lanes_run() {
    let main = std::fs::read_to_string("src/main.rs").expect("read main");

    assert!(main.contains("\"FIX491_PERSISTENCE_RECOVERY_BLOCKED\""));
    assert!(main.contains("postgres_bootstrap_proof != \"NOT_RUN\""));
    assert!(main.contains("retrieval_parity != \"NOT_RUN\""));
    assert!(
        !main.contains("FIX491_QDRANT_RECOVERY_PARTIAL_PASS"),
        "full-proof must not expose a top-level partial PASS"
    );
}
