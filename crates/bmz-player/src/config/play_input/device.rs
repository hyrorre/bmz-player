use super::*;

pub fn is_gamepad_device(device: &str) -> bool {
    gamepad_player_index(device).is_some() || device.trim().eq_ignore_ascii_case("gamepad")
}

/// 設定上の `device` 文字列が指す物理デバイス。
///
/// `Unresolved` は「番号付きスロット (`gamepad1` 等) に実機が割り当たっていない」状態で、
/// 全デバイス対象の wildcard (`Any`) とは区別する。未解決スロットの binding は
/// 生成しない (別側の実機へ補完したり wildcard 化したりしない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigDeviceResolution {
    Any,
    Device(DeviceId),
    Unresolved,
}

pub(super) fn resolve_config_device(device: &str, slots: GamepadSlotMap) -> ConfigDeviceResolution {
    match gamepad_player_index(device) {
        Some(index) => slots
            .device_id_for_player(index)
            .map_or(ConfigDeviceResolution::Unresolved, ConfigDeviceResolution::Device),
        None => ConfigDeviceResolution::Any,
    }
}

/// LaneBinding 用の device。外側の `None` は未解決スロットで、binding 自体を作らない。
pub(super) fn binding_device_from_config(
    device: &str,
    slots: GamepadSlotMap,
) -> Option<Option<DeviceId>> {
    match resolve_config_device(device, slots) {
        ConfigDeviceResolution::Any => Some(None),
        ConfigDeviceResolution::Device(id) => Some(Some(id)),
        ConfigDeviceResolution::Unresolved => None,
    }
}

pub fn gamepad_player_index(device: &str) -> Option<u32> {
    let lower = device.trim().to_ascii_lowercase();
    let suffix = lower.strip_prefix("gamepad")?;
    if suffix.is_empty() {
        return None;
    }
    suffix.parse::<u32>().ok().filter(|index| *index > 0)
}

pub fn control_from_config(device: &str, control: &str) -> PhysicalControl {
    match device.to_ascii_lowercase().as_str() {
        device if is_gamepad_device(device) => PhysicalControl::GamepadButton(control.to_string()),
        "hid" => control
            .parse::<u32>()
            .map(PhysicalControl::HidButton)
            .unwrap_or_else(|_| PhysicalControl::KeyboardKey(control.to_string())),
        _ => PhysicalControl::KeyboardKey(control.to_string()),
    }
}
