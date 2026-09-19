//! bms-rs を使った BMS → [`IntermediateChart`] adapter。
//!
//! 内製 parser (`bms_adapter.rs`) を置き換えるのが目的。出力契約は据え置きで、
//! 下流 (`normalize.rs`) の入力としてそのまま流せる形に整える。
//!
//! 対応範囲:
//! - メタデータ: title / subtitle / artist / sub_artist / genre / play_level
//!   / difficulty / judge_rank / total / initial_bpm / stage_file / banner /
//!   back_bmp / preview_file / has_bga / key_mode
//! - 定義: WAV / BMP / BPM (#BPMxx) / STOP
//! - チャネル:
//!   - 01: BGM
//!   - 02: 小節長
//!   - 03/08: BPM 変更（インライン / ref）
//!   - 04/06/07/0A: BGA (Base/Poor/Overlay→Layer/Layer2)
//!   - 09: STOP
//!   - 1x/2x: Visible (P1/P2)
//!   - 3x/4x: Invisible (P1/P2)
//!   - 5x/6x: Long-channel (P1/P2)
//!   - Dx/Ex: Landmine (P1/P2)
//! - `#LNOBJ`: bms-rs には渡さず、BMZ 側で marker WAV を通常ノート列から LN ペアへ解決する。
//! - `.pms`: `KeyLayoutPms` / `KeyLayoutPmsBmeType` による 9K SP (18K は drop + warning)
//!
//! 未対応 (warning に流すか drop):
//! - JUDGE 変更イベント (#EXRANK / chA0)
//! - TEXT / OPTION / VIDEO / SEEK 等
//! - foot pedal / free zone
//! - PMS 18K (2P 側ノート)

use std::collections::BTreeMap;
use std::path::Path;

use bms_rs::bms::command::channel::mapper::{
    KeyLayoutBeat, KeyLayoutMapper, KeyLayoutPms, KeyLayoutPmsBmeType,
};
use bms_rs::bms::command::channel::{
    Channel, Key, NoteChannelId, NoteKind as BmsNoteKind, PlayerSide, read_channel,
};
use bms_rs::bms::command::time::ObjTime;
use bms_rs::bms::command::{JudgeLevel, LnMode, ObjId};
use bms_rs::bms::model::Bms;
use bms_rs::bms::model::obj::{
    BgaArgbObj, BgaKeyboundObj, BgaLayer, BgaObj, BgaOpacityObj, BgmVolumeObj, BpmChangeObj,
    JudgeObj, KeyVolumeObj, ScrollingFactorObj, SpeedObj, StopObj, TextObj, WavObj,
};
use bms_rs::bms::rng::JavaRandom;
use bms_rs::bms::{BmsOutput, BmsWarning, default_config_with_rng, parse_bms};
use bmz_core::chart::ChartIdentity;
use bmz_core::lane::{ChartKeyLayout, KeyMode, Lane, PmsKeyLayout};
use bmz_core::time::ChartTick;

use crate::hash::compute_chart_identity;

use super::BmsRandomSource;
use super::decode::decode_bms_text;
use super::error::{ImportError, ImportWarning};
use super::intermediate::{
    BmpDef, BpmDef, IntermediateBgaKind, IntermediateChart, IntermediateMetadata,
    IntermediateObject, IntermediateObjectKind, IntermediateResources, MeasureInfo, StopDef,
    WavDef,
};

use crate::model::{JudgeRankKind, JudgeRankSpec, LongNoteMode};

mod compat;
mod long_end;
mod long_note_headers;
mod metadata;
mod objects;
mod random;
mod sparse;
mod timing;

use compat::*;
use metadata::*;
use objects::*;
use random::*;
use sparse::*;
use timing::*;

pub(crate) const MAX_SUPPORTED_MEASURE: u32 = 100_000;
const SPARSE_BMS_MESSAGE_OBJECT_THRESHOLD: usize = 8_192;
const SPARSE_BMS_MARKER_HEADER: &str = "BMZSPARSE";

