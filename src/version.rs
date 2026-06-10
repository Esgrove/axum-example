//! Compile-time version and build metadata.
//!
//! Values are populated by `build.rs` with `cargo:rustc-env` directives.
//! The `/version` route and structured logging helpers use this module so
//! responses and logs can be tied back to the exact binary that emitted them.

/// UTC timestamp recorded when `build.rs` generated the binary metadata.
pub static BUILD_TIME: &str = env!("BUILD_TIME");
/// Deployment tag injected by the build environment.
pub static DEPLOY_TAG: &str = env!("DEPLOY_TAG");
/// Git branch name captured by `build.rs`.
pub static GIT_BRANCH: &str = env!("GIT_BRANCH");
/// Git commit SHA captured by `build.rs`.
pub static GIT_COMMIT: &str = env!("GIT_COMMIT");
/// Cargo package name compiled into the service.
pub static PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
/// Package version compiled into the service.
pub static PACKAGE_VERSION: &str = env!("VERSION");
/// Rust compiler version captured by `build.rs`.
pub static RUST_VERSION: &str = env!("RUST_VERSION");

/// One-line human-readable identity line composed by `build.rs`:
/// `<name> <version> <build_time> <git_branch> <git_commit>`.
pub static VERSION_STRING: &str = env!("VERSION_STRING");
