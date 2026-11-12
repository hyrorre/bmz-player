use super::*;
use crate::screens::play_session::{PlaySessionOptions, PreloadedPlaySession};

pub(super) enum PlayPreloadSource {
    Import { library_db_path: PathBuf, normalization_output_gain: f32 },
    Cached { chart: Box<PreparedPlayChart>, normalization_gain: f32 },
}

impl PendingPlayPreload {
    pub(super) fn spawn(
        generation: u64,
        chart_id: i64,
        session_options: PlaySessionOptions,
        source: PlayPreloadSource,
    ) -> Self {
        let input = SharedInputBackend::default();
        let gamepad_slots = session_options.gamepad_slots;
        let audio_progress = Arc::new(AtomicU32::new(0));
        let prepared_chart = Arc::new(OnceLock::new());
        let skin_chart_state = PlaySkinChartState::default();
        // A reused chart is available before the audio worker starts, just like
        // an imported chart is published before its first audio progress update.
        if let PlayPreloadSource::Cached { chart, .. } = &source {
            let _ = prepared_chart.set(chart.as_ref().clone());
            skin_chart_state.finish(Ok(Arc::clone(&chart.render_snapshot_cache.skin_chart)));
        }
        let worker_input = input.clone();
        let worker_progress = Arc::clone(&audio_progress);
        let worker_chart = Arc::clone(&prepared_chart);
        let worker_skin_chart = skin_chart_state.clone();
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name(format!("play-preload-{chart_id}"))
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    source.load(
                        chart_id,
                        &session_options,
                        &worker_chart,
                        &worker_skin_chart,
                        &worker_progress,
                    )
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("play preload worker panicked")))
                .map(|preloaded| PreloadedInputPlaySession {
                    chart_id,
                    preloaded,
                    input: worker_input,
                    session_options,
                })
                .map_err(|error| format!("{error:#}"));
                if let Err(error) = &result {
                    worker_skin_chart.finish(Err(error.clone()));
                }
                let _ = tx.send(PlayPreloadResult { generation, chart_id, result });
            })
            .expect("failed to spawn play preload thread");
        Self {
            generation,
            chart_id,
            gamepad_slots,
            input,
            audio_progress,
            prepared_chart,
            skin_chart_state,
            rx,
        }
    }
}

impl PlayPreloadSource {
    fn load(
        self,
        chart_id: i64,
        options: &PlaySessionOptions,
        prepared: &OnceLock<PreparedPlayChart>,
        skin_chart_state: &PlaySkinChartState,
        progress: &AtomicU32,
    ) -> Result<PreloadedPlaySession> {
        let report_progress = |loaded, total| {
            progress.store(resource_load_progress_units(loaded, total), Ordering::Relaxed);
        };
        match self {
            Self::Import { library_db_path, normalization_output_gain } => {
                let library = LibraryDatabase::open(&library_db_path)?;
                crate::screens::play_session::preload_play_session_for_chart_with_callbacks(
                    &library,
                    chart_id,
                    options.clone(),
                    normalization_output_gain,
                    |chart| {
                        let _ = prepared.set(chart.clone());
                        skin_chart_state
                            .finish(Ok(Arc::clone(&chart.render_snapshot_cache.skin_chart)));
                    },
                    report_progress,
                )
            }
            Self::Cached { chart, normalization_gain } => {
                crate::screens::play_session::preload_play_session_reloading_audio_with_progress(
                    *chart,
                    options.sample_rate,
                    normalization_gain,
                    report_progress,
                )
            }
        }
    }
}

impl WinitApp {
    pub(super) fn spawn_play_preload(
        &mut self,
        chart_id: i64,
        mut options: PlaySessionOptions,
        source: PlayPreloadSource,
    ) -> u64 {
        options.read_archive_assets = true;
        self.skin.skin_pipeline.bump_generation(SkinKind::Play);
        self.skin.skin_pipeline.set_pending(SkinKind::Play, false);
        self.skin.last_play_skin_signature = None;
        if let Some(pending) = &self.play.pending_play_preload {
            pending.skin_chart_state.finish(Err("play preload superseded".into()));
        }
        // Course launch metadata is installed by its caller after receiving the
        // generation. Every other launch replaces any previous course preload.
        self.play.pending_course_stage_launch = None;
        self.play.play_preload_generation = self.play.play_preload_generation.wrapping_add(1);
        let generation = self.play.play_preload_generation;
        self.play.preloaded_play_session = None;
        self.play.pending_play_preload =
            Some(PendingPlayPreload::spawn(generation, chart_id, options, source));
        tracing::info!(chart_id, generation, "play preload started");
        generation
    }
}
