//! MCP tool parameter and response schemas.
//!
//! These types are the JSON-facing surface for MCP tools.

use std::collections::BTreeMap;

use axum::http::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Parameters for listing items.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListItemsParams {
    /// Number of sorted names to skip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) skip: Option<usize>,
    /// Maximum number of names to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) limit: Option<usize>,
}

/// Parameters that identify one item by name.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct ItemNameParams {
    /// Item name.
    pub(super) name: String,
}

/// Parameters for creating or updating an item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct ItemParams {
    /// Item name.
    pub(super) name: String,
    /// Optional item id between 1000 and 9999.
    ///
    /// A random id is generated when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<u64>,
}

/// Parameters for deleting an item through the admin route.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteItemParams {
    /// Item name.
    pub(super) name: String,
    /// Admin API key, sent as the `api-key` header.
    pub(super) api_key: String,
}

/// Normalized response returned by MCP tools that dispatch through the router.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolResponse {
    /// HTTP status code returned by the route.
    pub(super) status: u16,
    /// Whether the status is in the 2xx range.
    pub(super) success: bool,
    /// Response headers that could be represented as UTF-8 strings.
    pub(super) headers: BTreeMap<String, String>,
    /// Body text, with invalid UTF-8 replaced, when the body is not JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) body_text: Option<String>,
    /// Parsed JSON body, when the response body is valid JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) body_json: Option<Value>,
}

/// Supported HTTP methods for MCP-triggered internal requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub(super) enum HttpMethod {
    /// HTTP GET.
    Get,
    /// HTTP POST.
    Post,
    /// HTTP PUT.
    Put,
    /// HTTP DELETE.
    Delete,
}

impl From<HttpMethod> for Method {
    /// Convert an MCP HTTP method into Axum's HTTP method type.
    fn from(method: HttpMethod) -> Self {
        match method {
            HttpMethod::Get => Self::GET,
            HttpMethod::Post => Self::POST,
            HttpMethod::Put => Self::PUT,
            HttpMethod::Delete => Self::DELETE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assert every supported HTTP method maps to its Axum counterpart.
    #[test]
    fn http_method_maps_to_axum_method() {
        assert_eq!(Method::from(HttpMethod::Get), Method::GET);
        assert_eq!(Method::from(HttpMethod::Post), Method::POST);
        assert_eq!(Method::from(HttpMethod::Put), Method::PUT);
        assert_eq!(Method::from(HttpMethod::Delete), Method::DELETE);
    }

    /// Assert item parameters use camelCase and an optional id.
    #[test]
    fn item_params_deserialize_without_id() {
        let params: ItemParams = serde_json::from_str(r#"{"name":"esgrove"}"#).expect("params parse");
        assert_eq!(params.name, "esgrove");
        assert!(params.id.is_none());
    }

    /// Assert delete parameters read the camelCase API key field.
    #[test]
    fn delete_params_use_camel_case_api_key() {
        let params: DeleteItemParams =
            serde_json::from_str(r#"{"name":"esgrove","apiKey":"secret"}"#).expect("params parse");
        assert_eq!(params.api_key, "secret");
    }
}
