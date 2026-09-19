//! BMSON → [`IntermediateChart`] adapter。
//!
//! bms-rs の `parse_bmson` + `Bms::from_bmson` で BMS 相当へ変換し、
//! 既存の BMS 正規化パイプラインへ流す。

use std::path::{Path, PathBuf};

use bms_rs::bms::command::channel::mapper::{KeyLayoutBeat, KeyLayoutPmsBmeType};
use bms_rs::bms::command::graphics::Argb;
use bms_rs::bms::command::{JudgeLevel, LnMode, ObjId, StringValue};
use bms_rs::bms::model::Bms;
use bms_rs::bms::model::bmp::Bmp;
use bms_rs::bmson::bmson_to_bms::BmsonToBmsWarning;
use bms_rs::bmson::{Bmson, parse_bmson};
use bmz_core::lane::{ChartKeyLayout, KeyMode, Lane, PmsKeyLayout};

use crate::hash::compute_chart_identity;
use crate::model::{JudgeRankKind, JudgeRankSpec, LongNoteMode};

use super::bms_rs_adapter::build_intermediate_from_bms;
use super::bmson_timing::{
    BmsonBgaExtension, BmsonBgaKind, BmsonLaneLayout, BmsonLayeredSoundExtension,
    BmsonLongNoteExtension, BmsonSoundSliceExtension, build_measure_boundaries, max_pulse_in_bmson,
    pulse_to_obj_time, rebuild_bms_timing_from_bmson,
};
use super::error::{ImportError, ImportWarning};
use super::intermediate::{IntermediateChart, IntermediateObject, IntermediateObjectKind};

