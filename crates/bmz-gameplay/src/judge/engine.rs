use std::collections::HashMap;

use bmz_chart::model::{LongNoteMode, NoteEvent, NoteKind, PlayableChart};
use bmz_core::ids::NoteId;
use bmz_core::input::{InputEvent, InputKind};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use super::model::{
    ActiveLongNote, JudgeOutcome, JudgeWindow, JudgeWindows, JudgementEvent, LaneJudgeState,
    LongNoteEndRef, MineHitEvent,
};
use crate::rule::RuleMode;

#[derive(Debug, Clone)]
pub struct JudgeEngine {
    pub windows: JudgeWindow,
    pub window_set: JudgeWindows,
    pub rule_mode: RuleMode,
    pub lanes: [LaneJudgeState; LANE_COUNT],
    pub judged_notes: HashMap<NoteId, Judge>,
}

impl JudgeEngine {
    pub fn new(windows: JudgeWindow) -> Self {
        Self::new_with_rule_mode(windows, RuleMode::Beatoraja)
    }

    pub fn new_with_rule_mode(windows: JudgeWindow, rule_mode: RuleMode) -> Self {
        Self::new_with_window_set(JudgeWindows::uniform(windows), rule_mode)
    }

    pub fn new_with_window_set(window_set: JudgeWindows, rule_mode: RuleMode) -> Self {
        Self {
            windows: window_set.note,
            window_set,
            rule_mode,
            lanes: [LaneJudgeState::default(); LANE_COUNT],
            judged_notes: HashMap::new(),
        }
    }

    pub fn set_window_set(&mut self, window_set: JudgeWindows) {
        self.windows = window_set.note;
        self.window_set = window_set;
    }

    pub fn process_input(&mut self, chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        match input.kind {
            InputKind::Press => self.process_press(chart, input),
            InputKind::Release => self.process_release(chart, input),
        }
    }

    pub fn process_misses(&mut self, chart: &PlayableChart, now: TimeUs) -> JudgeOutcome {
        let mut outcome = JudgeOutcome::default();

        for lane in Lane::ALL {
            let lane_state = &mut self.lanes[lane.index()];

            while let Some((idx, note)) =
                next_press_reference_note(chart, lane, lane_state.next_note_index)
            {
                let windows = self.window_set.press_window(lane);
                if now.0 <= note.time.0 + windows.bad_slow_us {
                    break;
                }

                lane_state.next_note_index = idx + 1;
                self.judged_notes.insert(note.id, Judge::Poor);
                outcome.events.push(JudgementEvent {
                    note_id: Some(note.id),
                    lane,
                    judge: Judge::Poor,
                    side: TimingSide::Slow,
                    delta: TimeUs(now.0 - note.time.0),
                    time: now,
                });
            }

            if let Some(active) = lane_state.active_long {
                match active.mode {
                    LongNoteMode::Ln => {
                        if now.0 >= active.end.end_time.0 {
                            lane_state.active_long = None;
                        }
                    }
                    LongNoteMode::Cn | LongNoteMode::Hcn => {
                        let windows = self.window_set.long_end_window(lane);
                        if now.0 > active.end.end_time.0 + windows.bad_slow_us {
                            lane_state.active_long = None;
                            outcome.events.push(JudgementEvent {
                                note_id: Some(active.end.end_note_id),
                                lane,
                                judge: Judge::Poor,
                                side: TimingSide::Slow,
                                delta: TimeUs(now.0 - active.end.end_time.0),
                                time: now,
                            });
                        }
                    }
                }
            }
        }

        outcome
    }

    pub fn is_exhausted(&self, chart: &PlayableChart) -> bool {
        Lane::ALL.iter().copied().all(|lane| {
            let state = &self.lanes[lane.index()];
            state.active_long.is_none()
                && next_press_reference_note(chart, lane, state.next_note_index).is_none()
        })
    }

