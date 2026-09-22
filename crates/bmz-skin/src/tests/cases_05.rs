use super::*;

#[test]
fn lua_skin_infers_draw_with_skin_config_option_and_number() {
    let root = unique_test_dir("bmz-skin-lua-skin-config-draw");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local main_state = require("main_state")
            return {
                type = 0,
                property = {
                    {
                        name = "mybest スコアが存在しない時",
                        def = "976",
                        item = {
                            { name = "976", op = 976 },
                            { name = "off", op = 0 },
                        },
                    },
                },
                destination = {
                    {
                        id = "score-diff",
                        draw = function()
                            return main_state.number(150) == 0
                                and skin_config.option["mybest スコアが存在しない時"] == 976
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
            }
            "#,
    )
    .unwrap();

    let mut options = BTreeMap::new();
    options.insert("mybest スコアが存在しない時".to_string(), "976".to_string());
    let loaded =
        load_lua_skin(&root.join("play7.luaskin"), SkinKind::Play, &options, &BTreeMap::new())
            .unwrap();
    assert!(
        loaded.warnings.is_empty(),
        "warnings: {:?}",
        loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
    );
    let bmz_skin_document::DestinationListEntry::Single(destination) =
        &loaded.document.destination[0]
    else {
        panic!("expected single destination");
    };
    assert_eq!(destination.draw, "number(150) == 0");
}

#[test]
fn lua_skin_infers_skin_config_only_draw() {
    let root = unique_test_dir("bmz-skin-lua-skin-config-only-draw");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            return {
                type = 0,
                property = {
                    {
                        name = "グルーヴゲージ表示",
                        def = "default",
                        item = {
                            { name = "default", op = 930 },
                            { name = "all_off", op = 932 },
                        },
                    },
                },
                destination = {
                    {
                        id = "gaugevalue",
                        draw = function()
                            return skin_config.option["グルーヴゲージ表示"] ~= 932
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
            }
            "#,
    )
    .unwrap();

    let loaded = load_lua_skin(
        &root.join("play7.luaskin"),
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        loaded.warnings.is_empty(),
        "warnings: {:?}",
        loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
    );
    let bmz_skin_document::DestinationListEntry::Single(destination) =
        &loaded.document.destination[0]
    else {
        panic!("expected single destination");
    };
    assert_eq!(destination.draw, "number(0) >= 0");
}

#[test]
fn lua_skin_infers_fast_slow_ratio_graph_type() {
    let root = unique_test_dir("bmz-skin-lua-fs-graph");
    fs::create_dir_all(&root).unwrap();
    let refs = [410, 411, 412, 413, 414, 415, 416, 417, 418, 419, 421, 422];
    let sum_lines: String = refs
        .iter()
        .map(|ref_id| format!("main_state.number({ref_id})"))
        .collect::<Vec<_>>()
        .join(" + ");
    fs::write(
        root.join("select.luaskin"),
        format!(
            r#"
            local main_state = require("main_state")
            return {{
                type = 0,
                graph = {{
                    {{
                        id = "fast",
                        src = 1,
                        x = 0,
                        y = 0,
                        w = 10,
                        h = 10,
                        value = function()
                            local fastall = main_state.number(410) + main_state.number(412)
                                + main_state.number(414) + main_state.number(416)
                                + main_state.number(418) + main_state.number(421)
                            local fsall = {sum_lines}
                            if fsall == 0 then return 0 end
                            return fastall / fsall
                        end,
                    }},
                    {{
                        id = "slow",
                        src = 1,
                        x = 0,
                        y = 0,
                        w = 10,
                        h = 10,
                        value = function()
                            local slowall = main_state.number(411) + main_state.number(413)
                                + main_state.number(415) + main_state.number(417)
                                + main_state.number(419) + main_state.number(422)
                            local fsall = {sum_lines}
                            if fsall == 0 then return 0 end
                            return slowall / fsall
                        end,
                    }},
                }},
            }}
            "#
        ),
    )
    .unwrap();

    let loaded = load_lua_skin(
        &root.join("select.luaskin"),
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        loaded.warnings.is_empty(),
        "warnings: {:?}",
        loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(loaded.document.graph[0].graph_type, 148);
    assert_eq!(loaded.document.graph[1].graph_type, 149);
    assert!(loaded.document.graph[0].value_expr.is_empty());
    assert!(loaded.document.graph[1].value_expr.is_empty());
}

#[test]
fn lua_skin_stops_infinite_loop() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("play7.luaskin"), "while true do end").unwrap();

    let err = load_lua_skin(
        &root.join("play7.luaskin"),
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("instruction limit"));
}

#[test]
fn lua_skin_stops_infinite_inference_callback() {
    let root = unique_test_dir("bmz-skin-lua-inference-limit");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            return {
                type = 0,
                value = {{
                    id = "loop",
                    value = function() while true do end end,
                }},
            }
            "#,
    )
    .unwrap();

    let mut loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .expect("an uninferrable callback should become a bounded runtime callback");
    assert_eq!(loaded.runtime_callback_paths, ["$.value[1].value"]);
    let runtime = loaded.lua_runtime.as_mut().expect("runtime value fallback");
    assert_eq!(runtime.evaluate_number(0, &TestLuaMainState::default()), None);
    assert_eq!(runtime.failure_log_count(), 1);
}

