use super::*;

impl WinitApp {
    pub(super) fn update_restart_context(&self) -> bmz_updater::process::RestartContext {
        let paths = &self.boot.app_paths;
        let absolute = |path: &Path| {
            path.canonicalize()
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(path))
        };
        bmz_updater::process::RestartContext {
            data_dir: absolute(&paths.data_dir),
            cache_dir: absolute(&paths.cache_dir),
            logs_dir: absolute(&paths.logs_dir),
            resource_dir: absolute(&paths.resource_dir),
            profile: self
                .boot
                .profile_paths
                .root_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }
    }

    pub(super) fn poll_update_handoff(&mut self) {
        let Some(rx) = &self.jobs.pending_update_handoff else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("update helper disconnected"))
            }
        };
        self.jobs.pending_update_handoff = None;
        match result.and_then(|handoff| handoff.commit()) {
            Ok(()) => {
                self.jobs.update_prompt = None;
                self.shutdown_requested.store(true, Ordering::SeqCst);
            }
            Err(error) => {
                self.jobs.update_prompt =
                    Some(UpdatePrompt::Error { message: format!("{error:#}"), candidate: None });
            }
        }
        self.request_redraw();
    }

    pub(super) fn poll_sparkle_update(&mut self) {
        use crate::update::sparkle::Event;
        while let Some(event) = crate::update::sparkle::poll() {
            match event {
                Event::Available(candidate) => {
                    let candidate = *candidate;
                    if self.update_candidate_is_suppressed(&candidate) {
                        crate::update::sparkle::cancel();
                    } else {
                        self.jobs.update_prompt = Some(UpdatePrompt::Available(candidate));
                    }
                }
                Event::Progress { received, total, extracting } => {
                    let candidate =
                        self.jobs.update_prompt.as_ref().and_then(|p| p.candidate().cloned());
                    if let Some(candidate) = candidate {
                        let progress = self.jobs.update_progress.get_or_insert_with(|| {
                            Arc::new(crate::update::DownloadProgress::default())
                        });
                        progress.received.store(received, Ordering::Relaxed);
                        progress.total.store(total, Ordering::Relaxed);
                        progress.extracting.store(extracting, Ordering::Relaxed);
                        self.jobs.update_prompt =
                            Some(UpdatePrompt::Downloading(candidate, Arc::clone(progress)));
                    }
                }
                Event::Ready => {
                    self.jobs.update_progress = None;
                    if let Some(candidate) =
                        self.jobs.update_prompt.as_ref().and_then(|p| p.candidate().cloned())
                    {
                        self.jobs.update_prompt = Some(UpdatePrompt::Ready(candidate));
                    }
                }
                Event::Error(message) => {
                    tracing::error!(%message, "Sparkle update failed");
                    self.jobs.update_progress = None;
                    let candidate =
                        self.jobs.update_prompt.as_ref().and_then(|p| p.candidate().cloned());
                    self.jobs.update_prompt = Some(UpdatePrompt::Error { message, candidate });
                }
                Event::UpToDate => {
                    self.jobs.update_prompt = Some(UpdatePrompt::UpToDate);
                }
                Event::Shutdown => {
                    tracing::info!("Sparkle requested update relaunch handoff");
                    self.jobs.update_prompt = None;
                    self.prepare_for_process_exit(false, "Sparkle update relaunch");
                    if crate::update::sparkle::resume_install() {
                        tracing::info!("instructed Sparkle to continue update relaunch");
                    } else {
                        tracing::warn!("Sparkle update relaunch handler was no longer available");
                    }
                }
                Event::Canceled => {
                    tracing::info!("Sparkle update canceled");
                    self.jobs.update_progress = None;
                    self.jobs.update_prompt = None;
                }
            }
            self.request_redraw();
        }
    }
}
