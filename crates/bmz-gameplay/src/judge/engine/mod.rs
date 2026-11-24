use std::collections::{BTreeSet, HashMap, HashSet};

use bmz_chart::model::{LongNoteMode, NoteEvent, NoteKind, PlayableChart};
use bmz_core::ids::NoteId;
use bmz_core::input::{InputEvent, InputKind, ScratchDirection};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use super::model::{
    ActiveLongNote, JudgeAlgorithm, JudgeOutcome, JudgeWindow, JudgeWindows, JudgementEvent,
    KeySoundEvent, KeySoundTrigger, LaneJudgeState, LongNoteEndRef, MineHitEvent,
    PendingLongRelease, ScratchPressSuppression,
};
use crate::rule::RuleMode;

mod chart_index;
use chart_index::ChartIndex;

const BSS_REVERSE_PRESS_SUPPRESSION_US: i64 = 30_000;

/// OpenLR2 polls P1 scratch/keys, then P2 scratch/keys.
pub const LEGACY_LANE_ORDER: [Lane; LANE_COUNT] = [
    Lane::Scratch,
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key6,
    Lane::Key7,
    Lane::Scratch2,
    Lane::Key8,
    Lane::Key9,
    Lane::Key10,
    Lane::Key11,
    Lane::Key12,
    Lane::Key13,
    Lane::Key14,
];

fn press_window(window_set: JudgeWindows, scratch: bool) -> JudgeWindow {
    if scratch { window_set.scratch } else { window_set.note }
}

/// LN 終端の離し時刻差 `delta_us`（負が早離し）に使う窓。
fn long_end_window(window_set: JudgeWindows, scratch: bool, delta_us: i64) -> JudgeWindow {
    let (window, fast) = if scratch {
        (window_set.long_scratch_end, window_set.long_scratch_end_fast)
    } else {
        (window_set.long_note_end, window_set.long_note_end_fast)
    };
    if delta_us < 0 { fast.unwrap_or(window) } else { window }
}

fn long_release_margin_us(window_set: JudgeWindows, scratch: bool) -> i64 {
    if scratch {
        window_set.long_scratch_release_margin_us
    } else {
        window_set.long_note_release_margin_us
    }
}

#[derive(Debug, Clone)]
pub struct JudgeEngine {
    pub windows: JudgeWindow,
    pub window_set: JudgeWindows,
    pub rule_mode: RuleMode,
    pub algorithm: JudgeAlgorithm,
    pub window_rates: super::window::JudgeWindowRates,
    pub lanes: [LaneJudgeState; LANE_COUNT],
    pub judged_notes: HashMap<NoteId, Judge>,
    scratch_lane_mask: [bool; LANE_COUNT],
    attempted_notes: HashSet<NoteId>,
    bad_judge_vanish: bool,
    orphan_charge_ends: [BTreeSet<(TimeUs, NoteId)>; LANE_COUNT],
    hln: hln::HlnState,
    free_zone_scratched: HashSet<NoteId>,
    pub free_zone_autoplay: [bool; LANE_COUNT],
    chart_index: Option<std::sync::Arc<ChartIndex>>,
    legacy_inline_mines: bool,
    /// OpenLR2's `note_current`: LEGACY judgements other than EMPTY POOR.
    legacy_note_current: u32,
    /// Per lane, OpenLR2's free buffer slots and whether its objects are all mines
    /// (computed on use).
    legacy_lane_buffers: [Option<(u32, bool)>; LANE_COUNT],
    legacy_read_past_lane_buffer: bool,
}

impl JudgeEngine {
    pub fn rebind_chart(&mut self, old: &PlayableChart, new: &PlayableChart) {
        for lane in Lane::ALL {
            let state = &mut self.lanes[lane.index()];
            state.next_note_index = 0;
            state.next_mine_index = 0;
            if let Some(active) = &mut state.active_long
                && let Some(index) =
                    new.long_notes.iter().position(|p| p.start_note_id == active.start_note_id)
            {
                active.pair_index = index;
            }
        }
        self.hln.rebind(old, new);
        self.legacy_lane_buffers = [None; LANE_COUNT];
    }

    pub fn is_note_started(&self, id: NoteId) -> bool {
        self.judged_notes.contains_key(&id)
            || self.attempted_notes.contains(&id)
            || self
                .lanes
                .iter()
                .any(|lane| lane.active_long.as_ref().is_some_and(|p| p.start_note_id == id))
    }
    pub fn new(windows: JudgeWindow) -> Self {
        Self::new_with_rule_mode(windows, RuleMode::Beatoraja)
    }

    pub fn new_with_rule_mode(windows: JudgeWindow, rule_mode: RuleMode) -> Self {
        Self::new_with_window_set(JudgeWindows::uniform(windows), rule_mode)
    }

    pub fn new_with_window_set(window_set: JudgeWindows, rule_mode: RuleMode) -> Self {
        Self::new_with_window_set_and_algorithm(window_set, rule_mode, JudgeAlgorithm::Combo)
    }

    pub fn new_with_window_set_and_algorithm(
        window_set: JudgeWindows,
        rule_mode: RuleMode,
        algorithm: JudgeAlgorithm,
    ) -> Self {
        Self::new_with_window_set_algorithm_and_keymode(
            window_set,
            rule_mode,
            algorithm,
            KeyMode::K7,
        )
    }

