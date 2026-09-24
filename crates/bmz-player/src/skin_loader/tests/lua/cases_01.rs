use super::*;

#[test]
fn render_lua_shared_scope_uses_changed_rows_options_and_text_then_restores_outer_state() {
    let root = unique_test_dir("bmz-shared-render-state");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    std::fs::write(
        &path,
        r#"
        local main_state = require('main_state')
        local n = 0
        return { type = 7, value = {{ id = 'v', value = function()
            n = n + 1
            return main_state.number(71) + n
        end }}, text = {{ id = 't', value = function()
            return main_state.text(10) .. ':' .. tostring(main_state.option(920))
        end }} }
    "#,
    )
    .unwrap();
    let loaded = bmz_skin::load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &bmz_skin::LuaLoadRuntimeState {
            runtime_mode: bmz_skin::LuaSkinRuntimeMode::Compat,
            ..Default::default()
        },
    )
    .unwrap();
    let number = loaded.document.value[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
    let text = loaded.document.text[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
    let adapter = LuaSkinDrawRuntimeAdapter::new(loaded.lua_runtime.unwrap());
    let state = SkinDrawState { ex_score: 10, ..Default::default() };
    let options = [920];
    let texts = BTreeMap::from([(10, "outer".into())]);
    adapter.with_state(&state, &options, &texts, &mut || {
        assert_eq!(adapter.evaluate_number(number, &state, &options, &texts), Some(11.0));
        let mut row = state.clone();
        row.ex_score = 20;
        assert_eq!(adapter.evaluate_number(number, &row, &options, &texts), Some(22.0));
        row.ex_score = 30;
        adapter.with_state(&row, &options, &texts, &mut || {
            assert_eq!(adapter.evaluate_number(number, &row, &options, &texts), Some(33.0));
        });
        let other_texts = BTreeMap::from([(10, "inner".into())]);
        assert_eq!(
            adapter.evaluate_text(text, &state, &[], &other_texts).as_deref(),
            Some("inner:false")
        );
        assert_eq!(
            adapter.evaluate_text(text, &state, &options, &texts).as_deref(),
            Some("outer:true")
        );
        assert_eq!(adapter.evaluate_number(number, &state, &options, &texts), Some(14.0));
    });
    assert_eq!(adapter.evaluate_number(number, &state, &options, &texts), Some(15.0));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        adapter.with_state(&state, &options, &texts, &mut || panic!("scope unwind"));
    }));
    assert!(panic.is_err());
    assert_eq!(adapter.evaluate_number(number, &state, &options, &texts), Some(16.0));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn render_lua_main_state_reads_session_totals_and_selected_score_date() {
    let state = SkinDrawState {
        player_stats: bmz_render::scene::PlayerStatsSnapshot {
            session_play_count: 2,
            session_play_notes: 1234,
            play_count: 999,
            ..Default::default()
        },
        score_date_sec: 2_208_988_800,
        ..Default::default()
    };
    let text_values = BTreeMap::new();
    let provider =
        RenderLuaMainState { state: &state, enabled_options: &[], text_values: &text_values };
    assert_eq!(provider.total_play_counts_in_session(), 2);
    assert_eq!(provider.total_play_notes_in_session(), 1234);
    assert_eq!(provider.score_date_sec_time(), 2_208_988_800);
}

#[test]
fn render_lua_main_state_reads_current_frame_skin_offset() {
    let mut state = SkinDrawState::default();
    state
        .skin_offsets
        .set(45, bmz_render::skin_offset::SkinOffsetValue { x: 1, y: 2, w: 3, h: 4, r: 5, a: -6 });
    let text_values = BTreeMap::new();
    let provider =
        RenderLuaMainState { state: &state, enabled_options: &[], text_values: &text_values };

    assert_eq!(
        provider.offset(45),
        bmz_skin::LuaSkinOffsetValue { x: 1, y: 2, w: 3, h: 4, r: 5, a: -6 }
    );
    assert_eq!(provider.offset(46), bmz_skin::LuaSkinOffsetValue::default());
}

#[test]
fn lua_compat_virtual_io_contains_only_sanitized_beatoraja_config() {
    let files = lua_compat_virtual_io_files();
    assert_eq!(files.len(), 2);

    let system: serde_json::Value =
        serde_json::from_str(&files["config_sys.json"]).expect("system config should be JSON");
    assert_eq!(system, serde_json::json!({ "playername": "bmz" }));

    let player: serde_json::Value = serde_json::from_str(&files["player/bmz/config_player.json"])
        .expect("player config should be JSON");
    let player = player.as_object().expect("player config should be an object");
    assert_eq!(
        player.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from(["mode5", "mode7", "mode9", "mode10", "mode14", "mode24", "mode24double"])
    );
    for mode in player.values() {
        assert_eq!(mode["keyboard"], serde_json::json!({}));
        assert_eq!(mode["controller"], serde_json::json!([]));
        assert_eq!(mode["midi"], serde_json::json!({}));
    }
}

