//! Request telemetry middleware.
//!
//! Wraps each routed request and:
//!
//! 1. Propagates an `x-request-id` header, generating one when the client did not send it.
//! 2. Acquires an [`InProgressGuard`], which records the request start
//!    and releases the in-flight slot when dropped, even if the request future is cancelled.
//! 3. Records completion metrics and writes one structured completion log line.
//!
//! Route labels come from Axum's matched route pattern,
//! so metrics never use high-cardinality raw URLs.

use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use rand::RngExt;

use crate::logging::{log_info, log_warn};
use crate::state::SharedState;
use crate::telemetry::CompletedRequestMetric;
use crate::utils::{duration_millis, truncate_at_char_boundary};

/// Header used to correlate a request across services and log lines.
pub static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Longest accepted client-supplied request id, in bytes.
const MAX_REQUEST_ID_LEN: usize = 128;

/// Route label used when no route matched.
const UNMATCHED_ROUTE: &str = "unmatched";

/// Owns one in-flight request slot.
///
/// [`InProgressGuard::acquire`] records the request start and returns the guard,
/// so an increment can never be separated from the release that balances it.
/// Axum drops the middleware future without completing it when the client disconnects,
/// which would leak the in-flight gauge if the decrement lived on the normal completion path.
/// The drop guard releases the slot exactly once,
/// whether the request completes, panics, or is cancelled.
struct InProgressGuard {
    state: SharedState,
    route: String,
    method: String,
}

impl InProgressGuard {
    /// Record a request starting and acquire its in-flight slot.
    fn acquire(state: SharedState, route: String, method: String) -> Self {
        let in_progress = state.status.record_request_started();
        state
            .telemetry
            .metrics()
            .record_request_started(&route, &method, in_progress);
        Self { state, route, method }
    }
}

impl Drop for InProgressGuard {
    /// Release the in-flight slot and publish the updated gauge.
    fn drop(&mut self) {
        let in_progress = self.state.status.record_request_settled();
        self.state
            .telemetry
            .metrics()
            .record_in_progress(&self.route, &self.method, in_progress);
    }
}

/// Record metrics and a structured completion log around every HTTP request.
pub async fn request_telemetry_middleware(
    State(state): State<SharedState>,
    mut request: Request,
    next: Next,
) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or(UNMATCHED_ROUTE, MatchedPath::as_str)
        .to_string();
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let request_id = ensure_request_id(&mut request);

    let guard = InProgressGuard::acquire(state.clone(), route, method);
    let start = Instant::now();

    let mut response = next.run(request).await;

    let latency = start.elapsed();
    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
        state.status.record_error();
    }
    state
        .telemetry
        .metrics()
        .record_request_completed(CompletedRequestMetric {
            route: &guard.route,
            method: &guard.method,
            status: status.as_u16(),
            latency,
        });

    let latency_ms = duration_millis(latency);
    if status.is_server_error() {
        log_warn!(
            request_id = %request_id.to_str().unwrap_or_default(),
            method = %guard.method,
            route = %guard.route,
            path = %path,
            status = status.as_u16(),
            latency_ms,
            "request failed"
        );
    } else {
        log_info!(
            request_id = %request_id.to_str().unwrap_or_default(),
            method = %guard.method,
            route = %guard.route,
            path = %path,
            status = status.as_u16(),
            latency_ms,
            "request completed"
        );
    }

    response.headers_mut().insert(REQUEST_ID_HEADER.clone(), request_id);
    response
}

/// Return the request id, generating and inserting one when missing or invalid.
///
/// Client-supplied ids are kept when they are short, printable header values,
/// so callers can correlate their own logs with this service.
fn ensure_request_id(request: &mut Request) -> HeaderValue {
    if let Some(existing) = request.headers().get(&REQUEST_ID_HEADER)
        && let Ok(text) = existing.to_str()
        && !text.is_empty()
        && text.len() <= MAX_REQUEST_ID_LEN
    {
        return existing.clone();
    }

    let generated = generate_request_id();
    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER.clone(), generated.clone());
    generated
}

/// Generate a random 128-bit request id as 32 lowercase hex characters.
fn generate_request_id() -> HeaderValue {
    let value: u128 = rand::rng().random();
    let text = truncate_at_char_boundary(&format!("{value:032x}"), 32, "");
    HeaderValue::from_str(&text).unwrap_or_else(|_| HeaderValue::from_static("unknown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use axum::Router;
    use axum::body::Body;
    use axum::middleware::from_fn_with_state;
    use axum::routing::get;
    use tower::ServiceExt;

    use crate::state::AppState;

    #[test]
    fn generated_request_ids_are_hex_and_unique() {
        let first = generate_request_id();
        let second = generate_request_id();
        assert_eq!(first.len(), 32);
        assert!(first.to_str().unwrap().chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn ensure_request_id_keeps_valid_ids_and_replaces_oversized_ones() {
        let mut request = Request::builder()
            .header("x-request-id", "client-id")
            .body(Body::empty())
            .unwrap();
        assert_eq!(ensure_request_id(&mut request), "client-id");

        let mut request = Request::builder()
            .header("x-request-id", "x".repeat(MAX_REQUEST_ID_LEN + 1))
            .body(Body::empty())
            .unwrap();
        let id = ensure_request_id(&mut request);
        assert_eq!(id.len(), 32);
        assert_eq!(request.headers()["x-request-id"], id);
    }

    #[tokio::test]
    async fn cancelled_request_releases_in_flight_counter() {
        let state = AppState::new_shared(crate::config::Config::default());
        let app = Router::new()
            .route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    "done"
                }),
            )
            .layer(from_fn_with_state(state.clone(), request_telemetry_middleware));

        let request = Request::builder().uri("/slow").body(Body::empty()).unwrap();
        let result = tokio::time::timeout(Duration::from_millis(20), app.oneshot(request)).await;
        assert!(result.is_err(), "request should still be running when cancelled");

        let counters = state.status.counters();
        assert_eq!(counters.total, 1);
        assert_eq!(counters.in_progress, 0, "drop guard must release the slot");
    }
}
