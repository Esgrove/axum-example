//! Status service.
//!
//! Collects inexpensive in-process checks and request counters,
//! and renders them for the `/health` and `/status` endpoints.
//! Every check is rolled up into a [`HealthSummary`].
//!
//! Container-level CPU and memory metrics belong to the hosting platform,
//! so they are intentionally not collected here.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::json;

use crate::config::{
    API_ENV_ENV_VAR, API_KEY_ENV_VAR, Config, RATE_LIMIT_DISABLED_ENV_VAR, RATE_LIMIT_REQUESTS_ENV_VAR,
    RATE_LIMIT_WINDOW_SECONDS_ENV_VAR, REQUEST_TIMEOUT_SECONDS_ENV_VAR,
};
use crate::schemas::{
    EnvVarStatus, EnvVariableEntry, HealthCheck, HealthStatus, HealthSummary, RequestCounters, StatusConfig,
    StatusResponse,
};
use crate::utils::{decrement_saturating, duration_millis, increment_saturating, read_environment_variable};
use crate::version;

/// Env vars whose presence `/status` reports.
///
/// Only names are reported, never values.
pub const REPORTED_ENV_VARS: &[&str] = &[
    API_KEY_ENV_VAR,
    API_ENV_ENV_VAR,
    RATE_LIMIT_REQUESTS_ENV_VAR,
    RATE_LIMIT_WINDOW_SECONDS_ENV_VAR,
    RATE_LIMIT_DISABLED_ENV_VAR,
    REQUEST_TIMEOUT_SECONDS_ENV_VAR,
    crate::telemetry::OTEL_EXPORTER_OTLP_ENDPOINT_ENV_VAR,
];

/// Process-wide status tracker.
#[derive(Debug)]
pub struct StatusService {
    start_time: Instant,
    start_time_utc: DateTime<Utc>,
    requests_total: AtomicU64,
    requests_in_progress: AtomicU64,
    requests_errors: AtomicU64,
    requests_rate_limited: AtomicU64,
}

impl Default for StatusService {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusService {
    /// Build a status service, capturing the monotonic and wall-clock start times.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            start_time_utc: Utc::now(),
            requests_total: AtomicU64::new(0),
            requests_in_progress: AtomicU64::new(0),
            requests_errors: AtomicU64::new(0),
            requests_rate_limited: AtomicU64::new(0),
        }
    }

    /// Process uptime in whole seconds.
    #[must_use]
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Record a request starting and return the updated in-flight count.
    pub fn record_request_started(&self) -> u64 {
        increment_saturating(&self.requests_total);
        increment_saturating(&self.requests_in_progress)
    }

    /// Record a request settling, whether it completed or was cancelled.
    /// Returns the updated in-flight count.
    pub fn record_request_settled(&self) -> u64 {
        decrement_saturating(&self.requests_in_progress)
    }

    /// Record a completed request with an error status.
    pub fn record_error(&self) {
        increment_saturating(&self.requests_errors);
    }

    /// Record a request rejected by the rate limiter.
    pub fn record_rate_limited(&self) {
        increment_saturating(&self.requests_rate_limited);
    }

    /// Snapshot of the request counters.
    #[must_use]
    pub fn counters(&self) -> RequestCounters {
        RequestCounters {
            total: self.requests_total.load(Ordering::Relaxed),
            in_progress: self.requests_in_progress.load(Ordering::Relaxed),
            errors: self.requests_errors.load(Ordering::Relaxed),
            rate_limited: self.requests_rate_limited.load(Ordering::Relaxed),
        }
    }

    /// Uptime check.
    ///
    /// Always healthy.
    /// The metrics contain the uptime breakdown and the start timestamp.
    #[must_use]
    pub fn check_uptime(&self) -> HealthCheck {
        let uptime_ms = duration_millis(self.start_time.elapsed());
        let uptime_sec = uptime_ms / 1000;
        HealthCheck {
            healthy: true,
            status: "ok".to_string(),
            metrics: json!({
                "uptimeMs": uptime_ms,
                "uptimeSec": uptime_sec,
                "uptimeMin": uptime_sec / 60,
                "uptimeHours": uptime_sec / 3600,
                "uptimeDays": uptime_sec / 86_400,
                "startTimeISO": self.start_time_utc.to_rfc3339_opts(SecondsFormat::Millis, true),
            }),
        }
    }

    /// Item store check.
    ///
    /// The in-memory store is always reachable,
    /// but the check shows where a real database ping would go.
    #[must_use]
    pub fn check_item_store(item_count: usize) -> HealthCheck {
        HealthCheck {
            healthy: true,
            status: "ok".to_string(),
            metrics: json!({ "items": item_count }),
        }
    }

    /// Run every health check.
    #[must_use]
    pub fn collect_checks(&self, item_count: usize) -> HealthSummary {
        let mut checks = BTreeMap::new();
        checks.insert("uptime".to_string(), self.check_uptime());
        checks.insert("itemStore".to_string(), Self::check_item_store(item_count));
        let overall = checks.values().all(|check| check.healthy);
        HealthSummary { overall, checks }
    }

    /// Render the full `/status` payload.
    #[must_use]
    pub fn get_status(&self, config: &Config, item_count: usize) -> StatusResponse {
        StatusResponse {
            service: version::PACKAGE_NAME.to_string(),
            version: version::PACKAGE_VERSION.to_string(),
            environment: config.env.to_string(),
            timestamp: now_rfc3339(),
            health: self.collect_checks(item_count),
            requests: self.counters(),
            items: item_count,
            config: StatusConfig {
                rate_limit_enabled: config.rate_limit.enabled,
                rate_limit_requests: config.rate_limit.max_requests,
                rate_limit_window_seconds: config.rate_limit.window.as_secs(),
                request_timeout_seconds: config.request_timeout.as_secs(),
                docs_enabled: config.docs_enabled(),
            },
            environment_variables: env_var_status(read_environment_variable),
        }
    }
}