#[test]
fn modern_chic_result_bakes_runtime_song_label_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ModernChic/result.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let runtime_state = LuaLoadRuntimeState {
        text_values: BTreeMap::from([
            (10, "Song".to_string()),
            (11, "Subtitle".to_string()),
            (12, "Song Subtitle".to_string()),
            (13, "Genre".to_string()),
            (14, "Artist".to_string()),
            (1003, "Table ★12".to_string()),
        ]),
        ..LuaLoadRuntimeState::default()
    };
    let loaded = load_skin_document_uncached(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("unmodified ModernChic result should decode with runtime song text");
    let bottom = loaded
        .document
        .text
        .iter()
        .find(|text| text.id == "bottomResult")
        .expect("ModernChic bottomResult text");
    assert_eq!(bottom.constant_text, "Song Subtitle / Artist / Genre / Table ★12");
}

#[test]
fn luxe_flat_result_decodes_local_panel_state_and_tab_actions() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Luxez-Flat/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let runtime_state = LuaLoadRuntimeState {
        option_values: BTreeMap::from([(50, false), (51, true)]),
        ..LuaLoadRuntimeState::default()
    };
    let loaded = load_skin_document_uncached(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("unmodified Luxe Flat result should decode through the BMZ loader");

    assert_eq!(loaded.document.result_panel_default, Some(2));
    assert!(loaded.document.slider.iter().any(|slider| slider.slider_type == 8));
    assert_eq!(
        loaded
            .document
            .image
            .iter()
            .find(|image| image.id == "result_modeselect_graph_data_off")
            .and_then(|image| image.act),
        Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_GRAPH)
    );
    assert_eq!(
        loaded
            .document
            .image
            .iter()
            .find(|image| image.id == "result_modeselect_ir_ranking_off")
            .and_then(|image| image.act),
        Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_IR)
    );
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
    assert_eq!(
        loaded
            .document
            .value
            .iter()
            .find(|value| value.id == "rank_diff_count")
            .map(|value| value.value_expr.as_str()),
        Some("bmz:nearest_rank_diff_abs")
    );
    assert_eq!(
        loaded
            .document
            .value
            .iter()
            .find(|value| value.id == "ir_scorerate1")
            .map(|value| value.value_expr.as_str()),
        Some("bmz:ir_score_rate_integer:1")
    );
    assert_eq!(
        loaded
            .document
            .value
            .iter()
            .find(|value| value.id == "ir_scorerate_dot1")
            .map(|value| value.value_expr.as_str()),
        Some("bmz:ir_score_rate_fraction:1")
    );
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination)
            if destination.id == "rank_diff_aaa_plus"
                && destination.draw.contains("nearest_rank(AAA,plus)")
    )));
}

#[test]
fn luxe_flat_result_displays_extended_arrange_labels_and_lane_pattern() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Luxez-Flat/result/result.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let runtime_state = LuaLoadRuntimeState {
        event_index_values: BTreeMap::from([(42, 2), (43, 2), (344, 10), (345, 11)]),
        option_values: BTreeMap::from([(163, true)]),
        ..LuaLoadRuntimeState::default()
    };
    let loaded = load_skin_document_uncached(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("Luxe Flat result should decode extended arrange labels");

    assert_eq!(
        loaded
            .document
            .text
            .iter()
            .find(|text| text.id == "lane_option")
            .map(|text| text.constant_text.as_str()),
        Some("F-RANDOM / MF-RANDOM")
    );
    assert!(loaded.document.destination.iter().any(|entry| matches!(
        entry,
        DestinationListEntry::Single(destination)
            if destination.id == "1key"
                && destination.draw.contains("event_index(450) == 1")
    )));

    let course_skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Luxez-Flat/result/courseresult.luaskin");
    let course_loaded = load_skin_document_uncached(
        &course_skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("Luxe Flat course result should decode extended arrange labels");
    assert_eq!(
        course_loaded
            .document
            .text
            .iter()
            .find(|text| text.id == "lane_option")
            .map(|text| text.constant_text.as_str()),
        Some("F-RANDOM / MF-RANDOM")
    );
}

#[test]
fn ecfn_play7_1p_json_skin_can_be_applied_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7-1p.json");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();

    apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
}

#[test]
fn ecfn_result_json_skin_can_be_applied_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/ECFN/RESULT/result-converted.json");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();

    apply_beatoraja_result_json_skin(&mut renderer, &skin_path).unwrap();
}

