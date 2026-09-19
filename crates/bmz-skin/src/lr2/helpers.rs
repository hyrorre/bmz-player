use super::*;

pub(super) struct IfEval {
    pub(super) matches: bool,
    pub(super) runtime_ops: Vec<i32>,
}

pub(super) fn is_runtime_lr2_option(option: i32) -> bool {
    matches!(
        option,
        32 | 33
            | 40..=43
            | 60
            | 61
            | 82
            | 84
            | 150..=155
            | 160..=175
            | 177
            | 180..=191
            | 194
            | 195
            | 230..=273
            | 280..=293
            | 330..=354
            | 624
            | 625
            | 1046
            | 1080
            | 1177
            | 1242
            | 1243
            | 1262
            | 1263
            | 1362
            | 1363
    )
}

pub(super) fn negate_runtime_branches(branches: &[Vec<i32>]) -> Option<Vec<i32>> {
    branches.iter().map(|branch| (branch.len() == 1).then_some(-branch[0])).collect()
}

pub(super) fn lr2_option_text_matches(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

pub(super) fn lr2_disappear_line(value: i32, canvas_h: i32) -> i32 {
    if value > 0 { canvas_h.saturating_sub(value) } else { -1 }
}

pub(super) fn lr2_hidden_link_lift(line: &CsvLine, values: &[i32; 22]) -> bool {
    field(line, 12).is_empty() || values[12] != 0
}

pub(super) fn lr2_judge_slot(value: i32) -> usize {
    if value <= 5 { (5 - value).max(0) as usize } else { value as usize }
}

pub(super) fn set_judge_slot(slots: &mut Vec<JsonValue>, index: usize, value: JsonValue) {
    if slots.len() <= index {
        slots.resize_with(index + 1, || json!({ "id": "", "dst": [] }));
    }
    slots[index] = value;
}

pub(super) fn destination_def_with_default_offsets(
    id: &str,
    values: &[i32; 22],
    canvas_h: i32,
    conditional_ops: &[i32],
    default_offsets: &[i32],
) -> JsonValue {
    let frame = destination_frame(values, canvas_h);
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
        "dst": [frame],
    });
    if values[21] == 0 && !default_offsets.is_empty() {
        destination["offsets"] = json!(default_offsets);
    }
    destination
}

pub(super) fn gauge_destination_def(
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
        if add_x < 0 {
            values[3] -= add_x;
        }
    }
    if add_y.abs() >= 1 {
        values[6] = add_y * 50;
    }
    let frame = gauge_destination_frame(&values, canvas_h);
    let mut op = conditional_ops.to_vec();
    op.extend(values[18..=20].iter().copied().filter(|value| *value != 0));
    json!({
        "id": id,
        "blend": values[12],
        "filter": values[13],
        "timer": if values[17] != 0 { json!(values[17]) } else { JsonValue::Null },
        "loop": values[16],
        "center": values[15],
        "offset": values[21],
        "op": op,
        "dst": [frame],
    })
}

pub(super) fn gauge_destination_frame(values: &[i32; 22], canvas_h: i32) -> JsonValue {
    json!({
        "time": values[2],
        "x": values[3],
        "y": canvas_h - (values[4] + values[6]),
        "w": values[5],
        "h": values[6],
        "acc": values[7],
        "a": values[8],
        "r": values[9],
        "g": values[10],
        "b": values[11],
        "angle": values[14],
    })
}

