//! Integration tests for the service routes, the docs UIs, and the JSON fallback.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "integration tests fail fast on malformed fixtures and responses"
)]

mod helpers;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use axum_example::openapi::ApiDoc;
use axum_example::status::REPORTED_ENV_VARS;
use axum_example::{Config, Environment, version};
use utoipa::OpenApi;

use helpers::TestApp;

#[tokio::test]
async fn root_returns_package_name_and_datetime() {
    let app = TestApp::new();
    let response = app.get("/").await;

    assert_eq!(response.status, StatusCode::OK);
    let message = response.json()["message"].as_str().expect("message").to_string();
    assert!(
        message.starts_with(version::PACKAGE_NAME),
        "unexpected message: {message}"
    );
}

#[tokio::test]
async fn health_reports_healthy_with_version_and_uptime() {
    let app = TestApp::new();
    let response = app.get("/health").await;

    assert_eq!(response.status, StatusCode::OK);
    let body = response.json();
    assert_eq!(body["status"], "HEALTHY");
    assert_eq!(body["version"], version::PACKAGE_VERSION);
    assert!(body["uptimeSeconds"].is_u64());
    assert!(body["timestamp"].is_string());
    assert!(
        body.get("failedChecks").is_none(),
        "healthy response omits failedChecks"
    );
}

#[tokio::test]
async fn status_reports_checks_counters_config_and_env_var_names() {
    let app = TestApp::new();
    app.send_json(Method::POST, "/items", &json!({"name": "status-item", "id": 1234}))
        .await;

    let response = app.get("/status").await;

    assert_eq!(response.status, StatusCode::OK);
    let body = response.json();
    assert_eq!(body["service"], version::PACKAGE_NAME);
    assert_eq!(body["environment"], "LOCAL");
    assert_eq!(body["health"]["overall"], true);
    assert!(body["health"]["checks"]["uptime"].is_object());
    assert!(body["health"]["checks"]["itemStore"].is_object());
    assert_eq!(body["items"], 1);
    assert_eq!(body["config"]["rateLimitEnabled"], true);
    assert_eq!(body["config"]["docsEnabled"], true);
    assert!(body["requests"]["total"].as_u64().expect("total") >= 2);
    assert_eq!(body["environmentVariables"]["total"], REPORTED_ENV_VARS.len());

    let variables = body["environmentVariables"]["variables"].as_array().expect("variables");
    for variable in variables {
        let keys: Vec<&String> = variable.as_object().expect("entry object").keys().collect();
        assert_eq!(keys, ["name", "available"], "env var entries must never include values");
    }
}

#[tokio::test]
async fn metrics_returns_prometheus_text() {
    let app = TestApp::new();
    app.get("/").await;

    let response = app.get("/metrics").await;

    assert_eq!(response.status, StatusCode::OK);
    assert!(
        response
            .header("content-type")
            .is_some_and(|value| value.contains("text/plain")),
        "metrics should be served as text/plain"
    );
    let body = response.text();
    assert!(body.contains("axum_example_http_requests_started_total"));
    assert!(body.contains("axum_example_http_requests_completed_total"));
}

#[tokio::test]
async fn metrics_record_error_responses() {
    let app = TestApp::new();
    assert_eq!(app.get("/missing").await.status, StatusCode::NOT_FOUND);

    let body = app.get("/metrics").await.text();

    assert!(body.contains("axum_example_http_errors_total"));
    assert!(body.contains("status_class=\"4xx\""));
}

#[tokio::test]
async fn version_returns_build_metadata_in_snake_case() {
    let app = TestApp::new();
    let response = app.get("/version").await;

    assert_eq!(response.status, StatusCode::OK);
    let body = response.json();
    assert_eq!(body["name"], version::PACKAGE_NAME);
    assert_eq!(body["version"], version::PACKAGE_VERSION);
    assert_eq!(body["deploy_tag"], version::DEPLOY_TAG);
    assert_eq!(body["build_time"], version::BUILD_TIME);
    assert_eq!(body["branch"], version::GIT_BRANCH);
    assert_eq!(body["commit"], version::GIT_COMMIT);
    assert_eq!(body["rust_version"], version::RUST_VERSION);
}

#[tokio::test]
async fn responses_carry_a_request_id() {
    let app = TestApp::new();
    let generated = app.get("/health").await;
    let id = generated.header("x-request-id").expect("generated request id");
    assert_eq!(id.len(), 32);

    let request = axum::http::Request::builder()
        .uri("/health")
        .header("x-request-id", "client-supplied-id")
        .body(axum::body::Body::empty())
        .expect("request builds");
    let echoed = app.send(request).await;
    assert_eq!(echoed.header("x-request-id"), Some("client-supplied-id"));
}

#[tokio::test]
async fn unknown_route_returns_json_404_without_echoing_the_path() {
    let app = TestApp::new();
    let response = app.get("/totally-unknown").await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    let body = response.json();
    assert_eq!(body["error"], "Not Found");
    assert_eq!(body["message"], "Path does not exist");
    assert!(!response.text().contains("totally-unknown"));
}

#[tokio::test]
async fn docs_routes_are_served_outside_production() {
    let app = TestApp::new();
    for path in [
        "/doc/",
        "/redoc",
        "/rapidoc",
        "/docs",
        "/scalar",
        "/api-docs/openapi.json",
    ] {
        let response = app.get(path).await;
        assert_eq!(response.status, StatusCode::OK, "docs route {path} should be served");
    }
}

#[tokio::test]
async fn docs_routes_are_hidden_in_production() {
    let app = TestApp::with_config(Config {
        env: Environment::Production,
        ..Config::default()
    });
    for path in [
        "/doc",
        "/redoc",
        "/rapidoc",
        "/docs",
        "/scalar",
        "/api-docs/openapi.json",
    ] {
        let response = app.get(path).await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "docs route {path} should be hidden"
        );
    }
}

#[tokio::test]
async fn openapi_json_route_returns_the_spec() {
    let app = TestApp::new();
    let body = app.get("/api-docs/openapi.json").await.json();

    assert!(body["openapi"].as_str().is_some_and(|value| value.starts_with("3.")));
    assert!(body["paths"]["/health"].is_object());
    assert!(body["paths"]["/status"].is_object());
}

#[test]
fn openapi_spec_documents_every_route_and_schema() {
    let spec: Value = serde_json::to_value(ApiDoc::openapi()).expect("spec serializes");

    for path in [
        "/",
        "/health",
        "/status",
        "/metrics",
        "/version",
        "/item",
        "/items",
        "/items/{name}",
        "/admin/clear_items",
        "/admin/remove/{name}",
    ] {
        assert!(spec["paths"][path].is_object(), "missing path `{path}`");
    }
    for schema in [
        "HealthResponse",
        "StatusResponse",
        "ErrorResponse",
        "Item",
        "VersionInfo",
    ] {
        assert!(
            spec["components"]["schemas"][schema].is_object(),
            "missing schema `{schema}`"
        );
    }
    assert!(spec["components"]["securitySchemes"]["api_key"].is_object());
}
