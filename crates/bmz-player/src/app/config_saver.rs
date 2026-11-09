use super::*;

enum ConfigSnapshot {
    Profile(Box<ProfileConfig>),
    App(Box<AppConfig>),
}

struct SaveJob {
    path: PathBuf,
    /// `None` is a barrier ordered behind earlier writes.
    snapshot: Option<ConfigSnapshot>,
    reason: &'static str,
    reply: Option<mpsc::Sender<Result<(), String>>>,
}

/// Writes profile/app config in submission order on one worker. TOML-serializing
/// a profile with a long skin history takes milliseconds, so the window thread
/// only clones. Queued snapshots of one file collapse to the newest.
pub(super) struct ConfigSaver {
    tx: Option<mpsc::Sender<SaveJob>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ConfigSaver {
    pub(super) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        match thread::Builder::new().name("config-save".into()).spawn(move || run_worker(rx)) {
            Ok(worker) => Self { tx: Some(tx), worker: Some(worker) },
            Err(error) => {
                tracing::warn!(%error, "config save worker unavailable; saving on the caller");
                Self { tx: None, worker: None }
            }
        }
    }

    pub(super) fn queue_profile(&self, path: &Path, profile: &ProfileConfig, reason: &'static str) {
        let snapshot = ConfigSnapshot::Profile(Box::new(profile.clone()));
        let _ = self.submit(path, Some(snapshot), reason, false);
    }

    pub(super) fn queue_app(&self, path: &Path, config: &AppConfig, reason: &'static str) {
        let snapshot = ConfigSnapshot::App(Box::new(config.clone()));
        let _ = self.submit(path, Some(snapshot), reason, false);
    }

    /// Returns after this write and every earlier queued one have finished.
    pub(super) fn save_profile(&self, path: &Path, profile: &ProfileConfig) -> Result<()> {
        let snapshot = ConfigSnapshot::Profile(Box::new(profile.clone()));
        self.submit(path, Some(snapshot), "", true)
    }

    pub(super) fn save_app(&self, path: &Path, config: &AppConfig) -> Result<()> {
        let snapshot = ConfigSnapshot::App(Box::new(config.clone()));
        self.submit(path, Some(snapshot), "", true)
    }

    pub(super) fn flush(&self) {
        let _ = self.submit(Path::new(""), None, "", true);
    }

    /// Finishes queued writes; later saves run on the caller.
    pub(super) fn shutdown(&mut self) {
        self.tx = None;
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("config save worker panicked");
        }
    }

    fn submit(
        &self,
        path: &Path,
        snapshot: Option<ConfigSnapshot>,
        reason: &'static str,
        wait: bool,
    ) -> Result<()> {
        let (reply, done) = if wait {
            let (reply, done) = mpsc::channel();
            (Some(reply), Some(done))
        } else {
            (None, None)
        };
        let job = SaveJob { path: path.to_owned(), snapshot, reason, reply };
        let job = match &self.tx {
            Some(tx) => match tx.send(job) {
                Ok(()) => None,
                Err(mpsc::SendError(job)) => Some(job),
            },
            None => Some(job),
        };
        if let Some(job) = job {
            let result = write_snapshot(&job);
            if !wait {
                log_queued_save(&job, &result);
            }
            return result;
        }
        match done {
            Some(done) => done
                .recv()
                .map_err(|_| anyhow::anyhow!("config save worker stopped"))?
                .map_err(anyhow::Error::msg),
            None => Ok(()),
        }
    }
}

fn run_worker(rx: mpsc::Receiver<SaveJob>) {
    while let Ok(first) = rx.recv() {
        // One write per file: the newest snapshot covers every earlier one.
        let mut writes: Vec<(SaveJob, bool)> = Vec::new();
        let mut replies = Vec::new();
        for mut job in std::iter::once(first).chain(rx.try_iter()) {
            let reply = job.reply.take();
            let index = if job.snapshot.is_none() {
                None
            } else if let Some(index) =
                writes.iter().position(|(queued, _)| queued.path == job.path)
            {
                let waited = writes[index].1 || reply.is_some();
                writes[index] = (job, waited);
                Some(index)
            } else {
                writes.push((job, reply.is_some()));
                Some(writes.len() - 1)
            };
            if let Some(reply) = reply {
                replies.push((index, reply));
            }
        }
        let results: Vec<_> = writes
            .iter()
            .map(|(job, waited)| {
                let result = write_snapshot(job);
                // Waiting callers log and recover themselves.
                if !waited {
                    log_queued_save(job, &result);
                }
                result.map_err(|error| format!("{error:#}"))
            })
            .collect();
        for (index, reply) in replies {
            let _ = reply.send(index.map_or(Ok(()), |index| results[index].clone()));
        }
    }
}

