#![cfg(feature = "integration-tests")]

use astravector_runtime::{
    error::AstraError,
    persistence::Repository,
    recovery::{
        acquire_qdrant_projection_write_fence, acquire_qdrant_recovery_exclusive_fence,
        postgres::postgres_recovery_audit,
    },
};
use sqlx::PgPool;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};

async fn postgres_pool() -> PgPool {
    let postgres = GenericImage::new("pgvector/pgvector", "pg16")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_DB", "astravector")
        .with_env_var("POSTGRES_USER", "astravector")
        .with_env_var("POSTGRES_PASSWORD", "astravector")
        .start()
        .await
        .expect("PostgreSQL testcontainer must start");
    let pg_port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("PostgreSQL mapped port");
    let database_url =
        format!("postgres://astravector:astravector@127.0.0.1:{pg_port}/astravector");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect PostgreSQL testcontainer");

    // Keep the container alive for the lifetime of the pool by leaking the handle in this
    // test process. These tests run in short-lived isolated processes.
    Box::leak(Box::new(postgres));
    pool
}

#[tokio::test]
async fn fix491_clean_postgres_bootstrap_applies_all_migrations_and_passes_audit() {
    let pool = postgres_pool().await;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("all migrations must apply on a clean DB");
    let repo = Repository { pool };

    let audit = postgres_recovery_audit(&repo, std::path::Path::new("migrations"))
        .await
        .expect("postgres recovery audit");

    assert_eq!(audit.verdict, "POSTGRES_CANONICAL_AUDIT_PASS");
    assert_eq!(audit.migration_history.repository_migration_count, 39);
    assert_eq!(audit.migration_history.applied_migration_count, 39);
    assert_eq!(audit.migration_history.failed_migrations, 0);
    assert!(audit.migration_history.unknown_migrations.is_empty());
    assert!(audit.migration_history.pending_migrations.is_empty());
    assert!(audit.migration_history.checksum_mismatches.is_empty());
    assert_eq!(audit.canonical_integrity.blocking_total(), 0);
    assert!(!audit.schema_inventory.items.is_empty());
    assert_eq!(audit.schema_inventory.sha256.len(), 64);

    for table in [
        "document_versions",
        "content_chunks_v004",
        "vector_bindings_v004",
    ] {
        let partitions: i64 = sqlx::query_scalar(
            r#"
SELECT count(*)
FROM pg_inherits i
JOIN pg_class parent ON parent.oid=i.inhparent
JOIN pg_namespace n ON n.oid=parent.relnamespace
WHERE n.nspname='astravector'
  AND parent.relname=$1
"#,
        )
        .bind(table)
        .fetch_one(&repo.pool)
        .await
        .expect("count static hash partitions");
        assert_eq!(
            partitions, 32,
            "{table} must create all 32 static hash partitions during clean bootstrap"
        );
    }
}

#[tokio::test]
async fn fix491_recovery_fence_rejects_stale_recovery_and_projection_writer_until_release() {
    let pool = postgres_pool().await;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("all migrations must apply on a clean DB");

    let recovery_fence = acquire_qdrant_recovery_exclusive_fence(&pool)
        .await
        .expect("first recovery owns exclusive fence");

    let second_recovery = acquire_qdrant_recovery_exclusive_fence(&pool).await;
    assert!(
        matches!(second_recovery, Err(AstraError::ResourceExhausted(message)) if message.contains("QDRANT_RECOVERY_FENCE_BUSY")),
        "stale concurrent recovery must be rejected"
    );

    let projection_writer = acquire_qdrant_projection_write_fence(&pool).await;
    assert!(
        matches!(projection_writer, Err(AstraError::ResourceExhausted(message)) if message.contains("QDRANT_RECOVERY_FENCE_ACTIVE")),
        "normal projection writer must be rejected while recovery owns fence"
    );

    recovery_fence
        .release()
        .await
        .expect("exclusive recovery fence release");

    let writer_after_release = acquire_qdrant_projection_write_fence(&pool)
        .await
        .expect("projection writer succeeds after recovery release");
    writer_after_release
        .commit()
        .await
        .expect("projection fence transaction commit");
}
