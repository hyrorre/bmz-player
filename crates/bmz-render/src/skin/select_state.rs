use super::*;

pub(super) fn select_target_name(target: &str) -> String {
    if let Some(rival_index) = select_rival_index(target) {
        return format!("RIVAL {rival_index}");
    }
    if let Some(index) = select_target_index_for_name(target) {
        return SELECT_TARGET_NAMES[index].to_string();
    }
    String::new()
}

/// Play/Resultの解決済み表示名は、ターゲット設定IDとして再解釈しない。
pub fn play_target_name(target: &str, resolved_name: Option<&str>) -> String {
    if let Some(name) = resolved_name {
        name.to_string()
    } else if target.is_empty()
        || target == "NONE"
        || target.starts_with("IR_")
        || target.starts_with("RIVAL")
    {
        String::new()
    } else {
        select_target_name(target)
    }
}

/// Select/Play/Resultに共通のターゲット設定名 (text ref 3)。
pub fn target_setting_name(target: &str) -> String {
    select_target_name(target)
}

pub(super) fn select_target_name_by_offset(target: &str, offset: i32) -> String {
    let Some(index) = select_target_index_for_name(target) else {
        return String::new();
    };
    let len = SELECT_TARGET_NAMES.len() as i32;
    let shifted = (index as i32 + offset).rem_euclid(len) as usize;
    SELECT_TARGET_NAMES[shifted].to_string()
}

pub(super) fn select_target_index_for_name(target: &str) -> Option<usize> {
    SELECT_TARGET_IDS.iter().position(|id| *id == target).or(match target {
        "RIVAL" => Some(11),
        "AAA" => Some(5),
        "AA" => Some(3),
        "A" => Some(1),
        "B" | "C" | "D" | "E" => Some(1),
        _ => None,
    })
}

pub(super) fn select_rival_index(target: &str) -> Option<u8> {
    target.strip_prefix("RIVAL_")?.parse::<u8>().ok().filter(|&index| index > 0)
}

pub(super) fn full_label(primary: &str, secondary: &str) -> String {
    match (primary.is_empty(), secondary.is_empty()) {
        (true, true) => String::new(),
        (false, true) => primary.to_string(),
        (true, false) => secondary.to_string(),
        (false, false) => format!("{primary} {secondary}"),
    }
}

pub(super) fn select_row_level_number(row: &SelectRowSnapshot) -> i64 {
    if let Some(level) = &row.level_display_override {
        return level.trim().parse().unwrap_or(0);
    }
    row.play_level.chars().filter(|ch| ch.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

pub(super) fn select_row_difficulty_code(row: &SelectRowSnapshot) -> i64 {
    difficulty_code_from_label(&row.difficulty_name)
}

pub(super) fn difficulty_code_from_label(label: &str) -> i64 {
    let normalized = label.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "1" | "BEGINNER" => 1,
        "2" | "NORMAL" => 2,
        "3" | "HYPER" => 3,
        "4" | "ANOTHER" => 4,
        "5" | "INSANE" => 5,
        _ => 0,
    }
}

pub(super) fn score_target_timer_elapsed_ms(timer_id: i32, state: &SkinDrawState) -> Option<i32> {
    let max = state.total_notes.saturating_mul(2);
    let threshold = match timer_id {
        348 => rank_threshold(max, 18), // RANK A
        349 => rank_threshold(max, 21), // RANK AA
        350 => rank_threshold(max, 24), // RANK AAA
        351 => state.best_ex_score?,
        352 => state.target_ex_score?,
        _ => return None,
    };
    (threshold > 0 && state.ex_score >= threshold).then_some(state.elapsed_ms)
}

pub(super) fn select_settings_row_kind_index(kind: SelectRowKind) -> i32 {
    match kind {
        SelectRowKind::SettingsRoot | SelectRowKind::SettingsFolder => 1,
        SelectRowKind::SettingsBack => 2,
        SelectRowKind::SettingsClose => 3,
        _ => 0,
    }
}

pub(super) fn select_row_bar_image_index(row: &SelectRowSnapshot) -> usize {
    match row.kind {
        SelectRowKind::Song if !row.in_library => 4,
        SelectRowKind::Course | SelectRowKind::RandomCourse if !row.in_library => 4,
        SelectRowKind::NoSong => 4,
        SelectRowKind::Song => 0,
        SelectRowKind::Folder => 1,
        SelectRowKind::TableFolder | SelectRowKind::Executable | SelectRowKind::RandomCourse => 2,
        SelectRowKind::SearchFolder => 6,
        SelectRowKind::Course => 3,
        SelectRowKind::Command | SelectRowKind::Container => 5,
        SelectRowKind::SettingsRoot | SelectRowKind::SettingsFolder => 8,
        SelectRowKind::SettingsBack => 9,
        SelectRowKind::SettingsClose => 10,
        SelectRowKind::Config => 0,
    }
}

pub(super) fn select_row_bar_image_fallback_indices(row: &SelectRowSnapshot) -> &'static [usize] {
    match row.kind {
        SelectRowKind::SearchFolder => &[1],
        SelectRowKind::SettingsRoot
        | SelectRowKind::SettingsBack
        | SelectRowKind::SettingsClose => &[6, 1],
        SelectRowKind::SettingsFolder => &[1],
        _ => &[],
    }
}

