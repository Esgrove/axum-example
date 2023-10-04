//! Run with
//!
//! ```not_rust
//! cargo run
//! ```

use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use axum_macros::debug_handler;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

// basic handler that responds with a static string
#[debug_handler]
/// Root returns a simple json response with the current date and time
async fn root() -> impl IntoResponse {
    let datetime = Utc::now().to_rfc2822();
    tracing::info!("Root: {}", datetime);
    (StatusCode::OK, Json(Response { message: datetime }))
}

// debug handler macro generates better error messages in Rust compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html
#[debug_handler]
/// Example for doing a POST with some data
async fn create_user(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> impl IntoResponse {
    // insert your application logic here
    let user = User {
        id: 1337,
        username: payload.username,
    };

    tracing::info!("Create user: {}", user.username);

    // This will be converted into a JSON response with a status code of `201 Created`.
    (StatusCode::CREATED, Json(user))
}

#[debug_handler]
/// Example for using query parameters
async fn query_user(Query(user): Query<UserQuery>) -> impl IntoResponse {
    tracing::info!("Query user: {}", user.username);

    (
        StatusCode::OK,
        Json(User {
            id: 1234,
            username: user.username,
        }),
    )
}

#[derive(Deserialize)]
struct CreateUser {
    username: String,
}

#[derive(Deserialize)]
pub struct UserQuery {
    pub username: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    username: String,
}

#[derive(Serialize)]
struct Response {
    message: String,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build application with routes
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `GET /user` goes to `query_user`
        .route("/user", get(query_user))
        // `POST /users` goes to `create_user`
        .route("/users", post(create_user));

    // Run app with Hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
