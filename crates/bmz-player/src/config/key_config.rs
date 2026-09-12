use bmz_core::lane::KeyMode;

use super::play::{lane_from_config, lane_to_config};
use super::play_input::{
    default_play_bindings, gamepad_play_binding_for_device, is_gamepad_device, play_binding,
    resolve_play_bindings, scratch_play_binding,
};
use super::profile_config::{
    BindingConfigEntry, InputActionConfig, KeyboardBindingSlotConfig, LaneConfig,
    PlayModeInputConfig, ProfileConfig, ProfileInputConfig, ScratchDirectionConfig,
};

/// 選曲画面のキー設定で編集対象とする KEY モード。
pub const KEY_CONFIG_MODES: &[KeyMode] = &[
    KeyMode::K4,
    KeyMode::K5,
    KeyMode::K6,
    KeyMode::K7,
    KeyMode::K8,
    KeyMode::K9,
    KeyMode::K10,
    KeyMode::K14,
];

/// 1 レーンあたりの割り当てスロット。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyBindingSlot {
    KeyboardPrimary,
    KeyboardSecondary,
    /// 単一パッド用ワイルドカード (`device = "gamepad"`)。7K 等。
    Controller,
    /// 1P 側コントローラ (`device = "gamepad1"`)。10K/14K 用。
    Controller1P,
    /// 2P 側コントローラ (`device = "gamepad2"`)。10K/14K 用。
    Controller2P,
}

/// キー設定 UI のスロット雛形。`Controller` はレーン側に応じて 1P/2P へ解決する。
pub const KEY_BINDING_SLOTS: &[KeyBindingSlot] = &[
    KeyBindingSlot::KeyboardPrimary,
    KeyBindingSlot::KeyboardSecondary,
    KeyBindingSlot::Controller,
];

impl KeyBindingSlot {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::KeyboardPrimary => "KEYBOARD",
            Self::KeyboardSecondary => "KEYBOARD SUB",
            Self::Controller => "CONTROLLER",
            Self::Controller1P => "CONTROLLER 1P",
            Self::Controller2P => "CONTROLLER 2P",
        }
    }

    pub fn device(self) -> &'static str {
        match self {
            Self::KeyboardPrimary | Self::KeyboardSecondary => "keyboard",
            Self::Controller => "gamepad",
            Self::Controller1P => "gamepad1",
            Self::Controller2P => "gamepad2",
        }
    }

    pub fn is_controller(self) -> bool {
        matches!(self, Self::Controller | Self::Controller1P | Self::Controller2P)
    }

    pub fn listen_hint(self) -> &'static str {
        match self {
            Self::KeyboardPrimary | Self::KeyboardSecondary => {
                "PRESS KEY  Deleteキーで割り当てを解除"
            }
            Self::Controller | Self::Controller1P | Self::Controller2P => "PRESS BTN",
        }
    }
}

/// 10K/14K ではレーン側に応じて `Controller1P` / `Controller2P` を返す。
pub fn controller_slot_for_lane(key_mode: KeyMode, lane: LaneConfig) -> KeyBindingSlot {
    match key_mode {
        KeyMode::K10 | KeyMode::K14 => {
            if is_player2_lane(lane) {
                KeyBindingSlot::Controller2P
            } else {
                KeyBindingSlot::Controller1P
            }
        }
        _ => KeyBindingSlot::Controller,
    }
}

pub fn is_player2_lane(lane: LaneConfig) -> bool {
    matches!(
        lane,
        LaneConfig::Scratch2
            | LaneConfig::Key8
            | LaneConfig::Key9
            | LaneConfig::Key10
            | LaneConfig::Key11
            | LaneConfig::Key12
            | LaneConfig::Key13
            | LaneConfig::Key14
    )
}

/// `KEY_BINDING_SLOTS` の `Controller` 雛形をレーン側の実スロットへ解決する。
pub fn resolve_binding_slot(
    slot: KeyBindingSlot,
    key_mode: KeyMode,
    lane: LaneConfig,
) -> KeyBindingSlot {
    if slot == KeyBindingSlot::Controller { controller_slot_for_lane(key_mode, lane) } else { slot }
}

/// スクラッチの上下方向（UI / 選曲入力用。`Lane::Scratch` は増やさない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScratchDirection {
    Up,
    Down,
}