    fn process_press(&mut self, chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        // Mine ヒット判定は通常ノーツの判定に先んじて、もしくは並走して行う。
        // 入力は通常ノーツの判定を妨げないので、ここでは別ベクタに積むだけ。
        let mut mine_hits = Vec::new();
        if let Some(hit) = detect_mine_hit(
            chart,
            input.lane,
            input.time,
            self.window_set.press_window(input.lane).mine_hit_us,
            &self.lanes[input.lane.index()],
        ) {
            self.lanes[input.lane.index()].last_mine_hit_time = Some(hit.time);
            mine_hits.push(hit);
        }

        let rule_mode = self.rule_mode;
        let windows = self.window_set.press_window(input.lane);
        let lane_state = &mut self.lanes[input.lane.index()];
        let Some((idx, note)) =
            next_press_reference_note(chart, input.lane, lane_state.next_note_index)
        else {
            let mut outcome =
                classify_empty_poor_from_last_press(lane_state.last_press_time, input, windows)
                    .unwrap_or_default();
            outcome.mine_hits = mine_hits;
            return outcome;
        };

        let delta = input.time.0 - note.time.0;

        if let Some(judge) = classify_normal_delta(delta, windows).filter(|judge| {
            !suppresses_long_start_late_bad(rule_mode, windows, note, delta, *judge)
        }) {
            lane_state.next_note_index = idx + 1;
            lane_state.last_press_time = Some(note.time);
            self.judged_notes.insert(note.id, judge);

            if note.kind == NoteKind::LongStart
                && let Some(active) = make_active_long(chart, note.id, input.time)
            {
                lane_state.active_long = Some(active);
            }

            return JudgeOutcome {
                events: vec![JudgementEvent {
                    note_id: Some(note.id),
                    lane: input.lane,
                    judge,
                    side: side_from_delta(delta),
                    delta: TimeUs(delta),
                    time: input.time,
                }],
                mine_hits,
                consumed_input: true,
            };
        }

        let mut outcome = if let Some(outcome) =
            classify_empty_poor_from_last_press(lane_state.last_press_time, input, windows)
        {
            outcome
        } else if delta > windows.bad_slow_us && delta <= windows.empty_poor_slow_us {
            empty_poor(input.lane, TimingSide::Slow, TimeUs(delta), input.time)
        } else if delta < -windows.bad_fast_us && (-delta) <= windows.empty_poor_fast_us {
            empty_poor(input.lane, TimingSide::Fast, TimeUs(delta), input.time)
        } else {
            JudgeOutcome::default()
        };
        outcome.mine_hits = mine_hits;
        outcome
    }

    fn process_release(&mut self, _chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        let lane_state = &mut self.lanes[input.lane.index()];
        let Some(active) = lane_state.active_long else {
            return JudgeOutcome::default();
        };

        match active.mode {
            LongNoteMode::Ln => {
                lane_state.active_long = None;
                if input.time.0 >= active.end.end_time.0 {
                    return JudgeOutcome::default();
                }
                self.judged_notes.insert(active.start_note_id, Judge::Poor);
                JudgeOutcome {
                    events: vec![JudgementEvent {
                        note_id: Some(active.start_note_id),
                        lane: input.lane,
                        judge: Judge::Poor,
                        side: TimingSide::Fast,
                        delta: TimeUs(input.time.0 - active.end.end_time.0),
                        time: input.time,
                    }],
                    mine_hits: Vec::new(),
                    consumed_input: true,
                }
            }
            LongNoteMode::Cn | LongNoteMode::Hcn => {
                let delta = input.time.0 - active.end.end_time.0;
                let side = side_from_delta(delta);
                let windows = self.window_set.long_end_window(input.lane);
                let judge = classify_normal_delta(delta, windows).unwrap_or(Judge::Poor);
                lane_state.active_long = None;
                self.judged_notes.insert(active.end.end_note_id, judge);

                JudgeOutcome {
                    events: vec![JudgementEvent {
                        note_id: Some(active.end.end_note_id),
                        lane: input.lane,
                        judge,
                        side,
                        delta: TimeUs(delta),
                        time: input.time,
                    }],
                    mine_hits: Vec::new(),
                    consumed_input: true,
                }
            }
        }
    }
}

