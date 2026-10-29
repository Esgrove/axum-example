//! Shared harness for the integration tests.
//!
//! Each test file imports this module with `mod helpers;`,
//! so every file can focus on one behavior area while driving the same router setup as the binary.

#![allow(dead_code, reason = "each test binary uses a different subset of these helpers")]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use axum_example::{AppState, Config, SharedState, build_router_with_mcp};

/// Test app holding both the router and the shared state behind it.
///
/// Keeping the state lets tests inspect the item store and counters directly.
pub struct TestApp {
    /// Router built exactly like the binary builds it, including `/mcp`.
    pub router: Router,
    /// Shared state behind the router.
    pub state: SharedState,
}

/// Collected response parts.
pub struct TestResponse {
    /// HTTP status code.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Raw response body.
    pub body: Vec<u8>,
}

impl TestApp {
    /// Build a test app with the default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// Build a test app around an explicit configuration.
    #[must_use]
    pub fn with_config(config: Config) -> Self {
        let state = Arc::new(AppState::new(config));
        let router = build_router_with_mcp(&state);
        Self { router, state }
    }

    /// Return the admin API key the app accepts.
    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.state.config.api_key
    }

    /// Send a request and collect the full response.
    pub async fn send(&self, request: Request<Body>) -> TestResponse {
        send_request(&self.router, request).await
    }

    /// Send a `GET` request.
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.send(empty_request(Method::GET, uri)).await
    }

    /// Send a JSON request with the given method.
    pub async fn send_json(&self, method: Method, uri: &str, body: &Value) -> TestResponse {
        self.send(json_request(method, uri, body)).await
    }

    /// Send a `DELETE` request with the admin API key.
    pub async fn admin_delete(&self, uri: &str) -> TestResponse {
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .header("api-key", self.api_key())
            .body(Body::empty())
            .expect("request builds");
        self.send(request).await
    }
}

impl Default for TestApp {
    fn default() -> Self {
        Self::new()
    }
}

impl TestResponse {
    /// Parse the body as JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|err| panic!("body is not JSON ({err}): {}", String::from_utf8_lossy(&self.body)))
    }

    /// Return the body as UTF-8 text.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8(self.body.clone()).expect("body is UTF-8")
    }

    /// Return a header value as a string.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

/// Build a request without a body.
#[must_use]
pub fn empty_request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request builds")
}

/// Build a request with a JSON body and content type.
#[must_use]
pub fn json_request(method: Method, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request builds")
}

/// Send a request through a router and collect the full response.
pub async fn send_request(router: &Router, request: Request<Body>) -> TestResponse {
    let response = router.clone().oneshot(request).await.expect("router responds");
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes()
        .to_vec();
    TestResponse { status, headers, body }
}
