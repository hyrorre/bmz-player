use std::path::Path;

use bmz_core::time::ChartTick;

use crate::hash::compute_chart_identity;
use crate::timing::TICKS_PER_MEASURE;

use super::decode::decode_bms_text;
use super::error::{ImportError, ImportWarning};
use super::intermediate::{
    IntermediateChart, IntermediateMetadata, IntermediateResources, MeasureInfo,
};

pub fn import_bms_to_intermediate(
    source_path: &Path,
    _random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let bytes = read_source_bytes(source_path)?;
    let identity = compute_chart_identity(&bytes);
    let _text = decode_bms_text(&bytes, warnings);

    // The real bms-rs adapter lands here. Keeping this scaffold useful lets the
    // rest of the runtime compile while the parser mapping is implemented.
    Ok(IntermediateChart {
        identity,
        metadata: IntermediateMetadata::default(),
        resources: IntermediateResources::default(),
        measures: vec![MeasureInfo {
            index: 0,
            length_ratio_num: 1,
            length_ratio_den: 1,
            start_tick: ChartTick(0),
            tick_len: TICKS_PER_MEASURE as u64,
        }],
        objects: Vec::new(),
        lnobj_wav_key: None,
    })
}

fn read_source_bytes(path: &Path) -> Result<Vec<u8>, ImportError> {
    std::fs::read(path).map_err(|source| ImportError::Io { path: path.to_path_buf(), source })
}
