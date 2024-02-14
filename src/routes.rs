use axum::response::IntoResponse;
use axum::{extract::Query, http::StatusCode, Extension, Json};
use chrono::Utc;

use crate::build;
use crate::types::{
    CreateUser, CreateUserResponse, SharedState, SimpleResponse, User, UserListResponse, UserQuery, UserResponse,
    VersionInfo,
};

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
) -> impl IntoResponse {
    let mut state = state.write().await;
    if state.db.get(&payload.username).is_some() {
        tracing::error!("User already exists: {}", payload.username);
        return CreateUserResponse::Error(SimpleResponse {
            message: format!("User already exists: {}", payload.username),
        });
    }
    let user = User::new(payload.username);
    state.db.insert(user.username.clone(), user.clone());
    tracing::info!("Create user: {}", user.username);
    CreateUserResponse::Created(user)
}

#[axum::debug_handler]
/// Get user info.
/// Example for using query parameters.
pub async fn query_user(Query(user): Query<UserQuery>, Extension(state): Extension<SharedState>) -> impl IntoResponse {
    tracing::info!("Query user: {}", user.username);
    let state = state.read().await;
    match state.db.get(&user.username) {
        Some(existing_user) => {
            tracing::info!("{:?}", existing_user);
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
pub async fn list_users(Extension(state): Extension<SharedState>) -> (StatusCode, Json<UserListResponse>) {
    tracing::debug!("List users");
    let state = state.read().await;
    let usernames = state.db.keys().map(|key| key.to_string()).collect::<Vec<String>>();
    let num_users = usernames.len();
    let response = UserListResponse { num_users, usernames };
    tracing::debug!("List users: found {num_users} users");
    (StatusCode::OK, Json(response))
}

#[axum::debug_handler]
/// Return version information
pub async fn version() -> (StatusCode, Json<VersionInfo>) {
    tracing::info!("Version: {}", build::PKG_VERSION);
    (StatusCode::OK, Json(VersionInfo::from_build_info()))
}
