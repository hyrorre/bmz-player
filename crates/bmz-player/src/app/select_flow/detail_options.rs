use super::*;
use crate::select_detail_options::{CATALOG, DetailContext, DetailEffects, DetailOptionDef};
use bmz_render::scene::detail_options::{
    DETAIL_OPTIONS_CLOSE_MS, DetailOptionsClosingSnapshot, DetailOptionsSnapshot, DetailValueKind,
    detail_options_row_index, detail_options_viewport,
};
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct DetailOptionsState {
    pub cursor: usize,
    pub value_latched: bool,
    pub blocked_controls: std::collections::HashSet<(DeviceId, String)>,
    pub dirty: bool,
    pub closing: Option<DetailOptionsClosingSnapshot>,
    repeat_value: Option<(DeviceId, String, i32, Instant)>,
    scroll_started: Option<Instant>,
    scroll_duration: Duration,
    scroll_from: f32,
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
    fn repeat_step(&mut self, now: Instant, held: bool) -> Option<i32> {
        if !held {
            self.repeat_value = None;
        }
        let (_, _, direction, next) = self.repeat_value.as_mut()?;
        if now < *next {
            return None;
        }
        *next = now + Duration::from_millis(60);
        Some(*direction)
    }
    fn scroll_offset(&self, now: Instant) -> f32 {
        let Some(started) = self.scroll_started else { return 0.0 };
        if self.scroll_duration.is_zero() {
            return 0.0;
        }
        let elapsed = now.saturating_duration_since(started);
        self.scroll_from
            * (1.0 - elapsed.as_secs_f32() / self.scroll_duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn move_cursor(&mut self, direction: i32, now: Instant, low: Duration, high: Duration) {
        if direction == 0 {
            return;
        }
        self.repeat_value = None;
        let remaining = self.scroll_offset(now);
        self.cursor =
            (self.cursor as i64 + i64::from(direction)).rem_euclid(CATALOG.len() as i64) as usize;
        self.scroll_duration = if remaining == 0.0 { low } else { high };
        self.scroll_from = (remaining + direction as f32).clamp(-1.0, 1.0);
        self.scroll_started = Some(now);
    }

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
        if let Some((slot, direction)) = detail_options_numeric_event(id) {
            if let Some(index) =
                detail_options_row_index(self.select.detail_options.cursor, CATALOG.len(), slot)
                && matches!(
                    CATALOG[index].kind,
                    bmz_render::scene::detail_options::DetailValueKind::Number { .. }
                )
            {
                self.select_detail_options_index(index);
                self.change_detail_options_value(direction);
            }
            return;
        }
        if let Some((slot, choice, field)) = detail_options_choice_slot(id) {
            if field == 0
                && choice < SKIN_DETAIL_OPTIONS_CHOICES
                && let Some(index) =
                    detail_options_row_index(self.select.detail_options.cursor, CATALOG.len(), slot)
                && let Some(item) = CATALOG.get(index).copied()
                && (choice as i64) < item.choices
            {
                self.select_detail_options_index(index);
                self.apply_detail_setting(item, DetailValueEdit::Choice(choice));
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
                    detail_options_row_index(
                        self.select.detail_options.cursor,
                        CATALOG.len(),
                        (id - SKIN_EVENT_DETAIL_OPTIONS_ROW_BASE) as usize,
                    )
                } else {
                    None
                };
                if let Some(index) = position {
                    self.select_detail_options_index(index);
                }
            }
        }
    }
    pub(super) fn detail_options_active(&self) -> bool {
        self.select.select_option_panel == 2
            && self.detail_options_available()
            && self.detail_options_enabled()
    }

    pub(super) fn detail_options_enabled(&self) -> bool {
        self.boot.app_config.select.experimental_detail_options
            && self
                .renderer
                .select_skin_document()
                .is_some_and(|doc| doc.uses_detail_options() && doc.bmz_detail_options_numbers)
    }

    pub(super) fn detail_options_available(&self) -> bool {
        matches!(self.view_state(), AppViewState::Select)
            && self.ui.focused
            && !in_settings_stack(&self.select.folder_stack)
            && !self.select.search.is_active()
            && !self.select.ir_battle.active
            && self.select.key_config_edit.is_none()
            && !self.viewer_waiting
            && !self.ui.egui.as_ref().is_some_and(|ui| ui.blocks_game_input(false))
    }

    pub(super) fn detail_options_closing_snapshot(&self) -> Option<DetailOptionsClosingSnapshot> {
        if self.select.select_option_panel == 2
            || !self.detail_options_available()
            || !self
                .renderer
                .select_skin_document()
                .is_some_and(|doc| doc.uses_detail_options() && doc.bmz_detail_options_close)
            || self.select.option_panel_off_started_at[1]?.elapsed()
                >= Duration::from_millis(DETAIL_OPTIONS_CLOSE_MS as u64)
        {
            return None;
        }
        self.select.detail_options.closing.clone()
    }

    pub(super) fn capture_detail_options_close(&self) -> Option<DetailOptionsClosingSnapshot> {
        if !self
            .renderer
            .select_skin_document()
            .is_some_and(|doc| doc.uses_detail_options() && doc.bmz_detail_options_close)
        {
            return None;
        }
        Some(DetailOptionsClosingSnapshot {
            panel: self.detail_options_snapshot()?,
            scroll: self.detail_options_scroll(),
        })
    }

    pub(super) fn detail_options_mode(&self) -> KeyMode {
        let source = match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => {
                row.chart.as_ref().and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            }
            Some(SelectItem::Course(row)) => row.common_key_mode,
            _ => self.select.select_mode_filter.key_mode(),
        };
        detail_edit_mode(source, self.boot.profile_config.play.key_mode_conversion)
    }

    pub(super) fn reset_detail_options_input(&mut self) {
        self.select.detail_options.repeat_value = None;
        self.select.detail_options.closing = None;
        self.select.detail_options.scroll_started = None;
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
            detail_arrow_input(control, CATALOG[self.select.detail_options.cursor].kind)
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
        if !event.pressed
            && self
                .select
                .detail_options
                .repeat_value
                .as_ref()
                .is_some_and(|(device, name, _, _)| *device == event.device && name == control)
        {
            self.select.detail_options.repeat_value = None;
        }
        match self.select.detail_options.input_edge(action, event.pressed, event.repeat, held) {
            Some(DetailInput::Move(direction)) => self.move_detail_options(direction),
            Some(DetailInput::Value(direction)) => {
                self.change_detail_options_value(direction);
                if matches!(
                    CATALOG[self.select.detail_options.cursor].kind,
                    bmz_render::scene::detail_options::DetailValueKind::Number { .. }
                ) {
                    self.select.detail_options.repeat_value = Some((
                        event.device,
                        control.into(),
                        direction,
                        Instant::now() + Duration::from_millis(400),
                    ));
                }
            }
            _ => {}
        }
        true
    }

    pub(super) fn advance_detail_value_repeat(&mut self) {
        if !self.detail_options_active() {
            self.select.detail_options.repeat_value = None;
            return;
        }
        let held = self.select.detail_options.repeat_value.as_ref().is_some_and(
            |(device, control, _, _)| {
                self.input.pressed_control_sources().any(|(d, c)| d == device && c == control)
            },
        );
        if let Some(direction) = self.select.detail_options.repeat_step(Instant::now(), held) {
            self.change_detail_options_value(direction);
        }
    }

    pub(super) fn move_detail_options(&mut self, direction: i32) {
        if !self.detail_options_active() || direction == 0 {
            return;
        }
        self.select.detail_options.repeat_value = None;
        let low = self.select_scroll_duration_low();
        let high = self.select_scroll_duration_high();
        self.select.detail_options.move_cursor(direction, Instant::now(), low, high);
        self.select.detail_options.value_latched = self.detail_value_keys_held();
        self.play_system_sound(crate::system_sound::SoundType::Scratch);
    }

    fn select_detail_options_index(&mut self, index: usize) {
        let count = CATALOG.len() as i32;
        let forward = (index as i32 - self.select.detail_options.cursor as i32).rem_euclid(count);
        self.move_detail_options(if forward > count / 2 { forward - count } else { forward });
        self.select.detail_options.value_latched = self.detail_value_keys_held();
    }

    pub(super) fn detail_options_scroll(&self) -> f32 {
        if self.detail_options_active() {
            self.select.detail_options.scroll_offset(Instant::now())
        } else {
            0.0
        }
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
        let mode = Some(self.detail_options_mode());
        if item.effects == DetailEffects::GaugeMode {
            self.boot.profile_config.play.gauge_auto_shift = self.select.gauge_auto_shift_option;
        }
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
        if item.effects == DetailEffects::GaugeMode {
            self.select.gauge_auto_shift_option = self.boot.profile_config.play.gauge_auto_shift;
        }
        if matches!(item.effects, DetailEffects::Timing | DetailEffects::Lane) {
            self.sync_realtime_profile_settings();
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
        // Existing save merges current E1/GAS state and syncs the active mode.
        // Pass the E2 edit target explicitly even after the panel has closed.
        self.select.detail_options.dirty = !self.save_play_options_for_mode(
            self.detail_options_mode(),
            None,
            "detail options closed",
        );
    }

    pub(super) fn detail_options_snapshot(&self) -> Option<Arc<DetailOptionsSnapshot>> {
        if !self.detail_options_active() {
            return None;
        }
        let p = &self.boot.profile_config;
        let mode = Some(self.detail_options_mode());
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
            guide: text.text(CATALOG[cursor].guide_key()),
            position: format!("{} / {}", cursor + 1, CATALOG.len()),
        });
        *cache = Some((key, snapshot.clone()));
        Some(snapshot)
    }
}

