use super::*;

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
        self.jobs.song_scan.progress = Some(ScanProgress::default());
        thread::Builder::new()
            .name("song-scan".to_string())
            .spawn(move || {
                let result = (|| -> Result<ScanReport> {
                    migrate_library_db(&library_db_path)?;
                    let mut library_db = LibraryDatabase::open(&library_db_path)?;
                    let report = scan_songs_with_progress(
                        &mut library_db,
                        &roots,
                        &scan_config,
                        now_unix_seconds(),
                        force,
                        |progress| {
                            worker_progress.store(pack_scan_progress(progress), Ordering::Relaxed);
                        },
                    )?;
                    if scope == SongScanScope::Paths {
                        library_db.register_partial_song_roots(&roots)?;
                    }
                    Ok(report)
                })();
                let _ = tx.send(result);
            })
            .expect("failed to spawn song scan thread");
        self.jobs.song_scan.pending =
            Some(PendingSongScan { finished: rx, progress, library_roots });
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
            Ok(Ok(mut report)) => {
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
                self.reload_select_items();
                keep_pending = false;
            }
            Ok(Err(error)) => {
                self.song_scan_error(&error);
                self.jobs.song_scan.progress = None;
                keep_pending = false;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
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
