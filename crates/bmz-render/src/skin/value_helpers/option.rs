use super::*;

pub(super) fn best_score_option_index(ref_id: i32, state: &SkinDrawState) -> i64 {
    state
        .skin_attempt
        .best_score_options
        .and_then(|options| options.index(ref_id))
        .map_or(-1, |index| index as i64)
}

pub fn select_arrange_index(arrange: &str) -> usize {
    match arrange {
        "MIRROR" => 1,
        "RANDOM" | "F-RANDOM" | "MF-RANDOM" => 2,
        "R-RANDOM" => 3,
        "S-RANDOM" => 4,
        "SPIRAL" => 5,
        "H-RANDOM" => 6,
        "ALL-SCR" => 7,
        "RANDOM-EX" => 8,
        "S-RANDOM-EX" => 9,
        _ => 0,
    }
}

pub fn extended_arrange_index(arrange: &str) -> usize {
    match arrange {
        "F-RANDOM" => 10,
        "MF-RANDOM" => 11,
        _ => select_arrange_index(arrange),
    }
}

pub fn select_double_option_index(double_option: &str) -> usize {
    match double_option {
        "FLIP" => 1,
        "BATTLE" => 2,
        "BATTLE AS" => 3,
        _ => 0,
    }
}

pub fn select_hs_fix_index(hs_fix: &str) -> usize {
    match hs_fix {
        "START BPM" => 1,
        "MAX BPM" => 2,
        "MAIN BPM" => 3,
        "MIN BPM" => 4,
        _ => 0,
    }
}

pub(crate) fn random_lane_refs(
    pattern: &[u8],
    key_mode: KeyMode,
) -> [u8; SKIN_RANDOM_LANE_REF_COUNT] {
    let mut refs = [0; SKIN_RANDOM_LANE_REF_COUNT];
    if pattern.is_empty() {
        return refs;
    }

    let mut p1_slot = 0;
    let mut p2_slot = 0;
    for &lane in key_mode.active_lanes() {
        if is_p1_random_key_lane(key_mode, lane) {
            if p1_slot < 9 {
                refs[p1_slot] = random_lane_display_value(pattern, lane, key_mode, false);
            }
            p1_slot += 1;
        } else if is_p2_random_key_lane(lane) {
            if p2_slot < 9 {
                refs[10 + p2_slot] = random_lane_display_value(pattern, lane, key_mode, true);
            }
            p2_slot += 1;
        }
    }

    if key_mode.active_lanes().contains(&Lane::Scratch) {
        refs[9] = random_lane_display_value(pattern, Lane::Scratch, key_mode, false);
    }
    if key_mode.active_lanes().contains(&Lane::Scratch2) {
        refs[19] = random_lane_display_value(pattern, Lane::Scratch2, key_mode, true);
    }

    refs
}

pub(crate) fn fixed_random_lane_refs(
    pattern: &[u8],
    key_mode: KeyMode,
    arrange: &str,
    arrange_2p: &str,
) -> [u8; SKIN_RANDOM_LANE_REF_COUNT] {
    let mut refs = random_lane_refs(pattern, key_mode);
    let arrange_index = select_arrange_index(arrange);
    let arrange_2p_index = select_arrange_index(arrange_2p);
    for (slot, value) in refs.iter_mut().enumerate() {
        let side_arrange_index = if slot < 10 { arrange_index } else { arrange_2p_index };
        let scratch_ref = matches!(slot, 9 | 19);
        let displayable_arrange = if scratch_ref {
            side_arrange_index == 8
        } else {
            matches!(side_arrange_index, 2 | 3 | 8)
        };
        if !displayable_arrange {
            *value = 0;
        }
    }
    refs
}

pub(super) fn random_lane_display_value(
    pattern: &[u8],
    display_lane: Lane,
    key_mode: KeyMode,
    is_2p_side: bool,
) -> u8 {
    let Some(source) = pattern.get(display_lane.index()).copied().map(usize::from) else {
        return 0;
    };
    if source >= LANE_COUNT {
        return 0;
    }
    if is_2p_side {
        p2_random_lane_number(source, key_mode)
    } else {
        p1_random_lane_number(source, key_mode)
    }
}

pub(super) fn is_p1_random_key_lane(key_mode: KeyMode, lane: Lane) -> bool {
    matches!(
        lane,
        Lane::Key1 | Lane::Key2 | Lane::Key3 | Lane::Key4 | Lane::Key5 | Lane::Key6 | Lane::Key7
    ) || (key_mode == KeyMode::K9 && matches!(lane, Lane::Key8 | Lane::Key9))
}

pub(super) fn is_p2_random_key_lane(lane: Lane) -> bool {
    matches!(
        lane,
        Lane::Key8
            | Lane::Key9
            | Lane::Key10
            | Lane::Key11
            | Lane::Key12
            | Lane::Key13
            | Lane::Key14
    )
}

pub(super) fn p1_random_lane_number(source: usize, key_mode: KeyMode) -> u8 {
    match Lane::ALL[source] {
        Lane::Scratch => p1_random_side_key_count(key_mode),
        Lane::Key1 => 1,
        Lane::Key2 => 2,
        Lane::Key3 => 3,
        Lane::Key4 => 4,
        Lane::Key5 => 5,
        Lane::Key6 => 6,
        Lane::Key7 => 7,
        Lane::Key8 if key_mode == KeyMode::K9 => 8,
        Lane::Key9 if key_mode == KeyMode::K9 => 9,
        _ => 0,
    }
}

