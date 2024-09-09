use std::fmt;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{async_trait, Json};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::build;
use crate::types::{Config, Item};

pub static VERSION_INFO: VersionInfo = VersionInfo {
    name: build::PROJECT_NAME,
    version: build::PKG_VERSION,
    deploy_tag: build::DEPLOYMENT_TAG,
    build_time: build::BUILD_TIME_3339,
    branch: build::BRANCH,
    commit: build::COMMIT_HASH,
    commit_time: build::COMMIT_DATE,
    build_os: build::BUILD_OS,
    rust_version: build::RUST_VERSION,
    rust_channel: build::RUST_CHANNEL,
};

/// Post payload for creating a new item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateItem {
    #[schema(example = "esgrove")]
    pub name: String,
    /// Optional id field, allowing clients to specify an id or have the server generate one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
}

/// Query item information with name
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ItemQuery {
    #[schema(example = "esgrove")]
    pub name: String,
}

/// Simple response with a message
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MessageResponse {
    /// Message can be either information or an error message
    #[schema(example = "Item already exists: esgrove")]
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ItemListResponse {
    /// The total number of items
    #[schema(example = "5")]
    pub num_items: usize,
    /// List of all names
    pub names: Vec<String>,
}

/// API version information.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VersionInfo {
    #[schema(example = "axum-example")]
    pub name: &'static str,
    #[schema(example = "1.0.0")]
    pub version: &'static str,
    #[schema(example = "2024.02.14-100")]
    pub deploy_tag: &'static str,
    #[schema(example = "2024-02-14 14:42:35 +02:00")]
    pub build_time: &'static str,
    #[schema(example = "main")]
    pub branch: &'static str,
    #[schema(example = "ee9ec805f61944653a56a7e429b2fad03232be49")]
    pub commit: &'static str,
    #[schema(example = "2024-02-14 12:42:18 +00:00")]
    pub commit_time: &'static str,
    #[schema(example = "macos-aarch64")]
    pub build_os: &'static str,
    #[schema(example = "rustc 1.76.0 (07dca489a 2024-02-04)")]
    pub rust_version: &'static str,
    #[schema(example = "stable-aarch64-apple-darwin")]
    pub rust_channel: &'static str,
}

/// Authentication failed response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuthErrorResponse {
    message: String,
}

/// Combined response for JSON deserialization errors.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RejectionErrorResponse {
    error: String,
    message: String,
}

#[derive(Debug)]
pub struct RejectionError {
    status: StatusCode,
    message: String,
    rejection: String,
}

/// Custom error type that enables using anyhow error handling in routes.
/// This is used for server-side errors and returns status code 500 with the error message.
pub struct ServerError(pub anyhow::Error);

/// Custom extractor for checking api key.
/// Note: requires the Config extension to be present in the route as well,
/// so the correct api key can be accessed.
pub struct ApiKeyExtractor;

#[async_trait]
impl<S> FromRequestParts<S> for ApiKeyExtractor
where
    S: Send + Sync,
{
    type Rejection = AuthErrorResponse;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let config = parts
            .extensions
            .get::<Arc<Config>>()
            .ok_or_else(|| AuthErrorResponse::new_from_str("Config extension missing from route"))?;

        match parts.headers.get("api-key").and_then(|key| key.to_str().ok()) {
            Some(api_key) if api_key == config.api_key => Ok(Self),
            Some(api_key) => {
                tracing::warn!("Invalid API key: {} {}", parts.method.as_str(), parts.uri.path());
                Err(AuthErrorResponse::new(format!("Invalid API key: '{api_key}'")))
            }
            None => {
                tracing::warn!("Missing API key header: {} {}", parts.method.as_str(), parts.uri.path());
                Err(AuthErrorResponse::new_from_str("Missing api-key header"))
            }
        }
    }
}

pub enum ItemResponse {
    Found(Item),
    Error(MessageResponse),
}

pub enum CreateItemResponse {
    Created(Item),
    Error(MessageResponse),
}

