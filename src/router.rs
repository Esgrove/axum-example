//! Router assembly.
//!
//! Combines public routes, admin routes, shared middleware, `OpenAPI` renderers,
//! and the JSON fallback into the single Axum `Router` served by `main`.
//! This module is the runtime wiring layer.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_scalar::{Scalar, Servable as ScalarServable};
use utoipa_swagger_ui::SwaggerUi;

use crate::middleware::{RequestTelemetryState, request_telemetry_middleware};
use crate::openapi::ApiDoc;
use crate::routing::admin;
use crate::routing::routes;
use crate::schemas::NotFoundResponse;
use crate::types::{Config, Environment, SharedState};

/// Create Router app with routes and `OpenAPI` documentation.
pub fn build_router(shared_state: &SharedState, config: &Arc<Config>) -> Router {
    let router = Router::new()
        .route("/", get(routes::root))
        .route("/health", get(routes::health))
        .route("/metrics", get(routes::metrics))
        .route("/version", get(routes::version))
        .route("/item", get(routes::query_item))
        .route("/items", get(routes::list_items))
        .route("/items", post(routes::create_item))
        .nest("/admin", admin::routes())
        .fallback(not_found)
        .layer(
            ServiceBuilder::new()
                .layer(axum::Extension(Arc::clone(config)))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                        .on_response(DefaultOnResponse::new().level(Level::INFO)),
                )
                .layer(from_fn_with_state(
                    Arc::new(RequestTelemetryState::new(shared_state.telemetry().metrics())),
                    request_telemetry_middleware,
                ))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::SERVICE_UNAVAILABLE,
                    tokio::time::Duration::from_secs(10),
                )),
        )
        .with_state(Arc::clone(shared_state));

    // Add OpenAPI documentation routes only in non-production environments.
    if config.env == Environment::Production {
        router
    } else {
        router
            .merge(SwaggerUi::new("/doc").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
            .merge(RapiDoc::new("/api-docs/openapi.json").path("/rapidoc"))
            .merge(Scalar::with_url("/scalar", ApiDoc::openapi()))
    }
}

