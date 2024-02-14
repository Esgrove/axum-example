use crate::build;

use tokio::signal;

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
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

pub fn api_version_info() -> String {
    format!(
        "{} {} {} {} {} {} {}",
        build::PROJECT_NAME,
        build::PKG_VERSION,
        build::BUILD_TIME,
        build::BRANCH,
        build::SHORT_COMMIT,
        build::BUILD_OS,
        build::RUST_VERSION,
    )
}
