//! Schemas.
//!
//! Contains type definitions for all public-facing types,
//! meaning everything that shows up in the `OpenAPI` documentation.
//!

use std::fmt;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::types::Item;
use crate::version;

pub static VERSION_INFO: VersionInfo = VersionInfo {
    name: version::PACKAGE_NAME,
    version: version::PACKAGE_VERSION,
    deploy_tag: version::DEPLOY_TAG,
    build_time: version::BUILD_TIME,
    branch: version::GIT_BRANCH,
    commit: version::GIT_COMMIT,
    rust_version: version::RUST_VERSION,
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

/// Optional pagination parameters for listing items.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
pub struct ItemListQuery {
    #[param(example = 0)]
    pub skip: Option<usize>,
    #[param(example = 10)]
    pub limit: Option<usize>,
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
    #[schema(example = "2024-02-14_14:42:35")]
    pub build_time: &'static str,
    #[schema(example = "main")]
    pub branch: &'static str,
    #[schema(example = "ee9ec805f61944653a56a7e429b2fad03232be49")]
    pub commit: &'static str,
    #[schema(example = "rustc 1.76.0 (07dca489a 2024-02-04)")]
    pub rust_version: &'static str,
}

/// Basic service health response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    #[schema(example = "axum-example")]
    pub service: String,
    #[schema(example = "0.13.0")]
    pub version: String,
    #[schema(example = "LOCAL")]
    pub environment: String,
    #[schema(example = "ok")]
    pub status: String,
    #[schema(example = "2026-06-10T09:00:00Z")]
    pub timestamp: String,
    #[schema(example = "2026-06-10T08:59:00Z")]
    pub start_time: String,
    #[schema(example = 1234)]
    pub uptime_ms: u64,
}

/// Not found response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotFoundResponse {
    #[schema(example = "Not Found")]
    pub error: String,
    #[schema(example = "Path does not exist")]
    pub message: String,
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

impl NotFoundResponse {
    pub fn new() -> Self {
        Self {
            error: "Not Found".to_string(),
            message: "Path does not exist".to_string(),
        }
    }
}

impl RemoveItemResponse {
    // Accept any type that implements std::fmt::Display, not just strings.
    pub fn new_error<T: std::fmt::Display>(message: T) -> Self {
        Self::Error(MessageResponse::new(format!("{message}")))
    }
}

impl IntoResponse for CreateItemResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Created(item) => (StatusCode::CREATED, Json(item)).into_response(),
            Self::Error(message) => (StatusCode::CONFLICT, Json(message)).into_response(),
        }
    }
}

impl IntoResponse for ItemResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Found(item) => (StatusCode::OK, Json(item)).into_response(),
            Self::Error(message) => (StatusCode::NOT_FOUND, Json(message)).into_response(),
        }
    }
}

impl IntoResponse for RemoveItemResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Removed(item) => (StatusCode::OK, Json(item)).into_response(),
            Self::Error(message) => (StatusCode::NOT_FOUND, Json(message)).into_response(),
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
             \x20 rust version: {}",
            self.name, self.version, self.build_time, self.branch, self.commit, self.rust_version,
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
        write!(f, "rust version: {}", self.rust_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::Body;
    use axum::extract::FromRequest;
    use http_body_util::BodyExt;
    use serde_json::Value;

    async fn response_json(response: Response) -> Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body should collect")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("body should be json")
    }

    #[test]
    fn constructors_build_expected_messages() {
        assert_eq!(MessageResponse::new_from_str("hello").message, "hello");
        assert_eq!(AuthErrorResponse::new_from_str("denied").message, "denied");

        let not_found = NotFoundResponse::new();
        assert_eq!(not_found.error, "Not Found");
        assert_eq!(not_found.message, "Path does not exist");
    }

    #[tokio::test]
    async fn create_item_response_maps_success_and_conflict_statuses() {
        let item = Item {
            id: 1234,
            name: "created".to_string(),
        };

        let response = CreateItemResponse::Created(item).into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response_json(response).await["name"], "created");

        let response = CreateItemResponse::Error(MessageResponse::new_from_str("exists")).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(response_json(response).await["message"], "exists");
    }

    #[tokio::test]
    async fn item_response_maps_found_and_missing_statuses() {
        let item = Item {
            id: 2345,
            name: "found".to_string(),
        };

        let response = ItemResponse::Found(item).into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response_json(response).await["name"], "found");

        let response = ItemResponse::Error(MessageResponse::new_from_str("missing")).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response_json(response).await["message"], "missing");
    }

    #[tokio::test]
    async fn remove_item_response_maps_removed_and_missing_statuses() {
        let item = Item {
            id: 3456,
            name: "removed".to_string(),
        };

        let response = RemoveItemResponse::Removed(item).into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response_json(response).await["name"], "removed");

        let response = RemoveItemResponse::new_error("gone").into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response_json(response).await["message"], "gone");
    }

    #[tokio::test]
    async fn error_responses_map_to_expected_statuses() {
        let response = AuthErrorResponse::new_from_str("bad key").into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response_json(response).await["message"], "bad key");

        let response = ServerError(anyhow::anyhow!("boom")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            response_json(response)
                .await
                .as_str()
                .expect("server error body should be a string")
                .contains("boom")
        );
    }

    #[tokio::test]
    async fn rejection_error_preserves_status_and_kind() {
        let response = RejectionError {
            status: StatusCode::BAD_REQUEST,
            message: "malformed".to_string(),
            rejection: "JsonSyntaxError".to_string(),
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(response).await;
        assert_eq!(body["error"], "JsonSyntaxError");
        assert_eq!(body["message"], "malformed");
    }

    #[test]
    fn version_info_formats_compact_and_pretty_strings() {
        let compact = VERSION_INFO.to_string();
        assert!(compact.contains(version::PACKAGE_NAME));
        assert!(compact.contains(version::PACKAGE_VERSION));

        let pretty = VERSION_INFO.to_string_pretty();
        assert!(pretty.contains("Version information"));
        assert!(pretty.contains(version::GIT_COMMIT));
    }

    #[tokio::test]
    async fn json_rejection_conversion_labels_known_variants() {
        let request = axum::http::Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"broken",}"#))
            .expect("request should build");
        let rejection = Json::<CreateItem>::from_request(request, &())
            .await
            .expect_err("malformed JSON should reject");
        let rejection = RejectionError::from(rejection);
        let response = rejection.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response_json(response).await["error"], "JsonSyntaxError");
    }
}
