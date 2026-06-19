use std::collections::HashMap;

use bmz_chart::model::{LongNoteMode, NoteEvent, NoteKind, PlayableChart};
use bmz_core::ids::NoteId;
use bmz_core::input::{InputEvent, InputKind};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use super::model::{
    ActiveLongNote, JudgeAlgorithm, JudgeOutcome, JudgeWindow, JudgeWindows, JudgementEvent,
    KeySoundEvent, LaneJudgeState, LongNoteEndRef, MineHitEvent,
};
use crate::rule::RuleMode;

#[derive(Debug, Clone)]
pub struct JudgeEngine {
    pub windows: JudgeWindow,
    pub window_set: JudgeWindows,
    pub rule_mode: RuleMode,
    pub algorithm: JudgeAlgorithm,
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
        Self::new_with_window_set_and_algorithm(window_set, rule_mode, JudgeAlgorithm::Combo)
    }

    pub fn new_with_window_set_and_algorithm(
        window_set: JudgeWindows,
        rule_mode: RuleMode,
        algorithm: JudgeAlgorithm,
    ) -> Self {
        Self {
            windows: window_set.note,
            window_set,
            rule_mode,
            algorithm,
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

            while let Some((idx, note)) = next_unjudged_press_reference_note(
                chart,
                lane,
                lane_state.next_note_index,
                &self.judged_notes,
            ) {
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
                    affects_score: true,
                });
            }
            advance_press_cursor(chart, lane, &mut lane_state.next_note_index, &self.judged_notes);

            if let Some(active) = lane_state.active_long {
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
                            outcome
                                .keysounds
                                .push(KeySoundEvent { note_id: active.end.end_note_id, time: now });
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
                    damage: note.damage.unwrap_or(0),
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

        if self.lanes[input.lane.index()].active_long.is_some() {
            return JudgeOutcome { mine_hits, ..Default::default() };
        }

        let rule_mode = self.rule_mode;
        let windows = self.window_set.press_window(input.lane);
        let candidate = select_press_candidate(
            chart,
            input.lane,
            input.time,
            windows,
            rule_mode,
            self.algorithm,
            &self.judged_notes,
        );
        let Some(candidate) = candidate else {
            return JudgeOutcome { mine_hits, ..Default::default() };
        };

        if candidate.consumes_note {
            let note_id = candidate.note_id.expect("normal candidate must have note id");
            let note = chart.note_by_id(note_id).expect("candidate note exists");
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
                self.judged_notes.insert(multi_bad.note_id, Judge::Bad);
            }
            self.judged_notes.insert(note.id, candidate.judge);

            if note.kind == NoteKind::LongStart
                && let Some(active) =
                    make_active_long(chart, note.id, candidate.judge, candidate.delta, input.time)
            {
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
                keysounds: vec![KeySoundEvent { note_id, time: input.time }],
                mine_hits,
                consumed_input: true,
                ..Default::default()
            };
        }

        let keysound_note_id =
            candidate.keysound_note_id.expect("empty poor candidate must have key sound note id");
        let mut outcome =
            empty_poor(input.lane, candidate.side, candidate.delta, input.time, keysound_note_id);
        outcome.mine_hits = mine_hits;
        outcome
    }

    fn process_release(&mut self, chart: &PlayableChart, input: InputEvent) -> JudgeOutcome {
        let lane_state = &mut self.lanes[input.lane.index()];
        let Some(active) = lane_state.active_long else {
            return JudgeOutcome::default();
        };

        match active.mode {
            LongNoteMode::Ln => {
                lane_state.active_long = None;
                let end_delta = TimeUs(input.time.0 - active.end.end_time.0);
                let (judge, delta) = if end_delta.0 >= 0 {
                    (active.start_judge, active.start_delta)
                } else {
                    let windows = self.window_set.long_end_window(input.lane);
                    let end_judge =
                        classify_normal_delta(end_delta.0, windows).unwrap_or(Judge::Poor);
                    combine_ln_judgement(active, end_judge, end_delta)
                };
                self.judged_notes.insert(active.start_note_id, judge);
                let mut outcome = JudgeOutcome {
                    events: vec![ln_final_event(input.lane, active, judge, delta, input.time)],
                    keysounds: vec![KeySoundEvent {
                        note_id: active.end.end_note_id,
                        time: input.time,
                    }],
                    mine_hits: Vec::new(),
                    consumed_input: true,
                    ..Default::default()
                };
                push_early_bad_long_start_mute(chart, active, judge, end_delta, &mut outcome);
                outcome
            }
            LongNoteMode::Cn | LongNoteMode::Hcn => {
                let delta = input.time.0 - active.end.end_time.0;
                let side = side_from_delta(delta);
                let windows = self.window_set.long_end_window(input.lane);
                let judge = classify_normal_delta(delta, windows).unwrap_or(Judge::Poor);
                lane_state.active_long = None;
                self.judged_notes.insert(active.end.end_note_id, judge);

                let mut outcome = JudgeOutcome {
                    events: vec![JudgementEvent {
                        note_id: Some(active.end.end_note_id),
                        lane: input.lane,
                        judge,
                        side,
                        delta: TimeUs(delta),
                        time: input.time,
                        affects_score: true,
                    }],
                    keysounds: vec![KeySoundEvent {
                        note_id: active.end.end_note_id,
                        time: input.time,
                    }],
                    mine_hits: Vec::new(),
                    consumed_input: true,
                    ..Default::default()
                };
                if active.mode == LongNoteMode::Cn {
                    push_early_bad_long_start_mute(
                        chart,
                        active,
                        judge,
                        TimeUs(delta),
                        &mut outcome,
                    );
                }
                outcome
            }
        }
    }
}