/// Map a health summary to the overall verdict.
#[must_use]
pub const fn health_status(summary: &HealthSummary) -> HealthStatus {
    if summary.overall {
        HealthStatus::Healthy
    } else {
        HealthStatus::Unhealthy
    }
}

/// Current UTC time in RFC 3339 format with millisecond precision.
#[must_use]
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Report the presence of every name in [`REPORTED_ENV_VARS`] using `lookup`.
pub fn env_var_status(lookup: impl Fn(&str) -> Option<String>) -> EnvVarStatus {
    let variables: Vec<EnvVariableEntry> = REPORTED_ENV_VARS
        .iter()
        .map(|name| EnvVariableEntry {
            name: (*name).to_string(),
            available: lookup(name).is_some(),
        })
        .collect();
    EnvVarStatus {
        total_available: variables.iter().filter(|entry| entry.available).count(),
        total: variables.len(),
        variables,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_check_is_healthy_with_breakdown_fields() {
        let check = StatusService::new().check_uptime();
        assert!(check.healthy);
        for key in [
            "uptimeMs",
            "uptimeSec",
            "uptimeMin",
            "uptimeHours",
            "uptimeDays",
            "startTimeISO",
        ] {
            assert!(check.metrics.get(key).is_some(), "missing key {key}");
        }
    }

    #[test]
    fn counters_track_requests_errors_and_rejections() {
        let service = StatusService::new();
        assert_eq!(service.record_request_started(), 1);
        assert_eq!(service.record_request_started(), 2);
        assert_eq!(service.record_request_settled(), 1);
        service.record_error();
        service.record_rate_limited();

        let counters = service.counters();
        assert_eq!(counters.total, 2);
        assert_eq!(counters.in_progress, 1);
        assert_eq!(counters.errors, 1);
        assert_eq!(counters.rate_limited, 1);
    }

    #[test]
    fn status_includes_checks_and_config_summary() {
        let status = StatusService::new().get_status(&Config::default(), 3);
        assert!(status.health.overall);
        assert!(status.health.checks.contains_key("uptime"));
        assert!(status.health.checks.contains_key("itemStore"));
        assert_eq!(status.items, 3);
        assert!(status.config.rate_limit_enabled);
        assert_eq!(health_status(&status.health), HealthStatus::Healthy);
    }

    #[test]
    fn env_var_status_reports_names_only() {
        let status = env_var_status(|name| (name == API_KEY_ENV_VAR).then(|| "secret".to_string()));
        assert_eq!(status.total, REPORTED_ENV_VARS.len());
        assert_eq!(status.total_available, 1);
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(!json.contains("secret"));
    }
}
