use std::path::PathBuf;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_toml: PathBuf,
    pub library_db: PathBuf,
    pub profiles_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProfilePaths {
    pub root_dir: PathBuf,
    pub profile_toml: PathBuf,
    pub score_db: PathBuf,
    pub replay_dir: PathBuf,
}

pub fn resolve_app_paths() -> Result<AppPaths> {
    let data_dir = PathBuf::from("data");
    Ok(AppPaths {
        config_toml: data_dir.join("config.toml"),
        library_db: data_dir.join("library.db"),
        profiles_dir: data_dir.join("profiles"),
        cache_dir: data_dir.join("cache"),
        logs_dir: data_dir.join("logs"),
        data_dir,
    })
}

pub fn resolve_profile_paths(app: &AppPaths, profile_id: &str) -> ProfilePaths {
    let root_dir = app.profiles_dir.join(profile_id);
    ProfilePaths {
        profile_toml: root_dir.join("profile.toml"),
        score_db: root_dir.join("score.db"),
        replay_dir: root_dir.join("replay"),
        root_dir,
    }
}

impl AppPaths {
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.profiles_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(&self.logs_dir)?;
        Ok(())
    }
}

impl ProfilePaths {
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root_dir)?;
        std::fs::create_dir_all(&self.replay_dir)?;
        Ok(())
    }
}
