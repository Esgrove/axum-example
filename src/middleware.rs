//! Request telemetry middleware.
//!
//! Wraps each routed request, records start and completion metrics,
//! and keeps an in-process in-flight counter for the OpenTelemetry gauge.
//! Route labels come from Axum's matched route pattern so metrics do not
//! accidentally use high-cardinality raw URLs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::telemetry::{CompletedRequestMetric, TelemetryMetrics};

/// Shared state for request telemetry middleware.
#[derive(Debug)]
pub struct RequestTelemetryState {
    metrics: Arc<TelemetryMetrics>,
    in_progress: AtomicU64,
}

impl RequestTelemetryState {
    /// Build request telemetry state from shared OpenTelemetry instruments.
    #[must_use]
    pub const fn new(metrics: Arc<TelemetryMetrics>) -> Self {
        Self {
            metrics,
            in_progress: AtomicU64::new(0),
        }
    }
}

/// Record OpenTelemetry metrics around every HTTP request.
pub async fn request_telemetry_middleware(
    State(state): State<Arc<RequestTelemetryState>>,
    request: Request,
    next: Next,
) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unknown", MatchedPath::as_str)
        .to_string();
    let method = request.method().as_str().to_string();
    let in_progress = state.in_progress.fetch_add(1, Ordering::Relaxed) + 1;
    state.metrics.record_request_started(&route, &method, in_progress);

    let start = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let latency = start.elapsed();
    let in_progress = decrement_saturating(&state.in_progress);

    state.metrics.record_request_completed(CompletedRequestMetric {
        route: &route,
        method: &method,
        status,
        latency,
        in_progress,
    });

    response
}

fn decrement_saturating(counter: &AtomicU64) -> u64 {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current == 0 {
            return 0;
        }
        match counter.compare_exchange_weak(current, current - 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return current - 1,
            Err(next) => current = next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrement_saturates_at_zero() {
        let counter = AtomicU64::new(0);

        assert_eq!(decrement_saturating(&counter), 0);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
