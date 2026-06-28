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
        let row=sqlx::query("SELECT b.qdrant_point_id,b.lifecycle_status,b.qdrant_sync_status,b.payload_version,c.cache_key FROM astravector.vector_bindings_v004 b JOIN astravector.embedding_cache_entries c ON c.id=b.cache_entry_id WHERE b.access_zone_id=$1 AND b.id=$2").bind(zone).bind(binding).fetch_optional(&self.repo.pool).await.map_err(db)?;
        let Some(r) = row else {
            return Ok(ReconciliationSummary::default());
        };
        let point: Uuid = r.get("qdrant_point_id");
        let exists = self.qdrant.point_exists(point).await?;
        let lifecycle: String = r.get("lifecycle_status");
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
            self.qdrant.upsert(&QdrantPoint{id:point,dense:emb.dense,sparse_indices:emb.sparse_indices,sparse_values:emb.sparse_values,payload:json!({"access_zone_id":zone,"binding_id":binding,"lifecycle_status":"ACTIVE","payload_version":r.get::<i64,_>("payload_version")})}).await?;
            s.mismatches += 1;
            s.repairs += 1
        } else if lifecycle != "ACTIVE" && exists {
            self.qdrant.delete(point).await?;
            s.mismatches += 1;
            s.repairs += 1
        }
        Ok(s)
    }
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}