/// キー設定 UI の 1 行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyBindingTarget {
    Key { lane: LaneConfig, slot: KeyBindingSlot },
    Scratch { lane: LaneConfig, direction: ScratchDirection, slot: KeyBindingSlot },
    Action { action: InputActionConfig, slot: KeyBindingSlot },
}

/// キーコンフィグの共通/画面別グループ。
///
/// `Common` は全画面で有効な共通操作、その他は対応する画面だけで有効な
/// 操作を表す。競合解決では、現在の画面に応じて共通グループも候補に含める。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyBindingGroup {
    Common,
    Select,
    Play,
    Result,
}

impl KeyBindingTarget {
    pub fn slot(self) -> KeyBindingSlot {
        match self {
            Self::Key { slot, .. } | Self::Scratch { slot, .. } | Self::Action { slot, .. } => slot,
        }
    }
}

pub const COMMON_ACTIONS: &[InputActionConfig] =
    &[InputActionConfig::E1, InputActionConfig::E2, InputActionConfig::E3, InputActionConfig::E4];

pub const SELECT_ACTIONS: &[InputActionConfig] = &[
    InputActionConfig::SelectModeFilter,
    InputActionConfig::SelectSort,
    InputActionConfig::SelectLnMode,
    InputActionConfig::SelectReplayCycle,
    InputActionConfig::SelectReplayPlay,
    InputActionConfig::SelectOpenKeyConfig,
    InputActionConfig::SelectRivalCycle,
    InputActionConfig::SelectSameFolder,
    InputActionConfig::SelectOpenDocuments,
    InputActionConfig::SelectDifficultyFilter,
    InputActionConfig::SelectOpenFolder,
    InputActionConfig::SelectReload,
    InputActionConfig::SelectFavoriteSong,
    InputActionConfig::SelectFavoriteChart,
    InputActionConfig::SelectAutoplayFolder,
    InputActionConfig::SelectOpenIr,
    InputActionConfig::Screenshot,
];

pub const PLAY_ACTIONS: &[InputActionConfig] = &[
    InputActionConfig::PlayHispeedDown,
    InputActionConfig::PlayHispeedUp,
    InputActionConfig::PlayLaneCoverUp,
    InputActionConfig::PlayLaneCoverDown,
    InputActionConfig::PlayVisualOffsetUp,
    InputActionConfig::PlayVisualOffsetDown,
    InputActionConfig::PlayVisualOffsetAutoAdjust,
];

/// リザルト専用の設定可能アクション。現在は固定操作のみのため空だが、
/// 共通操作と画面専用操作を同じカテゴリ解決経路で扱うために予約する。
pub const RESULT_ACTIONS: &[InputActionConfig] = &[];

pub fn key_mode_settings_path(keys_root: &str, key_mode: KeyMode) -> String {
    format!("{keys_root}:{}", key_mode.play_map_key())
}

pub fn is_scratch_lane(lane: LaneConfig) -> bool {
    matches!(lane, LaneConfig::Scratch | LaneConfig::Scratch2)
}

pub fn lane_entries_for_key_mode(key_mode: KeyMode) -> Vec<LaneConfig> {
    key_mode.active_lanes().iter().map(|&lane| lane_to_config(lane)).collect()
}

pub fn scratch_lanes_for_key_mode(key_mode: KeyMode) -> Vec<LaneConfig> {
    lane_entries_for_key_mode(key_mode).into_iter().filter(|&lane| is_scratch_lane(lane)).collect()
}

pub fn key_lanes_for_key_mode(key_mode: KeyMode) -> Vec<LaneConfig> {
    lane_entries_for_key_mode(key_mode).into_iter().filter(|&lane| !is_scratch_lane(lane)).collect()
}

pub fn lane_label(lane: LaneConfig) -> &'static str {
    match lane {
        LaneConfig::Scratch => "SCRATCH",
        LaneConfig::Scratch2 => "SCRATCH 2",
        LaneConfig::Key1 => "KEY 1",
        LaneConfig::Key2 => "KEY 2",
        LaneConfig::Key3 => "KEY 3",
        LaneConfig::Key4 => "KEY 4",
        LaneConfig::Key5 => "KEY 5",
        LaneConfig::Key6 => "KEY 6",
        LaneConfig::Key7 => "KEY 7",
        LaneConfig::Key8 => "2P KEY 1",
        LaneConfig::Key9 => "2P KEY 2",
        LaneConfig::Key10 => "2P KEY 3",
        LaneConfig::Key11 => "2P KEY 4",
        LaneConfig::Key12 => "2P KEY 5",
        LaneConfig::Key13 => "2P KEY 6",
        LaneConfig::Key14 => "2P KEY 7",
    }
}

