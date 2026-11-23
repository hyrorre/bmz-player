//! LEGACY 判定が通常プレイの更新時刻に依存せず、リプレイと同じ結果になることの回帰テスト。
use std::sync::Arc;
use std::sync::Mutex;

use bmz_core::input::InputKind;
use bmz_core::lane::{KeyMode, Lane};
use bmz_core::replay::ReplayEvent;
use bmz_core::time::TimeUs;

use super::*;
use crate::input::backend::{
    DeviceId, DeviceInputEvent, DeviceTimestamp, InputBackend, PhysicalControl,
};
use crate::input::binding::{BindingEntry, LaneBinding};
use crate::judge::window::judge_windows_for_keymode_and_rule_mode;

/// テストから後で入力を足せる backend。取得時刻は更新した譜面時刻になる。
#[derive(Clone, Default)]
struct SharedBackend(Arc<Mutex<Vec<DeviceInputEvent>>>);

impl SharedBackend {
    fn press(&self) {
        self.push(InputKind::Press);
    }

    fn release(&self) {
        self.push(InputKind::Release);
    }

    fn push(&self, kind: InputKind) {
        self.0.lock().unwrap().push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind,
            timestamp: DeviceTimestamp::Unknown,
            bounce_policy: Default::default(),
        });
    }
}

impl InputBackend for SharedBackend {
    fn drain_events(&mut self) -> Vec<DeviceInputEvent> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

fn advance_at(session: &mut GameSession, audio: &mut TestAudio, now: i64) -> SessionFrame {
    session.audio_clock.pause_at(TimeUs(now));
    advance_session_frame(session, audio)
}

/// Key1 に 2 つのタップを持つ 7KEY 譜面。
fn two_tap_chart(first: TimeUs, second: TimeUs) -> PlayableChart {
    let mut chart = chart_with_keysound();
    let lane = Lane::Key1.index();
    chart.lane_notes[lane][0].time = first;
    let mut next = chart.lane_notes[lane][0].clone();
    next.id = NoteId(2);
    next.time = second;
    next.sound = Some(SoundId(8));
    chart.lane_notes[lane].push(next);
    chart.total_notes = 2;
    chart.end_time = second;
    chart.metadata.key_mode = KeyMode::K7;
    chart
}

fn legacy_session(chart: PlayableChart) -> GameSession {
    let mut session = session_with_autoplay(chart);
    let windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K7, RuleMode::Legacy);
    session.rule_mode = RuleMode::Legacy;
    session.primary_key_mode = KeyMode::K7;
    session.base_judge_windows = windows;
    session.base_judge_window = windows.note;
    session.judge = JudgeEngine::new_with_window_set(windows, RuleMode::Legacy)
        .with_chart_index(Arc::clone(&session.chart));
    session.autoplay = None;
    session
}

fn legacy_human_session(chart: PlayableChart) -> (GameSession, SharedBackend) {
    let mut session = legacy_session(chart);
    let backend = SharedBackend::default();
    session.input_system = InputSystem {
        backend: Box::new(backend.clone()),
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
        bounce_filter: Default::default(),
    };
    (session, backend)
}

/// (PGREAT, GREAT, GOOD, BAD, POOR, 空POOR)。
fn judge_counts(session: &GameSession) -> [u32; 6] {
    let judges = &session.score.judges;
    [
        judges.fast_pgreat + judges.slow_pgreat,
        judges.fast_great + judges.slow_great,
        judges.fast_good + judges.slow_good,
        judges.fast_bad + judges.slow_bad,
        judges.fast_poor + judges.slow_poor,
        judges.fast_empty_poor + judges.slow_empty_poor,
    ]
}

#[test]
fn legacy_frame_polls_lower_lane_miss_before_higher_lane_hit() {
    let mut chart = two_tap_chart(TimeUs(1_000_000), TimeUs(1_201_100));
    let mut second = chart.lane_notes[Lane::Key1.index()].pop().unwrap();
    second.lane = Lane::Key2;
    chart.lane_notes[Lane::Key2.index()].push(second);
    let mut session = legacy_session(chart);
    session.replay_player = Some(ReplayPlayer {
        events: vec![ReplayEvent {
            lane: Lane::Key2,
            kind: InputKind::Press,
            time: TimeUs(1_201_500),
            device_kind: Default::default(),
            scratch_direction: None,
        }],
        ..Default::default()
    });
    let frame = advance_at(&mut session, &mut TestAudio::default(), 1_202_000);
    assert_eq!(
        frame.judgements.iter().map(|e| e.judge).collect::<Vec<_>>(),
        vec![Judge::Poor, Judge::PGreat]
    );
    assert_eq!(session.score.ex_score(), 2);
}

