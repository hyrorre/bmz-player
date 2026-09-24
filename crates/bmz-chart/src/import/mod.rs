pub mod bms_rs_adapter;
pub mod bmson_adapter;
pub mod bmson_timing;
pub mod decode;
pub mod error;
pub mod intermediate;
pub mod long_note;
pub mod normalize;

use std::path::Path;

use crate::model::{ChartSourceFormat, PlayableChart};

use self::error::{ImportError, ImportWarning};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChartFileFormat {
    Bms,
    Bmson,
    Pms,
}

impl ChartFileFormat {
    const fn source_format(self) -> ChartSourceFormat {
        match self {
            Self::Bms => ChartSourceFormat::Bms,
            Self::Bmson => ChartSourceFormat::Bmson,
            Self::Pms => ChartSourceFormat::Pms,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub chart: PlayableChart,
    pub warnings: Vec<ImportWarning>,
    /// BMS `#RANDOM` ごとの実際の選択値。BMSON では常に空。
    pub bms_random_choices: Vec<i32>,
    /// BMS `#SWITCH` ごとの実際の選択値。BMSON では常に空。
    pub bms_switch_choices: Vec<u64>,
}

/// BMS の `#RANDOM` / `#SWITCH` 分岐を選ぶ方法。
///
/// `Choices` はリプレイ時に、記録済みの選択値を出現順に強制するために使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BmsRandomSource {
    Seed(Option<u64>),
    Choices { random: Vec<i32>, switches: Vec<u64> },
}

/// 拡張子に応じて BMS / BMSON を import する。
pub fn import_chart(
    path: &Path,
    random_seed: Option<u64>,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    import_chart_with_random_source(
        path,
        BmsRandomSource::Seed(random_seed),
        check_resource_existence,
    )
}

/// 拡張子に応じて BMS / BMSON を import し、BMS の乱数分岐方法を指定する。
pub fn import_chart_with_random_source(
    path: &Path,
    random_source: BmsRandomSource,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    let mut warnings = Vec::new();
    let mut bms_random_choices = Vec::new();
    let mut bms_switch_choices = Vec::new();
    let file_format = chart_file_format(path);
    let intermediate = match file_format {
        ChartFileFormat::Bmson => bmson_adapter::import_bmson_to_intermediate(path, &mut warnings)?,
        ChartFileFormat::Bms => bms_rs_adapter::import_bms_to_intermediate_with_random_source(
            path,
            &random_source,
            &mut bms_random_choices,
            &mut bms_switch_choices,
            &mut warnings,
        )?,
        ChartFileFormat::Pms => bms_rs_adapter::import_pms_to_intermediate_with_random_source(
            path,
            &random_source,
            &mut bms_random_choices,
            &mut bms_switch_choices,
            &mut warnings,
        )?,
    };
    let mut chart =
        normalize::normalize_chart(path, intermediate, &mut warnings, check_resource_existence)?;
    chart.metadata.source_format = file_format.source_format();
    if let Some(program) = chart.metadata.conditional.clone() {
        program.stabilize_default(&mut chart)?;
        chart.sounds = program.all_sounds()?;
    }
    Ok(ImportResult { chart, warnings, bms_random_choices, bms_switch_choices })
}

pub fn import_bms_chart(
    path: &Path,
    random_seed: Option<u64>,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    import_chart(path, random_seed, check_resource_existence)
}

/// BMS の `#RANDOM` 分岐方法を指定して import する。
pub fn import_bms_chart_with_random_source(
    path: &Path,
    random_source: BmsRandomSource,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    import_chart_with_random_source(path, random_source, check_resource_existence)
}

fn chart_file_format(path: &Path) -> ChartFileFormat {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
    {
        Some(ext) if ext == "bmson" => ChartFileFormat::Bmson,
        Some(ext) if ext == "pms" => ChartFileFormat::Pms,
        _ => ChartFileFormat::Bms,
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
