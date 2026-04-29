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
            return JudgeOutcome::default();
        };

        let delta = input.time.0 - note.time.0;

        if let Some(judge) = classify_normal_delta(delta, self.windows) {
            lane_state.next_note_index = idx + 1;

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