#[derive(Debug, Clone)]
struct SparseBmsMessage {
    id: usize,
    line_number: usize,
    measure: u64,
    channel: String,
    object_count: u64,
    objects: Vec<SparseBmsObject>,
}

#[derive(Debug, Clone)]
struct SparseBmsObject {
    index: u64,
    id: String,
}

#[derive(Debug, Clone)]
struct BgaMessage {
    line_number: usize,
    measure: u64,
    kind: IntermediateBgaKind,
    object_count: u64,
    objects: Vec<SparseBmsObject>,
}

pub fn import_bms_to_intermediate(
    source_path: &Path,
    random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    import_bms_to_intermediate_with_random_source(
        source_path,
        &BmsRandomSource::Seed(random_seed),
        &mut Vec::new(),
        &mut Vec::new(),
        warnings,
    )
}

pub fn import_bms_to_intermediate_with_random_source(
    source_path: &Path,
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    import_with_layout::<KeyLayoutBeat>(
        source_path,
        ChartKeyLayout::beat(),
        random_source,
        bms_random_choices,
        bms_switch_choices,
        warnings,
    )
}

pub fn import_pms_to_intermediate(
    source_path: &Path,
    random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    import_pms_to_intermediate_with_random_source(
        source_path,
        &BmsRandomSource::Seed(random_seed),
        &mut Vec::new(),
        &mut Vec::new(),
        warnings,
    )
}

pub fn import_pms_to_intermediate_with_random_source(
    source_path: &Path,
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let bytes = std::fs::read(source_path)
        .map_err(|source| ImportError::Io { path: source_path.to_path_buf(), source })?;
    let text = decode_bms_text(&bytes, warnings);
    let (variant, conflict) = detect_pms_variant(&text);
    if conflict {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "PmsLayoutConflict".to_string(),
            message: "PMS standard (2P upper) and BME-type (1P 16-19) channels both used; \
                      using standard layout"
                .to_string(),
        });
    }
    match variant {
        PmsKeyLayout::Standard => import_with_layout::<KeyLayoutPms>(
            source_path,
            ChartKeyLayout::pms(PmsKeyLayout::Standard),
            random_source,
            bms_random_choices,
            bms_switch_choices,
            warnings,
        ),
        PmsKeyLayout::BmeType => import_with_layout::<KeyLayoutPmsBmeType>(
            source_path,
            ChartKeyLayout::pms(PmsKeyLayout::BmeType),
            random_source,
            bms_random_choices,
            bms_switch_choices,
            warnings,
        ),
    }
}