fn push_early_bad_long_start_mute(
    chart: &PlayableChart,
    active: ActiveLongNote,
    judge: Judge,
    end_delta: TimeUs,
    outcome: &mut JudgeOutcome,
) {
    if end_delta.0 < 0
        && matches!(judge, Judge::Bad | Judge::Poor)
        && let Some(sound_id) = chart.long_notes.get(active.pair_index).and_then(|pair| pair.sound)
    {
        outcome.keysound_volumes.push((sound_id, 0.0));
    }
}

fn suppresses_long_start_late_bad(
    rule_mode: RuleMode,
    windows: JudgeWindow,
    note: &NoteEvent,
    delta: i64,
    judge: Judge,
) -> bool {
    matches!(rule_mode, RuleMode::Lr2Oraja | RuleMode::Dx)
        && note.kind == NoteKind::LongStart
        && judge == Judge::Bad
        && delta > windows.good_us
}

#[derive(Debug, Clone, Copy)]
struct PressCandidate {
    note_id: Option<NoteId>,
    keysound_note_id: Option<NoteId>,
    judge: Judge,
    side: TimingSide,
    delta: TimeUs,
    consumes_note: bool,
}

#[derive(Debug, Clone, Copy)]
struct MultiBadCandidate {
    note_id: NoteId,
    note_kind: NoteKind,
    delta: TimeUs,
}

