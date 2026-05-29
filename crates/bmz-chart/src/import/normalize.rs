use std::collections::HashMap;
use std::path::Path;

use bmz_core::ids::{NoteId, SoundId};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::{ChartTick, TimeUs};

use crate::model::{
    BarLine, BgaArgbEvent, BgaAssetId, BgaAssetKind, BgaAssetRef, BgaEvent, BgaEventKind,
    BgaOpacityEvent, ChartMetadata, ChartTextEvent, ChartVolumeEvent, JudgeRankEvent, LongNotePair,
    NoteEvent, NoteKind, PlayableChart, ScrollEvent, SoundAssetRef, SoundEvent, SpeedEvent,
    TimingEvent, TimingEventKind,
};
use crate::timing::{TickTimingEvent, TickTimingEventKind, TimingMap, build_timing_map};

use super::error::{ImportError, ImportWarning};
use super::intermediate::{
    IntermediateChart, IntermediateMetadata, IntermediateObject, IntermediateObjectKind,
    LaneObject, LaneObjectSource, MeasureInfo, ResolvedLaneEvent,
};
use super::long_note::normalize_lane_objects;

#[derive(Debug, Clone)]
struct SoundTable {
    by_wav_key: HashMap<u16, SoundId>,
    assets: Vec<SoundAssetRef>,
}

#[derive(Debug, Clone)]
struct BgaTable {
    by_bmp_key: HashMap<u16, BgaAssetId>,
    assets: Vec<BgaAssetRef>,
}

#[derive(Debug, Clone)]
struct TickObject {
    tick: ChartTick,
    kind: TickObjectKind,
}

#[derive(Debug, Clone)]
enum TickObjectKind {
    VisibleNote { lane: Lane, wav_key: Option<u16> },
    InvisibleNote { lane: Lane, wav_key: Option<u16> },
    LongChannelNote { lane: Lane, wav_key: Option<u16> },
    MineNote { lane: Lane, wav_key: Option<u16>, damage: u16 },
    Bgm { wav_key: u16 },
    Bga { bmp_key: u16, kind: BgaEventKind },
}

#[derive(Debug, Clone)]
struct PlayableChartDraft {
    identity: bmz_core::chart::ChartIdentity,
    metadata: ChartMetadata,
    lane_notes: [Vec<NoteEvent>; LANE_COUNT],
    long_notes: Vec<LongNotePair>,
    bgm_events: Vec<SoundEvent>,
    bga_events: Vec<BgaEvent>,
    timing_events: Vec<TimingEvent>,
    scroll_events: Vec<ScrollEvent>,
    speed_events: Vec<SpeedEvent>,
    judge_rank_events: Vec<JudgeRankEvent>,
    bgm_volume_events: Vec<ChartVolumeEvent>,
    key_volume_events: Vec<ChartVolumeEvent>,
    text_events: Vec<ChartTextEvent>,
    bga_opacity_events: Vec<BgaOpacityEvent>,
    bga_argb_events: Vec<BgaArgbEvent>,
    bar_lines: Vec<BarLine>,
    sounds: Vec<SoundAssetRef>,
    bga_assets: Vec<BgaAssetRef>,
    total_notes: u32,
    end_time: TimeUs,
}

pub fn normalize_chart(
    source_path: &Path,
    intermediate: IntermediateChart,
    warnings: &mut Vec<ImportWarning>,
    check_resource_existence: bool,
) -> Result<PlayableChart, ImportError> {
    let metadata = normalize_metadata(&intermediate.metadata);
    let sound_table =
        build_sound_table(source_path, &intermediate, warnings, check_resource_existence);
    let bga_table = build_bga_table(source_path, &intermediate, warnings, check_resource_existence);
    let tick_objects = materialize_tick_objects(&intermediate)?;
    let tick_timing_events = collect_timing_events(&intermediate, warnings)?;
    let timing_map =
        build_timing_map(intermediate.metadata.initial_bpm.max(1.0), tick_timing_events.clone());

    let mut draft = PlayableChartDraft::new(
        intermediate.identity.clone(),
        metadata,
        sound_table.assets.clone(),
        bga_table.assets.clone(),
    );
    let lane_buckets = collect_lane_objects(&tick_objects, &timing_map);

    let mut next_note_id = 0_u32;
    for lane in Lane::ALL {
        let resolved = normalize_lane_objects(
            lane,
            &lane_buckets[lane.index()],
            intermediate.lnobj_wav_key,
            warnings,
        );
        emit_resolved_lane_events(
            lane,
            resolved,
            &sound_table,
            &mut draft,
            &mut next_note_id,
            warnings,
        );
    }

    draft.bgm_events = build_bgm_events(&tick_objects, &timing_map, &sound_table, warnings);
    draft.bga_events = build_bga_events(&tick_objects, &timing_map, &bga_table, warnings);
    draft.timing_events = build_timing_events(
        intermediate.metadata.initial_bpm.max(1.0),
        &tick_timing_events,
        &timing_map,
    );
    draft.scroll_events = build_scroll_events(&intermediate, &timing_map)?;
    draft.speed_events = build_speed_events(&intermediate, &timing_map)?;
    draft.judge_rank_events = build_judge_rank_events(&intermediate, &timing_map)?;
    draft.bgm_volume_events = build_chart_volume_events(&intermediate, &timing_map, true)?;
    draft.key_volume_events = build_chart_volume_events(&intermediate, &timing_map, false)?;
    draft.text_events = build_text_events(&intermediate, &timing_map)?;
    draft.bga_opacity_events = build_bga_opacity_events(&intermediate, &timing_map)?;
    draft.bga_argb_events = build_bga_argb_events(&intermediate, &timing_map)?;
    draft.bar_lines = build_bar_lines(&intermediate.measures, &timing_map);

    Ok(finalize_playable_chart(draft))
}

