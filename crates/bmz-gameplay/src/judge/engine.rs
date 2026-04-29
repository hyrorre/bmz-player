use bmz_chart::model::{NoteEvent, NoteKind, PlayableChart};
use bmz_core::ids::NoteId;
use bmz_core::input::{InputEvent, InputKind};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use super::model::{
    ActiveLongNote, JudgeOutcome, JudgeWindow, JudgementEvent, LaneJudgeState, LongNoteEndRef,
};

#[derive(Debug, Clone)]
pub struct JudgeEngine {
    pub windows: JudgeWindow,
    pub lanes: [LaneJudgeState; LANE_COUNT],
}

impl JudgeEngine {
    pub fn new(windows: JudgeWindow) -> Self {
        Self { windows, lanes: [LaneJudgeState::default(); LANE_COUNT] }
    }

    pub fn process_input(&mut self, chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        match input.kind {
            InputKind::Press => self.process_press(chart, input),
            InputKind::Release => self.process_release(input),
        }
    }

    pub fn process_misses(&mut self, chart: &PlayableChart, now: TimeUs) -> JudgeOutcome {
        let mut outcome = JudgeOutcome::default();

        for lane in Lane::ALL {
            let lane_state = &mut self.lanes[lane.index()];

            loop {
                let Some((idx, note)) =
                    next_press_reference_note(chart, lane, lane_state.next_note_index)
                else {
                    break;
                };

                if now.0 <= note.time.0 + self.windows.bad_us {
                    break;
                }

                lane_state.next_note_index = idx + 1;
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
                if now.0 > active.end.end_time.0 + self.windows.bad_us {
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
        let lane_state = &mut self.lanes[input.lane.index()];
        let Some((idx, note)) =
            next_press_reference_note(chart, input.lane, lane_state.next_note_index)
        else {
            return classify_empty_poor_from_last_press(
                lane_state.last_press_time,
                input,
                self.windows,
            )
            .unwrap_or_default();
        };

        let delta = input.time.0 - note.time.0;

        if let Some(judge) = classify_normal_delta(delta, self.windows) {
            lane_state.next_note_index = idx + 1;
            lane_state.last_press_time = Some(note.time);

            if note.kind == NoteKind::LongStart {
                if let Some(active) = make_active_long(chart, note.id) {
                    lane_state.active_long = Some(active);
                }
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
                consumed_input: true,
            };
        }

        if let Some(outcome) =
            classify_empty_poor_from_last_press(lane_state.last_press_time, input, self.windows)
        {
            return outcome;
        }

        if delta > self.windows.bad_us && delta <= self.windows.empty_poor_slow_us {
            return empty_poor(input.lane, TimingSide::Slow, TimeUs(delta), input.time);
        }

        if delta < -self.windows.bad_us && (-delta) <= self.windows.empty_poor_fast_us {
            return empty_poor(input.lane, TimingSide::Fast, TimeUs(delta), input.time);
        }

        JudgeOutcome::default()
    }

    fn process_release(&mut self, input: InputEvent) -> JudgeOutcome {
        let lane_state = &mut self.lanes[input.lane.index()];
        let Some(active) = lane_state.active_long else {
            return JudgeOutcome::default();
        };

        let delta = input.time.0 - active.end.end_time.0;
        let side = side_from_delta(delta);
        let judge = classify_normal_delta(delta, self.windows).unwrap_or(Judge::Poor);
        lane_state.active_long = None;

        JudgeOutcome {
            events: vec![JudgementEvent {
                note_id: Some(active.end.end_note_id),
                lane: input.lane,
                judge,
                side,
                delta: TimeUs(delta),
                time: input.time,
            }],
            consumed_input: true,
        }
    }
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

fn classify_normal_delta(delta_us: i64, windows: JudgeWindow) -> Option<Judge> {
    let abs = delta_us.abs();

    if abs <= windows.pgreat_us {
        Some(Judge::PGreat)
    } else if abs <= windows.great_us {
        Some(Judge::Great)
    } else if abs <= windows.good_us {
        Some(Judge::Good)
    } else if abs <= windows.bad_us {
        Some(Judge::Bad)
    } else {
        None
    }
}

fn side_from_delta(delta_us: i64) -> TimingSide {
    if delta_us < 0 { TimingSide::Fast } else { TimingSide::Slow }
}

fn make_active_long(chart: &PlayableChart, start_note_id: NoteId) -> Option<ActiveLongNote> {
    let (pair_index, pair) = chart
        .long_notes
        .iter()
        .enumerate()
        .find(|(_, pair)| pair.start_note_id == start_note_id)?;

    Some(ActiveLongNote {
        pair_index,
        start_note_id,
        end: LongNoteEndRef {
            end_note_id: pair.end_note_id,
            end_tick: pair.end_tick,
            end_time: pair.end_time,
        },
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
    use bmz_chart::model::{ChartMetadata, SoundAssetRef, SoundEvent};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::input::InputSource;

    use super::*;

    fn windows() -> JudgeWindow {
        JudgeWindow {
            pgreat_us: 16_000,
            great_us: 40_000,
            good_us: 80_000,
            bad_us: 120_000,
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 200_000,
        }
    }

    fn chart_with_tap(time: TimeUs) -> PlayableChart {
        let lane = Lane::Key1;
        let note = NoteEvent {
            id: NoteId(1),
            lane,
            kind: NoteKind::Tap,
            tick: Default::default(),
            time,
            sound: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[lane.index()].push(note);

        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::<SoundEvent>::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::<SoundAssetRef>::new(),
            total_notes: 1,
            end_time: time,
        }
    }

    fn press_at(time: TimeUs) -> InputEvent {
        InputEvent { source: InputSource::Human, lane: Lane::Key1, kind: InputKind::Press, time }
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
