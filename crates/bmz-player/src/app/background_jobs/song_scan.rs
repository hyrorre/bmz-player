use super::*;

const SONG_SCAN_START_RETRY_DELAY: Duration = Duration::from_secs(5);
const SONG_SCAN_START_MAX_ATTEMPTS: u8 = 5;

enum SongScanStart {
    Started,
    Deferred { error: anyhow::Error, notify: bool },
    Failed { error: anyhow::Error },
}

impl SongScanRuntime {
    fn start_worker(
        &mut self,
        pending: PendingSongScan,
        mut request: QueuedSongScan,
        registrations: &mut Vec<(u64, crate::song_download::ChartDownloadResult)>,
        downloads: &mut crate::song_download::ChartDownloadManager,
        now: Instant,
        spawn: impl FnOnce() -> std::io::Result<thread::JoinHandle<()>>,
    ) -> SongScanStart {
        let ids: HashSet<_> = pending.download_ids.iter().copied().collect();
        if let Err(error) = spawn().context("failed to spawn song scan thread") {
            let notify = self.retry_at.is_none();
            // Even a terminal failure must not let the next request start on the next frame.
            self.retry_at = Some(now + SONG_SCAN_START_RETRY_DELAY);
            self.progress = None;
            request.start_failures += 1;
            if request.start_failures >= SONG_SCAN_START_MAX_ATTEMPTS {
                let error = error.context(format!(
                    "failed to start song scan after {} attempts",
                    request.start_failures
                ));
                let message = format!("{error:#}");
                for id in &ids {
                    downloads.finish_scan(*id, &[], Some(&message));
                }
                registrations.retain(|(id, _)| !ids.contains(id));
                return SongScanStart::Failed { error };
            }
            self.queued.push_front(request);
            return SongScanStart::Deferred { error, notify };
        }
        // Keep registrations available for retry (and late arrivals) until a worker owns them.
        registrations.retain(|(id, _)| !ids.contains(id));
        self.retry_at = None;
        self.progress = Some(ScanProgress::default());
        self.pending = Some(pending);
        SongScanStart::Started
    }
}

impl WinitApp {
    pub(in crate::app) fn publish_song_scope(&self) -> Result<()> {
        self.boot.library_db.set_configured_song_roots(&crate::songs_cmd::configured_library_roots(
            &self.boot.app_config,
            &self.boot.app_paths,
        ))
    }

    fn song_scan_error(&mut self, error: &anyhow::Error) {
        tracing::error!(error = %format_error_chain(error), "song scan failed");
        let mut args = FluentArgs::new();
        args.set("error", format!("{error:#}"));
        self.show_left_overlay_toast(
            Localizer::new(self.boot.profile_config.ui.locale())
                .format("toast-library-scan-failed", &args),
        );
    }

    pub(in crate::app) fn reload_from_select_context(&mut self) {
        let selected = self.select.select_items.get(self.select.selected_index);
        if let Some(url) = table_source_url_from_context(&self.select.folder_stack, selected) {
            if is_rian_table_source(&url) {
                self.spawn_rian_table_fetch(true);
            } else {
                self.spawn_table_fetch(url);
            }
            return;
        }
        if let Some(path) = song_scan_path_from_context(&self.select.folder_stack, selected) {
            let roots = vec![PathEntry { path, enabled: true, recursive: true }];
            // Like beatoraja's folder update, unchanged charts are skipped.
            self.spawn_song_scan(roots, false, "select song reload".to_string());
            return;
        }
        tracing::debug!("select reload: no applicable target in current context");
    }

    pub(in crate::app) fn spawn_song_scan_request(&mut self, request: SongScanRequest) {
        self.spawn_song_scan(request.roots, request.force, request.label);
    }

    pub(in crate::app) fn spawn_song_scan(
        &mut self,
        roots: Vec<PathEntry>,
        force: bool,
        label: String,
    ) {
        self.spawn_song_scan_with_scope(roots, force, label, SongScanScope::Paths);
    }

    pub(in crate::app) fn spawn_song_scan_with_scope(
        &mut self,
        roots: Vec<PathEntry>,
        force: bool,
        label: String,
        scope: SongScanScope,
    ) {
        self.jobs.song_scan.queued.push_back(QueuedSongScan {
            roots,
            force,
            label,
            scope,
            start_failures: 0,
        });
        self.start_queued_song_scan_if_idle();
    }

