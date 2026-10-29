//! Binary entrypoint.
//!
//! Parses CLI arguments, configures logging, builds shared application state,
//! and runs the Axum HTTP server with graceful shutdown.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;

use axum_example::logging::{initialize_logging, log_error, log_info};
use axum_example::schemas::VERSION_INFO;
use axum_example::telemetry::Telemetry;
use axum_example::utils::{self, get_address};
use axum_example::{AppState, Config, LogLevel, build_router_with_mcp, version};

// Command-line arguments.
// Clap would show a doc comment here as the `--help` description instead of the package description,
// so this is a plain comment.
// The `host` and `port` arguments also read the `HOST` and `PORT` environment variables,
// so containers can set the listen address without CLI flags.
#[derive(Parser)]
#[command(author, about, arg_required_else_help = false, disable_version_flag = true)]
struct Args {
    /// Optional host IP to listen on (for example "0.0.0.0").
    #[arg(short = 'H', long, value_name = "IP", env = "HOST")]
    host: Option<String>,

    /// Log level to use.
    #[arg(short, long, value_enum, value_name = "LEVEL", default_value = "info")]
    log: Option<LogLevel>,

    /// TCP port number to listen on.
    #[arg(short, long, value_name = "PORT", default_value_t = 3000, env = "PORT")]
    port: u16,

    /// Print version info and exit.
    // Custom version flag instead of the clap default, so the output format stays under our control.
    #[arg(short, long, help = "Print version info and exit")]
    version: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let startup_started_at = Instant::now();
    let args = Args::parse();
    if args.version {
        println!("{}", version::VERSION_STRING);
        return Ok(());
    }

    // Load configuration before logging, because the environment decides the log format.
    // A config error is still logged, using the JSON format that deployed environments expect.
    let config = Config::from_env();
    let use_json_logging = config.as_ref().map_or(true, |config| !config.env.is_local());
    initialize_logging(args.log, use_json_logging);
    let config = config.inspect_err(|err| log_error!(error = %err, "failed to load configuration"))?;

    log_info!("Starting {} {}", version::PACKAGE_NAME, config.env);
    if use_json_logging {
        log_info!("{}", VERSION_INFO);
    } else {
        log_info!("{}", VERSION_INFO.to_string_pretty());
    }

    let address = get_address(args.host, args.port)?;
    run_server(config, address, startup_started_at).await
}

/// Build the application state and router, bind the listener, and serve until shutdown.
async fn run_server(config: Config, address: SocketAddr, startup_started_at: Instant) -> Result<()> {
    let telemetry = Telemetry::from_env().inspect_err(|err| log_error!(error = %err, "failed to start telemetry"))?;
    let shared_state = Arc::new(AppState::new_with_telemetry(config, telemetry));
    let app = build_router_with_mcp(&shared_state);

    let listener = tokio::net::TcpListener::bind(address).await?;
    let listen_address = listener.local_addr()?;
    let startup_time_seconds = startup_started_at.elapsed().as_secs_f64();
    log_info!("Server started in {startup_time_seconds:.3}s, listening on {listen_address}");

    // `into_make_service_with_connect_info` exposes the peer address to the rate limiter
    // as the fallback client key when no `X-Forwarded-For` header is present.
    let server_result = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            utils::shutdown_signal().await;
            log_info!("Shutdown signal received, starting graceful shutdown");
        })
        .await;

    shared_state.telemetry.shutdown();
    match &server_result {
        Ok(()) => log_info!("Server shut down"),
        Err(error) => log_error!(error = %error, "Server stopped with error"),
    }
    server_result.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse from an argv-style slice where the first element is the binary name.
    fn parse(argv: &[&str]) -> Args {
        Args::try_parse_from(argv).expect("args should parse")
    }

    #[test]
    fn log_level_defaults_to_info_and_parses_explicit_values() {
        assert_eq!(parse(&["axum-example"]).log, Some(LogLevel::Info));
        assert_eq!(parse(&["axum-example", "--log", "debug"]).log, Some(LogLevel::Debug));
        assert_eq!(parse(&["axum-example", "-l", "error"]).log, Some(LogLevel::Error));
    }

    #[test]
    fn explicit_host_and_port_flags_are_parsed() {
        let args = parse(&["axum-example", "--host", "127.0.0.1", "--port", "7000"]);
        assert_eq!(args.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(args.port, 7000);
    }

    #[test]
    fn version_flag_is_off_by_default_and_set_by_both_spellings() {
        assert!(!parse(&["axum-example"]).version);
        assert!(parse(&["axum-example", "--version"]).version);
        assert!(parse(&["axum-example", "-v"]).version);
    }

    #[test]
    fn invalid_values_are_rejected() {
        assert!(Args::try_parse_from(["axum-example", "--port", "not-a-number"]).is_err());
        assert!(Args::try_parse_from(["axum-example", "--log", "verbose"]).is_err());
        assert!(Args::try_parse_from(["axum-example", "--frobnicate"]).is_err());
    }
}
