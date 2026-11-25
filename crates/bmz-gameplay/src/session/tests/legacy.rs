//! LEGACY 判定が通常プレイの更新時刻に依存せず、リプレイと同じ結果になることの回帰テスト。
use std::sync::Arc;
use std::sync::Mutex;

use bmz_core::input::{InputKind, ScratchDirection};
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
        self.push_key("Z", kind);
    }

    fn push_key(&self, key: &str, kind: InputKind) {
        self.0.lock().unwrap().push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey(key.to_string()),
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
    legacy_human_session_with(chart, &[("Z", Lane::Key1, None)])
}

/// A LEGACY session played from `keys`: (keyboard key, lane, scratch direction).
fn legacy_human_session_with(
    chart: PlayableChart,
    keys: &[(&str, Lane, Option<ScratchDirection>)],
) -> (GameSession, SharedBackend) {
    let mut session = legacy_session(chart);
    let backend = SharedBackend::default();
    session.input_system = InputSystem {
        backend: Box::new(backend.clone()),
        translator: Box::new(DefaultInputTranslator {
            binding: LaneBinding {
                entries: keys
                    .iter()
                    .map(|&(key, lane, scratch_direction)| BindingEntry {
                        device: None,
                        control: PhysicalControl::KeyboardKey(key.to_string()),
                        lane,
                        scratch_direction,
                    })
                    .collect(),
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
            applied_after: None,
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
                applied_after: None,
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(press),
                device_kind: Default::default(),
                scratch_direction: None,
            },
            ReplayEvent {
                applied_after: None,
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
                applied_after: None,
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(press),
                device_kind: Default::default(),
                scratch_direction: None,
            },
            ReplayEvent {
                applied_after: None,
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
            applied_after: None,
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

fn replay_event(
    lane: Lane,
    kind: InputKind,
    ms: i64,
    direction: Option<ScratchDirection>,
) -> ReplayEvent {
    ReplayEvent {
        applied_after: None,
        lane,
        kind,
        time: TimeUs(ms * 1_000),
        device_kind: Default::default(),
        scratch_direction: direction,
    }
}

fn legacy_replay_session(chart: PlayableChart, events: Vec<ReplayEvent>) -> GameSession {
    let mut session = legacy_session(chart);
    session.replay_player = Some(ReplayPlayer { events, ..Default::default() });
    session.replay_judge_rate_percent = 100;
    session
}

/// Standard LN at 1000–2000 ms on `lane`.
fn legacy_ln_chart(lane: Lane) -> PlayableChart {
    let mut chart = chart_with_hcn_long_note();
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.metadata.key_mode = KeyMode::K7;
    let mut notes = std::mem::take(&mut chart.lane_notes[Lane::Key1.index()]);
    for note in &mut notes {
        note.lane = lane;
        note.time = TimeUs(note.time.0 + 1_000_000);
    }
    chart.lane_notes[lane.index()] = notes;
    let pair = &mut chart.long_notes[0];
    pair.mode = Some(LongNoteMode::Ln);
    pair.lane = lane;
    pair.start_time = TimeUs(1_000_000);
    pair.end_time = TimeUs(2_000_000);
    chart.total_notes = 1;
    chart.end_time = TimeUs(2_000_000);
    chart
}

/// OpenLR2 merges both scratch directions into one key: releasing the first
/// direction while the second is held keeps the LN alive.
#[test]
fn legacy_scratch_release_while_the_other_direction_is_held_keeps_the_ln() {
    let up = Some(ScratchDirection::Up);
    let down = Some(ScratchDirection::Down);
    let mut session = legacy_replay_session(
        legacy_ln_chart(Lane::Scratch),
        vec![
            replay_event(Lane::Scratch, InputKind::Press, 1_000, up),
            replay_event(Lane::Scratch, InputKind::Press, 1_010, down),
            replay_event(Lane::Scratch, InputKind::Release, 1_100, up),
            replay_event(Lane::Scratch, InputKind::Release, 2_100, down),
        ],
    );
    advance_at(&mut session, &mut TestAudio::default(), 2_200_000);
    assert_eq!(judge_counts(&session), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn legacy_scratch_release_of_the_last_direction_is_still_an_early_release() {
    let up = Some(ScratchDirection::Up);
    let mut session = legacy_replay_session(
        legacy_ln_chart(Lane::Scratch),
        vec![
            replay_event(Lane::Scratch, InputKind::Press, 1_000, up),
            replay_event(Lane::Scratch, InputKind::Release, 1_100, up),
        ],
    );
    advance_at(&mut session, &mut TestAudio::default(), 2_200_000);
    assert_eq!(judge_counts(&session), [0, 0, 0, 1, 0, 0]);
}

/// Mine at 1000 ms (damage 10) and a tap at 2000 ms on Key1.
fn legacy_mine_chart() -> PlayableChart {
    let mut chart = chart_with_mine(TimeUs(1_000_000), 10.0);
    let mut tap = chart_with_keysound().lane_notes[Lane::Key1.index()][0].clone();
    tap.time = TimeUs(2_000_000);
    chart.lane_notes[Lane::Key1.index()].push(tap);
    chart.total_notes = 1;
    chart.end_time = TimeUs(2_000_000);
    chart.metadata.key_mode = KeyMode::K7;
    chart
}

/// `ApplyJudgeMine` counts a hit mine as POOR (BP) without breaking the
/// combo, and subtracts the damage from the gauge.
#[test]
fn legacy_mine_hit_counts_as_poor_and_takes_the_gauge_damage() {
    let mut session = legacy_replay_session(
        legacy_mine_chart(),
        vec![
            replay_event(Lane::Key1, InputKind::Press, 990, None),
            replay_event(Lane::Key1, InputKind::Release, 995, None),
            replay_event(Lane::Key1, InputKind::Press, 2_000, None),
            replay_event(Lane::Key1, InputKind::Release, 2_010, None),
        ],
    );
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 1);
    let before = session.gauge.current().value;
    let mut audio = TestAudio::default();
    advance_at(&mut session, &mut audio, 1_500_000);
    assert_eq!(judge_counts(&session), [0, 0, 0, 0, 1, 0]);
    assert_eq!(session.score.bp(), 1);
    // OpenLR2 keeps a mine out of the combo breaks (extendedStats.cb).
    assert_eq!(session.score.cb(), 0);
    assert!((before - session.gauge.current().value - 10.0).abs() < 1e-4);

    advance_at(&mut session, &mut audio, 2_100_000);
    assert_eq!(judge_counts(&session), [1, 0, 0, 0, 1, 0]);
    assert_eq!(session.score.combo, 1, "the mine did not break the combo");
    assert_eq!(session.score.past_notes, 1);
}

#[test]
fn legacy_key_held_across_a_mine_is_hit_once() {
    let mut session = legacy_replay_session(
        legacy_mine_chart(),
        vec![
            replay_event(Lane::Key1, InputKind::Press, 500, None),
            replay_event(Lane::Key1, InputKind::Release, 1_500, None),
        ],
    );
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 1);
    advance_at(&mut session, &mut TestAudio::default(), 1_600_000);
    assert_eq!(judge_counts(&session), [0, 0, 0, 0, 1, 0]);
    assert!((session.gauge.current().value - 90.0).abs() < 1e-4);
}

/// A press after the poll of its own millisecond is polled again at that
/// millisecond. The replay records the boundary, so it judges the same way.
#[test]
fn legacy_same_millisecond_input_after_a_poll_replays_the_same_judgements() {
    let chart = two_tap_chart(TimeUs(1_000_000), TimeUs(1_201_100));
    let (mut human, keys) = legacy_human_session(chart.clone());
    let mut audio = TestAudio::default();
    advance_at(&mut human, &mut audio, 1_201_100);
    keys.press();
    advance_at(&mut human, &mut audio, 1_201_500);
    keys.release();
    advance_at(&mut human, &mut audio, 1_210_000);
    assert_eq!(judge_counts(&human), [1, 0, 0, 0, 1, 0]);
    let recorded = human.replay_recorder.events.clone();
    assert!(recorded[0].applied_after.is_some(), "{recorded:?}");

    let mut replay = legacy_replay_session(chart, recorded);
    advance_at(&mut replay, &mut TestAudio::default(), 1_210_000);
    assert_eq!(judge_counts(&replay), judge_counts(&human));
}

/// Releasing the direction that started a scratch LN while the other is held,
/// then the other: the merged key is released at the second one, early.
#[test]
fn legacy_scratch_release_of_the_other_direction_ends_the_ln_early() {
    let up = Some(ScratchDirection::Up);
    let down = Some(ScratchDirection::Down);
    let mut session = legacy_replay_session(
        legacy_ln_chart(Lane::Scratch),
        vec![
            replay_event(Lane::Scratch, InputKind::Press, 1_000, up),
            replay_event(Lane::Scratch, InputKind::Press, 1_010, down),
            replay_event(Lane::Scratch, InputKind::Release, 1_100, up),
            replay_event(Lane::Scratch, InputKind::Release, 1_500, down),
        ],
    );
    advance_at(&mut session, &mut TestAudio::default(), 2_200_000);
    assert_eq!(judge_counts(&session), [0, 0, 0, 1, 0, 0]);
}

/// In one poll a new press of either direction wins over a release, so
/// switching directions in the same millisecond keeps the LN. The press
/// re-evaluates the held head as OpenLR2's `ProcLongnote` does; 500 ms after
/// it, that is an EMPTY POOR, and the LN is scored at its end.
#[test]
fn legacy_scratch_release_and_press_in_one_poll_is_a_press() {
    let up = Some(ScratchDirection::Up);
    let down = Some(ScratchDirection::Down);
    let mut session = legacy_replay_session(
        legacy_ln_chart(Lane::Scratch),
        vec![
            replay_event(Lane::Scratch, InputKind::Press, 1_000, up),
            replay_event(Lane::Scratch, InputKind::Release, 1_500, up),
            replay_event(Lane::Scratch, InputKind::Press, 1_500, down),
            replay_event(Lane::Scratch, InputKind::Release, 2_100, down),
        ],
    );
    advance_at(&mut session, &mut TestAudio::default(), 2_200_000);
    assert_eq!(judge_counts(&session), [1, 0, 0, 0, 0, 1]);
}

/// A lane of only mines judges its free slots, which can reach LR2's note
/// count. The later single note is never judged then, and the song still ends.
#[test]
fn legacy_session_ends_when_the_note_count_leaves_a_note_unjudged() {
    let mut chart = two_tap_chart(TimeUs(2_000_000), TimeUs(4_000_000));
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    let mut tap = notes.pop().unwrap();
    notes[0].kind = NoteKind::Mine;
    notes[0].damage = Some(10.0);
    tap.lane = Lane::Key2;
    chart.lane_notes[Lane::Key2.index()].push(tap);
    chart.total_notes = 1;
    let mut session = legacy_replay_session(chart, Vec::new());
    let mut audio = TestAudio::default();
    for ms in (0..=12_000).step_by(100) {
        advance_at(&mut session, &mut audio, ms * 1_000);
    }
    assert_eq!(judge_counts(&session), [0, 0, 0, 0, 1, 0]);
    assert_eq!(session.state, PlayState::Finished);
}

/// Replays `recorded` with an update at each of `updates` (µs).
fn replay_counts_with_updates(
    chart: PlayableChart,
    recorded: Vec<ReplayEvent>,
    updates: &[i64],
) -> [u32; 6] {
    let mut replay = legacy_replay_session(chart, recorded);
    let mut audio = TestAudio::default();
    for &now in updates {
        advance_at(&mut replay, &mut audio, now);
    }
    judge_counts(&replay)
}

/// The replay polls a millisecond only once all its recorded edges are due,
/// so an update inside the millisecond of a recorded press changes nothing.
#[test]
fn legacy_replay_split_inside_the_millisecond_of_a_press_judges_the_same() {
    let chart = two_tap_chart(TimeUs(1_000_000), TimeUs(1_201_100));
    let (mut human, keys) = legacy_human_session(chart.clone());
    let mut audio = TestAudio::default();
    advance_at(&mut human, &mut audio, 1_200_000);
    keys.press();
    advance_at(&mut human, &mut audio, 1_201_500);
    keys.release();
    advance_at(&mut human, &mut audio, 1_500_000);
    // The press at 1201 ms consumes the passed first note; the second passes.
    assert_eq!(judge_counts(&human), [0, 0, 0, 0, 2, 0]);
    let recorded = human.replay_recorder.events.clone();
    assert_eq!(recorded[0].applied_after, None, "{recorded:?}");

    for updates in [
        &[1_500_000][..],
        &[1_201_100, 1_201_600, 1_500_000],
        &[1_200_000, 1_201_000, 1_201_499, 1_201_500, 1_201_999, 1_202_000, 1_500_000],
    ] {
        assert_eq!(
            replay_counts_with_updates(chart.clone(), recorded.clone(), updates),
            judge_counts(&human),
            "{updates:?}"
        );
    }
}

/// Two updates after the poll of one millisecond are two more polls of it.
/// The replay keeps them apart, so a release and a press of the other
/// scratch direction are not merged into one poll.
#[test]
fn legacy_replay_keeps_each_repoll_of_a_millisecond_apart() {
    let chart = legacy_ln_chart(Lane::Scratch);
    let (mut human, keys) = legacy_human_session_with(
        chart.clone(),
        &[
            ("A", Lane::Scratch, Some(ScratchDirection::Up)),
            ("S", Lane::Scratch, Some(ScratchDirection::Down)),
        ],
    );
    let mut audio = TestAudio::default();
    advance_at(&mut human, &mut audio, 999_000);
    keys.push_key("A", InputKind::Press);
    advance_at(&mut human, &mut audio, 1_000_000);
    advance_at(&mut human, &mut audio, 1_500_000);
    keys.push_key("A", InputKind::Release);
    advance_at(&mut human, &mut audio, 1_500_100);
    keys.push_key("S", InputKind::Press);
    advance_at(&mut human, &mut audio, 1_500_500);
    keys.push_key("S", InputKind::Release);
    advance_at(&mut human, &mut audio, 2_200_000);
    // The release is a poll of its own: an early release, BAD.
    assert_eq!(judge_counts(&human), [0, 0, 0, 1, 0, 0]);
    let recorded = human.replay_recorder.events.clone();
    assert_ne!(recorded[1].applied_after, recorded[2].applied_after, "{recorded:?}");

    for updates in [&[2_200_000][..], &[1_500_000, 1_500_300, 1_500_999, 2_200_000]] {
        assert_eq!(
            replay_counts_with_updates(chart.clone(), recorded.clone(), updates),
            judge_counts(&human),
            "{updates:?}"
        );
    }
}