pub fn lane_label_for_key_mode(key_mode: KeyMode, lane: LaneConfig) -> &'static str {
    match (key_mode, lane) {
        (KeyMode::K8 | KeyMode::K9, LaneConfig::Key8) => "KEY 8",
        (KeyMode::K9, LaneConfig::Key9) => "KEY 9",
        _ => lane_label(lane),
    }
}

pub fn binding_row_label(key_mode: KeyMode, target: KeyBindingTarget) -> String {
    format!("{} ({})", binding_target_label(key_mode, target), target.slot().suffix())
}

pub fn binding_target_label(key_mode: KeyMode, target: KeyBindingTarget) -> String {
    match target {
        KeyBindingTarget::Key { lane, .. } => lane_label_for_key_mode(key_mode, lane).to_string(),
        KeyBindingTarget::Scratch { lane, direction, .. } => {
            let dir = match direction {
                ScratchDirection::Up => "UP",
                ScratchDirection::Down => "DOWN",
            };
            format!("{} {}", lane_label_for_key_mode(key_mode, lane), dir)
        }
        KeyBindingTarget::Action { action, .. } => action_label(action).to_string(),
    }
}

pub fn common_key_binding_targets(slot: KeyBindingSlot) -> Vec<KeyBindingTarget> {
    key_binding_targets_for_group(KeyBindingGroup::Common, slot)
}

pub fn key_binding_targets_for_group(
    group: KeyBindingGroup,
    slot: KeyBindingSlot,
) -> Vec<KeyBindingTarget> {
    let actions = match group {
        KeyBindingGroup::Common => COMMON_ACTIONS,
        KeyBindingGroup::Select => SELECT_ACTIONS,
        KeyBindingGroup::Play => PLAY_ACTIONS,
        KeyBindingGroup::Result => RESULT_ACTIONS,
    };
    actions
        .iter()
        .copied()
        .filter(|action| {
            !slot.is_controller()
                || !crate::config::profile_config::PLAY_KEYBOARD_SHORTCUT_ACTIONS.contains(action)
        })
        .map(|action| KeyBindingTarget::Action { action, slot })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingContext {
    Common,
    Select,
    Play,
    Result,
    KeyMode,
}

fn binding_context_for_action(action: InputActionConfig) -> BindingContext {
    if COMMON_ACTIONS.contains(&action) {
        BindingContext::Common
    } else if SELECT_ACTIONS.contains(&action) {
        BindingContext::Select
    } else if PLAY_ACTIONS.contains(&action) {
        BindingContext::Play
    } else {
        BindingContext::Result
    }
}

fn binding_context_for_target(target: KeyBindingTarget) -> BindingContext {
    match target {
        KeyBindingTarget::Action { action, .. } => binding_context_for_action(action),
        KeyBindingTarget::Key { .. } | KeyBindingTarget::Scratch { .. } => BindingContext::KeyMode,
    }
}

fn binding_priority(context: BindingContext) -> u8 {
    match context {
        BindingContext::KeyMode => 3,
        BindingContext::Common => 2,
        BindingContext::Select | BindingContext::Play | BindingContext::Result => 1,
    }
}

fn contexts_conflict(left: BindingContext, right: BindingContext) -> bool {
    matches!(
        (left, right),
        (BindingContext::Common, _)
            | (_, BindingContext::Common)
            | (BindingContext::KeyMode, BindingContext::Play)
            | (BindingContext::Play, BindingContext::KeyMode)
            | (BindingContext::KeyMode, BindingContext::KeyMode)
            | (BindingContext::Select, BindingContext::Select)
            | (BindingContext::Play, BindingContext::Play)
            | (BindingContext::Result, BindingContext::Result)
    )
}

fn devices_conflict(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    (is_gamepad_device(left) && right.eq_ignore_ascii_case("gamepad"))
        || (is_gamepad_device(right) && left.eq_ignore_ascii_case("gamepad"))
}

fn controls_conflict(left_device: &str, left: &str, right_device: &str, right: &str) -> bool {
    devices_conflict(left_device, right_device)
        && (left == right
            || (left_device.eq_ignore_ascii_case("keyboard")
                && right_device.eq_ignore_ascii_case("keyboard")
                && (numeric_keypad_alias(left) == Some(right)
                    || numeric_keypad_alias(right) == Some(left))))
}

fn numeric_keypad_alias(control: &str) -> Option<&'static str> {
    match control {
        "0" => Some("Numpad0"),
        "1" => Some("Numpad1"),
        "2" => Some("Numpad2"),
        "3" => Some("Numpad3"),
        "4" => Some("Numpad4"),
        "5" => Some("Numpad5"),
        "6" => Some("Numpad6"),
        "7" => Some("Numpad7"),
        "8" => Some("Numpad8"),
        "9" => Some("Numpad9"),
        "Numpad0" => Some("0"),
        "Numpad1" => Some("1"),
        "Numpad2" => Some("2"),
        "Numpad3" => Some("3"),
        "Numpad4" => Some("4"),
        "Numpad5" => Some("5"),
        "Numpad6" => Some("6"),
        "Numpad7" => Some("7"),
        "Numpad8" => Some("8"),
        "Numpad9" => Some("9"),
        _ => None,
    }
}