pub fn import_bmson_to_intermediate(
    source_path: &Path,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let bytes = std::fs::read(source_path)
        .map_err(|source| ImportError::Io { path: source_path.to_path_buf(), source })?;
    let identity = compute_chart_identity(&bytes);
    let text = std::str::from_utf8(&bytes).map_err(|_| ImportError::Parse {
        path: source_path.to_path_buf(),
        message: "BMSON file is not valid UTF-8".into(),
    })?;

    let PreparedBmson { text: parse_text, ln_type, hln_notes } = prepare_bmson_text(text)
        .map_err(|message| ImportError::Parse { path: source_path.to_path_buf(), message })?;

    let output = parse_bmson(&parse_text);
    let parse_errors: Vec<_> = output.errors.iter().map(|error| format!("{error:?}")).collect();
    for message in &parse_errors {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "BmsonParseError".into(),
            message: message.clone(),
        });
    }

    let bmson = output.bmson.ok_or_else(|| ImportError::Parse {
        path: source_path.to_path_buf(),
        message: format!("failed to parse BMSON: {}", parse_errors.join("; ")),
    })?;
    let layout = bmson_layout_for_mode_hint(&bmson.info.mode_hint)?;

    let suppress_bar_lines = matches!(bmson.lines.as_ref(), Some(lines) if lines.is_empty());
    let max_pulse = max_pulse_in_bmson(&bmson);
    let boundaries =
        build_measure_boundaries(bmson.lines.as_deref(), bmson.info.resolution.get(), max_pulse);

    let mut converted = bms_from_bmson_headers_and_resources(&bmson, warnings);
    let mut timing_warnings = Vec::new();
    let rebuild_info = match layout {
        BmsonLaneLayout::Beat5
        | BmsonLaneLayout::Beat7
        | BmsonLaneLayout::Beat10
        | BmsonLaneLayout::Beat14 => rebuild_bms_timing_from_bmson::<KeyLayoutBeat>(
            &mut converted.bms,
            &bmson,
            &boundaries,
            layout,
            &hln_notes,
            &mut timing_warnings,
        ),
        BmsonLaneLayout::Pms5 | BmsonLaneLayout::Pms9 => {
            rebuild_bms_timing_from_bmson::<KeyLayoutPmsBmeType>(
                &mut converted.bms,
                &bmson,
                &boundaries,
                layout,
                &hln_notes,
                &mut timing_warnings,
            )
        }
    };
    for warning in timing_warnings {
        push_bmson_to_bms_warning(warning, warnings);
    }

    converted.bms.repr.ln_mode = match ln_type {
        Some(LongNoteMode::Cn) => LnMode::Cn,
        Some(LongNoteMode::Hcn) => LnMode::Hcn,
        _ => LnMode::Ln,
    };
    converted.bms.music_info.sub_artist = Some(join_subartists(&bmson.info.subartists));
    converted.bms.sprite.back_bmp =
        resolve_backbmp_path(bmson.info.back_image.as_deref(), bmson.info.title_image.as_deref());

    let mut intermediate = match layout {
        BmsonLaneLayout::Beat5
        | BmsonLaneLayout::Beat7
        | BmsonLaneLayout::Beat10
        | BmsonLaneLayout::Beat14 => build_intermediate_from_bms::<KeyLayoutBeat>(
            &converted.bms,
            ChartKeyLayout::beat(),
            warnings,
        )?,
        BmsonLaneLayout::Pms5 | BmsonLaneLayout::Pms9 => {
            build_intermediate_from_bms::<KeyLayoutPmsBmeType>(
                &converted.bms,
                ChartKeyLayout::pms(PmsKeyLayout::BmeType),
                warnings,
            )?
        }
    };
    intermediate.identity = identity;
    intermediate.metadata.key_mode = bmson_key_mode(layout);
    intermediate.metadata.subtitle =
        bmson_subtitle(bmson.info.subtitle.as_ref(), bmson.info.chart_name.as_ref());
    intermediate.metadata.difficulty_name.clear();
    intermediate.metadata.total_is_bmson_percent = true;
    intermediate.metadata.play_level = bmson.info.level.to_string();
    apply_bmson_sound_slices(&mut intermediate, &rebuild_info.sound_slices);
    apply_bmson_layered_sounds(&mut intermediate, &rebuild_info.layered_sounds, layout);
    apply_bmson_bga_events(&mut intermediate, &rebuild_info.bga_events);
    if let Some(ln_type) = ln_type {
        intermediate.metadata.long_note_mode = ln_type;
        intermediate.metadata.long_note_mode_defined = true;
    }
    apply_bmson_long_note_extensions(&mut intermediate, &rebuild_info.long_notes, layout);
    push_bmson_stop_objects(&mut intermediate, &bmson, &boundaries);
    push_bmson_mine_objects(
        &mut intermediate,
        &bmson,
        &boundaries,
        layout,
        &rebuild_info.mine_channel_wav_keys,
    );
    intermediate.metadata.suppress_bar_lines = suppress_bar_lines;
    let judge_rank = bmson.info.judge_rank.as_f64() as i32;
    intermediate.metadata.judge_rank = Some(judge_rank);
    intermediate.metadata.judge_rank_spec =
        Some(JudgeRankSpec { value: judge_rank, kind: JudgeRankKind::BmsonJudgeRank });
    Ok(intermediate)
}

fn bmson_layout_for_mode_hint(mode_hint: &str) -> Result<BmsonLaneLayout, ImportError> {
    let normalized = mode_hint.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "beat-5k" => return Ok(BmsonLaneLayout::Beat5),
        "beat-7k" => return Ok(BmsonLaneLayout::Beat7),
        "beat-10k" => return Ok(BmsonLaneLayout::Beat10),
        "beat-14k" => return Ok(BmsonLaneLayout::Beat14),
        "popn-5k" => return Ok(BmsonLaneLayout::Pms5),
        "popn-9k" => return Ok(BmsonLaneLayout::Pms9),
        _ => {}
    }
    Err(ImportError::UnsupportedMode { mode: mode_hint.to_string() })
}

fn bmson_key_mode(layout: BmsonLaneLayout) -> KeyMode {
    match layout {
        BmsonLaneLayout::Beat5 => KeyMode::K5,
        BmsonLaneLayout::Beat7 => KeyMode::K7,
        BmsonLaneLayout::Beat10 => KeyMode::K10,
        BmsonLaneLayout::Beat14 => KeyMode::K14,
        BmsonLaneLayout::Pms5 | BmsonLaneLayout::Pms9 => KeyMode::K9,
    }
}

