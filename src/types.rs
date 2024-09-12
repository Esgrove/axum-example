use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::{env, fmt};

use anyhow::{anyhow, Context};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::level_filters::LevelFilter;
use utoipa::ToSchema;

// Thread-safe pointer to app state
pub type SharedState = Arc<RwLock<AppState>>;

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
    pub db: HashMap<String, Item>,
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

impl AppState {
    #[allow(unused)]
    pub fn new() -> Self {
        Self {
            db: HashMap::with_capacity(8192),
        }
    }

    pub fn new_shared_state() -> SharedState {
        Arc::new(RwLock::new(Self::new()))
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
            "K_DEVELOPMENT" => Ok(Self::Development),
            "K_PRODUCTION" => Ok(Self::Production),
            "K_TEST" => Ok(Self::Test),
            "K_LOCAL" => Ok(Self::Local),
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
