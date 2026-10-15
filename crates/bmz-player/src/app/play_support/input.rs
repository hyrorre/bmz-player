#[cfg(test)]
pub(in crate::app) fn hispeed_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<HispeedChange> {
    match keyboard_lane_action(
        &ControlInputEvent::keyboard_parts(physical_key, state, repeat),
        &crate::config::play_input::default_profile_input(),
    ) {
        Some(PlayLaneAction::Hispeed(change)) => Some(change),
        _ => None,
    }
}

pub(in crate::app) fn play_option_control_for_input(
    device: DeviceId,
    control: &PhysicalControl,
    e1_held: bool,
    e2_held: bool,
    play_input: Option<&PlayOptionInput>,
    profile_input: &ProfileInputConfig,
) -> Option<PlayOptionControl> {
    let play_input = play_input?;
    let resolved = play_input.resolve_entry(device, control);
    if resolved.is_none()
        && ((e1_held && play_input.is_action(device, control, InputActionConfig::E2))
            || (e2_held && play_input.is_action(device, control, InputActionConfig::E1)))
    {
        return Some(PlayOptionControl::ToggleHispeedMode);
    }
    if e1_held == e2_held {
        return None;
    }

    let resolved = resolved?;
    if let Some(direction) = crate::config::play_input::hispeed_direction_for_lane(
        profile_input,
        play_input.key_mode,
        resolved.lane,
    ) {
        return match (e1_held, direction) {
            (true, HispeedDirectionConfig::Down) => {
                Some(PlayOptionControl::Hispeed(HispeedChange::Down))
            }
            (true, HispeedDirectionConfig::Up) => {
                Some(PlayOptionControl::Hispeed(HispeedChange::Up))
            }
            (false, HispeedDirectionConfig::Down) => {
                Some(PlayOptionControl::GreenNumber(GreenNumberChange::Down))
            }
            (false, HispeedDirectionConfig::Up) => {
                Some(PlayOptionControl::GreenNumber(GreenNumberChange::Up))
            }
        };
    }

    let control_name = physical_control_name(control);
    let scratch_direction = resolved.scratch_direction.or_else(|| {
        control_name.and_then(|control| {
            if is_scratch_up_control(control) {
                Some(ScratchDirection::Up)
            } else if is_scratch_down_control(control) {
                Some(ScratchDirection::Down)
            } else {
                None
            }
        })
    })?;
    match (e1_held, scratch_direction) {
        (true, ScratchDirection::Up) => Some(PlayOptionControl::LaneCover(LaneCoverChange::Up)),
        (true, ScratchDirection::Down) => Some(PlayOptionControl::LaneCover(LaneCoverChange::Down)),
        (false, ScratchDirection::Up) => {
            Some(PlayOptionControl::GreenNumber(GreenNumberChange::Up))
        }
        (false, ScratchDirection::Down) => {
            Some(PlayOptionControl::GreenNumber(GreenNumberChange::Down))
        }
    }
}

/// Practice 設定 egui がゲーム入力を占有している間でも、プレイ操作へ渡すべき入力か。
///
/// E1/E2 自体の Press を先にプレイ側へ渡すことでホールドを開始できるようにし、
/// ホールド中は全キーをプレイ側へ切り替える。egui 表示前やホールド中に app 側へ
/// 渡したキーの Release も通し、押下状態を残さない。
pub(in crate::app) fn keyboard_input_bypasses_egui(
    has_play_context: bool,
    e1_held: bool,
    e2_held: bool,
    app_key_held: bool,
    control: Option<&PhysicalControl>,
    play_input: Option<&PlayOptionInput>,
) -> bool {
    if app_key_held {
        return true;
    }
    if !has_play_context {
        return false;
    }
    if e1_held || e2_held {
        return true;
    }
    let (Some(control), Some(play_input)) = (control, play_input) else {
        return false;
    };
    !play_input.resolves_lane(W_KEYBOARD_DEVICE_ID, control)
        && (play_input.is_action(W_KEYBOARD_DEVICE_ID, control, InputActionConfig::E1)
            || play_input.is_action(W_KEYBOARD_DEVICE_ID, control, InputActionConfig::E2))
}

pub(in crate::app) fn egui_blocks_raw_play_keyboard(
    practice_overlay: bool,
    e1_held: bool,
    e2_held: bool,
) -> bool {
    practice_overlay && !e1_held && !e2_held
}

