//! Internal types.
//!
//! Domain and runtime types that are shared across modules
//! but are not part of the HTTP schema surface.

use std::ops::RangeInclusive;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::level_filters::LevelFilter;
use utoipa::ToSchema;

use crate::errors::{ApiError, ItemError};
use crate::logging::log_warn;
use crate::state::SharedState;

/// Accepted range for item ids.
pub const ITEM_ID_RANGE: RangeInclusive<u64> = 1000..=9999;

/// Header carrying the admin API key.
pub const API_KEY_HEADER: &str = "api-key";

/// Logging level CLI parameter.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogLevel {
    /// Most verbose level.
    Trace,
    /// Debug diagnostics.
    Debug,
    /// Normal operational messages.
    #[default]
    Info,
    /// Recoverable problems.
    Warn,
    /// Failures only.
    Error,
}

/// Runtime environment.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
pub enum Environment {
    /// Production deployment, with documentation routes hidden.
    Production,
    /// Shared test deployment.
    Test,
    /// Shared development deployment.
    Development,
    /// Local developer machine.
    #[default]
    Local,
}

/// Item stored in the in-memory database.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ToSchema)]
pub struct Item {
    /// Item id between 1000 and 9999.
    #[schema(example = 1234)]
    pub id: u64,
    /// Unique item name.
    #[schema(example = "esgrove")]
    pub name: String,
}

/// Extractor that rejects requests without a valid `api-key` header.
#[derive(Debug, Clone, Copy)]
pub struct ApiKeyExtractor;

impl Item {
    /// Create an item with a client-supplied id.
    ///
    /// # Errors
    ///
    /// Returns [`ItemError::InvalidId`] when `id` is outside [`ITEM_ID_RANGE`].
    pub fn new(name: String, id: u64) -> Result<Self, ItemError> {
        if ITEM_ID_RANGE.contains(&id) {
            Ok(Self { id, name })
        } else {
            Err(ItemError::InvalidId {
                id,
                min: *ITEM_ID_RANGE.start(),
                max: *ITEM_ID_RANGE.end(),
            })
        }
    }

    /// Create an item with a random id inside [`ITEM_ID_RANGE`].
    #[must_use]
    pub fn new_with_random_id(name: String) -> Self {
        let id: u64 = rand::rng().random_range(ITEM_ID_RANGE);
        Self { id, name }
    }
}

impl LogLevel {
    /// Convert the CLI log level to a tracing level filter.
    #[must_use]
    pub const fn to_filter(self) -> LevelFilter {
        match self {
            Self::Trace => LevelFilter::TRACE,
            Self::Debug => LevelFilter::DEBUG,
            Self::Info => LevelFilter::INFO,
            Self::Warn => LevelFilter::WARN,
            Self::Error => LevelFilter::ERROR,
        }
    }
}

impl Environment {
    /// Return `true` for local development.
    #[must_use]
    pub const fn is_local(self) -> bool {
        matches!(self, Self::Local)
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "the header check is synchronous, but `async fn` keeps the extractor readable"
)]
impl FromRequestParts<SharedState> for ApiKeyExtractor {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &SharedState) -> Result<Self, Self::Rejection> {
        match parts.headers.get(API_KEY_HEADER).and_then(|key| key.to_str().ok()) {
            Some(api_key) if api_key == state.config.api_key => Ok(Self),
            Some(_) => {
                log_warn!("Invalid API key: {} {}", parts.method.as_str(), parts.uri.path());
                Err(ApiError::Unauthorized("Invalid API key".to_string()))
            }
            None => {
                log_warn!("Missing API key header: {} {}", parts.method.as_str(), parts.uri.path());
                Err(ApiError::Unauthorized(format!("Missing {API_KEY_HEADER} header")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::RequestPartsExt;
    use axum::http::Request;

    use crate::config::Config;
    use crate::state::AppState;

    #[test]
    fn environment_round_trips_and_parses_case_insensitively() {
        for variant in [
            Environment::Development,
            Environment::Local,
            Environment::Production,
            Environment::Test,
        ] {
            let parsed: Environment = variant.to_string().parse().expect("environment should parse");
            assert_eq!(parsed, variant);
        }
        assert_eq!("production".parse::<Environment>().unwrap(), Environment::Production);
        assert!("not-real".parse::<Environment>().is_err());
        assert!(Environment::Local.is_local());
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
    fn item_constructor_validates_id_range() {
        assert_eq!(Item::new("valid".into(), 1000).unwrap().id, 1000);
        assert_eq!(Item::new("valid".into(), 9999).unwrap().id, 9999);
        assert!(Item::new("invalid".into(), 999).is_err());
        assert!(Item::new("invalid".into(), 10_000).is_err());
        assert!(ITEM_ID_RANGE.contains(&Item::new_with_random_id("random".into()).id));
    }

    #[tokio::test]
    async fn api_key_extractor_accepts_valid_key_and_rejects_others() {
        let state = AppState::new_shared(Config::default());

        for (header, accepted) in [
            (Some(state.config.api_key.as_str()), true),
            (Some("wrong"), false),
            (None, false),
        ] {
            let mut builder = Request::builder().uri("/admin/clear_items");
            if let Some(value) = header {
                builder = builder.header(API_KEY_HEADER, value);
            }
            let (mut parts, ()) = builder.body(()).expect("request").into_parts();
            let result = parts.extract_with_state::<ApiKeyExtractor, _>(&state).await;
            assert_eq!(result.is_ok(), accepted, "header {header:?}");
        }
    }
}
