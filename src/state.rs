//! Shared application state.
//!
//! [`AppState`] is the service composition root.
//! It owns the runtime configuration, the in-memory item store,
//! the status service, telemetry, and the rate limiters,
//! and it is shared with every handler as [`SharedState`].

use std::sync::Arc;

use dashmap::DashMap;

use crate::config::Config;
use crate::rate_limit::RateLimiter;
use crate::status::StatusService;
use crate::telemetry::Telemetry;
use crate::types::Item;

/// Thread-safe handle to the shared app state.
pub type SharedState = Arc<AppState>;

/// Shared state handed to every route handler via `State<SharedState>`.
#[derive(Debug)]
pub struct AppState {
    /// Runtime configuration loaded at startup.
    pub config: Arc<Config>,
    /// In-memory item store standing in for a database.
    pub db: DashMap<String, Item>,
    /// Status and health tracker.
    pub status: StatusService,
    /// OpenTelemetry provider and metric instruments.
    pub telemetry: Telemetry,
    /// Per-client rate limiter.
    ///
    /// Stored here rather than built per router,
    /// so the MCP server's internal-dispatch router shares the same buckets as public HTTP traffic.
    pub rate_limiter: RateLimiter,
}

impl AppState {
    /// Build state around `config` with Prometheus-only telemetry.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self::new_with_telemetry(config, Telemetry::noop())
    }

    /// Build state around `config` and the supplied telemetry handle.
    #[must_use]
    pub fn new_with_telemetry(config: Config, telemetry: Telemetry) -> Self {
        let rate_limiter = RateLimiter::new(config.rate_limit);
        Self {
            config: Arc::new(config),
            db: DashMap::with_capacity(1024),
            status: StatusService::new(),
            telemetry,
            rate_limiter,
        }
    }

    /// Build shared state around `config` with Prometheus-only telemetry.
    #[must_use]
    pub fn new_shared(config: Config) -> SharedState {
        Arc::new(Self::new(config))
    }

    /// Build shared state from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration parsing or telemetry initialization fails.
    pub fn new_from_env() -> anyhow::Result<SharedState> {
        let config = Config::from_env()?;
        let telemetry = Telemetry::from_env()?;
        Ok(Arc::new(Self::new_with_telemetry(config, telemetry)))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(Config::default())
    }
}
