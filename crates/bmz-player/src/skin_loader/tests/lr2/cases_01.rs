use super::*;

#[test]
fn wmii_result_decodes_with_virtual_io_and_graph_default() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([("Expand Panel".to_string(), "ON - GRAPH DEFAULT".to_string())]);
    let runtime_state = LuaLoadRuntimeState {
        number_values: BTreeMap::new(),
        text_values: BTreeMap::new(),
        option_values: BTreeMap::from([(51, true), (160, true)]),
        ..LuaLoadRuntimeState::default()
    };
    let loaded = load_skin_document_uncached(
        &skin_path,
        SkinKind::Result,
        &options,
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("unmodified WMII result should decode through the BMZ loader");

    assert_eq!(loaded.document.result_panel_default, Some(2));
    assert!(loaded.document.slider.iter().any(|slider| slider.slider_type == 8));
    assert_eq!(
        loaded
            .document
            .image
            .iter()
            .find(|image| image.id == "BtnGraphData")
            .and_then(|image| image.act),
        Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_GRAPH)
    );
    assert_eq!(
        loaded
            .document
            .image
            .iter()
            .find(|image| image.id == "BtnIrData")
            .and_then(|image| image.act),
        Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_IR)
    );
    let favorite = loaded
        .document
        .image
        .iter()
        .find(|image| image.id == "favorite")
        .expect("WMII result favorite button should decode");
    assert_eq!(favorite.ref_id, 90);
    assert_eq!(favorite.act, Some(90));
    assert_eq!(favorite.divy, 3);
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination)
            if destination.draw.contains("result_panel(1)")
    )));
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination)
            if destination.draw.contains("result_panel(2)")
    )));
    let destinations = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            DestinationListEntry::Single(destination) => Some(destination),
            DestinationListEntry::Conditional { .. } => None,
        })
        .collect::<Vec<_>>();
    assert!(destinations.iter().any(|destination| destination.id == "randomButton1p"));
    let random_key = destinations
        .iter()
        .find(|destination| destination.id == "randomKeySet1P_1")
        .expect("7K Result should retain the RANDOM lane placement destinations");
    assert!(random_key.draw.contains("event_index(42)"));
    let rank_aaa = destinations
        .iter()
        .find(|destination| destination.id == "rankBig_AAA" && destination.loop_time == Some(100))
        .expect("rankBig_AAA should survive malformed op repair");
    assert_eq!(rank_aaa.op, [300, 920]);
    assert_eq!(rank_aaa.loop_time, Some(100));
    assert_eq!(rank_aaa.filter, 1);
    assert_eq!(rank_aaa.dst.len(), 2);
    for (id, rank) in [("AAA_BG", 300), ("AA_BG", 301), ("A_BG", 302)] {
        let backgrounds = destinations
            .iter()
            .filter(|destination| {
                destination.id == id && matches!(destination.loop_time, Some(500 | 600 | 700))
            })
            .collect::<Vec<_>>();
        assert_eq!(backgrounds.len(), 3, "expected three {id} animations");
        assert!(backgrounds.iter().all(|destination| destination.op == [90, rank]));
    }
    let clear_backgrounds = destinations
        .iter()
        .filter(|destination| {
            destination.id == "clearBG" && matches!(destination.loop_time, Some(500 | 600 | 700))
        })
        .collect::<Vec<_>>();
    assert_eq!(clear_backgrounds.len(), 3);
    assert!(clear_backgrounds.iter().all(|destination| destination.op == [90]));
    let expanded_timing_values = destinations
        .iter()
        .filter(|destination| {
            matches!(
                destination.id.as_str(),
                "timingAvg"
                    | "timingAvgAdot"
                    | "timingDotMS"
                    | "durationAvg"
                    | "durationAvgAdot"
                    | "stddav"
                    | "stddaAdot"
            ) && destination.dst.first().is_some_and(|entry| {
                matches!(
                    entry,
                    bmz_render::skin::SkinDstEntry::Frame(frame)
                        if frame.x.is_some_and(|x| x >= 1_000)
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(expanded_timing_values.len(), 12);
    assert!(
        expanded_timing_values.iter().all(|destination| {
            destination.draw.contains("result_panel(2)")
                && !destination.draw.contains("result_panel(0)")
                && !destination.draw.contains("result_panel(1)")
        }),
        "expanded timing values must stay hidden on the IR panel: {:?}",
        expanded_timing_values
            .iter()
            .map(|destination| (destination.id.as_str(), destination.draw.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        loaded.dependencies.virtual_io_files.get("config_sys.json"),
        Some(&Some("{\"playername\":\"bmz\"}".to_string()))
    );
    assert!(loaded.dependencies.virtual_io_files.contains_key("player/bmz/config_player.json"));
}

#[test]
fn wmii_course_result_uses_native_stage_titles_and_result_data() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/result/courseResult.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let runtime_state = LuaLoadRuntimeState {
        text_values: BTreeMap::from([
            (150, "Stage One".to_string()),
            (151, "Stage Two".to_string()),
            (152, "Stage Three".to_string()),
            (153, "Stage Four".to_string()),
        ]),
        option_values: BTreeMap::from([(160, true), (290, true)]),
        virtual_io_files: BTreeMap::from([(
            "skin/WMII_FHD/result/courseData.json".to_string(),
            serde_json::json!({
                "songs": [
                    { "stage": 1, "score": 1000, "gauge": 80, "miss": 10, "rate": 0.5 },
                    { "stage": 2, "score": 2000, "gauge": 81, "miss": 11, "rate": 0.6 },
                    { "stage": 3, "score": 3000, "gauge": 82, "miss": 12, "rate": 0.7 },
                    { "stage": 4, "score": 3456, "gauge": 88, "miss": 13, "rate": 0.75 }
                ]
            })
            .to_string(),
        )]),
        ..LuaLoadRuntimeState::default()
    };
    let loaded = load_skin_document_uncached(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("unmodified WMII course result should decode with native stage data");

    let graph_expr = loaded
        .document
        .graph
        .iter()
        .find(|graph| graph.id == "stage_scoreGraph4")
        .map(|graph| graph.value_expr.clone())
        .expect("missing stage 4 score-rate graph");
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination) if destination.id == "courseTitle4"
    )));
    assert_eq!(
        loaded
            .document
            .value
            .iter()
            .find(|value| value.id == "courseClearRate")
            .map(|value| value.value_expr.as_str()),
        Some(bmz_render::skin::SKIN_EXPR_COURSE_CLEAR_RATE)
    );
    assert_eq!(
        loaded.dependencies.virtual_io_files.get("skin/WMII_FHD/result/courseData.json"),
        runtime_state
            .virtual_io_files
            .get("skin/WMII_FHD/result/courseData.json")
            .cloned()
            .map(Some)
            .as_ref()
    );

    let mut lua_runtime = loaded.lua_runtime.expect("WMII stage values should use Lua callbacks");
    let state = SkinDrawState::default();
    let text_values = BTreeMap::new();
    let provider =
        RenderLuaMainState { state: &state, enabled_options: &[], text_values: &text_values };
    for (id, expected) in [("stage_gauge4", 88.0), ("stage_score4", 3456.0), ("stage_miss4", 13.0)]
    {
        let value = loaded
            .document
            .value
            .iter()
            .find(|value| value.id == id)
            .unwrap_or_else(|| panic!("missing {id}"));
        let callback_id = value
            .value_expr
            .strip_prefix("bmz:lua_value_callback:")
            .unwrap_or_else(|| panic!("{id} should retain a Lua callback: {}", value.value_expr))
            .parse::<usize>()
            .expect("valid Lua callback ID");
        lua_runtime.begin_frame();
        assert_eq!(lua_runtime.evaluate_number(callback_id, &provider), Some(expected), "{id}");
    }
    let graph_callback_id = graph_expr
        .strip_prefix("bmz:lua_value_callback:")
        .expect("stage score-rate graph should retain a Lua callback")
        .parse::<usize>()
        .expect("valid Lua callback ID");
    lua_runtime.begin_frame();
    assert_eq!(lua_runtime.evaluate_number(graph_callback_id, &provider), Some(0.75));
}

#[test]
fn wmii_result_renders_bmz_player_version_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin_with_options(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::from([("Display Version".to_string(), "ON".to_string())]),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        decoded.document.text.iter().any(|text| text.id == "version" && text.ref_id == 1010),
        "WMII version text should retain STRING_VERSION ref 1010"
    );
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let items = decoded.document.static_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 2_000, ..SkinDrawState::default() },
        &SkinTextState::default(),
    );

    assert!(items.iter().any(|item| matches!(
        item,
        SkinRenderItem::Text { text, .. }
            if text == &format!("bmz-player {}", env!("CARGO_PKG_VERSION"))
    )));
}

