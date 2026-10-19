use super::*;
use crate::select_detail_options::{CATALOG, DetailContext, DetailEffects, DetailOptionDef};
use bmz_render::scene::detail_options::{
    DETAIL_OPTIONS_CLOSE_MS, DetailOptionsClosingSnapshot, DetailOptionsSnapshot, DetailValueKind,
    detail_options_viewport,
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
    scroll: SelectBarScroll,
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
    fn scroll_frame(&self, now: Instant) -> (usize, f32) {
        let remaining = self.scroll.remaining_at(now);
        (
            select_scroll_center_index(self.cursor, CATALOG.len(), remaining),
            select_scroll_fraction(remaining) * remaining.signum() as f32,
        )
    }

    fn move_cursor(&mut self, direction: i32, now: Instant, duration: Duration) {
        if direction == 0 {
            return;
        }
        self.repeat_value = None;
        self.cursor =
            (self.cursor as i64 + i64::from(direction)).rem_euclid(CATALOG.len() as i64) as usize;
        self.scroll.move_by(direction, now, duration);
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
        if let (Some(panel), _) = self.detail_options_frame() {
            self.execute_detail_options_panel_event(id, arg, &panel);
        }
    }

    pub(super) fn execute_detail_options_panel_event(
        &mut self,
        id: i32,
        arg: i32,
        panel: &DetailOptionsSnapshot,
    ) {
        use bmz_render::skin::*;
        if !self.detail_options_active() {
            return;
        }
        if let Some((slot, direction)) = detail_options_numeric_event(id) {
            if let Some(index) = panel.row_index(slot)
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
                && let Some(index) = panel.row_index(slot)
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
                    panel.row_index((id - SKIN_EVENT_DETAIL_OPTIONS_ROW_BASE) as usize)
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
        let (panel, scroll) = self.detail_options_frame();
        Some(DetailOptionsClosingSnapshot { panel: panel?, scroll })
    }

    pub(super) fn detail_options_mode(&self) -> KeyMode {
        let source = match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => {
                row.chart.as_ref().and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            }
            Some(SelectItem::Course(row)) => row.common_key_mode,
            _ => self.select.select_mode_filter.key_mode(),
        };
        detail_edit_mode(source, self.selected_key_mode_conversion())
    }

    pub(super) fn reset_detail_options_input(&mut self) {
        self.select.detail_options.repeat_value = None;
        self.select.detail_options.closing = None;
        self.select.detail_options.scroll.reset();
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
            !matches!(action, DetailInput::Move(_)) || !event.synthesized_analog_axis
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
        self.move_detail_options_with_scroll(direction, self.select_scroll_duration_low());
    }

    pub(super) fn move_detail_options_with_scroll(&mut self, direction: i32, duration: Duration) {
        if !self.detail_options_active() || direction == 0 {
            return;
        }
        self.select.detail_options.move_cursor(direction, Instant::now(), duration);
        self.select.detail_options.value_latched = self.detail_value_keys_held();
        self.play_system_sound(crate::system_sound::SoundType::Scratch);
    }

    fn select_detail_options_index(&mut self, index: usize) {
        let count = CATALOG.len() as i32;
        let forward = (index as i32 - self.select.detail_options.cursor as i32).rem_euclid(count);
        self.move_detail_options(if forward > count / 2 { forward - count } else { forward });
        self.select.detail_options.value_latched = self.detail_value_keys_held();
    }

    pub(super) fn detail_options_frame(&self) -> (Option<Arc<DetailOptionsSnapshot>>, f32) {
        self.detail_options_frame_at(Instant::now())
    }

    fn detail_options_frame_at(&self, now: Instant) -> (Option<Arc<DetailOptionsSnapshot>>, f32) {
        let (cursor, scroll) = self.select.detail_options.scroll_frame(now);
        let panel = self.detail_options_snapshot(cursor);
        let scroll = if panel.is_some() { scroll } else { 0.0 };
        (panel, scroll)
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

    fn detail_options_snapshot(
        &self,
        displayed_cursor: usize,
    ) -> Option<Arc<DetailOptionsSnapshot>> {
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
        let viewport_start = detail_options_viewport(displayed_cursor, CATALOG.len());
        let mut cache = self.select.detail_options.cache.borrow_mut();
        if let Some((previous, snapshot)) = cache.as_mut()
            && *previous == key
        {
            if snapshot.viewport_start != viewport_start {
                Arc::make_mut(snapshot).viewport_start = viewport_start;
            }
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
            viewport_start,
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
    if !nine_key && let Some(lane) = select_option_lane_for_gamepad(input, slots, device, control) {
        return Some(lane);
    }
    // Match normal select options: retain explicit DP/device assignments, then
    // accept the independent 7K bindings. Resolve the fallback with its device too.
    crate::config::play::lane_binding_for_chart_with_slots(
        input,
        if nine_key { KeyMode::K9 } else { KeyMode::K7 },
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
        state.move_cursor(1, t, Duration::ZERO);
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
    fn detail_battle_uses_launch_profile_slot_and_preserves_converted_mode_settings() {
        use crate::config::profile_config::KeyModeConversionConfig as Conversion;
        for conversion in [Conversion::SevenToNine, Conversion::SpToDp, Conversion::SevenToSix] {
            for (session, double, target) in [
                (SessionMode::AutoplayBattle, DoubleOption::Off, false),
                (SessionMode::GBattle, DoubleOption::Off, false),
                (SessionMode::Normal, DoubleOption::Battle, false),
                (SessionMode::Normal, DoubleOption::BattleAutoScratch, false),
                (SessionMode::Normal, DoubleOption::Off, true),
            ] {
                let normalized =
                    key_mode_conversion_for_session(conversion, session, double, target);
                let mode = detail_edit_mode(Some(KeyMode::K7), normalized);
                assert_eq!(mode, KeyMode::K7);
                assert_eq!(mode, effective_play_key_mode(KeyMode::K7, normalized));
                let mut profile = ProfileConfig::new_default("test", "Test", 0);
                let converted_mode = effective_play_key_mode(KeyMode::K7, conversion);
                profile.activate_play_mode(converted_mode);
                profile.sync_active_play_mode();
                profile.activate_play_mode(mode);
                let original = profile.play_mode_config(converted_mode);
                for id in [crate::select_detail_options::LANE_COVER_ID, 304, 702] {
                    let item = CATALOG.iter().find(|item| item.id == id).unwrap();
                    let before = item.value(&profile);
                    assert!(item.adjust(&mut profile, Some(mode), 1));
                    assert_ne!(item.value(&profile), before);
                }
                let serialized = toml::to_string(&profile).unwrap();
                let loaded: ProfileConfig = toml::from_str(&serialized).unwrap();
                assert_eq!(loaded.play_mode_config(mode), profile.play_mode_config(mode));
                assert_eq!(loaded.play_mode_config(converted_mode), original);
            }
            assert_eq!(
                key_mode_conversion_for_session(
                    conversion,
                    SessionMode::Normal,
                    DoubleOption::Off,
                    false
                ),
                conversion
            );
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
        assert!(
            DetailOptionDef::by_id(crate::select_detail_options::LANE_COVER_ID)
                .unwrap()
                .select_choice(&mut p, Some(target), 1)
        );
        let edited = toml::to_string(&p.lane).unwrap();
        p.activate_play_mode(KeyMode::K9);
        assert_eq!(toml::to_string(&p.lane).unwrap(), original);
        let mut loaded: ProfileConfig = toml::from_str(&toml::to_string(&p).unwrap()).unwrap();
        loaded.activate_play_mode(KeyMode::K7);
        assert_eq!(toml::to_string(&loaded.lane).unwrap(), edited);
        let row = DetailOptionDef::by_id(crate::select_detail_options::LANE_COVER_ID).unwrap().row(
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
    fn detail_scroll_eases_wraps_and_resets_without_changing_values() {
        let mut state = DetailOptionsState { cursor: CATALOG.len() - 1, ..Default::default() };
        let now = Instant::now();
        let low = Duration::from_millis(120);
        state.move_cursor(1, now, low);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.scroll_frame(now), (CATALOG.len() - 1, 0.0));
        assert_eq!(state.scroll_frame(now + low / 4), (0, 0.84375));
        assert_eq!(state.scroll_frame(now + low / 2), (0, 0.5));
        assert_eq!(state.scroll_frame(now + low), (0, 0.0));
        // Reversing mid-animation preserves the visual position instead of
        // snapping a whole column in the opposite direction.
        state.move_cursor(-1, now + low / 2, low);
        assert_eq!(state.cursor, CATALOG.len() - 1);
        assert_eq!(state.scroll_frame(now + low / 2), (CATALOG.len() - 1, -0.5));
        assert_eq!(state.scroll_frame(now + low / 2 + low), (CATALOG.len() - 1, 0.0));
        state.scroll.reset(); // closing, E1+E2, focus loss
        assert_eq!(state.scroll_frame(now), (CATALOG.len() - 1, 0.0));
        state.move_cursor(1, now, Duration::ZERO);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.scroll_frame(now), (0, 0.0));
        assert!(!state.dirty);
    }

    #[test]
    fn detail_repeated_wheel_and_batched_analog_match_song_bar_motion() {
        let now = Instant::now();
        let mut state = DetailOptionsState { cursor: CATALOG.len() - 1, ..Default::default() };
        let mut bar = SelectBarScroll::default();
        let mut target = state.cursor as f64;
        for (elapsed_ms, direction, duration) in [
            (0, 1, Duration::from_millis(300)),
            (20, 1, Duration::from_millis(300)),
            (40, 1, Duration::from_millis(300)),
            (45, 3, select_analog_scroll_duration(3)),
            (50, -2, select_analog_scroll_duration(-2)),
            (80, -1, select_analog_scroll_duration(-1)),
        ] {
            let moved_at = now + Duration::from_millis(elapsed_ms);
            let before = target - state.scroll.remaining_at(moved_at);
            state.move_cursor(direction, moved_at, duration);
            bar.move_by(direction, moved_at, duration);
            target += f64::from(direction);
            assert!((target - state.scroll.remaining_at(moved_at) - before).abs() < 1e-9);
            for elapsed in [0, 1, 4, 10, 20, 30, 60, 120, 300] {
                let sample_at = moved_at + Duration::from_millis(elapsed);
                let residual = bar.remaining_at(sample_at);
                assert_eq!(state.scroll.remaining_at(sample_at), residual);
                let (center, fraction) = state.scroll_frame(sample_at);
                assert!(fraction.abs() < 1.0);
                let visible =
                    (center as f64 - f64::from(fraction)).rem_euclid(CATALOG.len() as f64);
                let expected = (target - residual).rem_euclid(CATALOG.len() as f64);
                assert!((visible - expected).abs() < 1e-6, "{visible} != {expected}");
            }
        }
        assert!(!state.dirty);
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "creates a Winit event loop; run alone with --ignored"]
    fn detail_scroll_input_routes_snapshot_cache_and_click_use_the_displayed_window() {
        use winit::platform::windows::EventLoopBuilderExtWindows;

        let event_loop =
            EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let mut boot = data.boot();
        boot.app_config.input.gamepad_enabled = false;
        boot.app_config.select.experimental_detail_options = true;
        boot.app_config.select.scroll_duration_low_ms = 1000;
        boot.app_config.select.scroll_duration_high_ms = 10;
        let (maintenance_tx, _) = tokio::sync::watch::channel(false);
        let mut app = WinitApp::new(
            boot,
            AppOptions { viewer_play: true, ..Default::default() },
            Instant::now(),
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            event_loop.create_proxy(),
            LogBuffer::default(),
            maintenance_tx,
            None,
        )
        .unwrap();
        app.viewer_mode = false;
        crate::skin_loader::set_decoded_skin_context(
            &mut app.renderer,
            crate::skin_loader::SkinKind::Select,
            bmz_render::skin::default_skin_manifest(),
            serde_json::from_value(serde_json::json!({
                "type": 5, "bmzDetailOptions": 1,
                "bmzDetailOptionsNumbers": true, "bmzDetailOptionsClose": true
            }))
            .unwrap(),
            None,
            vec![],
            false,
        );
        app.select.select_option_panel = 2;
        assert!(app.detail_options_active());

        // Exercise the actual wheel and analog routes, including their durations.
        for rows in [1, -1, 3, -3] {
            app.select.detail_options = DetailOptionsState::default();
            let before = Instant::now();
            let duration = if rows == 1 {
                app.route_mouse_wheel(MouseScrollDelta::LineDelta(0.0, -1.0));
                app.select_scroll_duration_low()
            } else {
                app.select.select_analog_scroll_buffer =
                    rows * app.boot.profile_config.input.analog_ticks_per_scroll.max(1) as i32;
                app.advance_select_analog_scroll();
                select_analog_scroll_duration(rows)
            };
            let after = Instant::now();
            assert_eq!(
                app.select.detail_options.cursor,
                rows.rem_euclid(CATALOG.len() as i32) as usize
            );
            assert_eq!(app.select.select_analog_scroll_buffer, 0);
            let sample = after + duration / 2;
            let bounds = [before, after].map(|start| {
                let mut bar = SelectBarScroll::default();
                bar.move_by(rows, start, duration);
                bar.remaining_at(sample)
            });
            let actual = app.select.detail_options.scroll.remaining_at(sample);
            assert!(actual >= bounds[0].min(bounds[1]) - 1e-9);
            assert!(actual <= bounds[0].max(bounds[1]) + 1e-9);
        }

        // Both the viewport and fractional displacement use one sample. Rebasing
        // the window shares item strings and never mutates an earlier snapshot.
        app.select.detail_options =
            DetailOptionsState { cursor: CATALOG.len() - 1, ..Default::default() };
        let now = Instant::now();
        let duration = Duration::from_millis(300);
        app.select.detail_options.move_cursor(3, now, duration);
        let (first, fraction) = app.detail_options_frame_at(now);
        let first = first.unwrap();
        assert_eq!(fraction, 0.0);
        assert_eq!(first.row_index(3), Some(CATALOG.len() - 1));
        assert_eq!(first.cursor, 2);
        let (settled, fraction) = app.detail_options_frame_at(now + duration);
        let settled = settled.unwrap();
        assert_eq!(fraction, 0.0);
        assert_eq!(settled.row_index(3), Some(2));
        assert!(Arc::ptr_eq(&first.items, &settled.items));
        assert_eq!(first.row_index(3), Some(CATALOG.len() - 1));

        // Even an event delivered after rebasing resolves against its hit snapshot.
        app.execute_detail_options_panel_event(19313, 1, &first);
        assert_eq!(app.select.detail_options.cursor, CATALOG.len() - 1);
        let frozen = app.capture_detail_options_close().unwrap();
        app.reset_detail_options_input();
        assert_eq!(app.select.detail_options.scroll_frame(Instant::now()).1, 0.0);
        assert_eq!(frozen.panel.cursor, CATALOG.len() - 1);
        assert!(!app.select.detail_options.dirty);

        // Choice and numeric events use the displayed item too, rather than the
        // same slot in the target-centered window.
        assert_eq!(first.row_index(4), Some(0));
        let choice = 1 - CATALOG[0].value(&app.boot.profile_config);
        app.execute_detail_options_panel_event(19500 + 4 * 64 + choice as i32 * 4, 1, &first);
        assert_eq!(app.select.detail_options.cursor, 0);
        assert_eq!(CATALOG[0].value(&app.boot.profile_config), choice);
        assert_eq!(first.row_index(5), Some(1));
        let before = CATALOG[1].value(&app.boot.profile_config);
        app.execute_detail_options_panel_event(19320 + 5 * 2 + 1, 1, &first);
        assert_eq!(app.select.detail_options.cursor, 1);
        assert_eq!(CATALOG[1].value(&app.boot.profile_config), before + 1);
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
    fn detail_gamepad_accepts_independent_7k_bindings_without_crossing_devices() {
        use crate::config::profile_config::{LaneConfig, PlayModeInputConfig};
        let mut profile = ProfileConfig::new_default("test", "Test", 0);
        for (mode, device, lane) in [
            (KeyMode::K7, "gamepad1", LaneConfig::Key3),
            (KeyMode::K14, "gamepad2", LaneConfig::Key10),
            (KeyMode::K9, "gamepad1", LaneConfig::Key8),
        ] {
            let mut bindings = crate::config::play_input::default_play_bindings(mode);
            bindings.retain(|binding| {
                binding.lane != Some(lane) || !binding.device.starts_with("gamepad")
            });
            bindings.push(crate::config::play_input::gamepad_play_binding_for_device(
                device, "Button13", lane,
            ));
            profile.input.play.insert(
                mode.play_map_key().into(),
                PlayModeInputConfig { bindings, ..Default::default() },
            );
        }
        let slots = crate::input::gamepad::GamepadSlotMap::from_slot_ids([Some(0), Some(1)]);
        let keys = SelectKeyBindings::from_profile(&profile.input);
        assert_eq!(keys.ui_lane_for_control("Button13"), Some(Lane::Key3));
        for (device, nine_key, expected, direction) in [
            (DeviceId(16), false, Lane::Key3, 1),
            (DeviceId(17), false, Lane::Key10, 1),
            (DeviceId(16), true, Lane::Key8, -1),
        ] {
            let lane = detail_gamepad_lane(&profile.input, slots, device, "Button13", nine_key);
            assert_eq!(lane, Some(expected));
            assert_eq!(
                lane.and_then(|lane| detail_lane_direction(lane, nine_key)),
                Some(direction)
            );
        }
        assert_eq!(
            detail_gamepad_lane(&profile.input, slots, DeviceId(18), "Button13", false),
            None
        );
        assert_eq!(
            detail_gamepad_lane(&profile.input, slots, DeviceId(17), "Button13", true),
            None
        );
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