    pub(in crate::app) fn start_song_scan(&mut self, request: QueuedSongScan) {
        let QueuedSongScan { mut roots, force, label, scope, .. } = request.clone();
        let library_roots = if scope == SongScanScope::Library {
            // A full sync is authoritative only after the settings were saved.
            if let Err(error) = self.save_app_config_now().and_then(|()| self.publish_song_scope())
            {
                self.song_scan_error(&error);
                return;
            }
            roots = crate::songs_cmd::configured_library_roots(
                &self.boot.app_config,
                &self.boot.app_paths,
            );
            Some(roots.clone())
        } else {
            None
        };
        let library_db_path = self.boot.app_paths.library_db.clone();
        let scan_config = self.boot.app_config.scan.clone();
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(AtomicU64::new(pack_scan_progress(ScanProgress::default())));
        let worker_progress = Arc::clone(&progress);
        let downloads: Vec<_> = self
            .jobs
            .download_registrations
            .iter()
            .filter(|(_, result)| {
                roots.iter().any(|root| {
                    root.enabled && root.recursive && Path::new(&root.path) == result.scan_path
                })
            })
            .cloned()
            .collect();
        let download_ids = downloads.iter().map(|(id, _)| *id).collect();
        let download_dirs = downloads.iter().map(|(_, result)| result.scan_path.clone()).collect();
        // Publish roots only after the FIFO admits this scan, never while an
        // earlier scan owns the SQLite writer on its background thread.
        if !downloads.is_empty()
            && let Err(error) = self.publish_song_scope()
        {
            tracing::error!(%error, "failed to publish downloaded song roots");
        }
        let pending =
            PendingSongScan { finished: rx, progress, library_roots, download_ids, download_dirs };
        let started = self.jobs.song_scan.start_worker(
            pending,
            request,
            &mut self.jobs.download_registrations,
            &mut self.jobs.chart_downloads,
            Instant::now(),
            || {
                thread::Builder::new().name("song-scan".to_string()).spawn(move || {
                    let result = (|| -> Result<SongScanResult> {
                        migrate_library_db(&library_db_path)?;
                        let mut library_db = LibraryDatabase::open(&library_db_path)?;
                        let report = scan_songs_with_progress(
                            &mut library_db,
                            &roots,
                            &scan_config,
                            now_unix_seconds(),
                            force,
                            |progress| {
                                worker_progress
                                    .store(pack_scan_progress(progress), Ordering::Relaxed);
                            },
                        )?;
                        if scope == SongScanScope::Paths {
                            library_db.register_partial_song_roots(&roots)?;
                        }
                        let downloaded = crate::song_download::verify_download_registrations(
                            &library_db,
                            downloads,
                        )?;
                        Ok(SongScanResult { report, downloaded })
                    })();
                    let _ = tx.send(result);
                })
            },
        );
        match started {
            SongScanStart::Started => tracing::info!(%label, force, "started song scan"),
            SongScanStart::Deferred { error, notify: true } => {
                tracing::warn!(error = %format_error_chain(&error), "song scan startup deferred");
                let mut args = FluentArgs::new();
                args.set("seconds", SONG_SCAN_START_RETRY_DELAY.as_secs() as i64);
                args.set("error", format!("{error:#}"));
                self.show_left_overlay_toast(
                    Localizer::new(self.boot.profile_config.ui.locale())
                        .format("toast-library-scan-deferred", &args),
                );
            }
            SongScanStart::Deferred { notify: false, .. } => {}
            SongScanStart::Failed { error } => self.song_scan_error(&error),
        }
    }

