//! Axum API example library.
//!
//! The binary owns CLI parsing and process startup.
//! This library exposes the service modules used by the binary and the integration tests in `tests/`.

pub mod config;
pub mod errors;
pub mod logging;
pub mod mcp;
pub mod middleware;
pub mod openapi;
pub mod rate_limit;
pub mod router;
pub mod routing;
pub mod schemas;
pub mod state;
pub mod status;
pub mod telemetry;
pub mod types;
pub mod utils;
pub mod version;

pub use config::Config;
pub use router::{build_router, build_router_with_mcp};
pub use state::{AppState, SharedState};
pub use types::{Environment, LogLevel};
