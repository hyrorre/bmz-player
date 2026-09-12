use bmz_render::scene::SelectRowKind;
use bmz_render::skin::default_skin_manifest;

use crate::config::app_config::{AppConfig, FrameLatencyModeConfig, PathEntry, VsyncModeConfig};
use crate::config::profile_config::ProfileConfig;
use crate::screens::select_model::{SelectChartRow, SelectCourseRow};
use crate::skin_loader::default_skin_root;
use crate::storage::score_db::BestScoreSummary;

use super::*;

pub(super) fn app_test_chart() -> bmz_chart::model::PlayableChart {
    bmz_chart::model::PlayableChart {
        identity: bmz_core::chart::ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
        metadata: bmz_chart::model::ChartMetadata {
            title: "app test".to_string(),
            initial_bpm: 120.0,
            total: Some(160.0),
            ..Default::default()
        },
        lane_notes: std::array::from_fn(|_| Vec::new()),
        long_notes: Vec::new(),
        bgm_events: Vec::new(),
        bga_events: Vec::new(),
        timing_events: Vec::new(),
        scroll_events: Vec::new(),
        speed_events: Vec::new(),
        judge_rank_events: Vec::new(),
        bgm_volume_events: Vec::new(),
        key_volume_events: Vec::new(),
        text_events: Vec::new(),
        bga_opacity_events: Vec::new(),
        bga_argb_events: Vec::new(),
        swbga_definitions: Vec::new(),
        bga_keybound_events: Vec::new(),
        bga_asset_by_bmp_key: std::collections::HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 0,
        end_time: TimeUs(0),
    }
}

fn default_select_keys() -> SelectKeyBindings {
    SelectKeyBindings::from_profile(&crate::config::play_input::default_profile_input())
}

fn select_keys_9k() -> SelectKeyBindings {
    let mut input = crate::config::play_input::default_profile_input();
    input.select_input_mode = SelectInputModeConfig::Key9;
    SelectKeyBindings::from_profile(&input)
}

fn play_option_input_for(input: &ProfileInputConfig, key_mode: KeyMode) -> PlayOptionInput {
    PlayOptionInput::new(
        key_mode,
        crate::config::play::lane_binding_for_chart(input, key_mode),
        input,
        crate::input::gamepad::GamepadSlotMap::default(),
    )
}

fn keyboard_play_option(
    control: &str,
    e1_held: bool,
    e2_held: bool,
    _keys: &SelectKeyBindings,
    play_input: &PlayOptionInput,
    input: &ProfileInputConfig,
) -> Option<PlayOptionControl> {
    play_option_control_for_input(
        W_KEYBOARD_DEVICE_ID,
        &PhysicalControl::KeyboardKey(control.to_string()),
        e1_held,
        e2_held,
        Some(play_input),
        input,
    )
}

fn select_keys_with_full_2p_bindings() -> SelectKeyBindings {
    let mut input = crate::config::play_input::default_profile_input();
    let key = KeyMode::K14.play_map_key().to_string();
    input.play.insert(
        key.clone(),
        crate::config::profile_config::PlayModeInputConfig {
            inherit: None,
            bindings: crate::config::play_input::default_play_14k_bindings(),
            ..Default::default()
        },
    );
    let play14 = input.play.get_mut(&key).expect("14K bindings");
    play14.bindings.push(crate::config::play_input::play_binding("P2K6", LaneConfig::Key13));
    play14.bindings.push(crate::config::play_input::play_binding("P2K7", LaneConfig::Key14));
    SelectKeyBindings::from_profile(&input)
}

fn chart_row_with_mode(index: usize, mode: &str) -> SelectItem {
    let mut row = select_chart_row(index);
    row.chart.as_mut().unwrap().mode = mode.to_string();
    SelectItem::Chart(row)
}