pub enum RemoveItemResponse {
    Removed(Item),
    Error(MessageResponse),
}

impl MessageResponse {
    pub const fn new(message: String) -> Self {
        Self { message }
    }

    #[allow(unused)]
    pub fn new_from_str(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl AuthErrorResponse {
    pub const fn new(message: String) -> Self {
        Self { message }
    }

    pub fn new_from_str(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl RemoveItemResponse {
    // Accept any type that implements std::fmt::Display, not just strings.
    pub fn new_error<T: std::fmt::Display>(message: T) -> Self {
        RemoveItemResponse::Error(MessageResponse::new(format!("{}", message)))
    }
}

impl IntoResponse for CreateItemResponse {
    fn into_response(self) -> Response {
        match self {
            CreateItemResponse::Created(item) => (StatusCode::CREATED, Json(item)).into_response(),
            CreateItemResponse::Error(message) => (StatusCode::CONFLICT, Json(message)).into_response(),
        }
    }
}

impl IntoResponse for ItemResponse {
    fn into_response(self) -> Response {
        match self {
            ItemResponse::Found(item) => (StatusCode::OK, Json(item)).into_response(),
            ItemResponse::Error(message) => (StatusCode::NOT_FOUND, Json(message)).into_response(),
        }
    }
}

impl IntoResponse for RemoveItemResponse {
    fn into_response(self) -> Response {
        match self {
            RemoveItemResponse::Removed(item) => (StatusCode::OK, Json(item)).into_response(),
            RemoveItemResponse::Error(message) => (StatusCode::NOT_FOUND, Json(message)).into_response(),
        }
    }
}

impl IntoResponse for AuthErrorResponse {
    fn into_response(self) -> Response {
        let body = Json(self);
        (StatusCode::UNAUTHORIZED, body).into_response()
    }
}

// Tell axum how to convert `ServerError` into a response.
impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", self.0))).into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>`
// to turn them into `Result<_, ServerError>`.
// This way we don't need to do that manually.
impl<E> From<E> for ServerError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl From<JsonRejection> for RejectionError {
    fn from(error: JsonRejection) -> Self {
        Self {
            status: error.status(),
            message: error.body_text(),
            rejection: match error {
                JsonRejection::JsonDataError(_) => "JsonDataError".to_string(),
                JsonRejection::JsonSyntaxError(_) => "JsonSyntaxError".to_string(),
                JsonRejection::MissingJsonContentType(_) => "MissingJsonContentType".to_string(),
                JsonRejection::BytesRejection(_) => "BytesRejection".to_string(),
                _ => "Unknown rejection".to_string(),
            },
        }
    }
}

impl IntoResponse for RejectionError {
    fn into_response(self) -> Response {
        let response = RejectionErrorResponse {
            error: self.rejection,
            message: self.message,
        };

        (self.status, Json(response)).into_response()
    }
}

impl VersionInfo {
    pub fn to_string_pretty(&self) -> String {
        format!(
            "Version information:\n\
             \x20 name: {}\n\
             \x20 version: {}\n\
             \x20 build time: {}\n\
             \x20 branch: {}\n\
             \x20 commit: {}\n\
             \x20 commit time: {}\n\
             \x20 build OS: {}\n\
             \x20 rust version: {}\n\
             \x20 rust channel: {}",
            self.name,
            self.version,
            self.build_time,
            self.branch,
            self.commit,
            self.commit_time,
            self.build_os,
            self.rust_version,
            self.rust_channel,
        )
    }
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Version information: ")?;
        write!(f, "name: {}, ", self.name)?;
        write!(f, "version: {}, ", self.version)?;
        write!(f, "build time: {}, ", self.build_time)?;
        write!(f, "branch: {}, ", self.branch)?;
        write!(f, "commit: {}, ", self.commit)?;
        write!(f, "commit time: {}, ", self.commit_time)?;
        write!(f, "build OS: {}, ", self.build_os)?;
        write!(f, "rust version: {}, ", self.rust_version)?;
        write!(f, "rust channel: {}, ", self.rust_channel)
    }
}