#[test]
fn normalize_lua_skin_category_map_converts_rm_skin_shape() {
    let value = JsonValue::Object(JsonMap::from_iter([(
        "category".to_string(),
        JsonValue::Object(JsonMap::from_iter([
            (
                "property".to_string(),
                JsonValue::Object(JsonMap::from_iter([
                    ("name".to_string(), JsonValue::String("Option".to_string())),
                    ("item".to_string(), JsonValue::Array(vec![])),
                ])),
            ),
            (
                "filepath".to_string(),
                JsonValue::Object(JsonMap::from_iter([
                    ("name".to_string(), JsonValue::String("Image".to_string())),
                    ("item".to_string(), JsonValue::Array(vec![])),
                ])),
            ),
        ])),
    )]));
    let normalized = normalize_lua_skin_category_map(value);
    let JsonValue::Object(map) = normalized else {
        panic!("expected object");
    };
    let JsonValue::Array(categories) = map.get("category").expect("category") else {
        panic!("expected category array");
    };
    assert_eq!(categories.len(), 2);
}

#[test]
fn normalize_lua_skin_category_labels_stringifies_modern_chic_ids() {
    let value = serde_json::json!({
        "category": [{ "name": 10, "item": [11, 12] }],
        "property": [{ "name": "Option", "category": 11, "item": [], "def": "" }],
        "filepath": [{ "name": "Image", "path": "*.png", "category": 12, "def": "" }],
        "offset": [{ "name": "Offset", "id": 40, "category": 13, "a": 0 }]
    });

    let normalized = normalize_lua_skin_document(value);

    assert_eq!(normalized["category"][0]["name"], "10");
    assert_eq!(normalized["category"][0]["item"][0], "11");
    assert_eq!(normalized["category"][0]["item"][1], "12");
    assert_eq!(normalized["property"][0]["category"], "11");
    assert_eq!(normalized["filepath"][0]["category"], "12");
    assert_eq!(normalized["offset"][0]["category"], "13");
    let document = serde_json::from_value::<SkinDocument>(normalized)
        .expect("normalized Lua header values should decode as a skin document");
    assert!(document.offset[0].a, "Lua numeric zero is truthy");
}

