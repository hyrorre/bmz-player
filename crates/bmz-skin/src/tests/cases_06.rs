use super::*;

#[test]
fn lua_runtime_shared_scope_preserves_nested_state_and_callback_order() {
    let mut loaded = load_runtime_value_fixture(
        "bmz-skin-shared-scope",
        LuaSkinRuntimeMode::Compat,
        r#"
        local n = 0
        local number = main_state.number
        local number_value = function() n = n + 1; return number(999) + n end
        local text_value = function() return main_state.text(10) .. ':' .. n end
        "#,
    );
    let number = loaded.document.value[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
    let text = loaded.document.text[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let scope = runtime.state_scope();
    let mut outer = TestLuaMainState::default();
    outer.numbers.insert(999, 10);
    outer.texts.insert(10, "outer".into());
    let mut inner = TestLuaMainState::default();
    inner.numbers.insert(999, 20);
    inner.texts.insert(10, "inner".into());
    scope
        .with_state(&outer, || {
            assert_eq!(runtime.evaluate_number_in_scope(number), Some(11.0));
            assert_eq!(runtime.evaluate_number(number, &inner), Some(22.0));
            assert_eq!(runtime.evaluate_text_in_scope(text).as_deref(), Some("outer:2"));
            scope
                .with_state(&inner, || {
                    assert_eq!(runtime.evaluate_number_in_scope(number), Some(23.0));
                })
                .unwrap();
            assert_eq!(runtime.evaluate_number_in_scope(number), Some(14.0));
        })
        .unwrap();
    // No borrowed provider survives the scope, including after a panic.
    assert_eq!(runtime.evaluate_number_in_scope(number), None);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scope.with_state(&inner, || panic!("test scope unwind")).unwrap();
    }));
    assert!(panic.is_err());
    assert_eq!(runtime.evaluate_number_in_scope(number), None);
    assert_eq!(runtime.evaluate_number(number, &outer), Some(15.0));
}

#[test]
fn lua_runtime_shared_scope_keeps_frame_and_call_instruction_limits() {
    let mut loaded = load_runtime_draw_fixture(
        "bmz-skin-shared-scope-budget",
        "local count = 0; local draw = function() count = count + 1; local n = 0; for i = 1, 10000 do n = n + i end; return count % 2 == 1 end",
    );
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let scope = runtime.state_scope();
    scope
        .with_state(&TestLuaMainState::default(), || {
            runtime.begin_frame();
            for _ in 0..1000 {
                runtime.evaluate_draw_in_scope(0);
                if runtime.failure_log_count() > 0 {
                    break;
                }
            }
            assert_eq!(runtime.failure_log_count(), 1);
            assert!(!runtime.evaluate_draw_in_scope(0));
            runtime.begin_frame();
            assert_ne!(runtime.evaluate_draw_in_scope(0), runtime.evaluate_draw_in_scope(0));
        })
        .unwrap();
    let mut infinite = load_runtime_draw_fixture(
        "bmz-skin-shared-scope-infinite",
        "local draw = function() while true do end end",
    );
    let runtime = infinite.lua_runtime.as_mut().unwrap();
    runtime
        .state_scope()
        .with_state(&TestLuaMainState::default(), || {
            assert!(!runtime.evaluate_draw_in_scope(0));
        })
        .unwrap();
    assert_eq!(runtime.failure_log_count(), 1);
}

