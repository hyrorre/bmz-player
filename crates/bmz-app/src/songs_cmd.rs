use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::SongsCommand;
use crate::config::app_config::PathEntry;
use crate::config::load::load_app_config;
use crate::config::save::save_app_config;
use crate::paths::resolve_app_paths;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::migrate_library_db;
use crate::storage::scan::{
    ScanProgress, ScanReport, scan_song_roots, scan_song_roots_with_progress,
};

pub fn run_songs_command(cmd: SongsCommand) -> Result<()> {
    match cmd {
        SongsCommand::Add { path, recursive, enabled } => add_song_root(&path, recursive, enabled),
        SongsCommand::List => list_song_roots(),
        SongsCommand::Load { target } => load_songs(target.as_deref(), false),
        SongsCommand::Reload { target } => load_songs(target.as_deref(), true),
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

/// `songs load` / `songs reload` のスキャン対象を解決する。
pub fn resolve_song_scan_target(
    target: Option<&str>,
    roots: &[PathEntry],
) -> Result<Vec<PathEntry>> {
    let Some(target) = target else {
        let enabled: Vec<_> = roots.iter().filter(|r| r.enabled).cloned().collect();
        if enabled.is_empty() {
            bail!("No enabled song roots configured. Use `songs add <PATH>` to add one.");
        }
        return Ok(enabled);
    };

    if looks_like_path(target) {
        let path = Path::new(target);
        if !path.exists() {
            bail!("path does not exist: {target}");
        }
        if !path.is_dir() {
            bail!("path is not a directory: {target}");
        }
        return Ok(vec![PathEntry { path: target.to_string(), enabled: true, recursive: true }]);
    }

    let name = target;
    let matches: Vec<_> =
        roots.iter().filter(|root| root_folder_name(&root.path) == name).collect();
    match matches.len() {
        0 => bail!("no registered root folder named '{name}'"),
        1 => Ok(vec![PathEntry {
            path: matches[0].path.clone(),
            enabled: true,
            recursive: matches[0].recursive,
        }]),
        _ => bail!("multiple root folders named '{name}'; specify PATH instead"),
    }
}

pub fn scan_songs(
    db: &mut LibraryDatabase,
    roots: &[PathEntry],
    scan: &crate::config::app_config::ScanConfig,
    scanned_at: i64,
    force: bool,
) -> Result<ScanReport> {
    scan_song_roots(db, roots, scan, scanned_at, force)
}

pub fn scan_songs_with_progress(
    db: &mut LibraryDatabase,
    roots: &[PathEntry],
    scan: &crate::config::app_config::ScanConfig,
    scanned_at: i64,
    force: bool,
    on_progress: impl FnMut(ScanProgress),
) -> Result<ScanReport> {
    scan_song_roots_with_progress(db, roots, scan, scanned_at, force, on_progress)
}

fn looks_like_path(s: &str) -> bool {
    s.starts_with('/')
        || s.starts_with('\\')
        || s.starts_with('.')
        || s.contains('/')
        || s.contains('\\')
        || s.as_bytes().get(1).is_some_and(|b| *b == b':')
}

fn root_folder_name(path: &str) -> &str {
    Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path)
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
        println!("  (disabled — use songs load or enable it in config.toml)");
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

fn load_songs(target: Option<&str>, force: bool) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    let app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    let roots = resolve_song_scan_target(target, &app_config.songs.roots)?;

    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let verb = if force { "Reloading" } else { "Scanning" };
    println!("{verb} {} root(s)...", roots.len());
    let report = scan_songs(&mut library_db, &roots, &app_config.scan, now, force)
        .with_context(|| format!("failed to scan song roots (force={force})"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_roots() -> Vec<PathEntry> {
        vec![
            PathEntry { path: "/music/beatmania".to_string(), enabled: true, recursive: true },
            PathEntry { path: "/archive/beatmania".to_string(), enabled: true, recursive: true },
            PathEntry { path: "/other/songs".to_string(), enabled: false, recursive: true },
        ]
    }

    #[test]
    fn resolve_target_none_returns_enabled_roots() {
        let roots = resolve_song_scan_target(None, &sample_roots()).unwrap();
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().all(|r| r.enabled));
    }

    #[test]
    fn resolve_target_by_name_matches_single_root() {
        let roots = resolve_song_scan_target(Some("songs"), &sample_roots()).unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/other/songs");
    }

    #[test]
    fn resolve_target_by_duplicate_name_errors() {
        let err = resolve_song_scan_target(Some("beatmania"), &sample_roots()).unwrap_err();
        assert!(err.to_string().contains("multiple root folders named 'beatmania'"));
        assert!(err.to_string().contains("specify PATH instead"));
    }

    #[test]
    fn resolve_target_by_unknown_name_errors() {
        assert!(resolve_song_scan_target(Some("missing"), &sample_roots()).is_err());
    }

    #[test]
    fn looks_like_path_detects_common_forms() {
        assert!(looks_like_path("/abs/path"));
        assert!(looks_like_path("rel/sub"));
        assert!(looks_like_path(".\\relative"));
        assert!(looks_like_path("C:\\music"));
        assert!(!looks_like_path("beatmania"));
    }
}
