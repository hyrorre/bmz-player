//! OpenLR2 keeps mines in the same per-lane object list as notes.
//!
//! `ProcSinglenote` handles whichever object the lane cursor points at, so a
//! mine at the head blocks later notes until it is consumed, and consuming it
//! uses up that poll of the lane. A mine is hit by a new press whose time is
//! within the early GREAT width (`realTiming - timing < GREAT`), or by a key
//! that is pressed or held on the first poll after the mine time. A release or
//! an idle lane lets the passed mine go without damage.
//!
//! This is enabled only on the LEGACY 1 ms polling clock, where the session
//! supplies whether an idle lane is held. Other paths keep the shared
//! `process_mine_passes` behaviour.

use super::*;

/// The state OpenLR2 passes to `ProcNoteOnTiming` for one lane poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyLaneKey {
    /// No key on the lane.
    Idle,
    /// A new key-down edge (`keypress == 1`).
    Press,
    /// Still held without a new edge (`keypress == 2`).
    Hold,
    /// A key-up edge (`keypress == 3`).
    Release,
}

impl JudgeEngine {
    /// Enables the OpenLR2 inline mine handling. Ignored outside LEGACY.
    pub fn set_legacy_inline_mines(&mut self, enabled: bool) {
        self.legacy_inline_mines = enabled && self.rule_mode == RuleMode::Legacy;
    }

    pub fn legacy_inline_mines(&self) -> bool {
        self.legacy_inline_mines
    }

    /// Index of the first mine of the lane that has not been consumed yet.
    /// LEGACY inline mines use `next_mine_index` only for this cursor.
    pub(super) fn legacy_pending_mine(
        &mut self,
        chart: &PlayableChart,
        lane: Lane,
    ) -> Option<usize> {
        if !self.legacy_inline_mines {
            return None;
        }
        let notes = chart.notes_for_lane(lane);
        let state = &mut self.lanes[lane.index()];
        while let Some(note) = notes.get(state.next_mine_index) {
            if note.kind == NoteKind::Mine {
                return Some(state.next_mine_index);
            }
            state.next_mine_index += 1;
        }
        None
    }

    /// The mine at the lane cursor, when it comes before the next note. An
    /// active LN stays at the cursor until it is finalized.
    fn legacy_mine_head(&mut self, chart: &PlayableChart, lane: Lane) -> Option<usize> {
        if self.lanes[lane.index()].active_long.is_some() {
            return None;
        }
        let mine = self.legacy_pending_mine(chart, lane)?;
        let next_note = next_unjudged_press_reference_note(
            chart,
            lane,
            self.lanes[lane.index()].next_note_index,
            &self.judged_notes,
        )
        .map(|(index, _)| index);
        next_note.is_none_or(|index| mine < index).then_some(mine)
    }

    /// `ProcSinglenote`'s mine branch. Returns `None` when the lane head is
    /// not a mine, so the caller handles the note as usual.
    pub(crate) fn process_legacy_mine_head(
        &mut self,
        chart: &PlayableChart,
        lane: Lane,
        key: LegacyLaneKey,
        judge_time: TimeUs,
        event_time: TimeUs,
    ) -> Option<JudgeOutcome> {
        let index = self.legacy_mine_head(chart, lane)?;
        let note = &chart.notes_for_lane(lane)[index];
        // OpenLR2 compares the integer-ms poll time with the double note time:
        // `timing <= realTiming` exactly, `realTiming - timing < GREAT` after
        // rounding the subtraction. The µs-floored time is used only for a
        // chart without OpenLR2's times.
        let now_us = judge_time.0.div_euclid(1_000) * 1_000;
        let great_us =
            press_window(self.window_set, self.scratch_lane_mask[lane.index()]).bands(true)[1];
        let (passed, early_hit) = match legacy_exact_ms(chart, note) {
            Some(ms) => {
                let t = judge_time.0.div_euclid(1_000) as f64;
                (t > ms, ms - t < great_us as f64 / 1_000.0)
            }
            None => (now_us > note.time.0, note.time.0 < now_us + great_us),
        };
        let hit = match key {
            LegacyLaneKey::Press => early_hit,
            LegacyLaneKey::Hold => passed,
            LegacyLaneKey::Idle | LegacyLaneKey::Release => false,
        };
        let consumed_input = key == LegacyLaneKey::Press;
        if !hit && !passed {
            // The mine still blocks the lane. A press here plays nothing.
            return Some(JudgeOutcome { consumed_input, ..Default::default() });
        }

        self.lanes[lane.index()].next_mine_index = index + 1;
        let mut outcome = JudgeOutcome { consumed_input, ..Default::default() };
        if hit {
            outcome.mine_hits.push(MineHitEvent {
                note_id: note.id,
                lane,
                damage: note.damage.unwrap_or(0.0),
                sound: note.sound,
                time: event_time,
            });
        }
        Some(outcome)
    }

    /// After a tap BAD, OpenLR2 recurses into `ProcSinglenote` when the next
    /// object is within the BAD width, and that object may be a mine.
    pub(super) fn process_legacy_recursive_mine(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        event_time: TimeUs,
    ) -> Option<JudgeOutcome> {
        let index = self.legacy_mine_head(chart, input.lane)?;
        let note = &chart.notes_for_lane(input.lane)[index];
        let windows = press_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
        let gap = legacy_offset(input.time, note.time).abs();
        if gap > windows.bad_us() {
            return Some(JudgeOutcome::default());
        }
        self.process_legacy_mine_head(
            chart,
            input.lane,
            LegacyLaneKey::Press,
            input.time,
            event_time,
        )
    }
}
