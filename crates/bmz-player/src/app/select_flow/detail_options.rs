use super::*;
use crate::select_detail_options::{CATALOG, DetailContext, DetailEffects, DetailOptionDef};
use bmz_render::scene::detail_options::{DetailOptionsSnapshot, detail_options_viewport};
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct DetailOptionsState {
    pub cursor: usize,
    pub value_latched: bool,
    pub blocked_controls: std::collections::HashSet<(DeviceId, String)>,
    pub dirty: bool,
    cache: RefCell<Option<(DetailOptionsCacheKey, Arc<DetailOptionsSnapshot>)>>,
}

#[derive(PartialEq, Eq)]
struct DetailOptionsCacheKey {
    values: [i64; CATALOG.len()],
    context: DetailContext,
    cursor: usize,
    locale: crate::i18n::AppLocale,
    amounts: [u32; 3],
    active_mode: KeyMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailInput {
    Move(i32),
    Value(i32),
}

enum DetailValueEdit {
    Step(i32),
    Choice(usize),
}

impl DetailOptionsState {
    fn input_edge(
        &mut self,
        action: Option<DetailInput>,
        pressed: bool,
        repeat: bool,
        value_keys_held: bool,
    ) -> Option<DetailInput> {
        if !pressed {
            if !value_keys_held {
                self.value_latched = false;
            }
            return None;
        }
        if repeat {
            return None;
        }
        match action {
            Some(DetailInput::Value(_)) if self.value_latched => None,
            Some(DetailInput::Value(_)) => {
                self.value_latched = true;
                action
            }
            Some(DetailInput::Move(_)) => {
                self.value_latched = value_keys_held;
                action
            }
            None => None,
        }
    }
}

impl WinitApp {
    pub(super) fn execute_detail_options_event(&mut self, id: i32, arg: i32) {
        use bmz_render::skin::*;
        if !self.detail_options_active() {
            return;
        }
        if let Some((slot, choice, field)) = detail_options_choice_slot(id) {
            if field == 0 && choice < SKIN_DETAIL_OPTIONS_CHOICES {
                let index =
                    detail_options_viewport(self.select.detail_options.cursor, CATALOG.len())
                        + slot;
                if let Some(item) = CATALOG.get(index).copied()
                    && (choice as i64) < item.choices
                {
                    self.select.detail_options.cursor = index;
                    self.select.detail_options.value_latched = self.detail_value_keys_held();
                    self.apply_detail_setting(item, DetailValueEdit::Choice(choice));
                }
            }
            return;
        }
        match id {
            SKIN_EVENT_DETAIL_OPTIONS_PREVIOUS => self.move_detail_options(-1),
            SKIN_EVENT_DETAIL_OPTIONS_NEXT => self.move_detail_options(1),
            SKIN_EVENT_DETAIL_OPTIONS_DECREASE => self.change_detail_options_value(-1),
            SKIN_EVENT_DETAIL_OPTIONS_INCREASE => self.change_detail_options_value(1),
            _ => {
                let position = if id == SKIN_EVENT_DETAIL_OPTIONS_SELECT_ID {
                    CATALOG.iter().position(|item| item.id == i64::from(arg))
                } else if (SKIN_EVENT_DETAIL_OPTIONS_ROW_BASE..=SKIN_EVENT_DETAIL_OPTIONS_ROW_LAST)
                    .contains(&id)
                {
                    let start =
                        detail_options_viewport(self.select.detail_options.cursor, CATALOG.len());
                    let index = start + (id - SKIN_EVENT_DETAIL_OPTIONS_ROW_BASE) as usize;
                    (index < CATALOG.len()).then_some(index)
                } else {
                    None
                };
                if let Some(index) = position {
                    self.select.detail_options.cursor = index;
                    self.select.detail_options.value_latched = self.detail_value_keys_held();
                }
            }
        }
    }
    pub(super) fn detail_options_active(&self) -> bool {
        self.select.select_option_panel == 2
            && matches!(self.view_state(), AppViewState::Select)
            && self.ui.focused
            && !in_settings_stack(&self.select.folder_stack)
            && !self.select.search.is_active()
            && !self.select.ir_battle.active
            && !self.ui.egui.as_ref().is_some_and(|ui| ui.blocks_game_input(false))
    }

    fn detail_options_mode(&self) -> Option<KeyMode> {
        let source = match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => {
                row.chart.as_ref().and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            }
            Some(SelectItem::Course(row)) => row.common_key_mode,
            _ => self.select.select_mode_filter.key_mode(),
        }?;
        Some(effective_play_key_mode(source, self.boot.profile_config.play.key_mode_conversion))
    }

