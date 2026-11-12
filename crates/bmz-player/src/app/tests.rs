use bmz_render::scene::SelectRowKind;
use bmz_render::skin::default_skin_manifest;

use crate::config::app_config::{AppConfig, FrameLatencyModeConfig, PathEntry, VsyncModeConfig};
use crate::config::profile_config::ProfileConfig;
use crate::screens::select_model::{SelectChartRow, SelectCourseRow};
use crate::skin_loader::default_skin_root;
use crate::storage::score_db::BestScoreSummary;

use super::*;

fn installed_lr2_test_root() -> PathBuf {
    std::env::var_os("BMZ_TEST_LR2_SKIN_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/skins"))
}

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

fn default_select_keys() -> SceneInputResolver {
    SceneInputResolver::from_profile(&crate::config::play_input::default_profile_input())
}

fn select_keys_9k() -> SceneInputResolver {
    let mut input = crate::config::play_input::default_profile_input();
    input.select_input_mode = SelectInputModeConfig::Key9;
    SceneInputResolver::from_profile(&input)
}

fn play_option_input_for(input: &ProfileInputConfig, key_mode: KeyMode) -> PlayOptionInput {
    PlayOptionInput::new(
        key_mode,
        crate::config::play::lane_binding_for_chart(input, key_mode),
        input,
        crate::input::gamepad::GamepadSlotMap::default(),
    )
}

/// 1P = DeviceId(16)、2P = DeviceId(17) の割り当て。
fn test_slots() -> crate::input::gamepad::GamepadSlotMap {
    crate::input::gamepad::GamepadSlotMap::from_slot_ids([Some(0), Some(1)])
}

/// 物理入力を選曲側の意味へ解決する。コントローラーは 1P 機器からの入力として扱う。
fn resolve_physical(keys: &SceneInputResolver, control: &PhysicalControl) -> ResolvedControl {
    let device = match control {
        PhysicalControl::KeyboardKey(_) => W_KEYBOARD_DEVICE_ID,
        _ => DeviceId(16),
    };
    keys.resolve_control(device, control, test_slots())
}

/// 入力名から物理入力を作って解決する。`Button` / `Axis` で始まる名前はコントローラー。
fn resolve_named(keys: &SceneInputResolver, name: &str) -> ResolvedControl {
    let control = if name.starts_with("Button") || name.starts_with("Axis") {
        PhysicalControl::GamepadButton(name.into())
    } else {
        PhysicalControl::KeyboardKey(name.into())
    };
    resolve_physical(keys, &control)
}

/// 指定機器のコントローラー入力を選曲オプション用の 14K 相当レーン (9K は 9K レーン) へ解決する。
fn option_lane_for(
    input: &ProfileInputConfig,
    slots: crate::input::gamepad::GamepadSlotMap,
    device: DeviceId,
    name: &str,
) -> Option<Lane> {
    let keys = SceneInputResolver::from_profile(input);
    let control = keys.resolve_control(device, &PhysicalControl::GamepadButton(name.into()), slots);
    if control.is_nine_key() { control.nine_key_lane() } else { control.option_lane() }
}

/// 押下中の入力元をそれぞれ解決して集約した E1 / E2 / E1〜E4 ホールド。
fn hold_state(
    runtime: &AppInputRuntime,
    keys: &SceneInputResolver,
) -> (bool, bool, HashSet<InputActionConfig>) {
    select_hold_state_from_resolved(
        runtime
            .pressed_sources()
            .map(|(device, control)| keys.resolve_control(device, control, test_slots())),
    )
}

/// 入力名の集合 (キーボード / 1P コントローラー) を解決して集約したホールド。
fn hold_state_named(
    pressed: &HashSet<String>,
    keys: &SceneInputResolver,
) -> (bool, bool, HashSet<InputActionConfig>) {
    select_hold_state_from_resolved(pressed.iter().map(|name| resolve_named(keys, name)))
}

/// キーボードの物理キーを選曲側の意味へ解決する。
fn resolve_key(keys: &SceneInputResolver, code: KeyCode) -> ResolvedControl {
    resolve_physical(keys, &physical_key_to_control(PhysicalKey::Code(code)).unwrap())
}

/// 入力名の押下を、プレイ文脈なしで共通入口から解決する。
fn resolve_named_input(keys: &SceneInputResolver, name: &str) -> input_resolve::ResolvedInput {
    let physical = if name.starts_with("Button") || name.starts_with("Axis") {
        PhysicalControl::GamepadButton(name.into())
    } else {
        PhysicalControl::KeyboardKey(name.into())
    };
    let device = match physical {
        PhysicalControl::KeyboardKey(_) => W_KEYBOARD_DEVICE_ID,
        _ => DeviceId(16),
    };
    let event = ControlInputEvent {
        device,
        physical: Some(physical),
        pressed: true,
        repeat: false,
        timestamp: bmz_gameplay::input::backend::DeviceTimestamp::MonotonicNs(1),
        synthesized_analog_axis: false,
    };
    input_resolve::resolve_input(keys, None, test_slots(), event)
}

/// 機器未割り当てのまま、選曲側とプレイ側の両方で解決した押下。
fn resolved_play_press(
    keys: &SceneInputResolver,
    play_input: &PlayOptionInput,
    device: DeviceId,
    control: &PhysicalControl,
) -> input_resolve::ResolvedInput {
    let event = ControlInputEvent {
        device,
        physical: Some(control.clone()),
        pressed: true,
        repeat: false,
        timestamp: bmz_gameplay::input::backend::DeviceTimestamp::MonotonicNs(1),
        synthesized_analog_axis: false,
    };
    input_resolve::resolve_input(
        keys,
        Some(play_input),
        crate::input::gamepad::GamepadSlotMap::default(),
        event,
    )
}

