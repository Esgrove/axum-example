//! Types.
//!
//! Type definitions for internal types and API configuration.
//!

use std::str::FromStr;
use std::sync::Arc;
use std::{env, fmt};

use anyhow::{anyhow, Context};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use dashmap::DashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::level_filters::LevelFilter;
use utoipa::ToSchema;

use crate::schemas::AuthErrorResponse;

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
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Environment {
    Production,
    Test,
    Development,
    #[default]
    Local,
}

/// Shared state that simulates a database
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    pub db: DashMap<String, Item>,
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
        Self {
            db: DashMap::with_capacity(8192),
        }
    }

    pub fn new_shared_state() -> SharedState {
        Arc::new(Self::new())
    }

    #[allow(unused)]
    /// Serialize to pretty json.
    pub fn to_json_pretty(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize state")
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
        let mut rng = rand::thread_rng();
        let id: u64 = rng.gen_range(1000..=9999);
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
        env::var("API_ENV").map_or_else(|_| Self::default(), |value| Self::from_str(&value).unwrap_or_default())
    }
}

impl FromStr for Environment {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "DEVELOPMENT" => Ok(Self::Development),
            "PRODUCTION" => Ok(Self::Production),
            "TEST" => Ok(Self::Test),
            "LOCAL" => Ok(Self::Local),
            _ => Err(anyhow::anyhow!("Invalid environment value: '{input}'")),
        }
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

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Production => {
                    "PRODUCTION"
                }
                Self::Test => {
                    "TEST"
                }
                Self::Development => {
                    "DEVELOPMENT"
                }
                Self::Local => {
                    "LOCAL"
                }
            }
        )
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
                tracing::warn!("Invalid API key: {} {}", parts.method.as_str(), parts.uri.path());
                Err(AuthErrorResponse::new(format!("Invalid API key: '{api_key}'")))
            }
            None => {
                tracing::warn!("Missing API key header: {} {}", parts.method.as_str(), parts.uri.path());
                Err(AuthErrorResponse::new_from_str("Missing api-key header"))
            }
        }
    }
}
