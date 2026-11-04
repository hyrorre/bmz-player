use super::*;

#[test]
#[ignore = "creates a Winit event loop; run alone with --ignored"]
fn select_arrange_controls_follow_sp_dp_transitions_from_either_side() {
    use bmz_render::skin::SkinContext;
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let mut boot = data.boot();
    boot.app_config.input.gamepad_enabled = false;
    boot.app_config.input.gamepad_slot_device_ids =
        [Some("test:left".into()), Some("test:right".into())];
    let left_device = crate::input::gamepad::gamepad_device_id_from_stable_id("test:left");
    let right_device = crate::input::gamepad::gamepad_device_id_from_stable_id("test:right");
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
    app.select.select_mode_filter = SelectModeFilter::All;
    app.select.select_items = vec![
        chart_row_with_mode(1, "7K"),
        chart_row_with_mode(2, "14K"),
        chart_row_with_mode(3, "5K"),
        chart_row_with_mode(4, "10K"),
        SelectItem::Course(select_course_row(5, 2)),
    ];

    // Exercise the actual keyboard/controller entry points for both skin layouts.
    // Each case starts with distinct options so an unintended 2P edit is visible.
    for lr2 in [false, true] {
        let document = serde_json::from_value(serde_json::json!({
            "type": 5, "w": 100, "h": 100, "lr2": lr2,
        }))
        .unwrap();
        app.renderer.set_select_skin_context(SkinContext::from_manifest_and_document(
            default_skin_manifest(),
            document,
            Vec::new(),
        ));
        assert_eq!(app.lr2_select_active(), lr2);
        for (index, double, session, conversion, ir_battle, edits_2p) in [
            (0, DoubleOption::Off, SessionMode::Normal, KeyModeConversionConfig::Off, false, false),
            (1, DoubleOption::Off, SessionMode::Normal, KeyModeConversionConfig::Off, false, true),
            (0, DoubleOption::Off, SessionMode::Normal, KeyModeConversionConfig::Off, false, false),
            (2, DoubleOption::Off, SessionMode::Normal, KeyModeConversionConfig::Off, false, false),
            (3, DoubleOption::Off, SessionMode::Normal, KeyModeConversionConfig::Off, false, true),
            (
                0,
                DoubleOption::Battle,
                SessionMode::Normal,
                KeyModeConversionConfig::Off,
                false,
                true,
            ),
            (
                2,
                DoubleOption::BattleAutoScratch,
                SessionMode::Practice,
                KeyModeConversionConfig::Off,
                false,
                true,
            ),
            (
                0,
                DoubleOption::Off,
                SessionMode::Normal,
                KeyModeConversionConfig::SpToDp,
                false,
                true,
            ),
            (
                0,
                DoubleOption::Battle,
                SessionMode::GBattle,
                KeyModeConversionConfig::SpToDp,
                false,
                false,
            ),
            (
                0,
                DoubleOption::Off,
                SessionMode::AutoplayBattle,
                KeyModeConversionConfig::Off,
                false,
                false,
            ),
            (
                0,
                DoubleOption::Battle,
                SessionMode::Normal,
                KeyModeConversionConfig::Off,
                true,
                false,
            ),
            (1, DoubleOption::Off, SessionMode::GBattle, KeyModeConversionConfig::Off, false, true),
            // An unresolved/mixed course retains the existing side-specific controls.
            (4, DoubleOption::Off, SessionMode::Normal, KeyModeConversionConfig::Off, false, true),
        ] {
            app.select.selected_index = index;
            app.select.double_option = double;
            app.select.session_mode = session;
            app.select.ir_battle.active = ir_battle;
            app.boot.profile_config.play.key_mode_conversion = conversion;
            for keyboard in [false, true] {
                for right in [false, true] {
                    for previous in [false, true] {
                        if lr2 && previous {
                            continue; // LR2 KEY2 cycles forward on both sides.
                        }
                        app.select.arrange_option = ArrangeOption::Normal;
                        app.select.arrange_option_2p = ArrangeOption::Mirror;
                        let control = if keyboard {
                            match (lr2 || previous, right) {
                                (false, false) => "Z",
                                (true, false) => "S",
                                (false, true) => "M",
                                (true, true) => "K",
                            }
                        } else if lr2 || previous {
                            "Button2"
                        } else {
                            "Button1"
                        };
                        if keyboard {
                            app.apply_play_option_control(control);
                        } else {
                            app.apply_gamepad_play_option_control(
                                if right { right_device } else { left_device },
                                control,
                            );
                        }
                        let edits_2p = right && edits_2p;
                        let before =
                            if edits_2p { ArrangeOption::Mirror } else { ArrangeOption::Normal };
                        let after = if lr2 {
                            cycle_lr2_value(LR2_ARRANGES, before, 1)
                        } else if previous {
                            before.cycle_prev()
                        } else {
                            before.cycle()
                        };
                        assert_eq!(
                            (app.select.arrange_option, app.select.arrange_option_2p),
                            if edits_2p {
                                (ArrangeOption::Normal, after)
                            } else {
                                (after, ArrangeOption::Mirror)
                            },
                            "lr2={lr2}, row={index}, double={double:?}, session={session:?}, conversion={conversion:?}, ir={ir_battle}, keyboard={keyboard}, right={right}, previous={previous}",
                        );
                    }
                }
            }
        }
    }

    // Device assignment and custom button numbers still determine the physical side.
    app.select.selected_index = 0;
    app.select.double_option = DoubleOption::Off;
    app.select.session_mode = SessionMode::Normal;
    app.select.ir_battle.active = false;
    app.boot.profile_config.play.key_mode_conversion = KeyModeConversionConfig::Off;
    app.boot.app_config.input.gamepad_slot_device_ids =
        [Some("test:right".into()), Some("test:left".into())];
    let mut bindings = crate::config::play_input::default_play_14k_bindings();
    for binding in &mut bindings {
        if binding.device == "gamepad2" && binding.lane == Some(LaneConfig::Key9) {
            binding.control = "Button24".into();
        }
    }
    app.boot.profile_config.input.play.insert(
        "14k".into(),
        crate::config::profile_config::PlayModeInputConfig { bindings, ..Default::default() },
    );
    app.select.arrange_option = ArrangeOption::Normal;
    app.select.arrange_option_2p = ArrangeOption::Normal;
    app.apply_gamepad_play_option_control(left_device, "Button24");
    assert_eq!(app.select.arrange_option, ArrangeOption::Mirror);
    assert_eq!(app.select.arrange_option_2p, ArrangeOption::Normal);
    app.select.selected_index = 1;
    app.apply_gamepad_play_option_control(left_device, "Button24");
    assert_eq!(app.select.arrange_option, ArrangeOption::Mirror);
    assert_eq!(app.select.arrange_option_2p, ArrangeOption::Mirror);

    // A 9K select layout's KEY8/KEY9 are not the right side of a DP controller.
    app.select.selected_index = 0;
    app.boot.profile_config.input.select_input_mode = SelectInputModeConfig::Key9;
    assert_eq!(app.selected_arrange_option_lane(Lane::Key8), Lane::Key8);
    assert_eq!(app.selected_arrange_option_lane(Lane::Key9), Lane::Key9);
}
