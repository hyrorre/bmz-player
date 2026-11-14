use super::*;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::content_cache::{ChartContentCache, ImportWork, ScanChart, ScanImportError};
use crate::storage::library_db::{ChartImportSource, library_path_key};

/// 1回のバッチで並列パースするファイル数
pub(super) const IMPORT_BATCH_SIZE: usize = 256;

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
        entries: Vec<(String, ChartFileEntry)>,
        archives: Vec<super::discovery::DiscoveredArchive>,
        discovery_complete: bool,
        root_readable: bool,
        recursive: bool,
        backend: ScanDiscoveryBackend,
    }

    struct FileTodo {
        path: PathBuf,
        file_size: u64,
        modified_at: i64,
        preview_fallback: Arc<OnceLock<Option<PathBuf>>>,
    }

    struct ParsedFile {
        path: PathBuf,
        file_size: u64,
        modified_at: i64,
        result: Result<(Arc<ScanChart>, String), ScanImportError>,
        work: ImportWork,
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
    // Unchanged archives keep their remembered generation instead of being re-hashed.
    if let Err(error) = db.prime_archive_generations() {
        tracing::warn!(error = %error, "failed to load remembered song archive generations");
    }
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
                if report.summary.everything_fallback_reason.is_none() {
                    report.summary.everything_fallback_reason = discovery.fallback_reason.clone();
                }
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
            entries: discovery
                .entries
                .into_iter()
                .map(|entry| (library_path_key(&entry.path), entry))
                .collect(),
            archives: discovery.archives,
            discovery_complete: discovery.complete,
            root_readable: discovery.root_readable,
            recursive: root.recursive,
            backend: discovery.backend,
        });
    }
    on_progress(ScanProgress { done: 0, total: files_total });

    // Discovery precedes all writes. Preserve positive evidence from every root,
    // including incomplete scans, until all imports have finished.
    let discovered_any: HashSet<&str> = discovered_roots
        .iter()
        .flat_map(|root| root.entries.iter().map(|(key, _)| key.as_str()))
        .collect();
    // Skip building the size index entirely when nothing needs importing.
    let content_cache = OnceLock::new();
    let cache_for_import = || {
        content_cache.get_or_init(|| {
            ChartContentCache::new(
                discovered_roots
                    .iter()
                    .flat_map(|root| root.entries.iter().map(|(_, entry)| entry.file_size)),
            )
        })
    };
    let root_key = |path: &Path| {
        let key = library_path_key(path);
        #[cfg(windows)]
        let key = key.to_lowercase();
        key.trim_end_matches('/').to_string()
    };
    let scanned_keys: HashSet<String> =
        discovered_roots.iter().map(|root| root_key(&root.root_path)).collect();
    let mut protected_roots: Vec<PathEntry> = db
        .retained_song_root_entries()?
        .into_iter()
        .filter(|root| !scanned_keys.contains(&root_key(Path::new(&root.path))))
        .collect();
    let mut pending_prune = Vec::new();
    let mut commit_time = Duration::ZERO;
    let mut batch_wait_time = Duration::ZERO;

    let mut progress_done = 0_u32;
    for root in &discovered_roots {
        let mut discovery_complete = root.discovery_complete;
        if !root.root_readable {
            protected_roots.push(PathEntry {
                path: library_path_key(&root.root_path),
                enabled: true,
                recursive: root.recursive,
            });
            continue;
        }

        let root_path = root.root_path.as_path();
        let entries = &root.entries;
        let root_files_total = usize_to_u32(entries.len());
        let root_skipped_start = report.summary.skipped;
        let root_imported_start = report.summary.imported;
        let root_failed_start = report.summary.failed;
        let mut folder_document_flags: Vec<(PathBuf, bool)> = entries
            .iter()
            .filter_map(|(_, entry)| entry.path.parent().map(|folder| (folder, entry.has_document)))
            .collect::<HashMap<_, _>>()
            .into_iter()
            .map(|(folder, flag)| (folder.to_path_buf(), flag))
            .collect();

        // Phase 2: skip判定（1クエリでrootの全fingerprintsをロードしてHashMap lookup）
        let fingerprint_start = Instant::now();
        let fingerprints = db.load_fingerprints_for_root(root.root_id)?;
        let fingerprint_ms = fingerprint_start.elapsed().as_millis();
        report.timing.fingerprint_ms += fingerprint_ms;
        let skip_start = Instant::now();
        let mut to_import: Vec<FileTodo> = Vec::new();
        let mut preview_fallbacks = HashMap::new();
        let mut unchanged_count = 0_u32;
        let mut archives: HashMap<PathBuf, (&crate::song_archive::ArchiveGeneration, Vec<&str>)> =
            root.archives
                .iter()
                .map(|archive| {
                    // Use the same spelling as virtual chart entries, including removal
                    // of Windows extended-length prefixes, to avoid an empty alias group.
                    (
                        PathBuf::from(library_path_key(&archive.path)),
                        (&archive.generation, Vec::new()),
                    )
                })
                .collect();
        for (key, entry) in entries {
            if let Ok(crate::chart_source::ChartLocator::Archive { container, .. }) =
                crate::chart_source::ChartLocator::parse(&entry.path)
            {
                if let Some((_, entries)) = archives.get_mut(&container) {
                    entries.push(key);
                }
                continue;
            }
            let unchanged = !force
                && fingerprints.get(key).is_some_and(|fp| {
                    fp.file_size == entry.file_size
                        && fp.modified_at == entry.modified_at
                        && fp.import_version == CHART_IMPORT_VERSION
                });
            if unchanged {
                report.summary.skipped = report.summary.skipped.saturating_add(1);
                unchanged_count = unchanged_count.saturating_add(1);
            } else {
                let preview_fallback = preview_fallbacks
                    .entry(entry.path.parent().unwrap_or(Path::new("")))
                    .or_insert_with(|| Arc::new(OnceLock::new()));
                to_import.push(FileTodo {
                    path: entry.path.clone(),
                    file_size: entry.file_size,
                    modified_at: entry.modified_at,
                    preview_fallback: Arc::clone(preview_fallback),
                });
            }
        }
        let skip_check_ms = skip_start.elapsed().as_millis();
        report.timing.skip_check_ms += skip_check_ms;
        progress_done = progress_done.saturating_add(unchanged_count).min(files_total);
        on_progress(ScanProgress { done: progress_done, total: files_total });

        let archive_start = Instant::now();
        let mut failed_archives = std::collections::HashSet::new();
        for (container, (generation, archive_entries)) in archives {
            let count = usize_to_u32(archive_entries.len());
            match super::archive::import_archive(
                db,
                &container,
                root.root_id,
                scanned_at,
                force,
                &fingerprints,
                &archive_entries,
                generation,
                cache_for_import,
            ) {
                Ok(archive) => {
                    report.summary.imported += archive.summary.imported;
                    report.summary.skipped += archive.summary.skipped;
                    report.summary.failed += archive.summary.failed;
                    report.summary.warnings += archive.summary.warnings;
                    report.summary.reused += archive.summary.reused;
                    report.timing.parse_ms += archive.timing.parse_ms;
                    report.failures.extend(archive.failures);
                }
                Err(error) => {
                    discovery_complete = false;
                    failed_archives.insert(container.clone());
                    report.summary.failed += count;
                    report.failures.push(ScanFailure {
                        path: container,
                        message: format!(
                            "archive import failed; existing registrations retained: {error:#}"
                        ),
                    });
                }
            }
            progress_done = progress_done.saturating_add(count).min(files_total);
            on_progress(ScanProgress { done: progress_done, total: files_total });
        }

        report.timing.archive_ms += archive_start.elapsed().as_millis();
        let new_total = to_import.len();
        tracing::info!(
            new_files = new_total,
            skipped = unchanged_count,
            fingerprint_ms,
            skip_check_ms,
            root = %root_path.display(),
            "skip check complete"
        );

        // Phase 3+4: 譜面ごとに並列パースし、結果をIMPORT_BATCH_SIZE件ずつ1トランザクションで
        // 書き込む。パースは別スレッドのrayonで進み、SQLiteへの書き込みと重なる。重い譜面が
        // あっても他のスレッドは次の譜面へ進む（バッチ全件の完了を待たない）。チャネルの容量は
        // IMPORT_BATCH_SIZEなので、メモリ上の解析結果は書き込み中のバッチ、チャネル内、
        // 解析中のものに限られる。同一内容の再利用には別途、上限付きのキャッシュを使う。
        // 書き込みが遅ければパース側は送信で待つ。書き込みが失敗すると
        // 受信側が破棄され、パース側の送信が失敗して終了してから scope が戻る。
        let mut last_log = std::time::Instant::now();
        let log_interval = std::time::Duration::from_secs(2);
        let mut analysis_time = Duration::ZERO;
        let mut encode_time = Duration::ZERO;
        let (parsed_tx, parsed_rx) = std::sync::mpsc::sync_channel::<ParsedFile>(IMPORT_BATCH_SIZE);
        let parse_ms = std::thread::scope(|scope| -> Result<u128> {
            let to_import = &to_import;
            let cache_for_import = &cache_for_import;
            let parser = scope.spawn(move || {
                let parse_start = std::time::Instant::now();
                // A closed receiver means the writer failed; stop parsing.
                let _ = to_import.par_iter().try_for_each_with(parsed_tx, |tx, todo| {
                    if todo.file_size >= 1024 * 1024 {
                        tracing::debug!(path = %todo.path.display(), file_size = todo.file_size, "reading large chart");
                    }
                    let started = Instant::now();
                    let bytes = bmz_song_resource::read_file(&todo.path);
                    let read_time = started.elapsed();
                    let mut work = ImportWork::default();
                    let result = match bytes {
                        Ok(bytes) => cache_for_import()
                            .import(&todo.path, &bytes, &mut work)
                            .map(|chart| {
                                let preview =
                                    crate::chart_asset::normalize_preview_file_with_fallback(
                                        &todo.path,
                                        chart.prepared.declared_preview_file(),
                                        &todo.preview_fallback,
                                    );
                                (chart, preview)
                            }),
                        Err(source) => Err(ScanImportError::Chart(ImportError::Io {
                            path: todo.path.clone(),
                            source,
                        })),
                    };
                    let elapsed = started.elapsed();
                    if elapsed >= Duration::from_millis(500) {
                        tracing::info!(
                            path = %todo.path.display(),
                            elapsed_ms = elapsed.as_millis(),
                            read_ms = read_time.as_millis(),
                            parse_ms = work.parse.as_millis(),
                            analysis_ms = work.analysis.as_millis(),
                            encode_ms = work.encode.as_millis(),
                            "slow chart import"
                        );
                    }
                    tx.send(ParsedFile {
                        path: todo.path.clone(),
                        file_size: todo.file_size,
                        modified_at: todo.modified_at,
                        result,
                        work,
                    })
                    .map_err(drop)
                });
                parse_start.elapsed().as_millis()
            });
            // Owned here, so an early return drops it and stops the parser.
            let parsed_rx = parsed_rx;
            let mut batch_done = 0_usize;
            for batch_idx in 0.. {
                let wait_started = Instant::now();
                let Ok(first) = parsed_rx.recv() else {
                    break;
                };
                let mut parsed = Vec::with_capacity(IMPORT_BATCH_SIZE);
                parsed.push(first);
                while parsed.len() < IMPORT_BATCH_SIZE {
                    match parsed_rx.recv() {
                        Ok(file) => parsed.push(file),
                        Err(_) => break,
                    }
                }
                let waited = wait_started.elapsed();
                batch_wait_time += waited;
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

                // 1トランザクションでバッチ書き込み
                let mut batch_reused = 0;
                for file in &parsed {
                    analysis_time += file.work.analysis;
                    encode_time += file.work.encode;
                    batch_reused += u32::from(file.work.reused);
                }
                report.summary.reused += batch_reused;
                let write_start = std::time::Instant::now();
                let commit_ms;
                {
                    let tx = db.conn_mut().transaction()?;
                    for p in &parsed {
                        match &p.result {
                            Ok((chart, preview_file)) => {
                                let record = ChartImportSource {
                                    root_id: Some(root.root_id),
                                    file_path: &p.path,
                                    file_size: p.file_size,
                                    modified_at: p.modified_at,
                                    scanned_at,
                                    preview_file,
                                };
                                let (_, chart_file_id) =
                                    LibraryDatabase::write_prepared_chart_import(
                                        &tx,
                                        &record,
                                        &chart.prepared,
                                    )?;
                                let warnings_written = LibraryDatabase::write_scan_warnings(
                                    &tx,
                                    chart_file_id,
                                    &chart.warnings,
                                    scanned_at,
                                )?;
                                report.summary.imported += 1;
                                report.summary.warnings += warnings_written as u32;
                            }
                            // Deleted after discovery: nothing failed. An earlier
                            // registration stays until the next scan finds it gone.
                            Err(ScanImportError::Chart(ImportError::Io { source, .. }))
                                if source.kind() == io::ErrorKind::NotFound =>
                            {
                                tracing::debug!(
                                    path = %p.path.display(),
                                    "chart disappeared before import"
                                );
                            }
                            Err(ScanImportError::Encode(error)) => {
                                anyhow::bail!("failed to encode chart analysis: {error:#}");
                            }
                            Err(ScanImportError::Chart(error)) => {
                                let message = error.to_string();
                                LibraryDatabase::write_failed_chart(
                                    &tx,
                                    Some(root.root_id),
                                    &p.path,
                                    p.file_size,
                                    p.modified_at,
                                    scanned_at,
                                    &message,
                                    failed_import_version(error),
                                )?;
                                report.summary.failed += 1;
                                report.failures.push(ScanFailure { path: p.path.clone(), message });
                            }
                        }
                    }
                    let commit_start = Instant::now();
                    tx.commit()?;
                    let committed = commit_start.elapsed();
                    commit_time += committed;
                    commit_ms = committed.as_millis();
                }
                let write_ms = write_start.elapsed().as_millis();
                report.timing.write_ms += write_ms;

                tracing::info!(
                    batch = batch_idx,
                    files = parsed.len(),
                    write_ms,
                    commit_ms,
                    wait_ms = waited.as_millis(),
                    reused = batch_reused,
                    root = %root_path.display(),
                    "batch timing"
                );
                batch_done += parsed.len();
                progress_done =
                    progress_done.saturating_add(usize_to_u32(parsed.len())).min(files_total);
                on_progress(ScanProgress { done: progress_done, total: files_total });
            }
            parser.join().map_err(|_| anyhow::anyhow!("chart parser thread panicked"))
        })?;
        report.timing.analysis_worker_ms += analysis_time.as_millis();
        report.timing.encode_worker_ms += encode_time.as_millis();
        report.timing.parse_ms += parse_ms;
        // Discovery already enumerated every song directory. Persist the shared
        // folder flag even when every chart was skipped as unchanged.
        folder_document_flags.retain(|(folder, _)| {
            !matches!(crate::chart_source::ChartLocator::parse(folder),
                Ok(crate::chart_source::ChartLocator::Archive { container, .. })
                if failed_archives.contains(&container))
        });
        let folder_flags_start = Instant::now();
        db.update_folder_document_flags(&folder_document_flags)?;
        report.timing.folder_flags_ms += folder_flags_start.elapsed().as_millis();

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

        if discovery_complete {
            if root.backend == ScanDiscoveryBackend::Everything {
                protected_roots.push(PathEntry {
                    path: library_path_key(root_path),
                    enabled: true,
                    recursive: root.recursive,
                });
            }
            pending_prune.push((
                root.root_path.clone(),
                root.root_id,
                root.recursive,
                root.backend,
                &root.archives,
            ));
        } else {
            protected_roots.push(PathEntry {
                path: library_path_key(root_path),
                enabled: true,
                recursive: root.recursive,
            });
            tracing::warn!(
                root = %root_path.display(),
                "song root discovery was incomplete; last_scan_at was not updated"
            );
        }
    }

    let prune_start = Instant::now();
    let mut validated = Vec::new();
    for (root_path, root_id, recursive, backend, archives) in pending_prune {
        let mut valid = true;
        for archive in archives {
            // An unchanged metadata stamp keeps the discovered generation; only an
            // unknown stamp needs the index (and possibly a hash) again.
            let current = match crate::song_archive::generation_record(&archive.path) {
                Some(record) => Ok(record.generation),
                None => crate::song_archive::inspect(&archive.path, &Default::default())
                    .map(|index| index.generation),
            };
            match current {
                Ok(generation) if generation == archive.generation => {}
                Err(_) if matches!(archive.path.try_exists(), Ok(false)) => {}
                _ => {
                    valid = false;
                    report.summary.discovery_skipped =
                        report.summary.discovery_skipped.saturating_add(1);
                    record_discovery_issue(
                        &mut report.discovery_issues,
                        &root_path,
                        &archive.path,
                        ScanDiscoveryOperation::ReadArchive,
                        io::Error::other("archive changed or became unreadable after discovery"),
                    );
                    report.failures.push(ScanFailure {
                        path: archive.path.clone(),
                        message: "archive changed or became unreadable after discovery; existing registrations retained".to_string(),
                    });
                }
            }
        }
        if !valid {
            protected_roots.push(PathEntry {
                path: library_path_key(&root_path),
                enabled: true,
                recursive,
            });
            continue;
        }
        validated.push((root_path, root_id, recursive, backend));
    }
    if !validated.is_empty() {
        let mut index = db.chart_paths_not_discovered(&discovered_any)?;
        for (root_path, root_id, recursive, backend) in validated {
            let request = crate::storage::library_db::PruneRequest {
                root: &root_path,
                recursive,
                // Everything results may lag and do not establish absence from scope.
                omission_proves_absence: backend != ScanDiscoveryBackend::Everything,
                protected_roots: &protected_roots,
            };
            report.summary.removed_files += db.prune_missing_chart_files(&mut index, &request)?;
            db.update_root_scanned_at(root_id, scanned_at)?;
        }
    }

    report.timing.prune_ms = prune_start.elapsed().as_millis();
    report.timing.commit_ms = commit_time.as_millis();
    report.timing.batch_wait_ms = batch_wait_time.as_millis();
    on_progress(ScanProgress { done: files_total, total: files_total });
    report.timing.total_ms = total_start.elapsed().as_millis();
    Ok(report)
}

/// The importer version to record for a failure. Bytes that failed to parse fail the
/// same way until the file or `CHART_IMPORT_VERSION` changes; a read error is retried.
pub(super) fn failed_import_version(error: &ImportError) -> i64 {
    if matches!(error, ImportError::Io { .. }) { 0 } else { CHART_IMPORT_VERSION }
}
