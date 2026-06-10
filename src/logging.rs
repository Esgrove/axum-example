//! Logging setup and structured logging helpers.
//!
//! The helper macros attach build metadata to each event so JSON logs can be
//! correlated with the exact binary version that emitted them.

use tracing_subscriber::EnvFilter;

use crate::types::LogLevel;

/// Log at DEBUG level with compile-time build metadata attached.
#[macro_export]
macro_rules! log_debug {
    (target: $target:expr, $($arg:tt)+) => {
        tracing::debug!(
            target: $target,
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        tracing::debug!(
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
}

/// Log at INFO level with compile-time build metadata attached.
#[macro_export]
macro_rules! log_info {
    (target: $target:expr, $($arg:tt)+) => {
        tracing::info!(
            target: $target,
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        tracing::info!(
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
}

/// Log at WARN level with compile-time build metadata attached.
#[macro_export]
macro_rules! log_warn {
    (target: $target:expr, $($arg:tt)+) => {
        tracing::warn!(
            target: $target,
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        tracing::warn!(
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
}

/// Log at ERROR level with compile-time build metadata attached.
#[macro_export]
macro_rules! log_error {
    (target: $target:expr, $($arg:tt)+) => {
        tracing::error!(
            target: $target,
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        tracing::error!(
            build_time = $crate::version::BUILD_TIME,
            branch = $crate::version::GIT_BRANCH,
            commit = $crate::version::GIT_COMMIT,
            version = $crate::version::PACKAGE_VERSION,
            $($arg)+
        )
    };
}

/// Initialize tracing logging.
pub fn initialize_logging(log_level: Option<&LogLevel>, use_json_format: bool) {
    let mut filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Some(level) = log_level {
        filter = filter.add_directive(level.to_filter().into());
    }

    if use_json_format {
        tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_span_list(false)
            .with_env_filter(filter)
            .with_target(true)
            .with_level(true)
            .with_ansi(false)
            .without_time()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_ansi(true)
            .with_level(true)
            .with_file(true)
            .with_line_number(true)
            .init();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn logging_macros_accept_structured_fields() {
        crate::log_debug!(route = "/health", "debug log");
        crate::log_info!(status = 200, "info log");
        crate::log_warn!(warning = "example", "warn log");
        crate::log_error!(error = "example", "error log");
    }
}
