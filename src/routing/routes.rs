//! Public routes.
//!
//! Unauthenticated service endpoints (root, health, status, version, metrics)
//! and the item CRUD examples.

#![allow(clippy::unused_async, reason = "Axum handlers must return futures")]
#![allow(
    clippy::missing_errors_doc,
    reason = "error responses are documented in the utoipa `responses` list, and doc comments become OpenAPI descriptions"
)]

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::WithRejection;
use chrono::{SecondsFormat, Utc};

use crate::errors::ApiError;
use crate::logging::{log_debug, log_info};
use crate::schemas::{
    CreateItem, ErrorResponse, HealthResponse, ItemListQuery, ItemListResponse, ItemQuery, MessageResponse,
    StatusResponse, VERSION_INFO, VersionInfo,
};
use crate::state::SharedState;
use crate::status::{health_status, now_rfc3339};
use crate::types::Item;
use crate::version;

// `axum::debug_handler` produces clearer compile errors
// when a handler signature does not satisfy Axum's handler trait bounds.
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html

/// Return the API name with the current date and time.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "service",
    responses(
        (status = OK, body = MessageResponse, description = "API name with current datetime")
    )
)]
pub async fn root() -> Json<MessageResponse> {
    let datetime = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    log_debug!("Root: {datetime}");
    Json(MessageResponse::new(format!("{} {datetime}", version::PACKAGE_NAME)))
}

/// Lightweight health check for load balancers and uptime probes.
///
/// Always responds with `200 OK`.
/// The verdict is in the `status` field, and failing checks are listed in `failedChecks`.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/health",
    tag = "service",
    responses(
        (status = OK, body = HealthResponse, description = "Health verdict with failing checks")
    )
)]
pub async fn health(State(state): State<SharedState>) -> Json<HealthResponse> {
    let summary = state.status.collect_checks(state.db.len());
    let status = health_status(&summary);
    let failed_checks = summary.checks.into_iter().filter(|(_, check)| !check.healthy).collect();
    Json(HealthResponse {
        status,
        version: version::PACKAGE_VERSION.to_string(),
        uptime_seconds: state.status.uptime_seconds(),
        timestamp: now_rfc3339(),
        failed_checks,
    })
}

/// Detailed runtime status.
///
/// Combines health checks, request counters, a non-secret config summary,
/// and environment variable presence (names only).
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/status",
    tag = "service",
    responses(
        (status = OK, body = StatusResponse, description = "Full service status snapshot")
    )
)]
pub async fn status(State(state): State<SharedState>) -> Json<StatusResponse> {
    Json(state.status.get_status(&state.config, state.db.len()))
}

/// Return OpenTelemetry metrics in Prometheus text format.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "service",
    responses(
        (status = OK, description = "Prometheus metrics in text format", content_type = "text/plain"),
        (status = INTERNAL_SERVER_ERROR, body = ErrorResponse, description = "Metrics encoding failed")
    )
)]
pub async fn metrics(State(state): State<SharedState>) -> Result<Response, ApiError> {
    let (body, content_type) = state
        .telemetry
        .render_prometheus()
        .map_err(|err| ApiError::Internal(format!("Failed to render metrics: {err}")))?;
    Ok(([(CONTENT_TYPE, content_type)], body).into_response())
}

/// Return version and build information.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/version",
    tag = "service",
    responses(
        (status = OK, body = VersionInfo, description = "Version information")
    )
)]
pub async fn version() -> Json<&'static VersionInfo> {
    Json(&VERSION_INFO)
}

/// Get an item by name using a query parameter.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/item",
    tag = "items",
    params(ItemQuery),
    responses(
        (status = OK, body = Item, description = "Found existing item"),
        (status = NOT_FOUND, body = ErrorResponse, description = "Item does not exist")
    )
)]
pub async fn query_item(
    Query(query): Query<ItemQuery>,
    State(state): State<SharedState>,
) -> Result<Json<Item>, ApiError> {
    find_item(&state, &query.name).map(Json)
}