#[test]
fn ecfn_select_json_skin_can_be_applied_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/ECFN/select/select-converted.json");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();

    apply_beatoraja_select_json_skin(&mut renderer, &skin_path).unwrap();
}

#[test]
#[ignore = "manual select skin profiling helper"]
fn profile_ecfn_select_plan_generation() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/select/select.luaskin");
    if !skin_path.is_file() {
        eprintln!("skip: {} is missing", skin_path.display());
        return;
    }

    let decoded = decode_beatoraja_skin_with_options(
        &skin_path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let document_textures = decoded
        .sources
        .iter()
        .map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: SkinImageSize { width: source.size.width, height: source.size.height },
        })
        .collect::<Vec<_>>();
    let uncached_document = decoded.document.clone();
    let document_sources = document_textures
        .iter()
        .cloned()
        .map(|source| (source.source_id.clone(), source))
        .collect::<HashMap<_, _>>();
    let skin = SkinContext::from_manifest_and_document(
        bmz_render::skin::default_skin_manifest(),
        decoded.document,
        document_textures,
    );
    let rows = (0..25)
        .map(|index| SelectRowSnapshot {
            index,
            title: format!("War in the Mirrorworld[{index:02}]"),
            artist: "Aoi".to_string(),
            difficulty_name: "ANOTHER".to_string(),
            play_level: "12".to_string(),
            total_notes: 2253,
            chart_normal_notes: 2167,
            chart_scratch_notes: 86,
            chart_density: 19.0,
            chart_peak_density: 38.0,
            chart_end_density: 25.0,
            min_bpm: 171.0,
            max_bpm: 171.0,
            chart_main_bpm: 171.0,
            initial_bpm: 171.0,
            length_ms: 115_000,
            ..SelectRowSnapshot::default()
        })
        .collect();
    let mut runtime = DynamicTimerRuntime::default();
    let mut snapshot = SelectSnapshot {
        time: TimeUs(0),
        selection_time: TimeUs(0),
        chart_count: 1_000,
        selected_index: 12,
        rows,
        stage_background: true,
        banner_image: true,
        ..SelectSnapshot::default()
    };

    for frame in 0..30 {
        snapshot.time = TimeUs(frame * 16_666);
        black_box(DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Select(snapshot.clone()),
            &skin,
            &mut runtime,
        ));
    }

    let frames = 300;
    let mut scene = AppSceneSnapshot::Select(snapshot);
    let start = Instant::now();
    let mut commands = 0_usize;
    for frame in 0..frames {
        let AppSceneSnapshot::Select(snapshot) = &mut scene else { unreachable!() };
        snapshot.time = TimeUs((frame + 30) * 16_666);
        let plan = DrawPlan::from_scene_with_skin(&scene, &skin, &mut runtime);
        commands += plan.commands.len();
        black_box(plan);
    }
    let elapsed = start.elapsed();

    let AppSceneSnapshot::Select(snapshot) = &scene else { unreachable!() };
    let settings_dest_index = bmz_render::select_settings_dest::SelectSettingsDestIndex::default();
    let mut cached_runtime = DynamicTimerRuntime::default();
    let cached_start = Instant::now();
    for _ in 0..frames {
        black_box(
            skin.select_document_items_with_dynamic_timers(snapshot, Some(&mut cached_runtime)),
        );
    }
    let cached_elapsed = cached_start.elapsed();
    let mut uncached_runtime = DynamicTimerRuntime::default();
    let uncached_start = Instant::now();
    for _ in 0..frames {
        black_box(uncached_document.select_render_items_with_dynamic_timers(
            &document_sources,
            snapshot,
            Some(&mut uncached_runtime),
            &settings_dest_index,
            None,
        ));
    }
    let uncached_elapsed = uncached_start.elapsed();

    println!(
        "profile_ecfn_select_plan_generation frames={frames} avg_plan_ms={:.3} \
         avg_cached_items_ms={:.3} avg_uncached_items_ms={:.3} avg_commands={}",
        elapsed.as_secs_f64() * 1000.0 / frames as f64,
        cached_elapsed.as_secs_f64() * 1000.0 / frames as f64,
        uncached_elapsed.as_secs_f64() * 1000.0 / frames as f64,
        commands / frames as usize
    );
}

