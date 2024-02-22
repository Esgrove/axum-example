use crate::build;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::level_filters::LevelFilter;
use utoipa::{IntoParams, ToSchema};

use std::collections::HashMap;
use std::sync::Arc;

pub type SharedState = Arc<RwLock<AppState>>;

/// Logging level CLI parameter
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Shared state that simulates a database
#[derive(Default)]
pub struct AppState {
    pub db: HashMap<String, User>,
}

/// Post payload for creating a new user
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateUser {
    #[schema(example = "esgrove")]
    pub username: String,
}

/// Query user information with username
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct UserQuery {
    #[schema(example = "esgrove")]
    pub username: String,
}

/// User information
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, ToSchema)]
pub struct User {
    /// `id` will be in range 1000..9999
    #[schema(example = "1234")]
    pub id: u64,
    #[schema(example = "esgrove")]
    pub username: String,
}

/// Simple response with a message
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MessageResponse {
    /// Message can be either information or an error message
    #[schema(example = "User already exists: esgrove")]
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserListResponse {
    /// The total number of users
    #[schema(example = "5")]
    pub num_users: usize,
    /// List of all usernames
    pub usernames: Vec<String>,
}

/// API version information.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VersionInfo {
    #[schema(example = "axum-example")]
    pub name: String,
    #[schema(example = "0.5.0")]
    pub version: String,
    #[schema(example = "2024-02-14 14:42:35 +02:00")]
    pub build_time: String,
    #[schema(example = "main")]
    pub branch: String,
    #[schema(example = "ee9ec805f61944653a56a7e429b2fad03232be49")]
    pub commit: String,
    #[schema(example = "2024-02-14 12:42:18 +00:00")]
    pub commit_time: String,
    #[schema(example = "macos-aarch64")]
    pub build_os: String,
    #[schema(example = "rustc 1.76.0 (07dca489a 2024-02-04)")]
    pub rust_version: String,
    #[schema(example = "stable-aarch64-apple-darwin")]
    pub rust_channel: String,
}

pub enum UserResponse {
    Found(User),
    Error(MessageResponse),
}

pub enum CreateUserResponse {
    Created(User),
    Error(MessageResponse),
}

pub enum RemoveUserResponse {
    Removed(User),
    Error(MessageResponse),
}

/// Custom error type that enables using anyhow error handling in routes.
struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>`
// to turn them into `Result<_, AppError>`.
// This way we don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl RemoveUserResponse {
    // Accept any type that implements std::fmt::Display, not just strings.
    pub fn new_error<T: std::fmt::Display>(message: T) -> Self {
        RemoveUserResponse::Error(MessageResponse::new(format!("{}", message)))
    }
}

impl IntoResponse for CreateUserResponse {
    fn into_response(self) -> Response {
        match self {
            CreateUserResponse::Created(user) => (StatusCode::CREATED, Json(user)).into_response(),
            CreateUserResponse::Error(message) => (StatusCode::CONFLICT, Json(message)).into_response(),
        }
    }
}

impl IntoResponse for UserResponse {
    fn into_response(self) -> Response {
        match self {
            UserResponse::Found(user) => (StatusCode::OK, Json(user)).into_response(),
            UserResponse::Error(message) => (StatusCode::NOT_FOUND, Json(message)).into_response(),
        }
    }
}

impl IntoResponse for RemoveUserResponse {
    fn into_response(self) -> Response {
        match self {
            RemoveUserResponse::Removed(user) => (StatusCode::OK, Json(user)).into_response(),
            RemoveUserResponse::Error(message) => (StatusCode::NOT_FOUND, Json(message)).into_response(),
        }
    }
}

impl MessageResponse {
    pub fn new(message: String) -> MessageResponse {
        MessageResponse { message }
    }
}

impl User {
    pub fn new(username: String) -> User {
        let mut rng = rand::thread_rng();
        let id: u64 = rng.gen_range(1000..=9999);
        User { username, id }
    }
}

impl LogLevel {
    pub fn to_filter(&self) -> LevelFilter {
        match self {
            LogLevel::Trace => LevelFilter::TRACE,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Error => LevelFilter::ERROR,
        }
    }
}

impl VersionInfo {
    pub fn from_build_info() -> VersionInfo {
        VersionInfo {
            name: build::PROJECT_NAME.to_string(),
            version: build::PKG_VERSION.to_string(),
            build_time: build::BUILD_TIME.to_string(),
            branch: build::BRANCH.to_string(),
            commit: build::COMMIT_HASH.to_string(),
            commit_time: build::COMMIT_DATE.to_string(),
            build_os: build::BUILD_OS.to_string(),
            rust_version: build::RUST_VERSION.to_string(),
            rust_channel: build::RUST_CHANNEL.to_string(),
        }
    }
}
