//! Types.
//!
//! Type definitions for internal types and API configuration.

use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::level_filters::LevelFilter;
use utoipa::ToSchema;

use crate::schemas::AuthErrorResponse;
use crate::telemetry::Telemetry;

// Thread-safe pointer to app state
pub type SharedState = Arc<AppState>;

// This should be stored for example in AWS Secrets Manager or similar,
// for environment-specific API keys
pub const DEFAULT_API_KEY: &str = "axum-api-key";

/// Logging level CLI parameter.
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

/// Runtime environment enum.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub enum Environment {
    Production,
    Test,
    Development,
    #[default]
    Local,
}

/// Shared state that simulates a database
#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub db: DashMap<String, Item>,
    #[serde(skip, default = "Instant::now")]
    start_time: Instant,
    start_time_utc: DateTime<Utc>,
    #[serde(skip)]
    pub(crate) telemetry: Telemetry,
}

/// API config for passing settings to routes.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub api_key: String,
    pub env: Environment,
}

/// Item information
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ToSchema)]
pub struct Item {
    #[schema(example = "1234")]
    pub id: u64,
    #[schema(example = "esgrove")]
    pub name: String,
}

/// Custom extractor for checking api key.
///
/// Note: requires the Config extension to be present in the route as well,
/// so the correct api key can be accessed.
pub struct ApiKeyExtractor;

impl AppState {
    #[allow(unused)]
    pub fn new() -> Self {
        Self::new_with_telemetry(Telemetry::noop())
    }

    pub fn new_with_telemetry(telemetry: Telemetry) -> Self {
        Self {
            db: DashMap::with_capacity(8192),
            start_time: Instant::now(),
            start_time_utc: Utc::now(),
            telemetry,
        }
    }

    #[cfg(test)]
    pub fn new_shared_state() -> SharedState {
        Arc::new(Self::new())
    }

