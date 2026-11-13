use std::collections::{HashMap, HashSet};
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use anyhow::Result;
use bmz_chart::import::ImportResult;
use bmz_chart::import::error::ImportError;
use bmz_chart::import::{AssetLookup, BmsRandomSource, import_chart_with_assets};
use rayon::prelude::*;

use crate::config::app_config::{PathEntry, ScanConfig};

use super::library_db::{CHART_IMPORT_VERSION, ChartImportRecord, LibraryDatabase};

#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub roots_seen: u32,
    pub roots_unreadable: u32,
    pub native_discovery_roots: u32,
    pub everything_discovery_roots: u32,
    pub everything_fallback_roots: u32,
    /// Why the first root that could not use Everything fell back, unless it only had
    /// no indexed files (an empty root is not an Everything problem worth reporting).
    pub everything_fallback_reason: Option<String>,
    pub files_seen: u32,
    pub discovery_skipped: u32,
    pub imported: u32,
    pub failed: u32,
    pub skipped: u32,
    pub warnings: u32,
    pub removed_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongScanScope {
    Paths,
    Library,
}

#[derive(Debug, Clone)]
pub struct ScanFailure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDiscoveryOperation {
    OpenDirectory,
    ReadEntry,
    ReadFileType,
    ReadMetadata,
    ReadArchive,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScanDiscoveryBackend {
    #[default]
    Native,
    Everything,
    NativeFallback,
}

impl ScanDiscoveryBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Everything => "everything",
            Self::NativeFallback => "native_fallback",
        }
    }
}