fn target_controls(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> Vec<(String, String)> {
    match target {
        KeyBindingTarget::Action { action, slot } => match slot {
            KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                read_action_keyboard_slots(&profile.input, action)
                    .get(slot)
                    .into_iter()
                    .map(|control| ("keyboard".to_string(), control))
                    .collect()
            }
            KeyBindingSlot::Controller
            | KeyBindingSlot::Controller1P
            | KeyBindingSlot::Controller2P => {
                action_controls_for_slot(&profile.input, action, slot)
                    .into_iter()
                    .map(|control| (slot.device().to_string(), control))
                    .collect()
            }
        },
        KeyBindingTarget::Key { lane, slot } => resolve_play_bindings(&profile.input, key_mode)
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| {
                entry.lane == Some(lane)
                    && devices_conflict(&entry.device, slot.device())
                    && (slot.is_controller()
                        || entry.keyboard_slot.is_none()
                        || entry.keyboard_slot
                            == Some(match slot {
                                KeyBindingSlot::KeyboardPrimary => {
                                    KeyboardBindingSlotConfig::Primary
                                }
                                KeyBindingSlot::KeyboardSecondary => {
                                    KeyboardBindingSlotConfig::Secondary
                                }
                                _ => unreachable!(),
                            }))
            })
            .map(|entry| (entry.device, entry.control))
            .collect(),
        KeyBindingTarget::Scratch { lane, direction, slot } => {
            let bindings = resolve_play_bindings(&profile.input, key_mode).unwrap_or_default();
            match slot {
                KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                    read_scratch_keyboard_slots(&bindings, lane)
                        .get(direction, slot)
                        .into_iter()
                        .map(|control| ("keyboard".to_string(), control))
                        .collect()
                }
                KeyBindingSlot::Controller
                | KeyBindingSlot::Controller1P
                | KeyBindingSlot::Controller2P => {
                    read_scratch_gamepad_slots_for_device(&bindings, lane, slot.device())
                        .get(direction)
                        .into_iter()
                        .map(|control| (slot.device().to_string(), control))
                        .collect()
                }
            }
        }
    }
}

