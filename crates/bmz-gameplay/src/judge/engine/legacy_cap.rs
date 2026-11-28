//! OpenLR2's processed-note cap and the empty slots after a lane's objects.
//!
//! `ProcSinglenote` judges a note only while `note_current < totalnotes`
//! (`note_current` counts every judgement except EMPTY POOR). `totalnotes` can
//! be lower than the number of judgeable notes: a long note that replaces an
//! object at the same time decrements it even when that object was a mine.
//! Once the count is reached, a passed single note is never settled and blocks
//! its lane. `ProcLongnote` has no such check.
//!
//! A lane buffer grows in steps of 100 objects, so at least one free slot
//! follows the last object. A free slot is a single note at `realTiming == -1`
//! with no keysound. OpenLR2 moves the lane cursor into the free slots in two
//! ways:
//! - `ProcNoteOnTiming` stops at the end of a lane only when the lane's last
//!   keysound (`noteVal`) is positive. A lane of only mines never sets it.
//! - After a BAD, `ProcSinglenote` recurses when the next slot is within the
//!   BAD width; it checks the buffer size, not the object count. A slot is
//!   within it until 199 ms.
//!
//! From then on every call judges the next free slot: POOR once passed (200 ms),
//! a hit or a BAD chain before that. Past the buffer OpenLR2 reads unallocated
//! memory; BMZ stops there and reports it.

use super::*;

/// The judgements that advance OpenLR2's `note_current`.
pub(super) fn legacy_scored(outcome: &JudgeOutcome) -> u32 {
    outcome.events.iter().filter(|e| e.affects_score && e.judge != Judge::EmptyPoor).count() as u32
}

impl JudgeEngine {
    /// Whether this play reached a lane state where OpenLR2 reads memory past
    /// a lane buffer, so its further result is undefined.
    pub fn legacy_read_past_lane_buffer(&self) -> bool {
        self.legacy_read_past_lane_buffer
    }

    /// Whether OpenLR2 refuses to judge a single note now. `pending` counts
    /// judgements of the current call that are not yet in the total.
    pub(super) fn legacy_capped(&self, chart: &PlayableChart, pending: u32) -> bool {
        self.rule_mode == RuleMode::Legacy
            && self.legacy_note_current + pending >= chart.total_notes
    }

    /// Adds a finished call's judgements to OpenLR2's `note_current`.
    pub(super) fn count_legacy_judgements(&mut self, outcome: &JudgeOutcome) {
        if self.rule_mode == RuleMode::Legacy {
            self.legacy_note_current += legacy_scored(outcome);
        }
    }

    /// The lane's free buffer slots and whether its objects are all mines.
    fn legacy_lane_buffer(&mut self, chart: &PlayableChart, lane: Lane) -> (u32, bool) {
        if let Some(buffer) = self.legacy_lane_buffers[lane.index()] {
            return buffer;
        }
        let mut objects = 0_u32;
        let mut only_mines = true;
        for note in chart.notes_for_lane(lane) {
            match note.kind {
                NoteKind::Mine => objects += 1,
                NoteKind::Tap | NoteKind::LongStart => {
                    objects += 1;
                    only_mines = false;
                }
                _ => {}
            }
        }
        let buffer = ((objects / 100 + 1) * 100 - objects, only_mines && objects > 0);
        self.legacy_lane_buffers[lane.index()] = Some(buffer);
        buffer
    }

    /// Whether the lane cursor is in the free slots.
    fn legacy_in_free_slots(&mut self, chart: &PlayableChart, lane: Lane) -> bool {
        if !self.legacy_inline_mines {
            return false;
        }
        if self.lanes[lane.index()].legacy_in_free_slots {
            return true;
        }
        let (_, only_mines) = self.legacy_lane_buffer(chart, lane);
        let entered = only_mines && self.legacy_pending_mine(chart, lane).is_none();
        self.lanes[lane.index()].legacy_in_free_slots = entered;
        entered
    }

    /// One `ProcNoteOnTiming` call on a lane whose cursor is in the free slots.
    /// `None` when it is not. `press` is a new key-down edge.
    pub(super) fn process_legacy_free_slot(
        &mut self,
        chart: &PlayableChart,
        lane: Lane,
        press: bool,
        judge_time: TimeUs,
        event_time: TimeUs,
    ) -> Option<JudgeOutcome> {
        if !self.legacy_in_free_slots(chart, lane) {
            return None;
        }
        let (slots, _) = self.legacy_lane_buffer(chart, lane);
        if self.lanes[lane.index()].legacy_free_slots_used >= slots {
            // OpenLR2 now reads (and writes `stage` of) memory past the buffer.
            self.legacy_read_past_lane_buffer = true;
            return Some(JudgeOutcome::default());
        }
        Some(self.judge_legacy_free_slot(chart, lane, press, judge_time, event_time, 0))
    }

    /// The BAD recursion of `ProcSinglenote` from the lane's last object into
    /// the free slots. `None` when the next slot is beyond the BAD width.
    pub(super) fn recurse_into_legacy_free_slot(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        event_time: TimeUs,
        pending: u32,
    ) -> Option<JudgeOutcome> {
        if !self.legacy_inline_mines {
            return None;
        }
        let windows = press_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
        if input.time.0.div_euclid(1_000) + 1 > windows.bad_slow_us / 1_000 {
            return None;
        }
        self.lanes[input.lane.index()].legacy_in_free_slots = true;
        Some(self.judge_legacy_free_slot(chart, input.lane, true, input.time, event_time, pending))
    }

    /// `ProcSinglenote` on free slots: the slot at the cursor, and the next
    /// ones while a press gives BAD.
    fn judge_legacy_free_slot(
        &mut self,
        chart: &PlayableChart,
        lane: Lane,
        press: bool,
        judge_time: TimeUs,
        event_time: TimeUs,
        pending: u32,
    ) -> JudgeOutcome {
        let (slots, _) = self.legacy_lane_buffer(chart, lane);
        let windows = press_window(self.window_set, self.scratch_lane_mask[lane.index()]);
        // `timing - (int)realTiming` with realTiming == -1.
        let delta_us = (judge_time.0.div_euclid(1_000) + 1) * 1_000;
        let mut outcome = JudgeOutcome::default();
        loop {
            if self.legacy_capped(chart, pending + legacy_scored(&outcome)) {
                return outcome;
            }
            let judge = if delta_us > windows.bad_slow_us {
                Judge::Poor
            } else if press {
                classify_normal_delta(delta_us, windows).unwrap_or(Judge::Bad)
            } else {
                return outcome;
            };
            self.lanes[lane.index()].legacy_free_slots_used += 1;
            outcome.events.push(JudgementEvent {
                note_id: None,
                lane,
                judge,
                side: TimingSide::Slow,
                delta: TimeUs(delta_us),
                time: event_time,
                affects_score: true,
                advances_score: true,
            });
            if judge != Judge::Bad || self.lanes[lane.index()].legacy_free_slots_used >= slots {
                return outcome;
            }
        }
    }
}