/// 旧来の (device, control) 形式のテスト入力を、共通入口で解決してから判定する。
fn play_option_for(
    device: DeviceId,
    control: &PhysicalControl,
    e1_held: bool,
    e2_held: bool,
    play_input: Option<&PlayOptionInput>,
    input: &ProfileInputConfig,
) -> Option<PlayOptionControl> {
    let keys = SceneInputResolver::from_profile(input);
    let play_input = play_input?;
    play_option_control_for_input(
        &resolved_play_press(&keys, play_input, device, control),
        e1_held,
        e2_held,
        Some(play_input),
        input,
    )
}

fn keyboard_play_option(
    control: &str,
    e1_held: bool,
    e2_held: bool,
    keys: &SceneInputResolver,
    play_input: &PlayOptionInput,
    input: &ProfileInputConfig,
) -> Option<PlayOptionControl> {
    let resolved = resolved_play_press(
        keys,
        play_input,
        W_KEYBOARD_DEVICE_ID,
        &PhysicalControl::KeyboardKey(control.to_string()),
    );
    play_option_control_for_input(&resolved, e1_held, e2_held, Some(play_input), input)
}

fn select_keys_with_full_2p_bindings() -> SceneInputResolver {
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
    SceneInputResolver::from_profile(&input)
}

fn chart_row_with_mode(index: usize, mode: &str) -> SelectItem {
    let mut row = select_chart_row(index);
    row.chart.as_mut().unwrap().mode = mode.to_string();
    SelectItem::Chart(row)
}

fn select_chart_row(index: usize) -> SelectChartRow {
    SelectChartRow {
        last_played_at: None,
        lr2_added_at: None,
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
            main_bpm: 128.0,
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
            has_long_notes: false,
            ln_profile: crate::ln_policy::ChartLnProfile::default(),
            has_mines: false,
            has_random: false,
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
        play_options: None,
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

#[cfg(target_os = "windows")]
impl WinitApp {
    /// テスト用: 物理入力を、現在の画面・プレイセッション・機器割り当てで共通入口から解決する。
    /// コントローラーは DeviceId(16) からの入力として扱う。
    fn test_input(
        &self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> input_resolve::ResolvedInput {
        let device = match control {
            PhysicalControl::KeyboardKey(_) => W_KEYBOARD_DEVICE_ID,
            _ => DeviceId(16),
        };
        self.test_device_input(device, control, pressed, repeat)
    }

    /// テスト用: プロファイルの入力設定と 1P / 2P の割り当てを直接変えた後、
    /// 設定保存・機器更新と同じく resolver とスロットを作り直す。
    fn test_apply_input_config(&mut self) {
        self.select.select_keys = SceneInputResolver::from_profile(&self.boot.profile_config.input);
        let slots =
            resolve_gamepad_runtime_slots(&self.boot.app_config.input, self.gamepad.as_ref());
        self.update_gamepad_slot_assignment(
            crate::input::gamepad::GamepadSlotMap::from_device_ids(slots),
        );
        self.sync_select_holds_from_pressed_controls();
    }

    /// テスト用: キーボードのキー名を選曲オプション操作として解決して適用する。
    fn test_apply_keyboard_option(&mut self, name: &str) -> bool {
        let control =
            *self.test_input(&PhysicalControl::KeyboardKey(name.into()), true, false).scene();
        self.apply_play_option_input(&control)
    }

    /// テスト用: 指定機器のボタンを選曲オプション操作として解決して適用する。
    fn test_apply_gamepad_option(&mut self, device: DeviceId, name: &str) -> bool {
        let control = *self
            .test_device_input(device, &PhysicalControl::GamepadButton(name.into()), true, false)
            .scene();
        self.apply_play_option_input(&control)
    }

    fn test_device_input(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> input_resolve::ResolvedInput {
        self.resolve_control_event(ControlInputEvent {
            device,
            physical: Some(control.clone()),
            pressed,
            repeat,
            timestamp: bmz_gameplay::input::backend::DeviceTimestamp::MonotonicNs(1),
            synthesized_analog_axis: false,
        })
    }
}

#[path = "tests/boot_chart.rs"]
mod boot_chart;
#[path = "tests/course.rs"]
mod course;
#[path = "tests/course_sources.rs"]
mod course_sources;
#[cfg(target_os = "windows")]
#[path = "tests/course_transitions.rs"]
mod course_transitions;
#[path = "tests/decide_difficulty.rs"]
mod decide_difficulty;
#[path = "tests/frame_stalls.rs"]
mod frame_stalls;
#[path = "tests/play.rs"]
mod play;
#[path = "tests/play_chart_length.rs"]
mod play_chart_length;
#[path = "tests/play_preload.rs"]
mod play_preload;
#[path = "tests/result.rs"]
mod result;
#[path = "tests/result_difficulty.rs"]
mod result_difficulty;
#[path = "tests/result_favorites.rs"]
mod result_favorites;
#[path = "tests/runtime.rs"]
mod runtime;
#[path = "tests/select.rs"]
mod select;
#[path = "tests/skin.rs"]
mod skin;
#[path = "tests/skin_more.rs"]
mod skin_more;
