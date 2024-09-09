use colored::{ColoredString, Colorize};
use tokio::signal;

use crate::build;

/// Format bool value as a coloured string.
pub fn colorize_bool(value: bool) -> ColoredString {
    if value {
        "true".green()
    } else {
        "false".yellow()
    }
}

/// Return formatted version information string
pub fn formatted_version_info() -> String {
    format!(
        "{} {} {} {} {} {} {}",
        build::PROJECT_NAME,
        build::PKG_VERSION,
        build::BUILD_TIME_3339,
        build::BRANCH,
        build::SHORT_COMMIT,
        build::BUILD_OS,
        build::RUST_VERSION,
    )
}

#[allow(clippy::redundant_pub_crate)]
/// Handle shutdown signal.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