pub(super) fn select_row_bar_text_index(row: &SelectRowSnapshot) -> usize {
    match row.kind {
        SelectRowKind::Song if !row.in_library => 8,
        SelectRowKind::Course | SelectRowKind::RandomCourse if !row.in_library => 8,
        SelectRowKind::NoSong => 8,
        SelectRowKind::Song => 2,
        SelectRowKind::Folder => 4,
        SelectRowKind::TableFolder | SelectRowKind::Executable | SelectRowKind::RandomCourse => 6,
        SelectRowKind::SearchFolder => 10,
        SelectRowKind::Course => 7,
        SelectRowKind::Command | SelectRowKind::Container => 9,
        SelectRowKind::SettingsRoot | SelectRowKind::SettingsFolder => 11,
        SelectRowKind::SettingsBack => 12,
        SelectRowKind::SettingsClose => 13,
        SelectRowKind::Config => 2,
    }
}

pub(super) fn select_row_bar_text_fallback_indices(row: &SelectRowSnapshot) -> &'static [usize] {
    match row.kind {
        SelectRowKind::SearchFolder => &[4],
        SelectRowKind::SettingsRoot
        | SelectRowKind::SettingsBack
        | SelectRowKind::SettingsClose => &[10, 4],
        SelectRowKind::SettingsFolder => &[4],
        _ => &[],
    }
}

pub(super) fn select_row_slot_with_fallbacks<'a, T>(
    slots: &'a [T],
    primary_index: usize,
    fallback_indices: &[usize],
) -> Option<&'a T> {
    slots
        .get(primary_index)
        .or_else(|| fallback_indices.iter().find_map(|&index| slots.get(index)))
        .or_else(|| slots.first())
}

pub(super) fn select_row_clear_index(row: &SelectRowSnapshot) -> usize {
    match row.clear_type.as_str() {
        "Failed" => 1,
        "AssistEasy" => 2,
        "LightAssistEasy" => 3,
        "Easy" => 4,
        "Normal" => 5,
        "Hard" => 6,
        "ExHard" => 7,
        "FullCombo" => 8,
        "Perfect" => 9,
        "Max" => 10,
        _ => 0,
    }
}

pub(super) fn select_row_replay_index(
    row: &SelectRowSnapshot,
    selected_replay_slot: Option<u8>,
) -> Option<usize> {
    selected_replay_slot
        .map(usize::from)
        .filter(|slot| row.replay_slots.get(*slot).copied().unwrap_or(false))
}

pub(super) fn select_row_trophy_index(row: &SelectRowSnapshot) -> Option<usize> {
    let mut trophy_index = None;
    for name in &row.achieved_trophy_names {
        let rank = match name.as_str() {
            "bronzemedal" => 0,
            "silvermedal" => 1,
            "goldmedal" => 2,
            _ => continue,
        };
        trophy_index = Some(trophy_index.map_or(rank, |current: usize| current.max(rank)));
    }
    trophy_index
}

pub(super) fn select_row_label_indices(row: &SelectRowSnapshot) -> Vec<usize> {
    let mut indices = Vec::new();
    if row.has_long_notes {
        indices.push(0);
    }
    if row.has_random {
        indices.push(1);
    }
    if row.has_mines {
        indices.push(2);
    }
    indices
}

