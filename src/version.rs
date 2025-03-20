pub static BUILD_TIME: &str = env!("BUILD_TIME");
pub static DEPLOY_TAG: &str = env!("DEPLOY_TAG");
pub static GIT_BRANCH: &str = env!("GIT_BRANCH");
pub static GIT_COMMIT: &str = env!("GIT_COMMIT");
pub static PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub static PACKAGE_VERSION: &str = env!("VERSION");
pub static RUST_VERSION: &str = env!("RUST_VERSION");

static VERSION_STRING: &str = concat!(
    env!("CARGO_PKG_NAME"),
    " ",
    env!("VERSION"),
    " ",
    env!("BUILD_TIME"),
    " ",
    env!("GIT_BRANCH"),
    " ",
    env!("GIT_COMMIT")
);

/// Return version info string
#[inline]
pub fn version_info() -> &'static str {
    VERSION_STRING
}