fn normalize_metadata(input: &IntermediateMetadata) -> ChartMetadata {
    ChartMetadata {
        title: input.title.clone(),
        subtitle: input.subtitle.clone(),
        artist: input.artist.clone(),
        subartist: input.subartist.clone(),
        genre: input.genre.clone(),
        difficulty_name: input.difficulty_name.clone(),
        judge_rank: input.judge_rank,
        play_level: input.play_level.clone(),
        initial_bpm: input.initial_bpm,
        total: input.total,
        stage_file: input.stage_file.clone(),
        banner_file: input.banner_file.clone(),
        backbmp_file: input.backbmp_file.clone(),
        preview_file: input.preview_file.clone(),
        volwav_percent: input.volwav_percent,
        has_bga: input.has_bga,
        key_mode: input.key_mode,
    }
}

fn build_sound_table(
    source_path: &Path,
    intermediate: &IntermediateChart,
    warnings: &mut Vec<ImportWarning>,
    check_resource_existence: bool,
) -> SoundTable {
    let mut by_wav_key = HashMap::new();
    let mut assets = Vec::new();
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new(""));

    for wav in &intermediate.resources.wavs {
        let id = SoundId(assets.len() as u32);
        let path = if wav.path.is_absolute() { wav.path.clone() } else { base_dir.join(&wav.path) };
        if check_resource_existence && !path.exists() {
            warnings.push(ImportWarning::MissingSoundFile { path: path.clone() });
        }
        by_wav_key.insert(wav.key, id);
        assets.push(SoundAssetRef { id, path });
    }

    SoundTable { by_wav_key, assets }
}

fn build_bga_table(
    source_path: &Path,
    intermediate: &IntermediateChart,
    warnings: &mut Vec<ImportWarning>,
    check_resource_existence: bool,
) -> BgaTable {
    let mut by_bmp_key = HashMap::new();
    let mut assets = Vec::new();
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new(""));

    for bmp in &intermediate.resources.bmps {
        let id = BgaAssetId(assets.len() as u32);
        let path = if bmp.path.is_absolute() { bmp.path.clone() } else { base_dir.join(&bmp.path) };
        if check_resource_existence && !path.exists() {
            warnings.push(ImportWarning::MissingBmpFile { path: path.clone() });
        }
        by_bmp_key.insert(bmp.key, id);
        assets.push(BgaAssetRef { id, path, kind: bga_asset_kind(&bmp.path) });
    }

    BgaTable { by_bmp_key, assets }
}

fn resolve_sound_id(
    wav_key: Option<u16>,
    table: &SoundTable,
    warnings: &mut Vec<ImportWarning>,
) -> Option<SoundId> {
    let key = wav_key?;
    match table.by_wav_key.get(&key).copied() {
        Some(id) => Some(id),
        None => {
            warnings.push(ImportWarning::MissingWavDefinition { key });
            None
        }
    }
}

