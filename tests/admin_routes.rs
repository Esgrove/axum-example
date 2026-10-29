//! Integration tests for the API-key protected admin routes.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "integration tests fail fast on malformed fixtures and responses"
)]

mod helpers;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::json;

use helpers::TestApp;

#[tokio::test]
async fn admin_routes_reject_missing_api_key() {
    let app = TestApp::new();
    for uri in ["/admin/clear_items", "/admin/remove/anything"] {
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .body(Body::empty())
            .expect("request builds");
        let response = app.send(request).await;

        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "wrong status for {uri}");
        assert_eq!(response.json()["message"], "Missing api-key header");
    }
}

#[tokio::test]
async fn admin_routes_reject_invalid_api_key() {
    let app = TestApp::new();
    let request = Request::builder()
        .method(Method::DELETE)
        .uri("/admin/clear_items")
        .header("api-key", "wrong-api-key")
        .body(Body::empty())
        .expect("request builds");

    let response = app.send(request).await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.json()["error"], "Unauthorized");
}

#[tokio::test]
async fn clear_items_removes_every_item() {
    let app = TestApp::new();
    for name in ["one", "two"] {
        app.send_json(Method::POST, "/items", &json!({ "name": name })).await;
    }

    let response = app.admin_delete("/admin/clear_items").await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.json()["message"], "Removed 2 items");
    assert!(app.state.db.is_empty());
}

#[tokio::test]
async fn remove_item_handles_found_and_missing_items() {
    let app = TestApp::new();
    app.send_json(Method::POST, "/items", &json!({"name": "removable", "id": 4444}))
        .await;

    let removed = app.admin_delete("/admin/remove/removable").await;
    assert_eq!(removed.status, StatusCode::OK);
    assert_eq!(removed.json(), json!({"id": 4444, "name": "removable"}));

    let missing = app.admin_delete("/admin/remove/removable").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(missing.json()["message"], "Item does not exist: removable");
}
