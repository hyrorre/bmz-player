use std::path::PathBuf;

use anyhow::{Result, bail};

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

pub fn resolve_profile_paths(app: &AppPaths, profile_id: &str) -> Result<ProfilePaths> {
    validate_profile_id(profile_id)?;
    let root_dir = app.profiles_dir.join(profile_id);
    Ok(ProfilePaths {
        profile_toml: root_dir.join("profile.toml"),
        score_db: root_dir.join("score.db"),
        replay_dir: root_dir.join("replay"),
        root_dir,
    })
}

pub fn validate_profile_id(profile_id: &str) -> Result<()> {
    if profile_id.is_empty() {
        bail!("profile id must not be empty");
    }

    if profile_id.len() > 64 {
        bail!("profile id must be 64 bytes or less");
    }

    if !profile_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("profile id may only contain ASCII letters, digits, '_' and '-'");
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_paths_are_rooted_under_profiles_dir() {
        let app = AppPaths {
            data_dir: PathBuf::from("data"),
            config_toml: PathBuf::from("data/config.toml"),
            library_db: PathBuf::from("data/library.db"),
            profiles_dir: PathBuf::from("data/profiles"),
            cache_dir: PathBuf::from("data/cache"),
            logs_dir: PathBuf::from("data/logs"),
        };

        let paths = resolve_profile_paths(&app, "default-1").unwrap();

        assert_eq!(paths.root_dir, PathBuf::from("data/profiles/default-1"));
        assert_eq!(paths.score_db, PathBuf::from("data/profiles/default-1/score.db"));
    }

    #[test]
    fn profile_id_rejects_path_traversal() {
        assert!(validate_profile_id("../default").is_err());
        assert!(validate_profile_id("profile/name").is_err());
        assert!(validate_profile_id("").is_err());
        assert!(validate_profile_id("default_1-2").is_ok());
    }
}
