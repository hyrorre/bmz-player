use super::*;

/// bms-rs keeps #BMP00 outside bmp_files. Restore its resource and implicit
/// initial Poor event only for the BMS/PMS import path, after RANDOM selection.
pub(super) fn restore_default_poor_bga(chart: &mut IntermediateChart, path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    if let Some(bmp) = chart.resources.bmps.iter_mut().find(|bmp| bmp.key == 0) {
        bmp.path = path.to_path_buf();
    } else {
        chart.resources.bmps.push(BmpDef { key: 0, path: path.to_path_buf() });
    }
    let has_initial_poor = chart.objects.iter().any(|object| {
        object.measure == 0
            && object.position_num == 0
            && matches!(
                object.kind,
                IntermediateObjectKind::Bga { kind: IntermediateBgaKind::Poor, .. }
            )
    });
    if !has_initial_poor {
        chart.objects.push(IntermediateObject {
            measure: 0,
            position_num: 0,
            position_den: 1,
            kind: IntermediateObjectKind::Bga { bmp_key: 0, kind: IntermediateBgaKind::Poor },
        });
    }
    chart.metadata.has_bga = true;
}

pub(super) fn build_resources(bms: &Bms) -> IntermediateResources {
    let wavs: Vec<WavDef> = bms
        .wav
        .wav_files
        .iter()
        .map(|(id, path)| WavDef { key: id.as_u16(), path: path.clone(), slice: None })
        .collect();
    let bmps: Vec<BmpDef> = bms
        .bmp
        .bmp_files
        .iter()
        .map(|(id, bmp)| BmpDef { key: id.as_u16(), path: bmp.file.clone() })
        .collect();
    let bpm_table: Vec<BpmDef> = bms
        .bpm
        .bpm_defs
        .iter()
        .filter_map(|(id, sv)| {
            sv.value().as_ref().ok().map(|v| BpmDef { key: id.as_u16(), bpm: v.get() })
        })
        .collect();
    let stop_table: Vec<StopDef> = bms
        .stop
        .stop_defs
        .iter()
        .filter_map(|(id, sv)| {
            sv.value().as_ref().ok().map(|v| StopDef { key: id.as_u16(), value: v.get() as u64 })
        })
        .collect();
    IntermediateResources { wavs, bmps, bpm_table, stop_table, swbga_defs: build_swbga_defs(bms) }
}

pub(super) fn build_swbga_defs(bms: &Bms) -> Vec<super::super::intermediate::SwBgaDef> {
    bms.bmp
        .swbga_events
        .iter()
        .map(|(id, event)| super::super::intermediate::SwBgaDef {
            id: id.as_u16(),
            frame_rate_ms: event.frame_rate,
            total_time_ms: event.total_time,
            line: event.line,
            loop_mode: event.loop_mode,
            chroma_alpha: event.argb.alpha,
            chroma_red: event.argb.red,
            chroma_green: event.argb.green,
            chroma_blue: event.argb.blue,
            pattern: event.pattern.clone(),
        })
        .collect()
}

pub(super) fn push_bga_keybound_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for keybound in bms.bmp.bga_keybound_events.values() {
        let swbga_key = bms
            .bmp
            .swbga_events
            .iter()
            .find(|(_, event)| *event == &keybound.event)
            .map(|(id, _)| id.as_u16());
        let Some(swbga_key) = swbga_key else {
            continue;
        };
        objects.push(IntermediateObject {
            measure: track_of(keybound.time),
            position_num: keybound.time.numerator() as u32,
            position_den: keybound.time.denominator().get() as u32,
            kind: IntermediateObjectKind::BgaKeybound { swbga_key },
        });
    }
}

pub(super) fn push_note_objects<T: KeyLayoutMapper>(
    bms: &Bms,
    layout: ChartKeyLayout,
    objects: &mut Vec<IntermediateObject>,
    warnings: &mut Vec<ImportWarning>,
) {
    for note in bms.notes().all_notes() {
        let Some(mapping) = T::from_channel_id(note.channel_id) else {
            continue;
        };
        let Some(lane) = map_lane(layout, mapping.side(), mapping.key()) else {
            if layout.is_pms() && mapping.side() == PlayerSide::Player2 {
                warnings.push(ImportWarning::UnsupportedPmsPlayerSide { side: 2 });
            } else {
                warnings
                    .push(ImportWarning::UnsupportedChannel { channel: note.channel_id.as_u16() });
            }
            continue;
        };
        let wav_id = note.wav_id.as_u16();
        let kind = match mapping.kind() {
            BmsNoteKind::Visible => IntermediateObjectKind::VisibleNote {
                lane,
                wav_key: (wav_id != 0).then_some(wav_id),
            },
            BmsNoteKind::Invisible => IntermediateObjectKind::InvisibleNote {
                lane,
                wav_key: (wav_id != 0).then_some(wav_id),
            },
            BmsNoteKind::Long => IntermediateObjectKind::LongChannelNote {
                lane,
                wav_key: (wav_id != 0).then_some(wav_id),
                mode: None,
                explicit_end_sound: false,
            },
            BmsNoteKind::Landmine => {
                IntermediateObjectKind::MineNote { lane, wav_key: None, damage: f64::from(wav_id) }
            }
        };
        objects.push(IntermediateObject {
            measure: track_of(note.offset),
            position_num: note.offset.numerator() as u32,
            position_den: note.offset.denominator().get() as u32,
            kind,
        });
    }
}

