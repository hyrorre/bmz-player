use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use bmz_core::time::TimeUs;

use crate::config::app_config::{AppConfig, PathEntry};
use crate::config::load::{load_app_config, load_profile_config};
use crate::config::profile_config::ProfileConfig;
use crate::config::save::{save_app_config, save_profile_config};
use crate::paths::{
    AppPaths, ProfilePaths, normalize_library_path, resolve_app_paths, resolve_profile_paths,
};
use crate::screens::play_start::{
    PlayStartOptions, StartedInputPlaySession, start_running_play_session_for_chart,
    start_running_play_session_for_chart_with_input_backend,
    start_running_play_session_for_chart_with_winit_input,
};
use crate::storage::collection_db::CollectionDatabase;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::network_db::NetworkDatabase;
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
    pub network_db: NetworkDatabase,
    pub startup_scan: Option<ScanReport>,
}

/// 切り替え先を現在のprofileに触れずに準備する。library DBや曲スキャンは共有する。
pub(crate) struct PreparedProfile {
    pub config: ProfileConfig,
    pub paths: ProfilePaths,
    pub collection_db: CollectionDatabase,
    pub score_db: ScoreDatabase,
    pub network_db: NetworkDatabase,
}

#[cfg(test)]
pub(crate) mod profile_tests;

impl PreparedProfile {
    pub(crate) fn load(app_paths: &AppPaths, id: &str) -> Result<Self> {
        let paths = resolve_profile_paths(app_paths, id)?;
        let config = load_profile_config(&paths.profile_toml)
            .with_context(|| format!("failed to load profile {id}"))?;
        if config.id != id {
            bail!("profile id mismatch: expected {id}, found {}", config.id);
        }
        Self::open(config, paths)
    }

    fn open(config: ProfileConfig, paths: ProfilePaths) -> Result<Self> {
        paths.ensure_dirs()?;
        crate::storage::migration::migrate_collection_db(&paths.collection_db)?;
        crate::storage::migration::migrate_score_db(&paths.score_db)?;
        crate::storage::migration::migrate_network_db(&paths.network_db)?;
        let collection_db = CollectionDatabase::open(&paths.collection_db)?;
        let score_db = ScoreDatabase::open(&paths.score_db)?;
        let network_db = NetworkDatabase::open(&paths.network_db)?;
        Ok(Self { config, paths, collection_db, score_db, network_db })
    }
}

pub struct ViewerBootstrap {
    pub app: BootstrappedApp,
    pub chart_path: PathBuf,
    pub chart_id: i64,
    pub start_time: TimeUs,
    pub cleanup: ViewerLibraryCleanup,
}

pub struct ViewerLibraryCleanup {
    path: PathBuf,
}

impl Drop for ViewerLibraryCleanup {
    fn drop(&mut self) {
        for path in sqlite_files(&self.path) {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %path.display(), %error, "failed to remove viewer library database");
            }
        }
    }
}

impl BootstrappedApp {
    /// 永続化に失敗した場合は実行中のprofile/DBを一切入れ替えない。
    pub(crate) fn activate_prepared_profile(&mut self, profile: PreparedProfile) -> Result<()> {
        let mut app_config = self.app_config.clone();
        app_config.active_profile = profile.config.id.clone();
        save_app_config(&self.app_paths.config_toml, &app_config)?;
        crate::ir::secret_store::set_store_mode(
            &profile.paths.root_dir,
            profile.config.ir.credential_store,
        );
        self.app_config = app_config;
        self.profile_config = profile.config;
        self.profile_paths = profile.paths;
        self.collection_db = profile.collection_db;
        self.score_db = profile.score_db;
        self.network_db = profile.network_db;
        Ok(())
    }

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
    ) -> Result<StartedInputPlaySession> {
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
    bootstrap_with_paths(app_paths)
}

/// 起動前loggerと同じ解決済みpathを使って通常bootstrapを行う。
pub fn bootstrap_with_paths(app_paths: AppPaths) -> Result<BootstrappedApp> {
    bootstrap_with_paths_mode(app_paths, true, None)
}

/// `active_profile`を書き換えず、指定profileで通常起動する。
pub fn bootstrap_with_paths_profile(
    app_paths: AppPaths,
    profile_id: Option<&str>,
) -> Result<BootstrappedApp> {
    bootstrap_with_paths_mode(app_paths, true, profile_id)
}