#[test]
fn rmz_skin_play6_decodes_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play6main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
        .expect("Rmz-skin play6 should decode");
    assert_eq!(loaded.document.skin_type, 23);
    assert!(!loaded.document.destination.is_empty());
    let fast_slow_draws = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "fast" || destination.id == "slow" =>
            {
                Some((destination.id.as_str(), destination.draw.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        fast_slow_draws.contains(&("fast", "option(1242) && number(525) != 0")),
        "Rmz play6 FAST draw should remain runtime-gated: {fast_slow_draws:?}"
    );
    assert!(
        fast_slow_draws.contains(&("slow", "option(1243) && number(525) != 0")),
        "Rmz play6 SLOW draw should remain runtime-gated: {fast_slow_draws:?}"
    );
    for (id, label, draw) in [
        ("lane-op-fran-tx", "F-RANDOM", "event_index(344) == 10"),
        ("lane-op-mfran-tx", "MF-RANDOM", "event_index(344) == 11"),
    ] {
        let text = loaded
            .document
            .text
            .iter()
            .find(|text| text.id == id && text.constant_text == label)
            .unwrap_or_else(|| panic!("Rmz play6 should decode {id} text"));
        assert_eq!(text.size, 30, "Rmz play6 {id} should match the sprite text height");
        assert_eq!(text.align, 1);
        let draws = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id == id =>
                {
                    Some(destination.draw.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(draws.contains(&draw), "Rmz play6 {id} should use {draw}, got {draws:?}");
    }
    let destination_frame = |id: &str| {
        loaded.document.destination.iter().find_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == id =>
            {
                destination.dst.first().and_then(|entry| match entry {
                    bmz_skin_document::SkinDstEntry::Frame(frame) => Some(*frame),
                    bmz_skin_document::SkinDstEntry::Conditional { .. } => None,
                })
            }
            _ => None,
        })
    };
    let sprite_frame = destination_frame("lane-op-tx").expect("Rmz arrange sprite destination");
    for id in ["lane-op-fran-tx", "lane-op-mfran-tx"] {
        let frame = destination_frame(id).unwrap_or_else(|| panic!("Rmz {id} destination"));
        assert_eq!(frame.x, sprite_frame.x.zip(sprite_frame.w).map(|(x, w)| x + w / 2));
        assert_eq!(frame.w, sprite_frame.w);
        assert_eq!(frame.h, sprite_frame.h);
    }
    let random_draw = (0..10)
        .map(|value| format!("event_index(344) == {value}"))
        .collect::<Vec<_>>()
        .join(" or ");
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "lane-op-tx" && destination.draw == random_draw
    )));
    let eon_shadow_draw = "timer(143) == timer_off and number(106)-number(110)-number(111)-number(112)-number(113)-number(114) == 0";
    let eon_destinations = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "eon" =>
            {
                Some((destination.timer, destination.draw.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        eon_destinations.iter().any(|(timer, _)| *timer == Some(143)),
        "Rmz play6 END_OF_NOTES animation should use timer 143: {eon_destinations:?}"
    );
    assert!(
        eon_destinations.iter().any(|(timer, draw)| timer.is_none() && *draw == eon_shadow_draw),
        "Rmz play6 END_OF_NOTES shadow should stay gated by remaining playable notes: {eon_destinations:?}"
    );
    let note = loaded.document.note.expect("play6 note definition");
    assert_eq!(note.note.len(), 6);
    assert_eq!(note.dst.len(), 6);
}

#[test]
fn rmz_skin_play5_keeps_default_lane_colors_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play5main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
        .expect("Rmz-skin play5 should decode");
    assert_eq!(loaded.document.skin_type, 1);
    assert!(
        loaded.document.property.iter().any(|property| property.name == "Notes 5Key Color"),
        "play5 should expose the lane color option"
    );
    let note = loaded.document.note.expect("play5 note definition");
    assert_eq!(note.note, vec!["note-Wh", "note-Bl", "note-Ye", "note-Bl", "note-Wh", "note-Sc"]);
    assert_eq!(note.dst.len(), 6);
}

#[test]
fn rmz_skin_play5_6key_like_colors_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play5main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([("Notes 5Key Color".to_string(), "6Key-like".to_string())]);
    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
        .expect("Rmz-skin play5 6Key-like colors should decode");
    let note = loaded.document.note.expect("play5 note definition");
    assert_eq!(note.note, vec!["note-Bl", "note-Wh", "note-Wh", "note-Bl", "note-Wh", "note-Wh"]);
    assert_eq!(note.dst.len(), 6);

    let options = BTreeMap::from([
        ("Scratch Side".to_string(), "Right".to_string()),
        ("Notes 5Key Color".to_string(), "6Key-like".to_string()),
    ]);
    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
        .expect("Rmz-skin play5 6Key-like right scratch colors should decode");
    let note = loaded.document.note.expect("play5 note definition");
    assert_eq!(note.note, vec!["note-Wh", "note-Bl", "note-Wh", "note-Wh", "note-Bl", "note-Wh"]);
    assert_eq!(note.dst.len(), 6);
}

#[test]
fn rmz_skin_play6_enlarge_uses_wide_note_lanes_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play6main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([("Notes 6Key Align".to_string(), "Enlarge".to_string())]);
    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
        .expect("Rmz-skin play6 enlarge should decode");
    let note = loaded.document.note.expect("play6 note definition");
    let widths: Vec<_> = note
        .dst
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::SkinDstEntry::Frame(frame) => frame.w,
            _ => None,
        })
        .collect();

    assert_eq!(widths, vec![132; 6]);
}

#[test]
fn rmz_skin_play4_decodes_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play4main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
        .expect("Rmz-skin play4 should decode");
    assert_eq!(loaded.document.skin_type, 22);
    assert!(!loaded.document.destination.is_empty());
    let note = loaded.document.note.expect("play4 note definition");
    assert_eq!(note.note, vec!["note-Wh", "note-Bl", "note-Bl", "note-Wh"]);
    assert_eq!(note.dst.len(), 4);
}

#[test]
fn rmz_skin_play4_enlarge_uses_wide_note_lanes_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rmz-skin/play4main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([("Notes 4Key Align".to_string(), "Enlarge".to_string())]);
    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
        .expect("Rmz-skin play4 enlarge should decode");
    let note = loaded.document.note.expect("play4 note definition");
    let widths: Vec<_> = note
        .dst
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::SkinDstEntry::Frame(frame) => frame.w,
            _ => None,
        })
        .collect();

    assert_eq!(widths, vec![132; 4]);
}

