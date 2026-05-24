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
//!   - 04/06/07: BGA (Base/Poor/Overlay→Layer)
//!   - 09: STOP
//!   - 1x/2x: Visible (P1/P2)
//!   - 3x/4x: Invisible (P1/P2)
//!   - 5x/6x: Long-channel (P1/P2)
//!   - Dx/Ex: Landmine (P1/P2)
//! - `#LNOBJ`: bms-rs 側で対応するノートが `NoteKind::Long` に書き換えられるため、
//!   こちらでは追加処理せず通常の Long-channel として扱う。
//!
//! 未対応 (warning に流すか drop):
//! - SCROLL / SPEED
//! - JUDGE 変更イベント
//! - TEXT / OPTION / VIDEO / SEEK 等
//! - foot pedal / free zone

use std::path::Path;

use bms_rs::bms::command::JudgeLevel;
use bms_rs::bms::command::channel::mapper::{KeyLayoutBeat, KeyLayoutMapper, KeyMapping};
use bms_rs::bms::command::channel::{Key, NoteKind as BmsNoteKind, PlayerSide};
use bms_rs::bms::command::time::ObjTime;
use bms_rs::bms::model::Bms;
use bms_rs::bms::rng::JavaRandom;
use bms_rs::bms::{BmsOutput, BmsWarning, default_config_with_rng, parse_bms};
use bmz_core::chart::ChartIdentity;
use bmz_core::lane::{KeyMode, Lane};
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

pub fn import_bms_to_intermediate(
    source_path: &Path,
    random_seed: Option<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<IntermediateChart, ImportError> {
    let bytes = std::fs::read(source_path)
        .map_err(|source| ImportError::Io { path: source_path.to_path_buf(), source })?;
    let identity = compute_chart_identity(&bytes);
    let text = decode_bms_text(&bytes, warnings);

    let BmsOutput { bms, warnings: bms_warnings } = parse_bms::<KeyLayoutBeat, _, _, _>(
        &text,
        default_config_with_rng(JavaRandom::new(random_seed.unwrap_or(0) as i64)),
    );
    for w in bms_warnings {
        if let Some(w) = map_bms_warning(&w) {
            warnings.push(w);
        }
    }
    let bms = bms.map_err(|err| ImportError::Parse {
        path: source_path.to_path_buf(),
        message: format!("{err:?}"),
    })?;

    let mut intermediate = build_intermediate(&bms, warnings);
    intermediate.identity = identity;
    Ok(intermediate)
}

fn build_intermediate(bms: &Bms, warnings: &mut Vec<ImportWarning>) -> IntermediateChart {
    let metadata = build_metadata(bms);
    let mut resources = build_resources(bms);
    let mut objects = Vec::new();

    push_note_objects(bms, &mut objects, warnings);
    push_bgm_objects(bms, &mut objects);
    push_bga_objects(bms, &mut objects);
    push_bpm_change_objects(bms, &mut objects);
    push_stop_objects(bms, &mut objects, &mut resources);

    let max_measure = compute_max_measure(bms, &objects);
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

    intermediate.metadata.key_mode =
        KeyMode::detect_from_lanes(intermediate.objects.iter().filter_map(|o| match o.kind {
            IntermediateObjectKind::VisibleNote { lane, .. }
            | IntermediateObjectKind::InvisibleNote { lane, .. }
            | IntermediateObjectKind::LongChannelNote { lane, .. }
            | IntermediateObjectKind::MineNote { lane, .. } => Some(lane),
            _ => None,
        }));

    intermediate
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
        has_bga: false,
        key_mode: KeyMode::default(),
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
    IntermediateResources { wavs, bmps, bpm_table, stop_table }
}

fn push_note_objects(
    bms: &Bms,
    objects: &mut Vec<IntermediateObject>,
    warnings: &mut Vec<ImportWarning>,
) {
    // `playables()` は Visible / Long のみを返すため Invisible / Landmine が抜け落ちる。
    // BGM チャネルは KeyLayoutBeat にマップできないので、`from_channel_id` が None なら
    // 通常ノーツ扱いせずに飛ばす（BGM は `push_bgm_objects` で別途処理する）。
    for note in bms.notes().all_notes() {
        let Some(mapping) = KeyLayoutBeat::from_channel_id(note.channel_id) else {
            continue;
        };
        let Some(lane) = map_lane(mapping.side(), mapping.key()) else {
            warnings.push(ImportWarning::UnsupportedChannel { channel: note.channel_id.as_u16() });
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
            BmsNoteKind::Landmine => IntermediateObjectKind::MineNote {
                lane,
                wav_key: None, // Mine の wav_id は damage 値であり鳴音 key ではない
                damage: wav_id,
            },
        };
        objects.push(IntermediateObject {
            measure: track_of(note.offset),
            position_num: note.offset.numerator() as u32,
            position_den: note.offset.denominator().get() as u32,
            kind,
        });
    }
}

fn push_bgm_objects(bms: &Bms, objects: &mut Vec<IntermediateObject>) {
    for note in bms.notes().bgms::<KeyLayoutBeat>() {
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
            BgaLayer::Overlay | BgaLayer::Overlay2 => IntermediateBgaKind::Layer,
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
    // チャネル 03: 1byte 16進数で BPM を直接指定。
    for (time, bpm) in &bms.bpm.bpm_changes_u8 {
        objects.push(IntermediateObject {
            measure: track_of(*time),
            position_num: time.numerator() as u32,
            position_den: time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBpm { bpm: *bpm as f64 },
        });
    }
    // チャネル 08: `#BPMxx` 定義を参照。
    for change in bms.bpm.bpm_changes.values() {
        objects.push(IntermediateObject {
            measure: track_of(change.time),
            position_num: change.time.numerator() as u32,
            position_den: change.time.denominator().get() as u32,
            kind: IntermediateObjectKind::SetBpm { bpm: change.bpm.get() },
        });
    }
}

fn push_stop_objects(
    bms: &Bms,
    objects: &mut Vec<IntermediateObject>,
    resources: &mut IntermediateResources,
) {
    // bms-rs は `#xxx09` を解決済みの StopObj として展開してくれるため、各 StopObj に
    // synthetic key を割り当てて StopDef を生やしておく。`build_resources` が拾った
    // `#STOPxx` 定義との key 衝突を避けるため、既存最大値+1 から採番する。
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
    time.track().0 as u32
}

fn compute_max_measure(bms: &Bms, objects: &[IntermediateObject]) -> u32 {
    let mut max = objects.iter().map(|o| o.measure).max().unwrap_or(0);
    if let Some(last) = bms.last_obj_time() {
        max = max.max(track_of(last));
    }
    for &track in bms.section_len.section_len_changes.keys() {
        max = max.max(track.0 as u32);
    }
    max
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

fn map_lane(side: PlayerSide, key: Key) -> Option<Lane> {
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

fn map_bms_warning(w: &BmsWarning) -> Option<ImportWarning> {
    // 細かい variant までは Phase 5 で整理する。ここでは汎用 UnsupportedCommand に
    // まとめ、ロード自体は継続させる。
    Some(ImportWarning::UnsupportedCommand { command: format!("{w:?}") })
}
