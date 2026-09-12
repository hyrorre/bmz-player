use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bmz_skin_document::{
    SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_EARLY_LATE, SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_MS,
    SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_OFF, SKIN_REF_BMZ_LR2_FAST_SLOW_1P, SKIN_REF_BMZ_LR2_GAUGE_2P,
    SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P, SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P, SKIN_REF_BMZ_LR2_HISPEED,
};
use encoding_rs::SHIFT_JIS;
use serde_json::{Value as JsonValue, json};

use crate::{LoadedLuaSkinValue, SkinLoadDependencies, SkinLoadWarning, SkinLoadedFileDependency};

mod builder_assets;
mod builder_core;
mod builder_destination;
mod builder_play;
mod helpers;
mod processor;

use helpers::*;
use processor::Processor;

const LR2_OFFSET_LIFT: i32 = 3;
const LR2_OFFSET_JUDGE_1P: i32 = 32;
const LR2_REFERENCE_IMAGES: [i32; 5] = [100, 101, 102, 110, 111];

#[derive(Debug, Clone)]
struct CsvLine {
    command: String,
    fields: Vec<String>,
}

#[derive(Debug, Clone)]
struct CustomOption {
    name: String,
    base: i32,
    items: Vec<String>,
}

#[derive(Debug, Clone)]
struct CustomFile {
    name: String,
    path: String,
    default: String,
}

#[derive(Debug, Clone)]
struct CustomOffset {
    name: String,
    id: i32,
    flags: [bool; 6],
}

#[derive(Debug, Clone)]
struct Header {
    skin_type: i32,
    name: String,
    author: String,
    w: u32,
    h: u32,
    explicit_resolution_dimensions: bool,
    fadeout: i32,
    input: i32,
    scene: i32,
    close: i32,
    loadstart: i32,
    loadend: i32,
    playstart: i32,
    judgetimer: i32,
    finishmargin: i32,
    builtin_options: [bool; 4],
    options: Vec<CustomOption>,
    files: Vec<CustomFile>,
    offsets: Vec<CustomOffset>,
    selected_ops: HashMap<i32, bool>,
}

struct LoadedHeader {
    header: Header,
    dependencies: SkinLoadDependencies,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            skin_type: 0,
            name: String::new(),
            author: String::new(),
            w: 1280,
            h: 720,
            explicit_resolution_dimensions: false,
            fadeout: 0,
            input: 0,
            scene: 0,
            close: 0,
            loadstart: 0,
            loadend: 0,
            playstart: 0,
            judgetimer: 1,
            finishmargin: 0,
            builtin_options: [true; 4],
            options: Vec::new(),
            files: Vec::new(),
            offsets: Vec::new(),
            selected_ops: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct SourceRegion {
    src: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    divx: i32,
    divy: i32,
    cycle: i32,
    timer: Option<i32>,
}

#[derive(Debug, Clone)]
struct CurrentObject {
    variants: Vec<CurrentObjectVariant>,
}

#[derive(Debug, Clone)]
struct CurrentObjectVariant {
    id: String,
    conditional_ops: Vec<i32>,
}

#[derive(Debug, Clone, Copy)]
enum NoteSlot {
    Note,
    LnStart,
    LnEnd,
    LnBody,
    LnBodyActive,
    HcnStart,
    HcnEnd,
    HcnBody,
    HcnActive,
    HcnDamage,
    HcnReactive,
    Mine,
}

struct CsvBuilder<'a> {
    skin_root: PathBuf,
    skin_file_dir: PathBuf,
    skin_file_dir_name: Option<String>,
    header: Header,
    files: &'a BTreeMap<String, String>,
    warnings: Vec<SkinLoadWarning>,
    sources: Vec<JsonValue>,
    source_paths: Vec<Option<String>>,
    fonts: Vec<JsonValue>,
    lr2font_ids: Vec<Option<String>>,
    images: Vec<JsonValue>,
    imagesets: Vec<JsonValue>,
    lr2_imagesets: Vec<Vec<SourceRegion>>,
    values: Vec<JsonValue>,
    texts: Vec<JsonValue>,
    sliders: Vec<JsonValue>,
    graphs: Vec<JsonValue>,
    judge_graphs: Vec<JsonValue>,
    bpm_graphs: Vec<JsonValue>,
    timing_visualizers: Vec<JsonValue>,
    special_destination_sizes: HashMap<String, (i32, i32)>,
    hidden_covers: Vec<JsonValue>,
    gauge: Option<JsonValue>,
    gauges: Vec<JsonValue>,
    note: NoteState,
    judges: Vec<JudgeState>,
    bga: Option<JsonValue>,
    destinations: Vec<JsonValue>,
    current: Option<CurrentObject>,
    conditional_ops: Vec<i32>,
    runtime_option_aliases: HashMap<i32, Vec<i32>>,
    stretch: i32,
    lr2_gauge_id: Option<String>,
    lr2_gauge_add_x: i32,
    lr2_gauge_add_y: i32,
    current_has_destination: bool,
    note_marker_inserted: bool,
    next_id: usize,
    remap_single_play_2p_lanes: bool,
    file_dependencies: BTreeSet<String>,
    loaded_file_dependencies: BTreeMap<PathBuf, SkinLoadedFileDependency>,
}

