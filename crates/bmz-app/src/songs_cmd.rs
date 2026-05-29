use anyhow::{Result, bail};

use crate::cli::SongsCommand;
use crate::config::app_config::PathEntry;
use crate::config::load::load_app_config;
use crate::config::save::save_app_config;
use crate::paths::resolve_app_paths;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::migrate_library_db;
use crate::storage::scan::scan_song_roots;

pub fn run_songs_command(cmd: SongsCommand) -> Result<()> {
    match cmd {
        SongsCommand::Add { path, recursive, enabled } => add_song_root(&path, recursive, enabled),
        SongsCommand::List => list_song_roots(),
        SongsCommand::Reload => reload_song_roots(),
    }
}

/// 曲ルート一覧へ 1 件追加する。重複パスはエラー。
pub fn add_song_root_entry(
    roots: &mut Vec<PathEntry>,
    path: &str,
    recursive: bool,
    enabled: bool,
) -> Result<()> {
    if roots.iter().any(|root| root.path == path) {
        bail!("already configured: {path}");
    }
    roots.push(PathEntry { path: path.to_string(), recursive, enabled });
    Ok(())
}

/// 曲ルート一覧から index 番目を削除する。
pub fn remove_song_root_entry(roots: &mut Vec<PathEntry>, index: usize) {
    if index < roots.len() {
        roots.remove(index);
    }
}

fn add_song_root(path: &str, recursive: bool, enabled: bool) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    let mut app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    add_song_root_entry(&mut app_config.songs.roots, path, recursive, enabled)?;
    save_app_config(&app_paths.config_toml, &app_config)?;

    println!("Added {path}");
    if !enabled {
        println!("  (disabled — use songs reload or enable it in config.toml)");
    }
    Ok(())
}

fn list_song_roots() -> Result<()> {
    let app_paths = resolve_app_paths()?;

    let app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    if app_config.songs.roots.is_empty() {
        println!("No song roots configured. Use `songs add <PATH>` to add one.");
        return Ok(());
    }

    for root in &app_config.songs.roots {
        let status = if root.enabled { "enabled" } else { "disabled" };
        let recurse = if root.recursive { "recursive" } else { "flat" };
        println!("{} [{status}, {recurse}]", root.path);
    }
    Ok(())
}

fn reload_song_roots() -> Result<()> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    let app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    let roots: Vec<_> = app_config.songs.roots.iter().filter(|r| r.enabled).cloned().collect();
    if roots.is_empty() {
        println!("No enabled song roots configured. Use `songs add <PATH>` to add one.");
        return Ok(());
    }

    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    println!("Scanning {} root(s)...", roots.len());
    let report = scan_song_roots(&mut library_db, &roots, &app_config.scan, now)?;

    let s = &report.summary;
    println!(
        "Done: {} imported, {} skipped, {} failed ({} warnings) across {} file(s) in {} root(s)",
        s.imported, s.skipped, s.failed, s.warnings, s.files_seen, s.roots_seen
    );

    for failure in &report.failures {
        println!("  FAIL: {} — {}", failure.path.display(), failure.message);
    }

    Ok(())
}
