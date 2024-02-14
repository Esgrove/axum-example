use axum::response::IntoResponse;
use axum::{extract::Query, http::StatusCode, Extension, Json};
use chrono::Utc;

use crate::build;
use crate::types::{CreateUser, SharedState, SimpleResponse, User, UserQuery, UserResponse, VersionInfo};

// Debug handler macro generates better error messages in Rust compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html

#[axum::debug_handler]
/// Root returns a simple json response with the current date and time
pub async fn root() -> (StatusCode, Json<SimpleResponse>) {
    let datetime = Utc::now().to_rfc2822();
    tracing::info!("Root: {}", datetime);
    (StatusCode::OK, Json(SimpleResponse { message: datetime }))
}

#[axum::debug_handler]
/// Create new user.
/// Example for doing a POST with some data.
pub async fn create_user(
    Extension(state): Extension<SharedState>,
    Json(payload): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let mut state = state.write().await;
    let user = User::new(payload.username);
    state.db.insert(user.username.clone(), user.clone());
    tracing::info!("Create user: {}", user.username);
    (StatusCode::CREATED, Json(user))
}

#[axum::debug_handler]
/// Get user info.
/// Example for using query parameters.
pub async fn query_user(Query(user): Query<UserQuery>, Extension(state): Extension<SharedState>) -> impl IntoResponse {
    tracing::info!("Query user: {}", user.username);
    let state = state.read().await;
    match state.db.get(&user.username) {
        Some(existing_user) => {
            tracing::info!("User {:?}", existing_user);
            UserResponse::Found(existing_user.clone())
        }
        None => {
            tracing::error!("User not found: {}", user.username);
            UserResponse::Error(SimpleResponse {
                message: format!("User does not exist: {}", user.username),
            })
        }
    }
}

#[axum::debug_handler]
/// List all users
pub async fn list_users(Extension(state): Extension<SharedState>) -> impl IntoResponse {
    tracing::debug!("List users");
    let state = state.read().await;
    let users = state
        .db
        .keys()
        .map(|key| key.to_string())
        .collect::<Vec<String>>()
        .join("\n");

    tracing::debug!("List users: found {} users", users.len());
    (StatusCode::OK, Json(users))
}

#[axum::debug_handler]
/// Return version information
pub async fn version() -> (StatusCode, Json<VersionInfo>) {
    tracing::info!("Version: {}", build::PKG_VERSION);
    (StatusCode::OK, Json(VersionInfo::from_build_info()))
}
