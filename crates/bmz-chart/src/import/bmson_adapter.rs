//! BMSON → [`IntermediateChart`] adapter。
//!
//! bms-rs の `parse_bmson` + `Bms::from_bmson` で BMS 相当へ変換し、
//! 既存の BMS 正規化パイプラインへ流す。

use std::path::{Path, PathBuf};

use bms_rs::bms::command::LnMode;
use bms_rs::bms::command::channel::mapper::KeyLayoutBeat;
use bms_rs::bms::model::Bms;
use bms_rs::bmson::bmson_to_bms::BmsonToBmsWarning;
use bms_rs::bmson::parse_bmson;
use bmz_core::lane::ChartKeyLayout;

use crate::hash::compute_chart_identity;

use super::bms_rs_adapter::build_intermediate_from_bms;
use super::bmson_timing::{
    build_measure_boundaries, max_pulse_in_bmson, rebuild_bms_timing_from_bmson,
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

    let suppress_bar_lines = matches!(bmson.lines.as_ref(), Some(lines) if lines.is_empty());
    let max_pulse = max_pulse_in_bmson(&bmson);
    let boundaries =
        build_measure_boundaries(bmson.lines.as_deref(), bmson.info.resolution.get(), max_pulse);

    let mut converted = Bms::from_bmson(bmson.clone());
    let mut timing_warnings = Vec::new();
    rebuild_bms_timing_from_bmson(&mut converted.bms, &bmson, &boundaries, &mut timing_warnings);
    for warning in timing_warnings {
        push_bmson_to_bms_warning(warning, warnings);
    }
    for warning in converted.warnings {
        push_bmson_to_bms_warning(warning, warnings);
    }
    for warning in converted.playing_warnings {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "BmsonPlayingWarning".into(),
            message: format!("{warning:?}"),
        });
    }
    for error in converted.playing_errors {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "BmsonPlayingError".into(),
            message: format!("{error:?}"),
        });
    }

    converted.bms.repr.ln_mode = ln_type;
    converted.bms.music_info.sub_artist = Some(join_subartists(&bmson.info.subartists));
    converted.bms.sprite.back_bmp =
        resolve_backbmp_path(bmson.info.back_image.as_deref(), bmson.info.title_image.as_deref());

    let mut intermediate = build_intermediate_from_bms::<KeyLayoutBeat>(
        &converted.bms,
        ChartKeyLayout::beat(),
        warnings,
    );
    intermediate.identity = identity;
    intermediate.metadata.suppress_bar_lines = suppress_bar_lines;
    Ok(intermediate)
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
    use crate::model::LongNoteMode;
    use bms_rs::bmson::parse_bmson;

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
}
