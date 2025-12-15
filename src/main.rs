//! Main.
//!
//! Handle CLI arguments and run API.
//!

mod file_config;
mod schemas;
mod types;
mod utils;
mod version;
mod routing {
    pub mod admin;
    pub mod routes;
}

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::Router;
use axum::http::StatusCode;
use axum::routing::{get, post};
use clap::Parser;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::EnvFilter;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_scalar::{Scalar, Servable as ScalarServable};
use utoipa_swagger_ui::SwaggerUi;

use crate::file_config::FileConfig;
use crate::routing::admin;
use crate::routing::routes;
use crate::schemas::VERSION_INFO;
use crate::types::{AppState, Config, Environment, LogLevel, SharedState};

#[derive(Parser)]
#[command(author, about, arg_required_else_help = false, disable_version_flag = true)]
struct Args {
    /// Optional host IP to listen to (for example "0.0.0.0")
    #[arg(long, value_name = "IP", env = "HOST")]
    host: Option<String>,

    /// Log level to use
    #[arg(value_enum, short, long, value_name = "LEVEL", default_value = "info")]
    log: Option<LogLevel>,

    /// Optional port number to use
    #[arg(short, long, value_name = "PORT", default_value_t = 3000, env = "PORT")]
    port: u16,

    // Custom version flag instead of clap default
    #[arg(short, long, help = "Print version info and exit")]
    version: bool,
}

/// `OpenAPI` documentation
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    paths(
        routes::root,
        routes::version,
        routes::query_item,
        routes::list_items,
        routes::create_item,
        admin::delete_all_items,
        admin::remove_item,
    ),
)]
pub struct ApiDoc;

/// Document api key in `OpenAPI` specs.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("api-key"))),
            );
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.version {
        println!("{}", version::version_info());
        return Ok(());
    }

    let run_environment = Environment::from_env();
    let use_json_logging = run_environment != Environment::Local;
    initialize_logging(args.log.as_ref(), use_json_logging);

    tracing::info!("Starting {} {}", version::PACKAGE_NAME, run_environment);
    if use_json_logging {
        tracing::info!("{}", VERSION_INFO);
    } else {
        tracing::info!("{}", VERSION_INFO.to_string_pretty());
    }

    let file_config = FileConfig::get_config();

    let shared_state = AppState::new_shared_state();
    let config = Arc::new(Config::new_from_env());

    if file_config.periodic_db_log_enabled {
        let state_for_log = Arc::clone(&shared_state);
        tokio::spawn(async move {
            periodic_history_log(state_for_log, file_config.periodic_db_log_interval).await;
        });
    }

    // Build application with routes
    let app = build_router(&shared_state, &config);

    let address = get_address(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    // Run server app with Hyper
    axum::serve(listener, app)
        .with_graceful_shutdown(utils::shutdown_signal())
        .await?;

    Ok(())
}

/// Initialize tracing logging.
fn initialize_logging(log_level: Option<&LogLevel>, use_json_format: bool) {
    // Log level filter
    let mut filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Some(level) = log_level {
        filter = filter.add_directive(level.to_filter().into());
    }

    if use_json_format {
        // Format logs as JSON for CloudWatch.
        tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_span_list(false)
            .with_env_filter(filter)
            .with_target(true)
            .with_level(true)
            // ANSI color codes do not work nicely in CloudWatch logs.
            .with_ansi(false)
            // Disable time because CloudWatch will add the ingestion time.
            .without_time()
            .init();
    } else {
        // Format logs for terminal printing
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_ansi(true)
            .with_level(true)
            // Add source code file and line number for local debugging.
            .with_file(true)
            .with_line_number(true)
            .init();
    }
}

/// Resolve socket address (ip and port) from arguments or use default.
fn get_address(host: Option<String>, port: u16) -> SocketAddr {
    let ip = host.map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |ip_string| {
        ip_string.parse::<IpAddr>().unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    });
    SocketAddr::new(ip, port)
}

/// Run history statistics logging periodically.
async fn periodic_history_log(state: SharedState, interval_seconds: u64) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
    loop {
        interval.tick().await;
        let num_keys = state.db.len();
        let capacity = state.db.capacity();
        // TODO: print more statistics / info
        tracing::info!("db items: {num_keys}");
        tracing::info!("db capacity: {capacity}");
    }
}

