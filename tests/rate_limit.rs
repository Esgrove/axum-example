//! Integration tests for the per-client rate limiter.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "integration tests fail fast on malformed fixtures and responses"
)]

mod helpers;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};

use axum_example::Config;
use axum_example::config::RateLimitConfig;

use helpers::TestApp;

/// Build an app whose limiter allows `max_requests` per minute.
fn limited_app(max_requests: usize) -> TestApp {
    TestApp::with_config(Config {
        rate_limit: RateLimitConfig {
            enabled: true,
            max_requests,
            window: Duration::from_mins(1),
        },
        ..Config::default()
    })
}

/// Build a `GET` request from the given forwarded client address.
fn request_from(client: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("x-forwarded-for", client)
        .body(Body::empty())
        .expect("request builds")
}

#[tokio::test]
async fn allowed_responses_carry_rate_limit_headers() {
    let app = limited_app(5);
    let response = app.send(request_from("203.0.113.1", "/version")).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.header("ratelimit-limit"), Some("5"));
    assert_eq!(response.header("ratelimit-remaining"), Some("4"));
    assert_eq!(response.header("ratelimit-reset"), Some("60"));
    assert!(response.header("retry-after").is_none());
}

#[tokio::test]
async fn requests_over_the_limit_get_429_with_retry_after() {
    let app = limited_app(2);
    for _ in 0..2 {
        let response = app.send(request_from("203.0.113.2", "/version")).await;
        assert_eq!(response.status, StatusCode::OK);
    }

    let rejected = app.send(request_from("203.0.113.2", "/version")).await;

    assert_eq!(rejected.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(rejected.header("ratelimit-remaining"), Some("0"));
    let retry_after: u64 = rejected
        .header("retry-after")
        .expect("retry-after header")
        .parse()
        .expect("numeric retry-after");
    assert!((1..=60).contains(&retry_after));
    assert_eq!(rejected.json()["error"], "Too Many Requests");
    assert_eq!(app.state.status.counters().rate_limited, 1);
}

#[tokio::test]
async fn clients_are_limited_independently() {
    let app = limited_app(1);
    assert_eq!(app.send(request_from("203.0.113.3", "/")).await.status, StatusCode::OK);
    assert_eq!(
        app.send(request_from("203.0.113.3", "/")).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(app.send(request_from("203.0.113.4", "/")).await.status, StatusCode::OK);
}

#[tokio::test]
async fn rightmost_forwarded_entry_is_the_client_key() {
    let app = limited_app(1);
    // A client prepending a spoofed address still lands in the bucket of the proxy-appended address.
    assert_eq!(
        app.send(request_from("198.51.100.1, 203.0.113.5", "/")).await.status,
        StatusCode::OK
    );
    assert_eq!(
        app.send(request_from("198.51.100.2, 203.0.113.5", "/")).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn health_and_metrics_are_exempt() {
    let app = limited_app(1);
    for _ in 0..5 {
        for uri in ["/health", "/metrics"] {
            let response = app.send(request_from("203.0.113.6", uri)).await;
            assert_eq!(response.status, StatusCode::OK, "{uri} should never be rate limited");
            assert!(response.header("ratelimit-limit").is_none());
        }
    }
}

#[tokio::test]
async fn disabled_limiter_never_rejects() {
    let app = TestApp::with_config(Config {
        rate_limit: RateLimitConfig {
            enabled: false,
            max_requests: 1,
            window: Duration::from_mins(1),
        },
        ..Config::default()
    });
    for _ in 0..5 {
        let response = app.send(request_from("203.0.113.7", "/")).await;
        assert_eq!(response.status, StatusCode::OK);
        assert!(response.header("ratelimit-limit").is_none());
    }
}
