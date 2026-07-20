use std::collections::HashMap;
use std::sync::Arc;

use bmz_audio::clock::AudioClock;
use bmz_audio::queue::{AudioScheduler, RestartPolicy, ScheduledSound};
use bmz_chart::model::{LongNoteMode, NoteEvent, NoteKind, PlayableChart};
use bmz_chart::timing::TimingMap;
use bmz_core::ids::{NoteId, SoundId};
use bmz_core::input::{InputEvent, InputKind, InputSource, ScratchDirection};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use crate::autoplay::AutoplayController;
use crate::gauge::GaugeState;
use crate::hit_error::HitErrorRing;
use crate::input::backend::monotonic_timestamp_ns;
use crate::input::system::InputSystem;
use crate::input::translator::{InputTimestampAnchor, InputTimingContext};
use crate::judge::engine::JudgeEngine;
use crate::judge::model::{
    JudgeOutcome, JudgeWindow, JudgeWindows, JudgementEvent, KeySoundEvent, MineHitEvent,
};
use crate::judge::window::{
    judge_percent_at_time_for_keymode, judge_windows_for_rule_mode_and_keymode,
};
use crate::replay::{ReplayPlayer, ReplayRecorder};
use crate::rule::RuleMode;
use crate::score::ScoreState;

pub const AUDIO_SCHEDULE_AHEAD_US: i64 = 100_000;
pub const SESSION_END_MARGIN_US: i64 = 5_000_000;
pub const JUDGEMENT_DISPLAY_US: i64 = 800_000;
pub const INPUT_DISPLAY_US: i64 = 160_000;
/// オートプレイ時のキー押下を「離す」までの時間。beatoraja の `auto_minduration` (80ms) と揃える。
/// この時間が経過すると lane_keyon → lane_keyoff へ遷移し、skin の KEYOFF タイマー演出が走る。
pub const AUTO_KEYBEAM_DURATION_US: i64 = 80_000;
const SCRATCH_ANGLE_PERIOD_MS: i64 = 2_160;
/// pre-ready の wall-clock key 時刻と chart 時刻を区別する閾値。
const CHART_KEY_FUTURE_SLACK_US: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Ready,
    Playing,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HispeedMode {
    Normal,
    Floating,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayOffsets {
    pub input_offset_us: i64,
    pub visual_offset_us: i64,
}

pub const INPUT_OFFSET_AUTO_ADJUST_MIN_US: i64 = -500_000;
pub const INPUT_OFFSET_AUTO_ADJUST_MAX_US: i64 = 500_000;
const INPUT_OFFSET_AUTO_ADJUST_STEP_US: i64 = 1_000;
const INPUT_OFFSET_AUTO_ADJUST_BATCH: u32 = 10;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputOffsetAutoAdjustState {
    pub sum_delta_us: i64,
    pub count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayAudioMix {
    pub master_volume: f32,
    pub normalization_gain: f32,
    pub key_volume: f32,
    pub bgm_volume: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlaySkinOffset {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub r: i32,
    pub a: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameTimes {
    pub audio_now: TimeUs,
    pub audio_schedule_until: TimeUs,
}

#[derive(Debug, Clone, Default)]
pub struct BgmScheduler {
    pub next_index: usize,
}

pub struct GameSession {
    pub chart: Arc<PlayableChart>,
    /// LN policy / course override 適用後の譜面で実際にスコア対象となるノート数。
    /// Tap と LongStart に加えて、CN / HCN の LongEnd を数える。
    pub scored_total_notes: u32,
    /// `chart` の BPM 変化と STOP を取り込んだ tick<->time マップ。
    /// スクロール位置を BPM に追従させるために使う。
    pub timing_map: TimingMap,
    pub audio_clock: AudioClock,
    pub input_system: InputSystem,
    pub judge: JudgeEngine,
    /// プロファイル既定の判定窓。`#RANK` / `#EXRANK` 倍率の基準値。
    pub base_judge_window: JudgeWindow,
    pub base_judge_windows: JudgeWindows,
    pub rule_mode: RuleMode,
    pub score: ScoreState,
    /// Course-mode display combo carry. ScoreState remains per-chart for
    /// storage; these fields only affect the rendered combo/max combo.
    pub course_combo_carry: u32,
    pub course_combo_carry_active: bool,
    pub course_max_combo: u32,
    pub gauge: GaugeState,
    pub replay_recorder: ReplayRecorder,
    pub replay_player: Option<ReplayPlayer>,
    pub autoplay: Option<AutoplayController>,
    pub recent_inputs: Vec<InputEvent>,
    /// 各レーンのキー押下開始時刻。押下中のみ Some。skin の keyon タイマー(100..=107)。
    pub lane_keyon_started_at: [Option<TimeUs>; LANE_COUNT],
    /// 各レーンのキー解放時刻。離した直後のみ Some(次の Press でクリア)。skin の keyoff タイマー(120..=127)。
    pub lane_keyoff_started_at: [Option<TimeUs>; LANE_COUNT],
    /// Scratch lane rotation direction while the scratch input is held.
    pub lane_scratch_direction: [Option<ScratchDirection>; LANE_COUNT],
    /// Extra turntable phase accumulated by scratch input, in beatoraja scratch-angle ms.
    pub lane_scratch_angle_delta_ms: [i64; LANE_COUNT],
    pub scratch_angle_last_render_at: Option<TimeUs>,
    /// オートプレイで Press したレーンの自動 Release 予定時刻。
    /// `audio_now` がこの時刻を超えたら keyon → keyoff へ遷移する。
    pub lane_auto_release_at: [Option<TimeUs>; LANE_COUNT],
    pub recent_judgements: Vec<JudgementEvent>,
    /// Skin runtime がフレーム間で入力・判定履歴を構築するためのイベント列。
    /// `advance_session_frame` の終端で drain し、長時間プレイでもセッション側に
    /// 無制限に蓄積しない。sequence は同一時刻のイベント順も一意にする。
    pub pending_skin_events: Vec<SkinRuntimeEvent>,
    pub next_skin_event_sequence: u64,
    /// Result 統計グラフ用の最終ノート判定詳細。
    /// beatoraja の Result は譜面上の Note.state / playTime を走査してグラフ化するため、
    /// BMZ でも score 集計とは別に note_id 単位の判定差分を保持する。
    pub result_judgements: HashMap<NoteId, ResultJudgementDetail>,
    /// HitErrorVisualizer 用の直近判定タイミング (ms)。beatoraja `recentJudges` 相当。
    pub hit_error_ring: HitErrorRing,
    /// Gauge increase animation start time. Skin timer 42/43 uses elapsed time from here.
    pub gauge_increase_started_at: Option<TimeUs>,
    /// Gauge max animation start time. Skin timer 44/45 is active while the current gauge is maxed.
    pub gauge_max_started_at: Option<TimeUs>,
    /// Full combo animation start time. Set once when all notes have been judged
    /// and the combo still matches the total note count.
    pub full_combo_started_at: Option<TimeUs>,
    pub bgm_scheduler: BgmScheduler,
    pub offsets: PlayOffsets,
    pub input_offset_auto_adjust_enabled: bool,
    pub input_offset_auto_adjust: Option<InputOffsetAutoAdjustState>,
    pub audio_mix: PlayAudioMix,
    pub hispeed: f32,
    pub hispeed_mode: HispeedMode,
    pub target_green_number: u32,
    /// Floating hispeed の曲開始前基準 BPM。曲開始後は現在 BPM で再計算する。
    pub hsfix_base_bpm: f64,
    pub lift: f32,
    pub lane_cover: f32,
    /// レーンカバー(SUDDEN+)の表示有無。
    /// false のときは描画/判定の visible_max 計算で lane_cover が 0 として扱われる。
    pub lane_cover_visible: bool,
    /// beatoraja `OPTION_LANECOVER1_ON` (271): SUDDEN+ 機能が有効か。
    pub lanecover_enabled: bool,
    /// beatoraja `OPTION_LIFT1_ON` (272): LIFT 機能が有効か。
    pub lift_enabled: bool,
    /// beatoraja `OPTION_HIDDEN1_ON` (273): HIDDEN 機能が有効か。
    pub hidden_enabled: bool,
    /// beatoraja `PlayConfig.enableHispeedAutoAdjust`。
    pub hispeed_auto_adjust: bool,
    /// Start/Select 押下中など、レーンカバー数値表示を出す状態。
    pub lane_cover_changing: bool,
    pub hidden_cover: f32,
    pub skin_offsets: Vec<PlaySkinOffset>,
    pub bga_enabled: bool,
    pub poor_bga_duration_us: i64,
    pub bga_stretch: i32,
    /// LN モードでも終端 (tail) キャップを描画するか。既定 OFF (beatoraja 準拠)。
    pub show_ln_tail_cap: bool,
    /// HCN passing 中のレーン状態。beatoraja の TIMER_HCN_ACTIVE / TIMER_HCN_DAMAGE 相当。
    /// HCN の区間内 (始端ノート判定済み) のレーンのみ Some。
    pub lane_hcn_timer: [Option<HcnLaneTimer>; LANE_COUNT],
    /// HCN キー音のミュート状態。終端を BAD 以下で判定済みの passing HCN のみ Some。
    pub lane_hcn_keysound_muted: [Option<bool>; LANE_COUNT],
    /// 判定処理で発火したキー音。EmptyPoor や LN 終端のように、スコア対象
    /// note_id とキー音対象 note_id が一致しないケースがあるため別に持つ。
    pub pending_keysounds: Vec<KeySoundEvent>,
    /// 当該フレームで適用するキー音音量変更。`advance_session_frame` の終端で
    /// `SessionFrame.keysound_volumes` に吸い出される (app 層が audio engine に反映)。
    pub pending_keysound_volumes: Vec<(SoundId, f32)>,
    /// beatoraja `event_index(BUTTON_HSFIX=55)`。
    pub hsfix_index: i32,
    pub input_timestamp_anchor: Option<InputTimestampAnchor>,
    /// 当該フレーム中に発火した Mine ヒット。`advance_session_frame` の終端で
    /// `SessionFrame.mine_hits` に吸い出される（app 層が地雷 SE を鳴らす）。
    pub pending_mine_hits: Vec<MineHitEvent>,
    pub state: PlayState,
    /// HCN ゲージ増減の前回更新時刻。
    pub last_hcn_gauge_at: Option<TimeUs>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultJudgementDetail {
    pub judge: Judge,
    pub side: TimingSide,
    /// BMZ 判定エンジンの差分。正が入力遅れ、負が入力早め。
    pub delta: TimeUs,
    pub time: TimeUs,
}

#[derive(Debug, Clone)]
pub struct FrameOutput<TSnapshot> {
    pub render_snapshot: TSnapshot,
    /// 当該フレームで発生した判定。result graph など、app 層がフレーム単位で
    /// 蓄積する副作用に使う。
    pub judgements: Vec<JudgementEvent>,
    /// 当該フレームで踏んだ Mine ノーツ。app 層が地雷 SE を鳴らすのに使う。
    pub mine_hits: Vec<MineHitEvent>,
    /// 当該フレームで適用するキー音音量変更 (HCN 早離し時のミュート/復帰)。
    pub keysound_volumes: Vec<(SoundId, f32)>,
    /// 当該フレームで発生した、skin runtime 向けの順序付き入力・判定イベント。
    pub skin_events: Vec<SkinRuntimeEvent>,
    pub state: PlayState,
}

#[derive(Debug, Clone)]
pub struct SessionFrame {
    pub times: FrameTimes,
    pub judgements: Vec<JudgementEvent>,
    /// 当該フレームで踏んだ Mine ノーツ。地雷 SE 再生など、UI / audio 側の
    /// 副作用処理に使う。`apply_judge_outcome` の中ですでにゲージは削っている。
    pub mine_hits: Vec<MineHitEvent>,
    /// 当該フレームで適用するキー音音量変更 (HCN 早離し時のミュート/復帰)。
    /// app 層が audio engine の `set_volume_for_sound` に反映する。
    pub keysound_volumes: Vec<(SoundId, f32)>,
    /// 当該フレームで発生した、skin runtime 向けの順序付き入力・判定イベント。
    pub skin_events: Vec<SkinRuntimeEvent>,
    pub state: PlayState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinRuntimeEvent {
    pub sequence: u64,
    pub kind: SkinRuntimeEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkinRuntimeEventKind {
    Input(InputEvent),
    Judgement(JudgementEvent),
}

impl BgmScheduler {
    pub fn schedule_until(
        &mut self,
        chart: &PlayableChart,
        clock: &AudioClock,
        until: TimeUs,
        volume: f32,
        audio: &mut dyn AudioScheduler,
    ) {
        while let Some(event) = chart.bgm_events.get(self.next_index) {
            if event.time > until {
                break;
            }

            let chart_volume = bmz_chart::volume::chart_channel_volume_factor(
                bmz_chart::volume::chart_volume_at_time(&chart.bgm_volume_events, event.time),
            );
            audio.schedule(ScheduledSound {
                start_frame: clock.time_to_output_frame(event.time),
                sound_id: event.sound,
                volume: (volume * chart_volume).clamp(0.0, 1.0),
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                catch_up: true,
                restart_policy: RestartPolicy::StopSameSound,
            });

            self.next_index += 1;
        }
    }

    pub fn is_done(&self, chart: &PlayableChart) -> bool {
        self.next_index >= chart.bgm_events.len()
    }
}

pub fn compute_frame_times(session: &GameSession) -> FrameTimes {
    let audio_now = session.audio_clock.now();
    let audio_schedule_until = TimeUs(audio_now.0 + AUDIO_SCHEDULE_AHEAD_US);
    FrameTimes { audio_now, audio_schedule_until }
}

pub fn apply_judge_outcome(
    session: &mut GameSession,
    outcome: JudgeOutcome,
) -> Vec<JudgementEvent> {
    let mut events = Vec::with_capacity(outcome.events.len());
    for event in outcome.events {
        if event.affects_score {
            session.score.apply(&event);
            update_course_combo_state(session, &event);
            let previous_gauge = session.gauge.current().value;
            session.gauge.apply_judge(event.judge, 1.0);
            update_gauge_increase_timer(session, previous_gauge, event.time);
            if let Some(note_id) = event.note_id {
                session.result_judgements.insert(
                    note_id,
                    ResultJudgementDetail {
                        judge: event.judge,
                        side: event.side,
                        delta: event.delta,
                        time: event.time,
                    },
                );
            }
        }
        push_skin_runtime_event(session, SkinRuntimeEventKind::Judgement(event.clone()));
        events.push(event);
    }
    for hit in outcome.mine_hits {
        // Mine はスコア/コンボに影響を与えず、ゲージのみ削る。SE 再生 (= app 層の
        // 副作用) は `pending_mine_hits` に積んでフレーム終端で吸い出す。
        session.gauge.apply_mine(hit.damage);
        session.pending_mine_hits.push(hit);
    }
    session.pending_keysounds.extend(outcome.keysounds);
    session.pending_keysound_volumes.extend(outcome.keysound_volumes);
    update_failed_state_from_gauge(session);
    events
}

fn push_skin_runtime_event(session: &mut GameSession, kind: SkinRuntimeEventKind) {
    let sequence = session.next_skin_event_sequence;
    session.next_skin_event_sequence = session
        .next_skin_event_sequence
        .checked_add(1)
        .expect("skin runtime event sequence exhausted");
    session.pending_skin_events.push(SkinRuntimeEvent { sequence, kind });
}

impl GameSession {
    pub fn display_combo(&self) -> u32 {
        if self.course_combo_carry_active {
            self.course_combo_carry.saturating_add(self.score.combo)
        } else {
            self.score.combo
        }
    }

    pub fn display_max_combo(&self) -> u32 {
        self.course_max_combo.max(self.score.max_combo)
    }
}

fn update_course_combo_state(session: &mut GameSession, event: &JudgementEvent) {
    match event.judge {
        Judge::PGreat | Judge::Great | Judge::Good => {
            session.course_max_combo = session.course_max_combo.max(session.display_combo());
        }
        Judge::Bad | Judge::Poor => {
            session.course_combo_carry_active = false;
        }
        Judge::EmptyPoor if session.score.empty_poor_breaks_combo => {
            session.course_combo_carry_active = false;
        }
        Judge::EmptyPoor => {}
    }
}

fn update_failed_state_from_gauge(session: &mut GameSession) {
    if session.state == PlayState::Playing && session.gauge.current_closes_play_on_zero() {
        session.state = PlayState::Failed;
    }
}

fn update_gauge_increase_timer(session: &mut GameSession, previous_value: f32, now: TimeUs) {
    let current_value = session.gauge.current().value;
    if current_value > previous_value + f32::EPSILON {
        session.gauge_increase_started_at = Some(TimeUs(now.0.max(0)));
    }
}

fn update_gauge_max_timer(session: &mut GameSession, now: TimeUs) {
    let current = session.gauge.current();
    let is_max = current.value >= current.definition.max.max(1.0);
    match (is_max, session.gauge_max_started_at) {
        (true, None) => session.gauge_max_started_at = Some(TimeUs(now.0.max(0))),
        (false, Some(_)) => session.gauge_max_started_at = None,
        _ => {}
    }
}

pub fn schedule_keysounds(session: &mut GameSession, audio: &mut dyn AudioScheduler) {
    for event in std::mem::take(&mut session.pending_keysounds) {
        let note_id = event.note_id;
        let Some(sound_id) = session.chart.note_by_id(note_id).and_then(|note| note.sound) else {
            continue;
        };

        let chart_volume = bmz_chart::volume::chart_channel_volume_factor(
            bmz_chart::volume::chart_volume_at_time(&session.chart.key_volume_events, event.time),
        );
        audio.schedule(ScheduledSound {
            start_frame: session.audio_clock.time_to_output_frame(event.time),
            sound_id,
            volume: (session.audio_mix.master_volume
                * session.audio_mix.normalization_gain
                * session.audio_mix.key_volume
                * chart_volume)
                .clamp(0.0, 1.0),
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::StopSameSound,
        });
    }
}

fn plays_keysound(judge: Judge) -> bool {
    matches!(judge, Judge::PGreat | Judge::Great | Judge::Good | Judge::Bad)
}

fn counts_for_input_offset_auto_adjust(judge: Judge) -> bool {
    plays_keysound(judge)
}

pub fn apply_input_offset_auto_adjust(session: &mut GameSession, events: &[JudgementEvent]) {
    let Some(state) = &mut session.input_offset_auto_adjust else {
        return;
    };
    // beatoraja/LR2-style judge timing adjustment shifts the note display timing.
    // The low-level input offset remains a separate BMZ-only calibration knob.
    for event in events {
        if !event.affects_score
            || event.note_id.is_none()
            || !counts_for_input_offset_auto_adjust(event.judge)
        {
            continue;
        }
        state.sum_delta_us = state.sum_delta_us.saturating_add(event.delta.0);
        state.count = state.count.saturating_add(1);
        if state.count < INPUT_OFFSET_AUTO_ADJUST_BATCH {
            continue;
        }

        let offset_delta = match state.sum_delta_us.cmp(&0) {
            std::cmp::Ordering::Greater => INPUT_OFFSET_AUTO_ADJUST_STEP_US,
            std::cmp::Ordering::Less => -INPUT_OFFSET_AUTO_ADJUST_STEP_US,
            std::cmp::Ordering::Equal => 0,
        };
        if offset_delta != 0 {
            session.offsets.visual_offset_us = session
                .offsets
                .visual_offset_us
                .saturating_add(offset_delta)
                .clamp(INPUT_OFFSET_AUTO_ADJUST_MIN_US, INPUT_OFFSET_AUTO_ADJUST_MAX_US);
        }
        *state = InputOffsetAutoAdjustState::default();
    }
}

pub fn update_recent_judgements(session: &mut GameSession, events: &[JudgementEvent], now: TimeUs) {
    for event in events {
        if !event.affects_score {
            continue;
        }
        session.hit_error_ring.push_judgement(event.judge, event.delta.0);
    }
    session.recent_judgements.extend(events.iter().filter(|event| event.affects_score).cloned());
    session.recent_judgements.retain(|event| now.0 <= event.time.0 + JUDGEMENT_DISPLAY_US);
}

pub fn update_recent_inputs(session: &mut GameSession, inputs: &[InputEvent], now: TimeUs) {
    session
        .recent_inputs
        .extend(inputs.iter().copied().filter(|input| input.kind == InputKind::Press));
    session.recent_inputs.retain(|input| now.0 <= input.time.0 + INPUT_DISPLAY_US);
}

/// レーンごとのキー押下/解放状態を更新する。beatoraja の KEYON/KEYOFF タイマー相当。
/// Press → keyon を新しい時刻にセット、keyoff をクリア(autoplay 入力なら 80ms 後の自動 release も予約)。
/// Release → 押下中だった場合のみ keyoff をセット、keyon と auto_release_at をクリア。
pub fn update_lane_key_states(session: &mut GameSession, inputs: &[InputEvent]) {
    for input in inputs {
        let lane_index = input.lane.index();
        match input.kind {
            InputKind::Press => {
                session.lane_keyon_started_at[lane_index] = Some(input.time);
                session.lane_keyoff_started_at[lane_index] = None;
                session.lane_scratch_direction[lane_index] = input.scratch_direction;
                session.lane_auto_release_at[lane_index] = match input.source {
                    InputSource::Auto => Some(TimeUs(input.time.0 + AUTO_KEYBEAM_DURATION_US)),
                    _ => None,
                };
            }
            InputKind::Release => {
                if session.lane_keyon_started_at[lane_index].is_some() {
                    session.lane_keyoff_started_at[lane_index] = Some(input.time);
                    session.lane_keyon_started_at[lane_index] = None;
                }
                if input.scratch_direction.is_none()
                    || session.lane_scratch_direction[lane_index] == input.scratch_direction
                {
                    session.lane_scratch_direction[lane_index] = None;
                }
                session.lane_auto_release_at[lane_index] = None;
            }
        }
    }
}

/// オートプレイで Press したレーンを `AUTO_KEYBEAM_DURATION_US` 経過後に自動で release する。
/// beatoraja の `auto_minduration` (80ms) 経過で `auto_presstime` を MIN_VALUE に戻す挙動に対応。
/// beatoraja 同様、LN ホールド中 (`state.processing != null`) は release しない。
pub fn apply_auto_key_release(session: &mut GameSession, audio_now: TimeUs) {
    for lane_index in 0..LANE_COUNT {
        if let Some(release_at) = session.lane_auto_release_at[lane_index]
            && audio_now.0 >= release_at.0
            && session.judge.lanes[lane_index].active_long.is_none()
        {
            push_skin_runtime_event(
                session,
                SkinRuntimeEventKind::Input(InputEvent {
                    lane: Lane::ALL[lane_index],
                    kind: InputKind::Release,
                    time: release_at,
                    source: InputSource::Auto,
                    device_kind: Default::default(),
                    scratch_direction: session.lane_scratch_direction[lane_index],
                }),
            );
            session.lane_keyoff_started_at[lane_index] = Some(release_at);
            session.lane_keyon_started_at[lane_index] = None;
            session.lane_scratch_direction[lane_index] = None;
            session.lane_auto_release_at[lane_index] = None;
        }
    }
}

pub fn process_human_inputs(session: &mut GameSession) -> Vec<JudgementEvent> {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let mut inputs = session.input_system.collect_game_inputs(&ctx);
    if let Some(autoplay) = &session.autoplay {
        inputs.retain(|input| !autoplay.is_lane_enabled(input.lane));
    }
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);
    let mut judgements = Vec::new();
    for input in inputs {
        session.replay_recorder.record(input);
        let events = process_session_input(session, input);
        apply_input_offset_auto_adjust(session, &events);
        judgements.extend(events);
    }
    judgements
}

/// リプレイ再生中は人間入力を判定に渡さない。視覚エフェクト用に recent_inputs だけ更新する。
pub fn drain_human_inputs(session: &mut GameSession) {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let inputs = session.input_system.collect_game_inputs(&ctx);
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);
}

/// READY 開始前の play 導入中に、判定へは渡さず keybeam / lazer 用の lane key 状態だけ更新する。
/// 入力時刻は壁時計ベースの `visual_now` に揃える。
pub fn drain_pre_ready_visual_inputs(session: &mut GameSession, visual_now: TimeUs) {
    if session.autoplay.as_ref().is_some_and(AutoplayController::is_full) {
        discard_human_inputs(session);
        return;
    }

    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: None,
    };
    let inputs = session.input_system.collect_game_inputs(&ctx);
    let visual_inputs: Vec<InputEvent> = inputs
        .into_iter()
        .map(|mut input| {
            input.time = visual_now;
            input
        })
        .collect();
    update_recent_inputs(session, &visual_inputs, visual_now);
    update_lane_key_states(session, &visual_inputs);
    update_scratch_angle_phase(session, visual_now);
}

pub fn is_wall_clock_lane_key_time(started_at: TimeUs, chart_time: TimeUs) -> bool {
    started_at.0 > chart_time.0 + CHART_KEY_FUTURE_SLACK_US
}

/// READY 開始時のwall-clockからchart-clockへの切替に合わせて、
/// pre-ready visual timer の経過時間を保ったまま時刻を移す。
///
/// 押下中のkeyonをchart 0で破棄すると、beatorajaと異なりkeybeamが
/// READY/PLAY境界で消える。最後のpre-ready frame時刻と現在のchart時刻の
/// 差を対象timerに加え、同じ経過時間でchart-clockへ引き継ぐ。
pub fn rebase_pre_ready_visual_times(session: &mut GameSession, chart_time: TimeUs) {
    let Some(wall_clock_now) = session.scratch_angle_last_render_at else {
        return;
    };
    // 通常のchart-clockは単調増加する。現在値が前frameより小さい場合だけ、
    // READY開始によるclock domain切替と扱う。短い導入でも検出できるよう
    // key timer判定用の1秒slackはここでは使わない。
    if wall_clock_now.0 <= chart_time.0 {
        return;
    }

    let clock_delta_us = chart_time.0.saturating_sub(wall_clock_now.0);
    for lane_index in 0..LANE_COUNT {
        for time in [
            &mut session.lane_keyon_started_at[lane_index],
            &mut session.lane_keyoff_started_at[lane_index],
            &mut session.lane_auto_release_at[lane_index],
        ]
        .into_iter()
        .flatten()
        {
            time.0 = time.0.saturating_add(clock_delta_us);
        }
    }
    for input in &mut session.recent_inputs {
        input.time.0 = input.time.0.saturating_add(clock_delta_us);
    }
    session.scratch_angle_last_render_at = Some(chart_time);
}

pub fn update_scratch_angle_phase(session: &mut GameSession, render_now: TimeUs) {
    let Some(last_render_at) = session.scratch_angle_last_render_at else {
        session.scratch_angle_last_render_at = Some(render_now);
        return;
    };
    session.scratch_angle_last_render_at = Some(render_now);
    let delta_ms = ((render_now.0 - last_render_at.0) / 1_000).max(0);
    if delta_ms == 0 {
        return;
    }

    for lane_index in 0..LANE_COUNT {
        if session.lane_keyon_started_at[lane_index].is_none() {
            continue;
        }
        let sign =
            match session.lane_scratch_direction[lane_index].unwrap_or(ScratchDirection::Down) {
                ScratchDirection::Up => 1,
                ScratchDirection::Down => -1,
            };
        session.lane_scratch_angle_delta_ms[lane_index] =
            (session.lane_scratch_angle_delta_ms[lane_index] + sign * delta_ms.saturating_mul(2))
                .rem_euclid(SCRATCH_ANGLE_PERIOD_MS);
    }
}

/// 入力バックエンドを drain するだけで、判定にも視覚エフェクト(recent_inputs)にも反映しない。
/// オートプレイ中はキービームをノーツ処理側で発火させるため、人間入力はここで捨てる。
pub fn discard_human_inputs(session: &mut GameSession) {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let _ = session.input_system.collect_game_inputs(&ctx);
}

pub fn process_replay_inputs(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let Some(player) = &mut session.replay_player else {
        return Vec::new();
    };

    let inputs = player.poll_until(audio_now);
    update_lane_key_states(session, &inputs);
    let mut judgements = Vec::new();
    for input in inputs {
        judgements.extend(process_session_input(session, input));
    }
    judgements
}

pub fn process_autoplay_inputs(
    session: &mut GameSession,
    audio_now: TimeUs,
) -> Vec<JudgementEvent> {
    let Some(auto) = &mut session.autoplay else {
        return Vec::new();
    };

    let inputs = auto.poll_until(&session.chart, audio_now);
    // オートプレイのキービームはノーツ処理時(=この autoplay 入力)で発火させる。
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);

    let mut judgements = Vec::new();
    for input in inputs {
        judgements.extend(process_session_input(session, input));
    }
    judgements
}

fn process_session_input(session: &mut GameSession, input: InputEvent) -> Vec<JudgementEvent> {
    push_skin_runtime_event(session, SkinRuntimeEventKind::Input(input));
    let mut outcome = session.judge.process_input(&session.chart, input);
    if input.kind == InputKind::Press {
        let hcn_passing = hcn_passing_at(session, input.lane, input.time);
        if hcn_passing && outcome.events.iter().all(|event| event.judge == Judge::EmptyPoor) {
            outcome.events.clear();
            outcome.keysounds.clear();
            outcome.consumed_input = false;
        }
        if outcome.events.is_empty()
            && outcome.keysounds.is_empty()
            && !hcn_passing
            && let Some(note_id) = fallback_keysound_note_id(session, input.lane, input.time)
        {
            outcome.keysounds.push(KeySoundEvent { note_id, time: input.time });
        }
    }
    apply_judge_outcome(session, outcome)
}

fn hcn_passing_at(session: &GameSession, lane: Lane, time: TimeUs) -> bool {
    session.chart.long_notes.iter().any(|pair| {
        pair.lane == lane
            && pair.mode.unwrap_or(session.chart.metadata.long_note_mode) == LongNoteMode::Hcn
            && time.0 >= pair.start_time.0
            && time.0 < pair.end_time.0
    })
}

fn fallback_keysound_note_id(session: &GameSession, lane: Lane, time: TimeUs) -> Option<NoteId> {
    if session.judge.lanes[lane.index()].active_long.is_some() {
        return None;
    }

    let notes = session.chart.notes_for_lane(lane);
    let mut candidate = notes
        .iter()
        .find(|note| is_fallback_playable_note(note) && !is_processed_long_start(session, note));

    for note in notes {
        if note.time.0 >= time.0 {
            break;
        }
        match note.kind {
            NoteKind::Invisible => candidate = Some(note),
            NoteKind::Tap | NoteKind::LongStart if !is_processed_long_start(session, note) => {
                if candidate.is_none_or(|current| current.time.0 <= note.time.0) {
                    candidate = Some(note);
                }
            }
            NoteKind::Tap | NoteKind::LongEnd | NoteKind::Mine | NoteKind::LongStart => {}
        }
    }

    candidate.map(|note| note.id)
}

fn is_fallback_playable_note(note: &NoteEvent) -> bool {
    matches!(note.kind, NoteKind::Tap | NoteKind::LongStart)
}

fn is_processed_long_start(session: &GameSession, note: &NoteEvent) -> bool {
    note.kind == NoteKind::LongStart && session.judge.judged_notes.contains_key(&note.id)
}

pub fn process_mine_passes(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let mut lane_keyon_started_at = [None; LANE_COUNT];
    for lane in Lane::ALL {
        let idx = lane.index();
        let Some(keyon_started_at) = session.lane_keyon_started_at[idx] else {
            continue;
        };
        if session.autoplay.as_ref().is_some_and(|autoplay| autoplay.is_lane_enabled(lane)) {
            continue;
        }
        lane_keyon_started_at[idx] = Some(keyon_started_at);
    }

    let outcome =
        session.judge.process_mine_passes(&session.chart, audio_now, &lane_keyon_started_at);
    apply_judge_outcome(session, outcome)
}

pub fn process_misses(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let outcome = session.judge.process_misses(&session.chart, audio_now);
    apply_judge_outcome(session, outcome)
}

/// HCN passing 中レーンの表示タイマー状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HcnLaneTimer {
    /// 押下中 (回復中) なら true、離している (減衰中) なら false。
    pub inclease: bool,
    /// 現在の inclease 状態が始まった時刻。タイマー経過時間の起点。
    pub since: TimeUs,
    /// beatoraja `mpassingcount`: inclease 中は加算、離している間は減算する
    /// 符号付き経過時間 (us)。±200ms を超えるたびにゲージを 1 tick 更新する。
    /// inclease の反転ではリセットせず (相殺される)、passing 終了でリセットする。
    pub passing_count_us: i64,
}

/// beatoraja の TIMER_HCN_ACTIVE / TIMER_HCN_DAMAGE 切り替えと
/// HCN キー音の音量制御に対応する。
/// 「HCN の始端〜終端の区間内 (passing) かつ始端ノート判定済み」のレーンで、
/// inclease（押下中、または終端を PG/GR/GD で判定済み）に応じて
/// active/damage を切り替える。状態が反転したらタイマー起点 (`since`) を
/// リセットする。
/// キー音は beatoraja 同様、終端が BAD 以下で判定済み（早離し）の passing HCN
/// に限り、離している間は音量 0、押し直したら元の音量に戻す。
pub fn update_hcn_lane_timers(session: &mut GameSession, audio_now: TimeUs) {
    let mut next: [Option<HcnLaneTimer>; LANE_COUNT] = [None; LANE_COUNT];
    let mut next_muted: [Option<bool>; LANE_COUNT] = [None; LANE_COUNT];
    for pair in &session.chart.long_notes {
        let mode = pair.mode.unwrap_or(session.chart.metadata.long_note_mode);
        if mode != LongNoteMode::Hcn
            || audio_now.0 < pair.start_time.0
            || audio_now.0 >= pair.end_time.0
            || !session.judge.judged_notes.contains_key(&pair.start_note_id)
        {
            continue;
        }
        let idx = pair.lane.index();
        let end_judge = session.judge.judged_notes.get(&pair.end_note_id).copied();
        // beatoraja: pressed || 終端が PG/GR/GD で判定済みなら inclease。
        let inclease = session.lane_keyon_started_at[idx].is_some()
            || matches!(end_judge, Some(Judge::PGreat | Judge::Great | Judge::Good));
        let since = match session.lane_hcn_timer[idx] {
            Some(prev) if prev.inclease == inclease => prev.since,
            _ => audio_now,
        };
        // mpassingcount は inclease 反転でもリセットしない (beatoraja 準拠)。
        let passing_count_us = session.lane_hcn_timer[idx].map_or(0, |prev| prev.passing_count_us);
        next[idx] = Some(HcnLaneTimer { inclease, since, passing_count_us });

        // beatoraja: passing.getPair().getState() > 3 (終端 BAD 以下で判定済み)
        // のときのみキー音音量を制御する。
        if matches!(end_judge, Some(Judge::Bad | Judge::Poor | Judge::EmptyPoor))
            && let Some(sound_id) = pair.sound
        {
            let muted = !inclease;
            if session.lane_hcn_keysound_muted[idx] != Some(muted) {
                let volume = if muted {
                    0.0
                } else {
                    let chart_volume = bmz_chart::volume::chart_channel_volume_factor(
                        bmz_chart::volume::chart_volume_at_time(
                            &session.chart.key_volume_events,
                            pair.start_time,
                        ),
                    );
                    (session.audio_mix.master_volume
                        * session.audio_mix.normalization_gain
                        * session.audio_mix.key_volume
                        * chart_volume)
                        .clamp(0.0, 1.0)
                };
                session.pending_keysound_volumes.push((sound_id, volume));
            }
            next_muted[idx] = Some(muted);
        }
    }
    session.lane_hcn_timer = next;
    session.lane_hcn_keysound_muted = next_muted;
}

/// beatoraja `JudgeManager` の `hcnmduration` (200ms)。
const HCN_UPDATE_US: i64 = 200_000;

/// beatoraja の HCN ゲージ増減判定 (passing ベース)。
/// `lane_hcn_timer` (HCN 区間内かつ始端判定済みのレーン) を参照し、
/// `mpassingcount` 相当の符号付きカウンタにフレーム経過時間を加減算して、
/// ±200ms を超えるたびに GREAT/BAD × rate 0.5 でゲージを 1 tick 更新する。
/// 始端を見逃しても途中から押し直せば回復する。
pub fn apply_hcn_gauge(session: &mut GameSession, audio_now: TimeUs) {
    if session.lane_hcn_timer.iter().all(Option::is_none) {
        session.last_hcn_gauge_at = None;
        return;
    }

    let previous = session.last_hcn_gauge_at.unwrap_or(audio_now);
    session.last_hcn_gauge_at = Some(audio_now);
    if audio_now.0 <= previous.0 {
        return;
    }

    let delta_us = audio_now.0 - previous.0;
    for idx in 0..LANE_COUNT {
        let Some(mut timer) = session.lane_hcn_timer[idx] else {
            continue;
        };
        if timer.inclease {
            timer.passing_count_us += delta_us;
            while timer.passing_count_us > HCN_UPDATE_US {
                let previous_gauge = session.gauge.current().value;
                session.gauge.apply_hcn_hold();
                update_gauge_increase_timer(session, previous_gauge, audio_now);
                timer.passing_count_us -= HCN_UPDATE_US;
            }
        } else {
            timer.passing_count_us -= delta_us;
            while timer.passing_count_us < -HCN_UPDATE_US {
                session.gauge.apply_hcn_drain();
                timer.passing_count_us += HCN_UPDATE_US;
            }
        }
        session.lane_hcn_timer[idx] = Some(timer);
    }
}

pub fn sync_judge_windows(session: &mut GameSession, now: TimeUs) {
    let percent = judge_percent_at_time_for_keymode(
        session.chart.metadata.judge_rank_spec,
        &session.chart.judge_rank_events,
        now,
        session.chart.metadata.key_mode,
        session.rule_mode,
    );
    session.judge.set_window_set(judge_windows_for_rule_mode_and_keymode(
        session.base_judge_windows,
        percent,
        session.rule_mode,
        session.chart.metadata.key_mode,
    ));
}

fn sync_input_timestamp_anchor(session: &mut GameSession, audio_now: TimeUs) {
    session.input_timestamp_anchor = if session.audio_clock.running {
        Some(InputTimestampAnchor { monotonic_ns: monotonic_timestamp_ns(), audio_time: audio_now })
    } else {
        None
    };
}

pub fn advance_session_frame(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
) -> SessionFrame {
    let times = compute_frame_times(session);
    sync_input_timestamp_anchor(session, times.audio_now);
    rebase_pre_ready_visual_times(session, times.audio_now);
    update_scratch_angle_phase(session, times.audio_now);
    let mut judgements = Vec::new();

    if session.state == PlayState::Ready && times.audio_now.0 < 0 {
        drain_pre_ready_visual_inputs(session, times.audio_now);
    } else if session.state == PlayState::Ready {
        session.state = PlayState::Playing;
    }

    if matches!(session.state, PlayState::Ready | PlayState::Playing) {
        // BGMはchart 0に間に合うようREADY中もschedule-aheadする。
        // 判定・keysound・MineはPlayingに入るまで開始しない。
        session.bgm_scheduler.schedule_until(
            &session.chart,
            &session.audio_clock,
            times.audio_schedule_until,
            session.audio_mix.master_volume
                * session.audio_mix.normalization_gain
                * session.audio_mix.bgm_volume,
            audio,
        );
    }

    if session.state == PlayState::Playing {
        sync_judge_windows(session, times.audio_now);

        if session.replay_player.is_some() {
            drain_human_inputs(session);
            judgements.extend(process_replay_inputs(session, times.audio_now));
        } else {
            if session.autoplay.as_ref().is_some_and(AutoplayController::is_full) {
                // フルオート中は人間のキー入力を判定にも視覚エフェクトにも渡さない。
                // キービームは process_autoplay_inputs 側(ノーツ処理時)で発火する。
                // (ハイスピード等のオプション操作は app 側で別途処理される)
                discard_human_inputs(session);
            } else {
                judgements.extend(process_human_inputs(session));
            }
            judgements.extend(process_autoplay_inputs(session, times.audio_now));
            if session.autoplay.is_some() {
                apply_auto_key_release(session, times.audio_now);
            }
        }
        judgements.extend(process_mine_passes(session, times.audio_now));
        judgements.extend(process_misses(session, times.audio_now));
        update_hcn_lane_timers(session, times.audio_now);
        apply_hcn_gauge(session, times.audio_now);
        update_failed_state_from_gauge(session);
        schedule_keysounds(session, audio);
        update_recent_judgements(session, &judgements, times.audio_now);
        update_full_combo_timer(session, &judgements);

        if should_finish(session, times.audio_now) {
            session.state = PlayState::Finished;
        }
    }
    update_gauge_max_timer(session, times.audio_now);

    let mine_hits = std::mem::take(&mut session.pending_mine_hits);
    let keysound_volumes = std::mem::take(&mut session.pending_keysound_volumes);
    let skin_events = std::mem::take(&mut session.pending_skin_events);
    SessionFrame {
        times,
        judgements,
        mine_hits,
        keysound_volumes,
        skin_events,
        state: session.state,
    }
}

fn update_full_combo_timer(session: &mut GameSession, judgements: &[JudgementEvent]) {
    if session.full_combo_started_at.is_some()
        || session.scored_total_notes == 0
        || session.score.past_notes < session.scored_total_notes
        || session.score.combo < session.scored_total_notes
    {
        return;
    }
    session.full_combo_started_at = judgements
        .iter()
        .rev()
        .find(|event| event.affects_score && event.note_id.is_some())
        .map(|event| event.time)
        .or_else(|| Some(session.audio_clock.now()));
}

pub fn should_finish(session: &GameSession, audio_now: TimeUs) -> bool {
    session.judge.is_exhausted(&session.chart)
        && session.bgm_scheduler.is_done(&session.chart)
        && audio_now.0 > session.chart.end_time.0 + SESSION_END_MARGIN_US
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, SoundAssetRef, SoundEvent};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::input::{InputDeviceKind, InputSource};
    use bmz_core::judge::TimingSide;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use crate::input::backend::NullInputBackend;
    use crate::input::binding::LaneBinding;
    use crate::input::system::InputSystem;
    use crate::input::translator::DefaultInputTranslator;
    use crate::judge::model::JudgeWindow;

    use super::*;
    use crate::score::scored_note_count;

    #[derive(Default)]
    struct TestAudio {
        scheduled: Vec<ScheduledSound>,
    }

    impl AudioScheduler for TestAudio {
        fn schedule(&mut self, sound: ScheduledSound) {
            self.scheduled.push(sound);
        }
    }

    #[test]
    fn advance_session_frame_schedules_autoplay_keysounds() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.audio_mix.master_volume = 0.5;
        session.audio_mix.key_volume = 0.25;
        session.audio_mix.normalization_gain = 0.5;
        let mut audio = TestAudio::default();

        let frame = advance_session_frame(&mut session, &mut audio);

        assert_eq!(frame.judgements.len(), 1);
        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
        assert_eq!(audio.scheduled[0].start_frame, 0);
        assert_eq!(audio.scheduled[0].volume, 0.0625);
        assert_eq!(audio.scheduled[0].restart_policy, RestartPolicy::StopSameSound);
        assert_eq!(session.recent_judgements.len(), 1);
    }

    #[test]
    fn advance_session_frame_keeps_ready_until_chart_zero() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let current_frame = Arc::new(AtomicU64::new(0));
        session.audio_clock =
            AudioClock::with_position(48_000, 0, -1_000_000, current_frame.clone(), true);
        let mut audio = TestAudio::default();

        let ready_frame = advance_session_frame(&mut session, &mut audio);

        assert_eq!(ready_frame.state, PlayState::Ready);
        assert!(ready_frame.judgements.is_empty());
        assert!(audio.scheduled.is_empty());
        assert_eq!(session.score.past_notes, 0);

        current_frame.store(48_000, std::sync::atomic::Ordering::Relaxed);
        let playing_frame = advance_session_frame(&mut session, &mut audio);

        assert_eq!(playing_frame.state, PlayState::Playing);
        assert_eq!(playing_frame.judgements.len(), 1);
        assert_eq!(audio.scheduled.len(), 1);
    }

    #[test]
    fn session_frame_drains_ordered_skin_input_and_judgement_events() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;

        process_session_input(&mut session, human_press(TimeUs(0)));
        process_session_input(&mut session, human_release(TimeUs(10_000)));
        session.state = PlayState::Finished;

        let mut audio = TestAudio::default();
        let frame = advance_session_frame(&mut session, &mut audio);

        assert_eq!(frame.skin_events.len(), 3);
        assert_eq!(
            frame.skin_events.iter().map(|event| event.sequence).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(matches!(
            frame.skin_events[0].kind,
            SkinRuntimeEventKind::Input(InputEvent { kind: InputKind::Press, .. })
        ));
        assert!(matches!(
            frame.skin_events[1].kind,
            SkinRuntimeEventKind::Judgement(JudgementEvent { judge: Judge::PGreat, .. })
        ));
        assert!(matches!(
            frame.skin_events[2].kind,
            SkinRuntimeEventKind::Input(InputEvent { kind: InputKind::Release, .. })
        ));
        assert!(session.pending_skin_events.is_empty());

        let next_frame = advance_session_frame(&mut session, &mut audio);
        assert!(next_frame.skin_events.is_empty());
    }

    #[test]
    fn auto_key_release_emits_skin_release_event() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(0));
        session.lane_auto_release_at[Lane::Key1.index()] = Some(TimeUs(80_000));

        apply_auto_key_release(&mut session, TimeUs(80_000));

        assert!(matches!(
            session.pending_skin_events.as_slice(),
            [SkinRuntimeEvent {
                sequence: 0,
                kind: SkinRuntimeEventKind::Input(InputEvent {
                    lane: Lane::Key1,
                    kind: InputKind::Release,
                    time: TimeUs(80_000),
                    source: InputSource::Auto,
                    ..
                }),
            }]
        ));
    }

    #[test]
    fn empty_poor_schedules_target_note_keysound() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;
        let mut audio = TestAudio::default();

        let judgements = process_session_input(&mut session, human_press(TimeUs(150_000)));
        schedule_keysounds(&mut session, &mut audio);

        assert_eq!(judgements.len(), 1);
        assert_eq!(judgements[0].judge, Judge::EmptyPoor);
        assert_eq!(judgements[0].note_id, None);
        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
    }

    #[test]
    fn unjudged_press_after_empty_poor_window_uses_previous_playable_keysound() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;
        let mut audio = TestAudio::default();

        let judgements = process_session_input(&mut session, human_press(TimeUs(800_000)));
        schedule_keysounds(&mut session, &mut audio);

        assert!(judgements.is_empty());
        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
    }

    #[test]
    fn unjudged_press_after_empty_poor_window_prefers_previous_invisible_keysound() {
        let mut session = session_with_autoplay(chart_with_invisible_keysound());
        session.autoplay = None;
        let mut audio = TestAudio::default();

        let judgements = process_session_input(&mut session, human_press(TimeUs(800_000)));
        schedule_keysounds(&mut session, &mut audio);

        assert!(judgements.is_empty());
        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(8));
    }

    #[test]
    fn ln_release_does_not_replay_start_keysound_when_end_has_no_sound() {
        let mut session = session_with_autoplay(ln_chart_with_start_sound_and_end_sound(None));
        session.autoplay = None;
        let mut audio = TestAudio::default();

        process_session_input(&mut session, human_press(TimeUs(0)));
        schedule_keysounds(&mut session, &mut audio);
        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
        audio.scheduled.clear();

        process_session_input(&mut session, human_release(TimeUs(1_000_000)));
        schedule_keysounds(&mut session, &mut audio);

        assert!(audio.scheduled.is_empty());
    }

    #[test]
    fn ln_release_plays_end_keysound_when_end_has_sound() {
        let mut session =
            session_with_autoplay(ln_chart_with_start_sound_and_end_sound(Some(SoundId(9))));
        session.autoplay = None;
        let mut audio = TestAudio::default();

        process_session_input(&mut session, human_press(TimeUs(0)));
        schedule_keysounds(&mut session, &mut audio);
        audio.scheduled.clear();

        process_session_input(&mut session, human_release(TimeUs(1_000_000)));
        schedule_keysounds(&mut session, &mut audio);

        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(9));
    }

    #[test]
    fn early_bad_ln_release_mutes_held_start_keysound() {
        let mut session = session_with_autoplay(ln_chart_with_start_sound_and_end_sound(None));
        session.autoplay = None;

        process_session_input(&mut session, human_press(TimeUs(0)));
        session.pending_keysounds.clear();
        session.pending_keysound_volumes.clear();

        let judgements = process_session_input(&mut session, human_release(TimeUs(700_000)));

        assert_eq!(judgements.len(), 1);
        assert_eq!(judgements[0].judge, Judge::Bad);
        assert_eq!(session.pending_keysound_volumes, vec![(SoundId(7), 0.0)]);
    }

    #[test]
    fn input_offset_auto_adjust_increases_after_ten_late_judgements() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.input_offset_auto_adjust = Some(InputOffsetAutoAdjustState::default());

        let events = vec![judgement_event(Judge::Great, 2_000); 10];
        apply_input_offset_auto_adjust(&mut session, &events);

        assert_eq!(session.offsets.visual_offset_us, 1_000);
        assert_eq!(session.offsets.input_offset_us, 0);
        assert_eq!(session.input_offset_auto_adjust, Some(InputOffsetAutoAdjustState::default()));
    }

    #[test]
    fn input_offset_auto_adjust_decreases_after_ten_early_judgements() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.input_offset_auto_adjust = Some(InputOffsetAutoAdjustState::default());

        let events = vec![judgement_event(Judge::Good, -2_000); 10];
        apply_input_offset_auto_adjust(&mut session, &events);

        assert_eq!(session.offsets.visual_offset_us, -1_000);
        assert_eq!(session.offsets.input_offset_us, 0);
    }

    #[test]
    fn input_offset_auto_adjust_ignores_poor_and_empty_poor() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.input_offset_auto_adjust = Some(InputOffsetAutoAdjustState::default());

        let mut events = vec![judgement_event(Judge::Poor, 30_000); 10];
        events.extend(vec![judgement_event(Judge::EmptyPoor, 30_000); 10]);
        apply_input_offset_auto_adjust(&mut session, &events);

        assert_eq!(session.offsets.visual_offset_us, 0);
        assert_eq!(session.offsets.input_offset_us, 0);
        assert_eq!(session.input_offset_auto_adjust.unwrap().count, 0);
    }

    #[test]
    fn advance_session_frame_starts_full_combo_timer_after_last_note() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        assert_eq!(session.full_combo_started_at, Some(TimeUs(0)));
    }

    #[test]
    fn scored_note_count_uses_effective_long_note_mode() {
        let mut chart = chart_with_hcn_long_note();
        assert_eq!(scored_note_count(&chart), 2);

        chart.long_notes[0].mode = Some(LongNoteMode::Cn);
        assert_eq!(scored_note_count(&chart), 2);

        chart.long_notes[0].mode = Some(LongNoteMode::Ln);
        assert_eq!(scored_note_count(&chart), 1);

        chart.long_notes[0].mode = None;
        chart.metadata.long_note_mode = LongNoteMode::Hcn;
        assert_eq!(scored_note_count(&chart), 2);
    }

    #[test]
    fn full_combo_timer_waits_for_cn_end_judgement() {
        let mut chart = chart_with_hcn_long_note();
        chart.long_notes[0].mode = Some(LongNoteMode::Cn);
        let mut session = session_with_autoplay(chart);

        let start_events = apply_judge_outcome(
            &mut session,
            JudgeOutcome { events: vec![judgement_event(Judge::PGreat, 0)], ..Default::default() },
        );
        update_full_combo_timer(&mut session, &start_events);

        assert_eq!(session.scored_total_notes, 2);
        assert_eq!(session.score.past_notes, 1);
        assert_eq!(session.full_combo_started_at, None);

        let mut end_event = judgement_event(Judge::PGreat, 0);
        end_event.note_id = Some(NoteId(2));
        end_event.time = TimeUs(1_000_000);
        let end_events = apply_judge_outcome(
            &mut session,
            JudgeOutcome { events: vec![end_event], ..Default::default() },
        );
        update_full_combo_timer(&mut session, &end_events);

        assert_eq!(session.score.past_notes, 2);
        assert_eq!(session.full_combo_started_at, Some(TimeUs(1_000_000)));
    }

    #[test]
    fn hard_gauge_zero_moves_session_to_failed() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.state = PlayState::Playing;
        session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 1000);
        session
            .gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == bmz_core::clear::GaugeType::Hard)
            .unwrap()
            .value = 0.0;

        update_failed_state_from_gauge(&mut session);

        assert_eq!(session.state, PlayState::Failed);
    }

    #[test]
    fn gauge_increase_timer_starts_when_judge_raises_gauge() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.gauge.set_initial_value(50.0);

        apply_judge_outcome(
            &mut session,
            JudgeOutcome {
                events: vec![JudgementEvent {
                    time: TimeUs(123_000),
                    ..judgement_event(Judge::PGreat, 0)
                }],
                ..Default::default()
            },
        );

        assert_eq!(session.gauge_increase_started_at, Some(TimeUs(123_000)));
    }

    #[test]
    fn gauge_max_timer_tracks_current_max_state() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.gauge.set_initial_value(99.0);

        update_gauge_max_timer(&mut session, TimeUs(25_000));
        assert_eq!(session.gauge_max_started_at, None);

        session.gauge.set_initial_value(100.0);
        update_gauge_max_timer(&mut session, TimeUs(50_000));
        assert_eq!(session.gauge_max_started_at, Some(TimeUs(50_000)));

        update_gauge_max_timer(&mut session, TimeUs(75_000));
        assert_eq!(session.gauge_max_started_at, Some(TimeUs(50_000)));

        session.gauge.set_initial_value(99.0);
        update_gauge_max_timer(&mut session, TimeUs(100_000));
        assert_eq!(session.gauge_max_started_at, None);
    }

    #[test]
    fn course_combo_carry_extends_display_combo_without_changing_score_max() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.course_combo_carry = 100;
        session.course_combo_carry_active = true;
        session.course_max_combo = 100;

        apply_judge_outcome(
            &mut session,
            JudgeOutcome { events: vec![judgement_event(Judge::PGreat, 0)], ..Default::default() },
        );

        assert_eq!(session.score.combo, 1);
        assert_eq!(session.score.max_combo, 1);
        assert_eq!(session.display_combo(), 101);
        assert_eq!(session.display_max_combo(), 101);
    }

    #[test]
    fn course_combo_carry_resets_on_combo_break() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.course_combo_carry = 100;
        session.course_combo_carry_active = true;
        session.course_max_combo = 100;

        apply_judge_outcome(
            &mut session,
            JudgeOutcome { events: vec![judgement_event(Judge::PGreat, 0)], ..Default::default() },
        );
        apply_judge_outcome(
            &mut session,
            JudgeOutcome { events: vec![judgement_event(Judge::Bad, 0)], ..Default::default() },
        );
        apply_judge_outcome(
            &mut session,
            JudgeOutcome { events: vec![judgement_event(Judge::Great, 0)], ..Default::default() },
        );

        assert!(!session.course_combo_carry_active);
        assert_eq!(session.score.combo, 1);
        assert_eq!(session.score.max_combo, 1);
        assert_eq!(session.display_combo(), 1);
        assert_eq!(session.display_max_combo(), 101);
    }

    #[test]
    fn auto_shift_hard_zero_falls_back_without_failed_state() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.state = PlayState::Playing;
        session.gauge = GaugeState::new_auto_shift(160.0, 1000);

        session.gauge.apply_judge(Judge::Poor, 7.0);
        update_failed_state_from_gauge(&mut session);

        assert_eq!(session.state, PlayState::Playing);
        assert_eq!(session.gauge.selected, bmz_core::clear::GaugeType::Hard);
    }

    /// Key1 に HCN ロングノート (0s 〜 1s, キー音 SoundId(7)) を持つ譜面。
    fn chart_with_hcn_long_note() -> PlayableChart {
        let mut chart = chart_with_keysound();
        chart.lane_notes = std::array::from_fn(|_| Vec::new());
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::LongStart,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: Some(SoundId(7)),
            damage: None,
        });
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(2),
            lane: Lane::Key1,
            kind: NoteKind::LongEnd,
            tick: ChartTick(192),
            time: TimeUs(1_000_000),
            sound: None,
            damage: None,
        });
        chart.long_notes.push(bmz_chart::model::LongNotePair {
            lane: Lane::Key1,
            style: bmz_chart::model::LongNoteStyle::ChannelPair,
            mode: Some(LongNoteMode::Hcn),
            start_note_id: NoteId(1),
            end_note_id: NoteId(2),
            start_tick: ChartTick(0),
            end_tick: ChartTick(192),
            start_time: TimeUs(0),
            end_time: TimeUs(1_000_000),
            sound: Some(SoundId(7)),
        });
        chart
    }

    fn human_press(time: TimeUs) -> InputEvent {
        InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time,
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }
    }

    fn human_release(time: TimeUs) -> InputEvent {
        InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Release,
            time,
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }
    }

    fn chart_with_invisible_keysound() -> PlayableChart {
        let mut chart = chart_with_keysound();
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(2),
            lane: Lane::Key1,
            kind: NoteKind::Invisible,
            tick: ChartTick(96),
            time: TimeUs(500_000),
            sound: Some(SoundId(8)),
            damage: None,
        });
        chart.sounds.push(SoundAssetRef { id: SoundId(8), path: "hidden.wav".into() });
        chart.end_time = TimeUs(500_000);
        chart
    }

    fn ln_chart_with_start_sound_and_end_sound(end_sound: Option<SoundId>) -> PlayableChart {
        let mut chart = chart_with_hcn_long_note();
        chart.metadata.long_note_mode = LongNoteMode::Ln;
        chart.long_notes[0].mode = Some(LongNoteMode::Ln);
        chart.lane_notes[Lane::Key1.index()][1].sound = end_sound;
        if let Some(sound_id) = end_sound {
            chart.sounds.push(SoundAssetRef {
                id: sound_id,
                path: format!("sound-{}.wav", sound_id.0).into(),
            });
        }
        chart
    }

    #[test]
    fn hcn_gauge_increases_while_passing_and_pressed_until_end() {
        let mut session = session_with_autoplay(chart_with_hcn_long_note());
        session.gauge.set_initial_value(50.0);
        session.judge.judged_notes.insert(NoteId(1), Judge::PGreat);
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(0));

        update_hcn_lane_timers(&mut session, TimeUs(0));
        apply_hcn_gauge(&mut session, TimeUs(0));
        update_hcn_lane_timers(&mut session, TimeUs(500_000));
        apply_hcn_gauge(&mut session, TimeUs(500_000));
        let mid = session.gauge.current().value;
        // 終端通過後は passing が外れ、ゲージは変化しない。
        update_hcn_lane_timers(&mut session, TimeUs(2_000_000));
        apply_hcn_gauge(&mut session, TimeUs(2_000_000));
        update_hcn_lane_timers(&mut session, TimeUs(3_000_000));
        apply_hcn_gauge(&mut session, TimeUs(3_000_000));

        assert!(mid > 50.0);
        assert!((session.gauge.current().value - mid).abs() < f32::EPSILON);
    }

    #[test]
    fn hcn_gauge_recovers_when_pressed_after_missed_start() {
        let mut session = session_with_autoplay(chart_with_hcn_long_note());
        session.gauge.set_initial_value(50.0);
        // 始端を見逃し (POOR) て離している → 減衰。
        session.judge.judged_notes.insert(NoteId(1), Judge::Poor);

        update_hcn_lane_timers(&mut session, TimeUs(100_000));
        apply_hcn_gauge(&mut session, TimeUs(100_000));
        // 250ms 経過 → mpassingcount が -200ms を下回り減衰 1 tick。
        update_hcn_lane_timers(&mut session, TimeUs(350_000));
        apply_hcn_gauge(&mut session, TimeUs(350_000));
        let drained = session.gauge.current().value;
        assert!(drained < 50.0);

        // 途中から押し直すと回復に転じる (beatoraja passing ベース)。
        // カウンタは反転でリセットされないため、残り -50ms を打ち消して
        // +200ms を超えるまで押し続けると回復 1 tick が入る。
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(350_000));
        update_hcn_lane_timers(&mut session, TimeUs(800_000));
        apply_hcn_gauge(&mut session, TimeUs(800_000));

        assert!(session.gauge.current().value > drained);
    }

    #[test]
    fn hcn_keysound_mutes_on_release_after_early_end_judge() {
        let mut session = session_with_autoplay(chart_with_hcn_long_note());
        session.judge.judged_notes.insert(NoteId(1), Judge::PGreat);
        // 早離しで終端が BAD 判定済み、キーは離している。
        session.judge.judged_notes.insert(NoteId(2), Judge::Bad);

        update_hcn_lane_timers(&mut session, TimeUs(500_000));
        assert_eq!(session.pending_keysound_volumes, vec![(SoundId(7), 0.0)]);

        // 押し直すと元の音量へ復帰する。同一状態の継続では再送しない。
        session.pending_keysound_volumes.clear();
        update_hcn_lane_timers(&mut session, TimeUs(600_000));
        assert!(session.pending_keysound_volumes.is_empty());
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(700_000));
        update_hcn_lane_timers(&mut session, TimeUs(700_000));
        assert_eq!(session.pending_keysound_volumes.len(), 1);
        assert_eq!(session.pending_keysound_volumes[0].0, SoundId(7));
        assert!(session.pending_keysound_volumes[0].1 > 0.0);
    }

    #[test]
    fn hcn_keysound_volume_untouched_while_end_unjudged() {
        let mut session = session_with_autoplay(chart_with_hcn_long_note());
        session.judge.judged_notes.insert(NoteId(1), Judge::PGreat);
        // 終端未判定で離していても音量は触らない (beatoraja: pair state > 3 のみ)。
        update_hcn_lane_timers(&mut session, TimeUs(500_000));
        assert!(session.pending_keysound_volumes.is_empty());
    }

    #[test]
    fn ln_mode_scores_once_at_long_note_end() {
        let mut chart = chart_with_hcn_long_note();
        chart.metadata.long_note_mode = LongNoteMode::Ln;
        chart.long_notes[0].mode = Some(LongNoteMode::Ln);
        let mut session = session_with_autoplay(chart);
        session.autoplay = None;

        let press = session.judge.process_input(
            &session.chart,
            InputEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(0),
                source: InputSource::Human,
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            },
        );
        apply_judge_outcome(&mut session, press);
        assert_eq!(session.score.past_notes, 0);
        assert_eq!(session.score.combo, 0);

        let end = session.judge.process_misses(&session.chart, TimeUs(1_000_001));
        apply_judge_outcome(&mut session, end);

        assert_eq!(session.score.past_notes, 1);
        assert_eq!(session.score.combo, 1);
        assert_eq!(session.score.ex_score(), 2);
    }

    fn chart_with_mine(time: TimeUs, damage: u16) -> PlayableChart {
        let mut chart = chart_with_keysound();
        chart.lane_notes = std::array::from_fn(|_| Vec::new());
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(7),
            lane: Lane::Key1,
            kind: NoteKind::Mine,
            tick: ChartTick(0),
            time,
            sound: None,
            damage: Some(damage),
        });
        chart.total_notes = 0;
        chart.end_time = time;
        chart
    }

    #[test]
    fn process_mine_passes_applies_damage_for_held_human_lane() {
        let mut session = session_with_autoplay(chart_with_mine(TimeUs(1_000_000), 8));
        session.autoplay = None;
        session.gauge.set_initial_value(50.0);
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));

        let events = process_mine_passes(&mut session, TimeUs(1_000_000));

        assert!(events.is_empty());
        assert_eq!(session.pending_mine_hits.len(), 1);
        assert_eq!(session.pending_mine_hits[0].note_id, NoteId(7));
        assert!((session.gauge.current().value - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn process_mine_passes_ignores_autoplay_lane() {
        let mut session = session_with_autoplay(chart_with_mine(TimeUs(1_000_000), 8));
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));

        process_mine_passes(&mut session, TimeUs(1_000_000));

        assert!(session.pending_mine_hits.is_empty());
        assert!((session.gauge.current().value - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn advance_session_frame_schedules_bgm_with_mix_volume() {
        let mut session = session_with_autoplay(chart_with_bgm());
        session.audio_mix.master_volume = 0.5;
        session.audio_mix.bgm_volume = 0.75;
        session.audio_mix.normalization_gain = 0.5;
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(3));
        assert_eq!(audio.scheduled[0].volume, 0.1875);
        assert_eq!(audio.scheduled[0].restart_policy, RestartPolicy::StopSameSound);
    }

    #[test]
    fn advance_session_frame_applies_chart_volume_channels() {
        let mut chart = chart_with_keysound();
        chart.key_volume_events.push(bmz_chart::model::ChartVolumeEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            value: 128,
        });
        let mut session = session_with_autoplay(chart);
        session.audio_mix.master_volume = 1.0;
        session.audio_mix.key_volume = 1.0;
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        let expected = 128.0 / 255.0;
        assert!((audio.scheduled[0].volume - expected).abs() < 0.001);

        let mut bgm_chart = chart_with_bgm();
        bgm_chart.bgm_volume_events.push(bmz_chart::model::ChartVolumeEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            value: 64,
        });
        let mut bgm_session = session_with_autoplay(bgm_chart);
        bgm_session.audio_mix.master_volume = 1.0;
        bgm_session.audio_mix.bgm_volume = 1.0;
        let mut bgm_audio = TestAudio::default();

        advance_session_frame(&mut bgm_session, &mut bgm_audio);

        let expected_bgm = 64.0 / 255.0;
        assert!((bgm_audio.scheduled[0].volume - expected_bgm).abs() < 0.001);
    }

    #[test]
    fn update_recent_judgements_expires_old_events() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let event = JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge: Judge::PGreat,
            side: TimingSide::Slow,
            delta: TimeUs(0),
            time: TimeUs(0),
            affects_score: true,
        };

        update_recent_judgements(&mut session, &[event], TimeUs(0));
        update_recent_judgements(&mut session, &[], TimeUs(JUDGEMENT_DISPLAY_US + 1));

        assert!(session.recent_judgements.is_empty());
    }

    #[test]
    fn advance_session_frame_skips_human_inputs_when_replay_active() {
        use crate::input::backend::{
            BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
        };
        use crate::input::binding::{BindingEntry, LaneBinding};

        let chart = chart_with_keysound();
        let mut session = session_with_autoplay(chart);
        // 入力バインディングを設定して Z キーを Key1 にマップ
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });
        session.input_system = InputSystem {
            backend: Box::new(backend),
            translator: Box::new(DefaultInputTranslator {
                binding: LaneBinding {
                    entries: vec![BindingEntry {
                        device: None,
                        control: PhysicalControl::KeyboardKey("Z".to_string()),
                        lane: Lane::Key1,
                        scratch_direction: None,
                    }],
                },
            }),
        };
        session.replay_player = Some(crate::replay::ReplayPlayer::default());
        session.autoplay = None;
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        // 人間入力は judge にも recorder にも渡らない
        assert_eq!(session.score.judges.fast_pgreat + session.score.judges.slow_pgreat, 0);
        assert_eq!(session.score.judges.fast_great + session.score.judges.slow_great, 0);
        assert!(session.replay_recorder.events.is_empty());
        // recent_inputs だけは Press が反映される (視覚エフェクト用)
        assert_eq!(session.recent_inputs.len(), 1);
    }

    #[test]
    fn advance_session_frame_skips_human_inputs_when_autoplay_active() {
        use crate::input::backend::{
            BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
        };
        use crate::input::binding::{BindingEntry, LaneBinding};

        let chart = chart_with_keysound();
        let mut session = session_with_autoplay(chart);
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });
        session.input_system = InputSystem {
            backend: Box::new(backend),
            translator: Box::new(DefaultInputTranslator {
                binding: LaneBinding {
                    entries: vec![BindingEntry {
                        device: None,
                        control: PhysicalControl::KeyboardKey("Z".to_string()),
                        lane: Lane::Key1,
                        scratch_direction: None,
                    }],
                },
            }),
        };
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        // オートプレイ中は人間入力を recorder に渡さない。
        assert!(session.replay_recorder.events.is_empty());
        // 人間のキー入力は recent_inputs(キービーム)にも反映されない。
        // recent_inputs に乗るのは autoplay のノーツ処理入力のみ。
        assert!(session.recent_inputs.iter().all(|i| i.source == InputSource::Auto));
        assert!(
            session.recent_inputs.iter().all(|i| i.source != InputSource::Human),
            "human key press must not produce a keybeam during autoplay",
        );
    }

    #[test]
    fn process_autoplay_inputs_flashes_keybeam_on_note_processing() {
        let mut session = session_with_autoplay(chart_with_keysound());

        // chart_with_keysound のノーツは time=0 / Key1。audio_now=0 で処理される。
        let judgements = process_autoplay_inputs(&mut session, TimeUs(0));

        assert!(!judgements.is_empty(), "autoplay should judge the note");
        // ノーツ処理に伴って autoplay 入力が recent_inputs に積まれる(キービーム発火)。
        assert_eq!(session.recent_inputs.len(), 1);
        assert_eq!(session.recent_inputs[0].lane, Lane::Key1);
        assert_eq!(session.recent_inputs[0].source, InputSource::Auto);
    }

    #[test]
    fn update_lane_key_states_press_sets_keyon_clears_keyoff() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.lane_keyoff_started_at[Lane::Key1.index()] = Some(TimeUs(1_000));

        let inputs = [InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(5_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }];
        update_lane_key_states(&mut session, &inputs);

        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(5_000)));
        assert_eq!(session.lane_keyoff_started_at[Lane::Key1.index()], None);
        // Human source の Press は自動 release を予約しない (押し続け対応)。
        assert_eq!(session.lane_auto_release_at[Lane::Key1.index()], None);
    }

    #[test]
    fn update_lane_key_states_tracks_scratch_direction_until_release() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let press = [InputEvent {
            lane: Lane::Scratch,
            kind: InputKind::Press,
            time: TimeUs(5_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Controller,
            scratch_direction: Some(ScratchDirection::Up),
        }];

        update_lane_key_states(&mut session, &press);

        assert_eq!(
            session.lane_scratch_direction[Lane::Scratch.index()],
            Some(ScratchDirection::Up)
        );

        let release = [InputEvent {
            lane: Lane::Scratch,
            kind: InputKind::Release,
            time: TimeUs(10_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Controller,
            scratch_direction: Some(ScratchDirection::Up),
        }];
        update_lane_key_states(&mut session, &release);

        assert_eq!(session.lane_scratch_direction[Lane::Scratch.index()], None);
    }

    #[test]
    fn update_scratch_angle_phase_accumulates_directionless_scratch_until_release() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.scratch_angle_last_render_at = Some(TimeUs(0));
        session.lane_keyon_started_at[Lane::Scratch.index()] = Some(TimeUs(0));

        update_scratch_angle_phase(&mut session, TimeUs(1_000_000));

        assert_eq!(session.lane_scratch_angle_delta_ms[Lane::Scratch.index()], 160);

        update_lane_key_states(
            &mut session,
            &[InputEvent {
                lane: Lane::Scratch,
                kind: InputKind::Release,
                time: TimeUs(1_000_000),
                source: InputSource::Human,
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
        );
        update_scratch_angle_phase(&mut session, TimeUs(1_500_000));

        assert_eq!(session.lane_scratch_angle_delta_ms[Lane::Scratch.index()], 160);
    }

    #[test]
    fn rebase_pre_ready_visual_times_keeps_timer_elapsed_across_clock_reset() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.scratch_angle_last_render_at = Some(TimeUs(5_000_000));
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(2_000_000));
        session.lane_keyoff_started_at[Lane::Key2.index()] = Some(TimeUs(3_000_000));
        session.recent_inputs.push(InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(4_900_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        });

        rebase_pre_ready_visual_times(&mut session, TimeUs(-1_000_000));

        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(-4_000_000)));
        assert_eq!(session.lane_keyoff_started_at[Lane::Key2.index()], Some(TimeUs(-3_000_000)));
        assert_eq!(session.recent_inputs[0].time, TimeUs(-1_100_000));
        assert_eq!(session.scratch_angle_last_render_at, Some(TimeUs(-1_000_000)));

        // READY/PLAYに移行しても押下状態は保たれ、実際のReleaseでのみ解除される。
        update_lane_key_states(
            &mut session,
            &[InputEvent {
                lane: Lane::Key1,
                kind: InputKind::Release,
                time: TimeUs(-500_000),
                source: InputSource::Human,
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
        );
        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], None);
        assert_eq!(session.lane_keyoff_started_at[Lane::Key1.index()], Some(TimeUs(-500_000)));
    }

    #[test]
    fn sync_input_timestamp_anchor_tracks_running_audio_clock() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.input_timestamp_anchor =
            Some(InputTimestampAnchor { monotonic_ns: 123, audio_time: TimeUs(456) });

        sync_input_timestamp_anchor(&mut session, TimeUs(1_234_567));

        assert!(session.input_timestamp_anchor.is_none());

        session.audio_clock.running = true;
        sync_input_timestamp_anchor(&mut session, TimeUs(1_234_567));

        let anchor = session.input_timestamp_anchor.unwrap();
        assert_eq!(anchor.audio_time, TimeUs(1_234_567));
        assert!(anchor.monotonic_ns <= monotonic_timestamp_ns());
    }

    #[test]
    fn process_human_inputs_uses_monotonic_event_time() {
        use crate::input::backend::{
            BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
        };
        use crate::input::binding::{BindingEntry, LaneBinding};

        let mut session = session_with_autoplay(chart_with_bgm());
        session.autoplay = None;
        session.audio_clock.running = true;
        session.audio_clock.current_frame = Arc::new(AtomicU64::new(48_000));
        session.input_timestamp_anchor =
            Some(InputTimestampAnchor { monotonic_ns: 2_000_000, audio_time: TimeUs(1_000_000) });
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::MonotonicNs(1_500_000),
        });
        session.input_system = InputSystem {
            backend: Box::new(backend),
            translator: Box::new(DefaultInputTranslator {
                binding: LaneBinding {
                    entries: vec![BindingEntry {
                        device: None,
                        control: PhysicalControl::KeyboardKey("Z".to_string()),
                        lane: Lane::Key1,
                        scratch_direction: None,
                    }],
                },
            }),
        };

        let judgements = process_human_inputs(&mut session);

        assert!(judgements.is_empty());
        assert_eq!(session.replay_recorder.events[0].time, TimeUs(999_500));
        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(999_500)));
    }

    #[test]
    fn drain_pre_ready_visual_inputs_updates_lane_key_states_without_judging() {
        use crate::input::backend::{
            BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
        };
        use crate::input::binding::{BindingEntry, LaneBinding};

        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });
        session.input_system = InputSystem {
            backend: Box::new(backend),
            translator: Box::new(DefaultInputTranslator {
                binding: LaneBinding {
                    entries: vec![BindingEntry {
                        device: None,
                        control: PhysicalControl::KeyboardKey("Z".to_string()),
                        lane: Lane::Key1,
                        scratch_direction: None,
                    }],
                },
            }),
        };

        drain_pre_ready_visual_inputs(&mut session, TimeUs(2_000_000));

        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(2_000_000)));
        assert_eq!(session.recent_inputs.len(), 1);
        assert_eq!(session.score.past_notes, 0);
    }

    #[test]
    fn drain_pre_ready_visual_inputs_advances_scratch_phase() {
        use crate::input::backend::{
            BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
        };
        use crate::input::binding::{BindingEntry, LaneBinding};

        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::GamepadButton("scratch-up".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });
        session.input_system = InputSystem {
            backend: Box::new(backend),
            translator: Box::new(DefaultInputTranslator {
                binding: LaneBinding {
                    entries: vec![BindingEntry {
                        device: None,
                        control: PhysicalControl::GamepadButton("scratch-up".to_string()),
                        lane: Lane::Scratch,
                        scratch_direction: Some(ScratchDirection::Up),
                    }],
                },
            }),
        };

        drain_pre_ready_visual_inputs(&mut session, TimeUs(2_000_000));
        drain_pre_ready_visual_inputs(&mut session, TimeUs(2_500_000));

        assert_eq!(session.lane_scratch_angle_delta_ms[Lane::Scratch.index()], 1_000);
        assert_eq!(session.scratch_angle_last_render_at, Some(TimeUs(2_500_000)));
        assert_eq!(session.score.past_notes, 0);
        assert!(session.replay_recorder.events.is_empty());
    }

    #[test]
    fn drain_pre_ready_visual_inputs_discards_human_inputs_during_full_autoplay() {
        use crate::input::backend::{
            BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
        };
        use crate::input::binding::{BindingEntry, LaneBinding};

        let mut session = session_with_autoplay(chart_with_keysound());
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });
        session.input_system = InputSystem {
            backend: Box::new(backend),
            translator: Box::new(DefaultInputTranslator {
                binding: LaneBinding {
                    entries: vec![BindingEntry {
                        device: None,
                        control: PhysicalControl::KeyboardKey("Z".to_string()),
                        lane: Lane::Key1,
                        scratch_direction: None,
                    }],
                },
            }),
        };

        drain_pre_ready_visual_inputs(&mut session, TimeUs(2_000_000));

        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], None);
        assert!(session.recent_inputs.is_empty());
        assert_eq!(session.score.past_notes, 0);
        assert!(session.replay_recorder.events.is_empty());
    }

    #[test]
    fn update_lane_key_states_release_transitions_to_keyoff() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(1_000));

        let inputs = [InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Release,
            time: TimeUs(10_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }];
        update_lane_key_states(&mut session, &inputs);

        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], None);
        assert_eq!(session.lane_keyoff_started_at[Lane::Key1.index()], Some(TimeUs(10_000)));
    }

    #[test]
    fn update_lane_key_states_autoplay_press_schedules_auto_release() {
        let mut session = session_with_autoplay(chart_with_keysound());

        let inputs = [InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(5_000),
            source: InputSource::Auto,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }];
        update_lane_key_states(&mut session, &inputs);

        // Auto は AUTO_KEYBEAM_DURATION_US (80ms) 後に自動 release が予約される。
        assert_eq!(
            session.lane_auto_release_at[Lane::Key1.index()],
            Some(TimeUs(5_000 + AUTO_KEYBEAM_DURATION_US))
        );
    }

    #[test]
    fn apply_auto_key_release_transitions_after_duration() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let inputs = [InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(0),
            source: InputSource::Auto,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }];
        update_lane_key_states(&mut session, &inputs);

        // 期限前: 何も起きない。
        apply_auto_key_release(&mut session, TimeUs(AUTO_KEYBEAM_DURATION_US - 1));
        assert!(session.lane_keyon_started_at[Lane::Key1.index()].is_some());
        assert!(session.lane_keyoff_started_at[Lane::Key1.index()].is_none());

        // 期限到達: keyon → keyoff へ遷移。
        apply_auto_key_release(&mut session, TimeUs(AUTO_KEYBEAM_DURATION_US));
        assert!(session.lane_keyon_started_at[Lane::Key1.index()].is_none());
        assert_eq!(
            session.lane_keyoff_started_at[Lane::Key1.index()],
            Some(TimeUs(AUTO_KEYBEAM_DURATION_US))
        );
        assert!(session.lane_auto_release_at[Lane::Key1.index()].is_none());
    }

    #[test]
    fn update_recent_inputs_keeps_presses_and_expires_old_events() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let inputs = [
            InputEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(10_000),
                source: InputSource::Human,
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            },
            InputEvent {
                lane: Lane::Key2,
                kind: InputKind::Release,
                time: TimeUs(20_000),
                source: InputSource::Human,
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            },
        ];

        update_recent_inputs(&mut session, &inputs, TimeUs(10_000));
        assert_eq!(session.recent_inputs.len(), 1);
        assert_eq!(session.recent_inputs[0].lane, Lane::Key1);
        update_recent_inputs(&mut session, &[], TimeUs(10_000 + INPUT_DISPLAY_US + 1));

        assert!(session.recent_inputs.is_empty());
    }

    fn session_with_autoplay(chart: PlayableChart) -> GameSession {
        let chart = Arc::new(chart);
        let timing_map =
            TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
        GameSession {
            chart: Arc::clone(&chart),
            scored_total_notes: scored_note_count(&chart),
            timing_map,
            audio_clock: AudioClock {
                sample_rate: 48_000,
                start_output_frame: 0,
                chart_zero_time_us: 0,
                current_frame: Arc::new(AtomicU64::new(0)),
                running: false,
            },
            input_system: InputSystem {
                backend: Box::new(NullInputBackend),
                translator: Box::new(DefaultInputTranslator {
                    binding: LaneBinding { entries: Vec::new() },
                }),
            },
            judge: JudgeEngine::new(JudgeWindow::symmetric(
                16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000,
            )),
            base_judge_window: JudgeWindow::symmetric(
                16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000,
            ),
            base_judge_windows: JudgeWindows::uniform(JudgeWindow::symmetric(
                16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000,
            )),
            rule_mode: RuleMode::Beatoraja,
            score: ScoreState::default(),
            course_combo_carry: 0,
            course_combo_carry_active: false,
            course_max_combo: 0,
            gauge: GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, chart.total_notes),
            replay_recorder: ReplayRecorder::default(),
            replay_player: None,
            autoplay: Some(AutoplayController::default()),
            recent_inputs: Vec::new(),
            lane_keyon_started_at: Default::default(),
            lane_keyoff_started_at: Default::default(),
            lane_scratch_direction: Default::default(),
            lane_scratch_angle_delta_ms: Default::default(),
            scratch_angle_last_render_at: None,
            lane_auto_release_at: Default::default(),
            recent_judgements: Vec::new(),
            pending_skin_events: Vec::new(),
            next_skin_event_sequence: 0,
            result_judgements: Default::default(),
            hit_error_ring: HitErrorRing::default(),
            gauge_increase_started_at: None,
            gauge_max_started_at: None,
            full_combo_started_at: None,
            bgm_scheduler: BgmScheduler::default(),
            offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
            input_offset_auto_adjust_enabled: false,
            input_offset_auto_adjust: None,
            audio_mix: PlayAudioMix {
                master_volume: 1.0,
                normalization_gain: 1.0,
                key_volume: 1.0,
                bgm_volume: 1.0,
            },
            hispeed: 2.0,
            hispeed_mode: HispeedMode::Normal,
            target_green_number: 300,
            hsfix_base_bpm: 120.0,
            lift: 0.0,
            lane_cover: 0.0,
            lane_cover_visible: true,
            lane_cover_changing: false,
            lanecover_enabled: false,
            lift_enabled: true,
            hidden_enabled: false,
            hispeed_auto_adjust: false,
            hidden_cover: 0.0,
            skin_offsets: Vec::new(),
            bga_enabled: true,
            poor_bga_duration_us: 500_000,
            bga_stretch: 1,
            show_ln_tail_cap: false,
            lane_hcn_timer: [None; LANE_COUNT],
            lane_hcn_keysound_muted: [None; LANE_COUNT],
            pending_keysounds: Vec::new(),
            pending_keysound_volumes: Vec::new(),
            hsfix_index: 0,
            input_timestamp_anchor: None,
            pending_mine_hits: Vec::new(),
            state: PlayState::Ready,
            last_hcn_gauge_at: None,
        }
    }

    fn chart_with_keysound() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: Some(SoundId(7)),
            damage: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(note);

        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: vec![SoundAssetRef { id: SoundId(7), path: "sound.wav".into() }],
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(0),
        }
    }

    fn chart_with_bgm() -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: vec![SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(3) }],
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: vec![SoundAssetRef { id: SoundId(3), path: "bgm.wav".into() }],
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }

    fn judgement_event(judge: Judge, delta_us: i64) -> JudgementEvent {
        JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge,
            side: if delta_us < 0 { TimingSide::Fast } else { TimingSide::Slow },
            delta: TimeUs(delta_us),
            time: TimeUs(0),
            affects_score: true,
        }
    }
}
