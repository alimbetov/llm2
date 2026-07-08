//! Access Zone Registry for fix4.5.4.
//!
//! Public contracts may use short immutable `access_zone_code` values (0000..9999),
//! while all internal storage/search/GraphRAG/TTL operations remain UUID-backed.

use crate::{config::AppConfig, error::AstraError};
use metrics::{counter, histogram};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tonic::Status;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ResolvedAccessZone {
    pub access_zone_id: Uuid,
    pub access_zone_code: String,
    pub default_ttl_days: u32,
    pub allow_never_expire: bool,
}

#[derive(Debug, Clone)]
struct RegistrySnapshot {
    loaded_at: Instant,
    by_code: HashMap<String, ResolvedAccessZone>,
    by_id: HashMap<Uuid, ResolvedAccessZone>,
}

static REGISTRY_CACHE: OnceLock<Arc<RwLock<Option<RegistrySnapshot>>>> = OnceLock::new();

fn cache() -> &'static Arc<RwLock<Option<RegistrySnapshot>>> {
    REGISTRY_CACHE.get_or_init(|| Arc::new(RwLock::new(None)))
}

pub fn is_valid_access_zone_code(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.len() == 4 && trimmed.chars().all(|c| c.is_ascii_digit())
}

pub fn default_ttl_days_from_access_zone_code(code: &str) -> Result<u32, AstraError> {
    if !is_valid_access_zone_code(code) {
        return Err(AstraError::InvalidArgument(
            "access_zone_code must match ^[0-9]{4}$".into(),
        ));
    }
    let value: u32 = code
        .parse()
        .map_err(|_| AstraError::InvalidArgument("access_zone_code must be numeric".into()))?;
    if value <= 999 {
        return Ok(0);
    }
    if value >= 9500 {
        return Ok(3650);
    }
    let block = (value - 1000) / 500;
    let months = (block + 1) * 6;
    Ok(months * 365 / 12)
}

async fn load_active_snapshot(pool: &PgPool) -> Result<RegistrySnapshot, AstraError> {
    let rows = sqlx::query(
        "SELECT access_zone_id, access_zone_code, default_ttl_days, allow_never_expire \
         FROM astravector.access_zones \
         WHERE status='ACTIVE'",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AstraError::Unavailable(format!("access zone registry load: {e}")))?;

    let mut by_code = HashMap::new();
    let mut by_id = HashMap::new();
    for row in rows {
        let zone = ResolvedAccessZone {
            access_zone_id: row.get("access_zone_id"),
            access_zone_code: row.get::<String, _>("access_zone_code"),
            default_ttl_days: row.get::<i32, _>("default_ttl_days").max(0) as u32,
            allow_never_expire: row.get("allow_never_expire"),
        };
        by_id.insert(zone.access_zone_id, zone.clone());
        by_code.insert(zone.access_zone_code.clone(), zone);
    }
    Ok(RegistrySnapshot {
        loaded_at: Instant::now(),
        by_code,
        by_id,
    })
}

async fn snapshot(pool: &PgPool, cfg: &AppConfig) -> Result<RegistrySnapshot, Status> {
    if !cfg.access_zone_registry.enabled {
        return Err(Status::failed_precondition(
            "access_zone_registry is disabled",
        ));
    }
    let ttl = Duration::from_secs(cfg.access_zone_registry.cache_ttl_seconds.max(1));
    {
        let guard = cache().read().await;
        if let Some(snap) = guard.as_ref() {
            if snap.loaded_at.elapsed() < ttl {
                counter!("access_zone_registry_cache_hit_total").increment(1);
                return Ok(snap.clone());
            }
        }
    }
    counter!("access_zone_registry_cache_miss_total").increment(1);
    let started = Instant::now();
    let loaded = load_active_snapshot(pool).await.map_err(Status::from)?;
    histogram!("access_zone_registry_reload_duration_ms")
        .record(started.elapsed().as_millis() as f64);
    let mut guard = cache().write().await;
    *guard = Some(loaded.clone());
    Ok(loaded)
}