    pub(super) fn reset_detail_options_input(&mut self) {
        self.select.detail_options.value_latched = self.detail_value_keys_held();
        self.select.detail_options.blocked_controls = self
            .input
            .pressed_control_sources()
            .filter(|(_, control)| self.select.select_keys.e_action_for_control(control).is_none())
            .cloned()
            .collect();
        self.clear_select_hold();
        self.reset_select_analog_scroll();
        self.select.select_slider_dragging_type = None;
    }

    pub(super) fn detail_options_blocks_held_input(&mut self, event: &ControlInputEvent) -> bool {
        let Some(control) = event.name.as_ref() else {
            return false;
        };
        let source = (event.device, control.clone());
        if !event.pressed {
            self.select.detail_options.blocked_controls.remove(&source);
            if !self.detail_value_keys_held() {
                self.select.detail_options.value_latched = false;
            }
            false
        } else {
            matches!(self.view_state(), AppViewState::Select)
                && self.select.detail_options.blocked_controls.contains(&source)
        }
    }

    fn detail_input(&self, device: DeviceId, control: &str) -> Option<DetailInput> {
        if self.select.select_keys.e_action_for_control(control).is_some() {
            return None;
        }
        let nine_key = self.boot.profile_config.input.select_input_mode
            == crate::config::profile_config::SelectInputModeConfig::Key9;
        let direction = if device == W_KEYBOARD_DEVICE_ID {
            self.select.select_keys.detail_value_direction(control, nine_key)
        } else {
            let config = self.play_session_app_config();
            let slots = crate::input::gamepad::GamepadSlotMap::from_runtime_or_legacy(
                config.input.gamepad_slot_runtime_device_ids,
                config.input.gamepad_slot_gilrs_ids,
            );
            detail_gamepad_lane(&self.boot.profile_config.input, slots, device, control, nine_key)
                .and_then(|lane| detail_lane_direction(lane, nine_key))
        };
        if let Some(direction) = direction {
            return Some(DetailInput::Value(direction));
        }
        if self.select.select_keys.is_select_scratch_up(control) {
            return Some(DetailInput::Move(-1));
        }
        if self.select.select_keys.is_select_scratch_down(control) {
            return Some(DetailInput::Move(1));
        }
        if device == W_KEYBOARD_DEVICE_ID {
            match control {
                "ArrowUp" => Some(DetailInput::Move(-1)),
                "ArrowDown" => Some(DetailInput::Move(1)),
                "ArrowLeft" => Some(DetailInput::Value(-1)),
                "ArrowRight" => Some(DetailInput::Value(1)),
                _ => None,
            }
        } else {
            None
        }
    }

    fn detail_value_keys_held(&self) -> bool {
        self.input.pressed_control_sources().any(|(device, control)| {
            matches!(self.detail_input(*device, control), Some(DetailInput::Value(_)))
        })
    }

    /// Physical holds have already been tracked before this route. Consume the
    /// entire E2 input surface, including unbound shortcuts, to isolate the list.
    pub(super) fn route_detail_options_input(&mut self, event: &ControlInputEvent) -> bool {
        if !self.detail_options_active() {
            return false;
        }
        let Some(control) = event.name.as_deref() else {
            return true;
        };
        if self.select.select_keys.e_action_for_control(control).is_some() {
            return false;
        }
        let action = self.detail_input(event.device, control).filter(|action| {
            !matches!(action, DetailInput::Move(_)) || !control.starts_with("Axis")
        });
        let held = self.detail_value_keys_held();
        match self.select.detail_options.input_edge(action, event.pressed, event.repeat, held) {
            Some(DetailInput::Move(direction)) => self.move_detail_options(direction),
            Some(DetailInput::Value(direction)) => self.change_detail_options_value(direction),
            _ => {}
        }
        true
    }

    pub(super) fn move_detail_options(&mut self, direction: i32) {
        if !self.detail_options_active() || direction == 0 {
            return;
        }
        self.select.detail_options.cursor = (self.select.detail_options.cursor as i32 + direction)
            .rem_euclid(CATALOG.len() as i32) as usize;
        self.select.detail_options.value_latched = self.detail_value_keys_held();
        self.play_system_sound(crate::system_sound::SoundType::Scratch);
    }

    pub(super) fn change_detail_options_value(&mut self, direction: i32) {
        if !self.detail_options_active() {
            return;
        }
        let item = CATALOG[self.select.detail_options.cursor];
        self.adjust_detail_setting(item, direction);
    }

    pub(super) fn adjust_detail_setting(&mut self, item: DetailOptionDef, direction: i32) -> bool {
        self.apply_detail_setting(item, DetailValueEdit::Step(direction))
    }

