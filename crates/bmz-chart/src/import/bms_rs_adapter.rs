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
//! - `#LNOBJ`: bms-rs 側で対応するノートが `NoteKind::Long` に書き換えられるため、
//!   こちらでは追加処理せず通常の Long-channel として扱う。
//! - `.pms`: `KeyLayoutPms` / `KeyLayoutPmsBmeType` による 9K SP (18K は drop + warning)
//!
//! 未対応 (warning に流すか drop):
//! - JUDGE 変更イベント (#EXRANK / chA0)
//! - TEXT / OPTION / VIDEO / SEEK 等
//! - foot pedal / free zone
//! - PMS 18K (2P 側ノート)

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
use crate::timing::TICKS_PER_MEASURE;

use super::decode::decode_bms_text;
use super::error::{ImportError, ImportWarning};
use super::intermediate::{
    BmpDef, BpmDef, IntermediateBgaKind, IntermediateChart, IntermediateMetadata,
    IntermediateObject, IntermediateObjectKind, IntermediateResources, MeasureInfo, StopDef,
    WavDef,
};

use crate::model::LongNoteMode;

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

pub fn import_bms_to_intermediate(
    source_path: &Path,
    random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    import_with_layout::<KeyLayoutBeat>(source_path, ChartKeyLayout::beat(), random_seed, warnings)
}

pub fn import_pms_to_intermediate(
    source_path: &Path,
    random_seed: Option<u64>,
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
            random_seed,
            warnings,
        ),
        PmsKeyLayout::BmeType => import_with_layout::<KeyLayoutPmsBmeType>(
            source_path,
            ChartKeyLayout::pms(PmsKeyLayout::BmeType),
            random_seed,
            warnings,
        ),
    }
}

fn import_with_layout<T: KeyLayoutMapper>(
    source_path: &Path,
    layout: ChartKeyLayout,
    random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let bytes = std::fs::read(source_path)
        .map_err(|source| ImportError::Io { path: source_path.to_path_buf(), source })?;
    let identity = compute_chart_identity(&bytes);
    let text = decode_bms_text(&bytes, warnings);
    let text = apply_beatoraja_random_control(&text, random_seed, warnings);
    let (parse_text, sparse_messages) = extract_sparse_bms_message_lines(&text, warnings);

    let BmsOutput { bms, warnings: bms_warnings } = parse_bms::<T, _, _, _>(
        &parse_text,
        default_config_with_rng(JavaRandom::new(random_seed.unwrap_or(0) as i64)).key_mapper::<T>(),
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

    let mut intermediate = build_intermediate_from_bms::<T>(&bms, layout, warnings)?;
    intermediate.identity = identity;
    Ok(intermediate)
}

#[derive(Debug, Clone, Copy)]
enum BeatorajaRandomControl<'a> {
    Random(&'a str),
    SetRandom(&'a str),
    If(&'a str),
    Else,
    ElseIf,
    EndIf,
    EndRandom,
}

fn apply_beatoraja_random_control(
    text: &str,
    random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> String {
    let mut rewritten = String::with_capacity(text.len());
    let mut rng = JavaRandom::new(random_seed.unwrap_or(0) as i64);
    let mut random_stack = Vec::new();
    let mut skip_stack = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;

        match beatoraja_random_control_line(line) {
            Some(BeatorajaRandomControl::Random(args)) => {
                if let Some(max) =
                    parse_beatoraja_control_int(args, line_number, "#RANDOM", warnings)
                {
                    let selected = if max <= 0 {
                        warnings.push(ImportWarning::ParserDiagnostic {
                            code: "RandomZeroClamped".to_string(),
                            message: format!(
                                "line {line_number} #RANDOM {max} is treated as #RANDOM 1 for beatoraja compatibility"
                            ),
                        });
                        1
                    } else {
                        rng.next_int_bound(max) + 1
                    };
                    random_stack.push(selected);
                }
            }
            Some(BeatorajaRandomControl::SetRandom(args)) => {
                if let Some(selected) =
                    parse_beatoraja_control_int(args, line_number, "#SETRANDOM", warnings)
                {
                    random_stack.push(selected);
                }
            }
            Some(BeatorajaRandomControl::If(args)) => {
                if let Some(&selected) = random_stack.last() {
                    if let Some(condition) =
                        parse_beatoraja_control_int(args, line_number, "#IF", warnings)
                    {
                        skip_stack.push(selected != condition);
                    }
                } else {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BeatorajaRandomIfWithoutRandom".to_string(),
                        message: format!(
                            "line {line_number} #IF has no active #RANDOM; continuing like beatoraja"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::Else | BeatorajaRandomControl::ElseIf) => {
                warnings.push(ImportWarning::ParserDiagnostic {
                    code: "BeatorajaRandomUnsupportedElse".to_string(),
                    message: format!(
                        "line {line_number} random #ELSE/#ELSEIF is ignored for beatoraja compatibility"
                    ),
                });
            }
            Some(BeatorajaRandomControl::EndIf) => {
                if skip_stack.pop().is_none() {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BeatorajaRandomEndifWithoutIf".to_string(),
                        message: format!(
                            "line {line_number} #ENDIF has no active #IF; continuing like beatoraja"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::EndRandom) => {
                if random_stack.pop().is_none() {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BeatorajaRandomEndrandomWithoutRandom".to_string(),
                        message: format!(
                            "line {line_number} #ENDRANDOM has no active #RANDOM; continuing like beatoraja"
                        ),
                    });
                }
            }
            None => {
                if !skip_stack.last().copied().unwrap_or(false) {
                    rewritten.push_str(line);
                }
            }
        }

        rewritten.push('\n');
    }

    rewritten
}

fn beatoraja_random_control_line(line: &str) -> Option<BeatorajaRandomControl<'_>> {
    let body = line.trim_start().strip_prefix('#')?;
    if starts_ignore_ascii_case(body, "ENDRANDOM") {
        return Some(BeatorajaRandomControl::EndRandom);
    }
    if starts_ignore_ascii_case(body, "ENDIF") {
        return Some(BeatorajaRandomControl::EndIf);
    }
    if starts_ignore_ascii_case(body, "ELSEIF") {
        return Some(BeatorajaRandomControl::ElseIf);
    }
    if starts_ignore_ascii_case(body, "ELSE") {
        return Some(BeatorajaRandomControl::Else);
    }
    if starts_ignore_ascii_case(body, "SETRANDOM") {
        return Some(BeatorajaRandomControl::SetRandom(command_args(body, "SETRANDOM")));
    }
    if starts_ignore_ascii_case(body, "RANDOM") {
        return Some(BeatorajaRandomControl::Random(command_args(body, "RANDOM")));
    }
    if starts_ignore_ascii_case(body, "IF") {
        return Some(BeatorajaRandomControl::If(command_args(body, "IF")));
    }
    None
}

fn starts_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value.get(..prefix.len()).is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

fn command_args<'a>(body: &'a str, command: &str) -> &'a str {
    body.get(command.len() + 1..).unwrap_or("").trim()
}

fn parse_beatoraja_control_int(
    args: &str,
    line_number: usize,
    command: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Option<i32> {
    match args.parse::<i32>() {
        Ok(value) => Some(value),
        Err(_) => {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "BeatorajaRandomInvalidArgument".to_string(),
                message: format!(
                    "line {line_number} {command} has invalid integer argument {args:?}; continuing like beatoraja"
                ),
            });
            None
        }
    }
}

pub(crate) fn build_intermediate_from_bms<T: KeyLayoutMapper>(
    bms: &Bms,
    layout: ChartKeyLayout,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let metadata = build_metadata(bms);
    let mut resources = build_resources(bms);
    let mut objects = Vec::new();

    push_note_objects::<T>(bms, layout, &mut objects, warnings);
    push_bgm_objects::<T>(bms, &mut objects);
    push_bga_objects(bms, &mut objects);
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
        lnobj_wav_key: None, // bms-rs 側で吸収済み
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

    Ok(intermediate)
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

fn extract_sparse_bms_message_lines(
    text: &str,
    warnings: &mut Vec<ImportWarning>,
) -> (String, Vec<SparseBmsMessage>) {
    let mut rewritten = String::with_capacity(text.len());
    let mut sparse_messages = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        if let Some(message) =
            extract_sparse_bms_message_line(line, line_number, sparse_messages.len())
        {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "SparseBmsMessage".to_string(),
                message: format!(
                    "line {} #{}{} has {} slots and {} non-zero objects; importing sparsely",
                    message.line_number,
                    message.measure,
                    message.channel,
                    message.object_count,
                    message.objects.len()
                ),
            });
            rewritten.push('#');
            rewritten.push_str(SPARSE_BMS_MARKER_HEADER);
            rewritten.push(' ');
            rewritten.push_str(&message.id.to_string());
            sparse_messages.push(message);
        } else {
            rewritten.push_str(line);
        }
        rewritten.push('\n');
    }

    (rewritten, sparse_messages)
}