    pub(in crate::app) fn poll_pending_song_scan(&mut self) {
        let Some(pending) = self.jobs.song_scan.pending.take() else {
            return;
        };
        self.jobs.song_scan.progress =
            Some(unpack_scan_progress(pending.progress.load(Ordering::Relaxed)));
        let mut keep_pending = true;
        match pending.finished.try_recv() {
            Ok(Ok(result)) => {
                let mut report = result.report;
                if let Some(roots) = &pending.library_roots {
                    let current = crate::songs_cmd::configured_library_roots(
                        &self.boot.app_config,
                        &self.boot.app_paths,
                    );
                    // Read back what queued saves have written, not an older file.
                    self.jobs.config_saver.flush();
                    let cleanup = (|| -> Result<usize> {
                        let saved =
                            crate::config::load::load_app_config(&self.boot.app_paths.config_toml)?;
                        let saved = crate::songs_cmd::configured_library_roots(
                            &saved,
                            &self.boot.app_paths,
                        );
                        anyhow::ensure!(
                            *roots == current && *roots == saved,
                            "song roots changed during scan; rescan again to remove obsolete registrations"
                        );
                        self.boot.library_db.reconcile_configured_song_roots(roots)
                    })();
                    match cleanup {
                        Ok(removed) => report.summary.removed_files += removed,
                        Err(error) => self.song_scan_error(&error),
                    }
                }
                tracing::info!(
                    removed_files = report.summary.removed_files,
                    "obsolete song registrations removed"
                );
                if report.discovery_issues.is_empty() {
                    tracing::info!(
                        imported = report.summary.imported,
                        skipped = report.summary.skipped,
                        failed = report.summary.failed,
                        discovery_skipped = report.summary.discovery_skipped,
                        roots_unreadable = report.summary.roots_unreadable,
                        everything_roots = report.summary.everything_discovery_roots,
                        native_roots = report.summary.native_discovery_roots,
                        everything_fallback_roots = report.summary.everything_fallback_roots,
                        total_ms = report.timing.total_ms,
                        discovery_ms = report.timing.discovery_ms,
                        fingerprint_ms = report.timing.fingerprint_ms,
                        skip_check_ms = report.timing.skip_check_ms,
                        parse_ms = report.timing.parse_ms,
                        write_ms = report.timing.write_ms,
                        analysis_worker_ms = report.timing.analysis_worker_ms,
                        encode_worker_ms = report.timing.encode_worker_ms,
                        commit_ms = report.timing.commit_ms,
                        archive_ms = report.timing.archive_ms,
                        folder_flags_ms = report.timing.folder_flags_ms,
                        prune_ms = report.timing.prune_ms,
                        "song scan complete"
                    );
                } else {
                    tracing::warn!(
                        imported = report.summary.imported,
                        skipped = report.summary.skipped,
                        failed = report.summary.failed,
                        discovery_skipped = report.summary.discovery_skipped,
                        roots_unreadable = report.summary.roots_unreadable,
                        everything_roots = report.summary.everything_discovery_roots,
                        native_roots = report.summary.native_discovery_roots,
                        everything_fallback_roots = report.summary.everything_fallback_roots,
                        total_ms = report.timing.total_ms,
                        discovery_ms = report.timing.discovery_ms,
                        fingerprint_ms = report.timing.fingerprint_ms,
                        skip_check_ms = report.timing.skip_check_ms,
                        parse_ms = report.timing.parse_ms,
                        write_ms = report.timing.write_ms,
                        analysis_worker_ms = report.timing.analysis_worker_ms,
                        encode_worker_ms = report.timing.encode_worker_ms,
                        commit_ms = report.timing.commit_ms,
                        archive_ms = report.timing.archive_ms,
                        folder_flags_ms = report.timing.folder_flags_ms,
                        prune_ms = report.timing.prune_ms,
                        "song scan complete with skipped paths"
                    );
                }
                self.jobs.song_scan.progress = None;
                // Falling back silently made the Everything setting look ineffective.
                if let Some(reason) = &report.summary.everything_fallback_reason {
                    let mut args = FluentArgs::new();
                    args.set("count", i64::from(report.summary.everything_fallback_roots));
                    args.set("reason", reason.clone());
                    self.show_left_overlay_toast(
                        Localizer::new(self.boot.profile_config.ui.locale())
                            .format("toast-everything-fallback", &args),
                    );
                }
                self.select.select_assets.invalidate_library();
                self.invalidate_select_folder_summaries();
                if pending.download_ids.is_empty() {
                    self.reload_select_items();
                } else {
                    self.change_select_list(SelectListAction::RefreshDownloads);
                }
                let mut download_failed = false;
                for mut registration in result.downloaded {
                    registration.task_ids.extend(
                        crate::song_download::attach_download_registrations(
                            &mut self.jobs.download_registrations,
                            &registration.scan_path,
                        ),
                    );
                    for id in registration.task_ids {
                        self.jobs.chart_downloads.finish_scan(id, &registration.identities, None);
                        download_failed |= self.finish_download_selection(id);
                    }
                }
                if download_failed {
                    self.show_left_overlay_toast(
                        Localizer::new(self.boot.profile_config.ui.locale())
                            .text("toast-chart-download-failed"),
                    );
                }
                keep_pending = false;
            }
            Ok(Err(error)) => {
                let mut ids = pending.download_ids.clone();
                for dir in &pending.download_dirs {
                    ids.extend(crate::song_download::attach_download_registrations(
                        &mut self.jobs.download_registrations,
                        dir,
                    ));
                }
                for id in &ids {
                    self.jobs.chart_downloads.finish_scan(*id, &[], Some(&format!("{error:#}")));
                }
                self.song_scan_error(&error);
                self.jobs.song_scan.progress = None;
                keep_pending = false;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                let mut ids = pending.download_ids.clone();
                for dir in &pending.download_dirs {
                    ids.extend(crate::song_download::attach_download_registrations(
                        &mut self.jobs.download_registrations,
                        dir,
                    ));
                }
                for id in &ids {
                    self.jobs.chart_downloads.finish_scan(
                        *id,
                        &[],
                        Some("scan worker disconnected"),
                    );
                }
                tracing::warn!("song scan worker disconnected");
                self.jobs.song_scan.progress = None;
                keep_pending = false;
            }
        }
        if keep_pending {
            self.jobs.song_scan.pending = Some(pending);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song_download::{
        ChartDownloadIdentity, ChartDownloadManager, ChartDownloadRequest, ChartDownloadResult,
        ChartDownloadSource, DownloadState, MissingChartAction,
    };

    fn submit(manager: &mut ChartDownloadManager, state: DownloadState) -> u64 {
        let request_id = manager.tasks().len() as u64;
        let (id, merged) = manager.submit(ChartDownloadRequest {
            request_id,
            identity: ChartDownloadIdentity { sha256: Some([request_id as u8; 32]), md5: None },
            title: format!("Chart {request_id}"),
            save_dir: PathBuf::new(),
            keep_archive: true,
            action: MissingChartAction::Http {
                api_url: format!("https://download.test/{request_id}.zip"),
                md5: String::new(),
                sha256: String::new(),
            },
        });
        assert!(!merged);
        manager.task(id).unwrap().control.set_state(state);
        id
    }

    fn pending(download_ids: Vec<u64>) -> PendingSongScan {
        let (_, finished) = mpsc::channel();
        PendingSongScan {
            finished,
            progress: Arc::new(AtomicU64::new(pack_scan_progress(ScanProgress::default()))),
            library_roots: None,
            download_ids,
            download_dirs: Vec::new(),
        }
    }

    fn fail_to_spawn() -> std::io::Result<thread::JoinHandle<()>> {
        Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "thread limit reached"))
    }

