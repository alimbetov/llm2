use crate::{error::AstraError, pb, persistence::Repository};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BindingStatus {
    pub id: Uuid,
    pub lifecycle_status: String,
    pub qdrant_sync_status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl Repository {
    pub async fn upsert_binding_with_outbox(
        &self,
        tenant: &str,
        workspace: &str,
        item: &pb::EncodeItem,
        request_access: i32,
        cache_entry_id: Uuid,
        collection: &str,
    ) -> Result<Uuid, AstraError> {
        let chunk_id = Uuid::parse_str(&item.chunk_id)
            .map_err(|_| AstraError::InvalidArgument("invalid chunk_id".into()))?;
        let document_id = item
            .document_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| AstraError::InvalidArgument("invalid document_id".into()))?;
        let parent = item
            .parent_chunk_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| AstraError::InvalidArgument("invalid parent_chunk_id".into()))?;
        let source = item
            .source_chunk_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| AstraError::InvalidArgument("invalid source_chunk_id".into()))?;
        let access = if item.access_level == pb::AccessLevel::Unspecified as i32 {
            request_access
        } else {
            item.access_level
        };
        if !(1..=4).contains(&access) {
            return Err(AstraError::InvalidArgument(
                "access_level must be 1..4".into(),
            ));
        }
        let ttl = item.ttl_days.map(|x| x as i32);
        let rep = pb::SearchRepresentationType::try_from(item.representation_type)
            .unwrap_or(pb::SearchRepresentationType::Original);
        let rep_name = format!("{:?}", rep).to_uppercase();
        let metadata =
            serde_json::to_value(&item.metadata).unwrap_or(Value::Object(Default::default()));
        let namespace = Uuid::NAMESPACE_URL;
        let point_id = Uuid::new_v5(
            &namespace,
            format!(
                "{tenant}|{workspace}|{}|{}|{}|{}",
                item.document_id.as_deref().unwrap_or(""),
                item.document_version.unwrap_or(0),
                item.chunk_id,
                rep_name
            )
            .as_bytes(),
        );
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row=sqlx::query("INSERT INTO astravector.vector_bindings(id,tenant_id,workspace_id,document_id,document_version,chunk_id,chunk_type,parent_chunk_id,source_chunk_id,representation_type,cache_entry_id,access_level,ttl_days,expires_at,qdrant_collection,qdrant_point_id,metadata) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,CASE WHEN $13 IS NULL THEN NULL ELSE now()+($13*interval '1 day') END,$14,$15,$16) ON CONFLICT(tenant_id,workspace_id,document_id,document_version,chunk_id,representation_type) DO UPDATE SET cache_entry_id=EXCLUDED.cache_entry_id,access_level=EXCLUDED.access_level,ttl_days=EXCLUDED.ttl_days,expires_at=CASE WHEN EXCLUDED.ttl_days IS NULL THEN NULL ELSE now()+(EXCLUDED.ttl_days*interval '1 day') END,lifecycle_status='ACTIVE',qdrant_sync_status='PENDING',payload_version=astravector.vector_bindings.payload_version+1,metadata=EXCLUDED.metadata,updated_at=now(),deleted_at=NULL,expired_at=NULL RETURNING id,qdrant_sync_status")
     .bind(id).bind(tenant).bind(workspace).bind(document_id).bind(item.document_version.map(|x|x as i64)).bind(chunk_id).bind(item.chunk_type as i16).bind(parent).bind(source).bind(rep_name).bind(cache_entry_id).bind(access as i16).bind(ttl).bind(collection).bind(point_id).bind(metadata).fetch_one(&mut*tx).await.map_err(db)?;
        let binding_id: Uuid = row.get("id");
        sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_id,operation,status) VALUES($1,$2,'UPSERT_POINT','PENDING')").bind(Uuid::new_v4()).bind(binding_id).execute(&mut*tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(binding_id)
    }

    pub async fn delete_document_vectors(
        &self,
        tenant: &str,
        workspace: &str,
        document_id: Uuid,
        version: Option<i64>,
    ) -> Result<u64, AstraError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let rows=sqlx::query("SELECT id FROM astravector.vector_bindings WHERE tenant_id=$1 AND workspace_id=$2 AND document_id=$3 AND($4::bigint IS NULL OR document_version=$4) AND lifecycle_status IN('ACTIVE','LEGAL_HOLD','DELETE_FAILED') FOR UPDATE").bind(tenant).bind(workspace).bind(document_id).bind(version).fetch_all(&mut*tx).await.map_err(db)?;
        for r in &rows {
            let id: Uuid = r.get("id");
            sqlx::query("UPDATE astravector.vector_bindings SET lifecycle_status='DELETION_PENDING',qdrant_sync_status='DELETE_PENDING',updated_at=now() WHERE id=$1 AND legal_hold=false").bind(id).execute(&mut*tx).await.map_err(db)?;
            sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_id,operation,status) SELECT $1,$2,'DELETE_POINT','PENDING' WHERE EXISTS(SELECT 1 FROM astravector.vector_bindings WHERE id=$2 AND legal_hold=false)").bind(Uuid::new_v4()).bind(id).execute(&mut*tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(rows.len() as u64)
    }

    pub async fn update_binding_metadata(
        &self,
        tenant: &str,
        workspace: &str,
        binding_id: Uuid,
        access_level: i32,
        ttl_days: Option<u32>,
        metadata: &std::collections::HashMap<String, String>,
    ) -> Result<(), AstraError> {
        if !(1..=4).contains(&access_level) {
            return Err(AstraError::InvalidArgument(
                "access_level must be 1..4".into(),
            ));
        }
        let meta = serde_json::to_value(metadata).unwrap_or(Value::Object(Default::default()));
        let ttl = ttl_days.map(|x| x as i32);
        let mut tx = self.pool.begin().await.map_err(db)?;
        let n=sqlx::query("UPDATE astravector.vector_bindings SET access_level=$4,ttl_days=$5,expires_at=CASE WHEN $5 IS NULL THEN NULL ELSE now()+($5*interval '1 day') END,metadata=$6,payload_version=payload_version+1,qdrant_sync_status='UPDATE_PENDING',updated_at=now() WHERE id=$1 AND tenant_id=$2 AND workspace_id=$3").bind(binding_id).bind(tenant).bind(workspace).bind(access_level as i16).bind(ttl).bind(meta).execute(&mut*tx).await.map_err(db)?.rows_affected();
        if n != 1 {
            return Err(AstraError::FailedPrecondition("binding not found".into()));
        }
        sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_id,operation,status) VALUES($1,$2,'UPDATE_PAYLOAD','PENDING')").bind(Uuid::new_v4()).bind(binding_id).execute(&mut*tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    pub async fn extend_binding_ttl(
        &self,
        tenant: &str,
        workspace: &str,
        binding_id: Uuid,
        ttl_days: u32,
        replace: bool,
    ) -> Result<DateTime<Utc>, AstraError> {
        if ttl_days == 0 {
            return Err(AstraError::InvalidArgument(
                "ttl_days must be positive".into(),
            ));
        }
        let row=sqlx::query("UPDATE astravector.vector_bindings SET ttl_days=CASE WHEN $5 THEN $4 ELSE GREATEST(COALESCE(ttl_days,0),$4) END,expires_at=CASE WHEN $5 THEN now()+($4*interval '1 day') ELSE GREATEST(COALESCE(expires_at,now()),now()+($4*interval '1 day')) END,payload_version=payload_version+1,qdrant_sync_status='UPDATE_PENDING',updated_at=now() WHERE id=$1 AND tenant_id=$2 AND workspace_id=$3 RETURNING expires_at").bind(binding_id).bind(tenant).bind(workspace).bind(ttl_days as i32).bind(replace).fetch_optional(&self.pool).await.map_err(db)?.ok_or_else(||AstraError::FailedPrecondition("binding not found".into()))?;
        sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_id,operation,status) VALUES($1,$2,'UPDATE_PAYLOAD','PENDING')").bind(Uuid::new_v4()).bind(binding_id).execute(&self.pool).await.map_err(db)?;
        Ok(row.get("expires_at"))
    }

    pub async fn binding_status(
        &self,
        tenant: &str,
        workspace: &str,
        binding_id: Uuid,
    ) -> Result<BindingStatus, AstraError> {
        let r=sqlx::query("SELECT b.id,b.lifecycle_status,b.qdrant_sync_status,b.expires_at,(SELECT error_message FROM astravector.vector_outbox o WHERE o.binding_id=b.id AND o.status IN('RETRY_PENDING','DEAD_LETTER') ORDER BY created_at DESC LIMIT 1) last_error FROM astravector.vector_bindings b WHERE b.id=$1 AND b.tenant_id=$2 AND b.workspace_id=$3").bind(binding_id).bind(tenant).bind(workspace).fetch_optional(&self.pool).await.map_err(db)?.ok_or_else(||AstraError::FailedPrecondition("binding not found".into()))?;
        Ok(BindingStatus {
            id: r.get("id"),
            lifecycle_status: r.get("lifecycle_status"),
            qdrant_sync_status: r.get("qdrant_sync_status"),
            expires_at: r.try_get("expires_at").ok(),
            last_error: r.try_get("last_error").ok(),
        })
    }
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}
