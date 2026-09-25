use super::*;

impl WinitApp {
    pub(super) fn reload_select_items(&mut self) {
        // Song scans and table/course updates all converge here. Keep the editor
        // cache until an actual library refresh instead of querying it per frame.
        self.course_editor_cache.invalidate();
        if self.select.score_refresh.take_dirty() {
            self.invalidate_select_folder_summaries();
        }
        self.select.select_folder_summaries.sync_view(&self.select.folder_stack);
        let previous_selected_key =
            self.select.select_items.get(self.select.selected_index).map(select_item_key);
        let history: Vec<String> = self.select.search.history().iter().cloned().collect();
        let (items, resolved_mode_filter) = load_items_for_stack(
            &self.boot,
            &self.select.folder_stack,
            &history,
            self.select.select_mode_filter,
            self.select.select_difficulty_filter,
            self.select.select_sort,
        );
        // beatoraja 準拠の自動送りで mode filter が変わることがあるので、
        // 表示状態と永続化用 profile config を実際に適用したモードへ揃える。
        self.select.select_mode_filter = resolved_mode_filter;
        self.boot.profile_config.select.mode_filter = resolved_mode_filter.as_str().to_string();
        self.select.select_items = items;
        // Resolve once on refresh so preview/images and playback share the same copy.
        for item in &mut self.select.select_items {
            if let SelectItem::Chart(row) = item
                && let Some(chart) = &row.chart
            {
                match self.boot.library_db.available_chart_source(chart.chart_id) {
                    Ok(Some(source)) if source.chart.chart_id != chart.chart_id => {
                        row.has_document = source.chart.has_document;
                        row.chart_analysis = self
                            .boot
                            .library_db
                            .chart_analysis_summaries_by_chart_ids(&[source.chart.chart_id])
                            .ok()
                            .and_then(|mut rows| rows.remove(&source.chart.chart_id));
                        row.chart = Some(source.chart);
                    }
                    _ => {}
                }
            }
        }
        if self.select.folder_stack.last().and_then(|path| parse_search_query(path)).is_some() {
            let count = self
                .select
                .select_items
                .iter()
                .filter(|item| matches!(item, SelectItem::Chart(_)))
                .count();
            let mut args = FluentArgs::new();
            args.set("count", count as i64);
            self.select.search.set_message(
                Localizer::new(self.boot.profile_config.ui.locale())
                    .format("select-search-results", &args),
            );
        }
        self.select.replay_slot_cache.replace(None);
        self.select.select_distribution_cache.borrow_mut().clear();
        self.select.selected_index = restored_select_index(
            &self.select.select_items,
            previous_selected_key.as_ref(),
            self.select.selected_index,
        );
        self.sync_selected_play_mode();
        self.normalize_selected_replay_slot();
    }

    pub(super) fn invalidate_select_folder_summaries(&mut self) {
        self.select.select_folder_summaries.invalidate_data();
    }

    pub(super) fn load_songs_and_reload(&mut self) {
        self.spawn_song_scan_with_scope(
            Vec::new(),
            true,
            "library rescan".to_string(),
            SongScanScope::Library,
        );
    }

    pub(super) fn import_external_scores(&mut self, request: ScoreImportRequest) {
        let label = request.kind.label();
        let path = request.path.display().to_string();
        match import_scores(
            &request,
            &mut self.boot.library_db,
            &mut self.boot.score_db,
            now_unix_seconds(),
        ) {
            Ok(report) => {
                let summary = report.summary();
                tracing::info!(kind = label, path, summary, "external scores imported");
                self.refresh_player_stats_snapshot();
                self.invalidate_select_folder_summaries();
                self.reload_select_items();
                if let Some(egui) = self.ui.egui.as_mut() {
                    let mut args = FluentArgs::new();
                    args.set("label", label);
                    args.set("path", request.path.display().to_string());
                    args.set(
                        "summary",
                        report.summary_for_locale(self.boot.profile_config.ui.locale()),
                    );
                    egui.set_score_import_status(
                        Localizer::new(self.boot.profile_config.ui.locale())
                            .format("score-import-success", &args),
                        report.failed > 0,
                    );
                }
            }
            Err(error) => {
                let mut args = FluentArgs::new();
                args.set("label", label);
                args.set("error", error.to_string());
                let message = Localizer::new(self.boot.profile_config.ui.locale())
                    .format("score-import-failed", &args);
                tracing::error!(kind = label, path, error = %format_error_chain(&error), "external score import failed");
                if let Some(egui) = self.ui.egui.as_mut() {
                    egui.set_score_import_status(message, true);
                }
            }
        }
    }

