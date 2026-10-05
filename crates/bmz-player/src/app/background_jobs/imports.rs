use super::*;

impl WinitApp {
    pub(in crate::app) fn import_external_scores(&mut self, request: ScoreImportRequest) {
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

    pub(in crate::app) fn spawn_beatoraja_replay_import(
        &mut self,
        request: ImportBeatorajaReplaysRequest,
    ) {
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

    pub(in crate::app) fn cancel_beatoraja_replay_import(&mut self) {
        let Some(pending) = &self.jobs.pending_replay_import else {
            return;
        };
        pending.cancel.store(true, Ordering::Relaxed);
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.set_replay_import_status("cancelling replay import...".to_string(), false);
        }
    }

    pub(in crate::app) fn poll_pending_replay_import(&mut self) {
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
}