/// Return a JSON body for unknown paths.
#[utoipa::path(
    get,
    path = "/{path}",
    responses(
        (status = NOT_FOUND, body = [NotFoundResponse], description = "Path does not exist")
    )
)]
pub async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(NotFoundResponse::new())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::schemas::ItemListResponse;
    use crate::types::{AppState, Config, Environment, Item};
    use crate::version;

    fn test_router() -> Router {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        build_router(&shared_state, &config)
    }

    fn test_router_with_config(config: Config) -> Router {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(config);
        build_router(&shared_state, &config)
    }

    #[tokio::test]
    async fn test_root() {
        let app = test_router();

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert!(body.get("message").is_some(), "Body does not contain 'message' key");
        assert!(body["message"].is_string(), "'message' is not a string");
    }

    #[tokio::test]
    async fn test_health() {
        let app = test_router();

        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(body["service"], version::PACKAGE_NAME);
        assert_eq!(body["version"], version::PACKAGE_VERSION);
        assert_eq!(body["environment"], "LOCAL");
        assert_eq!(body["status"], "ok");
        assert!(body["timestamp"].is_string());
        assert!(body["start_time"].is_string());
        assert!(body["uptime_ms"].is_number());
    }

    #[tokio::test]
    async fn test_metrics() {
        let app = test_router();

        let response = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.contains("text/plain"))
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).expect("metrics should be utf-8");

        assert!(body.contains("axum_example_http_requests_started_total"));
        assert!(body.contains("axum_example_http_requests_completed_total"));
    }

    #[tokio::test]
    async fn metrics_records_error_responses() {
        let app = test_router();

        let response = app
            .clone()
            .oneshot(Request::builder().uri("/missing").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = app
            .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).expect("metrics should be utf-8");

        assert!(body.contains("axum_example_http_errors_total"));
        assert!(body.contains("status_class=\"4xx\""));
    }

    #[test]
    fn openapi_spec_includes_health_and_metrics_routes() {
        let spec = ApiDoc::openapi();
        let value: Value = serde_json::from_str(&serde_json::to_string(&spec).expect("spec should serialize"))
            .expect("spec should parse");

        assert!(value["paths"]["/health"].is_object());
        assert!(value["paths"]["/metrics"].is_object());
        assert!(value["components"]["schemas"]["HealthResponse"].is_object());
        assert!(value["components"]["schemas"]["NotFoundResponse"].is_object());
    }

    #[tokio::test]
    async fn unknown_route_returns_json_404() {
        let app = test_router();

        let response = app
            .oneshot(Request::builder().uri("/totally-unknown").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(body["error"], "Not Found");
        assert_eq!(body["message"], "Path does not exist");
        assert!(body.get("path").is_none());
    }

    #[tokio::test]
    async fn test_version() {
        let app = test_router();

        let response = app
            .oneshot(Request::builder().uri("/version").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(body["name"], version::PACKAGE_NAME);
        assert_eq!(body["version"], version::PACKAGE_VERSION);
        assert_eq!(body["build_time"], version::BUILD_TIME);
        assert_eq!(body["branch"], version::GIT_BRANCH);
        assert_eq!(body["commit"], version::GIT_COMMIT);
        assert_eq!(body["rust_version"], version::RUST_VERSION);
    }

    #[tokio::test]
    async fn docs_routes_are_hidden_in_production() {
        let app = test_router_with_config(Config {
            env: Environment::Production,
            ..Config::default()
        });

        for path in ["/doc", "/redoc", "/rapidoc", "/scalar"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .expect("Failed to get response");
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "docs route {path} should be hidden"
            );
        }
    }

    #[tokio::test]
    async fn query_item_returns_not_found_for_missing_item() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/item?name=missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["message"], "Item does not exist: missing");
    }

    #[tokio::test]
    async fn create_item() {
        let item_json = r#"{"name": "test"}"#;
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to get body bytes")
            .to_bytes();

        assert!(!body.is_empty());

        let item: Item = serde_json::from_slice(&body).unwrap();
        assert_eq!(item.name, "test");
        assert!(item.id <= 9999);
        assert!(item.id >= 1000);

        let app = build_router(&shared_state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn list_items() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/items")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to get body bytes")
            .to_bytes();

        assert!(!body.is_empty());

        let item_list: ItemListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(item_list.num_items, 0);
        assert!(item_list.names.is_empty());

        let item_json = r#"{"name": "test"}"#;
        let app = build_router(&shared_state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::CREATED);

        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/items")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to get body bytes")
            .to_bytes();

        assert!(!body.is_empty());

        let item_list: ItemListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(item_list.num_items, 1);
        assert!(!item_list.names.is_empty());
    }

    #[tokio::test]
    async fn list_items_supports_skip_and_limit_query_parameters() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        for item_json in [
            r#"{"name":"alpha","id":1001}"#,
            r#"{"name":"bravo","id":1002}"#,
            r#"{"name":"charlie","id":1003}"#,
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/items")
                        .header("Content-Type", "application/json")
                        .body(Body::from(item_json))
                        .unwrap(),
                )
                .await
                .expect("Failed to create item");
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        for (uri, expected_names) in [
            ("/items", vec!["alpha", "bravo", "charlie"]),
            ("/items?skip=1", vec!["bravo", "charlie"]),
            ("/items?limit=2", vec!["alpha", "bravo"]),
            ("/items?skip=1&limit=1", vec!["bravo"]),
            ("/items?skip=10", vec![]),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .expect("Failed to list items");
            assert_eq!(response.status(), StatusCode::OK, "wrong status for {uri}");

            let body = response.into_body().collect().await.unwrap().to_bytes();
            let item_list: ItemListResponse = serde_json::from_slice(&body).unwrap();

            assert_eq!(item_list.num_items, 3, "total count should not be paginated for {uri}");
            assert_eq!(item_list.names, expected_names, "wrong names for {uri}");
        }
    }

    #[tokio::test]
    async fn query_item_returns_existing_item() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"name":"lookup","id":5555}"#))
                    .unwrap(),
            )
            .await
            .expect("Failed to create item");
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = app
            .oneshot(Request::builder().uri("/item?name=lookup").body(Body::empty()).unwrap())
            .await
            .expect("Failed to query item");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["name"], "lookup");
        assert_eq!(body["id"], 5555);
    }

    #[tokio::test]
    async fn create_item_with_invalid_id_returns_server_error() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"name":"bad","id":1}"#))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert!(
            body.as_str()
                .expect("server error body should be a string")
                .contains("ID must be between 1000 and 9999")
        );
    }

    #[tokio::test]
    async fn create_item_missing_data() {
        let app = test_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"wrong": "test"}"#))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn create_item_malformed_json_data() {
        let app = test_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"name": "test", "id": 1234,}"#))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_item_missing_content_type() {
        let app = test_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .body(Body::from(r#"{"name": "test", "id": 1234}"#))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn admin_missing_api_key() {
        let app = test_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/clear_items")
                    .body(Body::empty())
                    .expect("Oneshot failed for /admin/clear_items"),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_invalid_api_key() {
        let app = test_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/clear_items")
                    .header("api-key", "wrong_api_key")
                    .body(Body::empty())
                    .expect("Oneshot failed for /admin/clear_items"),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_clear_items_removes_existing_items() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"name":"temporary"}"#))
                    .unwrap(),
            )
            .await
            .expect("Failed to create item");
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/clear_items")
                    .header("api-key", &config.api_key)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to clear items");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["message"], "Removed 1 items");
        assert!(shared_state.db.is_empty());
    }

    #[tokio::test]
    async fn admin_remove_item_handles_found_and_missing_item() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"name":"removable","id":4444}"#))
                    .unwrap(),
            )
            .await
            .expect("Failed to create item");
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/remove/removable")
                    .header("api-key", &config.api_key)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to remove item");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["name"], "removable");
        assert_eq!(body["id"], 4444);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/remove/removable")
                    .header("api-key", &config.api_key)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to remove missing item");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["message"], "Item does not exist: removable");
    }
}