fn detail_edit_mode(
    source: Option<KeyMode>,
    conversion: crate::config::profile_config::KeyModeConversionConfig,
) -> KeyMode {
    source.map(|mode| effective_play_key_mode(mode, conversion)).unwrap_or(KeyMode::K7)
}

fn detail_arrow_input(control: &str, kind: DetailValueKind) -> Option<DetailInput> {
    // Choices follow their vertical layout; numbers increase upwards.
    let up = if matches!(kind, DetailValueKind::Number { .. }) { 1 } else { -1 };
    match control {
        "ArrowLeft" => Some(DetailInput::Move(-1)),
        "ArrowRight" => Some(DetailInput::Move(1)),
        "ArrowUp" => Some(DetailInput::Value(up)),
        "ArrowDown" => Some(DetailInput::Value(-up)),
        _ => None,
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
    fn numeric_repeat_has_delay_cadence_no_catchup_and_stops_on_move_or_release() {
        let t = Instant::now();
        let mut state = DetailOptionsState {
            repeat_value: Some((
                W_KEYBOARD_DEVICE_ID,
                "KeyZ".into(),
                1,
                t + Duration::from_millis(400),
            )),
            ..Default::default()
        };
        assert_eq!(state.repeat_step(t + Duration::from_millis(399), true), None);
        assert_eq!(state.repeat_step(t + Duration::from_millis(400), true), Some(1));
        assert_eq!(state.repeat_step(t + Duration::from_millis(459), true), None);
        assert_eq!(state.repeat_step(t + Duration::from_secs(2), true), Some(1));
        assert_eq!(state.repeat_step(t + Duration::from_secs(2), true), None);
        state.move_cursor(1, t, Duration::ZERO, Duration::ZERO);
        assert_eq!(state.repeat_step(t + Duration::from_secs(3), true), None);
        state.repeat_value = Some((W_KEYBOARD_DEVICE_ID, "KeyZ".into(), -1, t));
        assert_eq!(state.repeat_step(t, false), None);
        assert_eq!(state.repeat_step(t + Duration::from_secs(3), true), None);
    }

    #[test]
    fn legacy_assist_keyboard_and_gamepad_keep_fixed_seven_key_events() {
        use crate::config::profile_config::LaneConfig;
        let profile = ProfileConfig::new_default("test", "Test", 0);
        let keys = SelectKeyBindings::from_profile(&profile.input);
        let bindings =
            crate::config::play_input::resolve_play_bindings(&profile.input, KeyMode::K7).unwrap();
        for (key, lane, event_id) in [
            (LaneConfig::Key1, Lane::Key1, 301),
            (LaneConfig::Key2, Lane::Key2, 302),
            (LaneConfig::Key3, Lane::Key3, 303),
            (LaneConfig::Key4, Lane::Key4, 304),
            (LaneConfig::Key5, Lane::Key5, 305),
            (LaneConfig::Key6, Lane::Key6, 306),
            (LaneConfig::Key7, Lane::Key7, 307),
        ] {
            let binding =
                bindings.iter().find(|b| b.device == "keyboard" && b.lane == Some(key)).unwrap();
            assert_eq!(keys.legacy_assist_event(&binding.control), Some(event_id));
            assert_eq!(legacy_assist_event_for_lane(lane), Some(event_id));
            let mut assist = profile.play.assist;
            assert!(assist.toggle_beatoraja_button(event_id));
            let mut flags = [false; 7];
            flags[(event_id - 301) as usize] = true;
            assert_eq!(assist.flags(), flags);
            assert!(assist.toggle_beatoraja_button(event_id));
            assert_eq!(assist, profile.play.assist);
        }
        // Scratch/arrows never become a detail carousel in the legacy panel.
        for control in ["ScratchUp", "ScratchDown", "ArrowLeft", "ArrowRight"] {
            assert_eq!(keys.legacy_assist_event(control), None);
        }
        assert_eq!(legacy_assist_event_for_lane(Lane::Scratch), None);
        assert_eq!(legacy_assist_event_for_lane(Lane::Key8), None);
        let slots = crate::input::gamepad::GamepadSlotMap::from_slot_ids([Some(0), Some(1)]);
        assert_eq!(
            select_option_lane_for_gamepad(&profile.input, slots, DeviceId(16), "Button1")
                .and_then(legacy_assist_event_for_lane),
            Some(301)
        );
    }

    #[test]
    fn detail_arrows_follow_column_and_choice_axes() {
        for (key, action) in [
            ("ArrowLeft", DetailInput::Move(-1)),
            ("ArrowRight", DetailInput::Move(1)),
            ("ArrowUp", DetailInput::Value(-1)),
            ("ArrowDown", DetailInput::Value(1)),
        ] {
            for kind in [DetailValueKind::Bool, DetailValueKind::Enum] {
                assert_eq!(detail_arrow_input(key, kind), Some(action));
            }
        }
        assert_eq!(detail_arrow_input("Enter", DetailValueKind::Enum), None);
    }

    #[test]
    fn detail_numeric_up_and_odd_keys_increase_down_and_even_keys_decrease() {
        let mut profile = ProfileConfig::new_default("test", "Test", 0);
        let bindings = SelectKeyBindings::from_profile(&profile.input);
        for item in
            CATALOG.iter().filter(|item| matches!(item.kind, DetailValueKind::Number { .. }))
        {
            for (control, direction) in [("ArrowUp", 1), ("ArrowDown", -1)] {
                assert_eq!(
                    detail_arrow_input(control, item.kind),
                    Some(DetailInput::Value(direction))
                );
                let before = item.value(&profile);
                assert!(item.adjust(&mut profile, Some(KeyMode::K7), direction));
                assert_eq!(item.value(&profile), before + i64::from(direction));
            }
            for (key, lane, direction) in [
                ("Z", Lane::Key1, 1),
                ("S", Lane::Key2, -1),
                ("Z", Lane::Key8, 1),
                ("S", Lane::Key9, -1),
            ] {
                assert_eq!(bindings.detail_value_direction(key, false), Some(direction));
                assert_eq!(detail_lane_direction(lane, false), Some(direction));
            }
            assert_eq!(detail_arrow_input("ArrowLeft", item.kind), Some(DetailInput::Move(-1)));
            assert_eq!(detail_arrow_input("ArrowRight", item.kind), Some(DetailInput::Move(1)));
        }
    }

    #[test]
    fn detail_unresolved_mode_edits_and_persists_7k_without_converting_the_fallback() {
        use crate::config::profile_config::KeyModeConversionConfig;
        let conversion = KeyModeConversionConfig::SevenToNine;
        assert_eq!(detail_edit_mode(Some(KeyMode::K7), conversion), KeyMode::K9);
        assert_eq!(detail_edit_mode(Some(KeyMode::K14), conversion), KeyMode::K14);
        let target = detail_edit_mode(None, conversion);
        assert_eq!(target, KeyMode::K7);
        let mut p = ProfileConfig::new_default("test", "Test", 0);
        p.activate_play_mode(KeyMode::K9);
        let original = toml::to_string(&p.lane).unwrap();
        assert!(CATALOG[0].select_choice(&mut p, Some(target), 1));
        let edited = toml::to_string(&p.lane).unwrap();
        p.activate_play_mode(KeyMode::K9);
        assert_eq!(toml::to_string(&p.lane).unwrap(), original);
        let mut loaded: ProfileConfig = toml::from_str(&toml::to_string(&p).unwrap()).unwrap();
        loaded.activate_play_mode(KeyMode::K7);
        assert_eq!(toml::to_string(&loaded.lane).unwrap(), edited);
        let row = CATALOG[0].row(
            &loaded,
            DetailContext {
                mode: Some(target),
                gas: GaugeAutoShiftConfig::Off,
                practice: false,
                course: true,
            },
            &Localizer::new(crate::i18n::AppLocale::En),
        );
        assert_eq!(row.scope, 7);
        assert!(row.editable);
    }

    #[test]
    fn detail_scroll_wraps_and_uses_select_timing_without_changing_values() {
        let mut state = DetailOptionsState { cursor: CATALOG.len() - 1, ..Default::default() };
        let now = Instant::now();
        let low = Duration::from_millis(120);
        let high = Duration::from_millis(40);
        state.move_cursor(1, now, low, high);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.scroll_offset(now), 1.0);
        assert_eq!(state.scroll_offset(now + low / 2), 0.5);
        assert_eq!(state.scroll_offset(now + low), 0.0);
        // Reversing mid-animation preserves the visual position instead of
        // snapping a whole column in the opposite direction.
        state.move_cursor(-1, now + low / 2, low, high);
        assert_eq!(state.cursor, CATALOG.len() - 1);
        assert_eq!(state.scroll_duration, high);
        assert_eq!(state.scroll_offset(now + low / 2), -0.5);
        assert_eq!(state.scroll_offset(now + low / 2 + high), 0.0);
        state.scroll_started = None; // closing, E1+E2, focus loss
        assert_eq!(state.scroll_offset(now), 0.0);
        state.move_cursor(1, now, Duration::ZERO, Duration::ZERO);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.scroll_offset(now), 0.0);
        assert!(!state.dirty);
    }

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