    fn request(label: &str, path: &Path) -> QueuedSongScan {
        QueuedSongScan {
            roots: vec![PathEntry {
                path: path.to_string_lossy().into_owned(),
                enabled: true,
                recursive: true,
            }],
            force: true,
            label: label.into(),
            scope: SongScanScope::Paths,
            start_failures: 0,
        }
    }

    fn registration(
        manager: &ChartDownloadManager,
        id: u64,
        path: &Path,
    ) -> (u64, ChartDownloadResult) {
        (
            id,
            ChartDownloadResult {
                archive_cache_key: None,
                archive_sha256: None,
                request_id: id,
                identity: manager.task(id).unwrap().requests[0].identity.clone(),
                source: ChartDownloadSource::Http,
                root_dir: path.parent().unwrap().into(),
                scan_path: path.into(),
            },
        )
    }

    fn assert_deferred(outcome: SongScanStart, should_notify: bool) {
        let SongScanStart::Deferred { error, notify } = outcome else {
            panic!("scan should be deferred");
        };
        assert_eq!(notify, should_notify);
        let message = format!("{error:#}");
        assert!(message.contains("failed to spawn song scan thread"));
        assert!(message.contains("thread limit reached"));
    }

    fn assert_start_failed(outcome: SongScanStart) -> String {
        let SongScanStart::Failed { error } = outcome else {
            panic!("scan should fail after five unsuccessful starts");
        };
        let message = format!("{error:#}");
        assert!(message.contains("after 5 attempts"));
        assert!(message.contains("failed to spawn song scan thread"));
        assert!(message.contains("thread limit reached"));
        message
    }

