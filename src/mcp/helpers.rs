//! Helper functions for the embedded MCP server.
//!
//! This module owns path validation, internal Axum request construction,
//! response normalization, and static resource payload assembly.

use std::collections::BTreeMap;

use axum::body::{Body, to_bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, HeaderName, HeaderValue};
use axum::http::{HeaderMap, Method, Request, request::Parts};
use rmcp::RoleServer;
use rmcp::model::{CallToolResult, ErrorData};
use rmcp::service::RequestContext;
use serde::Serialize;
use serde_json::{Value, json};

use crate::mcp::schemas::{HttpMethod, ToolResponse};
use crate::version;

/// Largest response body collected from an internal dispatch.
const MCP_RESPONSE_BODY_LIMIT: usize = 1024 * 1024;

/// Header the rate limiter derives its per-client bucket key from.
///
/// Stripped from MCP-supplied headers
/// because the internal dispatch request never passes through a trusted load balancer
/// that would append the real client address.
/// An unfiltered value would let a tool caller spoof or rotate rate-limit buckets.
const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// Validate a path component that must not introduce new path segments.
pub(super) fn normalise_path_component(value: &str, field: &str) -> Result<String, ErrorData> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.contains(['/', '\\', '?', '#', '%'])
        || contains_control_character(value)
    {
        return Err(ErrorData::invalid_params(
            format!("{field} must be a non-empty single path component"),
            None,
        ));
    }
    Ok(value.to_string())
}

/// Build an internal Axum request for the assembled app router.
///
/// Caller-supplied credential and forwarding headers are filtered,
/// then `trusted_forwarded_for` is restored from the actual `/mcp` HTTP request context.
pub(super) fn build_internal_request(
    method: HttpMethod,
    uri: &str,
    headers: BTreeMap<String, String>,
    trusted_forwarded_for: Option<HeaderValue>,
    body_json: Option<Value>,
) -> Result<Request<Body>, ErrorData> {
    let (body, content_type) = request_body(body_json)?;
    let mut builder = Request::builder().method(Method::from(method)).uri(uri);
    let header_map = builder
        .headers_mut()
        .ok_or_else(|| ErrorData::internal_error("failed to build request headers", None))?;
    insert_request_headers(header_map, headers)?;
    if let Some(trusted_forwarded_for) = trusted_forwarded_for {
        header_map.insert(X_FORWARDED_FOR, trusted_forwarded_for);
    }
    if let Some(content_type) = content_type
        && !header_map.contains_key(CONTENT_TYPE)
    {
        header_map.insert(CONTENT_TYPE, content_type);
    }
    builder
        .body(body)
        .map_err(|err| ErrorData::invalid_params(format!("invalid internal request: {err}"), None))
}

/// Convert an Axum response into a structured MCP-friendly response object.
pub(super) async fn response_to_tool_response(response: axum::response::Response) -> Result<ToolResponse, ErrorData> {
    let status = response.status();
    let headers = response_headers(response.headers());
    let bytes = to_bytes(response.into_body(), MCP_RESPONSE_BODY_LIMIT)
        .await
        .map_err(|err| ErrorData::internal_error(format!("failed to collect response body: {err}"), None))?;
    let body_json = serde_json::from_slice::<Value>(&bytes).ok();
    let body_text = if body_json.is_none() && !bytes.is_empty() {
        Some(String::from_utf8_lossy(&bytes).into_owned())
    } else {
        None
    };
    Ok(ToolResponse {
        status: status.as_u16(),
        success: status.is_success(),
        headers,
        body_text,
        body_json,
    })
}

/// Return a successful structured MCP result.
pub(super) fn structured(value: impl Serialize) -> Result<CallToolResult, ErrorData> {
    serde_json::to_value(value)
        .map(CallToolResult::structured)
        .map_err(|err| ErrorData::internal_error(format!("failed to serialize tool response: {err}"), None))
}

/// Return a caller-visible structured MCP tool error.
pub(super) fn structured_error(value: impl Serialize) -> Result<CallToolResult, ErrorData> {
    serde_json::to_value(value)
        .map(CallToolResult::structured_error)
        .map_err(|err| ErrorData::internal_error(format!("failed to serialize tool error response: {err}"), None))
}