    pub fn new_with_window_set_algorithm_and_keymode(
        window_set: JudgeWindows,
        rule_mode: RuleMode,
        algorithm: JudgeAlgorithm,
        key_mode: KeyMode,
    ) -> Self {
        let mut scratch_lane_mask = [false; LANE_COUNT];
        scratch_lane_mask[Lane::Scratch.index()] = true;
        scratch_lane_mask[Lane::Scratch2.index()] = true;
        Self {
            windows: window_set.note,
            window_set,
            rule_mode,
            algorithm,
            window_rates: Default::default(),
            lanes: [LaneJudgeState::default(); LANE_COUNT],
            judged_notes: HashMap::new(),
            scratch_lane_mask,
            attempted_notes: HashSet::new(),
            bad_judge_vanish: bad_judge_vanish_for_keymode_and_rule_mode(key_mode, rule_mode),
            orphan_charge_ends: Default::default(),
            hln: Default::default(),
            free_zone_scratched: HashSet::new(),
            free_zone_autoplay: [false; LANE_COUNT],
            chart_index: None,
            legacy_inline_mines: false,
            legacy_note_current: 0,
            legacy_lane_buffers: [None; LANE_COUNT],
            legacy_read_past_lane_buffer: false,
        }
    }

    pub fn set_window_set(&mut self, window_set: JudgeWindows) {
        self.windows = window_set.note;
        self.window_set = window_set;
    }

    /// Overrides which chart lanes use the scratch press/LN windows. This is
    /// used when a source scratch is projected onto key lanes for 7K-to-9K.
    pub fn set_scratch_lane_mask(&mut self, scratch_lane_mask: [bool; LANE_COUNT]) {
        self.scratch_lane_mask = scratch_lane_mask;
    }

    /// Viewer の開始位置より前のノートを読み飛ばし、境界時刻のノートは残す。
    /// 開始位置をまたぐLN/CN/HCNはPGREAT始端として復元し、将来の終端入力を
    /// 通常どおり処理できる状態にする。
    pub fn skip_before(&mut self, chart: &PlayableChart, start_time: TimeUs) {
        self.judged_notes.clear();
        self.attempted_notes.clear();
        self.orphan_charge_ends = Default::default();
        self.hln = Default::default();
        self.free_zone_scratched.clear();
        self.legacy_note_current = 0;
        for lane in Lane::ALL {
            let next = chart.notes_for_lane(lane).partition_point(|note| note.time < start_time);
            self.lanes[lane.index()] = LaneJudgeState {
                next_note_index: next,
                next_mine_index: next,
                ..Default::default()
            };
        }
        for pair in &chart.long_notes {
            if pair.start_time >= start_time
                || pair.end_time < start_time
                || chart.is_dummy_long(pair)
            {
                continue;
            }
            let Some(active) =
                make_active_long(chart, pair.start_note_id, Judge::PGreat, TimeUs(0), start_time)
            else {
                continue;
            };
            self.judged_notes.insert(pair.start_note_id, Judge::PGreat);
            self.lanes[pair.lane.index()].active_long = Some(active);
        }
        self.restore_hln_seek(chart, start_time);
    }

    pub fn process_input(&mut self, chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        self.process_input_at(chart, input, input.time, false)
    }

    pub(crate) fn process_input_at(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        judge_time: TimeUs,
        play_miss_keysound: bool,
    ) -> JudgeOutcome {
        self.mark_free_zone_input(chart, input);
        let mut outcome = self.advance_hln(chart, input.time);
        // HLN の保持中にも他ノートを捕捉できる。保留取消の Press だけは既存の
        // active-long 経路で消費し、Mine も通常どおり判定する。
        if input.kind == InputKind::Press
            && self.lanes[input.lane.index()].active_long.is_some_and(|active| {
                active.mode == LongNoteMode::Hln && active.pending_release.is_none()
            })
        {
            self.lanes[input.lane.index()].active_long = None;
        }
        let timed_input = InputEvent { time: judge_time, ..input };
        let empty_slot = if self.rule_mode == RuleMode::Legacy {
            self.process_legacy_free_slot(
                chart,
                input.lane,
                input.kind == InputKind::Press,
                judge_time,
                input.time,
            )
        } else {
            None
        };
        let mut input_outcome = if let Some(empty_slot) = empty_slot {
            empty_slot
        } else {
            match input.kind {
                InputKind::Press => {
                    self.process_press(chart, timed_input, input.time, play_miss_keysound)
                }
                InputKind::Release => {
                    let mut expired = JudgeOutcome::default();
                    let mine = self.process_legacy_mine_head(
                        chart,
                        input.lane,
                        LegacyLaneKey::Release,
                        judge_time,
                        input.time,
                    );
                    if self.rule_mode == RuleMode::Legacy && mine.is_none() {
                        self.settle_missed_notes_in_lane(
                            chart,
                            input.lane,
                            input.time,
                            judge_time,
                            play_miss_keysound,
                            true,
                            &mut expired,
                        );
                    }
                    if let Some(mine) = mine {
                        // A mine at the lane head takes this poll (OpenLR2 returns).
                        mine
                    } else if expired.events.is_empty() {
                        self.process_release(chart, timed_input, input.time)
                    } else {
                        expired.consumed_input = true;
                        expired
                    }
                }
            }
        };
        if judge_time != input.time {
            input_outcome.stamp_judgement_time(input.time);
        }
        self.update_hln_input(chart, input, &mut input_outcome);
        append_outcome(&mut outcome, input_outcome);
        self.filter_free_zone_outcome(chart, &mut outcome);
        self.settle_free_zones(chart, input.time, &mut outcome);
        self.count_legacy_judgements(&outcome);
        outcome
    }

