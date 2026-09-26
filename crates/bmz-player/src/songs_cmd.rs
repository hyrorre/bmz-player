use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::SongsCommand;
use crate::config::app_config::{AppConfig, PathEntry};
use crate::config::load::load_app_config;
use crate::config::save::save_app_config;
use crate::paths::{AppPaths, normalize_library_path, resolve_app_paths};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::migrate_library_db;
use crate::storage::scan::{
    ScanProgress, ScanReport, scan_song_roots, scan_song_roots_with_progress,
};

pub fn run_songs_command(cmd: SongsCommand) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    run_songs_command_with_paths(cmd, &app_paths)
}

pub fn run_songs_command_with_paths(cmd: SongsCommand, app_paths: &AppPaths) -> Result<()> {
    match cmd {
        SongsCommand::Add { path, recursive, enabled } => {
            add_song_root(app_paths, &path, recursive, enabled)
        }
        SongsCommand::List => list_song_roots(app_paths),
        SongsCommand::Load { target, use_everything } => {
            load_songs(app_paths, target.as_deref(), false, use_everything)
        }
        SongsCommand::Reload { target, use_everything } => {
            load_songs(app_paths, target.as_deref(), true, use_everything)
        }
    }
}

/// 曲ルート一覧へ 1 件追加する。重複パスはエラー。
pub fn add_song_root_entry(
    roots: &mut Vec<PathEntry>,
    path: &str,
    recursive: bool,
    enabled: bool,
) -> Result<()> {
    let path = normalize_library_path(path);
    if roots.iter().any(|root| normalize_library_path(&root.path) == path) {
        bail!("already configured: {path}");
    }
    roots.push(PathEntry { path, recursive, enabled });
    Ok(())
}

/// 曲ルート一覧から index 番目を削除する。
pub fn remove_song_root_entry(roots: &mut Vec<PathEntry>, index: usize) {
    if index < roots.len() {
        roots.remove(index);
    }
}

