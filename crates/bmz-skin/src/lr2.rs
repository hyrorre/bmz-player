use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use encoding_rs::SHIFT_JIS;
use serde_json::{Value as JsonValue, json};

use crate::{LoadedLuaSkinValue, SkinLoadWarning};

const LR2_OFFSET_LIFT: i32 = 3;
const LR2_OFFSET_JUDGE_1P: i32 = 32;

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
    fadeout: i32,
    close: i32,
    loadstart: i32,
    loadend: i32,
    playstart: i32,
    finishmargin: i32,
    options: Vec<CustomOption>,
    files: Vec<CustomFile>,
    offsets: Vec<CustomOffset>,
    selected_ops: HashMap<i32, bool>,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            skin_type: 0,
            name: String::new(),
            author: String::new(),
            w: 1280,
            h: 720,
            fadeout: 0,
            close: 0,
            loadstart: 0,
            loadend: 0,
            playstart: 0,
            finishmargin: 0,
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
    LnActive,
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
    values: Vec<JsonValue>,
    texts: Vec<JsonValue>,
    sliders: Vec<JsonValue>,
    graphs: Vec<JsonValue>,
    hidden_covers: Vec<JsonValue>,
    gauge: Option<JsonValue>,
    gauges: Vec<JsonValue>,
    note: NoteState,
    judges: Vec<JudgeState>,
    bga: Option<JsonValue>,
    destinations: Vec<JsonValue>,
    current: Option<CurrentObject>,
    conditional_ops: Vec<i32>,
    lr2_gauge_id: Option<String>,
    lr2_gauge_add_x: i32,
    lr2_gauge_add_y: i32,
    current_has_destination: bool,
    note_marker_inserted: bool,
    next_id: usize,
    remap_single_play_2p_lanes: bool,
}

#[derive(Default)]
struct NoteState {
    note: Vec<String>,
    lnstart: Vec<String>,
    lnend: Vec<String>,
    lnbody: Vec<String>,
    lnactive: Vec<String>,
    hcnstart: Vec<String>,
    hcnend: Vec<String>,
    hcnbody: Vec<String>,
    hcnactive: Vec<String>,
    hcndamage: Vec<String>,
    hcnreactive: Vec<String>,
    mine: Vec<String>,
    size: Vec<i32>,
    dst: Vec<JsonValue>,
    group: Vec<JsonValue>,
}

#[derive(Default, Clone)]
struct JudgeState {
    images: Vec<JsonValue>,
    numbers: Vec<JsonValue>,
    shift: bool,
    marker_inserted: bool,
}

pub fn load_lr2_csv_skin_value(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    let mut header = load_header(path, options)?;
    apply_default_play_header_items(&mut header);
    let mut builder = CsvBuilder::new(path, header, files);
    let lines = read_csv_lines(path)?;
    let mut processor = Processor::new(builder.header.selected_ops.clone());
    processor.process_lines(&lines, path, &mut builder)?;
    let warnings = builder.warnings.clone();
    Ok(LoadedLuaSkinValue { value: builder.finish(), warnings, files: BTreeMap::new() })
}