    fn apply_detail_setting(&mut self, item: DetailOptionDef, edit: DetailValueEdit) -> bool {
        let mode = self.detail_options_mode();
        let before = SelectScoreContext::from_profile(&self.boot.profile_config);
        let changed = match edit {
            DetailValueEdit::Step(direction) => {
                item.adjust(&mut self.boot.profile_config, mode, direction)
            }
            DetailValueEdit::Choice(index) => {
                item.select_choice(&mut self.boot.profile_config, mode, index)
            }
        };
        if !changed {
            return false;
        }
        // Only synchronize the setting that changed; E1 options may have newer
        // transient values than the saved profile.
        if item.effects == DetailEffects::GaugeBottom {
            self.select.bottom_shiftable_gauge_option =
                self.boot.profile_config.play.bottom_shiftable_gauge;
        }
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.select.detail_options.dirty = true;
        self.invalidate_play_preload();
        if matches!(item.effects, DetailEffects::Assist | DetailEffects::ScoreContext) {
            self.play.play_media_cache = None;
        }
        if item.effects == DetailEffects::ScoreContext {
            self.sync_changed_select_score_context(before);
        }
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        true
    }

    pub(super) fn save_detail_options_if_dirty(&mut self) {
        if !self.select.detail_options.dirty {
            return;
        }
        if self.detail_options_mode().is_none() {
            // A common setting is still editable on an unresolved course/folder.
            // The normal save path falls back to 7K and merges HS-FIX; avoid
            // writing that unrelated mode merely to persist this common value.
            match save_profile_config(
                &self.boot.profile_paths.profile_toml,
                &self.boot.profile_config,
            ) {
                Ok(()) => self.select.detail_options.dirty = false,
                Err(error) => tracing::error!(%error, "failed to save detail options"),
            }
            return;
        }
        // Existing save merges current E1/GAS state and syncs the active mode.
        self.save_current_play_options(None, "detail options closed");
        self.select.detail_options.dirty = false;
    }

    pub(super) fn detail_options_snapshot(&self) -> Option<Arc<DetailOptionsSnapshot>> {
        if !self.detail_options_active() {
            return None;
        }
        let p = &self.boot.profile_config;
        let mode = self.detail_options_mode();
        let course = matches!(
            self.select.select_items.get(self.select.selected_index),
            Some(SelectItem::Course(_))
        );
        let context = DetailContext {
            mode,
            gas: self.select.gauge_auto_shift_option,
            practice: self.select.session_mode.is_practice(),
            course,
        };
        let key = DetailOptionsCacheKey {
            values: std::array::from_fn(|index| CATALOG[index].value(p)),
            context,
            cursor: self.select.detail_options.cursor,
            locale: p.ui.locale(),
            amounts: [p.lane.sudden, p.lane.hidden, p.lane.lift],
            active_mode: p.active_play_mode,
        };
        let mut cache = self.select.detail_options.cache.borrow_mut();
        if let Some((previous, snapshot)) = cache.as_ref()
            && *previous == key
        {
            return Some(snapshot.clone());
        }
        let text = Localizer::new(p.ui.locale());
        let items: Arc<[_]> = CATALOG.iter().map(|item| item.row(p, context, &text)).collect();
        let cursor = self.select.detail_options.cursor;
        let selected = &items[cursor];
        let scope_label = if selected.scope == 0 {
            text.text("detail-options-scope-global")
        } else {
            mode.map(|m| m.as_str().to_string())
                .unwrap_or_else(|| text.text("detail-options-scope-unknown"))
        };
        let snapshot = Arc::new(DetailOptionsSnapshot {
            cursor,
            viewport_start: detail_options_viewport(cursor, items.len()),
            items,
            title: text.text("detail-options-title"),
            scope_label,
            guide: text.text("detail-options-guide"),
            position: format!("{} / {}", cursor + 1, CATALOG.len()),
        });
        *cache = Some((key, snapshot.clone()));
        Some(snapshot)
    }
}

fn detail_gamepad_lane(
    input: &ProfileInputConfig,
    slots: crate::input::gamepad::GamepadSlotMap,
    device: DeviceId,
    control: &str,
    nine_key: bool,
) -> Option<Lane> {
    crate::config::play::lane_binding_for_chart_with_slots(
        input,
        if nine_key { KeyMode::K9 } else { KeyMode::K14 },
        slots,
    )
    .resolve(device, &PhysicalControl::GamepadButton(control.to_string()))
}

