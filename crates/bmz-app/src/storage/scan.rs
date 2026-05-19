use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use rayon::prelude::*;

use bmz_chart::import::ImportResult;
use bmz_chart::import::error::ImportError;
use bmz_chart::import::import_bms_chart;

use crate::config::app_config::{PathEntry, ScanConfig};

use super::library_db::{CHART_IMPORT_VERSION, ChartImportRecord, LibraryDatabase};

#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub roots_seen: u32,
    pub files_seen: u32,
    pub imported: u32,
    pub failed: u32,
    pub skipped: u32,
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

/// 1回のバッチで並列パースするファイル数
const IMPORT_BATCH_SIZE: usize = 256;

pub fn scan_song_roots(
    db: &mut LibraryDatabase,
    roots: &[PathEntry],
    scan: &ScanConfig,
    scanned_at: i64,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    let enabled_roots: Vec<&PathEntry> = roots.iter().filter(|r| r.enabled).collect();
    let root_count = enabled_roots.len();

    for (root_index, root) in enabled_roots.into_iter().enumerate() {
        report.summary.roots_seen += 1;
        let root_path = Path::new(&root.path);
        let root_id = db.upsert_root(root_path, root.enabled, root.recursive)?;
        let entries = discover_chart_files(root_path, root.recursive, scan)?;
        let files_total = entries.len();

        tracing::info!(
            root = %root_path.display(),
            root_num = root_index + 1,
            root_count,
            files = files_total,
            "scanning root"
        );

        // Phase 1: skip判定（1クエリでrootの全fingerprintsをロードしてHashMap lookup）
        struct FileTodo {
            path: PathBuf,
            file_size: u64,
            modified_at: i64,
        }
        let fingerprints = db.load_fingerprints_for_root(root_id)?;
        let mut to_import: Vec<FileTodo> = Vec::new();
        for entry in &entries {
            report.summary.files_seen += 1;
            let key = entry.path.to_string_lossy();
            let unchanged = fingerprints.get(key.as_ref()).is_some_and(|fp| {
                fp.file_size == entry.file_size
                    && fp.modified_at == entry.modified_at
                    && fp.import_version == CHART_IMPORT_VERSION
            });
            if unchanged {
                report.summary.skipped += 1;
            } else {
                to_import.push(FileTodo {
                    path: entry.path.clone(),
                    file_size: entry.file_size,
                    modified_at: entry.modified_at,
                });
            }
        }

        let new_total = to_import.len();
        tracing::info!(
            new_files = new_total,
            skipped = report.summary.skipped,
            root = %root_path.display(),
            "skip check complete"
        );

        // Phase 2+3: バッチごとに並列パース → 1トランザクションでまとめて書き込み
        struct ParsedFile {
            path: PathBuf,
            file_size: u64,
            modified_at: i64,
            result: Result<ImportResult, ImportError>,
        }

        let mut last_log = std::time::Instant::now();
        let log_interval = std::time::Duration::from_secs(2);

        for (batch_idx, chunk) in to_import.chunks(IMPORT_BATCH_SIZE).enumerate() {
            let batch_done = batch_idx * IMPORT_BATCH_SIZE;
            let now = std::time::Instant::now();
            if now.duration_since(last_log) >= log_interval || batch_idx == 0 {
                last_log = now;
                let pct = batch_done * 100 / new_total.max(1);
                tracing::info!(
                    pct,
                    done = batch_done,
                    total = new_total,
                    root = %root_path.display(),
                    "importing"
                );
            }

            // 並列パース
            let parse_start = std::time::Instant::now();
            let parsed: Vec<ParsedFile> = chunk
                .par_iter()
                .map(|todo| ParsedFile {
                    path: todo.path.clone(),
                    file_size: todo.file_size,
                    modified_at: todo.modified_at,
                    result: import_bms_chart(&todo.path, None, false),
                })
                .collect();
            let parse_ms = parse_start.elapsed().as_millis();

            // 1トランザクションでバッチ書き込み
            let write_start = std::time::Instant::now();
            {
                let tx = db.conn_mut().transaction()?;
                for p in &parsed {
                    match &p.result {
                        Ok(import_result) => {
                            let record = ChartImportRecord {
                                root_id: Some(root_id),
                                file_path: &p.path,
                                file_size: p.file_size,
                                modified_at: p.modified_at,
                                scanned_at,
                                chart: &import_result.chart,
                            };
                            let (_, chart_file_id) =
                                LibraryDatabase::write_chart_import(&tx, &record)?;
                            let warnings_written = LibraryDatabase::write_import_warnings(
                                &tx,
                                chart_file_id,
                                &import_result.warnings,
                                scanned_at,
                            )?;
                            report.summary.imported += 1;
                            report.summary.warnings += warnings_written as u32;
                        }
                        Err(error) => {
                            let message = error.to_string();
                            LibraryDatabase::write_failed_chart(
                                &tx,
                                Some(root_id),
                                &p.path,
                                p.file_size,
                                p.modified_at,
                                scanned_at,
                                &message,
                            )?;
                            report.summary.failed += 1;
                            report.failures.push(ScanFailure { path: p.path.clone(), message });
                        }
                    }
                }
                tx.commit()?;
            }
            let write_ms = write_start.elapsed().as_millis();

            tracing::info!(
                batch = batch_idx,
                files = chunk.len(),
                parse_ms,
                write_ms,
                root = %root_path.display(),
                "batch timing"
            );
        }

        tracing::info!(
            imported = report.summary.imported,
            skipped = report.summary.skipped,
            failed = report.summary.failed,
            root = %root_path.display(),
            "root scan complete"
        );

        db.update_root_scanned_at(root_id, scanned_at)?;
    }

    Ok(report)
}

#[derive(Debug, Clone)]
pub struct ChartFileEntry {
    pub path: PathBuf,
    pub file_size: u64,
    pub modified_at: i64,
}

pub fn discover_chart_files(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
) -> Result<Vec<ChartFileEntry>> {
    let mut out = Vec::new();
    discover_into(root, recursive, scan, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    Ok(out)
}

fn discover_into(
    dir: &Path,
    recursive: bool,
    scan: &ScanConfig,
    out: &mut Vec<ChartFileEntry>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if scan.skip_hidden && is_hidden(&path) {
            continue;
        }

        let (file_type, meta_opt) = if scan.follow_symlinks {
            let meta = entry.metadata()?;
            let ft = meta.file_type();
            (ft, Some(meta))
        } else {
            (entry.file_type()?, None)
        };

        if file_type.is_dir() {
            if recursive {
                discover_into(&path, recursive, scan, out)?;
            }
        } else if file_type.is_file() && is_chart_file(&path) {
            let (file_size, modified_at) = meta_opt
                .or_else(|| entry.metadata().ok())
                .map(|m| {
                    let mtime = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    (m.len(), mtime)
                })
                .unwrap_or((0, 0));
            out.push(ChartFileEntry { path, file_size, modified_at });
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
    fn scan_song_roots_records_failed_imports() {
        let root = make_temp_dir("scan-failed");
        // `0あ` は偶数バイト長だが、2文字トークンへの分割が UTF-8 文字境界を
        // またぐため非UTF-8トークンとなり、チャートのパースが失敗する。
        write_file(&root.join("broken.bms"), "#TITLE Broken\n#00011:0あ\n");

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

        let first = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_022).unwrap();
        let second = scan_song_roots(&mut db, &roots, &scan_config(), 1_700_000_023).unwrap();

        assert_eq!(first.summary.imported, 1);
        assert_eq!(second.summary.imported, 0);
        assert_eq!(second.summary.skipped, 1);

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
