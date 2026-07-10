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
use bmz_core::lane::{ChartKeyLayout, PmsKeyLayout};

use crate::hash::compute_chart_identity;
use crate::model::{JudgeRankKind, JudgeRankSpec};

use super::bms_rs_adapter::build_intermediate_from_bms;
use super::bmson_timing::{
    BmsonLaneLayout, build_measure_boundaries, max_pulse_in_bmson, rebuild_bms_timing_from_bmson,
};
use super::error::{ImportError, ImportWarning};
use super::intermediate::IntermediateChart;

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

    let (parse_text, ln_type) = prepare_bmson_text(text)
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
    match layout {
        BmsonLaneLayout::Beat => rebuild_bms_timing_from_bmson::<KeyLayoutBeat>(
            &mut converted.bms,
            &bmson,
            &boundaries,
            layout,
            &mut timing_warnings,
        ),
        BmsonLaneLayout::Pms => rebuild_bms_timing_from_bmson::<KeyLayoutPmsBmeType>(
            &mut converted.bms,
            &bmson,
            &boundaries,
            layout,
            &mut timing_warnings,
        ),
    }
    for warning in timing_warnings {
        push_bmson_to_bms_warning(warning, warnings);
    }

    converted.bms.repr.ln_mode = ln_type;
    converted.bms.music_info.sub_artist = Some(join_subartists(&bmson.info.subartists));
    converted.bms.sprite.back_bmp =
        resolve_backbmp_path(bmson.info.back_image.as_deref(), bmson.info.title_image.as_deref());

    let mut intermediate = match layout {
        BmsonLaneLayout::Beat => build_intermediate_from_bms::<KeyLayoutBeat>(
            &converted.bms,
            ChartKeyLayout::beat(),
            warnings,
        )?,
        BmsonLaneLayout::Pms => build_intermediate_from_bms::<KeyLayoutPmsBmeType>(
            &converted.bms,
            ChartKeyLayout::pms(PmsKeyLayout::BmeType),
            warnings,
        )?,
    };
    intermediate.identity = identity;
    intermediate.metadata.suppress_bar_lines = suppress_bar_lines;
    intermediate.metadata.judge_rank_spec = Some(JudgeRankSpec {
        value: bmson.info.judge_rank.as_f64() as i32,
        kind: JudgeRankKind::BmsonJudgeRank,
    });
    Ok(intermediate)
}

fn bmson_layout_for_mode_hint(mode_hint: &str) -> Result<BmsonLaneLayout, ImportError> {
    let normalized = mode_hint.trim().to_ascii_lowercase();
    if normalized == "popn-9k" {
        return Ok(BmsonLaneLayout::Pms);
    }
    if matches!(normalized.as_str(), "beat-5k" | "beat-7k" | "beat-10k" | "beat-14k") {
        return Ok(BmsonLaneLayout::Beat);
    }
    Err(ImportError::UnsupportedMode { mode: mode_hint.to_string() })
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
/// パース前に整数値を取り出し、JSON から除去する。
fn prepare_bmson_text(text: &str) -> Result<(String, LnMode), String> {
    let mut value: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("invalid BMSON JSON: {err}"))?;
    let ln_type = value.pointer("/info/ln_type").map(parse_ln_type_json).unwrap_or_default();
    if let Some(info) = value.get_mut("info").and_then(serde_json::Value::as_object_mut) {
        info.remove("ln_type");
    }
    strip_integer_ln_type_fields(&mut value);
    let sanitized = serde_json::to_string(&value)
        .map_err(|err| format!("failed to serialize BMSON JSON: {err}"))?;
    Ok((sanitized, ln_type))
}

fn join_subartists(subartists: &[std::borrow::Cow<'_, str>]) -> String {
    subartists
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn resolve_backbmp_path(back_image: Option<&str>, title_image: Option<&str>) -> Option<PathBuf> {
    back_image
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .or_else(|| title_image.map(str::trim).filter(|path| !path.is_empty()))
        .map(PathBuf::from)
}

fn parse_ln_type_json(value: &serde_json::Value) -> LnMode {
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .and_then(|raw| u8::try_from(raw).ok())
            .and_then(|raw| LnMode::try_from(raw).ok())
            .unwrap_or_default(),
        serde_json::Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "ln" | "1" => LnMode::Ln,
            "cn" | "2" => LnMode::Cn,
            "hcn" | "hell" | "3" => LnMode::Hcn,
            _ => LnMode::default(),
        },
        _ => LnMode::default(),
    }
}

fn strip_integer_ln_type_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if matches!(map.get("t"), Some(serde_json::Value::Number(_))) {
                map.remove("t");
            }
            for nested in map.values_mut() {
                strip_integer_ln_type_fields(nested);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_integer_ln_type_fields(item);
            }
        }
        _ => {}
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
    fn prepare_bmson_text_strips_integer_ln_type_for_bms_rs() {
        let json = r#"{"version":"1.0.0","info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"ln_type":3,"resolution":240},"sound_channels":[]}"#;
        let (sanitized, ln_type) = prepare_bmson_text(json).unwrap();
        assert_eq!(ln_type, LnMode::Hcn);
        assert!(parse_bmson(&sanitized).bmson.is_some());
        assert_eq!(
            LongNoteMode::Hcn,
            match ln_type {
                LnMode::Ln => LongNoteMode::Ln,
                LnMode::Cn => LongNoteMode::Cn,
                LnMode::Hcn => LongNoteMode::Hcn,
            }
        );
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
