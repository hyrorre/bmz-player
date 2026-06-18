use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::cli::ProfileCommand;
use crate::config::app_config::AppConfig;
use crate::config::load::{load_app_config, load_profile_config};
use crate::config::profile_config::ProfileConfig;
use crate::config::save::{save_app_config, save_profile_config};
use crate::paths::{AppPaths, ProfilePaths, resolve_app_paths, resolve_profile_paths};
use crate::storage::migration::migrate_score_db;

pub fn run_profile_command(cmd: ProfileCommand) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    match cmd {
        ProfileCommand::List => list_profiles(&app_paths),
        ProfileCommand::Current => show_current_profile(&app_paths),
        ProfileCommand::Use { id } => activate_profile(&app_paths, &id),
        ProfileCommand::Create { id, display_name, activate } => {
            create_profile(&app_paths, &id, display_name.as_deref(), activate)
        }
        ProfileCommand::Copy { source_id, target_id, display_name, activate } => {
            copy_profile(&app_paths, &source_id, &target_id, display_name.as_deref(), activate)
        }
    }
}

fn list_profiles(app_paths: &AppPaths) -> Result<()> {
    let app_config = load_or_default_app_config(app_paths)?;
    let profiles = profile_summaries(app_paths)?;

    if profiles.is_empty() {
        println!("No profiles found. Use `profile create <ID>` to add one.");
        return Ok(());
    }

    for profile in profiles {
        let active = if profile.id == app_config.active_profile { "*" } else { " " };
        println!("{active} {} ({})", profile.id, profile.display_name);
    }
    Ok(())
}

fn show_current_profile(app_paths: &AppPaths) -> Result<()> {
    let app_config = load_or_default_app_config(app_paths)?;
    println!("{}", app_config.active_profile);
    Ok(())
}

fn activate_profile(app_paths: &AppPaths, id: &str) -> Result<()> {
    let profile_paths = resolve_profile_paths(app_paths, id)?;
    ensure_profile_exists(&profile_paths, id)?;

    let mut app_config = load_or_default_app_config(app_paths)?;
    app_config.active_profile = id.to_string();
    save_app_config(&app_paths.config_toml, &app_config)?;

    println!("Active profile: {id}");
    Ok(())
}

fn create_profile(
    app_paths: &AppPaths,
    id: &str,
    display_name: Option<&str>,
    activate: bool,
) -> Result<()> {
    let profile_paths = resolve_profile_paths(app_paths, id)?;
    ensure_profile_can_be_created(&profile_paths, id)?;

    let now = now_unix_seconds();
    let profile = ProfileConfig::new_default(id, display_name.unwrap_or(id), now);
    profile_paths.ensure_dirs()?;
    save_profile_config(&profile_paths.profile_toml, &profile)?;
    migrate_score_db(&profile_paths.score_db)?;

    if activate {
        set_active_profile(app_paths, id)?;
    }

    println!("Created profile: {id}");
    if activate {
        println!("Active profile: {id}");
    }
    Ok(())
}

fn copy_profile(
    app_paths: &AppPaths,
    source_id: &str,
    target_id: &str,
    display_name: Option<&str>,
    activate: bool,
) -> Result<()> {
    if source_id == target_id {
        bail!("source and target profile ids must be different");
    }

    let source_paths = resolve_profile_paths(app_paths, source_id)?;
    ensure_profile_exists(&source_paths, source_id)?;

    let target_paths = resolve_profile_paths(app_paths, target_id)?;
    ensure_profile_can_be_created(&target_paths, target_id)?;

    copy_dir_recursive(&source_paths.root_dir, &target_paths.root_dir).with_context(|| {
        format!(
            "failed to copy profile directory {} to {}",
            source_paths.root_dir.display(),
            target_paths.root_dir.display()
        )
    })?;

    let mut profile = load_profile_config(&target_paths.profile_toml)?;
    let now = now_unix_seconds();
    profile.id = target_id.to_string();
    if let Some(display_name) = display_name {
        profile.display_name = display_name.to_string();
    }
    profile.created_at = now;
    profile.updated_at = now;
    save_profile_config(&target_paths.profile_toml, &profile)?;

    if activate {
        set_active_profile(app_paths, target_id)?;
    }

    println!("Copied profile: {source_id} -> {target_id}");
    if activate {
        println!("Active profile: {target_id}");
    }
    Ok(())
}

