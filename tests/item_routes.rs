//! Integration tests for the item CRUD routes.

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

use axum_example::schemas::ItemListResponse;

use helpers::TestApp;

#[tokio::test]
async fn create_item_with_explicit_id() {
    let app = TestApp::new();
    let response = app
        .send_json(Method::POST, "/items", &json!({"name": "esgrove", "id": 1234}))
        .await;

    assert_eq!(response.status, StatusCode::CREATED);
    assert_eq!(response.json(), json!({"id": 1234, "name": "esgrove"}));
    assert!(app.state.db.contains_key("esgrove"));
}

#[tokio::test]
async fn create_item_generates_an_id_in_range() {
    let app = TestApp::new();
    let response = app.send_json(Method::POST, "/items", &json!({"name": "random"})).await;

    assert_eq!(response.status, StatusCode::CREATED);
    let id = response.json()["id"].as_u64().expect("id");
    assert!((1000..=9999).contains(&id), "id {id} out of range");
}

#[tokio::test]
async fn create_duplicate_item_returns_conflict() {
    let app = TestApp::new();
    let body = json!({"name": "duplicate", "id": 1111});
    assert_eq!(
        app.send_json(Method::POST, "/items", &body).await.status,
        StatusCode::CREATED
    );

    let response = app.send_json(Method::POST, "/items", &body).await;

    assert_eq!(response.status, StatusCode::CONFLICT);
    let body = response.json();
    assert_eq!(body["error"], "Conflict");
    assert_eq!(body["message"], "Item already exists: duplicate");
}

#[tokio::test]
async fn create_item_with_invalid_id_returns_bad_request() {
    let app = TestApp::new();
    let response = app
        .send_json(Method::POST, "/items", &json!({"name": "bad", "id": 1}))
        .await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let body = response.json();
    assert_eq!(body["error"], "Bad Request");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("ID must be between 1000 and 9999")
    );
}

#[tokio::test]
async fn create_item_missing_field_returns_unprocessable_entity() {
    let app = TestApp::new();
    let response = app.send_json(Method::POST, "/items", &json!({"wrong": "test"})).await;

    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.json()["error"], "JsonDataError");
}

#[tokio::test]
async fn create_item_malformed_json_returns_bad_request() {
    let app = TestApp::new();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/items")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name": "test", "id": 1234,}"#))
        .expect("request builds");

    let response = app.send(request).await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.json()["error"], "JsonSyntaxError");
}

#[tokio::test]
async fn create_item_without_content_type_returns_unsupported_media_type() {
    let app = TestApp::new();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/items")
        .body(Body::from(r#"{"name": "test", "id": 1234}"#))
        .expect("request builds");

    let response = app.send(request).await;

    assert_eq!(response.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(response.json()["error"], "MissingJsonContentType");
}

#[tokio::test]
async fn get_item_by_query_and_path() {
    let app = TestApp::new();
    app.send_json(Method::POST, "/items", &json!({"name": "lookup", "id": 5555}))
        .await;

    for uri in ["/item?name=lookup", "/items/lookup"] {
        let response = app.get(uri).await;
        assert_eq!(response.status, StatusCode::OK, "wrong status for {uri}");
        assert_eq!(
            response.json(),
            json!({"id": 5555, "name": "lookup"}),
            "wrong body for {uri}"
        );
    }
}

#[tokio::test]
async fn get_missing_item_returns_json_404() {
    let app = TestApp::new();
    for uri in ["/item?name=missing", "/items/missing"] {
        let response = app.get(uri).await;
        assert_eq!(response.status, StatusCode::NOT_FOUND, "wrong status for {uri}");
        assert_eq!(response.json()["message"], "Item does not exist: missing");
    }
}

#[tokio::test]
async fn update_item_replaces_the_id() {
    let app = TestApp::new();
    app.send_json(Method::POST, "/items", &json!({"name": "mutable", "id": 2000}))
        .await;

    let response = app
        .send_json(Method::PUT, "/items/mutable", &json!({"name": "mutable", "id": 3000}))
        .await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.json()["id"], 3000);
    assert_eq!(app.state.db.get("mutable").expect("item").id, 3000);
}

#[tokio::test]
async fn update_item_rejects_missing_items_and_name_mismatch() {
    let app = TestApp::new();
    let missing = app
        .send_json(Method::PUT, "/items/ghost", &json!({"name": "ghost", "id": 3000}))
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    app.send_json(Method::POST, "/items", &json!({"name": "real", "id": 2000}))
        .await;
    let mismatch = app
        .send_json(Method::PUT, "/items/real", &json!({"name": "other", "id": 3000}))
        .await;
    assert_eq!(mismatch.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        app.state.db.get("real").expect("item").id,
        2000,
        "item must be unchanged"
    );
}

#[tokio::test]
async fn list_items_returns_sorted_names_with_pagination() {
    let app = TestApp::new();
    for (name, id) in [("charlie", 1003), ("alpha", 1001), ("bravo", 1002)] {
        let response = app
            .send_json(Method::POST, "/items", &json!({"name": name, "id": id}))
            .await;
        assert_eq!(response.status, StatusCode::CREATED);
    }

    for (uri, expected_names) in [
        ("/items", vec!["alpha", "bravo", "charlie"]),
        ("/items?skip=1", vec!["bravo", "charlie"]),
        ("/items?limit=2", vec!["alpha", "bravo"]),
        ("/items?skip=1&limit=1", vec!["bravo"]),
        ("/items?skip=10", vec![]),
    ] {
        let response = app.get(uri).await;
        assert_eq!(response.status, StatusCode::OK, "wrong status for {uri}");

        let list: ItemListResponse = serde_json::from_slice(&response.body).expect("list parses");
        assert_eq!(list.num_items, 3, "total count should not be paginated for {uri}");
        assert_eq!(list.names, expected_names, "wrong names for {uri}");
    }
}