#[test]
fn peaceful_play_integral_property_ops_are_selectable_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/PeacefulPlay/play9.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
        .expect("PeacefulPlay play9 should decode");
    let property_warnings = loaded
        .warnings
        .iter()
        .filter(|warning| warning.message.contains("has no selectable op"))
        .map(|warning| warning.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        property_warnings.is_empty(),
        "PeacefulPlay properties should accept integral Lua-number ops: {property_warnings:?}"
    );
    let duration_info = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if matches!(
                    destination.id.as_str(),
                    "val-duration" | "val-lanecover-amount" | "val-duration-green"
                ) =>
            {
                Some(destination)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(duration_info.len(), 3);
    assert!(
        duration_info.iter().all(|destination| {
            destination.draw == "option(80) or option(81) and timer(40) == timer_off"
        }),
        "duration info: {duration_info:?}"
    );
    assert_eq!(
        loaded
            .document
            .value
            .iter()
            .find(|value| value.id == "val-hits-per-sec")
            .map(|value| value.value_expr.as_str()),
        Some("bmz:keylogger_nps")
    );
    let keylogger_graphs = loaded
        .document
        .graph
        .iter()
        .filter(|graph| graph.id.starts_with("keylogger-graph-"))
        .collect::<Vec<_>>();
    assert!(!keylogger_graphs.is_empty());
    assert!(
        keylogger_graphs
            .iter()
            .all(|graph| { graph.value_expr.starts_with("bmz:keylogger_graph:") })
    );
    let judge_color = load_lua_skin(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::from([("ノーツ色 Note Color".to_string(), "JUDGE".to_string())]),
        &BTreeMap::new(),
    )
    .expect("PeacefulPlay judge-color key logger should decode");
    let keylogger_notes = judge_color
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id.starts_with("keylogger-note-judge-") =>
            {
                Some(destination)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!keylogger_notes.is_empty());
    assert!(keylogger_notes.iter().all(|destination| {
        destination.timer_expr.starts_with("bmz:keylogger_event:")
            && destination.draw.starts_with("keylogger_judge(")
    }));
    let chattering = load_lua_skin(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::from([("下部表示情報 Bottom Info".to_string(), "CHATTERING ALERT".to_string())]),
        &BTreeMap::new(),
    )
    .expect("PeacefulPlay chattering alert should decode");
    let chattering_destinations = chattering
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id.starts_with("keylogger-chattering-alert-") =>
            {
                Some(destination)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(chattering_destinations.len(), 9);
    for (lane, destination) in chattering_destinations.iter().enumerate() {
        assert_eq!(destination.timer_expr, format!("bmz:keylogger_chattering:{}", lane + 1));
        assert!(destination.timer.is_none());
    }
    assert!(
        chattering
            .warnings
            .iter()
            .all(|warning| !warning.message.contains("unsupported field `timer`")),
        "chattering timer warnings: {:?}",
        chattering.warnings
    );
    let keybeams = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id.starts_with("key-beam-") =>
            {
                Some(destination)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(keybeams.len(), 9 * 4 * 2);
    for pair in keybeams.as_chunks::<2>().0 {
        assert!(pair[0].timer.is_none());
        assert!(pair[0].draw.starts_with("keybeam_hold("), "hold: {:?}", pair[0]);
        assert!(matches!(pair[1].timer, Some(120..=129)));
        assert!(pair[1].draw.starts_with("keybeam_fade("), "fade: {:?}", pair[1]);
    }
    assert_eq!(loaded.warnings.len(), 8, "warnings: {:?}", loaded.warnings);
    assert!(loaded.warnings.iter().all(|warning| {
        warning.message.starts_with("skipping unsupported custom timer function id 1190")
    }));
    let gauge_lead_glow = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id.starts_with("gauge-lead-glow-") =>
            {
                Some(destination)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(gauge_lead_glow.len(), 216);
    assert!(
        gauge_lead_glow
            .iter()
            .all(|destination| { destination.draw.starts_with("gauge_lead_glow(") }),
        "unexpected gauge predicates: {:?}",
        gauge_lead_glow
            .iter()
            .filter(|destination| !destination.draw.starts_with("gauge_lead_glow("))
            .map(|destination| (&destination.id, &destination.draw))
            .collect::<Vec<_>>()
    );
    let sevenkeys_path = skin_path.with_file_name("play7_9lane.luaskin");
    let sevenkeys =
        load_lua_skin(&sevenkeys_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
            .expect("PeacefulPlay play7_9lane should decode");
    assert!(sevenkeys.document.destination.iter().any(|entry| matches!(
        entry,
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "gauge-lead-glow-groove-below"
                && destination.draw.starts_with("gauge_lead_glow(groove,")
    )));
    assert_eq!(
        loaded.document.fixed_delay_timers,
        vec![bmz_skin_document::SkinFixedDelayTimerDef {
            id: 11900,
            source_timer: 143,
            delay_ms: 2000,
        }],
        "only PeacefulPlay's end-of-note fixed-delay timer should be inferred"
    );
}

#[test]
fn peaceful_play_gauge_overlay_keeps_one_destination_per_integer_width() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/PeacefulPlay/play9.luaskin");
    if !skin_path.is_file() {
        return;
    }

    for (display, mode, integer_id) in [
        ("%", "percent", "val-gauge-percent-integer"),
        ("Value", "amount", "val-gauge-amount-integer"),
    ] {
        let properties = BTreeMap::from([
            ("ゲージ量オーバーレイ Gauge Value Overlay".to_string(), "ON(100%)".to_string()),
            ("ゲージ量表示方式 Gauge Value Display Mode".to_string(), display.to_string()),
        ]);
        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &properties, &BTreeMap::new())
            .expect("PeacefulPlay gauge overlay should decode");
        assert_eq!(loaded.warnings.len(), 8, "{display} overlay warnings: {:?}", loaded.warnings);
        assert!(loaded.warnings.iter().all(|warning| {
            warning.message.starts_with("skipping unsupported custom timer function id 1190")
        }));
        let predicates = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id == integer_id =>
                {
                    Some(destination.draw.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            predicates,
            (1..=3)
                .map(|digits| format!("gauge_value_digits({mode},{digits})"))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn lua_static_boolean_draw_stays_static() {
    let loaded = load_runtime_draw_fixture("bmz-skin-static-bool-draw", "local draw = true");
    assert_eq!(only_destination_draw(&loaded), "number(0) >= 0");
    assert!(loaded.lua_runtime.is_none());
}

#[test]
fn lua_inferable_draw_keeps_compiled_path() {
    let loaded = load_runtime_draw_fixture(
        "bmz-skin-compiled-draw",
        "local draw = function() return main_state.option(46) end",
    );
    assert_eq!(only_destination_draw(&loaded), "option(46)");
    assert!(loaded.lua_runtime.is_none());
}

#[test]
fn lua_compat_mode_keeps_inferable_draw_in_runtime_vm() {
    let root = unique_test_dir("bmz-skin-compat-draw");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
            local main_state = require("main_state")
            return {
                type = 0,
                destination = {{
                    id = "runtime",
                    draw = function() return main_state.option(46) end,
                    dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                }}
            }
        "#,
    )
    .unwrap();
    let runtime_state =
        LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() };
    let mut loaded =
        load_lua_skin_with_runtime_state(&path, &BTreeMap::new(), &BTreeMap::new(), &runtime_state)
            .unwrap();
    assert_eq!(only_destination_draw(&loaded), "bmz:lua_draw_callback:0");
    let runtime = loaded.lua_runtime.as_mut().expect("compat runtime");
    let mut state = TestLuaMainState::default();
    assert!(!runtime.evaluate_draw(0, &state));
    state.options.insert(46, true);
    assert!(runtime.evaluate_draw(0, &state));
}

#[test]
fn lua_scene_state_syncs_existing_module_practice_boolean() {
    let root = unique_test_dir("bmz-skin-scene-practice-state");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("scene_state.lua"), "return { isPractice = false }").unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
            local main_state = require("main_state")
            local scene_state = require("scene_state")
            local is_auto = main_state.option(33)
            local function is_course()
                return main_state.option(290)
            end
            return {
                type = 0,
                destination = {
                    {
                        id = "demoplay",
                        draw = function() return is_auto and not is_course() end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                    },
                    {
                        id = "extrastage",
                        draw = function()
                            return not scene_state.isPractice and not is_auto and not is_course()
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                    },
                    {
                        id = "practice",
                        draw = function()
                            return scene_state.isPractice and not is_auto and not is_course()
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                    }
                }
            }
        "#,
    )
    .unwrap();

    for (name, practice, autoplay, course, expected_draws, expected_visibility) in [
        (
            "normal",
            false,
            false,
            false,
            ["number(0) < 0", "!option(290)", "number(0) < 0"],
            [false, true, false],
        ),
        (
            "practice",
            true,
            false,
            false,
            ["number(0) < 0", "number(0) < 0", "!option(290)"],
            [false, false, true],
        ),
        (
            "autoplay",
            false,
            true,
            false,
            ["!option(290)", "number(0) < 0", "number(0) < 0"],
            [true, false, false],
        ),
        (
            "course",
            false,
            false,
            true,
            ["number(0) < 0", "!option(290)", "number(0) < 0"],
            [false, false, false],
        ),
    ] {
        let option_values = BTreeMap::from([(33, autoplay), (290, course), (1080, practice)]);
        let compiled_state =
            LuaLoadRuntimeState { option_values: option_values.clone(), ..Default::default() };
        let mut compiled = load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &compiled_state,
        )
        .unwrap();
        let compiled_draws = compiled
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination) => {
                    Some(destination.draw.clone())
                }
                bmz_skin_document::DestinationListEntry::Conditional { .. } => None,
            })
            .collect::<Vec<_>>();
        let state = TestLuaMainState { options: option_values.clone(), ..Default::default() };
        for ((draw, expected), visible) in
            compiled_draws.iter().zip(expected_draws).zip(expected_visibility)
        {
            assert_compiled_or_runtime_draw(&mut compiled, draw, expected, visible, &state);
        }
        assert_eq!(
            compiled.dependencies.option_values.get(&1080),
            Some(&practice),
            "compiled Practice dependency for {name}"
        );

        let runtime_state = LuaLoadRuntimeState {
            runtime_mode: LuaSkinRuntimeMode::Compat,
            option_values: option_values.clone(),
            ..Default::default()
        };
        let mut loaded = load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .unwrap();
        assert_eq!(
            loaded.dependencies.option_values.get(&1080),
            Some(&practice),
            "Practice dependency for {name}"
        );
        let runtime = loaded.lua_runtime.as_mut().expect("compat runtime");
        let state = TestLuaMainState { options: option_values, ..Default::default() };
        let actual = [
            runtime.evaluate_draw(0, &state),
            runtime.evaluate_draw(1, &state),
            runtime.evaluate_draw(2, &state),
        ];
        assert_eq!(actual, expected_visibility, "stage visibility for {name}");
    }
}

