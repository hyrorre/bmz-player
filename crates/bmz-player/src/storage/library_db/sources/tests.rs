use super::*;
use crate::config::app_config::AppConfig;
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
        let dir = std::env::temp_dir().join(format!("bmz-source-{}-{stamp}", std::process::id()));
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
            "sl0 iteration={iteration} entries={} previous_ms={:.3} batched_ms={:.3}",
            batched.len(),
            previous_time.as_secs_f64() * 1000.0,
            batched_time.as_secs_f64() * 1000.0
        );
    }
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
