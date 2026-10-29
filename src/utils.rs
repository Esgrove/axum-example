//! Shared utility functions.
//!
//! Small cross-cutting helpers that do not belong to a specific route or service module,
//! such as socket address parsing, environment variable reading, saturating counters,
//! and shutdown signal handling.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::signal;

/// Increment an atomic counter, saturating at [`u64::MAX`], and return the updated value.
///
/// `AtomicU64` has no built-in "add but stop at the maximum" operation,
/// so this uses a small compare-and-swap loop.
/// Use this for counters whose updated value the caller reads back,
/// where `fetch_add(1) + 1` would be an overflowing addition.
///
/// # Examples
///
/// ```rust
/// # use std::sync::atomic::AtomicU64;
/// # use axum_example::utils::increment_saturating;
/// let counter = AtomicU64::new(u64::MAX - 1);
/// assert_eq!(increment_saturating(&counter), u64::MAX);
/// assert_eq!(increment_saturating(&counter), u64::MAX);
/// ```
#[inline]
pub fn increment_saturating(counter: &AtomicU64) -> u64 {
    let mut current_value = counter.load(Ordering::Relaxed);
    loop {
        let next_value = current_value.saturating_add(1);
        match counter.compare_exchange_weak(current_value, next_value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next_value,
            Err(updated_value) => current_value = updated_value,
        }
    }
}

/// Decrement an atomic counter without allowing it to underflow, and return the updated value.
///
/// The returned value is the count this call published,
/// so callers reporting a gauge do not need a second load that another thread could have changed.
///
/// # Examples
///
/// ```rust
/// # use std::sync::atomic::AtomicU64;
/// # use axum_example::utils::decrement_saturating;
/// let counter = AtomicU64::new(2);
/// assert_eq!(decrement_saturating(&counter), 1);
/// assert_eq!(decrement_saturating(&counter), 0);
/// assert_eq!(decrement_saturating(&counter), 0);
/// ```
#[inline]
pub fn decrement_saturating(counter: &AtomicU64) -> u64 {
    let mut current_value = counter.load(Ordering::Relaxed);
    while current_value > 0 {
        let next_value = current_value.saturating_sub(1);
        match counter.compare_exchange_weak(current_value, next_value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next_value,
            Err(updated_value) => current_value = updated_value,
        }
    }
    0
}

/// Convert a duration to whole milliseconds, saturating at [`u64::MAX`].
///
/// [`Duration::as_millis`] returns `u128`,
/// while logs, metrics, and public schemas in this crate use `u64`.
#[must_use]
#[inline]
pub fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Build the socket address the HTTP server should bind.
///
/// A missing host binds to `127.0.0.1`.
/// An explicitly provided host must parse as an IP address.
///
/// # Errors
///
/// Returns an error when `host` is set but is not a valid IPv4 or IPv6 address.
///
/// # Examples
///
/// ```rust
/// # use axum_example::utils::get_address;
/// let address = get_address(None, 3000).expect("default address should parse");
/// assert_eq!(address.to_string(), "127.0.0.1:3000");
/// assert!(get_address(Some("not-an-ip".to_string()), 3000).is_err());
/// ```
pub fn get_address(host: Option<String>, port: u16) -> Result<SocketAddr> {
    let ip = match host {
        Some(ip_string) => ip_string
            .trim()
            .parse::<IpAddr>()
            .with_context(|| format!("invalid host IP `{ip_string}`"))?,
        None => IpAddr::V4(Ipv4Addr::LOCALHOST),
    };
    Ok(SocketAddr::new(ip, port))
}