/// Convert a dispatch result into a caller-visible MCP tool result.
///
/// Successful JSON bodies are returned directly as structured content.
/// Non-2xx responses and validation failures become tool errors,
/// so the MCP client sees them instead of a protocol-level failure.
pub(super) fn tool_result(result: Result<ToolResponse, ErrorData>) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(response) if response.success => match response.body_json {
            Some(value) => Ok(CallToolResult::structured(value)),
            None => structured(response),
        },
        Ok(response) => structured_error(response),
        Err(err) => structured_error(err),
    }
}

/// Return service metadata exposed as an MCP resource.
pub(super) fn service_metadata() -> Value {
    json!({
        "name": version::PACKAGE_NAME,
        "version": version::PACKAGE_VERSION,
        "deployTag": version::DEPLOY_TAG,
        "buildTime": version::BUILD_TIME,
        "branch": version::GIT_BRANCH,
        "commit": version::GIT_COMMIT,
        "rustVersion": version::RUST_VERSION,
        "mcpEndpoint": "/mcp"
    })
}

/// Return the trusted `X-Forwarded-For` value from the HTTP request that reached `/mcp`.
///
/// rmcp stores the original HTTP request parts in the request context.
/// Copying from that context keeps the real client key for internal dispatch,
/// while tool arguments still cannot spoof the rate-limit bucket.
pub(super) fn trusted_forwarded_for(context: &RequestContext<RoleServer>) -> Option<HeaderValue> {
    context
        .extensions
        .get::<Parts>()
        .and_then(|parts| parts.headers.get(X_FORWARDED_FOR))
        .cloned()
}

/// Return true when the value contains a control character.
fn contains_control_character(value: &str) -> bool {
    value.chars().any(char::is_control)
}

/// Convert an optional JSON body into an Axum body and optional content type.
fn request_body(body_json: Option<Value>) -> Result<(Body, Option<HeaderValue>), ErrorData> {
    match body_json {
        Some(value) => {
            let bytes = serde_json::to_vec(&value)
                .map_err(|err| ErrorData::invalid_params(format!("body could not be encoded: {err}"), None))?;
            Ok((Body::from(bytes), Some(HeaderValue::from_static("application/json"))))
        }
        None => Ok((Body::empty(), None)),
    }
}

/// Insert safe request headers into the internal request.
///
/// `Authorization` and `Cookie` are dropped so tool callers cannot smuggle credentials.
/// `X-Forwarded-For` is dropped so a caller cannot influence the rate-limit bucket key.
fn insert_request_headers(target: &mut HeaderMap, headers: BTreeMap<String, String>) -> Result<(), ErrorData> {
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|err| ErrorData::invalid_params(format!("invalid request header name: {err}"), None))?;
        if name == AUTHORIZATION || name == COOKIE || name == X_FORWARDED_FOR {
            continue;
        }
        let value = HeaderValue::from_str(&value).map_err(|err| {
            ErrorData::invalid_params(format!("invalid request header value for `{name}`: {err}"), None)
        })?;
        target.insert(name, value);
    }
    Ok(())
}

