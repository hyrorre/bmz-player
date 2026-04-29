pub mod bms_adapter;
pub mod decode;
pub mod error;
pub mod intermediate;
pub mod long_note;
pub mod normalize;

use std::path::Path;

use crate::model::PlayableChart;

use self::error::{ImportError, ImportWarning};

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub chart: PlayableChart,
    pub warnings: Vec<ImportWarning>,
}

pub fn import_bms_chart(
    path: &Path,
    random_seed: Option<u64>,
) -> Result<ImportResult, ImportError> {
    let mut warnings = Vec::new();
    let intermediate = bms_adapter::import_bms_to_intermediate(path, random_seed, &mut warnings)?;
    let chart = normalize::normalize_chart(path, intermediate, &mut warnings)?;
    Ok(ImportResult { chart, warnings })
}
