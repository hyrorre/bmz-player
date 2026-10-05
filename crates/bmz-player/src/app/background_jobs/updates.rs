use super::*;

impl WinitApp {
    pub(in crate::app) fn spawn_update_check(
        &mut self,
        label: &'static str,
        report_up_to_date: bool,
    ) {
        if !self.select_maintenance_allowed() {
            self.jobs.updates.queue_check(label, report_up_to_date);
            tracing::debug!(label, "queued update check until Select");
            return;
        }
        if self.jobs.updates.pending_check.is_some() {
            tracing::debug!(label, "update check already in progress");
            return;
        }
        let channel = self.boot.app_config.updates.channel;
        if crate::update::sparkle::available() {
            if let Err(error) = crate::update::sparkle::check(channel, report_up_to_date) {
                self.jobs.updates.prompt =
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
        self.jobs.updates.pending_check =
            Some(update_runtime::PendingUpdateCheck { rx, report_up_to_date });
        tracing::info!(?channel, label, "started update check");
    }

    pub(in crate::app) fn poll_pending_update_check(&mut self) {
        let Some((report_up_to_date, result)) = self.jobs.updates.poll_check() else {
            return;
        };
        match result {
            Ok(UpdateCheckWorkerResult::Available(candidate)) => {
                let candidate = *candidate;
                tracing::info!(version = %candidate.version, "update available");
                if self.update_candidate_is_suppressed(&candidate) {
                    return;
                }
                self.jobs.updates.prompt = Some(UpdatePrompt::Available(candidate));
                self.request_redraw();
            }
            Ok(UpdateCheckWorkerResult::UpToDate) => {
                tracing::info!("no update available");
                if report_up_to_date {
                    self.jobs.updates.prompt = Some(UpdatePrompt::UpToDate);
                    self.request_redraw();
                }
            }
            Ok(UpdateCheckWorkerResult::Failed(error)) => {
                tracing::warn!(%error, "update check failed");
                let report_error = report_up_to_date;
                if report_error {
                    self.jobs.updates.prompt = Some(UpdatePrompt::Error {
                        message: format!("{error:#}"),
                        candidate: None,
                    });
                    self.request_redraw();
                }
            }
            Ok(UpdateCheckWorkerResult::Paused) => {
                self.jobs.updates.queue_check("resumed update check", report_up_to_date);
                tracing::debug!("paused update check outside Select");
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("update check worker disconnected");
            }
        }
    }

    pub(in crate::app) fn spawn_update_download(&mut self, candidate: UpdateCandidate) {
        if self.jobs.updates.pending_download.is_some() {
            tracing::debug!("update download already in progress");
            return;
        }
        let cache_dir = self.boot.app_paths.cache_dir.clone();
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(crate::update::DownloadProgress::default());
        self.jobs.updates.progress = Some(Arc::clone(&progress));
        self.jobs.updates.prompt =
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
        self.jobs.updates.pending_download = Some(rx);
        tracing::info!("started update download");
        self.request_redraw();
    }

    pub(in crate::app) fn poll_pending_update_download(&mut self) {
        let Some(rx) = &self.jobs.updates.pending_download else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(downloaded)) => {
                tracing::info!(path = %downloaded.path.display(), "update downloaded");
                self.jobs.updates.pending_download = None;
                self.jobs.updates.progress = None;
                self.jobs.updates.prompt = Some(UpdatePrompt::Ready(downloaded.candidate.clone()));
                self.jobs.updates.downloaded = Some(downloaded);
                self.request_redraw();
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "update download failed");
                let candidate = self
                    .jobs
                    .updates
                    .prompt
                    .as_ref()
                    .and_then(|prompt| prompt.candidate().cloned());
                self.jobs.updates.pending_download = None;
                let progress = self.jobs.updates.progress.take();
                self.jobs.updates.prompt =
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
                self.jobs.updates.pending_download = None;
                self.jobs.updates.progress = None;
                self.jobs.updates.prompt = Some(UpdatePrompt::Error {
                    message: "Update worker stopped".into(),
                    candidate: self
                        .jobs
                        .updates
                        .prompt
                        .as_ref()
                        .and_then(|p| p.candidate().cloned()),
                });
            }
        }
    }

    pub(in crate::app) fn update_candidate_is_suppressed(
        &self,
        candidate: &UpdateCandidate,
    ) -> bool {
        self.boot.app_config.updates.skipped_version == candidate.version
            || self.jobs.updates.dismissed_session_version.as_deref()
                == Some(candidate.version.as_str())
    }

    pub(in crate::app) fn handle_update_dialog_action(&mut self, action: UpdateDialogAction) {
        if !self.select_maintenance_allowed() {
            return;
        }
        match action {
            UpdateDialogAction::Cancel => {
                if let Some(progress) = &self.jobs.updates.progress {
                    progress.cancel.store(true, Ordering::Relaxed);
                }
                crate::update::sparkle::cancel();
                self.jobs.updates.downloaded = None;
                self.jobs.updates.prompt = None;
            }
            UpdateDialogAction::Install => {
                if crate::update::sparkle::available() {
                    let candidate = self
                        .jobs
                        .updates
                        .prompt
                        .as_ref()
                        .and_then(UpdatePrompt::candidate)
                        .cloned();
                    match crate::update::sparkle::install(&self.update_restart_context()) {
                        Ok(()) => {
                            tracing::info!("approved Sparkle update and relaunch");
                            if let Some(candidate) = candidate {
                                self.jobs.updates.prompt = Some(UpdatePrompt::Preparing(candidate));
                            }
                        }
                        Err(error) => {
                            tracing::error!(%error, "failed to approve Sparkle update and relaunch");
                            self.jobs.updates.prompt = Some(UpdatePrompt::Error {
                                message: format!("{error:#}"),
                                candidate: None,
                            });
                        }
                    }
                } else if let Some(downloaded) = self.jobs.updates.downloaded.take()
                    && let Err(error) = self.apply_downloaded_update(downloaded)
                {
                    self.jobs.updates.prompt = Some(UpdatePrompt::Error {
                        message: format!("{error:#}"),
                        candidate: None,
                    });
                }
            }
            UpdateDialogAction::Update => {
                let Some(candidate) =
                    self.jobs.updates.prompt.as_ref().and_then(UpdatePrompt::candidate).cloned()
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
                            self.jobs.updates.prompt = Some(UpdatePrompt::Error {
                                message: {
                                    let mut args = FluentArgs::new();
                                    args.set("error", format!("{error:#}"));
                                    Localizer::new(self.boot.profile_config.ui.locale())
                                        .format("update-release-open-failed", &args)
                                },
                                candidate: Some(candidate),
                            });
                        } else {
                            self.jobs.updates.dismissed_session_version =
                                Some(candidate.version.clone());
                            self.jobs.updates.prompt = None;
                        }
                    }
                }
            }
            UpdateDialogAction::NotNow => {
                crate::update::sparkle::cancel();
                if let Some(version) =
                    self.jobs.updates.prompt.as_ref().and_then(UpdatePrompt::candidate_version)
                {
                    self.jobs.updates.dismissed_session_version = Some(version.to_string());
                }
                self.jobs.updates.prompt = None;
            }
            UpdateDialogAction::SkipRelease => {
                crate::update::sparkle::cancel();
                let Some(version) = self
                    .jobs
                    .updates
                    .prompt
                    .as_ref()
                    .and_then(UpdatePrompt::candidate_version)
                    .map(str::to_string)
                else {
                    self.jobs.updates.prompt = None;
                    return;
                };
                self.boot.app_config.updates.skipped_version = version;
                match save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config) {
                    Ok(()) => tracing::info!("skipped update version saved"),
                    Err(error) => tracing::warn!(%error, "failed to save skipped update version"),
                }
                self.jobs.updates.prompt = None;
            }
            UpdateDialogAction::OpenReleasePage => {
                let url = self
                    .jobs
                    .updates
                    .prompt
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

    pub(in crate::app) fn apply_downloaded_update(
        &mut self,
        downloaded: DownloadedUpdate,
    ) -> Result<()> {
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
                self.jobs.updates.prompt = Some(UpdatePrompt::Preparing(downloaded.candidate));
                thread::Builder::new().name("update-handoff".into()).spawn(move || {
                    let _ = tx.send(bmz_updater::process::start(&request));
                })?;
                self.jobs.updates.pending_handoff = Some(rx);
                Ok(())
            }
            Some(UpdateAssetKind::WindowsInstaller) => {
                launch_update_installer(&downloaded.path)?;
                self.jobs.updates.prompt = None;
                self.shutdown_requested.store(true, Ordering::SeqCst);
                Ok(())
            }
            _ => {
                open_external_url(&downloaded.candidate.html_url)?;
                self.jobs.updates.prompt = None;
                Ok(())
            }
        }
    }
}