/// Create Router app with routes and `OpenAPI` documentation.
fn build_router(shared_state: &SharedState, config: &Arc<Config>) -> Router {
    let router = Router::new()
        .route("/", get(routes::root))
        .route("/version", get(routes::version))
        .route("/item", get(routes::query_item))
        .route("/items", get(routes::list_items))
        .route("/items", post(routes::create_item))
        // Put all admin routes under /admin
        .nest("/admin", admin::routes())
        .layer(
            ServiceBuilder::new()
                // Pass config with api key and env to routes
                .layer(axum::Extension(Arc::clone(config)))
                // TraceLayer automatically creates spans for each HTTP request and logs relevant information.
                .layer(
                    TraceLayer::new_for_http()
                        // Log the request path at INFO level
                        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                        // Log the response time and path at INFO level
                        .on_response(DefaultOnResponse::new().level(Level::INFO))
                )
                .layer(
                    // Graceful shutdown will wait for outstanding requests to complete.
                    // Add a timeout so requests do not hang forever.
                    TimeoutLayer::with_status_code(StatusCode::SERVICE_UNAVAILABLE, tokio::time::Duration::from_secs(10)),
                ),
        )
        .with_state(Arc::clone(shared_state));

    // Don't add OpenAPI documentation for production environment.
    if config.env == Environment::Production {
        router
    } else {
        router
            .merge(SwaggerUi::new("/doc").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
            .merge(RapiDoc::new("/api-docs/openapi.json").path("/rapidoc"))
            .merge(Scalar::with_url("/scalar", ApiDoc::openapi()))
    }
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

    use crate::schemas::ItemListResponse;
    use crate::types::AppState;
    use crate::types::Item;
    use crate::version;

    #[tokio::test]
    async fn test_root() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert!(body.get("message").is_some(), "Body does not contain 'message' key");
        assert!(body["message"].is_string(), "'message' is not a string");
    }

    #[tokio::test]
    async fn test_version() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(Request::builder().uri("/version").body(Body::empty()).unwrap())
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(body["name"], version::PACKAGE_NAME);
        assert_eq!(body["version"], version::PACKAGE_VERSION);
        assert_eq!(body["build_time"], version::BUILD_TIME);
        assert_eq!(body["branch"], version::GIT_BRANCH);
        assert_eq!(body["commit"], version::GIT_COMMIT);
        assert_eq!(body["rust_version"], version::RUST_VERSION);
    }

    #[tokio::test]
    async fn create_item() {
        let item_json = r#"{"name": "test"}"#;

        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to get body bytes")
            .to_bytes();

        assert!(!body.is_empty());

        let item: Item = serde_json::from_slice(&body).unwrap();
        assert_eq!(item.name, "test");
        assert!(item.id <= 9999);
        assert!(item.id >= 1000);

        let app = build_router(&shared_state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn list_items() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/items")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to get body bytes")
            .to_bytes();

        assert!(!body.is_empty());

        let item_list: ItemListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(item_list.num_items, 0);
        assert!(item_list.names.is_empty());

        let item_json = r#"{"name": "test"}"#;
        let app = build_router(&shared_state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::CREATED);

        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/items")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to get body bytes")
            .to_bytes();

        assert!(!body.is_empty());

        let item_list: ItemListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(item_list.num_items, 1);
        assert!(!item_list.names.is_empty());
    }

    #[tokio::test]
    async fn create_item_missing_data() {
        let item_json = r#"{"wrong": "test"}"#;

        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .header("api-key", &config.api_key)
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        // Missing data -> 422 status
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn create_item_malformed_json_data() {
        // Malformed JSON
        let item_json = r#"{"name": "test", "id": 1234,}"#;

        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .header("Content-Type", "application/json")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        // JSON Syntax error -> 400 status
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_item_missing_content_type() {
        let item_json = r#"{"name": "test", "id": 1234}"#;

        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items")
                    .body(Body::from(item_json))
                    .unwrap(),
            )
            .await
            .expect("Failed to get response");

        // 415 status
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn admin_missing_api_key() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/clear_items")
                    .body(Body::empty())
                    .expect("Oneshot failed for /analyze"),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_invalid_api_key() {
        let shared_state = AppState::new_shared_state();
        let config = Arc::new(Config::default());
        let app = build_router(&shared_state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/clear_items")
                    .header("api-key", "wrong_api_key")
                    .body(Body::empty())
                    .expect("Oneshot failed for /analyze"),
            )
            .await
            .expect("Failed to get response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
