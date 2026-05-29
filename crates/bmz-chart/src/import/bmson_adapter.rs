//! BMSON → [`IntermediateChart`] adapter。
//!
//! bms-rs の `parse_bmson` + `Bms::from_bmson` で BMS 相当へ変換し、
//! 既存の BMS 正規化パイプラインへ流す。

use std::path::Path;

use bms_rs::bms::model::Bms;
use bms_rs::bmson::bmson_to_bms::BmsonToBmsWarning;
use bms_rs::bmson::parse_bmson;

use crate::hash::compute_chart_identity;

use super::bms_rs_adapter::build_intermediate_from_bms;
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

    let output = parse_bmson(text);
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

    let converted = Bms::from_bmson(bmson);
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

    let mut intermediate = build_intermediate_from_bms(&converted.bms, warnings);
    intermediate.identity = identity;
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
