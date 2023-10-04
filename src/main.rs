//! Run with
//!
//! ```not_rust
//! cargo run
//! ```

mod routes;

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build application with routes
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(routes::root))
        // `GET /user` goes to `query_user`
        .route("/user", get(routes::query_user))
        // `POST /users` goes to `create_user`
        .route("/users", post(routes::create_user));

    // Run app with Hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
