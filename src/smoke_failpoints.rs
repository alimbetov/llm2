use crate::error::AstraError;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    OnceLock,
};

#[cfg(feature = "smoke-failpoints")]
pub fn hit(name: &str) -> Result<(), AstraError> {
    let enabled = std::env::var("ASTRAVECTOR_SMOKE_FAILPOINTS_ENABLED")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return Ok(());
    }
    let configured = std::env::var("ASTRA_SMOKE_FAILPOINT")
        .or_else(|_| std::env::var("ASTRAVECTOR_SMOKE_FAILPOINT"))
        .unwrap_or_default();
    if configured
        .split(',')
        .map(str::trim)
        .any(|configured_name| configured_name == name)
    {
        return Err(AstraError::Internal(format!("smoke failpoint hit: {name}")));
    }
    Ok(())
}

#[cfg(not(feature = "smoke-failpoints"))]
pub fn hit(_name: &str) -> Result<(), AstraError> {
    Ok(())
}

static QDRANT_FAIL_COUNTER: OnceLock<AtomicUsize> = OnceLock::new();

pub fn qdrant_upsert() -> Result<(), AstraError> {
    let mode = std::env::var("ASTRAVECTOR_SMOKE_QDRANT_FAIL_MODE").unwrap_or_default();
    match mode.as_str() {
        "always_fail" => Err(AstraError::Unavailable(
            "smoke qdrant upsert failure: always_fail".into(),
        )),
        "fail_n_times" => {
            let limit = std::env::var("ASTRAVECTOR_SMOKE_QDRANT_FAIL_COUNT")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            let counter = QDRANT_FAIL_COUNTER.get_or_init(|| AtomicUsize::new(0));
            let current = counter.fetch_add(1, Ordering::SeqCst);
            if current < limit {
                Err(AstraError::Unavailable(format!(
                    "smoke qdrant upsert failure {}/{}",
                    current + 1,
                    limit
                )))
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}
