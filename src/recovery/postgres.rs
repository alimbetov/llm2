use crate::persistence::Repository;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{migrate::Migrator, Column, PgPool, Row};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryMigration {
    pub version: i64,
    pub description: String,
    pub checksum_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedMigration {
    pub version: i64,
    pub success: bool,
    pub checksum_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationChecksumMismatch {
    pub version: i64,
    pub expected_checksum_hex: String,
    pub applied_checksum_hex: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationHistoryAudit {
    pub repository_migration_count: usize,
    pub applied_migration_count: usize,
    pub failed_migrations: u64,
    pub unknown_migrations: Vec<i64>,
    pub pending_migrations: Vec<i64>,
    pub checksum_mismatches: Vec<MigrationChecksumMismatch>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalIntegrityAudit {
    pub partial_active_documents: i64,
    pub orphan_chunks: i64,
    pub orphan_bindings: i64,
    pub orphan_outbox: i64,
    pub orphan_graph_nodes: i64,
    pub orphan_graph_edges: i64,
    pub duplicate_chunks: i64,
    pub duplicate_bindings: i64,
    pub duplicate_outbox_events: i64,
    pub active_searchable_bindings_missing_dense: i64,
    pub active_searchable_bindings_missing_sparse: i64,
    pub dead_or_failed_outbox: i64,
}

impl CanonicalIntegrityAudit {
    pub fn blocking_total(&self) -> u64 {
        self.blocking_total_with_policy(PostgresRecoveryAuditPolicy::default())
    }

    pub fn blocking_total_with_policy(&self, policy: PostgresRecoveryAuditPolicy) -> u64 {
        [
            self.partial_active_documents,
            self.orphan_chunks,
            self.orphan_bindings,
            self.orphan_outbox,
            self.orphan_graph_nodes,
            self.orphan_graph_edges,
            self.duplicate_chunks,
            self.duplicate_bindings,
            self.duplicate_outbox_events,
            self.active_searchable_bindings_missing_dense,
            self.dead_or_failed_outbox,
        ]
        .into_iter()
        .map(|value| value.max(0) as u64)
        .sum::<u64>()
            + if policy.sparse_required {
                self.active_searchable_bindings_missing_sparse.max(0) as u64
            } else {
                0
            }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaInventory {
    pub sha256: String,
    pub items: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalFingerprint {
    pub sha256: String,
    pub items: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostgresRecoveryAudit {
    pub verdict: String,
    pub migration_history: MigrationHistoryAudit,
    pub canonical_integrity: CanonicalIntegrityAudit,
    pub schema_inventory: SchemaInventory,
    pub canonical_fingerprint: CanonicalFingerprint,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostgresRecoveryAuditPolicy {
    pub sparse_required: bool,
}

impl MigrationHistoryAudit {
    pub fn blocking_total(&self) -> u64 {
        self.failed_migrations
            + self.unknown_migrations.len() as u64
            + self.pending_migrations.len() as u64
            + self.checksum_mismatches.len() as u64
    }
}

pub async fn repository_migrations_from_dir(
    migrations_dir: &Path,
) -> anyhow::Result<Vec<RepositoryMigration>> {
    let migrator = Migrator::new(migrations_dir)
        .await
        .with_context(|| format!("resolve migrations from {}", migrations_dir.display()))?;
    let mut migrations = migrator
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .map(|migration| RepositoryMigration {
            version: migration.version,
            description: migration.description.to_string(),
            checksum_hex: hex::encode(&migration.checksum),
        })
        .collect::<Vec<_>>();
    migrations.sort_by_key(|migration| migration.version);
    Ok(migrations)
}

pub fn compare_migration_history(
    expected: &[RepositoryMigration],
    applied: &[AppliedMigration],
) -> MigrationHistoryAudit {
    let expected_by_version = expected
        .iter()
        .map(|migration| (migration.version, migration))
        .collect::<HashMap<_, _>>();
    let applied_by_version = applied
        .iter()
        .map(|migration| (migration.version, migration))
        .collect::<HashMap<_, _>>();

    let mut unknown_migrations = Vec::new();
    let mut checksum_mismatches = Vec::new();
    let mut failed_migrations = 0_u64;
    for migration in applied {
        if !migration.success {
            failed_migrations += 1;
        }
        match expected_by_version.get(&migration.version) {
            Some(expected) if expected.checksum_hex != migration.checksum_hex => {
                checksum_mismatches.push(MigrationChecksumMismatch {
                    version: migration.version,
                    expected_checksum_hex: expected.checksum_hex.clone(),
                    applied_checksum_hex: migration.checksum_hex.clone(),
                });
            }
            Some(_) => {}
            None => unknown_migrations.push(migration.version),
        }
    }

    let mut pending_migrations = expected
        .iter()
        .filter(|migration| !applied_by_version.contains_key(&migration.version))
        .map(|migration| migration.version)
        .collect::<Vec<_>>();

    unknown_migrations.sort_unstable();
    pending_migrations.sort_unstable();
    checksum_mismatches.sort_by_key(|mismatch| mismatch.version);

    MigrationHistoryAudit {
        repository_migration_count: expected.len(),
        applied_migration_count: applied.len(),
        failed_migrations,
        unknown_migrations,
        pending_migrations,
        checksum_mismatches,
    }
}

pub async fn applied_migrations(pool: &PgPool) -> anyhow::Result<Vec<AppliedMigration>> {
    let rows =
        sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await
            .context("read _sqlx_migrations")?;
    Ok(rows
        .into_iter()
        .map(|row| AppliedMigration {
            version: row.get("version"),
            success: row.get("success"),
            checksum_hex: hex::encode(row.get::<Vec<u8>, _>("checksum")),
        })
        .collect())
}

pub async fn audit_migration_history(
    pool: &PgPool,
    migrations_dir: &Path,
) -> anyhow::Result<MigrationHistoryAudit> {
    let expected = repository_migrations_from_dir(migrations_dir).await?;
    let applied = applied_migrations(pool).await?;
    Ok(compare_migration_history(&expected, &applied))
}

pub async fn audit_canonical_integrity(pool: &PgPool) -> anyhow::Result<CanonicalIntegrityAudit> {
    let partial_active_documents: i64 = sqlx::query_scalar(
        r#"
WITH active_docs AS (
  SELECT access_zone_id, document_id, document_version
  FROM astravector.document_versions
  WHERE status='ACTIVE'
),
chunk_docs AS (
  SELECT DISTINCT access_zone_id, document_id, document_version
  FROM astravector.content_chunks_v004
),
binding_docs AS (
  SELECT DISTINCT access_zone_id, document_id, document_version
  FROM astravector.vector_bindings_v004
),
completed_outbox_docs AS (
  SELECT DISTINCT b.access_zone_id, b.document_id, b.document_version
  FROM astravector.vector_outbox o
  JOIN astravector.vector_bindings_v004 b
    ON b.access_zone_id=o.binding_access_zone_id
   AND b.id=o.binding_id
  WHERE o.operation='UPSERT_POINT'
    AND o.status='COMPLETED'
)
SELECT count(*)
FROM active_docs d
LEFT JOIN chunk_docs c
  ON c.access_zone_id=d.access_zone_id
 AND c.document_id=d.document_id
 AND c.document_version=d.document_version
LEFT JOIN binding_docs b
  ON b.access_zone_id=d.access_zone_id
 AND b.document_id=d.document_id
 AND b.document_version=d.document_version
LEFT JOIN completed_outbox_docs o
  ON o.access_zone_id=d.access_zone_id
 AND o.document_id=d.document_id
 AND o.document_version=d.document_version
WHERE c.document_id IS NULL
   OR b.document_id IS NULL
   OR o.document_id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit partial active documents")?;

    let orphan_chunks: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM astravector.content_chunks_v004 c
LEFT JOIN astravector.document_versions d
  ON d.access_zone_id=c.access_zone_id
 AND d.document_id=c.document_id
 AND d.document_version=c.document_version
LEFT JOIN astravector.access_zones az
  ON az.access_zone_id=c.access_zone_id
WHERE d.document_id IS NULL OR az.access_zone_id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit orphan chunks")?;

    let orphan_bindings: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM astravector.vector_bindings_v004 b
LEFT JOIN astravector.content_chunks_v004 c
  ON c.access_zone_id=b.access_zone_id AND c.id=b.chunk_id
LEFT JOIN astravector.document_versions d
  ON d.access_zone_id=b.access_zone_id
 AND d.document_id=b.document_id
 AND d.document_version=b.document_version
LEFT JOIN astravector.embedding_cache_entries ce
  ON ce.id=b.cache_entry_id
LEFT JOIN astravector.access_zones az
  ON az.access_zone_id=b.access_zone_id
WHERE c.id IS NULL OR d.document_id IS NULL OR ce.id IS NULL OR az.access_zone_id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit orphan bindings")?;

    let orphan_outbox: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM astravector.vector_outbox o
LEFT JOIN astravector.vector_bindings_v004 b
  ON b.access_zone_id=o.binding_access_zone_id
 AND b.id=o.binding_id
WHERE b.id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit orphan outbox")?;

    let orphan_graph_nodes: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM astravector.rag_graph_nodes n
LEFT JOIN astravector.document_versions d
  ON d.access_zone_id=n.access_zone_id
 AND d.document_id=n.document_id
 AND d.document_version=n.document_version
LEFT JOIN astravector.access_zones az
  ON az.access_zone_id=n.access_zone_id
WHERE d.document_id IS NULL OR az.access_zone_id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit orphan graph nodes")?;

    let orphan_graph_edges: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM astravector.rag_graph_edges e
LEFT JOIN astravector.rag_graph_nodes s
  ON s.access_zone_id=e.access_zone_id
 AND s.node_id=e.source_node_id
LEFT JOIN astravector.rag_graph_nodes t
  ON t.access_zone_id=e.access_zone_id
 AND t.node_id=e.target_node_id
LEFT JOIN astravector.document_versions d
  ON d.access_zone_id=e.access_zone_id
 AND d.document_id=e.document_id
 AND d.document_version=e.document_version
WHERE s.node_id IS NULL OR t.node_id IS NULL OR d.document_id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit orphan graph edges")?;

    let duplicate_chunks: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM (
  SELECT access_zone_id,document_id,document_version,root_chunk_id,granularity,representation_type,sequence_no,count(*)
  FROM astravector.content_chunks_v004
  GROUP BY access_zone_id,document_id,document_version,root_chunk_id,granularity,representation_type,sequence_no
  HAVING count(*) > 1
) duplicates
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit duplicate chunks")?;

    let duplicate_bindings: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM (
  SELECT access_zone_id,document_id,document_version,chunk_id,representation_type,count(*)
  FROM astravector.vector_bindings_v004
  GROUP BY access_zone_id,document_id,document_version,chunk_id,representation_type
  HAVING count(*) > 1
) duplicates
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit duplicate bindings")?;

    let duplicate_outbox_events: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM (
  SELECT binding_access_zone_id,binding_id,operation,operation_version,count(*)
  FROM astravector.vector_outbox
  GROUP BY binding_access_zone_id,binding_id,operation,operation_version
  HAVING count(*) > 1
) duplicates
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit duplicate outbox events")?;

    let active_searchable_bindings_missing_dense: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM astravector.vector_bindings_v004 b
JOIN astravector.embedding_cache_entries ce ON ce.id=b.cache_entry_id
LEFT JOIN astravector.embedding_dense d ON d.cache_entry_id=b.cache_entry_id
WHERE b.lifecycle_status='ACTIVE'
  AND b.qdrant_sync_status='SYNCED'
  AND b.chunk_granularity IN ('PARENT','SUB_180','SUB_260')
  AND ce.status='COMPLETED'
  AND d.cache_entry_id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit active searchable bindings missing dense")?;

    let active_searchable_bindings_missing_sparse: i64 = sqlx::query_scalar(
        r#"
SELECT count(*) FROM astravector.vector_bindings_v004 b
JOIN astravector.embedding_cache_entries ce ON ce.id=b.cache_entry_id
LEFT JOIN astravector.embedding_sparse s ON s.cache_entry_id=b.cache_entry_id
WHERE b.lifecycle_status='ACTIVE'
  AND b.qdrant_sync_status='SYNCED'
  AND b.chunk_granularity IN ('PARENT','SUB_180','SUB_260')
  AND ce.status='COMPLETED'
  AND NULLIF(ce.sparse_version,'') IS NOT NULL
  AND s.cache_entry_id IS NULL
"#,
    )
    .fetch_one(pool)
    .await
    .context("audit active searchable bindings missing sparse")?;

    let dead_or_failed_outbox: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM astravector.vector_outbox WHERE status IN ('DEAD_LETTER','FAILED')",
    )
    .fetch_one(pool)
    .await
    .context("audit dead or failed outbox")?;

    Ok(CanonicalIntegrityAudit {
        partial_active_documents,
        orphan_chunks,
        orphan_bindings,
        orphan_outbox,
        orphan_graph_nodes,
        orphan_graph_edges,
        duplicate_chunks,
        duplicate_bindings,
        duplicate_outbox_events,
        active_searchable_bindings_missing_dense,
        active_searchable_bindings_missing_sparse,
        dead_or_failed_outbox,
    })
}

pub async fn schema_inventory(pool: &PgPool) -> anyhow::Result<SchemaInventory> {
    let mut items = Vec::new();
    push_rows(
        pool,
        &mut items,
        "extensions",
        r#"
SELECT extname AS name, extversion AS version
FROM pg_extension
WHERE extname IN ('plpgsql','vector','uuid-ossp','pgcrypto')
ORDER BY extname
"#,
    )
    .await?;
    push_rows(
        pool,
        &mut items,
        "relations",
        r#"
SELECT n.nspname AS schema_name,c.relname AS relation_name,c.relkind::text AS relation_kind,
       COALESCE(pg_get_partkeydef(c.oid),'') AS partition_key
FROM pg_class c
JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname='astravector'
  AND c.relkind IN ('r','p','v','m','S')
ORDER BY n.nspname,c.relname,c.relkind::text
"#,
    )
    .await?;
    push_rows(
        pool,
        &mut items,
        "columns",
        r#"
SELECT n.nspname AS schema_name,c.relname AS relation_name,a.attnum AS ordinal,
       a.attname AS column_name,format_type(a.atttypid,a.atttypmod) AS data_type,
       a.attnotnull AS not_null,COALESCE(pg_get_expr(ad.adbin,ad.adrelid),'') AS default_expr
FROM pg_attribute a
JOIN pg_class c ON c.oid=a.attrelid
JOIN pg_namespace n ON n.oid=c.relnamespace
LEFT JOIN pg_attrdef ad ON ad.adrelid=a.attrelid AND ad.adnum=a.attnum
WHERE n.nspname='astravector'
  AND a.attnum > 0
  AND NOT a.attisdropped
  AND c.relkind IN ('r','p','v','m')
ORDER BY n.nspname,c.relname,a.attnum
"#,
    )
    .await?;
    push_rows(
        pool,
        &mut items,
        "constraints",
        r#"
SELECT n.nspname AS schema_name,c.relname AS relation_name,con.conname AS constraint_name,
       con.contype::text AS constraint_type,pg_get_constraintdef(con.oid,true) AS definition
FROM pg_constraint con
JOIN pg_class c ON c.oid=con.conrelid
JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname='astravector'
ORDER BY n.nspname,c.relname,con.conname
"#,
    )
    .await?;
    push_rows(
        pool,
        &mut items,
        "indexes",
        r#"
SELECT ns.nspname AS schema_name,t.relname AS relation_name,i.relname AS index_name,
       ix.indisunique AS is_unique,ix.indisprimary AS is_primary,
       pg_get_indexdef(i.oid) AS definition,
       COALESCE(pg_get_expr(ix.indpred,ix.indrelid),'') AS predicate
FROM pg_index ix
JOIN pg_class t ON t.oid=ix.indrelid
JOIN pg_class i ON i.oid=ix.indexrelid
JOIN pg_namespace ns ON ns.oid=t.relnamespace
WHERE ns.nspname='astravector'
ORDER BY ns.nspname,t.relname,i.relname
"#,
    )
    .await?;
    push_rows(
        pool,
        &mut items,
        "triggers",
        r#"
SELECT n.nspname AS schema_name,c.relname AS relation_name,t.tgname AS trigger_name,
       pg_get_triggerdef(t.oid,true) AS definition
FROM pg_trigger t
JOIN pg_class c ON c.oid=t.tgrelid
JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname='astravector'
  AND NOT t.tgisinternal
ORDER BY n.nspname,c.relname,t.tgname
"#,
    )
    .await?;
    let sha256 = canonical_json_sha256(&items)?;
    Ok(SchemaInventory { sha256, items })
}

pub async fn canonical_fingerprint(pool: &PgPool) -> anyhow::Result<CanonicalFingerprint> {
    let mut items = Vec::new();
    for (name, sql) in [
        (
            "access_zones",
            "SELECT count(*)::bigint AS row_count FROM astravector.access_zones",
        ),
        (
            "document_versions",
            "SELECT status,count(*)::bigint AS row_count FROM astravector.document_versions GROUP BY status ORDER BY status",
        ),
        (
            "content_chunks_v004",
            "SELECT granularity,lifecycle_status,count(*)::bigint AS row_count FROM astravector.content_chunks_v004 GROUP BY granularity,lifecycle_status ORDER BY granularity,lifecycle_status",
        ),
        (
            "embedding_cache_entries",
            "SELECT status,count(*)::bigint AS row_count FROM astravector.embedding_cache_entries GROUP BY status ORDER BY status",
        ),
        (
            "embedding_dense",
            "SELECT count(*)::bigint AS row_count FROM astravector.embedding_dense",
        ),
        (
            "embedding_sparse",
            "SELECT count(*)::bigint AS row_count FROM astravector.embedding_sparse",
        ),
        (
            "vector_bindings_v004",
            "SELECT chunk_granularity,lifecycle_status,qdrant_sync_status,count(*)::bigint AS row_count FROM astravector.vector_bindings_v004 GROUP BY chunk_granularity,lifecycle_status,qdrant_sync_status ORDER BY chunk_granularity,lifecycle_status,qdrant_sync_status",
        ),
        (
            "vector_outbox",
            "SELECT operation,status,count(*)::bigint AS row_count FROM astravector.vector_outbox GROUP BY operation,status ORDER BY operation,status",
        ),
        (
            "rag_graph_nodes",
            "SELECT node_type,lifecycle_status,count(*)::bigint AS row_count FROM astravector.rag_graph_nodes GROUP BY node_type,lifecycle_status ORDER BY node_type,lifecycle_status",
        ),
        (
            "rag_graph_edges",
            "SELECT relation_type,lifecycle_status,count(*)::bigint AS row_count FROM astravector.rag_graph_edges GROUP BY relation_type,lifecycle_status ORDER BY relation_type,lifecycle_status",
        ),
    ] {
        push_rows(pool, &mut items, name, sql).await?;
    }
    let sha256 = canonical_json_sha256(&items)?;
    Ok(CanonicalFingerprint { sha256, items })
}

pub async fn postgres_recovery_audit(
    repo: &Repository,
    migrations_dir: &Path,
) -> anyhow::Result<PostgresRecoveryAudit> {
    postgres_recovery_audit_with_policy(
        repo,
        migrations_dir,
        PostgresRecoveryAuditPolicy::default(),
    )
    .await
}

pub async fn postgres_recovery_audit_with_policy(
    repo: &Repository,
    migrations_dir: &Path,
    policy: PostgresRecoveryAuditPolicy,
) -> anyhow::Result<PostgresRecoveryAudit> {
    let migration_history = audit_migration_history(&repo.pool, migrations_dir).await?;
    let canonical_integrity = audit_canonical_integrity(&repo.pool).await?;
    let schema_inventory = schema_inventory(&repo.pool).await?;
    let canonical_fingerprint = canonical_fingerprint(&repo.pool).await?;
    let blocking =
        migration_history.blocking_total() + canonical_integrity.blocking_total_with_policy(policy);
    Ok(PostgresRecoveryAudit {
        verdict: if blocking == 0 {
            "POSTGRES_CANONICAL_AUDIT_PASS".to_string()
        } else {
            "POSTGRES_CANONICAL_AUDIT_FAIL".to_string()
        },
        migration_history,
        canonical_integrity,
        schema_inventory,
        canonical_fingerprint,
        read_only: true,
    })
}

async fn push_rows(
    pool: &PgPool,
    items: &mut Vec<serde_json::Value>,
    category: &str,
    sql: &str,
) -> anyhow::Result<()> {
    for row in sqlx::query(sql)
        .fetch_all(pool)
        .await
        .with_context(|| format!("read schema inventory category {category}"))?
    {
        let mut fields = serde_json::Map::new();
        for column in row.columns() {
            let name = column.name();
            let value = row_value_to_json(&row, name);
            fields.insert(name.to_string(), value);
        }
        items.push(serde_json::json!({
            "category": category,
            "fields": fields
        }));
    }
    Ok(())
}

fn row_value_to_json(row: &sqlx::postgres::PgRow, name: &str) -> serde_json::Value {
    if let Ok(value) = row.try_get::<String, _>(name) {
        return serde_json::Value::String(value);
    }
    if let Ok(value) = row.try_get::<i64, _>(name) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<i32, _>(name) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<i16, _>(name) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<bool, _>(name) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<serde_json::Value, _>(name) {
        return value;
    }
    serde_json::Value::Null
}

fn canonical_json_sha256(value: &impl Serialize) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(value).context("serialize canonical JSON")?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