#[test]
fn legacy_new_input_after_a_poll_in_the_same_millisecond_is_not_lost() {
    let (mut human, keys) =
        legacy_human_session(two_tap_chart(TimeUs(1_000_000), TimeUs(1_201_100)));
    let mut audio = TestAudio::default();
    advance_at(&mut human, &mut audio, 1_201_100);
    assert_eq!(judge_counts(&human), [0, 0, 0, 0, 1, 0]);
    keys.press();
    advance_at(&mut human, &mut audio, 1_201_500);
    assert_eq!(judge_counts(&human), [1, 0, 0, 0, 1, 0]);
}

/// LR2 polls integer-ms ticks. An expired head consumes the lane's press;
/// it cannot also hit the next note during that invocation.
#[test]
fn legacy_human_press_after_the_head_expired_matches_the_replay() {
    // A=1000ms は BAD 窓 200ms を過ぎた 1201.5ms で見逃し。
    // B=1201.1ms は同じ押下で PGREAT (±18ms 以内)。
    let first = TimeUs(1_000_000);
    let second = TimeUs(1_201_100);
    let press = 1_201_500;
    let release = press + 1_000;

    let (mut human, keys) = legacy_human_session(two_tap_chart(first, second));
    let mut audio = TestAudio::default();
    // 直前の更新では A はまだ期限切れではない (1000ms + 200ms = 1200ms)。
    advance_at(&mut human, &mut audio, 1_200_000);
    keys.press();
    advance_at(&mut human, &mut audio, press);
    keys.release();
    advance_at(&mut human, &mut audio, release);

    assert_eq!(judge_counts(&human), [0, 0, 0, 0, 1, 0]);

    let mut replay = legacy_session(two_tap_chart(first, second));
    replay.replay_player = Some(ReplayPlayer {
        events: vec![
            ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(press),
                device_kind: Default::default(),
                scratch_direction: None,
            },
            ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Release,
                time: TimeUs(release),
                device_kind: Default::default(),
                scratch_direction: None,
            },
        ],
        ..Default::default()
    });
    replay.replay_judge_rate_percent = 100;
    advance_at(&mut replay, &mut audio, release);

    assert_eq!(judge_counts(&replay), judge_counts(&human));
}

/// Key2 に押されないノート、Key1 に押すノートを持つ 7KEY 譜面。
fn missed_and_pressed_chart(missed: TimeUs, pressed: TimeUs) -> PlayableChart {
    let mut chart = chart_with_keysound();
    let base = chart.lane_notes[Lane::Key1.index()][0].clone();
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    chart.lane_notes[Lane::Key2.index()].push(NoteEvent {
        lane: Lane::Key2,
        time: missed,
        ..base.clone()
    });
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key1,
        time: pressed,
        sound: Some(SoundId(8)),
        ..base
    });
    chart.total_notes = 2;
    chart.end_time = pressed;
    chart.metadata.key_mode = KeyMode::K7;
    chart
}

/// LR2 polls every lane before checking failure. A Key1 hit and a Key2
/// miss in the same integer-ms poll are both scored before HAZARD closes.
#[test]
fn legacy_hazard_checks_failure_after_the_lane_poll_in_live_play_and_replay() {
    let missed = TimeUs(1_000_000);
    let pressed = TimeUs(1_201_100);
    let press = 1_201_500;
    let release = press + 1_000;

    let (mut human, keys) = legacy_human_session(missed_and_pressed_chart(missed, pressed));
    let total_notes = human.chart.total_notes;
    human.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hazard, 160.0, total_notes);
    let mut audio = TestAudio::default();
    // 直前の更新では Key2 のノートはまだ期限切れではない (1000ms + 200ms)。
    advance_at(&mut human, &mut audio, 1_200_000);
    keys.press();
    advance_at(&mut human, &mut audio, press);
    assert_eq!(judge_counts(&human), [1, 0, 0, 0, 1, 0]);
    assert_eq!(human.score.ex_score(), 2);
    assert_eq!(human.state, PlayState::Failed);

    let mut replay = legacy_session(missed_and_pressed_chart(missed, pressed));
    let total_notes = replay.chart.total_notes;
    replay.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hazard, 160.0, total_notes);
    replay.replay_player = Some(ReplayPlayer {
        events: vec![
            ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(press),
                device_kind: Default::default(),
                scratch_direction: None,
            },
            ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Release,
                time: TimeUs(release),
                device_kind: Default::default(),
                scratch_direction: None,
            },
        ],
        ..Default::default()
    });
    replay.replay_judge_rate_percent = 100;
    advance_at(&mut replay, &mut audio, release);

    assert_eq!(judge_counts(&replay), judge_counts(&human));
    assert_eq!(replay.state, human.state);
}