fn extract_sparse_bms_message_line(
    line: &str,
    line_number: usize,
    sparse_id: usize,
) -> Option<SparseBmsMessage> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix('#')?;
    let colon = body.find(':')?;
    let head = &body[..colon];
    if head.len() < 5 || !head.is_ascii() {
        return None;
    }
    let measure_text = &head[..head.len() - 2];
    let channel = &head[head.len() - 2..];
    if channel.eq_ignore_ascii_case("02") {
        return None;
    }
    let payload = body[colon + 1..].trim();
    if payload.len() % 2 != 0 {
        return None;
    }
    let object_count = payload.len() / 2;
    if object_count <= SPARSE_BMS_MESSAGE_OBJECT_THRESHOLD {
        return None;
    }
    let measure = measure_text.parse::<u64>().ok()?;
    let mut objects = Vec::new();
    for (index, chunk) in payload.as_bytes().chunks_exact(2).enumerate() {
        if chunk != b"00" {
            let id = std::str::from_utf8(chunk).ok()?.to_string();
            objects.push(SparseBmsObject { index: index as u64, id });
        }
    }
    Some(SparseBmsMessage {
        id: sparse_id,
        line_number,
        measure,
        channel: channel.to_ascii_uppercase(),
        object_count: object_count as u64,
        objects,
    })
}

fn inject_sparse_bms_messages<T: KeyLayoutMapper>(
    bms: &mut Bms,
    sparse_messages: &[SparseBmsMessage],
    warnings: &mut Vec<ImportWarning>,
) {
    if sparse_messages.is_empty() {
        return;
    }

    let active_sparse_ids: Vec<usize> =
        bms.repr.raw_command_lines.iter().filter_map(|line| sparse_marker_id(line)).collect();
    for sparse_id in active_sparse_ids {
        if let Some(message) = sparse_messages.get(sparse_id) {
            inject_sparse_bms_message::<T>(bms, message, warnings);
        }
    }
    bms.repr.raw_command_lines.retain(|line| sparse_marker_id(line).is_none());

    for randomized in &mut bms.randomized {
        for branch in randomized.branches_mut() {
            inject_sparse_bms_messages::<T>(branch.sub_mut(), sparse_messages, warnings);
        }
    }
}

fn sparse_marker_id(line: &str) -> Option<usize> {
    let line = line.trim();
    let body = line.strip_prefix('#')?;
    let args = body.strip_prefix(SPARSE_BMS_MARKER_HEADER)?;
    args.trim().parse().ok()
}