    pub fn new_shared_state_from_env() -> anyhow::Result<SharedState> {
        Ok(Arc::new(Self::new_with_telemetry(Telemetry::from_env()?)))
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub const fn start_time_utc(&self) -> DateTime<Utc> {
        self.start_time_utc
    }

    pub const fn telemetry(&self) -> &Telemetry {
        &self.telemetry
    }

    #[allow(unused)]
    /// Serialize to pretty json.
    pub fn to_json_pretty(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize state")
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl Item {
    /// Try to create new Item with given name and id.
    /// Returns Err if id is not valid.
    pub fn new(name: String, id: u64) -> anyhow::Result<Self> {
        if (1000..=10000).contains(&id) {
            Ok(Self { id, name })
        } else {
            Err(anyhow!("ID must be between 1000 and 9999"))
        }
    }

    pub fn new_with_random_id(name: String) -> Self {
        let id: u64 = rand::rng().random_range(1000..=9999);
        Self { id, name }
    }
}

impl LogLevel {
    /// Convert CLI log level to tracing log level filter
    pub const fn to_filter(&self) -> LevelFilter {
        match self {
            Self::Trace => LevelFilter::TRACE,
            Self::Debug => LevelFilter::DEBUG,
            Self::Info => LevelFilter::INFO,
            Self::Warn => LevelFilter::WARN,
            Self::Error => LevelFilter::ERROR,
        }
    }
}

impl Config {
    #[allow(unused)]
    pub const fn new(api_key: String, env: Environment) -> Self {
        Self { api_key, env }
    }

    /// Try to get values from env variables or otherwise use defaults.
    pub fn new_from_env() -> Self {
        Self {
            api_key: env::var("API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY.to_string()),
            env: Environment::from_env(),
        }
    }
}

impl Environment {
    /// Try to read runtime environment from env variable or otherwise use default.
    pub fn from_env() -> Self {
        env::var("API_ENV").map_or_else(|_| Self::default(), |value| value.parse().unwrap_or_default())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: DEFAULT_API_KEY.to_string(),
            env: Environment::default(),
        }
    }
}

/// This implements a custom Axum extractor for checking the api key in routes.
/// `FromRequestParts` is used here since this does not need access to the request body.
/// We only need to check the request headers for the api key.
impl<S> FromRequestParts<S> for ApiKeyExtractor
where
    S: Send + Sync,
{
    type Rejection = AuthErrorResponse;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let config = parts
            .extensions
            .get::<Arc<Config>>()
            .ok_or_else(|| AuthErrorResponse::new_from_str("Config extension missing from route"))?;

        match parts.headers.get("api-key").and_then(|key| key.to_str().ok()) {
            Some(api_key) if api_key == config.api_key => Ok(Self),
            Some(api_key) => {
                crate::log_warn!("Invalid API key: {} {}", parts.method.as_str(), parts.uri.path());
                Err(AuthErrorResponse::new(format!("Invalid API key: '{api_key}'")))
            }
            None => {
                crate::log_warn!("Missing API key header: {} {}", parts.method.as_str(), parts.uri.path());
                Err(AuthErrorResponse::new_from_str("Missing api-key header"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::RequestPartsExt;
    use axum::http::Request;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    #[test]
    fn environment_round_trips_through_strum() {
        for variant in [
            Environment::Development,
            Environment::Local,
            Environment::Production,
            Environment::Test,
        ] {
            let rendered = variant.to_string();
            let parsed: Environment = rendered.parse().expect("environment should parse");
            assert_eq!(parsed, variant);
        }
    }

    #[test]
    fn environment_parses_case_insensitively() {
        assert_eq!("production".parse::<Environment>().unwrap(), Environment::Production);
        assert_eq!("Local".parse::<Environment>().unwrap(), Environment::Local);
        assert_eq!("DEVELOPMENT".parse::<Environment>().unwrap(), Environment::Development);
        assert!("not-real".parse::<Environment>().is_err());
    }

    #[test]
    fn log_level_maps_every_variant_to_filter() {
        assert_eq!(LogLevel::Trace.to_filter(), LevelFilter::TRACE);
        assert_eq!(LogLevel::Debug.to_filter(), LevelFilter::DEBUG);
        assert_eq!(LogLevel::Info.to_filter(), LevelFilter::INFO);
        assert_eq!(LogLevel::Warn.to_filter(), LevelFilter::WARN);
        assert_eq!(LogLevel::Error.to_filter(), LevelFilter::ERROR);
    }

    #[test]
    fn config_default_uses_local_environment_and_default_key() {
        let config = Config::default();

        assert_eq!(config.api_key, DEFAULT_API_KEY);
        assert_eq!(config.env, Environment::Local);
    }

    #[test]
    fn item_constructor_accepts_valid_ids_and_rejects_out_of_range_ids() {
        let item = Item::new("valid".to_string(), 1000).expect("lower bound should be valid");
        assert_eq!(item.name, "valid");
        assert_eq!(item.id, 1000);

        let item = Item::new("valid".to_string(), 10_000).expect("upper bound should be valid");
        assert_eq!(item.id, 10_000);

        let error = Item::new("invalid".to_string(), 999).expect_err("low id should fail");
        assert!(error.to_string().contains("ID must be between 1000 and 9999"));

        let error = Item::new("invalid".to_string(), 10_001).expect_err("high id should fail");
        assert!(error.to_string().contains("ID must be between 1000 and 9999"));
    }

    #[test]
    fn app_state_serializes_database_without_runtime_fields() {
        let state = AppState::new();
        state.db.insert(
            "stored".to_string(),
            Item {
                id: 4321,
                name: "stored".to_string(),
            },
        );

        let json = state.to_json_pretty().expect("state should serialize");

        assert!(json.contains("stored"));
        assert!(json.contains("start_time_utc"));
        assert!(!json.contains("telemetry"));
    }

    #[tokio::test]
    async fn api_key_extractor_accepts_valid_key() {
        let config = Arc::new(Config::default());
        let request = Request::builder()
            .uri("/admin/clear_items")
            .header("api-key", &config.api_key)
            .body(())
            .expect("request should build");
        let (mut parts, ()) = request.into_parts();
        parts.extensions.insert(config);

        let extracted = parts
            .extract::<ApiKeyExtractor>()
            .await
            .expect("valid api key should extract");

        assert!(matches!(extracted, ApiKeyExtractor));
    }

    #[tokio::test]
    async fn api_key_extractor_rejects_missing_config_extension() {
        let request = Request::builder()
            .uri("/admin/clear_items")
            .header("api-key", DEFAULT_API_KEY)
            .body(())
            .expect("request should build");
        let (mut parts, ()) = request.into_parts();

        let Err(rejection) = parts.extract::<ApiKeyExtractor>().await else {
            panic!("missing config extension should reject");
        };
        let response = rejection.into_response();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body should collect")
            .to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).expect("body should be json");

        assert_eq!(body["message"], "Config extension missing from route");
    }
}