#[derive(Default)]
struct NoteState {
    note: Vec<String>,
    lnstart: Vec<String>,
    lnend: Vec<String>,
    lnbody: Vec<String>,
    lnbody_active: Vec<String>,
    hcnstart: Vec<String>,
    hcnend: Vec<String>,
    hcnbody: Vec<String>,
    hcnactive: Vec<String>,
    hcndamage: Vec<String>,
    hcnreactive: Vec<String>,
    mine: Vec<String>,
    size: Vec<i32>,
    dst: Vec<JsonValue>,
    dst2: Option<i32>,
    expansion_rate: Option<[i32; 2]>,
    line_sources: Vec<Option<String>>,
    line_destinations: Vec<Option<JsonValue>>,
    group: Vec<JsonValue>,
    bpm: Vec<JsonValue>,
    stop: Vec<JsonValue>,
    time: Vec<JsonValue>,
}

#[derive(Default, Clone)]
struct JudgeState {
    images: Vec<JsonValue>,
    numbers: Vec<JsonValue>,
    shift: bool,
    marker_inserted: bool,
    detail_inserted: bool,
}

pub fn load_lr2_csv_skin_value(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    let LoadedHeader { mut header, mut dependencies } = load_header(path, options)?;
    apply_default_play_header_items(&mut header);
    apply_selected_header_options(&mut header, options);
    let mut builder = CsvBuilder::new(path, header, files);
    let lines = read_csv_lines(path)?;
    let mut processor = Processor::new(builder.header.selected_ops.clone());
    processor.process_lines(&lines, path, &mut builder)?;
    dependencies.option_values.extend(processor.option_dependencies);
    dependencies.option_values.extend(builder.load_time_option_dependencies());
    dependencies.files.extend(builder.file_dependencies.iter().cloned());
    dependencies.loaded_files.extend(builder.loaded_file_dependencies.clone());
    let warnings = builder.warnings.clone();
    let internal_enabled_options = builder.internal_enabled_options();
    Ok(LoadedLuaSkinValue {
        value: builder.finish(),
        lua_runtime: None,
        runtime_callback_paths: Vec::new(),
        runtime_draw_paths: Vec::new(),
        warnings,
        files: BTreeMap::new(),
        dependencies,
        internal_enabled_options,
    })
}

pub fn load_lr2_csv_skin_dependency_option_values(
    path: &Path,
    options: &BTreeMap<String, String>,
    option_ids: impl IntoIterator<Item = i32>,
) -> Result<BTreeMap<i32, bool>> {
    let LoadedHeader { mut header, .. } = load_header(path, options)?;
    apply_default_play_header_items(&mut header);
    apply_selected_header_options(&mut header, options);
    Ok(option_ids
        .into_iter()
        .map(|option_id| {
            let option_id = option_id.abs();
            (option_id, header.selected_ops.get(&option_id).copied().unwrap_or(false))
        })
        .collect())
}

