//! Rate limiter constants.
//!
//! Runtime-tunable settings (request budget, window, on/off) live in
//! [`crate::config::RateLimitConfig`] and are read from the environment.
//! This module holds the fixed values.

/// Paths that are never rate limited.
///
/// Probes and metric scrapers poll these routes on a fixed schedule,
/// and throttling them would only produce false alarms.
pub const EXEMPT_PATHS: &[&str] = &["/health", "/metrics"];

/// Number of `check` calls between full sweeps of drained client buckets.
///
/// Stale per-client buckets are pruned only when that client is seen again,
/// so a one-shot client would otherwise leave an empty entry behind forever.
/// A periodic sweep drops those keys to keep the map bounded
/// without paying for a full scan on every request.
pub const SWEEP_INTERVAL: u64 = 1000;

/// Bucket key used when no client address can be determined.
pub const UNKNOWN_CLIENT: &str = "unknown";
