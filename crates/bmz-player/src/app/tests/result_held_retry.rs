use super::*;
use bmz_gameplay::input::backend::DeviceTimestamp;
use winit::platform::windows::EventLoopBuilderExtWindows;

fn enter_result(app: &mut WinitApp, chart_id: i64) {
    app.clear_pending_play_start();
    app.result.finished_play = Some(debug_boot_finished_play_session());
    app.result.finished_play.as_mut().unwrap().applied_arrange.seed = Some(42);
    app.result.finished_play.as_mut().unwrap().applied_arrange.arrange = ArrangeOption::Random;
    // A replay recorded before thresholds were stored: SAME must keep it.
    app.result.finished_play.as_mut().unwrap().applied_arrange.h_random_threshold_ms = None;
    app.play.last_started_chart_id = Some(chart_id);
    app.result.result_exit = None;
    app.result.result_key5_held = false;
    app.result.result_key7_held = false;
    app.result.result_scene_started_at = Instant::now() - Duration::from_secs(2);
    crate::skin_loader::set_decoded_skin_context(
        &mut app.renderer,
        SkinKind::Result,
        default_skin_manifest(),
        serde_json::from_value(serde_json::json!({
            "type": 7, "input": 0, "scene": 0, "fadeout": 3000,
            "destination": [{"id": "fade", "timer": 2,
                "dst": [{"time": 0}, {"time": 1000}]}]
        }))
        .unwrap(),
        None,
        vec![],
        false,
    );
}

fn control_event(app: &mut WinitApp, control: PhysicalControl, pressed: bool) {
    let event = match &control {
        PhysicalControl::KeyboardKey(name) => ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(match name.as_str() {
                "C" => KeyCode::KeyC,
                "V" => KeyCode::KeyV,
                _ => unreachable!(),
            }),
            if pressed { ElementState::Pressed } else { ElementState::Released },
            false,
        ),
        PhysicalControl::GamepadButton(name) => {
            ControlInputEvent::gamepad_button(&crate::input::gamepad::GamepadButtonEvent {
                name: name.clone(),
                device_id: DeviceId(16),
                pressed,
                timestamp: DeviceTimestamp::MonotonicNs(0),
                synthesized_analog_axis: false,
            })
        }
        _ => unreachable!(),
    };
    app.input.track_control(&event);
    app.track_result_lane_hold(&control, pressed);
}

#[test]
#[ignore = "requires an isolated Winit event loop and offscreen GPU; run alone with --ignored"]
fn result_retry_uses_holds_at_exit_after_taps_skips_and_physical_resets() {
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let (mut boot, path, _) = super::super::boot_chart::registered_charts(&data);
    boot.app_config.input.gamepad_enabled = false;
    let chart_id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
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
    app.renderer
        .attach_offscreen(bmz_render::renderer::SurfaceSize { width: 32, height: 32 })
        .unwrap();
    app.viewer_mode = false;
    app.viewer_waiting = false;

    for control in [
        PhysicalControl::KeyboardKey("C".into()),
        PhysicalControl::KeyboardKey("V".into()),
        PhysicalControl::GamepadButton("Button5".into()),
        PhysicalControl::GamepadButton("Button7".into()),
    ] {
        enter_result(&mut app, chart_id);
        control_event(&mut app, control.clone(), true);
        assert!(app.handle_result_control(&control, true, false, false));
        assert_eq!(app.result.result_exit.as_ref().unwrap().action, ResultExitAction::HeldLanes);
        app.advance_result_exit();
        assert!(app.result.finished_play.is_some(), "press must wait for the fadeout");
        assert!(app.play.pending_play_start.is_none());
        control_event(&mut app, control.clone(), false);
        app.result.result_exit.as_mut().unwrap().started_at =
            Instant::now() - Duration::from_secs(60);
        app.advance_result_exit();
        assert!(app.result.finished_play.is_none(), "tap leaves without retry");
        assert!(app.play.pending_play_start.is_none());

        enter_result(&mut app, chart_id);
        app.apply_result_action(ResultAction::Leave, false);
        control_event(&mut app, control.clone(), true);
        assert!(app.request_result_exit_skip_for_control(&control, true, false));
        let exit = app.result.result_exit.as_mut().unwrap();
        assert!(exit.skip_requested, "Key5/7 must skip like Key1/3");
        exit.started_at = Instant::now() - Duration::from_secs(60);
        app.advance_result_exit();
        assert!(app.result.result_exit.as_ref().unwrap().skip_final_frame_held);
        control_event(&mut app, control.clone(), false);
        app.advance_result_exit();
        assert!(app.result.finished_play.is_none(), "release on the final frame cancels retry");
        assert!(app.play.pending_play_start.is_none());
    }

    for held in [vec!["C"], vec!["V"], vec!["C", "V"]] {
        enter_result(&mut app, chart_id);
        app.apply_result_action(ResultAction::Leave, false);
        for name in &held {
            control_event(&mut app, PhysicalControl::KeyboardKey((*name).into()), true);
        }
        app.sync_result_lane_holds_from_pressed_controls();
        let mode =
            result_action_for_held_lanes(app.result.result_key5_held, app.result.result_key7_held)
                .unwrap();
        assert_eq!(
            mode,
            if held.contains(&"V") {
                ResultRetryMode::SameArrange
            } else {
                ResultRetryMode::DifferentArrange
            },
        );
        let retry_options = match mode {
            ResultRetryMode::SameArrange => app.result_retry_same_arrange_options(),
            ResultRetryMode::DifferentArrange => app.result_retry_different_arrange_options(),
        };
        if mode == ResultRetryMode::SameArrange {
            assert_eq!(retry_options.arrange_seed, Some(42));
            assert_eq!(retry_options.h_random_threshold_ms, None);
        } else {
            assert_eq!(
                retry_options.h_random_threshold_ms,
                Some(crate::screens::play_session::DEFAULT_H_RANDOM_THRESHOLD_MS)
            );
        }
        let preload_generation = app.play.play_preload_generation;
        app.result.result_exit.as_mut().unwrap().started_at =
            Instant::now() - Duration::from_secs(60);
        app.advance_result_exit();
        assert!(app.result.finished_play.is_none());
        // The isolated app has no audio output. Verify that retry launches a
        // new preload even if an immediately completed worker then fails to open audio.
        assert!(
            app.play.play_preload_generation > preload_generation,
            "{held:?} must retry rather than return directly to Select",
        );
        for name in held {
            control_event(&mut app, PhysicalControl::KeyboardKey(name.into()), false);
        }
    }

    enter_result(&mut app, chart_id);
    control_event(&mut app, PhysicalControl::KeyboardKey("C".into()), true);
    control_event(&mut app, PhysicalControl::GamepadButton("Button5".into()), true);
    control_event(&mut app, PhysicalControl::KeyboardKey("C".into()), false);
    app.sync_result_lane_holds_from_pressed_controls();
    assert!(app.result.result_key5_held, "another physical binding still holds Key5");
    // A backend resync/focus loss does not dispatch ordinary result release events.
    app.input.handle_focus_lost();
    app.begin_result_exit(ResultExitAction::Leave);
    app.result.result_exit.as_mut().unwrap().started_at = Instant::now() - Duration::from_secs(60);
    app.advance_result_exit();
    assert!(app.result.finished_play.is_none(), "stale hold must not retry after focus loss");
    assert!(app.play.pending_play_start.is_none());
}