fn load_header(path: &Path, options: &BTreeMap<String, String>) -> Result<LoadedHeader> {
    let mut header = Header::default();
    let lines = read_csv_lines(path)?;
    let mut processor = Processor::new(HashMap::new());
    for line in &lines {
        if !processor.should_execute(line) {
            continue;
        }
        match line.command.as_str() {
            "RESOLUTION" => {
                (header.w, header.h) = lr2_resolution(line);
                header.explicit_resolution_dimensions =
                    lr2_resolution_has_explicit_dimensions(line);
            }
            "INFORMATION" => {
                header.skin_type = parse_i32(line.fields.get(1));
                header.name = field(line, 2).to_string();
                header.author = field(line, 3).to_string();
            }
            "FADEOUT" => header.fadeout = parse_i32(line.fields.get(1)),
            "STARTINPUT" => header.input = parse_i32(line.fields.get(1)),
            "SCENETIME" => header.scene = parse_i32(line.fields.get(1)),
            "CLOSE" => header.close = parse_i32(line.fields.get(1)),
            "LOADSTART" => header.loadstart = parse_i32(line.fields.get(1)),
            "LOADEND" => header.loadend = parse_i32(line.fields.get(1)),
            "PLAYSTART" => header.playstart = parse_i32(line.fields.get(1)),
            "JUDGETIMER" => header.judgetimer = parse_i32(line.fields.get(1)),
            "FINISHMARGIN" => header.finishmargin = parse_i32(line.fields.get(1)),
            "CUSTOMOPTION_ADDITION_SETTING" => {
                for (index, enabled) in header.builtin_options.iter_mut().enumerate() {
                    if let Some(value) = line.fields.get(index + 1) {
                        *enabled = parse_i32(Some(value)) != 0;
                    }
                }
            }
            "CUSTOMOPTION" => {
                let name = field(line, 1).to_string();
                let base = parse_i32(line.fields.get(2));
                let items = line
                    .fields
                    .iter()
                    .skip(3)
                    .map(|item| item.trim())
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                if !name.is_empty() && !items.is_empty() {
                    header.options.push(CustomOption { name, base, items });
                }
            }
            "CUSTOMFILE" => {
                let name = field(line, 1).to_string();
                let path =
                    relative_to_skin_file_parent(path, &normalize_lr2_asset_path(field(line, 2)));
                let default = field(line, 3).to_string();
                if !name.is_empty() && !path.is_empty() {
                    header.files.push(CustomFile { name, path, default });
                }
            }
            "CUSTOMOFFSET" => {
                let mut flags = [true; 6];
                for (index, flag) in flags.iter_mut().enumerate() {
                    if line.fields.len() > index + 3 {
                        *flag = parse_i32(line.fields.get(index + 3)) > 0;
                    }
                }
                header.offsets.push(CustomOffset {
                    name: field(line, 1).to_string(),
                    id: parse_i32(line.fields.get(2)),
                    flags,
                });
            }
            _ => {}
        }
    }

    apply_selected_header_options(&mut header, options);
    apply_derived_play_options(&mut header);
    let dependencies = SkinLoadDependencies {
        number_values: BTreeMap::new(),
        text_values: BTreeMap::new(),
        option_values: processor.option_dependencies,
        event_index_values: BTreeMap::new(),
        offset_values: BTreeMap::new(),
        offset_id_values: BTreeMap::new(),
        files: BTreeSet::new(),
        loaded_files: BTreeMap::new(),
        virtual_io_files: BTreeMap::new(),
        opaque: false,
    };
    Ok(LoadedHeader { header, dependencies })
}

