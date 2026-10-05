//! Logical panel lifetime is independent of the physical E1/E2 holds.
use super::*;

const TAP_DURATION: Duration = Duration::from_millis(300);

#[derive(Default)]
pub(super) struct OptionPanelSession {
    pub panel: u8,
    pinned: bool,
    e1: bool,
    e2: bool,
    opened_at: Option<Instant>,
}

impl OptionPanelSession {
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
        self.detail_options_enabled()
            && self.select.select_option_panel == 0
            && self.select.option_panel_off_started_at[..2]
                .iter()
                .flatten()
                .any(|started| started.elapsed() < Duration::from_millis(300))
    }

    pub(super) fn route_select_option_session_event(&mut self, event: &ControlInputEvent) -> bool {
        if !self.detail_options_enabled() || !matches!(self.view_state(), AppViewState::Select) {
            return false;
        }
        let Some(control) = event.name.as_deref() else { return false };
        if !self.select.select_keys.is_start(control)
            && !self.select.select_keys.is_e2_action(control)
            && control != "Select"
        {
            return false;
        }
        if self.detail_options_available() && !event.repeat {
            self.select.option_session.edge(
                self.input.start_held,
                self.input.select_held,
                Instant::now(),
            );
            self.update_select_option_panel();
        }
        true
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
mod tests {
    use super::*;

    #[test]
    fn tap_pins_hold_closes_and_next_press_closes_without_reopening() {
        for duration in [0, 1, 299, 300, 900] {
            let mut session = OptionPanelSession::default();
            let now = Instant::now();
            session.edge(true, false, now);
            assert_eq!(session.panel, 1);
            session.edge(false, false, now + Duration::from_millis(duration));
            assert_eq!(session.panel, if duration < 300 { 1 } else { 0 });
            if duration < 300 {
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
