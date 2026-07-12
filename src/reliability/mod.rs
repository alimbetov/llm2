use crate::{error::AstraError, inference::EmbeddingResult, persistence::Repository};
use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::Row;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadKind {
    Query,
    DocumentPublisher,
    Reconciliation,
}

#[derive(Debug, Clone)]
pub struct OperationBudget {
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    pub workload: WorkloadKind,
}

impl OperationBudget {
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub fn is_expired(&self) -> bool {
        self.cancellation.is_cancelled() || Instant::now() >= self.deadline
    }

    pub fn allows_retry(
        &self,
        backoff: Duration,
        minimum_operation_time: Duration,
        safety_margin: Duration,
    ) -> bool {
        !self.is_expired()
            && self.remaining()
                >= backoff
                    .saturating_add(minimum_operation_time)
                    .saturating_add(safety_margin)
    }
}

pub fn resolve_optional_stage_budget(
    deadline: Instant,
    configured_max: Duration,
    minimum_useful_budget: Duration,
    response_reserve: Duration,
) -> Option<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining <= response_reserve {
        return None;
    }
    let budget = configured_max.min(remaining - response_reserve);
    (budget >= minimum_useful_budget).then_some(budget)
}
#[derive(Debug, Clone)]
pub struct RequiredPersistCommand {
    pub access_zone_id: Uuid,
    pub cache_entry_id: Uuid,
    pub owner: String,
    pub lease_token: i64,
    pub chunk_id: Uuid,
    pub root_chunk_id: Uuid,
    pub source_chunk_id: Uuid,
    pub parent_chunk_id: Option<Uuid>,
    pub document_id: Uuid,
    pub document_version: i64,
    pub binding_id: Uuid,
    pub point_id: Uuid,
    pub collection: String,
    pub representation_type: String,
    pub granularity: String,
    pub access_level: i16,
    pub ttl_days: Option<i32>,
    pub result: EmbeddingResult,
    pub dense_name: String,
    pub dense_version: String,
    pub sparse_name: String,
    pub sparse_version: String,
}
impl Repository {
    pub async fn persist_required_v004(
        &self,
        c: &RequiredPersistCommand,
    ) -> Result<(), AstraError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let owned=sqlx::query("SELECT lease_expires_at FROM astravector.embedding_cache_entries WHERE id=$1 AND status='PROCESSING' AND owner_instance_id=$2 AND lease_token=$3 FOR UPDATE").bind(c.cache_entry_id).bind(&c.owner).bind(c.lease_token).fetch_optional(&mut*tx).await.map_err(db)?;
        let Some(row) = owned else {
            return Err(AstraError::OwnershipLost("lease no longer owned".into()));
        };
        let expiry: Option<DateTime<Utc>> = row.try_get("lease_expires_at").ok();
        if expiry.map(|x| x < Utc::now()).unwrap_or(true) {
            return Err(AstraError::OwnershipLost("lease expired".into()));
        }
        if let Some(v) = &c.result.dense {
            sqlx::query("INSERT INTO astravector.embedding_dense(id,cache_entry_id,representation_name,representation_version,dimension,normalized,distance,vector_value) VALUES($1,$2,$3,$4,$5,true,'COSINE',$6) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET vector_value=EXCLUDED.vector_value").bind(Uuid::new_v4()).bind(c.cache_entry_id).bind(&c.dense_name).bind(&c.dense_version).bind(v.len() as i32).bind(Vector::from(v.clone())).execute(&mut*tx).await.map_err(db)?;
        }
        if let (Some(i), Some(v)) = (&c.result.sparse_indices, &c.result.sparse_values) {
            let idx: Vec<i32> = i.iter().map(|x| *x as i32).collect();
            sqlx::query("INSERT INTO astravector.embedding_sparse(id,cache_entry_id,representation_name,representation_version,indices,values,non_zero_count,min_weight,max_non_zero) VALUES($1,$2,$3,$4,$5,$6,$7,0,$7) ON CONFLICT(cache_entry_id,representation_name,representation_version) DO UPDATE SET indices=EXCLUDED.indices,values=EXCLUDED.values").bind(Uuid::new_v4()).bind(c.cache_entry_id).bind(&c.sparse_name).bind(&c.sparse_version).bind(idx).bind(v).bind(i.len() as i32).execute(&mut*tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE astravector.embedding_cache_entries SET status='COMPLETED',owner_instance_id=NULL,lease_expires_at=NULL,completed_at=now(),model_input_token_count=$4,truncated=$5 WHERE id=$1 AND owner_instance_id=$2 AND lease_token=$3").bind(c.cache_entry_id).bind(&c.owner).bind(c.lease_token).bind(c.result.token_count as i32).bind(c.result.truncated).execute(&mut*tx).await.map_err(db)?;
        sqlx::query("INSERT INTO astravector.vector_bindings_v004(access_zone_id,id,document_id,document_version,root_chunk_id,source_chunk_id,parent_chunk_id,chunk_id,chunk_granularity,representation_type,cache_entry_id,access_level,ttl_days,expires_at,qdrant_collection,qdrant_point_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,CASE WHEN $13 IS NULL THEN NULL ELSE now()+($13*interval '1 day') END,$14,$15) ON CONFLICT(access_zone_id,document_id,document_version,chunk_id,representation_type) DO NOTHING").bind(c.access_zone_id).bind(c.binding_id).bind(c.document_id).bind(c.document_version).bind(c.root_chunk_id).bind(c.source_chunk_id).bind(c.parent_chunk_id).bind(c.chunk_id).bind(&c.granularity).bind(&c.representation_type).bind(c.cache_entry_id).bind(c.access_level).bind(c.ttl_days).bind(&c.collection).bind(c.point_id).execute(&mut*tx).await.map_err(db)?;
        sqlx::query("INSERT INTO astravector.vector_outbox(id,binding_access_zone_id,binding_id,operation,operation_version,status) VALUES($1,$2,$3,'UPSERT_POINT',1,'PENDING') ON CONFLICT(binding_access_zone_id,binding_id,operation,operation_version) DO NOTHING").bind(Uuid::new_v4()).bind(c.access_zone_id).bind(c.binding_id).execute(&mut*tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
    pub async fn wait_or_takeover_v004(
        &self,
        key: &str,
        owner: &str,
        lease_seconds: i64,
        deadline: tokio::time::Instant,
    ) -> Result<crate::persistence::ClaimResult, AstraError> {
        let mut delay = 50u64;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(AstraError::DeadlineExceeded(
                    "cache wait deadline exceeded".into(),
                ));
            }
            let row=sqlx::query("SELECT id,status,lease_expires_at FROM astravector.embedding_cache_entries WHERE cache_key=$1").bind(key).fetch_optional(&self.pool).await.map_err(db)?;
            let Some(row) = row else {
                return Err(AstraError::FailedPrecondition(
                    "cache entry disappeared".into(),
                ));
            };
            let id: Uuid = row.get("id");
            let status: String = row.get("status");
            if status == "COMPLETED" {
                let result = self
                    .load_completed(key)
                    .await?
                    .ok_or_else(|| AstraError::Internal("completed entry has no vectors".into()))?;
                return Ok(crate::persistence::ClaimResult::Completed {
                    cache_entry_id: id,
                    result,
                });
            }
            let expires: Option<DateTime<Utc>> = row.try_get("lease_expires_at").ok();
            if status == "FAILED" || expires.map(|x| x <= Utc::now()).unwrap_or(true) {
                if let Some(token) = self.takeover(id, owner, lease_seconds).await? {
                    return Ok(crate::persistence::ClaimResult::RetryAcquired {
                        cache_entry_id: id,
                        lease_token: token,
                    });
                }
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + std::time::Duration::from_millis(delay))
                    .min(deadline),
            )
            .await;
            delay = (delay * 3 / 2).min(500)
        }
    }
}
fn db(e: sqlx::Error) -> AstraError {
    AstraError::Unavailable(format!("postgres: {e}"))
}