pub fn bootstrap_viewer_with_paths(
    mut app_paths: AppPaths,
    chart_path: &Path,
    start_measure: u32,
    bms_random_seed: u64,
    profile_id: Option<&str>,
) -> Result<ViewerBootstrap> {
    let chart_path = chart_path
        .canonicalize()
        .with_context(|| format!("failed to resolve viewer chart: {}", chart_path.display()))?;
    if !crate::storage::scan::is_chart_file(&chart_path) {
        bail!("unsupported viewer chart extension: {}", chart_path.display());
    }
    let library_path = viewer_library_path();
    let cleanup = ViewerLibraryCleanup { path: library_path.clone() };
    app_paths.library_db = library_path;
    let mut app = bootstrap_with_paths_mode(app_paths, false, profile_id)?;
    let imported = crate::storage::import::import_chart_file(
        &mut app.library_db,
        &chart_path,
        None,
        Some(bms_random_seed),
        now_unix_seconds(),
    )?;
    let start_time = viewer_measure_start_time(&imported.chart, start_measure)?;
    Ok(ViewerBootstrap { app, chart_path, chart_id: imported.chart_id, start_time, cleanup })
}

fn bootstrap_with_paths_mode(
    app_paths: AppPaths,
    startup_scan_enabled: bool,
    profile_id_override: Option<&str>,
) -> Result<BootstrappedApp> {
    let bootstrap_started_at = Instant::now();
    app_paths.ensure_required_dirs()?;

    let config_started_at = Instant::now();
    let had_saved_config = app_paths.config_toml.is_file();
    let mut app_config = load_or_create_app_config(&app_paths)?;
    if let Some(sample_root) = bundled_sample_song_root(&app_paths) {
        let sample_root_str = normalize_library_path(&sample_root.to_string_lossy());
        if !app_config.songs.roots.iter().any(|r| r.path == sample_root_str) {
            app_config.songs.roots.push(PathEntry {
                path: sample_root_str,
                enabled: true,
                recursive: true,
            });
        }
    }
    let profile_id = profile_id_override.unwrap_or(&app_config.active_profile).to_string();
    let profile_paths = resolve_profile_paths(&app_paths, &profile_id)?;
    if profile_id_override.is_some() && !profile_paths.profile_toml.is_file() {
        bail!("profile not found: {profile_id}");
    }
    profile_paths.ensure_dirs()?;
    let profile_config = if profile_id_override.is_some() {
        load_profile_config(&profile_paths.profile_toml)
            .with_context(|| format!("failed to load profile {profile_id}"))?
    } else {
        load_or_create_profile_config(&profile_paths, &profile_id)?
    };
    tracing::info!(
        profile_id,
        profile_override = profile_id_override.is_some(),
        config_ms = config_started_at.elapsed().as_millis(),
        "startup configuration loaded"
    );
    // IR 秘密情報の保存先 (File / OS credential store) をprofile rootに紐付ける。
    crate::ir::secret_store::set_store_mode(
        &profile_paths.root_dir,
        profile_config.ir.credential_store,
    );

    let migration_started_at = Instant::now();
    crate::storage::migration::migrate_library_db(&app_paths.library_db)?;
    let library_migration_ms = migration_started_at.elapsed().as_millis();
    let profile_started_at = Instant::now();
    let profile = PreparedProfile::open(profile_config, profile_paths)?;
    tracing::info!(
        library_migration_ms,
        profile_migration_ms = profile_started_at.elapsed().as_millis(),
        "startup database migrations complete"
    );

    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;
    let library_roots = crate::songs_cmd::configured_library_roots(&app_config, &app_paths);
    library_db.set_configured_song_roots(&library_roots)?;
    let bundled_sample_root = bundled_sample_song_root(&app_paths);
    let scan_started_at = Instant::now();
    let scan_roots = if startup_scan_enabled {
        startup_scan_roots(&app_config, bundled_sample_root.as_deref())
    } else {
        Vec::new()
    };
    let mut startup_scan = if scan_roots.is_empty() {
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
    if startup_scan_enabled && app_config.scan.auto_rescan_on_startup && had_saved_config {
        let removed = library_db.reconcile_configured_song_roots(&library_roots)?;
        startup_scan.get_or_insert_with(Default::default).summary.removed_files += removed;
    }
    tracing::info!(
        scan_root_count = scan_roots.len(),
        scan_ms = scan_started_at.elapsed().as_millis(),
        "startup song scan complete"
    );
    let boot = BootstrappedApp {
        app_config,
        profile_config: profile.config,
        app_paths,
        profile_paths: profile.paths,
        library_db,
        collection_db: profile.collection_db,
        score_db: profile.score_db,
        network_db: profile.network_db,
        startup_scan,
    };
    tracing::info!(
        bootstrap_total_ms = bootstrap_started_at.elapsed().as_millis(),
        "startup bootstrap timings"
    );
    Ok(boot)
}

pub(crate) fn viewer_measure_start_time(
    chart: &bmz_chart::model::PlayableChart,
    measure: u32,
) -> Result<TimeUs> {
    let bar_time = chart
        .bar_lines
        .iter()
        .find(|bar| bar.measure == measure)
        .map(|bar| bar.time)
        .or_else(|| (measure == 0).then_some(TimeUs(0)))
        .with_context(|| format!("start measure {measure} is not present in the chart"))?;
    let mut shifted = chart.clone();
    let margin = bmz_chart::start_margin::apply_start_note_margin(&mut shifted);
    Ok(TimeUs(bar_time.0.saturating_add(margin.0)))
}

fn viewer_library_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("bmz-player-viewer-{}-{nonce}.db", std::process::id()))
}

