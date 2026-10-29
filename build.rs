//! Cargo build script, which runs automatically before compilation.
//!
//! Gathers build metadata and exposes it as compile-time env vars,
//! which source code reads with the `env!()` macro.

use std::process::Command;

/// Gather build metadata and export it at compile time.
fn main() {
    // Cargo guarantees these values while compiling a package.
    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");

    // Each value comes from an external command.
    // `command_output_or_unknown` falls back to "unknown" when the command fails to spawn,
    // exits non-zero, or produces empty output, so version metadata is never silently blank.
    let git_hash = command_output_or_unknown("git", &["rev-parse", "--short", "HEAD"]);
    let git_branch = command_output_or_unknown("git", &["rev-parse", "--abbrev-ref", "HEAD"]);
    let build_time = command_output_or_unknown("date", &["-u", "+%Y-%m-%d_%H:%M:%S"]);
    let rust_version = command_output_or_unknown("rustc", &["--version"]);

    let tag = std::env::var("DEPLOY_TAG").unwrap_or_else(|_| "local".to_string());

    // Combined human-readable version string
    let version_string = format!("{name} {version} {build_time} {git_branch} {git_hash}");

    // Set compile time env variables
    println!("cargo:rustc-env=BUILD_TIME={build_time}");
    println!("cargo:rustc-env=DEPLOY_TAG={tag}");
    println!("cargo:rustc-env=GIT_BRANCH={git_branch}");
    println!("cargo:rustc-env=GIT_COMMIT={git_hash}");
    println!("cargo:rustc-env=RUST_VERSION={rust_version}");
    println!("cargo:rustc-env=VERSION_STRING={version_string}");
    println!("cargo:rustc-env=VERSION={version}");

    // Tell Cargo to rerun the build script when any of these change.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DEPLOY_TAG");
}

/// Run a command and return its trimmed stdout.
///
/// Falls back to `"unknown"` when the command fails to spawn,
/// exits with a non-zero status, or produces empty output.
/// Checking the exit status matters because a failing `git`,
/// for example outside a repository, exits non-zero with empty stdout.
fn command_output_or_unknown(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
