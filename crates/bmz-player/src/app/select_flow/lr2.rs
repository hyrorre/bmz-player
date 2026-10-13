use super::*;

impl WinitApp {
    pub(super) fn lr2_select_active(&self) -> bool {
        self.renderer.select_skin_document().is_some_and(|d| d.lr2)
    }

    /// Executes an LR2 select button. LR2 options cycle the BMZ value in LR2 order;
    /// options without a BMZ counterpart are ignored and never open a BMZ window.
    pub(super) fn execute_lr2_select_button(&mut self, button: i32, direction: i32) {
        let direction = lr2_select_button_direction(button, direction);
        match lr2_select_button_action(button) {
            Lr2SelectButtonAction::TogglePanel(panel) => {
                self.select.option_session.toggle_lr2_panel(
                    panel,
                    self.select.select_option_panel,
                    self.input.start_held,
                    self.input.select_held,
                );
                self.update_select_option_panel();
                return;
            }
            Lr2SelectButtonAction::DifficultyTab(index) => {
                self.select.select_difficulty_filter = SelectDifficultyFilter::ORDER[index];
                self.boot.profile_config.select.difficulty_filter =
                    self.select.select_difficulty_filter.as_str().to_string();
                self.reload_select_items();
            }
            Lr2SelectButtonAction::SkinEvent(event_id) => {
                self.execute_select_skin_event(event_id, direction);
                return;
            }
            Lr2SelectButtonAction::KeyFilter => {
                self.select.select_mode_filter =
                    cycle_lr2_value(LR2_KEY_FILTERS, self.select.select_mode_filter, direction);
                self.reload_select_items();
            }
            Lr2SelectButtonAction::Sort => {
                self.select.select_sort =
                    cycle_lr2_value(LR2_SORTS, self.select.select_sort, direction);
                self.boot.profile_config.select.sort = self.select.select_sort.as_str().to_string();
                self.reload_select_items();
            }
            Lr2SelectButtonAction::Gauge => {
                self.select.gauge_option =
                    cycle_lr2_value(LR2_GAUGES, self.select.gauge_option, direction);
            }
            Lr2SelectButtonAction::Arrange1P => {
                self.select.arrange_option =
                    cycle_lr2_value(LR2_ARRANGES, self.select.arrange_option, direction);
            }
            Lr2SelectButtonAction::Arrange2P => {
                self.select.arrange_option_2p =
                    cycle_lr2_value(LR2_ARRANGES, self.select.arrange_option_2p, direction);
            }
            Lr2SelectButtonAction::LaneEffect => {
                if !self.begin_selected_play_mode_edit() {
                    return;
                }
                self.boot.profile_config.play.lane_effect = cycle_lr2_value(
                    LR2_LANE_EFFECTS,
                    self.boot.profile_config.play.lane_effect,
                    direction,
                );
                self.finish_selected_play_mode_edit();
            }
            Lr2SelectButtonAction::DpFlip => {
                self.select.double_option =
                    cycle_lr2_value(LR2_DP_OPTIONS, self.select.double_option, direction);
            }
            Lr2SelectButtonAction::HsFix => {
                if !self.begin_selected_play_mode_edit() {
                    return;
                }
                self.select.hs_fix_option =
                    cycle_lr2_value(LR2_HS_FIXES, self.select.hs_fix_option, direction);
                self.boot.profile_config.play.hs_fix =
                    hs_fix_config_from_option(self.select.hs_fix_option);
                self.finish_selected_play_mode_edit();
            }
            Lr2SelectButtonAction::JudgeAutoAdjust => self.toggle_visual_offset_auto_adjust(),
            Lr2SelectButtonAction::Target => {
                self.select.target_option =
                    cycle_lr2_value(LR2_TARGETS, self.select.target_option, direction);
                tracing::info!(
                    target = self.select.target_option.as_str(),
                    "target option changed"
                );
            }
            Lr2SelectButtonAction::SkinTarget(button) => {
                if self.select_lr2_skin_target(button) {
                    self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                }
                return;
            }
            Lr2SelectButtonAction::SkinChange => {
                if self.cycle_lr2_skin(direction) {
                    self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                }
                return;
            }
            Lr2SelectButtonAction::SkinItem(row) => {
                if self.cycle_lr2_skin_item(row, direction) {
                    self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                }
                return;
            }
            Lr2SelectButtonAction::Unsupported => {
                tracing::debug!(button, "LR2 select button has no BMZ equivalent");
                return;
            }
        }
        self.invalidate_play_preload();
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn apply_lr2_option_lane(&mut self, lane: Lane) -> bool {
        let (button, direction) = match lane {
            Lane::Key1 | Lane::Key8 => (10, 1),
            Lane::Key2 => (42, 1),
            Lane::Key9 => (43, 1),
            Lane::Key3 | Lane::Key10 => (54, 1),
            Lane::Key4 | Lane::Key11 => (40, 1),
            Lane::Key5 | Lane::Key12 => (57, -1),
            Lane::Key6 | Lane::Key13 => (44, 1),
            Lane::Key7 | Lane::Key14 => (57, 1),
            _ => return false,
        };
        self.execute_lr2_select_button(button, direction);
        true
    }

    pub(super) fn apply_lr2_option_control(&mut self, control: &str) -> bool {
        let keys = &self.select.select_keys;
        let candidates = [
            (keys.is_key1(control), Lane::Key1),
            (keys.is_key2(control), Lane::Key2),
            (keys.is_key3(control), Lane::Key3),
            (keys.is_key4(control), Lane::Key4),
            (keys.is_key5(control), Lane::Key5),
            (keys.is_key6(control), Lane::Key6),
            (keys.is_key7(control), Lane::Key7),
            (keys.is_key8(control), Lane::Key8),
            (keys.is_key9(control), Lane::Key9),
            (keys.is_key10(control), Lane::Key10),
            (keys.is_key11(control), Lane::Key11),
            (keys.is_key12(control), Lane::Key12),
            (keys.is_key13(control), Lane::Key13),
            (keys.is_key14(control), Lane::Key14),
        ];
        candidates
            .into_iter()
            .find(|(matches, _)| *matches)
            .is_some_and(|(_, lane)| self.apply_lr2_option_lane(lane))
    }
}
