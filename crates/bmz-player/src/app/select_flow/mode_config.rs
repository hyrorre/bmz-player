use super::*;

pub(in crate::app) fn select_item_play_mode(
    item: Option<&SelectItem>,
    filter: SelectModeFilter,
) -> Option<KeyMode> {
    match item {
        Some(SelectItem::Chart(row)) => row
            .chart
            .as_ref()
            .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            .or_else(|| filter.key_mode())
            .or(Some(KeyMode::K7)),
        Some(SelectItem::Course(row)) => row.common_key_mode,
        _ => filter.key_mode().or(Some(KeyMode::K7)),
    }
}

pub(in crate::app) fn play_config_key_mode_for_runtime(
    active_play: Option<KeyMode>,
    pending_play: Option<KeyMode>,
) -> Option<KeyMode> {
    active_play.or(pending_play)
}

impl WinitApp {
    /// The source chart mode, before conversion (library queries and skin attempt identity).
    pub(super) fn selected_source_key_mode(&self) -> Option<KeyMode> {
        select_item_play_mode(
            self.select.select_items.get(self.select.selected_index),
            self.select.select_mode_filter,
        )
    }

    /// The profile slot used for editing and starting the selected chart.
    pub(super) fn selected_play_config_key_mode(&self) -> Option<KeyMode> {
        self.selected_source_key_mode()
            .map(|mode| effective_play_key_mode(mode, self.selected_key_mode_conversion()))
    }

    pub(super) fn selected_key_mode_conversion(&self) -> KeyModeConversionConfig {
        key_mode_conversion_for_session(
            self.boot.profile_config.play.key_mode_conversion,
            self.select.session_mode,
            self.select.double_option,
            self.select.ir_battle.active,
        )
    }

    pub(super) fn sync_selected_play_mode(&mut self) {
        let mode = if self.select.select_option_panel == 2 && self.detail_options_enabled() {
            Some(self.detail_options_mode())
        } else {
            self.selected_play_config_key_mode()
        };
        let Some(key_mode) = mode else {
            return;
        };
        if self.boot.profile_config.active_play_mode == key_mode {
            return;
        }
        self.boot.profile_config.activate_play_mode(key_mode);
        self.select.hs_fix_option =
            hs_fix_option_from_profile(self.boot.profile_config.play.hs_fix);
        tracing::debug!(mode = key_mode.as_str(), "activated key-mode play settings");
    }

    /// Select-side lane options are undefined for a mixed/unresolved course.
    /// Every mutating entry point uses this guard so it cannot accidentally
    /// overwrite the mode that happened to be active before the course row.
    pub(super) fn begin_selected_play_mode_edit(&mut self) -> bool {
        if self.selected_source_key_mode().is_none() {
            return false;
        }
        self.sync_selected_play_mode();
        true
    }

    pub(super) fn finish_selected_play_mode_edit(&mut self) {
        self.boot.profile_config.sync_active_play_mode();
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.invalidate_play_preload();
    }
}
