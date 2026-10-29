//! Typed errors.
//!
//! [`ConfigError`] covers startup configuration failures.
//! [`ApiError`] is the single error type returned by route handlers,
//! and it renders every failure as the same JSON [`ErrorResponse`] shape.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

use crate::schemas::ErrorResponse;

/// Startup configuration error.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// A variable is set but could not be parsed.
    #[error("invalid value `{value}` for {name}: {message}")]
    Invalid {
        /// Environment variable name.
        name: &'static str,
        /// Raw value that failed to parse.
        value: String,
        /// Parser error message.
        message: String,
    },
    /// A variable parsed but is outside the accepted range.
    #[error("{name} {message}")]
    OutOfRange {
        /// Environment variable name.
        name: &'static str,
        /// Human-readable constraint description.
        message: String,
    },
}

/// Domain validation error for items.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ItemError {
    /// Client-supplied item id is outside the accepted range.
    #[error("ID must be between {min} and {max}, got {id}")]
    InvalidId {
        /// Rejected id.
        id: u64,
        /// Smallest accepted id.
        min: u64,
        /// Largest accepted id.
        max: u64,
    },
}

/// Error returned by route handlers.
#[derive(Debug, Error)]
pub enum ApiError {
    /// Request was malformed or failed validation.
    #[error("{0}")]
    BadRequest(String),
    /// Authentication failed.
    #[error("{0}")]
    Unauthorized(String),
    /// Requested resource does not exist.
    #[error("{0}")]
    NotFound(String),
    /// Resource already exists.
    #[error("{0}")]
    Conflict(String),
    /// Client exceeded the rate limit.
    #[error("{0}")]
    TooManyRequests(String),
    /// JSON body extraction failed.
    #[error("{message}")]
    JsonRejection {
        /// Status chosen by Axum for this rejection.
        status: StatusCode,
        /// Rejection kind, for example `JsonSyntaxError`.
        kind: &'static str,
        /// Rejection details.
        message: String,
    },
    /// Unexpected server-side failure.
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    /// HTTP status code for this error.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::JsonRejection { status, .. } => *status,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Short error label used in the `error` field of the response body.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::JsonRejection { kind, .. } => kind,
            _ => self.status().canonical_reason().unwrap_or("Error"),
        }
    }
}

impl From<ItemError> for ApiError {
    fn from(error: ItemError) -> Self {
        Self::BadRequest(error.to_string())
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        let kind = match &rejection {
            JsonRejection::JsonDataError(_) => "JsonDataError",
            JsonRejection::JsonSyntaxError(_) => "JsonSyntaxError",
            JsonRejection::MissingJsonContentType(_) => "MissingJsonContentType",
            JsonRejection::BytesRejection(_) => "BytesRejection",
            _ => "UnknownRejection",
        };
        Self::JsonRejection {
            status: rejection.status(),
            kind,
            message: rejection.body_text(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorResponse::new(self.label(), self.to_string());
        (status, Json(body)).into_response()
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
        let bytes = response.into_body().collect().await.expect("body").to_bytes();
        serde_json::from_slice(&bytes).expect("body should be json")
    }

    #[tokio::test]
    async fn api_errors_map_to_status_and_json_body() {
        for (error, status, label) in [
            (
                ApiError::BadRequest("bad".into()),
                StatusCode::BAD_REQUEST,
                "Bad Request",
            ),
            (
                ApiError::Unauthorized("no".into()),
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
            ),
            (ApiError::NotFound("gone".into()), StatusCode::NOT_FOUND, "Not Found"),
            (ApiError::Conflict("dup".into()), StatusCode::CONFLICT, "Conflict"),
            (
                ApiError::TooManyRequests("slow".into()),
                StatusCode::TOO_MANY_REQUESTS,
                "Too Many Requests",
            ),
            (
                ApiError::Internal("boom".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
            ),
        ] {
            let message = error.to_string();
            let response = error.into_response();
            assert_eq!(response.status(), status);
            let body = response_json(response).await;
            assert_eq!(body["error"], label);
            assert_eq!(body["message"], message);
        }
    }

    #[test]
    fn item_error_becomes_bad_request() {
        let error = ApiError::from(ItemError::InvalidId {
            id: 1,
            min: 1000,
            max: 9999,
        });
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        assert!(error.to_string().contains("between 1000 and 9999"));
    }

    #[tokio::test]
    async fn json_rejection_keeps_status_and_kind() {
        let request = axum::http::Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"broken",}"#))
            .expect("request should build");
        let rejection = Json::<Value>::from_request(request, &())
            .await
            .expect_err("malformed JSON should reject");
        let response = ApiError::from(rejection).into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response_json(response).await["error"], "JsonSyntaxError");
    }
}
