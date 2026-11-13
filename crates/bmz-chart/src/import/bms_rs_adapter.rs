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
//! - OPTION / VIDEO / SEEK 等 (TEXT は `text.rs` が原文から読む)
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
use super::decode::decode_bms_text_cow;
use super::error::{ImportError, ImportWarning};
use super::intermediate::{
    BmpDef, BpmDef, IntermediateBgaKind, IntermediateChart, IntermediateMetadata,
    IntermediateObject, IntermediateObjectKind, IntermediateResources, MeasureInfo, StopDef,
    WavDef,
};

use crate::model::{JudgeRankKind, JudgeRankSpec, LongNoteMode};

mod compat;
mod kaleid;
mod long_end;
mod long_note_headers;
mod metadata;
mod objects;
mod prepare;
mod random;
mod sparse;
mod text;
mod timing;

use compat::*;
use metadata::*;
use objects::*;
use prepare::*;
use random::*;
use sparse::*;
use text::*;
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
    let bytes = bmz_song_resource::read_file(source_path)
        .map_err(|source| ImportError::Io { path: source_path.to_path_buf(), source })?;
    import_bms_bytes_to_intermediate(
        source_path,
        &bytes,
        random_source,
        bms_random_choices,
        bms_switch_choices,
        warnings,
    )
}