fn inject_sparse_bms_message<T: KeyLayoutMapper>(
    bms: &mut Bms,
    message: &SparseBmsMessage,
    warnings: &mut Vec<ImportWarning>,
) {
    let Some(channel) = read_channel(&message.channel) else {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "SparseBmsMessageWarning".to_string(),
            message: format!(
                "line {} uses unsupported sparse channel {}",
                message.line_number, message.channel
            ),
        });
        return;
    };

    for object in &message.objects {
        let Some(time) = ObjTime::new(message.measure, object.index, message.object_count) else {
            continue;
        };
        let Ok(obj_id) = ObjId::try_from(&object.id, bms_uses_base62_obj_ids(bms)) else {
            continue;
        };
        if obj_id.as_u16() == 0 {
            continue;
        }

        match channel {
            Channel::Bgm => {
                bms.wav.notes.push_note(WavObj {
                    offset: time,
                    channel_id: NoteChannelId::bgm(),
                    wav_id: obj_id,
                });
            }
            Channel::Note { channel_id } if T::from_channel_id(channel_id).is_some() => {
                bms.wav.notes.push_note(WavObj { offset: time, channel_id, wav_id: obj_id });
            }
            Channel::BpmChangeU8 => {
                bms.bpm.bpm_changes_u8.insert(time, obj_id.as_u16().min(u8::MAX as u16) as u8);
            }
            Channel::BpmChange => {
                if let Some(bpm) =
                    bms.bpm.bpm_defs.get(&obj_id).and_then(|sv| sv.value().as_ref().ok()).cloned()
                {
                    bms.bpm.bpm_changes.insert(time, BpmChangeObj { time, bpm });
                } else {
                    warnings.push(ImportWarning::MissingBpmDefinition { key: obj_id.as_u16() });
                }
            }
            Channel::Stop => {
                if let Some(duration) =
                    bms.stop.stop_defs.get(&obj_id).and_then(|sv| sv.value().as_ref().ok()).cloned()
                {
                    bms.stop.stops.insert(time, StopObj { time, duration });
                } else {
                    warnings.push(ImportWarning::MissingStopDefinition { key: obj_id.as_u16() });
                }
            }
            Channel::Scroll => {
                if let Some(factor) = bms
                    .scroll
                    .scroll_defs
                    .get(&obj_id)
                    .and_then(|sv| sv.value().as_ref().ok())
                    .cloned()
                {
                    bms.scroll
                        .scrolling_factor_changes
                        .insert(time, ScrollingFactorObj { time, factor });
                }
            }
            Channel::Speed => {
                if let Some(factor) = bms
                    .speed
                    .speed_defs
                    .get(&obj_id)
                    .and_then(|sv| sv.value().as_ref().ok())
                    .cloned()
                {
                    bms.speed.speed_factor_changes.insert(time, SpeedObj { time, factor });
                }
            }
            Channel::BgaBase | Channel::BgaPoor | Channel::BgaLayer | Channel::BgaLayer2 => {
                if let Some(layer) = BgaLayer::from_channel(channel) {
                    bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer });
                }
            }
            Channel::BgaBaseOpacity
            | Channel::BgaPoorOpacity
            | Channel::BgaLayerOpacity
            | Channel::BgaLayer2Opacity => {
                if let Some(layer) = BgaLayer::from_channel(channel) {
                    bms.bmp.bga_opacity_changes.entry(layer).or_default().insert(
                        time,
                        BgaOpacityObj {
                            time,
                            layer,
                            opacity: obj_id.as_u16().min(u8::MAX as u16) as u8,
                        },
                    );
                }
            }
            Channel::BgaBaseArgb
            | Channel::BgaPoorArgb
            | Channel::BgaLayerArgb
            | Channel::BgaLayer2Argb => {
                if let (Some(layer), Some(argb)) =
                    (BgaLayer::from_channel(channel), bms.bmp.argb_defs.get(&obj_id).copied())
                {
                    bms.bmp
                        .bga_argb_changes
                        .entry(layer)
                        .or_default()
                        .insert(time, BgaArgbObj { time, layer, argb });
                }
            }
            Channel::BgaKeybound => {
                if let Some(event) = bms.bmp.swbga_events.get(&obj_id).cloned() {
                    bms.bmp.bga_keybound_events.insert(time, BgaKeyboundObj { time, event });
                }
            }
            Channel::BgmVolume => {
                bms.volume.bgm_volume_changes.insert(
                    time,
                    BgmVolumeObj { time, volume: obj_id.as_u16().min(u8::MAX as u16) as u8 },
                );
            }
            Channel::KeyVolume => {
                bms.volume.key_volume_changes.insert(
                    time,
                    KeyVolumeObj { time, volume: obj_id.as_u16().min(u8::MAX as u16) as u8 },
                );
            }
            Channel::Text => {
                if let Some(text) = bms.text.texts.get(&obj_id).cloned() {
                    bms.text.text_events.insert(time, TextObj { time, text });
                }
            }
            Channel::Judge => {
                if let Some(judge_level) =
                    bms.judge.exrank_defs.get(&obj_id).map(|def| def.judge_level)
                {
                    bms.judge.judge_events.insert(time, JudgeObj { time, judge_level });
                }
            }
            Channel::SectionLen | Channel::Seek | Channel::OptionChange => {}
            _ => {}
        }
    }
}

fn message_channel_bytes(line: &str) -> Option<[u8; 2]> {
    let line = line.trim();
    if !line.starts_with('#') {
        return None;
    }
    let body = line.strip_prefix('#')?;
    let colon = body.find(':')?;
    let head = &body[..colon];
    if head.len() < 5 || !head.is_ascii() {
        return None;
    }
    let channel_str = &head[head.len() - 2..];
    let bytes = channel_str.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    Some([bytes[0], bytes[1]])
}

/// PMS Standard: P2 K2–K5 (= PMS K6–K9) の各 note kind。
fn pms_standard_upper_channel(channel: [u8; 2]) -> bool {
    let first = channel[0].to_ascii_uppercase();
    matches!(first, b'2' | b'3' | b'5' | b'6' | b'D' | b'E') && matches!(channel[1], b'2'..=b'5')
}