pub(super) fn push_bgm_objects<T: KeyLayoutMapper>(
    bms: &Bms,
    objects: &mut Vec<IntermediateObject>,
) {
    for (_, note) in bms.notes().notes_on::<T>(NoteChannelId::bgm()) {
        objects.push(IntermediateObject {
            measure: track_of(note.offset),
            position_num: note.offset.numerator() as u32,
            position_den: note.offset.denominator().get() as u32,
            kind: IntermediateObjectKind::Bgm { wav_key: note.wav_id.as_u16() },
        });
    }
}

pub(super) fn push_bga_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    use bms_rs::bms::model::obj::BgaLayer;
    for bga in bms.bmp.bga_changes.values() {
        let kind = match bga.layer {
            BgaLayer::Base => IntermediateBgaKind::Base,
            BgaLayer::Poor => IntermediateBgaKind::Poor,
            BgaLayer::Overlay => IntermediateBgaKind::Layer,
            BgaLayer::Overlay2 => IntermediateBgaKind::Layer2,
            _ => continue,
        };
        objects.push(IntermediateObject {
            measure: track_of(bga.time),
            position_num: bga.time.numerator() as u32,
            position_den: bga.time.denominator().get() as u32,
            kind: IntermediateObjectKind::Bga { bmp_key: bga.id.as_u16(), kind },
        });
    }
}

pub(super) fn push_bpm_change_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for (time, bpm) in &bms.bpm.bpm_changes_u8 {
        if *bpm == 0 {
            continue;
        }
        objects.push(IntermediateObject {
            measure: track_of(*time),
            position_num: time.numerator() as u32,
            position_den: time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBpm { bpm: *bpm as f64 },
        });
    }
    for change in bms.bpm.bpm_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBpm { bpm: change.bpm.get() },
        });
    }
}

pub(super) fn push_scroll_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for change in bms.scroll.scrolling_factor_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetScroll { factor: change.factor.get() },
        });
    }
}

pub(super) fn push_speed_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for change in bms.speed.speed_factor_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetSpeed { factor: change.factor.get() },
        });
    }
}

pub(super) fn push_judge_rank_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for judge_obj in bms.judge.judge_events.values() {
        objects.push(IntermediateObject {
            measure: track_of(judge_obj.time),
            position_num: judge_obj.time.numerator() as u32,
            position_den: judge_obj.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetJudgeRank {
                rank_percent: judge_level_to_rank_percent(judge_obj.judge_level),
            },
        });
    }
}

pub(super) fn push_volume_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for change in bms.volume.bgm_volume_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBgmVolume { volume: change.volume },
        });
    }
    for change in bms.volume.key_volume_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetKeyVolume { volume: change.volume },
        });
    }
}

pub(super) fn push_text_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for text_obj in bms.text.text_events.values() {
        objects.push(IntermediateObject {
            measure: track_of(text_obj.time),
            position_num: text_obj.time.numerator() as u32,
            position_den: text_obj.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetText { text: text_obj.text.clone() },
        });
    }
}

pub(super) fn push_bga_opacity_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for changes in bms.bmp.bga_opacity_changes.values() {
        for change in changes.values() {
            let Some(kind) = map_bga_layer_kind(change.layer) else {
                continue;
            };
            objects.push(IntermediateObject {
                measure: track_of(change.time),
                position_num: change.time.numerator() as u32,
                position_den: change.time.denominator().get() as u32,
                kind: IntermediateObjectKind::SetBgaOpacity { kind, opacity: change.opacity },
            });
        }
    }
}

pub(super) fn push_bga_argb_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for changes in bms.bmp.bga_argb_changes.values() {
        for change in changes.values() {
            let Some(kind) = map_bga_layer_kind(change.layer) else {
                continue;
            };
            objects.push(IntermediateObject {
                measure: track_of(change.time),
                position_num: change.time.numerator() as u32,
                position_den: change.time.denominator().get() as u32,
                kind: IntermediateObjectKind::SetBgaArgb {
                    kind,
                    alpha: change.argb.alpha,
                    red: change.argb.red,
                    green: change.argb.green,
                    blue: change.argb.blue,
                },
            });
        }
    }
}

pub(super) fn map_bga_layer_kind(
    layer: bms_rs::bms::model::obj::BgaLayer,
) -> Option<IntermediateBgaKind> {
    use bms_rs::bms::model::obj::BgaLayer;
    match layer {
        BgaLayer::Base => Some(IntermediateBgaKind::Base),
        BgaLayer::Poor => Some(IntermediateBgaKind::Poor),
        BgaLayer::Overlay => Some(IntermediateBgaKind::Layer),
        BgaLayer::Overlay2 => Some(IntermediateBgaKind::Layer2),
        _ => None,
    }
}

pub(super) fn judge_level_to_rank_percent(level: JudgeLevel) -> i32 {
    judge_rank_to_percent(judge_level_to_int(level))
}

pub(super) fn judge_rank_to_percent(rank: i32) -> i32 {
    match rank {
        0 => 25,
        1 => 50,
        2 => 75,
        3 => 100,
        4 => 125,
        r if r >= 10 => r,
        _ => 75,
    }
}

pub(super) fn push_stop_objects(
    bms: &Bms,
    objects: &mut Vec<IntermediateObject>,
    resources: &mut IntermediateResources,
) {
    let start_key = resources.stop_table.iter().map(|d| d.key).max().unwrap_or(0) + 1;
    for (key, stop) in (start_key..).zip(bms.stop.stops.values()) {
        resources.stop_table.push(StopDef { key, value: stop.duration.get() as u64 });
        objects.push(IntermediateObject {
            measure: track_of(stop.time),
            position_num: stop.time.numerator() as u32,
            position_den: stop.time.denominator().get() as u32,
            kind: IntermediateObjectKind::Stop { stop_key: key },
        });
    }
}