struct BmzBmsonToBmsOutput {
    bms: Bms,
}

fn bms_from_bmson_headers_and_resources(
    bmson: &Bmson<'_>,
    warnings: &mut Vec<ImportWarning>,
) -> BmzBmsonToBmsOutput {
    let mut bms = Bms::default();

    bms.music_info.title = Some(bmson.info.title.clone().into_owned());
    bms.music_info.subtitle = Some(bmson.info.subtitle.clone().into_owned());
    bms.music_info.artist = Some(bmson.info.artist.clone().into_owned());
    bms.music_info.sub_artist = bmson.info.subartists.first().map(|s| s.clone().into_owned());
    bms.music_info.genre = Some(bmson.info.genre.clone().into_owned());
    bms.metadata.play_level = Some(bmson.info.level as u8);
    bms.judge.total = Some(StringValue::from_value(bmson.info.total));
    bms.sprite.back_bmp = bmson.info.back_image.clone().map(|s| PathBuf::from(s.into_owned()));
    bms.sprite.stage_file =
        bmson.info.eyecatch_image.clone().map(|s| PathBuf::from(s.into_owned()));
    bms.sprite.banner = bmson.info.banner_image.clone().map(|s| PathBuf::from(s.into_owned()));
    bms.music_info.preview_music =
        bmson.info.preview_music.clone().map(|s| PathBuf::from(s.into_owned()));
    bms.judge.rank = Some(JudgeLevel::OtherInt((bmson.info.judge_rank.as_f64() * 18.0) as i64));
    bms.bpm.bpm = Some(StringValue::from_value(bmson.info.init_bpm));

    let mut bga_header_obj_id_issuer = ObjId::all_values();
    for bga_header in &bmson.bga.bga_header {
        let Some(obj_id) = bga_header_obj_id_issuer.next() else {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "BmsonToBmsBgaHeaderObjIdOutOfRange".into(),
                message: BmsonToBmsWarning::BgaHeaderObjIdOutOfRange.to_string(),
            });
            continue;
        };
        bms.bmp.bmp_files.insert(
            obj_id,
            Bmp {
                file: PathBuf::from(bga_header.name.as_ref()),
                transparent_color: Argb::default(),
            },
        );
    }

    BmzBmsonToBmsOutput { bms }
}

fn push_bmson_to_bms_warning(warning: BmsonToBmsWarning, warnings: &mut Vec<ImportWarning>) {
    let (code, message) = match warning {
        BmsonToBmsWarning::WavObjIdOutOfRange => {
            ("BmsonToBmsWavObjIdOutOfRange", warning.to_string())
        }
        BmsonToBmsWarning::BgaHeaderObjIdOutOfRange => {
            ("BmsonToBmsBgaHeaderObjIdOutOfRange", warning.to_string())
        }
        BmsonToBmsWarning::BgaEventObjIdOutOfRange => {
            ("BmsonToBmsBgaEventObjIdOutOfRange", warning.to_string())
        }
        BmsonToBmsWarning::BpmDefOutOfRange => ("BmsonToBmsBpmDefOutOfRange", warning.to_string()),
        BmsonToBmsWarning::StopDefOutOfRange => {
            ("BmsonToBmsStopDefOutOfRange", warning.to_string())
        }
        BmsonToBmsWarning::ScrollDefOutOfRange => {
            ("BmsonToBmsScrollDefOutOfRange", warning.to_string())
        }
        _ => ("BmsonToBmsWarning", warning.to_string()),
    };
    warnings.push(ImportWarning::ParserDiagnostic { code: code.into(), message });
}

/// beatoraja 拡張の `ln_type` / `t` は整数だが、bms-rs の serde は variant 名を期待する。
/// パース前に整数値を取り出し、note `t` は variant 名へ正規化する。
struct PreparedBmson {
    text: String,
    ln_type: Option<LongNoteMode>,
    hln_notes: std::collections::HashSet<(usize, usize)>,
}

