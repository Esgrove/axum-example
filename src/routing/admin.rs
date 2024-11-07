//! Admin Routes.
//!
//! Admin routes that require an api key to use.
//!

use std::sync::Arc;

use axum::extract::{Extension, Json};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::delete;
use axum::Router;

use crate::schemas::{AuthErrorResponse, MessageResponse, RemoveItemResponse};
use crate::types::{ApiKeyExtractor, Config, Item, SharedState};

/// Create admin routes.
///
/// Helper method to easily nest all admin routes under common prefix.
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/clear_items", delete(delete_all_items))
        .route("/remove/:name", delete(remove_item))
}

/// Remove all items.
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/clear_items",
    security(
        ("api_key" = [])
    ),
    responses(
        (status = 200, body = [MessageResponse], description = "Report number of items deleted"),
        (status = UNAUTHORIZED, body = [AuthErrorResponse], description = "Unauthorized"),
    )
)]
async fn delete_all_items(
    _api_key: ApiKeyExtractor,
    State(state): State<SharedState>,
    Extension(_config): Extension<Arc<Config>>,
) -> impl IntoResponse {
    let number_of_items = state.db.len();
    state.db.clear();
    tracing::debug!("Delete all {number_of_items} items");
    (
        StatusCode::OK,
        Json(MessageResponse::new(format!("Removed {number_of_items} items"))),
    )
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/remove/:name",
    security(
        ("api_key" = [])
    ),
    responses(
        (status = OK, body = [Item], description = "Item removed"),
        (status = NOT_FOUND, body = [MessageResponse], description = "Item does not exist"),
        (status = UNAUTHORIZED, body = [AuthErrorResponse], description = "Unauthorized"),
    )
)]
/// Remove item with given name.
async fn remove_item(
    _api_key: ApiKeyExtractor,
    State(state): State<SharedState>,
    Extension(_config): Extension<Arc<Config>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    state.db.remove(&name).map_or_else(
        || {
            tracing::error!("Remove item failed for non-existing name: {}", name);
            RemoveItemResponse::new_error(format!("Item does not exist: {name}"))
        },
        |existing_item| {
            tracing::debug!("Remove item: {}", name);
            RemoveItemResponse::Removed(existing_item.1)
        },
    )
}