#[test]
fn legacy_hazard_keeps_the_same_press_bad_chain_in_live_play_and_replay() {
    let chart = two_tap_chart(TimeUs(1_000_000), TimeUs(1_150_000));
    let (mut human, keys) = legacy_human_session(chart.clone());
    human.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hazard, 160.0, 2);
    let mut audio = TestAudio::default();
    advance_at(&mut human, &mut audio, 1_000_000);
    keys.press();
    advance_at(&mut human, &mut audio, 1_150_000);
    assert_eq!(judge_counts(&human), [1, 0, 0, 1, 0, 0]);
    assert_eq!(human.score.ex_score(), 2);
    assert_eq!(human.state, PlayState::Failed);

    let mut replay = legacy_session(chart);
    replay.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hazard, 160.0, 2);
    replay.replay_player = Some(ReplayPlayer {
        events: vec![ReplayEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(1_150_000),
            device_kind: Default::default(),
            scratch_direction: None,
        }],
        ..Default::default()
    });
    replay.replay_judge_rate_percent = 100;
    advance_at(&mut replay, &mut audio, 1_200_000);
    assert_eq!(judge_counts(&replay), judge_counts(&human));
    assert_eq!(replay.score.ex_score(), human.score.ex_score());
    assert_eq!(replay.state, human.state);
}

fn legacy_hln_chart() -> PlayableChart {
    let mut chart = chart_with_hcn_long_note();
    chart.long_notes[0].mode = Some(LongNoteMode::Hln);
    chart
}

/// HLN 始点は LEGACY の汎用見逃し (GOOD 幅) では確定せず、`advance_hln()` の
/// `bad_slow_us + 1µs` での保持状態初期化に任せる。
#[test]
fn legacy_hln_head_is_settled_by_advance_hln_at_bad_slow_plus_one() {
    let mut session = legacy_session(legacy_hln_chart());
    session.state = PlayState::Playing;
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, 2);

    // GOOD 幅 (100ms) を過ぎただけでは汎用見逃しは始点を確定しない。
    process_misses(&mut session, TimeUs(101_000));
    assert_eq!(judge_counts(&session)[4], 0);

    // bad_slow_us + 1µs = 200.001ms で advance_hln() が始点を POOR にする。
    process_misses(&mut session, TimeUs(200_001));
    assert_eq!(judge_counts(&session)[4], 1);
}

/// NO GOOD / NO GREAT はタップの GOOD / GREAT 帯を潰すが、LN の始点見逃し
/// 期限と終端早離し許容は残っている帯の最大幅を使う。
#[test]
fn legacy_no_good_and_no_great_keep_the_long_note_margin() {
    fn run(rates: [u16; 3], head: i64) -> [u32; 6] {
        let (mut session, keys) =
            legacy_human_session(ln_chart_with_start_sound_and_end_sound(None));
        session.judge.window_rates.keys = rates;
        let mut audio = TestAudio::default();
        advance_at(&mut session, &mut audio, 0);
        keys.press();
        advance_at(&mut session, &mut audio, head);
        keys.release();
        advance_at(&mut session, &mut audio, 990_000);
        judge_counts(&session)
    }

    // NO GOOD: GREAT 幅 40ms が残るので、+30ms の始点と -10ms の終端は GREAT。
    assert_eq!(run([100, 100, 0], 30_000), [0, 1, 0, 0, 0, 0]);
    // NO GREAT: PGREAT 幅 18ms が残るので、+10ms / -10ms は PGREAT。
    assert_eq!(run([100, 0, 0], 10_000), [1, 0, 0, 0, 0, 0]);
}