pub(super) fn select_replay_op_matches(op: i32, state: &SkinDrawState) -> bool {
    if state.in_settings {
        return false;
    }
    let slot = match op {
        196..=198 => Some(0),
        1196..=1198 => Some(1),
        1199..=1201 => Some(2),
        1202..=1204 => Some(3),
        1205..=1208 => return state.select_replay_index == Some((op - 1205) as usize),
        _ => None,
    };
    let Some(slot) = slot else {
        return false;
    };
    let has_replay = state.select_replay_slots.get(slot).copied().unwrap_or(false);
    match op {
        196 | 1196 | 1199 | 1202 => !has_replay,
        197 | 1197 | 1200 | 1203 => has_replay,
        198 | 1198 | 1201 | 1204 => false,
        _ => false,
    }
}

pub(super) fn result_replay_op_matches(op: i32, state: &SkinDrawState) -> bool {
    let slot = match op {
        196..=198 => Some(0),
        1196..=1198 => Some(1),
        1199..=1201 => Some(2),
        1202..=1204 => Some(3),
        1205..=1208 => return false,
        _ => None,
    };
    let Some(slot) = slot else {
        return false;
    };
    let saved = state.result_saved_replay_slots.get(slot).copied().unwrap_or(false);
    let exists = state.result_replay_slots.get(slot).copied().unwrap_or(false) && !saved;
    match op {
        196 | 1196 | 1199 | 1202 => !exists && !saved,
        197 | 1197 | 1200 | 1203 => exists,
        198 | 1198 | 1201 | 1204 => saved,
        _ => false,
    }
}

pub(super) fn result_arrange_op_matches(op: i32, state: &SkinDrawState) -> bool {
    let Some(index) = (match op {
        126 => Some(0),  // OPTION_CLEAR_NORMAL
        127 => Some(1),  // OPTION_CLEAR_MIRROR
        128 => Some(2),  // OPTION_CLEAR_RANDOM
        1128 => Some(3), // OPTION_CLEAR_RRANDOM
        129 => Some(4),  // OPTION_CLEAR_SRANDOM
        1129 => Some(5), // OPTION_CLEAR_SPIRAL
        130 => Some(6),  // OPTION_CLEAR_HRANDOM
        131 => Some(7),  // OPTION_CLEAR_ALLSCR
        1130 => Some(8), // OPTION_CLEAR_EXRANDOM
        1131 => Some(9), // OPTION_CLEAR_EXSRANDOM
        _ => None,
    }) else {
        return false;
    };
    state.result_arrange_index == index
}

pub(super) fn select_song_detail_row(state: &SkinDrawState) -> bool {
    matches!(state.select_row_kind, SelectRowKind::Song) && !state.select_is_folder
}

pub(super) fn select_banner_option_matches(want_banner: bool, state: &SkinDrawState) -> bool {
    if !state.select_screen {
        return false;
    }
    state.select_has_banner == want_banner
}

pub(super) fn select_song_option_matches(state: &SkinDrawState) -> bool {
    state.select_screen
        && state.select_row_kind == SelectRowKind::Song
        && !state.in_settings
        && state.select_in_library
}

pub(super) fn select_key_mode_option_matches(op: i32, state: &SkinDrawState) -> bool {
    effective_skin_key_mode(state).is_some_and(|mode| key_mode_option_matches(op, mode))
}

pub(super) fn effective_skin_key_mode(state: &SkinDrawState) -> Option<KeyMode> {
    if state.select_screen && state.result_failed.is_none() {
        // beatoraja MusicSelector only exposes SongData for a SongBar. DirectoryBar,
        // GradeBar, and settings rows must not inherit the active play-mode setting.
        if state.in_settings
            || state.select_row_kind != SelectRowKind::Song
            || state.select_is_folder
        {
            return None;
        }
        return state.skin_attempt.effective_key_mode.or(state.select_chart_key_mode);
    }
    if let Some(key_mode) = state.skin_attempt.effective_key_mode {
        return Some(key_mode);
    }
    Some(state.key_mode)
}

pub(super) fn skin_key_mode_number(mode: KeyMode) -> i32 {
    skin_key_mode_number_i64(mode) as i32
}

pub(super) fn skin_key_mode_number_i64(mode: KeyMode) -> i64 {
    match mode {
        KeyMode::K4 => 4,
        KeyMode::K5 => 5,
        KeyMode::K6 => 6,
        KeyMode::K7 => 7,
        KeyMode::K8 => 8,
        KeyMode::K9 => 9,
        KeyMode::K10 => 10,
        KeyMode::K14 => 14,
    }
}

