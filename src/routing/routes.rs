//! Public routes.
//!
//! Contains unauthenticated service endpoints, item CRUD examples,
//! the health response, version information, and Prometheus metrics output.

use axum::Json;
use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::WithRejection;
use chrono::{SecondsFormat, Utc};

use crate::schemas::{
    CreateItem, CreateItemResponse, HealthResponse, ItemListQuery, ItemListResponse, ItemQuery, ItemResponse,
    MessageResponse, RejectionError, RejectionErrorResponse, ServerError, VERSION_INFO, VersionInfo,
};
use crate::types::{Config, Item, SharedState};
use crate::version;

// Debug handler macro generates better error messages during compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html

/// Return API name with the current date and time.
///
/// Used primarily as a health check to verify the API is up and responding.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    responses(
        (status = OK, body = [MessageResponse], description = "Return API name with current datetime")
    )
)]
pub async fn root() -> (StatusCode, Json<MessageResponse>) {
    let datetime = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    crate::log_debug!("Root: {}", datetime);
    (
        StatusCode::OK,
        Json(MessageResponse::new(format!("{} {}", version::PACKAGE_NAME, datetime))),
    )
}

/// Return basic service health information.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = OK, body = [HealthResponse], description = "Service health information")
    )
)]
pub async fn health(
    State(state): State<SharedState>,
    Extension(config): Extension<Arc<Config>>,
) -> (StatusCode, Json<HealthResponse>) {
    let uptime_ms = u64::try_from(state.uptime().as_millis()).unwrap_or(u64::MAX);
    (
        StatusCode::OK,
        Json(HealthResponse {
            service: version::PACKAGE_NAME.to_string(),
            version: version::PACKAGE_VERSION.to_string(),
            environment: config.env.to_string(),
            status: "ok".to_string(),
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            start_time: state.start_time_utc().to_rfc3339_opts(SecondsFormat::Millis, true),
            uptime_ms,
        }),
    )
}

/// Return OpenTelemetry metrics in Prometheus text format.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/metrics",
    responses(
        (status = OK, description = "Prometheus metrics in text format", content_type = "text/plain"),
        (status = INTERNAL_SERVER_ERROR, body = [MessageResponse], description = "Metrics encoding failed")
    )
)]
pub async fn metrics(State(state): State<SharedState>) -> Response {
    match state.telemetry().render_prometheus() {
        Ok((body, content_type)) => ([(CONTENT_TYPE, content_type)], body).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(MessageResponse::new(format!("Failed to render metrics: {err}"))),
        )
            .into_response(),
    }
}

/// Return version and build information.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/version",
    responses(
        (status = OK, body = [VersionInfo], description = "Version information")
    )
)]
pub async fn version() -> (StatusCode, Json<&'static VersionInfo>) {
    crate::log_debug!("Version: {}", version::PACKAGE_VERSION);
    (StatusCode::OK, Json(&VERSION_INFO))
}

/// Get item info.
///
/// Example for using query parameters.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/item",
    params(ItemQuery),
    responses(
        (status = 200, body = [Item], description = "Found existing item"),
        (status = 400, body = [MessageResponse], description = "Item does not exist")
    )
)]
pub async fn query_item(Query(item): Query<ItemQuery>, State(state): State<SharedState>) -> impl IntoResponse {
    crate::log_debug!("Query item: {}", item.name);
    if let Some(existing_item) = state.db.get(&item.name) {
        crate::log_info!("{:?}", existing_item);
        ItemResponse::Found(existing_item.clone())
    } else {
        crate::log_error!("Item not found: {}", item.name);
        ItemResponse::Error(MessageResponse {
            message: format!("Item does not exist: {}", item.name),
        })
    }
}

/// Create new item.
///
/// Example for doing post with data.
#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/items",
    request_body = CreateItem,
    responses(
        (status = CREATED, body = [Item], description = "New item created"),
        (status = CONFLICT, body = [MessageResponse], description = "Item already exists"),
        (status = BAD_REQUEST, body = [RejectionErrorResponse], description = "Malformed JSON data"),
        (status = UNPROCESSABLE_ENTITY, body = [RejectionErrorResponse], description = "JSON deserialization error"),
        (status = UNSUPPORTED_MEDIA_TYPE, body = [RejectionErrorResponse], description = "Missing JSON content type header"),
        (status = PAYLOAD_TOO_LARGE, body = [RejectionErrorResponse], description = "Too many bytes"),
    )
)]
pub async fn create_item(
    State(state): State<SharedState>,
    WithRejection(Json(payload), _): WithRejection<Json<CreateItem>, RejectionError>,
) -> Result<CreateItemResponse, ServerError> {
    if state.db.contains_key(&payload.name) {
        crate::log_error!("Item already exists: {}", payload.name);
        return Ok(CreateItemResponse::Error(MessageResponse::new(format!(
            "Item already exists: {}",
            payload.name
        ))));
    }
    // Check if id was provided by client
    let item = match payload.id {
        // Creating item with client provided id can fail if id is not valid,
        // which will cause this method to exit with `ServerError` due to the `?` operator.
        Some(id) => Item::new(payload.name, id)?,
        _ => Item::new_with_random_id(payload.name),
    };
    // TODO: should probably ensure ids are unique too
    state.db.insert(item.name.clone(), item.clone());
    crate::log_debug!("Create item: {}", item.name);
    Ok(CreateItemResponse::Created(item))
}

/// List all items.
///
/// Supports optional `skip` and `limit` query parameters for simple pagination.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/items",
    params(ItemListQuery),
    responses(
        (status = 200, body = [ItemListResponse])
    )
)]
pub async fn list_items(
    Query(query): Query<ItemListQuery>,
    State(state): State<SharedState>,
) -> (StatusCode, Json<ItemListResponse>) {
    crate::log_debug!("List items");
    let mut names: Vec<String> = state.db.iter().map(|entry| entry.key().clone()).collect();
    names.sort();
    let num_items = names.len();
    let skip = query.skip.unwrap_or_default();
    let names = names
        .into_iter()
        .skip(skip)
        .take(query.limit.unwrap_or(usize::MAX))
        .collect();
    crate::log_debug!("List items: found {num_items} items");
    (StatusCode::OK, Json(ItemListResponse { num_items, names }))
}