fn select_chart_row(index: usize) -> SelectChartRow {
    SelectChartRow {
        chart: Some(ChartListItem {
            chart_id: index as i64,
            md5: [0u8; 16],
            sha256: [index as u8; 32],
            title: format!("Title {index}"),
            subtitle: String::new(),
            artist: format!("Artist {index}"),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: index.to_string(),
            mode: "7K".to_string(),
            total_notes: 100,
            initial_bpm: 128.0,
            min_bpm: 128.0,
            max_bpm: 128.0,
            length_ms: 90_000,
            folder_path: String::new(),
            stage_file: String::new(),
            banner_file: String::new(),
            backbmp_file: String::new(),
            preview_file: String::new(),
            has_document: false,
            has_bga: false,
            has_long_notes: false,
            has_mines: false,
            has_bms_random: false,
            judge_rank: Some(1),
            bms_total: 200.0,
            ln_profile: Default::default(),
            ln_counts: Default::default(),
        }),
        chart_analysis: Some(crate::storage::library_db::ChartAnalysisSummary {
            normal_notes: 40 + index as u32,
            long_notes: 1 + index as u32,
            scratch_notes: 3,
            long_scratch_notes: 1,
            density: 4.5,
            peak_density: 12.5,
            end_density: 8.25,
            total_gauge: 260.0,
            main_bpm: 128.0,
            speed_changes: Vec::new(),
        }),
        has_document: false,
        fallback_title: String::new(),
        fallback_artist: String::new(),
        entry_sha256: None,
        download_metadata: crate::song_download::ChartDownloadMetadata::default(),
        best_score: None,
        replay_slots: [false; 4],
        favorite_chart: false,
        favorite_song: false,
        table_level: String::new(),
        table_text: DifficultyTableText::default(),
    }
}

fn select_course_row(resolved_count: usize, entry_count: usize) -> SelectCourseRow {
    let entry_previews = (0..entry_count)
        .map(|index| crate::screens::select_model::CourseEntryPreview {
            title: format!("Stage {}", index + 1),
            artist: String::new(),
            play_level: String::new(),
            difficulty_name: String::new(),
            total_notes: 0,
            resolved: index < resolved_count,
        })
        .collect();
    SelectCourseRow {
        course_id: resolved_count as i64,
        course_hash: None,
        rian_course_hash_v1: None,
        bms_ir_course_key: None,
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
        title: format!("Course {resolved_count}/{entry_count}"),
        kind: bmz_core::course::CourseKind::Dan,
        constraints: bmz_core::course::CourseConstraints::default(),
        entry_count,
        resolved_count,
        common_key_mode: None,
        total_notes: 100,
        total_length_ms: 90_000,
        min_bpm: 128.0,
        max_bpm: 128.0,
        category_label: "DAN".to_string(),
        trophy_names: Vec::new(),
        entry_previews,
        best_score: None,
        replay_slots: [false; 4],
        achieved_trophy_names: Vec::new(),
    }
}

fn best_score_with_replay(ex_score: u32, replay_path: &str) -> BestScoreSummary {
    BestScoreSummary {
        chart_sha256: [0; 32],
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
        double_option: crate::select_options::DoubleOptionScoreBucket::Off,
        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
        clear_type: "Normal".to_string(),
        gauge_type: "Normal".to_string(),
        gauge_value: Some(80.0),
        ex_score,
        bp: 0,
        cb: 0,
        max_combo: 100,
        judge_counts: DisplayJudgeCounts::default(),
        fast_slow_counts: FastSlowJudgeCounts::default(),
        play_count: 42,
        clear_count: 31,
        device_type: bmz_core::input::InputDeviceKind::Keyboard,
        played_at: 1,
        replay_path: replay_path.to_string(),
    }
}

#[path = "tests/course.rs"]
mod course;
#[path = "tests/play.rs"]
mod play;
#[path = "tests/result.rs"]
mod result;
#[path = "tests/runtime.rs"]
mod runtime;
#[path = "tests/select.rs"]
mod select;
#[path = "tests/skin.rs"]
mod skin;
#[path = "tests/skin_more.rs"]
mod skin_more;