fn materialize_tick_objects(
    intermediate: &IntermediateChart,
) -> Result<Vec<TickObject>, ImportError> {
    let mut out = Vec::new();

    for object in &intermediate.objects {
        let tick = object_to_tick(object, &intermediate.measures)?;
        let kind = match object.kind {
            IntermediateObjectKind::VisibleNote { lane, wav_key } => {
                Some(TickObjectKind::VisibleNote { lane, wav_key })
            }
            IntermediateObjectKind::InvisibleNote { lane, wav_key } => {
                Some(TickObjectKind::InvisibleNote { lane, wav_key })
            }
            IntermediateObjectKind::LongChannelNote { lane, wav_key } => {
                Some(TickObjectKind::LongChannelNote { lane, wav_key })
            }
            IntermediateObjectKind::MineNote { lane, wav_key, damage } => {
                Some(TickObjectKind::MineNote { lane, wav_key, damage })
            }
            IntermediateObjectKind::Bgm { wav_key } => Some(TickObjectKind::Bgm { wav_key }),
            IntermediateObjectKind::Bga { bmp_key, kind } => {
                Some(TickObjectKind::Bga { bmp_key, kind: bga_event_kind(kind) })
            }
            IntermediateObjectKind::SetBpm { .. }
            | IntermediateObjectKind::SetExtendedBpm { .. }
            | IntermediateObjectKind::Stop { .. }
            | IntermediateObjectKind::SetScroll { .. }
            | IntermediateObjectKind::SetSpeed { .. }
            | IntermediateObjectKind::SetJudgeRank { .. }
            | IntermediateObjectKind::SetBgmVolume { .. }
            | IntermediateObjectKind::SetKeyVolume { .. }
            | IntermediateObjectKind::SetText { .. }
            | IntermediateObjectKind::SetBgaOpacity { .. }
            | IntermediateObjectKind::SetBgaArgb { .. } => None,
        };

        if let Some(kind) = kind {
            out.push(TickObject { tick, kind });
        }
    }

    out.sort_by_key(|object| object.tick);
    Ok(out)
}

fn bga_event_kind(kind: super::intermediate::IntermediateBgaKind) -> BgaEventKind {
    match kind {
        super::intermediate::IntermediateBgaKind::Base => BgaEventKind::Base,
        super::intermediate::IntermediateBgaKind::Poor => BgaEventKind::Poor,
        super::intermediate::IntermediateBgaKind::Layer => BgaEventKind::Layer,
    }
}

fn bga_asset_kind(path: &Path) -> BgaAssetKind {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => match ext.to_ascii_lowercase().as_str() {
            "mp4" | "avi" | "wmv" | "mpg" | "mpeg" | "mkv" | "mov" => BgaAssetKind::Video,
            _ => BgaAssetKind::Static,
        },
        None => BgaAssetKind::Static,
    }
}

fn object_to_tick(
    object: &IntermediateObject,
    measures: &[MeasureInfo],
) -> Result<ChartTick, ImportError> {
    if object.position_den == 0 {
        return Err(ImportError::InvalidChart {
            message: "object position denominator is zero".to_string(),
        });
    }

    let measure =
        measures.iter().find(|measure| measure.index == object.measure).ok_or_else(|| {
            ImportError::InvalidChart { message: format!("missing measure {}", object.measure) }
        })?;

    let local_tick =
        measure.tick_len.saturating_mul(object.position_num as u64) / object.position_den as u64;
    Ok(ChartTick(measure.start_tick.0 + local_tick))
}

fn collect_timing_events(
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
            _ => {}
        }
    }

    Ok(events)
}

