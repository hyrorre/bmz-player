use super::*;
use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::DeviceInputEvent;

fn timestamp(ms: u64) -> DeviceTimestamp {
    DeviceTimestamp::MonotonicNs(u128::from(ms) * 1_000_000)
}

fn bindings() -> SelectKeyBindings {
    let mut profile = crate::config::play_input::default_profile_input();
    for (action, control) in
        [(InputActionConfig::E1, "Button9"), (InputActionConfig::E2, "Button10")]
    {
        let mut binding =
            profile.ui.bindings.iter().find(|entry| entry.action == Some(action)).unwrap().clone();
        binding.device = "gamepad1".into();
        binding.control = control.into();
        profile.ui.bindings.push(binding);
    }
    SelectKeyBindings::from_profile(&profile)
}

struct PanelInput {
    input: AppInputRuntime,
    session: OptionPanelSession,
    keys: SelectKeyBindings,
    bounce: InputBounceConfig,
}

impl PanelInput {
    fn new(bounce_ms: u64) -> Self {
        Self {
            input: Default::default(),
            session: Default::default(),
            keys: bindings(),
            bounce: InputBounceConfig {
                keyboard_threshold_us: bounce_ms * 1000,
                controller_threshold_us: bounce_ms * 1000,
            },
        }
    }

    fn route(&mut self, event: ControlInputEvent, available: bool) -> bool {
        self.input.track_control(&event);
        reconcile_select_holds_from_pressed_controls(&mut self.input, &self.keys);
        let accepted = self
            .input
            .accept_app_event(
                self.bounce,
                DeviceInputEvent {
                    device: event.device,
                    control: event.physical.clone().unwrap(),
                    kind: if event.pressed { InputKind::Press } else { InputKind::Release },
                    timestamp: event.timestamp,
                    bounce_policy: Default::default(),
                },
            )
            .is_some();
        self.session.route_event(
            &event,
            &self.keys,
            available,
            (self.input.start_held, self.input.select_held),
            self.bounce,
            accepted,
        );
        accepted
    }

    fn button(&mut self, ms: u64, control: &str, pressed: bool) -> bool {
        self.route(
            ControlInputEvent::gamepad_button(&crate::input::gamepad::GamepadButtonEvent {
                name: control.into(),
                device_id: DeviceId(2),
                pressed,
                timestamp: timestamp(ms),
                synthesized_analog_axis: false,
            }),
            true,
        )
    }

    fn frame(&mut self, ms: u64) {
        self.session.reconcile(self.input.start_held, self.input.select_held);
        self.session.advance(u128::from(ms) * 1_000_000);
    }
}

#[test]
fn suppressed_press_and_final_release_continue_the_original_hold() {
    let mut state = PanelInput::new(20);
    assert!(state.button(0, "Button9", true));
    state.frame(0);
    assert!(state.button(5, "Button9", false));
    state.frame(5);
    assert!(!state.session.pinned, "a brief release must not pin immediately");
    assert!(!state.button(10, "Button9", true));
    state.frame(10);
    assert_eq!(state.session.opened_at, Some(timestamp(0)));
    state.button(300, "Button10", true);
    state.button(310, "Button10", false);
    assert_eq!(state.session.panel, 2);
    assert!(!state.button(1000, "Button9", false));
    state.frame(1000);
    assert_eq!(state.session.panel, 2);
    state.frame(1021);
    assert_eq!(state.session.panel, 0);
    assert!(!state.session.pinned);
}

#[test]
fn release_bounce_does_not_reopen_or_pin_the_panel() {
    let mut state = PanelInput::new(20);
    state.button(0, "Button9", true);
    for (ms, pressed) in [(1000, false), (1005, true), (1010, false)] {
        state.button(ms, "Button9", pressed);
        state.frame(ms);
        assert_eq!(state.session.panel, 1, "no intermediate close or reopen");
    }
    state.frame(1031);
    assert_eq!(state.session.panel, 0);
    assert!(!state.session.pinned);
}

#[test]
fn queued_inputs_and_debounce_keep_the_199_200ms_boundary() {
    for bounce_ms in [0, 20] {
        for duration in [0, 1, 100, 199, 200, 1000] {
            let mut state = PanelInput::new(bounce_ms);
            state.button(0, "Button9", true);
            state.button(duration, "Button9", false);
            // Both events may be consumed in one frame after a UI stall.
            state.frame(2000);
            assert_eq!(state.session.panel, u8::from(duration < 200));
            assert_eq!(state.session.pinned, duration < 200);
        }
    }
}

#[test]
fn queued_repress_at_the_bounce_deadline_keeps_the_hold() {
    for gap in [19, 20, 21] {
        let mut state = PanelInput::new(20);
        state.button(0, "Button9", true);
        state.button(5, "Button9", false);
        state.button(5 + gap, "Button9", true);
        // Drain the re-press before advancing time, even when it is very old.
        state.frame(500);
        assert_eq!(state.session.panel, u8::from(gap <= 20));
        state.button(1000, "Button9", false);
        state.frame(1021);
        assert_eq!(state.session.panel, 0);
    }
}

