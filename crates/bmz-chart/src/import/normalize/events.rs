use super::*;

pub(super) fn resolve_bga_asset_id(
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

pub(super) fn build_bga_events(
    tick_objects: &[TickObject],
    timing_map: &TimingMap,
    bga_table: &BgaTable,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<BgaEvent> {
    tick_objects
        .iter()
        .filter_map(|object| match object.kind {
            TickObjectKind::Bga { bmp_key, kind } => {
                let asset = resolve_bga_asset_id(bmp_key, bga_table, warnings);
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

pub(super) fn build_timing_events(
    initial_bpm: f64,
    events: &[TickTimingEvent],
    timing_map: &TimingMap,
) -> Vec<TimingEvent> {
    let mut events = events.to_vec();
    events.sort_by_key(|event| {
        (
            event.tick,
            match event.kind {
                // Match beatoraja: BPM changes take effect before a STOP at
                // the same position, so the STOP uses the new BPM.
                TickTimingEventKind::SetBpm(_) | TickTimingEventKind::SetNegativeBmsBpm(_) => 0,
                TickTimingEventKind::StopRaw { .. } | TickTimingEventKind::BmsonStop { .. } => 1,
            },
        )
    });

    let mut bpm = initial_bpm;
    let mut normalized = Vec::with_capacity(events.len());
    for event in events {
        let time = timing_map.tick_to_time(event.tick);
        let kind = match event.kind {
            TickTimingEventKind::SetBpm(next_bpm) => {
                bpm = next_bpm;
                TimingEventKind::BpmChange { bpm }
            }
            TickTimingEventKind::SetNegativeBmsBpm(magnitude) => {
                bpm = magnitude;
                normalized.push(TimingEvent {
                    tick: event.tick,
                    time,
                    kind: TimingEventKind::BpmReflection,
                });
                TimingEventKind::BpmChange { bpm }
            }
            TickTimingEventKind::StopRaw { value } => {
                TimingEventKind::Stop { duration_us: crate::timing::stop_raw_to_us(value, bpm) }
            }
            TickTimingEventKind::BmsonStop { duration_pulses, resolution } => {
                TimingEventKind::Stop {
                    duration_us: crate::timing::bmson_stop_pulses_to_us(
                        duration_pulses,
                        resolution,
                        bpm,
                    ),
                }
            }
        };

        normalized.push(TimingEvent { tick: event.tick, time, kind });
    }
    normalized
}

pub(super) fn build_scroll_events(
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

pub(super) fn build_speed_events(
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

pub(super) fn build_judge_rank_events(
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

pub(super) fn build_chart_volume_events(
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

pub(super) fn build_text_events(
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

pub(super) fn build_bga_opacity_events(
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

pub(super) fn build_bga_argb_events(
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

pub(super) fn build_swbga_definitions(intermediate: &IntermediateChart) -> Vec<SwBgaDefinition> {
    intermediate
        .resources
        .swbga_defs
        .iter()
        .map(|def| SwBgaDefinition {
            id: def.id,
            frame_rate_ms: def.frame_rate_ms,
            total_time_ms: def.total_time_ms,
            line: def.line,
            loop_mode: def.loop_mode,
            chroma_alpha: def.chroma_alpha,
            chroma_red: def.chroma_red,
            chroma_green: def.chroma_green,
            chroma_blue: def.chroma_blue,
            pattern_bmp_keys: parse_swbga_pattern(
                &def.pattern,
                intermediate.metadata.base62_obj_ids,
            ),
        })
        .collect()
}

pub(super) fn build_bga_keybound_events(
    intermediate: &IntermediateChart,
    timing_map: &TimingMap,
) -> Result<Vec<BgaKeyboundEvent>, ImportError> {
    let mut out = Vec::new();
    for object in &intermediate.objects {
        let IntermediateObjectKind::BgaKeybound { swbga_key } = object.kind else {
            continue;
        };
        let tick = object_to_tick(object, &intermediate.measures)?;
        out.push(BgaKeyboundEvent {
            tick,
            time: timing_map.tick_to_time(tick),
            swbga_id: swbga_key,
        });
    }
    Ok(out)
}

pub(super) fn build_bar_lines(measures: &[MeasureInfo], timing_map: &TimingMap) -> Vec<BarLine> {
    measures
        .iter()
        .map(|measure| BarLine {
            measure: measure.index,
            tick: measure.start_tick,
            time: timing_map.tick_to_time(measure.start_tick),
        })
        .collect()
}
