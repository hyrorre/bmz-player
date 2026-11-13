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

/// How import treats the sound and image files a chart refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetLookup {
    /// Resolve each BGA file on disk, trying same-stem alternatives, as playback needs.
    /// With `warn_missing`, also report missing sounds and images as warnings.
    Resolve { warn_missing: bool },
    /// Keep the declared paths and derive BGA kinds from them, without touching the
    /// file system. A library scan stores only metadata and analysis, and a chart with
    /// hundreds of `#BMP` lines would otherwise cost several file lookups per line.
    Declared,
}

impl AssetLookup {
    const fn from_check(check_resource_existence: bool) -> Self {
        Self::Resolve { warn_missing: check_resource_existence }
    }
}

/// 拡張子に応じて BMS / BMSON を import し、BMS の乱数分岐方法を指定する。
pub fn import_chart_with_random_source(
    path: &Path,
    random_source: BmsRandomSource,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    import_chart_with_assets(path, random_source, AssetLookup::from_check(check_resource_existence))
}

/// [`import_chart_with_random_source`] with an explicit [`AssetLookup`].
pub fn import_chart_with_assets(
    path: &Path,
    random_source: BmsRandomSource,
    assets: AssetLookup,
) -> Result<ImportResult, ImportError> {
    let bytes = bmz_song_resource::read_file(path)
        .map_err(|source| ImportError::Io { path: path.to_path_buf(), source })?;
    import_chart_bytes_with_assets(path, &bytes, random_source, assets)
}

/// Import original chart bytes, using `path` only for format, diagnostics and asset bases.
/// The identity always hashes `bytes` before decoding or RANDOM processing.
pub fn import_chart_bytes_with_random_source(
    path: &Path,
    bytes: &[u8],
    random_source: BmsRandomSource,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    import_chart_bytes_with_assets(
        path,
        bytes,
        random_source,
        AssetLookup::from_check(check_resource_existence),
    )
}

/// [`import_chart_bytes_with_random_source`] with an explicit [`AssetLookup`].
pub fn import_chart_bytes_with_assets(
    path: &Path,
    bytes: &[u8],
    random_source: BmsRandomSource,
    assets: AssetLookup,
) -> Result<ImportResult, ImportError> {
    let mut warnings = Vec::new();
    let mut bms_random_choices = Vec::new();
    let mut bms_switch_choices = Vec::new();
    let file_format = chart_file_format(path);
    let intermediate = match file_format {
        ChartFileFormat::Bmson => {
            bmson_adapter::import_bmson_bytes_to_intermediate(path, bytes, &mut warnings)?
        }
        ChartFileFormat::Bms => bms_rs_adapter::import_bms_bytes_to_intermediate(
            path,
            bytes,
            &random_source,
            &mut bms_random_choices,
            &mut bms_switch_choices,
            &mut warnings,
        )?,
        ChartFileFormat::Pms => bms_rs_adapter::import_pms_bytes_to_intermediate(
            path,
            bytes,
            &random_source,
            &mut bms_random_choices,
            &mut bms_switch_choices,
            &mut warnings,
        )?,
    };
    let mut chart =
        normalize::normalize_chart_with_assets(path, intermediate, &mut warnings, assets)?;
    chart.metadata.source_format = file_format.source_format();
    if let Some(mut program) = chart.metadata.conditional.take() {
        std::sync::Arc::make_mut(&mut program).set_bga_assets(&chart.bga_assets);
        program.stabilize_default(&mut chart)?;
        chart.sounds = program.all_sounds()?;
        chart.metadata.conditional = Some(program);
    }
    for message in &chart.metadata.kaleid.diagnostics {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "KaleidPartial".into(),
            message: message.clone(),
        });
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
