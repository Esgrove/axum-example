// https://github.com/baoyachi/shadow-rs

use shadow_rs::SdResult;
use std::fs::File;
use std::io::Write;

fn main() {
    // Invalidate the build script if DEPLOYMENT_TAG changes
    println!("cargo:rerun-if-env-changed=DEPLOYMENT_TAG");
    // Generate build information
    // https://github.com/baoyachi/shadow-rs
    shadow_rs::ShadowBuilder::builder()
        .hook(hook)
        .build()
        .expect("Shadow build failed");
}

fn hook(file: &File) -> SdResult<()> {
    append_deployment_tag(file)?;
    Ok(())
}

fn append_deployment_tag(mut file: &File) -> SdResult<()> {
    // Read the deployment version from the environment variable
    let tag = std::env::var("DEPLOYMENT_TAG").unwrap_or_else(|_| "local".to_string());
    let hook_const = format!("pub const DEPLOYMENT_TAG: &str = \"{tag}\";");
    writeln!(file, "{hook_const}")?;
    Ok(())
}
