pub(in crate::ui) struct ProfileSettingsPanelActions {
    pub(in crate::ui) save: bool,
    pub(in crate::ui) save_app_config: bool,
    pub(in crate::ui) key_config_action: Option<EguiKeyConfigAction>,
}

pub(in crate::ui) struct ProfileSettingsPanelContext<'a> {
    pub(in crate::ui) profile: &'a mut ProfileConfig,
    pub(in crate::ui) app_config: &'a mut AppConfig,
    pub(in crate::ui) show_fps: &'a mut bool,
    pub(in crate::ui) ir_login: &'a mut IrLoginUiState,
    pub(in crate::ui) ir_device_key: &'a mut IrDeviceKeyUiState,
    pub(in crate::ui) profile_manager: &'a mut ProfileManagerUiState,
    pub(in crate::ui) key_config: &'a mut EguiKeyConfigUiState,
    pub(in crate::ui) profile_root: &'a std::path::Path,
    pub(in crate::ui) unrestricted: bool,
    pub(in crate::ui) text: Localizer,
}

pub(in crate::ui) fn scene_restricts_settings(scene: &str) -> bool {
    matches!(scene, "Decide" | "Play")
}

pub(in crate::ui) fn restore_restricted_profile_settings(
    profile: &mut ProfileConfig,
    mut readonly: ProfileConfig,
) {
    readonly.audio_mix = profile.audio_mix.clone();
    readonly.ui = profile.ui.clone();
    readonly.system_sound = profile.system_sound.clone();
    readonly.judge = profile.judge.clone();
    readonly.lane = profile.lane.clone();
    readonly.input = profile.input.clone();
    *profile = readonly;
}
use super::*;