#[test]
fn normalize_lua_skin_offset_map_converts_skin_config_shape() {
    let value = JsonValue::Object(JsonMap::from_iter([(
        "offset".to_string(),
        JsonValue::Object(JsonMap::from_iter([(
            "Song title".to_string(),
            JsonValue::Object(JsonMap::from_iter([
                ("id".to_string(), JsonValue::Number(serde_json::Number::from(60))),
                ("name".to_string(), JsonValue::String("Song title".to_string())),
                ("y".to_string(), JsonValue::Bool(true)),
            ])),
        )])),
    )]));
    let normalized = normalize_lua_skin_offset_map(value);
    let JsonValue::Object(map) = normalized else {
        panic!("expected object");
    };
    let JsonValue::Array(offsets) = map.get("offset").expect("offset") else {
        panic!("expected offset array");
    };
    assert_eq!(offsets.len(), 1);
}

#[test]
fn normalize_lua_skin_offset_map_wraps_single_offset_def() {
    let value = JsonValue::Object(JsonMap::from_iter([(
        "offset".to_string(),
        JsonValue::Object(JsonMap::from_iter([
            ("id".to_string(), JsonValue::Number(serde_json::Number::from(60))),
            ("name".to_string(), JsonValue::String("Song title".to_string())),
            ("y".to_string(), JsonValue::Bool(true)),
        ])),
    )]));
    let normalized = normalize_lua_skin_offset_map(value);
    let JsonValue::Object(map) = normalized else {
        panic!("expected object");
    };
    let JsonValue::Array(offsets) = map.get("offset").expect("offset") else {
        panic!("expected offset array");
    };
    assert_eq!(offsets.len(), 1);
}

#[test]
fn m_select_lua_select_skin_loads_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/music_select.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let loaded =
        load_lua_skin(&skin_path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(loaded.document.skin_type, 5);
    assert!(loaded.document.songlist.is_some());
    let version = loaded
        .document
        .text
        .iter()
        .find(|text| text.id == "default_version")
        .expect("m-select version text should decode");
    assert_eq!(version.constant_text, env!("CARGO_PKG_VERSION"));
    for ref_id in 27..=29 {
        assert!(
            loaded.document.value.iter().any(|value| value.ref_id == ref_id),
            "m-select should retain operating-time ref {ref_id}"
        );
    }
    for id in ["bmz_select_gauge", "bmz_select_double_option", "bmz_select_hs_fix"] {
        assert!(
            loaded.document.text.iter().all(|text| text.id != id),
            "m-select should use option images instead of dynamic {id} text"
        );
    }
    for (id, ref_id, image_count, left_x) in [
        ("default_stateplayoption_option_gauge", 40, 6, 628),
        ("default_stateplayoption_option_dp", 54, 4, 794),
        ("default_stateplayoption_option_hsfix", 55, 5, 960),
    ] {
        let imageset = loaded
            .document
            .imageset
            .iter()
            .find(|imageset| imageset.id == id)
            .unwrap_or_else(|| panic!("m-select should decode {id}"));
        assert_eq!(imageset.ref_id, ref_id);
        assert_eq!(imageset.images.len(), image_count);
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == id
                    && destination.act.is_none()
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_skin_document::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x) && frame.w == Some(166)
            )
        )));
    }
    for (id, ref_id, left_x) in [
        ("default_stateplayoption_random", 344, 462),
        ("default_stateplayoption_random_2p", 345, 1126),
    ] {
        let imageset = loaded
            .document
            .imageset
            .iter()
            .find(|imageset| imageset.id == id)
            .unwrap_or_else(|| panic!("m-select should decode {id}"));
        assert_eq!(imageset.ref_id, ref_id);
        assert_eq!(imageset.images.len(), 12);
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == id
                    && destination.act.is_none()
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_skin_document::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x) && frame.w == Some(166)
            )
        )));
    }
    for (id, ref_id, left_x) in [
        ("default_optionpanel_option_random", 344, 318),
        ("default_optionpanel_option_random2", 345, 1118),
    ] {
        let imageset = loaded
            .document
            .imageset
            .iter()
            .find(|imageset| imageset.id == id)
            .unwrap_or_else(|| panic!("m-select should decode {id}"));
        assert_eq!(imageset.ref_id, ref_id);
        assert_eq!(imageset.images.len(), 12);
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == id
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_skin_document::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x)
                                && frame.y == Some(40)
                                && frame.w == Some(170)
                                && frame.h == Some(600)
                    )
        )));
    }
    assert!(loaded.document.source.iter().any(|source| {
        source.id == "src-default-optionpanel-panel1"
            && source.path.ends_with("default_optionpanel4/panel1_bmz.png")
    }));
    assert!(loaded.document.source.iter().any(|source| {
        source.id == "src-default-optionpanel-random-cursor-bmz"
            && source.path.ends_with("default_optionpanel4/random_cursor_bmz.png")
    }));
    assert!(loaded.document.image.iter().any(|image| {
        image.id == "default_optionpanel_option_panel1" && image.w == 1315 && image.h == 1124
    }));
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "default_optionpanel_option_panel1"
                && matches!(
                    destination.dst.first(),
                    Some(bmz_skin_document::SkinDstEntry::Frame(frame))
                        if frame.y == Some(-22) && frame.w == Some(1315) && frame.h == Some(1124)
                )
    )));
    assert!(
        loaded
            .document
            .panel
            .iter()
            .any(|panel| panel.id == "bmz_select_option_hit" && panel.color == "00000000")
    );
    for (act, left_x) in [(42, 462), (40, 628), (54, 794), (55, 960), (43, 1126)] {
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "bmz_select_option_hit"
                    && destination.act == Some(act)
                    && destination.click == 2
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_skin_document::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x) && frame.w == Some(166)
                    )
        )));
    }
}