/// 明示的な自動DLの保存先を、再帰スキャン可能な有効ルートにする。
/// 有効な再帰ルートの配下なら追加せず、保存先自身の既存設定は再利用する。
pub(crate) fn ensure_download_song_root(roots: &mut Vec<PathEntry>, path: &str) -> bool {
    let path = normalize_library_path(path).trim_end_matches('/').to_string();
    if roots.iter().any(|root| {
        let root_path = normalize_library_path(&root.path).trim_end_matches('/').to_string();
        root.enabled
            && root.recursive
            && (path == root_path || path.starts_with(&format!("{root_path}/")))
    }) {
        return false;
    }
    if let Some(root) = roots
        .iter_mut()
        .find(|root| normalize_library_path(&root.path).trim_end_matches('/') == path)
    {
        root.path = path;
        root.enabled = true;
        root.recursive = true;
    } else {
        roots.push(PathEntry { path, enabled: true, recursive: true });
    }
    true
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

/// Authoritative scope, including disabled roots and the bundled sample.
pub(crate) fn configured_library_roots(config: &AppConfig, paths: &AppPaths) -> Vec<PathEntry> {
    let mut roots = config.songs.roots.clone();
    let sample = paths.resource_dir.join("songs/sample-playable");
    let sample = sample.canonicalize().unwrap_or(sample);
    let path = normalize_library_path(&sample.to_string_lossy());
    if !roots.iter().any(|root| normalize_library_path(&root.path) == path) {
        roots.push(PathEntry { path, enabled: true, recursive: true });
    }
    roots
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

fn add_song_root(app_paths: &AppPaths, path: &str, recursive: bool, enabled: bool) -> Result<()> {
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

fn list_song_roots(app_paths: &AppPaths) -> Result<()> {
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

fn load_songs(
    app_paths: &AppPaths,
    target: Option<&str>,
    force: bool,
    use_everything: Option<bool>,
) -> Result<()> {
    app_paths.ensure_dirs()?;

    // Missing/unreadable configuration is not an explicit empty library.
    let app_config = load_app_config(&app_paths.config_toml)?;

    let library_roots = configured_library_roots(&app_config, app_paths);
    let roots = if target.is_none() {
        library_roots.clone()
    } else {
        resolve_song_scan_target(target, &app_config.songs.roots)?
    };

    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;
    library_db.set_configured_song_roots(&library_roots)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut scan_config = app_config.scan.clone();
    if let Some(use_everything) = use_everything {
        scan_config.use_everything = use_everything;
    }

    let verb = if force { "Reloading" } else { "Scanning" };
    let discovery = if scan_config.use_everything { "everything" } else { "native" };
    println!("{verb} {} root(s) with {discovery} discovery...", roots.len());
    let mut report = scan_songs(&mut library_db, &roots, &scan_config, now, force)
        .with_context(|| format!("failed to scan song roots (force={force})"))?;
    if target.is_none() {
        report.summary.removed_files +=
            library_db.reconcile_configured_song_roots(&library_roots)?;
    } else {
        library_db.register_partial_song_roots(&roots)?;
    }

    let s = &report.summary;
    println!("Removed {} obsolete file registration(s)", s.removed_files);
    println!(
        "Done: {} imported, {} skipped, {} failed ({} warnings), {} filesystem path(s) skipped \
         across {} file(s) in {} root(s) ({} unreadable)",
        s.imported,
        s.skipped,
        s.failed,
        s.warnings,
        s.discovery_skipped,
        s.files_seen,
        s.roots_seen,
        s.roots_unreadable
    );

    let t = &report.timing;
    println!(
        "Timing: total={}ms discovery={}ms fingerprint={}ms skip={}ms parse={}ms write={}ms",
        t.total_ms, t.discovery_ms, t.fingerprint_ms, t.skip_check_ms, t.parse_ms, t.write_ms
    );
    println!(
        "Discovery backends: everything={} native={} fallback={}",
        s.everything_discovery_roots, s.native_discovery_roots, s.everything_fallback_roots
    );

    for issue in &report.discovery_issues {
        println!(
            "  SKIP FS: {} — operation={} kind={:?} os_error={:?}: {}",
            issue.path.display(),
            issue.operation.as_str(),
            issue.error_kind,
            issue.raw_os_error,
            issue.message
        );
    }

    for failure in &report.failures {
        println!("  FAIL BMS: {} — {}", failure.path.display(), failure.message);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_partial_scan_remains_usable_across_reopen_until_full_sync() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        drop(data.boot());
        let partial = data.paths.cache_dir.join("partial-songs");
        std::fs::create_dir_all(&partial).unwrap();
        let path = partial.join("song.bms");
        std::fs::write(&path, "#TITLE Partial scan\n#BPM 120\n#00011:01\n").unwrap();
        let config_before = std::fs::read(&data.paths.config_toml).unwrap();

        load_songs(&data.paths, partial.to_str(), false, Some(false)).unwrap();
        let db = LibraryDatabase::open(&data.paths.library_db).unwrap();
        let id = db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
        let chart = db.verified_chart_source(id).unwrap().chart;
        assert!(db.registered_chart_sources(&[&chart]).unwrap().contains_key(&id));
        assert_eq!(db.available_chart_id_by_sha256(chart.sha256).unwrap(), Some(id));
        drop(db);

        // Startup republishes the saved configuration on a new connection.
        let db = LibraryDatabase::open(&data.paths.library_db).unwrap();
        let app_config = load_app_config(&data.paths.config_toml).unwrap();
        let configured = configured_library_roots(&app_config, &data.paths);
        db.set_configured_song_roots(&configured).unwrap();
        assert_eq!(db.configured_song_roots().unwrap(), Some(configured));
        assert!(db.verified_chart_source(id).is_ok());
        drop(db);
        // Unchanged files must remain usable on a second partial scan too.
        load_songs(&data.paths, partial.to_str(), false, Some(false)).unwrap();
        assert_eq!(std::fs::read(&data.paths.config_toml).unwrap(), config_before);

        load_songs(&data.paths, None, false, Some(false)).unwrap();
        let db = LibraryDatabase::open(&data.paths.library_db).unwrap();
        assert_eq!(db.chart_id_by_chart_file_path(&path).unwrap(), None);
        assert!(db.available_chart_id_by_sha256(chart.sha256).unwrap().is_none());
        assert!(path.is_file(), "full sync only removes registrations");
    }

    fn sample_roots() -> Vec<PathEntry> {
        vec![
            PathEntry { path: "/music/beatmania".to_string(), enabled: true, recursive: true },
            PathEntry { path: "/archive/beatmania".to_string(), enabled: true, recursive: true },
            PathEntry { path: "/other/songs".to_string(), enabled: false, recursive: true },
        ]
    }

    #[test]
    fn download_song_root_reuses_normalized_disabled_entry() {
        let mut roots = vec![PathEntry {
            path: r"\\?\C:\data\songs\ipfs\".to_string(),
            enabled: false,
            recursive: false,
        }];
        assert!(ensure_download_song_root(&mut roots, "C:/data/songs/ipfs"));
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "C:/data/songs/ipfs");
        assert!(roots[0].enabled && roots[0].recursive);
        assert!(!ensure_download_song_root(&mut roots, "C:/data/songs/ipfs"));
    }

    #[test]
    fn download_song_root_only_reuses_enabled_recursive_ancestors() {
        for (parent, enabled, recursive, changes) in [
            ("/data/songs/", true, true, false),
            ("/data/songs", false, true, true),
            ("/data/songs", true, false, true),
            ("/data/song", true, true, true),
        ] {
            let mut roots = vec![PathEntry { path: parent.into(), enabled, recursive }];
            assert_eq!(ensure_download_song_root(&mut roots, "/data/songs/http"), changes);
            assert_eq!(roots.len(), if changes { 2 } else { 1 });
            assert_eq!(roots[0].enabled, enabled);
            assert_eq!(roots[0].recursive, recursive);
        }
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

    #[test]
    fn add_song_root_normalizes_and_rejects_separator_variant() {
        let mut roots = Vec::new();

        add_song_root_entry(&mut roots, r"\\?\G:\BMS", true, true).unwrap();

        assert_eq!(roots[0].path, "G:/BMS");
        let error = add_song_root_entry(&mut roots, "G:/BMS", false, false).unwrap_err();
        assert!(error.to_string().contains("already configured: G:/BMS"));
    }
}
