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
                seed_scheme: crate::screens::play_start::SeedScheme::Beatoraja24,
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

#[cfg(target_os = "windows")]
#[test]
#[ignore = "creates a Winit event loop; run alone with --ignored"]
fn d_battle_select_preload_and_start_activate_the_dp_profile_slot() {
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
    // Distinct HS-FIX and duration values show which slot each step uses.
    let slots = [
        (KeyMode::K5, HsFixConfig::StartBpm),
        (KeyMode::K7, HsFixConfig::MinBpm),
        (KeyMode::K10, HsFixConfig::MaxBpm),
        (KeyMode::K14, HsFixConfig::MainBpm),
    ];
    for (mode, hs_fix) in slots {
        app.boot.profile_config.activate_play_mode(mode);
        app.boot.profile_config.play.hs_fix = hs_fix;
        app.boot.profile_config.lane.target_green_number = 240;
        if mode == KeyMode::K7 {
            app.boot.profile_config.play.lane_effect = LaneEffectConfig::HiddenSudden;
            let lane = &mut app.boot.profile_config.lane;
            lane.floating_policy = FloatingPolicyConfig::Locked;
            lane.target_green_number = 283;
            lane.sudden = 224;
            lane.lift = 136;
            lane.lift_enabled = true;
            lane.hidden = 150;
        }
        app.boot.profile_config.sync_active_play_mode();
    }
    let hs_fix_of =
        |mode: KeyMode| slots.iter().find(|(slot, _)| *slot == mode).map(|(_, hs_fix)| *hs_fix);
    let assert_slot = |app: &WinitApp, mode: KeyMode, context: &str| {
        assert_eq!(app.boot.profile_config.active_play_mode, mode, "{context}");
        assert_eq!(Some(app.boot.profile_config.play.hs_fix), hs_fix_of(mode), "{context}");
        assert_eq!(app.select.hs_fix_option, hs_fix_option_from_profile(hs_fix_of(mode).unwrap()));
    };
    app.select.select_items = vec![
        chart_row_with_mode(1, "7K"),
        chart_row_with_mode(2, "5K"),
        chart_row_with_mode(3, "14K"),
    ];
    for (index, expected) in [(0, 472), (2, 400), (0, 472)] {
        app.select.selected_index = index;
        assert_eq!(app.select_snapshot().note_display_duration_ms, Some(expected));
    }

    for (index, source, doubled) in [(0, KeyMode::K7, KeyMode::K14), (1, KeyMode::K5, KeyMode::K10)]
    {
        let chart_id = index as i64 + 1;
        app.select.selected_index = index;
        for (session, double, expected) in [
            (SessionMode::Normal, DoubleOption::Battle, doubled),
            (SessionMode::Normal, DoubleOption::BattleAutoScratch, doubled),
            (SessionMode::Normal, DoubleOption::Off, source),
            (SessionMode::GBattle, DoubleOption::Battle, source),
        ] {
            let context = format!("{source:?} {session:?} {double:?}");
            app.set_session_mode(session);
            app.select.double_option = double;

            // Select: the slot follows the selected double option.
            assert_eq!(app.selected_play_config_key_mode(), Some(expected), "{context}");
            app.sync_selected_play_mode();
            assert_eq!(app.boot.profile_config.active_play_mode, expected, "{context}");

            // Leave the other slot active, so preload and start must switch it.
            let other = if expected == source { doubled } else { source };
            app.boot.profile_config.activate_play_mode(other);
            let mut options = app.play_start_options();
            assert_eq!(options.double_option, double, "{context}");
            app.normalize_key_mode_conversion_options(chart_id, &mut options);
            assert_eq!(app.play_skin_key_mode_for_chart(chart_id, &options), expected, "{context}");

            assert_eq!(app.activate_play_start_key_mode(chart_id, &mut options), expected);
            assert_slot(&app, expected, &context);
            assert_eq!(options.hs_fix, app.select.hs_fix_option, "{context}");
        }
    }
    app.set_session_mode(SessionMode::Normal);
}