fn import_with_layout<T: KeyLayoutMapper>(
    source_path: &Path,
    layout: ChartKeyLayout,
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let bytes = std::fs::read(source_path)
        .map_err(|source| ImportError::Io { path: source_path.to_path_buf(), source })?;
    let identity = compute_chart_identity(&bytes);
    let raw_text = decode_bms_text(&bytes, warnings);
    let has_bms_random = source_text_has_bms_random(&raw_text);
    let layout_text = if layout == ChartKeyLayout::pms(PmsKeyLayout::Standard) {
        strip_pms_bme_upper_channels(&raw_text)
    } else {
        raw_text.clone()
    };
    let compatible_text = normalize_beatoraja_header_separators(&layout_text);
    let text = apply_beatoraja_random_control(
        &compatible_text,
        random_source,
        bms_random_choices,
        bms_switch_choices,
        warnings,
    );
    let (hln_parse_text, hln_mode) = long_note_headers::preprocess(&text);
    let metadata_text = strip_empty_metadata_commands(&hln_parse_text);
    let lnobj_parse_text = strip_lnobj_commands(&metadata_text);
    let bga_messages = extract_bga_message_lines(&lnobj_parse_text);
    let (parse_text, sparse_messages) =
        extract_sparse_bms_message_lines(&lnobj_parse_text, warnings);

    let BmsOutput { bms, warnings: bms_warnings } = parse_bms::<T, _, _, _>(
        &parse_text,
        default_config_with_rng(JavaRandom::new(random_source_seed(random_source) as i64))
            .key_mapper::<T>(),
    );
    for w in bms_warnings {
        if let Some(w) = map_bms_warning(&w) {
            warnings.push(w);
        }
    }
    let mut bms = bms.map_err(|err| ImportError::Parse {
        path: source_path.to_path_buf(),
        message: format!("{err:?}"),
    })?;
    inject_sparse_bms_messages::<T>(&mut bms, &sparse_messages, warnings);
    let bga_objects = bga_messages_to_intermediate_objects(
        &bga_messages,
        bms_uses_base62_obj_ids(&bms),
        warnings,
    );
    // bms-rs stores BGA changes in a single map keyed only by time, so simultaneous
    // Base/Poor/Layer changes overwrite each other. Use the source-derived events instead.
    bms.bmp.bga_changes.clear();

    let mut intermediate = build_intermediate_from_bms_with_extra_bga_objects::<T>(
        &bms,
        layout,
        &bga_objects,
        warnings,
    )?;
    intermediate.lnobj_wav_key =
        extract_lnobj_wav_key(&text, bms_uses_base62_obj_ids(&bms), warnings);
    intermediate.typed_lnobj =
        long_end::typed_markers(&text, bms_uses_base62_obj_ids(&bms), warnings);
    long_end::restore_marker_overlays::<T>(
        &text,
        layout,
        bms_uses_base62_obj_ids(&bms),
        &mut intermediate,
    );
    if hln_mode {
        intermediate.metadata.long_note_mode = LongNoteMode::Hln;
        intermediate.metadata.long_note_mode_defined = true;
    }
    let bms_headers = extract_bms_headers_from_text(&raw_text);
    intermediate.metadata.has_bms_random = has_bms_random;
    intermediate.metadata.bms_headers = bms_headers.clone();
    apply_raw_judge_rank_headers(&mut intermediate, &bms_headers);
    intermediate.metadata.source_url = bms
        .metadata
        .url
        .clone()
        .filter(|url| !url.is_empty())
        .or_else(|| bms_headers.get("URL").cloned())
        .unwrap_or_default();
    intermediate.metadata.append_url = append_url_from_headers(&bms_headers);
    intermediate.identity = identity;
    Ok(intermediate)
}

pub(crate) fn build_intermediate_from_bms<T: KeyLayoutMapper>(
    bms: &Bms,
    layout: ChartKeyLayout,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    build_intermediate_from_bms_with_extra_bga_objects::<T>(bms, layout, &[], warnings)
}

fn build_intermediate_from_bms_with_extra_bga_objects<T: KeyLayoutMapper>(
    bms: &Bms,
    layout: ChartKeyLayout,
    extra_bga_objects: &[IntermediateObject],
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let metadata = build_metadata(bms);
    let mut resources = build_resources(bms);
    let mut objects = Vec::new();

    push_note_objects::<T>(bms, layout, &mut objects, warnings);
    push_bgm_objects::<T>(bms, &mut objects);
    push_bga_objects(bms, &mut objects);
    objects.extend_from_slice(extra_bga_objects);
    push_bpm_change_objects(bms, &mut objects);
    push_stop_objects(bms, &mut objects, &mut resources);
    push_scroll_objects(bms, &mut objects);
    push_speed_objects(bms, &mut objects);
    push_judge_rank_objects(bms, &mut objects);
    push_volume_objects(bms, &mut objects);
    push_text_objects(bms, &mut objects);
    push_bga_opacity_objects(bms, &mut objects);
    push_bga_argb_objects(bms, &mut objects);
    push_bga_keybound_objects(bms, &mut objects);

    let max_measure = compute_max_measure(bms, &objects)?;
    let measures = build_measures(max_measure, bms);

    let mut intermediate = IntermediateChart {
        identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
        metadata,
        resources,
        measures,
        objects,
        layered_note_sounds: Vec::new(),
        lnobj_wav_key: None, // bms-rs 側で吸収済み
        typed_lnobj: Vec::new(),
        long_end_sounds: Vec::new(),
    };

    intermediate.metadata.has_bga = intermediate
        .objects
        .iter()
        .any(|object| matches!(object.kind, IntermediateObjectKind::Bga { .. }));

    let lane_key_mode = KeyMode::detect_from_lanes_with_layout(
        layout,
        intermediate.objects.iter().filter_map(|o| match o.kind {
            IntermediateObjectKind::VisibleNote { lane, .. }
            | IntermediateObjectKind::InvisibleNote { lane, .. }
            | IntermediateObjectKind::LongChannelNote { lane, .. }
            | IntermediateObjectKind::MineNote { lane, .. } => Some(lane),
            _ => None,
        }),
    );
    intermediate.metadata.key_mode =
        detect_key_mode_from_bms_headers(bms, layout).unwrap_or(lane_key_mode);
    normalize_qwilight_lanes(&mut intermediate.objects, intermediate.metadata.key_mode);

    Ok(intermediate)
}

