use std::collections::{HashMap, HashSet};

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

const BSS_REVERSE_PRESS_SUPPRESSION_US: i64 = 30_000;

fn press_window(window_set: JudgeWindows, scratch: bool) -> JudgeWindow {
    if scratch { window_set.scratch } else { window_set.note }
}

fn long_end_window(window_set: JudgeWindows, scratch: bool) -> JudgeWindow {
    if scratch { window_set.long_scratch_end } else { window_set.long_note_end }
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
    pub lanes: [LaneJudgeState; LANE_COUNT],
    pub judged_notes: HashMap<NoteId, Judge>,
    scratch_lane_mask: [bool; LANE_COUNT],
    bad_attempted_notes: HashSet<NoteId>,
    bad_judge_vanish: bool,
}

impl JudgeEngine {
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
            lanes: [LaneJudgeState::default(); LANE_COUNT],
            judged_notes: HashMap::new(),
            scratch_lane_mask,
            bad_attempted_notes: HashSet::new(),
            bad_judge_vanish: bad_judge_vanish_for_keymode_and_rule_mode(key_mode, rule_mode),
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
        self.bad_attempted_notes.clear();
        for lane in Lane::ALL {
            let next = chart.notes_for_lane(lane).partition_point(|note| note.time < start_time);
            self.lanes[lane.index()] = LaneJudgeState {
                next_note_index: next,
                next_mine_index: next,
                ..Default::default()
            };
        }
        for pair in &chart.long_notes {
            if pair.start_time >= start_time || pair.end_time < start_time {
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

            while let Some((idx, note)) = next_unjudged_press_reference_note(
                chart,
                lane,
                lane_state.next_note_index,
                &self.judged_notes,
            ) {
                let windows = press_window(self.window_set, self.scratch_lane_mask[lane.index()]);
                if now.0 <= note.time.0 + windows.bad_slow_us {
                    break;
                }

                lane_state.next_note_index = idx + 1;
                let bad_was_already_scored = self.bad_attempted_notes.remove(&note.id);
                self.judged_notes.insert(note.id, Judge::Poor);
                let miss_delta = TimeUs(now.0 - note.time.0);
                if !bad_was_already_scored {
                    outcome.events.push(JudgementEvent {
                        note_id: Some(note.id),
                        lane,
                        judge: Judge::Poor,
                        side: TimingSide::Slow,
                        delta: miss_delta,
                        time: now,
                        affects_score: true,
                    });
                }
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
                    });
                }
            }
            advance_press_cursor(chart, lane, &mut lane_state.next_note_index, &self.judged_notes);

            if let Some(active) = lane_state.active_long {
                let release_margin_us =
                    long_release_margin_us(self.window_set, self.scratch_lane_mask[lane.index()]);
                if let Some(pending) = active.pending_release
                    && now.0 >= pending.released_at.0 + release_margin_us
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
                            now,
                        ),
                    );
                    continue;
                }

                match active.mode {
                    LongNoteMode::Ln => {
                        if now.0 > active.end.end_time.0 {
                            lane_state.active_long = None;
                            self.judged_notes.insert(active.start_note_id, active.start_judge);
                            outcome.events.push(ln_final_event(
                                lane,
                                active,
                                active.start_judge,
                                active.start_delta,
                                now,
                            ));
                            outcome.keysounds.push(KeySoundEvent {
                                note_id: active.end.end_note_id,
                                time: now,
                                trigger: KeySoundTrigger::NoteJudged,
                            });
                        }
                    }
                    LongNoteMode::Cn | LongNoteMode::Hcn => {
                        // beatoraja uses the normal note BAD-late boundary for
                        // an unreleased CN/HCN tail. The long-end window is only
                        // used when an actual Release input occurs.
                        let windows =
                            press_window(self.window_set, self.scratch_lane_mask[lane.index()]);
                        if now.0 > active.end.end_time.0 + windows.bad_slow_us {
                            lane_state.active_long = None;
                            self.judged_notes.insert(active.end.end_note_id, Judge::Poor);
                            outcome.events.push(JudgementEvent {
                                note_id: Some(active.end.end_note_id),
                                lane,
                                judge: Judge::Poor,
                                side: TimingSide::Slow,
                                delta: TimeUs(now.0 - active.end.end_time.0),
                                time: now,
                                affects_score: true,
                            });
                        }
                    }
                }
            }
        }

        outcome
    }

    pub fn process_mine_passes(
        &mut self,
        chart: &PlayableChart,
        now: TimeUs,
        lane_keyon_started_at: &[Option<TimeUs>; LANE_COUNT],
    ) -> JudgeOutcome {
        let mut outcome = JudgeOutcome::default();

        for lane in Lane::ALL {
            let lane_index = lane.index();
            let lane_state = &mut self.lanes[lane.index()];
            let notes = chart.notes_for_lane(lane);
            while let Some(note) = notes.get(lane_state.next_mine_index) {
                if note.time > now {
                    break;
                }
                lane_state.next_mine_index += 1;
                let Some(keyon_started_at) = lane_keyon_started_at[lane_index] else {
                    continue;
                };
                if note.kind != NoteKind::Mine
                    || keyon_started_at > note.time
                    || Some(note.time) == lane_state.last_mine_hit_time
                {
                    continue;
                }

                lane_state.last_mine_hit_time = Some(note.time);
                outcome.mine_hits.push(MineHitEvent {
                    note_id: note.id,
                    lane,
                    damage: note.damage.unwrap_or(0.0),
                    sound: note.sound,
                    time: note.time,
                });
            }
        }

        outcome
    }

    pub fn is_exhausted(&self, chart: &PlayableChart) -> bool {
        Lane::ALL.iter().copied().all(|lane| {
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

    fn process_press(&mut self, chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        // A release-judged BSS still produces the reverse Press used by physical scratch
        // controllers. Consume only that first matching edge before it can reach a note or Mine.
        if let Some(suppression) = self.lanes[input.lane.index()].scratch_press_suppression.take()
            && input.time.0 >= suppression.started_at.0
            && input.time.0 <= suppression.expires_at.0
            && input.scratch_direction == Some(suppression.direction)
        {
            return JudgeOutcome { consumed_input: true, ..Default::default() };
        }

        // Mine ヒット判定は通常ノーツの判定に先んじて、もしくは並走して行う。
        // 入力は通常ノーツの判定を妨げないので、ここでは別ベクタに積むだけ。
        let mut mine_hits = Vec::new();
        if let Some(hit) = detect_mine_hit(
            chart,
            input.lane,
            input.time,
            press_window(self.window_set, self.scratch_lane_mask[input.lane.index()]).mine_hit_us,
            &self.lanes[input.lane.index()],
        ) {
            self.lanes[input.lane.index()].last_mine_hit_time = Some(hit.time);
            mine_hits.push(hit);
        }

        if let Some(mut active) = self.lanes[input.lane.index()].active_long {
            if matches!(input.lane, Lane::Scratch | Lane::Scratch2)
                && matches!(active.mode, LongNoteMode::Cn | LongNoteMode::Hcn)
                && input.scratch_direction.is_some()
                && active.scratch_direction.is_some()
                && input.scratch_direction != active.scratch_direction
            {
                let delta = input.time.0 - active.end.end_time.0;
                let windows =
                    long_end_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
                let judge = classify_normal_delta(delta, windows).unwrap_or(Judge::Poor);
                self.lanes[input.lane.index()].active_long = None;
                self.judged_notes.insert(active.end.end_note_id, judge);
                let mut outcome = finalize_long_release(
                    chart,
                    input.lane,
                    active,
                    judge,
                    TimeUs(delta),
                    input.time,
                );
                outcome.mine_hits = mine_hits;
                outcome.consumed_input = true;
                return outcome;
            }
            if active.pending_release.take().is_some() {
                self.lanes[input.lane.index()].active_long = Some(active);
                return JudgeOutcome { mine_hits, consumed_input: true, ..Default::default() };
            }
            return JudgeOutcome { mine_hits, ..Default::default() };
        }

        let rule_mode = self.rule_mode;
        let windows = press_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
        let candidate = select_press_candidate(
            chart,
            input.lane,
            input.time,
            windows,
            rule_mode,
            self.algorithm,
            &self.judged_notes,
            &self.bad_attempted_notes,
        );
        let Some(candidate) = candidate else {
            return JudgeOutcome { mine_hits, ..Default::default() };
        };

        if candidate.consumes_note {
            // candidate 生成側の不変条件が崩れてもプレイ中に panic せず、
            // その入力の判定だけを捨てる (debug build では検知する)。
            let Some(note_id) = candidate.note_id else {
                debug_assert!(false, "normal candidate must have note id");
                return JudgeOutcome { mine_hits, ..Default::default() };
            };
            let Some(note) = chart.note_by_id(note_id) else {
                debug_assert!(false, "candidate note {note_id:?} must exist in chart");
                return JudgeOutcome { mine_hits, ..Default::default() };
            };
            let note_vanishes = candidate.judge != Judge::Bad || self.bad_judge_vanish;
            let multi_bad_candidates = if matches!(rule_mode, RuleMode::Lr2Oraja | RuleMode::Dx) {
                lr2oraja_multi_bad_candidates(
                    chart,
                    input.lane,
                    input.time,
                    windows,
                    note,
                    candidate,
                    &self.judged_notes,
                )
            } else {
                Vec::new()
            };

            let lane_state = &mut self.lanes[input.lane.index()];
            lane_state.last_press_time = Some(note.time);
            for multi_bad in &multi_bad_candidates {
                if self.bad_judge_vanish {
                    self.judged_notes.insert(multi_bad.note_id, Judge::Bad);
                } else {
                    self.bad_attempted_notes.insert(multi_bad.note_id);
                }
            }
            if note_vanishes {
                self.bad_attempted_notes.remove(&note.id);
                self.judged_notes.insert(note.id, candidate.judge);
            } else {
                self.bad_attempted_notes.insert(note.id);
            }

            if note_vanishes
                && note.kind == NoteKind::LongStart
                && let Some(mut active) =
                    make_active_long(chart, note.id, candidate.judge, candidate.delta, input.time)
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

            let mut events = Vec::with_capacity(multi_bad_candidates.len() + 1);
            events.extend(multi_bad_candidates.into_iter().map(|multi_bad| JudgementEvent {
                note_id: Some(multi_bad.note_id),
                lane: input.lane,
                judge: Judge::Bad,
                side: side_from_delta(multi_bad.delta.0),
                delta: multi_bad.delta,
                time: input.time,
                affects_score: true,
            }));
            events.push(JudgementEvent {
                note_id: Some(note_id),
                lane: input.lane,
                judge: candidate.judge,
                side: candidate.side,
                delta: candidate.delta,
                time: input.time,
                affects_score: note.kind != NoteKind::LongStart
                    || active_long_scores_on_start(chart, note.id),
            });

            return JudgeOutcome {
                events,
                keysounds: vec![KeySoundEvent {
                    note_id,
                    time: input.time,
                    trigger: KeySoundTrigger::NoteJudged,
                }],
                mine_hits,
                consumed_input: true,
                ..Default::default()
            };
        }

        let Some(keysound_note_id) = candidate.keysound_note_id else {
            debug_assert!(false, "empty poor candidate must have key sound note id");
            return JudgeOutcome { mine_hits, ..Default::default() };
        };
        let mut outcome =
            empty_poor(input.lane, candidate.side, candidate.delta, input.time, keysound_note_id);
        outcome.mine_hits = mine_hits;
        outcome
    }

    fn process_release(&mut self, chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        let lane_state = &mut self.lanes[input.lane.index()];
        let Some(mut active) = lane_state.active_long else {
            return JudgeOutcome::default();
        };
        if matches!(input.lane, Lane::Scratch | Lane::Scratch2)
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
                long_end_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
            if let Some(judge) = classify_normal_delta(delta, windows) {
                let reverse_direction = match start_direction {
                    ScratchDirection::Up => ScratchDirection::Down,
                    ScratchDirection::Down => ScratchDirection::Up,
                };
                lane_state.active_long = None;
                lane_state.scratch_press_suppression = Some(ScratchPressSuppression {
                    direction: reverse_direction,
                    started_at: input.time,
                    expires_at: TimeUs(
                        input.time.0.saturating_add(BSS_REVERSE_PRESS_SUPPRESSION_US),
                    ),
                });
                self.judged_notes.insert(active.end.end_note_id, judge);
                return finalize_long_release(
                    chart,
                    input.lane,
                    active,
                    judge,
                    TimeUs(delta),
                    input.time,
                );
            }
        }
        let release_margin_us =
            long_release_margin_us(self.window_set, self.scratch_lane_mask[input.lane.index()]);

        match active.mode {
            LongNoteMode::Ln => {
                let end_delta = TimeUs(input.time.0 - active.end.end_time.0);
                let (judge, delta) = if end_delta.0 >= 0 {
                    (active.start_judge, active.start_delta)
                } else {
                    let windows = long_end_window(
                        self.window_set,
                        self.scratch_lane_mask[input.lane.index()],
                    );
                    let end_judge =
                        classify_normal_delta(end_delta.0, windows).unwrap_or(Judge::Poor);
                    combine_ln_judgement(active, end_judge, end_delta)
                };
                if release_margin_us > 0
                    && end_delta.0 < 0
                    && matches!(judge, Judge::Bad | Judge::Poor)
                {
                    active.pending_release =
                        Some(PendingLongRelease { released_at: input.time, judge, delta });
                    lane_state.active_long = Some(active);
                    return JudgeOutcome { consumed_input: true, ..Default::default() };
                }
                lane_state.active_long = None;
                self.judged_notes.insert(active.start_note_id, judge);
                finalize_long_release(chart, input.lane, active, judge, delta, input.time)
            }
            LongNoteMode::Cn | LongNoteMode::Hcn => {
                let delta = input.time.0 - active.end.end_time.0;
                let windows =
                    long_end_window(self.window_set, self.scratch_lane_mask[input.lane.index()]);
                let judge = classify_normal_delta(delta, windows).unwrap_or(Judge::Poor);
                if release_margin_us > 0 && delta < 0 && matches!(judge, Judge::Bad | Judge::Poor) {
                    active.pending_release = Some(PendingLongRelease {
                        released_at: input.time,
                        judge,
                        delta: TimeUs(delta),
                    });
                    lane_state.active_long = Some(active);
                    return JudgeOutcome { consumed_input: true, ..Default::default() };
                }
                lane_state.active_long = None;
                self.judged_notes.insert(active.end.end_note_id, judge);
                finalize_long_release(chart, input.lane, active, judge, TimeUs(delta), input.time)
            }
        }
    }
}

mod helpers;

use helpers::*;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
