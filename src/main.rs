//! Run with
//!
//! ```not_rust
//! cargo run --release
//! ```

mod admin;
mod routes;
mod types;
mod utils;

use crate::types::{AppState, LogLevel, SharedState};

use anyhow::Result;
use axum::routing::{get, post};
use axum::Router;
use clap::{arg, Parser};
use shadow_rs::shadow;
use tokio::sync::RwLock;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

use std::sync::Arc;
use std::time::Duration;

// Get build information
shadow!(build);

/// Command line arguments
///
/// Basic info is read from `Cargo.toml`
/// See Clap `Derive` documentation for details:
/// <https://docs.rs/clap/latest/clap/_derive/index.html>
#[derive(Parser)]
#[command(
    author,
    about = "Rust Axum REST API example.",
    long_about = "Rust Axum REST API example.",
    arg_required_else_help = false,
    disable_version_flag = true
)]
struct Args {
    /// Optional host IP to listen to (for example "0.0.0.0")
    #[arg(long, value_name = "HOST")]
    host: Option<String>,

    /// Log level to use
    #[arg(value_enum, short, long, value_name = "LEVEL")]
    log: Option<LogLevel>,

    /// Optional port number to use (default is 3000)
    #[arg(short, long, value_name = "PORT")]
    port: Option<u16>,

    /// Custom version flag instead of clap default
    #[arg(short, long, help = "Print version info and exit")]
    version: bool,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::root,
        routes::version,
        routes::query_item,
        routes::list_items,
        routes::create_item,
        admin::delete_all_items,
        admin::remove_item,
    ),
    components(schemas(
        types::CreateItem,
        types::MessageResponse,
        types::Item,
        types::ItemListResponse,
        types::ItemQuery,
        types::VersionInfo,
    ))
)]
pub struct ApiDoc;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    if args.version {
        println!("{}", utils::formatted_version_info());
        return Ok(());
    }

    let host = args.host.unwrap_or_else(|| "127.0.0.1".to_string());
    let port_number = args.port.unwrap_or(3000);

    let mut filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Some(ref level) = args.log {
        filter_layer = filter_layer.add_directive(level.to_filter().into());
    }

    tracing_subscriber::fmt().with_env_filter(filter_layer).init();
    tracing::info!("{}", build::VERSION);

    let listener = tokio::net::TcpListener::bind(format!("{host}:{port_number}")).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    // Build application with routes
    let shared_state = SharedState::default();
    let app = build_router(&shared_state);

    // Run app with Hyper
    axum::serve(listener, app)
        .with_graceful_shutdown(utils::shutdown_signal())
        .await?;

    Ok(())
}

/// Create Router app with routes
fn build_router(shared_state: &Arc<RwLock<AppState>>) -> Router {
    Router::new()
        .merge(SwaggerUi::new("/doc").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
        .merge(RapiDoc::new("/api-docs/openapi.json").path("/rapidoc"))
        .route("/", get(routes::root))
        .route("/version", get(routes::version))
        .route("/item", get(routes::query_item))
        .route("/list_items", get(routes::list_items))
        .route("/items", post(routes::create_item))
        // Put all admin routes under /admin
        .nest("/admin", admin::admin_routes())
        .layer((
            TraceLayer::new_for_http(),
            // Graceful shutdown will wait for outstanding requests to complete.
            // Add a timeout so requests don't hang forever.
            TimeoutLayer::new(Duration::from_secs(10)),
        ))
        .with_state(Arc::clone(shared_state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_root() {
        let shared_state = SharedState::default();
        let app = build_router(&shared_state);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert!(body.get("message").is_some(), "Body does not contain 'message' key");
        assert!(body["message"].is_string(), "'message' is not a string");
    }

    // TODO: more tests
}
