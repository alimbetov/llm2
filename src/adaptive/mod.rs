use crate::config::{AdaptiveConfig, AdaptivePolicyConfig};
use metrics::{counter, gauge};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptiveMode {
    Off,
    DryRun,
    AutoSafe,
}

impl AdaptiveMode {
    pub fn from_config_value(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "DRY_RUN" => Self::DryRun,
            "AUTO_SAFE" => Self::AutoSafe,
            _ => Self::Off,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::DryRun => "DRY_RUN",
            Self::AutoSafe => "AUTO_SAFE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeOverride {
    pub parameter: String,
    pub value: u64,
    pub previous_value: u64,
    pub reason: String,
    pub source: String,
    pub applied_at: Instant,
    pub expires_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy)]
pub enum TuningAction {
    Increase,
    Decrease,
}

impl TuningAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Increase => "increase",
            Self::Decrease => "decrease",
        }
    }
}

#[derive(Clone)]
pub struct AdaptiveRuntime {
    config: AdaptiveConfig,
    mode: AdaptiveMode,
    overrides: Arc<RwLock<HashMap<String, RuntimeOverride>>>,
    last_change: Arc<RwLock<HashMap<String, Instant>>>,
}

impl AdaptiveRuntime {
    pub fn new(config: AdaptiveConfig) -> Self {
        let mode = AdaptiveMode::from_config_value(&config.mode);
        gauge!("astravector_adaptive_mode", "mode" => mode.as_str()).set(match mode {
            AdaptiveMode::Off => 0.0,
            AdaptiveMode::DryRun => 1.0,
            AdaptiveMode::AutoSafe => 2.0,
        });
        Self {
            config,
            mode,
            overrides: Arc::new(RwLock::new(HashMap::new())),
            last_change: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn mode(&self) -> AdaptiveMode {
        self.mode
    }
    pub fn is_enabled(&self) -> bool {
        self.mode != AdaptiveMode::Off
    }

    pub fn get_u64(&self, parameter: &str, default: u64) -> u64 {
        self.prune_expired();
        self.overrides
            .read()
            .ok()
            .and_then(|m| m.get(parameter).map(|o| o.value))
            .unwrap_or(default)
    }

    pub fn get_i64(&self, parameter: &str, default: i64) -> i64 {
        self.get_u64(parameter, default.max(1) as u64) as i64
    }

    pub fn observe_qdrant_scroll(
        &self,
        pages: u64,
        latency_secs: f64,
        error_reason: Option<&str>,
        current_default: u64,
    ) {
        if self.mode == AdaptiveMode::Off || !self.config.policies.qdrant_scroll_page_size.enabled {
            return;
        }
        if let Some(reason) = error_reason {
            counter!("astravector_adaptive_observations_total", "parameter" => "qdrant.scroll_page_size", "result" => "error", "reason" => reason.to_string()).increment(1);
            if reason == "timeout" || reason == "limit_exceeded" || latency_secs > 10.0 {
                self.try_adjust(
                    "qdrant.scroll_page_size",
                    &self.config.policies.qdrant_scroll_page_size,
                    current_default,
                    TuningAction::Decrease,
                    "qdrant_scroll_error_or_latency_regression",
                );
            }
            return;
        }
        counter!("astravector_adaptive_observations_total", "parameter" => "qdrant.scroll_page_size", "result" => "success", "reason" => "ok").increment(1);
        if pages > 20 && latency_secs > 5.0 {
            self.try_adjust(
                "qdrant.scroll_page_size",
                &self.config.policies.qdrant_scroll_page_size,
                current_default,
                TuningAction::Increase,
                "scroll_pages_high_latency_high",
            );
        } else if latency_secs > 10.0 {
            self.try_adjust(
                "qdrant.scroll_page_size",
                &self.config.policies.qdrant_scroll_page_size,
                current_default,
                TuningAction::Decrease,
                "scroll_latency_too_high",
            );
        }
    }

    pub fn observe_outbox_claim(&self, claimed: usize, default_batch_size: i64) {
        if self.mode == AdaptiveMode::Off || !self.config.policies.publisher_batch_size.enabled {
            return;
        }
        let effective = self
            .get_i64("publisher.batch_size", default_batch_size)
            .max(1) as usize;
        if claimed >= effective && effective > 0 {
            self.try_adjust(
                "publisher.batch_size",
                &self.config.policies.publisher_batch_size,
                default_batch_size.max(1) as u64,
                TuningAction::Increase,
                "outbox_claimed_full_batch",
            );
        } else if claimed == 0 && self.config.policies.outbox_poll_interval_ms.enabled {
            self.try_adjust(
                "outbox.poll_interval_ms",
                &self.config.policies.outbox_poll_interval_ms,
                self.config.default_outbox_poll_interval_ms.max(100),
                TuningAction::Increase,
                "outbox_empty_reduce_polling_pressure",
            );
        }
    }

    pub fn observe_outbox_error(&self, default_batch_size: i64) {
        if self.mode == AdaptiveMode::Off || !self.config.policies.publisher_batch_size.enabled {
            return;
        }
        self.try_adjust(
            "publisher.batch_size",
            &self.config.policies.publisher_batch_size,
            default_batch_size.max(1) as u64,
            TuningAction::Decrease,
            "outbox_or_qdrant_error_reduce_batch",
        );
    }

    pub fn current_overrides(&self) -> Vec<RuntimeOverride> {
        self.prune_expired();
        self.overrides
            .read()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    fn try_adjust(
        &self,
        parameter: &str,
        policy: &AdaptivePolicyConfig,
        default_value: u64,
        action: TuningAction,
        reason: &str,
    ) {
        if !policy.enabled || !Self::is_safe_parameter(parameter) {
            self.reject(parameter, action, "FORBIDDEN_OR_DISABLED_PARAMETER");
            return;
        }
        let current = self.get_u64(parameter, default_value);
        if self.cooldown_active(parameter, policy.cooldown_secs) {
            self.reject(parameter, action, "COOLDOWN_ACTIVE");
            return;
        }
        let proposed = match action {
            TuningAction::Increase => current.saturating_add(policy.step),
            TuningAction::Decrease => current.saturating_sub(policy.step),
        };
        if proposed < policy.min || proposed > policy.max || proposed == current {
            self.reject(parameter, action, "GUARDRAIL_BOUNDARY");
            return;
        }
        match self.mode {
            AdaptiveMode::Off => {}
            AdaptiveMode::DryRun => {
                counter!("astravector_adaptive_dry_run_decisions_total", "parameter" => parameter.to_string(), "action" => action.as_str()).increment(1);
                info!(event="ADAPTIVE_TUNING_DRY_RUN", parameter, old_value=current, proposed_value=proposed, reason, mode=%self.mode.as_str(), "adaptive tuning dry run decision");
            }
            AdaptiveMode::AutoSafe => {
                let now = Instant::now();
                let expires_at = Some(now + Duration::from_secs(policy.ttl_secs.max(1)));
                let override_value = RuntimeOverride {
                    parameter: parameter.to_string(),
                    value: proposed,
                    previous_value: current,
                    reason: reason.to_string(),
                    source: "AUTO_SAFE".to_string(),
                    applied_at: now,
                    expires_at,
                };
                if let Ok(mut map) = self.overrides.write() {
                    map.insert(parameter.to_string(), override_value);
                }
                if let Ok(mut last) = self.last_change.write() {
                    last.insert(parameter.to_string(), now);
                }
                counter!("astravector_adaptive_decisions_total", "parameter" => parameter.to_string(), "action" => action.as_str(), "result" => "applied").increment(1);
                counter!("astravector_adaptive_applied_overrides_total", "parameter" => parameter.to_string()).increment(1);
                gauge!("astravector_adaptive_current_value", "parameter" => parameter.to_string())
                    .set(proposed as f64);
                info!(event="ADAPTIVE_TUNING_DECISION_APPLIED", parameter, old_value=current, new_value=proposed, reason, mode=%self.mode.as_str(), ttl_secs=policy.ttl_secs, "adaptive tuning decision applied");
            }
        }
    }

    fn cooldown_active(&self, parameter: &str, cooldown_secs: u64) -> bool {
        self.last_change
            .read()
            .ok()
            .and_then(|m| m.get(parameter).copied())
            .map(|last| last.elapsed() < Duration::from_secs(cooldown_secs))
            .unwrap_or(false)
    }

    fn reject(&self, parameter: &str, action: TuningAction, reason: &str) {
        counter!("astravector_adaptive_rejected_decisions_total", "parameter" => parameter.to_string(), "reason" => reason.to_string()).increment(1);
        counter!("astravector_adaptive_decisions_total", "parameter" => parameter.to_string(), "action" => action.as_str(), "result" => "rejected").increment(1);
        warn!(event="ADAPTIVE_TUNING_DECISION_REJECTED", parameter, action=%action.as_str(), reason, "adaptive tuning decision rejected");
    }

    fn prune_expired(&self) {
        let now = Instant::now();
        if let Ok(mut map) = self.overrides.write() {
            let expired: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| v.expires_at.filter(|e| *e <= now).map(|_| k.clone()))
                .collect();
            for key in expired {
                if let Some(removed) = map.remove(&key) {
                    counter!("astravector_adaptive_expired_overrides_total", "parameter" => removed.parameter.clone()).increment(1);
                    info!(event="ADAPTIVE_TUNING_OVERRIDE_EXPIRED", parameter=%removed.parameter, value=removed.value, previous_value=removed.previous_value, reason=%removed.reason, source=%removed.source, age_secs=removed.applied_at.elapsed().as_secs(), "adaptive tuning override expired");
                }
            }
        }
    }

    fn is_safe_parameter(parameter: &str) -> bool {
        matches!(
            parameter,
            "qdrant.scroll_page_size"
                | "qdrant.scroll_max_concurrency"
                | "publisher.batch_size"
                | "outbox.poll_interval_ms"
                | "embedding.batch_size"
                | "qdrant.timeout_ms"
                | "max_concurrent_search"
                | "max_concurrent_indexing"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AdaptiveConfig, AdaptivePoliciesConfig, AdaptivePolicyConfig};

    fn policy(enabled: bool, min: u64, max: u64, step: u64) -> AdaptivePolicyConfig {
        AdaptivePolicyConfig {
            enabled,
            min,
            max,
            step,
            cooldown_secs: 0,
            ttl_secs: 60,
        }
    }

    fn config(mode: &str) -> AdaptiveConfig {
        AdaptiveConfig {
            mode: mode.to_string(),
            window_secs: 300,
            default_outbox_poll_interval_ms: 500,
            policies: AdaptivePoliciesConfig {
                qdrant_scroll_page_size: policy(true, 500, 5000, 500),
                qdrant_scroll_max_concurrency: policy(false, 1, 16, 1),
                publisher_batch_size: policy(true, 10, 500, 25),
                outbox_poll_interval_ms: policy(true, 100, 5000, 100),
                embedding_batch_size: policy(false, 1, 64, 1),
                qdrant_timeout_ms: policy(false, 500, 30000, 500),
                max_concurrent_search: policy(false, 4, 256, 4),
                max_concurrent_indexing: policy(false, 1, 64, 1),
            },
        }
    }

    #[test]
    fn dry_run_does_not_apply_override() {
        let runtime = AdaptiveRuntime::new(config("DRY_RUN"));
        runtime.observe_qdrant_scroll(25, 6.0, None, 1000);
        assert!(runtime.current_overrides().is_empty());
        assert_eq!(runtime.get_u64("qdrant.scroll_page_size", 1000), 1000);
    }

    #[test]
    fn auto_safe_applies_only_safe_runtime_override() {
        let runtime = AdaptiveRuntime::new(config("AUTO_SAFE"));
        runtime.observe_qdrant_scroll(25, 6.0, None, 1000);
        assert_eq!(runtime.get_u64("qdrant.scroll_page_size", 1000), 1500);
    }

    #[test]
    fn guardrail_blocks_out_of_range_override() {
        let runtime = AdaptiveRuntime::new(config("AUTO_SAFE"));
        runtime.observe_qdrant_scroll(25, 6.0, None, 5000);
        assert_eq!(runtime.get_u64("qdrant.scroll_page_size", 5000), 5000);
    }
}