#[test]
fn modern_chic_lua_select_header_loads_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/ModernChic/musicselect.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let loaded = load_lua_skin_header_value(&skin_path).unwrap();
    let document: SkinDocument = serde_path_to_error::deserialize(loaded.value)
        .unwrap_or_else(|error| panic!("decode {} header: {error:#}", skin_path.display()));
    assert_eq!(document.skin_type, 5);
}

#[test]
fn lua_skin_infers_rm_skin_score_diff_draw() {
    let root = unique_test_dir("bmz-skin-rm-score-diff");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local main_state = require("main_state")
            return {
                type = 0,
                destination = {
                    {
                        id = "score-diff-best",
                        draw = function()
                            return main_state.float_number(113) == 0 and main_state.number(152) ~= 0
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                    {
                        id = "score-diff-zero",
                        draw = function()
                            return not (main_state.number(153) ~= 0)
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
            }
            "#,
    )
    .unwrap();

    let loaded = load_lua_skin(
        &root.join("play7.luaskin"),
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let draws: Vec<_> = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(d) => Some(d.draw.as_str()),
            _ => None,
        })
        .collect();
    assert!(draws.contains(&"float_number(113) == 0 && number(152) != 0"));
    assert!(draws.contains(&"number(153) == 0"));
}

#[test]
fn rm_skin_play7_convert_warnings_baseline() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rm-skin/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let loaded = load_lua_skin_value(&skin_path, &BTreeMap::new(), &BTreeMap::new())
        .expect("Rm-skin play7 should convert");
    let messages: Vec<_> = loaded.warnings.iter().map(|warning| warning.message.as_str()).collect();
    assert!(
        messages.is_empty(),
        "Rm-skin play7 should convert without unsupported-function warnings: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("destination[51].draw")),
        "score diff draw should be inferred: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("value[14].value")),
        "getDummyNumber values should be inferred: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("value[31].value")),
        "adjusted-rate should be inferred: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("slider[3].value")),
        "adjustedcover slider should be inferred: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("value[50].value")),
        "threshold-num should be inferred: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("text[4].value")),
        "course table text should be inferred: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("`process`")),
        "loader process callback should be silently skipped: {messages:?}"
    );
}

