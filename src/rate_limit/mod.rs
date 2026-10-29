//! In-memory per-client rate limiting.
//!
//! A sliding-window limiter keyed by client address.
//! State is process-local and resets on restart,
//! so put a shared store such as Redis behind it when running multiple replicas.

mod config;

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::config::RateLimitConfig;
use crate::errors::ApiError;
use crate::logging::log_warn;
use crate::state::SharedState;
use crate::utils::increment_saturating;

pub use config::EXEMPT_PATHS;
use config::{SWEEP_INTERVAL, UNKNOWN_CLIENT};

/// Sliding-window per-client request limiter.
#[derive(Debug)]
pub struct RateLimiter {
    settings: RateLimitConfig,
    buckets: Mutex<HashMap<String, VecDeque<Instant>>>,
    /// `check` calls since the last sweep, used to amortize bucket cleanup.
    ops_since_sweep: AtomicU64,
}

/// Outcome of checking one request against the limiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    /// Whether the request may proceed.
    pub allowed: bool,
    /// Maximum requests per window.
    pub limit: usize,
    /// Requests left in the current window.
    pub remaining: usize,
    /// How long a rejected client should wait.
    pub retry_after: Duration,
}

impl RateLimiter {
    /// Build a limiter from the runtime settings.
    #[must_use]
    pub fn new(settings: RateLimitConfig) -> Self {
        Self {
            settings,
            buckets: Mutex::new(HashMap::new()),
            ops_since_sweep: AtomicU64::new(0),
        }
    }

    /// Whether the limiter is enabled.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.settings.enabled
    }

    /// Check and record one request for `client_key` at time `now`.
    #[expect(
        clippy::significant_drop_tightening,
        reason = "an explicit lock scope keeps the bucket update easy to follow"
    )]
    pub fn check(&self, client_key: &str, now: Instant) -> RateLimitDecision {
        let limit = self.settings.max_requests;
        let window = self.settings.window;

        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| {
            log_warn!("rate limit mutex was poisoned, recovering in-memory buckets");
            self.buckets.clear_poison();
            poisoned.into_inner()
        });

        self.maybe_sweep(&mut buckets, now);
        let entries = buckets.entry(client_key.to_string()).or_default();
        prune_old_entries(entries, now, window);

        if entries.len() >= limit {
            RateLimitDecision {
                allowed: false,
                limit,
                remaining: 0,
                retry_after: retry_after(entries, now, window),
            }
        } else {
            entries.push_back(now);
            RateLimitDecision {
                allowed: true,
                limit,
                remaining: limit.saturating_sub(entries.len()),
                retry_after: Duration::ZERO,
            }
        }
    }

    /// Number of client buckets currently tracked.
    #[must_use]
    pub fn tracked_clients(&self) -> usize {
        self.buckets
            .lock()
            .map_or_else(|poisoned| poisoned.into_inner().len(), |buckets| buckets.len())
    }

    /// Every [`SWEEP_INTERVAL`] calls, drop client buckets whose window has fully drained.
    /// Must run under the caller's `buckets` lock.
    fn maybe_sweep(&self, buckets: &mut HashMap<String, VecDeque<Instant>>, now: Instant) {
        if increment_saturating(&self.ops_since_sweep) < SWEEP_INTERVAL {
            return;
        }
        self.ops_since_sweep.store(0, Ordering::Relaxed);
        let window = self.settings.window;
        buckets.retain(|_, entries| {
            prune_old_entries(entries, now, window);
            !entries.is_empty()
        });
    }
}

/// Axum middleware applying the per-client limiter.
///
/// Exempt paths and a disabled limiter pass straight through.
/// Allowed responses get `RateLimit-*` headers,
/// and rejected requests get a JSON 429 with `Retry-After`.
pub async fn rate_limit_middleware(State(state): State<SharedState>, request: Request, next: Next) -> Response {
    let limiter = &state.rate_limiter;
    if !limiter.enabled() || EXEMPT_PATHS.contains(&request.uri().path()) {
        return next.run(request).await;
    }

    let client_key = client_key(&request);
    let decision = limiter.check(&client_key, Instant::now());
    if decision.allowed {
        let mut response = next.run(request).await;
        insert_rate_limit_headers(response.headers_mut(), decision, state.config.rate_limit.window);
        return response;
    }

    log_warn!(
        client = %client_key,
        path = %request.uri().path(),
        "rate limit exceeded"
    );
    state.status.record_rate_limited();
    state.telemetry.metrics().record_rate_limited(request.uri().path());
    let mut response =
        ApiError::TooManyRequests("Too many requests, please try again later.".to_string()).into_response();
    insert_rate_limit_headers(response.headers_mut(), decision, state.config.rate_limit.window);
    response
}

/// Return the limiter key for a request.
///
/// Uses the rightmost `X-Forwarded-For` entry,
/// which is the address appended by the nearest trusted proxy.
/// Clients can prepend arbitrary values, so the leftmost entry is not trustworthy.
/// Falls back to the TCP peer address and then to a shared `unknown` bucket.
/// Adjust this when running behind more than one proxy hop.
#[must_use]
pub fn client_key(request: &Request) -> String {
    forwarded_client(request.headers())
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|info| info.0.ip().to_string())
        })
        .unwrap_or_else(|| UNKNOWN_CLIENT.to_string())
}

