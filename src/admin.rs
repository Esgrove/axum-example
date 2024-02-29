use crate::types::{MessageResponse, RemoveItemResponse, SharedState};

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    extract::{Path, State},
    routing::delete,
    Json, Router,
};

/// Create admin routes.
/// Helper method to easily nest all admin routes under common prefix.
pub fn admin_routes() -> Router<SharedState> {
    Router::new()
        .route("/clear_items", delete(delete_all_items))
        .route("/remove/:name", delete(remove_item))
}

/// Remove all items.
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/clear_items",
    responses(
    (status = 200, body = [MessageResponse], description = "Report number of items deleted")
    )
)]
async fn delete_all_items(State(state): State<SharedState>) -> impl IntoResponse {
    let mut state = state.write().await;
    let number_of_items = state.db.len();
    tracing::info!("Delete all {number_of_items} items");
    state.db.clear();
    (
        StatusCode::OK,
        Json(MessageResponse::new(format!("Removed {number_of_items} items"))),
    )
}

/// Try to remove item with given name.
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/remove/:name",
    responses(
    (status = OK, body = [Item], description = "Item removed"),
    (status = NOT_FOUND, body = [MessageResponse], description = "Item does not exist")
    )
)]
async fn remove_item(Path(name): Path<String>, State(state): State<SharedState>) -> impl IntoResponse {
    let mut state = state.write().await;
    match state.db.remove(&name) {
        Some(existing_item) => {
            tracing::info!("Remove item: {}", name);
            RemoveItemResponse::Removed(existing_item.clone())
        }
        None => {
            tracing::error!("Remove item failed for non-existing name: {}", name);
            RemoveItemResponse::new_error(format!("Item does not exist: {}", name))
        }
    }
}
