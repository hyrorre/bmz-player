use super::*;
use crate::config::app_config::{AppConfig, PathEntry};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};
use crate::storage::scan::{ScanReport, scan_song_roots};
use bmz_chart::import::BmsRandomSource;
use std::io::Write;

const BMS: &[u8] = b"#TITLE Archive Song\n#BPM 120\n#WAV01 ../shared/key.wav\n#00011:01\n";
const BMSON: &[u8] = br#"{"version":"1.0.0","info":{"title":"Archive BMSON","artist":"Test","genre":"Test","level":5,"init_bpm":120.0,"judge_rank":100.0,"total":200.0,"resolution":240},"sound_channels":[]}"#;

struct Fixture {
    dir: PathBuf,
    db: LibraryDatabase,
    roots: Vec<PathEntry>,
}
impl Fixture {
    fn new() -> Self {
        let mut random = [0_u8; 16];
        getrandom::getrandom(&mut random).unwrap();
        let id = uuid::Builder::from_random_bytes(random).into_uuid();
        let dir = std::env::temp_dir().join(format!("bmz-archive-source-{id}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        run_migrations(&mut connection, LIBRARY_MIGRATIONS).unwrap();
        let fixture = Self {
            roots: vec![PathEntry {
                path: dir.to_string_lossy().into_owned(),
                enabled: true,
                recursive: false,
            }],
            dir,
            db: LibraryDatabase::from_connection(connection),
        };
        fixture.db.set_configured_song_roots(&fixture.roots).unwrap();
        fixture
    }
    fn zip(&self, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = self.dir.join(name);
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            writer
                .start_file(
                    *name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().sync_all().unwrap();
        path
    }
    fn scan(&mut self) -> ScanReport {
        let scan = crate::config::app_config::ScanConfig {
            use_everything: false,
            skip_hidden: true,
            ..AppConfig::default().scan
        };
        scan_song_roots(&mut self.db, &self.roots, &scan, 10, false).unwrap()
    }
    fn locator(&self, archive: &str, entry: &str) -> ChartLocator {
        ChartLocator::Archive { container: self.dir.join(archive), entry: entry.to_owned() }
    }
    fn id(&self, archive: &str, entry: &str) -> i64 {
        self.db
            .chart_id_by_chart_file_path(&self.locator(archive, entry).to_path_buf())
            .unwrap()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn locator_normalizes_entries_without_confusing_ordinary_bang_names() {
    let source = ChartLocator::parse(Path::new("music/pack.ZIP!/sub/../曲/song.pms")).unwrap();
    assert_eq!(source.container_path(), Path::new("music/pack.ZIP"));
    assert_eq!(source.to_path_buf(), PathBuf::from("music/pack.ZIP!/曲/song.pms"));
    assert!(ChartLocator::parse(Path::new("pack.zip!/../../outside.bms")).is_err());
    assert!(ChartLocator::parse(Path::new("pack.zip!/C:/outside.bms")).is_err());
    assert!(!ChartLocator::parse(Path::new("song!name.bms")).unwrap().is_archive());
    assert_eq!(
        ChartLocator::parse(Path::new("pack.zip!")).unwrap().to_path_buf(),
        PathBuf::from("pack.zip!")
    );
}

#[test]
fn archive_preview_metadata_uses_declared_extension_fallback_before_prefix() {
    let index = crate::song_archive::ArchiveIndex {
        generation: ArchiveGeneration { fingerprint: "test".into() },
        entries: ["folder/custom.ogg", "folder/preview.wav"]
            .into_iter()
            .map(|name| crate::song_archive::ArchiveEntry {
                name: name.into(),
                size: 1,
                is_directory: false,
            })
            .collect(),
    };
    assert_eq!(archive_preview_file(&index, "folder/song.bms", "custom.wav"), "custom.ogg");
}

#[test]
fn shallow_scan_registers_nested_archive_charts_bytes_and_metadata_without_materializing() {
    let mut f = Fixture::new();
    f.zip(
        "songs.zip",
        &[
            ("deep/song.bms", BMS),
            ("deep/pop.pms", BMS),
            ("deep/song.bmson", BMSON),
            ("deep/readme.txt", b"readme"),
            ("deep/preview.ogg", b"preview"),
            ("shared/key.wav", b"audio"),
        ],
    );
    let report = f.scan();
    assert_eq!(
        (report.summary.files_seen, report.summary.imported, report.summary.failed),
        (3, 3, 0)
    );
    assert_eq!(
        std::fs::read_dir(&f.dir).unwrap().count(),
        1,
        "scan must not extract into song root"
    );
    for chart in f.db.list_charts(20, 0).unwrap() {
        assert!(chart.folder_path.ends_with("songs.zip!/deep"));
        assert!(chart.has_document);
        assert_eq!(chart.preview_file, "preview.ogg");
        let source = f.db.verified_chart_source(chart.chart_id).unwrap();
        assert!(source.locator().unwrap().is_archive());
        let (_, loaded) =
            f.db.load_chart_source_bytes(chart.chart_id, BmsRandomSource::Seed(None)).unwrap();
        assert_eq!(loaded.chart.identity.file_sha256, chart.sha256);
    }
    let stable = f.locator("songs.zip", "deep/song.bms");
    let absolute = stable.canonicalize().unwrap().to_path_buf();
    assert_eq!(
        f.db.chart_id_by_chart_file_path(&absolute).unwrap(),
        Some(f.id("songs.zip", "deep/song.bms"))
    );
    assert_eq!(f.scan().summary.skipped, 3);
}

#[test]
fn hidden_archive_members_obey_scan_setting_and_appledouble_is_always_excluded() {
    let mut f = Fixture::new();
    f.zip(
        "songs.zip",
        &[
            ("song.bms", BMS),
            (".hidden/song.bms", BMS),
            (".secret.bms", BMS),
            ("__MACOSX/song.bms", BMS),
            ("._song.bms", BMS),
        ],
    );
    assert_eq!(f.scan().summary.imported, 1);
    let scan = crate::config::app_config::ScanConfig {
        use_everything: false,
        skip_hidden: false,
        ..AppConfig::default().scan
    };
    let report = scan_song_roots(&mut f.db, &f.roots, &scan, 11, false).unwrap();
    assert_eq!(report.summary.files_seen, 3);
    assert_eq!(f.db.list_charts(20, 0).unwrap().len(), 3);
}

#[test]
fn archive_cache_rebuild_keeps_hash_copy_assets_and_root_scope() {
    let mut f = Fixture::new();
    for (archive, audio) in [("a.zip", b"copy a".as_slice()), ("b.zip", b"copy b".as_slice())] {
        f.zip(archive, &[("chart/song.bms", BMS), ("shared/key.wav", audio)]);
    }
    f.scan();
    let id = f.id("a.zip", "chart/song.bms");
    let cache = f.dir.join("cache");
    let (source, resolved, import) =
        f.db.load_chart_source_with_cache(id, BmsRandomSource::Seed(None), &cache).unwrap();
    assert_eq!(source.chart.chart_id, id);
    assert_eq!(
        import.chart.identity.file_sha256,
        bmz_chart::hash::compute_chart_identity(BMS).file_sha256
    );
    assert_eq!(std::fs::read(&import.chart.sounds[0].path).unwrap(), b"copy a");
    assert_eq!(resolved.locator.to_path_buf(), source.path);
    std::fs::remove_dir_all(&cache).unwrap();
    assert!(f.db.available_chart_source(id).unwrap().is_some());
    let (_, regenerated, _) =
        f.db.load_chart_source_with_cache(id, BmsRandomSource::Seed(None), &cache).unwrap();
    assert!(regenerated.path.is_file());
    std::fs::remove_file(f.dir.join("a.zip")).unwrap();
    assert_eq!(
        f.db.verified_chart_source(id).unwrap().chart.chart_id,
        f.id("b.zip", "chart/song.bms")
    );
    f.roots[0].enabled = false;
    f.db.set_configured_song_roots(&f.roots).unwrap();
    assert!(f.db.available_chart_source(id).unwrap().is_none());
}

#[test]
fn registered_archive_sources_do_not_require_container_or_cache_io() {
    let mut f = Fixture::new();
    f.zip("songs.zip", &[("song.bms", BMS)]);
    f.scan();
    let charts = f.db.list_charts(10, 0).unwrap();
    std::fs::remove_file(f.dir.join("songs.zip")).unwrap();
    assert_eq!(f.db.registered_chart_sources(&[&charts[0]]).unwrap().len(), 1);
    assert!(f.db.available_chart_sources(&[&charts[0]]).unwrap().is_empty());
}

#[test]
fn archive_updates_prune_only_confirmed_removed_entries_and_preserve_corrupt_sources() {
    let mut f = Fixture::new();
    f.zip("songs.zip", &[("keep.bms", BMS), ("removed.bms", BMS)]);
    f.scan();
    std::fs::write(f.dir.join("songs.zip"), b"truncated archive").unwrap();
    let failed = f.scan();
    assert_eq!(failed.summary.discovery_skipped, 1);
    assert_eq!(failed.summary.removed_files, 0);
    assert_eq!(f.db.list_charts(10, 0).unwrap().len(), 2);
    f.zip("songs.zip", &[("keep.bms", BMS)]);
    assert_eq!(f.scan().summary.removed_files, 1);
    assert_eq!(f.db.list_charts(10, 0).unwrap().len(), 1);
    f.zip("songs.zip", &[("readme.txt", b"all charts removed")]);
    assert_eq!(f.scan().summary.removed_files, 1);
    assert!(f.db.list_charts(10, 0).unwrap().is_empty());
    f.zip("songs.zip", &[("keep.bms", BMS)]);
    assert_eq!(f.scan().summary.imported, 1);
    std::fs::remove_file(f.dir.join("songs.zip")).unwrap();
    assert_eq!(f.scan().summary.removed_files, 1);
    assert!(f.db.list_charts(10, 0).unwrap().is_empty());
}

#[test]
fn archive_crc_failure_rolls_back_earlier_members_and_does_not_prune() {
    let mut f = Fixture::new();
    f.zip(
        "songs.zip",
        &[("first.bms", BMS), ("second.bms", BMS), ("removed.bms", BMS), ("readme.txt", b"readme")],
    );
    f.scan();
    let changed = b"#TITLE Replacement\n#BPM 120\n#00011:01\n";
    let corrupt = b"#TITLE Corrupted payload\n#BPM 120\n#00011:01\n";
    let path = f.zip("songs.zip", &[("first.bms", changed), ("second.bms", corrupt)]);
    let mut bytes = std::fs::read(&path).unwrap();
    let offset = bytes.windows(corrupt.len()).position(|window| window == corrupt).unwrap();
    bytes[offset + 8] ^= 1;
    std::fs::write(path, bytes).unwrap();
    let report = f.scan();
    assert_eq!(report.summary.failed, 2);
    assert_eq!(report.summary.imported, 0);
    assert_eq!(report.summary.removed_files, 0);
    let charts = f.db.list_charts(10, 0).unwrap();
    assert_eq!(charts.len(), 3);
    assert!(charts.iter().all(|chart| chart.title == "Archive Song"));
    assert!(
        charts.iter().all(|chart| chart.has_document),
        "failed decode must not publish new folder metadata"
    );
}

#[test]
fn asset_only_update_invalidates_generation_without_changing_chart_identity() {
    let mut f = Fixture::new();
    // canonicalize() uses an extended-length prefix on Windows. The discovered
    // container and archive!/entry must still share one import group.
    f.roots[0].path = f.dir.canonicalize().unwrap().to_string_lossy().into_owned();
    f.zip("songs.zip", &[("chart/song.bms", BMS), ("shared/key.wav", b"old")]);
    f.scan();
    let cache = f.dir.join("cache");
    let id = f.id("songs.zip", "chart/song.bms");
    let (_, old, old_import) =
        f.db.load_chart_source_with_cache(id, BmsRandomSource::Seed(None), &cache).unwrap();
    f.zip("songs.zip", &[("chart/song.bms", BMS), ("shared/key.wav", b"new audio")]);
    assert_eq!(f.scan().summary.imported, 1);
    let (_, new, new_import) =
        f.db.load_chart_source_with_cache(id, BmsRandomSource::Seed(None), &cache).unwrap();
    assert_ne!(old.generation, new.generation);
    assert_eq!(old_import.chart.identity, new_import.chart.identity);
    assert_eq!(std::fs::read(&new_import.chart.sounds[0].path).unwrap(), b"new audio");
}

#[test]
fn archive_asset_resolution_allows_shared_parent_but_rejects_external_paths() {
    let f = Fixture::new();
    f.zip("songs.zip", &[("chart/song.bms", BMS), ("shared/key.wav", b"audio")]);
    let resolved =
        f.locator("songs.zip", "chart/song.bms").materialize(&f.dir.join("cache")).unwrap();
    assert_eq!(
        std::fs::read(resolved.resolve_asset("../shared/key.wav").unwrap()).unwrap(),
        b"audio"
    );
    for relative in [
        "../../outside.wav",
        "/outside.wav",
        "C:\\outside.wav",
        "\\\\server\\outside.wav",
        "key.wav:stream",
    ] {
        assert!(resolved.resolve_asset(relative).is_err(), "{relative}");
    }
    let folder = f.locator("songs.zip", "chart").materialize(&f.dir.join("cache")).unwrap();
    assert!(folder.resolve_asset("../shared/key.wav").unwrap().is_file());
}

#[test]
fn explicit_archive_import_and_hash_change_verification_do_not_materialize() {
    let mut f = Fixture::new();
    f.zip("songs.zip", &[("chart/song.bms", BMS)]);
    let locator = f.locator("songs.zip", "chart/song.bms").to_path_buf();
    let imported =
        crate::storage::import::import_chart_file(&mut f.db, &locator, None, None, 1).unwrap();
    assert_eq!(std::fs::read_dir(&f.dir).unwrap().count(), 1);
    assert!(f.db.verified_chart_source(imported.chart_id).is_ok());
    f.zip("songs.zip", &[("chart/song.bms", b"#TITLE Different\n#BPM 120\n")]);
    assert!(
        f.db.verified_chart_source(imported.chart_id)
            .unwrap_err()
            .to_string()
            .contains("hash changed")
    );
}

#[test]
fn archive_chart_assets_cannot_escape_after_normal_import() {
    let mut f = Fixture::new();
    for (name, directive) in [
        ("sound.zip", "#WAV01 ../../outside.wav\n#00011:01"),
        ("bga.zip", "#BMP01 ../../outside.png\n#00004:01"),
        ("stage.zip", "#STAGEFILE ../../outside.png"),
    ] {
        let chart = format!("#TITLE Escape {name}\n#BPM 120\n{directive}\n");
        f.zip(name, &[("sub/song.bms", chart.as_bytes())]);
    }
    assert_eq!(f.scan().summary.imported, 3);
    for name in ["sound.zip", "bga.zip", "stage.zip"] {
        let id = f.id(name, "sub/song.bms");
        let error = f
            .db
            .load_chart_source_with_cache(id, BmsRandomSource::Seed(None), &f.dir.join("cache"))
            .unwrap_err();
        assert!(format!("{error:#}").contains("escapes archive root"), "{name}: {error:#}");
    }
}

#[test]
fn course_hash_and_preferred_copy_resolution_accept_archive_entries() {
    use bmz_core::course::{CourseDefinition, CourseEntry, CourseKind};
    let mut f = Fixture::new();
    f.zip("songs.zip", &[("sub/song.bms", BMS)]);
    f.scan();
    let archive_id = f.id("songs.zip", "sub/song.bms");
    let native = f.dir.join("copy.bms");
    std::fs::write(&native, BMS).unwrap();
    let missing_id = crate::storage::import::import_chart_file(&mut f.db, &native, None, None, 11)
        .unwrap()
        .chart_id;
    std::fs::remove_file(native).unwrap();
    let sha = bmz_chart::hash::compute_chart_identity(BMS).file_sha256;
    let hash = sha.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let definition = CourseDefinition {
        key: "archive-course".into(),
        title: "Archive Course".into(),
        kind: CourseKind::Course,
        entries: [None, Some(archive_id), Some(missing_id)]
            .into_iter()
            .map(|chart_id| CourseEntry {
                sha256: Some(hash.clone()),
                md5: None,
                chart_id,
                title_hint: String::new(),
            })
            .collect(),
        constraints: Default::default(),
        trophies: Vec::new(),
        release: true,
    };
    for configured in [true, false] {
        if !configured {
            f.db.conn().execute("DELETE FROM library_song_scope", []).unwrap();
        }
        let course_id = f.db.upsert_course("archive-test", &definition, 0, 12).unwrap();
        assert!(
            f.db.list_course_entries(course_id)
                .unwrap()
                .iter()
                .all(|entry| entry.entry.chart_id == Some(archive_id))
        );
    }
}

#[test]
fn scanned_archive_generation_is_persisted_for_later_processes() {
    let mut f = Fixture::new();
    let archive = f.zip("songs.zip", &[("chart/song.bms", BMS)]);
    f.scan();
    let id = f.id("songs.zip", "chart/song.bms");
    let canonical = archive.canonicalize().unwrap();
    let (stamp, generation): (String, String) =
        f.db.conn()
            .query_row(
                "SELECT stamp, generation FROM song_archive_fingerprints WHERE path = ?1",
                [canonical.to_string_lossy()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
    assert_eq!(stamp, song_archive::metadata_stamp(&canonical).unwrap().token());
    let scanned: String =
        f.db.conn()
            .query_row("SELECT generation FROM song_archive_scans", [], |row| row.get(0))
            .unwrap();
    assert_eq!(generation, scanned);
    f.db.prime_archive_generations().unwrap();
    assert_eq!(song_archive::generation_record(&archive).unwrap().generation.fingerprint, scanned);
    assert_eq!(f.db.verified_chart_source(id).unwrap().chart.chart_id, id);

    // A rewrite invalidates the scanned generation, so verification re-reads the entry.
    f.zip("songs.zip", &[("chart/song.bms", b"#TITLE Different\n#BPM 120\n")]);
    assert!(f.db.verified_chart_source(id).unwrap_err().to_string().contains("hash changed"));
}
