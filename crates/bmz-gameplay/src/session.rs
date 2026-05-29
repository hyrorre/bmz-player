use std::sync::Arc;

use bmz_audio::clock::AudioClock;
use bmz_audio::queue::{AudioScheduler, ScheduledSound};
use bmz_chart::model::{LongNoteMode, PlayableChart};
use bmz_chart::timing::TimingMap;
use bmz_core::input::{InputEvent, InputKind, InputSource};
use bmz_core::judge::Judge;
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use crate::autoplay::AutoplayController;
use crate::gauge::GaugeState;
use crate::input::system::InputSystem;
use crate::input::translator::{InputTimestampAnchor, InputTimingContext};
use crate::judge::engine::JudgeEngine;
use crate::judge::model::{JudgeOutcome, JudgeWindow, JudgementEvent, MineHitEvent};
use crate::judge::window::{judge_percent_at_time, judge_window_for_rank};
use crate::replay::{ReplayPlayer, ReplayRecorder};
use crate::score::ScoreState;

pub const AUDIO_SCHEDULE_AHEAD_US: i64 = 100_000;
pub const SESSION_END_MARGIN_US: i64 = 500_000;
pub const JUDGEMENT_DISPLAY_US: i64 = 800_000;
pub const INPUT_DISPLAY_US: i64 = 160_000;
/// オートプレイ時のキー押下を「離す」までの時間。beatoraja の `auto_minduration` (80ms) と揃える。
/// この時間が経過すると lane_keyon → lane_keyoff へ遷移し、skin の KEYOFF タイマー演出が走る。
pub const AUTO_KEYBEAM_DURATION_US: i64 = 80_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Ready,
    Playing,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayOffsets {
    pub input_offset_us: i64,
    pub visual_offset_us: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayAudioMix {
    pub master_volume: f32,
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
    pub render_now: TimeUs,
    pub audio_schedule_until: TimeUs,
}

#[derive(Debug, Clone, Default)]
pub struct BgmScheduler {
    pub next_index: usize,
}

pub struct GameSession {
    pub chart: Arc<PlayableChart>,
    /// `chart` の BPM 変化と STOP を取り込んだ tick<->time マップ。
    /// スクロール位置を BPM に追従させるために使う。
    pub timing_map: TimingMap,
    pub audio_clock: AudioClock,
    pub input_system: InputSystem,
    pub judge: JudgeEngine,
    /// プロファイル既定の判定窓。`#RANK` / `#EXRANK` 倍率の基準値。
    pub base_judge_window: JudgeWindow,
    pub score: ScoreState,
    pub gauge: GaugeState,
    pub replay_recorder: ReplayRecorder,
    pub replay_player: Option<ReplayPlayer>,
    pub autoplay: Option<AutoplayController>,
    pub recent_inputs: Vec<InputEvent>,
    /// 各レーンのキー押下開始時刻。押下中のみ Some。skin の keyon タイマー(100..=107)。
    pub lane_keyon_started_at: [Option<TimeUs>; LANE_COUNT],
    /// 各レーンのキー解放時刻。離した直後のみ Some(次の Press でクリア)。skin の keyoff タイマー(120..=127)。
    pub lane_keyoff_started_at: [Option<TimeUs>; LANE_COUNT],
    /// オートプレイで Press したレーンの自動 Release 予定時刻。
    /// `audio_now` がこの時刻を超えたら keyon → keyoff へ遷移する。
    pub lane_auto_release_at: [Option<TimeUs>; LANE_COUNT],
    pub recent_judgements: Vec<JudgementEvent>,
    /// Full combo animation start time. Set once when all notes have been judged
    /// and the combo still matches the total note count.
    pub full_combo_started_at: Option<TimeUs>,
    pub bgm_scheduler: BgmScheduler,
    pub offsets: PlayOffsets,
    pub audio_mix: PlayAudioMix,
    pub hispeed: f32,
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
    /// Start/Select 押下中など、レーンカバー数値表示を出す状態。
    pub lane_cover_changing: bool,
    pub hidden_cover: f32,
    pub skin_offsets: Vec<PlaySkinOffset>,
    pub bga_enabled: bool,
    pub poor_bga_duration_us: i64,
    pub bga_stretch: i32,
    pub input_timestamp_anchor: Option<InputTimestampAnchor>,
    /// 当該フレーム中に発火した Mine ヒット。`advance_session_frame` の終端で
    /// `SessionFrame.mine_hits` に吸い出される（app 層が地雷 SE を鳴らす）。
    pub pending_mine_hits: Vec<MineHitEvent>,
    pub state: PlayState,
    /// HCN ゲージ増減の前回更新時刻。
    pub last_hcn_gauge_at: Option<TimeUs>,
}

#[derive(Debug, Clone)]
pub struct FrameOutput<TSnapshot> {
    pub render_snapshot: TSnapshot,
    /// 当該フレームで踏んだ Mine ノーツ。app 層が地雷 SE を鳴らすのに使う。
    pub mine_hits: Vec<MineHitEvent>,
    pub state: PlayState,
}

#[derive(Debug, Clone)]
pub struct SessionFrame {
    pub times: FrameTimes,
    pub judgements: Vec<JudgementEvent>,
    /// 当該フレームで踏んだ Mine ノーツ。地雷 SE 再生など、UI / audio 側の
    /// 副作用処理に使う。`apply_judge_outcome` の中ですでにゲージは削っている。
    pub mine_hits: Vec<MineHitEvent>,
    pub state: PlayState,
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
    let render_now = TimeUs(audio_now.0 + session.offsets.visual_offset_us);
    let audio_schedule_until = TimeUs(audio_now.0 + AUDIO_SCHEDULE_AHEAD_US);
    FrameTimes { audio_now, render_now, audio_schedule_until }
}

pub fn apply_judge_outcome(
    session: &mut GameSession,
    outcome: JudgeOutcome,
) -> Vec<JudgementEvent> {
    let mut events = Vec::with_capacity(outcome.events.len());
    for event in outcome.events {
        session.score.apply(&event);
        session.gauge.apply_judge(event.judge, 1.0);
        events.push(event);
    }
    for hit in outcome.mine_hits {
        // Mine はスコア/コンボに影響を与えず、ゲージのみ削る。SE 再生 (= app 層の
        // 副作用) は `pending_mine_hits` に積んでフレーム終端で吸い出す。
        session.gauge.apply_mine(hit.damage);
        session.pending_mine_hits.push(hit);
    }
    events
}

pub fn schedule_keysounds(
    session: &GameSession,
    judgements: &[JudgementEvent],
    audio: &mut dyn AudioScheduler,
) {
    for event in judgements {
        if !plays_keysound(event.judge) {
            continue;
        }
        let Some(note_id) = event.note_id else {
            continue;
        };
        let Some(sound_id) = session.chart.note_by_id(note_id).and_then(|note| note.sound) else {
            continue;
        };

        let chart_volume = bmz_chart::volume::chart_channel_volume_factor(
            bmz_chart::volume::chart_volume_at_time(&session.chart.key_volume_events, event.time),
        );
        audio.schedule(ScheduledSound {
            start_frame: session.audio_clock.time_to_output_frame(event.time),
            sound_id,
            volume: (session.audio_mix.master_volume * session.audio_mix.key_volume * chart_volume)
                .clamp(0.0, 1.0),
            pan: 0.0,
            loop_playback: false,
        });
    }
}

fn plays_keysound(judge: Judge) -> bool {
    matches!(judge, Judge::PGreat | Judge::Great | Judge::Good | Judge::Bad)
}

pub fn update_recent_judgements(session: &mut GameSession, events: &[JudgementEvent], now: TimeUs) {
    session.recent_judgements.extend(events.iter().cloned());
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
                session.lane_auto_release_at[lane_index] = None;
            }
        }
    }
}