pub(super) fn key_mode_option_matches(op: i32, mode: KeyMode) -> bool {
    match op {
        160 => matches!(mode, KeyMode::K7 | KeyMode::K8),
        161 => matches!(mode, KeyMode::K5),
        162 => matches!(mode, KeyMode::K14),
        163 => matches!(mode, KeyMode::K10),
        164 => matches!(mode, KeyMode::K9),
        1160 | 1161 => false,
        SKIN_OPTION_BMZ_KEY_MODE_BASE => mode == KeyMode::K4,
        op if op == SKIN_OPTION_BMZ_KEY_MODE_BASE + 1 => mode == KeyMode::K5,
        op if op == SKIN_OPTION_BMZ_KEY_MODE_BASE + 2 => mode == KeyMode::K6,
        op if op == SKIN_OPTION_BMZ_KEY_MODE_BASE + 3 => mode == KeyMode::K7,
        op if op == SKIN_OPTION_BMZ_KEY_MODE_BASE + 4 => mode == KeyMode::K8,
        op if op == SKIN_OPTION_BMZ_KEY_MODE_BASE + 5 => mode == KeyMode::K9,
        op if op == SKIN_OPTION_BMZ_KEY_MODE_BASE + 6 => mode == KeyMode::K10,
        op if op == SKIN_OPTION_BMZ_KEY_MODE_BASE + 7 => mode == KeyMode::K14,
        SKIN_OPTION_BMZ_NO_SCRATCH => {
            matches!(mode, KeyMode::K4 | KeyMode::K6 | KeyMode::K8 | KeyMode::K9)
        }
        SKIN_OPTION_BMZ_SINGLE_PLAY => matches!(mode, KeyMode::K5 | KeyMode::K7),
        SKIN_OPTION_BMZ_DOUBLE_PLAY => matches!(mode, KeyMode::K10 | KeyMode::K14),
        _ => false,
    }
}

pub(super) fn select_detail_artist<'a>(
    snapshot: &SelectSnapshot,
    selected_row: Option<&'a SelectRowSnapshot>,
) -> &'a str {
    if !snapshot.in_settings {
        return selected_row
            .filter(|row| row.kind == SelectRowKind::Song)
            .map(|row| row.artist.as_str())
            .unwrap_or_default();
    }
    selected_row
        .filter(|row| row.kind == SelectRowKind::Config)
        .map(|row| row.artist.as_str())
        .unwrap_or_default()
}

pub(super) fn select_detail_title<'a>(
    snapshot: &'a SelectSnapshot,
    selected_row: Option<&'a SelectRowSnapshot>,
) -> &'a str {
    let Some(row) = selected_row else {
        return if snapshot.in_settings { "" } else { &snapshot.selected_title };
    };
    if snapshot.in_settings {
        return row.title.as_str();
    }
    match row.kind {
        SelectRowKind::Song
        | SelectRowKind::Folder
        | SelectRowKind::TableFolder
        | SelectRowKind::SearchFolder
        | SelectRowKind::Command
        | SelectRowKind::Container
        | SelectRowKind::SettingsRoot
        | SelectRowKind::SettingsFolder
        | SelectRowKind::SettingsBack
        | SelectRowKind::SettingsClose => row.title.as_str(),
        SelectRowKind::Course
        | SelectRowKind::Executable
        | SelectRowKind::RandomCourse
        | SelectRowKind::NoSong
        | SelectRowKind::Config => "",
    }
}

pub(super) fn select_detail_genre<'a>(
    snapshot: &SelectSnapshot,
    selected_row: Option<&'a SelectRowSnapshot>,
) -> &'a str {
    if snapshot.in_settings {
        if snapshot.settings_editing
            && selected_row.is_some_and(|row| row.kind == SelectRowKind::Config)
        {
            return "[編集中]";
        }
        return selected_row.map(|row| row.genre.as_str()).unwrap_or_default();
    }
    selected_row
        .filter(|row| row.kind == SelectRowKind::Song)
        .map(|row| row.genre.as_str())
        .unwrap_or_default()
}

pub(super) fn select_detail_subtitle<'a>(
    snapshot: &SelectSnapshot,
    selected_row: Option<&'a SelectRowSnapshot>,
) -> &'a str {
    if snapshot.in_settings {
        return selected_row
            .filter(|row| row.kind == SelectRowKind::Config)
            .map(|row| row.subtitle.as_str())
            .unwrap_or_default();
    }
    selected_row
        .filter(|row| row.kind == SelectRowKind::Song)
        .map(|row| row.subtitle.as_str())
        .unwrap_or_default()
}