    pub fn process_misses(&mut self, chart: &PlayableChart, now: TimeUs) -> JudgeOutcome {
        self.process_misses_with_keysounds(chart, now, false)
    }

    /// Optionally plays only notes missed without input, rather than every POOR
    /// score event (a missed CN head also scores its tail immediately).
    pub fn process_misses_with_keysounds(
        &mut self,
        chart: &PlayableChart,
        now: TimeUs,
        play_keysound: bool,
    ) -> JudgeOutcome {
        self.process_misses_at(chart, now, |_| now, play_keysound)
    }

    /// Settle expired press-reference notes. A LEGACY press settles only the
    /// first expired note and consumes that press; frame maintenance settles
    /// every due note.
    ///
    /// LEGACY の HLN 始点はここで確定しない。`advance_hln()` が
    /// `bad_slow_us + 1µs` で保持状態を初期化するため、先に POOR を入れると
    /// 初期化条件 (`!judged_notes.contains_key`) が崩れる。
    fn settle_missed_notes_in_lane(
        &mut self,
        chart: &PlayableChart,
        lane: Lane,
        now: TimeUs,
        judge_time: TimeUs,
        play_keysound: bool,
        stop_after_first: bool,
        outcome: &mut JudgeOutcome,
    ) {
        // An unconsumed LEGACY inline mine blocks every later note of the lane.
        let mine_bound = self.legacy_pending_mine(chart, lane);
        let lane_state = &mut self.lanes[lane.index()];
        while let Some((idx, note)) = next_unjudged_press_reference_note(
            chart,
            lane,
            lane_state.next_note_index,
            &self.judged_notes,
        ) {
            if mine_bound.is_some_and(|mine| mine < idx) {
                break;
            }
            if self.rule_mode == RuleMode::Legacy && hln::is_hln_head(chart, note.id) {
                break;
            }
            // OpenLR2 never settles a passed single note once the cap is reached.
            if self.rule_mode == RuleMode::Legacy
                && note.kind == NoteKind::Tap
                && self.legacy_note_current + legacy_cap::legacy_scored(outcome)
                    >= chart.total_notes
            {
                break;
            }
            let windows = press_window(self.window_set, self.scratch_lane_mask[lane.index()]);
            let expired = if self.rule_mode == RuleMode::Legacy {
                legacy_note_expired(note, legacy_exact_ms(chart, note), judge_time, windows)
            } else {
                judge_time.0 > note.time.0 + windows.bad_slow_us
            };
            if !expired {
                break;
            }

            lane_state.next_note_index = idx + 1;
            let miss_was_already_scored = self.attempted_notes.remove(&note.id);
            self.judged_notes.insert(note.id, Judge::Poor);
            let miss_delta = TimeUs(judge_time.0 - note.time.0);
            if !miss_was_already_scored && play_keysound {
                outcome.keysounds.push(KeySoundEvent {
                    note_id: note.id,
                    time: now,
                    trigger: KeySoundTrigger::Miss,
                });
            }
            outcome.events.push(JudgementEvent {
                note_id: Some(note.id),
                lane,
                judge: Judge::Poor,
                side: TimingSide::Slow,
                delta: miss_delta,
                time: now,
                affects_score: !miss_was_already_scored,
                advances_score: true,
            });
            // beatoraja treats a missed CN/HCN head as two misses: both the
            // head and its paired tail become POOR immediately. If the head
            // had already produced a non-vanishing BAD, only the tail adds
            // another score event (MissCondition.ONE).
            if let Some(end_note_id) = missed_charge_end_for_start(chart, note.id) {
                self.judged_notes.insert(end_note_id, Judge::Poor);
                outcome.events.push(JudgementEvent {
                    note_id: Some(end_note_id),
                    lane,
                    judge: Judge::Poor,
                    side: TimingSide::Slow,
                    delta: miss_delta,
                    time: now,
                    affects_score: true,
                    advances_score: true,
                });
            }
            if stop_after_first {
                break;
            }
        }
        advance_press_cursor(chart, lane, &mut lane_state.next_note_index, &self.judged_notes);
    }

    pub(crate) fn process_misses_at(
        &mut self,
        chart: &PlayableChart,
        now: TimeUs,
        lane_judge_time: impl Fn(Lane) -> TimeUs,
        play_keysound: bool,
    ) -> JudgeOutcome {
        self.process_misses_at_filtered(chart, now, lane_judge_time, play_keysound, None)
    }

    /// One idle-lane poll, as ProcNoteOnTiming does when there is no key edge.
    pub fn process_legacy_idle_lane(
        &mut self,
        chart: &PlayableChart,
        lane: Lane,
        now: TimeUs,
        play_keysound: bool,
    ) -> JudgeOutcome {
        self.process_legacy_lane_poll(chart, lane, now, false, play_keysound)
    }

    /// One poll of a lane without a key edge. `held` is OpenLR2's
    /// `keypress == 2`; it matters only for a passing inline mine.
    pub fn process_legacy_lane_poll(
        &mut self,
        chart: &PlayableChart,
        lane: Lane,
        now: TimeUs,
        held: bool,
        play_keysound: bool,
    ) -> JudgeOutcome {
        if chart.notes_for_lane(lane).is_empty() {
            return JudgeOutcome::default();
        }
        if let Some(empty_slot) = self.process_legacy_free_slot(chart, lane, false, now, now) {
            self.count_legacy_judgements(&empty_slot);
            return empty_slot;
        }
        let key = if held { LegacyLaneKey::Hold } else { LegacyLaneKey::Idle };
        if let Some(mine) = self.process_legacy_mine_head(chart, lane, key, now, now) {
            return mine;
        }
        self.process_misses_at_filtered(chart, now, |_| now, play_keysound, Some(lane))
    }

