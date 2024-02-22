use crate::types::{MessageResponse, RemoveUserResponse, SharedState};

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    extract::{Path, State},
    routing::delete,
    Json, Router,
};

/// Create admin routes.
pub fn admin_routes() -> Router<SharedState> {
    Router::new()
        .route("/clear_users", delete(delete_all_users))
        .route("/remove/:username", delete(remove_user))
}

/// Remove all users.
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/clear_users",
    responses(
    (status = 200, body = [MessageResponse], description = "Report number of users deleted")
    )
)]
async fn delete_all_users(State(state): State<SharedState>) -> impl IntoResponse {
    let mut state = state.write().await;
    let number_of_users = state.db.len();
    tracing::info!("Delete all {number_of_users} users");
    state.db.clear();
    (
        StatusCode::OK,
        Json(MessageResponse::new(format!("Removed {number_of_users} users"))),
    )
}

/// Try to remove user with given username.
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/admin/remove/:username",
    responses(
    (status = OK, body = [User], description = "User removed"),
    (status = NOT_FOUND, body = [MessageResponse], description = "User does not exist")
    )
)]
async fn remove_user(Path(username): Path<String>, State(state): State<SharedState>) -> impl IntoResponse {
    let mut state = state.write().await;
    match state.db.remove(&username) {
        Some(existing_user) => {
            tracing::info!("Remove user: {}", username);
            RemoveUserResponse::Removed(existing_user.clone())
        }
        None => {
            tracing::error!("Remove user failed for non-existing username: {}", username);
            RemoveUserResponse::new_error(format!("User does not exist: {}", username))
        }
    }
}