pub(super) fn import_bms_bytes_to_intermediate(
    source_path: &Path,
    bytes: &[u8],
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let identity = compute_chart_identity(bytes);
    let text = decode_bms_text_cow(bytes, warnings);
    import_with_layout::<KeyLayoutBeat>(
        source_path,
        (&text, identity),
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
    let bytes = bmz_song_resource::read_file(source_path)
        .map_err(|source| ImportError::Io { path: source_path.to_path_buf(), source })?;
    import_pms_bytes_to_intermediate(
        source_path,
        &bytes,
        random_source,
        bms_random_choices,
        bms_switch_choices,
        warnings,
    )
}

pub(super) fn import_pms_bytes_to_intermediate(
    source_path: &Path,
    bytes: &[u8],
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let decode_start = warnings.len();
    let text = decode_bms_text_cow(bytes, warnings);
    let decode_end = warnings.len();
    let (variant, conflict) = detect_pms_variant(&text);
    if conflict {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "PmsLayoutConflict".to_string(),
            message: "PMS standard (2P upper) and BME-type (1P 16-19) channels both used; \
                      using standard layout"
                .to_string(),
        });
    }
    // PMS previously decoded once before layout detection and again after it.
    // Keep the existing diagnostic order while reusing the decoded text.
    warnings.extend_from_within(decode_start..decode_end);
    let source = (&*text, compute_chart_identity(bytes));
    match variant {
        PmsKeyLayout::Standard => import_with_layout::<KeyLayoutPms>(
            source_path,
            source,
            ChartKeyLayout::pms(PmsKeyLayout::Standard),
            random_source,
            bms_random_choices,
            bms_switch_choices,
            warnings,
        ),
        PmsKeyLayout::BmeType => import_with_layout::<KeyLayoutPmsBmeType>(
            source_path,
            source,
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
    (raw_text, identity): (&str, ChartIdentity),
    layout: ChartKeyLayout,
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let pms_standard = layout == ChartKeyLayout::pms(PmsKeyLayout::Standard);
    if !kaleid::needs_interpretation(raw_text, layout) {
        // Keep main's single-pass preparation for ordinary charts.
        let source = prepare_bms_source(
            raw_text,
            pms_standard,
            random_source,
            bms_random_choices,
            bms_switch_choices,
            warnings,
        );
        return parse_prepared_with_layout::<T>(
            source_path,
            layout,
            source,
            &[],
            identity,
            random_source,
            warnings,
        )
        .map(|(chart, _)| chart);
    }
    let text = resolve_kaleid_source(
        raw_text,
        pms_standard,
        random_source,
        bms_random_choices,
        bms_switch_choices,
        warnings,
    );
    let mut bms_headers = BTreeMap::new();
    for line in raw_text.lines() {
        insert_bms_header_line(&mut bms_headers, line);
    }
    let has_bms_random = raw_text.lines().any(declares_bms_random);
    crate::conditional::compile(&text, source_path, |text| {
        let mut chart = parse_resolved_with_layout::<T>(
            source_path,
            layout,
            text,
            identity.clone(),
            random_source,
            warnings,
        )?;
        apply_raw_judge_rank_headers(&mut chart, &bms_headers);
        if chart.metadata.source_url.is_empty() {
            chart.metadata.source_url = bms_headers.get("URL").cloned().unwrap_or_default();
        }
        chart.metadata.append_url = append_url_from_headers(&bms_headers);
        chart.metadata.bms_headers = bms_headers.clone();
        chart.metadata.has_bms_random = has_bms_random;
        Ok(chart)
    })
}

fn parse_resolved_with_layout<T: KeyLayoutMapper>(
    source_path: &Path,
    layout: ChartKeyLayout,
    text: &str,
    identity: ChartIdentity,
    random_source: &BmsRandomSource,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let (hln_parse_text, hln_mode) = long_note_headers::preprocess(text);
    let kaleid = kaleid::extract::<T>(&hln_parse_text, layout);
    let source = prepare_bms_source(
        &kaleid.text,
        false,
        random_source,
        &mut Vec::new(),
        &mut Vec::new(),
        warnings,
    );
    let (mut intermediate, base62_obj_ids) = parse_prepared_with_layout::<T>(
        source_path,
        layout,
        source,
        &kaleid.objects,
        identity,
        random_source,
        warnings,
    )?;
    intermediate.metadata.kaleid.source = kaleid.features;
    intermediate.metadata.kaleid.declared = kaleid.features;
    intermediate.metadata.kaleid.diagnostics = kaleid.diagnostics;
    intermediate.typed_lnobj = long_end::typed_markers(text, base62_obj_ids, warnings);
    for warning in warnings.iter() {
        if let ImportWarning::ParserDiagnostic { code, message } = warning
            && matches!(code.as_str(), "InvalidCnobj" | "InvalidHcnobj" | "InvalidHlnobj")
        {
            intermediate.metadata.kaleid.diagnostics.push(message.clone());
        }
    }
    for line in text.lines() {
        let Some(body) = line.trim().strip_prefix('#') else { continue };
        let (command, value) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
        let command = command.to_ascii_uppercase();
        if command == "LNMODE" && !matches!(value.trim(), "1" | "2" | "3" | "4") {
            intermediate
                .metadata
                .kaleid
                .diagnostics
                .push(format!("invalid #LNMODE {value}; ignored"));
        }
        if matches!(
            command.as_str(),
            "BMD" | "SOUNDFONT" | "TARGETHASH" | "LOADINGFILE" | "READYFILE" | "BGAOVERFLOW"
        ) {
            intermediate
                .metadata
                .kaleid
                .diagnostics
                .push(format!("unsupported #{command}; ignored"));
        }
    }
    intermediate.metadata.kaleid.declared.typed_end = !intermediate.typed_lnobj.is_empty();
    intermediate.metadata.kaleid.declared.hln |=
        intermediate.typed_lnobj.iter().any(|(_, mode)| *mode == LongNoteMode::Hln);
    intermediate.metadata.kaleid.source.typed_end = intermediate.objects.iter().any(|object| {
        matches!(object.kind, IntermediateObjectKind::VisibleNote { wav_key: Some(key), .. }
            if intermediate.typed_lnobj.iter().any(|(marker, _)| *marker == key))
    });
    intermediate.metadata.kaleid.source.hln = intermediate.objects.iter().any(|object|
        matches!(object.kind, IntermediateObjectKind::LongChannelNote { .. } if hln_mode)
        || matches!(object.kind, IntermediateObjectKind::VisibleNote { wav_key: Some(key), .. }
            if intermediate.typed_lnobj.iter().any(|(marker, mode)| *marker == key && *mode == LongNoteMode::Hln)));
    long_end::restore_marker_overlays::<T>(text, layout, base62_obj_ids, &mut intermediate);
    if hln_mode {
        intermediate.metadata.kaleid.declared.hln = true;
        intermediate.metadata.long_note_mode = LongNoteMode::Hln;
        intermediate.metadata.long_note_mode_defined = true;
    }
    Ok(intermediate)
}

#[allow(clippy::too_many_arguments)]
fn parse_prepared_with_layout<T: KeyLayoutMapper>(
    source_path: &Path,
    layout: ChartKeyLayout,
    source: PreparedBmsSource,
    kaleid_objects: &[IntermediateObject],
    identity: ChartIdentity,
    random_source: &BmsRandomSource,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(IntermediateChart, bool), ImportError> {
    let BmsOutput { bms, warnings: bms_warnings } = parse_bms::<T, _, _, _>(
        &source.parse_text,
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
    inject_sparse_bms_messages::<T>(&mut bms, &source.sparse_messages, warnings);
    let base62_obj_ids = bms_uses_base62_obj_ids(&bms);
    let mut extra_objects =
        bga_messages_to_intermediate_objects(&source.bga_messages, base62_obj_ids, warnings);
    // bms-rs stores BGA changes in a single map keyed only by time, so simultaneous
    // Base/Poor/Layer changes overwrite each other. Use the source-derived events instead.
    bms.bmp.bga_changes.clear();
    // Text events have the same limitation; bms-rs also drops the rest of a #99 message
    // after an undefined object.
    let source_texts = source_text_objects(&source.source_texts, base62_obj_ids, warnings);
    extra_objects.extend(source_texts.objects);
    extra_objects.extend_from_slice(kaleid_objects);
    bms.text.text_events.clear();

    let mut intermediate = build_intermediate_from_bms_with_extra_objects::<T>(
        &bms,
        layout,
        &extra_objects,
        warnings,
    )?;
    restore_default_poor_bga(&mut intermediate, bms.bmp.poor_bmp.as_deref());
    intermediate.lnobj_wav_key = lnobj_wav_key(&source.lnobj_lines, base62_obj_ids, warnings);
    let bms_headers = source.bms_headers;
    intermediate.metadata.has_bms_random = source.has_bms_random;
    intermediate.metadata.miss_text = source_texts.miss_text;
    apply_raw_judge_rank_headers(&mut intermediate, &bms_headers);
    intermediate.metadata.source_url = bms
        .metadata
        .url
        .clone()
        .filter(|url| !url.is_empty())
        .or_else(|| bms_headers.get("URL").cloned())
        .unwrap_or_default();
    intermediate.metadata.append_url = append_url_from_headers(&bms_headers);
    intermediate.metadata.bms_headers = bms_headers;
    intermediate.identity = identity;
    Ok((intermediate, base62_obj_ids))
}

pub(crate) fn build_intermediate_from_bms<T: KeyLayoutMapper>(
    bms: &Bms,
    layout: ChartKeyLayout,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    build_intermediate_from_bms_with_extra_objects::<T>(bms, layout, &[], warnings)
}

fn build_intermediate_from_bms_with_extra_objects<T: KeyLayoutMapper>(
    bms: &Bms,
    layout: ChartKeyLayout,
    extra_objects: &[IntermediateObject],
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let metadata = build_metadata(bms);
    let mut resources = build_resources(bms);
    let mut objects = Vec::new();

    push_note_objects::<T>(bms, layout, &mut objects, warnings);
    push_bgm_objects::<T>(bms, &mut objects);
    push_bga_objects(bms, &mut objects);
    objects.extend_from_slice(extra_objects);
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
            | IntermediateObjectKind::MineNote { lane, .. }
            | IntermediateObjectKind::KaleidNote { lane, .. }
            | IntermediateObjectKind::FreeZone { lane, .. } => Some(lane),
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

#[cfg(test)]
#[path = "bms_rs_adapter/import_bench.rs"]
mod import_bench;
