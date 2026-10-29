//! Integration tests for the embedded MCP endpoint.
//!
//! These tests drive the Streamable HTTP transport directly through the assembled router,
//! so the MCP server is verified without starting a TCP listener.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "integration tests fail fast on malformed fixtures and responses"
)]

mod helpers;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};

use helpers::TestApp;

const MCP_ENDPOINT: &str = "/mcp";
const ACCEPT_BOTH: &str = "application/json, text/event-stream";

/// Build a Streamable HTTP MCP POST request.
fn mcp_request(body: &Value, forwarded_for: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(MCP_ENDPOINT)
        .header(header::ACCEPT, ACCEPT_BOTH)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::HOST, "localhost");
    if let Some(forwarded_for) = forwarded_for {
        builder = builder.header("x-forwarded-for", forwarded_for);
    }
    builder.body(Body::from(body.to_string())).expect("MCP request builds")
}

/// Parse an MCP response body, accepting direct JSON or SSE framing.
fn parse_mcp_response(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|_| {
        let text = std::str::from_utf8(body).expect("SSE body is UTF-8");
        let data = text
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .next_back()
            .unwrap_or_else(|| panic!("SSE body did not contain a data line: {text}"));
        serde_json::from_str(data).expect("SSE data is JSON")
    })
}

/// Send one JSON-RPC request and return the parsed response.
async fn rpc(app: &TestApp, id: u64, method: &str, params: Value) -> Value {
    rpc_from(app, None, id, method, params).await
}

/// Send one JSON-RPC request with an optional forwarded client address.
async fn rpc_from(app: &TestApp, forwarded_for: Option<&str>, id: u64, method: &str, params: Value) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    let response = app.send(mcp_request(&body, forwarded_for)).await;
    assert_eq!(
        response.status,
        StatusCode::OK,
        "MCP HTTP call failed: {}",
        response.text()
    );
    parse_mcp_response(&response.body)
}

/// Run the MCP `initialize` handshake.
async fn initialize(app: &TestApp) -> Value {
    rpc(
        app,
        1,
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "axum-example-test", "version": "1.0.0" }
        }),
    )
    .await
}

/// Call one tool and return the JSON-RPC response.
async fn call_tool(app: &TestApp, name: &str, arguments: Value) -> Value {
    rpc(app, 2, "tools/call", json!({ "name": name, "arguments": arguments })).await
}

/// Return the structured content of a tool call response.
fn structured(response: &Value) -> &Value {
    &response["result"]["structuredContent"]
}

/// Return true when a tool call failed, as a JSON-RPC error or a structured tool error.
fn call_failed(response: &Value) -> bool {
    response.get("error").is_some() || response["result"]["isError"] == json!(true)
}

#[tokio::test]
async fn initialize_advertises_tools_and_resources() {
    let app = TestApp::new();
    let response = initialize(&app).await;

    assert_eq!(response["result"]["serverInfo"]["name"], "axum-example");
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert!(response["result"]["capabilities"]["resources"].is_object());
}