/// Window keyboard を app 側の画面操作へ伝播させず、egui に留めるか。
///
/// Practice 設定は未消費キーも含めて排他的に扱う。通常プレイは egui の消費状態に
/// かかわらずプレイへ渡し、Select 等の非プレイ画面は egui が実際に消費した入力だけを止める。
pub(in crate::app) fn egui_blocks_window_keyboard_route(
    has_play_context: bool,
    practice_overlay: bool,
    play_owns_keyboard_input: bool,
    egui_consumed: bool,
) -> bool {
    if play_owns_keyboard_input || (has_play_context && !practice_overlay) {
        return false;
    }
    practice_overlay || egui_consumed
}

pub(in crate::app) fn visual_offset_delta_control(
    control: &str,
    bindings: &SelectKeyBindings,
) -> Option<i32> {
    if bindings.is_ui_key5(control) {
        Some(-1)
    } else if bindings.is_ui_key7(control) {
        Some(1)
    } else {
        None
    }
}

pub(in crate::app) fn green_number_delta_control(
    control: &str,
    bindings: &SelectKeyBindings,
) -> Option<i32> {
    if bindings.is_ui_key4(control) {
        Some(-1)
    } else if bindings.is_ui_key6(control) {
        Some(1)
    } else {
        None
    }
}

#[cfg(test)]
pub(in crate::app) fn lane_cover_step(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<f32> {
    match keyboard_lane_action(
        &ControlInputEvent::keyboard_parts(physical_key, state, repeat),
        &crate::config::play_input::default_profile_input(),
    ) {
        Some(PlayLaneAction::LaneCoverDelta(delta)) => Some(delta),
        _ => None,
    }
}

pub(in crate::app) fn lane_cover_change_step(change: LaneCoverChange) -> f32 {
    match change {
        LaneCoverChange::Up => LANE_COVER_STEP,
        LaneCoverChange::Down => -LANE_COVER_STEP,
    }
}

/// アナログスクラッチによる緑数字操作は、レーンカバー操作とは増減方向が逆。
/// 正の step (Scratch Down) で緑数字を上げ、負の step (Scratch Up) で下げる。
pub(in crate::app) fn green_number_change_from_analog_steps(steps: i32) -> GreenNumberChange {
    if steps > 0 { GreenNumberChange::Up } else { GreenNumberChange::Down }
}

pub(in crate::app) fn green_number_change_step(change: GreenNumberChange) -> i32 {
    match change {
        GreenNumberChange::Up => 1,
        GreenNumberChange::Down => -1,
    }
}

pub(in crate::app) fn hispeed_step_for_profile(profile: &ProfileConfig, mode: HispeedMode) -> f32 {
    match mode {
        HispeedMode::Normal | HispeedMode::Classic => normalize_hispeed_step(
            profile.lane.classic_hispeed_step,
            default_classic_hispeed_step(),
        ),
        HispeedMode::Floating => normalize_hispeed_step(
            profile.lane.floating_hispeed_step,
            default_floating_hispeed_step(),
        ),
    }
}

pub(in crate::app) fn adjusted_normal_hispeed_level(current: u8, change: HispeedChange) -> u8 {
    let delta = match change {
        HispeedChange::Down => -1,
        HispeedChange::Up => 1,
    };
    (i16::from(current) + delta).clamp(
        i16::from(crate::config::play::NORMAL_HISPEED_LEVEL_MIN),
        i16::from(crate::config::play::NORMAL_HISPEED_LEVEL_MAX),
    ) as u8
}

pub(in crate::app) fn adjusted_hispeed(current: f32, change: HispeedChange, step: f32) -> f32 {
    let delta = match change {
        HispeedChange::Down => -step,
        HispeedChange::Up => step,
    };
    clamp_hispeed(current + delta)
}

/// Classic + HS-FIXでは、実効倍率ではなくBPM150基準のHS値に対して操作する。
pub(in crate::app) fn adjust_hispeed_in_display_units(
    hispeed: f32,
    mode: HispeedMode,
    classic_hsfix_scale: f32,
    adjust: impl FnOnce(f32) -> f32,
) -> f32 {
    if mode != HispeedMode::Classic || classic_hsfix_scale == 1.0 {
        return adjust(hispeed);
    }
    let display = crate::config::play::classic_display_hispeed(hispeed, classic_hsfix_scale);
    crate::config::play::classic_effective_hispeed(adjust(display), classic_hsfix_scale)
}
use super::*;
