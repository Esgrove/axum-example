//! Runtime configuration.
//!
//! All settings are read once at startup from environment variables.
//! Values are trimmed, blank values count as unset,
//! and malformed values fail startup with a typed [`ConfigError`]
//! instead of silently falling back to a default.

use std::str::FromStr;
use std::time::Duration;

use crate::errors::ConfigError;
use crate::types::Environment;
use crate::utils::read_environment_variable;

/// Env var holding the admin API key.
pub const API_KEY_ENV_VAR: &str = "API_KEY";
/// Env var selecting the runtime environment.
pub const API_ENV_ENV_VAR: &str = "API_ENV";
/// Env var for the maximum number of requests per client in one rate-limit window.
pub const RATE_LIMIT_REQUESTS_ENV_VAR: &str = "RATE_LIMIT_REQUESTS";
/// Env var for the rate-limit window length in seconds.
pub const RATE_LIMIT_WINDOW_SECONDS_ENV_VAR: &str = "RATE_LIMIT_WINDOW_SECONDS";
/// Env var that disables rate limiting when set to `true`.
pub const RATE_LIMIT_DISABLED_ENV_VAR: &str = "RATE_LIMIT_DISABLED";
/// Env var for the per-request timeout in seconds.
pub const REQUEST_TIMEOUT_SECONDS_ENV_VAR: &str = "REQUEST_TIMEOUT_SECONDS";

/// Default admin API key for local development.
///
/// Real deployments should inject an environment-specific key from a secret store.
pub const DEFAULT_API_KEY: &str = "axum-api-key";
/// Default number of requests allowed per client in one window.
pub const DEFAULT_RATE_LIMIT_REQUESTS: usize = 300;
/// Default rate-limit window.
pub const DEFAULT_RATE_LIMIT_WINDOW: Duration = Duration::from_mins(1);
/// Default per-request timeout.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Runtime configuration shared by the router, middleware, and route handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// API key required by the `/admin` routes.
    pub api_key: String,
    /// Runtime environment, controlling log format and whether docs routes are mounted.
    pub env: Environment,
    /// Per-client rate-limit settings.
    pub rate_limit: RateLimitConfig,
    /// Timeout applied to every request.
    pub request_timeout: Duration,
}

/// Per-client rate-limit settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// Whether rate limiting is applied at all.
    pub enabled: bool,
    /// Maximum requests per client in one window.
    pub max_requests: usize,
    /// Sliding window length.
    pub window: Duration,
}

impl Config {
    /// Build configuration from the process environment.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when a set variable cannot be parsed or is out of range.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(read_environment_variable)
    }

    /// Build configuration from an arbitrary variable lookup.
    ///
    /// Tests pass a closure over a fixed map so they never mutate the real process environment.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when a set variable cannot be parsed or is out of range.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let env = match lookup(API_ENV_ENV_VAR) {
            Some(value) => parse_value(API_ENV_ENV_VAR, &value)?,
            None => Environment::default(),
        };
        let max_requests = parse_optional(&lookup, RATE_LIMIT_REQUESTS_ENV_VAR)?.unwrap_or(DEFAULT_RATE_LIMIT_REQUESTS);
        let window = parse_seconds(&lookup, RATE_LIMIT_WINDOW_SECONDS_ENV_VAR)?.unwrap_or(DEFAULT_RATE_LIMIT_WINDOW);
        let disabled = parse_optional::<bool>(&lookup, RATE_LIMIT_DISABLED_ENV_VAR)?.unwrap_or(false);
        let request_timeout =
            parse_seconds(&lookup, REQUEST_TIMEOUT_SECONDS_ENV_VAR)?.unwrap_or(DEFAULT_REQUEST_TIMEOUT);

        if max_requests == 0 {
            return Err(ConfigError::OutOfRange {
                name: RATE_LIMIT_REQUESTS_ENV_VAR,
                message: "must be at least 1".to_string(),
            });
        }

        Ok(Self {
            api_key: lookup(API_KEY_ENV_VAR).unwrap_or_else(|| DEFAULT_API_KEY.to_string()),
            env,
            rate_limit: RateLimitConfig {
                enabled: !disabled,
                max_requests,
                window,
            },
            request_timeout,
        })
    }

    /// Return `true` when the `OpenAPI` documentation routes should be mounted.
    #[must_use]
    pub fn docs_enabled(&self) -> bool {
        self.env != Environment::Production
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: DEFAULT_API_KEY.to_string(),
            env: Environment::default(),
            rate_limit: RateLimitConfig::default(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_requests: DEFAULT_RATE_LIMIT_REQUESTS,
            window: DEFAULT_RATE_LIMIT_WINDOW,
        }
    }
}

/// Parse a raw value into `T`, attaching the variable name to the error.
fn parse_value<T: FromStr>(name: &'static str, value: &str) -> Result<T, ConfigError>
where
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|err| ConfigError::Invalid {
        name,
        value: value.to_string(),
        message: err.to_string(),
    })
}

/// Parse an optional variable into `T`.
fn parse_optional<T: FromStr>(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &'static str,
) -> Result<Option<T>, ConfigError>
where
    T::Err: std::fmt::Display,
{
    lookup(name).map(|value| parse_value(name, &value)).transpose()
}

/// Parse an optional whole-second duration that must be at least one second.
fn parse_seconds(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &'static str,
) -> Result<Option<Duration>, ConfigError> {
    match parse_optional::<u64>(lookup, name)? {
        Some(0) => Err(ConfigError::OutOfRange {
            name,
            message: "must be at least 1 second".to_string(),
        }),
        Some(seconds) => Ok(Some(Duration::from_secs(seconds))),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    fn config_from(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        Config::from_lookup(|name| map.get(name).cloned())
    }

    #[test]
    fn defaults_apply_when_nothing_is_set() {
        let config = config_from(&[]).expect("empty env should parse");
        assert_eq!(config, Config::default());
        assert!(config.docs_enabled());
    }

    #[test]
    fn parses_all_supported_variables() {
        let config = config_from(&[
            (API_KEY_ENV_VAR, "secret"),
            (API_ENV_ENV_VAR, "production"),
            (RATE_LIMIT_REQUESTS_ENV_VAR, "5"),
            (RATE_LIMIT_WINDOW_SECONDS_ENV_VAR, "30"),
            (RATE_LIMIT_DISABLED_ENV_VAR, "true"),
            (REQUEST_TIMEOUT_SECONDS_ENV_VAR, "3"),
        ])
        .expect("valid env should parse");

        assert_eq!(config.api_key, "secret");
        assert_eq!(config.env, Environment::Production);
        assert!(!config.docs_enabled());
        assert_eq!(
            config.rate_limit,
            RateLimitConfig {
                enabled: false,
                max_requests: 5,
                window: Duration::from_secs(30),
            }
        );
        assert_eq!(config.request_timeout, Duration::from_secs(3));
    }

    #[test]
    fn rejects_invalid_values() {
        for (name, value) in [
            (API_ENV_ENV_VAR, "staging-ish"),
            (RATE_LIMIT_REQUESTS_ENV_VAR, "lots"),
            (RATE_LIMIT_REQUESTS_ENV_VAR, "0"),
            (RATE_LIMIT_WINDOW_SECONDS_ENV_VAR, "0"),
            (RATE_LIMIT_DISABLED_ENV_VAR, "maybe"),
            (REQUEST_TIMEOUT_SECONDS_ENV_VAR, "-1"),
        ] {
            let error = config_from(&[(name, value)]).expect_err("invalid value should fail");
            assert!(error.to_string().contains(name), "error should name {name}: {error}");
        }
    }
}