/// PMS BME-type: P1 ch 16–19 (= PMS K6–K9) の各 note kind。
fn pms_bme_upper_channel(channel: [u8; 2]) -> bool {
    let first = channel[0].to_ascii_uppercase();
    matches!(first, b'1' | b'3' | b'5' | b'6' | b'D' | b'E') && matches!(channel[1], b'6'..=b'9')
}

fn build_metadata(bms: &Bms) -> IntermediateMetadata {
    let initial_bpm = bms
        .bpm
        .bpm
        .as_ref()
        .and_then(|sv| sv.value().as_ref().ok().map(|v| v.get()))
        .unwrap_or(130.0);
    let total = bms.judge.total.as_ref().and_then(|sv| sv.value().as_ref().ok().map(|v| v.get()));
    let judge_rank = bms.judge.rank.map(judge_level_to_int);

    IntermediateMetadata {
        title: bms.music_info.title.clone().unwrap_or_default(),
        subtitle: bms.music_info.subtitle.clone().unwrap_or_default(),
        artist: bms.music_info.artist.clone().unwrap_or_default(),
        subartist: bms.music_info.sub_artist.clone().unwrap_or_default(),
        genre: bms.music_info.genre.clone().unwrap_or_default(),
        play_level: bms.metadata.play_level.map(|v| v.to_string()).unwrap_or_default(),
        difficulty_name: bms.metadata.difficulty.map(|v| v.to_string()).unwrap_or_default(),
        judge_rank,
        initial_bpm,
        total,
        stage_file: bms
            .sprite
            .stage_file
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        banner_file: bms
            .sprite
            .banner
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        backbmp_file: bms
            .sprite
            .back_bmp
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        preview_file: bms
            .music_info
            .preview_music
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        volwav_percent: bms.volume.volume.relative_percent,
        long_note_mode: map_ln_mode(bms.repr.ln_mode),
        long_note_mode_defined: bms_has_explicit_ln_mode(bms),
        has_bga: false,
        key_mode: KeyMode::default(),
        base62_obj_ids: bms_uses_base62_obj_ids(bms),
        suppress_bar_lines: false,
    }
}

fn bms_uses_base62_obj_ids(bms: &Bms) -> bool {
    if bms.repr.case_sensitive_obj_id {
        return true;
    }
    // bms-rs は `#BASE 62` 処理時に RefCell だけ更新し `repr.case_sensitive_obj_id` を
    // 立てないことがあるため、記録済みヘッダ行も見る。
    bms.repr.raw_command_lines.iter().any(|line| line.eq_ignore_ascii_case("#BASE 62"))
}

fn bms_has_explicit_ln_mode(bms: &Bms) -> bool {
    bms.repr.raw_command_lines.iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.get(..7).is_some_and(|prefix| prefix.eq_ignore_ascii_case("#LNMODE"))
    })
}

fn map_ln_mode(mode: LnMode) -> LongNoteMode {
    match mode {
        LnMode::Ln => LongNoteMode::Ln,
        LnMode::Cn => LongNoteMode::Cn,
        LnMode::Hcn => LongNoteMode::Hcn,
    }
}

fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn judge_level_to_int(level: JudgeLevel) -> i32 {
    match level {
        JudgeLevel::VeryHard => 0,
        JudgeLevel::Hard => 1,
        JudgeLevel::Normal => 2,
        JudgeLevel::Easy => 3,
        JudgeLevel::OtherInt(v) => v.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
    }
}

fn build_resources(bms: &Bms) -> IntermediateResources {
    let wavs: Vec<WavDef> = bms
        .wav
        .wav_files
        .iter()
        .map(|(id, path)| WavDef { key: id.as_u16(), path: path.clone() })
        .collect();
    let bmps: Vec<BmpDef> = bms
        .bmp
        .bmp_files
        .iter()
        .map(|(id, bmp)| BmpDef { key: id.as_u16(), path: bmp.file.clone() })
        .collect();
    let bpm_table: Vec<BpmDef> = bms
        .bpm
        .bpm_defs
        .iter()
        .filter_map(|(id, sv)| {
            sv.value().as_ref().ok().map(|v| BpmDef { key: id.as_u16(), bpm: v.get() })
        })
        .collect();
    let stop_table: Vec<StopDef> = bms
        .stop
        .stop_defs
        .iter()
        .filter_map(|(id, sv)| {
            sv.value().as_ref().ok().map(|v| StopDef { key: id.as_u16(), value: v.get() as u64 })
        })
        .collect();
    IntermediateResources { wavs, bmps, bpm_table, stop_table, swbga_defs: build_swbga_defs(bms) }
}

fn build_swbga_defs(bms: &Bms) -> Vec<super::intermediate::SwBgaDef> {
    bms.bmp
        .swbga_events
        .iter()
        .map(|(id, event)| super::intermediate::SwBgaDef {
            id: id.as_u16(),
            frame_rate_ms: event.frame_rate,
            total_time_ms: event.total_time,
            line: event.line,
            loop_mode: event.loop_mode,
            chroma_alpha: event.argb.alpha,
            chroma_red: event.argb.red,
            chroma_green: event.argb.green,
            chroma_blue: event.argb.blue,
            pattern: event.pattern.clone(),
        })
        .collect()
}

fn push_bga_keybound_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for keybound in bms.bmp.bga_keybound_events.values() {
        let swbga_key = bms
            .bmp
            .swbga_events
            .iter()
            .find(|(_, event)| *event == &keybound.event)
            .map(|(id, _)| id.as_u16());
        let Some(swbga_key) = swbga_key else {
            continue;
        };
        objects.push(IntermediateObject {
            measure: track_of(keybound.time),
            position_num: keybound.time.numerator() as u32,
            position_den: keybound.time.denominator().get() as u32,
            kind: IntermediateObjectKind::BgaKeybound { swbga_key },
        });
    }
}

