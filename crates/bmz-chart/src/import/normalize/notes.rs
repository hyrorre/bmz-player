use super::*;

pub(crate) fn object_to_tick(
    object: &IntermediateObject,
    measures: &[MeasureInfo],
) -> Result<ChartTick, ImportError> {
    position_to_tick(object.measure, object.position_num, object.position_den, measures)
}

fn position_to_tick(
    measure_index: u32,
    position_num: u32,
    position_den: u32,
    measures: &[MeasureInfo],
) -> Result<ChartTick, ImportError> {
    if position_den == 0 {
        return Err(ImportError::InvalidChart {
            message: "object position denominator is zero".to_string(),
        });
    }

    let measure =
        measures.iter().find(|measure| measure.index == measure_index).ok_or_else(|| {
            ImportError::InvalidChart { message: format!("missing measure {measure_index}") }
        })?;

    let local_tick = measure.tick_len.saturating_mul(position_num as u64) / position_den as u64;
    Ok(ChartTick(measure.start_tick.0.saturating_add(local_tick)))
}

pub(crate) fn collect_timing_events(
    intermediate: &IntermediateChart,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Vec<TickTimingEvent>, ImportError> {
    let mut events = Vec::new();

    for object in &intermediate.objects {
        let tick = object_to_tick(object, &intermediate.measures)?;
        match object.kind {
            IntermediateObjectKind::SetBpm { bpm } => {
                events.push(TickTimingEvent { tick, kind: TickTimingEventKind::SetBpm(bpm) });
            }
            IntermediateObjectKind::SetExtendedBpm { bpm_key } => {
                if let Some(def) =
                    intermediate.resources.bpm_table.iter().find(|def| def.key == bpm_key)
                {
                    events
                        .push(TickTimingEvent { tick, kind: TickTimingEventKind::SetBpm(def.bpm) });
                } else {
                    warnings.push(ImportWarning::MissingBpmDefinition { key: bpm_key });
                }
            }
            IntermediateObjectKind::Stop { stop_key } => {
                if let Some(def) =
                    intermediate.resources.stop_table.iter().find(|def| def.key == stop_key)
                {
                    events.push(TickTimingEvent {
                        tick,
                        kind: TickTimingEventKind::StopRaw { value: def.value },
                    });
                } else {
                    warnings.push(ImportWarning::MissingStopDefinition { key: stop_key });
                }
            }
            IntermediateObjectKind::BmsonStop { duration_pulses, resolution } => {
                events.push(TickTimingEvent {
                    tick,
                    kind: TickTimingEventKind::BmsonStop { duration_pulses, resolution },
                });
            }
            _ => {}
        }
    }

    Ok(events)
}

pub(super) fn collect_lane_objects(
    tick_objects: &[TickObject],
    timing_map: &TimingMap,
) -> [Vec<LaneObject>; LANE_COUNT] {
    let mut buckets: [Vec<LaneObject>; LANE_COUNT] = std::array::from_fn(|_| Vec::new());
    let mut visible_by_tick: [HashMap<ChartTick, usize>; LANE_COUNT] =
        std::array::from_fn(|_| HashMap::new());

    for object in tick_objects {
        let time = timing_map.tick_to_time(object.tick);
        match object.kind {
            TickObjectKind::VisibleNote { lane, wav_key } => {
                let lane_index = lane.index();
                let lane_object = LaneObject {
                    lane,
                    tick: object.tick,
                    time,
                    wav_key,
                    source: LaneObjectSource::Visible,
                };
                if let Some(index) = visible_by_tick[lane_index].get(&object.tick).copied() {
                    // beatoraja/jbms-parser merges repeated definitions of the same
                    // visible lane position, with the later definition taking effect.
                    buckets[lane_index][index] = lane_object;
                } else {
                    let index = buckets[lane_index].len();
                    buckets[lane_index].push(lane_object);
                    visible_by_tick[lane_index].insert(object.tick, index);
                }
            }
            TickObjectKind::InvisibleNote { lane, wav_key } => {
                buckets[lane.index()].push(LaneObject {
                    lane,
                    tick: object.tick,
                    time,
                    wav_key,
                    source: LaneObjectSource::Invisible,
                });
            }
            TickObjectKind::LongChannelNote { lane, wav_key, mode, explicit_end_sound } => {
                buckets[lane.index()].push(LaneObject {
                    lane,
                    tick: object.tick,
                    time,
                    wav_key,
                    source: LaneObjectSource::LongChannel { mode, explicit_end_sound },
                });
            }
            TickObjectKind::MineNote { lane, wav_key, damage } => {
                buckets[lane.index()].push(LaneObject {
                    lane,
                    tick: object.tick,
                    time,
                    wav_key,
                    source: LaneObjectSource::Mine { damage },
                });
            }
            TickObjectKind::Bgm { .. } | TickObjectKind::Bga { .. } => {}
        }
    }

    for bucket in &mut buckets {
        bucket.sort_by_key(|object| object.time);
    }

    buckets
}

pub(super) fn emit_resolved_lane_events(
    lane: Lane,
    events: Vec<ResolvedLaneEvent>,
    sound_table: &SoundTable,
    draft: &mut PlayableChartDraft,
    next_note_id: &mut u32,
    warnings: &mut Vec<ImportWarning>,
) {
    for event in events {
        match event {
            ResolvedLaneEvent::Tap { tick, time, wav_key, .. } => {
                let id = alloc_note_id(next_note_id);
                draft.lane_notes[lane.index()].push(NoteEvent {
                    id,
                    lane,
                    kind: NoteKind::Tap,
                    tick,
                    time,
                    sound: resolve_sound_id(wav_key, sound_table, warnings),
                    layered_sounds: Vec::new(),
                    damage: None,
                });
            }
            ResolvedLaneEvent::Invisible { tick, time, wav_key, .. } => {
                let id = alloc_note_id(next_note_id);
                draft.lane_notes[lane.index()].push(NoteEvent {
                    id,
                    lane,
                    kind: NoteKind::Invisible,
                    tick,
                    time,
                    sound: resolve_sound_id(wav_key, sound_table, warnings),
                    layered_sounds: Vec::new(),
                    damage: None,
                });
            }
            ResolvedLaneEvent::Mine { tick, time, wav_key, damage, .. } => {
                let id = alloc_note_id(next_note_id);
                draft.lane_notes[lane.index()].push(NoteEvent {
                    id,
                    lane,
                    kind: NoteKind::Mine,
                    tick,
                    time,
                    sound: resolve_sound_id(wav_key, sound_table, warnings),
                    layered_sounds: Vec::new(),
                    damage: Some(damage),
                });
            }
            ResolvedLaneEvent::Long { pair } => {
                let mode = pair.mode.or_else(|| {
                    draft.metadata.long_note_mode_defined.then_some(draft.metadata.long_note_mode)
                });
                if mode == Some(crate::model::LongNoteMode::Hln) && pair.start_time >= pair.end_time
                {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "InvalidHlnLength".to_string(),
                        message: format!("HLN on {lane:?} has no positive duration; ignored"),
                    });
                    continue;
                }
                let start_note_id = alloc_note_id(next_note_id);
                let end_note_id = alloc_note_id(next_note_id);
                let sound = resolve_sound_id(pair.wav_key, sound_table, warnings);
                let end_sound = resolve_sound_id(pair.end_wav_key, sound_table, warnings);

                draft.lane_notes[lane.index()].push(NoteEvent {
                    id: start_note_id,
                    lane,
                    kind: NoteKind::LongStart,
                    tick: pair.start_tick,
                    time: pair.start_time,
                    sound,
                    layered_sounds: Vec::new(),
                    damage: None,
                });
                draft.lane_notes[lane.index()].push(NoteEvent {
                    id: end_note_id,
                    lane,
                    kind: NoteKind::LongEnd,
                    tick: pair.end_tick,
                    time: pair.end_time,
                    sound: end_sound,
                    layered_sounds: Vec::new(),
                    damage: None,
                });
                draft.long_notes.push(LongNotePair {
                    lane,
                    style: pair.style,
                    mode: pair.mode.or_else(|| {
                        draft
                            .metadata
                            .long_note_mode_defined
                            .then_some(draft.metadata.long_note_mode)
                    }),
                    start_note_id,
                    end_note_id,
                    start_tick: pair.start_tick,
                    end_tick: pair.end_tick,
                    start_time: pair.start_time,
                    end_time: pair.end_time,
                    sound,
                });
            }
        }
    }
}