impl ScanDiscoveryOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenDirectory => "open_directory",
            Self::ReadEntry => "read_entry",
            Self::ReadFileType => "read_file_type",
            Self::ReadMetadata => "read_metadata",
            Self::ReadArchive => "read_archive",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanDiscoveryIssue {
    pub root: PathBuf,
    pub path: PathBuf,
    pub operation: ScanDiscoveryOperation,
    pub error_kind: io::ErrorKind,
    pub raw_os_error: Option<i32>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    pub summary: ScanSummary,
    pub timing: ScanTiming,
    pub failures: Vec<ScanFailure>,
    pub discovery_issues: Vec<ScanDiscoveryIssue>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanTiming {
    pub total_ms: u128,
    pub discovery_ms: u128,
    pub fingerprint_ms: u128,
    pub skip_check_ms: u128,
    pub parse_ms: u128,
    /// Worker time summed across files, so this can exceed the scan's wall time.
    pub analysis_worker_ms: u128,
    pub encode_worker_ms: u128,
    pub write_ms: u128,
    /// Commit time for regular-file batches, included in write_ms.
    pub commit_ms: u128,
    /// Archive listing checks and imports, including their own parsing.
    pub archive_ms: u128,
    pub folder_flags_ms: u128,
    /// Post-import archive revalidation, pruning, and root timestamp updates.
    pub prune_ms: u128,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanProgress {
    pub done: u32,
    pub total: u32,
}

mod archive;
mod discovery;
#[cfg(test)]
mod discovery_bench;
#[cfg(windows)]
mod everything;
mod import;
#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod partial_scan_tests;

pub(crate) use discovery::folder_has_document;
pub use discovery::{ChartFileEntry, discover_chart_files};
pub use import::{scan_song_roots, scan_song_roots_with_progress};

pub fn is_chart_file(path: &Path) -> bool {
    path.file_name().is_some_and(discovery::is_chart_file_name)
}

use discovery::*;
#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::library_db::{LibraryDatabase, library_path_key};
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    fn scan_config() -> ScanConfig {
        ScanConfig {
            follow_symlinks: false,
            use_everything: false,
            auto_rescan_on_startup: false,
            rescan_missing_files: true,
        }
    }

    #[test]
    fn encrypted_archive_is_reported_without_blocking_root_pruning() {
        let root = make_temp_dir("scan-encrypted-archive");
        std::fs::create_dir_all(root.join("song")).unwrap();
        write_file(&root.join("song/a.bms"), "#TITLE A\n#BPM 120\n#00011:01\n");
        write_file(&root.join("song/b.bms"), "#TITLE B\n#BPM 120\n#00011:01\n");
        let mut writer = sevenz_rust2::ArchiveWriter::create(root.join("secret.7z")).unwrap();
        writer.set_content_methods(vec![
            sevenz_rust2::encoder_options::AesEncoderOptions::new("secret".into()).into(),
            sevenz_rust2::EncoderMethod::LZMA2.into(),
        ]);
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("secret.bms"),
                Some(std::io::Cursor::new(b"#TITLE Secret\n")),
            )
            .unwrap();
        writer.finish().unwrap();
        // AppleDouble metadata copied from macOS is neither a chart nor a damaged archive.
        let apple_double = b"\x00\x05\x16\x07\x00\x02\x00\x00 resource fork";
        std::fs::write(root.join("song/._a.bms"), apple_double).unwrap();
        std::fs::write(root.join("._pack.zip"), apple_double).unwrap();
        std::fs::create_dir_all(root.join("__MACOSX/song")).unwrap();
        write_file(&root.join("__MACOSX/song/a.bms"), "#TITLE A\n#BPM 120\n#00011:01\n");

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let first = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_070, false).unwrap();
        assert_eq!(first.summary.files_seen, 2);
        assert_eq!(first.summary.imported, 2);
        assert_eq!(first.discovery_issues.len(), 1);
        assert_eq!(first.discovery_issues[0].operation, ScanDiscoveryOperation::ReadArchive);
        let last_scan_at: Option<i64> =
            db.conn().query_row("SELECT last_scan_at FROM roots", [], |row| row.get(0)).unwrap();
        assert_eq!(last_scan_at, Some(1_700_000_070));

        std::fs::remove_file(root.join("song/b.bms")).unwrap();
        let second =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_071, false).unwrap();
        assert_eq!(second.discovery_issues.len(), 1);
        assert_eq!(second.summary.removed_files, 1);
        assert_eq!(db.list_charts(10, 0).unwrap().len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rescan_prunes_deleted_duplicate_and_preserves_unchanged_copy() {
        let root = make_temp_dir("scan-deleted-duplicate");
        let old = root.join("old");
        std::fs::create_dir_all(&old).unwrap();
        let contents = "#TITLE Duplicate\n#BPM 120\n#00011:01\n";
        write_file(&root.join("keep.bms"), contents);
        write_file(&old.join("removed.bms"), contents);
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];
        scan_song_roots(&mut db, &roots, &scan_config(), 1, false).unwrap();
        assert_eq!(db.list_charts(10, 0).unwrap().len(), 2);
        std::fs::remove_file(old.join("removed.bms")).unwrap();
        // A shallow scan must not prune descendants, even when their files are missing.
        let shallow = vec![PathEntry { recursive: false, ..roots[0].clone() }];
        scan_song_roots(&mut db, &shallow, &scan_config(), 2, false).unwrap();
        assert_eq!(db.list_charts(10, 0).unwrap().len(), 2);
        let report = scan_song_roots(&mut db, &roots, &scan_config(), 3, false).unwrap();
        assert_eq!(report.summary.skipped, 1);
        let charts = db.list_charts(10, 0).unwrap();
        assert_eq!(charts.len(), 1);
        assert_eq!(
            db.primary_chart_file_path(charts[0].chart_id).unwrap(),
            Some(library_path_key(&root.join("keep.bms")))
        );
        let count: i64 =
            db.conn().query_row("SELECT COUNT(*) FROM chart_files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discover_chart_files_respects_recursion_and_keeps_hidden_files() {
        let root = make_temp_dir("discover");
        write_file(&root.join("a.bms"), "#TITLE A\n#BPM 120\n");
        write_file(&root.join("ignore.txt"), "");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        write_file(&root.join("sub").join("b.bme"), "#TITLE B\n#BPM 120\n");
        write_file(&root.join(".hidden.bms"), "#TITLE Hidden\n#BPM 120\n");

        let shallow = discover_chart_files(&root, false, &scan_config()).unwrap();
        let deep = discover_chart_files(&root, true, &scan_config()).unwrap();

        // Hidden files are scanned like beatoraja; only fixed names are excluded.
        assert_eq!(shallow.len(), 2);
        assert_eq!(deep.len(), 3);
        assert!(shallow[0].has_document);
        assert!(deep.iter().any(|entry| entry.has_document));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_refreshes_document_flag_without_reimporting_chart() {
        let root = make_temp_dir("scan-document");
        write_file(&root.join("song.bms"), "#TITLE Document\n#BPM 120\n#00011:01\n");

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_030, false).unwrap();
        assert!(!db.list_charts(10, 0).unwrap()[0].has_document);

        write_file(&root.join("README.TXT"), "document");
        let with_document =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_031, false).unwrap();
        assert_eq!(with_document.summary.skipped, 1);
        assert!(db.list_charts(10, 0).unwrap()[0].has_document);

        std::fs::remove_file(root.join("README.TXT")).unwrap();
        let without_document =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_032, false).unwrap();
        assert_eq!(without_document.summary.skipped, 1);
        assert!(!db.list_charts(10, 0).unwrap()[0].has_document);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_imports_enabled_roots() {
        let root = make_temp_dir("scan");
        write_file(
            &root.join("song.bms"),
            "\
#TITLE Scan Song
#BPM 120
#WAV01 key.wav
#00011:01
",
        );

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let report =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_020, false).unwrap();

        assert_eq!(report.summary.roots_seen, 1);
        assert_eq!(report.summary.files_seen, 1);
        assert_eq!(report.summary.imported, 1);
        assert_eq!(report.summary.failed, 0);
        assert_eq!(report.summary.skipped, 0);

        let title: String =
            db.conn().query_row("SELECT title FROM charts", [], |row| row.get(0)).unwrap();
        let last_scan_at: i64 =
            db.conn().query_row("SELECT last_scan_at FROM roots", [], |row| row.get(0)).unwrap();
        assert_eq!(title, "Scan Song");
        assert_eq!(last_scan_at, 1_700_000_020);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_reports_progress() {
        let root = make_temp_dir("scan-progress");
        write_file(&root.join("a.bms"), "#TITLE A\n#BPM 120\n#00011:01\n");
        write_file(&root.join("b.bms"), "#TITLE B\n#BPM 120\n#00011:01\n");

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];
        let mut progress = Vec::new();

        let report = scan_song_roots_with_progress(
            &mut db,
            &roots,
            &scan_config(),
            1_700_000_020,
            false,
            |p| progress.push(p),
        )
        .unwrap();

        assert_eq!(report.summary.imported, 2);
        assert_eq!(progress.first(), Some(&ScanProgress { done: 0, total: 0 }));
        assert!(progress.contains(&ScanProgress { done: 0, total: 1 }));
        assert!(progress.contains(&ScanProgress { done: 0, total: 2 }));
        assert_eq!(progress.last(), Some(&ScanProgress { done: 2, total: 2 }));
        assert!(progress.windows(2).all(|pair| pair[0].done <= pair[1].done));
        assert!(progress.windows(2).all(|pair| pair[0].total <= pair[1].total));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_skips_unreadable_root_and_continues_with_global_progress() {
        let valid_root = make_temp_dir("scan-after-unreadable");
        let missing_root = valid_root.join("missing-external-volume");
        write_file(&valid_root.join("song.bms"), "#TITLE Available\n#BPM 120\n#00011:01\n");

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![
            PathEntry {
                path: missing_root.to_string_lossy().into_owned(),
                enabled: true,
                recursive: true,
            },
            PathEntry {
                path: valid_root.to_string_lossy().into_owned(),
                enabled: true,
                recursive: true,
            },
        ];
        let mut progress = Vec::new();

        let report = scan_song_roots_with_progress(
            &mut db,
            &roots,
            &scan_config(),
            1_700_000_040,
            false,
            |value| progress.push(value),
        )
        .unwrap();

        assert_eq!(report.summary.roots_seen, 2);
        assert_eq!(report.summary.roots_unreadable, 1);
        assert_eq!(report.summary.discovery_skipped, 1);
        assert_eq!(report.summary.files_seen, 1);
        assert_eq!(report.summary.imported, 1);
        assert_eq!(report.discovery_issues.len(), 1);
        assert_eq!(report.discovery_issues[0].root, missing_root);
        assert_eq!(report.discovery_issues[0].path, missing_root);
        assert_eq!(report.discovery_issues[0].operation, ScanDiscoveryOperation::OpenDirectory);
        assert_eq!(report.discovery_issues[0].error_kind, io::ErrorKind::NotFound);
        assert!(progress.contains(&ScanProgress { done: 0, total: 1 }));
        assert_eq!(progress.last(), Some(&ScanProgress { done: 1, total: 1 }));
        assert!(progress.windows(2).all(|pair| pair[0].total <= pair[1].total));

        let missing_last_scan: Option<i64> = db
            .conn()
            .query_row(
                "SELECT last_scan_at FROM roots WHERE path = ?1",
                [library_path_key(&missing_root)],
                |row| row.get(0),
            )
            .unwrap();
        let valid_last_scan: Option<i64> = db
            .conn()
            .query_row(
                "SELECT last_scan_at FROM roots WHERE path = ?1",
                [library_path_key(&valid_root)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(missing_last_scan, None);
        assert_eq!(valid_last_scan, Some(1_700_000_040));

        std::fs::remove_dir_all(valid_root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn everything_incompatible_symlink_setting_falls_back_to_native_discovery() {
        let root = make_temp_dir("everything-symlink-fallback");
        let mut config = scan_config();
        config.use_everything = true;

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let report = scan_song_roots(&mut db, &roots, &config, 1_700_000_042, false).unwrap();

        assert_eq!(report.summary.everything_discovery_roots, 0);
        assert_eq!(report.summary.native_discovery_roots, 1);
        assert_eq!(report.summary.everything_fallback_roots, 1);
        assert!(
            report
                .summary
                .everything_fallback_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("follow_symlinks")),
            "{:?}",
            report.summary.everything_fallback_reason
        );
        assert_eq!(report.summary.files_seen, 0);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files() {
        use std::os::unix::fs::PermissionsExt;

        let root = make_temp_dir("scan-unreadable-subdirectory");
        let unreadable = root.join("unreadable");
        std::fs::create_dir_all(&unreadable).unwrap();
        write_file(&root.join("available.bms"), "#TITLE Available\n#BPM 120\n#00011:01\n");
        write_file(&unreadable.join("hidden.bms"), "#TITLE Hidden\n#BPM 120\n#00011:01\n");
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let result = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_041, false);
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let report = result.unwrap();

        assert_eq!(report.summary.roots_unreadable, 0);
        assert_eq!(report.summary.discovery_skipped, 1);
        assert_eq!(report.summary.files_seen, 1);
        assert_eq!(report.summary.imported, 1);
        assert_eq!(report.discovery_issues.len(), 1);
        assert_eq!(report.discovery_issues[0].path, unreadable);
        assert_eq!(report.discovery_issues[0].operation, ScanDiscoveryOperation::OpenDirectory);
        assert_eq!(report.discovery_issues[0].error_kind, io::ErrorKind::PermissionDenied);

        let last_scan_at: Option<i64> =
            db.conn().query_row("SELECT last_scan_at FROM roots", [], |row| row.get(0)).unwrap();
        assert_eq!(last_scan_at, None);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_records_failed_imports() {
        let root = make_temp_dir("scan-failed");
        // 未定義 WAV id (`#00011:99`) を参照させて warning を発生させる。
        // bms-rs はこのケースでもチャート自体は import するので、`imported_with_warnings`
        // 経路に乗る。
        write_file(&root.join("broken.bms"), "#TITLE Broken\n#BPM 120\n#TOTAL 200\n#00011:0199\n");

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let report =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_021, false).unwrap();

        assert_eq!(report.summary.files_seen, 1);
        assert_eq!(report.summary.imported, 1);
        assert_eq!(report.summary.failed, 0);

        let (status, warning): (String, String) = db
            .conn()
            .query_row(
                "SELECT chart_files.parse_status, chart_import_warnings.code
                FROM chart_files
                JOIN chart_import_warnings ON chart_import_warnings.chart_file_id = chart_files.id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "Parsed");
        assert_eq!(warning, "MissingWavDefinition");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_continues_after_oversized_bmson() {
        let root = tempfile::tempdir().unwrap();
        write_file(
            &root.path().join("oversized.bmson"),
            r#"{"version":"1.0.0","info":{"title":"Oversized","artist":"Test","genre":"Test",
                "level":1,"init_bpm":120,"resolution":240,"mode_hint":"beat-7k"},
                "sound_channels":[{"name":"note.wav","notes":[{"x":1,"y":9223372036854775807,"l":0,"c":false}]}]}"#,
        );
        write_file(&root.path().join("valid.bms"), "#TITLE Valid\n#BPM 120\n#00011:01\n");

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = [PathEntry {
            path: root.path().to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];
        let report =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_021, false).unwrap();

        assert_eq!(report.summary.files_seen, 2);
        assert_eq!(report.summary.imported, 1);
        assert_eq!(report.summary.failed, 1);
    }

    #[test]
    fn scan_song_roots_skips_unchanged_failed_files_until_file_or_importer_changes() {
        let root = make_temp_dir("scan-skip-failed");
        let path = root.join("24key.bmson");
        let unsupported = r#"{"version":"1.0.0","info":{"title":"Unsupported","artist":"T",
            "genre":"T","level":1,"init_bpm":120,"resolution":240,"mode_hint":"keyboard-24k"},
            "sound_channels":[]}"#;
        write_file(&path, unsupported);

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let first = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_060, false).unwrap();
        assert_eq!((first.summary.failed, first.summary.skipped), (1, 0));
        let second =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_061, false).unwrap();
        assert_eq!((second.summary.failed, second.summary.skipped), (0, 1));

        // A failure recorded by an older importer is retried.
        db.conn()
            .execute(
                "UPDATE chart_files SET failed_import_version = ?1",
                [CHART_IMPORT_VERSION - 1],
            )
            .unwrap();
        let retried =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_062, false).unwrap();
        assert_eq!((retried.summary.failed, retried.summary.skipped), (1, 0));

        // A changed file is parsed again and can succeed.
        write_file(&path, &unsupported.replace("keyboard-24k", "beat-7k"));
        let fixed = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_063, false).unwrap();
        assert_eq!((fixed.summary.imported, fixed.summary.failed), (1, 0));
        let status: String = db
            .conn()
            .query_row("SELECT parse_status FROM chart_files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "Parsed");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_skips_unchanged_imported_files() {
        let root = make_temp_dir("scan-skip");
        let path = root.join("song.bms");
        write_file(
            &path,
            "\
#TITLE Skip Song
#BPM 120
#00011:01
",
        );

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let first = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_022, false).unwrap();
        let second =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_023, false).unwrap();
        let forced = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_024, true).unwrap();

        assert_eq!(first.summary.imported, 1);
        assert_eq!(second.summary.imported, 0);
        assert_eq!(second.summary.skipped, 1);
        assert_eq!(forced.summary.imported, 1);
        assert_eq!(forced.summary.skipped, 0);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn forced_bmson_rescan_with_forward_slash_root_updates_the_existing_chart() {
        let root = make_temp_dir("scan-bmson-path-key");
        write_file(
            &root.join("Normal.bmson"),
            r#"{
                "version": "1.0.0",
                "info": {
                    "title": "BMSON Path Key",
                    "artist": "Test Artist",
                    "genre": "Test",
                    "level": 5,
                    "init_bpm": 120.0,
                    "judge_rank": 100.0,
                    "total": 200.0,
                    "resolution": 240
                },
                "sound_channels": []
            }"#,
        );

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let native_roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];
        let slash_roots = vec![PathEntry {
            path: root.to_string_lossy().replace('\\', "/"),
            enabled: true,
            recursive: true,
        }];

        let first =
            scan_song_roots(&mut db, &native_roots, &scan_config(), 1_700_000_050, false).unwrap();
        let refreshed =
            scan_song_roots(&mut db, &slash_roots, &scan_config(), 1_700_000_051, true).unwrap();

        assert_eq!(first.summary.imported, 1);
        assert_eq!(refreshed.summary.imported, 1);
        let counts: (i64, i64, i64, i64) = db
            .conn()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM roots),
                    (SELECT COUNT(*) FROM chart_files),
                    (SELECT COUNT(*) FROM charts),
                    (SELECT COUNT(*) FROM chart_file_links)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 1, 1, 1));
        let stored_path: String =
            db.conn().query_row("SELECT path FROM chart_files", [], |row| row.get(0)).unwrap();
        assert!(!stored_path.contains('\\'));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_normalizes_preview_extension() {
        let root = make_temp_dir("scan-preview-extension");
        write_file(&root.join("_Preview.ogg"), "ogg");
        write_file(
            &root.join("song.bms"),
            "\
#TITLE Preview Extension
#BPM 120
#PREVIEW _Preview.wav
#00011:01
",
        );

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let report =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_025, false).unwrap();

        assert_eq!(report.summary.imported, 1);
        let preview_file: String =
            db.conn().query_row("SELECT preview_file FROM charts", [], |row| row.get(0)).unwrap();
        assert_eq!(preview_file, "_Preview.ogg");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_fills_preview_prefix_audio_when_header_is_empty() {
        let root = make_temp_dir("scan-preview-prefix");
        write_file(&root.join("preview.ogg"), "ogg");
        write_file(
            &root.join("song.bms"),
            "\
#TITLE Preview Prefix
#BPM 120
#00011:01
",
        );

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let report =
            scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_026, false).unwrap();

        assert_eq!(report.summary.imported, 1);
        let preview_file: String =
            db.conn().query_row("SELECT preview_file FROM charts", [], |row| row.get(0)).unwrap();
        assert_eq!(preview_file, "preview.ogg");

        std::fs::remove_dir_all(root).unwrap();
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-player-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_file(path: &Path, text: &str) {
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.sync_all().unwrap();
    }
}