pub(super) fn judge_combo_destination_def(
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

pub(super) fn judge_detail_destination(
    id: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    timer: i32,
    op: &[i32],
) -> JsonValue {
    json!({
        "id": id,
        "timer": timer,
        "loop": -1,
        "offsets": [33, LR2_OFFSET_LIFT],
        "op": op,
        "dst": [
            { "time": 0, "x": x, "y": y, "w": w, "h": h, "a": 255, "r": 255, "g": 255, "b": 255 },
            { "time": 500, "x": x, "y": y, "w": w, "h": h, "a": 255, "r": 255, "g": 255, "b": 255 },
        ],
    })
}

pub(super) fn lr2_gauge_nodes(
    cell_ids: &[String],
    animation_type: i32,
    is_ex: bool,
) -> Vec<String> {
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

pub(super) fn lr2_gauge_slots(
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

pub(super) fn merge_or_push_current_destination(
    destinations: &mut Vec<JsonValue>,
    destination: JsonValue,
) {
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

pub(super) fn merge_destination_entry(previous: &mut JsonValue, destination: JsonValue) -> bool {
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

pub(super) fn destination_frame(values: &[i32; 22], canvas_h: i32) -> JsonValue {
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

pub(super) fn note_destination_frame(values: &[i32; 22], canvas_h: i32) -> JsonValue {
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

pub(super) fn is_empty_note_frame(value: &JsonValue) -> bool {
    let w = value.get("w").and_then(JsonValue::as_i64).unwrap_or(0);
    let h = value.get("h").and_then(JsonValue::as_i64).unwrap_or(0);
    w == 0 || h == 0
}

pub(super) fn note_vec_mut(note: &mut NoteState, slot: NoteSlot) -> &mut Vec<String> {
    match slot {
        NoteSlot::Note => &mut note.note,
        NoteSlot::LnStart => &mut note.lnstart,
        NoteSlot::LnEnd => &mut note.lnend,
        NoteSlot::LnBody => &mut note.lnbody,
        NoteSlot::LnBodyActive => &mut note.lnbody_active,
        NoteSlot::HcnStart => &mut note.hcnstart,
        NoteSlot::HcnEnd => &mut note.hcnend,
        NoteSlot::HcnBody => &mut note.hcnbody,
        NoteSlot::HcnActive => &mut note.hcnactive,
        NoteSlot::HcnDamage => &mut note.hcndamage,
        NoteSlot::HcnReactive => &mut note.hcnreactive,
        NoteSlot::HlnStart => &mut note.hlnstart,
        NoteSlot::HlnEnd => &mut note.hlnend,
        NoteSlot::HlnBody => &mut note.hlnbody,
        NoteSlot::HlnActive => &mut note.hlnactive,
        NoteSlot::HlnReactive => &mut note.hlnreactive,
        NoteSlot::HlnDamage => &mut note.hlndamage,
        NoteSlot::Mine => &mut note.mine,
    }
}

pub(super) fn set_lane_note_value_if_empty(values: &mut Vec<String>, lane: i32, value: String) {
    let lane = lane as usize;
    if values.len() <= lane {
        values.resize(lane + 1, String::new());
    }
    if values[lane].is_empty() {
        values[lane] = value;
    }
}

pub(super) fn set_lane_note_size_if_empty(values: &mut Vec<i32>, lane: i32, value: i32) {
    let lane = lane as usize;
    if values.len() <= lane {
        values.resize(lane + 1, 0);
    }
    if values[lane] <= 0 {
        values[lane] = value;
    }
}

pub(super) fn lr2_lane_to_beatoraja_index(lane: i32) -> Option<i32> {
    match lane {
        0 => Some(7),
        1..=9 => Some(lane - 1),
        10 | 20 => Some(15),
        11..=19 => Some(lane - 3),
        _ => None,
    }
}

pub(super) fn resolve_include_path(
    builder: &CsvBuilder<'_>,
    current_path: &Path,
    raw: &str,
) -> PathBuf {
    let normalized = normalize_lr2_asset_path(raw);
    let root_candidate = builder.skin_root.join(&normalized);
    if root_candidate.is_file() {
        return root_candidate;
    }
    current_path.parent().unwrap_or_else(|| Path::new(".")).join(normalized)
}

pub(super) fn infer_skin_root(path: &Path) -> PathBuf {
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

pub(super) fn normalize_lr2_asset_path(path: &str) -> String {
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

pub(super) fn relative_to_skin_file_parent(skin_path: &Path, normalized: &str) -> String {
    if let Some(dir_name) =
        skin_path.parent().and_then(|parent| parent.file_name()).and_then(|name| name.to_str())
        && let Some(stripped) = normalized.strip_prefix(&format!("{dir_name}/"))
    {
        return stripped.to_string();
    }
    normalized.to_string()
}

pub(super) fn normalize_selected_skin_file(selected: &str) -> Option<String> {
    use std::path::Component;

    let selected = selected.replace('\\', "/");
    let relative = Path::new(&selected);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return None;
    }
    Some(selected)
}

pub(super) fn same_wildcard_prefix(a: &str, b: &str) -> bool {
    let Some((a_prefix, _)) = a.split_once('*') else {
        return false;
    };
    let Some((b_prefix, _)) = b.split_once('*') else {
        return false;
    };
    a_prefix == b_prefix
}

pub(super) fn substitute_wildcard(asset_path: &str, definition: &str, selected: &str) -> String {
    let Some((asset_prefix, asset_suffix)) = asset_path.split_once('*') else {
        return selected.replace('\\', "/");
    };
    let Some(wildcard) = selected_wildcard_value(definition, selected) else {
        return selected.replace('\\', "/");
    };
    format!("{asset_prefix}{wildcard}{asset_suffix}")
}

pub(super) fn selected_wildcard_value(definition: &str, selected: &str) -> Option<String> {
    let (def_prefix, def_suffix) = definition.split_once('*')?;
    let selected = selected.replace('\\', "/");
    let stripped = selected.strip_prefix(def_prefix)?;
    let wildcard = stripped.strip_suffix(def_suffix)?;
    Some(wildcard.to_string())
}

pub(super) fn substitute_wildcard_default(
    asset_path: &str,
    definition: &str,
    default: &str,
) -> String {
    let Some((asset_prefix, asset_suffix)) = asset_path.split_once('*') else {
        return asset_path.to_string();
    };
    if definition.split_once('*').is_none() {
        return asset_path.to_string();
    }
    format!("{asset_prefix}{default}{asset_suffix}")
}

pub(super) fn parse_values(line: &CsvLine) -> [i32; 22] {
    let mut values = [0; 22];
    for index in 1..values.len().min(line.fields.len()) {
        values[index] = parse_i32(line.fields.get(index));
    }
    values
}

pub(super) fn parse_i32(value: Option<&String>) -> i32 {
    value.map(|value| parse_i32_str(value)).unwrap_or(0)
}

pub(super) fn parse_i32_str(value: &str) -> i32 {
    let value = value.trim().replace('!', "-").replace(' ', "");
    value.parse::<i32>().unwrap_or(0)
}

pub(super) fn parse_option_token(value: &str) -> i32 {
    let cleaned = value
        .trim()
        .replace('!', "-")
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    cleaned.parse::<i32>().unwrap_or(0)
}

pub(super) fn field(line: &CsvLine, index: usize) -> &str {
    line.fields.get(index).map(|field| field.trim()).unwrap_or_default()
}

pub(super) fn read_csv_lines(path: &Path) -> Result<Vec<CsvLine>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read lr2 csv skin: {}", path.display()))?;
    let (decoded, _, _) = SHIFT_JIS.decode(&bytes);
    Ok(decoded.lines().filter_map(parse_csv_line).collect())
}

pub(super) fn parse_csv_line(line: &str) -> Option<CsvLine> {
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

pub(super) fn split_csv_line(line: &str) -> Vec<String> {
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