    fn start_successfully(
        scan: &mut SongScanRuntime,
        request: QueuedSongScan,
        ids: Vec<u64>,
        registrations: &mut Vec<(u64, ChartDownloadResult)>,
        downloads: &mut ChartDownloadManager,
        now: Instant,
    ) {
        let (tx, rx) = mpsc::channel();
        let pending = PendingSongScan { finished: rx, ..pending(ids) };
        assert!(matches!(
            scan.start_worker(pending, request, registrations, downloads, now, || {
                thread::Builder::new().spawn(move || {
                    tx.send(Ok(SongScanResult {
                        report: ScanReport::default(),
                        downloaded: vec![],
                    }))
                    .unwrap();
                })
            }),
            SongScanStart::Started
        ));
        scan.pending
            .as_ref()
            .unwrap()
            .finished
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(scan.progress.is_some());
        assert!(scan.retry_at.is_none());
    }

    #[test]
    fn spawn_failure_preserves_downloads_until_scanning_can_resume() {
        let root = tempfile::tempdir().unwrap();
        let saved = root.path().join("saved.zip");
        let other_path = root.path().join("other.zip");
        std::fs::write(&saved, b"saved archive bytes").unwrap();
        let mut downloads = ChartDownloadManager::default();
        let mut ids = vec![
            submit(&mut downloads, DownloadState::Scanning),
            submit(&mut downloads, DownloadState::Scanning),
        ];
        let unrelated = submit(&mut downloads, DownloadState::Scanning);
        let mut registrations: Vec<_> =
            ids.iter().map(|id| registration(&downloads, *id, &saved)).collect();
        registrations.push(registration(&downloads, unrelated, &other_path));
        let mut scan = SongScanRuntime::default();
        scan.queued.push_back(request("first", &saved));
        scan.queued.push_back(request("second", &other_path));
        let now = Instant::now();
        let first = scan.take_ready(now).unwrap();

        assert_deferred(
            scan.start_worker(
                pending(ids.clone()),
                first,
                &mut registrations,
                &mut downloads,
                now,
                fail_to_spawn,
            ),
            true,
        );
        assert!(scan.pending.is_none());
        assert!(scan.progress.is_none());
        assert_eq!(registrations.len(), 3);
        assert!(scan.has_download_scan_for(&saved));
        assert!(scan.has_download_scan_for(&other_path));
        for id in ids.iter().chain([&unrelated]) {
            let task = downloads.task(*id).unwrap();
            assert_eq!(task.state(), DownloadState::Scanning);
            assert!(task.error.is_none());
            assert!(task.finished_at.is_none());
            assert!(!task.registration_missing);
        }

        // New requests join the same FIFO; another chart in the saved archive can join its scan.
        scan.queued.push_back(request("third", &root.path().join("third.zip")));
        let late = submit(&mut downloads, DownloadState::Scanning);
        ids.push(late);
        registrations.push(registration(&downloads, late, &saved));
        for millis in [0, 16, 100, 1_000, 4_999] {
            assert!(scan.take_ready(now + Duration::from_millis(millis)).is_none());
            assert_eq!(
                scan.queued.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
                ["first", "second", "third"]
            );
            assert_eq!(registrations.len(), 4);
        }

        let ready_at = now + SONG_SCAN_START_RETRY_DELAY;
        let first = scan.take_ready(ready_at).unwrap();
        assert_eq!(first.label, "first");
        start_successfully(
            &mut scan,
            first,
            ids.clone(),
            &mut registrations,
            &mut downloads,
            ready_at,
        );
        assert_eq!(scan.pending.as_ref().unwrap().download_ids, ids);
        assert_eq!(registrations.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [unrelated]);
        assert!(scan.take_ready(ready_at + Duration::from_secs(10)).is_none());
        for id in ids {
            let identity = downloads.task(id).unwrap().requests[0].identity.clone();
            downloads.finish_scan(id, &[identity], None);
            assert_eq!(downloads.task(id).unwrap().state(), DownloadState::Completed);
        }
        assert_eq!(downloads.task(unrelated).unwrap().state(), DownloadState::Scanning);
        assert_eq!(std::fs::read(&saved).unwrap(), b"saved archive bytes");
        // Applying a worker result releases the next queued scan, without another delay.
        scan.pending = None;
        scan.progress = None;
        assert_eq!(scan.take_ready(ready_at).unwrap().label, "second");
        assert_eq!(scan.take_ready(ready_at).unwrap().label, "third");
        assert!(scan.take_ready(ready_at).is_none());
    }

