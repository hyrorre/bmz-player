use std::path::Path;

use anyhow::Result;

use super::app_config::AppConfig;
use super::profile_config::ProfileConfig;

pub fn load_app_config(path: &Path) -> Result<AppConfig> {
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

pub fn load_profile_config(path: &Path) -> Result<ProfileConfig> {
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}