#[test]
fn m_select_lua_select_skin_renders_items_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/music_select.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let decoded = decode_beatoraja_skin_with_options(
        &skin_path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(decoded.document.text.iter().any(|text| {
        text.id == "defaultNotesProcessingCounter_notes"
            || text.id == "defaultNotesProcessingCounter_stroke"
    }));
    let random_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-stateplayoption-random-bmz")
        .expect("m-select F-RANDOM source")
        .texture;
    let option_source = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-stateplayoption-parts")
        .expect("m-select play option source");
    let option_texture = option_source.texture;
    let option_source_height = option_source.size.height as f32;
    let option_panel_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-optionpanel-panel1")
        .expect("m-select extended option panel source")
        .texture;
    let option_panel_cursor_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-optionpanel-cursor")
        .expect("m-select option panel cursor source")
        .texture;
    let option_panel_random_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-optionpanel-random-cursor-bmz")
        .expect("m-select extended option panel cursor source")
        .texture;
    let option_panel_image = image::open(
        skin_path
            .parent()
            .expect("m-select skin directory")
            .join("customize/advanced/default_optionpanel4/panel1_bmz.png"),
    )
    .expect("m-select extended option panel image")
    .into_rgba8();
    let target_panel_bounds = option_panel_image
        .enumerate_pixels()
        .filter_map(|(x, y, pixel)| (x < 301 && pixel.0[3] > 0).then_some((x, y)))
        .fold(None::<(u32, u32, u32, u32)>, |bounds, (x, y)| {
            Some(match bounds {
                None => (x, y, x, y),
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }
            })
        });
    assert_eq!(target_panel_bounds, Some((16, 97, 298, 1005)));
    let target_cursor_bounds = option_panel_image
        .enumerate_pixels()
        .filter_map(|(x, y, pixel)| {
            let [r, g, b, a] = pixel.0;
            (x < 301
                && (480..640).contains(&y)
                && a > 0
                && g > 38
                && b > 38
                && u16::from(b) * 5 > u16::from(r) * 6)
                .then_some((x, y))
        })
        .fold(None::<(u32, u32, u32, u32)>, |bounds, (x, y)| {
            Some(match bounds {
                None => (x, y, x, y),
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }
            })
        });
    assert_eq!(target_cursor_bounds, Some((16, 548, 298, 600)));
    let document_textures =
        decoded.sources.iter().map(|source| bmz_render::skin::SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: bmz_render::skin::SkinImageSize {
                width: source.size.width,
                height: source.size.height,
            },
        });
    let context = bmz_render::skin::SkinContext::from_manifest_and_document(
        bmz_render::skin::default_skin_manifest(),
        decoded.document,
        document_textures,
    );
    assert!(context.document().is_some_and(|document| document.skin_type == 5));
    let snapshot = bmz_render::scene::SelectSnapshot {
        time: bmz_core::time::TimeUs(300_000),
        option_panel_time: bmz_core::time::TimeUs(300_000),
        option_panel: 1,
        arrange: "F-RANDOM".to_string(),
        arrange_2p: "MF-RANDOM".to_string(),
        gauge: "EX-HARD".to_string(),
        double_option: "BATTLE".to_string(),
        hs_fix: "MIN BPM".to_string(),
        rows: vec![bmz_render::scene::SelectRowSnapshot {
            title: "Song".to_string(),
            ..Default::default()
        }],
        chart_count: 1,
        ..Default::default()
    };
    let items = context.select_document_items_with_dynamic_timers(&snapshot, None);
    assert!(!items.is_empty(), "m_select select skin should produce render items");
    assert!(
            items
                .iter()
                .any(|item| matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == "Song")),
            "m_select select skin should render the song title text"
        );
    for label in ["EX-HARD", "BATTLE", "MIN BPM"] {
        assert!(
            items.iter().all(
                |item| !matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == label)
            ),
            "m-select should not render the option label {label} as text"
        );
    }
    for (left_x, source_y) in [(628.0, 76.0), (794.0, 38.0), (960.0, 76.0)] {
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. }
                    if *texture == option_texture
                        && (rect.x - left_x / 1920.0).abs() < 0.001
                        && (uv.y - source_y / option_source_height).abs() < 0.001
            )),
            "m-select should render the option sprite at x={left_x}"
        );
    }
    for (left_x, uv_y) in [(462.0, 0.0), (1126.0, 0.5)] {
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. }
                    if *texture == random_texture
                        && (rect.x - left_x / 1920.0).abs() < 0.001
                        && (uv.y - uv_y).abs() < 0.001
            )),
            "m-select should render the extended arrange image at x={left_x}"
        );
    }
    assert!(items.iter().any(|item| matches!(
        item,
        bmz_render::skin::SkinRenderItem::Image { texture, rect, .. }
            if *texture == option_panel_texture
                && rect.x.abs() < 0.001
                && (rect.y - (-22.0 / 1080.0)).abs() < 0.001
                && (rect.width - 1315.0 / 1920.0).abs() < 0.001
                && (rect.height - 1124.0 / 1080.0).abs() < 0.001
    )));
    for (left_x, uv_y) in [(318.0, 50.0 / 1150.0), (1118.0, 0.0)] {
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. }
                    if *texture == option_panel_random_texture
                        && (rect.x - left_x / 1920.0).abs() < 0.001
                        && (rect.y - 440.0 / 1080.0).abs() < 0.001
                        && (rect.height - 600.0 / 1080.0).abs() < 0.001
                        && (uv.y - uv_y).abs() < 0.001
            )),
            "m-select should render the extended option panel arrange cursor at x={left_x}"
        );
    }
    for left_x in [518.0, 718.0, 918.0] {
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, .. }
                    if *texture == option_panel_cursor_texture
                        && (rect.x - left_x / 1920.0).abs() < 0.001
                        && (rect.y - 440.0 / 1080.0).abs() < 0.001
            )),
            "m-select should align the option panel cursor at x={left_x}"
        );
    }
    for x in [503.0, 586.0] {
        let hit = context
            .select_click_hit(&snapshot, x / 1920.0, 0.98)
            .expect("m_select arrange cell should remain clickable across its full width");
        assert_eq!(hit.target, bmz_render::skin::SkinClickTarget::Event { event_id: 42, click: 2 });
        assert!((hit.rect.x - 462.0 / 1920.0).abs() < f32::EPSILON);
        assert!((hit.rect.width - 166.0 / 1920.0).abs() < f32::EPSILON);
    }
}

