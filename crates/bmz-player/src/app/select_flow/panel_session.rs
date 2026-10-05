//! Logical panel lifetime is independent of the physical E1/E2 holds.
use super::*;

const TAP_DURATION: Duration = Duration::from_millis(200);

#[derive(Default)]
pub(super) struct OptionPanelSession {
    pub panel: u8,
    pinned: bool,
    e1: bool,
    e2: bool,
    opened_at: Option<Instant>,
}

impl OptionPanelSession {
    fn route_event(
        &mut self,
        event: &ControlInputEvent,
        keys: &SelectKeyBindings,
        available: bool,
        holds: (bool, bool),
        now: Instant,
    ) -> bool {
        if !available {
            self.cancel(holds.0, holds.1);
            return false;
        }
        let Some(control) = event.name.as_deref() else { return false };
        if !keys.is_start(control) && !keys.is_e2_action(control) && control != "Select" {
            return false;
        }
        if !event.repeat {
            self.edge(holds.0, holds.1, now);
        }
        true
    }

    fn edge(&mut self, e1: bool, e2: bool, now: Instant) {
        let start_pressed = e1 && !self.e1;
        let start_released = !e1 && self.e1;
        let switch_pressed = e2 && !self.e2;
        self.e1 = e1;
        self.e2 = e2;
        if start_pressed {
            if self.pinned {
                self.panel = 0;
                self.pinned = false;
                self.opened_at = None;
            } else {
                self.panel = 1;
                self.opened_at = Some(now);
            }
        }
        if start_released && let Some(opened) = self.opened_at.take() {
            self.pinned = now.saturating_duration_since(opened) < TAP_DURATION;
            if !self.pinned {
                self.panel = 0;
            }
        }
        if switch_pressed && self.panel != 0 {
            self.panel = if self.panel == 1 { 2 } else { 1 };
        }
    }

    fn cancel(&mut self, e1: bool, e2: bool) {
        *self = Self { e1, e2, ..Default::default() };
    }

    // A repaired release or a device reconnect is never a tap/press gesture.
    fn reconcile(&mut self, e1: bool, e2: bool) {
        if self.e1 && !e1 && self.opened_at.is_some() {
            self.panel = 0;
            self.opened_at = None;
        }
        self.e1 = e1;
        self.e2 = e2;
    }
}

impl WinitApp {
    pub(super) fn option_panel_exit_blocks_input(&self) -> bool {
        panel_exit_blocks_input(
            self.detail_options_enabled() && self.detail_options_available(),
            self.select.select_option_panel,
            &self.select.option_panel_off_started_at,
            Instant::now(),
        )
    }

