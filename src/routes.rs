use axum::response::IntoResponse;
use axum::{extract::Query, http::StatusCode, Extension, Json};
use chrono::Utc;

use crate::utils::{CreateUser, SimpleResponse, User, UserQuery, UserResponse, VersionInfo};
use crate::{build, GlobalState};

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
/// Example for doing a POST with some data
pub async fn create_user(
    Extension(state): Extension<GlobalState>,
    Json(payload): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    let mut users = state.write().await;
    let user = User::new(payload.username);
    users.insert(user.username.clone(), user.clone());
    tracing::info!("Create user: {}", user.username);
    // This will be converted into a JSON response with a status code of `201 Created`.
    (StatusCode::CREATED, Json(user))
}

#[axum::debug_handler]
/// Example for using query parameters
pub async fn query_user(Query(user): Query<UserQuery>, Extension(state): Extension<GlobalState>) -> impl IntoResponse {
    tracing::info!("Query user: {}", user.username);
    let users = state.read().await;
    match users.get(&user.username) {
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
/// Return version information
pub async fn version() -> (StatusCode, Json<VersionInfo>) {
    tracing::info!("Version: {}", build::PKG_VERSION);
    (StatusCode::OK, Json(VersionInfo::from_build_info()))
}
