use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;

use crate::config::app_config::{PathEntry, ScanConfig};

use super::import::import_chart_file;
use super::library_db::LibraryDatabase;

#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub roots_seen: u32,
    pub files_seen: u32,
    pub imported: u32,
    pub failed: u32,
    pub warnings: u32,
}

#[derive(Debug, Clone)]
pub struct ScanFailure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    pub summary: ScanSummary,
    pub failures: Vec<ScanFailure>,
}

pub fn scan_song_roots(
    db: &mut LibraryDatabase,
    roots: &[PathEntry],
    scan: &ScanConfig,
    scanned_at: i64,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();

    for root in roots.iter().filter(|root| root.enabled) {
        report.summary.roots_seen += 1;
        let root_path = Path::new(&root.path);
        let root_id = db.upsert_root(root_path, root.enabled, root.recursive)?;
        let files = discover_chart_files(root_path, root.recursive, scan)?;

        for path in files {
            report.summary.files_seen += 1;
            match import_chart_file(db, &path, Some(root_id), scanned_at) {
                Ok(imported) => {
                    report.summary.imported += 1;
                    report.summary.warnings += imported.warnings.len() as u32;
                }
                Err(error) => {
                    report.summary.failed += 1;
                    let message = error.to_string();
                    let (file_size, modified_at) = file_metadata_for_failure(&path);
                    db.upsert_failed_chart_file(
                        Some(root_id),
                        &path,
                        file_size,
                        modified_at,
                        scanned_at,
                        &message,
                    )?;
                    report.failures.push(ScanFailure { path, message });
                }
            }
        }

        db.update_root_scanned_at(root_id, scanned_at)?;
    }

    Ok(report)
}

pub fn discover_chart_files(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    discover_into(root, recursive, scan, &mut out)?;
    out.sort();
    out.dedup();
    Ok(out)
}

fn discover_into(
    dir: &Path,
    recursive: bool,
    scan: &ScanConfig,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type =
            if scan.follow_symlinks { entry.metadata()?.file_type() } else { entry.file_type()? };

        if scan.skip_hidden && is_hidden(&path) {
            continue;
        }

        if file_type.is_dir() {
            if recursive {
                discover_into(&path, recursive, scan, out)?;
            }
        } else if file_type.is_file() && is_chart_file(&path) {
            out.push(path);
        }
    }

    Ok(())
}

fn is_chart_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(extension.to_ascii_lowercase().as_str(), "bms" | "bme" | "bml" | "pms")
        })
        .unwrap_or(false)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn file_metadata_for_failure(path: &Path) -> (u64, i64) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    (metadata.len(), modified_at)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::library_db::LibraryDatabase;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    fn scan_config() -> ScanConfig {
        ScanConfig {
            follow_symlinks: false,
            skip_hidden: true,
            auto_rescan_on_startup: false,
            rescan_missing_files: true,
        }
    }

    #[test]
    fn discover_chart_files_respects_recursion_and_hidden_files() {
        let root = make_temp_dir("discover");
        write_file(&root.join("a.bms"), "#TITLE A\n#BPM 120\n");
        write_file(&root.join("ignore.txt"), "");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        write_file(&root.join("sub").join("b.bme"), "#TITLE B\n#BPM 120\n");
        write_file(&root.join(".hidden.bms"), "#TITLE Hidden\n#BPM 120\n");

        let shallow = discover_chart_files(&root, false, &scan_config()).unwrap();
        let deep = discover_chart_files(&root, true, &scan_config()).unwrap();

        assert_eq!(shallow.len(), 1);
        assert_eq!(deep.len(), 2);

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

        let report = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_020).unwrap();

        assert_eq!(report.summary.roots_seen, 1);
        assert_eq!(report.summary.files_seen, 1);
        assert_eq!(report.summary.imported, 1);
        assert_eq!(report.summary.failed, 0);

        let title: String =
            db.conn().query_row("SELECT title FROM charts", [], |row| row.get(0)).unwrap();
        let last_scan_at: i64 =
            db.conn().query_row("SELECT last_scan_at FROM roots", [], |row| row.get(0)).unwrap();
        assert_eq!(title, "Scan Song");
        assert_eq!(last_scan_at, 1_700_000_020);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_song_roots_records_failed_imports() {
        let root = make_temp_dir("scan-failed");
        write_file(&root.join("broken.bms"), "#TITLE Broken\n#00011:0\n");

        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let roots = vec![PathEntry {
            path: root.to_string_lossy().into_owned(),
            enabled: true,
            recursive: true,
        }];

        let report = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_021).unwrap();

        assert_eq!(report.summary.files_seen, 1);
        assert_eq!(report.summary.imported, 0);
        assert_eq!(report.summary.failed, 1);

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
        assert_eq!(status, "Failed");
        assert_eq!(warning, "ImportFailed");

        std::fs::remove_dir_all(root).unwrap();
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-app-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_file(path: &Path, text: &str) {
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.sync_all().unwrap();
    }
}
