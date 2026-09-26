use super::*;
use crate::config::app_config::{AppConfig, PathEntry};
use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};
use crate::storage::scan::scan_song_roots;
use bmz_core::course::{CourseConstraints, CourseDefinition, CourseEntry, CourseKind};

struct Fixture {
    db: LibraryDatabase,
    dir: PathBuf,
    roots: Vec<PathEntry>,
}
impl Fixture {
    fn new() -> Self {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("bmz-source-{}-{stamp}", std::process::id()));
        let mut roots = Vec::new();
        for name in ["old", "active", "disabled"] {
            let folder = dir.join(name);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("song.bms"), "#TITLE Same\n#BPM 120\n#00011:01\n").unwrap();
            roots.push(PathEntry {
                path: folder.to_string_lossy().into_owned(),
                enabled: true,
                recursive: true,
            });
        }
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let mut config = AppConfig::default().scan;
        config.use_everything = false;
        scan_song_roots(&mut db, &roots, &config, 1, false).unwrap();
        Self { db, dir, roots }
    }
    fn id(&self, folder: &str) -> i64 {
        self.db
            .chart_id_by_chart_file_path(&self.dir.join(folder).join("song.bms"))
            .unwrap()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

#[test]
fn partial_scans_do_not_reenable_disabled_roots_or_restore_removed_configured_roots() {
    let mut f = Fixture::new();
    f.roots[2].enabled = false;
    f.db.set_configured_song_roots(&f.roots[1..]).unwrap();
    let old = f.id("old");
    let active = f.id("active");
    let disabled = f.id("disabled");
    let charts = f.db.list_charts_by_ids(&[old, active, disabled]).unwrap();
    assert_eq!(
        f.db.registered_chart_sources(&[&charts[0]]).unwrap()[&charts[0].chart_id].chart.chart_id,
        active
    );

    // Explicitly scanning a parent enables its unconfigured children, but the
    // disabled subtree is still excluded by the saved configuration.
    let parent =
        PathEntry { path: f.dir.to_string_lossy().into_owned(), enabled: true, recursive: true };
    let scan = crate::config::app_config::ScanConfig {
        use_everything: false,
        ..AppConfig::default().scan
    };
    scan_song_roots(&mut f.db, std::slice::from_ref(&parent), &scan, 2, false).unwrap();
    f.db.register_partial_song_roots(std::slice::from_ref(&parent)).unwrap();
    let sources = f.db.registered_chart_sources(&charts.iter().collect::<Vec<_>>()).unwrap();
    assert_eq!(sources[&old].chart.chart_id, old);
    assert_eq!(sources[&active].chart.chart_id, active);
    assert_ne!(sources[&disabled].chart.chart_id, disabled);

    // After promoting the partial target to a setting, removing the setting
    // must not revive the old temporary scope.
    f.db.set_configured_song_roots(&[parent]).unwrap();
    f.db.set_configured_song_roots(&[]).unwrap();
    assert!(f.db.registered_chart_sources(&charts.iter().collect::<Vec<_>>()).unwrap().is_empty());
    assert!(f.db.verified_chart_source(old).is_err());
}

#[cfg(unix)]
#[test]
fn partial_scan_alias_refreshes_its_target_and_survives_offline_scope_publication() {
    let f = Fixture::new();
    f.db.set_configured_song_roots(&[]).unwrap();
    let alias = f.dir.join("alias");
    std::os::unix::fs::symlink(f.dir.join("old"), &alias).unwrap();
    let target =
        PathEntry { path: alias.to_string_lossy().into_owned(), enabled: true, recursive: true };
    f.db.register_partial_song_roots(std::slice::from_ref(&target)).unwrap();
    assert_eq!(f.db.verified_chart_source(f.id("old")).unwrap().chart.chart_id, f.id("old"));

    std::fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(f.dir.join("active"), &alias).unwrap();
    f.db.register_partial_song_roots(std::slice::from_ref(&target)).unwrap();
    std::fs::remove_file(&alias).unwrap();
    f.db.set_configured_song_roots(&[]).unwrap();
    assert_eq!(f.db.verified_chart_source(f.id("old")).unwrap().chart.chart_id, f.id("active"));
    let mut configured = f.roots[1].clone();
    configured.enabled = false;
    f.db.set_configured_song_roots(&[configured]).unwrap();
    assert!(f.db.verified_chart_source(f.id("old")).is_err());
}

#[test]
fn course_metadata_links_survive_unavailable_sources_and_restore_cleared_links() {
    let mut f = Fixture::new();
    let id = f.id("active");
    let chart = f.db.list_charts_by_ids(&[id]).unwrap().pop().unwrap();
    for root in &mut f.roots {
        root.enabled = false;
    }
    f.db.set_configured_song_roots(&f.roots).unwrap();
    std::fs::remove_file(f.dir.join("active/song.bms")).unwrap();
    let mut entries = vec![
        CourseEntry {
            md5: None,
            sha256: Some(hash_to_hex(&chart.sha256)),
            title_hint: String::new(),
            chart_id: Some(id),
        },
        CourseEntry {
            md5: Some(hash_to_hex(&chart.md5)),
            sha256: None,
            title_hint: String::new(),
            chart_id: None,
        },
    ];
    entries.push(CourseEntry { sha256: Some("ab".repeat(32)), ..entries[0].clone() });
    let definition = CourseDefinition {
        key: "unavailable".into(),
        title: "Unavailable".into(),
        kind: CourseKind::Course,
        entries,
        constraints: Default::default(),
        trophies: Vec::new(),
        release: true,
    };
    let course = f.db.upsert_course("test", &definition, 0, 1).unwrap();
    let entries = f.db.list_course_entries(course).unwrap();
    assert_eq!(entries[0].entry.chart_id, Some(id));
    assert!(entries[1].entry.chart_id.is_some());
    assert_eq!(entries[2].entry.chart_id, None, "never reuse a mismatched metadata link");
    assert!(f.db.available_chart_source(id).unwrap().is_none());
    assert!(f.db.verified_chart_source(id).is_err());

    f.db.conn
        .execute("UPDATE course_entries SET chart_id = NULL WHERE course_id = ?1", [course])
        .unwrap();
    assert_eq!(f.db.repair_course_entry_chart_links_for_course(course).unwrap(), 2);
    for entry in f.db.list_course_entries(course).unwrap().iter().take(2) {
        let linked = entry.entry.chart_id.unwrap();
        assert_eq!(f.db.chart_sha256_by_chart_id(linked).unwrap(), Some(chart.sha256));
    }
}

#[test]
fn source_fallback_keeps_identity_assets_and_metadata_together() {
    let mut f = Fixture::new();
    let old = f.id("old");
    let active = f.id("active");
    let sha = f.db.chart_sha256_by_chart_id(old).unwrap().unwrap();
    f.roots[2].enabled = false;
    f.db.set_configured_song_roots(&f.roots[1..]).unwrap();
    let resolved = f.db.available_chart_source(old).unwrap().unwrap();
    assert_eq!(resolved.chart.chart_id, active);
    assert_eq!(resolved.chart.folder_path, library_path_key(&f.dir.join("active")));
    assert_eq!(f.db.available_chart_id_by_sha256(sha).unwrap(), Some(active));
    let (source, import) = f.db.load_chart_source(old, BmsRandomSource::Seed(None)).unwrap();
    assert_eq!(source.chart.chart_id, active);
    assert_eq!(import.chart.identity.file_sha256, sha);
    // Course links move to the usable copy, and full cleanup preserves the definition.
    let definition = CourseDefinition {
        key: "test".into(),
        title: "Course".into(),
        kind: CourseKind::Dan,
        entries: vec![CourseEntry {
            md5: None,
            sha256: Some(hash_to_hex(&sha)),
            title_hint: "Same".into(),
            chart_id: Some(old),
        }],
        constraints: CourseConstraints::default(),
        trophies: Vec::new(),
        release: true,
    };
    let course = f.db.upsert_course("test", &definition, 0, 1).unwrap();
    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    f.db.upsert_difficulty_table(&FetchedDifficultyTable {
        source_url: "test".into(),
        head_url: "test".into(),
        name: "Test".into(),
        symbol: "★".into(),
        level_order: vec!["1".into()],
        entries: vec![FetchedTableEntry {
            level: "1".into(),
            sha256: hash_to_hex(&sha),
            title: "Same".into(),
            url: "https://example.com/song".into(),
            ..Default::default()
        }],
        courses: vec![],
        fetched_at: 1,
    })
    .unwrap();
    assert_eq!(
        f.db.available_table_entries_at_level("test", None).unwrap()[0]
            .chart
            .as_ref()
            .unwrap()
            .chart_id,
        active
    );
    assert_eq!(f.db.list_course_entries(course).unwrap()[0].entry.chart_id, Some(active));
    f.db.reconcile_configured_song_roots(&f.roots[1..]).unwrap();
    assert_eq!(f.db.list_course_entries(course).unwrap()[0].entry.chart_id, Some(active));
    std::fs::remove_file(f.dir.join("active/song.bms")).unwrap();
    assert!(f.db.available_chart_source(active).unwrap().is_none());
    let table = f.db.available_table_entries_at_level("test", None).unwrap();
    assert!(table[0].chart.is_none());
    assert_eq!(table[0].url, "https://example.com/song");
    assert!(f.db.available_chart_id_by_sha256(sha).unwrap().is_none());
    assert!(f.db.load_chart_source(active, BmsRandomSource::Seed(None)).is_err());
    // IR/import metadata is still available without a readable file.
    assert_eq!(f.db.preferred_chart_metadata(sha).unwrap()[0].chart.chart_id, active);
}

#[test]
#[ignore = "set BMZ_SELECT_BENCH_LIBRARY to a real library DB; read-only timing probe"]
fn benchmark_table_source_resolution() {
    let path = std::env::var_os("BMZ_SELECT_BENCH_LIBRARY").expect("library DB path");
    let db = LibraryDatabase::open_read_only(Path::new(&path)).unwrap();
    let table = db
        .list_difficulty_tables()
        .unwrap()
        .into_iter()
        .find(|table| table.symbol == "sl")
        .expect("Satellite table");
    for iteration in 0..5 {
        let started = std::time::Instant::now();
        let mut previous =
            db.list_table_entries_with_chart_at_level(&table.source_url, Some("0")).unwrap();
        for entry in &mut previous {
            if let Some(chart) = entry.chart.take() {
                entry.chart =
                    db.available_chart_source(chart.chart_id).unwrap().map(|source| source.chart);
            }
        }
        for entry in &previous {
            if let Some(chart) = &entry.chart {
                db.available_chart_source(chart.chart_id).unwrap();
            }
        }
        let previous_time = started.elapsed();
        let started = std::time::Instant::now();
        let batched = db.available_table_entries_at_level(&table.source_url, Some("0")).unwrap();
        let batched_time = started.elapsed();
        let started = std::time::Instant::now();
        let registered =
            db.registered_table_entries_at_level(&table.source_url, Some("0")).unwrap();
        let registered_time = started.elapsed();
        assert_eq!(registered.len(), batched.len());
        assert_eq!(
            previous
                .iter()
                .map(|entry| entry.chart.as_ref().map(|c| c.chart_id))
                .collect::<Vec<_>>(),
            batched
                .iter()
                .map(|entry| entry.chart.as_ref().map(|c| c.chart_id))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "sl0 iteration={iteration} entries={} previous_ms={:.3} batched_ms={:.3} registered_ms={:.3}",
            batched.len(),
            previous_time.as_secs_f64() * 1000.0,
            batched_time.as_secs_f64() * 1000.0,
            registered_time.as_secs_f64() * 1000.0
        );
    }
}

#[test]
fn registered_sources_use_scan_state_but_playback_rechecks_files() {
    let mut f = Fixture::new();
    f.roots[2].enabled = false;
    f.db.set_configured_song_roots(&f.roots).unwrap();
    let old = f.db.available_chart_source(f.id("old")).unwrap().unwrap().chart;
    let active_id = f.id("active");
    std::fs::remove_file(f.dir.join("old/song.bms")).unwrap();
    // Browsing uses the persisted scan snapshot, even after an external deletion.
    assert_eq!(
        f.db.registered_chart_sources(&[&old]).unwrap()[&old.chart_id].chart.chart_id,
        old.chart_id
    );
    assert_eq!(f.db.verified_chart_source(old.chart_id).unwrap().chart.chart_id, active_id);
    let mut scan = AppConfig::default().scan;
    scan.use_everything = false;
    scan_song_roots(&mut f.db, &f.roots, &scan, 2, false).unwrap();
    assert_eq!(
        f.db.registered_chart_sources(&[&old]).unwrap()[&old.chart_id].chart.chart_id,
        active_id
    );
    f.roots[1].enabled = false;
    f.db.set_configured_song_roots(&f.roots).unwrap();
    assert!(f.db.registered_chart_sources(&[&old]).unwrap().is_empty());
}

#[test]
fn batched_sources_preserve_preferred_copies_and_refresh_missing_files() {
    let mut f = Fixture::new();
    f.roots[2].enabled = false;
    f.db.set_configured_song_roots(&f.roots).unwrap();
    let old = f.db.available_chart_source(f.id("old")).unwrap().unwrap().chart;
    let active = f.db.available_chart_source(f.id("active")).unwrap().unwrap().chart;
    let rows = [&old, &active, &old];
    let resolved = f.db.available_chart_sources(&rows).unwrap();
    assert_eq!(resolved[&old.chart_id].chart.chart_id, old.chart_id);
    assert_eq!(resolved[&active.chart_id].chart.chart_id, active.chart_id);
    std::fs::remove_file(f.dir.join("old/song.bms")).unwrap();
    let resolved = f.db.available_chart_sources(&rows).unwrap();
    assert_eq!(resolved[&old.chart_id].chart.chart_id, active.chart_id);
    std::fs::remove_file(f.dir.join("active/song.bms")).unwrap();
    assert!(f.db.available_chart_sources(&rows).unwrap().is_empty());
}

#[test]
fn changed_hash_is_never_played_and_valid_selected_copy_is_preserved() {
    let f = Fixture::new();
    f.db.set_configured_song_roots(&f.roots).unwrap();
    let old = f.id("old");
    assert_eq!(
        f.db.load_chart_source(old, BmsRandomSource::Seed(None)).unwrap().0.chart.chart_id,
        old
    );
    std::fs::write(f.dir.join("old/song.bms"), "#TITLE Different\n#BPM 150\n#00011:01\n").unwrap();
    assert_ne!(f.db.verified_chart_source(old).unwrap().chart.chart_id, old);
    let (source, import) = f.db.load_chart_source(old, BmsRandomSource::Seed(None)).unwrap();
    assert_ne!(source.chart.chart_id, old);
    assert_eq!(import.chart.metadata.title, "Same");
    for name in ["active", "disabled"] {
        std::fs::remove_file(f.dir.join(name).join("song.bms")).unwrap();
    }
    let error = f.db.load_chart_source(old, BmsRandomSource::Seed(None)).unwrap_err();
    assert!(error.to_string().contains("file hash changed"));
    assert!(f.db.verified_chart_source(old).is_err());
}

#[test]
fn metadata_prefers_current_parser_over_obsolete_duplicate() {
    let f = Fixture::new();
    let id = f.id("active");
    let sha = f.db.chart_sha256_by_chart_id(id).unwrap().unwrap();
    f.db.conn.execute("UPDATE charts SET import_version = 0 WHERE id <> ?1", [id]).unwrap();
    let sources = f.db.preferred_chart_metadata(sha).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].chart.chart_id, id);
}

#[test]
fn explicit_viewer_import_can_play_inside_disabled_root_without_becoming_an_automatic_candidate() {
    let mut f = Fixture::new();
    let id = f.id("old");
    let sha = f.db.chart_sha256_by_chart_id(id).unwrap().unwrap();
    for root in &mut f.roots {
        root.enabled = false;
    }
    f.db.set_configured_song_roots(&f.roots).unwrap();
    f.db.conn.execute("UPDATE chart_files SET root_id = NULL WHERE id IN (SELECT chart_file_id FROM chart_file_links WHERE chart_id = ?1)", [id]).unwrap();
    assert!(f.db.available_chart_id_by_sha256(sha).unwrap().is_none());
    assert!(f.db.available_chart_source(id).unwrap().is_none());
    assert_eq!(f.db.verified_chart_source(id).unwrap().chart.chart_id, id);
    assert_eq!(
        f.db.load_chart_source(id, BmsRandomSource::Seed(None)).unwrap().0.chart.chart_id,
        id
    );
}