fn push_note_objects<T: KeyLayoutMapper>(
    bms: &Bms,
    layout: ChartKeyLayout,
    objects: &mut Vec<IntermediateObject>,
    warnings: &mut Vec<ImportWarning>,
) {
    for note in bms.notes().all_notes() {
        let Some(mapping) = T::from_channel_id(note.channel_id) else {
            continue;
        };
        let Some(lane) = map_lane(layout, mapping.side(), mapping.key()) else {
            if layout.is_pms() && mapping.side() == PlayerSide::Player2 {
                warnings.push(ImportWarning::UnsupportedPmsPlayerSide { side: 2 });
            } else {
                warnings
                    .push(ImportWarning::UnsupportedChannel { channel: note.channel_id.as_u16() });
            }
            continue;
        };
        let wav_id = note.wav_id.as_u16();
        let kind = match mapping.kind() {
            BmsNoteKind::Visible => IntermediateObjectKind::VisibleNote {
                lane,
                wav_key: (wav_id != 0).then_some(wav_id),
            },
            BmsNoteKind::Invisible => IntermediateObjectKind::InvisibleNote {
                lane,
                wav_key: (wav_id != 0).then_some(wav_id),
            },
            BmsNoteKind::Long => IntermediateObjectKind::LongChannelNote {
                lane,
                wav_key: (wav_id != 0).then_some(wav_id),
            },
            BmsNoteKind::Landmine => {
                IntermediateObjectKind::MineNote { lane, wav_key: None, damage: wav_id }
            }
        };
        objects.push(IntermediateObject {
            measure: track_of(note.offset),
            position_num: note.offset.numerator() as u32,
            position_den: note.offset.denominator().get() as u32,
            kind,
        });
    }
}

fn push_bgm_objects<T: KeyLayoutMapper>(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for note in bms.notes().bgms::<T>() {
        objects.push(IntermediateObject {
            measure: track_of(note.offset),
            position_num: note.offset.numerator() as u32,
            position_den: note.offset.denominator().get() as u32,
            kind: IntermediateObjectKind::Bgm { wav_key: note.wav_id.as_u16() },
        });
    }
}

fn push_bga_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    use bms_rs::bms::model::obj::BgaLayer;
    for bga in bms.bmp.bga_changes.values() {
        let kind = match bga.layer {
            BgaLayer::Base => IntermediateBgaKind::Base,
            BgaLayer::Poor => IntermediateBgaKind::Poor,
            BgaLayer::Overlay => IntermediateBgaKind::Layer,
            BgaLayer::Overlay2 => IntermediateBgaKind::Layer2,
            _ => continue,
        };
        objects.push(IntermediateObject {
            measure: track_of(bga.time),
            position_num: bga.time.numerator() as u32,
            position_den: bga.time.denominator().get() as u32,
            kind: IntermediateObjectKind::Bga { bmp_key: bga.id.as_u16(), kind },
        });
    }
}

fn push_bpm_change_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for (time, bpm) in &bms.bpm.bpm_changes_u8 {
        if *bpm == 0 {
            continue;
        }
        objects.push(IntermediateObject {
            measure: track_of(*time),
            position_num: time.numerator() as u32,
            position_den: time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBpm { bpm: *bpm as f64 },
        });
    }
    for change in bms.bpm.bpm_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBpm { bpm: change.bpm.get() },
        });
    }
}

fn push_scroll_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for change in bms.scroll.scrolling_factor_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetScroll { factor: change.factor.get() },
        });
    }
}

fn push_speed_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for change in bms.speed.speed_factor_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetSpeed { factor: change.factor.get() },
        });
    }
}

fn push_judge_rank_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for judge_obj in bms.judge.judge_events.values() {
        objects.push(IntermediateObject {
            measure: track_of(judge_obj.time),
            position_num: judge_obj.time.numerator() as u32,
            position_den: judge_obj.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetJudgeRank {
                rank_percent: judge_level_to_rank_percent(judge_obj.judge_level),
            },
        });
    }
}

fn push_volume_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for change in bms.volume.bgm_volume_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBgmVolume { volume: change.volume },
        });
    }
    for change in bms.volume.key_volume_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetKeyVolume { volume: change.volume },
        });
    }
}

fn push_text_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for text_obj in bms.text.text_events.values() {
        objects.push(IntermediateObject {
            measure: track_of(text_obj.time),
            position_num: text_obj.time.numerator() as u32,
            position_den: text_obj.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetText { text: text_obj.text.clone() },
        });
    }
}

fn push_bga_opacity_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for changes in bms.bmp.bga_opacity_changes.values() {
        for change in changes.values() {
            let Some(kind) = map_bga_layer_kind(change.layer) else {
                continue;
            };
            objects.push(IntermediateObject {
                measure: track_of(change.time),
                position_num: change.time.numerator() as u32,
                position_den: change.time.denominator().get() as u32,
                kind: IntermediateObjectKind::SetBgaOpacity { kind, opacity: change.opacity },
            });
        }
    }
}

fn push_bga_argb_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for changes in bms.bmp.bga_argb_changes.values() {
        for change in changes.values() {
            let Some(kind) = map_bga_layer_kind(change.layer) else {
                continue;
            };
            objects.push(IntermediateObject {
                measure: track_of(change.time),
                position_num: change.time.numerator() as u32,
                position_den: change.time.denominator().get() as u32,
                kind: IntermediateObjectKind::SetBgaArgb {
                    kind,
                    alpha: change.argb.alpha,
                    red: change.argb.red,
                    green: change.argb.green,
                    blue: change.argb.blue,
                },
            });
        }
    }
}

fn map_bga_layer_kind(layer: bms_rs::bms::model::obj::BgaLayer) -> Option<IntermediateBgaKind> {
    use bms_rs::bms::model::obj::BgaLayer;
    match layer {
        BgaLayer::Base => Some(IntermediateBgaKind::Base),
        BgaLayer::Poor => Some(IntermediateBgaKind::Poor),
        BgaLayer::Overlay => Some(IntermediateBgaKind::Layer),
        BgaLayer::Overlay2 => Some(IntermediateBgaKind::Layer2),
        _ => None,
    }
}

