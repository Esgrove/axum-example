use crate::build;
use axum::{extract::Query, http::StatusCode, response::IntoResponse, Json};
use axum_macros::debug_handler;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateUser {
    username: String,
}

#[derive(Deserialize)]
pub struct UserQuery {
    pub username: String,
}

#[derive(Serialize)]
pub struct User {
    id: u64,
    username: String,
}

#[derive(Serialize)]
pub struct Response {
    message: String,
}

#[derive(Serialize)]
pub struct VersionInfo {
    name: String,
    version: String,
    build_time: String,
    branch: String,
    commit: String,
    commit_time: String,
    os: String,
    rust_version: String,
    rust_channel: String,
}

// basic handler that responds with a static string
#[debug_handler]
/// Root returns a simple json response with the current date and time
pub async fn root() -> impl IntoResponse {
    let datetime = Utc::now().to_rfc2822();
    tracing::info!("Root: {}", datetime);
    (StatusCode::OK, Json(Response { message: datetime }))
}

// debug handler macro generates better error messages in Rust compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html
#[debug_handler]
/// Example for doing a POST with some data
pub async fn create_user(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> impl IntoResponse {
    // insert your application logic here
    let user = User {
        id: 1337,
        username: payload.username,
    };

    tracing::info!("Create user: {}", user.username);

    // This will be converted into a JSON response with a status code of `201 Created`.
    (StatusCode::CREATED, Json(user))
}

#[debug_handler]
/// Example for using query parameters
pub async fn query_user(Query(user): Query<UserQuery>) -> impl IntoResponse {
    tracing::info!("Query user: {}", user.username);

    (
        StatusCode::OK,
        Json(User {
            id: 1234,
            username: user.username,
        }),
    )
}

#[debug_handler]
/// Root returns a simple json response with the current date and time
pub async fn version() -> impl IntoResponse {
    tracing::info!("Version: {}", build::VERSION);
    (
        StatusCode::OK,
        Json(VersionInfo {
            name: build::PROJECT_NAME.to_string(),
            version: build::VERSION.to_string(),
            build_time: build::BUILD_TIME.to_string(),
            branch: build::BRANCH.to_string(),
            commit: build::COMMIT_HASH.to_string(),
            commit_time: build::COMMIT_DATE.to_string(),
            os: build::BUILD_OS.to_string(),
            rust_version: build::RUST_VERSION.to_string(),
            rust_channel: build::RUST_CHANNEL.to_string(),
        }),
    )
}
