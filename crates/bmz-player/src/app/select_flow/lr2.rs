use super::*;

impl WinitApp {
    pub(super) fn lr2_select_active(&self) -> bool {
        self.renderer.select_skin_document().is_some_and(|d| d.lr2)
    }

    pub(super) fn execute_lr2_select_button(&mut self, button: i32, direction: i32) {
        match button {
            91..=96 => {
                self.select.select_difficulty_filter =
                    SelectDifficultyFilter::ORDER[(button - 91) as usize];
                self.boot.profile_config.select.difficulty_filter =
                    self.select.select_difficulty_filter.as_str().to_string();
                self.reload_select_items();
            }
            11 => {
                const VALUES: [SelectModeFilter; 6] = [
                    SelectModeFilter::All,
                    SelectModeFilter::K7,
                    SelectModeFilter::K5,
                    SelectModeFilter::K14,
                    SelectModeFilter::K10,
                    SelectModeFilter::K9,
                ];
                if !VALUES.contains(&self.select.select_mode_filter) {
                    self.open_advanced_settings_from_select();
                    return;
                }
                self.select.select_mode_filter =
                    cycle_enum(VALUES, self.select.select_mode_filter, direction);
                self.reload_select_items();
            }
            12 => {
                const VALUES: [SelectSort; 4] =
                    [SelectSort::Level, SelectSort::Title, SelectSort::Clear, SelectSort::Score];
                if !VALUES.contains(&self.select.select_sort) {
                    self.open_advanced_settings_from_select();
                    return;
                }
                self.select.select_sort = cycle_enum(VALUES, self.select.select_sort, direction);
                self.boot.profile_config.select.sort = self.select.select_sort.as_str().to_string();
                self.reload_select_items();
            }
            1 => {
                let next = if self.select.select_option_panel == 1 { 0 } else { 1 };
                transition_select_option_panel(
                    &mut self.select.select_option_panel,
                    &mut self.select.option_panel_started_at,
                    &mut self.select.option_panel_off_started_at,
                    &mut self.select.select_exit_hold_started_at,
                    next,
                    Instant::now(),
                );
            }
            40 => {
                const VALUES: [GaugeTypeConfig; 4] = [
                    GaugeTypeConfig::Normal,
                    GaugeTypeConfig::Hard,
                    GaugeTypeConfig::Hazard,
                    GaugeTypeConfig::Easy,
                ];
                if VALUES.contains(&self.select.gauge_option) {
                    self.select.gauge_option =
                        cycle_enum(VALUES, self.select.gauge_option, direction);
                } else {
                    self.open_advanced_settings_from_select();
                }
            }
            42 | 43 => {
                const VALUES: [ArrangeOption; 4] = [
                    ArrangeOption::Normal,
                    ArrangeOption::Mirror,
                    ArrangeOption::Random,
                    ArrangeOption::SRandom,
                ];
                let current = if button == 42 {
                    self.select.arrange_option
                } else {
                    self.select.arrange_option_2p
                };
                if !VALUES.contains(&current) {
                    self.open_advanced_settings_from_select();
                    return;
                }
                let next = cycle_enum(VALUES, current, direction);
                if button == 42 {
                    self.select.arrange_option = next;
                } else {
                    self.select.arrange_option_2p = next;
                }
            }
            54 => {
                if matches!(self.select.double_option, DoubleOption::Off | DoubleOption::Flip) {
                    self.select.double_option = if self.select.double_option == DoubleOption::Off {
                        DoubleOption::Flip
                    } else {
                        DoubleOption::Off
                    };
                } else {
                    self.open_advanced_settings_from_select();
                }
            }
            55 => {
                const VALUES: [HsFixOption; 4] = [
                    HsFixOption::Off,
                    HsFixOption::MaxBpm,
                    HsFixOption::MinBpm,
                    HsFixOption::MainBpm,
                ];
                if !VALUES.contains(&self.select.hs_fix_option) {
                    self.open_advanced_settings_from_select();
                    return;
                }
                if !self.begin_selected_play_mode_edit() {
                    return;
                }
                self.select.hs_fix_option =
                    cycle_enum(VALUES, self.select.hs_fix_option, direction);
                self.boot.profile_config.play.hs_fix =
                    hs_fix_config_from_option(self.select.hs_fix_option);
                self.finish_selected_play_mode_edit();
            }
            10 | 13..=17 | 19 | 57 | 72 | 74 => {
                self.execute_select_skin_event(button, direction);
                return;
            }
            // The remaining LR2 panels/settings do not have identical BMZ semantics.
            // Keep their current values and provide the common settings entry point.
            _ => self.open_advanced_settings_from_select(),
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