fn sqlite_files(path: &Path) -> [PathBuf; 3] {
    let append = |suffix: &str| {
        let mut value = OsString::from(path.as_os_str());
        value.push(suffix);
        PathBuf::from(value)
    };
    [path.to_path_buf(), append("-wal"), append("-shm")]
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
        let sample_root = normalize_library_path(&sample_root.to_string_lossy());
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
    let mut config = ProfileConfig::new_default(profile_id, "Default", now);
    config.normalize_play_mode_configs();
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
    use std::io::Write;

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
    fn startup_scan_roots_deduplicates_sample_root_separator_variant() {
        let mut config = AppConfig::default();
        config.scan.auto_rescan_on_startup = true;
        config.songs.roots.push(PathEntry {
            path: "G:/samples".to_string(),
            enabled: true,
            recursive: false,
        });

        let roots = startup_scan_roots(&config, Some(Path::new(r"\\?\G:\samples")));

        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "G:/samples");
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

    #[test]
    fn viewer_start_measure_uses_source_measure_after_start_margin() {
        let path = std::env::temp_dir().join(format!(
            "bmz-viewer-measure-{}-{}.bms",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"#TITLE Viewer Measure\n#BPM 120\n#00011:01\n#00212:01\n").unwrap();
        file.sync_all().unwrap();

        let chart = import_bms_chart(&path, None, true).unwrap().chart;

        assert_eq!(viewer_measure_start_time(&chart, 0).unwrap(), TimeUs(1_000_000));
        assert_eq!(viewer_measure_start_time(&chart, 2).unwrap(), TimeUs(5_000_000));
        assert!(viewer_measure_start_time(&chart, 99).is_err());

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn viewer_profile_override_loads_existing_profile_without_changing_active_profile() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root =
            std::env::temp_dir().join(format!("bmz-viewer-profile-{}-{nonce}", std::process::id()));
        let app_paths = AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );
        app_paths.ensure_required_dirs().unwrap();

        let app_config = AppConfig::default();
        save_app_config(&app_paths.config_toml, &app_config).unwrap();
        let alt_paths = resolve_profile_paths(&app_paths, "alt").unwrap();
        alt_paths.ensure_dirs().unwrap();
        save_profile_config(&alt_paths.profile_toml, &ProfileConfig::new_default("alt", "Alt", 1))
            .unwrap();

        let boot = bootstrap_with_paths_mode(app_paths.clone(), false, Some("alt")).unwrap();
        assert_eq!(boot.profile_config.id, "alt");
        assert_eq!(boot.profile_paths.root_dir, alt_paths.root_dir);
        drop(boot);

        let saved = load_app_config(&app_paths.config_toml).unwrap();
        assert_eq!(saved.active_profile, "default");
        assert!(bootstrap_with_paths_mode(app_paths.clone(), false, Some("missing")).is_err());
        assert!(!app_paths.profiles_dir.join("missing").exists());

        let _ = std::fs::remove_dir_all(root);
    }
}
