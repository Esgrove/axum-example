//! Router assembly.
//!
//! Combines public routes, admin routes, shared middleware, `OpenAPI` renderers,
//! and the JSON fallback into the single Axum `Router` served by `main`.
//! This module is the runtime wiring layer.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_scalar::{Scalar, Servable as ScalarServable};
use utoipa_swagger_ui::SwaggerUi;

use crate::mcp;
use crate::middleware::request_telemetry_middleware;
use crate::openapi::ApiDoc;
use crate::rate_limit::rate_limit_middleware;
use crate::routing::{admin, routes};
use crate::schemas::ErrorResponse;
use crate::state::SharedState;

/// Build the Axum `Router` with all routes, shared middleware, and the `OpenAPI` docs UIs.
///
/// Layout:
///
/// - Service routes: `/`, `/health`, `/status`, `/metrics`, `/version`.
/// - Item CRUD examples under `/item` and `/items`.
/// - Admin routes nested under `/admin`, guarded by the API-key extractor.
/// - Docs UIs and the raw spec, mounted only outside production.
/// - A JSON 404 fallback for any unmatched path.
pub fn build_router(shared_state: &SharedState) -> Router {
    let router = add_shared_middleware(base_routes(), shared_state);
    if shared_state.config.docs_enabled() {
        add_docs_routes(router)
    } else {
        router
    }
}

/// Build the public router with the embedded MCP server mounted at `/mcp`.
///
/// This is [`build_router`] plus the `/mcp` routes, and is what the binary serves.
/// The MCP endpoint is merged after the shared middleware, like the docs UIs.
/// Its tool calls dispatch back through [`build_router`], where rate limiting and telemetry still apply.
/// Dispatching through [`build_router`] means the MCP service never nests itself.
/// It also avoids an `Arc` cycle from the cached dispatch router back into [`crate::state::AppState`].
pub fn build_router_with_mcp(shared_state: &SharedState) -> Router {
    build_router(shared_state).merge(mcp::router(shared_state))
}

/// Build all routes before middleware and state are attached.
fn base_routes() -> Router<SharedState> {
    Router::new()
        .route("/", get(routes::root))
        .route("/health", get(routes::health))
        .route("/status", get(routes::status))
        .route("/metrics", get(routes::metrics))
        .route("/version", get(routes::version))
        .route("/item", get(routes::query_item))
        .route("/items", get(routes::list_items).post(routes::create_item))
        .route("/items/{name}", get(routes::get_item).put(routes::update_item))
        .nest("/admin", admin::routes())
}

/// Attach cross-cutting middleware and shared application state.
fn add_shared_middleware(router: Router<SharedState>, shared_state: &SharedState) -> Router {
    // Order in `ServiceBuilder` is outermost-first:
    // - `TraceLayer` opens a span that attaches the method and URI to every event logged inside the request.
    //   It also logs every response and failure.
    //   This example is not deployed behind a cloud load balancer or platform that would log requests on its own,
    //   so the service logs them itself.
    //   On a platform that already logs requests, pass `()` to `on_response` and `on_failure`,
    //   and rely on the structured completion log from the telemetry middleware instead.
    // - `rate_limit_middleware` runs before the telemetry middleware,
    //   so a request rejected locally with `429` skips the request bookkeeping.
    //   The limiter records its own rejection metric.
    // - `request_telemetry_middleware` records metrics, the in-flight gauge, and the completion log.
    // - `TimeoutLayer` is innermost, so a timed-out request is still logged and counted as a 503.
    router
        .fallback(not_found)
        .layer(
            ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                        .on_response(DefaultOnResponse::new().level(Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
                )
                .layer(from_fn_with_state(Arc::clone(shared_state), rate_limit_middleware))
                .layer(from_fn_with_state(
                    Arc::clone(shared_state),
                    request_telemetry_middleware,
                ))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::SERVICE_UNAVAILABLE,
                    shared_state.config.request_timeout,
                )),
        )
        .with_state(Arc::clone(shared_state))
}

/// Mount the `OpenAPI` documentation UIs and the raw spec.
///
/// Every renderer reads the same spec.
/// Swagger UI is served at `/doc` and also serves the raw spec at `/api-docs/openapi.json`.
/// Redoc is at `/redoc`, `RapiDoc` at `/rapidoc`, and Scalar at `/docs` with a `/scalar` alias.
fn add_docs_routes(router: Router) -> Router {
    router
        .merge(SwaggerUi::new("/doc").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
        .merge(RapiDoc::new("/api-docs/openapi.json").path("/rapidoc"))
        .merge(Scalar::with_url("/docs", ApiDoc::openapi()))
        .merge(Scalar::with_url("/scalar", ApiDoc::openapi()))
}

/// Return a JSON body for unknown paths.
///
/// The body never echoes the requested path, so logs and clients cannot be fed reflected input.
#[utoipa::path(
    get,
    path = "/{path}",
    tag = "service",
    params(("path" = String, Path, description = "Any path without a matching route")),
    responses(
        (status = NOT_FOUND, body = ErrorResponse, description = "Path does not exist")
    )
)]
pub async fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::new("Not Found", "Path does not exist")),
    )
        .into_response()
}