#[test]
fn wmii_result_uses_runtime_combo_break_for_clear_animation() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([("Expand Panel".to_string(), "ON - GRAPH DEFAULT".to_string())]);
    let load = |combo_break: i32| {
        load_skin_document_uncached(
            &skin_path,
            SkinKind::Result,
            &options,
            &BTreeMap::new(),
            &LuaLoadRuntimeState {
                number_values: BTreeMap::from([(425, combo_break)]),
                text_values: BTreeMap::new(),
                option_values: BTreeMap::from([(51, true), (160, true)]),
                ..LuaLoadRuntimeState::default()
            },
        )
        .expect("unmodified WMII result should decode")
    };
    let destination_ids = |loaded: &LoadedSkinDocumentWithDependencies| {
        loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                DestinationListEntry::Single(destination) => Some(destination.id.clone()),
                DestinationListEntry::Conditional { .. } => None,
            })
            .collect::<Vec<_>>()
    };

    let full_combo = load(0);
    let full_combo_ids = destination_ids(&full_combo);
    assert!(full_combo_ids.iter().any(|id| id == "result_FULL"));
    assert!(full_combo_ids.iter().any(|id| id == "result_COMBO"));
    assert!(!full_combo_ids.iter().any(|id| id == "result_CLEAR"));

    let normal_clear = load(1);
    let normal_clear_ids = destination_ids(&normal_clear);
    assert!(normal_clear_ids.iter().any(|id| id == "result_CLEAR"));
    assert!(!normal_clear_ids.iter().any(|id| id == "result_FULL"));
    assert!(!normal_clear_ids.iter().any(|id| id == "result_COMBO"));
}