fn collect_lane_objects(
    tick_objects: &[TickObject],
    timing_map: &TimingMap,
) -> [Vec<LaneObject>; LANE_COUNT] {
    let mut buckets: [Vec<LaneObject>; LANE_COUNT] = std::array::from_fn(|_| Vec::new());

    for object in tick_objects {
        let time = timing_map.tick_to_time(object.tick);
        match object.kind {
            TickObjectKind::VisibleNote { lane, wav_key } => {
                buckets[lane.index()].push(LaneObject {
                    lane,
                    tick: object.tick,
                    time,
                    wav_key,
                    source: LaneObjectSource::Visible,
                });
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
            TickObjectKind::LongChannelNote { lane, wav_key } => {
                buckets[lane.index()].push(LaneObject {
                    lane,
                    tick: object.tick,
                    time,
                    wav_key,
                    source: LaneObjectSource::LongChannel,
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

fn emit_resolved_lane_events(
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
                    damage: Some(damage),
                });
            }
            ResolvedLaneEvent::Long { pair } => {
                let start_note_id = alloc_note_id(next_note_id);
                let end_note_id = alloc_note_id(next_note_id);
                let sound = resolve_sound_id(pair.wav_key, sound_table, warnings);

                draft.lane_notes[lane.index()].push(NoteEvent {
                    id: start_note_id,
                    lane,
                    kind: NoteKind::LongStart,
                    tick: pair.start_tick,
                    time: pair.start_time,
                    sound,
                    damage: None,
                });
                draft.lane_notes[lane.index()].push(NoteEvent {
                    id: end_note_id,
                    lane,
                    kind: NoteKind::LongEnd,
                    tick: pair.end_tick,
                    time: pair.end_time,
                    sound: None,
                    damage: None,
                });
                draft.long_notes.push(LongNotePair {
                    lane,
                    style: pair.style,
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

fn alloc_note_id(next_note_id: &mut u32) -> NoteId {
    let id = NoteId(*next_note_id);
    *next_note_id += 1;
    id
}

fn build_bgm_events(
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

fn resolve_bga_asset_id(
    bmp_key: u16,
    table: &BgaTable,
    warnings: &mut Vec<ImportWarning>,
) -> Option<BgaAssetId> {
    match table.by_bmp_key.get(&bmp_key).copied() {
        Some(id) => Some(id),
        None => {
            warnings.push(ImportWarning::MissingBmpDefinition { key: bmp_key });
            None
        }
    }
}

fn build_bga_events(
    tick_objects: &[TickObject],
    timing_map: &TimingMap,
    bga_table: &BgaTable,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<BgaEvent> {
    tick_objects
        .iter()
        .filter_map(|object| match object.kind {
            TickObjectKind::Bga { bmp_key, kind } => {
                let asset = resolve_bga_asset_id(bmp_key, bga_table, warnings)?;
                Some(BgaEvent {
                    tick: object.tick,
                    time: timing_map.tick_to_time(object.tick),
                    asset,
                    kind,
                })
            }
            _ => None,
        })
        .collect()
}

fn build_timing_events(
    initial_bpm: f64,
    events: &[TickTimingEvent],
    timing_map: &TimingMap,
) -> Vec<TimingEvent> {
    let mut events = events.to_vec();
    events.sort_by_key(|event| {
        (
            event.tick,
            match event.kind {
                TickTimingEventKind::StopRaw { .. } => 0,
                TickTimingEventKind::SetBpm(_) => 1,
            },
        )
    });

    let mut bpm = initial_bpm;
    events
        .iter()
        .map(|event| {
            let kind = match event.kind {
                TickTimingEventKind::SetBpm(next_bpm) => {
                    bpm = next_bpm;
                    TimingEventKind::BpmChange { bpm: next_bpm }
                }
                TickTimingEventKind::StopRaw { value } => {
                    TimingEventKind::Stop { duration_us: crate::timing::stop_raw_to_us(value, bpm) }
                }
            };

            TimingEvent { tick: event.tick, time: timing_map.tick_to_time(event.tick), kind }
        })
        .collect()
}

fn build_scroll_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
) -> Result<Vec<ScrollEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        if let IntermediateObjectKind::SetScroll { factor } = object.kind {
            let tick = object_to_tick(object, &intermediate.measures)?;
            out.push(ScrollEvent { tick, time: timing_map.tick_to_time(tick), factor });
        }
    }
    Ok(out)
}

fn build_speed_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
) -> Result<Vec<SpeedEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        if let IntermediateObjectKind::SetSpeed { factor } = object.kind {
            let tick = object_to_tick(object, &intermediate.measures)?;
            out.push(SpeedEvent { tick, time: timing_map.tick_to_time(tick), factor });
        }
    }
    Ok(out)
}

fn build_judge_rank_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
) -> Result<Vec<JudgeRankEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        if let IntermediateObjectKind::SetJudgeRank { rank_percent } = object.kind {
            let tick = object_to_tick(object, &intermediate.measures)?;
            out.push(JudgeRankEvent { tick, time: timing_map.tick_to_time(tick), rank_percent });
        }
    }
    Ok(out)
}

fn build_chart_volume_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
    bgm: bool,
) -> Result<Vec<ChartVolumeEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        let value = match (bgm, &object.kind) {
            (true, IntermediateObjectKind::SetBgmVolume { volume }) => *volume,
            (false, IntermediateObjectKind::SetKeyVolume { volume }) => *volume,
            _ => continue,
        };
        let tick = object_to_tick(object, &intermediate.measures)?;
        out.push(ChartVolumeEvent { tick, time: timing_map.tick_to_time(tick), value });
    }
    Ok(out)
}

fn build_text_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
) -> Result<Vec<ChartTextEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        let IntermediateObjectKind::SetText { text } = &object.kind else {
            continue;
        };
        let tick = object_to_tick(object, &intermediate.measures)?;
        out.push(ChartTextEvent { tick, time: timing_map.tick_to_time(tick), text: text.clone() });
    }
    Ok(out)
}

