//! E2 catalog and registry adapter. No input or rendering code owns settings.
use bmz_core::lane::KeyMode;
use bmz_render::scene::detail_options::{DetailOptionChoice, DetailOptionRow, DetailValueKind};

use crate::config::profile_config::*;
use crate::config::settings_registry::{
    SettingsEntryId, adjust_settings_value, format_settings_value,
};
use crate::i18n::Localizer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailContext {
    pub mode: Option<KeyMode>,
    pub gas: GaugeAutoShiftConfig,
    pub practice: bool,
    pub course: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct DetailOptionDef {
    pub id: i64,
    pub key: &'static str,
    pub category: i64,
    pub setting: SettingsEntryId,
    pub mode_scoped: bool,
    pub kind: DetailValueKind,
    pub choices: i64,
    pub effects: DetailEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailEffects {
    Lane,
    GaugeBottom,
    ScoreContext,
    Assist,
}

const fn item(
    id: i64,
    key: &'static str,
    category: i64,
    setting: SettingsEntryId,
    mode_scoped: bool,
    choices: i64,
) -> DetailOptionDef {
    DetailOptionDef {
        id,
        key,
        category,
        setting,
        mode_scoped,
        kind: if choices == 0 { DetailValueKind::Bool } else { DetailValueKind::Enum },
        choices: if choices == 0 { 2 } else { choices },
        effects: match setting {
            SettingsEntryId::BottomShiftableGauge => DetailEffects::GaugeBottom,
            SettingsEntryId::LnModePolicy => DetailEffects::ScoreContext,
            _ if mode_scoped => DetailEffects::Lane,
            _ => DetailEffects::Assist,
        },
    }
}

pub const CATALOG: &[DetailOptionDef] = &[
    item(101, "sudden", 1, SettingsEntryId::SuddenEnabled, true, 0),
    item(102, "hidden", 1, SettingsEntryId::HiddenEnabled, true, 0),
    item(103, "lift", 1, SettingsEntryId::LiftEnabled, true, 0),
    item(201, "gas-bottom", 2, SettingsEntryId::BottomShiftableGauge, false, 3),
    item(301, "hs-auto", 3, SettingsEntryId::HispeedAutoAdjust, true, 0),
    item(302, "hs-config", 3, SettingsEntryId::HispeedMode, true, 5),
    item(303, "constant", 3, SettingsEntryId::Constant, true, 0),
    item(401, "ln-mode", 4, SettingsEntryId::LnModePolicy, false, 6),
    item(501, "scroll-modifier", 5, SettingsEntryId::AssistScrollMode, false, 2),
    item(502, "ln-modifier", 5, SettingsEntryId::AssistLongNoteMode, false, 2),
    item(503, "mine-modifier", 5, SettingsEntryId::AssistMineMode, false, 2),
    item(504, "expand-judge", 5, SettingsEntryId::AssistExpandJudge, false, 0),
    item(505, "judge-area", 5, SettingsEntryId::AssistJudgeArea, false, 0),
    item(506, "mark-note", 5, SettingsEntryId::AssistMarkNote, false, 0),
    item(507, "bpm-guide", 5, SettingsEntryId::AssistBpmGuide, false, 0),
];

/// Pure stepping contract also used by future numeric rows. Unknown enum values
/// enter the offered set at the first/last choice, without modifying on read.
pub fn stepped_value(kind: DetailValueKind, value: i64, choices: i64, direction: i32) -> i64 {
    if direction == 0 {
        return value;
    }
    match kind {
        DetailValueKind::Number { min, max, step } => {
            value.saturating_add(step.saturating_mul(i64::from(direction.signum()))).clamp(min, max)
        }
        DetailValueKind::Bool | DetailValueKind::Enum => {
            if !(0..choices).contains(&value) {
                if direction > 0 { 0 } else { choices - 1 }
            } else {
                (value + i64::from(direction.signum())).rem_euclid(choices)
            }
        }
    }
}

impl DetailOptionDef {
    pub fn for_setting(setting: SettingsEntryId) -> Option<Self> {
        CATALOG.iter().find(|item| item.setting == setting).copied()
    }
    pub fn value(self, p: &ProfileConfig) -> i64 {
        use SettingsEntryId::*;
        match self.setting {
            SuddenEnabled => i64::from(p.play.lane_effect.sudden_enabled()),
            HiddenEnabled => i64::from(p.play.lane_effect.hidden_enabled()),
            LiftEnabled => i64::from(p.lane.lift_enabled),
            BottomShiftableGauge => match p.play.bottom_shiftable_gauge {
                BottomShiftableGaugeConfig::AssistEasy => 0,
                BottomShiftableGaugeConfig::Easy => 1,
                BottomShiftableGaugeConfig::Normal => 2,
            },
            HispeedAutoAdjust => i64::from(p.lane.hispeed_auto_adjust),
            HispeedMode => i64::from(p.lane.hispeed_config().index()),
            Constant => i64::from(p.lane.constant_enabled),
            LnModePolicy => {
                crate::skin_extension::ln_policy_setting_index(p.play.ln_mode_policy) as i64
            }
            AssistScrollMode => match p.play.assist.scroll_mode {
                crate::config::profile_config::AssistScrollMode::Off => 0,
                crate::config::profile_config::AssistScrollMode::Remove => 1,
                crate::config::profile_config::AssistScrollMode::Add => 2,
            },
            AssistLongNoteMode => match p.play.assist.long_note_mode {
                crate::config::profile_config::AssistLongNoteMode::Off => 0,
                crate::config::profile_config::AssistLongNoteMode::Remove => 1,
                crate::config::profile_config::AssistLongNoteMode::AddLn => 2,
                crate::config::profile_config::AssistLongNoteMode::AddCn => 3,
                crate::config::profile_config::AssistLongNoteMode::AddHcn => 4,
                crate::config::profile_config::AssistLongNoteMode::AddAll => 5,
            },
            AssistMineMode => match p.play.assist.mine_mode {
                crate::config::profile_config::AssistMineMode::Off => 0,
                crate::config::profile_config::AssistMineMode::Remove => 1,
                crate::config::profile_config::AssistMineMode::AddRandom => 2,
                crate::config::profile_config::AssistMineMode::AddNear => 3,
                crate::config::profile_config::AssistMineMode::AddBlank => 4,
            },
            AssistExpandJudge => i64::from(p.play.assist.expand_judge),
            AssistJudgeArea => i64::from(p.play.assist.judge_area),
            AssistMarkNote => i64::from(p.play.assist.mark_note),
            AssistBpmGuide => i64::from(p.play.assist.bpm_guide),
            _ => unreachable!("catalog entry needs a value reader"),
        }
    }

    pub fn adjust(self, p: &mut ProfileConfig, mode: Option<KeyMode>, direction: i32) -> bool {
        if direction == 0 || (self.mode_scoped && mode.is_none()) {
            return false;
        }
        if self.mode_scoped {
            p.activate_play_mode(mode.expect("checked mode"));
        }
        let current = self.value(p);
        let next = stepped_value(self.kind, current, self.choices, direction);
        self.set_value(p, next, direction)
    }

    pub fn select_choice(self, p: &mut ProfileConfig, mode: Option<KeyMode>, index: usize) -> bool {
        if index >= self.choices as usize
            || matches!(self.kind, DetailValueKind::Number { .. })
            || (self.mode_scoped && mode.is_none())
        {
            return false;
        }
        if self.mode_scoped {
            p.activate_play_mode(mode.expect("checked mode"));
        }
        self.set_value(p, index as i64, 1)
    }

    fn set_value(self, p: &mut ProfileConfig, next: i64, direction: i32) -> bool {
        if self.value(p) == next {
            return false;
        }
        // Use the same registry mutation as the settings screen; skip ADD values
        // that remain available through their existing configuration paths.
        for _ in 0..8 {
            if !adjust_settings_value(p, self.setting, direction.signum()) {
                return false;
            }
            if self.value(p) == next {
                if self.mode_scoped {
                    p.sync_active_play_mode();
                }
                return true;
            }
        }
        unreachable!("registry cycle must contain the catalog choice")
    }

    fn choice_label(self, index: usize, text: &Localizer) -> String {
        use crate::config::settings_registry::{
            format_bottom_shiftable_gauge, format_hispeed_mode,
        };
        if self.kind == DetailValueKind::Bool {
            return text.text(if index == 0 { "detail-options-off" } else { "detail-options-on" });
        }
        match self.setting {
            SettingsEntryId::BottomShiftableGauge => format_bottom_shiftable_gauge(
                [
                    BottomShiftableGaugeConfig::AssistEasy,
                    BottomShiftableGaugeConfig::Easy,
                    BottomShiftableGaugeConfig::Normal,
                ][index],
            ),
            SettingsEntryId::HispeedMode => format_hispeed_mode(HispeedConfigPreset::ORDER[index]),
            SettingsEntryId::LnModePolicy => {
                crate::ln_policy::LnPolicySetting::ORDER[index].display_label().to_string()
            }
            SettingsEntryId::AssistScrollMode
            | SettingsEntryId::AssistLongNoteMode
            | SettingsEntryId::AssistMineMode => {
                text.text(if index == 0 { "detail-options-off" } else { "detail-options-remove" })
            }
            _ => unreachable!("catalog enum needs choice labels"),
        }
    }

    pub fn row(
        self,
        p: &ProfileConfig,
        context: DetailContext,
        text: &Localizer,
    ) -> DetailOptionRow {
        let DetailContext { mode, gas, practice, course } = context;
        let editable = !self.mode_scoped || mode.is_some();
        let reason_key = if !editable {
            Some("detail-options-no-mode")
        } else if self.setting == SettingsEntryId::BottomShiftableGauge && course {
            Some("detail-options-gas-course")
        } else if self.setting == SettingsEntryId::BottomShiftableGauge
            && !matches!(gas, GaugeAutoShiftConfig::BestClear | GaugeAutoShiftConfig::SelectToUnder)
        {
            Some("detail-options-gas-inactive")
        } else if self.setting == SettingsEntryId::HispeedAutoAdjust
            && !p.lane.hispeed_config().supports_floating()
        {
            Some("detail-options-hs-inactive")
        } else if self.setting == SettingsEntryId::Constant && practice {
            Some("detail-options-practice-inactive")
        } else {
            None
        };
        let value = if editable { self.value(p) } else { -1 };
        let value_label = if !editable {
            text.text("detail-options-unavailable")
        } else if self.kind == DetailValueKind::Bool {
            text.text(if value == 0 { "detail-options-off" } else { "detail-options-on" })
        } else if matches!(
            self.setting,
            SettingsEntryId::AssistScrollMode
                | SettingsEntryId::AssistLongNoteMode
                | SettingsEntryId::AssistMineMode
        ) && value < 2
        {
            text.text(if value == 0 { "detail-options-off" } else { "detail-options-remove" })
        } else {
            format_settings_value(p, self.setting)
        };
        let auxiliary = match self.setting {
            SettingsEntryId::SuddenEnabled
            | SettingsEntryId::HiddenEnabled
            | SettingsEntryId::LiftEnabled
                if editable =>
            {
                let amount = match self.setting {
                    SettingsEntryId::SuddenEnabled => p.lane.sudden,
                    SettingsEntryId::HiddenEnabled => p.lane.hidden,
                    _ => p.lane.lift,
                };
                format!("{}: {amount} / 1000", text.text("detail-options-saved-amount"))
            }
            SettingsEntryId::BottomShiftableGauge => format!(
                "GAS: {}",
                match gas {
                    GaugeAutoShiftConfig::Off => "OFF",
                    GaugeAutoShiftConfig::Continue => "CONTINUE",
                    GaugeAutoShiftConfig::HardToGroove => "HARD TO GROOVE",
                    GaugeAutoShiftConfig::BestClear => "BEST CLEAR",
                    GaugeAutoShiftConfig::SelectToUnder => "SELECT TO UNDER",
                }
            ),
            _ => String::new(),
        };
        let reason = if editable && value >= self.choices {
            text.text("detail-options-external-value")
        } else {
            reason_key.map(|key| text.text(key)).unwrap_or_default()
        };
        DetailOptionRow {
            item_id: self.id,
            category_id: self.category,
            scope: if self.mode_scoped {
                mode.map(|m| m.as_str().trim_end_matches('K').parse().unwrap_or(-1)).unwrap_or(-1)
            } else {
                0
            },
            value,
            value_index: if (0..self.choices).contains(&value) { value } else { -1 },
            choice_count: self.choices,
            choices: (0..self.choices as usize)
                .map(|index| DetailOptionChoice {
                    value: index as i64,
                    label: self.choice_label(index, text),
                })
                .collect(),
            kind: self.kind,
            label: text.text(&format!("detail-options-item-{}", self.key)),
            value_label,
            category: text.text(&format!("detail-options-category-{}", self.category)),
            description: text.text(match self.setting {
                SettingsEntryId::AssistScrollMode => "detail-options-scroll-description",
                SettingsEntryId::AssistLongNoteMode => "detail-options-ln-description",
                SettingsEntryId::AssistMineMode => "detail-options-mine-description",
                _ => self.setting.description_key(),
            }),
            reason,
            auxiliary,
            status: if !editable {
                text.text("detail-options-status-locked")
            } else if reason_key.is_some() {
                text.text("detail-options-status-inactive")
            } else {
                String::new()
            },
            editable,
            effective: reason_key.is_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::AppLocale;

    fn profile() -> ProfileConfig {
        let mut p = ProfileConfig::new_default("detail-test", "Test", 0);
        p.normalize_play_mode_configs();
        p
    }

    #[test]
    fn catalog_ids_are_unique_and_independent_of_order() {
        assert!(CATALOG.len() > 7);
        assert_eq!(
            CATALOG[..4].iter().map(|item| item.id).collect::<Vec<_>>(),
            [101, 102, 103, 201]
        );
        let ids: std::collections::HashSet<_> = CATALOG.iter().map(|item| item.id).collect();
        assert_eq!(ids.len(), CATALOG.len());
        for item in CATALOG {
            assert_eq!(DetailOptionDef::for_setting(item.setting).unwrap().id, item.id);
            let mut p = profile();
            let before = item.value(&p);
            assert!(item.adjust(&mut p, Some(KeyMode::K7), 1), "{}", item.key);
            assert_ne!(item.value(&p), before);
            assert!(item.adjust(&mut p, Some(KeyMode::K7), -1));
            assert_eq!(item.value(&p), before);
            for _ in 0..item.choices {
                assert!(item.adjust(&mut p, Some(KeyMode::K7), 1));
            }
            assert_eq!(item.value(&p), before);
        }
    }

    #[test]
    fn numeric_values_clamp_and_boundaries_are_noops() {
        let kind = DetailValueKind::Number { min: 0, max: 100, step: 5 };
        assert_eq!(stepped_value(kind, 98, 0, 1), 100);
        assert_eq!(stepped_value(kind, 100, 0, 1), 100);
        assert_eq!(stepped_value(kind, 0, 0, -1), 0);
        assert_eq!(stepped_value(kind, 3, 0, -1), 0);
        assert_eq!(stepped_value(kind, 50, 0, 0), 50);
    }

    #[test]
    fn independent_lane_toggles_preserve_amount_and_other_modes() {
        let mut p = profile();
        let k7 = p.play_mode_config(KeyMode::K7);
        p.activate_play_mode(KeyMode::K14);
        p.play.lane_effect = LaneEffectConfig::Off;
        p.lane.sudden = 300;
        p.lane.hidden = 0;
        p.lane.lift = 120;
        p.lane.lift_enabled = false;
        for item in &CATALOG[..3] {
            item.adjust(&mut p, Some(KeyMode::K14), 1);
        }
        assert!(p.play.lane_effect.sudden_enabled());
        assert!(p.play.lane_effect.hidden_enabled());
        assert!(p.lane.lift_enabled);
        assert_eq!(
            CATALOG[1]
                .row(
                    &p,
                    DetailContext {
                        mode: Some(KeyMode::K14),
                        gas: GaugeAutoShiftConfig::Off,
                        practice: false,
                        course: false
                    },
                    &Localizer::new(AppLocale::En)
                )
                .value_label,
            "ON"
        );
        CATALOG[0].adjust(&mut p, Some(KeyMode::K14), -1);
        assert!(p.play.lane_effect.hidden_enabled());
        CATALOG[1].adjust(&mut p, Some(KeyMode::K14), -1);
        CATALOG[2].adjust(&mut p, Some(KeyMode::K14), -1);
        assert_eq!((p.lane.sudden, p.lane.hidden, p.lane.lift), (300, 0, 120));
        assert_eq!(p.play_mode_config(KeyMode::K7), k7);
        let saved = toml::to_string(&p).unwrap();
        let loaded: ProfileConfig = toml::from_str(&saved).unwrap();
        assert_eq!(loaded.play_mode_config(KeyMode::K14), p.play_mode_config(KeyMode::K14));
    }

    #[test]
    fn unresolved_mode_locks_only_mode_settings() {
        let mut p = profile();
        let before = p.play_mode_config(KeyMode::K7);
        let text = Localizer::new(AppLocale::Ja);
        for item in CATALOG.iter().filter(|item| item.mode_scoped) {
            assert!(!item.adjust(&mut p, None, 1));
            let row = item.row(
                &p,
                DetailContext {
                    mode: None,
                    gas: GaugeAutoShiftConfig::Off,
                    practice: false,
                    course: false,
                },
                &text,
            );
            assert!(!row.editable);
            assert!(!row.reason.is_empty());
            assert_eq!(row.value, -1);
        }
        assert_eq!(p.play_mode_config(KeyMode::K7), before);
        assert!(CATALOG[3].adjust(&mut p, None, 1));
        let loaded: ProfileConfig = toml::from_str(&toml::to_string(&p).unwrap()).unwrap();
        assert_eq!(loaded.play_mode_config(KeyMode::K7), before);
        assert_eq!(loaded.play.bottom_shiftable_gauge, BottomShiftableGaugeConfig::Easy);
    }

    #[test]
    fn gas_bottom_is_editable_while_off_and_conditionally_applies() {
        let mut p = profile();
        let item = CATALOG[3];
        assert!(item.adjust(&mut p, None, 1));
        assert_eq!(p.play.bottom_shiftable_gauge, BottomShiftableGaugeConfig::Easy);
        for (gas, effective) in [
            (GaugeAutoShiftConfig::Off, false),
            (GaugeAutoShiftConfig::Continue, false),
            (GaugeAutoShiftConfig::HardToGroove, false),
            (GaugeAutoShiftConfig::BestClear, true),
            (GaugeAutoShiftConfig::SelectToUnder, true),
        ] {
            let row = item.row(
                &p,
                DetailContext { mode: None, gas, practice: false, course: false },
                &Localizer::new(AppLocale::En),
            );
            assert!(row.editable);
            assert_eq!(row.effective, effective);
        }
    }

    #[test]
    fn similarly_named_options_and_external_add_values_stay_distinct() {
        let mut p = profile();
        let ln_mode = p.play.ln_mode_policy;
        CATALOG[9].adjust(&mut p, None, 1);
        assert_eq!(p.play.ln_mode_policy, ln_mode);
        assert_eq!(p.play.assist.long_note_mode, AssistLongNoteMode::Remove);
        CATALOG[6].adjust(&mut p, Some(KeyMode::K7), 1);
        assert!(p.lane.constant_enabled);
        assert_eq!(p.play.assist.scroll_mode, AssistScrollMode::Off);
        p.play.assist.scroll_mode = AssistScrollMode::Add;
        let row = CATALOG[8].row(
            &p,
            DetailContext {
                mode: None,
                gas: GaugeAutoShiftConfig::Off,
                practice: false,
                course: false,
            },
            &Localizer::new(AppLocale::En),
        );
        assert_eq!(row.value_label, "ADD");
        assert_eq!(row.value_index, -1);
        CATALOG[8].adjust(&mut p, None, 1);
        assert_eq!(p.play.assist.scroll_mode, AssistScrollMode::Off);
        CATALOG[8].adjust(&mut p, None, -1);
        assert_eq!(p.play.assist.scroll_mode, AssistScrollMode::Remove);
    }

    #[test]
    fn catalog_and_settings_registry_produce_identical_changes() {
        for item in CATALOG {
            let mut panel = profile();
            let mut settings = panel.clone();
            item.adjust(&mut panel, Some(KeyMode::K7), 1);
            adjust_settings_value(&mut settings, item.setting, 1);
            settings.sync_active_play_mode();
            assert_eq!(item.value(&panel), item.value(&settings), "{}", item.key);
            assert_eq!(panel.play_mode_config(KeyMode::K7), settings.play_mode_config(KeyMode::K7));
        }
    }

    #[test]
    fn detail_choice_labels_and_direct_selection_match_current_values() {
        let text = Localizer::new(AppLocale::Ja);
        let context = DetailContext {
            mode: Some(KeyMode::K7),
            gas: GaugeAutoShiftConfig::Off,
            practice: false,
            course: false,
        };
        for item in CATALOG {
            assert!(item.choices as usize <= bmz_render::skin::SKIN_DETAIL_OPTIONS_CHOICES);
            let mut p = profile();
            for index in 0..item.choices as usize {
                item.select_choice(&mut p, context.mode, index);
                let row = item.row(&p, context, &text);
                assert_eq!(row.choices.len(), item.choices as usize);
                assert_eq!(row.value_index, index as i64);
                assert_eq!(row.value, row.choices[index].value);
                assert_eq!(row.value_label, row.choices[index].label);
                assert!(!item.select_choice(&mut p, context.mode, index));
            }
            assert!(!item.select_choice(&mut p, context.mode, item.choices as usize));
            assert!(!item.select_choice(&mut p, context.mode, usize::MAX));
        }
        let mut p = profile();
        assert!(!CATALOG[0].select_choice(&mut p, None, 1));
        p.play.assist.scroll_mode = AssistScrollMode::Add;
        assert!(CATALOG[8].select_choice(&mut p, None, 1));
        assert_eq!(p.play.assist.scroll_mode, AssistScrollMode::Remove);
    }

    #[test]
    fn legacy_assist_events_keep_their_fixed_meaning() {
        for (event, id) in
            [(301, 504), (302, 501), (303, 505), (304, 502), (305, 506), (306, 507), (307, 503)]
        {
            let mut panel = profile();
            let mut legacy = panel.clone();
            let item = CATALOG.iter().find(|item| item.id == id).unwrap();
            for _ in 0..2 {
                assert!(legacy.play.assist.toggle_beatoraja_button(event));
                assert!(item.adjust(&mut panel, None, 1));
                assert_eq!(legacy.play.assist, panel.play.assist);
            }
        }
    }

    #[test]
    fn course_gas_floor_remains_editable_without_claiming_effect() {
        let p = profile();
        let row = CATALOG[3].row(
            &p,
            DetailContext {
                mode: Some(KeyMode::K7),
                gas: GaugeAutoShiftConfig::BestClear,
                practice: false,
                course: true,
            },
            &Localizer::new(AppLocale::En),
        );
        assert!(row.editable);
        assert!(!row.effective);
        assert!(row.reason.contains("CLASS"));
    }
}