/// Return the rightmost non-empty `X-Forwarded-For` entry.
fn forwarded_client(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("x-forwarded-for")?.to_str().ok()?;
    let last = raw.split(',').next_back()?.trim();
    (!last.is_empty()).then(|| last.to_string())
}

/// Drop timestamps that fell out of the window.
fn prune_old_entries(entries: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while let Some(oldest) = entries.front() {
        if now.saturating_duration_since(*oldest) < window {
            break;
        }
        entries.pop_front();
    }
}

/// How long until the oldest request leaves the window.
fn retry_after(entries: &VecDeque<Instant>, now: Instant, window: Duration) -> Duration {
    entries.front().map_or(Duration::ZERO, |oldest| {
        window.saturating_sub(now.saturating_duration_since(*oldest))
    })
}

/// Insert the standard rate-limit headers.
fn insert_rate_limit_headers(headers: &mut HeaderMap, decision: RateLimitDecision, window: Duration) {
    insert_header(headers, "ratelimit-limit", &decision.limit.to_string());
    insert_header(headers, "ratelimit-remaining", &decision.remaining.to_string());
    insert_header(headers, "ratelimit-reset", &window.as_secs().to_string());
    if !decision.allowed {
        insert_header(
            headers,
            header::RETRY_AFTER.as_str(),
            &decision.retry_after.as_secs().max(1).to_string(),
        );
    }
}

/// Insert a header when the value is valid header text.
fn insert_header(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::Body;

    fn limiter(max_requests: usize, window: Duration) -> RateLimiter {
        RateLimiter::new(RateLimitConfig {
            enabled: true,
            max_requests,
            window,
        })
    }

    #[test]
    fn allows_up_to_limit_then_rejects() {
        let limiter = limiter(2, Duration::from_secs(60));
        let now = Instant::now();

        let first = limiter.check("client", now);
        assert!(first.allowed);
        assert_eq!(first.remaining, 1);
        let second = limiter.check("client", now);
        assert!(second.allowed);
        assert_eq!(second.remaining, 0);
        let third = limiter.check("client", now);
        assert!(!third.allowed);
        assert_eq!(third.retry_after, Duration::from_secs(60));

        assert!(limiter.check("other", now).allowed, "buckets are per client");
    }

    #[test]
    fn window_slides_and_frees_capacity() {
        let limiter = limiter(1, Duration::from_secs(10));
        let start = Instant::now();
        assert!(limiter.check("client", start).allowed);
        let rejected = limiter.check("client", start + Duration::from_secs(4));
        assert!(!rejected.allowed);
        assert_eq!(rejected.retry_after, Duration::from_secs(6));
        assert!(limiter.check("client", start + Duration::from_secs(10)).allowed);
    }

    #[test]
    fn periodic_sweep_drops_drained_buckets() {
        let limiter = limiter(10, Duration::from_secs(1));
        let start = Instant::now();
        for index in 0..10 {
            limiter.check(&format!("one-shot-{index}"), start);
        }
        assert_eq!(limiter.tracked_clients(), 10);

        let later = start + Duration::from_secs(5);
        for _ in 0..SWEEP_INTERVAL {
            limiter.check("steady", later);
        }
        assert_eq!(limiter.tracked_clients(), 1);
    }

    #[test]
    fn client_key_prefers_rightmost_forwarded_entry() {
        let request = Request::builder()
            .header("x-forwarded-for", "203.0.113.9, 198.51.100.7")
            .body(Body::empty())
            .expect("request");
        assert_eq!(client_key(&request), "198.51.100.7");

        let request = Request::builder()
            .header("x-forwarded-for", " , ")
            .body(Body::empty())
            .expect("request");
        assert_eq!(client_key(&request), UNKNOWN_CLIENT);

        let mut request = Request::builder().body(Body::empty()).expect("request");
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([10, 0, 0, 1], 1234))));
        assert_eq!(client_key(&request), "10.0.0.1");
    }

    #[test]
    fn headers_include_retry_after_only_when_rejected() {
        let mut headers = HeaderMap::new();
        let allowed = RateLimitDecision {
            allowed: true,
            limit: 5,
            remaining: 4,
            retry_after: Duration::ZERO,
        };
        insert_rate_limit_headers(&mut headers, allowed, Duration::from_secs(60));
        assert_eq!(headers["ratelimit-limit"], "5");
        assert_eq!(headers["ratelimit-remaining"], "4");
        assert_eq!(headers["ratelimit-reset"], "60");
        assert!(headers.get(header::RETRY_AFTER).is_none());

        let rejected = RateLimitDecision {
            allowed: false,
            remaining: 0,
            retry_after: Duration::from_millis(200),
            ..allowed
        };
        insert_rate_limit_headers(&mut headers, rejected, Duration::from_secs(60));
        assert_eq!(headers[header::RETRY_AFTER], "1");
    }
}
