use super::*;

/// Stop the long start keysound when the release itself failed.
///
/// `release_delta` is the offset of the release from the long end, and
/// `early_release_limit_us` is the early-release tolerance. Comparing the
/// release against the tolerance (rather than the reported delta or the final
/// judgement) keeps a start-BAD long note from being silenced when it was held
/// to the end, and an early-release BAD from staying audible.
pub(super) fn push_early_bad_long_start_mute(
    chart: &PlayableChart,
    active: ActiveLongNote,
    judge: Judge,
    release_delta: TimeUs,
    early_release_limit_us: i64,
    outcome: &mut JudgeOutcome,
) {
    if release_delta.0 < -early_release_limit_us && matches!(judge, Judge::Bad | Judge::Poor) {
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

/// Finalize a released long note.
///
/// `delta` is the timing offset reported with the judgement (LR2 reports the
/// head offset for an early BAD on a LEGACY LN). `release_delta` is compared
/// against the long end and its tolerance only to decide whether the release
/// failed early; the two must not be conflated.
pub(super) fn finalize_long_release(
    chart: &PlayableChart,
    lane: Lane,
    active: ActiveLongNote,
    judge: Judge,
    delta: TimeUs,
    release_delta: TimeUs,
    early_release_limit_us: i64,
    time: TimeUs,
) -> JudgeOutcome {
    let mut outcome = match active.mode {
        LongNoteMode::Hln => JudgeOutcome {
            events: vec![ln_final_event(lane, active, judge, delta, time)],
            consumed_input: true,
            ..Default::default()
        },
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
                advances_score: true,
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
    if matches!(active.mode, LongNoteMode::Ln | LongNoteMode::Cn) {
        push_early_bad_long_start_mute(
            chart,
            active,
            judge,
            release_delta,
            early_release_limit_us,
            &mut outcome,
        );
    }
    outcome
}

pub(super) fn append_outcome(target: &mut JudgeOutcome, mut source: JudgeOutcome) {
    target.hold_ticks.append(&mut source.hold_ticks);
    target.hold_sounds.append(&mut source.hold_sounds);
    target.mine_hits.append(&mut source.mine_hits);
    target.potion_hits.append(&mut source.potion_hits);
    target.events.append(&mut source.events);
    target.keysounds.append(&mut source.keysounds);
    target.keysound_volumes.append(&mut source.keysound_volumes);
    target.consumed_input |= source.consumed_input;
}

pub(super) fn active_scored_note_id(active: ActiveLongNote) -> NoteId {
    match active.mode {
        LongNoteMode::Ln | LongNoteMode::Hln => active.start_note_id,
        LongNoteMode::Cn | LongNoteMode::Hcn => active.end.end_note_id,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PressCandidate {
    pub(super) note_id: NoteId,
    pub(super) judge: Judge,
    pub(super) delta: TimeUs,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MultiBadCandidate {
    pub(super) note_id: NoteId,
    pub(super) note_kind: NoteKind,
    pub(super) delta: TimeUs,
}

pub(super) fn select_press_candidate(
    engine: &JudgeEngine,
    chart: &PlayableChart,
    lane: Lane,
    input_time: TimeUs,
    windows: JudgeWindow,
) -> Option<PressCandidate> {
    let mut selected: Option<PressCandidate> = None;
    let (scan_slow, scan_fast) = engine.window_set.press_scan_limits();
    let notes = chart.notes_for_lane(lane);
    if engine.algorithm == JudgeAlgorithm::Duration
        && windows.pgreat_us >= 0
        && scan_fast > 0
        && scan_slow >= 0
    {
        // A normal hit at zero distance wins over every other candidate.
        // Start at the first equal timestamp to preserve the original tie order.
        let exact = notes.partition_point(|note| note.time < input_time);
        if let Some(note) =
            notes[exact..].iter().take_while(|note| note.time == input_time).find(|note| {
                is_press_reference_note(note) && !engine.judged_notes.contains_key(&note.id)
            })
        {
            return Some(PressCandidate {
                note_id: note.id,
                judge: Judge::PGreat,
                delta: TimeUs(0),
            });
        }
    }
    let earliest = input_time.0.saturating_sub(scan_slow);
    let start = notes.partition_point(|note| note.time.0 < earliest);
    for note in &notes[start..] {
        let delta = input_time.0 - note.time.0;
        if delta <= -scan_fast {
            break;
        }
        if !is_press_reference_note(note) {
            continue;
        }

        let consumed = engine.judged_notes.contains_key(&note.id);
        if !engine.bad_judge_vanish
            && (consumed
                || (engine.attempted_notes.contains(&note.id) && !in_good_range(delta, windows)))
        {
            continue;
        }
        let mut judge = if consumed { None } else { classify_normal_delta(delta, windows) };
        if !consumed
            && note.kind == NoteKind::LongStart
            && matches!(engine.rule_mode, RuleMode::Lr2Oraja | RuleMode::Dx)
            && delta > good_width(delta, windows)
        {
            judge = None;
        } else if judge.is_none()
            && delta >= -windows.empty_poor_fast_us
            && delta <= windows.empty_poor_slow_us
        {
            judge = Some(Judge::EmptyPoor);
        }
        if selected.is_some_and(|current| {
            let current_consumed = engine.judged_notes.contains_key(&current.note_id);
            !current_consumed
                && (consumed
                    || !judge_algorithm_prefers_new_candidate(
                        engine.algorithm,
                        current.delta.0,
                        delta,
                        windows,
                    ))
        }) {
            continue;
        }
        match judge {
            None => selected = None,
            Some(judge)
                if judge != Judge::EmptyPoor
                    || selected.is_none_or(|current| delta.abs() < current.delta.0.abs()) =>
            {
                selected = Some(PressCandidate { note_id: note.id, judge, delta: TimeUs(delta) });
            }
            Some(_) => {}
        }
    }

    selected
}

/// LEGACY compares the integer millisecond positions of input and note.
pub(super) fn legacy_offset(input: TimeUs, note: TimeUs) -> i64 {
    (input.0 / 1_000 - note.0 / 1_000) * 1_000
}

/// LEGACY の LN 始点見逃し期限と終端早離し許容に使う幅。
///
/// OpenLR2 はどちらも GOOD 幅を使うが、BMZ の NO GOOD / NO GREAT はタップの
/// GOOD / GREAT 帯だけを 0 にする。0 をそのまま LN へ流用すると、有効な
/// PGREAT / GREAT 帯の入力まで LN で失敗するため、有効な帯の最大幅を使う。
pub(super) fn legacy_ln_margin_us(windows: JudgeWindow) -> i64 {
    let [pgreat, great, good] = windows.bands(false);
    pgreat.max(great).max(good)
}

pub(super) fn legacy_note_expired(note: &NoteEvent, now: TimeUs, windows: JudgeWindow) -> bool {
    let late_limit = match note.kind {
        NoteKind::LongStart => legacy_ln_margin_us(windows),
        _ => windows.bad_slow_us,
    };
    now.0 / 1_000 * 1_000 - note.time.0 > late_limit
}

pub(super) fn legacy_press_candidate(
    note: &NoteEvent,
    input: TimeUs,
    windows: JudgeWindow,
) -> Option<PressCandidate> {
    if legacy_note_expired(note, input, windows) {
        return None;
    }

    let delta = legacy_offset(input, note.time);
    let judge = classify_normal_delta(delta, windows).or_else(|| {
        (delta > -windows.empty_poor_fast_us && delta < 0).then_some(Judge::EmptyPoor)
    })?;
    Some(PressCandidate { note_id: note.id, judge, delta: TimeUs(delta) })
}

pub(super) fn judge_algorithm_prefers_new_candidate(
    algorithm: JudgeAlgorithm,
    current_delta: i64,
    candidate_delta: i64,
    windows: JudgeWindow,
) -> bool {
    match algorithm {
        JudgeAlgorithm::Combo => {
            current_delta > windows.good_us && candidate_delta >= -windows.bands(true)[2]
        }
        JudgeAlgorithm::Duration => candidate_delta.abs() < current_delta.abs(),
        JudgeAlgorithm::Lowest => false,
        JudgeAlgorithm::Score => {
            current_delta > windows.great_us && candidate_delta >= -windows.bands(true)[1]
        }
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
    let notes = chart.notes_for_lane(lane);
    let earliest = input_time.0.saturating_sub(windows.bad_slow_us);
    let start = notes.partition_point(|note| note.time.0 < earliest);
    let mut candidates = notes[start..]
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

    let selected_in_bad = in_bad_range(selected_candidate.delta.0, windows)
        && !in_good_range(selected_candidate.delta.0, windows);
    if !selected_in_bad || selected_note.kind == NoteKind::LongStart {
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

pub(super) fn classify_normal_delta(delta_us: i64, windows: JudgeWindow) -> Option<Judge> {
    let abs = delta_us.abs();

    let [pgreat, great, good] = windows.bands(delta_us < 0);
    if abs <= pgreat {
        Some(Judge::PGreat)
    } else if abs <= great {
        Some(Judge::Great)
    } else if abs <= good {
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
    delta_us.abs() <= good_width(delta_us, windows)
}

fn good_width(delta_us: i64, windows: JudgeWindow) -> i64 {
    windows.bands(delta_us < 0)[2]
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
    charge_pair_for_start(chart, start_note_id).map(|pair| pair.end_note_id)
}

pub(super) fn charge_pair_for_start(
    chart: &PlayableChart,
    start_note_id: NoteId,
) -> Option<&bmz_chart::model::LongNotePair> {
    chart.long_notes.iter().find(|pair| {
        pair.start_note_id == start_note_id
            && matches!(
                pair.mode.unwrap_or(chart.metadata.long_note_mode),
                LongNoteMode::Cn | LongNoteMode::Hcn
            )
    })
}

pub(super) fn active_long_scores_on_start(chart: &PlayableChart, start_note_id: NoteId) -> bool {
    chart
        .long_notes
        .iter()
        .find(|pair| pair.start_note_id == start_note_id)
        .map(|pair| {
            matches!(
                pair.mode.unwrap_or(chart.metadata.long_note_mode),
                LongNoteMode::Cn | LongNoteMode::Hcn
            )
        })
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
        advances_score: true,
    }
}

pub(super) fn empty_poor(
    lane: Lane,
    side: TimingSide,
    delta: TimeUs,
    time: TimeUs,
    keysound_note_id: NoteId,
    mut events: Vec<JudgementEvent>,
) -> JudgeOutcome {
    events.push(JudgementEvent {
        note_id: None,
        lane,
        judge: Judge::EmptyPoor,
        side,
        delta,
        time,
        affects_score: true,
        advances_score: false,
    });
    JudgeOutcome {
        events,
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