fn row_to_resolved_zone(row: &sqlx::postgres::PgRow) -> ResolvedAccessZone {
    ResolvedAccessZone {
        access_zone_id: row.get("access_zone_id"),
        access_zone_code: row.get::<String, _>("access_zone_code"),
        default_ttl_days: row.get::<i32, _>("default_ttl_days").max(0) as u32,
        allow_never_expire: row.get("allow_never_expire"),
    }
}

fn zone_status_error(status: &str, mode: &str) -> Status {
    match status {
        "DISABLED" => {
            counter!("access_zone_registry_resolve_failed_total", "reason" => "disabled", "mode" => mode.to_string()).increment(1);
            counter!("access_zone_registry_db_fallback_disabled_total").increment(1);
            Status::failed_precondition("ACCESS_ZONE_DISABLED")
        }
        "DELETED" => {
            counter!("access_zone_registry_resolve_failed_total", "reason" => "deleted", "mode" => mode.to_string()).increment(1);
            counter!("access_zone_registry_db_fallback_deleted_total").increment(1);
            Status::failed_precondition("ACCESS_ZONE_DELETED")
        }
        _ => {
            counter!("access_zone_registry_resolve_failed_total", "reason" => "not_active", "mode" => mode.to_string()).increment(1);
            Status::failed_precondition(format!("ACCESS_ZONE_NOT_ACTIVE: {status}"))
        }
    }
}

async fn lookup_zone_by_code_any_status(
    pool: &PgPool,
    code: &str,
) -> Result<Option<(ResolvedAccessZone, String)>, Status> {
    counter!("access_zone_registry_db_fallback_total", "mode" => "code").increment(1);
    let row = sqlx::query(
        "SELECT access_zone_id, access_zone_code, status, default_ttl_days, allow_never_expire \
         FROM astravector.access_zones \
         WHERE access_zone_code=$1",
    )
    .bind(code)
    .fetch_optional(pool)
    .await
    .map_err(|e| Status::unavailable(format!("access zone DB fallback by code: {e}")))?;
    Ok(row.map(|row| (row_to_resolved_zone(&row), row.get::<String, _>("status"))))
}

