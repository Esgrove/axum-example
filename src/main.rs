//! Run with
//!
//! ```not_rust
//! cargo run --release
//! ```

mod routes;
mod utils;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::{arg, Parser};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::EnvFilter;

use shadow_rs::shadow;

use crate::utils::LogLevel;

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
    /// Optional port number to use
    #[arg(short, long, help = "Port number", value_name = "PORT")]
    port: Option<u16>,

    /// Log level
    #[arg(value_enum, short, long, help = "Log level", value_name = "LEVEL")]
    log: Option<LogLevel>,

    /// Custom version flag instead of clap default
    #[arg(short, long, help = "Print version info and exit")]
    version: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    if args.version {
        println!(
            "{} {} {} {} {} {} {}",
            build::PROJECT_NAME,
            build::PKG_VERSION,
            build::BUILD_TIME,
            build::BRANCH,
            build::SHORT_COMMIT,
            build::BUILD_OS,
            build::RUST_VERSION,
        );
        return Ok(());
    }

    let port_number = args.port.unwrap_or(3000);

    // Get logging level to use
    let log_level_filter = match args.log {
        None => LevelFilter::INFO,
        Some(ref level) => level.to_filter(),
    };

    let filter_layer = match EnvFilter::try_from_default_env() {
        Ok(level) => level,
        Err(_) => EnvFilter::from_default_env().add_directive(log_level_filter.into()),
    };

    // Initialize tracing
    tracing_subscriber::fmt().with_env_filter(filter_layer).init();

    // Build application with routes
    let app = Router::new()
        .route("/", get(routes::root))
        .route("/version", get(routes::version))
        .route("/user", get(routes::query_user))
        .route("/users", post(routes::create_user));

    // Run app with Hyper
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port_number)).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