#[test]
fn antique_play_skin_shows_random_lane_pattern_before_ready_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let load_context = |options: &BTreeMap<String, String>| {
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            options,
            &BTreeMap::new(),
        )
        .unwrap();
        let number_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "src_number_lane")
            .expect("antique number lane source")
            .texture;
        let document_textures = decoded.sources.iter().map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: SkinImageSize { width: source.size.width, height: source.size.height },
        });
        (
            SkinContext::from_manifest_and_document(
                SkinManifest::default(),
                decoded.document,
                document_textures,
            ),
            number_texture,
        )
    };
    let (default_context, default_number_texture) = load_context(&BTreeMap::new());
    let default_document = default_context.document().expect("antique play document");
    assert!(default_document.enabled_options().contains(&916));
    assert!(!default_document.enabled_options().contains(&917));
    assert!(
        default_document
            .property
            .iter()
            .any(|property| { property.name == "RANDOM配置表示" && property.def == "OFF" })
    );
    let options = BTreeMap::from([("RANDOM配置表示".to_string(), "ON".to_string())]);
    let (context, number_texture) = load_context(&options);
    let document = context.document().expect("antique play document");
    assert!(document.enabled_options().contains(&917));
    assert!(!document.enabled_options().contains(&916));
    assert!(
        document
            .all_destinations(&document.enabled_options())
            .iter()
            .filter(|destination| destination.id.starts_with("num_random_"))
            .all(|destination| !destination.draw.starts_with("bmz:lua_draw_callback:")),
        "RANDOM digit color predicates should compile without per-frame Lua callbacks"
    );
    let displayed_values = [2_u8, 3, 4, 5, 6, 7, 1];
    let mut pattern = (0..bmz_core::lane::LANE_COUNT as u8).collect::<Vec<_>>();
    for (destination, source) in (1..=7).zip(displayed_values) {
        pattern[destination] = source;
    }
    let applied_arrange = crate::screens::play_session::AppliedArrange {
        arrange: crate::select_options::ArrangeOption::Random,
        pattern: Some(pattern.clone()),
        ..crate::screens::play_session::AppliedArrange::default()
    };
    let mut pre_ready = bmz_render::snapshot::RenderSnapshot {
        key_mode: KeyMode::K7,
        ready_elapsed_time: None,
        ..Default::default()
    };
    crate::screens::play_loop::apply_play_arrange_to_snapshot(&mut pre_ready, &applied_arrange);
    assert_eq!(pre_ready.lane_shuffle_pattern, pattern);

    let render = |context: &SkinContext, snapshot| {
        bmz_render::plan::DrawPlan::from_scene_with_skin(
            &bmz_render::scene::AppSceneSnapshot::Play(snapshot),
            context,
            &mut bmz_render::skin::DynamicTimerRuntime::default(),
        )
    };
    let random_digits = |plan: &bmz_render::plan::DrawPlan, number_texture: SkinTextureId| {
        let mut digits = plan
            .commands
            .iter()
            .filter_map(|command| match command {
                bmz_render::plan::DrawCommand::Image { texture, rect, tint, .. }
                    if texture.0 == number_texture.0 && (0.69..0.72).contains(&rect.y) =>
                {
                    Some((rect.x, *tint))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        digits.sort_by(|left, right| left.0.total_cmp(&right.0));
        digits
    };

    let digits = random_digits(&render(&context, pre_ready.clone()), number_texture);
    assert_eq!(digits.len(), 7, "expected seven pre-READY RANDOM digits");
    for (index, (_, tint)) in digits.into_iter().enumerate() {
        let value = displayed_values[index];
        let expected = if matches!(value, 1 | 3 | 5 | 7) {
            (1.0, 1.0, 1.0)
        } else {
            (64.0 / 255.0, 160.0 / 255.0, 1.0)
        };
        assert!((tint.r - expected.0).abs() < 0.01);
        assert!((tint.g - expected.1).abs() < 0.01);
        assert!((tint.b - expected.2).abs() < 0.01);
    }

    let mut ready = pre_ready.clone();
    ready.ready_elapsed_time = Some(TimeUs(0));
    let ready_digits = random_digits(&render(&context, ready.clone()), number_texture);
    assert_eq!(ready_digits.len(), 7, "READY should start at full opacity");
    assert!(ready_digits.iter().all(|(_, tint)| (tint.a - 1.0).abs() < 0.01));

    ready.ready_elapsed_time = Some(TimeUs(250_000));
    let fading_digits = random_digits(&render(&context, ready.clone()), number_texture);
    assert_eq!(fading_digits.len(), 7, "RANDOM digits should fade for 500 ms");
    assert!(fading_digits.iter().all(|(_, tint)| (tint.a - 0.5).abs() < 0.02));

    ready.ready_elapsed_time = Some(TimeUs(501_000));
    assert!(
        random_digits(&render(&context, ready), number_texture).is_empty(),
        "RANDOM digits should disappear after the BACKBMP 500 ms fade"
    );

    let mut no_pattern = pre_ready.clone();
    crate::screens::play_loop::apply_play_arrange_to_snapshot(
        &mut no_pattern,
        &crate::screens::play_session::AppliedArrange {
            arrange: crate::select_options::ArrangeOption::Random,
            ..crate::screens::play_session::AppliedArrange::default()
        },
    );
    assert!(random_digits(&render(&context, no_pattern), number_texture).is_empty());

    assert!(
        random_digits(&render(&default_context, pre_ready), default_number_texture).is_empty(),
        "RANDOM display should default to OFF"
    );
}

#[test]
fn luxe_flat_lua_select_skin_keeps_operating_time_refs_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Luxez-Flat/music_select.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Select).unwrap();
    for (id, value_expr) in [
        ("default_playerdata_diff_count", "bmz:nearest_rank_diff_abs"),
        ("tn_count", SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_INTEGER),
        ("tn_dot_count", SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_FRACTION),
    ] {
        assert!(
            decoded
                .document
                .value
                .iter()
                .any(|value| { value.id == id && value.value_expr == value_expr }),
            "Luxe Flat should compile {id} to {value_expr}"
        );
    }
    for id in ["songtime_m_count", "songtime_s_count", "tn_count", "tn_dot_count"] {
        assert!(
            decoded.document.destination.iter().any(|entry| matches!(
                entry,
                DestinationListEntry::Single(destination)
                    if destination.id == id
                        && destination.draw.contains("option(2)")
                        && destination.draw.contains("number(74) > 0")
                        && destination.draw.contains("number(92) > 0")
                        && destination.draw.contains("number(368) > 0")
            )),
            "Luxe Flat should retain the runtime chart-data guard for {id}"
        );
    }
    assert!(
        decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == "diff_rank_max"
                    && destination.draw
                        == "select_score_available() and nearest_rank(MAX,minus)"
        )),
        "Luxe Flat should compile the local MAX- score-grade destination"
    );
    for ref_id in 27..=29 {
        assert!(
            decoded.document.value.iter().any(|value| value.ref_id == ref_id),
            "Luxe Flat should retain operating-time ref {ref_id}"
        );
    }
    for id in ["bmz_select_gauge", "bmz_select_double_option", "bmz_select_hs_fix"] {
        assert!(
            decoded.document.text.iter().all(|text| text.id != id),
            "Luxe Flat should use option images instead of dynamic {id} text"
        );
    }
    for (id, ref_id, image_count, left_x, width) in [
        ("default_stateplayoption_option_gauge", 40, 6, 254, 96),
        ("default_stateplayoption_option_dp", 54, 4, 381, 129),
        ("default_stateplayoption_option_hsfix", 55, 5, 550, 126),
    ] {
        let imageset = decoded
            .document
            .imageset
            .iter()
            .find(|imageset| imageset.id == id)
            .unwrap_or_else(|| panic!("Luxe Flat should decode {id}"));
        assert_eq!(imageset.ref_id, ref_id);
        assert_eq!(imageset.images.len(), image_count);
        assert!(decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == id
                    && destination.act.is_none()
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_render::skin::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x) && frame.w == Some(width)
            )
        )));
    }
    for (id, ref_id, left_x) in [
        ("default_stateplayoption_random", 344, 69),
        ("default_stateplayoption_random_2p", 345, 721),
    ] {
        let imageset = decoded
            .document
            .imageset
            .iter()
            .find(|imageset| imageset.id == id)
            .unwrap_or_else(|| panic!("Luxe Flat should decode {id}"));
        assert_eq!(imageset.ref_id, ref_id);
        assert_eq!(imageset.images.len(), 12);
        assert!(decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == id
                    && destination.act.is_none()
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_render::skin::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x) && frame.w == Some(138)
                    )
        )));
    }
    assert!(
        decoded
            .document
            .panel
            .iter()
            .any(|panel| panel.id == "bmz_select_option_hit" && panel.color == "00000000")
    );
    for (act, left_x, width) in
        [(42, 69, 138), (40, 254, 96), (54, 381, 129), (55, 550, 126), (43, 721, 138)]
    {
        assert!(decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == "bmz_select_option_hit"
                    && destination.act == Some(act)
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_render::skin::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x) && frame.w == Some(width)
            )
        )));
    }
    for (id, ref_id, left_x) in [
        ("default_optionpanel_option_random", 344, 536),
        ("default_optionpanel_option_random2", 345, 1166),
    ] {
        let imageset = decoded
            .document
            .imageset
            .iter()
            .find(|imageset| imageset.id == id)
            .unwrap_or_else(|| panic!("Luxe Flat should decode {id}"));
        assert_eq!(imageset.ref_id, ref_id);
        assert_eq!(imageset.images.len(), 12);
        assert!(decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == id
                    && matches!(
                        destination.dst.first(),
                        Some(bmz_render::skin::SkinDstEntry::Frame(frame))
                            if frame.x == Some(left_x)
                                && frame.y == Some(153)
                                && frame.w == Some(213)
                                && frame.h == Some(658)
                    )
        )));
    }
    assert!(decoded.document.source.iter().any(|source| {
        source.id == "src-default-optionpanel-panel1"
            && source.path.ends_with("default_optionpanel/option1_panel_bmz.png")
    }));
    assert!(decoded.document.source.iter().any(|source| {
        source.id == "option1_text"
            && source.path.ends_with("default_optionpanel/option1_text_bmz.png")
    }));
    assert!(decoded.document.source.iter().any(|source| {
        source.id == "src-default-optionpanel-random-cursor-bmz"
            && source.path.ends_with("default_optionpanel/random_cursor_bmz.png")
    }));
    let option_panel_image = image::open(
        skin_path
            .parent()
            .expect("Luxe Flat skin directory")
            .join("select_skinparts/default_optionpanel/option1_panel_bmz.png"),
    )
    .expect("Luxe Flat extended option panel image")
    .into_rgba8();
    for x in (560..=724).chain(1190..=1354) {
        let background = option_panel_image.get_pixel(x, 910);
        assert_eq!(background, option_panel_image.get_pixel(x, 912));
        assert_eq!(
            background,
            option_panel_image.get_pixel(x, 911),
            "Luxe Flat should not leave a black seam below MF-RANDOM at x={x}"
        );
        assert_ne!(*background, image::Rgba([0, 0, 0, 255]));
    }
    let random_source = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-stateplayoption-random-bmz")
        .expect("Luxe Flat F-RANDOM source");
    assert_eq!((random_source.size.width, random_source.size.height), (138.0, 42.0));
    let random_texture = random_source.texture;
    let option_panel_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-optionpanel-panel1")
        .expect("Luxe Flat extended option panel source")
        .texture;
    let option_panel_random_source = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "src-default-optionpanel-random-cursor-bmz")
        .expect("Luxe Flat extended option panel cursor source");
    assert_eq!(
        (option_panel_random_source.size.width, option_panel_random_source.size.height),
        (213.0, 1158.0)
    );
    let option_panel_random_texture = option_panel_random_source.texture;
    let document_textures =
        decoded.sources.iter().map(|source| bmz_render::skin::SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: bmz_render::skin::SkinImageSize {
                width: source.size.width,
                height: source.size.height,
            },
        });
    let context = bmz_render::skin::SkinContext::from_manifest_and_document(
        bmz_render::skin::default_skin_manifest(),
        decoded.document,
        document_textures,
    );
    let snapshot = bmz_render::scene::SelectSnapshot {
        arrange: "F-RANDOM".to_string(),
        arrange_2p: "MF-RANDOM".to_string(),
        ..Default::default()
    };
    let items = context.select_document_items_with_dynamic_timers(&snapshot, None);
    for (left_x, uv_y) in [(69.0, 0.0), (721.0, 0.5)] {
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. }
                    if *texture == random_texture
                        && (rect.x - left_x / 1920.0).abs() < 0.001
                        && (uv.y - uv_y).abs() < 0.001
            )),
            "Luxe Flat should render the extended arrange image at x={left_x}"
        );
    }
    let hit = context
        .select_click_hit(&snapshot, 100.0 / 1920.0, 0.98)
        .expect("Luxe Flat arrange cell should be clickable from its left half");
    assert_eq!(hit.target, bmz_render::skin::SkinClickTarget::Event { event_id: 42, click: 0 });
    assert!((hit.rect.x - 69.0 / 1920.0).abs() < f32::EPSILON);
    assert!((hit.rect.width - 138.0 / 1920.0).abs() < f32::EPSILON);

    let option_panel_snapshot = bmz_render::scene::SelectSnapshot {
        time: bmz_core::time::TimeUs(300_000),
        option_panel_time: bmz_core::time::TimeUs(300_000),
        option_panel: 1,
        arrange: "F-RANDOM".to_string(),
        arrange_2p: "MF-RANDOM".to_string(),
        ..Default::default()
    };
    let option_panel_items =
        context.select_document_items_with_dynamic_timers(&option_panel_snapshot, None);
    assert!(option_panel_items.iter().any(|item| matches!(
        item,
        bmz_render::skin::SkinRenderItem::Image { texture, rect, .. }
            if *texture == option_panel_texture
                && rect.x.abs() < 0.001
                && rect.y.abs() < 0.001
                && (rect.width - 1.0).abs() < 0.001
                && (rect.height - 1.0).abs() < 0.001
    )));
    for (left_x, uv_y) in [(536.0, 41.0 / 1158.0), (1166.0, 0.0)] {
        assert!(
            option_panel_items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. }
                    if *texture == option_panel_random_texture
                        && (rect.x - left_x / 1920.0).abs() < 0.001
                        && (rect.y - 269.0 / 1080.0).abs() < 0.001
                        && (rect.height - 658.0 / 1080.0).abs() < 0.001
                        && (uv.y - uv_y).abs() < 0.001
            )),
            "Luxe Flat should render the extended option panel arrange cursor at x={left_x}"
        );
    }
}