#[test]
fn session_and_score_date_apis_track_runtime_state() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load_runtime_value_fixture(
            "bmz-session-state",
            mode,
            r#"
            local number_value = function() return main_state.total_play_counts_in_session() * 1000000 + main_state.total_play_notes_in_session() end
            local text_value = function() return os.date("%Y-%m-%d", main_state.score_date_sec_time()) end
        "#,
        );
        let number_id =
            loaded.document.value[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
        let text_id =
            loaded.document.text[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        for (counts, date, expected) in
            [((0, 0), 0, "1970-01-01"), ((3, 2345), 2208988800, "2040-01-01")]
        {
            let state =
                TestLuaMainState { session_counts: counts, score_date: date, ..Default::default() };
            assert_eq!(
                runtime.evaluate_number(number_id, &state),
                Some((counts.0 * 1000000 + counts.1) as f64)
            );
            assert_eq!(runtime.evaluate_text(text_id, &state).as_deref(), Some(expected));
        }
        assert_eq!(runtime.failure_log_count(), 0);
    }
    let mut loaded = load_runtime_draw_fixture(
        "bmz-score-date-draw",
        r#"
        local draw = function() return main_state.option(5) and main_state.score_date_sec_time() ~= 0 end
    "#,
    );
    let id = only_destination_draw(&loaded)
        .strip_prefix("bmz:lua_draw_callback:")
        .unwrap()
        .parse()
        .unwrap();
    for (date, expected) in [(0, false), (1700000000, true), (0, false)] {
        let state = TestLuaMainState {
            options: BTreeMap::from([(5, true)]),
            score_date: date,
            ..Default::default()
        };
        assert_eq!(loaded.lua_runtime.as_mut().unwrap().evaluate_draw(id, &state), expected);
    }
}