fn load_header(path: &Path, options: &BTreeMap<String, String>) -> Result<Header> {
    let mut header = Header::default();
    let lines = read_csv_lines(path)?;
    let mut processor = Processor::new(HashMap::new());
    for line in &lines {
        if !processor.should_execute(line) {
            continue;
        }
        match line.command.as_str() {
            "RESOLUTION" => match parse_i32(line.fields.get(1)) {
                1 => {
                    header.w = 1280;
                    header.h = 720;
                }
                2 => {
                    header.w = 1920;
                    header.h = 1080;
                }
                3 => {
                    header.w = 3840;
                    header.h = 2160;
                }
                _ => {
                    header.w = 640;
                    header.h = 480;
                }
            },
            "INFORMATION" => {
                header.skin_type = parse_i32(line.fields.get(1));
                header.name = field(line, 2).to_string();
                header.author = field(line, 3).to_string();
            }
            "FADEOUT" => header.fadeout = parse_i32(line.fields.get(1)),
            "CLOSE" => header.close = parse_i32(line.fields.get(1)),
            "LOADSTART" => header.loadstart = parse_i32(line.fields.get(1)),
            "LOADEND" => header.loadend = parse_i32(line.fields.get(1)),
            "PLAYSTART" => header.playstart = parse_i32(line.fields.get(1)),
            "FINISHMARGIN" => header.finishmargin = parse_i32(line.fields.get(1)),
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
    apply_derived_play_options(&mut header);
    Ok(header)
}

fn apply_default_play_header_items(header: &mut Header) {
    if !matches!(header.skin_type, 0 | 1 | 2 | 3 | 4 | 12 | 13) {
        return;
    }
    add_builtin_option(header, "BGA Size", 30, &["Normal", "Extend"]);
    add_builtin_option(header, "Ghost", 34, &["Off", "Type A", "Type B", "Type C"]);
    let has_score_graph_option = header.options.iter().any(|option| option.name == "Score Graph");
    add_builtin_option(header, "Score Graph", 38, &["Off", "On"]);
    if !has_score_graph_option {
        header.selected_ops.insert(38, false);
        header.selected_ops.insert(39, true);
    }
    add_builtin_option(header, "Judge Detail", 1997, &["Off", "EARLY/LATE", "+-ms"]);
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

impl<'a> CsvBuilder<'a> {
    fn new(path: &'a Path, header: Header, files: &'a BTreeMap<String, String>) -> Self {
        let skin_root = infer_skin_root(path);
        let skin_file_dir = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let remap_single_play_2p_lanes = matches!(header.skin_type, 0 | 1 | 3 | 4 | 12 | 13)
            && header.selected_ops.get(&901).copied().unwrap_or(false);
        Self {
            skin_root,
            skin_file_dir,
            skin_file_dir_name: path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .map(str::to_string),
            header,
            files,
            warnings: Vec::new(),
            sources: Vec::new(),
            source_paths: Vec::new(),
            fonts: Vec::new(),
            lr2font_ids: Vec::new(),
            images: Vec::new(),
            values: Vec::new(),
            texts: Vec::new(),
            sliders: Vec::new(),
            graphs: Vec::new(),
            hidden_covers: Vec::new(),
            gauge: None,
            gauges: Vec::new(),
            note: NoteState::default(),
            judges: Vec::new(),
            bga: None,
            destinations: Vec::new(),
            current: None,
            conditional_ops: Vec::new(),
            lr2_gauge_id: None,
            lr2_gauge_add_x: 0,
            lr2_gauge_add_y: 0,
            current_has_destination: false,
            note_marker_inserted: false,
            next_id: 0,
            remap_single_play_2p_lanes,
        }
    }

    fn execute(&mut self, line: &CsvLine) -> Result<()> {
        match line.command.as_str() {
            "IMAGE" => self.add_source(field(line, 1)),
            "FONT" => self.add_system_font(line),
            "LR2FONT" => self.add_lr2_font(field(line, 1)),
            "SRC_IMAGE" | "SRC_BUTTON" => self.add_image(line),
            "DST_IMAGE" | "DST_BUTTON" => self.add_destination(line),
            "SRC_NUMBER" => self.add_number(line),
            "DST_NUMBER" => self.add_destination(line),
            "SRC_TEXT" => self.add_text(line),
            "DST_TEXT" => self.add_destination(line),
            "SRC_SLIDER" => self.add_slider(line, false),
            "SRC_SLIDER_REFNUMBER" => self.add_slider(line, true),
            "DST_SLIDER" => self.add_destination(line),
            "SRC_BARGRAPH" => self.add_graph(line, false),
            "SRC_BARGRAPH_REFNUMBER" => self.add_graph(line, true),
            "DST_BARGRAPH" => self.add_destination(line),
            "SRC_GROOVEGAUGE" | "SRC_GROOVEGAUGE_EX" => self.add_gauge(line),
            "DST_GROOVEGAUGE" => self.add_destination(line),
            "SRC_LINE" => self.add_image(line),
            "DST_LINE" => self.add_note_group_destination(line, &[LR2_OFFSET_LIFT]),
            "SRC_JUDGELINE" => self.add_image(line),
            "DST_JUDGELINE" => self.add_destination_with_default_offsets(line, &[LR2_OFFSET_LIFT]),
            "SRC_BGA" => self.add_bga(),
            "DST_BGA" => self.add_destination(line),
            "SRC_NOTE" | "SRC_AUTO_NOTE" => self.add_note_source(line, NoteSlot::Note),
            "SRC_LN_START" | "SRC_AUTO_LN_START" => self.add_note_source(line, NoteSlot::LnStart),
            "SRC_LN_END" | "SRC_AUTO_LN_END" => self.add_note_source(line, NoteSlot::LnEnd),
            "SRC_LN_BODY" | "SRC_AUTO_LN_BODY" => {
                self.add_note_source(line, NoteSlot::LnBody);
                self.add_note_source(line, NoteSlot::LnActive);
            }
            "SRC_LN_BODY_INACTIVE" => self.add_note_source(line, NoteSlot::LnBody),
            "SRC_LN_BODY_ACTIVE" => self.add_note_source(line, NoteSlot::LnActive),
            "SRC_HCN_START" => self.add_note_source(line, NoteSlot::HcnStart),
            "SRC_HCN_END" => self.add_note_source(line, NoteSlot::HcnEnd),
            "SRC_HCN_BODY" => {
                self.add_note_source(line, NoteSlot::HcnBody);
                self.add_note_source(line, NoteSlot::HcnActive);
            }
            "SRC_HCN_BODY_INACTIVE" => self.add_note_source(line, NoteSlot::HcnBody),
            "SRC_HCN_BODY_ACTIVE" => self.add_note_source(line, NoteSlot::HcnActive),
            "SRC_HCN_DAMAGE" => self.add_note_source(line, NoteSlot::HcnDamage),
            "SRC_HCN_REACTIVE" => self.add_note_source(line, NoteSlot::HcnReactive),
            "SRC_MINE" | "SRC_AUTO_MINE" => self.add_note_source(line, NoteSlot::Mine),
            "DST_NOTE" => self.add_note_destination(line),
            "SRC_NOWJUDGE_1P" => self.add_judge_image(line, 0),
            "DST_NOWJUDGE_1P" => self.add_judge_image_destination(line, 0),
            "SRC_NOWJUDGE_2P" => self.add_judge_image(line, 1),
            "DST_NOWJUDGE_2P" => self.add_judge_image_destination(line, 1),
            "SRC_NOWCOMBO_1P" => self.add_judge_number(line, 0),
            "DST_NOWCOMBO_1P" => self.add_judge_number_destination(line, 0),
            "SRC_NOWCOMBO_2P" => self.add_judge_number(line, 1),
            "DST_NOWCOMBO_2P" => self.add_judge_number_destination(line, 1),
            "SRC_HIDDEN" => self.add_hidden_cover(line),
            "DST_HIDDEN" => self.add_destination(line),
            "SRC_LIFT" => self.add_lift_cover(line),
            "DST_LIFT" => self.add_destination(line),
            "STARTINPUT" => {}
            "FADEOUT" | "CLOSE" | "LOADSTART" | "LOADEND" | "PLAYSTART" | "FINISHMARGIN" => {}
            "TRANSCLOLR" | "SCRATCHSIDE" | "ENDOFHEADER" | "STRETCH" => {}
            other if other.starts_with("DST_") || other.starts_with("SRC_") => {
                self.warn(format!("unsupported lr2 csv command: #{other}"));
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_play_header_command(&mut self, line: &CsvLine) {
        match line.command.as_str() {
            "FADEOUT" => self.header.fadeout = parse_i32(line.fields.get(1)),
            "CLOSE" => self.header.close = parse_i32(line.fields.get(1)),
            "LOADSTART" => self.header.loadstart = parse_i32(line.fields.get(1)),
            "LOADEND" => self.header.loadend = parse_i32(line.fields.get(1)),
            "PLAYSTART" => self.header.playstart = parse_i32(line.fields.get(1)),
            "FINISHMARGIN" => self.header.finishmargin = parse_i32(line.fields.get(1)),
            _ => {}
        }
    }

    fn add_source(&mut self, raw_path: &str) {
        let path = self.resolve_source_path(raw_path);
        let id = format!("{}", self.source_paths.len());
        self.sources.push(json!({ "id": id, "path": path }));
        self.source_paths.push(Some(path));
    }

    fn ensure_reference_source(&mut self, source_index: i32) {
        let path = match source_index {
            // beatoraja SkinProperty.IMAGE_BACKBMP / IMAGE_BLACK / IMAGE_WHITE.
            101 => "bmz://lr2/backbmp",
            110 => "bmz://lr2/black",
            111 => "bmz://lr2/white",
            _ => return,
        };
        let id = source_index.to_string();
        if !self.sources.iter().any(|source| {
            source.get("id").and_then(JsonValue::as_str).is_some_and(|existing| existing == id)
        }) {
            self.sources.push(json!({ "id": id, "path": path }));
        }
    }

    fn add_system_font(&mut self, line: &CsvLine) {
        let _ = line;
    }

    fn add_lr2_font(&mut self, raw_path: &str) {
        let path = self.resolve_lr2_font_path(raw_path);
        let id = format!("lr2font-{}", self.fonts.len());
        self.fonts.push(json!({ "id": id, "path": path, "type": 1 }));
        self.lr2font_ids.push(Some(id));
    }

    fn add_image(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-image");
        let mut image = json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
        });
        if line.command == "SRC_BUTTON" {
            image["act"] = json!(values[11]);
            image["click"] = json!(values[12]);
        }
        self.images.push(image);
        self.set_current(id);
    }

    fn add_number(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-number");
        self.values.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "ref": values[11],
            "align": values[12],
            "digit": values[13],
            "zeropadding": values[15],
        }));
        self.set_current(id);
    }

    fn add_text(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let id = self.alloc_id("lr2-text");
        let font = self
            .lr2font_ids
            .get(values[2].max(0) as usize)
            .and_then(|id| id.clone())
            .unwrap_or_default();
        self.texts.push(json!({
            "id": id,
            "font": font,
            "ref": values[3],
            "align": values[4],
            "size": self.lr2_text_size(values[2]),
        }));
        self.set_current(id);
    }

    fn add_slider(&mut self, line: &CsvLine, is_ref_num: bool) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-slider");
        self.sliders.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "angle": values[11],
            "range": values[12],
            "type": values[13],
            "changeable": values[14] == 0,
            "isRefNum": is_ref_num,
            "min": values[15],
            "max": values[16],
        }));
        self.set_current(id);
    }

    fn add_graph(&mut self, line: &CsvLine, is_ref_num: bool) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-graph");
        let graph_type = if is_ref_num { values[11] } else { values[11] + 100 };
        self.graphs.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "type": graph_type,
            "angle": values[12],
            "isRefNum": is_ref_num,
            "min": values[13],
            "max": values[14],
        }));
        self.set_current(id);
    }

    fn add_gauge(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-gauge");
        let source_cells = (region.divx * region.divy).max(1);
        let cell_ids = (0..source_cells)
            .map(|index| {
                let image_id = format!("{id}-cell-{index}");
                let divx = region.divx.max(1);
                let divy = region.divy.max(1);
                let cell_w = (region.w / divx).max(1);
                let cell_h = (region.h / divy).max(1);
                self.images.push(json!({
                    "id": image_id,
                    "src": region.src,
                    "x": region.x + cell_w * (index % divx),
                    "y": region.y + cell_h * (index / divx),
                    "w": cell_w,
                    "h": cell_h,
                    "divx": 1,
                    "divy": 1,
                    "cycle": region.cycle,
                    "timer": region.timer,
                }));
                image_id
            })
            .collect::<Vec<_>>();
        let nodes = lr2_gauge_nodes(&cell_ids, values[14], line.command == "SRC_GROOVEGAUGE_EX");
        self.lr2_gauge_id = Some(id.clone());
        self.lr2_gauge_add_x = values[11];
        self.lr2_gauge_add_y = values[12];
        let gauge = json!({
            "id": id,
            "nodes": nodes,
            "parts": if values[13] == 0 { 50 } else { values[13] },
            "type": values[14],
            "range": values[15],
            "cycle": values[16],
            "starttime": values[17],
            "endtime": values[18],
        });
        if self.gauge.is_none() {
            self.gauge = Some(gauge.clone());
        }
        self.gauges.push(gauge);
        self.set_current(id);
    }

    fn add_bga(&mut self) {
        let id = "bga".to_string();
        self.bga = Some(json!({ "id": id }));
        self.set_current(id);
    }

    fn add_note_source(&mut self, line: &CsvLine, slot: NoteSlot) {
        let values = parse_values(line);
        let Some(lane) = self.lr2_lane_to_beatoraja_index(values[1]) else {
            return;
        };
        let Some(region) = self.source_region(&values) else {
            return;
        };
        let id = self.alloc_id("lr2-note");
        self.images.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
        }));
        set_lane_note_value_if_empty(note_vec_mut(&mut self.note, slot), lane, id);
    }

    fn add_note_destination(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(lane) = self.lr2_lane_to_beatoraja_index(values[1]) else {
            return;
        };
        if !self.note_marker_inserted {
            self.destinations.push(json!({ "id": "notes" }));
            self.note_marker_inserted = true;
        }
        while self.note.dst.len() < lane as usize {
            self.note.dst.push(json!({ "time": 0, "x": 0, "y": 0, "w": 0, "h": 0 }));
        }
        if self.note.dst.len() == lane as usize {
            let frame = note_destination_frame(&values, self.header.h as i32);
            set_lane_note_size_if_empty(&mut self.note.size, lane, values[6].abs());
            self.note.dst.push(frame);
        } else if is_empty_note_frame(&self.note.dst[lane as usize]) {
            let frame = note_destination_frame(&values, self.header.h as i32);
            set_lane_note_size_if_empty(&mut self.note.size, lane, values[6].abs());
            self.note.dst[lane as usize] = frame;
        }
    }

    fn add_note_group_destination(&mut self, line: &CsvLine, default_offsets: &[i32]) {
        let Some(current) = self.current.clone() else {
            return;
        };
        let values = parse_values(line);
        for variant in current.variants {
            let ops = self.combined_conditional_ops(&variant);
            let destination = self.destination_def_with_default_offsets(
                &variant.id,
                &values,
                &ops,
                default_offsets,
            );
            push_destination(&mut self.note.group, destination);
        }
    }

    fn lr2_lane_to_beatoraja_index(&self, lane: i32) -> Option<i32> {
        let mapped = lr2_lane_to_beatoraja_index(lane)?;
        if self.remap_single_play_2p_lanes && mapped >= 8 { Some(mapped - 8) } else { Some(mapped) }
    }

    fn add_judge_image(&mut self, line: &CsvLine, index: usize) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            return;
        };
        let id = self.alloc_id("lr2-judge-image");
        self.images.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
        }));
        self.ensure_judge(index);
        if !self.judges[index].marker_inserted {
            self.destinations.push(json!({ "id": format!("judge-{index}") }));
            self.judges[index].marker_inserted = true;
        }
        self.judges[index].shift = values[11] != 1;
        set_judge_slot(
            &mut self.judges[index].images,
            lr2_judge_slot(values[1]),
            json!({ "id": id, "dst": [] }),
        );
        self.set_current(id);
    }

    fn add_judge_image_destination(&mut self, line: &CsvLine, index: usize) {
        let Some(current) = self.current.clone() else {
            return;
        };
        self.ensure_judge(index);
        let values = parse_values(line);
        for variant in current.variants {
            let ops = self.combined_conditional_ops(&variant);
            let dst = self.destination_def_with_default_offsets(
                &variant.id,
                &values,
                &ops,
                &[LR2_OFFSET_JUDGE_1P, LR2_OFFSET_LIFT],
            );
            if let Some(entry) = self.judges[index].images.iter_mut().rev().find(|entry| {
                entry.get("id").and_then(JsonValue::as_str) == Some(variant.id.as_str())
            }) {
                merge_destination_entry(entry, dst);
            }
        }
    }

    fn add_judge_number(&mut self, line: &CsvLine, index: usize) {
        let values = parse_values(line);
        self.add_number(line);
        if let Some(variant) = self.current_primary_variant() {
            self.ensure_judge(index);
            set_judge_slot(
                &mut self.judges[index].numbers,
                lr2_judge_slot(values[1]),
                json!({ "id": variant.id, "dst": [] }),
            );
        }
    }

    fn add_judge_number_destination(&mut self, line: &CsvLine, index: usize) {
        let Some(current) = self.current.clone() else {
            return;
        };
        self.ensure_judge(index);
        let values = parse_values(line);
        for variant in current.variants {
            let ops = self.combined_conditional_ops(&variant);
            let dst = judge_combo_destination_def(
                &variant.id,
                &values,
                &ops,
                &[LR2_OFFSET_JUDGE_1P, LR2_OFFSET_LIFT],
            );
            if let Some(entry) = self.judges[index].numbers.iter_mut().rev().find(|entry| {
                entry.get("id").and_then(JsonValue::as_str) == Some(variant.id.as_str())
            }) {
                merge_destination_entry(entry, dst);
            }
        }
    }

    fn add_hidden_cover(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-hidden");
        self.hidden_covers.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "disapearLine": lr2_disappear_line(values[11], self.header.h as i32),
            "isDisapearLineLinkLift": lr2_hidden_link_lift(line, &values),
        }));
        self.set_current(id);
    }

    fn add_lift_cover(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-liftcover");
        self.hidden_covers.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "disapearLine": lr2_disappear_line(values[11], self.header.h as i32),
            "isDisapearLineLinkLift": lr2_hidden_link_lift(line, &values),
        }));
        self.set_current(id);
    }

    fn add_destination(&mut self, line: &CsvLine) {
        self.add_destination_with_default_offsets(line, &[]);
    }

    fn add_destination_with_default_offsets(&mut self, line: &CsvLine, default_offsets: &[i32]) {
        let Some(current) = self.current.clone() else {
            return;
        };
        let values = parse_values(line);
        for variant in current.variants {
            let ops = self.combined_conditional_ops(&variant);
            let dst = if self.lr2_gauge_id.as_deref() == Some(variant.id.as_str()) {
                gauge_destination_def(
                    &variant.id,
                    &values,
                    self.header.h as i32,
                    self.lr2_gauge_add_x,
                    self.lr2_gauge_add_y,
                    &ops,
                )
            } else if variant.id.contains("lr2-liftcover") {
                self.destination_def_with_default_offsets(
                    &variant.id,
                    &values,
                    &ops,
                    &[LR2_OFFSET_LIFT],
                )
            } else if !default_offsets.is_empty() {
                self.destination_def_with_default_offsets(
                    &variant.id,
                    &values,
                    &ops,
                    default_offsets,
                )
            } else {
                self.destination_def_with_ops(&variant.id, &values, &ops)
            };
            if self.current_has_destination {
                merge_or_push_current_destination(&mut self.destinations, dst);
            } else {
                self.destinations.push(dst);
                self.current_has_destination = true;
            }
        }
    }

    fn set_current(&mut self, id: String) {
        let variant = CurrentObjectVariant { id, conditional_ops: self.conditional_ops.clone() };
        if !variant.conditional_ops.is_empty()
            && !self.current_has_destination
            && let Some(current) = &mut self.current
        {
            current.variants.push(variant);
            return;
        }

        self.current = Some(CurrentObject { variants: vec![variant] });
        self.current_has_destination = false;
    }

    fn current_primary_variant(&self) -> Option<CurrentObjectVariant> {
        self.current.as_ref().and_then(|current| current.variants.first().cloned())
    }

    fn combined_conditional_ops(&self, variant: &CurrentObjectVariant) -> Vec<i32> {
        let mut ops = variant.conditional_ops.clone();
        ops.extend(self.conditional_ops.iter().copied());
        ops
    }

    fn destination_def_with_ops(
        &self,
        id: &str,
        values: &[i32; 22],
        conditional_ops: &[i32],
    ) -> JsonValue {
        destination_def_with_default_offsets(id, values, self.header.h as i32, conditional_ops, &[])
    }

    fn destination_def_with_default_offsets(
        &self,
        id: &str,
        values: &[i32; 22],
        conditional_ops: &[i32],
        default_offsets: &[i32],
    ) -> JsonValue {
        destination_def_with_default_offsets(
            id,
            values,
            self.header.h as i32,
            conditional_ops,
            default_offsets,
        )
    }

    fn ensure_judge(&mut self, index: usize) {
        while self.judges.len() <= index {
            self.judges.push(JudgeState::default());
        }
    }

    fn source_region(&mut self, values: &[i32; 22]) -> Option<SourceRegion> {
        let source_index = values[2];
        if source_index < 0 {
            return None;
        }
        let source_id = source_index.to_string();
        if matches!(source_index, 101 | 110 | 111) {
            self.ensure_reference_source(source_index);
            return Some(SourceRegion {
                src: source_id,
                x: values[3],
                y: values[4],
                w: values[5],
                h: values[6],
                divx: values[7].max(1),
                divy: values[8].max(1),
                cycle: values[9],
                timer: (values[10] != 0).then_some(values[10]),
            });
        }
        if source_index as usize >= self.source_paths.len() {
            self.warn(format!("lr2 csv source index {source_index} is not defined"));
            return None;
        }
        Some(SourceRegion {
            src: source_id,
            x: values[3],
            y: values[4],
            w: values[5],
            h: values[6],
            divx: values[7].max(1),
            divy: values[8].max(1),
            cycle: values[9],
            timer: (values[10] != 0).then_some(values[10]),
        })
    }

    fn resolve_source_path(&self, raw_path: &str) -> String {
        let normalized = self.relative_source_path(&normalize_lr2_asset_path(raw_path));
        if let Some(file) = self.header.files.iter().find(|file| file.path == normalized)
            && let Some(selected) =
                self.files.get(&file.name).filter(|selected| !selected.is_empty())
            && self.selected_skin_file_exists(selected)
        {
            return selected.replace('\\', "/");
        }
        if let Some(file) =
            self.header.files.iter().find(|file| same_wildcard_prefix(&file.path, &normalized))
        {
            if let Some(selected) =
                self.files.get(&file.name).filter(|selected| !selected.is_empty())
                && selected_wildcard_value(&file.path, selected).is_some()
                && self.selected_skin_file_exists(selected)
            {
                return substitute_wildcard(&normalized, &file.path, selected);
            }
            if !file.default.is_empty() {
                return substitute_wildcard_default(&normalized, &file.path, &file.default);
            }
        }
        normalized
    }

    fn resolve_lr2_font_path(&self, raw_path: &str) -> String {
        let path = self.resolve_source_path(raw_path);
        if !path.to_ascii_lowercase().ends_with(".lr2font") {
            return path;
        }
        let fnt = format!("{}fnt", &path[..path.len() - "lr2font".len()]);
        if self.skin_root.join(&fnt).is_file() || self.skin_file_dir.join(&fnt).is_file() {
            return self.relative_font_path_for_skin_file(&fnt);
        }
        path
    }

    fn relative_font_path_for_skin_file(&self, path: &str) -> String {
        if self.skin_file_dir.join(path).is_file() {
            return path.to_string();
        }
        let parent_relative = format!("../{path}");
        if self.skin_file_dir.join(&parent_relative).is_file() {
            return parent_relative;
        }
        path.to_string()
    }

    fn lr2_text_size(&self, font_index: i32) -> i32 {
        if let Some(Some(font_id)) = self.lr2font_ids.get(font_index.max(0) as usize)
            && self.fonts.iter().any(|font| {
                font.get("id").and_then(JsonValue::as_str) == Some(font_id.as_str())
                    && font
                        .get("path")
                        .and_then(JsonValue::as_str)
                        .is_some_and(|path| path.to_ascii_lowercase().ends_with(".fnt"))
            })
        {
            return 0;
        }
        self.fonts
            .get(font_index.max(0) as usize)
            .and_then(|font| font.get("size"))
            .and_then(JsonValue::as_i64)
            .and_then(|size| i32::try_from(size).ok())
            .filter(|size| *size > 0)
            .unwrap_or(48)
    }

    fn selected_skin_file_exists(&self, selected: &str) -> bool {
        use std::path::Component;

        let selected = selected.replace('\\', "/");
        let relative = Path::new(&selected);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return false;
        }
        self.skin_file_dir.join(relative).is_file()
    }

    fn relative_source_path(&self, normalized: &str) -> String {
        if let Some(dir_name) = &self.skin_file_dir_name
            && let Some(stripped) = normalized.strip_prefix(&format!("{dir_name}/"))
        {
            return stripped.to_string();
        }
        normalized.to_string()
    }

    fn alloc_id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn warn(&mut self, message: String) {
        self.warnings.push(SkinLoadWarning { message });
    }

    fn finish(self) -> JsonValue {
        let category = json!([{ "name": "LR2", "item": ["property", "filepath", "offset"] }]);
        let property = self
            .header
            .options
            .iter()
            .map(|option| {
                let items = option
                    .items
                    .iter()
                    .enumerate()
                    .map(|(index, name)| json!({ "name": name, "op": option.base + index as i32 }))
                    .collect::<Vec<_>>();
                let default_item = option
                    .items
                    .iter()
                    .enumerate()
                    .find(|(index, _)| {
                        self.header
                            .selected_ops
                            .get(&(option.base + *index as i32))
                            .copied()
                            .unwrap_or(false)
                    })
                    .map(|(_, item)| item.clone())
                    .or_else(|| option.items.first().cloned())
                    .unwrap_or_default();
                json!({
                    "category": "LR2",
                    "name": option.name,
                    "item": items,
                    "def": default_item,
                })
            })
            .collect::<Vec<_>>();
        let filepath = self
            .header
            .files
            .iter()
            .map(|file| {
                json!({
                    "category": "LR2",
                    "name": file.name,
                    "path": file.path,
                    "def": file.default,
                })
            })
            .collect::<Vec<_>>();
        let offset = self
            .header
            .offsets
            .iter()
            .map(|offset| {
                json!({
                    "category": "LR2",
                    "name": offset.name,
                    "id": offset.id,
                    "x": offset.flags[0],
                    "y": offset.flags[1],
                    "w": offset.flags[2],
                    "h": offset.flags[3],
                    "r": offset.flags[4],
                    "a": offset.flags[5],
                })
            })
            .collect::<Vec<_>>();
        let note = (!self.note.note.is_empty() || !self.note.dst.is_empty()).then(|| {
            json!({
                "id": "notes",
                "note": self.note.note,
                "lnstart": self.note.lnstart,
                "lnend": self.note.lnend,
                "lnbody": self.note.lnbody,
                "lnactive": self.note.lnactive,
                "hcnstart": self.note.hcnstart,
                "hcnend": self.note.hcnend,
                "hcnbody": self.note.hcnbody,
                "hcnactive": self.note.hcnactive,
                "hcndamage": self.note.hcndamage,
                "hcnreactive": self.note.hcnreactive,
                "mine": self.note.mine,
                "size": self.note.size,
                "dst": self.note.dst,
                "group": self.note.group,
            })
        });
        let judge = self
            .judges
            .into_iter()
            .enumerate()
            .map(|(index, judge)| {
                json!({
                    "id": format!("judge-{index}"),
                    "index": index as i32,
                    "images": judge.images,
                    "numbers": judge.numbers,
                    "shift": judge.shift,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "type": self.header.skin_type,
            "name": self.header.name,
            "author": self.header.author,
            "w": self.header.w,
            "h": self.header.h,
            "fadeout": self.header.fadeout,
            "close": self.header.close,
            "loadstart": self.header.loadstart,
            "loadend": self.header.loadend,
            "playstart": self.header.playstart,
            "finishmargin": self.header.finishmargin,
            "category": category,
            "property": property,
            "filepath": filepath,
            "offset": offset,
            "source": self.sources,
            "font": self.fonts,
            "image": self.images,
            "value": self.values,
            "text": self.texts,
            "slider": self.sliders,
            "graph": self.graphs,
            "hiddenCover": self.hidden_covers,
            "gauge": self.gauge,
            "gauges": self.gauges,
            "note": note,
            "judge": judge,
            "bga": self.bga,
            "destination": self.destinations,
        })
    }
}

struct Processor {
    ops: HashMap<i32, bool>,
    stack: Vec<IfState>,
}

#[derive(Debug, Clone)]
struct IfState {
    parent_active: bool,
    branch_taken: bool,
    active: bool,
    runtime_ops: Vec<i32>,
}

impl Processor {
    fn new(ops: HashMap<i32, bool>) -> Self {
        Self { ops, stack: Vec::new() }
    }

    fn process_lines(
        &mut self,
        lines: &[CsvLine],
        current_path: &Path,
        builder: &mut CsvBuilder,
    ) -> Result<()> {
        for line in lines {
            if self.handle_control(line) {
                continue;
            }
            if !self.active() {
                continue;
            }
            if line.command == "SETOPTION" {
                let index = parse_i32(line.fields.get(1));
                let value = parse_i32(line.fields.get(2)) >= 1;
                if self.active_runtime_ops().is_empty() {
                    self.ops.insert(index, value);
                    builder.header.selected_ops.insert(index, value);
                }
                continue;
            }
            builder.conditional_ops = self.active_runtime_ops();
            builder.apply_play_header_command(line);
            if line.command == "INCLUDE" {
                let include = resolve_include_path(builder, current_path, field(line, 1));
                if include.is_file() {
                    let include_lines = read_csv_lines(&include)?;
                    self.process_lines(&include_lines, &include, builder)?;
                } else {
                    builder.warn(format!("lr2 include not found: {}", include.display()));
                }
                continue;
            }
            builder.execute(line)?;
        }
        builder.conditional_ops.clear();
        Ok(())
    }

    fn should_execute(&mut self, line: &CsvLine) -> bool {
        if self.handle_control(line) {
            return false;
        }
        self.active()
    }

    fn handle_control(&mut self, line: &CsvLine) -> bool {
        match line.command.as_str() {
            "IF" => {
                let parent_active = self.active();
                let eval = self.eval_if(line);
                let condition = parent_active && eval.matches;
                self.stack.push(IfState {
                    parent_active,
                    branch_taken: condition,
                    active: condition,
                    runtime_ops: if condition { eval.runtime_ops } else { Vec::new() },
                });
                true
            }
            "ELSEIF" => {
                let Some(mut state) = self.stack.pop() else {
                    return true;
                };
                if !state.parent_active || state.branch_taken {
                    state.active = false;
                    state.runtime_ops.clear();
                } else {
                    let eval = self.eval_if(line);
                    state.active = eval.matches;
                    state.branch_taken |= state.active;
                    state.runtime_ops = if state.active { eval.runtime_ops } else { Vec::new() };
                }
                self.stack.push(state);
                true
            }
            "ELSE" => {
                if let Some(state) = self.stack.last_mut() {
                    state.active = state.parent_active && !state.branch_taken;
                    state.branch_taken = true;
                    state.runtime_ops.clear();
                }
                true
            }
            "ENDIF" => {
                self.stack.pop();
                true
            }
            _ => false,
        }
    }

    fn active(&self) -> bool {
        self.stack.iter().all(|state| state.active)
    }

    fn eval_if(&self, line: &CsvLine) -> IfEval {
        let mut runtime_ops = Vec::new();
        let matches =
            line.fields.iter().skip(1).filter(|field| !field.trim().is_empty()).all(|field| {
                let option = parse_option_token(field);
                let option_id = option.abs();
                if is_runtime_lr2_option(option_id) {
                    runtime_ops.push(option);
                    true
                } else if let Some(enabled) = self.ops.get(&option_id).copied() {
                    if option >= 0 { enabled } else { !enabled }
                } else {
                    option < 0
                }
            });
        IfEval { matches, runtime_ops }
    }

    fn active_runtime_ops(&self) -> Vec<i32> {
        self.stack
            .iter()
            .filter(|state| state.active)
            .flat_map(|state| state.runtime_ops.iter().copied())
            .collect()
    }
}

struct IfEval {
    matches: bool,
    runtime_ops: Vec<i32>,
}

fn is_runtime_lr2_option(option: i32) -> bool {
    matches!(option, 32 | 33 | 150..=155)
}

fn lr2_option_text_matches(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn lr2_disappear_line(value: i32, canvas_h: i32) -> i32 {
    if value > 0 { canvas_h.saturating_sub(value) } else { -1 }
}

fn lr2_hidden_link_lift(line: &CsvLine, values: &[i32; 22]) -> bool {
    field(line, 12).is_empty() || values[12] != 0
}

fn lr2_judge_slot(value: i32) -> usize {
    if value <= 5 { (5 - value).max(0) as usize } else { value as usize }
}

fn set_judge_slot(slots: &mut Vec<JsonValue>, index: usize, value: JsonValue) {
    if slots.len() <= index {
        slots.resize_with(index + 1, || json!({ "id": "", "dst": [] }));
    }
    slots[index] = value;
}

fn destination_def_with_ops(
    id: &str,
    values: &[i32; 22],
    canvas_h: i32,
    conditional_ops: &[i32],
) -> JsonValue {
    destination_def_with_default_offsets(id, values, canvas_h, conditional_ops, &[])
}

fn destination_def_with_default_offsets(
    id: &str,
    values: &[i32; 22],
    canvas_h: i32,
    conditional_ops: &[i32],
    default_offsets: &[i32],
) -> JsonValue {
    let frame = destination_frame(values, canvas_h);
    let mut op = conditional_ops.to_vec();
    op.extend(values[18..=20].iter().copied().filter(|value| *value != 0));
    normalize_lr2_destination_ops(&mut op);
    let mut destination = json!({
        "id": id,
        "blend": values[12],
        "filter": values[13],
        "timer": if values[17] != 0 { json!(values[17]) } else { JsonValue::Null },
        "loop": values[16],
        "center": values[15],
        "offset": values[21],
        "op": op,
        "dst": [frame],
    });
    if values[21] == 0 && !default_offsets.is_empty() {
        destination["offsets"] = json!(default_offsets);
    }
    destination
}

fn normalize_lr2_destination_ops(op: &mut Vec<i32>) {
    if op.contains(&39) {
        op.retain(|value| !matches!(*value, 32 | -33));
    }
}

fn gauge_destination_def(
    id: &str,
    values: &[i32; 22],
    canvas_h: i32,
    add_x: i32,
    add_y: i32,
    conditional_ops: &[i32],
) -> JsonValue {
    let mut values = *values;
    if add_x.abs() >= 1 {
        values[5] = add_x * 50;
    }
    if add_y.abs() >= 1 {
        values[6] = add_y * 50;
    }
    destination_def_with_ops(id, &values, canvas_h, conditional_ops)
}

fn judge_combo_destination_def(
    id: &str,
    values: &[i32; 22],
    conditional_ops: &[i32],
    default_offsets: &[i32],
) -> JsonValue {
    let mut op = conditional_ops.to_vec();
    op.extend(values[18..=20].iter().copied().filter(|value| *value != 0));
    let mut destination = json!({
        "id": id,
        "blend": values[12],
        "filter": values[13],
        "timer": if values[17] != 0 { json!(values[17]) } else { JsonValue::Null },
        "loop": values[16],
        "center": values[15],
        "offset": values[21],
        "op": op,
        "dst": [{
            "time": values[2],
            "x": values[3],
            "y": -values[4],
            "w": values[5],
            "h": values[6],
            "acc": values[7],
            "a": values[8],
            "r": values[9],
            "g": values[10],
            "b": values[11],
            "angle": values[14],
        }],
    });
    if values[21] == 0 && !default_offsets.is_empty() {
        destination["offsets"] = json!(default_offsets);
    }
    destination
}

fn lr2_gauge_nodes(cell_ids: &[String], animation_type: i32, is_ex: bool) -> Vec<String> {
    let mut nodes = vec![cell_ids.first().cloned().unwrap_or_default(); 36];
    let cells_per_frame = if is_ex {
        if animation_type == 3 && cell_ids.len().is_multiple_of(12) { 12 } else { 8 }
    } else if animation_type == 3 && cell_ids.len().is_multiple_of(6) {
        6
    } else {
        4
    };
    let frame_cells = cells_per_frame.min(cell_ids.len().max(1));
    for (dy, cell_id) in cell_ids.iter().take(frame_cells).enumerate() {
        for slot in lr2_gauge_slots(dy, animation_type, is_ex, cells_per_frame) {
            if let Some(node) = nodes.get_mut(slot) {
                *node = cell_id.clone();
            }
        }
    }
    nodes
}

fn lr2_gauge_slots(
    dy: usize,
    animation_type: i32,
    is_ex: bool,
    cells_per_frame: usize,
) -> Vec<usize> {
    if !is_ex {
        if animation_type == 3 && cells_per_frame == 6 {
            return (0..6).map(|group| dy + group * 6).collect();
        }
        let mut slots = (0..6).map(|group| dy + group * 6).collect::<Vec<_>>();
        if dy < 2 {
            slots.extend((0..6).map(|group| dy + 4 + group * 6));
        }
        return slots;
    }

    if animation_type == 3 && cells_per_frame == 12 {
        return match dy {
            0..=3 => (0..4).map(|group| dy + group * 6).collect(),
            4..=7 => vec![dy + 20, dy + 26],
            8 | 9 => vec![dy - 4, dy + 2, dy + 8, dy + 14],
            _ => vec![dy + 18, dy + 24],
        };
    }

    if dy < 4 {
        let mut slots = (0..4).map(|group| dy + group * 6).collect::<Vec<_>>();
        if dy < 2 {
            slots.extend((0..4).map(|group| dy + 4 + group * 6));
        }
        slots
    } else {
        let mut slots = vec![dy + 20, dy + 26];
        if dy < 6 {
            slots.extend([dy + 24, dy + 30]);
        }
        slots
    }
}

fn push_destination(destinations: &mut Vec<JsonValue>, destination: JsonValue) {
    if let Some(previous) = destinations.last_mut()
        && merge_destination_entry(previous, destination.clone())
    {
        return;
    }
    destinations.push(destination);
}

fn merge_or_push_current_destination(destinations: &mut Vec<JsonValue>, destination: JsonValue) {
    let Some(next_id) = destination.get("id").and_then(JsonValue::as_str) else {
        destinations.push(destination);
        return;
    };
    if let Some(previous) = destinations
        .iter_mut()
        .rev()
        .find(|previous| previous.get("id").and_then(JsonValue::as_str) == Some(next_id))
        && merge_destination_entry(previous, destination.clone())
    {
        return;
    }
    destinations.push(destination);
}

fn merge_destination_entry(previous: &mut JsonValue, destination: JsonValue) -> bool {
    let Some(previous_id) = previous.get("id").and_then(JsonValue::as_str) else {
        return false;
    };
    let Some(next_id) = destination.get("id").and_then(JsonValue::as_str) else {
        return false;
    };
    if previous_id != next_id {
        return false;
    }

    let Some(next_frames) = destination.get("dst").and_then(JsonValue::as_array) else {
        return false;
    };
    let is_empty_placeholder = previous.as_object().is_some_and(|object| object.len() == 2)
        && previous.get("dst").and_then(JsonValue::as_array).is_some_and(Vec::is_empty);
    if is_empty_placeholder {
        *previous = destination;
        return true;
    }
    let Some(previous_frames) = previous.get_mut("dst").and_then(JsonValue::as_array_mut) else {
        return false;
    };
    previous_frames.extend(next_frames.iter().cloned());
    true
}

fn destination_frame(values: &[i32; 22], canvas_h: i32) -> JsonValue {
    let mut x = values[3];
    let mut y = values[4];
    let mut w = values[5];
    let mut h = values[6];
    if w < 0 {
        x += w;
        w = -w;
    }
    if h < 0 {
        y += h;
        h = -h;
    }
    json!({
        "time": values[2],
        "x": x,
        "y": canvas_h - (y + h),
        "w": w,
        "h": h,
        "acc": values[7],
        "a": values[8],
        "r": values[9],
        "g": values[10],
        "b": values[11],
        "angle": values[14],
    })
}

fn note_destination_frame(values: &[i32; 22], canvas_h: i32) -> JsonValue {
    let x = values[3];
    let y = canvas_h - (values[4] + values[6]);
    let w = values[5].abs();
    let h = (values[4] + values[6]).max(values[6]).max(1);
    json!({
        "time": values[2],
        "x": x,
        "y": y,
        "w": w,
        "h": h,
        "acc": values[7],
        "a": values[8],
        "r": values[9],
        "g": values[10],
        "b": values[11],
        "angle": values[14],
    })
}

fn is_empty_note_frame(value: &JsonValue) -> bool {
    let w = value.get("w").and_then(JsonValue::as_i64).unwrap_or(0);
    let h = value.get("h").and_then(JsonValue::as_i64).unwrap_or(0);
    w == 0 || h == 0
}

fn note_vec_mut(note: &mut NoteState, slot: NoteSlot) -> &mut Vec<String> {
    match slot {
        NoteSlot::Note => &mut note.note,
        NoteSlot::LnStart => &mut note.lnstart,
        NoteSlot::LnEnd => &mut note.lnend,
        NoteSlot::LnBody => &mut note.lnbody,
        NoteSlot::LnActive => &mut note.lnactive,
        NoteSlot::HcnStart => &mut note.hcnstart,
        NoteSlot::HcnEnd => &mut note.hcnend,
        NoteSlot::HcnBody => &mut note.hcnbody,
        NoteSlot::HcnActive => &mut note.hcnactive,
        NoteSlot::HcnDamage => &mut note.hcndamage,
        NoteSlot::HcnReactive => &mut note.hcnreactive,
        NoteSlot::Mine => &mut note.mine,
    }
}

fn set_lane_note_value_if_empty(values: &mut Vec<String>, lane: i32, value: String) {
    let lane = lane as usize;
    if values.len() <= lane {
        values.resize(lane + 1, String::new());
    }
    if values[lane].is_empty() {
        values[lane] = value;
    }
}

fn set_lane_note_size_if_empty(values: &mut Vec<i32>, lane: i32, value: i32) {
    let lane = lane as usize;
    if values.len() <= lane {
        values.resize(lane + 1, 0);
    }
    if values[lane] <= 0 {
        values[lane] = value;
    }
}

fn lr2_lane_to_beatoraja_index(lane: i32) -> Option<i32> {
    match lane {
        0 => Some(7),
        1..=9 => Some(lane - 1),
        10 | 20 => Some(15),
        11..=19 => Some(lane - 3),
        _ => None,
    }
}

fn resolve_include_path(builder: &CsvBuilder<'_>, current_path: &Path, raw: &str) -> PathBuf {
    let normalized = normalize_lr2_asset_path(raw);
    let root_candidate = builder.skin_root.join(&normalized);
    if root_candidate.is_file() {
        return root_candidate;
    }
    current_path.parent().unwrap_or_else(|| Path::new(".")).join(normalized)
}

fn infer_skin_root(path: &Path) -> PathBuf {
    let mut current = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    loop {
        let Some(name) = current.file_name().and_then(|name| name.to_str()) else {
            return path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        };
        if name.eq_ignore_ascii_case("WMII_FHD")
            || current.join("play").is_dir() && current.join("font").is_dir()
        {
            return current;
        }
        let Some(parent) = current.parent() else {
            return path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        };
        current = parent.to_path_buf();
    }
}

fn normalize_lr2_asset_path(path: &str) -> String {
    let mut normalized = path.trim().trim_matches('"').replace('\\', "/");
    if let Some(index) = normalized.find("//") {
        normalized.truncate(index);
    }
    normalized = normalized.trim().to_string();
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    if let Some(stripped) = normalized.strip_prefix("LR2files/Theme/") {
        let mut parts = stripped.splitn(2, '/');
        let _theme = parts.next();
        return parts.next().unwrap_or_default().to_string();
    }
    normalized
}

fn relative_to_skin_file_parent(skin_path: &Path, normalized: &str) -> String {
    if let Some(dir_name) =
        skin_path.parent().and_then(|parent| parent.file_name()).and_then(|name| name.to_str())
        && let Some(stripped) = normalized.strip_prefix(&format!("{dir_name}/"))
    {
        return stripped.to_string();
    }
    normalized.to_string()
}

fn same_wildcard_prefix(a: &str, b: &str) -> bool {
    let Some((a_prefix, _)) = a.split_once('*') else {
        return false;
    };
    let Some((b_prefix, _)) = b.split_once('*') else {
        return false;
    };
    a_prefix == b_prefix
}

fn substitute_wildcard(asset_path: &str, definition: &str, selected: &str) -> String {
    let Some((asset_prefix, asset_suffix)) = asset_path.split_once('*') else {
        return selected.replace('\\', "/");
    };
    let Some(wildcard) = selected_wildcard_value(definition, selected) else {
        return selected.replace('\\', "/");
    };
    format!("{asset_prefix}{wildcard}{asset_suffix}")
}

fn selected_wildcard_value(definition: &str, selected: &str) -> Option<String> {
    let (def_prefix, def_suffix) = definition.split_once('*')?;
    let selected = selected.replace('\\', "/");
    let stripped = selected.strip_prefix(def_prefix)?;
    let wildcard = stripped.strip_suffix(def_suffix)?;
    Some(wildcard.to_string())
}

fn substitute_wildcard_default(asset_path: &str, definition: &str, default: &str) -> String {
    let Some((asset_prefix, asset_suffix)) = asset_path.split_once('*') else {
        return asset_path.to_string();
    };
    if definition.split_once('*').is_none() {
        return asset_path.to_string();
    }
    format!("{asset_prefix}{default}{asset_suffix}")
}

fn parse_values(line: &CsvLine) -> [i32; 22] {
    let mut values = [0; 22];
    for index in 1..values.len().min(line.fields.len()) {
        values[index] = parse_i32(line.fields.get(index));
    }
    values
}

fn parse_i32(value: Option<&String>) -> i32 {
    value.map(|value| parse_i32_str(value)).unwrap_or(0)
}

fn parse_i32_str(value: &str) -> i32 {
    let value = value.trim().replace('!', "-").replace(' ', "");
    value.parse::<i32>().unwrap_or(0)
}

fn parse_option_token(value: &str) -> i32 {
    let cleaned = value
        .trim()
        .replace('!', "-")
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    cleaned.parse::<i32>().unwrap_or(0)
}

fn field(line: &CsvLine, index: usize) -> &str {
    line.fields.get(index).map(|field| field.trim()).unwrap_or_default()
}

fn read_csv_lines(path: &Path) -> Result<Vec<CsvLine>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read lr2 csv skin: {}", path.display()))?;
    let (decoded, _, _) = SHIFT_JIS.decode(&bytes);
    Ok(decoded.lines().filter_map(parse_csv_line).collect())
}

fn parse_csv_line(line: &str) -> Option<CsvLine> {
    let mut fields = split_csv_line(line);
    if fields.is_empty() {
        return None;
    }
    let command = fields[0].trim();
    if !command.starts_with('#') {
        return None;
    }
    let command = command.trim_start_matches('#').trim().to_ascii_uppercase();
    if command.is_empty() {
        return None;
    }
    fields[0] = format!("#{command}");
    Some(CsvLine { command, fields })
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quotes = !in_quotes,
            // `//` starts a trailing comment in LR2 skins; drop the rest of the
            // line so inline comments (e.g. `#IF,38,32 //scoregraph off`) are not
            // parsed as extra fields/conditions.
            '/' if !in_quotes && chars.peek() == Some(&'/') => break,
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("{name}-{nanos}"))
    }

    #[test]
    fn lr2_asset_path_strips_theme_prefix() {
        assert_eq!(
            normalize_lr2_asset_path(r".\LR2files\Theme\WMII_FHD\play\parts\note\*.png"),
            "play/parts/note/*.png"
        );
    }

    #[test]
    fn lr2_destination_converts_top_origin_to_bottom_origin() {
        let mut values = [0; 22];
        values[2] = 100;
        values[3] = 10;
        values[4] = 20;
        values[5] = 30;
        values[6] = 40;
        let frame = destination_frame(&values, 1080);
        assert_eq!(frame["time"], 100);
        assert_eq!(frame["x"], 10);
        assert_eq!(frame["y"], 1020);
        assert_eq!(frame["w"], 30);
        assert_eq!(frame["h"], 40);
    }

    #[test]
    fn lr2_destination_preserves_custom_offset_id() {
        let mut values = [0; 22];
        values[21] = 32;

        let destination = destination_def_with_ops("image", &values, 1080, &[]);

        assert_eq!(destination["offset"], 32);
    }

    #[test]
    fn lr2_dst_line_defaults_to_lift_offset() {
        let files = BTreeMap::new();
        let skin_path = unique_test_dir("bmz-lr2-dst-line").join("play.lr2skin");
        let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
        builder.add_source("line.png");
        builder
            .execute(&parse_csv_line("#SRC_LINE,0,0,0,0,10,1,1,1,0,0").expect("valid SRC_LINE"))
            .unwrap();
        builder
            .execute(
                &parse_csv_line("#DST_LINE,0,0,10,20,40,2,0,255,255,255,255,0,0,0,0,0,0,0,0,0,")
                    .expect("valid DST_LINE"),
            )
            .unwrap();

        let group = builder.note.group.first().expect("DST_LINE should produce note.group");

        assert_eq!(group["offset"], 0);
        assert_eq!(group["offsets"].as_array().unwrap(), &[json!(LR2_OFFSET_LIFT)]);
    }

    #[test]
    fn lr2_nowjudge_indices_match_beatoraja_slots() {
        assert_eq!(lr2_judge_slot(5), 0);
        assert_eq!(lr2_judge_slot(4), 1);
        assert_eq!(lr2_judge_slot(3), 2);
        assert_eq!(lr2_judge_slot(2), 3);
        assert_eq!(lr2_judge_slot(1), 4);
        assert_eq!(lr2_judge_slot(0), 5);
        assert_eq!(lr2_judge_slot(6), 6);
    }

    #[test]
    fn lr2_number_ref_preserves_poor_plus_miss() {
        let files = BTreeMap::new();
        let skin_path = unique_test_dir("bmz-lr2-number-ref").join("play.lr2skin");
        let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
        builder.add_source("numbers.png");
        builder
            .execute(
                &parse_csv_line("#SRC_NUMBER,0,0,0,0,10,20,1,10,0,0,426,0,4,0,1")
                    .expect("valid SRC_NUMBER"),
            )
            .unwrap();

        assert_eq!(builder.values.first().unwrap()["ref"], json!(426));
    }

    #[test]
    fn lr2_customfile_default_replaces_wildcard_once() {
        assert_eq!(
            substitute_wildcard_default("parts/note/*.png", "parts/note/*.png", "photon"),
            "parts/note/photon.png"
        );
    }

    #[test]
    fn lr2_customfile_selection_uses_existing_skin_file() {
        let root = unique_test_dir("bmz-lr2-customfile");
        let play_dir = root.join("play");
        std::fs::create_dir_all(play_dir.join("parts/gauge")).unwrap();
        std::fs::write(play_dir.join("parts/gauge/default.png"), []).unwrap();
        std::fs::write(play_dir.join("parts/gauge/blue.png"), []).unwrap();
        let skin_path = play_dir.join("FHDPLAY_AC.lr2skin");
        std::fs::write(&skin_path, []).unwrap();
        let mut header = Header::default();
        header.files.push(CustomFile {
            name: "GAUGE COLOR".to_string(),
            path: "parts/gauge/*.png".to_string(),
            default: "default".to_string(),
        });
        let files =
            BTreeMap::from([("GAUGE COLOR".to_string(), "parts/gauge/blue.png".to_string())]);
        let builder = CsvBuilder::new(&skin_path, header, &files);

        assert_eq!(
            builder.resolve_source_path(r".\LR2files\Theme\WMII_FHD\play\parts\gauge\*.png"),
            "parts/gauge/blue.png"
        );
    }

    #[test]
    fn lr2_customfile_selection_falls_back_when_saved_file_is_missing() {
        let root = unique_test_dir("bmz-lr2-customfile-missing");
        let play_dir = root.join("play");
        std::fs::create_dir_all(play_dir.join("parts/gauge")).unwrap();
        std::fs::write(play_dir.join("parts/gauge/default.png"), []).unwrap();
        let skin_path = play_dir.join("FHDPLAY_AC.lr2skin");
        std::fs::write(&skin_path, []).unwrap();
        let mut header = Header::default();
        header.files.push(CustomFile {
            name: "GAUGE COLOR".to_string(),
            path: "parts/gauge/*.png".to_string(),
            default: "default".to_string(),
        });
        let files =
            BTreeMap::from([("GAUGE COLOR".to_string(), "parts/gauge/missing.png".to_string())]);
        let builder = CsvBuilder::new(&skin_path, header, &files);

        assert_eq!(
            builder.resolve_source_path(r".\LR2files\Theme\WMII_FHD\play\parts\gauge\*.png"),
            "parts/gauge/default.png"
        );
    }

    #[test]
    fn processor_selects_default_custom_option_branch() {
        let mut ops = HashMap::new();
        ops.insert(900, true);
        ops.insert(901, false);
        let mut processor = Processor::new(ops);
        assert!(!processor.should_execute(&CsvLine {
            command: "IF".into(),
            fields: vec!["#IF".into(), "900".into()],
        }));
        assert!(processor.active());
        assert!(
            !processor.should_execute(&CsvLine {
                command: "ENDIF".into(),
                fields: vec!["#ENDIF".into()],
            })
        );
        assert!(processor.active());
    }

    #[test]
    fn processor_keeps_outer_false_branch_inactive_inside_true_nested_if() {
        let mut ops = HashMap::new();
        ops.insert(900, false);
        ops.insert(901, true);
        let mut processor = Processor::new(ops);
        assert!(!processor.should_execute(&CsvLine {
            command: "IF".into(),
            fields: vec!["#IF".into(), "900".into()],
        }));
        assert!(!processor.active());
        assert!(!processor.should_execute(&CsvLine {
            command: "IF".into(),
            fields: vec!["#IF".into(), "901".into()],
        }));
        assert!(!processor.active());
        assert!(
            !processor.should_execute(&CsvLine {
                command: "ENDIF".into(),
                fields: vec!["#ENDIF".into()],
            })
        );
        assert!(!processor.active());
        assert!(
            !processor.should_execute(&CsvLine {
                command: "ENDIF".into(),
                fields: vec!["#ENDIF".into()],
            })
        );
        assert!(processor.active());
    }

    #[test]
    fn processor_keeps_autoplay_conditions_as_runtime_ops() {
        let ops = HashMap::from([(32, true), (33, false)]);
        let mut processor = Processor::new(ops);
        assert!(!processor.should_execute(&CsvLine {
            command: "IF".into(),
            fields: vec!["#IF".into(), "33".into()],
        }));

        assert!(processor.active());
        assert_eq!(processor.active_runtime_ops(), vec![33]);
    }

    #[test]
    fn processor_does_not_leak_setoption_inside_runtime_if() {
        let path = Path::new("skin/play/test.lr2skin");
        let files = BTreeMap::new();
        let mut builder = CsvBuilder::new(path, Header::default(), &files);
        let lines = [
            parse_csv_line("#IF,33").unwrap(),
            parse_csv_line("#SETOPTION,985,1").unwrap(),
            parse_csv_line("#ENDIF").unwrap(),
        ];
        let mut processor = Processor::new(HashMap::new());

        processor.process_lines(&lines, path, &mut builder).unwrap();

        assert!(!processor.ops.contains_key(&985));
        assert!(!builder.header.selected_ops.contains_key(&985));
    }

    #[test]
    fn processor_attaches_autoplay_runtime_op_to_destination() {
        let path = Path::new("skin/play/test.lr2skin");
        let files = BTreeMap::new();
        let mut builder = CsvBuilder::new(path, Header::default(), &files);
        let lines = [
            parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
            parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
            parse_csv_line("#IF,33").unwrap(),
            parse_csv_line("#DST_IMAGE,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0")
                .unwrap(),
            parse_csv_line("#ENDIF").unwrap(),
        ];
        let mut processor = Processor::new(HashMap::new());

        processor.process_lines(&lines, path, &mut builder).unwrap();

        let op = builder.destinations[0]["op"].as_array().unwrap();
        assert_eq!(op, &[json!(33)]);
    }

    #[test]
    fn processor_drops_autoplay_off_from_score_graph_destinations() {
        let path = Path::new("skin/play/test.lr2skin");
        let files = BTreeMap::new();
        let mut builder = CsvBuilder::new(path, Header::default(), &files);
        let lines = [
            parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
            parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
            parse_csv_line("#IF,32").unwrap(),
            parse_csv_line("#DST_IMAGE,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,39,0,0")
                .unwrap(),
            parse_csv_line("#ENDIF").unwrap(),
        ];
        let mut processor = Processor::new(HashMap::new());

        processor.process_lines(&lines, path, &mut builder).unwrap();

        let op = builder.destinations[0]["op"].as_array().unwrap();
        assert_eq!(op, &[json!(39)]);
    }

    #[test]
    fn consecutive_lr2_destinations_merge_into_keyframes() {
        let path = Path::new("skin/play/test.lr2skin");
        let files = BTreeMap::new();
        let mut builder = CsvBuilder::new(path, Header::default(), &files);
        builder
            .execute(&CsvLine {
                command: "IMAGE".into(),
                fields: vec!["#IMAGE".into(), "parts/frame.png".into()],
            })
            .unwrap();
        builder
            .execute(&CsvLine {
                command: "SRC_IMAGE".into(),
                fields: vec![
                    "#SRC_IMAGE".into(),
                    "0".into(),
                    "0".into(),
                    "0".into(),
                    "0".into(),
                    "10".into(),
                    "20".into(),
                    "1".into(),
                    "1".into(),
                    "0".into(),
                    "0".into(),
                ],
            })
            .unwrap();
        builder
            .execute(&CsvLine {
                command: "DST_IMAGE".into(),
                fields: vec![
                    "#DST_IMAGE".into(),
                    "0".into(),
                    "0".into(),
                    "10".into(),
                    "20".into(),
                    "30".into(),
                    "40".into(),
                    "0".into(),
                    "0".into(),
                    "255".into(),
                    "255".into(),
                    "255".into(),
                    "1".into(),
                    "1".into(),
                    "0".into(),
                    "0".into(),
                    "500".into(),
                    "0".into(),
                    "41".into(),
                    "30".into(),
                    "0".into(),
                ],
            })
            .unwrap();
        builder
            .execute(&CsvLine {
                command: "DST_IMAGE".into(),
                fields: vec![
                    "#DST_IMAGE".into(),
                    "0".into(),
                    "500".into(),
                    "10".into(),
                    "20".into(),
                    "30".into(),
                    "40".into(),
                    "0".into(),
                    "255".into(),
                    "255".into(),
                    "255".into(),
                    "255".into(),
                    "1".into(),
                    "1".into(),
                ],
            })
            .unwrap();

        assert_eq!(builder.destinations.len(), 1);
        let frames = builder.destinations[0].get("dst").and_then(JsonValue::as_array).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0]["a"], 0);
        assert_eq!(frames[1]["a"], 255);
        assert_eq!(builder.destinations[0]["loop"], 500);
    }

    #[test]
    fn lr2_note_destination_uses_lane_region_height() {
        let mut values = [0; 22];
        values[2] = 0;
        values[3] = 75;
        values[4] = 704;
        values[5] = 90;
        values[6] = 27;

        let frame = note_destination_frame(&values, 1080);

        assert_eq!(frame["x"], 75);
        assert_eq!(frame["y"], 349);
        assert_eq!(frame["w"], 90);
        assert_eq!(frame["h"], 731);
    }

    #[test]
    fn lr2_gauge_destination_uses_additive_part_span() {
        let mut values = [0; 22];
        values[2] = 1400;
        values[3] = 54;
        values[4] = 897;
        values[5] = 8;
        values[6] = 28;
        values[8] = 255;

        let destination = gauge_destination_def("gauge", &values, 1080, 9, 0, &[]);
        let frame = destination["dst"].as_array().unwrap().first().unwrap();

        assert_eq!(frame["x"], 54);
        assert_eq!(frame["y"], 155);
        assert_eq!(frame["w"], 450);
        assert_eq!(frame["h"], 28);
    }

    #[test]
    fn lr2_gauge_nodes_expand_standard_cells_to_beatoraja_slots() {
        let cells =
            ["red", "green", "back-red", "back-green"].map(|cell| cell.to_string()).to_vec();

        let nodes = lr2_gauge_nodes(&cells, 0, false);

        assert_eq!(nodes.len(), 36);
        assert_eq!(nodes[0], "red");
        assert_eq!(nodes[1], "green");
        assert_eq!(nodes[2], "back-red");
        assert_eq!(nodes[3], "back-green");
        assert_eq!(nodes[4], "red");
        assert_eq!(nodes[5], "green");
        assert_eq!(nodes[18], "red");
        assert_eq!(nodes[24], "red");
        assert_eq!(nodes[34], "red");
        assert_eq!(nodes[35], "green");
    }

    #[test]
    fn wmii_fhd_lr2skin_parse_has_no_unsupported_command_warnings_when_available() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !path.is_file() {
            return;
        }

        let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
        assert!(
            loaded
                .warnings
                .iter()
                .all(|warning| !warning.message.contains("unsupported lr2 csv command")),
            "unexpected warnings: {:?}",
            loaded.warnings
        );
        assert!(
            loaded.warnings.iter().all(|warning| !warning.message.contains("source index 101")
                && !warning.message.contains("source index 110")
                && !warning.message.contains("source index 111")),
            "unexpected reference source warnings: {:?}",
            loaded.warnings
        );
        assert_eq!(loaded.value["name"], "WMII FHD play AC");
        assert!(loaded.value["destination"].as_array().unwrap().len() > 100);
        assert!(!loaded.value["note"]["group"].as_array().unwrap().is_empty());
    }

    #[test]
    fn wmii_fhd_lr2skin_keeps_gauge_sources_separate_when_available() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !path.is_file() {
            return;
        }

        let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
        let gauges = loaded.value["gauges"].as_array().expect("gauges array");

        assert!(gauges.len() >= 4, "expected WMII gauge objects, got {gauges:?}");
        for gauge in gauges.iter().take(4) {
            let nodes = gauge["nodes"].as_array().unwrap();
            assert_eq!(nodes.len(), 36);
        }
        assert_ne!(gauges[0]["id"], gauges[1]["id"]);
    }
}
