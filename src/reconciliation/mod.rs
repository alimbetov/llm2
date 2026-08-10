use crate::{
    config::DenseConfig, error::AstraError, persistence::Repository,
    projection::CanonicalProjectionInput, qdrant::QdrantClient, recovery,
};
use serde::Serialize;
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
#[derive(Debug, Clone, Copy)]
pub enum ReconciliationMode {
    Incremental,
    Full,
    Document,
    DocumentVersion,
    AccessZone,
    Collection,
}
#[derive(Debug, Default)]
pub struct ReconciliationSummary {
    pub scanned: u64,
    pub mismatches: u64,
    pub repairs: u64,
    pub quarantined: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QdrantProjectionAuditSummary {
    pub expected_eligible_bindings: u64,
    pub actual_points: u64,
    pub missing_points: u64,
    pub orphan_points: u64,
    pub payload_mismatches: u64,
    pub pages_scanned: u64,
    pub points_scanned: u64,
    pub scan_completed: bool,
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QdrantRebuildSummary {
    pub expected_eligible_bindings: u64,
    pub batches_scanned: u64,
    pub points_upserted: u64,
    pub failed_points: u64,
    pub batch_size: i64,
    pub replace_existing: bool,
    pub used_inference_fallback: bool,
    pub verdict: String,
}
pub struct Reconciler {
    pub repo: Repository,
    pub qdrant: QdrantClient,
}
impl Reconciler {
    async fn fetch_projection_batch(
        &self,
        after_zone: Option<Uuid>,
        after_binding: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<sqlx::postgres::PgRow>, AstraError> {
        sqlx::query(
            r#"
SELECT b.access_zone_id,b.id AS binding_id,b.qdrant_point_id,b.document_id,b.document_version,
       b.root_chunk_id,b.source_chunk_id,b.parent_chunk_id,b.chunk_id,b.chunk_granularity,
       b.representation_type,b.access_level,b.expires_at,b.legal_hold,b.lifecycle_status,
       b.payload_version,b.metadata,c.cache_key,c.model_version,c.tokenizer_version,
       c.dense_version,c.sparse_version,az.access_zone_code
FROM astravector.vector_bindings_v004 b
JOIN astravector.embedding_cache_entries c
  ON c.id=b.cache_entry_id
 AND c.status='COMPLETED'
JOIN astravector.content_chunks_v004 ch
  ON ch.access_zone_id=b.access_zone_id
 AND ch.id=b.chunk_id
 AND ch.document_id=b.document_id
 AND ch.document_version=b.document_version
JOIN astravector.document_versions d
  ON d.access_zone_id=b.access_zone_id
 AND d.document_id=b.document_id
 AND d.document_version=b.document_version
JOIN astravector.access_zones az
  ON az.access_zone_id=b.access_zone_id
WHERE b.lifecycle_status='ACTIVE'
  AND b.qdrant_sync_status='SYNCED'
  AND b.qdrant_point_id IS NOT NULL
  AND b.representation_type='ORIGINAL'
  AND b.chunk_granularity IN ('PARENT','SUB_180','SUB_260')
  AND b.deleted_at IS NULL
  AND (b.expires_at IS NULL OR b.expires_at > now())
  AND ch.lifecycle_status='ACTIVE'
  AND ch.deleted_at IS NULL
  AND (ch.expires_at IS NULL OR ch.expires_at > now())
  AND d.status='ACTIVE'
  AND d.lifecycle_status='ACTIVE'
  AND d.delete_operation_id IS NULL
  AND (d.expires_at IS NULL OR d.expires_at > now())
  AND az.status='ACTIVE'
  AND (
    $2::uuid IS NULL
    OR b.access_zone_id > $2::uuid
    OR (b.access_zone_id = $2::uuid AND b.id > $3::uuid)
  )
ORDER BY b.access_zone_id,b.id
LIMIT $1
"#,
        )
        .bind(limit.max(1))
        .bind(after_zone)
        .bind(after_binding)
        .fetch_all(&self.repo.pool)
        .await
        .map_err(db)
    }

    pub async fn qdrant_rebuild_from_postgres(
        &self,
        dense: &DenseConfig,
        batch_size: i64,
        replace_existing: bool,
    ) -> Result<QdrantRebuildSummary, AstraError> {
        let fence = recovery::acquire_qdrant_recovery_exclusive_fence(&self.repo.pool).await?;
        let result = async {
            if replace_existing {
                self.qdrant.delete_collection().await?;
            }
            self.qdrant.ensure_collection(dense.dimension).await?;

            let mut after_zone = None;
            let mut after_binding = None;
            let mut expected = 0_u64;
            let mut batches = 0_u64;
            let mut upserted = 0_u64;
            let failed = 0_u64;
            loop {
                let rows = self
                    .fetch_projection_batch(after_zone, after_binding, batch_size.max(1))
                    .await?;
                if rows.is_empty() {
                    break;
                }
                batches += 1;
                for row in &rows {
                    expected += 1;
                    let zone: Uuid = row.get("access_zone_id");
                    let binding: Uuid = row.get("binding_id");
                    let key: String = row.get("cache_key");
                    let projection = CanonicalProjectionInput::from_pg_row(row, zone, binding);
                    let embedding = self.repo.load_completed(&key).await?.ok_or_else(|| {
                        AstraError::FailedPrecondition("canonical vector missing".into())
                    })?;
                    self.qdrant.upsert(&projection.point(embedding)).await?;
                    upserted += 1;
                }
                let last = rows.last().expect("non-empty batch");
                after_zone = Some(last.get("access_zone_id"));
                after_binding = Some(last.get("binding_id"));
            }
            Ok(QdrantRebuildSummary {
                expected_eligible_bindings: expected,
                batches_scanned: batches,
                points_upserted: upserted,
                failed_points: failed,
                batch_size: batch_size.max(1),
                replace_existing,
                used_inference_fallback: false,
                verdict: "QDRANT_REBUILD_COMPLETED".to_string(),
            })
        }
        .await;
        let release = fence.release().await;
        match result {
            Ok(summary) => {
                release?;
                Ok(summary)
            }
            Err(error) => {
                if let Err(release_error) = release {
                    tracing::warn!(%release_error, "qdrant recovery fence release failed after rebuild error");
                }
                Err(error)
            }
        }
    }

    pub async fn qdrant_projection_audit(
        &self,
        batch_size: i64,
    ) -> Result<QdrantProjectionAuditSummary, AstraError> {
        let actual = self.qdrant.scroll_all_points_with_payload().await?;
        if !actual.completed {
            return Ok(QdrantProjectionAuditSummary {
                expected_eligible_bindings: 0,
                actual_points: actual.payloads.len() as u64,
                missing_points: 0,
                orphan_points: 0,
                payload_mismatches: 0,
                pages_scanned: actual.pages_read,
                points_scanned: actual.points_read,
                scan_completed: false,
                verdict: "QDRANT_AUDIT_INCOMPLETE".to_string(),
            });
        }

        let mut expected_payloads: HashMap<Uuid, serde_json::Value> = HashMap::new();
        let mut after_zone = None;
        let mut after_binding = None;
        loop {
            let rows = self
                .fetch_projection_batch(after_zone, after_binding, batch_size.max(1))
                .await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                let zone: Uuid = row.get("access_zone_id");
                let binding: Uuid = row.get("binding_id");
                let projection = CanonicalProjectionInput::from_pg_row(row, zone, binding);
                expected_payloads.insert(projection.qdrant_point_id, projection.payload());
            }
            let last = rows.last().expect("non-empty batch");
            after_zone = Some(last.get("access_zone_id"));
            after_binding = Some(last.get("binding_id"));
        }

        let expected_ids: HashSet<Uuid> = expected_payloads.keys().copied().collect();
        let actual_ids: HashSet<Uuid> = actual.payloads.keys().copied().collect();
        let missing_points = expected_ids.difference(&actual_ids).count() as u64;
        let orphan_points = actual_ids.difference(&expected_ids).count() as u64;
        let payload_mismatches = expected_payloads
            .iter()
            .filter(|(point_id, expected)| {
                actual
                    .payloads
                    .get(point_id)
                    .is_some_and(|actual| actual != *expected)
            })
            .count() as u64;
        let drift = missing_points + orphan_points + payload_mismatches;
        Ok(QdrantProjectionAuditSummary {
            expected_eligible_bindings: expected_payloads.len() as u64,
            actual_points: actual.payloads.len() as u64,
            missing_points,
            orphan_points,
            payload_mismatches,
            pages_scanned: actual.pages_read,
            points_scanned: actual.points_read,
            scan_completed: actual.completed,
            verdict: if drift == 0 {
                "QDRANT_PROJECTION_CONSISTENT".to_string()
            } else {
                "QDRANT_PROJECTION_DRIFT".to_string()
            },
        })
    }

    pub async fn reconcile_binding(
        &self,
        zone: Uuid,
        binding: Uuid,
    ) -> Result<ReconciliationSummary, AstraError> {
        let row=sqlx::query("SELECT b.qdrant_point_id,b.lifecycle_status,b.qdrant_sync_status,b.payload_version,b.document_id,b.document_version,b.root_chunk_id,b.source_chunk_id,b.parent_chunk_id,b.chunk_id,b.chunk_granularity,b.representation_type,b.access_level,b.expires_at,b.legal_hold,b.metadata,c.cache_key,c.model_version,c.tokenizer_version,c.dense_version,c.sparse_version,az.access_zone_code FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_cache_entries c ON c.id=b.cache_entry_id LEFT JOIN astravector.access_zones az ON az.access_zone_id=b.access_zone_id WHERE b.access_zone_id=$1 AND b.id=$2").bind(zone).bind(binding).fetch_optional(&self.repo.pool).await.map_err(db)?;
        let Some(r) = row else {
            return Ok(ReconciliationSummary::default());
        };
        let point: Uuid = r.get("qdrant_point_id");
        let exists = self.qdrant.point_exists(point).await?;
        let lifecycle: String = r.get("lifecycle_status");
        let legal_hold: bool = r.get("legal_hold");
        let mut s = ReconciliationSummary {
            scanned: 1,
            ..Default::default()
        };
        if lifecycle == "ACTIVE" && !exists {
            let key: String = r.get("cache_key");
            let emb = self
                .repo
                .load_completed(&key)
                .await?
                .ok_or_else(|| AstraError::Internal("canonical vector missing".into()))?;
            let projection = CanonicalProjectionInput::from_pg_row(&r, zone, binding);
            let fence = recovery::acquire_qdrant_projection_write_fence(&self.repo.pool).await?;
            self.qdrant.upsert(&projection.point(emb)).await?;
            fence.commit().await.map_err(db)?;
            s.mismatches += 1;
            s.repairs += 1
        } else if lifecycle != "ACTIVE" && exists {
            if legal_hold {
                metrics::counter!("reconciliation_skipped_legal_hold_total").increment(1);
            } else {
                let fence =
                    recovery::acquire_qdrant_projection_write_fence(&self.repo.pool).await?;
                self.qdrant.delete(point).await?;
                fence.commit().await.map_err(db)?;
                s.mismatches += 1;
                s.repairs += 1;
                metrics::counter!("reconciliation_bindings_deleted_total").increment(1);
            }
        }
        Ok(s)
    }

    pub async fn reconcile_unsynced_batch(
        &self,
        limit: i64,
    ) -> Result<ReconciliationSummary, AstraError> {
        let rows = sqlx::query(
            "SELECT access_zone_id,id FROM astravector.vector_bindings_v004              WHERE qdrant_sync_status <> 'SYNCED' OR COALESCE(last_qdrant_sync_version,0) < payload_version              ORDER BY updated_at LIMIT $1"
        )
        .bind(limit.max(1))
        .fetch_all(&self.repo.pool)
        .await
        .map_err(db)?;
        let mut total = ReconciliationSummary::default();
        for row in rows {
            let zone: Uuid = row.get("access_zone_id");
            let binding: Uuid = row.get("id");
            metrics::counter!("reconciliation_bindings_scanned_total").increment(1);
            match self.reconcile_binding(zone, binding).await {
                Ok(summary) => {
                    total.scanned += summary.scanned;
                    total.mismatches += summary.mismatches;
                    total.repairs += summary.repairs;
                    total.quarantined += summary.quarantined;
                    if summary.repairs > 0 {
                        metrics::counter!("reconciliation_bindings_repaired_total")
                            .increment(summary.repairs);
                    }
                }
                Err(e) => {
                    metrics::counter!("reconciliation_errors_total").increment(1);
                    tracing::warn!(error=%e, %zone, %binding, "reconciliation binding failed");
                }
            }
        }
        Ok(total)
    }
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}
