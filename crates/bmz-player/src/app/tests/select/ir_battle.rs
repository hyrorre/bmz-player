//! Actual app input routes, without audio/GPU or a user's profile.
//! Run alone under X11 (Xvfb is sufficient); xdotool supplies one valid Winit
//! KeyEvent because its platform-specific field is private. Cases clone that
//! event and change its public key/state fields, then use route_keyboard_input.
use super::*;
use crate::config::key_config::{KeyBindingSlot, KeyBindingTarget, apply_play_binding};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::platform::x11::EventLoopBuilderExtX11;

struct KeySeed {
    window: Option<Window>,
    event: Option<winit::event::KeyEvent>,
    deadline: Instant,
}

impl ApplicationHandler<AppUserEvent> for KeySeed {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let title = format!("bmz-input-regression-{}", std::process::id());
        self.window = Some(
            event_loop.create_window(Window::default_attributes().with_title(&title)).unwrap(),
        );
        let result = std::process::Command::new("xdotool")
            .args(["search", "--name", &title, "windowfocus", "key", "--window", "0", "a"])
            .status()
            .expect("install xdotool for this isolated routing test");
        assert!(result.success());
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.deadline));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let WindowEvent::KeyboardInput { event, .. } = event {
            self.event = Some(event);
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if Instant::now() >= self.deadline {
            event_loop.exit();
        }
    }
}

fn keyboard(app: &mut WinitApp, seed: &winit::event::KeyEvent, key: KeyCode, pressed: bool) {
    let mut event = seed.clone();
    event.physical_key = PhysicalKey::Code(key);
    event.logical_key =
        winit::keyboard::Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
    event.text = None;
    event.state = if pressed { ElementState::Pressed } else { ElementState::Released };
    event.repeat = false;
    app.route_keyboard_input(&event);
}

fn gamepad(app: &mut WinitApp, device: DeviceId, name: &str, pressed: bool) {
    app.route_gamepad_button_event(&crate::input::gamepad::GamepadButtonEvent {
        device_id: device,
        name: name.into(),
        pressed,
        timestamp: bmz_gameplay::input::backend::DeviceTimestamp::MonotonicNs(
            bmz_gameplay::input::backend::monotonic_timestamp_ns(),
        ),
        synthesized_analog_axis: false,
    });
}

fn reset(app: &mut WinitApp, nine: bool, detail: bool) {
    app.select.list_loader.cancel();
    app.select.ir_battle = Default::default();
    app.input = AppInputRuntime::with_gamepad_slots(test_slots());
    app.ui.focused = true;
    app.select.search.set_active(false);
    app.select.folder_stack = vec!["outer".into(), "inner".into()];
    app.select.selected_index_stack = vec![0, 0];
    app.select.select_items = vec![chart_row_with_mode(1, "7K"), chart_row_with_mode(2, "7K")];
    app.select.selected_index = 0;
    app.select.select_option_panel = 0;
    app.select.option_session = Default::default();
    app.select.key_config_edit = None;
    app.boot.app_config.select.experimental_detail_options = detail;
    app.boot.profile_config.input = crate::config::play_input::default_profile_input();
    app.boot.profile_config.input.select_input_mode =
        if nine { SelectInputModeConfig::Key9 } else { SelectInputModeConfig::Key7Key14 };
    let mode = if nine { KeyMode::K9 } else { KeyMode::K7 };
    let lane = if nine { LaneConfig::Key8 } else { LaneConfig::Key4 };
    for (slot, control) in
        [(KeyBindingSlot::KeyboardPrimary, "Q"), (KeyBindingSlot::Controller, "Button24")]
    {
        apply_play_binding(
            &mut app.boot.profile_config.input,
            mode,
            KeyBindingTarget::Key { lane, slot },
            control,
        )
        .unwrap();
    }
    for (action, key, button) in [
        (InputActionConfig::E1, "LShift", "Button9"),
        (InputActionConfig::E2, "LControl", "Button10"),
    ] {
        for (slot, control) in
            [(KeyBindingSlot::KeyboardPrimary, key), (KeyBindingSlot::Controller, button)]
        {
            apply_play_binding(
                &mut app.boot.profile_config.input,
                mode,
                KeyBindingTarget::Action { action, slot },
                control,
            )
            .unwrap();
        }
    }
    app.select.select_keys = SceneInputResolver::from_profile(&app.boot.profile_config.input);
    crate::skin_loader::set_decoded_skin_context(
        &mut app.renderer,
        SkinKind::Select,
        default_skin_manifest(),
        serde_json::from_value(serde_json::json!({
            "type": 5, "bmzDetailOptions": 1, "bmzDetailOptionsNumbers": true
        }))
        .unwrap(),
        None,
        vec![],
        false,
    );
    app.set_session_mode(SessionMode::Normal);
    app.select.double_option = DoubleOption::Off;
}

