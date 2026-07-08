use crate::{
    error::AstraError,
    persistence::Repository,
    qdrant::{QdrantClient, QdrantPoint},
};
use serde_json::json;
use sqlx::Row;
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
pub struct Reconciler {
    pub repo: Repository,
    pub qdrant: QdrantClient,
}
impl Reconciler {
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
            let expires_at = r
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")
                .ok()
                .flatten();
            let metadata: serde_json::Value = r.get("metadata");
            let chunking_profile_version = metadata
                .get("chunking_profile_version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let payload = json!({
                "access_zone_id": zone,
                "access_zone_code": r.try_get::<Option<String>,_>("access_zone_code").ok().flatten().unwrap_or_default(),
                "binding_id": binding,
                "qdrant_point_id": point,
                "document_id": r.get::<Uuid,_>("document_id"),
                "document_version": r.get::<i64,_>("document_version"),
                "root_chunk_id": r.get::<Uuid,_>("root_chunk_id"),
                "source_chunk_id": r.get::<Uuid,_>("source_chunk_id"),
                "parent_chunk_id": r.try_get::<Option<Uuid>,_>("parent_chunk_id").ok().flatten(),
                "chunk_id": r.get::<Uuid,_>("chunk_id"),
                "chunk_granularity": r.get::<String,_>("chunk_granularity"),
                "representation_type": r.get::<String,_>("representation_type"),
                "access_level": r.get::<i16,_>("access_level"),
                "lifecycle_status": "ACTIVE",
                "expires_at_epoch": expires_at.map(|x| x.timestamp()).unwrap_or(253_402_300_799_i64),
                "legal_hold": legal_hold,
                "payload_version": r.get::<i64,_>("payload_version"),
                "model_version": r.get::<String,_>("model_version"),
                "tokenizer_version": r.get::<String,_>("tokenizer_version"),
                "dense_version": r.try_get::<Option<String>,_>("dense_version").ok().flatten(),
                "sparse_version": r.try_get::<Option<String>,_>("sparse_version").ok().flatten(),
                "chunking_profile_version": chunking_profile_version,
                "quarantined": false
            });
            self.qdrant
                .upsert(&QdrantPoint {
                    id: point,
                    dense: emb.dense,
                    sparse_indices: emb.sparse_indices,
                    sparse_values: emb.sparse_values,
                    payload,
                })
                .await?;
            s.mismatches += 1;
            s.repairs += 1
        } else if lifecycle != "ACTIVE" && exists {
            if legal_hold {
                metrics::counter!("reconciliation_skipped_legal_hold_total").increment(1);
            } else {
                self.qdrant.delete(point).await?;
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
