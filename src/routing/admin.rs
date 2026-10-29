//! Admin routes.
//!
//! Contains endpoints nested under `/admin`.
//! Every route in this module requires the custom API-key extractor.

#![allow(clippy::unused_async, reason = "Axum handlers must return futures")]
#![allow(
    clippy::missing_errors_doc,
    reason = "error responses are documented in the utoipa `responses` list, and doc comments become OpenAPI descriptions"
)]

use axum::Router;
use axum::extract::{Json, Path, State};
use axum::routing::delete;

use crate::errors::ApiError;
use crate::logging::log_info;
use crate::schemas::{ErrorResponse, MessageResponse};
use crate::state::SharedState;
use crate::types::{ApiKeyExtractor, Item};

/// Create admin routes.
///
/// Returned without a prefix so the caller can nest them under `/admin`.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/clear_items", delete(delete_all_items))
        .route("/remove/{name}", delete(remove_item))
}

/// Remove all items.
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/clear_items",
    tag = "admin",
    security(("api_key" = [])),
    responses(
        (status = OK, body = MessageResponse, description = "Report number of items deleted"),
        (status = UNAUTHORIZED, body = ErrorResponse, description = "Missing or invalid API key"),
    )
)]
pub async fn delete_all_items(_api_key: ApiKeyExtractor, State(state): State<SharedState>) -> Json<MessageResponse> {
    let number_of_items = state.db.len();
    state.db.clear();
    log_info!("Deleted all {number_of_items} items");
    Json(MessageResponse::new(format!("Removed {number_of_items} items")))
}

/// Remove the item with the given name.
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/remove/{name}",
    tag = "admin",
    security(("api_key" = [])),
    params(("name" = String, Path, description = "Item name")),
    responses(
        (status = OK, body = Item, description = "Item removed"),
        (status = NOT_FOUND, body = ErrorResponse, description = "Item does not exist"),
        (status = UNAUTHORIZED, body = ErrorResponse, description = "Missing or invalid API key"),
    )
)]
pub async fn remove_item(
    _api_key: ApiKeyExtractor,
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> Result<Json<Item>, ApiError> {
    let (_, item) = state
        .db
        .remove(&name)
        .ok_or_else(|| ApiError::NotFound(format!("Item does not exist: {name}")))?;
    log_info!("Removed item: {name}");
    Ok(Json(item))
}