fn profile_summaries(app_paths: &AppPaths) -> Result<Vec<crate::storage::profile::ProfileSummary>> {
    let mut profiles = Vec::new();
    if !app_paths.profiles_dir.exists() {
        return Ok(profiles);
    }

    for entry in fs::read_dir(&app_paths.profiles_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        let profile_paths = resolve_profile_paths(app_paths, &id)?;
        if !profile_paths.profile_toml.exists() {
            continue;
        }
        let profile = load_profile_config(&profile_paths.profile_toml).with_context(|| {
            format!("failed to load profile {}", profile_paths.profile_toml.display())
        })?;
        profiles.push(crate::storage::profile::ProfileSummary {
            id: profile.id,
            display_name: profile.display_name,
        });
    }

    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(profiles)
}

fn ensure_profile_exists(profile_paths: &ProfilePaths, id: &str) -> Result<()> {
    if !profile_paths.profile_toml.exists() {
        bail!("profile not found: {id}");
    }
    Ok(())
}

fn ensure_profile_can_be_created(profile_paths: &ProfilePaths, id: &str) -> Result<()> {
    if profile_paths.root_dir.exists() {
        bail!("profile already exists: {id}");
    }
    Ok(())
}

fn set_active_profile(app_paths: &AppPaths, id: &str) -> Result<()> {
    let mut app_config = load_or_default_app_config(app_paths)?;
    app_config.active_profile = id.to_string();
    save_app_config(&app_paths.config_toml, &app_config)
}

fn load_or_default_app_config(app_paths: &AppPaths) -> Result<AppConfig> {
    if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)
    } else {
        Ok(AppConfig::default())
    }
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path).with_context(|| {
                format!("failed to copy {} to {}", source_path.display(), target_path.display())
            })?;
        }
    }
    Ok(())
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app_paths(name: &str) -> AppPaths {
        let root = std::env::temp_dir()
            .join(format!("bmz-player-profile-cmd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        AppPaths {
            config_toml: root.join("config.toml"),
            library_db: root.join("library.db"),
            profiles_dir: root.join("profiles"),
            cache_dir: root.join("cache"),
            logs_dir: root.join("logs"),
            data_dir: root,
        }
    }

    #[test]
    fn create_profile_writes_profile_config_and_can_activate() {
        let app_paths = test_app_paths("create");
        app_paths.ensure_dirs().unwrap();

        create_profile(&app_paths, "alt", Some("Alt Player"), true).unwrap();

        let profile_paths = resolve_profile_paths(&app_paths, "alt").unwrap();
        let profile = load_profile_config(&profile_paths.profile_toml).unwrap();
        let app_config = load_app_config(&app_paths.config_toml).unwrap();

        assert_eq!(profile.id, "alt");
        assert_eq!(profile.display_name, "Alt Player");
        assert_eq!(app_config.active_profile, "alt");
        assert!(profile_paths.score_db.exists());
        assert!(profile_paths.replay_dir.exists());

        let _ = fs::remove_dir_all(&app_paths.data_dir);
    }

    #[test]
    fn copy_profile_copies_files_and_rewrites_identity() {
        let app_paths = test_app_paths("copy");
        app_paths.ensure_dirs().unwrap();
        create_profile(&app_paths, "default", Some("Default"), false).unwrap();

        let source_paths = resolve_profile_paths(&app_paths, "default").unwrap();
        fs::write(source_paths.root_dir.join("note.txt"), "kept").unwrap();
        fs::create_dir_all(source_paths.replay_dir.join("nested")).unwrap();
        fs::write(source_paths.replay_dir.join("nested").join("replay.toml"), "replay").unwrap();

        copy_profile(&app_paths, "default", "alt", Some("Alt"), true).unwrap();

        let target_paths = resolve_profile_paths(&app_paths, "alt").unwrap();
        let profile = load_profile_config(&target_paths.profile_toml).unwrap();
        let app_config = load_app_config(&app_paths.config_toml).unwrap();

        assert_eq!(profile.id, "alt");
        assert_eq!(profile.display_name, "Alt");
        assert_eq!(app_config.active_profile, "alt");
        assert_eq!(fs::read_to_string(target_paths.root_dir.join("note.txt")).unwrap(), "kept");
        assert_eq!(
            fs::read_to_string(target_paths.replay_dir.join("nested").join("replay.toml")).unwrap(),
            "replay"
        );

        let _ = fs::remove_dir_all(&app_paths.data_dir);
    }
}