fn build_bga_opacity_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
) -> Result<Vec<BgaOpacityEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        let IntermediateObjectKind::SetBgaOpacity { kind, opacity } = object.kind else {
            continue;
        };
        let tick = object_to_tick(object, &intermediate.measures)?;
        out.push(BgaOpacityEvent {
            tick,
            time: timing_map.tick_to_time(tick),
            layer: bga_event_kind(kind),
            opacity,
        });
    }
    Ok(out)
}

fn build_bga_argb_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
) -> Result<Vec<BgaArgbEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        let IntermediateObjectKind::SetBgaArgb { kind, alpha, red, green, blue } = object.kind
        else {
            continue;
        };
        let tick = object_to_tick(object, &intermediate.measures)?;
        out.push(BgaArgbEvent {
            tick,
            time: timing_map.tick_to_time(tick),
            layer: bga_event_kind(kind),
            alpha,
            red,
            green,
            blue,
        });
    }
    Ok(out)
}

fn build_bar_lines(measures: &[MeasureInfo], timing_map: &TimingMap) -> Vec<BarLine> {
    measures
        .iter()
        .map(|measure| BarLine {
            measure: measure.index,
            tick: measure.start_tick,
            time: timing_map.tick_to_time(measure.start_tick),
        })
        .collect()
}

fn finalize_playable_chart(mut draft: PlayableChartDraft) -> PlayableChart {
    for lane_notes in &mut draft.lane_notes {
        lane_notes.sort_by_key(|note| note.time);
    }
    draft.long_notes.sort_by_key(|pair| pair.start_time);
    draft.bgm_events.sort_by_key(|event| event.time);
    draft.bga_events.sort_by_key(|event| event.time);
    draft.timing_events.sort_by_key(|event| event.time);
    draft.scroll_events.sort_by_key(|event| event.time);
    draft.speed_events.sort_by_key(|event| event.time);
    draft.judge_rank_events.sort_by_key(|event| event.time);
    draft.bgm_volume_events.sort_by_key(|event| event.time);
    draft.key_volume_events.sort_by_key(|event| event.time);
    draft.text_events.sort_by_key(|event| event.time);
    draft.bga_opacity_events.sort_by_key(|event| event.time);
    draft.bga_argb_events.sort_by_key(|event| event.time);
    draft.bar_lines.sort_by_key(|line| line.time);

    draft.total_notes = compute_total_notes(&draft.lane_notes);
    draft.end_time = compute_end_time(&draft);

    PlayableChart {
        identity: draft.identity,
        metadata: draft.metadata,
        lane_notes: draft.lane_notes,
        long_notes: draft.long_notes,
        bgm_events: draft.bgm_events,
        bga_events: draft.bga_events,
        timing_events: draft.timing_events,
        scroll_events: draft.scroll_events,
        speed_events: draft.speed_events,
        judge_rank_events: draft.judge_rank_events,
        bgm_volume_events: draft.bgm_volume_events,
        key_volume_events: draft.key_volume_events,
        text_events: draft.text_events,
        bga_opacity_events: draft.bga_opacity_events,
        bga_argb_events: draft.bga_argb_events,
        bar_lines: draft.bar_lines,
        sounds: draft.sounds,
        bga_assets: draft.bga_assets,
        total_notes: draft.total_notes,
        end_time: draft.end_time,
    }
}

fn compute_total_notes(lane_notes: &[Vec<NoteEvent>; LANE_COUNT]) -> u32 {
    lane_notes
        .iter()
        .flat_map(|notes| notes.iter())
        .filter(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
        .count() as u32
}

fn compute_end_time(draft: &PlayableChartDraft) -> TimeUs {
    let lane_end = draft
        .lane_notes
        .iter()
        .flat_map(|notes| notes.iter().map(|note| note.time.0))
        .max()
        .unwrap_or(0);
    let bgm_end = draft.bgm_events.iter().map(|event| event.time.0).max().unwrap_or(0);
    TimeUs(lane_end.max(bgm_end))
}

impl PlayableChartDraft {
    fn new(
        identity: bmz_core::chart::ChartIdentity,
        metadata: ChartMetadata,
        sounds: Vec<SoundAssetRef>,
        bga_assets: Vec<BgaAssetRef>,
    ) -> Self {
        Self {
            identity,
            metadata,
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
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
            bar_lines: Vec::new(),
            sounds,
            bga_assets,
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }
}