#[tokio::test]
async fn tools_list_includes_every_tool() {
    let app = TestApp::new();
    initialize(&app).await;

    let response = rpc(&app, 2, "tools/list", json!({})).await;
    let mut names: Vec<&str> = response["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    names.sort_unstable();

    assert_eq!(
        names,
        [
            "create_item",
            "delete_item",
            "get_item",
            "health",
            "list_items",
            "metrics",
            "status",
            "update_item",
            "version"
        ]
    );
}

#[tokio::test]
async fn service_tools_return_route_bodies() {
    let app = TestApp::new();
    initialize(&app).await;

    let health = call_tool(&app, "health", json!({})).await;
    assert_eq!(structured(&health)["status"], "HEALTHY");

    let status = call_tool(&app, "status", json!({})).await;
    assert_eq!(structured(&status)["service"], "axum-example");

    let version = call_tool(&app, "version", json!({})).await;
    assert_eq!(structured(&version)["name"], "axum-example");

    let metrics = call_tool(&app, "metrics", json!({})).await;
    assert!(!call_failed(&metrics));
    let text = structured(&metrics)["bodyText"].as_str().expect("metrics text");
    assert!(text.contains("axum_example_http_requests_started_total"));
}

#[tokio::test]
async fn item_tools_run_full_crud_cycle() {
    let app = TestApp::new();
    initialize(&app).await;

    let created = call_tool(&app, "create_item", json!({ "name": "mcp-item", "id": 1234 })).await;
    assert_eq!(structured(&created), &json!({ "id": 1234, "name": "mcp-item" }));
    assert!(app.state.db.contains_key("mcp-item"));

    let fetched = call_tool(&app, "get_item", json!({ "name": "mcp-item" })).await;
    assert_eq!(structured(&fetched)["id"], 1234);

    let updated = call_tool(&app, "update_item", json!({ "name": "mcp-item", "id": 4321 })).await;
    assert_eq!(structured(&updated)["id"], 4321);

    let listed = call_tool(&app, "list_items", json!({ "limit": 10 })).await;
    assert_eq!(structured(&listed)["names"], json!(["mcp-item"]));

    let api_key = app.api_key().to_string();
    let deleted = call_tool(&app, "delete_item", json!({ "name": "mcp-item", "apiKey": api_key })).await;
    assert_eq!(structured(&deleted)["name"], "mcp-item");
    assert!(app.state.db.is_empty());
}

#[tokio::test]
async fn route_errors_become_tool_errors() {
    let app = TestApp::new();
    initialize(&app).await;

    let missing = call_tool(&app, "get_item", json!({ "name": "missing" })).await;
    assert!(call_failed(&missing));
    assert_eq!(structured(&missing)["status"], 404);

    let unauthorized = call_tool(&app, "delete_item", json!({ "name": "missing", "apiKey": "wrong" })).await;
    assert!(call_failed(&unauthorized));
    assert_eq!(structured(&unauthorized)["status"], 401);

    let invalid_id = call_tool(&app, "create_item", json!({ "name": "bad", "id": 1 })).await;
    assert!(call_failed(&invalid_id));
    assert_eq!(structured(&invalid_id)["status"], 400);
}

#[tokio::test]
async fn path_traversal_in_item_names_is_rejected() {
    let app = TestApp::new();
    initialize(&app).await;

    for name in ["../health", "a/b", "..", "a?b"] {
        let response = call_tool(&app, "get_item", json!({ "name": name })).await;
        assert!(call_failed(&response), "{name:?} should be rejected");
    }
}

#[tokio::test]
async fn tool_calls_share_the_rate_limiter_with_http_traffic() {
    let app = TestApp::with_config(axum_example::Config {
        rate_limit: axum_example::config::RateLimitConfig {
            enabled: true,
            max_requests: 1,
            window: std::time::Duration::from_mins(1),
        },
        ..axum_example::Config::default()
    });
    let client = Some("203.0.113.50");

    let first = rpc_from(
        &app,
        client,
        2,
        "tools/call",
        json!({ "name": "version", "arguments": {} }),
    )
    .await;
    assert!(!call_failed(&first));

    let second = rpc_from(
        &app,
        client,
        3,
        "tools/call",
        json!({ "name": "version", "arguments": {} }),
    )
    .await;
    assert!(call_failed(&second));
    assert_eq!(structured(&second)["status"], 429);
}

#[tokio::test]
async fn resources_list_and_read() {
    let app = TestApp::new();
    initialize(&app).await;

    let list = rpc(&app, 2, "resources/list", json!({})).await;
    let uris: Vec<&str> = list["result"]["resources"]
        .as_array()
        .expect("resources array")
        .iter()
        .filter_map(|resource| resource["uri"].as_str())
        .collect();
    assert!(uris.contains(&"axum-example://service/metadata"));
    assert!(uris.contains(&"axum-example://openapi.json"));

    let read = rpc(
        &app,
        3,
        "resources/read",
        json!({ "uri": "axum-example://service/metadata" }),
    )
    .await;
    let text = read["result"]["contents"][0]["text"].as_str().expect("resource text");
    let metadata: Value = serde_json::from_str(text).expect("metadata JSON");
    assert_eq!(metadata["mcpEndpoint"], "/mcp");

    let unknown = rpc(&app, 4, "resources/read", json!({ "uri": "axum-example://nope" })).await;
    assert!(unknown.get("error").is_some());
}