fn suppresses_long_start_late_bad(
    rule_mode: RuleMode,
    windows: JudgeWindow,
    note: &NoteEvent,
    delta: i64,
    judge: Judge,
) -> bool {
    rule_mode == RuleMode::Lr2Oraja
        && note.kind == NoteKind::LongStart
        && judge == Judge::Bad
        && delta > windows.good_us
}

fn next_press_reference_note(
    chart: &PlayableChart,
    lane: Lane,
    start_index: usize,
) -> Option<(usize, &NoteEvent)> {
    chart
        .notes_for_lane(lane)
        .iter()
        .enumerate()
        .skip(start_index)
        .find(|(_, note)| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
}

/// 指定レーンに置かれた Mine の中から、入力時刻と `window_us` 以内に一致するものを探す。
/// 直近に同じ time の Mine をヒット済みなら無視する（二重ヒット防止）。
fn detect_mine_hit(
    chart: &PlayableChart,
    lane: Lane,
    input_time: TimeUs,
    window_us: i64,
    lane_state: &LaneJudgeState,
) -> Option<MineHitEvent> {
    chart
        .notes_for_lane(lane)
        .iter()
        .filter(|note| note.kind == NoteKind::Mine)
        .filter(|note| Some(note.time) != lane_state.last_mine_hit_time)
        .find(|note| (input_time.0 - note.time.0).abs() <= window_us)
        .map(|note| MineHitEvent {
            note_id: note.id,
            lane,
            damage: note.damage.unwrap_or(0),
            time: note.time,
        })
}

fn classify_normal_delta(delta_us: i64, windows: JudgeWindow) -> Option<Judge> {
    let abs = delta_us.abs();

    if abs <= windows.pgreat_us {
        Some(Judge::PGreat)
    } else if abs <= windows.great_us {
        Some(Judge::Great)
    } else if abs <= windows.good_us {
        Some(Judge::Good)
    } else if (delta_us < 0 && abs <= windows.bad_fast_us)
        || (delta_us >= 0 && abs <= windows.bad_slow_us)
    {
        Some(Judge::Bad)
    } else {
        None
    }
}

fn side_from_delta(delta_us: i64) -> TimingSide {
    if delta_us < 0 { TimingSide::Fast } else { TimingSide::Slow }
}

fn make_active_long(
    chart: &PlayableChart,
    start_note_id: NoteId,
    started_at: TimeUs,
) -> Option<ActiveLongNote> {
    let (pair_index, pair) = chart
        .long_notes
        .iter()
        .enumerate()
        .find(|(_, pair)| pair.start_note_id == start_note_id)?;

    Some(ActiveLongNote {
        pair_index,
        mode: pair.mode.unwrap_or(chart.metadata.long_note_mode),
        start_note_id,
        end: LongNoteEndRef {
            end_note_id: pair.end_note_id,
            end_tick: pair.end_tick,
            end_time: pair.end_time,
        },
        started_at,
    })
}

fn empty_poor(lane: Lane, side: TimingSide, delta: TimeUs, time: TimeUs) -> JudgeOutcome {
    JudgeOutcome {
        events: vec![JudgementEvent {
            note_id: None,
            lane,
            judge: Judge::EmptyPoor,
            side,
            delta,
            time,
        }],
        mine_hits: Vec::new(),
        consumed_input: false,
    }
}

fn classify_empty_poor_from_last_press(
    last_press_time: Option<TimeUs>,
    input: InputEvent,
    windows: JudgeWindow,
) -> Option<JudgeOutcome> {
    let note_time = last_press_time?;
    let delta = input.time.0 - note_time.0;

    if delta >= 0 && delta <= windows.empty_poor_slow_us {
        return Some(empty_poor(input.lane, TimingSide::Slow, TimeUs(delta), input.time));
    }

    if delta < 0 && (-delta) <= windows.empty_poor_fast_us {
        return Some(empty_poor(input.lane, TimingSide::Fast, TimeUs(delta), input.time));
    }

    None
}

#[cfg(test)]
mod tests {
    use bmz_chart::model::{ChartMetadata, LongNotePair, LongNoteStyle, SoundAssetRef, SoundEvent};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::input::InputSource;

    use super::*;

    fn windows() -> JudgeWindow {
        JudgeWindow::symmetric(16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000)
    }

    fn chart_with_tap(time: TimeUs) -> PlayableChart {
        chart_with_lane_tap(Lane::Key1, time)
    }

    fn chart_with_lane_tap(lane: Lane, time: TimeUs) -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane,
            kind: NoteKind::Tap,
            tick: Default::default(),
            time,
            sound: None,
            damage: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[lane.index()].push(note);

        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::<SoundEvent>::new(),
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
            sounds: Vec::<SoundAssetRef>::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: time,
        }
    }

    fn chart_with_long_start(time: TimeUs, end_time: TimeUs) -> PlayableChart {
        chart_with_lane_long_start(Lane::Key1, time, end_time)
    }

    fn chart_with_lane_long_start(lane: Lane, time: TimeUs, end_time: TimeUs) -> PlayableChart {
        let start = NoteEvent {
            id: NoteId(1),
            lane,
            kind: NoteKind::LongStart,
            tick: Default::default(),
            time,
            sound: None,
            damage: None,
        };
        let end = NoteEvent {
            id: NoteId(2),
            lane,
            kind: NoteKind::LongEnd,
            tick: Default::default(),
            time: end_time,
            sound: None,
            damage: None,
        };
        let mut chart = chart_with_tap(time);
        chart.metadata.long_note_mode = LongNoteMode::Ln;
        chart.lane_notes[lane.index()] = vec![start, end];
        chart.long_notes = vec![LongNotePair {
            lane,
            style: LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(1),
            end_note_id: NoteId(2),
            start_tick: Default::default(),
            end_tick: Default::default(),
            start_time: time,
            end_time,
            sound: None,
        }];
        chart
    }

    fn press_at(time: TimeUs) -> InputEvent {
        press_lane_at(Lane::Key1, time)
    }

    fn press_lane_at(lane: Lane, time: TimeUs) -> InputEvent {
        InputEvent {
            source: InputSource::Human,
            lane,
            kind: InputKind::Press,
            time,
            device_kind: bmz_core::input::InputDeviceKind::Keyboard,
            scratch_direction: None,
        }
    }

    fn release_at(time: TimeUs) -> InputEvent {
        release_lane_at(Lane::Key1, time)
    }

    fn release_lane_at(lane: Lane, time: TimeUs) -> InputEvent {
        InputEvent {
            source: InputSource::Human,
            lane,
            kind: InputKind::Release,
            time,
            device_kind: bmz_core::input::InputDeviceKind::Keyboard,
            scratch_direction: None,
        }
    }

    #[test]
    fn normal_window_consumes_note() {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut engine = JudgeEngine::new(windows());

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_030_000)));

        assert!(outcome.consumed_input);
        assert_eq!(outcome.events.len(), 1);
        assert_eq!(outcome.events[0].judge, Judge::Great);
        assert_eq!(outcome.events[0].side, TimingSide::Slow);
        assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
    }

    #[test]
    fn slow_empty_poor_does_not_consume_note() {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut engine = JudgeEngine::new(windows());

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

        assert!(!outcome.consumed_input);
        assert_eq!(outcome.events.len(), 1);
        assert_eq!(outcome.events[0].judge, Judge::EmptyPoor);
        assert_eq!(outcome.events[0].side, TimingSide::Slow);
        assert_eq!(outcome.events[0].note_id, None);
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
    }

    #[test]
    fn fast_empty_poor_does_not_consume_note() {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut engine = JudgeEngine::new(windows());

        let outcome = engine.process_input(&chart, press_at(TimeUs(700_000)));

        assert!(!outcome.consumed_input);
        assert_eq!(outcome.events.len(), 1);
        assert_eq!(outcome.events[0].judge, Judge::EmptyPoor);
        assert_eq!(outcome.events[0].side, TimingSide::Fast);
        assert_eq!(outcome.events[0].note_id, None);
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
    }

    #[test]
    fn outside_empty_poor_windows_is_unjudged() {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut engine = JudgeEngine::new(windows());

        let too_late = engine.process_input(&chart, press_at(TimeUs(1_250_000)));
        let too_early = engine.process_input(&chart, press_at(TimeUs(400_000)));

        assert!(too_late.events.is_empty());
        assert!(!too_late.consumed_input);
        assert!(too_early.events.is_empty());
        assert!(!too_early.consumed_input);
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
    }

    #[test]
    fn double_press_after_normal_judge_is_slow_empty_poor() {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut engine = JudgeEngine::new(windows());

        let first = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let second = engine.process_input(&chart, press_at(TimeUs(1_005_000)));

        assert_eq!(first.events[0].judge, Judge::PGreat);
        assert_eq!(first.events[0].note_id, Some(NoteId(1)));
        assert!(!second.consumed_input);
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].judge, Judge::EmptyPoor);
        assert_eq!(second.events[0].side, TimingSide::Slow);
        assert_eq!(second.events[0].note_id, None);
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
    }

    #[test]
    fn beatoraja_7k_double_press_after_slow_empty_poor_window_is_unjudged() {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut engine =
            JudgeEngine::new(crate::judge::window::beatoraja_note_judge_window_for_keymode(
                bmz_core::lane::KeyMode::K7,
            ));

        let first = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let second = engine.process_input(&chart, press_at(TimeUs(1_151_000)));

        assert_eq!(first.events[0].judge, Judge::PGreat);
        assert!(second.events.is_empty());
        assert!(!second.consumed_input);
    }

    #[test]
    fn scratch_press_uses_scratch_window() {
        let chart = chart_with_lane_tap(Lane::Scratch, TimeUs(1_000_000));
        let mut engine = JudgeEngine::new_with_window_set(
            crate::judge::window::beatoraja_judge_windows_for_keymode(bmz_core::lane::KeyMode::K7),
            RuleMode::Beatoraja,
        );

        let outcome = engine.process_input(&chart, press_lane_at(Lane::Scratch, TimeUs(1_065_000)));

        assert_eq!(outcome.events[0].judge, Judge::Great);
        assert_eq!(outcome.events[0].side, TimingSide::Slow);
    }

    #[test]
    fn cn_release_uses_long_note_end_window() {
        let mut window_set = JudgeWindows::uniform(windows());
        window_set.long_note_end =
            JudgeWindow::symmetric(120_000, 160_000, 200_000, 220_000, 0, 0, 16_000);
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.long_notes[0].mode = Some(LongNoteMode::Cn);
        let mut engine = JudgeEngine::new_with_window_set(window_set, RuleMode::Beatoraja);

        let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let release = engine.process_input(&chart, release_at(TimeUs(2_150_000)));

        assert_eq!(press.events[0].judge, Judge::PGreat);
        assert_eq!(release.events[0].judge, Judge::Great);
    }

    #[test]
    fn lr2oraja_suppresses_late_bad_on_long_note_start() {
        let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        let input = press_at(TimeUs(1_100_000));

        let mut beatoraja = JudgeEngine::new(windows());
        let beatoraja_outcome = beatoraja.process_input(&chart, input);
        assert_eq!(beatoraja_outcome.events[0].judge, Judge::Bad);
        assert_eq!(beatoraja.lanes[Lane::Key1.index()].next_note_index, 1);

        let mut lr2oraja = JudgeEngine::new_with_rule_mode(windows(), RuleMode::Lr2Oraja);
        let lr2oraja_outcome = lr2oraja.process_input(&chart, input);
        assert!(lr2oraja_outcome.events.is_empty());
        assert!(!lr2oraja_outcome.consumed_input);
        assert_eq!(lr2oraja.lanes[Lane::Key1.index()].next_note_index, 0);
    }

    #[test]
    fn defined_cn_pair_judges_release_even_when_chart_default_is_ln() {
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.metadata.long_note_mode = LongNoteMode::Ln;
        chart.long_notes[0].mode = Some(LongNoteMode::Cn);
        let mut engine = JudgeEngine::new(windows());

        let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let release = engine.process_input(&chart, release_at(TimeUs(2_000_000)));

        assert_eq!(press.events[0].judge, Judge::PGreat);
        assert_eq!(release.events.len(), 1);
        assert_eq!(release.events[0].note_id, Some(NoteId(2)));
        assert_eq!(release.events[0].judge, Judge::PGreat);
    }

    #[test]
    fn defined_hcn_pair_judges_early_release_even_when_chart_default_is_ln() {
        // 早離し後の減衰は judge engine ではなく session 側の passing ベース
        // (update_hcn_lane_timers / apply_hcn_gauge) で処理される。
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.metadata.long_note_mode = LongNoteMode::Ln;
        chart.long_notes[0].mode = Some(LongNoteMode::Hcn);
        let mut engine = JudgeEngine::new(windows());

        let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let release = engine.process_input(&chart, release_at(TimeUs(1_500_000)));

        assert_eq!(press.events[0].judge, Judge::PGreat);
        assert_eq!(release.events[0].note_id, Some(NoteId(2)));
        assert_eq!(release.events[0].judge, Judge::Poor);
        assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
    }

    fn chart_with_mine(time: TimeUs, damage: u16) -> PlayableChart {
        let lane = Lane::Key1;
        let note = NoteEvent {
            id: NoteId(7),
            lane,
            kind: NoteKind::Mine,
            tick: Default::default(),
            time,
            sound: None,
            damage: Some(damage),
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[lane.index()].push(note);
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::<SoundEvent>::new(),
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
            sounds: Vec::<SoundAssetRef>::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: time,
        }
    }

    #[test]
    fn mine_hit_emits_event_with_damage() {
        let chart = chart_with_mine(TimeUs(1_000_000), 8);
        let mut engine = JudgeEngine::new(windows());

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_000_000)));

        assert_eq!(outcome.mine_hits.len(), 1);
        assert_eq!(outcome.mine_hits[0].damage, 8);
        assert_eq!(outcome.mine_hits[0].note_id, NoteId(7));
        // Mine ヒットは通常判定とは別ベクタに入る。スコア対象ノーツが無いので
        // events は空、consumed_input も false のまま。
        assert!(outcome.events.is_empty());
        assert!(!outcome.consumed_input);
    }

    #[test]
    fn mine_does_not_hit_outside_window() {
        let chart = chart_with_mine(TimeUs(1_000_000), 8);
        let mut engine = JudgeEngine::new(windows());

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));
        assert!(outcome.mine_hits.is_empty());
    }

    #[test]
    fn mine_hit_does_not_double_fire() {
        let chart = chart_with_mine(TimeUs(1_000_000), 8);
        let mut engine = JudgeEngine::new(windows());

        let first = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let second = engine.process_input(&chart, press_at(TimeUs(1_000_000)));

        assert_eq!(first.mine_hits.len(), 1);
        assert!(second.mine_hits.is_empty(), "same Mine must not fire twice");
    }

    #[test]
    fn miss_is_reported_after_bad_window() {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut engine = JudgeEngine::new(windows());

        let still_candidate = engine.process_misses(&chart, TimeUs(1_110_000));
        let missed = engine.process_misses(&chart, TimeUs(1_130_000));

        assert!(still_candidate.events.is_empty());
        assert_eq!(missed.events.len(), 1);
        assert_eq!(missed.events[0].judge, Judge::Poor);
        assert_eq!(missed.events[0].side, TimingSide::Slow);
        assert_eq!(missed.events[0].note_id, Some(NoteId(1)));
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
    }
}