/// オートプレイで Press したレーンを `AUTO_KEYBEAM_DURATION_US` 経過後に自動で release する。
/// beatoraja の `auto_minduration` (80ms) 経過で `auto_presstime` を MIN_VALUE に戻す挙動に対応。
pub fn apply_auto_key_release(session: &mut GameSession, audio_now: TimeUs) {
    for lane_index in 0..LANE_COUNT {
        if let Some(release_at) = session.lane_auto_release_at[lane_index]
            && audio_now.0 >= release_at.0
        {
            session.lane_keyoff_started_at[lane_index] = Some(release_at);
            session.lane_keyon_started_at[lane_index] = None;
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
    let inputs = session.input_system.collect_game_inputs(&ctx);
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);
    let mut judgements = Vec::new();
    for input in inputs {
        session.replay_recorder.record(input);
        let outcome = session.judge.process_input(&session.chart, input);
        judgements.extend(apply_judge_outcome(session, outcome));
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
        let outcome = session.judge.process_input(&session.chart, input);
        judgements.extend(apply_judge_outcome(session, outcome));
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
        let outcome = session.judge.process_input(&session.chart, input);
        judgements.extend(apply_judge_outcome(session, outcome));
    }
    judgements
}

pub fn process_misses(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let outcome = session.judge.process_misses(&session.chart, audio_now);
    apply_judge_outcome(session, outcome)
}

pub fn apply_hcn_gauge(session: &mut GameSession, audio_now: TimeUs) {
    if session.chart.metadata.long_note_mode != LongNoteMode::Hcn {
        session.last_hcn_gauge_at = None;
        return;
    }

    let delta_us =
        session.last_hcn_gauge_at.map(|prev| audio_now.0.saturating_sub(prev.0)).unwrap_or(0);
    session.last_hcn_gauge_at = Some(audio_now);
    if delta_us <= 0 {
        return;
    }
    let delta_secs = delta_us as f32 / 1_000_000.0;

    for lane in Lane::ALL {
        let idx = lane.index();
        let lane_state = &session.judge.lanes[idx];
        if lane_state.active_long.is_some() && session.lane_keyon_started_at[idx].is_some() {
            session.gauge.apply_hcn_hold(delta_secs);
        } else if lane_state.hcn_draining {
            session.gauge.apply_hcn_drain(delta_secs);
        }
    }
}

pub fn sync_judge_windows(session: &mut GameSession, now: TimeUs) {
    let percent = judge_percent_at_time(
        session.chart.metadata.judge_rank,
        &session.chart.judge_rank_events,
        now,
    );
    session.judge.windows = judge_window_for_rank(session.base_judge_window, percent);
}

pub fn advance_session_frame(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
) -> SessionFrame {
    if session.state == PlayState::Ready {
        session.state = PlayState::Playing;
    }

    let times = compute_frame_times(session);
    let mut judgements = Vec::new();

    if session.state == PlayState::Playing {
        sync_judge_windows(session, times.audio_now);

        session.bgm_scheduler.schedule_until(
            &session.chart,
            &session.audio_clock,
            times.audio_schedule_until,
            session.audio_mix.master_volume * session.audio_mix.bgm_volume,
            audio,
        );

        if session.replay_player.is_some() {
            drain_human_inputs(session);
            judgements.extend(process_replay_inputs(session, times.audio_now));
        } else if session.autoplay.is_some() {
            // オートプレイ中は人間のキー入力を判定にも視覚エフェクトにも渡さない。
            // キービームは process_autoplay_inputs 側(ノーツ処理時)で発火する。
            // (ハイスピード等のオプション操作は app 側で別途処理される)
            discard_human_inputs(session);
            judgements.extend(process_autoplay_inputs(session, times.audio_now));
            apply_auto_key_release(session, times.audio_now);
        } else {
            judgements.extend(process_human_inputs(session));
        }
        judgements.extend(process_misses(session, times.audio_now));
        apply_hcn_gauge(session, times.audio_now);
        schedule_keysounds(session, &judgements, audio);
        update_recent_judgements(session, &judgements, times.render_now);
        update_full_combo_timer(session, &judgements);

        if should_finish(session, times.audio_now) {
            session.state = PlayState::Finished;
        }
    }

    let mine_hits = std::mem::take(&mut session.pending_mine_hits);
    SessionFrame { times, judgements, mine_hits, state: session.state }
}

fn update_full_combo_timer(session: &mut GameSession, judgements: &[JudgementEvent]) {
    if session.full_combo_started_at.is_some()
        || session.chart.total_notes == 0
        || session.score.past_notes < session.chart.total_notes
        || session.score.combo < session.chart.total_notes
    {
        return;
    }
    session.full_combo_started_at = judgements
        .iter()
        .rev()
        .find(|event| event.note_id.is_some())
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
    use bmz_core::input::InputSource;
    use bmz_core::judge::TimingSide;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use crate::input::backend::NullInputBackend;
    use crate::input::binding::LaneBinding;
    use crate::input::system::InputSystem;
    use crate::input::translator::DefaultInputTranslator;
    use crate::judge::model::JudgeWindow;

    use super::*;

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
        let mut audio = TestAudio::default();

        let frame = advance_session_frame(&mut session, &mut audio);

        assert_eq!(frame.judgements.len(), 1);
        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
        assert_eq!(audio.scheduled[0].start_frame, 0);
        assert_eq!(audio.scheduled[0].volume, 0.125);
        assert_eq!(session.recent_judgements.len(), 1);
    }

    #[test]
    fn advance_session_frame_starts_full_combo_timer_after_last_note() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        assert_eq!(session.full_combo_started_at, Some(TimeUs(0)));
    }

    #[test]
    fn advance_session_frame_schedules_bgm_with_mix_volume() {
        let mut session = session_with_autoplay(chart_with_bgm());
        session.audio_mix.master_volume = 0.5;
        session.audio_mix.bgm_volume = 0.75;
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(3));
        assert_eq!(audio.scheduled[0].volume, 0.375);
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
        }];
        update_lane_key_states(&mut session, &inputs);

        assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(5_000)));
        assert_eq!(session.lane_keyoff_started_at[Lane::Key1.index()], None);
        // Human source の Press は自動 release を予約しない (押し続け対応)。
        assert_eq!(session.lane_auto_release_at[Lane::Key1.index()], None);
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
            },
            InputEvent {
                lane: Lane::Key2,
                kind: InputKind::Release,
                time: TimeUs(20_000),
                source: InputSource::Human,
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
            judge: JudgeEngine::new(JudgeWindow {
                pgreat_us: 16_000,
                great_us: 40_000,
                good_us: 80_000,
                bad_us: 120_000,
                empty_poor_fast_us: 500_000,
                empty_poor_slow_us: 200_000,
                mine_hit_us: 16_000,
            }),
            base_judge_window: JudgeWindow {
                pgreat_us: 16_000,
                great_us: 40_000,
                good_us: 80_000,
                bad_us: 120_000,
                empty_poor_fast_us: 500_000,
                empty_poor_slow_us: 200_000,
                mine_hit_us: 16_000,
            },
            score: ScoreState::default(),
            gauge: GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, chart.total_notes),
            replay_recorder: ReplayRecorder::default(),
            replay_player: None,
            autoplay: Some(AutoplayController::default()),
            recent_inputs: Vec::new(),
            lane_keyon_started_at: Default::default(),
            lane_keyoff_started_at: Default::default(),
            lane_auto_release_at: Default::default(),
            recent_judgements: Vec::new(),
            full_combo_started_at: None,
            bgm_scheduler: BgmScheduler::default(),
            offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
            audio_mix: PlayAudioMix { master_volume: 1.0, key_volume: 1.0, bgm_volume: 1.0 },
            hispeed: 2.0,
            lift: 0.0,
            lane_cover: 0.0,
            lane_cover_visible: true,
            lane_cover_changing: false,
            lanecover_enabled: false,
            lift_enabled: true,
            hidden_enabled: false,
            hidden_cover: 0.0,
            skin_offsets: Vec::new(),
            bga_enabled: true,
            poor_bga_duration_us: 500_000,
            bga_stretch: 1,
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
            bar_lines: Vec::new(),
            sounds: vec![SoundAssetRef { id: SoundId(3), path: "bgm.wav".into() }],
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }
}
