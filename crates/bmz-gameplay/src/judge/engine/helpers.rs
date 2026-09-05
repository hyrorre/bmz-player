use super::*;

pub(super) fn push_early_bad_long_start_mute(
    chart: &PlayableChart,
    active: ActiveLongNote,
    judge: Judge,
    end_delta: TimeUs,
    outcome: &mut JudgeOutcome,
) {
    if end_delta.0 < 0 && matches!(judge, Judge::Bad | Judge::Poor) {
        let Some(pair) = chart.long_notes.get(active.pair_index) else {
            return;
        };
        if let Some(note) = chart.note_by_id(pair.start_note_id) {
            outcome.keysound_volumes.extend(note.sounds().map(|sound_id| (sound_id, 0.0)));
        } else if let Some(sound_id) = pair.sound {
            outcome.keysound_volumes.push((sound_id, 0.0));
        }
    }
}

pub(super) fn finalize_long_release(
    chart: &PlayableChart,
    lane: Lane,
    active: ActiveLongNote,
    judge: Judge,
    delta: TimeUs,
    time: TimeUs,
) -> JudgeOutcome {
    let mut outcome = match active.mode {
        LongNoteMode::Ln => JudgeOutcome {
            events: vec![ln_final_event(lane, active, judge, delta, time)],
            keysounds: vec![KeySoundEvent {
                note_id: active.end.end_note_id,
                time,
                trigger: KeySoundTrigger::NoteJudged,
            }],
            consumed_input: true,
            ..Default::default()
        },
        LongNoteMode::Cn | LongNoteMode::Hcn => JudgeOutcome {
            events: vec![JudgementEvent {
                note_id: Some(active.end.end_note_id),
                lane,
                judge,
                side: side_from_delta(delta.0),
                delta,
                time,
                affects_score: true,
            }],
            keysounds: vec![KeySoundEvent {
                note_id: active.end.end_note_id,
                time,
                trigger: KeySoundTrigger::NoteJudged,
            }],
            consumed_input: true,
            ..Default::default()
        },
    };
    if active.mode != LongNoteMode::Hcn {
        push_early_bad_long_start_mute(chart, active, judge, delta, &mut outcome);
    }
    outcome
}

pub(super) fn append_outcome(target: &mut JudgeOutcome, mut source: JudgeOutcome) {
    target.events.append(&mut source.events);
    target.keysounds.append(&mut source.keysounds);
    target.keysound_volumes.append(&mut source.keysound_volumes);
    target.consumed_input |= source.consumed_input;
}

pub(super) fn active_scored_note_id(active: ActiveLongNote) -> NoteId {
    match active.mode {
        LongNoteMode::Ln => active.start_note_id,
        LongNoteMode::Cn | LongNoteMode::Hcn => active.end.end_note_id,
    }
}

