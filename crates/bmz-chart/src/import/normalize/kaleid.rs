use super::*;
use crate::import::intermediate::KaleidNoteKind;
use crate::model::{FreeZone, LongNoteStyle};

pub(super) fn emit_kaleid_objects(
    intermediate: &IntermediateChart,
    timing: &TimingMap,
    draft: &mut PlayableChartDraft,
    next_id: &mut u32,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), ImportError> {
    let mut notes = Vec::new();
    let mut markers: [Vec<(ChartTick, bool, Option<u64>)>; LANE_COUNT] =
        std::array::from_fn(|_| Vec::new());
    for object in &intermediate.objects {
        let tick = object_to_tick(object, &intermediate.measures)?;
        match object.kind {
            IntermediateObjectKind::KaleidNote { lane, kind, recovery, mode } => {
                notes.push((kind, tick, lane, recovery, mode));
            }
            IntermediateObjectKind::FreeZone { lane, paired, length_ticks } => {
                markers[lane.index()].push((tick, paired, length_ticks));
            }
            IntermediateObjectKind::BarLine => draft.bar_lines.push(BarLine {
                measure: object.measure,
                tick,
                time: timing.tick_to_time(tick),
            }),
            _ => {}
        }
    }
    // Stable ordering gives real potions priority over every dummy shape.
    notes.sort_by_key(|(kind, tick, _, _, _)| (*kind, *tick));
    let mut pending: [Option<(ChartTick, Option<crate::model::LongNoteMode>)>; LANE_COUNT] =
        [None; LANE_COUNT];
    for (kind, tick, lane, recovery, mode) in notes {
        let at = timing.tick_to_time(tick);
        if kind == KaleidNoteKind::DummyLong {
            if let Some((start, mode)) = pending[lane.index()].take() {
                if start == tick || occupied(draft, lane, start, tick) {
                    diagnostic(draft, warnings, format!("conflicting dummy LN on {lane:?}"));
                    continue;
                }
                let start_id = alloc_note_id(next_id);
                let end_id = alloc_note_id(next_id);
                let start_time = timing.tick_to_time(start);
                draft.lane_notes[lane.index()].extend([
                    silent_note(start_id, lane, NoteKind::DummyLongStart, start, start_time, None),
                    silent_note(end_id, lane, NoteKind::DummyLongEnd, tick, at, None),
                ]);
                draft.long_notes.push(LongNotePair {
                    lane,
                    style: LongNoteStyle::ChannelPair,
                    mode: mode.or_else(|| {
                        draft
                            .metadata
                            .long_note_mode_defined
                            .then_some(draft.metadata.long_note_mode)
                    }),
                    start_note_id: start_id,
                    end_note_id: end_id,
                    start_tick: start,
                    end_tick: tick,
                    start_time,
                    end_time: at,
                    sound: None,
                });
            } else {
                pending[lane.index()] = Some((tick, mode));
            }
            continue;
        }
        if occupied(draft, lane, tick, tick) {
            diagnostic(draft, warnings, format!("conflicting {kind:?} on {lane:?}"));
            continue;
        }
        let note_kind = match kind {
            KaleidNoteKind::Potion => NoteKind::Potion,
            KaleidNoteKind::DummyTap => NoteKind::DummyTap,
            KaleidNoteKind::DummyMine => NoteKind::DummyMine,
            KaleidNoteKind::DummyPotion => NoteKind::DummyPotion,
            KaleidNoteKind::DummyLong => unreachable!(),
        };
        draft.lane_notes[lane.index()].push(silent_note(
            alloc_note_id(next_id),
            lane,
            note_kind,
            tick,
            at,
            (kind == KaleidNoteKind::Potion).then_some(recovery),
        ));
    }
    for lane in Lane::ALL {
        if pending[lane.index()].is_some() {
            diagnostic(draft, warnings, format!("unclosed dummy LN on {lane:?}"));
        }
        let markers = &mut markers[lane.index()];
        markers.sort_by_key(|(tick, _, _)| *tick);
        let mut open = None;
        let mut ranges = Vec::new();
        for &(tick, paired, length) in markers.iter() {
            if !paired {
                ranges.push((
                    tick,
                    ChartTick(tick.0.saturating_add(length.unwrap_or(
                        u64::from(crate::timing::TICKS_PER_MEASURE) * IMPORT_TICK_SCALE / 4,
                    ))),
                ));
            } else if let Some(start) = open.take() {
                if tick > start {
                    ranges.push((start, tick));
                }
            } else {
                open = Some(tick);
            }
        }
        if open.is_some() {
            diagnostic(draft, warnings, format!("unclosed free zone on {lane:?}"));
        }
        ranges.sort_unstable_by_key(|&(start, end)| (start, end));
        let mut merged: Vec<(ChartTick, ChartTick)> = Vec::new();
        for (start, end) in ranges {
            if let Some(last) = merged.last_mut().filter(|last| start <= last.1) {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        for (start, end) in merged {
            let mut inner: Vec<NoteId> = draft.lane_notes[lane.index()]
                .iter()
                .filter(|n| {
                    matches!(n.kind, NoteKind::Tap | NoteKind::LongStart | NoteKind::LongEnd)
                        && n.tick >= start
                        && n.tick <= end
                })
                .map(|n| n.id)
                .collect();
            for pair in draft.long_notes.iter().filter(|p| p.lane == lane) {
                if inner.contains(&pair.start_note_id) || inner.contains(&pair.end_note_id) {
                    inner.extend([pair.start_note_id, pair.end_note_id]);
                }
            }
            inner.sort_unstable_by_key(|id| id.0);
            inner.dedup();
            let note_id = alloc_note_id(next_id);
            let end_time = timing.tick_to_time(end);
            draft.lane_notes[lane.index()].push(silent_note(
                note_id,
                lane,
                NoteKind::FreeZoneEnd,
                end,
                end_time,
                None,
            ));
            draft.metadata.free_zones.push(FreeZone {
                note_id,
                lane,
                start_tick: start,
                end_tick: end,
                start_time: timing.tick_to_time(start),
                end_time,
                inner_notes: inner,
            });
        }
    }
    draft.bar_lines.sort_by_key(|line| line.tick);
    draft.bar_lines.dedup_by_key(|line| line.tick);
    Ok(())
}

fn occupied(draft: &PlayableChartDraft, lane: Lane, start: ChartTick, end: ChartTick) -> bool {
    draft.lane_notes[lane.index()]
        .iter()
        .any(|n| n.kind != NoteKind::Invisible && n.tick >= start && n.tick <= end)
        || draft
            .long_notes
            .iter()
            .any(|p| p.lane == lane && p.start_tick <= end && p.end_tick >= start)
}

fn silent_note(
    id: NoteId,
    lane: Lane,
    kind: NoteKind,
    tick: ChartTick,
    time: TimeUs,
    damage: Option<f64>,
) -> NoteEvent {
    NoteEvent { id, lane, kind, tick, time, sound: None, layered_sounds: Vec::new(), damage }
}

fn diagnostic(draft: &mut PlayableChartDraft, warnings: &mut Vec<ImportWarning>, message: String) {
    draft.metadata.kaleid.diagnostics.push(message.clone());
    warnings.push(ImportWarning::ParserDiagnostic { code: "KaleidObject".into(), message });
}
