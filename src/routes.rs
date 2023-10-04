use axum::{extract::Query, http::StatusCode, response::IntoResponse, Json};
use axum_macros::debug_handler;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateUser {
    username: String,
}

#[derive(Deserialize)]
pub struct UserQuery {
    pub username: String,
}

#[derive(Serialize)]
pub struct User {
    id: u64,
    username: String,
}

#[derive(Serialize)]
pub struct Response {
    message: String,
}

// basic handler that responds with a static string
#[debug_handler]
/// Root returns a simple json response with the current date and time
pub async fn root() -> impl IntoResponse {
    let datetime = Utc::now().to_rfc2822();
    tracing::info!("Root: {}", datetime);
    (StatusCode::OK, Json(Response { message: datetime }))
}

// debug handler macro generates better error messages in Rust compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html
#[debug_handler]
/// Example for doing a POST with some data
pub async fn create_user(
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
pub async fn query_user(Query(user): Query<UserQuery>) -> impl IntoResponse {
    tracing::info!("Query user: {}", user.username);

    (
        StatusCode::OK,
        Json(User {
            id: 1234,
            username: user.username,
        }),
    )
}