#[cfg(test)]
mod operation_budget_tests {
    use super::*;

    #[test]
    fn qdrant_retry_respects_deadline_budget() {
        let budget = OperationBudget {
            deadline: Instant::now() + Duration::from_millis(199),
            cancellation: CancellationToken::new(),
            workload: WorkloadKind::Query,
        };
        assert!(!budget.allows_retry(
            Duration::from_millis(50),
            Duration::from_millis(100),
            Duration::from_millis(50),
        ));
    }

    #[test]
    fn qdrant_retry_is_allowed_when_budget_covers_all_components() {
        let budget = OperationBudget {
            deadline: Instant::now() + Duration::from_secs(2),
            cancellation: CancellationToken::new(),
            workload: WorkloadKind::DocumentPublisher,
        };
        assert!(budget.allows_retry(
            Duration::from_millis(100),
            Duration::from_millis(500),
            Duration::from_millis(50),
        ));
    }

    #[test]
    fn optional_stage_budget_preserves_response_reserve() {
        let deadline = Instant::now() + Duration::from_millis(200);
        let budget = resolve_optional_stage_budget(
            deadline,
            Duration::from_millis(500),
            Duration::from_millis(50),
            Duration::from_millis(100),
        )
        .unwrap();
        assert!(budget <= Duration::from_millis(100));
    }

    #[test]
    fn optional_stage_budget_rejects_non_useful_remainder() {
        assert!(resolve_optional_stage_budget(
            Instant::now() + Duration::from_millis(120),
            Duration::from_millis(500),
            Duration::from_millis(50),
            Duration::from_millis(100),
        )
        .is_none());
    }
}