fn judge_level_to_rank_percent(level: JudgeLevel) -> i32 {
    judge_rank_to_percent(judge_level_to_int(level))
}

fn judge_rank_to_percent(rank: i32) -> i32 {
    match rank {
        0 => 25,
        1 => 50,
        2 => 75,
        3 => 100,
        4 => 125,
        r if r >= 10 => r,
        _ => 75,
    }
}

fn push_stop_objects(
    bms: &Bms,
    objects: &mut Vec<IntermediateObject>,
    resources: &mut IntermediateResources,
) {
    let start_key = resources.stop_table.iter().map(|d| d.key).max().unwrap_or(0) + 1;
    for (key, stop) in (start_key..).zip(bms.stop.stops.values()) {
        resources.stop_table.push(StopDef { key, value: stop.duration.get() as u64 });
        objects.push(IntermediateObject {
            measure: track_of(stop.time),
            position_num: stop.time.numerator() as u32,
            position_den: stop.time.denominator().get() as u32,
            kind: IntermediateObjectKind::Stop { stop_key: key },
        });
    }
}

fn track_of(time: ObjTime) -> u32 {
    u32::try_from(time.track().0).unwrap_or(u32::MAX)
}

fn compute_max_measure(bms: &Bms, objects: &[IntermediateObject]) -> Result<u32, ImportError> {
    let mut max = objects.iter().map(|o| o.measure).max().unwrap_or(0);
    if let Some(last) = bms.last_obj_time() {
        max = max.max(track_of(last));
    }
    for &track in bms.section_len.section_len_changes.keys() {
        max = max.max(u32::try_from(track.0).unwrap_or(u32::MAX));
    }
    if max > MAX_SUPPORTED_MEASURE {
        return Err(ImportError::InvalidChart {
            message: format!(
                "chart has measure {max}, exceeding supported maximum {MAX_SUPPORTED_MEASURE}"
            ),
        });
    }
    Ok(max)
}

fn build_measures(max_measure: u32, bms: &Bms) -> Vec<MeasureInfo> {
    let mut measures = Vec::with_capacity(max_measure as usize + 1);
    let mut start_tick = 0_u64;
    for index in 0..=max_measure {
        let (num, den) = bms
            .section_len
            .section_len_changes
            .get(&bms_rs::bms::command::time::Track(index as u64))
            .map(|change| fin_f64_to_ratio(change.length.get()))
            .unwrap_or((1, 1));
        let tick_len = TICKS_PER_MEASURE as u64 * num as u64 / den.max(1) as u64;
        measures.push(MeasureInfo {
            index,
            length_ratio_num: num,
            length_ratio_den: den.max(1),
            start_tick: ChartTick(start_tick),
            tick_len,
        });
        start_tick += tick_len;
    }
    measures
}

