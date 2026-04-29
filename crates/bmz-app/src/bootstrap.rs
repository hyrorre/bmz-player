use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::config::app_config::AppConfig;
use crate::config::load::{load_app_config, load_profile_config};
use crate::config::profile_config::ProfileConfig;
use crate::config::save::{save_app_config, save_profile_config};
use crate::paths::{AppPaths, ProfilePaths, resolve_app_paths, resolve_profile_paths};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::score_db::ScoreDatabase;

pub struct BootstrappedApp {
    pub app_config: AppConfig,
    pub profile_config: ProfileConfig,
    pub app_paths: AppPaths,
    pub profile_paths: ProfilePaths,
    pub library_db: LibraryDatabase,
    pub score_db: ScoreDatabase,
}

pub fn bootstrap() -> Result<BootstrappedApp> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    let app_config = load_or_create_app_config(&app_paths)?;
    let profile_paths = resolve_profile_paths(&app_paths, &app_config.active_profile);
    profile_paths.ensure_dirs()?;
    let profile_config = load_or_create_profile_config(&profile_paths, &app_config.active_profile)?;

    crate::storage::migration::migrate_library_db(&app_paths.library_db)?;
    crate::storage::migration::migrate_score_db(&profile_paths.score_db)?;

    let library_db = LibraryDatabase::open(&app_paths.library_db)?;
    let score_db = ScoreDatabase::open(&profile_paths.score_db)?;

    Ok(BootstrappedApp {
        app_config,
        profile_config,
        app_paths,
        profile_paths,
        library_db,
        score_db,
    })
}

fn load_or_create_app_config(paths: &AppPaths) -> Result<AppConfig> {
    if paths.config_toml.exists() {
        return load_app_config(&paths.config_toml);
    }

    let config = AppConfig::default();
    save_app_config(&paths.config_toml, &config)?;
    Ok(config)
}

fn load_or_create_profile_config(paths: &ProfilePaths, profile_id: &str) -> Result<ProfileConfig> {
    if paths.profile_toml.exists() {
        return load_profile_config(&paths.profile_toml);
    }

    let now = now_unix_seconds();
    let config = ProfileConfig::new_default(profile_id, "Default", now);
    save_profile_config(&paths.profile_toml, &config)?;
    Ok(config)
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
