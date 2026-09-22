use super::*;
use rusqlite::Connection;

pub(crate) struct ProfileTestDir {
    pub paths: AppPaths,
    root: PathBuf,
}

impl ProfileTestDir {
    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "bmz-profile-switch-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
        ));
        let paths = AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );
        Self { paths, root }
    }

    pub(crate) fn boot(&self) -> BootstrappedApp {
        bootstrap_with_paths_mode(self.paths.clone(), false, None).unwrap()
    }
}

impl Drop for ProfileTestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn mark_profile(boot: &BootstrappedApp, value: &str) {
    for conn in [boot.score_db.conn(), boot.collection_db.conn(), boot.network_db.conn()] {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS switch_test (value TEXT NOT NULL)").unwrap();
        conn.execute("INSERT INTO switch_test VALUES (?1)", [value]).unwrap();
    }
}

fn markers(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT value FROM switch_test ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn profile_switch_round_trip_isolates_all_profile_databases_and_preserves_library() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    crate::profile_cmd::create_profile(&data.paths, "other", Some("Other"), false).unwrap();
    mark_profile(&boot, "default-before");
    boot.library_db
        .conn()
        .execute_batch("CREATE TEMP TABLE shared_library_marker (id INTEGER)")
        .unwrap();
    let mut other = PreparedProfile::load(&data.paths, "other").unwrap();
    other.config.audio_mix.master_volume = 25;
    save_profile_config(&other.paths.profile_toml, &other.config).unwrap();
    boot.activate_prepared_profile(other).unwrap();
    assert_eq!(boot.profile_config.id, "other");
    assert_eq!(boot.profile_config.audio_mix.master_volume, 25);
    assert_eq!(boot.profile_paths.replay_dir, data.paths.profiles_dir.join("other/replay"));
    mark_profile(&boot, "other-only");
    for conn in [boot.score_db.conn(), boot.collection_db.conn(), boot.network_db.conn()] {
        assert_eq!(markers(conn), ["other-only"]);
    }
    boot.activate_prepared_profile(PreparedProfile::load(&data.paths, "default").unwrap()).unwrap();
    mark_profile(&boot, "default-after");
    for conn in [boot.score_db.conn(), boot.collection_db.conn(), boot.network_db.conn()] {
        assert_eq!(markers(conn), ["default-before", "default-after"]);
    }
    boot.library_db.conn().execute("INSERT INTO shared_library_marker VALUES (1)", []).unwrap();
    assert_eq!(load_app_config(&data.paths.config_toml).unwrap().active_profile, "default");
    assert_eq!(
        PreparedProfile::load(&data.paths, "other").unwrap().config.audio_mix.master_volume,
        25
    );
}

#[test]
fn profile_switch_save_failure_keeps_config_paths_and_database_handles() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    crate::profile_cmd::create_profile(&data.paths, "other", None, false).unwrap();
    mark_profile(&boot, "original");
    let prepared = PreparedProfile::load(&data.paths, "other").unwrap();
    // rename先をdirectoryにして、権限や実行ユーザーによらず保存を失敗させる。
    boot.app_paths.config_toml = data.paths.data_dir.join("config-is-directory");
    std::fs::create_dir(&boot.app_paths.config_toml).unwrap();
    assert!(boot.activate_prepared_profile(prepared).is_err());
    assert_eq!(boot.profile_config.id, "default");
    assert_eq!(boot.app_config.active_profile, "default");
    assert_eq!(boot.profile_paths.root_dir, data.paths.profiles_dir.join("default"));
    for conn in [boot.score_db.conn(), boot.collection_db.conn(), boot.network_db.conn()] {
        assert_eq!(markers(conn), ["original"]);
    }
    assert_eq!(load_app_config(&data.paths.config_toml).unwrap().active_profile, "default");
}

#[test]
fn profile_switch_rejects_missing_corrupt_mismatched_and_unopenable_profiles() {
    let data = ProfileTestDir::new();
    let boot = data.boot();
    assert!(PreparedProfile::load(&data.paths, "missing").is_err());
    assert!(!data.paths.profiles_dir.join("missing").exists());
    assert!(PreparedProfile::load(&data.paths, "../default").is_err());
    crate::profile_cmd::create_profile(&data.paths, "broken", None, false).unwrap();
    let paths = resolve_profile_paths(&data.paths, "broken").unwrap();
    std::fs::write(&paths.profile_toml, "invalid TOML [").unwrap();
    assert!(PreparedProfile::load(&data.paths, "broken").is_err());
    let mut config = ProfileConfig::new_default("wrong-id", "Test", 0);
    save_profile_config(&paths.profile_toml, &config).unwrap();
    assert!(PreparedProfile::load(&data.paths, "broken").is_err());
    config.id = "broken".into();
    save_profile_config(&paths.profile_toml, &config).unwrap();
    std::fs::remove_file(&paths.score_db).unwrap();
    std::fs::create_dir(&paths.score_db).unwrap();
    assert!(PreparedProfile::load(&data.paths, "broken").is_err());
    assert_eq!(boot.profile_config.id, "default");
    assert_eq!(load_app_config(&data.paths.config_toml).unwrap().active_profile, "default");
}

#[test]
fn explicit_profile_startup_does_not_change_default_until_switch_commits() {
    let data = ProfileTestDir::new();
    drop(data.boot());
    crate::profile_cmd::create_profile(&data.paths, "other", None, false).unwrap();
    let mut boot = bootstrap_with_paths_mode(data.paths.clone(), false, Some("other")).unwrap();
    assert_eq!(boot.profile_config.id, "other");
    assert_eq!(load_app_config(&data.paths.config_toml).unwrap().active_profile, "default");
    boot.activate_prepared_profile(PreparedProfile::load(&data.paths, "other").unwrap()).unwrap();
    assert_eq!(load_app_config(&data.paths.config_toml).unwrap().active_profile, "other");
}
