//! Logical panel lifetime is independent of the physical E1/E2 holds.
use super::*;
use bmz_gameplay::input::backend::{DeviceTimestamp, monotonic_timestamp_ns};
use bmz_gameplay::input::bounce::InputBounceConfig;

const TAP_DURATION: Duration = Duration::from_millis(200);

#[derive(Default)]
pub(super) struct OptionPanelSession {
    pub panel: u8,
    pinned: bool,
    e1: bool,
    e2: bool,
    opened_at: Option<DeviceTimestamp>,
    pending_release: Option<PanelRelease>,
}

struct PanelRelease {
    at: DeviceTimestamp,
    deadline_ns: u128,
}

impl OptionPanelSession {
    pub(super) fn toggle_lr2_panel(&mut self, panel: u8, current_panel: u8, e1: bool, e2: bool) {
        self.cancel(e1, e2);
        self.panel = if current_panel == panel { 0 } else { panel };
    }

    pub(super) fn lr2_panel_for_holds(&mut self, e1: bool, e2: bool) -> u8 {
        // Ordinary input/resync keeps a mouse-opened panel. A modifier edge
        // hands ownership back to the established E1/E2 hold controls.
        if (self.e1, self.e2) != (e1, e2) {
            self.cancel(e1, e2);
            self.panel = select_option_panel_for_holds(e1, e2);
        }
        self.panel
    }

    fn route_event(
        &mut self,
        event: &ControlInputEvent,
        keys: &SelectKeyBindings,
        available: bool,
        holds: (bool, bool),
        bounce: InputBounceConfig,
        accepted: bool,
    ) -> bool {
        if !available {
            self.cancel(holds.0, holds.1);
            return false;
        }
        let Some(control) = event.name.as_deref() else { return false };
        if !keys.is_start(control) && !keys.is_e2_action(control) {
            return false;
        }
        if !event.repeat {
            let is_start = keys.is_start(control);
            let e1 = if is_start { holds.0 } else { self.e1 };
            let e2 = if accepted { holds.1 } else { self.e2 };
            if is_start || accepted {
                let threshold_us =
                    event.physical.as_ref().map_or(0, |physical| bounce.threshold_for(physical));
                self.edge(e1, e2, event.timestamp, Duration::from_micros(threshold_us));
            }
        }
        true
    }

    fn edge(&mut self, e1: bool, e2: bool, at: DeviceTimestamp, bounce: Duration) {
        if let Some(release) = &self.pending_release {
            match (release.at, at) {
                (DeviceTimestamp::MonotonicNs(released), DeviceTimestamp::MonotonicNs(now))
                    if now >= released =>
                {
                    self.advance(now);
                }
                _ => {
                    // An unorderable event cannot prove a short press.
                    self.pending_release = None;
                    self.finish_release(DeviceTimestamp::Unknown);
                }
            }
        }
        let start_pressed = e1 && !self.e1;
        let start_released = !e1 && self.e1;
        let switch_pressed = e2 && !self.e2;
        self.e1 = e1;
        self.e2 = e2;
        // A brief contact break continues the original gesture, including
        // a press that already closed a pinned panel.
        if start_pressed && self.pending_release.take().is_none() {
            if self.pinned {
                self.panel = 0;
                self.pinned = false;
                self.opened_at = None;
            } else {
                self.panel = 1;
                self.opened_at = Some(at);
            }
        }
        if start_released {
            if let DeviceTimestamp::MonotonicNs(now) = at
                && !bounce.is_zero()
            {
                self.pending_release =
                    Some(PanelRelease { at, deadline_ns: now.saturating_add(bounce.as_nanos()) });
            } else {
                self.finish_release(at);
            }
        }
        if switch_pressed && self.panel != 0 {
            self.panel = if self.panel == 1 { 2 } else { 1 };
        }
    }

    fn finish_release(&mut self, at: DeviceTimestamp) {
        if let Some(opened) = self.opened_at.take() {
            self.pinned = match (opened, at) {
                (DeviceTimestamp::MonotonicNs(opened), DeviceTimestamp::MonotonicNs(released)) => {
                    released
                        .checked_sub(opened)
                        .is_some_and(|elapsed| elapsed < TAP_DURATION.as_nanos())
                }
                _ => false,
            };
            if !self.pinned {
                self.panel = 0;
            }
        }
    }

    fn advance(&mut self, now_ns: u128) {
        if self.pending_release.as_ref().is_some_and(|release| now_ns > release.deadline_ns) {
            let release = self.pending_release.take().unwrap();
            self.finish_release(release.at);
        }
    }

    fn cancel(&mut self, e1: bool, e2: bool) {
        *self = Self { e1, e2, ..Default::default() };
    }