fn detail_lane_direction(lane: Lane, nine_key: bool) -> Option<i32> {
    let index = match lane {
        Lane::Key1 => 0,
        Lane::Key2 => 1,
        Lane::Key3 => 2,
        Lane::Key4 => 3,
        Lane::Key5 => 4,
        Lane::Key6 => 5,
        Lane::Key7 => 6,
        Lane::Key8 => 7,
        Lane::Key9 => 8,
        Lane::Key10 => 9,
        Lane::Key11 => 10,
        Lane::Key12 => 11,
        Lane::Key13 => 12,
        Lane::Key14 => 13,
        _ => return None,
    };
    let local = if nine_key { index } else { index % 7 };
    Some(if local % 2 == 0 { 1 } else { -1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_value_chords_repeats_and_opposite_keys_use_first_edge() {
        let mut state = DetailOptionsState::default();
        assert_eq!(
            state.input_edge(Some(DetailInput::Value(1)), true, false, true),
            Some(DetailInput::Value(1))
        );
        for (direction, repeat) in [(1, false), (1, true), (-1, false), (-1, true)] {
            assert_eq!(
                state.input_edge(Some(DetailInput::Value(direction)), true, repeat, true),
                None
            );
        }
        state.input_edge(None, false, false, true); // one key remains down
        assert_eq!(state.input_edge(Some(DetailInput::Value(-1)), true, false, true), None);
        state.input_edge(None, false, false, false);
        assert_eq!(
            state.input_edge(Some(DetailInput::Value(-1)), true, false, true),
            Some(DetailInput::Value(-1))
        );
    }

    #[test]
    fn detail_item_moves_and_panel_transitions_require_repress() {
        let mut state = DetailOptionsState { value_latched: true, ..Default::default() };
        // A key held before E2 opened cannot edit the first row.
        assert_eq!(state.input_edge(Some(DetailInput::Value(1)), true, true, true), None);
        assert_eq!(
            state.input_edge(Some(DetailInput::Move(1)), true, false, true),
            Some(DetailInput::Move(1))
        );
        assert_eq!(state.input_edge(Some(DetailInput::Value(1)), true, false, true), None);
        state.input_edge(None, false, false, false);
        assert_eq!(
            state.input_edge(Some(DetailInput::Value(1)), true, false, true),
            Some(DetailInput::Value(1))
        );
        // Focus loss / all release clears the guard without resetting cursor.
        state.cursor = 12;
        state.input_edge(None, false, false, false);
        assert!(!state.value_latched);
        assert_eq!(state.cursor, 12);
    }

    #[test]
    fn detail_key_parity_is_local_to_each_player_side() {
        for (left, right, direction) in [
            (Lane::Key1, Lane::Key8, 1),
            (Lane::Key2, Lane::Key9, -1),
            (Lane::Key3, Lane::Key10, 1),
            (Lane::Key4, Lane::Key11, -1),
            (Lane::Key5, Lane::Key12, 1),
            (Lane::Key6, Lane::Key13, -1),
            (Lane::Key7, Lane::Key14, 1),
        ] {
            assert_eq!(detail_lane_direction(left, false), Some(direction));
            assert_eq!(detail_lane_direction(right, false), Some(direction));
        }
        assert_eq!(detail_lane_direction(Lane::Key8, true), Some(-1));
        assert_eq!(detail_lane_direction(Lane::Key9, true), Some(1));
        assert_eq!(detail_lane_direction(Lane::Scratch, false), None);
    }

    #[test]
    fn detail_keyboard_9k_keeps_key_priority_over_list_navigation() {
        let mut profile = ProfileConfig::new_default("test", "Test", 0);
        profile.input.select_input_mode =
            crate::config::profile_config::SelectInputModeConfig::Key9;
        let bindings = SelectKeyBindings::from_profile(&profile.input);
        // 9K list navigation normally uses KEY4/KEY6; E2 edits via even keys.
        for entry in
            crate::config::play_input::resolve_play_bindings(&profile.input, KeyMode::K9).unwrap()
        {
            if entry.device == "keyboard"
                && matches!(
                    entry.lane,
                    Some(
                        crate::config::profile_config::LaneConfig::Key4
                            | crate::config::profile_config::LaneConfig::Key6
                    )
                )
            {
                assert_eq!(bindings.detail_value_direction(&entry.control, true), Some(-1));
            }
        }
        assert_eq!(bindings.detail_value_direction("ArrowUp", true), None);
    }

    #[test]
    fn detail_gamepad_slots_preserve_local_parity() {
        let p = ProfileConfig::new_default("test", "Test", 0);
        let slots = crate::input::gamepad::GamepadSlotMap::from_slot_ids([Some(0), Some(1)]);
        for (device, expected) in [(DeviceId(16), Lane::Key1), (DeviceId(17), Lane::Key8)] {
            let lane = detail_gamepad_lane(&p.input, slots, device, "Button1", false).unwrap();
            assert_eq!(lane, expected);
            assert_eq!(detail_lane_direction(lane, false), Some(1));
        }
    }
}