/// Read an environment variable.
///
/// Values are trimmed before returning.
/// Empty or whitespace-only strings are treated the same as missing values.
#[must_use]
pub fn read_environment_variable(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

/// Truncate the input to at most `limit` bytes without splitting a UTF-8 code point.
///
/// `marker` is appended only when something was actually removed.
/// The cut point walks back to the nearest character boundary,
/// which can land a few bytes below `limit` when multi-byte characters straddle it.
///
/// # Examples
///
/// ```rust
/// # use axum_example::utils::truncate_at_char_boundary;
/// assert_eq!(truncate_at_char_boundary("short", 10, "…"), "short");
/// assert_eq!(truncate_at_char_boundary("hello world", 5, "…"), "hello…");
/// assert_eq!(truncate_at_char_boundary("ååå", 3, "…"), "å…");
/// ```
#[must_use]
pub fn truncate_at_char_boundary(input: &str, limit: usize, marker: &str) -> String {
    if input.len() <= limit {
        return input.to_string();
    }
    let mut end = limit;
    while end > 0 && !input.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let mut truncated = input.get(..end).unwrap_or_default().to_string();
    truncated.push_str(marker);
    truncated
}

/// Resolve when the process receives `Ctrl+C` (SIGINT) or, on Unix, `SIGTERM`.
///
/// Used by `axum::serve(...).with_graceful_shutdown(...)`
/// so in-flight requests can finish before the process exits.
///
/// # Panics
///
/// Panics if Tokio cannot install a supported platform signal handler.
#[expect(
    clippy::expect_used,
    reason = "a service without shutdown signal handlers cannot terminate gracefully and must fail fast"
)]
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_address_defaults_to_loopback_when_host_missing() {
        let address = get_address(None, 3000).expect("default address should parse");
        assert_eq!(address.to_string(), "127.0.0.1:3000");
    }

    #[test]
    fn get_address_uses_explicit_ipv4_and_ipv6_hosts() {
        let address = get_address(Some("0.0.0.0".to_string()), 8080).expect("ipv4 host should parse");
        assert_eq!(address.to_string(), "0.0.0.0:8080");
        let address = get_address(Some(" ::1 ".to_string()), 4444).expect("ipv6 host should parse");
        assert_eq!(address.to_string(), "[::1]:4444");
    }

    #[test]
    fn get_address_rejects_non_ip_hosts() {
        for host in ["", "not-an-ip", "localhost"] {
            let error = get_address(Some(host.to_string()), 8080).expect_err("non-IP host should error");
            assert!(error.to_string().contains(host));
        }
    }

    #[test]
    fn saturating_counters_stop_at_bounds() {
        let counter = AtomicU64::new(1);
        assert_eq!(decrement_saturating(&counter), 0);
        assert_eq!(decrement_saturating(&counter), 0);

        let counter = AtomicU64::new(u64::MAX.saturating_sub(1));
        assert_eq!(increment_saturating(&counter), u64::MAX);
        assert_eq!(increment_saturating(&counter), u64::MAX);
    }

    #[test]
    fn duration_millis_converts_and_saturates() {
        assert_eq!(duration_millis(Duration::from_millis(1234)), 1234);
        assert_eq!(duration_millis(Duration::MAX), u64::MAX);
    }

    #[test]
    fn truncate_at_char_boundary_respects_limit_and_utf8() {
        assert_eq!(truncate_at_char_boundary("exact", 5, "…"), "exact");
        assert_eq!(truncate_at_char_boundary("äääää", 5, "…"), "ää…");
        assert_eq!(truncate_at_char_boundary("ä", 1, "…"), "…");
    }

    #[test]
    #[allow(unsafe_code, reason = "env mutation is unavoidable to exercise both branches")]
    fn read_environment_variable_trims_values_and_treats_blank_as_missing() {
        // SAFETY: the variable name is unique to this test,
        // so no other test reads or writes it concurrently.
        let name = "AXUM_EXAMPLE_TEST_READ_ENV_VAR";
        unsafe { env::set_var(name, "") };
        assert_eq!(read_environment_variable(name), None);
        unsafe { env::set_var(name, " value\n") };
        assert_eq!(read_environment_variable(name), Some("value".to_string()));
        unsafe { env::remove_var(name) };
        assert_eq!(read_environment_variable(name), None);
    }
}