#[test]
fn wmii_fhd_play_lua_features_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/WMII_FHD/play");
    let runtime_state = LuaLoadRuntimeState {
        option_values: BTreeMap::from([
            (32, true),
            (33, false),
            (82, true),
            (84, false),
            (1080, false),
        ]),
        ..LuaLoadRuntimeState::default()
    };

    for name in [
        "play5ac.luaskin",
        "play5wide.luaskin",
        "play7ac.luaskin",
        "play7wide.luaskin",
        "play10ac.luaskin",
        "play10wide.luaskin",
        "play14ac.luaskin",
        "play14wide.luaskin",
    ] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let mut loaded = load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .unwrap();
        let destinations = loaded.document.destination.iter().filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination) => Some(destination),
            bmz_skin_document::DestinationListEntry::Conditional { .. } => None,
        });
        let stages = destinations
            .filter(|destination| matches!(destination.id.as_str(), "extrastage" | "practice"))
            .map(|destination| (destination.id.clone(), destination.draw.clone()))
            .collect::<Vec<_>>();
        assert_eq!(stages.len(), 2);
        for ((id, draw), (expected_id, expected, visible)) in stages
            .iter()
            .zip([("extrastage", "!option(290)", true), ("practice", "number(0) < 0", false)])
        {
            assert_eq!(id, expected_id);
            assert_compiled_or_runtime_draw(
                &mut loaded,
                draw,
                expected,
                visible,
                &TestLuaMainState::default(),
            );
        }
        assert_eq!(loaded.dependencies.option_values.get(&1080), Some(&false));
        let next_rank_draws = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id.starts_with("nextRank") || destination.id == "diff_rank" =>
                {
                    Some((destination.id.as_str(), destination.draw.as_str()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            next_rank_draws.iter().any(|(id, draw)| {
                *id == "nextRank-0"
                    && *draw == "timer(143) == timer_off and wmii_next_rank_stage(0)"
            }),
            "MAX stage predicate for {name}: {next_rank_draws:?}"
        );
        assert!(
            next_rank_draws
                .contains(&("nextRank-3", "timer(143) == timer_off and wmii_next_rank_stage(3)")),
            "A stage predicate for {name}: {next_rank_draws:?}"
        );
        assert!(
            next_rank_draws
                .contains(&("nextRank-2", "timer(143) == timer_off and wmii_next_rank_stage(2)")),
            "AA stage predicate for {name}: {next_rank_draws:?}"
        );
        assert!(
            next_rank_draws.contains(&("nextRankMinus", "nearest_rank_sign(minus)")),
            "negative difference predicate for {name}: {next_rank_draws:?}"
        );
        assert!(
            next_rank_draws.iter().any(|(id, draw)| {
                *id == "nextRankPlus"
                    && draw.contains("nearest_rank(AAA,plus)")
                    && draw.contains("nearest_rank(MAX,plus)")
            }),
            "positive nearest-rank predicate for {name}: {next_rank_draws:?}"
        );
        let next_rank = loaded
            .document
            .value
            .iter()
            .find(|value| value.id == "diff_rank")
            .unwrap_or_else(|| panic!("diff_rank is missing for {name}"));
        assert!(
            matches!(
                next_rank.value_expr.as_str(),
                "bmz:wmii_next_rank_diff"
                    | "bmz:wmii_next_rank_diff_no_max_minus"
                    | "bmz:nearest_rank_diff_abs"
            ),
            "diff_rank for {name}: {}",
            next_rank.value_expr
        );
        assert!(
            !loaded.warnings.iter().any(|warning| {
                warning.message.contains("unsupported draw function")
                    || warning.message.contains("unsupported value function")
            }),
            "unsupported WMII draw/value function for {name}: {:?}",
            loaded.warnings
        );
    }
}

