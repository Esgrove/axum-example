use axum::response::IntoResponse;
use axum::{extract::Query, http::StatusCode, Extension, Json};
use chrono::Utc;

use crate::build;
use crate::types::{
    CreateUser, CreateUserResponse, MessageResponse, SharedState, User, UserListResponse, UserQuery, UserResponse,
    VersionInfo,
};

// Debug handler macro generates better error messages during compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html

/// Root returns a simple json response with the current date and time.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/root",
    responses(
        (status = 200, body = [SimpleResponse], description = "Show current date and time")
    )
)]
pub async fn root() -> (StatusCode, Json<MessageResponse>) {
    let datetime = Utc::now().to_rfc2822();
    tracing::info!("Root: {}", datetime);
    (StatusCode::OK, Json(MessageResponse { message: datetime }))
}

/// Return version information for API.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/version",
    responses(
        (status = 200, body = [VersionInfo], description = "Version information")
    )
)]
pub async fn version() -> (StatusCode, Json<VersionInfo>) {
    tracing::info!("Version: {}", build::PKG_VERSION);
    (StatusCode::OK, Json(VersionInfo::from_build_info()))
}

/// Get user info.
/// Example for using query parameters.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/user",
    params(UserQuery),
    responses(
        (status = 200, description = "Found existing user", body = [User]),
        (status = 400, description = "User does not exist", body = [SimpleResponse])
    )
)]
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
            UserResponse::Error(MessageResponse {
                message: format!("User does not exist: {}", user.username),
            })
        }
    }
}

/// Create new user.
/// Example for doing post with data.
#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/users",
    request_body = CreateUser,
    responses(
        (status = CREATED, body = [User], description = "New user created"),
        (status = CONFLICT, body = [SimpleResponse], description = "User already exists")
    )
)]
pub async fn create_user(
    Extension(state): Extension<SharedState>,
    Json(payload): Json<CreateUser>,
) -> impl IntoResponse {
    let mut state = state.write().await;
    if state.db.get(&payload.username).is_some() {
        tracing::error!("User already exists: {}", payload.username);
        return CreateUserResponse::Error(MessageResponse::new(format!(
            "User already exists: {}",
            payload.username
        )));
    }
    let user = User::new(payload.username);
    state.db.insert(user.username.clone(), user.clone());
    tracing::info!("Create user: {}", user.username);
    CreateUserResponse::Created(user)
}

/// List all users
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/list_users",
    responses(
        (status = 200, body = [UserListResponse])
    )
)]
pub async fn list_users(Extension(state): Extension<SharedState>) -> (StatusCode, Json<UserListResponse>) {
    tracing::debug!("List users");
    let state = state.read().await;
    let usernames = state.db.keys().map(|key| key.to_string()).collect::<Vec<String>>();
    let num_users = usernames.len();
    let response = UserListResponse { num_users, usernames };
    tracing::debug!("List users: found {num_users} users");
    (StatusCode::OK, Json(response))
}