fn write_snapshot(job: &SaveJob) -> Result<()> {
    match &job.snapshot {
        Some(ConfigSnapshot::Profile(profile)) => save_profile_config(&job.path, profile),
        Some(ConfigSnapshot::App(config)) => save_app_config(&job.path, config),
        None => Ok(()),
    }
}

fn log_queued_save(job: &SaveJob, result: &Result<()>) {
    let kind = match job.snapshot {
        Some(ConfigSnapshot::Profile(_)) => "profile",
        Some(ConfigSnapshot::App(_)) => "app",
        None => return,
    };
    let reason = job.reason;
    match result {
        Ok(()) => tracing::info!(kind, reason, "saved config"),
        Err(error) => tracing::error!(%error, kind, reason, "failed to save config"),
    }
}

impl WinitApp {
    /// Waits behind queued saves, for callers that roll back edits on failure.
    pub(super) fn save_profile_config_now(&self) -> Result<()> {
        self.jobs
            .config_saver
            .save_profile(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
    }

    pub(super) fn save_app_config_now(&self) -> Result<()> {
        self.jobs.config_saver.save_app(&self.boot.app_paths.config_toml, &self.boot.app_config)
    }

    pub(super) fn queue_profile_config_save(&self, reason: &'static str) {
        self.jobs.config_saver.queue_profile(
            &self.boot.profile_paths.profile_toml,
            &self.boot.profile_config,
            reason,
        );
    }

    pub(super) fn queue_app_config_save(&self, reason: &'static str) {
        self.jobs.config_saver.queue_app(
            &self.boot.app_paths.config_toml,
            &self.boot.app_config,
            reason,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(volume: u32) -> ProfileConfig {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.audio_mix.master_volume = volume;
        profile
    }

    fn saved_volume(path: &Path) -> u32 {
        crate::config::load::load_profile_config(path).unwrap().audio_mix.master_volume
    }

    #[test]
    fn queued_profile_saves_land_in_order_before_a_waiting_save_returns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.toml");
        let saver = ConfigSaver::new();
        for volume in 1..=20 {
            saver.queue_profile(&path, &profile(volume), "test");
        }
        saver.flush();
        assert_eq!(saved_volume(&path), 20);

        saver.queue_profile(&path, &profile(30), "test");
        let app_path = dir.path().join("config.toml");
        saver.save_app(&app_path, &AppConfig::default()).unwrap();
        // The app write was queued last, so the earlier profile write is on disk.
        assert_eq!(saved_volume(&path), 30);
        saver.save_profile(&path, &profile(40)).unwrap();
        assert_eq!(saved_volume(&path), 40);
    }

    #[test]
    fn waiting_save_reports_write_errors_and_shutdown_falls_back_to_the_caller() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").unwrap();
        let mut saver = ConfigSaver::new();
        // A regular file cannot be the parent directory of the config.
        assert!(saver.save_profile(&blocker.join("profile.toml"), &profile(1)).is_err());

        let path = dir.path().join("profile.toml");
        saver.queue_profile(&path, &profile(5), "test");
        saver.shutdown();
        assert_eq!(saved_volume(&path), 5);
        saver.queue_profile(&path, &profile(6), "test");
        assert_eq!(saved_volume(&path), 6);
    }

    /// Caller time of the play-end save (profile + app), queued vs waiting.
    #[test]
    #[ignore = "release config-save probe; set BMZ_PROBE_CONFIG_DIR to a dir with profile.toml and config.toml"]
    fn profile_queued_config_save() -> Result<()> {
        use crate::config::load::{load_app_config, load_profile_config};
        anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
        let dir = std::env::var_os("BMZ_PROBE_CONFIG_DIR")
            .ok_or_else(|| anyhow::anyhow!("set BMZ_PROBE_CONFIG_DIR"))?;
        let profile = load_profile_config(&Path::new(&dir).join("profile.toml"))?;
        let app = load_app_config(&Path::new(&dir).join("config.toml"))?;
        let out = tempfile::tempdir()?;
        let (profile_path, app_path) =
            (out.path().join("profile.toml"), out.path().join("config.toml"));
        let saver = ConfigSaver::new();
        for queued in [true, false] {
            let mut samples = Vec::new();
            for iteration in 0..110 {
                let started = Instant::now();
                if queued {
                    saver.queue_profile(&profile_path, &profile, "probe");
                    saver.queue_app(&app_path, &app, "probe");
                } else {
                    saver.save_profile(&profile_path, &profile)?;
                    saver.save_app(&app_path, &app)?;
                }
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                // Each sample starts with an idle worker, like a play end.
                saver.flush();
                if iteration >= 10 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "play-end save queued={queued} median={:.3}ms p95={:.3}ms max={:.3}ms",
                samples[50], samples[95], samples[99]
            );
        }
        Ok(())
    }
}