pub(super) fn suppresses_long_start_late_bad(
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
pub(super) struct PressCandidate {
    pub(super) note_id: Option<NoteId>,
    pub(super) keysound_note_id: Option<NoteId>,
    pub(super) judge: Judge,
    pub(super) side: TimingSide,
    pub(super) delta: TimeUs,
    pub(super) consumes_note: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MultiBadCandidate {
    pub(super) note_id: NoteId,
    pub(super) note_kind: NoteKind,
    pub(super) delta: TimeUs,
}

pub(super) fn select_press_candidate(
    chart: &PlayableChart,
    lane: Lane,
    input_time: TimeUs,
    windows: JudgeWindow,
    rule_mode: RuleMode,
    algorithm: JudgeAlgorithm,
    judged_notes: &HashMap<NoteId, Judge>,
    bad_attempted_notes: &HashSet<NoteId>,
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
        let bad_attempted = bad_attempted_notes.contains(&note.id);
        if !already_judged
            && let Some(judge) = classify_normal_delta(delta, windows).filter(|judge| {
                !suppresses_long_start_late_bad(rule_mode, windows, note, delta, *judge)
            })
        {
            if bad_attempted && judge == Judge::Bad {
                continue;
            }
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

        if bad_attempted {
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

pub(super) fn judge_algorithm_prefers_new_candidate(
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

pub(super) fn choose_closest_empty_poor(
    slot: &mut Option<PressCandidate>,
    candidate: PressCandidate,
) {
    if slot.as_ref().is_none_or(|current| candidate.delta.0.abs() < current.delta.0.abs()) {
        *slot = Some(candidate);
    }
}

pub(super) fn lr2oraja_multi_bad_candidates(
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

pub(super) fn combine_ln_judgement(
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

pub(super) fn worse_judge(left: Judge, right: Judge) -> Judge {
    if judge_order(left) >= judge_order(right) { left } else { right }
}

pub(super) fn judge_order(judge: Judge) -> u8 {
    match judge {
        Judge::PGreat => 0,
        Judge::Great => 1,
        Judge::Good => 2,
        Judge::Bad => 3,
        Judge::Poor => 4,
        Judge::EmptyPoor => 5,
    }
}

pub(super) fn next_unjudged_press_reference_note<'a>(
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

pub(super) fn advance_press_cursor(
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

pub(super) fn is_press_reference_note(note: &NoteEvent) -> bool {
    matches!(note.kind, NoteKind::Tap | NoteKind::LongStart)
}

/// 指定レーンに置かれた Mine の中から、入力時刻と `window_us` 以内に一致するものを探す。
/// 直近に同じ time の Mine をヒット済みなら無視する（二重ヒット防止）。
pub(super) fn detect_mine_hit(
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
            damage: note.damage.unwrap_or(0.0),
            sound: note.sound,
            time: note.time,
        })
}

pub(super) fn classify_normal_delta(delta_us: i64, windows: JudgeWindow) -> Option<Judge> {
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

pub(super) fn in_good_range(delta_us: i64, windows: JudgeWindow) -> bool {
    delta_us.abs() <= windows.good_us
}

pub(super) fn in_bad_range(delta_us: i64, windows: JudgeWindow) -> bool {
    (delta_us < 0 && -delta_us <= windows.bad_fast_us)
        || (delta_us >= 0 && delta_us <= windows.bad_slow_us)
}

pub(super) fn bad_judge_vanish_for_keymode_and_rule_mode(
    key_mode: KeyMode,
    rule_mode: RuleMode,
) -> bool {
    !(matches!(rule_mode, RuleMode::Beatoraja | RuleMode::Dx) && key_mode == KeyMode::K9)
}

pub(super) fn side_from_delta(delta_us: i64) -> TimingSide {
    if delta_us < 0 { TimingSide::Fast } else { TimingSide::Slow }
}

pub(super) fn make_active_long(
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
        scratch_direction: None,
        pending_release: None,
    })
}

pub(super) fn missed_charge_end_for_start(
    chart: &PlayableChart,
    start_note_id: NoteId,
) -> Option<NoteId> {
    chart.long_notes.iter().find_map(|pair| {
        let mode = pair.mode.unwrap_or(chart.metadata.long_note_mode);
        (pair.start_note_id == start_note_id
            && matches!(mode, LongNoteMode::Cn | LongNoteMode::Hcn))
        .then_some(pair.end_note_id)
    })
}

pub(super) fn active_long_scores_on_start(chart: &PlayableChart, start_note_id: NoteId) -> bool {
    chart
        .long_notes
        .iter()
        .find(|pair| pair.start_note_id == start_note_id)
        .map(|pair| pair.mode.unwrap_or(chart.metadata.long_note_mode) != LongNoteMode::Ln)
        .unwrap_or(true)
}

pub(super) fn ln_final_event(
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

pub(super) fn empty_poor(
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
        keysounds: vec![KeySoundEvent {
            note_id: keysound_note_id,
            time,
            trigger: KeySoundTrigger::NoteJudged,
        }],
        mine_hits: Vec::new(),
        consumed_input: false,
        ..Default::default()
    }
}