fn lr2_resolution(line: &CsvLine) -> (u32, u32) {
    let width_or_preset = parse_i32(line.fields.get(1));
    let height = parse_i32(line.fields.get(2));
    if width_or_preset > 0 && height > 0 {
        return (width_or_preset as u32, height as u32);
    }

    match width_or_preset {
        1 => (1280, 720),
        2 => (1920, 1080),
        3 => (3840, 2160),
        _ => (640, 480),
    }
}

fn lr2_resolution_has_explicit_dimensions(line: &CsvLine) -> bool {
    parse_i32(line.fields.get(1)) > 0 && parse_i32(line.fields.get(2)) > 0
}

fn apply_selected_header_options(header: &mut Header, options: &BTreeMap<String, String>) {
    for option in &header.options {
        let selected_index = options
            .iter()
            .find(|(name, _)| lr2_option_text_matches(name, &option.name))
            .map(|(_, selected)| selected)
            .and_then(|selected| {
                option.items.iter().position(|item| lr2_option_text_matches(item, selected))
            })
            .unwrap_or(0);
        for (index, _) in option.items.iter().enumerate() {
            header.selected_ops.insert(option.base + index as i32, index == selected_index);
        }
    }
}

fn apply_default_play_header_items(header: &mut Header) {
    if !matches!(header.skin_type, 0 | 1 | 2 | 3 | 4 | 12 | 13) {
        return;
    }
    if header.builtin_options[0] {
        add_builtin_option(header, "BGA Size", 30, &["Normal", "Extend"]);
    }
    if header.builtin_options[1] {
        add_builtin_option(header, "Ghost", 34, &["Off", "Type A", "Type B", "Type C"]);
    }
    if header.builtin_options[2] {
        add_builtin_option(header, "Score Graph", 38, &["Off", "On"]);
    }
    if header.builtin_options[3] {
        add_builtin_option(
            header,
            "Judge Detail",
            SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_OFF,
            &["Off", "EARLY/LATE", "+-ms"],
        );
    }
    add_builtin_offset(header, "All offset(%)", 1, [true, true, true, true, false, false]);
    add_builtin_offset(header, "Notes offset", 31, [false, false, false, true, false, false]);
    add_builtin_offset(header, "Judge offset", 32, [true, true, true, true, false, true]);
    add_builtin_offset(header, "Judge Detail offset", 33, [true, true, true, true, false, true]);
}

fn apply_derived_play_options(header: &mut Header) {
    if !matches!(header.skin_type, 0 | 1 | 2 | 3 | 4 | 12 | 13) {
        return;
    }

    for op in [160, 161, 162, 163, 164] {
        header.selected_ops.entry(op).or_insert(false);
    }
    let mode_op = match header.skin_type {
        0 | 12 | 13 => Some(160),
        1 => Some(161),
        2 => Some(162),
        3 => Some(163),
        4 => Some(164),
        _ => None,
    };
    if let Some(op) = mode_op {
        header.selected_ops.insert(op, true);
    }

    if header.selected_ops.get(&981).copied().unwrap_or(false) {
        header.selected_ops.entry(965).or_insert(true);
        header.selected_ops.entry(966).or_insert(false);
    }
}

fn add_builtin_option(header: &mut Header, name: &str, base: i32, items: &[&str]) {
    if header.options.iter().any(|option| option.name == name) {
        return;
    }
    header.options.push(CustomOption {
        name: name.to_string(),
        base,
        items: items.iter().map(|item| item.to_string()).collect(),
    });
    header.selected_ops.entry(base).or_insert(true);
    for index in 1..items.len() {
        header.selected_ops.entry(base + index as i32).or_insert(false);
    }
}

fn add_builtin_offset(header: &mut Header, name: &str, id: i32, flags: [bool; 6]) {
    if header.offsets.iter().any(|offset| offset.id == id) {
        return;
    }
    header.offsets.push(CustomOffset { name: name.to_string(), id, flags });
}

#[cfg(test)]
#[path = "lr2/tests/mod.rs"]
mod tests;