fn select_press_candidate(
    chart: &PlayableChart,
    lane: Lane,
    input_time: TimeUs,
    windows: JudgeWindow,
    rule_mode: RuleMode,
    algorithm: JudgeAlgorithm,
    judged_notes: &HashMap<NoteId, Judge>,
) -> Option<PressCandidate> {
    let mut normal: Option<PressCandidate> = None;
    let mut slow_empty_poor: Option<PressCandidate> = None;
    let mut fast_empty_poor: Option<PressCandidate> = None;
    let scan_fast_us = windows.bad_fast_us.max(windows.empty_poor_fast_us);
    let scan_slow_us = windows.bad_slow_us.max(windows.empty_poor_slow_us);

    for note in chart.notes_for_lane(lane) {
        if note.time.0 - input_time.0 > scan_fast_us {
            break;
        }
        if input_time.0 - note.time.0 > scan_slow_us || !is_press_reference_note(note) {
            continue;
        }

        let delta = input_time.0 - note.time.0;
        let already_judged = judged_notes.contains_key(&note.id);
        if !already_judged
            && let Some(judge) = classify_normal_delta(delta, windows).filter(|judge| {
                !suppresses_long_start_late_bad(rule_mode, windows, note, delta, *judge)
            })
        {
            let candidate = PressCandidate {
                note_id: Some(note.id),
                keysound_note_id: Some(note.id),
                judge,
                side: side_from_delta(delta),
                delta: TimeUs(delta),
                consumes_note: true,
            };
            if normal.as_ref().is_none_or(|current| {
                judge_algorithm_prefers_new_candidate(algorithm, *current, candidate, windows)
            }) {
                normal = Some(candidate);
            }
            continue;
        }

        let empty_poor_candidate = if already_judged {
            if delta >= 0 && delta <= windows.empty_poor_slow_us {
                Some(PressCandidate {
                    note_id: None,
                    keysound_note_id: Some(note.id),
                    judge: Judge::EmptyPoor,
                    side: TimingSide::Slow,
                    delta: TimeUs(delta),
                    consumes_note: false,
                })
            } else if delta < 0 && -delta <= windows.empty_poor_fast_us {
                Some(PressCandidate {
                    note_id: None,
                    keysound_note_id: Some(note.id),
                    judge: Judge::EmptyPoor,
                    side: TimingSide::Fast,
                    delta: TimeUs(delta),
                    consumes_note: false,
                })
            } else {
                None
            }
        } else if delta > windows.bad_slow_us && delta <= windows.empty_poor_slow_us {
            Some(PressCandidate {
                note_id: None,
                keysound_note_id: Some(note.id),
                judge: Judge::EmptyPoor,
                side: TimingSide::Slow,
                delta: TimeUs(delta),
                consumes_note: false,
            })
        } else if delta < -windows.bad_fast_us && -delta <= windows.empty_poor_fast_us {
            Some(PressCandidate {
                note_id: None,
                keysound_note_id: Some(note.id),
                judge: Judge::EmptyPoor,
                side: TimingSide::Fast,
                delta: TimeUs(delta),
                consumes_note: false,
            })
        } else {
            None
        };

        let Some(candidate) = empty_poor_candidate else {
            continue;
        };
        match candidate.side {
            TimingSide::Slow => choose_closest_empty_poor(&mut slow_empty_poor, candidate),
            TimingSide::Fast => choose_closest_empty_poor(&mut fast_empty_poor, candidate),
        }
    }

    normal.or(slow_empty_poor).or(fast_empty_poor)
}

fn judge_algorithm_prefers_new_candidate(
    algorithm: JudgeAlgorithm,
    current: PressCandidate,
    candidate: PressCandidate,
    windows: JudgeWindow,
) -> bool {
    match algorithm {
        JudgeAlgorithm::Combo => {
            current.delta.0 > windows.good_us && candidate.delta.0 >= -windows.good_us
        }
        JudgeAlgorithm::Duration => candidate.delta.0.abs() < current.delta.0.abs(),
        JudgeAlgorithm::Lowest => false,
        JudgeAlgorithm::Score => {
            current.delta.0 > windows.great_us && candidate.delta.0 >= -windows.great_us
        }
    }
}

fn choose_closest_empty_poor(slot: &mut Option<PressCandidate>, candidate: PressCandidate) {
    if slot.as_ref().is_none_or(|current| candidate.delta.0.abs() < current.delta.0.abs()) {
        *slot = Some(candidate);
    }
}