#[test]
fn captured_main_state_accessors_follow_each_runtime_state() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load_runtime_value_fixture(
            "bmz-captured-state",
            mode,
            r#"
            local number = main_state.number
            local text = main_state.text
            local calls = 0
            local number_value = function()
                calls = calls + 1
                if number(74) < 0 then error("invalid row") end
                return number(74) + calls
            end
            local text_value = function() return text(10) .. ":" .. number(74) end
        "#,
        );
        let number_id =
            loaded.document.value[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
        let text_id =
            loaded.document.text[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        for (notes, expected) in
            [(100, Some(101.0)), (200, Some(202.0)), (-1, None), (300, Some(304.0))]
        {
            let state = TestLuaMainState {
                numbers: BTreeMap::from([(74, notes)]),
                texts: BTreeMap::from([(10, "row".into())]),
                ..Default::default()
            };
            assert_eq!(runtime.evaluate_number(number_id, &state), expected);
            assert_eq!(runtime.evaluate_text(text_id, &state), Some(format!("row:{notes}")));
        }
        assert_eq!(runtime.failure_log_count(), 1);
    }
}

#[test]
fn dynamic_text_and_ratio_draw_do_not_freeze_at_load() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load_runtime_value_fixture(
            "bmz-dynamic-song-text",
            mode,
            r#"
            local number_value = 0
            local text_value = function()
                return string.format("TIME %d:%02d TOTAL %.0f GAUGE %.1f", main_state.number(1163), main_state.number(1164), main_state.number(368), main_state.number(368) / main_state.number(74))
            end
        "#,
        );
        let text = &loaded.document.text[0];
        assert!(text.constant_text.is_empty());
        let id = text.value_expr.rsplit(':').next().unwrap().parse().unwrap();
        for (notes, total, minutes, seconds, expected) in [
            (1000, 300, 2, 30, "TIME 2:30 TOTAL 300 GAUGE 0.3"),
            (500, 400, 1, 5, "TIME 1:05 TOTAL 400 GAUGE 0.8"),
        ] {
            let state = TestLuaMainState {
                numbers: BTreeMap::from([
                    (74, notes),
                    (368, total),
                    (1163, minutes),
                    (1164, seconds),
                ]),
                ..Default::default()
            };
            assert_eq!(
                loaded.lua_runtime.as_mut().unwrap().evaluate_text(id, &state).as_deref(),
                Some(expected)
            );
        }
    }
    let mut loaded = load_runtime_draw_fixture(
        "bmz-dynamic-total-draw",
        r#"
        local draw = function()
            local notes = main_state.number(74)
            return main_state.number(368) / (7.605 * notes / (0.01 * notes + 6.5)) < 0.8
        end
    "#,
    );
    let id = only_destination_draw(&loaded)
        .strip_prefix("bmz:lua_draw_callback:")
        .unwrap()
        .parse()
        .unwrap();
    for (total, visible) in [(300, true), (500, false), (200, true)] {
        let state = TestLuaMainState {
            numbers: BTreeMap::from([(74, 1000), (368, total)]),
            ..Default::default()
        };
        assert_eq!(loaded.lua_runtime.as_mut().unwrap().evaluate_draw(id, &state), visible);
    }
    let mut loaded = load_runtime_draw_fixture(
        "bmz-delayed-mutable-draw",
        r#"
        local calls = 0
        local draw = function() calls = calls + 1; return calls > 1000 end
    "#,
    );
    let id = only_destination_draw(&loaded)
        .strip_prefix("bmz:lua_draw_callback:")
        .unwrap()
        .parse()
        .unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    for i in 1..=1001 {
        runtime.begin_frame();
        assert_eq!(runtime.evaluate_draw(id, &TestLuaMainState::default()), i > 1000);
    }
}

