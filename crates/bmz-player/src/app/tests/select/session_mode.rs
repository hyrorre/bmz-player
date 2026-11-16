use super::*;

#[cfg(target_os = "linux")]
pub(super) fn assert_battle_session_mode_snapshots(app: &mut WinitApp) {
    for mode in SessionMode::VALUES {
        app.set_session_mode(mode);
        let expected = Some(crate::skin_extension::session_mode_index(mode));
        for open in [false, true, false] {
            if open {
                app.open_select_ir_battle();
            } else {
                app.close_select_ir_battle();
            }
            let snapshot = app.select_snapshot();
            assert_eq!(snapshot.skin_attempt.session_mode_index, expected, "{mode:?} open={open}");
            assert_eq!(snapshot.battle.is_some(), open);
            assert_eq!(app.select.session_mode, mode);
            if open {
                assert_eq!(app.select.ir_battle.cursor, 0);
                assert_eq!(snapshot.battle.as_ref().unwrap().name, "BATTLE OFF");
                assert_eq!(snapshot.assist, "G-BATTLE", "preserve legacy assist display");
            }
        }
    }
    app.set_session_mode(SessionMode::Normal);
    app.select.double_option = DoubleOption::Battle;
    assert_eq!(app.select_snapshot().skin_attempt.effective_key_mode, Some(KeyMode::K14));
    assert_eq!(app.selected_arrange_option_lane(Lane::Key8), Lane::Key8);
    app.open_select_ir_battle();
    let snapshot = app.select_snapshot();
    assert_eq!(snapshot.skin_attempt.session_mode_index, Some(0));
    assert_eq!(snapshot.skin_attempt.effective_key_mode, Some(KeyMode::K7));
    assert_eq!(snapshot.assist, "G-BATTLE");
    assert_eq!(app.selected_arrange_option_lane(Lane::Key8), Lane::Key1);
    app.execute_select_skin_event(SKIN_EVENT_SELECT_SESSION_MODE, -1);
    assert_eq!(app.select.session_mode, SessionMode::GBattle);
    assert_eq!(app.select_snapshot().skin_attempt.session_mode_index, Some(4));
    app.close_select_ir_battle();
    assert_eq!(app.select_snapshot().skin_attempt.session_mode_index, Some(4));

    // An opponent does not rewrite the setting. Decide freezes the mode in its
    // real entry path, even if the select setting subsequently changes.
    app.select.double_option = DoubleOption::Off;
    for (mode, index) in [(SessionMode::Normal, 0), (SessionMode::GBattle, 4)] {
        app.set_session_mode(mode);
        app.open_select_ir_battle();
        let mut options = app.play_start_options();
        options.battle_target = Some(crate::screens::play_start::BattleTarget {
            provider: "test".into(),
            score_id: "test".into(),
            player_id: "test".into(),
            player_name: "Opponent".into(),
            rank: 1,
            ex_score: 0,
            gauge: None,
            playback: crate::screens::play_start::BattleTargetPlayback::Seed {
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                packed_seed: None,
            },
        });
        let chart = app.selected_chart_row().unwrap().chart.clone().unwrap();
        app.begin_decide_for_chart_with_snapshot(
            chart.chart_id,
            options,
            RenderSnapshot::default(),
            None,
            Some(chart),
            DecideLaunch::Play,
        );
        app.advance_select_ir_battle_hold();
        assert!(!app.select.ir_battle.active, "screen transition closes a pinned list");
        assert_eq!(app.select.session_mode, mode);
        app.set_session_mode(mode.cycle());
        let decide = app.play.pending_decide.as_deref().unwrap();
        assert_eq!(app.decide_snapshot(decide).skin_attempt.session_mode_index, Some(index));
        app.play.pending_decide = None;
        app.invalidate_play_preload();
    }
}

