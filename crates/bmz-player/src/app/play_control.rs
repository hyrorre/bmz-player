use crate::config::profile_config::{InputActionConfig, ProfileInputConfig};
use bmz_gameplay::input::backend::PhysicalControl;

use super::LANE_COVER_STEP;
use super::input_runtime::ControlInputEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HispeedChange {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PlayLaneAction {
    ToggleHispeedMode,
    Hispeed(HispeedChange),
    LaneCoverDelta(f32),
    AnalogLaneCoverDelta(f32),
    GreenNumberDelta(i32),
    ToggleLaneCoverVisibility,
    VisualOffsetDelta(i32),
    ToggleVisualOffsetAutoAdjust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayLaneTarget {
    Sudden,
    Lift,
    Hidden,
}

impl PlayLaneTarget {
    pub(super) const fn toggled_lift_hidden(self) -> Self {
        match self {
            Self::Hidden => Self::Lift,
            Self::Sudden | Self::Lift => Self::Hidden,
        }
    }
}

pub(super) fn resolved_play_lane_target(
    sudden_enabled: bool,
    lane_cover_visible: bool,
    lift_enabled: bool,
    hidden_enabled: bool,
    preferred: PlayLaneTarget,
) -> Option<PlayLaneTarget> {
    if sudden_enabled && lane_cover_visible {
        return Some(PlayLaneTarget::Sudden);
    }
    match (lift_enabled, hidden_enabled) {
        (true, true) => Some(match preferred {
            PlayLaneTarget::Hidden => PlayLaneTarget::Hidden,
            PlayLaneTarget::Sudden | PlayLaneTarget::Lift => PlayLaneTarget::Lift,
        }),
        (true, false) => Some(PlayLaneTarget::Lift),
        (false, true) => Some(PlayLaneTarget::Hidden),
        (false, false) => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayOptionControl {
    ToggleHispeedMode,
    Hispeed(HispeedChange),
    LaneCover(LaneCoverChange),
    GreenNumber(GreenNumberChange),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayAnalogOptionMode {
    LaneCover,
    GreenNumber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaneCoverChange {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GreenNumberChange {
    Up,
    Down,
}

/// レーンカバー長押しで連続変化を加える間隔 (1/60秒)。
pub(super) const LANE_COVER_HOLD_TICK_US: u64 = 1_000_000 / 60;
/// フレームが止まった後に一度に加える連続変化の上限 (1秒分)。
const LANE_COVER_HOLD_MAX_TICKS_PER_UPDATE: u32 = 60;

/// ↑↓ (PlayLaneCoverUp/Down) の長押し状態。OSのキーリピートには依存しない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LaneCoverKeyHold {
    pub(super) control: String,
    /// `PlayLaneCoverUp` は +1、`PlayLaneCoverDown` は -1。
    pub(super) direction: i32,
    /// E2 hold中の微調整。押した時点で決める。
    pub(super) fine: bool,
    pub(super) started_at: std::time::Instant,
    pub(super) applied_ticks: u32,
}

impl LaneCoverKeyHold {
    pub(super) fn new(
        control: String,
        direction: i32,
        fine: bool,
        started_at: std::time::Instant,
    ) -> Self {
        Self { control, direction, fine, started_at, applied_ticks: 0 }
    }

    /// `now` までに加えるべき連続変化のうち未適用の分を返し、適用済みにする。
    /// 戻り値は1/1000単位の符号付き変化量。
    pub(super) fn take_due_units(
        &mut self,
        now: std::time::Instant,
        input: &ProfileInputConfig,
    ) -> i32 {
        let elapsed = now.saturating_duration_since(self.started_at);
        let delay = std::time::Duration::from_millis(u64::from(input.lane_cover_hold_delay_ms));
        let Some(holding) = elapsed.checked_sub(delay) else {
            return 0;
        };
        let due = u32::try_from(holding.as_micros() / u128::from(LANE_COVER_HOLD_TICK_US))
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        let ticks =
            due.saturating_sub(self.applied_ticks).min(LANE_COVER_HOLD_MAX_TICKS_PER_UPDATE);
        self.applied_ticks = due;
        let step = if self.fine { 1 } else { lane_cover_hold_step_units(input) };
        self.direction * step * ticks as i32
    }
}

/// ↑↓を1回押したときの変化量 (1/1000単位)。E2 hold中は常に1。
pub(super) fn lane_cover_press_step_units(input: &ProfileInputConfig, fine: bool) -> i32 {
    if fine {
        1
    } else {
        input.lane_cover_step.clamp(
            crate::config::profile_config::LANE_COVER_STEP_MIN,
            crate::config::profile_config::LANE_COVER_STEP_MAX,
        ) as i32
    }
}

fn lane_cover_hold_step_units(input: &ProfileInputConfig) -> i32 {
    input.lane_cover_hold_step.clamp(
        crate::config::profile_config::LANE_COVER_HOLD_STEP_MIN,
        crate::config::profile_config::LANE_COVER_HOLD_STEP_MAX,
    ) as i32
}

pub(super) fn lane_cover_units_to_delta(units: i32) -> f32 {
    units as f32 * LANE_COVER_STEP
}

/// `PixelDelta` (トラックパッド等) を1ノッチとみなす量。
pub(super) const WHEEL_PIXELS_PER_NOTCH: f32 = 40.0;
/// 1イベントで適用するホイールノッチ数の上限。
pub(super) const LANE_COVER_WHEEL_MAX_NOTCHES_PER_EVENT: i32 = 20;

/// プレイ中のマウスホイール回転量を1ノッチ単位にまとめる。
/// 高分解能ホイールやトラックパッドの細かいイベントも、回転量に比例させる。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct LaneCoverWheelAccumulator {
    notches: f32,
    last_at: Option<std::time::Instant>,
}

impl LaneCoverWheelAccumulator {
    /// 上回しを正とする整数ノッチ数を返し、端数を保持する。向きが反転したとき、
    /// または `SELECT_ANALOG_SCROLL_TOLERANCE_MS` 以上空いたときは端数を捨てる。
    pub(super) fn take_whole_notches(
        &mut self,
        delta: winit::event::MouseScrollDelta,
        now: std::time::Instant,
    ) -> i32 {
        let amount = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => y,
            winit::event::MouseScrollDelta::PixelDelta(position) => {
                position.y as f32 / WHEEL_PIXELS_PER_NOTCH
            }
        };
        if !amount.is_finite() || amount == 0.0 {
            return 0;
        }
        let idle = self.last_at.is_none_or(|last| {
            now.saturating_duration_since(last)
                > std::time::Duration::from_millis(super::SELECT_ANALOG_SCROLL_TOLERANCE_MS)
        });
        self.last_at = Some(now);
        if idle || self.notches.signum() * amount.signum() < 0.0 {
            self.notches = 0.0;
        }
        self.notches += amount;
        let whole = self.notches.trunc();
        self.notches -= whole;
        (whole as i32)
            .clamp(-LANE_COVER_WHEEL_MAX_NOTCHES_PER_EVENT, LANE_COVER_WHEEL_MAX_NOTCHES_PER_EVENT)
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// ホイール1ノッチの変化量 (1/1000単位)。E2 hold中は常に1。
pub(super) fn lane_cover_wheel_step_units(input: &ProfileInputConfig, fine: bool) -> i32 {
    if fine {
        1
    } else {
        input.lane_cover_wheel_step.clamp(
            crate::config::profile_config::LANE_COVER_WHEEL_STEP_MIN,
            crate::config::profile_config::LANE_COVER_WHEEL_STEP_MAX,
        ) as i32
    }
}

/// キーボード入力がレーンカバー操作キーなら、そのcontrol名と向き (+1 / -1) を返す。
/// 押下・解放のどちらにも使う。
pub(super) fn keyboard_lane_cover_direction(
    event: &ControlInputEvent,
    input: &ProfileInputConfig,
) -> Option<(String, i32)> {
    let PhysicalControl::KeyboardKey(control) = event.physical.as_ref()? else {
        return None;
    };
    let direction = match keyboard_shortcut_action(control, input)? {
        InputActionConfig::PlayLaneCoverUp => 1,
        InputActionConfig::PlayLaneCoverDown => -1,
        _ => return None,
    };
    Some((control.clone(), direction))
}

fn keyboard_shortcut_action(
    control: &str,
    input: &ProfileInputConfig,
) -> Option<InputActionConfig> {
    // 設定ファイル内の行順に依存させず、プレイ操作の固定順で同一キーを
    // 解決する。共通 E1〜E4 は呼び出し側で先に判定されるため、ここでは
    // プレイ用ショートカット同士の決定順だけを定義する。
    crate::config::profile_config::PLAY_KEYBOARD_SHORTCUT_ACTIONS.iter().copied().find(|action| {
        input.ui.bindings.iter().any(|entry| {
            entry.device == "keyboard"
                && entry.action == Some(*action)
                && keyboard_controls_match(&entry.control, control)
        })
    })
}

#[cfg(test)]
pub(super) fn keyboard_lane_action(
    event: &ControlInputEvent,
    input: &ProfileInputConfig,
) -> Option<PlayLaneAction> {
    keyboard_lane_action_with_fine(event, input, false)
}

/// `fine_lane_cover` はE2 hold中の微調整。レーンカバー操作はOSのキーリピートを
/// 無視し、長押しは [`LaneCoverKeyHold`] で扱う。
pub(super) fn keyboard_lane_action_with_fine(
    event: &ControlInputEvent,
    input: &ProfileInputConfig,
    fine_lane_cover: bool,
) -> Option<PlayLaneAction> {
    if !event.pressed {
        return None;
    }
    let PhysicalControl::KeyboardKey(control) = event.physical.as_ref()? else {
        return None;
    };
    let lane_cover_step =
        lane_cover_units_to_delta(lane_cover_press_step_units(input, fine_lane_cover));
    match keyboard_shortcut_action(control, input)? {
        InputActionConfig::PlayHispeedDown => Some(PlayLaneAction::Hispeed(HispeedChange::Down)),
        InputActionConfig::PlayHispeedUp => Some(PlayLaneAction::Hispeed(HispeedChange::Up)),
        InputActionConfig::PlayLaneCoverUp => {
            (!event.repeat).then_some(PlayLaneAction::LaneCoverDelta(lane_cover_step))
        }
        InputActionConfig::PlayLaneCoverDown => {
            (!event.repeat).then_some(PlayLaneAction::LaneCoverDelta(-lane_cover_step))
        }
        InputActionConfig::PlayVisualOffsetUp => Some(PlayLaneAction::VisualOffsetDelta(1)),
        InputActionConfig::PlayVisualOffsetDown => Some(PlayLaneAction::VisualOffsetDelta(-1)),
        InputActionConfig::PlayVisualOffsetAutoAdjust => {
            (!event.repeat).then_some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
        }
        _ => None,
    }
}

fn keyboard_controls_match(configured: &str, pressed: &str) -> bool {
    configured == pressed
        || numeric_keypad_alias(configured) == Some(pressed)
        || numeric_keypad_alias(pressed) == Some(configured)
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

pub(super) fn lane_action_from_option(
    action: PlayOptionControl,
    is_axis: bool,
) -> Option<PlayLaneAction> {
    match action {
        PlayOptionControl::ToggleHispeedMode => Some(PlayLaneAction::ToggleHispeedMode),
        PlayOptionControl::Hispeed(change) => Some(PlayLaneAction::Hispeed(change)),
        PlayOptionControl::LaneCover(_) if is_axis => None,
        PlayOptionControl::LaneCover(LaneCoverChange::Up) => {
            Some(PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP))
        }
        PlayOptionControl::LaneCover(LaneCoverChange::Down) => {
            Some(PlayLaneAction::LaneCoverDelta(-LANE_COVER_STEP))
        }
        PlayOptionControl::GreenNumber(_) if is_axis => None,
        PlayOptionControl::GreenNumber(GreenNumberChange::Up) => {
            Some(PlayLaneAction::GreenNumberDelta(1))
        }
        PlayOptionControl::GreenNumber(GreenNumberChange::Down) => {
            Some(PlayLaneAction::GreenNumberDelta(-1))
        }
    }
}

#[cfg(test)]
mod tests {
    use winit::event::ElementState;
    use winit::keyboard::{KeyCode, PhysicalKey};

    use super::*;

    fn keyboard(code: KeyCode, repeat: bool) -> ControlInputEvent {
        ControlInputEvent::keyboard_parts(PhysicalKey::Code(code), ElementState::Pressed, repeat)
    }

    #[test]
    fn keyboard_arrows_map_to_shared_lane_actions() {
        let input = crate::config::play_input::default_profile_input();
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::ArrowLeft, false), &input),
            Some(PlayLaneAction::Hispeed(HispeedChange::Down))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::ArrowUp, false), &input),
            Some(PlayLaneAction::LaneCoverDelta(lane_cover_units_to_delta(10)))
        );
        assert_eq!(
            keyboard_lane_action_with_fine(&keyboard(KeyCode::ArrowDown, false), &input, true),
            Some(PlayLaneAction::LaneCoverDelta(-lane_cover_units_to_delta(1)))
        );
        // 長押しはOSのキーリピートではなくLaneCoverKeyHoldで扱う。
        assert_eq!(keyboard_lane_action(&keyboard(KeyCode::ArrowDown, true), &input), None);
    }

    #[test]
    fn remapped_and_cleared_shortcuts_survive_profile_reload() {
        use crate::config::key_config::{
            KeyBindingSlot, KeyBindingTarget, apply_play_binding, clear_play_binding,
        };
        use bmz_core::lane::KeyMode;
        let mut input = crate::config::play_input::default_profile_input();
        for (action, old_key, new_key, control, expected) in [
            (
                InputActionConfig::PlayHispeedDown,
                KeyCode::ArrowLeft,
                KeyCode::KeyH,
                "H",
                PlayLaneAction::Hispeed(HispeedChange::Down),
            ),
            (
                InputActionConfig::PlayHispeedUp,
                KeyCode::ArrowRight,
                KeyCode::KeyJ,
                "J",
                PlayLaneAction::Hispeed(HispeedChange::Up),
            ),
            (
                InputActionConfig::PlayLaneCoverUp,
                KeyCode::ArrowUp,
                KeyCode::KeyK,
                "K",
                PlayLaneAction::LaneCoverDelta(lane_cover_units_to_delta(10)),
            ),
            (
                InputActionConfig::PlayLaneCoverDown,
                KeyCode::ArrowDown,
                KeyCode::KeyL,
                "L",
                PlayLaneAction::LaneCoverDelta(-lane_cover_units_to_delta(10)),
            ),
        ] {
            let target = KeyBindingTarget::Action { action, slot: KeyBindingSlot::KeyboardPrimary };
            apply_play_binding(&mut input, KeyMode::K7, target, control).unwrap();
            input = toml::from_str(&toml::to_string(&input).unwrap()).unwrap();
            crate::config::play_input::normalize_profile_input(&mut input);
            assert_eq!(keyboard_lane_action(&keyboard(old_key, false), &input), None);
            assert_eq!(keyboard_lane_action(&keyboard(new_key, false), &input), Some(expected));
            let mut release = keyboard(new_key, false);
            release.pressed = false;
            assert_eq!(keyboard_lane_action(&release, &input), None);
            clear_play_binding(&mut input, KeyMode::K7, target).unwrap();
            input = toml::from_str(&toml::to_string(&input).unwrap()).unwrap();
            crate::config::play_input::normalize_profile_input(&mut input);
            assert_eq!(keyboard_lane_action(&keyboard(new_key, false), &input), None);
            assert_eq!(keyboard_lane_action(&keyboard(old_key, false), &input), None);
        }
    }

    #[test]
    fn visual_offset_shortcuts_accept_numeric_keypad_aliases() {
        let input = crate::config::play_input::default_profile_input();
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Digit3, false), &input),
            Some(PlayLaneAction::VisualOffsetDelta(1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad3, false), &input),
            Some(PlayLaneAction::VisualOffsetDelta(1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad9, false), &input),
            Some(PlayLaneAction::VisualOffsetDelta(-1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad0, false), &input),
            Some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
        );
    }

    #[test]
    fn visual_offset_auto_adjust_toggles_only_on_press_edges() {
        let input = crate::config::play_input::default_profile_input();
        for code in [KeyCode::Digit0, KeyCode::Numpad0] {
            assert_eq!(
                keyboard_lane_action(&keyboard(code, false), &input),
                Some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
            );
            assert_eq!(keyboard_lane_action(&keyboard(code, true), &input), None);
            let mut release = keyboard(code, false);
            release.pressed = false;
            assert_eq!(keyboard_lane_action(&release, &input), None);
        }

        use crate::config::key_config::{KeyBindingSlot, KeyBindingTarget, apply_play_binding};
        use bmz_core::lane::KeyMode;
        let mut remapped = input.clone();
        apply_play_binding(
            &mut remapped,
            KeyMode::K7,
            KeyBindingTarget::Action {
                action: InputActionConfig::PlayVisualOffsetAutoAdjust,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
            "H",
        )
        .unwrap();
        crate::config::play_input::normalize_profile_input(&mut remapped);
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::KeyH, false), &remapped),
            Some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
        );
        assert_eq!(keyboard_lane_action(&keyboard(KeyCode::KeyH, true), &remapped), None);
        let mut release = keyboard(KeyCode::KeyH, false);
        release.pressed = false;
        assert_eq!(keyboard_lane_action(&release, &remapped), None);

        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Digit3, true), &remapped),
            Some(PlayLaneAction::VisualOffsetDelta(1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad9, true), &remapped),
            Some(PlayLaneAction::VisualOffsetDelta(-1))
        );
    }

    #[test]
    fn duplicate_play_shortcuts_use_fixed_action_order() {
        use crate::config::key_config::{KeyBindingSlot, KeyBindingTarget, apply_play_binding};
        use bmz_core::lane::KeyMode;

        let mut input = crate::config::play_input::default_profile_input();
        apply_play_binding(
            &mut input,
            KeyMode::K7,
            KeyBindingTarget::Action {
                action: InputActionConfig::PlayVisualOffsetUp,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
            "H",
        )
        .unwrap();
        apply_play_binding(
            &mut input,
            KeyMode::K7,
            KeyBindingTarget::Action {
                action: InputActionConfig::PlayHispeedUp,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
            "H",
        )
        .unwrap();

        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::KeyH, false), &input),
            Some(PlayLaneAction::Hispeed(HispeedChange::Up))
        );
    }

    #[test]
    fn axis_button_events_do_not_duplicate_analog_lane_changes() {
        assert_eq!(
            lane_action_from_option(PlayOptionControl::LaneCover(LaneCoverChange::Up), true,),
            None
        );
        assert_eq!(
            lane_action_from_option(PlayOptionControl::GreenNumber(GreenNumberChange::Down), true,),
            None
        );
        assert_eq!(
            lane_action_from_option(PlayOptionControl::Hispeed(HispeedChange::Up), true,),
            Some(PlayLaneAction::Hispeed(HispeedChange::Up))
        );
    }
}