#[test]
fn rm_skin_play_lua_skins_can_be_decoded_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rm-skin");
    let cases = [
        (root.join("play5main.luaskin"), SkinKind::Play),
        (root.join("play7main.luaskin"), SkinKind::Play),
        (root.join("play9main.luaskin"), SkinKind::Play),
    ];
    for (skin_path, kind) in cases {
        if !skin_path.is_file() {
            continue;
        }
        let decoded = decode_beatoraja_skin(&skin_path, kind).unwrap();
        assert!(!decoded.document.destination.is_empty(), "{}", skin_path.display());
    }
}

#[test]
fn ecfn_lua_skins_can_be_decoded_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN");
    let cases = [
        (root.join("select/select.luaskin"), SkinKind::Select),
        (root.join("play/play7.luaskin"), SkinKind::Play),
        (root.join("RESULT/result.luaskin"), SkinKind::Result),
    ];
    for (skin_path, kind) in cases {
        if !skin_path.is_file() {
            continue;
        }
        let decoded = decode_beatoraja_skin(&skin_path, kind).unwrap();
        assert!(!decoded.document.destination.is_empty());
    }
}

#[test]
fn luxe_flat_lua_select_skin_keeps_score_availability_guards_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Luxez-Flat/music_select.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Select).unwrap();
    let clear_state = decoded
        .document
        .destination
        .iter()
        .find_map(|entry| match entry {
            DestinationListEntry::Single(destination)
                if destination.id == "default_playerdata_state_clear" =>
            {
                Some(destination)
            }
            DestinationListEntry::Single(_) | DestinationListEntry::Conditional { .. } => None,
        })
        .expect("Luxe Flat should retain the player clear-state destination");
    assert_eq!(clear_state.draw, "select_score_available()");
}

#[test]
fn mz_select_lua_select_skin_keeps_local_score_availability_guards() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/music_select.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Select).unwrap();
    let guarded = decoded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            DestinationListEntry::Single(destination)
                if destination.id.starts_with("default_playerdata_")
                    && destination.draw == "select_score_available()" =>
            {
                Some(destination.id.as_str())
            }
            DestinationListEntry::Single(_) | DestinationListEntry::Conditional { .. } => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(guarded.len(), 21, "mz-select player-data score guards: {guarded:?}");
    assert!(guarded.contains(&"default_playerdata_state_clear"));
    assert!(guarded.contains(&"default_playerdata_score_count"));
    assert!(guarded.contains(&"default_playerdata_scorerate_dot_count"));
}