#[test]
fn session_mode_click_directions_keep_beatoraja_button_and_region_rules() {
    let rect = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
    for button in [MouseButton::Left, MouseButton::Right] {
        let direction = if button == MouseButton::Left { 1 } else { -1 };
        assert_eq!(select_click_event_arg(0, button, rect, 0.25, 0.25), Some(direction));
        assert_eq!(select_click_event_arg(1, button, rect, 0.25, 0.25), Some(-direction));
        for (x, y, horizontal, vertical) in
            [(0.25, 0.25, -1, 1), (0.75, 0.25, 1, 1), (0.25, 0.75, -1, -1), (0.75, 0.75, 1, -1)]
        {
            assert_eq!(select_click_event_arg(2, button, rect, x, y), Some(horizontal));
            assert_eq!(select_click_event_arg(3, button, rect, x, y), Some(vertical));
            assert_eq!(select_click_event_arg(4, button, rect, x, y), None);
        }
        assert!(select_skin_click_allowed(SKIN_EVENT_SELECT_SESSION_MODE, button));
    }
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "creates a Winit event loop; run alone with --ignored"]
fn session_mode_skin_clicks_update_profile_like_keyboard_without_starting_play() {
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
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
    app.select.select_items = vec![chart_row_with_mode(1, "7K")];
    app.select.selected_index = 0;
    let profile_state = |app: &WinitApp| {
        (
            app.select.session_mode,
            app.boot.profile_config.play.session_mode,
            app.boot.profile_config.play.auto_play,
            app.select_snapshot().skin_attempt.session_mode_index,
        )
    };
    for initial in SessionMode::VALUES {
        app.set_session_mode(initial);
        assert!(app.test_apply_keyboard_option("V"));
        let keyboard_state = profile_state(&app);
        app.set_session_mode(initial);
        app.execute_select_skin_event(SKIN_EVENT_SELECT_SESSION_MODE, 1);
        assert_eq!(profile_state(&app), keyboard_state);
        for arg in [0, i32::MIN, i32::MAX] {
            app.set_session_mode(initial);
            app.execute_select_skin_event(SKIN_EVENT_SELECT_SESSION_MODE, arg);
            let expected = if arg > 0 {
                initial.cycle()
            } else if arg < 0 {
                initial.cycle_prev()
            } else {
                initial
            };
            assert_eq!(app.select.session_mode, expected);
        }
        for imageset in [false, true] {
            for click in 0..=4 {
                let document: SkinDocument = serde_json::from_value(serde_json::json!({
                    "type": 5, "w": 100, "h": 100,
                    "image": if imageset { serde_json::json!([]) } else {
                        serde_json::json!([{"id": "mode", "ref": 1970,
                            "act": -10006, "click": click}]) },
                    "imageset": if imageset { serde_json::json!([{"id": "mode",
                        "ref": 1970, "act": -10006, "click": click}]) }
                        else { serde_json::json!([]) },
                    "destination": [{"id": "mode", "dst": [{"x": 10, "y": 20, "w": 40, "h": 40}]}]
                }))
                .unwrap();
                for button in [MouseButton::Left, MouseButton::Right] {
                    for (x, y) in [(0.2, 0.5), (0.4, 0.7)] {
                        app.set_session_mode(initial);
                        let snapshot = app.select_snapshot();
                        let hit = document
                            .select_click_hit(
                                &Default::default(),
                                &snapshot,
                                &Default::default(),
                                x,
                                y,
                            )
                            .unwrap();
                        assert_eq!(
                            hit.target,
                            SkinClickTarget::Event {
                                event_id: SKIN_EVENT_SELECT_SESSION_MODE,
                                click,
                            }
                        );
                        let arg = select_click_event_arg(click, button, hit.rect, x, y);
                        let expected = match arg {
                            Some(value) if value > 0 => initial.cycle(),
                            Some(value) if value < 0 => initial.cycle_prev(),
                            _ => initial,
                        };
                        let timestamp = app.boot.profile_config.updated_at;
                        app.handle_select_skin_click(hit, button, x, y);
                        assert_eq!(
                            profile_state(&app),
                            (
                                expected,
                                Some(expected),
                                expected.primary_autoplay(),
                                Some(crate::skin_extension::session_mode_index(expected)),
                            )
                        );
                        assert_eq!(app.boot.profile_config.updated_at, timestamp);
                        assert!(app.play.pending_play_start.is_none());
                        assert!(app.play.active_play.is_none());
                        assert!(app.play.pending_decide.is_none());
                    }
                }
            }
        }
    }
}
