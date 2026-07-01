use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::config::app_config::{AppConfig, PathEntry};
use crate::config::load::{load_app_config, load_profile_config};
use crate::config::profile_config::ProfileConfig;
use crate::config::save::{save_app_config, save_profile_config};
use crate::paths::{AppPaths, ProfilePaths, resolve_app_paths, resolve_profile_paths};
use crate::screens::play_start::{
    PlayStartOptions, StartedWinitPlaySession, start_running_play_session_for_chart,
    start_running_play_session_for_chart_with_input_backend,
    start_running_play_session_for_chart_with_winit_input,
};
use crate::storage::collection_db::CollectionDatabase;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::scan::{ScanReport, scan_song_roots};
use crate::storage::score_db::ScoreDatabase;
use bmz_gameplay::input::backend::InputBackend;

pub struct BootstrappedApp {
    pub app_config: AppConfig,
    pub profile_config: ProfileConfig,
    pub app_paths: AppPaths,
    pub profile_paths: ProfilePaths,
    pub library_db: LibraryDatabase,
    pub collection_db: CollectionDatabase,
    pub score_db: ScoreDatabase,
    pub startup_scan: Option<ScanReport>,
}

impl BootstrappedApp {
    pub fn start_play_for_chart(
        &self,
        chart_id: i64,
        options: PlayStartOptions,
    ) -> Result<crate::audio::RunningPlaySession> {
        start_running_play_session_for_chart(
            &self.library_db,
            &self.score_db,
            &self.app_config,
            &self.profile_config,
            chart_id,
            options,
        )
    }

    pub fn start_play_for_chart_with_input_backend(
        &self,
        chart_id: i64,
        options: PlayStartOptions,
        input_backend: Box<dyn InputBackend>,
    ) -> Result<crate::audio::RunningPlaySession> {
        start_running_play_session_for_chart_with_input_backend(
            &self.library_db,
            &self.score_db,
            &self.app_config,
            &self.profile_config,
            chart_id,
            options,
            input_backend,
        )
    }

    pub fn start_play_for_chart_with_winit_input(
        &self,
        chart_id: i64,
        options: PlayStartOptions,
    ) -> Result<StartedWinitPlaySession> {
        start_running_play_session_for_chart_with_winit_input(
            &self.library_db,
            &self.score_db,
            &self.app_config,
            &self.profile_config,
            chart_id,
            options,
        )
    }
}

pub fn bootstrap() -> Result<BootstrappedApp> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    let mut app_config = load_or_create_app_config(&app_paths)?;
    if let Some(sample_root) = bundled_sample_song_root(&app_paths) {
        let sample_root_str = sample_root.to_string_lossy().into_owned();
        if !app_config.songs.roots.iter().any(|r| r.path == sample_root_str) {
            app_config.songs.roots.push(PathEntry {
                path: sample_root_str,
                enabled: true,
                recursive: true,
            });
        }
    }
    let profile_paths = resolve_profile_paths(&app_paths, &app_config.active_profile)?;
    profile_paths.ensure_dirs()?;
    let profile_config = load_or_create_profile_config(&profile_paths, &app_config.active_profile)?;
    // IR 秘密情報の保存先 (File / OS credential store) をプロセス全体へ反映する。
    crate::ir::secret_store::set_store_mode(profile_config.ir.credential_store);

    crate::storage::migration::migrate_library_db(&app_paths.library_db)?;
    crate::storage::migration::migrate_collection_db(&profile_paths.collection_db)?;
    crate::storage::migration::migrate_score_db(&profile_paths.score_db)?;

    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;
    let bundled_sample_root = bundled_sample_song_root(&app_paths);
    let scan_roots = startup_scan_roots(&app_config, bundled_sample_root.as_deref());
    let startup_scan = if scan_roots.is_empty() {
        None
    } else {
        Some(scan_song_roots(
            &mut library_db,
            &scan_roots,
            &app_config.scan,
            now_unix_seconds(),
            false,
        )?)
    };
    let collection_db = CollectionDatabase::open(&profile_paths.collection_db)?;
    let score_db = ScoreDatabase::open(&profile_paths.score_db)?;

    Ok(BootstrappedApp {
        app_config,
        profile_config,
        app_paths,
        profile_paths,
        library_db,
        collection_db,
        score_db,
        startup_scan,
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

fn startup_scan_roots(app_config: &AppConfig, sample_root: Option<&Path>) -> Vec<PathEntry> {
    let mut roots = if app_config.scan.auto_rescan_on_startup {
        app_config.songs.roots.clone()
    } else {
        Vec::new()
    };

    if let Some(sample_root) = sample_root {
        let sample_root = sample_root.to_string_lossy().into_owned();
        if !roots.iter().any(|root| root.path == sample_root) {
            roots.push(PathEntry { path: sample_root, enabled: true, recursive: true });
        }
    }

    roots
}

fn bundled_sample_song_root(app_paths: &AppPaths) -> Option<PathBuf> {
    let root = app_paths.resource_dir.join("songs/sample-playable").canonicalize().ok()?;
    root.is_dir().then_some(root)
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

#[cfg(test)]
mod tests {
    use bmz_chart::import::import_bms_chart;
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    fn test_app_paths() -> AppPaths {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        AppPaths::from_dirs(data.clone(), data.clone(), data.join("cache"), data.join("logs"))
    }

    #[test]
    fn startup_scan_roots_includes_sample_root_when_auto_scan_is_disabled() {
        let config = AppConfig::default();

        let roots = startup_scan_roots(&config, Some(Path::new("/samples")));

        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/samples");
        assert!(roots[0].enabled);
        assert!(roots[0].recursive);
    }

    #[test]
    fn startup_scan_roots_keeps_user_roots_when_auto_scan_is_enabled() {
        let mut config = AppConfig::default();
        config.scan.auto_rescan_on_startup = true;
        config.songs.roots.push(PathEntry {
            path: "/songs".to_string(),
            enabled: true,
            recursive: false,
        });

        let roots = startup_scan_roots(&config, Some(Path::new("/samples")));

        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].path, "/songs");
        assert_eq!(roots[1].path, "/samples");
    }

    #[test]
    fn startup_scan_roots_deduplicates_sample_root() {
        let mut config = AppConfig::default();
        config.scan.auto_rescan_on_startup = true;
        config.songs.roots.push(PathEntry {
            path: "/samples".to_string(),
            enabled: true,
            recursive: false,
        });

        let roots = startup_scan_roots(&config, Some(Path::new("/samples")));

        assert_eq!(roots.len(), 1);
        assert!(!roots[0].recursive);
    }

    #[test]
    fn bundled_sample_root_imports_playable_chart() {
        let app_paths = test_app_paths();
        let sample_root =
            bundled_sample_song_root(&app_paths).expect("sample song root should exist");
        let sample_chart = sample_root.join("sample-playable.bms");
        let import = import_bms_chart(&sample_chart, None, true).unwrap();
        assert!(import.warnings.is_empty());
        assert_eq!(import.chart.sounds.len(), 1);
        assert!(import.chart.sounds[0].path.exists());

        let config = AppConfig::default();
        let roots = startup_scan_roots(&config, Some(&sample_root));
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let report = scan_song_roots(&mut db, &roots, &config.scan, 1_700_000_100, false).unwrap();

        assert_eq!(report.summary.failed, 0);
        assert!(report.summary.imported >= 1);
        let (title, total_notes): (String, u32) = db
            .conn()
            .query_row(
                "SELECT title, total_notes FROM charts WHERE title = 'BMZ Sample Playable'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "BMZ Sample Playable");
        assert!(total_notes > 0);
    }
}