/// Get an item by name using a path parameter.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/items/{name}",
    tag = "items",
    params(("name" = String, Path, description = "Item name")),
    responses(
        (status = OK, body = Item, description = "Found existing item"),
        (status = NOT_FOUND, body = ErrorResponse, description = "Item does not exist")
    )
)]
pub async fn get_item(Path(name): Path<String>, State(state): State<SharedState>) -> Result<Json<Item>, ApiError> {
    find_item(&state, &name).map(Json)
}

/// Create a new item.
#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/items",
    tag = "items",
    request_body = CreateItem,
    responses(
        (status = CREATED, body = Item, description = "New item created"),
        (status = BAD_REQUEST, body = ErrorResponse, description = "Malformed JSON or invalid id"),
        (status = CONFLICT, body = ErrorResponse, description = "Item already exists"),
        (status = UNPROCESSABLE_ENTITY, body = ErrorResponse, description = "JSON deserialization error"),
        (status = UNSUPPORTED_MEDIA_TYPE, body = ErrorResponse, description = "Missing JSON content type header"),
    )
)]
pub async fn create_item(
    State(state): State<SharedState>,
    WithRejection(Json(payload), _): WithRejection<Json<CreateItem>, ApiError>,
) -> Result<(StatusCode, Json<Item>), ApiError> {
    let item = match payload.id {
        Some(id) => Item::new(payload.name, id)?,
        None => Item::new_with_random_id(payload.name),
    };
    // `entry` holds the shard lock, so check-and-insert is atomic across concurrent requests.
    match state.db.entry(item.name.clone()) {
        dashmap::Entry::Occupied(_) => Err(ApiError::Conflict(format!("Item already exists: {}", item.name))),
        dashmap::Entry::Vacant(entry) => {
            entry.insert(item.clone());
            log_info!("Created item: {}", item.name);
            Ok((StatusCode::CREATED, Json(item)))
        }
    }
}

/// Replace the id of an existing item.
#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/items/{name}",
    tag = "items",
    params(("name" = String, Path, description = "Item name")),
    request_body = CreateItem,
    responses(
        (status = OK, body = Item, description = "Item updated"),
        (status = BAD_REQUEST, body = ErrorResponse, description = "Invalid id or name mismatch"),
        (status = NOT_FOUND, body = ErrorResponse, description = "Item does not exist"),
    )
)]
pub async fn update_item(
    Path(name): Path<String>,
    State(state): State<SharedState>,
    WithRejection(Json(payload), _): WithRejection<Json<CreateItem>, ApiError>,
) -> Result<Json<Item>, ApiError> {
    if payload.name != name {
        return Err(ApiError::BadRequest(format!(
            "Body name `{}` does not match path name `{name}`",
            payload.name
        )));
    }
    let item = match payload.id {
        Some(id) => Item::new(name, id)?,
        None => Item::new_with_random_id(name),
    };
    let mut existing = state
        .db
        .get_mut(&item.name)
        .ok_or_else(|| ApiError::NotFound(format!("Item does not exist: {}", item.name)))?;
    *existing = item.clone();
    drop(existing);
    log_info!("Updated item: {}", item.name);
    Ok(Json(item))
}

/// List items with optional pagination.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/items",
    tag = "items",
    params(ItemListQuery),
    responses(
        (status = OK, body = ItemListResponse, description = "Sorted item names")
    )
)]
pub async fn list_items(
    Query(query): Query<ItemListQuery>,
    State(state): State<SharedState>,
) -> Json<ItemListResponse> {
    let mut names: Vec<String> = state.db.iter().map(|entry| entry.key().clone()).collect();
    names.sort_unstable();
    let num_items = names.len();
    let names = names
        .into_iter()
        .skip(query.skip.unwrap_or_default())
        .take(query.limit.unwrap_or(usize::MAX))
        .collect();
    Json(ItemListResponse { num_items, names })
}

/// Look up an item by name.
fn find_item(state: &SharedState, name: &str) -> Result<Item, ApiError> {
    state
        .db
        .get(name)
        .map(|entry| entry.value().clone())
        .ok_or_else(|| ApiError::NotFound(format!("Item does not exist: {name}")))
}
