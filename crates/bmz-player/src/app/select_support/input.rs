#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum SelectRowClickAction {
    Select(usize),
    EnterOrPlay,
    CancelSettingsEdit,
    ExitFolder,
}

pub(in crate::app) fn select_row_click_action(
    row_index: u32,
    button: MouseButton,
    selected_index: usize,
    item_len: usize,
    settings_editing: bool,
) -> Option<SelectRowClickAction> {
    match button {
        MouseButton::Left => {
            let next = row_index as usize;
            if next >= item_len {
                None
            } else if next == selected_index {
                Some(SelectRowClickAction::EnterOrPlay)
            } else {
                Some(SelectRowClickAction::Select(next))
            }
        }
        MouseButton::Right => Some(if settings_editing {
            SelectRowClickAction::CancelSettingsEdit
        } else {
            SelectRowClickAction::ExitFolder
        }),
        _ => None,
    }
}

pub(in crate::app) fn select_scroll_slider_index(value: f32, item_len: usize) -> Option<usize> {
    if item_len == 0 {
        return None;
    }
    if item_len == 1 {
        return Some(0);
    }
    let max_index = item_len - 1;
    Some((value.clamp(0.0, 1.0) * max_index as f32).round() as usize)
}

pub(in crate::app) fn select_scroll_duration_low_ms(
    config: &crate::config::app_config::AppConfig,
) -> u32 {
    config.select.scroll_duration_low_ms.clamp(2, 1000)
}

pub(in crate::app) fn select_scroll_duration_high_ms(
    config: &crate::config::app_config::AppConfig,
) -> u32 {
    config.select.scroll_duration_high_ms.clamp(1, 1000)
}

pub(in crate::app) fn select_analog_scroll_duration(mov: i32) -> Duration {
    let remaining = mov.abs().clamp(1, 2);
    Duration::from_millis((120 / remaining / remaining) as u64)
}

pub(in crate::app) fn log_gamepad_key_config_raw_event(
    backend: &str,
    event: &crate::input::gamepad::RawInputEvent,
) {
    let mapped_control = event.mapped_control.as_deref().unwrap_or("<unmapped>");
    tracing::info!(
        device_id = event.device_id.0,
        kind = event.kind.as_str(),
        logical = %event.logical,
        raw_code = event.raw_code.value,
        raw_code_label = %event.raw_code.label,
        mapped_control = %mapped_control,
        pressed = ?event.pressed,
        value = ?event.value,
        ticks = ?event.ticks,
        backend,
        "gamepad key config input"
    );
}

#[cfg(test)]
pub(in crate::app) fn select_control_action(
    control: &str,
    bindings: &SelectKeyBindings,
) -> Option<SelectAction> {
    scene_select_action(&ControlInputEvent::gamepad(DeviceId(1), control, true), bindings)
}

#[cfg(test)]
pub(in crate::app) fn select_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
    bindings: &SelectKeyBindings,
) -> Option<SelectAction> {
    scene_select_action(&ControlInputEvent::keyboard_parts(physical_key, state, repeat), bindings)
}

pub(in crate::app) fn select_wheel_move(delta: MouseScrollDelta) -> Option<SelectMove> {
    let y = mouse_wheel_y(delta);

    if y > 0.0 {
        Some(SelectMove::Previous)
    } else if y < 0.0 {
        Some(SelectMove::Next)
    } else {
        None
    }
}

pub(in crate::app) fn mouse_wheel_y(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(position) => position.y as f32,
    }
}
use super::*;