fn lr2oraja_multi_bad_candidates(
    chart: &PlayableChart,
    lane: Lane,
    input_time: TimeUs,
    windows: JudgeWindow,
    selected_note: &NoteEvent,
    selected_candidate: PressCandidate,
    judged_notes: &HashMap<NoteId, Judge>,
) -> Vec<MultiBadCandidate> {
    let selected_dmtime = -selected_candidate.delta.0;
    let mut candidates = chart
        .notes_for_lane(lane)
        .iter()
        .take_while(|note| note.time.0 - input_time.0 <= windows.bad_fast_us)
        .filter(|note| {
            is_press_reference_note(note)
                && note.id != selected_note.id
                && !judged_notes.contains_key(&note.id)
        })
        .filter_map(|note| {
            let delta = input_time.0 - note.time.0;
            (in_bad_range(delta, windows) && !in_good_range(delta, windows)).then_some(
                MultiBadCandidate { note_id: note.id, note_kind: note.kind, delta: TimeUs(delta) },
            )
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|candidate| -candidate.delta.0);

    if selected_candidate.judge != Judge::Bad || selected_note.kind == NoteKind::LongStart {
        candidates.retain(|candidate| -candidate.delta.0 < selected_dmtime);
    }

    let array_start = candidates
        .iter()
        .position(|candidate| {
            -candidate.delta.0 >= selected_dmtime || candidate.note_kind != NoteKind::LongStart
        })
        .unwrap_or(candidates.len());
    candidates.into_iter().skip(array_start).collect()
}

fn combine_ln_judgement(
    active: ActiveLongNote,
    end_judge: Judge,
    end_delta: TimeUs,
) -> (Judge, TimeUs) {
    let mut judge = worse_judge(active.start_judge, end_judge);
    let mut delta =
        if active.start_delta.0.abs() > end_delta.0.abs() { active.start_delta } else { end_delta };

    if end_delta.0 < 0 && matches!(judge, Judge::Bad | Judge::Poor) {
        judge = Judge::Bad;
        delta = end_delta;
    }

    (judge, delta)
}

fn worse_judge(left: Judge, right: Judge) -> Judge {
    if judge_order(left) >= judge_order(right) { left } else { right }
}

fn judge_order(judge: Judge) -> u8 {
    match judge {
        Judge::PGreat => 0,
        Judge::Great => 1,
        Judge::Good => 2,
        Judge::Bad => 3,
        Judge::Poor => 4,
        Judge::EmptyPoor => 5,
    }
}

fn next_unjudged_press_reference_note<'a>(
    chart: &'a PlayableChart,
    lane: Lane,
    start_index: usize,
    judged_notes: &HashMap<NoteId, Judge>,
) -> Option<(usize, &'a NoteEvent)> {
    chart
        .notes_for_lane(lane)
        .iter()
        .enumerate()
        .skip(start_index)
        .find(|(_, note)| is_press_reference_note(note) && !judged_notes.contains_key(&note.id))
}

fn advance_press_cursor(
    chart: &PlayableChart,
    lane: Lane,
    next_note_index: &mut usize,
    judged_notes: &HashMap<NoteId, Judge>,
) {
    let notes = chart.notes_for_lane(lane);
    while let Some(note) = notes.get(*next_note_index) {
        if is_press_reference_note(note) && !judged_notes.contains_key(&note.id) {
            break;
        }
        *next_note_index += 1;
    }
}

fn is_press_reference_note(note: &NoteEvent) -> bool {
    matches!(note.kind, NoteKind::Tap | NoteKind::LongStart)
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

fn in_good_range(delta_us: i64, windows: JudgeWindow) -> bool {
    delta_us.abs() <= windows.good_us
}

fn in_bad_range(delta_us: i64, windows: JudgeWindow) -> bool {
    (delta_us < 0 && -delta_us <= windows.bad_fast_us)
        || (delta_us >= 0 && delta_us <= windows.bad_slow_us)
}

fn side_from_delta(delta_us: i64) -> TimingSide {
    if delta_us < 0 { TimingSide::Fast } else { TimingSide::Slow }
}

fn make_active_long(
    chart: &PlayableChart,
    start_note_id: NoteId,
    start_judge: Judge,
    start_delta: TimeUs,
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
        start_judge,
        start_delta,
        end: LongNoteEndRef {
            end_note_id: pair.end_note_id,
            end_tick: pair.end_tick,
            end_time: pair.end_time,
        },
        started_at,
    })
}