fn prepare_bmson_text(text: &str) -> Result<PreparedBmson, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("invalid BMSON JSON: {err}"))?;
    let ln_type = value.pointer("/info/ln_type").and_then(parse_ln_type_json);
    if let Some(info) = value.get_mut("info").and_then(serde_json::Value::as_object_mut) {
        info.remove("ln_type");
    }
    let hln_notes = normalize_note_ln_type_fields(&mut value);
    let sanitized = serde_json::to_string(&value)
        .map_err(|err| format!("failed to serialize BMSON JSON: {err}"))?;
    Ok(PreparedBmson { text: sanitized, ln_type, hln_notes })
}

fn join_subartists(subartists: &[std::borrow::Cow<'_, str>]) -> String {
    subartists
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn bmson_subtitle(subtitle: &str, chart_name: &str) -> String {
    match (subtitle.is_empty(), chart_name.is_empty()) {
        (_, true) => subtitle.to_string(),
        (true, false) => format!("[{chart_name}]"),
        (false, false) => format!("{subtitle} [{chart_name}]"),
    }
}

fn resolve_backbmp_path(back_image: Option<&str>, title_image: Option<&str>) -> Option<PathBuf> {
    back_image
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .or_else(|| title_image.map(str::trim).filter(|path| !path.is_empty()))
        .map(PathBuf::from)
}

fn parse_ln_type_json(value: &serde_json::Value) -> Option<LongNoteMode> {
    match value {
        serde_json::Value::Number(number) => {
            number.as_u64().and_then(|raw| u8::try_from(raw).ok()).and_then(|raw| match raw {
                4 => Some(LongNoteMode::Hln),
                _ => LnMode::try_from(raw).ok().map(map_ln_mode),
            })
        }
        serde_json::Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "ln" | "1" => Some(LongNoteMode::Ln),
            "cn" | "2" => Some(LongNoteMode::Cn),
            "hcn" | "hell" | "3" => Some(LongNoteMode::Hcn),
            "hln" | "4" => Some(LongNoteMode::Hln),
            _ => None,
        },
        _ => None,
    }
}

fn normalize_note_ln_type_fields(
    value: &mut serde_json::Value,
) -> std::collections::HashSet<(usize, usize)> {
    let mut hln_notes = std::collections::HashSet::new();
    let Some(channels) = value.get_mut("sound_channels").and_then(serde_json::Value::as_array_mut)
    else {
        return hln_notes;
    };
    for (channel_index, channel) in channels.iter_mut().enumerate() {
        let Some(notes) = channel.get_mut("notes").and_then(serde_json::Value::as_array_mut) else {
            continue;
        };
        for (note_index, note) in notes.iter_mut().enumerate() {
            let Some(map) = note.as_object_mut() else {
                continue;
            };
            let Some(value) = map.get("t").cloned() else {
                continue;
            };
            if let Some(mode) = parse_ln_type_json(&value) {
                let name = match mode {
                    LongNoteMode::Ln => "Ln",
                    LongNoteMode::Cn => "Cn",
                    LongNoteMode::Hcn => "Hcn",
                    LongNoteMode::Hln => {
                        hln_notes.insert((channel_index, note_index));
                        "Ln"
                    }
                };
                map.insert("t".into(), serde_json::Value::String(name.into()));
            } else {
                map.remove("t");
            }
        }
    }
    hln_notes
}

fn map_ln_mode(mode: LnMode) -> crate::model::LongNoteMode {
    match mode {
        LnMode::Ln => crate::model::LongNoteMode::Ln,
        LnMode::Cn => crate::model::LongNoteMode::Cn,
        LnMode::Hcn => crate::model::LongNoteMode::Hcn,
    }
}

fn apply_bmson_long_note_extensions(
    intermediate: &mut IntermediateChart,
    extensions: &[BmsonLongNoteExtension],
    layout: BmsonLaneLayout,
) {
    for extension in extensions {
        let Some(lane) = bmson_lane(extension.lane.get(), layout) else {
            continue;
        };
        if let Some(object) = intermediate.objects.iter_mut().find(|object| {
            object.measure == extension.start.measure
                && object.position_num == extension.start.numerator
                && object.position_den == extension.start.denominator
                && matches!(
                    object.kind,
                    super::intermediate::IntermediateObjectKind::LongChannelNote {
                        lane: object_lane,
                        ..
                    } if object_lane == lane
                )
        }) && let super::intermediate::IntermediateObjectKind::LongChannelNote { mode, .. } =
            &mut object.kind
        {
            *mode = if extension.hln {
                Some(LongNoteMode::Hln)
            } else {
                extension.mode.map(map_ln_mode)
            };
        }

        if extension.explicit_end_sound
            && let Some(object) = intermediate.objects.iter_mut().find(|object| {
                object.measure == extension.end.measure
                    && object.position_num == extension.end.numerator
                    && object.position_den == extension.end.denominator
                    && matches!(
                        object.kind,
                        super::intermediate::IntermediateObjectKind::LongChannelNote {
                            lane: object_lane,
                            ..
                        } if object_lane == lane
                    )
            })
            && let super::intermediate::IntermediateObjectKind::LongChannelNote {
                wav_key,
                explicit_end_sound,
                ..
            } = &mut object.kind
        {
            *wav_key = extension.end_wav_key;
            *explicit_end_sound = true;
        }
    }
}

fn apply_bmson_sound_slices(
    intermediate: &mut IntermediateChart,
    extensions: &[BmsonSoundSliceExtension],
) {
    apply_bmson_sound_slices_to_wavs(&mut intermediate.resources.wavs, extensions);
}

fn apply_bmson_sound_slices_to_wavs(
    wavs: &mut [super::intermediate::WavDef],
    extensions: &[BmsonSoundSliceExtension],
) {
    let slices_by_wav_key = extensions
        .iter()
        .map(|extension| (extension.wav_key, extension.slice))
        .collect::<std::collections::HashMap<_, _>>();

    for wav in wavs {
        if let Some(slice) = slices_by_wav_key.get(&wav.key) {
            wav.slice = Some(*slice);
        }
    }
}

fn apply_bmson_layered_sounds(
    intermediate: &mut IntermediateChart,
    extensions: &[BmsonLayeredSoundExtension],
    layout: BmsonLaneLayout,
) {
    for extension in extensions {
        let Some(lane) = bmson_lane(extension.lane.get(), layout) else {
            continue;
        };
        intermediate.layered_note_sounds.push(super::intermediate::IntermediateLayeredSound {
            lane,
            measure: extension.position.measure,
            position_num: extension.position.numerator,
            position_den: extension.position.denominator,
            wav_key: extension.wav_key,
        });
    }
}

fn apply_bmson_bga_events(intermediate: &mut IntermediateChart, extensions: &[BmsonBgaExtension]) {
    intermediate
        .objects
        .retain(|object| !matches!(object.kind, IntermediateObjectKind::Bga { .. }));
    for extension in extensions {
        let kind = match extension.kind {
            BmsonBgaKind::Base => super::intermediate::IntermediateBgaKind::Base,
            BmsonBgaKind::Layer => super::intermediate::IntermediateBgaKind::Layer,
            BmsonBgaKind::Poor => super::intermediate::IntermediateBgaKind::Poor,
        };
        intermediate.objects.push(IntermediateObject {
            measure: extension.position.measure,
            position_num: extension.position.numerator,
            position_den: extension.position.denominator,
            kind: IntermediateObjectKind::Bga { bmp_key: extension.bmp_key, kind },
        });
    }
    intermediate.metadata.has_bga = !extensions.is_empty();
}

fn push_bmson_stop_objects(
    intermediate: &mut IntermediateChart,
    bmson: &Bmson<'_>,
    boundaries: &super::bmson_timing::MeasureBoundaries,
) {
    let resolution = bmson.info.resolution.get();
    for event in &bmson.stop_events {
        let time = pulse_to_obj_time(event.y.0, boundaries);
        intermediate.objects.push(IntermediateObject {
            measure: u32::try_from(time.track().0).unwrap_or(u32::MAX),
            position_num: u32::try_from(time.numerator()).unwrap_or(u32::MAX),
            position_den: u32::try_from(time.denominator().get()).unwrap_or(u32::MAX),
            kind: IntermediateObjectKind::BmsonStop { duration_pulses: event.duration, resolution },
        });
    }
}

fn push_bmson_mine_objects(
    intermediate: &mut IntermediateChart,
    bmson: &Bmson<'_>,
    boundaries: &super::bmson_timing::MeasureBoundaries,
    layout: BmsonLaneLayout,
    wav_keys: &[u16],
) {
    for (channel, wav_key) in bmson.mine_channels.iter().zip(wav_keys) {
        let wav_key = (!channel.name.trim().is_empty()).then_some(*wav_key);
        for event in &channel.notes {
            let Some(lane) = event.x.and_then(|value| bmson_lane(value.get(), layout)) else {
                continue;
            };
            let time = pulse_to_obj_time(event.y.0, boundaries);
            intermediate.objects.push(IntermediateObject {
                measure: u32::try_from(time.track().0).unwrap_or(u32::MAX),
                position_num: u32::try_from(time.numerator()).unwrap_or(u32::MAX),
                position_den: u32::try_from(time.denominator().get()).unwrap_or(u32::MAX),
                kind: IntermediateObjectKind::MineNote {
                    lane,
                    wav_key,
                    damage: event.damage.get(),
                },
            });
        }
    }
}

fn bmson_lane(value: u8, layout: BmsonLaneLayout) -> Option<Lane> {
    match layout {
        BmsonLaneLayout::Pms5 if value <= 5 => Lane::from_pms_key(value),
        BmsonLaneLayout::Pms9 => Lane::from_pms_key(value),
        BmsonLaneLayout::Beat5 => match value {
            1 => Some(Lane::Key1),
            2 => Some(Lane::Key2),
            3 => Some(Lane::Key3),
            4 => Some(Lane::Key4),
            5 => Some(Lane::Key5),
            8 => Some(Lane::Scratch),
            _ => None,
        },
        BmsonLaneLayout::Beat7 => match value {
            1 => Some(Lane::Key1),
            2 => Some(Lane::Key2),
            3 => Some(Lane::Key3),
            4 => Some(Lane::Key4),
            5 => Some(Lane::Key5),
            6 => Some(Lane::Key6),
            7 => Some(Lane::Key7),
            8 => Some(Lane::Scratch),
            _ => None,
        },
        BmsonLaneLayout::Beat10 => match value {
            1 => Some(Lane::Key1),
            2 => Some(Lane::Key2),
            3 => Some(Lane::Key3),
            4 => Some(Lane::Key4),
            5 => Some(Lane::Key5),
            8 => Some(Lane::Scratch),
            9 => Some(Lane::Key8),
            10 => Some(Lane::Key9),
            11 => Some(Lane::Key10),
            12 => Some(Lane::Key11),
            13 => Some(Lane::Key12),
            16 => Some(Lane::Scratch2),
            _ => None,
        },
        BmsonLaneLayout::Beat14 => match value {
            1 => Some(Lane::Key1),
            2 => Some(Lane::Key2),
            3 => Some(Lane::Key3),
            4 => Some(Lane::Key4),
            5 => Some(Lane::Key5),
            6 => Some(Lane::Key6),
            7 => Some(Lane::Key7),
            8 => Some(Lane::Scratch),
            9 => Some(Lane::Key8),
            10 => Some(Lane::Key9),
            11 => Some(Lane::Key10),
            12 => Some(Lane::Key11),
            13 => Some(Lane::Key12),
            14 => Some(Lane::Key13),
            15 => Some(Lane::Key14),
            16 => Some(Lane::Scratch2),
            _ => None,
        },
        BmsonLaneLayout::Pms5 => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::intermediate::IntermediateObjectKind;
    use crate::model::LongNoteMode;
    use bms_rs::bmson::parse_bmson;

    fn import_bmson_text(text: &str) -> Result<IntermediateChart, ImportError> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bmson");
        std::fs::write(&path, text).unwrap();
        let mut warnings = Vec::new();
        import_bmson_to_intermediate(&path, &mut warnings)
    }

    #[test]
    fn resolve_backbmp_path_prefers_back_image() {
        assert_eq!(
            resolve_backbmp_path(Some("back.png"), Some("title.png")).as_deref(),
            Some(Path::new("back.png"))
        );
    }

    #[test]
    fn resolve_backbmp_path_falls_back_to_title_image() {
        assert_eq!(
            resolve_backbmp_path(Some(""), Some("_Back.png")).as_deref(),
            Some(Path::new("_Back.png"))
        );
        assert_eq!(
            resolve_backbmp_path(None, Some("_Back.png")).as_deref(),
            Some(Path::new("_Back.png"))
        );
    }

    #[test]
    fn join_subartists_joins_all_entries() {
        use std::borrow::Cow;

        assert_eq!(
            join_subartists(&[
                Cow::Borrowed("music:Alice"),
                Cow::Borrowed("chart:Bob"),
                Cow::Borrowed(" movie.Sphere ")
            ]),
            "music:Alice / chart:Bob / movie.Sphere"
        );
    }

    #[test]
    fn bmson_chart_name_is_appended_to_subtitle_like_beatoraja() {
        assert_eq!(bmson_subtitle("", "ANOTHER"), "[ANOTHER]");
        assert_eq!(bmson_subtitle("Original", "ANOTHER"), "Original [ANOTHER]");
        assert_eq!(bmson_subtitle("Original", ""), "Original");
    }

    #[test]
    fn apply_bmson_sound_slices_uses_last_extension_for_each_wav_key() {
        use crate::import::intermediate::WavDef;
        use crate::model::SoundSlice;

        let mut wavs = vec![
            WavDef { key: 1, path: PathBuf::from("first.wav"), slice: None },
            WavDef { key: 2, path: PathBuf::from("second.wav"), slice: None },
        ];
        let extensions = [
            BmsonSoundSliceExtension {
                wav_key: 1,
                slice: SoundSlice { start_us: 10, duration_us: Some(20) },
            },
            BmsonSoundSliceExtension {
                wav_key: 1,
                slice: SoundSlice { start_us: 30, duration_us: None },
            },
        ];

        apply_bmson_sound_slices_to_wavs(&mut wavs, &extensions);

        assert_eq!(wavs[0].slice, Some(SoundSlice { start_us: 30, duration_us: None }));
        assert_eq!(wavs[1].slice, None);
    }

    #[test]
    fn prepare_bmson_text_normalizes_integer_ln_types_for_bms_rs() {
        let json = r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"ln_type":3,"resolution":240},"sound_channels":[]}"#;
        let PreparedBmson { text: sanitized, ln_type, .. } = prepare_bmson_text(json).unwrap();
        assert_eq!(ln_type, Some(LongNoteMode::Hcn));
        assert!(parse_bmson(&sanitized).bmson.is_some());
    }

    #[test]
    fn prepare_bmson_text_preserves_integer_note_type() {
        let json = r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240},"sound_channels":[{"name":"key.wav","notes":[{"x":1,"y":0,"l":240,"c":false,"t":2}]}]}"#;
        let PreparedBmson { text: sanitized, .. } = prepare_bmson_text(json).unwrap();
        let bmson = parse_bmson(&sanitized).bmson.unwrap();
        assert_eq!(bmson.sound_channels[0].notes[0].t, Some(LnMode::Cn));
    }

    #[test]
    fn bmson_sound_channel_without_x_is_bgm() {
        let chart = import_bmson_text(
            r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"beat-7k"},"sound_channels":[{"name":"bgm.wav","notes":[{"x":0,"y":0,"l":0,"c":false}]},{"name":"key.wav","notes":[{"x":1,"y":0,"l":0,"c":false}]}]}"#,
        )
        .unwrap();

        let visible = chart
            .objects
            .iter()
            .filter(|object| matches!(object.kind, IntermediateObjectKind::VisibleNote { .. }))
            .count();
        let bgm = chart
            .objects
            .iter()
            .filter(|object| matches!(object.kind, IntermediateObjectKind::Bgm { .. }))
            .count();
        assert_eq!((visible, bgm), (1, 1));
    }

    #[test]
    fn bmson_popn_9k_uses_pms_layout() {
        let chart = import_bmson_text(
            r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"popn-9k"},"sound_channels":[{"name":"key.wav","notes":[{"x":1,"y":0,"l":0,"c":false},{"x":9,"y":240,"l":0,"c":false}]}]}"#,
        )
        .unwrap();

        assert_eq!(chart.metadata.key_mode, bmz_core::lane::KeyMode::K9);
        let lanes: Vec<_> = chart
            .objects
            .iter()
            .filter_map(|object| match object.kind {
                IntermediateObjectKind::VisibleNote { lane, .. } => Some(lane),
                _ => None,
            })
            .collect();
        assert_eq!(lanes, vec![bmz_core::lane::Lane::Key1, bmz_core::lane::Lane::Key9]);
    }

    #[test]
    fn bmson_beat_5k_uses_hint_and_drops_reserved_lanes() {
        let chart = import_bmson_text(
            r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"beat-5k"},"sound_channels":[{"name":"key.wav","notes":[{"x":1,"y":0,"l":0,"c":false},{"x":6,"y":240,"l":0,"c":false},{"x":8,"y":480,"l":0,"c":false}]}]}"#,
        )
        .unwrap();

        assert_eq!(chart.metadata.key_mode, KeyMode::K5);
        let lanes = chart
            .objects
            .iter()
            .filter_map(|object| match object.kind {
                IntermediateObjectKind::VisibleNote { lane, .. } => Some(lane),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(lanes, vec![Lane::Key1, Lane::Scratch]);
        assert!(
            !chart
                .objects
                .iter()
                .any(|object| { matches!(object.kind, IntermediateObjectKind::Bgm { .. }) })
        );
    }

    #[test]
    fn bmson_beat_10k_keeps_sparse_mode_and_drops_seven_key_gaps() {
        let chart = import_bmson_text(
            r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"beat-10k"},"sound_channels":[{"name":"key.wav","notes":[{"x":1,"y":0,"l":0,"c":false},{"x":6,"y":240,"l":0,"c":false},{"x":9,"y":480,"l":0,"c":false},{"x":14,"y":720,"l":0,"c":false},{"x":16,"y":960,"l":0,"c":false}]}]}"#,
        )
        .unwrap();

        assert_eq!(chart.metadata.key_mode, KeyMode::K10);
        let lanes = chart
            .objects
            .iter()
            .filter_map(|object| match object.kind {
                IntermediateObjectKind::VisibleNote { lane, .. } => Some(lane),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(lanes, vec![Lane::Key1, Lane::Key8, Lane::Scratch2]);
    }

    #[test]
    fn bmson_empty_beat_14k_keeps_declared_mode() {
        let chart = import_bmson_text(
            r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"beat-14k"},"sound_channels":[]}"#,
        )
        .unwrap();

        assert_eq!(chart.metadata.key_mode, KeyMode::K14);
    }

    #[test]
    fn bmson_popn_5k_is_accepted_as_pms_mode() {
        let chart = import_bmson_text(
            r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"popn-5k"},"sound_channels":[{"name":"key.wav","notes":[{"x":1,"y":0,"l":0,"c":false},{"x":6,"y":240,"l":0,"c":false}]}]}"#,
        )
        .unwrap();

        assert_eq!(chart.metadata.key_mode, KeyMode::K9);
        let lanes = chart
            .objects
            .iter()
            .filter_map(|object| match object.kind {
                IntermediateObjectKind::VisibleNote { lane, .. } => Some(lane),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(lanes, vec![Lane::Key1]);
    }

    #[test]
    fn bmson_keyboard_24k_is_rejected() {
        let error = import_bmson_text(
            r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"keyboard-24k"},"sound_channels":[]}"#,
        )
        .unwrap_err();

        assert!(
            matches!(error, ImportError::UnsupportedMode { ref mode } if mode == "keyboard-24k")
        );
    }
}