    fn process_misses_at_filtered(
        &mut self,
        chart: &PlayableChart,
        now: TimeUs,
        lane_judge_time: impl Fn(Lane) -> TimeUs,
        play_keysound: bool,
        lane_filter: Option<Lane>,
    ) -> JudgeOutcome {
        let mut outcome = self.advance_hln(chart, now);

        let lanes = lane_filter.as_ref().map_or(Lane::ALL.as_slice(), std::slice::from_ref);
        for &lane in lanes {
            let judge_time = lane_judge_time(lane);
            self.settle_missed_notes_in_lane(
                chart,
                lane,
                now,
                judge_time,
                play_keysound,
                self.rule_mode == RuleMode::Legacy && lane_filter.is_some(),
                &mut outcome,
            );
            let lane_state = &mut self.lanes[lane.index()];

            let windows = press_window(self.window_set, self.scratch_lane_mask[lane.index()]);
            let orphan_ends = &mut self.orphan_charge_ends[lane.index()];
            while let Some(&(time, id)) = orphan_ends.first() {
                if judge_time.0 <= time.0 + windows.bad_slow_us {
                    break;
                }
                orphan_ends.pop_first();
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    self.judged_notes.entry(id)
                {
                    entry.insert(Judge::Poor);
                    outcome.events.push(JudgementEvent {
                        note_id: Some(id),
                        lane,
                        judge: Judge::Poor,
                        side: TimingSide::Slow,
                        delta: TimeUs(judge_time.0 - time.0),
                        time: now,
                        affects_score: true,
                        advances_score: true,
                    });
                }
            }

            if let Some(active) = lane_state.active_long {
                let release_margin_us =
                    long_release_margin_us(self.window_set, self.scratch_lane_mask[lane.index()]);
                if let Some(pending) = active.pending_release
                    && ((matches!(pending.judge, Judge::Bad | Judge::Poor)
                        && now.0 >= pending.released_at.0 + release_margin_us)
                        || (self.rule_mode == RuleMode::Beatoraja
                            && active.mode == LongNoteMode::Ln
                            && judge_time > active.end.end_time))
                {
                    lane_state.active_long = None;
                    let judged_note_id = active_scored_note_id(active);
                    self.judged_notes.insert(judged_note_id, pending.judge);
                    append_outcome(
                        &mut outcome,
                        finalize_long_release(
                            chart,
                            lane,
                            active,
                            pending.judge,
                            pending.delta,
                            pending.delta,
                            self.early_release_mute_limit(lane, pending.delta.0),
                            now,
                        ),
                    );
                    continue;
                }

                match active.mode {
                    LongNoteMode::Ln | LongNoteMode::Hln => {
                        let elapsed = if self.rule_mode == RuleMode::Legacy
                            && active.mode == LongNoteMode::Ln
                        {
                            legacy_offset(judge_time, active.end.end_time)
                        } else {
                            judge_time.0 - active.end.end_time.0
                        };
                        if elapsed > 0 {
                            lane_state.active_long = None;
                            self.judged_notes.insert(active.start_note_id, active.start_judge);
                            outcome.events.push(ln_final_event(
                                lane,
                                active,
                                active.start_judge,
                                active.start_delta,
                                now,
                            ));
                            // OpenLR2 finishes an LN without playing an end sound.
                            if !(self.rule_mode == RuleMode::Legacy
                                && active.mode == LongNoteMode::Ln)
                            {
                                outcome.keysounds.push(KeySoundEvent {
                                    note_id: active.end.end_note_id,
                                    time: now,
                                    trigger: KeySoundTrigger::NoteJudged,
                                });
                            }
                        }
                    }
                    LongNoteMode::Cn | LongNoteMode::Hcn => {
                        // beatoraja uses the normal note BAD-late boundary for
                        // an unreleased CN/HCN tail. The long-end window is only
                        // used when an actual Release input occurs.
                        let windows =
                            press_window(self.window_set, self.scratch_lane_mask[lane.index()]);
                        if judge_time.0 > active.end.end_time.0 + windows.bad_slow_us {
                            lane_state.active_long = None;
                            self.judged_notes.insert(active.end.end_note_id, Judge::Poor);
                            if play_keysound {
                                outcome.keysounds.push(KeySoundEvent {
                                    note_id: active.end.end_note_id,
                                    time: now,
                                    trigger: KeySoundTrigger::Miss,
                                });
                            }
                            outcome.events.push(JudgementEvent {
                                note_id: Some(active.end.end_note_id),
                                lane,
                                judge: Judge::Poor,
                                side: TimingSide::Slow,
                                delta: TimeUs(judge_time.0 - active.end.end_time.0),
                                time: now,
                                affects_score: true,
                                advances_score: true,
                            });
                        }
                    }
                }
            }
        }

        self.filter_free_zone_outcome(chart, &mut outcome);
        self.settle_free_zones(chart, now, &mut outcome);
        self.count_legacy_judgements(&outcome);
        outcome
    }

    pub fn process_mine_passes(
        &mut self,
        chart: &PlayableChart,
        now: TimeUs,
        lane_keyon_started_at: &[Option<TimeUs>; LANE_COUNT],
    ) -> JudgeOutcome {
        self.process_mine_passes_at(chart, now, None, lane_keyon_started_at)
    }