#[test]
fn closing_a_pinned_panel_does_not_reopen_after_contact_breaks() {
    let mut state = PanelInput::new(20);
    state.button(0, "Button9", true);
    state.button(100, "Button9", false);
    state.frame(121);
    assert!(state.session.pinned);
    state.button(200, "Button9", true);
    assert_eq!(state.session.panel, 0, "the next press closes immediately");
    state.button(205, "Button9", false);
    state.button(210, "Button9", true);
    state.frame(210);
    state.button(1000, "Button9", false);
    state.frame(1021);
    assert_eq!(state.session.panel, 0);
    state.button(1500, "Button9", true);
    assert_eq!(state.session.panel, 1, "a fresh gesture opens normally");
}

#[test]
fn keyboard_and_multiple_e1_sources_preserve_the_hold() {
    let mut state = PanelInput::new(20);
    state.button(0, "Button9", true);
    let mut keyboard = ControlInputEvent::keyboard_parts(
        PhysicalKey::Code(KeyCode::KeyQ),
        ElementState::Pressed,
        false,
    );
    keyboard.timestamp = timestamp(100);
    state.route(keyboard, true);
    state.button(150, "Button9", false);
    state.frame(500);
    assert!(state.input.start_held);
    assert!(state.session.pending_release.is_none());
    assert_eq!(state.session.opened_at, Some(timestamp(0)));
    for (ms, pressed) in [(600, false), (605, true), (1000, false)] {
        let mut event = ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(KeyCode::KeyQ),
            if pressed { ElementState::Pressed } else { ElementState::Released },
            false,
        );
        event.timestamp = timestamp(ms);
        state.route(event, true);
        state.frame(ms);
    }
    state.frame(1021);
    assert_eq!(state.session.panel, 0);
}

#[test]
fn e2_bounce_is_still_filtered_without_ending_an_e1_gesture() {
    let mut state = PanelInput::new(20);
    state.button(0, "Button9", true);
    state.button(100, "Button10", true);
    state.button(105, "Button10", false);
    assert!(!state.button(110, "Button10", true));
    assert!(!state.button(115, "Button10", false));
    state.frame(500);
    assert_eq!(state.session.panel, 2);
    assert_eq!(state.session.opened_at, Some(timestamp(0)));
}

#[test]
fn unavailable_panels_and_repaired_releases_never_pin() {
    for repaired in [false, true] {
        let mut state = PanelInput::new(20);
        state.button(0, "Button9", true);
        if repaired {
            state.session.reconcile(false, false);
        } else {
            state.button(100, "Button9", false);
            let event = ControlInputEvent::gamepad(DeviceId(2), "Button9", false);
            state.route(event, false);
        }
        state.session.advance(2_000_000_000);
        assert_eq!(state.session.panel, 0);
        assert!(state.session.pending_release.is_none());
        assert!(!state.session.pinned);
    }
}

#[test]
fn missing_or_reversed_timestamps_do_not_pin() {
    for (press, release) in [
        (DeviceTimestamp::Unknown, timestamp(100)),
        (timestamp(0), DeviceTimestamp::Unknown),
        (DeviceTimestamp::BackendTicks(1), DeviceTimestamp::BackendTicks(2)),
        (timestamp(100), timestamp(99)),
    ] {
        let mut state = PanelInput::new(20);
        for (pressed, at) in [(true, press), (false, release)] {
            let mut event = ControlInputEvent::gamepad(DeviceId(2), "Button9", pressed);
            event.timestamp = at;
            state.route(event, true);
        }
        state.frame(1000);
        assert_eq!(state.session.panel, 0);
        assert!(!state.session.pinned);
    }
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "creates a Winit event loop; run alone with --ignored"]
fn gamepad_route_keeps_suppressed_e1_edges_and_captured_timestamps() {
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let mut boot = data.boot();
    boot.app_config.input.gamepad_enabled = false;
    boot.app_config.select.experimental_detail_options = true;
    boot.profile_config.input.controller_release_bounce_ms = 20;
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
    app.select.select_keys = bindings();
    crate::skin_loader::set_decoded_skin_context(
        &mut app.renderer,
        crate::skin_loader::SkinKind::Select,
        bmz_render::skin::default_skin_manifest(),
        serde_json::from_value(serde_json::json!({
            "type": 5, "bmzDetailOptions": 1, "bmzDetailOptionsNumbers": true
        }))
        .unwrap(),
        None,
        vec![],
        false,
    );
    assert!(app.detail_options_available());
    for (ms, pressed) in [(0, true), (5, false), (10, true), (1000, false)] {
        app.route_gamepad_button_event(&crate::input::gamepad::GamepadButtonEvent {
            name: "Button9".into(),
            device_id: DeviceId(2),
            pressed,
            timestamp: timestamp(ms),
            synthesized_analog_axis: false,
        });
        assert_eq!(app.select.select_option_panel, 1);
    }
    // No wall-clock wait: the queued events themselves describe a one-second hold.
    app.select.option_session.advance(1_021_000_000);
    app.update_select_option_panel();
    assert_eq!(app.select.select_option_panel, 0);
    assert!(!app.select.option_session.pinned);

    app.boot.app_config.select.experimental_detail_options = false;
    app.cancel_select_option_session();
    for (ms, control, pressed, panel) in [
        (2000, "Button9", true, 1),
        (2100, "Button10", true, 3),
        (3000, "Button9", false, 2),
        (3100, "Button10", false, 0),
    ] {
        app.route_gamepad_button_event(&crate::input::gamepad::GamepadButtonEvent {
            name: control.into(),
            device_id: DeviceId(2),
            pressed,
            timestamp: timestamp(ms),
            synthesized_analog_axis: false,
        });
        assert_eq!(app.select.select_option_panel, panel, "OFF keeps the three hold panels");
    }
}