async fn lookup_zone_by_id_any_status(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<(ResolvedAccessZone, String)>, Status> {
    counter!("access_zone_registry_db_fallback_total", "mode" => "uuid").increment(1);
    let row = sqlx::query(
        "SELECT access_zone_id, access_zone_code, status, default_ttl_days, allow_never_expire \
         FROM astravector.access_zones \
         WHERE access_zone_id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Status::unavailable(format!("access zone DB fallback by UUID: {e}")))?;
    Ok(row.map(|row| (row_to_resolved_zone(&row), row.get::<String, _>("status"))))
}

async fn resolve_single_code_fresh(
    pool: &PgPool,
    code: &str,
    mode: &str,
) -> Result<ResolvedAccessZone, Status> {
    if !is_valid_access_zone_code(code) {
        counter!("access_zone_code_invalid_total").increment(1);
        return Err(Status::invalid_argument(
            "access_zone_code must match ^[0-9]{4}$",
        ));
    }
    match lookup_zone_by_code_any_status(pool, code).await? {
        Some((zone, status)) if status == "ACTIVE" => {
            counter!("access_zone_registry_resolve_total", "mode" => mode.to_string(), "source" => "db_strict").increment(1);
            Ok(zone)
        }
        Some((_zone, status)) => Err(zone_status_error(&status, mode)),
        None => {
            counter!("access_zone_registry_db_fallback_not_found_total", "mode" => mode.to_string()).increment(1);
            Err(Status::failed_precondition("ACCESS_ZONE_NOT_FOUND"))
        }
    }
}

async fn resolve_single_id_fresh(
    pool: &PgPool,
    id: &str,
    mode: &str,
) -> Result<ResolvedAccessZone, Status> {
    let uuid = Uuid::parse_str(id.trim())
        .map_err(|_| Status::invalid_argument("access_zone_id must be UUID"))?;
    match lookup_zone_by_id_any_status(pool, uuid).await? {
        Some((zone, status)) if status == "ACTIVE" => {
            counter!("access_zone_registry_resolve_total", "mode" => mode.to_string(), "source" => "db_strict").increment(1);
            Ok(zone)
        }
        Some((_zone, status)) => Err(zone_status_error(&status, mode)),
        None => {
            counter!("access_zone_registry_db_fallback_not_found_total", "mode" => mode.to_string()).increment(1);
            Err(Status::failed_precondition("ACCESS_ZONE_NOT_FOUND"))
        }
    }
}

pub async fn resolve_single_code(
    pool: &PgPool,
    cfg: &AppConfig,
    code: &str,
) -> Result<ResolvedAccessZone, Status> {
    let code = code.trim();
    if !is_valid_access_zone_code(code) {
        counter!("access_zone_code_invalid_total").increment(1);
        return Err(Status::invalid_argument(
            "access_zone_code must match ^[0-9]{4}$",
        ));
    }
    let snap = snapshot(pool, cfg).await?;
    if let Some(zone) = snap.by_code.get(code).cloned() {
        // P1 hardening: cached ACTIVE zone is not authoritative for status revocation.
        // Re-check DB for DISABLED/DELETED transitions at a short interval so access
        // revocation is not delayed by the full snapshot TTL.
        if cfg.access_zone_registry.active_recheck_interval_ms == 0
            || snap.loaded_at.elapsed()
                >= Duration::from_millis(cfg.access_zone_registry.active_recheck_interval_ms)
        {
            counter!("access_zone_registry_db_recheck_total", "mode" => "code").increment(1);
            match lookup_zone_by_code_any_status(pool, code).await? {
                Some((fresh, status)) if status == "ACTIVE" => {
                    counter!("access_zone_registry_resolve_total", "mode" => "code", "source" => "cache_rechecked").increment(1);
                    return Ok(fresh);
                }
                Some((_fresh, status)) => {
                    invalidate_cache().await;
                    counter!("access_zone_registry_stale_active_rejected_total", "mode" => "code", "status" => status.clone()).increment(1);
                    return Err(zone_status_error(&status, "code"));
                }
                None => {
                    invalidate_cache().await;
                    counter!("access_zone_registry_stale_active_rejected_total", "mode" => "code", "status" => "MISSING").increment(1);
                    return Err(Status::failed_precondition("ACCESS_ZONE_NOT_FOUND"));
                }
            }
        }
        counter!("access_zone_registry_resolve_total", "mode" => "code", "source" => "cache")
            .increment(1);
        return Ok(zone);
    }

    counter!("access_zone_registry_cache_stale_miss_total", "mode" => "code").increment(1);
    match lookup_zone_by_code_any_status(pool, code).await? {
        Some((zone, status)) if status == "ACTIVE" => {
            counter!("access_zone_registry_db_fallback_found_total", "mode" => "code").increment(1);
            invalidate_cache().await;
            counter!("access_zone_registry_resolve_total", "mode" => "code", "source" => "db_fallback").increment(1);
            Ok(zone)
        }
        Some((_zone, status)) => Err(zone_status_error(&status, "code")),
        None => {
            counter!("access_zone_registry_db_fallback_not_found_total", "mode" => "code")
                .increment(1);
            counter!("access_zone_registry_resolve_failed_total", "reason" => "code_not_found")
                .increment(1);
            Err(Status::failed_precondition("ACCESS_ZONE_NOT_FOUND"))
        }
    }
}

pub async fn resolve_single_id(
    pool: &PgPool,
    cfg: &AppConfig,
    id: &str,
) -> Result<ResolvedAccessZone, Status> {
    let uuid = Uuid::parse_str(id.trim())
        .map_err(|_| Status::invalid_argument("access_zone_id must be UUID"))?;
    let snap = snapshot(pool, cfg).await?;
    if let Some(zone) = snap.by_id.get(&uuid).cloned() {
        if cfg.access_zone_registry.active_recheck_interval_ms == 0
            || snap.loaded_at.elapsed()
                >= Duration::from_millis(cfg.access_zone_registry.active_recheck_interval_ms)
        {
            counter!("access_zone_registry_db_recheck_total", "mode" => "uuid").increment(1);
            match lookup_zone_by_id_any_status(pool, uuid).await? {
                Some((fresh, status)) if status == "ACTIVE" => {
                    counter!("access_zone_registry_resolve_total", "mode" => "uuid", "source" => "cache_rechecked").increment(1);
                    return Ok(fresh);
                }
                Some((_fresh, status)) => {
                    invalidate_cache().await;
                    counter!("access_zone_registry_stale_active_rejected_total", "mode" => "uuid", "status" => status.clone()).increment(1);
                    return Err(zone_status_error(&status, "uuid"));
                }
                None => {
                    invalidate_cache().await;
                    counter!("access_zone_registry_stale_active_rejected_total", "mode" => "uuid", "status" => "MISSING").increment(1);
                    return Err(Status::failed_precondition("ACCESS_ZONE_NOT_FOUND"));
                }
            }
        }
        counter!("access_zone_registry_resolve_total", "mode" => "uuid", "source" => "cache")
            .increment(1);
        return Ok(zone);
    }

    counter!("access_zone_registry_cache_stale_miss_total", "mode" => "uuid").increment(1);
    match lookup_zone_by_id_any_status(pool, uuid).await? {
        Some((zone, status)) if status == "ACTIVE" => {
            counter!("access_zone_registry_db_fallback_found_total", "mode" => "uuid").increment(1);
            invalidate_cache().await;
            counter!("access_zone_registry_resolve_total", "mode" => "uuid", "source" => "db_fallback").increment(1);
            Ok(zone)
        }
        Some((_zone, status)) => Err(zone_status_error(&status, "uuid")),
        None => {
            counter!("access_zone_registry_db_fallback_not_found_total", "mode" => "uuid")
                .increment(1);
            counter!("access_zone_registry_resolve_failed_total", "reason" => "id_not_found")
                .increment(1);
            Err(Status::failed_precondition("ACCESS_ZONE_NOT_FOUND"))
        }
    }
}

pub async fn resolve_request_zones(
    pool: &PgPool,
    cfg: &AppConfig,
    legacy_id: &str,
    legacy_ids: &[String],
    code: &str,
    codes: &[String],
) -> Result<Vec<ResolvedAccessZone>, Status> {
    let has_codes = !code.trim().is_empty() || !codes.is_empty();
    let has_ids = !legacy_id.trim().is_empty() || !legacy_ids.is_empty();

    let mut resolved = Vec::new();
    if has_codes {
        let mut raw_codes = Vec::new();
        if !codes.is_empty() {
            raw_codes.extend(codes.iter().cloned());
        }
        if !code.trim().is_empty() {
            raw_codes.push(code.to_string());
        }
        let mut seen = HashSet::new();
        for raw in raw_codes {
            let c = raw.trim().to_string();
            if seen.insert(c.clone()) {
                resolved.push(resolve_single_code(pool, cfg, &c).await?);
            }
        }
        if has_ids {
            let mut raw_ids = Vec::new();
            if !legacy_ids.is_empty() {
                raw_ids.extend(legacy_ids.iter().cloned());
            }
            if !legacy_id.trim().is_empty() {
                raw_ids.push(legacy_id.to_string());
            }
            let mut id_zones = Vec::new();
            let mut seen_ids = HashSet::new();
            for raw in raw_ids {
                let z = resolve_single_id(pool, cfg, raw.trim()).await?;
                if seen_ids.insert(z.access_zone_id) {
                    id_zones.push(z);
                }
            }
            let expected: HashSet<Uuid> = resolved.iter().map(|z| z.access_zone_id).collect();
            let got: HashSet<Uuid> = id_zones.iter().map(|z| z.access_zone_id).collect();
            if expected != got {
                counter!("access_zone_code_uuid_mismatch_total").increment(1);
                return Err(Status::invalid_argument(
                    "access_zone_code/access_zone_id mismatch",
                ));
            }
        }
    } else if has_ids {
        let mut raw_ids = Vec::new();
        if !legacy_ids.is_empty() {
            raw_ids.extend(legacy_ids.iter().cloned());
        }
        if !legacy_id.trim().is_empty() {
            raw_ids.push(legacy_id.to_string());
        }
        let mut seen = HashSet::new();
        for raw in raw_ids {
            let z = resolve_single_id(pool, cfg, raw.trim()).await?;
            if seen.insert(z.access_zone_id) {
                resolved.push(z);
            }
        }
    } else {
        counter!("access_zone_search_rejected_total", "reason" => "missing").increment(1);
        return Err(Status::invalid_argument(
            "access_zone_code/access_zone_codes or access_zone_id/access_zone_ids is required",
        ));
    }

    if resolved.len() > cfg.access_zones.max_search_access_zones {
        counter!("access_zone_search_rejected_total", "reason" => "too_many_zones").increment(1);
        return Err(Status::invalid_argument(format!(
            "at most {} access zones are allowed",
            cfg.access_zones.max_search_access_zones
        )));
    }
    histogram!("access_zone_search_zones_count").record(resolved.len() as f64);
    if resolved.len() > 10 {
        counter!("access_zone_search_large_zone_set_total").increment(1);
    }
    Ok(resolved)
}

async fn fetch_active_zone_by_code(
    pool: &PgPool,
    code: &str,
) -> Result<Option<ResolvedAccessZone>, Status> {
    match lookup_zone_by_code_any_status(pool, code).await? {
        Some((zone, status)) if status == "ACTIVE" => Ok(Some(zone)),
        Some((_zone, status)) => Err(zone_status_error(&status, "code")),
        None => Ok(None),
    }
}

async fn invalidate_cache() {
    let mut guard = cache().write().await;
    *guard = None;
}

async fn create_access_zone_from_code(
    pool: &PgPool,
    cfg: &AppConfig,
    code: &str,
) -> Result<ResolvedAccessZone, Status> {
    if !is_valid_access_zone_code(code) {
        counter!("access_zone_code_invalid_total").increment(1);
        counter!("access_zone_auto_create_denied_total", "reason" => "invalid_code").increment(1);
        return Err(Status::invalid_argument(
            "access_zone_code must match ^[0-9]{4}$",
        ));
    }

    if !cfg.access_zone_registry.auto_create_on_ingestion {
        counter!("access_zone_auto_create_denied_total", "reason" => "config_disabled")
            .increment(1);
        return Err(Status::failed_precondition("ACCESS_ZONE_NOT_FOUND"));
    }

    counter!("access_zone_auto_create_attempt_total", "reason" => "ingestion").increment(1);
    let ttl_days = default_ttl_days_from_access_zone_code(code).map_err(Status::from)?;
    let allow_never_expire = ttl_days == 0;
    let status = cfg.access_zone_registry.auto_create_default_status.as_str();
    let new_id = Uuid::new_v4();

    let inserted = sqlx::query(
        "INSERT INTO astravector.access_zones (
            access_zone_id, access_zone_code, access_zone_name, status,
            default_ttl_days, ttl_policy_source, allow_never_expire,
            auto_created, created_reason, first_seen_at, last_seen_at, created_at, updated_at
         ) VALUES ($1,$2,$3,$4,$5,'CODE_MATRIX',$6,true,'INGESTION_AUTO_CREATE',now(),now(),now(),now())
         ON CONFLICT (access_zone_code) DO NOTHING"
    )
    .bind(new_id)
    .bind(code)
    .bind(format!("auto-zone-{code}"))
    .bind(status)
    .bind(ttl_days as i32)
    .bind(allow_never_expire)
    .execute(pool)
    .await
    .map_err(|e| {
        counter!("access_zone_auto_create_failed_total", "reason" => "db_error").increment(1);
        Status::unavailable(format!("access zone auto-create: {e}"))
    })?
    .rows_affected();

    if inserted == 0 {
        match lookup_zone_by_code_any_status(pool, code).await? {
            Some((zone, existing_status)) if existing_status == "ACTIVE" => {
                counter!("access_zone_auto_create_conflict_active_total").increment(1);
                invalidate_cache().await;
                return Ok(zone);
            }
            Some((_zone, existing_status)) if existing_status == "DISABLED" => {
                counter!("access_zone_auto_create_conflict_disabled_total").increment(1);
                return Err(Status::failed_precondition(
                    "ACCESS_ZONE_ALREADY_EXISTS_DISABLED",
                ));
            }
            Some((_zone, existing_status)) if existing_status == "DELETED" => {
                counter!("access_zone_auto_create_conflict_deleted_total").increment(1);
                return Err(Status::failed_precondition(
                    "ACCESS_ZONE_ALREADY_EXISTS_DELETED",
                ));
            }
            Some((_zone, existing_status)) => {
                counter!("access_zone_auto_create_conflict_not_active_total", "status" => existing_status.clone()).increment(1);
                return Err(Status::failed_precondition(format!(
                    "ACCESS_ZONE_ALREADY_EXISTS_NOT_ACTIVE: {existing_status}"
                )));
            }
            None => {
                counter!("access_zone_auto_create_failed_total", "reason" => "conflict_missing_after_insert").increment(1);
                return Err(Status::unavailable(
                    "access zone auto-create conflict but existing row was not found",
                ));
            }
        }
    }

    sqlx::query("UPDATE astravector.access_zones SET last_seen_at=now() WHERE access_zone_code=$1")
        .bind(code)
        .execute(pool)
        .await
        .map_err(|e| Status::unavailable(format!("access zone last_seen update: {e}")))?;

    invalidate_cache().await;

    let Some(zone) = fetch_active_zone_by_code(pool, code).await? else {
        if status == "DISABLED" {
            counter!("access_zone_auto_create_success_total", "status" => "disabled").increment(1);
            return Err(Status::failed_precondition(
                "access_zone_code auto-created as DISABLED and must be activated before ingestion",
            ));
        }
        counter!("access_zone_auto_create_failed_total", "reason" => "created_not_active")
            .increment(1);
        return Err(Status::failed_precondition(
            "access_zone_code is not ACTIVE after auto-create",
        ));
    };

    counter!("access_zone_auto_create_success_total", "status" => "active").increment(1);
    counter!("access_zone_auto_created_total").increment(1);
    counter!("access_zone_code_matrix_ttl_assigned_total").increment(1);
    Ok(zone)
}

pub async fn resolve_or_create_ingestion_zone(
    pool: &PgPool,
    cfg: &AppConfig,
    legacy_id: &str,
    code: &str,
) -> Result<ResolvedAccessZone, Status> {
    let trimmed_code = code.trim();
    if !trimmed_code.is_empty() {
        let resolved = if cfg.access_zone_registry.always_recheck_on_ingestion {
            resolve_single_code_fresh(pool, trimmed_code, "ingestion_code").await
        } else {
            resolve_single_code(pool, cfg, trimmed_code).await
        };
        match resolved {
            Ok(zone) => {
                if !legacy_id.trim().is_empty() {
                    let legacy_zone = if cfg.access_zone_registry.always_recheck_on_ingestion {
                        resolve_single_id_fresh(pool, legacy_id.trim(), "ingestion_uuid").await?
                    } else {
                        resolve_single_id(pool, cfg, legacy_id.trim()).await?
                    };
                    if legacy_zone.access_zone_id != zone.access_zone_id {
                        counter!("access_zone_code_uuid_mismatch_total").increment(1);
                        return Err(Status::invalid_argument(
                            "access_zone_code/access_zone_id mismatch",
                        ));
                    }
                }
                return Ok(zone);
            }
            Err(status)
                if status.code() == tonic::Code::FailedPrecondition
                    && status.message().contains("ACCESS_ZONE_NOT_FOUND") =>
            {
                let zone = create_access_zone_from_code(pool, cfg, trimmed_code).await?;
                if !legacy_id.trim().is_empty() {
                    let legacy_uuid = Uuid::parse_str(legacy_id.trim())
                        .map_err(|_| Status::invalid_argument("access_zone_id must be UUID"))?;
                    if legacy_uuid != zone.access_zone_id {
                        counter!("access_zone_code_uuid_mismatch_total").increment(1);
                        return Err(Status::invalid_argument(
                            "access_zone_code/access_zone_id mismatch",
                        ));
                    }
                }
                return Ok(zone);
            }
            Err(status) => return Err(status),
        }
    }

    if cfg.access_zone_registry.always_recheck_on_ingestion {
        let zone = resolve_single_id_fresh(pool, legacy_id.trim(), "ingestion_uuid").await?;
        return Ok(zone);
    }
    let zones = resolve_request_zones(pool, cfg, legacy_id, &[], "", &[]).await?;
    if zones.len() != 1 {
        return Err(Status::invalid_argument(
            "ingestion requires exactly one access zone",
        ));
    }
    Ok(zones[0].clone())
}

pub async fn resolve_ingestion_zone(
    pool: &PgPool,
    cfg: &AppConfig,
    legacy_id: &str,
    code: &str,
) -> Result<ResolvedAccessZone, Status> {
    if cfg.access_zone_registry.always_recheck_on_ingestion {
        if !code.trim().is_empty() {
            let zone = resolve_single_code_fresh(pool, code.trim(), "ingestion_code").await?;
            if !legacy_id.trim().is_empty() {
                let legacy_zone =
                    resolve_single_id_fresh(pool, legacy_id.trim(), "ingestion_uuid").await?;
                if legacy_zone.access_zone_id != zone.access_zone_id {
                    counter!("access_zone_code_uuid_mismatch_total").increment(1);
                    return Err(Status::invalid_argument(
                        "access_zone_code/access_zone_id mismatch",
                    ));
                }
            }
            return Ok(zone);
        }
        let zone = resolve_single_id_fresh(pool, legacy_id.trim(), "ingestion_uuid").await?;
        return Ok(zone);
    }
    let zones = resolve_request_zones(pool, cfg, legacy_id, &[], code, &[]).await?;
    if zones.len() != 1 {
        return Err(Status::invalid_argument(
            "ingestion requires exactly one access zone",
        ));
    }
    Ok(zones[0].clone())
}

#[cfg(test)]
mod tests {
    use super::{default_ttl_days_from_access_zone_code, is_valid_access_zone_code};

    #[test]
    fn access_zone_code_validation_accepts_exactly_four_digits() {
        assert!(is_valid_access_zone_code("0000"));
        assert!(is_valid_access_zone_code("1500"));
        assert!(is_valid_access_zone_code("9999"));
        assert!(!is_valid_access_zone_code("150"));
        assert!(!is_valid_access_zone_code("15000"));
        assert!(!is_valid_access_zone_code("15A0"));
    }

    #[test]
    fn code_matrix_ttl_boundaries_are_stable() {
        assert_eq!(default_ttl_days_from_access_zone_code("0001").unwrap(), 0);
        assert_eq!(default_ttl_days_from_access_zone_code("1000").unwrap(), 182);
        assert_eq!(default_ttl_days_from_access_zone_code("1499").unwrap(), 182);
        assert_eq!(default_ttl_days_from_access_zone_code("1500").unwrap(), 365);
        assert_eq!(default_ttl_days_from_access_zone_code("1999").unwrap(), 365);
        assert_eq!(default_ttl_days_from_access_zone_code("2500").unwrap(), 730);
        assert_eq!(
            default_ttl_days_from_access_zone_code("9500").unwrap(),
            3650
        );
    }
}