    pub(crate) fn process_mine_passes_at(
        &mut self,
        chart: &PlayableChart,
        now: TimeUs,
        reflected_time: Option<TimeUs>,
        lane_keyon_started_at: &[Option<TimeUs>; LANE_COUNT],
    ) -> JudgeOutcome {
        let mut outcome = JudgeOutcome::default();

        for lane in Lane::ALL {
            let lane_index = lane.index();
            let lane_state = &mut self.lanes[lane.index()];
            let notes = chart.notes_for_lane(lane);
            while let Some(note) = notes.get(lane_state.next_mine_index) {
                let mine_time = reflected_time.filter(|_| note.kind == NoteKind::Mine);
                let evaluation_time = mine_time.unwrap_or(now);
                if note.time > evaluation_time {
                    break;
                }
                lane_state.next_mine_index += 1;
                let Some(keyon_started_at) = lane_keyon_started_at[lane_index] else {
                    continue;
                };
                if note.kind == NoteKind::Potion && keyon_started_at <= note.time {
                    outcome.potion_hits.push(crate::judge::model::PotionHitEvent {
                        note_id: note.id,
                        lane,
                        time: note.time,
                        recovery: note.damage.unwrap_or_default(),
                    });
                    continue;
                }
                if note.kind != NoteKind::Mine
                    || keyon_started_at > if mine_time.is_some() { now } else { note.time }
                    || (lane_state.scratch_press_absorbed_at == Some(keyon_started_at)
                        && keyon_started_at == if mine_time.is_some() { now } else { note.time })
                {
                    continue;
                }

                outcome.mine_hits.push(MineHitEvent {
                    note_id: note.id,
                    lane,
                    damage: note.damage.unwrap_or(0.0),
                    sound: note.sound,
                    time: if mine_time.is_some() { now } else { note.time },
                });
            }
        }

        outcome
    }

    pub fn is_exhausted(&self, chart: &PlayableChart) -> bool {
        self.hln.is_exhausted()
            && self.orphan_charge_ends.iter().all(BTreeSet::is_empty)
            && chart.metadata.free_zones.iter().all(|z| self.judged_notes.contains_key(&z.note_id))
            && Lane::ALL.iter().copied().all(|lane| {
                let state = &self.lanes[lane.index()];
                state.active_long.is_none()
                    && next_unjudged_press_reference_note(
                        chart,
                        lane,
                        state.next_note_index,
                        &self.judged_notes,
                    )
                    .is_none()
            })
    }

    /// A release-judged BSS still produces the reverse Press used by physical scratch
    /// controllers. Consumes only that first matching edge, and disarms the one-shot
    /// suppression on any other Press of the lane, exactly as the scored path does.
    ///
    /// The unscored path after the result settles uses this too, so the reverse edge
    /// stays silent instead of falling through to a fallback keysound.
    pub fn consume_scratch_press_suppression(&mut self, input: InputEvent) -> bool {
        self.lanes[input.lane.index()].scratch_press_suppression.take().is_some_and(|suppression| {
            input.time.0 >= suppression.started_at.0
                && input.time.0 <= suppression.expires_at.0
                && input.scratch_direction == Some(suppression.direction)
        })
    }

    fn process_press(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        event_time: TimeUs,
        play_miss_keysound: bool,
    ) -> JudgeOutcome {
        let mut outcome = JudgeOutcome::default();
        if let Some(mine) = self.process_legacy_mine_head(
            chart,
            input.lane,
            LegacyLaneKey::Press,
            input.time,
            event_time,
        ) {
            return mine;
        }
        if self.rule_mode == RuleMode::Legacy {
            // OpenLR2 consumes this invocation after one expired POOR.
            // Only a tap BAD continues to the next note with the same press.
            self.settle_missed_notes_in_lane(
                chart,
                input.lane,
                input.time,
                input.time,
                play_miss_keysound,
                true,
                &mut outcome,
            );
            if !outcome.events.is_empty() {
                outcome.consumed_input = true;
                return outcome;
            }
        }
        let first = self.process_press_once(chart, input, event_time);
        append_outcome(&mut outcome, first);
        if self.rule_mode != RuleMode::Legacy {
            return outcome;
        }

        // A consumed tap BAD can expose another note within the same press window.
        let mut recurse = outcome.events.last().is_some_and(|event| {
            event.judge == Judge::Bad
                && event
                    .note_id
                    .and_then(|id| self.note_by_id(chart, id))
                    .is_some_and(|note| note.kind == NoteKind::Tap)
        });
        while recurse {
            let pending = legacy_cap::legacy_scored(&outcome);
            let next = self.process_legacy_recursive_single(chart, input, event_time, pending);
            // A BAD on a free slot already continued through the free slots.
            recurse = next
                .events
                .last()
                .is_some_and(|event| event.judge == Judge::Bad && event.note_id.is_some());
            append_outcome(&mut outcome, next);
        }
        outcome
    }

