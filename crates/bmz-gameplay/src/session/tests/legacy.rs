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

/// 通常プレイは「入力 → 見逃し」の順に更新する。押下の直前の更新で先頭ノートが
/// まだ期限切れでなくても、押下時刻には BAD 窓を過ぎている。OpenLR2 と同じく
/// 同じ押下で後続ノートを判定できること、そしてリプレイと結果が一致することを見る。
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

    assert_eq!(judge_counts(&human), [1, 0, 0, 0, 1, 0]);

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
