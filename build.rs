//! Cargo build script — runs automatically before compilation.
//!
//! Gathers build metadata and exposes them as compile-time env vars
//! readable via `env!()` macro in source code.

use std::process::Command;

fn main() {
    let name = std::env::var("CARGO_PKG_NAME").expect("CARGO_PKG_NAME not set");
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION not set");

    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map_or_else(
            |_| "unknown".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );

    let git_branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_or_else(
            |_| "unknown".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );

    let build_time = Command::new("date")
        .args(["-u", "+%Y-%m-%d_%H:%M:%S"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string());

    let rust_version = Command::new("rustc").args(["--version"]).output().map_or_else(
        |_| "unknown".to_string(),
        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
    );

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