    pub(super) fn route_select_option_session_event(&mut self, event: &ControlInputEvent) -> bool {
        if !self.detail_options_enabled() || !matches!(self.view_state(), AppViewState::Select) {
            return false;
        }
        let available = self.detail_options_available();
        let consumed = self.select.option_session.route_event(
            event,
            &self.select.select_keys,
            available,
            (self.input.start_held, self.input.select_held),
            Instant::now(),
        );
        if consumed || !available {
            self.update_select_option_panel();
        }
        consumed
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

fn panel_exit_blocks_input(
    available: bool,
    panel: u8,
    off_started_at: &[Option<Instant>; 6],
    now: Instant,
) -> bool {
    available
        && panel == 0
        && off_started_at[..2]
            .iter()
            .flatten()
            .any(|started| now.saturating_duration_since(*started) < Duration::from_millis(300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_exit_animation_never_blocks_modal_input() {
        let base = Instant::now();
        for slot in [0, 1] {
            let mut off = [None; 6];
            off[slot] = Some(base);
            assert!(panel_exit_blocks_input(true, 0, &off, base));
            assert!(
                !panel_exit_blocks_input(false, 0, &off, base),
                "search/settings own input immediately"
            );
            assert!(!panel_exit_blocks_input(true, 1, &off, base));
            assert!(!panel_exit_blocks_input(true, 0, &off, base + Duration::from_millis(300)));
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
                session.edge(true, false, base);
                if detail {
                    session.edge(true, true, base);
                }
                if pinned {
                    session.edge(false, detail, base + Duration::from_millis(100));
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
    fn unavailable_panel_passes_modifier_keys_to_search_and_key_config() {
        let keys =
            SelectKeyBindings::from_profile(&crate::config::play_input::default_profile_input());
        let now = Instant::now();
        let mut session = OptionPanelSession::default();
        session.edge(true, false, now);
        session.edge(false, false, now);
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
                assert!(!session.route_event(&event, &keys, false, holds, now));
                assert_eq!(session.panel, 0);
            }
        }
        // A modal ending while E1 is held must not turn its release into a pinned panel.
        let release = ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(KeyCode::KeyQ),
            ElementState::Released,
            false,
        );
        assert!(session.route_event(&release, &keys, true, (false, false), now));
        assert_eq!(session.panel, 0);
        let press = ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(KeyCode::KeyQ),
            ElementState::Pressed,
            false,
        );
        assert!(session.route_event(&press, &keys, true, (true, false), now));
        assert_eq!(session.panel, 1, "fresh E1 must work again outside the modal");
    }

    #[test]
    fn tap_pins_hold_closes_and_next_press_closes_without_reopening() {
        for duration in [0, 1, 199, 200, 299, 300, 900] {
            let mut session = OptionPanelSession::default();
            let now = Instant::now();
            session.edge(true, false, now);
            assert_eq!(session.panel, 1);
            session.edge(false, false, now + Duration::from_millis(duration));
            assert_eq!(session.panel, if duration < 200 { 1 } else { 0 });
            if duration < 200 {
                session.edge(true, false, now + Duration::from_secs(2));
                assert_eq!(session.panel, 0);
                session.edge(false, false, now + Duration::from_millis(2100));
                assert_eq!(session.panel, 0);
            }
        }
    }

    #[test]
    fn switches_on_fresh_e2_edges_in_both_hold_and_pinned_modes() {
        for pinned in [false, true] {
            let mut s = OptionPanelSession::default();
            let t = Instant::now();
            s.edge(false, true, t);
            assert_eq!(s.panel, 0);
            s.edge(true, true, t);
            assert_eq!(s.panel, 1, "held E2 must not switch on opening");
            s.edge(true, false, t);
            if pinned {
                s.edge(false, false, t);
            }
            s.edge(!pinned, true, t);
            assert_eq!(s.panel, 2);
            s.edge(!pinned, true, t + Duration::from_secs(1));
            assert_eq!(s.panel, 2, "repeated press cannot toggle");
            s.edge(!pinned, false, t + Duration::from_secs(1));
            assert_eq!(s.panel, 2);
            s.edge(!pinned, true, t + Duration::from_secs(1));
            assert_eq!(s.panel, 1);
        }
    }

    #[test]
    fn release_order_reconciliation_and_modal_cancel_are_deterministic() {
        let t = Instant::now();
        for e2_first in [false, true] {
            let mut s = OptionPanelSession::default();
            s.edge(true, true, t);
            assert_eq!(s.panel, 2);
            if e2_first {
                s.edge(true, false, t);
            }
            s.edge(false, !e2_first, t + Duration::from_secs(1));
            s.edge(false, false, t + Duration::from_secs(1));
            assert_eq!(s.panel, 0);
        }
        let mut s = OptionPanelSession::default();
        s.edge(true, false, t);
        s.reconcile(false, false);
        assert_eq!(s.panel, 0, "repaired release must not pin");
        s.reconcile(true, true);
        s.edge(true, true, t);
        assert_eq!(s.panel, 0, "resync must not open");
        s.cancel(true, true);
        s.edge(false, false, t);
        assert_eq!(s.panel, 0);
    }
}