    /// ProcSinglenote recurses directly into itself after BAD. It does not
    /// redispatch through ProcNoteOnTiming, even when the next object is an LN.
    fn process_legacy_recursive_single(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        event_time: TimeUs,
        pending: u32,
    ) -> JudgeOutcome {
        if let Some(mine) = self.process_legacy_recursive_mine(chart, input, event_time) {
            return mine;
        }
        let state = &self.lanes[input.lane.index()];
        let Some((index, note)) = next_unjudged_press_reference_note(
            chart,
            input.lane,
            state.next_note_index,
            &self.judged_notes,
        ) else {
            return self
                .recurse_into_legacy_free_slot(chart, input, event_time, pending)
                .unwrap_or_default();
        };
        if self.legacy_capped(chart, pending) {
            return JudgeOutcome::default();
        }
        let mut single = note.clone();
        single.kind = NoteKind::Tap;
        let windows = press_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
        let Some(candidate) =
            legacy_press_candidate(&single, legacy_exact_ms(chart, note), input.time, windows)
                .filter(|candidate| candidate.judge != Judge::EmptyPoor)
        else {
            return JudgeOutcome::default();
        };
        self.judged_notes.insert(note.id, candidate.judge);
        self.lanes[input.lane.index()].next_note_index = index + 1;
        self.lanes[input.lane.index()].last_press_time = Some(note.time);
        JudgeOutcome {
            consumed_input: true,
            events: vec![JudgementEvent {
                note_id: Some(note.id),
                lane: input.lane,
                judge: candidate.judge,
                side: side_from_delta(candidate.delta.0),
                delta: candidate.delta,
                time: event_time,
                affects_score: true,
                advances_score: true,
            }],
            keysounds: vec![KeySoundEvent {
                note_id: note.id,
                time: event_time,
                trigger: KeySoundTrigger::NoteJudged,
            }],
            ..Default::default()
        }
    }

    /// LEGACY の LN 始点見逃し期限と早離し許容に使う幅。それ以外のルールでは
    /// 従来どおり「終点より前なら早離し」として扱う。
    fn early_release_mute_limit(&self, lane: Lane, delta_us: i64) -> i64 {
        if self.rule_mode == RuleMode::Legacy {
            legacy_ln_margin_us(long_end_window(
                self.window_set,
                self.scratch_lane_mask[lane.index()],
                delta_us,
            ))
        } else {
            0
        }
    }