    // A repaired release or a device reconnect is never a tap/press gesture.
    fn reconcile(&mut self, e1: bool, e2: bool) {
        if self.e1 && !e1 && self.opened_at.is_some() {
            self.cancel(e1, e2);
        }
        if e1 {
            // Snapshots preserve a hold, but never synthesize a new gesture.
            self.pending_release = None;
        }
        self.e1 = e1;
        self.e2 = e2;
    }
}

impl WinitApp {
    pub(super) fn route_select_option_session_event(
        &mut self,
        event: &ControlInputEvent,
        accepted: bool,
    ) -> bool {
        if !self.detail_options_enabled() || !matches!(self.view_state(), AppViewState::Select) {
            return false;
        }
        let available = self.detail_options_available();
        let consumed = self.select.option_session.route_event(
            event,
            &self.select.select_keys,
            available,
            (self.input.start_held, self.input.select_held),
            input_bounce_config_from_profile(&self.boot.profile_config.input),
            accepted,
        );
        if consumed || !available {
            self.update_select_option_panel();
        }
        consumed
    }

    pub(super) fn advance_select_option_session(&mut self) {
        // Run only after draining the whole capture batch: a queued re-press
        // inside the debounce window must be able to cancel its release first.
        if self.select.option_session.pending_release.is_some() {
            self.select.option_session.advance(monotonic_timestamp_ns());
        }
    }

    pub(super) fn reconcile_select_option_session(&mut self) {
        self.select.option_session.reconcile(self.input.start_held, self.input.select_held);
        self.update_select_option_panel();
    }

    pub(super) fn cancel_select_option_session(&mut self) {
        self.select.option_session.cancel(self.input.start_held, self.input.select_held);
        self.save_detail_options_if_dirty();
        self.select.select_option_panel = 0;
        self.select.option_panel_off_started_at = [None; 6];
        self.reset_detail_options_input();
    }

    pub(super) fn reset_option_session_for_modal(&mut self) {
        self.select.option_session.cancel(self.input.start_held, self.input.select_held);
    }
}

