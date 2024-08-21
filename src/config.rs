use std::{env, fmt, fs, path::PathBuf};

use anyhow::{anyhow, Context};
use colored::Colorize;
use dirs::home_dir;
use serde::Deserialize;

use crate::utils;

const CONFIG_FILE_NAME: &str = "axum-example.toml";

/// User config options from a config file.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    /// Enable S3 history backup
    pub enable_s3_backup: bool,
}

impl Config {
    /// Try to read config from file if it exists.
    /// Otherwise, fall back to default config.
    pub fn get_config() -> Self {
        Self::read_user_config().unwrap_or_default()
    }

    /// Read and parse user config if it exists.
    fn read_user_config() -> Option<Self> {
        Self::user_config_file_path()
            .ok()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|config_string| toml::from_str(&config_string).ok())
    }

    /// Get user config file if it exists.
    fn user_config_file_path() -> anyhow::Result<PathBuf> {
        // Check in the current working directory first
        let current_dir = env::current_dir().context("Failed to get current directory")?;
        let local_config_path = current_dir.join(CONFIG_FILE_NAME);

        // Using try_exists() to check file existence in the current directory
        if local_config_path
            .try_exists()
            .context("Failed to check local config file existence")?
        {
            tracing::info!("Found local config file: {}", local_config_path.display());
            return Ok(local_config_path);
        }

        // If not found, check in the home directory under .config
        let config_dir = home_dir().context("Failed to find home directory")?.join(".config");
        let config_path = config_dir.join(CONFIG_FILE_NAME);
        if config_path
            .try_exists()
            .context("Failed to check home config file existence")?
        {
            tracing::info!("Found config file: {}", config_path.display());
            return Ok(config_path);
        }

        // If neither location has the config file, return an error
        Err(anyhow!(
            "Config file not found in current directory or home config directory"
        ))
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", "Config:".bold())?;
        write!(f, "  enable_s3_backup: {}", utils::colorize_bool(self.enable_s3_backup))
    }
}