    fn process_press_once(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        event_time: TimeUs,
    ) -> JudgeOutcome {
        // Consume the BSS reverse edge before it can reach a note or Mine.
        if self.consume_scratch_press_suppression(InputEvent { time: event_time, ..input }) {
            self.lanes[input.lane.index()].scratch_press_absorbed_at = Some(event_time);
            return JudgeOutcome { consumed_input: true, ..Default::default() };
        }
        self.lanes[input.lane.index()].scratch_press_absorbed_at = None;

        if let Some(mut active) = self.lanes[input.lane.index()].active_long {
            if self.rule_mode == RuleMode::Legacy && active.mode == LongNoteMode::Ln {
                let head_time = chart.long_notes[active.pair_index].start_time;
                let delta = TimeUs(legacy_offset(input.time, head_time));
                let windows =
                    press_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
                if let Some(grade) = classify_normal_delta(delta.0, windows) {
                    // ProcLongnote evaluates a new key edge even while active;
                    // it overwrites the provisional head grade, not the score.
                    active.start_judge = grade;
                    active.start_delta = delta;
                    self.judged_notes.insert(active.start_note_id, grade);
                    self.lanes[input.lane.index()].active_long = Some(active);
                    return JudgeOutcome {
                        consumed_input: true,
                        events: vec![JudgementEvent {
                            note_id: Some(active.start_note_id),
                            lane: input.lane,
                            judge: grade,
                            side: side_from_delta(delta.0),
                            delta,
                            time: event_time,
                            affects_score: false,
                            advances_score: false,
                        }],
                        keysounds: vec![KeySoundEvent {
                            note_id: active.start_note_id,
                            time: event_time,
                            trigger: KeySoundTrigger::NoteJudged,
                        }],
                        ..Default::default()
                    };
                }
                if -delta.0 < windows.empty_poor_fast_us {
                    return empty_poor(
                        input.lane,
                        TimingSide::Fast,
                        delta,
                        event_time,
                        active.start_note_id,
                        Vec::new(),
                    );
                }
                return JudgeOutcome::default();
            }
            if matches!(input.lane, Lane::Scratch | Lane::Scratch2)
                && matches!(active.mode, LongNoteMode::Cn | LongNoteMode::Hcn)
                && input.scratch_direction.is_some()
                && active.scratch_direction.is_some()
                && input.scratch_direction != active.scratch_direction
            {
                let delta = input.time.0 - active.end.end_time.0;
                let windows = long_end_window(
                    self.window_set,
                    self.scratch_lane_mask[input.lane.index()],
                    delta,
                );
                let judge = classify_normal_delta(delta, windows).unwrap_or(Judge::Poor);
                self.lanes[input.lane.index()].active_long = None;
                self.judged_notes.insert(active.end.end_note_id, judge);
                let mut outcome = finalize_long_release(
                    chart,
                    input.lane,
                    active,
                    judge,
                    TimeUs(delta),
                    TimeUs(delta),
                    0,
                    input.time,
                );
                outcome.consumed_input = true;
                return outcome;
            }
            if active.pending_release.take().is_some() {
                self.lanes[input.lane.index()].active_long = Some(active);
                return JudgeOutcome { consumed_input: true, ..Default::default() };
            }
            return JudgeOutcome::default();
        }

        let rule_mode = self.rule_mode;
        let windows = press_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
        let candidate = if rule_mode == RuleMode::Legacy {
            let capped = self.legacy_capped(chart, 0);
            next_unjudged_press_reference_note(
                chart,
                input.lane,
                self.lanes[input.lane.index()].next_note_index,
                &self.judged_notes,
            )
            .filter(|(_, note)| !(capped && note.kind == NoteKind::Tap))
            .and_then(|(_, note)| {
                legacy_press_candidate(note, legacy_exact_ms(chart, note), input.time, windows)
            })
        } else {
            select_press_candidate(self, chart, input.lane, input.time, windows)
        };
        let Some(candidate) = candidate else {
            return JudgeOutcome::default();
        };

        let note_id = candidate.note_id;
        let side = side_from_delta(candidate.delta.0);
        // A regrab can restore the HCN body after its tail has already failed.
        if candidate.judge == Judge::EmptyPoor
            && self.hcn_pairs_at(chart, input.time).any(|pair| {
                pair.lane == input.lane
                    && pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hcn
                    && pair.start_time <= input.time
                    && input.time < pair.end_time
                    && !chart.is_dummy_long(pair)
            })
        {
            return JudgeOutcome { consumed_input: true, ..Default::default() };
        }
        let Some(note) = self.note_by_id(chart, note_id) else {
            debug_assert!(false, "candidate note {note_id:?} must exist in chart");
            return JudgeOutcome::default();
        };
        let mut events = Vec::new();
        if matches!(rule_mode, RuleMode::Lr2Oraja | RuleMode::Dx) {
            for extra in lr2oraja_multi_bad_candidates(
                chart,
                input.lane,
                input.time,
                windows,
                note,
                candidate,
                &self.judged_notes,
            ) {
                let hln = hln::is_hln_head(chart, extra.note_id);
                if hln
                    && !chart.long_notes.iter().any(|pair| {
                        pair.start_note_id == extra.note_id && input.time >= pair.end_time
                    })
                {
                    continue;
                }
                if self.bad_judge_vanish || hln {
                    self.judged_notes.insert(extra.note_id, Judge::Bad);
                    if let Some(pair) = charge_pair_for_start(chart, extra.note_id) {
                        self.orphan_charge_ends[input.lane.index()]
                            .insert((pair.end_time, pair.end_note_id));
                    }
                } else {
                    self.attempted_notes.insert(extra.note_id);
                }
                events.push(JudgementEvent {
                    note_id: Some(extra.note_id),
                    lane: input.lane,
                    judge: Judge::Bad,
                    side: side_from_delta(extra.delta.0),
                    delta: extra.delta,
                    time: input.time,
                    affects_score: !hln,
                    advances_score: self.bad_judge_vanish && !hln,
                });
            }
        }
        if candidate.judge == Judge::EmptyPoor {
            if !self.bad_judge_vanish && candidate.delta.0 != 0 {
                self.attempted_notes.insert(note_id);
            }
            return empty_poor(input.lane, side, candidate.delta, input.time, note_id, events);
        }
        let note_vanishes = candidate.judge != Judge::Bad
            || self.bad_judge_vanish
            || rule_mode == RuleMode::Legacy
            || hln::is_hln_head(chart, note.id);
        let lane_state = &mut self.lanes[input.lane.index()];
        lane_state.last_press_time = Some(note.time);
        if note_vanishes {
            self.attempted_notes.remove(&note.id);
            self.judged_notes.insert(note.id, candidate.judge);
        } else {
            self.attempted_notes.insert(note.id);
        }

        if note_vanishes
            && note.kind == NoteKind::LongStart
            && let Some(mut active) =
                make_active_long(chart, note.id, candidate.judge, candidate.delta, event_time)
        {
            if matches!(input.lane, Lane::Scratch | Lane::Scratch2) {
                active.scratch_direction = input.scratch_direction;
            }
            lane_state.active_long = Some(active);
        }
        advance_press_cursor(
            chart,
            input.lane,
            &mut lane_state.next_note_index,
            &self.judged_notes,
        );
        events.push(JudgementEvent {
            note_id: Some(note_id),
            lane: input.lane,
            judge: candidate.judge,
            side,
            delta: candidate.delta,
            time: input.time,
            affects_score: !note_vanishes
                || note.kind != NoteKind::LongStart
                || active_long_scores_on_start(chart, note.id),
            advances_score: note_vanishes
                && (note.kind != NoteKind::LongStart
                    || active_long_scores_on_start(chart, note.id)),
        });

        JudgeOutcome {
            events,
            keysounds: vec![KeySoundEvent {
                note_id,
                time: input.time,
                trigger: KeySoundTrigger::NoteJudged,
            }],
            consumed_input: true,
            ..Default::default()
        }
    }