#[test]
fn wmii_fhd_play_stage_draws_follow_scene_modes_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/WMII_FHD/play");
    let modes = [
        ("normal", false, false, false, ["number(0) < 0", "!option(290)", "number(0) < 0"]),
        ("practice", true, false, false, ["number(0) < 0", "number(0) < 0", "!option(290)"]),
        ("autoplay", false, true, false, ["!option(290)", "number(0) < 0", "number(0) < 0"]),
        ("course", false, false, true, ["number(0) < 0", "!option(290)", "number(0) < 0"]),
    ];

    for name in [
        "play5ac.luaskin",
        "play5wide.luaskin",
        "play7ac.luaskin",
        "play7wide.luaskin",
        "play10ac.luaskin",
        "play10wide.luaskin",
        "play14ac.luaskin",
        "play14wide.luaskin",
    ] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        for (mode, practice, autoplay, course, expected) in modes {
            let runtime_state = LuaLoadRuntimeState {
                option_values: BTreeMap::from([
                    (32, !autoplay),
                    (33, autoplay),
                    (82, !autoplay),
                    (84, false),
                    (290, course),
                    (1080, practice),
                ]),
                ..LuaLoadRuntimeState::default()
            };
            let mut loaded = load_lua_skin_with_runtime_state(
                &path,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &runtime_state,
            )
            .unwrap();
            let stages = loaded
                .document
                .destination
                .iter()
                .filter_map(|entry| match entry {
                    bmz_skin_document::DestinationListEntry::Single(destination)
                        if matches!(
                            destination.id.as_str(),
                            "demoplay" | "extrastage" | "practice"
                        ) =>
                    {
                        Some((destination.id.clone(), destination.draw.clone()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(stages.len(), 3);
            let state = TestLuaMainState {
                options: runtime_state.option_values.clone(),
                ..Default::default()
            };
            for (((id, draw), expected_id), expected) in
                stages.iter().zip(["demoplay", "extrastage", "practice"]).zip(expected)
            {
                assert_eq!(id, expected_id);
                let visible = expected == "!option(290)" && !course;
                assert_compiled_or_runtime_draw(&mut loaded, draw, expected, visible, &state);
            }
            assert_eq!(
                loaded.dependencies.option_values.get(&1080),
                Some(&practice),
                "Practice dependency for {name} in {mode}"
            );
        }
    }
}

#[test]
fn wmii_fhd_result_lua_skin_decodes_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let runtime_state = LuaLoadRuntimeState {
        number_values: BTreeMap::new(),
        text_values: BTreeMap::new(),
        option_values: BTreeMap::from([(50, false), (51, true), (162, true)]),
        ..LuaLoadRuntimeState::default()
    };
    let virtual_io_files = BTreeMap::from([
        ("config_sys.json".to_string(), r#"{"playername":"bmz"}"#.to_string()),
        (
            "player/bmz/config_player.json".to_string(),
            serde_json::json!({
                "mode5": {"keyboard": {}, "controller": [], "midi": {}},
                "mode7": {"keyboard": {}, "controller": [], "midi": {}},
                "mode9": {"keyboard": {}, "controller": [], "midi": {}},
                "mode10": {"keyboard": {}, "controller": [], "midi": {}},
                "mode14": {"keyboard": {}, "controller": [], "midi": {}},
                "mode24": {"keyboard": {}, "controller": [], "midi": {}},
                "mode24double": {"keyboard": {}, "controller": [], "midi": {}}
            })
            .to_string(),
        ),
    ]);
    let loaded = load_lua_skin_with_runtime_state_and_virtual_io_files(
        &skin_path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
        &virtual_io_files,
    )
    .expect("WMII FHD result should decode as a skin document");

    assert!(!loaded.document.destination.is_empty());
    assert_eq!(loaded.document.result_panel_default, Some(1));
    assert!(loaded.document.graph.iter().any(|graph| {
        graph.id == "ir_scoreGraph1" && graph.value_expr == "bmz:ir_score_rate:1"
    }));
    assert!(loaded.document.value.iter().any(|value| {
        value.id == "ir_diff_score1" && value.value_expr == "bmz:ir_score_diff:1"
    }));
    assert!(
        loaded.document.text.iter().any(|text| text.id == "ir_username1" && text.ref_id == 120)
    );

    let ir_score_draws = loaded.document.destination.iter().filter_map(|entry| match entry {
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "ir_scoreGraph1" =>
        {
            Some(destination.draw.as_str())
        }
        _ => None,
    });
    assert!(ir_score_draws.into_iter().any(|draw| {
        draw.contains("result_panel(1)") && draw.contains("ir_score_rate_band(1,")
    }));
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "irYouFrame"
                && destination.draw.contains("result_panel(1)")
                && destination.draw.contains("ir_ranking_user(1)")
    )));
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "irWait"
                && destination.timer == Some(172)
                && destination.draw.contains("result_panel(1)")
                && destination.draw.contains(
                    "timer(173) == timer_off and timer(174) == timer_off"
                )
    )));
    let p2_random_draws = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id.starts_with("randomKeySet2P_") =>
            {
                Some(destination.draw.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(p2_random_draws.len(), 7);
    assert!(p2_random_draws.iter().all(|draw| {
        draw.contains("event_index(43) == 2 and option(162)")
            && draw.contains("event_index(43) == 3 and option(163)")
            && !draw.contains("number(0) < 0")
    }));
    assert_eq!(
        loaded.dependencies.virtual_io_files.get("config_sys.json"),
        Some(&Some(r#"{"playername":"bmz"}"#.to_string()))
    );

    let graph_options =
        BTreeMap::from([("Expand Panel".to_string(), "ON - GRAPH DEFAULT".to_string())]);
    let graph_loaded = load_lua_skin_with_runtime_state_and_virtual_io_files(
        &skin_path,
        &graph_options,
        &BTreeMap::new(),
        &runtime_state,
        &virtual_io_files,
    )
    .expect("WMII FHD graph panel should decode as a skin document");
    assert_eq!(graph_loaded.document.result_panel_default, Some(2));
    assert!(graph_loaded.document.destination.iter().any(|entry| matches!(
        entry,
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "graphDataFrame"
                && destination.draw.contains("result_panel(2)")
    )));
    assert!(graph_loaded.document.destination.iter().any(|entry| matches!(
        entry,
        bmz_skin_document::DestinationListEntry::Single(destination)
            if destination.id == "irDataFrame"
                && destination.draw.contains("result_panel(1)")
    )));
    let timing_average_draws =
        graph_loaded.document.destination.iter().filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "timingAvg" =>
            {
                Some(destination.draw.as_str())
            }
            _ => None,
        });
    let timing_average_draws = timing_average_draws.collect::<Vec<_>>();
    assert!(timing_average_draws.iter().any(|draw| {
        *draw == "result_panel(2) and number(374) < 0 or result_panel(2) and number(375) < 0"
    }));
    assert!(
        timing_average_draws.iter().any(|draw| {
            draw.contains("result_panel(2)")
                && draw.contains("number(374) >= 0 and number(375) >= 0")
        }),
        "WMII timing average layers must remain mutually exclusive: {timing_average_draws:?}"
    );
    assert!(!timing_average_draws.contains(&"number(0) >= 0"));
}

#[test]
fn rm_skin_play7_decodes_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rm-skin/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
        .expect("Rm-skin play7 should decode");
    assert!(!loaded.document.destination.is_empty());
    assert_eq!(loaded.document.skin_type, 0);
    let eon_shadow_draw = "timer(143) == timer_off and number(106)-number(110)-number(111)-number(112)-number(113)-number(114) == 0";
    let eon_destinations: Vec<_> = loaded
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
        .collect();
    assert!(
        eon_destinations.iter().any(|(timer, _)| *timer == Some(143)),
        "Rm-skin end-of-note animation should use timer 143: {eon_destinations:?}"
    );
    assert!(
        eon_destinations.iter().all(|(timer, draw)| timer.is_some() || *draw == eon_shadow_draw),
        "Rm-skin end-of-note shadow layers should keep their runtime draw gate: {eon_destinations:?}"
    );
}
