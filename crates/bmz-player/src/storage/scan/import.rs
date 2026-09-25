use super::*;
use crate::storage::library_db::library_path_key;

/// 1回のバッチで並列パースするファイル数
const IMPORT_BATCH_SIZE: usize = 256;

pub fn scan_song_roots(
    db: &mut LibraryDatabase,
    roots: &[PathEntry],
    scan: &ScanConfig,
    scanned_at: i64,
    force: bool,
) -> Result<ScanReport> {
    scan_song_roots_with_progress(db, roots, scan, scanned_at, force, |_| {})
}

pub fn scan_song_roots_with_progress(
    db: &mut LibraryDatabase,
    roots: &[PathEntry],
    scan: &ScanConfig,
    scanned_at: i64,
    force: bool,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanReport> {
    struct DiscoveredRoot {
        root_path: PathBuf,
        root_id: i64,
        root_num: usize,
        entries: Vec<ChartFileEntry>,
        discovery_complete: bool,
        root_readable: bool,
        recursive: bool,
    }

    struct FileTodo {
        path: PathBuf,
        file_size: u64,
        modified_at: i64,
    }

    struct ParsedFile {
        path: PathBuf,
        file_size: u64,
        modified_at: i64,
        result: Result<ImportResult, ImportError>,
    }

    let total_start = Instant::now();
    let mut report = ScanReport::default();
    let enabled_roots: Vec<&PathEntry> = roots.iter().filter(|r| r.enabled).collect();
    let root_count = enabled_roots.len();
    let mut discovered_roots = Vec::with_capacity(root_count);
    let mut files_total = 0_u32;

    // Discovery runs across every root before fingerprinting/importing so the
    // progress denominator can grow in real time without resetting per root.
    on_progress(ScanProgress::default());
    for (root_index, root) in enabled_roots.into_iter().enumerate() {
        report.summary.roots_seen = report.summary.roots_seen.saturating_add(1);
        let root_path = Path::new(&root.path);
        let root_id = db.upsert_root(root_path, root.enabled, root.recursive)?;
        let discovery_start = Instant::now();
        let files_before_root = files_total;
        let discovery =
            discover_chart_files_with_progress(root_path, root.recursive, scan, |root_total| {
                on_progress(ScanProgress {
                    done: 0,
                    total: files_before_root.saturating_add(root_total),
                });
            });
        let discovery_ms = discovery_start.elapsed().as_millis();
        report.timing.discovery_ms += discovery_ms;
        match discovery.backend {
            ScanDiscoveryBackend::Native => {
                report.summary.native_discovery_roots =
                    report.summary.native_discovery_roots.saturating_add(1);
            }
            ScanDiscoveryBackend::Everything => {
                report.summary.everything_discovery_roots =
                    report.summary.everything_discovery_roots.saturating_add(1);
            }
            ScanDiscoveryBackend::NativeFallback => {
                report.summary.native_discovery_roots =
                    report.summary.native_discovery_roots.saturating_add(1);
                report.summary.everything_fallback_roots =
                    report.summary.everything_fallback_roots.saturating_add(1);
            }
        }
        let root_files = usize_to_u32(discovery.entries.len());
        files_total = files_total.saturating_add(root_files);
        report.summary.files_seen = files_total;
        report.summary.discovery_skipped =
            report.summary.discovery_skipped.saturating_add(usize_to_u32(discovery.issues.len()));
        if !discovery.root_readable {
            report.summary.roots_unreadable = report.summary.roots_unreadable.saturating_add(1);
        }

        tracing::info!(
            root = %root_path.display(),
            root_num = root_index + 1,
            root_count,
            discovery_backend = discovery.backend.as_str(),
            files = root_files,
            discovery_issues = discovery.issues.len(),
            discovery_ms,
            "song root discovery complete"
        );

        report.discovery_issues.extend(discovery.issues);
        discovered_roots.push(DiscoveredRoot {
            root_path: root_path.to_path_buf(),
            root_id,
            root_num: root_index + 1,
            entries: discovery.entries,
            discovery_complete: discovery.complete,
            root_readable: discovery.root_readable,
            recursive: root.recursive,
        });
    }
    on_progress(ScanProgress { done: 0, total: files_total });

    let mut progress_done = 0_u32;
    for root in discovered_roots {
        if !root.root_readable {
            continue;
        }

        let root_path = root.root_path.as_path();
        let entries = root.entries;
        let root_files_total = usize_to_u32(entries.len());
        let root_skipped_start = report.summary.skipped;
        let root_imported_start = report.summary.imported;
        let root_failed_start = report.summary.failed;
        let folder_document_flags: Vec<(PathBuf, bool)> = entries
            .iter()
            .filter_map(|entry| {
                entry.path.parent().map(|folder| (folder.to_path_buf(), entry.has_document))
            })
            .collect::<HashMap<_, _>>()
            .into_iter()
            .collect();

        // Phase 2: skip判定（1クエリでrootの全fingerprintsをロードしてHashMap lookup）
        let fingerprint_start = Instant::now();
        let fingerprints = db.load_fingerprints_for_root(root.root_id)?;
        let fingerprint_ms = fingerprint_start.elapsed().as_millis();
        report.timing.fingerprint_ms += fingerprint_ms;
        let skip_start = Instant::now();
        let mut to_import: Vec<FileTodo> = Vec::new();
        let mut unchanged_count = 0_u32;
        for entry in &entries {
            let key = library_path_key(&entry.path);
            let unchanged = !force
                && fingerprints.get(&key).is_some_and(|fp| {
                    fp.file_size == entry.file_size
                        && fp.modified_at == entry.modified_at
                        && fp.import_version == CHART_IMPORT_VERSION
                });
            if unchanged {
                report.summary.skipped = report.summary.skipped.saturating_add(1);
                unchanged_count = unchanged_count.saturating_add(1);
            } else {
                to_import.push(FileTodo {
                    path: entry.path.clone(),
                    file_size: entry.file_size,
                    modified_at: entry.modified_at,
                });
            }
        }
        let skip_check_ms = skip_start.elapsed().as_millis();
        report.timing.skip_check_ms += skip_check_ms;
        progress_done = progress_done.saturating_add(unchanged_count).min(files_total);
        on_progress(ScanProgress { done: progress_done, total: files_total });

        let new_total = to_import.len();
        tracing::info!(
            new_files = new_total,
            skipped = unchanged_count,
            fingerprint_ms,
            skip_check_ms,
            root = %root_path.display(),
            "skip check complete"
        );

        // Phase 3+4: バッチごとに並列パース → 1トランザクションでまとめて書き込み
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
                    result: import_bms_chart_catching_unwind(&todo.path),
                })
                .collect();
            let parse_ms = parse_start.elapsed().as_millis();
            report.timing.parse_ms += parse_ms;

            // 1トランザクションでバッチ書き込み
            let write_start = std::time::Instant::now();
            {
                let tx = db.conn_mut().transaction()?;
                for p in &parsed {
                    match &p.result {
                        Ok(import_result) => {
                            let record = ChartImportRecord {
                                root_id: Some(root.root_id),
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
                                Some(root.root_id),
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
            report.timing.write_ms += write_ms;

            tracing::info!(
                batch = batch_idx,
                files = chunk.len(),
                parse_ms,
                write_ms,
                root = %root_path.display(),
                "batch timing"
            );
            progress_done =
                progress_done.saturating_add(usize_to_u32(parsed.len())).min(files_total);
            on_progress(ScanProgress { done: progress_done, total: files_total });
        }

        // Discovery already enumerated every song directory. Persist the shared
        // folder flag even when every chart was skipped as unchanged.
        db.update_folder_document_flags(&folder_document_flags)?;

        tracing::info!(
            root_num = root.root_num,
            root_count,
            files = root_files_total,
            imported = report.summary.imported.saturating_sub(root_imported_start),
            skipped = report.summary.skipped.saturating_sub(root_skipped_start),
            failed = report.summary.failed.saturating_sub(root_failed_start),
            root = %root_path.display(),
            "root scan complete"
        );

        if root.discovery_complete {
            report.summary.removed_files +=
                db.prune_missing_chart_files(root_path, root.recursive)?;
            db.update_root_scanned_at(root.root_id, scanned_at)?;
        } else {
            tracing::warn!(
                root = %root_path.display(),
                "song root discovery was incomplete; last_scan_at was not updated"
            );
        }
    }

    on_progress(ScanProgress { done: files_total, total: files_total });
    report.timing.total_ms = total_start.elapsed().as_millis();
    Ok(report)
}

pub(super) fn import_bms_chart_catching_unwind(path: &Path) -> Result<ImportResult, ImportError> {
    match catch_unwind(AssertUnwindSafe(|| import_bms_chart(path, None, false))) {
        Ok(result) => result.map(|mut result| {
            result.chart.metadata.preview_file = crate::chart_asset::normalize_preview_file(
                path,
                &result.chart.metadata.preview_file,
            );
            result
        }),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&'static str>().copied())
                .unwrap_or("unknown panic");
            Err(ImportError::Parse {
                path: path.to_path_buf(),
                message: format!("chart import panicked: {message}"),
            })
        }
    }
}