pub(super) fn apply_layered_note_sounds(
    layers: &[IntermediateLayeredSound],
    measures: &[MeasureInfo],
    sound_table: &SoundTable,
    draft: &mut PlayableChartDraft,
    warnings: &mut Vec<ImportWarning>,
    end_only: bool,
) -> Result<(), ImportError> {
    for layer in layers {
        let tick =
            position_to_tick(layer.measure, layer.position_num, layer.position_den, measures)?;
        let Some(sound_id) = resolve_sound_id(Some(layer.wav_key), sound_table, warnings) else {
            continue;
        };
        let Some(note_index) = draft.lane_notes[layer.lane.index()].iter().position(|note| {
            note.tick == tick
                && if end_only {
                    note.kind == NoteKind::LongEnd
                } else {
                    matches!(note.kind, NoteKind::Tap | NoteKind::LongStart)
                }
        }) else {
            continue;
        };
        if draft.lane_notes[layer.lane.index()][note_index].sounds().any(|id| id == sound_id) {
            continue;
        }
        let sound_id = if end_only {
            // HCNの始点ミュートやCNの早離しミュートで、同じWAVの終端音を巻き込まない。
            let mut asset = draft
                .sounds
                .iter()
                .find(|asset| asset.id == sound_id)
                .expect("resolved sound asset")
                .clone();
            asset.id = SoundId(
                draft
                    .sounds
                    .iter()
                    .map(|asset| asset.id.0)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .expect("long end sound id space exhausted"),
            );
            let id = asset.id;
            draft.sounds.push(asset);
            id
        } else {
            sound_id
        };
        let note = &mut draft.lane_notes[layer.lane.index()][note_index];
        if end_only && note.sound.is_none() {
            note.sound = Some(sound_id);
        } else if note.sound != Some(sound_id) && !note.layered_sounds.contains(&sound_id) {
            note.layered_sounds.push(sound_id);
        }
    }
    Ok(())
}

pub(super) fn alloc_note_id(next_note_id: &mut u32) -> NoteId {
    let id = NoteId(*next_note_id);
    *next_note_id += 1;
    id
}

pub(super) fn build_bgm_events(
    tick_objects: &[TickObject],
    timing_map: &TimingMap,
    sound_table: &SoundTable,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<SoundEvent> {
    tick_objects
        .iter()
        .filter_map(|object| match object.kind {
            TickObjectKind::Bgm { wav_key } => {
                let sound = resolve_sound_id(Some(wav_key), sound_table, warnings)?;
                Some(SoundEvent {
                    tick: object.tick,
                    time: timing_map.tick_to_time(object.tick),
                    sound,
                })
            }
            _ => None,
        })
        .collect()
}
