//! Binary entrypoint.
//!
//! Parses CLI arguments, configures logging, builds shared application state,
//! and runs the Axum HTTP server with graceful shutdown.

mod logging;
mod middleware;
mod openapi;
mod router;
mod schemas;
mod telemetry;
mod types;
mod utils;
mod version;
mod routing {
    pub mod admin;
    pub mod routes;
}

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;

use crate::logging::initialize_logging;
use crate::router::build_router;
use crate::schemas::VERSION_INFO;
use crate::types::{AppState, Config, Environment, LogLevel};

#[derive(Parser)]
#[command(author, about, arg_required_else_help = false, disable_version_flag = true)]
struct Args {
    /// Optional host IP to listen to (for example "0.0.0.0")
    #[arg(short = 'H', long, value_name = "IP", env = "HOST")]
    host: Option<String>,

    /// Log level to use
    #[arg(short, long, value_enum, value_name = "LEVEL", default_value = "info")]
    log: Option<LogLevel>,

    /// Optional port number to use
    #[arg(short, long, value_name = "PORT", default_value_t = 3000, env = "PORT")]
    port: u16,

    // Custom version flag instead of clap default
    #[arg(short, long, help = "Print version info and exit")]
    version: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.version {
        println!("{}", version::VERSION_STRING);
        return Ok(());
    }

    let run_environment = Environment::from_env();
    let use_json_logging = run_environment != Environment::Local;
    initialize_logging(args.log.as_ref(), use_json_logging);

    log_info!("Starting {} {}", version::PACKAGE_NAME, run_environment);
    if use_json_logging {
        log_info!("{}", VERSION_INFO);
    } else {
        log_info!("{}", VERSION_INFO.to_string_pretty());
    }

    run_server(args).await
}

/// Set up application state, spawn background tasks and run the HTTP server.
async fn run_server(args: Args) -> Result<()> {
    let shared_state = AppState::new_shared_state_from_env()?;
    let config = Arc::new(Config::new_from_env());

    // Build application with routes
    let app = build_router(&shared_state, &config);

    let address = get_address(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(address).await?;
    log_info!("listening on {}", listener.local_addr()?);

    // Run server app with Hyper
    axum::serve(listener, app)
        .with_graceful_shutdown(utils::shutdown_signal())
        .await?;

    Ok(())
}

/// Resolve socket address (ip and port) from arguments or use default.
fn get_address(host: Option<String>, port: u16) -> SocketAddr {
    let ip = host.map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |ip_string| {
        ip_string.parse::<IpAddr>().unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    });
    SocketAddr::new(ip, port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_address_defaults_to_localhost() {
        let address = get_address(None, 3000);

        assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(address.port(), 3000);
    }

    #[test]
    fn get_address_uses_valid_host_argument() {
        let address = get_address(Some("0.0.0.0".to_string()), 8080);

        assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(address.port(), 8080);
    }

    #[test]
    fn get_address_falls_back_to_unspecified_for_invalid_host() {
        let address = get_address(Some("not-an-ip".to_string()), 1234);

        assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(address.port(), 1234);
    }
}