pub(super) fn p2_random_lane_number(source: usize, key_mode: KeyMode) -> u8 {
    match Lane::ALL[source] {
        Lane::Key8 => 1,
        Lane::Key9 => 2,
        Lane::Key10 => 3,
        Lane::Key11 => 4,
        Lane::Key12 => 5,
        Lane::Key13 => 6,
        Lane::Key14 => 7,
        Lane::Scratch2 => p2_random_side_key_count(key_mode),
        _ => 0,
    }
}

pub(super) fn p1_random_side_key_count(key_mode: KeyMode) -> u8 {
    match key_mode {
        KeyMode::K4 => 4,
        KeyMode::K5 => 6,
        KeyMode::K6 => 6,
        KeyMode::K7 | KeyMode::K8 | KeyMode::K14 => 8,
        KeyMode::K9 => 9,
        KeyMode::K10 => 6,
    }
}

pub(super) fn p2_random_side_key_count(key_mode: KeyMode) -> u8 {
    match key_mode {
        KeyMode::K10 => 6,
        KeyMode::K14 => 8,
        _ => 0,
    }
}

pub(super) fn select_gauge_index(gauge: &str) -> usize {
    match gauge {
        "A-EASY" => 0,
        "EASY" => 1,
        "NORMAL" => 2,
        "HARD" => 3,
        "EX-HARD" => 4,
        "HAZARD" => 5,
        _ => 2,
    }
}

pub fn select_gauge_auto_shift_index(mode: &str) -> usize {
    match mode {
        "CONTINUE" => 1,
        "HARD TO GROOVE" => 2,
        "BEST CLEAR" => 3,
        "SELECT TO UNDER" => 4,
        _ => 0,
    }
}

pub fn select_bottom_shiftable_gauge_index(mode: &str) -> usize {
    match mode {
        "EASY" => 1,
        "NORMAL" => 2,
        _ => 0,
    }
}

/// beatoraja の既定 target list と、play skin の target graph (ref 41/77) が
/// 使う 11 段階の画像 index。選曲画面用の BMZ target 列挙順とは別物。
pub(crate) fn play_target_image_index(target: &str) -> usize {
    match target {
        "RANK_A" | "A" => 1,
        "RANK_AA-" => 3,
        "RANK_AA" | "AA" => 4,
        "RANK_AAA-" => 6,
        "RANK_AAA" | "AAA" => 7,
        "RANK_MAX-" => 9,
        "MAX" => 10,
        // BMZ 固有の動的 target は専用 sprite を持たないため、先頭へ
        // fallback する（従来の NONE と同じ扱い）。
        _ => 0,
    }
}

pub(super) fn select_bga_index(bga: &str) -> usize {
    match bga {
        "AUTO" => 1,
        "OFF" => 2,
        _ => 0,
    }
}

pub(super) fn select_assist_index(assist: &str) -> usize {
    match assist {
        "AUTOPLAY" | "AUTOPLAY BATTLE" => 1,
        _ => 0,
    }
}

pub(super) fn select_session_mode_index(assist: &str) -> usize {
    match assist {
        "PRACTICE" => 1,
        "AUTOPLAY" => 2,
        "AUTOPLAY BATTLE" => 3,
        "G-BATTLE" | "GHOST BATTLE" => 4,
        _ => 0,
    }
}

pub(super) fn select_mode_index(mode: &str) -> usize {
    match mode {
        "5K" => 1,
        "7K" => 2,
        "10K" => 3,
        "14K" => 4,
        "9K" => 5,
        "24K" => 6,
        "24K_DOUBLE" => 7,
        _ => 0,
    }
}

pub(super) fn select_sort_index(sort: &str) -> usize {
    match sort {
        "ARTIST" => 1,
        "BPM" => 2,
        "LENGTH" => 3,
        "LEVEL" => 4,
        "CLEAR" => 5,
        "SCORE" => 6,
        "BPCOUNT" => 7,
        _ => 0,
    }
}

pub fn select_ln_mode_index(mode: &str) -> usize {
    match mode {
        "CN" | "AUTO(CN)" | "FORCE(CN)" => 1,
        "HCN" | "AUTO(HCN)" | "FORCE(HCN)" => 2,
        _ => 0,
    }
}

pub fn select_judge_algorithm_index(algorithm: &str) -> usize {
    match algorithm {
        "Duration" | "DURATION" => 1,
        "Lowest" | "LOWEST" => 2,
        _ => 0,
    }
}

pub(super) fn select_scroll_progress(snapshot: &SelectSnapshot) -> f32 {
    if snapshot.chart_count <= 1 {
        return 0.0;
    }
    snapshot.selected_index.min(snapshot.chart_count - 1) as f32 / (snapshot.chart_count - 1) as f32
}

pub(super) fn select_snapshot_selected_row_position(
    rows: &[SelectRowSnapshot],
    selected_index: u32,
) -> usize {
    let center = rows.len() / 2;
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.index == selected_index)
        .min_by_key(|(index, _)| index.abs_diff(center))
        .map(|(index, _)| index)
        .unwrap_or(0)
}