fn fin_f64_to_ratio(value: f64) -> (u32, u32) {
    if !value.is_finite() || value <= 0.0 {
        return (1, 1);
    }
    let den = 1_000_000_u32;
    let num = (value * den as f64).round() as u32;
    let gcd = gcd(num.max(1), den);
    (num.max(1) / gcd, den / gcd)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn map_lane(layout: ChartKeyLayout, side: PlayerSide, key: Key) -> Option<Lane> {
    match layout {
        ChartKeyLayout::Beat(_) => map_lane_beat(side, key),
        ChartKeyLayout::Pms(_) => map_lane_pms(side, key),
    }
}

fn map_lane_beat(side: PlayerSide, key: Key) -> Option<Lane> {
    match (side, key) {
        (PlayerSide::Player1, Key::Key(1)) => Some(Lane::Key1),
        (PlayerSide::Player1, Key::Key(2)) => Some(Lane::Key2),
        (PlayerSide::Player1, Key::Key(3)) => Some(Lane::Key3),
        (PlayerSide::Player1, Key::Key(4)) => Some(Lane::Key4),
        (PlayerSide::Player1, Key::Key(5)) => Some(Lane::Key5),
        (PlayerSide::Player1, Key::Key(6)) => Some(Lane::Key6),
        (PlayerSide::Player1, Key::Key(7)) => Some(Lane::Key7),
        (PlayerSide::Player1, Key::Scratch(_)) => Some(Lane::Scratch),
        (PlayerSide::Player2, Key::Key(1)) => Some(Lane::Key8),
        (PlayerSide::Player2, Key::Key(2)) => Some(Lane::Key9),
        (PlayerSide::Player2, Key::Key(3)) => Some(Lane::Key10),
        (PlayerSide::Player2, Key::Key(4)) => Some(Lane::Key11),
        (PlayerSide::Player2, Key::Key(5)) => Some(Lane::Key12),
        (PlayerSide::Player2, Key::Key(6)) => Some(Lane::Key13),
        (PlayerSide::Player2, Key::Key(7)) => Some(Lane::Key14),
        (PlayerSide::Player2, Key::Scratch(_)) => Some(Lane::Scratch2),
        _ => None,
    }
}

fn map_lane_pms(side: PlayerSide, key: Key) -> Option<Lane> {
    match (side, key) {
        (PlayerSide::Player1, Key::Key(n)) => Lane::from_pms_key(n),
        _ => None,
    }
}

fn map_bms_warning(w: &BmsWarning) -> Option<ImportWarning> {
    use bms_rs::bms::parse::ParseWarning;
    use bms_rs::bms::parse::check_playing::{PlayingError, PlayingWarning};

    let (code, message) = match w {
        BmsWarning::Lex(inner) => ("LexWarning", format!("{}", inner.content())),
        BmsWarning::Parse(inner) => {
            let code = match inner.content() {
                ParseWarning::SyntaxError(_) => "ParseSyntaxError",
                ParseWarning::UndefinedObject(_) => "ParseUndefinedObject",
                ParseWarning::DuplicatingDef(_) => "ParseDuplicatingDef",
                ParseWarning::DuplicatingTrackObj(_, _) => "ParseDuplicatingTrackObj",
                ParseWarning::DuplicatingChannelObj(_, _) => "ParseDuplicatingChannelObj",
                ParseWarning::OutOfBase62 => "ParseOutOfBase62",
            };
            (code, format!("{}", inner.content()))
        }
        BmsWarning::PlayingWarning(w) => {
            let code = match w {
                PlayingWarning::TotalUndefined => "PlayingTotalUndefined",
                PlayingWarning::NoDisplayableNotes => "PlayingNoDisplayableNotes",
                PlayingWarning::NoPlayableNotes => "PlayingNoPlayableNotes",
                PlayingWarning::StartBpmUndefined => "PlayingStartBpmUndefined",
                _ => "PlayingWarningOther",
            };
            (code, format!("{w}"))
        }
        BmsWarning::PlayingError(e) => {
            let code = match e {
                PlayingError::InvalidBpm { .. } => "PlayingInvalidBpm",
                PlayingError::InvalidStop { .. } => "PlayingInvalidStop",
                PlayingError::InvalidSpeed { .. } => "PlayingInvalidSpeed",
                PlayingError::InvalidScroll { .. } => "PlayingInvalidScroll",
                PlayingError::InvalidSeek { .. } => "PlayingInvalidSeek",
                _ => "PlayingErrorOther",
            };
            (code, format!("{e}"))
        }
        _ => ("BmsWarningOther", format!("{w:?}")),
    };
    Some(ImportWarning::ParserDiagnostic { code: code.to_string(), message })
}

#[cfg(test)]
mod tests {
    use bmz_core::lane::KeyMode;

    use super::*;

    const PMS_HEADER: &str = "\
#TITLE PMS Test
#ARTIST Tester
#BPM 120
#WAV01 key.wav
";

    fn pms_note_lines_standard() -> String {
        let mut lines = String::from(PMS_HEADER);
        for (i, channel) in
            ["11", "12", "13", "14", "15", "22", "23", "24", "25"].into_iter().enumerate()
        {
            let measure = i + 1;
            lines.push_str(&format!("#{measure:03}{channel}:01\n"));
        }
        lines
    }

    fn pms_note_lines_bme() -> String {
        let mut lines = String::from(PMS_HEADER);
        for (i, channel) in
            ["11", "12", "13", "14", "15", "16", "17", "18", "19"].into_iter().enumerate()
        {
            let measure = i + 1;
            lines.push_str(&format!("#{measure:03}{channel}:01\n"));
        }
        lines
    }

    fn import_pms_text(text: &str) -> IntermediateChart {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pms");
        std::fs::write(&path, text).unwrap();
        std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
        let mut warnings = Vec::new();
        import_pms_to_intermediate(&path, None, &mut warnings).unwrap()
    }

    fn note_lanes(chart: &IntermediateChart) -> Vec<Lane> {
        chart
            .objects
            .iter()
            .filter_map(|object| match object.kind {
                IntermediateObjectKind::VisibleNote { lane, .. } => Some(lane),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn detect_pms_variant_standard_from_p2_upper_channels() {
        let (variant, conflict) = detect_pms_variant(&pms_note_lines_standard());
        assert_eq!(variant, PmsKeyLayout::Standard);
        assert!(!conflict);
    }

    #[test]
    fn detect_pms_variant_ignores_non_message_headers_with_colons() {
        let text = "\
#TITLE 赤 (原曲: 天衣無縫) [9K NORMAL]
#BPM 120
";
        let (variant, conflict) = detect_pms_variant(text);
        assert_eq!(variant, PmsKeyLayout::Standard);
        assert!(!conflict);
    }

    #[test]
    fn detect_pms_variant_bme_from_p1_upper_channels() {
        let (variant, conflict) = detect_pms_variant(&pms_note_lines_bme());
        assert_eq!(variant, PmsKeyLayout::BmeType);
        assert!(!conflict);
    }

    #[test]
    fn pms_standard_9k_maps_key1_through_key9() {
        let chart = import_pms_text(&pms_note_lines_standard());
        assert_eq!(chart.metadata.key_mode, KeyMode::K9);
        let lanes = note_lanes(&chart);
        assert_eq!(lanes.len(), 9);
        for (expected, actual) in [
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
            Lane::Key6,
            Lane::Key7,
            Lane::Key8,
            Lane::Key9,
        ]
        .into_iter()
        .zip(lanes)
        {
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn pms_bme_9k_maps_key1_through_key9() {
        let chart = import_pms_text(&pms_note_lines_bme());
        assert_eq!(chart.metadata.key_mode, KeyMode::K9);
        let lanes = note_lanes(&chart);
        assert_eq!(lanes.len(), 9);
        assert!(lanes.contains(&Lane::Key9));
    }

    #[test]
    fn pms_5k_still_reports_k9_key_mode() {
        let mut text = String::from(PMS_HEADER);
        for (i, channel) in ["11", "12", "13", "14", "15"].into_iter().enumerate() {
            let measure = i + 1;
            text.push_str(&format!("#{measure:03}{channel}:01\n"));
        }
        let chart = import_pms_text(&text);
        assert_eq!(chart.metadata.key_mode, KeyMode::K9);
        assert_eq!(note_lanes(&chart).len(), 5);
    }

    fn import_bms_text(text: &str) -> IntermediateChart {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bms");
        std::fs::write(&path, text).unwrap();
        std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
        let mut warnings = Vec::new();
        import_bms_to_intermediate(&path, None, &mut warnings).unwrap()
    }

    fn import_bms_text_with_warnings(text: &str) -> (IntermediateChart, Vec<ImportWarning>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bms");
        std::fs::write(&path, text).unwrap();
        std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
        let mut warnings = Vec::new();
        let chart = import_bms_to_intermediate(&path, None, &mut warnings).unwrap();
        (chart, warnings)
    }

    const BMS_HEADER: &str = "\
#TITLE BMS Test
#ARTIST Tester
#BPM 120
#WAV01 key.wav
";

    fn ue_8k_note_lines() -> String {
        let mut lines = String::from(BMS_HEADER);
        for (i, channel) in ["11", "12", "13", "14", "15", "16", "18", "19"].into_iter().enumerate()
        {
            let measure = i + 1;
            lines.push_str(&format!("#{measure:03}{channel}:01\n"));
        }
        lines
    }

    #[test]
    fn detect_key_mode_from_headers_parses_qwilight_tags() {
        use bms_rs::bms::command::channel::mapper::KeyLayoutBeat;
        use bms_rs::bms::{default_config, parse_bms};

        let parse =
            |text: &str| parse_bms::<KeyLayoutBeat, _, _, _>(&text, default_config()).bms.unwrap();

        assert_eq!(
            detect_key_mode_from_bms_headers(&parse("#4K\n"), ChartKeyLayout::beat()),
            Some(KeyMode::K4),
        );
        assert_eq!(
            detect_key_mode_from_bms_headers(&parse("#6K\n"), ChartKeyLayout::beat()),
            Some(KeyMode::K6),
        );
        assert_eq!(
            detect_key_mode_from_bms_headers(&parse("#8K\n"), ChartKeyLayout::beat()),
            Some(KeyMode::K8),
        );
        assert_eq!(
            detect_key_mode_from_bms_headers(
                &parse("* EXPANSION\n#6K\n#8K\n"),
                ChartKeyLayout::beat(),
            ),
            Some(KeyMode::K8),
        );
        assert_eq!(
            detect_key_mode_from_bms_headers(&parse("#TITLE x\n"), ChartKeyLayout::beat()),
            None,
        );
        assert_eq!(
            detect_key_mode_from_bms_headers(
                &parse("#8K\n"),
                ChartKeyLayout::pms(PmsKeyLayout::Standard),
            ),
            None,
        );
    }

    #[test]
    fn bms_8k_header_overrides_lane_detected_k7() {
        let mut text = ue_8k_note_lines();
        text.push_str("#8K\n");
        let chart = import_bms_text(&text);
        assert_eq!(chart.metadata.key_mode, KeyMode::K8);
    }

    #[test]
    fn bms_without_qwilight_header_uses_lane_detect() {
        let chart = import_bms_text(&ue_8k_note_lines());
        assert_eq!(chart.metadata.key_mode, KeyMode::K7);
    }

    #[test]
    fn bms_4k_and_6k_headers_set_key_mode() {
        let mut text = ue_8k_note_lines();
        text.push_str("#4K\n");
        assert_eq!(import_bms_text(&text).metadata.key_mode, KeyMode::K4);

        let mut text = ue_8k_note_lines();
        text.push_str("#6K\n");
        assert_eq!(import_bms_text(&text).metadata.key_mode, KeyMode::K6);
    }

    #[test]
    fn bms_random_zero_is_clamped_to_one_for_beatoraja_compatibility() {
        let (chart, warnings) = import_bms_text_with_warnings(
            "\
#TITLE Random Zero
#BPM 120
#WAV01 key.wav
#RANDOM 0
#IF 1
#00111:01
#ENDIF
#ENDRANDOM
",
        );

        assert_eq!(note_lanes(&chart), vec![Lane::Key1]);
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ImportWarning::ParserDiagnostic { code, .. } if code == "RandomZeroClamped"
        )));
    }

    #[test]
    fn bms_random_control_is_flattened_like_beatoraja() {
        let (chart, _warnings) = import_bms_text_with_warnings(
            "\
#TITLE Random Flatten
#BPM 120
#WAV01 key.wav
#RANDOM 1
#IF 2
#00111:01
#ENDIF
#IF 1
#00212:01
#ENDIF
",
        );

        assert_eq!(note_lanes(&chart), vec![Lane::Key2]);
    }

    #[test]
    fn bms_random_orphan_if_warns_and_continues_like_beatoraja() {
        let (chart, warnings) = import_bms_text_with_warnings(
            "\
#TITLE Orphan If
#BPM 120
#WAV01 key.wav
#IF 1
#00111:01
#ENDIF
",
        );

        assert_eq!(note_lanes(&chart), vec![Lane::Key1]);
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ImportWarning::ParserDiagnostic { code, .. }
                if code == "BeatorajaRandomIfWithoutRandom"
        )));
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ImportWarning::ParserDiagnostic { code, .. }
                if code == "BeatorajaRandomEndifWithoutIf"
        )));
    }

    #[test]
    fn bms_setrandom_is_flattened_with_fixed_condition() {
        let (chart, _warnings) = import_bms_text_with_warnings(
            "\
#TITLE SetRandom
#BPM 120
#WAV01 key.wav
#SETRANDOM 2
#IF 1
#00111:01
#ENDIF
#IF 2
#00212:01
#ENDIF
#ENDRANDOM
",
        );

        assert_eq!(note_lanes(&chart), vec![Lane::Key2]);
    }

    #[test]
    fn bms_8k_ue_sample_reports_k8_when_present() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/songs/8K U_E FULL PACK 1.1/[r] Baby/_baby_8K_Hard.bms"
        ));
        if !path.exists() {
            return;
        }
        let mut warnings = Vec::new();
        let chart = import_bms_to_intermediate(path, None, &mut warnings).unwrap();
        assert_eq!(chart.metadata.key_mode, KeyMode::K8);
    }

    #[test]
    fn pms_18k_player2_notes_are_dropped_with_warning() {
        let mut text = String::from(PMS_HEADER);
        text.push_str("#00121:01\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pms");
        std::fs::write(&path, &text).unwrap();
        std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
        let mut warnings = Vec::new();
        let chart = import_pms_to_intermediate(&path, None, &mut warnings).unwrap();
        assert!(note_lanes(&chart).is_empty());
        assert!(
            warnings.iter().any(|warning| matches!(
                warning,
                ImportWarning::UnsupportedPmsPlayerSide { side: 2 }
            ))
        );
    }
}
