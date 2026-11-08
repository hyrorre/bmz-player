use super::*;

impl SongScanRuntime {
    fn start_worker(
        &mut self,
        pending: PendingSongScan,
        downloads: &mut crate::song_download::ChartDownloadManager,
        spawn: impl FnOnce() -> std::io::Result<thread::JoinHandle<()>>,
    ) -> Result<()> {
        self.progress = Some(ScanProgress::default());
        if let Err(error) = spawn().context("failed to spawn song scan thread") {
            // These registrations have already left the queue. Without a worker,
            // finish them here so they do not remain stuck in Scanning.
            let message = format!("{error:#}");
            for id in &pending.download_ids {
                downloads.finish_scan(*id, &[], Some(&message));
            }
            self.progress = None;
            return Err(error);
        }
        self.pending = Some(pending);
        Ok(())
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
            self.spawn_song_scan(roots, true, "select song reload".to_string());
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
        mut roots: Vec<PathEntry>,
        force: bool,
        label: String,
        scope: SongScanScope,
    ) {
        if !self.select_maintenance_allowed() || self.jobs.song_scan.pending.is_some() {
            self.jobs.song_scan.queued.push_back((roots, force, label.clone(), scope));
            tracing::debug!(
                %label,
                queued = self.jobs.song_scan.queued.len(),
                "queued song scan until Select maintenance is available"
            );
            return;
        }
        let library_roots = if scope == SongScanScope::Library {
            // A full sync is authoritative only after the settings were saved.
            if let Err(error) =
                save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config)
                    .and_then(|()| self.publish_song_scope())
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
        let mut downloads = Vec::new();
        self.jobs.download_registrations.retain(|(id, result)| {
            if roots.iter().any(|root| {
                root.enabled && root.recursive && Path::new(&root.path) == result.scan_path
            }) {
                downloads.push((*id, result.clone()));
                false
            } else {
                true
            }
        });
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
        let started =
            self.jobs.song_scan.start_worker(pending, &mut self.jobs.chart_downloads, || {
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
            });
        if let Err(error) = started {
            self.song_scan_error(&error);
            return;
        }
        tracing::info!(%label, force, "started song scan");
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
                        "song scan complete with skipped paths"
                    );
                }
                self.jobs.song_scan.progress = None;
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
        ChartDownloadIdentity, ChartDownloadManager, ChartDownloadRequest, DownloadState,
        MissingChartAction,
    };

    fn submit(manager: &mut ChartDownloadManager, state: DownloadState) -> u64 {
        let request_id = manager.tasks().len() as u64;
        let (id, merged) = manager.submit(ChartDownloadRequest {
            request_id,
            identity: ChartDownloadIdentity::default(),
            title: format!("Chart {request_id}"),
            download_root: PathBuf::new(),
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

    #[test]
    fn spawn_failure_fails_downloads_and_allows_retry_and_next_scan() {
        let mut downloads = ChartDownloadManager::default();
        let ids = [
            submit(&mut downloads, DownloadState::Scanning),
            submit(&mut downloads, DownloadState::Scanning),
        ];
        let unrelated = submit(&mut downloads, DownloadState::Queued);
        let mut scan = SongScanRuntime::default();
        scan.queued.push_back((Vec::new(), true, "next scan".into(), SongScanScope::Paths));

        let error =
            scan.start_worker(pending(ids.to_vec()), &mut downloads, fail_to_spawn).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("failed to spawn song scan thread"));
        assert!(message.contains("thread limit reached"));
        assert!(scan.pending.is_none());
        assert!(scan.progress.is_none());
        assert_eq!(scan.queued.len(), 1);
        assert_eq!(scan.queued[0].2, "next scan");
        assert_eq!(downloads.task(unrelated).unwrap().state(), DownloadState::Queued);
        for id in ids {
            let task = downloads.task(id).unwrap();
            assert_eq!(task.state(), DownloadState::Failed);
            assert_eq!(task.error.as_deref(), Some(message.as_str()));
            assert!(task.finished_at.is_some());
            // A spawn error is a registration error, not a missing-chart result.
            assert!(!task.registration_missing);
            downloads.retry(id);
            assert_eq!(downloads.task(id).unwrap().state(), DownloadState::Queued);
            assert!(downloads.task(id).unwrap().error.is_none());
        }

        let (tx, rx) = mpsc::channel();
        let next = PendingSongScan { finished: rx, ..pending(Vec::new()) };
        scan.start_worker(next, &mut downloads, || {
            thread::Builder::new().spawn(move || {
                tx.send(Ok(SongScanResult { report: ScanReport::default(), downloaded: vec![] }))
                    .unwrap();
            })
        })
        .unwrap();
        assert!(scan.progress.is_some());
        scan.pending
            .as_ref()
            .unwrap()
            .finished
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
    }

    #[test]
    fn spawn_failure_without_downloads_clears_scan_progress() {
        let mut scan = SongScanRuntime::default();
        let mut downloads = ChartDownloadManager::default();
        let scan_request = PendingSongScan { library_roots: Some(Vec::new()), ..pending(vec![]) };
        assert!(scan.start_worker(scan_request, &mut downloads, fail_to_spawn).is_err());
        assert!(scan.pending.is_none());
        assert!(scan.progress.is_none());
        assert!(downloads.tasks().is_empty());
    }
}
