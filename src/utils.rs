use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing_subscriber::filter::LevelFilter;

use crate::build;

/// Logging level CLI parameter
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Payload for creating a new user
#[derive(Deserialize, Debug, Clone)]
pub struct CreateUser {
    pub username: String,
}

/// Query with username
#[derive(Deserialize, Debug, Clone)]
pub struct UserQuery {
    pub username: String,
}

#[derive(Serialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct User {
    pub id: u64,
    pub username: String,
}

/// Simple response with a message
#[derive(Serialize, Debug, Clone)]
pub struct SimpleResponse {
    pub message: String,
}

/// API version information
#[derive(Serialize, Debug, Clone)]
pub struct VersionInfo {
    pub name: String,
    pub version: String,
    pub build_time: String,
    pub branch: String,
    pub commit: String,
    pub commit_time: String,
    pub build_os: String,
    pub rust_version: String,
    pub rust_channel: String,
}

pub enum UserResponse {
    Found(User),
    Error(SimpleResponse),
}

impl IntoResponse for UserResponse {
    fn into_response(self) -> Response {
        match self {
            UserResponse::Found(user) => (StatusCode::OK, Json(user)).into_response(),
            UserResponse::Error(resp) => (StatusCode::NOT_FOUND, Json(resp)).into_response(),
        }
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
