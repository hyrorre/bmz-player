use super::*;

pub(super) struct PendingPlayPreload {
    pub(super) generation: u64,
    pub(super) chart_id: i64,
    pub(super) input: SharedInputBackend,
    pub(super) audio_progress: Arc<AtomicU32>,
    pub(super) prepared_chart: Arc<OnceLock<PreparedPlayChart>>,
    pub(super) rx: Receiver<PlayPreloadResult>,
}

pub(super) struct PlayPreloadResult {
    pub(super) generation: u64,
    pub(super) chart_id: i64,
    pub(super) result: std::result::Result<PreloadedInputPlaySession, String>,
}

/// 中間リザルト中に先読みしている次のコース譜面。
///
/// preload に渡した開始条件をそのまま Play 入場へ引き継ぎ、曲間で
/// gauge/combo/arrange 条件を作り直して食い違わせないために保持する。
pub(super) struct PendingCourseStageLaunch {
    pub(super) course_id: i64,
    pub(super) entry_index: usize,
    pub(super) chart_id: i64,
    pub(super) options: PlayStartOptions,
    pub(super) preload_generation: u64,
    pub(super) preload_error: Option<String>,
}

impl PendingCourseStageLaunch {
    pub(super) fn matches(&self, course_id: i64, entry_index: usize, chart_id: i64) -> bool {
        self.course_id == course_id && self.entry_index == entry_index && self.chart_id == chart_id
    }
}

/// Media kept across same-song retry (beatoraja `BMSResource` style).
/// Cleared when leaving result back to select, or when starting an unrelated chart.
pub(super) struct PlayMediaCache {
    pub(super) chart_id: i64,
    /// All same-arrangement metadata is present together or absent together.
    pub(super) prepared_chart: Option<PreparedPlayChart>,
    pub(super) chart_normalization_gain: f32,
    pub(super) bga_frames: BgaFrameCatalog,
    pub(super) bga_assets: Vec<BgaAssetRef>,
    pub(super) video_bga_decoders: crate::video_bga::VideoBgaDecoderMap,
}

impl PlayMediaCache {
    pub(super) fn from_running(
        chart_id: i64,
        running: &mut crate::audio::RunningPlaySession,
        mode: ResultRetryMode,
    ) -> Self {
        let prepared_chart = (mode == ResultRetryMode::SameArrange).then(|| PreparedPlayChart {
            chart: Arc::clone(&running.session.chart),
            opponent_chart: running
                .session
                .battle_opponent
                .as_ref()
                .map(|opponent| Arc::clone(&opponent.chart)),
            skin_attempt: running.skin_attempt,
            source_ln_profile: running.source_ln_profile,
            chart_length_ms: running.chart_length_ms,
            render_snapshot_cache: running.render_snapshot_cache.clone(),
            applied_arrange: running.applied_arrange.clone(),
            score_key: running.score_key,
            assist_runtime: running.session.assist,
            score_save_disabled: running.score_save_disabled,
        });
        Self {
            chart_id,
            prepared_chart,
            chart_normalization_gain: running.session.audio_mix.chart_normalization_gain,
            bga_frames: running.bga_frames.clone(),
            bga_assets: running.session.chart.bga_assets.clone(),
            video_bga_decoders: std::mem::take(&mut running.video_bga_decoders),
        }
    }
}