fn assert_canceled(app: &mut WinitApp) {
    app.advance_select_ir_battle_hold();
    assert!(!app.select.ir_battle.active);
    assert!(app.select.ir_battle.hold_control.is_none());
    assert!(app.select.ir_battle.hold_started_at.is_none());
    assert!(app.select.ir_battle.hold_short_action.is_none());
    assert!(!app.select.list_loader.loading(), "cancellation must not execute Back");
}

#[test]
#[ignore = "isolated X11 event loop; run alone under Xvfb with xdotool installed"]
fn select_battle_input_and_snapshot_regressions() {
    let mut event_loop = EventLoop::<AppUserEvent>::with_user_event()
        .with_x11()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut seed =
        KeySeed { window: None, event: None, deadline: Instant::now() + Duration::from_secs(5) };
    event_loop.run_app_on_demand(&mut seed).unwrap();
    let window = seed.window.take().unwrap();
    let seed = seed.event.expect("xdotool must deliver a Winit keyboard event");
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let mut boot = data.boot();
    boot.app_config.input.gamepad_enabled = false;
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

    for nine in [false, true] {
        for controller in [false, true] {
            for detail in [false, true] {
                // KEY4/KEY8 at 0 ms, E1 at 20 ms, release at 40 ms, E1 still held.
                // No sleeps: move only the hold clock past 120 ms after releasing.
                reset(&mut app, nine, detail);
                let route = |app: &mut WinitApp, key, button: &str, pressed| {
                    if controller {
                        gamepad(app, DeviceId(16), button, pressed);
                    } else {
                        keyboard(app, &seed, key, pressed);
                    }
                };
                route(&mut app, KeyCode::KeyQ, "Button24", true);
                assert!(
                    app.select.ir_battle.hold_started_at.is_some(),
                    "nine={nine} controller={controller} detail={detail}"
                );
                route(&mut app, KeyCode::ShiftLeft, "Button9", true);
                assert_eq!(app.select.select_option_panel, 1);
                route(&mut app, KeyCode::KeyQ, "Button24", false);
                assert_canceled(&mut app);
                assert!(app.input.start_held);
                assert_eq!(app.select.select_option_panel, 1);

                // E1 -> E2 switches to DETAIL while the original input is held.
                reset(&mut app, nine, detail);
                route(&mut app, KeyCode::KeyQ, "Button24", true);
                route(&mut app, KeyCode::ShiftLeft, "Button9", true);
                route(&mut app, KeyCode::ControlLeft, "Button10", true);
                assert!(
                    app.select.select_option_panel > 1,
                    "nine={nine} controller={controller} detail={detail}"
                );
                route(&mut app, KeyCode::KeyQ, "Button24", false);
                assert_canceled(&mut app);

                // A real long hold opens temporarily and its original release closes.
                reset(&mut app, nine, detail);
                route(&mut app, KeyCode::KeyQ, "Button24", true);
                app.select.ir_battle.hold_started_at =
                    Some(Instant::now() - Duration::from_millis(120));
                app.advance_select_ir_battle_hold();
                assert!(app.select.ir_battle.active && !app.select.ir_battle.pinned);
                route(&mut app, KeyCode::KeyQ, "Button24", false);
                assert_canceled(&mut app);

                // Short KEY4 executes Back once; KEY8 has no short action.
                reset(&mut app, nine, detail);
                route(&mut app, KeyCode::KeyQ, "Button24", true);
                route(&mut app, KeyCode::KeyQ, "Button24", false);
                assert_eq!(app.select.list_loader.loading(), !nine);
                app.select.list_loader.cancel();
                route(&mut app, KeyCode::KeyQ, "Button24", false);
                assert!(!app.select.list_loader.loading());
            }
        }
    }

    // Same button name on another device cannot take over or release the owner.
    reset(&mut app, false, false);
    gamepad(&mut app, DeviceId(16), "Button24", true);
    let owner = app.select.ir_battle.hold_control.clone();
    gamepad(&mut app, DeviceId(17), "Button24", true);
    gamepad(&mut app, DeviceId(17), "Button24", false);
    assert_eq!(app.select.ir_battle.hold_control, owner);
    app.select.ir_battle.hold_started_at = Some(Instant::now() - Duration::from_millis(120));
    app.advance_select_ir_battle_hold();
    assert!(app.select.ir_battle.active);
    // A snapshot repairs a missed release/disconnect without executing Back.
    app.resync_gamepad_pressed_controls(&[]);
    assert_canceled(&mut app);
    app.resync_gamepad_pressed_controls(&[crate::input::gamepad::GamepadPressedButton {
        device_id: DeviceId(16),
        name: "Button24".into(),
    }]);
    assert_canceled(&mut app);

    for reason in ["focus", "search", "settings", "rebind", "resync", "selection", "transition"] {
        reset(&mut app, false, false);
        keyboard(&mut app, &seed, KeyCode::KeyQ, true);
        match reason {
            "focus" => {
                app.ui.focused = false;
                app.input.handle_focus_lost();
            }
            "search" => app.set_search_mode(true),
            "settings" => app
                .select
                .folder_stack
                .push(crate::screens::settings_model::CONFIG_ROOT_PATH.into()),
            "rebind" => {
                apply_play_binding(
                    &mut app.boot.profile_config.input,
                    KeyMode::K7,
                    KeyBindingTarget::Key {
                        lane: LaneConfig::Key4,
                        slot: KeyBindingSlot::KeyboardPrimary,
                    },
                    "W",
                )
                .unwrap();
                app.select.select_keys =
                    SceneInputResolver::from_profile(&app.boot.profile_config.input);
            }
            "resync" => {
                app.input.reconcile_keyboard_releases(&[PhysicalKey::Code(KeyCode::KeyQ)]);
            }
            "selection" => app.move_selection(SelectMove::Next),
            "transition" => {
                app.change_select_list(SelectListAction::Enter("other".into()));
                app.select.list_loader.cancel();
            }
            _ => unreachable!(),
        }
        assert_canceled(&mut app);
        keyboard(&mut app, &seed, KeyCode::KeyQ, false);
        assert_canceled(&mut app);
    }

    // Advance itself checks physical state, even if no release route/resync ran.
    reset(&mut app, false, false);
    keyboard(&mut app, &seed, KeyCode::KeyQ, true);
    app.input.pressed_play_inputs.clear();
    app.select.ir_battle.hold_started_at = Some(Instant::now() - Duration::from_millis(120));
    assert_canceled(&mut app);

    // Tab during a pending hold pins without inheriting the physical owner.
    reset(&mut app, false, false);
    keyboard(&mut app, &seed, KeyCode::KeyQ, true);
    keyboard(&mut app, &seed, KeyCode::Tab, true);
    assert!(app.select.ir_battle.active && app.select.ir_battle.pinned);
    assert!(app.select.ir_battle.hold_control.is_none());
    keyboard(&mut app, &seed, KeyCode::KeyQ, false);
    app.resync_gamepad_pressed_controls(&[]);
    app.sync_select_holds_from_pressed_controls();
    assert!(app.select.ir_battle.active && app.select.ir_battle.pinned);
    app.change_select_list(SelectListAction::Refresh);
    app.select.list_loader.cancel();
    app.advance_select_ir_battle_hold();
    assert!(app.select.ir_battle.active && app.select.ir_battle.pinned);
    keyboard(&mut app, &seed, KeyCode::Tab, false);
    keyboard(&mut app, &seed, KeyCode::Tab, true);
    assert_canceled(&mut app);
    for reason in ["escape", "focus", "transition"] {
        reset(&mut app, false, false);
        keyboard(&mut app, &seed, KeyCode::Tab, true);
        match reason {
            "escape" => keyboard(&mut app, &seed, KeyCode::Escape, true),
            "focus" => app.ui.focused = false,
            "transition" => {
                app.change_select_list(SelectListAction::Enter("other".into()));
                app.select.list_loader.cancel();
            }
            _ => unreachable!(),
        }
        assert_canceled(&mut app);
    }

    // A modal consumes input-owned gestures, while a pinned list keeps its
    // established lifetime (until an explicit close, focus loss or transition).
    reset(&mut app, false, false);
    keyboard(&mut app, &seed, KeyCode::KeyQ, true);
    let mut ui = EguiLayer::new(&window, false, vec![], app.jobs.ir_manual.clone());
    ui.toggle();
    app.ui.egui = Some(ui);
    keyboard(&mut app, &seed, KeyCode::KeyQ, false);
    assert_canceled(&mut app);
    let ui = app.ui.egui.take();
    reset(&mut app, false, false);
    keyboard(&mut app, &seed, KeyCode::Tab, true);
    app.ui.egui = ui;
    app.sync_select_holds_from_pressed_controls();
    assert!(app.select.ir_battle.active && app.select.ir_battle.pinned);
    app.ui.egui = None;

    reset(&mut app, false, false);
    super::session_mode::assert_battle_session_mode_snapshots(&mut app);
}
