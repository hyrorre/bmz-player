use std::io::Write;
use std::path::Path;

use anyhow::Result;

use super::app_config::AppConfig;
use super::profile_config::ProfileConfig;

pub fn save_app_config(path: &Path, config: &AppConfig) -> Result<()> {
    atomic_write(path, &toml::to_string_pretty(config)?)?;
    Ok(())
}

pub fn save_profile_config(path: &Path, profile: &ProfileConfig) -> Result<()> {
    atomic_write(path, &toml::to_string_pretty(profile)?)?;
    Ok(())
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(tmp_path, path)?;
    Ok(())
}