#[cfg(test)]
#[path = "panel_session/input_tests.rs"]
mod input_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(ms: u64) -> DeviceTimestamp {
        DeviceTimestamp::MonotonicNs(u128::from(ms) * 1_000_000)
    }

    #[test]
    fn lr2_mouse_panel_survives_keyboard_and_gamepad_option_edges_without_restarting_timer() {
        let keys =
            SelectKeyBindings::from_profile(&crate::config::play_input::default_profile_input());
        let mut input = AppInputRuntime::default();
        let mut session = OptionPanelSession::default();
        let now = Instant::now();
        let mut panel = 0;
        let mut on = now;
        let mut off = [None; 6];
        let mut exit_hold = Some(now);
        session.toggle_lr2_panel(1, panel, false, false);
        assert!(transition_select_option_panel(
            &mut panel,
            &mut on,
            &mut off,
            &mut exit_hold,
            session.lr2_panel_for_holds(false, false),
            now,
        ));
        assert_eq!(panel, 1);
        assert_eq!(exit_hold, None);

        for pressed in [true, false] {
            for event in [
                ControlInputEvent::keyboard_parts(
                    PhysicalKey::Code(KeyCode::KeyS),
                    if pressed { ElementState::Pressed } else { ElementState::Released },
                    false,
                ),
                ControlInputEvent::gamepad(DeviceId(16), "Button2", pressed),
            ] {
                input.track_control(&event);
                let (e1, e2, _) =
                    select_hold_state_from_pressed_controls(&input.pressed_controls, &keys);
                assert_eq!((e1, e2), (false, false));
                let next = session.lr2_panel_for_holds(e1, e2);
                assert!(!transition_select_option_panel(
                    &mut panel,
                    &mut on,
                    &mut off,
                    &mut exit_hold,
                    next,
                    now + Duration::from_secs(1),
                ));
                assert_eq!(panel, 1, "option input must still reach panel 1");
                assert_eq!(on, now);
                assert_eq!(off, [None; 6]);
            }
        }
        session.toggle_lr2_panel(1, panel, false, false);
        assert_eq!(session.lr2_panel_for_holds(false, false), 0);
    }

    #[test]
    fn lr2_mouse_panel_hands_back_to_modifier_holds_and_click_can_close_a_held_panel() {
        let mut session = OptionPanelSession::default();
        for (e1, e2, expected) in [(true, false, 1), (false, true, 2), (true, true, 3)] {
            session.cancel(false, false);
            session.toggle_lr2_panel(1, 0, false, false);
            assert_eq!(session.lr2_panel_for_holds(e1, e2), expected);
            assert_eq!(session.lr2_panel_for_holds(e1, e2), expected);
            assert_eq!(session.lr2_panel_for_holds(false, false), 0);
        }
        assert_eq!(session.lr2_panel_for_holds(true, false), 1);
        session.toggle_lr2_panel(1, 1, true, false);
        assert_eq!(session.lr2_panel_for_holds(true, false), 0);
        assert_eq!(session.lr2_panel_for_holds(false, false), 0);
        assert_eq!(session.lr2_panel_for_holds(true, false), 1);
    }

    #[test]
    fn lr2_mouse_panel_cancel_does_not_reopen_on_resync_or_modifier_release() {
        for (e1, e2) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut session = OptionPanelSession::default();
            session.toggle_lr2_panel(1, 0, e1, e2);
            assert_eq!(session.lr2_panel_for_holds(e1, e2), 1);
            // Shared reset used on modal entry, focus loss and scene/profile changes.
            session.cancel(e1, e2);
            session.reconcile(e1, e2);
            assert_eq!(session.lr2_panel_for_holds(e1, e2), 0);
            assert_eq!(session.lr2_panel_for_holds(false, false), 0);
            assert_eq!(session.lr2_panel_for_holds(true, false), 1);
        }
    }

    #[test]
    fn select_exit_hold_is_cancelled_by_normal_detail_and_pinned_panels() {
        let base = Instant::now();
        for detail in [false, true] {
            for pinned in [false, true] {
                let mut session = OptionPanelSession::default();
                let mut input = AppInputRuntime::default();
                let mut exit_hold = Some(base);
                input.track_control(&ControlInputEvent::keyboard_parts(
                    PhysicalKey::Code(KeyCode::Escape),
                    ElementState::Pressed,
                    false,
                ));
                session.edge(true, false, timestamp(0), Duration::ZERO);
                if detail {
                    session.edge(true, true, timestamp(0), Duration::ZERO);
                }
                if pinned {
                    session.edge(false, detail, timestamp(100), Duration::ZERO);
                }
                let mut panel = 0;
                let mut on = base;
                let mut off = [None; 6];
                assert!(transition_select_option_panel(
                    &mut panel,
                    &mut on,
                    &mut off,
                    &mut exit_hold,
                    session.panel,
                    base,
                ));
                assert_eq!(exit_hold, None, "opening cancels even before Escape release");
                input.track_control(&ControlInputEvent::keyboard_parts(
                    PhysicalKey::Code(KeyCode::Escape),
                    ElementState::Released,
                    false,
                ));
                let later = base + SELECT_EXIT_HOLD_DURATION;
                assert!(!select_exit_hold_due(
                    &mut exit_hold,
                    panel == 0 && input.pressed_controls.contains("Escape"),
                    later
                ));
                assert!(transition_select_option_panel(
                    &mut panel,
                    &mut on,
                    &mut off,
                    &mut exit_hold,
                    0,
                    later,
                ));
                update_select_exit_hold(&mut exit_hold, ElementState::Pressed, true, later);
                assert!(
                    !select_exit_hold_due(&mut exit_hold, true, later),
                    "closing must not resume the old timer"
                );
            }
        }
    }

    #[test]
    fn select_exit_hold_requires_continuous_input_and_keeps_normal_deadline() {
        let base = Instant::now();
        let mut hold = None;
        update_select_exit_hold(&mut hold, ElementState::Pressed, false, base);
        let deadline = base + SELECT_EXIT_HOLD_DURATION;
        update_select_exit_hold(&mut hold, ElementState::Pressed, true, deadline);
        assert!(!select_exit_hold_due(&mut hold, true, deadline - Duration::from_millis(1)));
        assert!(select_exit_hold_due(&mut hold, true, deadline));
        // Release/focus reconciliation cancels a stale timer even if a modal consumed keyup.
        assert!(!select_exit_hold_due(&mut hold, false, deadline));
        assert_eq!(hold, None);
        assert!(!select_exit_hold_due(&mut hold, true, deadline + Duration::from_secs(1)));
        update_select_exit_hold(&mut hold, ElementState::Pressed, false, deadline);
        update_select_exit_hold(&mut hold, ElementState::Released, false, deadline);
        assert!(!select_exit_hold_due(&mut hold, true, deadline + SELECT_EXIT_HOLD_DURATION));
    }

    #[test]
    fn select_escape_closes_folders_before_arming_the_root_exit_hold() {
        assert_eq!(select_escape_press(true, 2), SelectEscapePress::CancelEdit);
        assert_eq!(select_escape_press(false, 2), SelectEscapePress::ExitFolder);
        assert_eq!(select_escape_press(false, 1), SelectEscapePress::ExitFolder);
        assert_eq!(select_escape_press(false, 0), SelectEscapePress::ArmExitHold);
        // フォルダを閉じた押下を押し続けても、OS repeat では終了の計測を始めない。
        let now = Instant::now();
        let mut hold = None;
        update_select_exit_hold(&mut hold, ElementState::Pressed, true, now);
        assert!(!select_exit_hold_due(&mut hold, true, now + SELECT_EXIT_HOLD_DURATION));
    }

    #[test]
    fn unavailable_panel_passes_modifier_keys_to_search_and_key_config() {
        let keys =
            SelectKeyBindings::from_profile(&crate::config::play_input::default_profile_input());
        let mut session = OptionPanelSession::default();
        session.edge(true, false, timestamp(0), Duration::ZERO);
        session.edge(false, false, timestamp(0), Duration::ZERO);
        assert_eq!(session.panel, 1);
        for code in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyV, KeyCode::KeyE, KeyCode::KeyQ] {
            let holds = (code == KeyCode::KeyQ, code == KeyCode::KeyW);
            for (state, repeat) in [
                (ElementState::Pressed, false),
                (ElementState::Pressed, true),
                (ElementState::Released, false),
            ] {
                let event =
                    ControlInputEvent::keyboard_parts(PhysicalKey::Code(code), state, repeat);
                assert!(!session.route_event(
                    &event,
                    &keys,
                    false,
                    holds,
                    Default::default(),
                    true
                ));
                assert_eq!(session.panel, 0);
            }
        }
        // A modal ending while E1 is held must not turn its release into a pinned panel.
        let release = ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(KeyCode::KeyQ),
            ElementState::Released,
            false,
        );
        assert!(session.route_event(
            &release,
            &keys,
            true,
            (false, false),
            Default::default(),
            true
        ));
        assert_eq!(session.panel, 0);
        let press = ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(KeyCode::KeyQ),
            ElementState::Pressed,
            false,
        );
        assert!(session.route_event(&press, &keys, true, (true, false), Default::default(), true));
        assert_eq!(session.panel, 1, "fresh E1 must work again outside the modal");
    }

    #[test]
    fn tap_pins_hold_closes_and_next_press_closes_without_reopening() {
        for duration in [0, 1, 199, 200, 299, 300, 900] {
            let mut session = OptionPanelSession::default();
            session.edge(true, false, timestamp(0), Duration::ZERO);
            assert_eq!(session.panel, 1);
            session.edge(false, false, timestamp(duration), Duration::ZERO);
            assert_eq!(session.panel, if duration < 200 { 1 } else { 0 });
            if duration < 200 {
                session.edge(true, false, timestamp(2000), Duration::ZERO);
                assert_eq!(session.panel, 0);
                session.edge(false, false, timestamp(2100), Duration::ZERO);
                assert_eq!(session.panel, 0);
            }
        }
    }

    #[test]
    fn switches_on_fresh_e2_edges_in_both_hold_and_pinned_modes() {
        for pinned in [false, true] {
            let mut s = OptionPanelSession::default();
            s.edge(false, true, timestamp(0), Duration::ZERO);
            assert_eq!(s.panel, 0);
            s.edge(true, true, timestamp(0), Duration::ZERO);
            assert_eq!(s.panel, 1, "held E2 must not switch on opening");
            s.edge(true, false, timestamp(0), Duration::ZERO);
            if pinned {
                s.edge(false, false, timestamp(0), Duration::ZERO);
            }
            s.edge(!pinned, true, timestamp(0), Duration::ZERO);
            assert_eq!(s.panel, 2);
            s.edge(!pinned, true, timestamp(1000), Duration::ZERO);
            assert_eq!(s.panel, 2, "repeated press cannot toggle");
            s.edge(!pinned, false, timestamp(1000), Duration::ZERO);
            assert_eq!(s.panel, 2);
            s.edge(!pinned, true, timestamp(1000), Duration::ZERO);
            assert_eq!(s.panel, 1);
        }
    }

    #[test]
    fn release_order_reconciliation_and_modal_cancel_are_deterministic() {
        for e2_first in [false, true] {
            let mut s = OptionPanelSession::default();
            s.edge(true, true, timestamp(0), Duration::ZERO);
            assert_eq!(s.panel, 2);
            if e2_first {
                s.edge(true, false, timestamp(0), Duration::ZERO);
            }
            s.edge(false, !e2_first, timestamp(1000), Duration::ZERO);
            s.edge(false, false, timestamp(1000), Duration::ZERO);
            assert_eq!(s.panel, 0);
        }
        let mut s = OptionPanelSession::default();
        s.edge(true, false, timestamp(0), Duration::ZERO);
        s.reconcile(false, false);
        assert_eq!(s.panel, 0, "repaired release must not pin");
        s.reconcile(true, true);
        s.edge(true, true, timestamp(0), Duration::ZERO);
        assert_eq!(s.panel, 0, "resync must not open");
        s.cancel(true, true);
        s.edge(false, false, timestamp(0), Duration::ZERO);
        assert_eq!(s.panel, 0);
    }
}