/// 指定した割り当てと同じ画面で競合する設定を説明する。
/// 割り当て自体は削除せず、実行時の優先順位を表示するために利用する。
pub fn key_binding_conflict_description(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> Option<String> {
    let target_context = binding_context_for_target(target);
    let target_action = match target {
        KeyBindingTarget::Action { action, .. } => Some(action),
        KeyBindingTarget::Key { .. } | KeyBindingTarget::Scratch { .. } => None,
    };
    let target_controls = target_controls(profile, key_mode, target);
    if target_controls.is_empty() {
        return None;
    }
    let target_display =
        target_controls.iter().map(|(_, control)| control.clone()).collect::<Vec<_>>().join(" / ");

    let mut conflicts = Vec::new();
    for (device, control) in target_controls {
        for entry in &profile.input.ui.bindings {
            let Some(action) = entry.action else { continue };
            if !controls_conflict(&entry.device, &entry.control, &device, &control) {
                continue;
            }
            // 同じアクションの別スロットは競合ではない。アクション行自身の
            // 現在値を候補から除外し、同じキーに割り当てた別アクションだけを
            // 警告する。
            if target_action == Some(action)
                || !contexts_conflict(target_context, binding_context_for_action(action))
            {
                continue;
            }
            conflicts
                .push((action_label(action), binding_priority(binding_context_for_action(action))));
        }

        for entry in resolve_play_bindings(&profile.input, key_mode).unwrap_or_default() {
            if !controls_conflict(&entry.device, &entry.control, &device, &control)
                || entry.lane.is_none()
                || target_lane_binding_is_self(target, &entry)
            {
                continue;
            }
            if contexts_conflict(target_context, BindingContext::KeyMode) {
                conflicts.push((
                    lane_label(entry.lane.expect("checked above")),
                    binding_priority(BindingContext::KeyMode),
                ));
            }
        }
    }
    conflicts
        .sort_unstable_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    conflicts.dedup();
    let (winner, winner_priority) = conflicts.first()?;
    let target_priority = binding_priority(target_context);
    if *winner_priority > target_priority {
        Some(format!("{winner} が優先 (同じキー: {target_display})"))
    } else {
        Some(format!("同じキーに {winner} が割り当てられています ({target_display})"))
    }
}

fn target_lane_binding_is_self(target: KeyBindingTarget, entry: &BindingConfigEntry) -> bool {
    match target {
        KeyBindingTarget::Key { lane, .. } => entry.lane == Some(lane),
        KeyBindingTarget::Scratch { lane, direction, .. } => {
            entry.lane == Some(lane)
                && entry.scratch
                    == Some(match direction {
                        ScratchDirection::Up => ScratchDirectionConfig::Up,
                        ScratchDirection::Down => ScratchDirectionConfig::Down,
                    })
        }
        KeyBindingTarget::Action { .. } => false,
    }
}

pub fn key_mode_binding_targets(key_mode: KeyMode, slot: KeyBindingSlot) -> Vec<KeyBindingTarget> {
    let scratch_rows = scratch_lanes_for_key_mode(key_mode).into_iter().flat_map(|lane| {
        let slot = resolve_binding_slot(slot, key_mode, lane);
        [ScratchDirection::Up, ScratchDirection::Down]
            .into_iter()
            .map(move |direction| KeyBindingTarget::Scratch { lane, direction, slot })
    });
    let key_rows = key_lanes_for_key_mode(key_mode).into_iter().map(|lane| {
        let slot = resolve_binding_slot(slot, key_mode, lane);
        KeyBindingTarget::Key { lane, slot }
    });
    scratch_rows.chain(key_rows).collect()
}

pub fn action_label(action: InputActionConfig) -> &'static str {
    match action {
        InputActionConfig::PlayHispeedDown => "HISPEED DOWN",
        InputActionConfig::PlayHispeedUp => "HISPEED UP",
        InputActionConfig::PlayLaneCoverUp => "LANE COVER UP",
        InputActionConfig::PlayLaneCoverDown => "LANE COVER DOWN",
        InputActionConfig::PlayVisualOffsetUp => "VISUAL OFFSET +1",
        InputActionConfig::PlayVisualOffsetDown => "VISUAL OFFSET -1",
        InputActionConfig::PlayVisualOffsetAutoAdjust => "AUTO JUDGE ADJUST",
        InputActionConfig::E1 => "E1",
        InputActionConfig::E2 => "E2",
        InputActionConfig::E3 => "E3",
        InputActionConfig::E4 => "E4",
        InputActionConfig::SelectEnter => "ENTER",
        InputActionConfig::SelectOptionArrange => "OPTION ARRANGE",
        InputActionConfig::SelectOptionGauge => "OPTION GAUGE",
        InputActionConfig::SelectOptionAssist => "OPTION ASSIST",
        InputActionConfig::SelectOptionBga => "OPTION BGA",
        InputActionConfig::SelectOpenFolder => "OPEN FOLDER / COPY HASH",
        InputActionConfig::SelectReload => "RELOAD",
        InputActionConfig::SelectAutoplayFolder => "AUTOPLAY FOLDER",
        InputActionConfig::SelectOpenIr => "OPEN IR",
        InputActionConfig::SelectOpenKeyConfig => "KEY CONFIG",
        InputActionConfig::Screenshot => "SCREENSHOT",
        InputActionConfig::SelectRivalCycle => "RIVAL CYCLE",
        InputActionConfig::SelectOpenDocuments => "OPEN SONG TEXT",
        InputActionConfig::SelectFavoriteSong => "FAVORITE SONG",
        InputActionConfig::SelectFavoriteChart => "FAVORITE CHART",
        InputActionConfig::SelectSameFolder => "SAME FOLDER",
        InputActionConfig::SelectModeFilter => "MODE FILTER",
        InputActionConfig::SelectSort => "SORT",
        InputActionConfig::SelectLnMode => "LN MODE",
        InputActionConfig::SelectDifficultyFilter => "DIFFICULTY FILTER",
        InputActionConfig::SelectReplayCycle => "REPLAY CYCLE",
        InputActionConfig::SelectReplayPlay => "REPLAY PLAY",
    }
}