fn active_long_scores_on_start(chart: &PlayableChart, start_note_id: NoteId) -> bool {
    chart
        .long_notes
        .iter()
        .find(|pair| pair.start_note_id == start_note_id)
        .map(|pair| pair.mode.unwrap_or(chart.metadata.long_note_mode) != LongNoteMode::Ln)
        .unwrap_or(true)
}

fn ln_final_event(
    lane: Lane,
    active: ActiveLongNote,
    judge: Judge,
    delta: TimeUs,
    time: TimeUs,
) -> JudgementEvent {
    JudgementEvent {
        note_id: Some(active.start_note_id),
        lane,
        judge,
        side: side_from_delta(delta.0),
        delta,
        time,
        affects_score: true,
    }
}

fn empty_poor(
    lane: Lane,
    side: TimingSide,
    delta: TimeUs,
    time: TimeUs,
    keysound_note_id: NoteId,
) -> JudgeOutcome {
    JudgeOutcome {
        events: vec![JudgementEvent {
            note_id: None,
            lane,
            judge: Judge::EmptyPoor,
            side,
            delta,
            time,
            affects_score: true,
        }],
        keysounds: vec![KeySoundEvent { note_id: keysound_note_id, time }],
        mine_hits: Vec::new(),
        consumed_input: false,
        ..Default::default()
    }
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

    fn chart_with_two_taps(first_time: TimeUs, second_time: TimeUs) -> PlayableChart {
        let lane = Lane::Key1;
        let first = NoteEvent {
            id: NoteId(1),
            lane,
            kind: NoteKind::Tap,
            tick: Default::default(),
            time: first_time,
            sound: None,
            damage: None,
        };
        let second = NoteEvent {
            id: NoteId(2),
            lane,
            kind: NoteKind::Tap,
            tick: Default::default(),
            time: second_time,
            sound: None,
            damage: None,
        };
        let mut chart = chart_with_tap(first_time);
        chart.lane_notes[lane.index()] = vec![first, second];
        chart.total_notes = 2;
        chart.end_time = second_time;
        chart
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
        assert_eq!(
            outcome.keysounds,
            vec![KeySoundEvent { note_id: NoteId(1), time: TimeUs(1_150_000) }]
        );
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
        assert_eq!(
            outcome.keysounds,
            vec![KeySoundEvent { note_id: NoteId(1), time: TimeUs(700_000) }]
        );
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
        assert_eq!(
            second.keysounds,
            vec![KeySoundEvent { note_id: NoteId(1), time: TimeUs(1_005_000) }]
        );
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
    fn combo_candidate_prefers_later_combo_note_over_slow_bad() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_100_000));
        let mut engine = JudgeEngine::new(windows());

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));
        let missed = engine.process_misses(&chart, TimeUs(1_130_000));

        assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
        assert_eq!(outcome.events[0].judge, Judge::PGreat);
        assert_eq!(missed.events[0].note_id, Some(NoteId(1)));
        assert_eq!(missed.events[0].judge, Judge::Poor);
    }

    #[test]
    fn duration_candidate_prefers_closest_note() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_040_000));
        let mut engine = JudgeEngine::new_with_window_set_and_algorithm(
            JudgeWindows::uniform(windows()),
            RuleMode::Beatoraja,
            JudgeAlgorithm::Duration,
        );

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_030_000)));

        assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
        assert_eq!(outcome.events[0].judge, Judge::PGreat);
        assert_eq!(outcome.events[0].delta, TimeUs(-10_000));
    }

    #[test]
    fn lowest_candidate_keeps_first_note() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_040_000));
        let mut engine = JudgeEngine::new_with_window_set_and_algorithm(
            JudgeWindows::uniform(windows()),
            RuleMode::Beatoraja,
            JudgeAlgorithm::Lowest,
        );

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_030_000)));

        assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
        assert_eq!(outcome.events[0].judge, Judge::Great);
        assert_eq!(outcome.events[0].delta, TimeUs(30_000));
    }

    #[test]
    fn score_candidate_uses_great_threshold_instead_of_duration() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_150_000));
        let mut engine = JudgeEngine::new_with_window_set_and_algorithm(
            JudgeWindows::uniform(windows()),
            RuleMode::Beatoraja,
            JudgeAlgorithm::Score,
        );

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));

        assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
        assert_eq!(outcome.events[0].judge, Judge::Bad);
        assert_eq!(outcome.events[0].delta, TimeUs(100_000));
    }

    #[test]
    fn lr2oraja_multi_bad_adds_preceding_bad_before_selected_note() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_090_000));
        let mut engine = JudgeEngine::new_with_rule_mode(
            crate::judge::window::lr2oraja_note_judge_window(),
            RuleMode::Lr2Oraja,
        );

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

        assert!(outcome.consumed_input);
        assert_eq!(outcome.events.len(), 2);
        assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
        assert_eq!(outcome.events[0].judge, Judge::Bad);
        assert_eq!(outcome.events[0].delta, TimeUs(150_000));
        assert_eq!(outcome.events[1].note_id, Some(NoteId(2)));
        assert_eq!(outcome.events[1].judge, Judge::Great);
        assert_eq!(outcome.events[1].delta, TimeUs(60_000));
        assert_eq!(
            outcome.keysounds,
            vec![KeySoundEvent { note_id: NoteId(2), time: TimeUs(1_150_000) }]
        );
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 2);
    }

    #[test]
    fn dx_mode_adds_lr2oraja_multi_bad() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_090_000));
        let mut engine = JudgeEngine::new_with_rule_mode(
            crate::judge::window::dx_note_judge_window(),
            RuleMode::Dx,
        );

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

        assert!(outcome.consumed_input);
        assert_eq!(outcome.events.len(), 2);
        assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
        assert_eq!(outcome.events[0].judge, Judge::Bad);
        assert_eq!(outcome.events[1].note_id, Some(NoteId(2)));
        assert_eq!(outcome.events[1].judge, Judge::Good);
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 2);
    }

    #[test]
    fn beatoraja_mode_does_not_add_lr2oraja_multi_bad() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_090_000));
        let mut engine = JudgeEngine::new_with_rule_mode(
            crate::judge::window::lr2oraja_note_judge_window(),
            RuleMode::Beatoraja,
        );

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

        assert_eq!(outcome.events.len(), 1);
        assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
        assert_eq!(outcome.events[0].judge, Judge::Great);
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
    }

    #[test]
    fn lr2oraja_multi_bad_keeps_following_bad_when_selected_note_is_bad() {
        let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_260_000));
        let mut engine = JudgeEngine::new_with_rule_mode(
            crate::judge::window::lr2oraja_note_judge_window(),
            RuleMode::Lr2Oraja,
        );

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_130_000)));

        assert!(outcome.consumed_input);
        assert_eq!(outcome.events.len(), 2);
        assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
        assert_eq!(outcome.events[0].judge, Judge::Bad);
        assert_eq!(outcome.events[0].delta, TimeUs(-130_000));
        assert_eq!(outcome.events[1].note_id, Some(NoteId(1)));
        assert_eq!(outcome.events[1].judge, Judge::Bad);
        assert_eq!(outcome.events[1].delta, TimeUs(130_000));
        assert_eq!(
            outcome.keysounds,
            vec![KeySoundEvent { note_id: NoteId(1), time: TimeUs(1_130_000) }]
        );
        assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 2);
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
    fn lr2oraja_derived_modes_suppress_late_bad_on_long_note_start() {
        let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        let input = press_at(TimeUs(1_100_000));

        let mut beatoraja = JudgeEngine::new(windows());
        let beatoraja_outcome = beatoraja.process_input(&chart, input);
        assert_eq!(beatoraja_outcome.events[0].judge, Judge::Bad);
        assert_eq!(beatoraja.lanes[Lane::Key1.index()].next_note_index, 2);

        let mut lr2oraja = JudgeEngine::new_with_rule_mode(windows(), RuleMode::Lr2Oraja);
        let lr2oraja_outcome = lr2oraja.process_input(&chart, input);
        assert!(lr2oraja_outcome.events.is_empty());
        assert!(!lr2oraja_outcome.consumed_input);
        assert_eq!(lr2oraja.lanes[Lane::Key1.index()].next_note_index, 0);

        let mut dx = JudgeEngine::new_with_rule_mode(windows(), RuleMode::Dx);
        let dx_outcome = dx.process_input(&chart, input);
        assert!(dx_outcome.events.is_empty());
        assert!(!dx_outcome.consumed_input);
        assert_eq!(dx.lanes[Lane::Key1.index()].next_note_index, 0);
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
    fn ln_start_defers_scoring_until_end() {
        let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        let mut engine = JudgeEngine::new(windows());

        let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let end = engine.process_misses(&chart, TimeUs(2_000_001));

        assert_eq!(press.events[0].note_id, Some(NoteId(1)));
        assert_eq!(press.events[0].judge, Judge::PGreat);
        assert!(!press.events[0].affects_score);
        assert_eq!(end.events[0].note_id, Some(NoteId(1)));
        assert_eq!(end.events[0].judge, Judge::PGreat);
        assert!(end.events[0].affects_score);
    }

    #[test]
    fn ln_early_release_scores_once_with_combined_judge() {
        let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        let mut engine = JudgeEngine::new(windows());

        let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let release = engine.process_input(&chart, release_at(TimeUs(1_900_000)));

        assert!(!press.events[0].affects_score);
        assert_eq!(release.events[0].note_id, Some(NoteId(1)));
        assert_eq!(release.events[0].judge, Judge::Bad);
        assert_eq!(release.events[0].side, TimingSide::Fast);
        assert_eq!(release.events[0].delta, TimeUs(-100_000));
        assert!(release.events[0].affects_score);
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
    fn mine_pass_hits_when_lane_is_held() {
        let chart = chart_with_mine(TimeUs(1_000_000), 8);
        let mut engine = JudgeEngine::new(windows());
        let mut lane_keyon_started_at = [None; LANE_COUNT];
        lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));

        let outcome = engine.process_mine_passes(&chart, TimeUs(1_000_000), &lane_keyon_started_at);

        assert_eq!(outcome.mine_hits.len(), 1);
        assert_eq!(outcome.mine_hits[0].note_id, NoteId(7));
        assert_eq!(outcome.mine_hits[0].damage, 8);
    }

    #[test]
    fn mine_pass_without_pressed_lane_is_skipped() {
        let chart = chart_with_mine(TimeUs(1_000_000), 8);
        let mut engine = JudgeEngine::new(windows());
        let lane_keyon_started_at = [None; LANE_COUNT];

        let outcome = engine.process_mine_passes(&chart, TimeUs(1_000_000), &lane_keyon_started_at);

        assert!(outcome.mine_hits.is_empty());
    }

    #[test]
    fn mine_pass_ignores_key_pressed_after_mine_time() {
        let chart = chart_with_mine(TimeUs(1_000_000), 8);
        let mut engine = JudgeEngine::new(windows());
        let mut lane_keyon_started_at = [None; LANE_COUNT];
        lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(1_050_000));

        let outcome = engine.process_mine_passes(&chart, TimeUs(1_100_000), &lane_keyon_started_at);

        assert!(outcome.mine_hits.is_empty());
    }

    #[test]
    fn mine_does_not_hit_after_it_already_passed_unpressed() {
        let chart = chart_with_mine(TimeUs(1_000_000), 8);
        let mut engine = JudgeEngine::new(windows());
        let lane_keyon_started_at = [None; LANE_COUNT];
        engine.process_mine_passes(&chart, TimeUs(1_000_000), &lane_keyon_started_at);

        let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));

        assert!(outcome.mine_hits.is_empty());
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
