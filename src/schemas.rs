//! Public schemas.
//!
//! Type definitions for every public-facing request and response body,
//! meaning everything that shows up in the `OpenAPI` documentation.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::version;

/// Static version information compiled into the binary.
pub static VERSION_INFO: VersionInfo = VersionInfo {
    name: version::PACKAGE_NAME,
    version: version::PACKAGE_VERSION,
    deploy_tag: version::DEPLOY_TAG,
    build_time: version::BUILD_TIME,
    branch: version::GIT_BRANCH,
    commit: version::GIT_COMMIT,
    rust_version: version::RUST_VERSION,
};

/// Post payload for creating a new item.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateItem {
    /// Unique item name.
    #[schema(example = "esgrove")]
    pub name: String,
    /// Optional id, letting clients pick an id instead of the server generating one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
}

/// Query an item by name.
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ItemQuery {
    /// Item name to look up.
    #[schema(example = "esgrove")]
    pub name: String,
}

/// Optional pagination parameters for listing items.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
pub struct ItemListQuery {
    /// Number of items to skip.
    #[param(example = 0)]
    pub skip: Option<usize>,
    /// Maximum number of items to return.
    #[param(example = 10)]
    pub limit: Option<usize>,
}

/// Simple response with a message.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MessageResponse {
    /// Informational message.
    #[schema(example = "Removed 3 items")]
    pub message: String,
}

/// Item list response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ItemListResponse {
    /// Total number of items before pagination.
    #[schema(example = 5)]
    pub num_items: usize,
    /// Sorted item names after pagination.
    pub names: Vec<String>,
}

/// Consistent JSON error body returned by every failing route.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Short error label, usually the HTTP reason phrase.
    #[schema(example = "Not Found")]
    pub error: String,
    /// Human-readable error details.
    #[schema(example = "Item does not exist: esgrove")]
    pub message: String,
}

/// API version information.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VersionInfo {
    /// Package name.
    #[schema(example = "axum-example")]
    pub name: &'static str,
    /// Package version.
    #[schema(example = "1.0.0")]
    pub version: &'static str,
    /// Deployment tag injected at build time.
    #[schema(example = "2024.02.14-100")]
    pub deploy_tag: &'static str,
    /// UTC build timestamp.
    #[schema(example = "2024-02-14_14:42:35")]
    pub build_time: &'static str,
    /// Git branch.
    #[schema(example = "main")]
    pub branch: &'static str,
    /// Short git commit hash.
    #[schema(example = "ee9ec80")]
    pub commit: &'static str,
    /// Rust compiler version.
    #[schema(example = "rustc 1.99.1")]
    pub rust_version: &'static str,
}

/// Overall health verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    /// Every check passed.
    Healthy,
    /// At least one check failed.
    Unhealthy,
}

/// Lightweight health response for probes.
///
/// The HTTP status is always `200`, and the verdict is carried in `status`,
/// so load-balancer probes do not flap on a degraded dependency.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Overall verdict.
    pub status: HealthStatus,
    /// Service version.
    #[schema(example = "0.14.0")]
    pub version: String,
    /// Process uptime in whole seconds.
    #[schema(example = 3600)]
    pub uptime_seconds: u64,
    /// Response timestamp in RFC 3339 format.
    #[schema(example = "2026-06-10T09:00:00.000Z")]
    pub timestamp: String,
    /// Checks that failed, keyed by check name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub failed_checks: BTreeMap<String, HealthCheck>,
}

/// Result of one named health check.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthCheck {
    /// Whether the check passed.
    pub healthy: bool,
    /// Short status text.
    #[schema(example = "ok")]
    pub status: String,
    /// Check-specific details.
    #[schema(value_type = Object)]
    pub metrics: Value,
}

/// All health checks rolled up.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthSummary {
    /// `true` when every check is healthy.
    pub overall: bool,
    /// Individual checks keyed by name.
    pub checks: BTreeMap<String, HealthCheck>,
}

/// Presence of one environment variable.
///
/// Only the name is reported, never the value.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EnvVariableEntry {
    /// Variable name.
    #[schema(example = "API_KEY")]
    pub name: String,
    /// Whether the variable is set to a non-blank value.
    pub available: bool,
}

/// Environment variable presence report.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarStatus {
    /// Number of variables that are set.
    pub total_available: usize,
    /// Number of variables checked.
    pub total: usize,
    /// Per-variable presence.
    pub variables: Vec<EnvVariableEntry>,
}

/// Non-secret runtime configuration summary.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusConfig {
    /// Whether per-client rate limiting is enabled.
    pub rate_limit_enabled: bool,
    /// Maximum requests per client per window.
    pub rate_limit_requests: usize,
    /// Rate-limit window in seconds.
    pub rate_limit_window_seconds: u64,
    /// Request timeout in seconds.
    pub request_timeout_seconds: u64,
    /// Whether the documentation UIs are mounted.
    pub docs_enabled: bool,
}

/// Request counters since process start.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestCounters {
    /// Requests that reached the router.
    pub total: u64,
    /// Requests currently in flight.
    pub in_progress: u64,
    /// Completed requests with a 4xx or 5xx status.
    pub errors: u64,
    /// Requests rejected by the rate limiter.
    pub rate_limited: u64,
}

/// Detailed service status.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    /// Service name.
    #[schema(example = "axum-example")]
    pub service: String,
    /// Service version.
    #[schema(example = "0.14.0")]
    pub version: String,
    /// Runtime environment.
    #[schema(example = "LOCAL")]
    pub environment: String,
    /// Response timestamp in RFC 3339 format.
    pub timestamp: String,
    /// Health check results.
    pub health: HealthSummary,
    /// Request counters.
    pub requests: RequestCounters,
    /// Number of items in the in-memory store.
    pub items: usize,
    /// Non-secret configuration summary.
    pub config: StatusConfig,
    /// Environment variable presence.
    pub environment_variables: EnvVarStatus,
}

impl MessageResponse {
    /// Build a message response.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl ErrorResponse {
    /// Build an error body.
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

impl VersionInfo {
    /// Multi-line version information for local logs.
    #[must_use]
    pub fn to_string_pretty(&self) -> String {
        format!(
            "Version information:\n\
             \x20 name: {}\n\
             \x20 version: {}\n\
             \x20 build time: {}\n\
             \x20 branch: {}\n\
             \x20 commit: {}\n\
             \x20 rust version: {}",
            self.name, self.version, self.build_time, self.branch, self.commit, self.rust_version,
        )
    }
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Version information: name: {}, version: {}, build time: {}, branch: {}, commit: {}, rust version: {}",
            self.name, self.version, self.build_time, self.branch, self.commit, self.rust_version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_info_formats_compact_and_pretty_strings() {
        let compact = VERSION_INFO.to_string();
        assert!(compact.contains(version::PACKAGE_NAME));
        assert!(compact.contains(version::PACKAGE_VERSION));

        let pretty = VERSION_INFO.to_string_pretty();
        assert!(pretty.contains("Version information"));
        assert!(pretty.contains(version::GIT_COMMIT));
    }

    #[test]
    fn health_response_omits_empty_failed_checks() {
        let body = HealthResponse {
            status: HealthStatus::Healthy,
            version: "1.0.0".to_string(),
            uptime_seconds: 1,
            timestamp: "now".to_string(),
            failed_checks: BTreeMap::new(),
        };
        let value = serde_json::to_value(body).expect("serialize");
        assert_eq!(value["status"], "HEALTHY");
        assert_eq!(value["uptimeSeconds"], 1);
        assert!(value.get("failedChecks").is_none());
    }
}