    #[test]
    fn spawn_failure_limit_fails_only_affected_downloads_including_late_arrivals() {
        let root = tempfile::tempdir().unwrap();
        let saved = root.path().join("saved.zip");
        std::fs::write(&saved, b"saved archive bytes").unwrap();
        let other_path = root.path().join("other.zip");
        let mut downloads = ChartDownloadManager::default();
        let mut ids = vec![
            submit(&mut downloads, DownloadState::Scanning),
            submit(&mut downloads, DownloadState::Scanning),
        ];
        let unrelated = submit(&mut downloads, DownloadState::Scanning);
        let mut registrations: Vec<_> =
            ids.iter().map(|id| registration(&downloads, *id, &saved)).collect();
        registrations.push(registration(&downloads, unrelated, &other_path));
        let mut scan = SongScanRuntime::default();
        scan.queued.push_back(request("first", &saved));
        scan.queued.push_back(request("next", &other_path));
        let now = Instant::now();

        for attempt in 0..5 {
            if attempt == 4 {
                // Joining an existing scan must not restart its attempt budget.
                let late = submit(&mut downloads, DownloadState::Scanning);
                ids.push(late);
                registrations.push(registration(&downloads, late, &saved));
            }
            let at = now + SONG_SCAN_START_RETRY_DELAY * attempt;
            let first = scan.take_ready(at).unwrap();
            assert_eq!(first.label, "first");
            let outcome = scan.start_worker(
                pending(ids.clone()),
                first,
                &mut registrations,
                &mut downloads,
                at,
                fail_to_spawn,
            );
            if attempt < 4 {
                assert_deferred(outcome, attempt == 0);
                assert_eq!(registrations.len(), ids.len() + 1);
                for id in &ids {
                    let task = downloads.task(*id).unwrap();
                    assert_eq!(task.state(), DownloadState::Scanning);
                    assert!(task.error.is_none());
                    assert!(task.finished_at.is_none());
                }
            } else {
                let message = assert_start_failed(outcome);
                for id in &ids {
                    let task = downloads.task(*id).unwrap();
                    assert_eq!(task.state(), DownloadState::Failed);
                    assert_eq!(task.error.as_deref(), Some(message.as_str()));
                    assert!(task.finished_at.is_some());
                    assert!(!task.registration_missing);
                }
            }
            assert!(
                scan.take_ready(at + SONG_SCAN_START_RETRY_DELAY - Duration::from_nanos(1))
                    .is_none()
            );
        }
        assert!(scan.pending.is_none());
        assert!(scan.progress.is_none());
        assert!(!scan.has_download_scan_for(&saved));
        assert!(scan.has_download_scan_for(&other_path));
        assert_eq!(registrations.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [unrelated]);
        let task = downloads.task(unrelated).unwrap();
        assert_eq!(task.state(), DownloadState::Scanning);
        assert!(task.error.is_none());
        assert!(task.finished_at.is_none());
        assert_eq!(std::fs::read(&saved).unwrap(), b"saved archive bytes");

        // Recovery after a terminal failure releases the cooldown and lets the FIFO continue.
        let next_at = now + SONG_SCAN_START_RETRY_DELAY * 5;
        let next = scan.take_ready(next_at).unwrap();
        assert_eq!(next.label, "next");
        start_successfully(
            &mut scan,
            next,
            vec![unrelated],
            &mut registrations,
            &mut downloads,
            next_at,
        );
        assert!(registrations.is_empty());
    }

