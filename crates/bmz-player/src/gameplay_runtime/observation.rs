use bmz_audio::clock::AudioClock;
use bmz_chart::model::PlayableChart;
use bmz_core::lane::KeyMode;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_gameplay::session::*;
use std::sync::Arc;

/// A detached, read-only observation. It contains no input consumer, judgement
/// engine or replay recorder. Only the gameplay owner can publish changes.
#[derive(Clone)]
pub struct PlaySessionObservation {
    pub chart: Arc<PlayableChart>,
    pub play_config_key_mode: KeyMode,
    pub primary_key_mode: KeyMode,
    pub scored_total_notes: u32,
    pub assist: AssistRuntime,
    pub audio_clock: AudioClock,
    pub recent_judgements: Vec<JudgementEvent>,
    pub offsets: PlayOffsets,
    pub input_offset_auto_adjust_enabled: bool,
    pub input_offset_auto_adjust: Option<InputOffsetAutoAdjustState>,
    pub audio_mix: PlayAudioMix,
    pub hispeed: f32,
    /// skin表示とprofile保存に使うHS値。Classic + HS-FIXではBPM150基準。
    pub display_hispeed: f32,
    pub hispeed_mode: HispeedMode,
    pub base_hispeed_mode: HispeedMode,
    pub floating_policy: FloatingPolicy,
    pub normal_hispeed_level: u8,
    pub target_green_number: u32,
    pub guide_se_enabled: bool,
    pub lift: f32,
    pub lane_cover: f32,
    pub lane_cover_visible: bool,
    pub lanecover_enabled: bool,
    pub lift_enabled: bool,
    pub hidden_enabled: bool,
    pub lane_cover_changing: bool,
    pub hidden_cover: f32,
    pub bga_enabled: bool,
    pub poor_bga_duration_us: i64,
    pub state: PlayState,
    pub autoplay: Option<bmz_gameplay::autoplay::AutoplayController>,
    pub replay_player: Option<()>,
    pub exhausted: bool,
    pub settled: bool,
    pub battle_opponent: Option<BattleObservation>,
}

#[derive(Clone)]
pub struct BattleObservation {
    pub chart: Arc<PlayableChart>,
}

impl PlaySessionObservation {
    pub fn from_session(session: &GameSession) -> Self {
        Self {
            chart: session.chart.clone(),
            play_config_key_mode: session.play_config_key_mode,
            primary_key_mode: session.primary_key_mode,
            scored_total_notes: session.scored_total_notes,
            assist: session.assist,
            audio_clock: session.audio_clock.clone(),
            recent_judgements: session.recent_judgements.clone(),
            offsets: session.offsets,
            input_offset_auto_adjust_enabled: session.input_offset_auto_adjust_enabled,
            input_offset_auto_adjust: session.input_offset_auto_adjust,
            audio_mix: session.audio_mix,
            hispeed: session.hispeed,
            display_hispeed: crate::screens::play_snapshot::display_hispeed_for_session(session),
            hispeed_mode: session.hispeed_mode,
            base_hispeed_mode: session.base_hispeed_mode,
            floating_policy: session.floating_policy,
            normal_hispeed_level: session.normal_hispeed_level,
            target_green_number: session.target_green_number,
            guide_se_enabled: session.guide_se_enabled,
            lift: session.lift,
            lane_cover: session.lane_cover,
            lane_cover_visible: session.lane_cover_visible,
            lanecover_enabled: session.lanecover_enabled,
            lift_enabled: session.lift_enabled,
            hidden_enabled: session.hidden_enabled,
            lane_cover_changing: session.lane_cover_changing,
            hidden_cover: session.hidden_cover,
            bga_enabled: session.bga_enabled,
            poor_bga_duration_us: session.poor_bga_duration_us,
            state: session.state,
            autoplay: session.autoplay.clone(),
            replay_player: session.replay_player.as_ref().map(|_| ()),
            exhausted: session.judge.is_exhausted(&session.chart),
            settled: result_is_settled(session, session.audio_clock.now()),
            battle_opponent: session
                .battle_opponent
                .as_ref()
                .map(|opponent| BattleObservation { chart: Arc::clone(&opponent.chart) }),
        }
    }

    pub fn chart_started(&self) -> bool {
        self.audio_clock.running && self.audio_clock.now().0 >= 0
    }
}
