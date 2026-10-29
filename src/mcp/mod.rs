//! Embedded Model Context Protocol server.
//!
//! The MCP surface is a thin adapter over the existing Axum router.
//! Every tool call builds an internal HTTP request and sends it through [`crate::router::build_router`],
//! so validation, rate limits, metrics, and error responses stay identical to ordinary HTTP traffic.

mod helpers;
mod schemas;

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ErrorData, Implementation, ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{RoleServer, ServerHandler, tool, tool_handler, tool_router};
use serde_json::json;
use tower::ServiceExt;
use utoipa::OpenApi;

use crate::mcp::helpers::{
    build_internal_request, normalise_path_component, response_to_tool_response, service_metadata, tool_result,
    trusted_forwarded_for,
};
use crate::mcp::schemas::{DeleteItemParams, HttpMethod, ItemNameParams, ItemParams, ListItemsParams, ToolResponse};
use crate::openapi::ApiDoc;
use crate::state::SharedState;
use crate::types::API_KEY_HEADER;
use crate::version;

/// Path where the MCP service is mounted.
pub const MCP_PATH: &str = "/mcp";

const RESOURCE_SERVICE_METADATA: &str = "axum-example://service/metadata";
const RESOURCE_OPENAPI: &str = "axum-example://openapi.json";

/// MCP server implementation for the example API.
///
/// Holds a pre-built clone of the app router so every tool call dispatches through the same router,
/// reusing its rate limiter and metrics instead of rebuilding them per request.
#[derive(Debug, Clone)]
struct ExampleMcpServer {
    dispatch_router: axum::Router,
    tool_router: ToolRouter<Self>,
}

impl ExampleMcpServer {
    /// Create a new MCP server that dispatches tool calls through the supplied router.
    const fn new(dispatch_router: axum::Router, tool_router: ToolRouter<Self>) -> Self {
        Self {
            dispatch_router,
            tool_router,
        }
    }

    /// Build and send one internal request, then map the response into a tool result.
    async fn dispatch(
        &self,
        method: HttpMethod,
        uri: &str,
        headers: BTreeMap<String, String>,
        body_json: Option<serde_json::Value>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        tool_result(
            self.send_internal_request(method, uri, headers, body_json, context)
                .await,
        )
    }

    /// Send one internal Axum request through the app router and collect its response.
    async fn send_internal_request(
        &self,
        method: HttpMethod,
        uri: &str,
        headers: BTreeMap<String, String>,
        body_json: Option<serde_json::Value>,
        context: &RequestContext<RoleServer>,
    ) -> Result<ToolResponse, ErrorData> {
        let request = build_internal_request(method, uri, headers, trusted_forwarded_for(context), body_json)?;
        let response = self
            .dispatch_router
            .clone()
            .oneshot(request)
            .await
            .map_err(|err| ErrorData::internal_error(format!("router failed: {err}"), None))?;
        response_to_tool_response(response).await
    }

    /// Call a read-only service endpoint.
    async fn get(&self, uri: &str, context: &RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        self.dispatch(HttpMethod::Get, uri, BTreeMap::new(), None, context)
            .await
    }
}

#[tool_router(router = tool_router)]
impl ExampleMcpServer {
    /// Return the health body.
    #[tool(description = "Return the same JSON health body as GET /health.")]
    async fn health(&self, context: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        self.get("/health", &context).await
    }

    /// Return the detailed status body.
    #[tool(description = "Return the same JSON status body as GET /status.")]
    async fn status(&self, context: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        self.get("/status", &context).await
    }

    /// Return version and build metadata.
    #[tool(description = "Return the same JSON version body as GET /version.")]
    async fn version(&self, context: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        self.get("/version", &context).await
    }

    /// Return Prometheus metrics text.
    #[tool(description = "Return the Prometheus metrics text from GET /metrics.")]
    async fn metrics(&self, context: RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        self.get("/metrics", &context).await
    }

    /// List item names with optional pagination.
    #[tool(description = "List sorted item names like GET /items, with optional skip and limit.")]
    async fn list_items(
        &self,
        Parameters(params): Parameters<ListItemsParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut query = Vec::new();
        if let Some(skip) = params.skip {
            query.push(format!("skip={skip}"));
        }
        if let Some(limit) = params.limit {
            query.push(format!("limit={limit}"));
        }
        let uri = if query.is_empty() {
            "/items".to_string()
        } else {
            format!("/items?{}", query.join("&"))
        };
        self.get(&uri, &context).await
    }