    #[test]
    fn spawn_failure_limit_releases_library_scan_but_keeps_cooldown() {
        let mut scan = SongScanRuntime::default();
        let mut registrations = Vec::new();
        let mut downloads = ChartDownloadManager::default();
        scan.queued.push_back(QueuedSongScan {
            scope: SongScanScope::Library,
            ..request("library", Path::new("library"))
        });
        scan.queued.push_back(request("next", Path::new("next")));
        let now = Instant::now();
        for attempt in 0..5 {
            let at = now + SONG_SCAN_START_RETRY_DELAY * attempt;
            let request = scan.take_ready(at).unwrap();
            assert_eq!(request.label, "library");
            let outcome = scan.start_worker(
                pending(vec![]),
                request,
                &mut registrations,
                &mut downloads,
                at,
                fail_to_spawn,
            );
            if attempt < 4 {
                assert_deferred(outcome, attempt == 0);
            } else {
                assert_start_failed(outcome);
            }
        }
        assert_eq!(
            scan.queued.iter().map(|request| request.label.as_str()).collect::<Vec<_>>(),
            ["next"],
            "a scan must stop blocking the queue after five failed starts"
        );
        assert!(scan.pending.is_none());
        assert!(scan.progress.is_none());
        let next_at = now + SONG_SCAN_START_RETRY_DELAY * 5;
        assert!(scan.take_ready(next_at - Duration::from_nanos(1)).is_none());
        // A new request has its own five attempts, even during a continuous outage.
        for attempt in 0..5 {
            let at = next_at + SONG_SCAN_START_RETRY_DELAY * attempt;
            let request = scan.take_ready(at).unwrap();
            assert_eq!(request.label, "next");
            let outcome = scan.start_worker(
                pending(vec![]),
                request,
                &mut registrations,
                &mut downloads,
                at,
                fail_to_spawn,
            );
            if attempt < 4 {
                assert_deferred(outcome, false);
            } else {
                assert_start_failed(outcome);
            }
        }
        assert!(scan.queued.is_empty());
    }

    #[test]
    fn repeated_spawn_failures_hold_library_scan_and_notify_once_until_recovery() {
        let mut scan = SongScanRuntime::default();
        let mut registrations = Vec::new();
        let mut downloads = ChartDownloadManager::default();
        scan.queued.push_back(QueuedSongScan {
            scope: SongScanScope::Library,
            ..request("library", Path::new("library"))
        });
        scan.queued.push_back(request("next", Path::new("next")));
        let now = Instant::now();
        for attempt in 0..4 {
            let at = now + SONG_SCAN_START_RETRY_DELAY * attempt;
            let request = scan.take_ready(at).unwrap();
            assert_eq!(request.label, "library");
            assert_eq!(request.scope, SongScanScope::Library);
            assert!(request.force);
            assert_eq!(request.roots[0].path, "library");
            let pending = PendingSongScan { library_roots: Some(Vec::new()), ..pending(vec![]) };
            assert_deferred(
                scan.start_worker(
                    pending,
                    request,
                    &mut registrations,
                    &mut downloads,
                    at,
                    fail_to_spawn,
                ),
                attempt == 0,
            );
            assert!(scan.pending.is_none());
            assert!(scan.progress.is_none());
            assert!(
                scan.take_ready(at + SONG_SCAN_START_RETRY_DELAY - Duration::from_nanos(1))
                    .is_none()
            );
            assert_eq!(scan.queued.len(), 2);
            assert!(registrations.is_empty());
        }
        let recovered_at = now + SONG_SCAN_START_RETRY_DELAY * 4;
        let request = scan.take_ready(recovered_at).unwrap();
        start_successfully(
            &mut scan,
            request,
            vec![],
            &mut registrations,
            &mut downloads,
            recovered_at,
        );
        scan.pending = None;
        scan.progress = None;
        let next = scan.take_ready(recovered_at).unwrap();
        assert_eq!(next.label, "next");
        // A new failure after recovery starts a new notification period.
        assert_deferred(
            scan.start_worker(
                pending(vec![]),
                next,
                &mut registrations,
                &mut downloads,
                recovered_at,
                fail_to_spawn,
            ),
            true,
        );
    }
}