#[test]
fn lua_next_rank_value_tracks_score_in_auto_and_compat_modes() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load_runtime_value_fixture(
            "bmz-skin-next-rank-value",
            mode,
            r#"
                local function next_rank_info()
                    local notes = main_state.number(74)
                    local score = main_state.exscore()
                    if notes <= 0 then return 7, 0 end
                    score = math.max(0, math.floor(score))
                    for _, target in ipairs({
                        {2, 7}, {3, 6}, {4, 5}, {5, 4},
                        {6, 3}, {7, 2}, {8, 1}, {9, 0}
                    }) do
                        local border = math.ceil(notes * 2 * target[1] / 9)
                        if score < border then return target[2], border - score end
                    end
                    return 0, 0
                end
                local number_value = function()
                    local _, diff = next_rank_info()
                    return diff
                end
                local text_value = "unused"
            "#,
        );
        // The fixture deliberately uses an arbitrary object ID, not diff_rank.
        let expr = &loaded.document.value[0].value_expr;
        assert!(
            expr.starts_with("bmz:lua_value_callback:"),
            "{mode:?}: {:?}",
            loaded.document.value[0]
        );
        let callback = expr.rsplit(':').next().unwrap().parse::<usize>().unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let mut state = TestLuaMainState::default();
        for (notes, score, expected) in [
            (0, 0, 0),
            (37, 0, 17),
            (37, 16, 1),
            (37, 17, 8),
            (37, 49, 1),
            (37, 50, 8),
            (37, 65, 1),
            (37, 66, 8),
            (37, 73, 1),
            (37, 74, 0),
            (9, 0, 4),
        ] {
            state.numbers.insert(74, notes);
            state.numbers.insert(71, score);
            runtime.begin_frame();
            assert_eq!(
                runtime.evaluate_number(callback, &state),
                Some(f64::from(expected)),
                "{mode:?}: notes={notes}, score={score}"
            );
        }
    }
}

#[test]
fn lua_numeric_closure_keeps_mutable_state_in_runtime() {
    let mut loaded = load_runtime_value_fixture(
        "bmz-skin-numeric-closure",
        LuaSkinRuntimeMode::Auto,
        r#"
            local count = 0
            local number_value = function()
                count = count + 1
                return count
            end
            local text_value = "unused"
        "#,
    );
    let expr = &loaded.document.value[0].value_expr;
    assert!(expr.starts_with("bmz:lua_value_callback:"), "{expr}");
    let callback = expr.rsplit(':').next().unwrap().parse::<usize>().unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let state = TestLuaMainState::default();
    assert_eq!(runtime.evaluate_number(callback, &state), Some(1.0));
    assert_eq!(runtime.evaluate_number(callback, &state), Some(2.0));
}