    /// Get one item by name.
    #[tool(description = "Get one item by name like GET /items/{name}.")]
    async fn get_item(
        &self,
        Parameters(params): Parameters<ItemNameParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let name = match normalise_path_component(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return tool_result(Err(err)),
        };
        self.get(&format!("/items/{name}"), &context).await
    }

    /// Create a new item.
    #[tool(description = "Create an item like POST /items. The id is random when omitted.")]
    async fn create_item(
        &self,
        Parameters(params): Parameters<ItemParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({ "name": params.name, "id": params.id });
        self.dispatch(HttpMethod::Post, "/items", BTreeMap::new(), Some(body), &context)
            .await
    }

    /// Replace the id of an existing item.
    #[tool(description = "Update an existing item like PUT /items/{name}. The id is random when omitted.")]
    async fn update_item(
        &self,
        Parameters(params): Parameters<ItemParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let name = match normalise_path_component(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return tool_result(Err(err)),
        };
        let body = json!({ "name": name, "id": params.id });
        self.dispatch(
            HttpMethod::Put,
            &format!("/items/{name}"),
            BTreeMap::new(),
            Some(body),
            &context,
        )
        .await
    }

    /// Delete an item through the admin route.
    #[tool(description = "Delete an item like DELETE /admin/remove/{name}. Requires the admin API key.")]
    async fn delete_item(
        &self,
        Parameters(params): Parameters<DeleteItemParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let name = match normalise_path_component(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return tool_result(Err(err)),
        };
        let headers = BTreeMap::from([(API_KEY_HEADER.to_string(), params.api_key)]);
        self.dispatch(
            HttpMethod::Delete,
            &format!("/admin/remove/{name}"),
            headers,
            None,
            &context,
        )
        .await
    }
}

#[tool_handler(router = self.tool_router)]
#[expect(
    clippy::unused_async_trait_impl,
    reason = "these handlers serve in-memory data, and the tool_handler macro also expands to an async trait method"
)]
impl ServerHandler for ExampleMcpServer {
    /// Return MCP server metadata and advertised capabilities.
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_server_info(Implementation::new(version::PACKAGE_NAME, version::PACKAGE_VERSION))
            .with_instructions("Use the tools to inspect the service and manage items in the in-memory store.")
    }

    /// List static resources exposed to MCP clients.
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(RESOURCE_SERVICE_METADATA, "service-metadata")
                .with_description("Build and runtime metadata for the MCP server.")
                .with_mime_type("application/json"),
            Resource::new(RESOURCE_OPENAPI, "openapi")
                .with_description("OpenAPI document served by the API.")
                .with_mime_type("application/json"),
        ]))
    }

    /// Read a static MCP resource.
    ///
    /// Every resource is served synchronously from memory,
    /// so the response is always a completed [`ReadResourceResponse`].
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let value = match request.uri.as_str() {
            RESOURCE_SERVICE_METADATA => service_metadata(),
            RESOURCE_OPENAPI => serde_json::to_value(ApiDoc::openapi()).map_err(|err| {
                ErrorData::internal_error(format!("failed to serialize OpenAPI document: {err}"), None)
            })?,
            _ => {
                return Err(ErrorData::resource_not_found(
                    format!("unknown MCP resource `{}`", request.uri),
                    None,
                ));
            }
        };
        let text = serde_json::to_string_pretty(&value)
            .map_err(|err| ErrorData::internal_error(format!("failed to serialize MCP resource: {err}"), None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri).with_mime_type("application/json"),
        ])
        .into())
    }
}

/// Build the `/mcp` route tree around the shared state.
pub fn router(shared_state: &SharedState) -> axum::Router {
    // Assemble the internal-dispatch router once and clone it into each per-request server instance.
    // `build_router` (rather than `build_router_with_mcp`) omits the `/mcp` routes,
    // so this neither recurses into the MCP service nor forms an `Arc` cycle back into `AppState`,
    // while still sharing the rate limiter and metrics with public HTTP traffic.
    let dispatch_router = crate::router::build_router(shared_state);
    // Access control for a real deployment belongs in front of the service.
    // Disable rmcp's local DNS-rebinding host allowlist so deployed hostnames work like every other route.
    let config = StreamableHttpServerConfig::default()
        .disable_allowed_hosts()
        .with_legacy_session_mode(false)
        .with_json_response(true);
    let tool_router = ExampleMcpServer::tool_router();
    let service = StreamableHttpService::new(
        move || Ok(ExampleMcpServer::new(dispatch_router.clone(), tool_router.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    );

    axum::Router::new().nest_service(MCP_PATH, service)
}