pub fn is_scratch_up_control(control: &str) -> bool {
    control.contains("ScratchUp")
        || control.ends_with('-')
        || control == "Axis1-"
        || control == "Axis2-"
        || control == "Button9"
}

pub fn is_scratch_down_control(control: &str) -> bool {
    control.contains("ScratchDown")
        || control.ends_with('+')
        || control == "Axis1+"
        || control == "Axis2+"
        || control == "Button8"
}

pub fn format_play_binding(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> String {
    match target {
        KeyBindingTarget::Action { .. } => format_action_binding(&profile.input, target),
        _ => format_target_control(&resolved_play_bindings(&profile.input, key_mode), target),
    }
}

fn resolved_play_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Vec<BindingConfigEntry> {
    resolve_play_bindings(input, key_mode).unwrap_or_else(|_| default_play_bindings(key_mode))
}

fn format_target_control(bindings: &[BindingConfigEntry], target: KeyBindingTarget) -> String {
    match target {
        KeyBindingTarget::Key { lane, slot } => match slot {
            KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                read_lane_keyboard_slots(bindings, lane)
                    .get(slot)
                    .unwrap_or_else(|| "(none)".to_string())
            }
            KeyBindingSlot::Controller
            | KeyBindingSlot::Controller1P
            | KeyBindingSlot::Controller2P => {
                let controls = gamepad_controls_for_lane_device(bindings, lane, slot.device());
                if controls.is_empty() { "(none)".to_string() } else { controls.join(" / ") }
            }
        },
        KeyBindingTarget::Scratch { lane, direction, slot } => match slot {
            KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                read_scratch_keyboard_slots(bindings, lane)
                    .get(direction, slot)
                    .unwrap_or_else(|| "(none)".to_string())
            }
            KeyBindingSlot::Controller
            | KeyBindingSlot::Controller1P
            | KeyBindingSlot::Controller2P => {
                read_scratch_gamepad_slots_for_device(bindings, lane, slot.device())
                    .get(direction)
                    .unwrap_or_else(|| "(none)".to_string())
            }
        },
        KeyBindingTarget::Action { .. } => "(none)".to_string(),
    }
}

fn format_action_binding(input: &ProfileInputConfig, target: KeyBindingTarget) -> String {
    let KeyBindingTarget::Action { action, slot } = target else {
        return "(none)".to_string();
    };
    match slot {
        KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
            read_action_keyboard_slots(input, action)
                .get(slot)
                .unwrap_or_else(|| "(none)".to_string())
        }
        KeyBindingSlot::Controller
        | KeyBindingSlot::Controller1P
        | KeyBindingSlot::Controller2P => {
            let controls = action_controls_for_slot(input, action, slot);
            if controls.is_empty() { "(none)".to_string() } else { controls.join(" / ") }
        }
    }
}

#[path = "key_config/api.rs"]
mod api;
#[path = "key_config/read.rs"]
mod read;
#[path = "key_config/write.rs"]
mod write;

pub use api::*;
use read::*;
use write::*;

#[cfg(test)]
#[path = "key_config/tests.rs"]
mod tests;
