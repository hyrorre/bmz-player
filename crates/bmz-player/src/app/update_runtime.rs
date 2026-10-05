use super::*;

#[derive(Default)]
pub(super) struct AppUpdateRuntime {
    pub(super) pending_check: Option<PendingUpdateCheck>,
    /// Selectまで保留する要求。手動要求の通知意図を維持する。
    pub(super) queued_check: Option<(&'static str, bool)>,
    pub(super) pending_download: Option<Receiver<Result<DownloadedUpdate>>>,
    pub(super) progress: Option<Arc<crate::update::DownloadProgress>>,
    pub(super) downloaded: Option<DownloadedUpdate>,
    pub(super) pending_handoff: Option<Receiver<Result<bmz_updater::process::Handoff>>>,
    pub(super) prompt: Option<UpdatePrompt>,
    pub(super) dismissed_session_version: Option<String>,
}

pub(super) struct PendingUpdateCheck {
    pub(super) rx: Receiver<UpdateCheckWorkerResult>,
    pub(super) report_up_to_date: bool,
}

impl AppUpdateRuntime {
    pub(super) fn queue_check(&mut self, label: &'static str, report_up_to_date: bool) {
        match &mut self.queued_check {
            Some((queued_label, queued_report)) => {
                if report_up_to_date {
                    *queued_label = label;
                }
                *queued_report |= report_up_to_date;
            }
            None => self.queued_check = Some((label, report_up_to_date)),
        }
    }

    /// Emptyだけは受信口と通知意図を保持し、完了/切断では両方を消費する。
    pub(super) fn poll_check(
        &mut self,
    ) -> Option<(bool, Result<UpdateCheckWorkerResult, mpsc::TryRecvError>)> {
        let pending = self.pending_check.as_ref()?;
        let result = pending.rx.try_recv();
        if matches!(result, Err(mpsc::TryRecvError::Empty)) {
            return None;
        }
        let pending = self.pending_check.take()?;
        Some((pending.report_up_to_date, result))
    }
}

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
        let Some(rx) = &self.jobs.updates.pending_handoff else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("update helper disconnected"))
            }
        };
        self.jobs.updates.pending_handoff = None;
        match result.and_then(|handoff| handoff.commit()) {
            Ok(()) => {
                self.jobs.updates.prompt = None;
                self.shutdown_requested.store(true, Ordering::SeqCst);
            }
            Err(error) => {
                self.jobs.updates.prompt =
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
                        self.jobs.updates.prompt = Some(UpdatePrompt::Available(candidate));
                    }
                }
                Event::Progress { received, total, extracting } => {
                    let candidate =
                        self.jobs.updates.prompt.as_ref().and_then(|p| p.candidate().cloned());
                    if let Some(candidate) = candidate {
                        let progress = self.jobs.updates.progress.get_or_insert_with(|| {
                            Arc::new(crate::update::DownloadProgress::default())
                        });
                        progress.received.store(received, Ordering::Relaxed);
                        progress.total.store(total, Ordering::Relaxed);
                        progress.extracting.store(extracting, Ordering::Relaxed);
                        self.jobs.updates.prompt =
                            Some(UpdatePrompt::Downloading(candidate, Arc::clone(progress)));
                    }
                }
                Event::Ready => {
                    self.jobs.updates.progress = None;
                    if let Some(candidate) =
                        self.jobs.updates.prompt.as_ref().and_then(|p| p.candidate().cloned())
                    {
                        self.jobs.updates.prompt = Some(UpdatePrompt::Ready(candidate));
                    }
                }
                Event::Error(message) => {
                    tracing::error!(%message, "Sparkle update failed");
                    self.jobs.updates.progress = None;
                    let candidate =
                        self.jobs.updates.prompt.as_ref().and_then(|p| p.candidate().cloned());
                    self.jobs.updates.prompt = Some(UpdatePrompt::Error { message, candidate });
                }
                Event::UpToDate => {
                    self.jobs.updates.prompt = Some(UpdatePrompt::UpToDate);
                }
                Event::Shutdown => {
                    tracing::info!("Sparkle requested update relaunch handoff");
                    self.jobs.updates.prompt = None;
                    self.prepare_for_process_exit(false, "Sparkle update relaunch");
                    if crate::update::sparkle::resume_install() {
                        tracing::info!("instructed Sparkle to continue update relaunch");
                    } else {
                        tracing::warn!("Sparkle update relaunch handler was no longer available");
                    }
                }
                Event::Canceled => {
                    tracing::info!("Sparkle update canceled");
                    self.jobs.updates.progress = None;
                    self.jobs.updates.prompt = None;
                }
            }
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_manual_check_survives_automatic_requests() {
        let mut state = AppUpdateRuntime::default();
        state.queue_check("startup", false);
        state.queue_check("manual", true);
        state.queue_check("automatic", false);
        assert_eq!(state.queued_check, Some(("manual", true)));
    }

    #[test]
    fn check_notification_intent_stays_with_receiver_until_completion() {
        let mut state = AppUpdateRuntime::default();
        let (tx, rx) = mpsc::channel();
        state.pending_check = Some(PendingUpdateCheck { rx, report_up_to_date: true });
        assert!(state.poll_check().is_none());
        assert!(state.pending_check.is_some());
        tx.send(UpdateCheckWorkerResult::Paused).unwrap();
        let (report, result) = state.poll_check().unwrap();
        assert!(report);
        assert!(matches!(result, Ok(UpdateCheckWorkerResult::Paused)));
        assert!(state.pending_check.is_none());
        state.queue_check("resumed", report);
        assert_eq!(state.queued_check, Some(("resumed", true)));

        let (tx, rx) = mpsc::channel();
        state.pending_check = Some(PendingUpdateCheck { rx, report_up_to_date: false });
        drop(tx);
        let (report, result) = state.poll_check().unwrap();
        assert!(!report);
        assert!(matches!(result, Err(mpsc::TryRecvError::Disconnected)));
        assert!(state.pending_check.is_none());
    }
}