/// Convert response headers into a string map for MCP structured content.
fn response_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::http::StatusCode;

    /// Build a minimal [`ToolResponse`] for tool-result mapping tests.
    fn sample_response(status: u16, success: bool) -> ToolResponse {
        ToolResponse {
            status,
            success,
            headers: BTreeMap::new(),
            body_text: None,
            body_json: None,
        }
    }

    /// Assert single-component validation accepts a bare component and rejects separators.
    #[test]
    fn normalise_path_component_accepts_component_and_rejects_separators() {
        assert_eq!(normalise_path_component("item-1", "name").expect("valid"), "item-1");
        for value in ["", ".", "..", "a/b", "a\\b", "a?b", "a#b", "a%2Fb", "a\nb"] {
            assert!(
                normalise_path_component(value, "name").is_err(),
                "{value:?} should be rejected"
            );
        }
    }

    /// Assert credential and forwarding headers supplied by MCP callers are stripped.
    #[test]
    fn build_internal_request_strips_credential_and_forwarding_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("x-forwarded-for".to_string(), "1.2.3.4".to_string());
        headers.insert("authorization".to_string(), "Bearer secret".to_string());
        headers.insert("cookie".to_string(), "session=abc".to_string());
        headers.insert("x-custom".to_string(), "keep".to_string());

        let request = build_internal_request(HttpMethod::Get, "/health", headers, None, None).expect("request");
        let request_headers = request.headers();

        assert!(request_headers.get("x-forwarded-for").is_none());
        assert!(request_headers.get(AUTHORIZATION).is_none());
        assert!(request_headers.get(COOKIE).is_none());
        assert_eq!(request_headers.get("x-custom").expect("custom header kept"), "keep");
    }

    /// Assert the trusted forwarding header replaces any caller-supplied value.
    #[test]
    fn build_internal_request_uses_trusted_forwarding_header() {
        let mut headers = BTreeMap::new();
        headers.insert("x-forwarded-for".to_string(), "1.2.3.4".to_string());

        let request = build_internal_request(
            HttpMethod::Get,
            "/health",
            headers,
            Some(HeaderValue::from_static("203.0.113.25")),
            None,
        )
        .expect("request");

        assert_eq!(
            request.headers().get("x-forwarded-for").expect("trusted header"),
            "203.0.113.25"
        );
    }

    /// Assert invalid header values are rejected as caller input errors.
    #[test]
    fn build_internal_request_rejects_invalid_header_values() {
        let mut headers = BTreeMap::new();
        headers.insert("x-custom".to_string(), "line\nbreak".to_string());

        let error = build_internal_request(HttpMethod::Get, "/health", headers, None, None)
            .expect_err("invalid header value should fail");

        assert!(error.message.contains("invalid request header value"));
    }

    /// Assert a JSON body sets the JSON content type.
    #[test]
    fn build_internal_request_sets_json_content_type() {
        let request = build_internal_request(
            HttpMethod::Post,
            "/items",
            BTreeMap::new(),
            None,
            Some(json!({"name": "esgrove"})),
        )
        .expect("request");

        assert_eq!(
            request.headers().get(CONTENT_TYPE).expect("content type"),
            "application/json"
        );
    }

    /// Assert service metadata exposes the identity fields MCP clients read.
    #[test]
    fn service_metadata_exposes_identity_fields() {
        let metadata = service_metadata();
        assert_eq!(metadata["name"], version::PACKAGE_NAME);
        assert_eq!(metadata["version"], version::PACKAGE_VERSION);
        assert_eq!(metadata["mcpEndpoint"], "/mcp");
    }

    /// Assert the tool-result mapper marks success as structured output and failures as errors.
    #[test]
    fn tool_result_marks_success_and_failure() {
        let ok = serde_json::to_value(tool_result(Ok(sample_response(200, true))).expect("ok")).expect("serialize");
        assert_ne!(ok["isError"], Value::Bool(true));
        assert!(ok["structuredContent"].is_object());

        let failure =
            serde_json::to_value(tool_result(Ok(sample_response(404, false))).expect("failure")).expect("serialize");
        assert_eq!(failure["isError"], true);

        let error = serde_json::to_value(tool_result(Err(ErrorData::invalid_params("bad", None))).expect("error"))
            .expect("serialize");
        assert_eq!(error["isError"], true);
    }

    /// Assert a JSON response is parsed into structured fields.
    #[tokio::test]
    async fn response_to_tool_response_parses_json_body() {
        let response = axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"ok":true}"#))
            .expect("response builds");

        let tool = response_to_tool_response(response).await.expect("tool response");

        assert_eq!(tool.status, 200);
        assert!(tool.success);
        assert_eq!(tool.body_json.expect("json body")["ok"], true);
        assert!(tool.body_text.is_none());
    }

    /// Assert a plain-text response is surfaced as text.
    #[tokio::test]
    async fn response_to_tool_response_keeps_text_body() {
        let response = axum::response::Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::from("service unavailable"))
            .expect("response builds");

        let tool = response_to_tool_response(response).await.expect("tool response");

        assert_eq!(tool.status, 502);
        assert!(!tool.success);
        assert!(tool.body_json.is_none());
        assert_eq!(tool.body_text.as_deref(), Some("service unavailable"));
    }
}