#[test]
fn wmii_beatoraja_branch_next_rank_updates_when_available() {
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    if !library.join("WMII_FHD/play/play7ac.luaskin").is_file() {
        return;
    }
    let root = unique_test_dir("bmz-skin-wmii-beatoraja-branch");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    for entry in ["play7ac", "play7wide"] {
        // Select the original skin's beatoraja branch without modifying assets.
        fs::write(
            &path,
            format!(
                r#"
                    bmz = nil
                    package.path = "skin/WMII_FHD/play/?.lua"
                    local skin = require("{entry}_main")
                    if skin_config then return skin.main() else return skin.header end
                "#
            ),
        )
        .unwrap();
        let context = SkinPathContext::new(&path, [root.clone(), library.clone()]).unwrap();
        for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
            for color in ["WHITE/RED", "WHITE ONLY"] {
                let mut loaded = load_lua_skin_with_path_context(
                    &context,
                    &BTreeMap::from([("GHOST COLOR".to_string(), color.to_string())]),
                    &BTreeMap::new(),
                    &LuaLoadRuntimeState {
                        runtime_mode: mode,
                        option_values: BTreeMap::from([(32, true), (33, false)]),
                        ..Default::default()
                    },
                    &BTreeMap::new(),
                )
                .unwrap();
                let value =
                    loaded.document.value.iter().find(|v| v.id == "diff_rank_next").unwrap_or_else(
                        || {
                            panic!(
                                "{entry}: {:?}",
                                loaded.document.value.iter().map(|v| &v.id).collect::<Vec<_>>()
                            )
                        },
                    );
                assert!(value.value_expr.starts_with("bmz:lua_value_callback:"), "{value:?}");
                let callback =
                    value.value_expr.rsplit(':').next().unwrap().parse::<usize>().unwrap();
                let runtime = loaded.lua_runtime.as_mut().unwrap();
                let mut state = TestLuaMainState::default();
                state.numbers.insert(74, 37);
                for (score, expected) in [(16, 1.0), (17, 8.0), (73, 1.0), (74, 0.0)] {
                    state.numbers.insert(71, score);
                    runtime.begin_frame();
                    assert_eq!(
                        runtime.evaluate_number(callback, &state),
                        Some(expected),
                        "{entry}/{mode:?}/{color}"
                    );
                }
            }
        }
    }
}

#[test]
fn lua_literal_numeric_function_stays_compiled() {
    let loaded = load_runtime_value_fixture(
        "bmz-skin-literal-number",
        LuaSkinRuntimeMode::Auto,
        "local number_value = function() return 42.5 end; local text_value = 'unused'",
    );
    assert_eq!(loaded.document.value[0].value_expr, "42.5");
    assert!(loaded.lua_runtime.is_none());
}

#[test]
fn lua_compat_mode_evaluates_number_and_text_functions_from_current_state() {
    let mut loaded = load_runtime_value_fixture(
        "bmz-skin-compat-values",
        LuaSkinRuntimeMode::Compat,
        r#"
            local number_value = function() return main_state.number(999) + 0.75 end
            local text_value = function() return "[" .. main_state.text(10) .. "]" end
        "#,
    );
    let number_expr = &loaded.document.value[0].value_expr;
    let text_expr = &loaded.document.text[0].value_expr;
    assert!(number_expr.starts_with("bmz:lua_value_callback:"));
    assert!(text_expr.starts_with("bmz:lua_value_callback:"));
    let number_callback = number_expr.rsplit(':').next().unwrap().parse::<usize>().unwrap();
    let text_callback = text_expr.rsplit(':').next().unwrap().parse::<usize>().unwrap();
    let runtime = loaded.lua_runtime.as_mut().expect("compat runtime");
    let mut state = TestLuaMainState::default();
    state.numbers.insert(999, 4);
    state.texts.insert(10, "first".to_string());
    assert_eq!(runtime.evaluate_number(number_callback, &state), Some(4.75));
    assert_eq!(runtime.evaluate_text(text_callback, &state).as_deref(), Some("[first]"));
    state.numbers.insert(999, 8);
    state.texts.insert(10, "updated".to_string());
    assert_eq!(runtime.evaluate_number(number_callback, &state), Some(8.75));
    assert_eq!(runtime.evaluate_text(text_callback, &state).as_deref(), Some("[updated]"));
}

#[test]
fn lua_stateful_draw_uses_clean_runtime_vm_and_runs_each_call() {
    let mut loaded = load_runtime_draw_fixture(
        "bmz-skin-stateful-runtime-draw",
        r#"
            local count = 0
            local draw = function()
                count = count + 1
                return count % 2 == 0
            end
            "#,
    );
    assert_eq!(only_destination_draw(&loaded), "bmz:lua_draw_callback:0");
    let runtime = loaded.lua_runtime.as_mut().expect("runtime fallback");
    let state = TestLuaMainState::default();
    // Inference invoked its own closure repeatedly. Runtime must still begin
    // at the untouched count=0 state and must not cache between calls.
    assert!(!runtime.evaluate_draw(0, &state));
    assert!(runtime.evaluate_draw(0, &state));
    assert!(!runtime.evaluate_draw(0, &state));
}

#[test]
fn lua_runtime_draw_reads_updated_main_state_each_call() {
    let mut loaded = load_runtime_draw_fixture(
        "bmz-skin-runtime-current-state",
        r#"
            local draw = function()
                if main_state.number(999) == 0 then
                    error("analysis values are intentionally unsupported")
                end
                return main_state.option(46)
                    and main_state.number(71) == 5
                    and main_state.float(72) > 1.5
                    and main_state.text(10) == "updated"
                    and main_state.timer(2) == 123
            end
            "#,
    );
    assert_eq!(only_destination_draw(&loaded), "bmz:lua_draw_callback:0");
    let runtime = loaded.lua_runtime.as_mut().expect("runtime fallback");
    let mut state = TestLuaMainState::default();
    state.numbers.insert(999, 1);
    assert!(!runtime.evaluate_draw(0, &state));
    state.options.insert(46, true);
    state.numbers.insert(71, 5);
    state.floats.insert(72, 2.0);
    state.texts.insert(10, "updated".to_string());
    state.timers.insert(2, 123);
    assert!(runtime.evaluate_draw(0, &state));
    state.texts.insert(10, "changed".to_string());
    assert!(!runtime.evaluate_draw(0, &state));
}