    pub(super) fn spawn_beatoraja_replay_import(&mut self, request: ImportBeatorajaReplaysRequest) {
        if self.jobs.pending_replay_import.is_some() {
            if let Some(egui) = self.ui.egui.as_mut() {
                egui.set_replay_import_status("replay import is already running".to_string(), true);
            }
            return;
        }

        let path = request.source.display().to_string();
        let library_db_path = self.boot.app_paths.library_db.clone();
        let score_db_path = self.boot.profile_paths.score_db.clone();
        let profile_paths = self.boot.profile_paths.clone();
        let logs_dir = self.boot.app_paths.logs_dir.clone();
        let (tx, rx) = mpsc::channel();
        let done = Arc::new(AtomicU32::new(0));
        let total = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_done = Arc::clone(&done);
        let worker_total = Arc::clone(&total);
        let worker_cancel = Arc::clone(&cancel);
        thread::Builder::new()
            .name("beatoraja-replay-import".to_string())
            .spawn(move || {
                let result = (|| -> Result<ReplayImportReport> {
                    migrate_library_db(&library_db_path)?;
                    migrate_score_db(&score_db_path)?;
                    let library_db = LibraryDatabase::open(&library_db_path)?;
                    let mut score_db = ScoreDatabase::open(&score_db_path)?;
                    let mut report = import_beatoraja_replays_with_progress(
                        &library_db,
                        &mut score_db,
                        &profile_paths,
                        &request,
                        |progress| {
                            worker_done.store(
                                u32::try_from(progress.done).unwrap_or(u32::MAX),
                                Ordering::Relaxed,
                            );
                            worker_total.store(
                                u32::try_from(progress.total).unwrap_or(u32::MAX),
                                Ordering::Relaxed,
                            );
                        },
                        || worker_cancel.load(Ordering::Relaxed),
                    )?;
                    if !report.issues.is_empty() {
                        match write_replay_import_details(&logs_dir, &request.source, &report) {
                            Ok(path) => report.details_path = Some(path),
                            Err(error) => {
                                tracing::warn!(%error, "failed to write replay import details")
                            }
                        }
                    }
                    Ok(report)
                })();
                let _ = tx.send(result);
            })
            .expect("failed to spawn beatoraja replay import thread");
        self.jobs.pending_replay_import =
            Some(PendingReplayImport { finished: rx, done, total, cancel });
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.set_replay_import_progress(Some(ReplayImportProgress::default()));
            egui.set_replay_import_status(format!("importing {path}"), false);
        }
        tracing::info!(path, "started beatoraja replay import");
    }

    pub(super) fn cancel_beatoraja_replay_import(&mut self) {
        let Some(pending) = &self.jobs.pending_replay_import else {
            return;
        };
        pending.cancel.store(true, Ordering::Relaxed);
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.set_replay_import_status("cancelling replay import...".to_string(), false);
        }
    }

    pub(super) fn poll_pending_replay_import(&mut self) {
        let Some(pending) = self.jobs.pending_replay_import.take() else {
            return;
        };
        let progress = ReplayImportProgress {
            done: pending.done.load(Ordering::Relaxed) as usize,
            total: pending.total.load(Ordering::Relaxed) as usize,
        };
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.set_replay_import_progress(Some(progress));
        }

        let mut keep_pending = true;
        match pending.finished.try_recv() {
            Ok(Ok(report)) => {
                let summary = report.summary();
                tracing::info!(summary, "beatoraja replays imported");
                for issue in &report.issues {
                    tracing::warn!(
                        replay_path = %issue.path.display(),
                        issue_kind = ?issue.kind,
                        message = %issue.message,
                        "beatoraja replay was not imported"
                    );
                }
                self.reload_select_items();
                if let Some(egui) = self.ui.egui.as_mut() {
                    egui.set_replay_import_progress(None);
                    let status = match &report.threshold_warning {
                        Some(warning) => format!("{summary}\nwarning: {warning}"),
                        None => summary,
                    };
                    let status = match &report.details_path {
                        Some(path) => format!("{status}\ndetails: {}", path.display()),
                        None => status,
                    };
                    egui.set_replay_import_status(status, false);
                }
                keep_pending = false;
            }
            Ok(Err(error)) => {
                tracing::error!(error = %format_error_chain(&error), "beatoraja replay import failed");
                if let Some(egui) = self.ui.egui.as_mut() {
                    egui.set_replay_import_progress(None);
                    egui.set_replay_import_status(format!("import failed: {error:#}"), true);
                }
                keep_pending = false;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("beatoraja replay import worker disconnected");
                if let Some(egui) = self.ui.egui.as_mut() {
                    egui.set_replay_import_progress(None);
                    egui.set_replay_import_status(
                        "replay import worker disconnected".to_string(),
                        true,
                    );
                }
                keep_pending = false;
            }
        }
        if keep_pending {
            self.jobs.pending_replay_import = Some(pending);
        }
    }

    pub(super) fn publish_song_scope(&self) -> Result<()> {
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

    pub(super) fn reload_from_select_context(&mut self) {
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

    pub(super) fn spawn_song_scan_request(&mut self, request: SongScanRequest) {
        self.spawn_song_scan(request.roots, request.force, request.label);
    }

    pub(super) fn spawn_song_scan(&mut self, roots: Vec<PathEntry>, force: bool, label: String) {
        self.spawn_song_scan_with_scope(roots, force, label, SongScanScope::Paths);
    }

    pub(super) fn spawn_song_scan_with_scope(
        &mut self,
        mut roots: Vec<PathEntry>,
        force: bool,
        label: String,
        scope: SongScanScope,
    ) {
        if !self.select_maintenance_allowed() || self.jobs.pending_song_scan.is_some() {
            self.jobs.queued_song_scans.push_back((roots, force, label.clone(), scope));
            tracing::debug!(
                %label,
                queued = self.jobs.queued_song_scans.len(),
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
        self.jobs.song_scan_progress = Some(ScanProgress::default());
        thread::Builder::new()
            .name("song-scan".to_string())
            .spawn(move || {
                let result = (|| -> Result<ScanReport> {
                    migrate_library_db(&library_db_path)?;
                    let mut library_db = LibraryDatabase::open(&library_db_path)?;
                    scan_songs_with_progress(
                        &mut library_db,
                        &roots,
                        &scan_config,
                        now_unix_seconds(),
                        force,
                        |progress| {
                            worker_progress.store(pack_scan_progress(progress), Ordering::Relaxed);
                        },
                    )
                })();
                let _ = tx.send(result);
            })
            .expect("failed to spawn song scan thread");
        self.jobs.pending_song_scan =
            Some(PendingSongScan { finished: rx, progress, library_roots });
        tracing::info!(%label, force, "started song scan");
    }

    pub(super) fn poll_pending_song_scan(&mut self) {
        let Some(pending) = self.jobs.pending_song_scan.take() else {
            return;
        };
        self.jobs.song_scan_progress =
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
                self.jobs.song_scan_progress = None;
                self.select.select_assets.invalidate_library();
                self.invalidate_select_folder_summaries();
                self.reload_select_items();
                keep_pending = false;
            }
            Ok(Err(error)) => {
                self.song_scan_error(&error);
                self.jobs.song_scan_progress = None;
                keep_pending = false;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("song scan worker disconnected");
                self.jobs.song_scan_progress = None;
                keep_pending = false;
            }
        }
        if keep_pending {
            self.jobs.pending_song_scan = Some(pending);
        }
    }

    pub(super) fn spawn_table_fetch(&mut self, url: String) {
        self.spawn_table_fetches(vec![url], "table fetch".to_string());
    }

    pub(super) fn start_startup_course_link_repair_after_first_frame(&mut self) {
        if !self.select_maintenance_allowed()
            || !self.jobs.startup_course_link_repair
            || self.jobs.pending_course_link_repair.is_some()
        {
            return;
        }
        self.jobs.startup_course_link_repair = false;
        let library_db_path = self.boot.app_paths.library_db.clone();
        let (tx, rx) = mpsc::channel();
        let event_proxy = self.event_proxy.clone();
        match thread::Builder::new().name("course-link-repair".to_string()).spawn(move || {
            let result =
                crate::storage::migration::repair_course_entry_chart_links_once(&library_db_path);
            let _ = tx.send(result);
            let _ = event_proxy.send_event(AppUserEvent::CourseLinkRepair);
        }) {
            Ok(_) => {
                self.jobs.pending_course_link_repair = Some(rx);
                tracing::info!("started one-time course link repair worker");
            }
            Err(error) => {
                tracing::warn!(%error, "failed to start course link repair worker");
            }
        }
    }

    pub(super) fn poll_pending_course_link_repair(&mut self) {
        let Some(rx) = self.jobs.pending_course_link_repair.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(run)) => {
                tracing::info!(
                    already_completed = run.already_completed,
                    scanned_entries = run.scanned_entries,
                    repaired_entries = run.repaired_entries,
                    "course link repair worker complete"
                );
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "course link repair worker failed");
                // 次回起動ではmaintenance markerが無いため再試行される。
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.jobs.pending_course_link_repair = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("course link repair worker disconnected");
            }
        }
    }

    /// 起動直後の初回描画が完了してから、未取得の有効な表を取得する。
    pub(super) fn start_startup_table_fetch_after_first_frame(&mut self) {
        if !self.select_maintenance_allowed() {
            return;
        }
        let Some(urls) = self.jobs.table_fetch.startup_urls.take() else {
            return;
        };
        self.spawn_table_fetches(urls, "startup table fetch".to_string());
        self.spawn_rian_table_fetch(false);
    }

    pub(super) fn spawn_rian_table_fetch(&mut self, manual: bool) {
        if !self.select_maintenance_allowed() {
            self.jobs.table_fetch.rian_refresh_queued = true;
            self.jobs.table_fetch.rian_refresh_manual |= manual;
            tracing::debug!(manual, "queued rianIR table refresh until Select");
            return;
        }
        let Some(identity) = self.jobs.table_fetch.rian_identity.clone() else {
            return;
        };
        if self.jobs.table_fetch.pending_rian.is_some() {
            tracing::debug!("rianIR table fetch already in progress");
            return;
        }
        let now = Instant::now();
        let minimum_interval =
            if manual { RIAN_TABLE_MANUAL_REFRESH_COOLDOWN } else { RIAN_TABLE_REFRESH_INTERVAL };
        if self
            .jobs
            .table_fetch
            .rian_last_started_at
            .is_some_and(|started| now.duration_since(started) < minimum_interval)
        {
            return;
        }

        let generation = self.jobs.table_fetch.rian_generation;
        let fetched_at = now_unix_seconds();
        let (tx, rx) = mpsc::channel();
        let event_proxy = self.event_proxy.clone();
        let worker_identity = identity.clone();
        let worker_profile_root = self.boot.profile_paths.root_dir.clone();
        let mut maintenance_allowed = self.jobs.maintenance_select_tx.subscribe();
        thread::Builder::new()
            .name("rian-table-fetch".to_string())
            .spawn(move || {
                let result = match tokio::runtime::Runtime::new()
                    .context("failed to create tokio runtime")
                {
                    Err(error) => RianTableFetchOutcome::Completed(Err(error)),
                    Ok(runtime) => runtime.block_on(async {
                        tokio::select! {
                            biased;
                            _ = async {
                                while *maintenance_allowed.borrow() {
                                    if maintenance_allowed.changed().await.is_err() {
                                        break;
                                    }
                                }
                            } => RianTableFetchOutcome::Paused,
                            result = crate::ir::table::fetch_account_tables(
                                &worker_identity,
                                &worker_profile_root,
                                fetched_at,
                            ) => RianTableFetchOutcome::Completed(result),
                        }
                    }),
                };
                let _ = tx.send(RianTableFetchWorkerResult {
                    generation,
                    identity: worker_identity,
                    result,
                });
                let _ = event_proxy.send_event(AppUserEvent::TableFetch);
            })
            .expect("failed to spawn rianIR table fetch thread");
        self.jobs.table_fetch.pending_rian = Some(rx);
        self.jobs.table_fetch.rian_last_started_at = Some(now);
        self.jobs.table_fetch.rian_next_refresh_at = now.checked_add(RIAN_TABLE_REFRESH_INTERVAL);
        tracing::info!(
            provider = %identity.provider_key,
            manual,
            "started rianIR table fetch"
        );
    }

    pub(super) fn poll_pending_rian_table_fetch(&mut self) {
        let Some(rx) = self.jobs.table_fetch.pending_rian.take() else {
            return;
        };
        let provider_name = self
            .jobs
            .table_fetch
            .rian_identity
            .as_ref()
            .map_or("IR", RianTableIdentity::display_name);
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                self.jobs.table_fetch.pending_rian = Some(rx);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("rianIR table fetch worker disconnected");
                self.show_left_overlay_toast(format!("{provider_name} TABLE: worker disconnected"));
                return;
            }
        };

        if result.generation != self.jobs.table_fetch.rian_generation
            || self.jobs.table_fetch.rian_identity.as_ref() != Some(&result.identity)
        {
            tracing::info!("ignored stale rianIR table fetch result");
            return;
        }

        let provider_name = result.identity.display_name();
        match result.result {
            RianTableFetchOutcome::Completed(Ok(tables)) => {
                match crate::ir::table::store_account_tables(
                    &mut self.boot.library_db,
                    &result.identity,
                    &tables,
                ) {
                    Ok((table_count, entry_count)) => {
                        tracing::info!(
                            tables = table_count,
                            entries = entry_count,
                            "rianIR table fetch complete"
                        );
                        self.refresh_difficulty_tables_and_select();
                        self.show_left_overlay_toast(format!(
                            "{provider_name} TABLE: {table_count} tables, {entry_count} entries"
                        ));
                    }
                    Err(error) => {
                        tracing::error!(%error, "failed to store rianIR tables");
                        self.show_left_overlay_toast(format!(
                            "{provider_name} TABLE: cache update failed"
                        ));
                    }
                }
            }
            RianTableFetchOutcome::Completed(Err(error)) => {
                // stale-while-revalidate: 既存キャッシュは消さず、そのまま選曲に残す。
                tracing::warn!(%error, "failed to fetch rianIR tables; keeping cached tables");
                self.show_left_overlay_toast(format!(
                    "{provider_name} TABLE: fetch failed (using cache)"
                ));
            }
            RianTableFetchOutcome::Paused => {
                self.jobs.table_fetch.rian_last_started_at = None;
                self.jobs.table_fetch.rian_next_refresh_at = None;
                self.jobs.table_fetch.rian_refresh_queued = true;
                tracing::debug!("paused rianIR table fetch outside Select");
            }
        }
    }

    pub(super) fn maybe_start_periodic_rian_table_fetch(&mut self) {
        if !self.select_maintenance_allowed() {
            return;
        }
        if self.jobs.table_fetch.pending_rian.is_some() {
            return;
        }
        if self
            .jobs
            .table_fetch
            .rian_next_refresh_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.spawn_rian_table_fetch(false);
        }
    }

    pub(super) fn start_queued_rian_table_fetch_if_idle(&mut self) {
        if self.jobs.table_fetch.pending_rian.is_some()
            || !self.jobs.table_fetch.rian_refresh_queued
        {
            return;
        }
        let manual = std::mem::take(&mut self.jobs.table_fetch.rian_refresh_manual);
        self.jobs.table_fetch.rian_refresh_queued = false;
        self.spawn_rian_table_fetch(manual);
    }

    pub(super) fn reconcile_rian_table_identity(&mut self) {
        let next = RianTableIdentity::from_ir_config(&self.boot.profile_config.ir);
        if next == self.jobs.table_fetch.rian_identity {
            return;
        }
        if !self.select_maintenance_allowed() {
            self.jobs.table_fetch.rian_refresh_queued = true;
            return;
        }

        let previous = self.jobs.table_fetch.rian_identity.take();
        self.jobs.table_fetch.rian_generation =
            self.jobs.table_fetch.rian_generation.wrapping_add(1);
        self.jobs.table_fetch.pending_rian = None;
        self.jobs.table_fetch.rian_last_started_at = None;
        self.jobs.table_fetch.rian_next_refresh_at = None;
        self.jobs.table_fetch.rian_refresh_queued = false;
        self.jobs.table_fetch.rian_refresh_manual = false;

        if let Some(previous) = &previous {
            match self
                .boot
                .library_db
                .delete_difficulty_tables_by_source_prefix(previous.source_prefix())
            {
                Ok(removed) => tracing::info!(
                    removed,
                    "removed rianIR account table cache after identity change"
                ),
                Err(error) => tracing::warn!(%error, "failed to remove old rianIR table cache"),
            }
            if let Err(error) =
                self.boot.library_db.delete_table_courses_by_source_prefix(previous.source_prefix())
            {
                tracing::warn!(%error, "failed to remove old rianIR course cache");
            }
            if self.select.folder_stack.iter().any(|path| path.contains(previous.source_prefix())) {
                self.select.folder_stack.clear();
                self.select.selected_index_stack.clear();
                self.select.selected_index = 0;
                self.reset_selected_replay_slot();
            }
        }

        self.jobs.table_fetch.rian_identity = next;
        self.refresh_difficulty_tables_and_select();
        if self.first_frame_startup_completed {
            self.spawn_rian_table_fetch(true);
        }
    }

    pub(super) fn refresh_difficulty_tables_and_select(&mut self) {
        match self.boot.library_db.list_difficulty_tables() {
            Ok(tables) => self.select.difficulty_tables = tables,
            Err(error) => tracing::warn!(%error, "failed to refresh difficulty table metadata"),
        }
        self.select.table_breadcrumb_cache.borrow_mut().clear();
        self.invalidate_select_folder_summaries();
        self.reload_select_items();
    }

    pub(super) fn spawn_table_fetches(&mut self, urls: Vec<String>, label: String) {
        let urls = self.jobs.table_fetch.filter_new_urls(urls);
        if urls.is_empty() {
            return;
        }
        if !self.select_maintenance_allowed() || self.jobs.table_fetch.pending.is_some() {
            self.jobs.table_fetch.queued_urls.extend(urls);
            tracing::debug!(
                queued = self.jobs.table_fetch.queued_urls.len(),
                %label,
                "queued table fetch until Select maintenance is available"
            );
            return;
        }
        let (tx, rx) = mpsc::channel();
        let fetch_urls = urls.clone();
        let progress_tx = tx.clone();
        let event_proxy = self.event_proxy.clone();
        let maintenance_allowed = self.jobs.maintenance_select_tx.subscribe();
        thread::Builder::new()
            .name("table-fetch".to_string())
            .spawn(move || {
                let result = (|| -> Result<crate::table_cmd::TableFetchDownloadBatchResult> {
                    let rt =
                        tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                    rt.block_on(crate::table_cmd::download_table_urls_with_progress(
                        fetch_urls,
                        maintenance_allowed,
                        |outcome| {
                            let _ = progress_tx.send(TableFetchWorkerEvent::Downloaded(outcome));
                            let _ = event_proxy.send_event(AppUserEvent::TableFetch);
                        },
                    ))
                })();
                let _ = tx.send(TableFetchWorkerEvent::Finished(result));
                let _ = event_proxy.send_event(AppUserEvent::TableFetch);
            })
            .expect("failed to spawn table fetch thread");
        self.jobs.table_fetch.pending_urls = urls.iter().cloned().collect();
        self.jobs.table_fetch.progress = Some(TableFetchProgress {
            label: label.clone(),
            total: urls.len(),
            completed: 0,
            succeeded: 0,
            failed: 0,
            outcomes: Vec::with_capacity(urls.len()),
        });
        self.jobs.table_fetch.pending = Some(rx);
        tracing::info!(count = urls.len(), %label, "started table fetch");
    }

    pub(super) fn poll_pending_table_fetch(&mut self) {
        let Some(rx) = self.jobs.table_fetch.pending.take() else {
            return;
        };
        let mut keep_pending = true;
        loop {
            match rx.try_recv() {
                Ok(TableFetchWorkerEvent::Downloaded(downloaded)) => {
                    let outcome = match downloaded {
                        crate::table_cmd::TableFetchDownloadOutcome::Succeeded(table) => {
                            match crate::table_cmd::store_fetched_table(
                                &mut self.boot.library_db,
                                &table,
                            ) {
                                Ok(success) => TableFetchOutcome::Succeeded(success),
                                Err(error) => {
                                    TableFetchOutcome::Failed(crate::table_cmd::TableFetchFailure {
                                        url: table.source_url,
                                        error: format!(
                                            "failed to store difficulty table: {error:#}"
                                        ),
                                    })
                                }
                            }
                        }
                        crate::table_cmd::TableFetchDownloadOutcome::Failed(failure) => {
                            TableFetchOutcome::Failed(failure)
                        }
                    };
                    if let Some(progress) = &mut self.jobs.table_fetch.progress {
                        progress.completed =
                            progress.completed.saturating_add(1).min(progress.total);
                        match &outcome {
                            TableFetchOutcome::Succeeded(_) => progress.succeeded += 1,
                            TableFetchOutcome::Failed(_) => progress.failed += 1,
                        }
                        progress.outcomes.push(outcome.clone());
                    }
                    match &outcome {
                        TableFetchOutcome::Succeeded(success) => tracing::info!(
                            url = %success.url,
                            name = %success.name,
                            entries = success.entries,
                            courses = success.courses,
                            "difficulty table fetched"
                        ),
                        TableFetchOutcome::Failed(failure) => tracing::warn!(
                            url = %failure.url,
                            error = %failure.error,
                            "failed to fetch difficulty table"
                        ),
                    }
                }
                Ok(TableFetchWorkerEvent::Finished(Ok(batch))) => {
                    keep_pending = false;
                    let outcomes = self
                        .jobs
                        .table_fetch
                        .progress
                        .as_mut()
                        .map(|progress| std::mem::take(&mut progress.outcomes))
                        .unwrap_or_default();
                    let completed = batch.requested.saturating_sub(batch.remaining_urls.len());
                    if completed > 0 {
                        self.finish_table_fetch(TableFetchReport {
                            requested: completed,
                            outcomes,
                        });
                    }
                    if !batch.remaining_urls.is_empty() {
                        tracing::debug!(
                            remaining = batch.remaining_urls.len(),
                            "paused table fetch outside Select"
                        );
                        self.jobs.table_fetch.queued_urls.extend(batch.remaining_urls);
                    }
                    break;
                }
                Ok(TableFetchWorkerEvent::Finished(Err(error))) => {
                    keep_pending = false;
                    let label = self
                        .jobs
                        .table_fetch
                        .progress
                        .as_ref()
                        .map(|progress| progress.label.as_str())
                        .unwrap_or("table fetch");
                    tracing::error!(%label, %error, "table fetch worker failed");
                    self.show_left_overlay_toast("TABLE: fetch failed");
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    keep_pending = false;
                    tracing::warn!("table fetch worker disconnected");
                    self.show_left_overlay_toast("TABLE: worker disconnected");
                    break;
                }
            }
        }
        if keep_pending {
            self.jobs.table_fetch.pending = Some(rx);
            return;
        }

        self.jobs.table_fetch.pending_urls.clear();
        self.jobs.table_fetch.progress = None;
        self.start_queued_table_fetch_if_idle();
    }

    pub(super) fn start_queued_table_fetch_if_idle(&mut self) {
        if self.jobs.table_fetch.pending.is_some()
            || self.jobs.table_fetch.queued_urls.is_empty()
            || !self.select_maintenance_allowed()
        {
            return;
        }
        let queued = std::mem::take(&mut self.jobs.table_fetch.queued_urls);
        self.spawn_table_fetches(queued, "queued table fetch".to_string());
    }

    pub(super) fn finish_table_fetch(&mut self, report: TableFetchReport) {
        let succeeded = report.succeeded_count();
        let failed = report.failed_count();
        tracing::info!(requested = report.requested, succeeded, failed, "table fetch complete");
        if succeeded > 0 {
            match self.boot.library_db.list_difficulty_tables() {
                Ok(tables) => self.select.difficulty_tables = tables,
                Err(error) => {
                    tracing::warn!(%error, "failed to refresh difficulty table metadata")
                }
            }
            self.select.table_breadcrumb_cache.borrow_mut().clear();
            self.invalidate_select_folder_summaries();
            self.reload_select_items();
        }
        self.show_left_overlay_toast(format!("TABLE: {succeeded} succeeded, {failed} failed"));
    }

    pub(super) fn spawn_update_check(&mut self, label: &'static str, report_up_to_date: bool) {
        if !self.select_maintenance_allowed() {
            match &mut self.jobs.queued_update_check {
                Some((queued_label, queued_report)) => {
                    if report_up_to_date {
                        *queued_label = label;
                    }
                    *queued_report |= report_up_to_date;
                }
                None => self.jobs.queued_update_check = Some((label, report_up_to_date)),
            }
            tracing::debug!(label, "queued update check until Select");
            return;
        }
        if self.jobs.pending_update_check.is_some() {
            tracing::debug!(label, "update check already in progress");
            return;
        }
        let channel = self.boot.app_config.updates.channel;
        if crate::update::sparkle::available() {
            if let Err(error) = crate::update::sparkle::check(channel, report_up_to_date) {
                self.jobs.update_prompt =
                    Some(UpdatePrompt::Error { message: format!("{error:#}"), candidate: None });
            }
            return;
        }
        let (tx, rx) = mpsc::channel();
        let mut maintenance_allowed = self.jobs.maintenance_select_tx.subscribe();
        thread::Builder::new()
            .name("update-check".to_string())
            .spawn(move || {
                let result = match tokio::runtime::Runtime::new()
                    .context("failed to create tokio runtime")
                {
                    Err(error) => UpdateCheckWorkerResult::Failed(error),
                    Ok(runtime) => runtime.block_on(async {
                        tokio::select! {
                            biased;
                            _ = async {
                                while *maintenance_allowed.borrow() {
                                    if maintenance_allowed.changed().await.is_err() {
                                        break;
                                    }
                                }
                            } => UpdateCheckWorkerResult::Paused,
                            result = crate::update::check_for_update(channel) => {
                                match result {
                                    Ok(Some(candidate)) => {
                                        UpdateCheckWorkerResult::Available(Box::new(candidate))
                                    }
                                    Ok(None) => UpdateCheckWorkerResult::UpToDate,
                                    Err(error) => UpdateCheckWorkerResult::Failed(error),
                                }
                            }
                        }
                    }),
                };
                let _ = tx.send(result);
            })
            .expect("failed to spawn update check thread");
        self.jobs.pending_update_check = Some(rx);
        self.jobs.pending_update_check_reports_up_to_date = report_up_to_date;
        tracing::info!(?channel, label, "started update check");
    }

    pub(super) fn poll_pending_update_check(&mut self) {
        let Some(rx) = &self.jobs.pending_update_check else {
            return;
        };
        match rx.try_recv() {
            Ok(UpdateCheckWorkerResult::Available(candidate)) => {
                let candidate = *candidate;
                tracing::info!(version = %candidate.version, "update available");
                self.jobs.pending_update_check = None;
                self.jobs.pending_update_check_reports_up_to_date = false;
                if self.update_candidate_is_suppressed(&candidate) {
                    return;
                }
                self.jobs.update_prompt = Some(UpdatePrompt::Available(candidate));
                self.request_redraw();
            }
            Ok(UpdateCheckWorkerResult::UpToDate) => {
                tracing::info!("no update available");
                self.jobs.pending_update_check = None;
                if self.jobs.pending_update_check_reports_up_to_date {
                    self.jobs.update_prompt = Some(UpdatePrompt::UpToDate);
                    self.request_redraw();
                }
                self.jobs.pending_update_check_reports_up_to_date = false;
            }
            Ok(UpdateCheckWorkerResult::Failed(error)) => {
                tracing::warn!(%error, "update check failed");
                let report_error = self.jobs.pending_update_check_reports_up_to_date;
                self.jobs.pending_update_check = None;
                self.jobs.pending_update_check_reports_up_to_date = false;
                if report_error {
                    self.jobs.update_prompt = Some(UpdatePrompt::Error {
                        message: format!("{error:#}"),
                        candidate: None,
                    });
                    self.request_redraw();
                }
            }
            Ok(UpdateCheckWorkerResult::Paused) => {
                let report_up_to_date = self.jobs.pending_update_check_reports_up_to_date;
                self.jobs.pending_update_check = None;
                self.jobs.pending_update_check_reports_up_to_date = false;
                self.jobs.queued_update_check = Some(("resumed update check", report_up_to_date));
                tracing::debug!("paused update check outside Select");
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("update check worker disconnected");
                self.jobs.pending_update_check = None;
                self.jobs.pending_update_check_reports_up_to_date = false;
            }
        }
    }

    pub(super) fn spawn_update_download(&mut self, candidate: UpdateCandidate) {
        if self.jobs.pending_update_download.is_some() {
            tracing::debug!("update download already in progress");
            return;
        }
        let cache_dir = self.boot.app_paths.cache_dir.clone();
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(crate::update::DownloadProgress::default());
        self.jobs.update_progress = Some(Arc::clone(&progress));
        self.jobs.update_prompt =
            Some(UpdatePrompt::Downloading(candidate.clone(), Arc::clone(&progress)));
        thread::Builder::new()
            .name("update-download".to_string())
            .spawn(move || {
                let result = (|| -> Result<DownloadedUpdate> {
                    let rt =
                        tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                    rt.block_on(async {
                        tokio::select! {
                            result = crate::update::download_update(candidate, &cache_dir, Arc::clone(&progress)) => result,
                            _ = async {
                                while !progress.cancel.load(Ordering::Relaxed) { tokio::time::sleep(Duration::from_millis(100)).await; }
                            } => Err(anyhow::anyhow!("update download canceled")),
                        }
                    })
                })();
                let _ = tx.send(result);
            })
            .expect("failed to spawn update download thread");
        self.jobs.pending_update_download = Some(rx);
        tracing::info!("started update download");
        self.request_redraw();
    }

    pub(super) fn poll_pending_update_download(&mut self) {
        let Some(rx) = &self.jobs.pending_update_download else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(downloaded)) => {
                tracing::info!(path = %downloaded.path.display(), "update downloaded");
                self.jobs.pending_update_download = None;
                self.jobs.update_progress = None;
                self.jobs.update_prompt = Some(UpdatePrompt::Ready(downloaded.candidate.clone()));
                self.jobs.downloaded_update = Some(downloaded);
                self.request_redraw();
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "update download failed");
                let candidate =
                    self.jobs.update_prompt.as_ref().and_then(|prompt| prompt.candidate().cloned());
                self.jobs.pending_update_download = None;
                let progress = self.jobs.update_progress.take();
                self.jobs.update_prompt =
                    if progress.as_ref().is_some_and(|p| p.cancel.load(Ordering::Relaxed)) {
                        candidate.map(UpdatePrompt::Available)
                    } else {
                        Some(UpdatePrompt::Error { message: format!("{error:#}"), candidate })
                    };
                self.request_redraw();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("update download worker disconnected");
                self.jobs.pending_update_download = None;
                self.jobs.update_progress = None;
                self.jobs.update_prompt = Some(UpdatePrompt::Error {
                    message: "Update worker stopped".into(),
                    candidate: self
                        .jobs
                        .update_prompt
                        .as_ref()
                        .and_then(|p| p.candidate().cloned()),
                });
            }
        }
    }

    pub(super) fn update_candidate_is_suppressed(&self, candidate: &UpdateCandidate) -> bool {
        self.boot.app_config.updates.skipped_version == candidate.version
            || self.jobs.update_dismissed_session_version.as_deref()
                == Some(candidate.version.as_str())
    }

    pub(super) fn handle_update_dialog_action(&mut self, action: UpdateDialogAction) {
        if !self.select_maintenance_allowed() {
            return;
        }
        match action {
            UpdateDialogAction::Cancel => {
                if let Some(progress) = &self.jobs.update_progress {
                    progress.cancel.store(true, Ordering::Relaxed);
                }
                crate::update::sparkle::cancel();
                self.jobs.downloaded_update = None;
                self.jobs.update_prompt = None;
            }
            UpdateDialogAction::Install => {
                if crate::update::sparkle::available() {
                    let candidate =
                        self.jobs.update_prompt.as_ref().and_then(UpdatePrompt::candidate).cloned();
                    match crate::update::sparkle::install(&self.update_restart_context()) {
                        Ok(()) => {
                            tracing::info!("approved Sparkle update and relaunch");
                            if let Some(candidate) = candidate {
                                self.jobs.update_prompt = Some(UpdatePrompt::Preparing(candidate));
                            }
                        }
                        Err(error) => {
                            tracing::error!(%error, "failed to approve Sparkle update and relaunch");
                            self.jobs.update_prompt = Some(UpdatePrompt::Error {
                                message: format!("{error:#}"),
                                candidate: None,
                            });
                        }
                    }
                } else if let Some(downloaded) = self.jobs.downloaded_update.take()
                    && let Err(error) = self.apply_downloaded_update(downloaded)
                {
                    self.jobs.update_prompt = Some(UpdatePrompt::Error {
                        message: format!("{error:#}"),
                        candidate: None,
                    });
                }
            }
            UpdateDialogAction::Update => {
                let Some(candidate) =
                    self.jobs.update_prompt.as_ref().and_then(UpdatePrompt::candidate).cloned()
                else {
                    return;
                };
                match candidate.asset.as_ref().map(|asset| asset.kind) {
                    Some(UpdateAssetKind::WindowsInstaller | UpdateAssetKind::WindowsPortable) => {
                        self.spawn_update_download(candidate)
                    }
                    Some(UpdateAssetKind::MacosAppZip) if crate::update::sparkle::available() => {
                        crate::update::sparkle::download();
                    }
                    _ => {
                        if let Err(error) = open_external_url(&candidate.html_url) {
                            tracing::warn!(%error, "failed to open release page");
                            self.jobs.update_prompt = Some(UpdatePrompt::Error {
                                message: {
                                    let mut args = FluentArgs::new();
                                    args.set("error", format!("{error:#}"));
                                    Localizer::new(self.boot.profile_config.ui.locale())
                                        .format("update-release-open-failed", &args)
                                },
                                candidate: Some(candidate),
                            });
                        } else {
                            self.jobs.update_dismissed_session_version =
                                Some(candidate.version.clone());
                            self.jobs.update_prompt = None;
                        }
                    }
                }
            }
            UpdateDialogAction::NotNow => {
                crate::update::sparkle::cancel();
                if let Some(version) =
                    self.jobs.update_prompt.as_ref().and_then(UpdatePrompt::candidate_version)
                {
                    self.jobs.update_dismissed_session_version = Some(version.to_string());
                }
                self.jobs.update_prompt = None;
            }
            UpdateDialogAction::SkipRelease => {
                crate::update::sparkle::cancel();
                let Some(version) = self
                    .jobs
                    .update_prompt
                    .as_ref()
                    .and_then(UpdatePrompt::candidate_version)
                    .map(str::to_string)
                else {
                    self.jobs.update_prompt = None;
                    return;
                };
                self.boot.app_config.updates.skipped_version = version;
                match save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config) {
                    Ok(()) => tracing::info!("skipped update version saved"),
                    Err(error) => tracing::warn!(%error, "failed to save skipped update version"),
                }
                self.jobs.update_prompt = None;
            }
            UpdateDialogAction::OpenReleasePage => {
                let url = self
                    .jobs
                    .update_prompt
                    .as_ref()
                    .and_then(UpdatePrompt::candidate)
                    .map(|candidate| candidate.html_url.as_str())
                    .unwrap_or(crate::update::RELEASES_PAGE_URL);
                if let Err(error) = open_external_url(url) {
                    tracing::warn!(%error, "failed to open release page");
                }
            }
        }
    }

    pub(super) fn apply_downloaded_update(&mut self, downloaded: DownloadedUpdate) -> Result<()> {
        match downloaded.candidate.asset.as_ref().map(|asset| asset.kind) {
            Some(UpdateAssetKind::WindowsPortable) => {
                let (root, _) =
                    crate::update::installed_package().context("missing package metadata")?;
                let work = downloaded.work.context("missing staged update")?;
                let request = bmz_updater::process::Request {
                    root,
                    work,
                    restart: self.update_restart_context(),
                };
                let (tx, rx) = mpsc::channel();
                self.jobs.update_prompt = Some(UpdatePrompt::Preparing(downloaded.candidate));
                thread::Builder::new().name("update-handoff".into()).spawn(move || {
                    let _ = tx.send(bmz_updater::process::start(&request));
                })?;
                self.jobs.pending_update_handoff = Some(rx);
                Ok(())
            }
            Some(UpdateAssetKind::WindowsInstaller) => {
                launch_update_installer(&downloaded.path)?;
                self.jobs.update_prompt = None;
                self.shutdown_requested.store(true, Ordering::SeqCst);
                Ok(())
            }
            _ => {
                open_external_url(&downloaded.candidate.html_url)?;
                self.jobs.update_prompt = None;
                Ok(())
            }
        }
    }

    pub(super) fn refresh_visible_select_folder_summaries(&mut self) {
        let visible_indices = select_visible_item_indices(
            self.select.select_items.len(),
            self.select.selected_index,
            25,
        );
        let ln_policy = self.boot.profile_config.play.ln_mode_policy;
        let rule_mode = self.boot.profile_config.play.rule_mode;
        self.select.select_folder_summaries.refresh(
            &mut self.select.select_items,
            &visible_indices,
            ln_policy,
            rule_mode,
        );
    }
}