    fn process_release(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        event_time: TimeUs,
    ) -> JudgeOutcome {
        let lane_state = &mut self.lanes[input.lane.index()];
        let Some(mut active) = lane_state.active_long else {
            return JudgeOutcome::default();
        };
        if active.mode == LongNoteMode::Hln {
            return JudgeOutcome::default(); // pair ごとの LN 合成を update_hln_input で行う。
        }
        // LEGACY merges both scratch directions into one key state before
        // judging (as OpenLR2 does), so its release ends the LN whichever
        // direction started it.
        let merged_scratch = self.rule_mode == RuleMode::Legacy && active.mode == LongNoteMode::Ln;
        if matches!(input.lane, Lane::Scratch | Lane::Scratch2)
            && !merged_scratch
            && input.scratch_direction.is_some()
            && active.scratch_direction.is_some()
            && input.scratch_direction != active.scratch_direction
        {
            lane_state.active_long = Some(active);
            return JudgeOutcome::default();
        }
        if matches!(input.lane, Lane::Scratch | Lane::Scratch2)
            && matches!(active.mode, LongNoteMode::Cn | LongNoteMode::Hcn)
            && let Some(start_direction) = active.scratch_direction
            && input.scratch_direction == Some(start_direction)
        {
            // BMZ accepts stopping the scratch as a BSS tail input. Arm a short one-shot
            // suppression so the customary reverse edge cannot BAD a following scratch note.
            let delta = input.time.0 - active.end.end_time.0;
            let windows =
                long_end_window(self.window_set, self.scratch_lane_mask[input.lane.index()], delta);
            if let Some(judge) = classify_normal_delta(delta, windows) {
                let reverse_direction = match start_direction {
                    ScratchDirection::Up => ScratchDirection::Down,
                    ScratchDirection::Down => ScratchDirection::Up,
                };
                lane_state.active_long = None;
                lane_state.scratch_press_suppression = Some(ScratchPressSuppression {
                    direction: reverse_direction,
                    started_at: event_time,
                    expires_at: TimeUs(
                        event_time.0.saturating_add(BSS_REVERSE_PRESS_SUPPRESSION_US),
                    ),
                });
                self.judged_notes.insert(active.end.end_note_id, judge);
                return finalize_long_release(
                    chart,
                    input.lane,
                    active,
                    judge,
                    TimeUs(delta),
                    TimeUs(delta),
                    0,
                    input.time,
                );
            }
        }
        let release_margin_us =
            long_release_margin_us(self.window_set, self.scratch_lane_mask[input.lane.index()]);

        match active.mode {
            LongNoteMode::Ln | LongNoteMode::Hln => {
                let end_delta = TimeUs(input.time.0 - active.end.end_time.0);
                let end_window = long_end_window(
                    self.window_set,
                    self.scratch_lane_mask[input.lane.index()],
                    end_delta.0,
                );
                let (judge, delta, release_delta, mute_limit) =
                    if self.rule_mode == RuleMode::Legacy && active.mode == LongNoteMode::Ln {
                        let margin = legacy_ln_margin_us(end_window);
                        let release = TimeUs(legacy_offset(input.time, active.end.end_time));
                        let grade =
                            if release.0 < -margin { Judge::Bad } else { active.start_judge };
                        // LR2 reports the head offset with the final judgement.
                        // It stops the head keysound only in the early-release
                        // branch, decided on the same integer milliseconds.
                        (grade, active.start_delta, release, margin)
                    } else if end_delta.0 >= 0 {
                        (active.start_judge, active.start_delta, active.start_delta, 0)
                    } else {
                        let end_judge =
                            classify_normal_delta(end_delta.0, end_window).unwrap_or(Judge::Poor);
                        let (judge, delta) = combine_ln_judgement(active, end_judge, end_delta);
                        (judge, delta, delta, 0)
                    };
                // beatoraja defers an early LN release until the tail, so a regrab
                // before the tail restores the start judgement.
                let beatoraja_defer = self.rule_mode == RuleMode::Beatoraja
                    && active.mode == LongNoteMode::Ln
                    && end_delta.0 < 0
                    && delta.0 < 0
                    && !matches!(judge, Judge::Bad | Judge::Poor);
                if beatoraja_defer {
                    active.pending_release = Some(PendingLongRelease {
                        released_at: event_time,
                        judge,
                        delta: end_delta,
                    });
                    lane_state.active_long = Some(active);
                    return JudgeOutcome { consumed_input: true, ..Default::default() };
                }
                if release_margin_us > 0
                    && end_delta.0 < 0
                    && matches!(judge, Judge::Bad | Judge::Poor)
                {
                    active.pending_release =
                        Some(PendingLongRelease { released_at: event_time, judge, delta });
                    lane_state.active_long = Some(active);
                    return JudgeOutcome { consumed_input: true, ..Default::default() };
                }
                lane_state.active_long = None;
                self.judged_notes.insert(active.start_note_id, judge);
                let mut outcome = finalize_long_release(
                    chart,
                    input.lane,
                    active,
                    judge,
                    delta,
                    release_delta,
                    mute_limit,
                    input.time,
                );
                if self.rule_mode == RuleMode::Legacy && active.mode == LongNoteMode::Ln {
                    // OpenLR2 finishes an LN without playing an end sound.
                    outcome.keysounds.retain(|sound| sound.note_id != active.end.end_note_id);
                }
                outcome
            }
            LongNoteMode::Cn | LongNoteMode::Hcn => {
                let delta = input.time.0 - active.end.end_time.0;
                let windows = long_end_window(
                    self.window_set,
                    self.scratch_lane_mask[input.lane.index()],
                    delta,
                );
                let judge = classify_normal_delta(delta, windows).unwrap_or(Judge::Poor);
                if release_margin_us > 0 && delta < 0 && matches!(judge, Judge::Bad | Judge::Poor) {
                    active.pending_release = Some(PendingLongRelease {
                        released_at: event_time,
                        judge,
                        delta: TimeUs(delta),
                    });
                    lane_state.active_long = Some(active);
                    return JudgeOutcome { consumed_input: true, ..Default::default() };
                }
                lane_state.active_long = None;
                self.judged_notes.insert(active.end.end_note_id, judge);
                finalize_long_release(
                    chart,
                    input.lane,
                    active,
                    judge,
                    TimeUs(delta),
                    TimeUs(delta),
                    0,
                    input.time,
                )
            }
        }
    }
}

mod helpers;
mod hln;
mod kaleid;
mod legacy_cap;
mod legacy_mine;

use helpers::*;
pub(crate) use legacy_mine::LegacyLaneKey;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