pub(super) fn select_row_shows_score_decorations(row: &SelectRowSnapshot) -> bool {
    !row.is_folder
        && row.in_library
        && matches!(row.kind, SelectRowKind::Song | SelectRowKind::Course)
}

pub(super) fn select_row_shows_level(row: &SelectRowSnapshot) -> bool {
    row.kind == SelectRowKind::Song && row.show_level
}

pub(super) fn select_row_shows_lamp(row: &SelectRowSnapshot) -> bool {
    row.in_library
        && matches!(
            row.kind,
            SelectRowKind::Song
                | SelectRowKind::Course
                | SelectRowKind::Folder
                | SelectRowKind::TableFolder
                | SelectRowKind::SearchFolder
                | SelectRowKind::Command
                | SelectRowKind::Container
        )
}

pub(super) fn select_row_shows_course_trophy(row: &SelectRowSnapshot) -> bool {
    row.kind == SelectRowKind::Course
}

pub(super) fn select_row_shows_folder_distribution(row: &SelectRowSnapshot) -> bool {
    row.is_folder
        && matches!(
            row.kind,
            SelectRowKind::Folder
                | SelectRowKind::TableFolder
                | SelectRowKind::SearchFolder
                | SelectRowKind::Command
                | SelectRowKind::Container
        )
}

pub(super) fn select_rank_op_matches(op: i32, state: &SkinDrawState) -> bool {
    if !select_rank_available(state) {
        return false;
    }
    let Some(rank) = current_rank_index(state) else {
        return false;
    };
    op == 200 + rank as i32
}

pub(super) fn select_small_rank_op_matches(op: i32, state: &SkinDrawState) -> bool {
    if !select_rank_available(state) {
        return false;
    }
    let (ex_score, total_notes) = current_rank_inputs(state);
    let max_score = total_notes.saturating_mul(2);
    if max_score == 0 || ex_score.is_none() {
        return false;
    }
    let ex_score = ex_score.unwrap();
    if ex_score >= max_score {
        return op == 300;
    }
    let Some(rank) = current_rank_index(state) else {
        return false;
    };
    rank <= 6 && op == 301 + rank as i32
}

pub(super) fn select_rank_available(state: &SkinDrawState) -> bool {
    if state.in_settings {
        return false;
    }
    !state.select_screen
        || (state.select_row_kind == SelectRowKind::Song
            && !state.select_is_folder
            && state.select_in_library)
}

pub(super) fn result_rank_op_matches(op: i32, state: &SkinDrawState) -> bool {
    if matches!(op, 308 | 318) {
        return state.ex_score == 0 && state.total_notes > 0;
    }
    let Some(rank) = current_rank_index(state) else {
        return false;
    };
    match op {
        300..=307 => op == 300 + rank as i32,
        310..=317 => op == 310 + rank as i32,
        _ => false,
    }
}

pub(super) fn is_grade_diff_rank_destination(
    destination: &SkinDestinationDef,
    state: &SkinDrawState,
) -> bool {
    (state.result_failed.is_some() || state.select_screen)
        && grade_diff_rank_destination_grade(&destination.id).is_some()
        && destination.op.iter().any(|op| op.unsigned_abs() >= 300 && op.unsigned_abs() <= 307)
}

pub(super) fn grade_diff_rank_destination_matches(
    destination: &SkinDestinationDef,
    op: i32,
    state: &SkinDrawState,
) -> bool {
    let Some(facts) = score_grade_facts(state) else {
        return false;
    };
    let grade = facts.next_label();
    grade_diff_rank_destination_grade(&destination.id) == Some(grade)
        && grade_diff_rank_op(grade).is_some_and(|rank_op| op == rank_op)
}

fn grade_diff_rank_op(grade: &str) -> Option<i32> {
    match grade {
        "MAX" => Some(300),
        "AAA" => Some(301),
        "AA" => Some(302),
        "A" => Some(303),
        "B" => Some(304),
        "C" => Some(305),
        "D" => Some(306),
        "E" => Some(307),
        _ => None,
    }
}

fn grade_diff_rank_destination_grade(id: &str) -> Option<&'static str> {
    match id {
        "RANK_s_MAX" => Some("MAX"),
        "RANK_s_AAA" => Some("AAA"),
        "RANK_s_AA" => Some("AA"),
        "RANK_s_A" => Some("A"),
        "RANK_s_B" => Some("B"),
        "RANK_s_C" => Some("C"),
        "RANK_s_D" => Some("D"),
        "RANK_s_E" => Some("E"),
        _ => None,
    }
}