fn normalize_qwilight_lanes(objects: &mut [IntermediateObject], key_mode: KeyMode) {
    if !matches!(key_mode, KeyMode::K4 | KeyMode::K6 | KeyMode::K8) {
        return;
    }

    for object in objects {
        let lane = match &mut object.kind {
            IntermediateObjectKind::VisibleNote { lane, .. }
            | IntermediateObjectKind::InvisibleNote { lane, .. }
            | IntermediateObjectKind::LongChannelNote { lane, .. }
            | IntermediateObjectKind::MineNote { lane, .. } => lane,
            _ => continue,
        };
        *lane = match key_mode {
            KeyMode::K4 => match *lane {
                Lane::Key4 => Lane::Key3,
                Lane::Key5 => Lane::Key4,
                lane => lane,
            },
            KeyMode::K6 => match *lane {
                Lane::Key5 => Lane::Key4,
                Lane::Key6 => Lane::Key5,
                Lane::Key7 => Lane::Key6,
                lane => lane,
            },
            KeyMode::K8 => match *lane {
                Lane::Scratch => Lane::Key1,
                Lane::Key1 => Lane::Key2,
                Lane::Key2 => Lane::Key3,
                Lane::Key3 => Lane::Key4,
                Lane::Key4 => Lane::Key5,
                Lane::Key5 => Lane::Key6,
                Lane::Key6 => Lane::Key7,
                Lane::Key7 => Lane::Key8,
                lane => lane,
            },
            _ => *lane,
        };
    }
}

/// Qwilight / BMSE 拡張ヘッダ (`#4K`, `#6K`, `#8K`) からキーモードを読む。
///
/// bms-rs はこれらを構造化しないため `repr.raw_command_lines` を走査する。
/// 複数行ある場合は後勝ち（EXPANSION FIELD の宣言を優先）。
pub(crate) fn detect_key_mode_from_bms_headers(
    bms: &Bms,
    layout: ChartKeyLayout,
) -> Option<KeyMode> {
    if layout.is_pms() {
        return None;
    }

    let mut mode = None;
    for line in &bms.repr.raw_command_lines {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("#4K") {
            mode = Some(KeyMode::K4);
        } else if trimmed.eq_ignore_ascii_case("#6K") {
            mode = Some(KeyMode::K6);
        } else if trimmed.eq_ignore_ascii_case("#8K") {
            mode = Some(KeyMode::K8);
        }
    }
    mode
}

/// `.pms` テキストから Standard / BME-type を判定する。
pub(crate) fn detect_pms_variant(source: &str) -> (PmsKeyLayout, bool) {
    let mut has_standard_upper = false;
    let mut has_bme_upper = false;

    for line in source.lines() {
        let Some(channel) = message_channel_bytes(line) else {
            continue;
        };
        if pms_standard_upper_channel(channel) {
            has_standard_upper = true;
        }
        if pms_bme_upper_channel(channel) {
            has_bme_upper = true;
        }
    }

    let variant = if has_standard_upper {
        PmsKeyLayout::Standard
    } else if has_bme_upper {
        PmsKeyLayout::BmeType
    } else {
        PmsKeyLayout::Standard
    };
    (variant, has_standard_upper && has_bme_upper)
}

#[cfg(test)]
#[path = "bms_rs_adapter/tests.rs"]
mod tests;