#[test]
fn lua_runtime_draw_reads_updated_main_state_offset_each_call() {
    let mut loaded = load_runtime_draw_fixture(
        "bmz-skin-runtime-current-offset",
        r#"
            local draw = function()
                if main_state.number(999) == 0 then
                    error("analysis values are intentionally unsupported")
                end
                local offset = main_state.offset(45)
                return offset.x == 1
                    and offset.y == 2
                    and offset.w == 3
                    and offset.h == 4
                    and offset.r == 5
                    and offset.a == -6
            end
            "#,
    );
    assert_eq!(only_destination_draw(&loaded), "bmz:lua_draw_callback:0");
    let runtime = loaded.lua_runtime.as_mut().expect("runtime fallback");
    let mut state = TestLuaMainState::default();
    state.numbers.insert(999, 1);
    assert!(!runtime.evaluate_draw(0, &state));
    state.offsets.insert(45, LuaSkinOffsetValue { x: 1, y: 2, w: 3, h: 4, r: 5, a: -6 });
    assert!(runtime.evaluate_draw(0, &state));
    state.offsets.get_mut(&45).unwrap().a = 0;
    assert!(!runtime.evaluate_draw(0, &state));
}

#[test]
fn lua_runtime_draw_errors_and_invalid_values_are_log_once_false() {
    for (name, source) in [
        ("bmz-skin-runtime-error", "local draw = function() error('expected test error') end"),
        ("bmz-skin-runtime-invalid-return", "local draw = function() return 'not boolean' end"),
        (
            "bmz-skin-runtime-missing-main-state-api",
            "local draw = function() return main_state.missing_api() end",
        ),
    ] {
        let mut loaded = load_runtime_draw_fixture(name, source);
        let runtime = loaded.lua_runtime.as_mut().expect("runtime fallback");
        let state = TestLuaMainState::default();
        assert!(!runtime.evaluate_draw(0, &state));
        assert!(!runtime.evaluate_draw(0, &state));
        assert_eq!(runtime.failure_log_count(), 1);
    }
}

#[test]
fn lua_runtime_draw_nil_is_false_without_a_failure() {
    let mut loaded = load_runtime_draw_fixture(
        "bmz-skin-runtime-nil-return",
        "local draw = function() return nil end",
    );
    let runtime = loaded.lua_runtime.as_mut().expect("runtime fallback");
    let state = TestLuaMainState::default();

    assert!(!runtime.evaluate_draw(0, &state));
    assert!(!runtime.evaluate_draw(0, &state));
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn lua_runtime_draw_instruction_limit_falls_back_to_false() {
    let mut loaded = load_runtime_draw_fixture(
        "bmz-skin-runtime-instruction-limit",
        "local draw = function() while true do end end",
    );
    let runtime = loaded.lua_runtime.as_mut().expect("runtime fallback");
    assert!(!runtime.evaluate_draw(0, &TestLuaMainState::default()));
    assert_eq!(runtime.failure_log_count(), 1);
}

#[test]
fn lua_runtime_frame_budget_recovers_even_when_playback_time_is_frozen() {
    let mut loaded = load_runtime_draw_fixture(
        "bmz-skin-runtime-frozen-frame",
        "local count = 0; local draw = function() count = count + 1; local n = 0; for i = 1, 10000 do n = n + i end; return count % 2 == 1 end",
    );
    let runtime = loaded.lua_runtime.as_mut().expect("runtime fallback");
    let state = TestLuaMainState::default();
    runtime.begin_frame();
    for _ in 0..1000 {
        runtime.evaluate_draw(0, &state);
        if runtime.failure_log_count() > 0 {
            break;
        }
    }
    assert_eq!(runtime.failure_log_count(), 1, "callbacks share a per-frame ceiling");
    assert!(!runtime.evaluate_draw(0, &state));
    runtime.begin_frame();
    let first = runtime.evaluate_draw(0, &state);
    let second = runtime.evaluate_draw(0, &state);
    assert_ne!(first, second, "a new frame restores callbacks at the same playback time");
}

#[test]
fn lua_to_json_rejects_runtime_draw_callbacks() {
    let root = unique_test_dir("bmz-skin-runtime-json-convert");
    fs::create_dir_all(&root).unwrap();
    let input = root.join("skin.luaskin");
    let output = root.join("skin.json");
    fs::write(
        &input,
        r#"
            local count = 0
            return {
                destination = {{
                    id = "runtime",
                    draw = function()
                        count = count + 1
                        return count % 2 == 0
                    end,
                    dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                }}
            }
            "#,
    )
    .unwrap();
    let error = convert_lua_skin_to_json_file(&input, &output, &BTreeMap::new(), &BTreeMap::new())
        .unwrap_err();
    assert!(error.to_string().contains("cannot serialize runtime callbacks"));
    assert!(error.to_string().contains("$.destination[1].draw"));
    assert!(!output.exists());
}
