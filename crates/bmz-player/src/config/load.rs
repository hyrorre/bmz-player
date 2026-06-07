use std::path::Path;

use anyhow::Result;

use super::app_config::AppConfig;
use super::play_input::{normalize_profile_input, validate_play_inherit_config};
use super::profile_config::ProfileConfig;

pub fn load_app_config(path: &Path) -> Result<AppConfig> {
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

pub fn load_profile_config(path: &Path) -> Result<ProfileConfig> {
    let text = std::fs::read_to_string(path)?;
    let mut config: ProfileConfig = toml::from_str(&text)?;
    normalize_profile_input(&mut config.input);
    validate_play_inherit_config(&config.input).map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(config)
}
