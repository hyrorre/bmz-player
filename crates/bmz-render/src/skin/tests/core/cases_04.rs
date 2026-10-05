use super::*;

#[test]
fn skin_value_number_evaluates_value_expr() {
    let state = SkinDrawState {
        total_duration_ms: 305_000,
        duration_green_ms: Some(183_000),
        ..SkinDrawState::default()
    };
    let value = SkinValueDef {
        id: "lanecover-green".to_string(),
        src: String::new(),
        value_expr: "0.6*number(312)".to_string(),
        ..Default::default()
    };
    assert_eq!(skin_value_number(&value, &state), Some(183_000));
}

#[test]
fn skin_value_number_for_destination_prefers_value_expr_over_ref_zero_fallback() {
    let state = SkinDrawState {
        play_level: 12,
        total_duration_ms: 500,
        duration_green_ms: Some(300),
        ..SkinDrawState::default()
    };
    let value = SkinValueDef {
        id: "lanecover-green".to_string(),
        src: String::new(),
        value_expr: "0.6*number(312)".to_string(),
        ..Default::default()
    };
    assert_eq!(skin_value_number_for_destination(&value, &state), Some(300));
}

#[test]
fn lua_runtime_value_callbacks_feed_number_and_text_rendering() {
    let state = SkinDrawState {
        lua_runtime: Some(SkinLuaRuntimeContext {
            runtime: Arc::new(AlternatingLuaDrawRuntime::default()),
            enabled_options: Arc::from([]),
            text_values: Arc::new(BTreeMap::new()),
        }),
        ..SkinDrawState::default()
    };
    let value =
        SkinValueDef { value_expr: "bmz:lua_value_callback:1".to_string(), ..Default::default() };
    assert_eq!(skin_value_number(&value, &state), Some(42));

    let text =
        SkinTextDef { value_expr: "bmz:lua_value_callback:2".to_string(), ..Default::default() };
    assert_eq!(
        skin_state_text_with_draw_state(&text, Some(&state), &SkinTextState::default()),
        "runtime text"
    );
}

#[test]
fn skin_value_number_evaluates_floor_division_value_expr() {
    let state = SkinDrawState {
        total_notes: 74,
        judge_counts: DisplayJudgeCounts { pgreat: 1, great: 1, good: 1, ..Default::default() },
        ..SkinDrawState::default()
    };
    let value = SkinValueDef {
        id: "pscore".to_string(),
        src: String::new(),
        value_expr: "floor((100000*number(110)+70000*number(111)+40000*number(112))/number(74))"
            .to_string(),
        ..Default::default()
    };

    assert_eq!(skin_value_number(&value, &state), Some(2837));
}

#[test]
fn skin_value_number_evaluates_remain_rate_scaled_after_division() {
    let state = SkinDrawState {
        total_notes: 100,
        judge_counts: DisplayJudgeCounts {
            pgreat: 30,
            great: 20,
            good: 5,
            bad: 3,
            poor: 2,
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    let value = SkinValueDef {
            id: "remain-rate-num".to_string(),
            src: String::new(),
            value_expr:
                "(number(106)-number(110)-number(111)-number(112)-number(113)-number(114))/number(106)*100"
                    .to_string(),
            ..Default::default()
        };
    let afterdot = SkinValueDef {
            id: "remain-rate-adot-num".to_string(),
            src: String::new(),
            value_expr:
                "(number(106)-number(110)-number(111)-number(112)-number(113)-number(114))/number(106)*10000"
                    .to_string(),
            ..Default::default()
        };

    assert_eq!(skin_value_number(&value, &state), Some(40));
    assert_eq!(skin_value_number(&afterdot, &state), Some(4000));
}

#[test]
fn skin_value_number_truncates_lua_value_expr_like_beatoraja_integer_property() {
    let state = SkinDrawState {
        total_notes: 2480,
        judge_counts: DisplayJudgeCounts { pgreat: 1, ..Default::default() },
        adjusted_rate: Some(0.6),
        adjusted_rate_adot: Some(60),
        ..SkinDrawState::default()
    };
    let remain_integer = SkinValueDef {
            id: "remain-rate-num".to_string(),
            src: String::new(),
            value_expr:
                "(number(106)-number(110)-number(111)-number(112)-number(113)-number(114))/number(106)*100"
                    .to_string(),
            ..Default::default()
        };
    let remain_afterdot = SkinValueDef {
            id: "remain-rate-adot-num".to_string(),
            src: String::new(),
            value_expr:
                "(number(106)-number(110)-number(111)-number(112)-number(113)-number(114))/number(106)*10000"
                    .to_string(),
            ..Default::default()
        };
    let adjusted_integer = SkinValueDef {
        id: "adjusted-rate-num".to_string(),
        src: String::new(),
        value_expr: SKIN_EXPR_ADJUSTED_RATE.to_string(),
        ..Default::default()
    };

    assert_eq!(skin_value_number(&remain_integer, &state), Some(99));
    assert_eq!(skin_value_number(&remain_afterdot, &state), Some(9995));
    assert_eq!(skin_value_number(&adjusted_integer, &state), Some(0));
}

#[test]
fn skin_state_float_expr_evaluates_option_weighted_terms() {
    let expr = "0.102*option(180)*number(350)+0.09*option(181)*number(350)";
    let very_hard = SkinDrawState {
        judge_rank: Some(0),
        select_screen: true,
        select_total_notes: 100,
        ..SkinDrawState::default()
    };
    let hard = SkinDrawState {
        judge_rank: Some(1),
        select_screen: true,
        select_total_notes: 100,
        ..SkinDrawState::default()
    };

    assert!((skin_state_float_expr(expr, &very_hard).unwrap() - 10.2).abs() < 0.001);
    assert!((skin_state_float_expr(expr, &hard).unwrap() - 9.0).abs() < 0.001);
}

#[test]
fn skin_state_text_maps_string_refs() {
    let ir_ranking = crate::scene::ResultIrSnapshot {
        online: true,
        state: crate::scene::ResultIrState::Loaded,
        provider_name: crate::scene::ResultIrRankingName::from_display_name("rianIR"),
        user_name: crate::scene::ResultIrRankingName::from_display_name("hyrorre"),
        entries: [
            crate::scene::ResultIrRankingEntrySnapshot {
                rank: Some(1),
                ex_score: Some(2000),
                clear_index: Some(8),
                player_name: crate::scene::ResultIrRankingName::from_display_name("Alice"),
            },
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
            crate::scene::ResultIrRankingEntrySnapshot::default(),
        ],
        ..Default::default()
    };
    let state = SkinTextState {
        player_name: "BMZ Player",
        title: "My Title",
        subtitle: "Sub",
        artist: "Artist Name",
        subartist: "Feat. X",
        genre: "TRANCE",
        target: "RANK_AAA",
        select_chart_replication: "RIVALCHART",
        ir_ranking: &ir_ranking,
        course_titles: [
            "Stage 1", "Stage 2", "Stage 3", "Stage 4", "Stage 5", "Stage 6", "Stage 7", "Stage 8",
            "Stage 9", "Stage 10",
        ],
        ..SkinTextState::default()
    };

    let make_text = |ref_id: i32| SkinTextDef {
        id: "t".to_string(),
        ref_id,
        constant_text: String::new(),
        ..SkinTextDef::default()
    };

    // STRING_TITLE (10)
    assert_eq!(skin_state_text(&make_text(10), &state), "My Title");
    // STRING_SUBTITLE (11)
    assert_eq!(skin_state_text(&make_text(11), &state), "Sub");
    // STRING_FULLTITLE (12) = title + " " + subtitle
    assert_eq!(skin_state_text(&make_text(12), &state), "My Title Sub");
    // STRING_GENRE (13)
    assert_eq!(skin_state_text(&make_text(13), &state), "TRANCE");
    // STRING_ARTIST (14)
    assert_eq!(skin_state_text(&make_text(14), &state), "Artist Name");
    // STRING_SUBARTIST (15)
    assert_eq!(skin_state_text(&make_text(15), &state), "Feat. X");
    // STRING_FULLARTIST (16) = artist + " " + subartist
    assert_eq!(skin_state_text(&make_text(16), &state), "Artist Name Feat. X");
    // STRING_RIVAL (1) is also target score player name during play in beatoraja.
    assert_eq!(skin_state_text(&make_text(1), &state), "RANK AAA");
    assert_eq!(
        skin_state_text(&make_text(1), &SkinTextState { rival: "Rival A", ..state.clone() }),
        "Rival A"
    );
    // STRING_PLAYER (2)
    assert_eq!(skin_state_text(&make_text(2), &state), "BMZ Player");
    // STRING_TARGET (3)
    assert_eq!(skin_state_text(&make_text(3), &state), "RANK AAA");
    // STRING_CHART_REPLICATION_MODE (86)
    assert_eq!(skin_state_text(&make_text(86), &state), "RIVALCHART");
    // STRING_TARGETNAME_P1/N1 (209/210)
    assert_eq!(skin_state_text(&make_text(209), &state), "RANK AAA-");
    assert_eq!(skin_state_text(&make_text(210), &state), "RANK MAX-");
    assert_eq!(select_target_name("RIVAL_2"), "RIVAL 2");
    assert_eq!(select_target_name("AAA"), "RANK AAA");
    // STRING_RANKINGNAME1..10
    assert_eq!(skin_state_text(&make_text(120), &state), "Alice");
    assert_eq!(skin_state_text(&make_text(121), &state), "");
    // STRING_COURSE1_TITLE..10_TITLE (150..159)
    assert_eq!(skin_state_text(&make_text(150), &state), "Stage 1");
    assert_eq!(skin_state_text(&make_text(159), &state), "Stage 10");
    // STRING_IR_NAME / STRING_IR_USERNAME
    assert_eq!(skin_state_text(&make_text(1020), &state), "rianIR");
    assert_eq!(skin_state_text(&make_text(1021), &state), "hyrorre");
    let idle_ir_ranking = crate::scene::ResultIrSnapshot {
        online: true,
        state: crate::scene::ResultIrState::Offline,
        provider_name: crate::scene::ResultIrRankingName::from_display_name("rianIR"),
        ..Default::default()
    };
    let idle_ir_state = SkinTextState { ir_ranking: &idle_ir_ranking, ..state.clone() };
    assert_eq!(skin_state_text(&make_text(1020), &idle_ir_state), "rianIR");
    // Unknown ref → empty
    assert_eq!(skin_state_text(&make_text(99), &state), "");

    let difficulty = SkinTextDef {
        id: "difficulty_name".to_string(),
        value_expr: SKIN_EXPR_DIFFICULTY_NAME.to_string(),
        ..SkinTextDef::default()
    };
    assert_eq!(
        skin_state_text(
            &difficulty,
            &SkinTextState { difficulty_name: "ANOTHER", ..SkinTextState::default() },
        ),
        "ANOTHER"
    );
    assert_eq!(
        skin_state_text(
            &SkinTextDef { id: "difficulty_name".to_string(), ..SkinTextDef::default() },
            &SkinTextState { difficulty_name: "ANOTHER", ..SkinTextState::default() },
        ),
        ""
    );

    let m_select_bar_text =
        SkinTextDef { id: "default_songlist2_bartext".to_string(), ..SkinTextDef::default() };
    assert_eq!(
        skin_state_text(
            &m_select_bar_text,
            &SkinTextState { bar_text: "Song Title", ..SkinTextState::default() },
        ),
        "Song Title"
    );
    for id in ["bar_text", "bar_text_gray", "folderbar_text", "Folder-Bar_Text"] {
        assert_eq!(
            skin_state_text(
                &SkinTextDef { id: id.to_string(), ..SkinTextDef::default() },
                &SkinTextState { bar_text: "LITONE Song Title", ..SkinTextState::default() },
            ),
            "LITONE Song Title",
            "select bar text id {id}"
        );
    }
    for id in ["foobar_texture", "bartexture", "folderbar_textured"] {
        assert_eq!(
            skin_state_text(
                &SkinTextDef { id: id.to_string(), ref_id: 10, ..SkinTextDef::default() },
                &SkinTextState {
                    title: "Ordinary Text",
                    bar_text: "Wrong Bar Text",
                    ..SkinTextState::default()
                },
            ),
            "Ordinary Text",
            "non-bar text id {id}"
        );
    }
}

#[test]
fn skin_state_text_formats_bmz_number_ref_extension() {
    let text = SkinTextDef {
        id: "gauge_text".to_string(),
        number_ref: Some(107),
        prefix: "GAUGE ".to_string(),
        suffix: "%".to_string(),
        ..SkinTextDef::default()
    };
    let draw_state = SkinDrawState { gauge: 78.6, ..SkinDrawState::default() };

    assert_eq!(
        skin_state_text_with_draw_state(&text, Some(&draw_state), &SkinTextState::default()),
        "GAUGE 78%"
    );
    assert_eq!(skin_state_text(&text, &SkinTextState::default()), "");
}

#[test]
fn lua_text_values_preserve_resolved_target_names_and_select_option_labels() {
    let play = SkinDrawState { play_screen: true, ..Default::default() };
    for name in ["ライバル_A", "AAA", "NONE", "RIVAL_2", "IR_TOP", ""] {
        let text = SkinTextState {
            target: "RANK_AAA",
            resolved_target_name: Some(name),
            ..Default::default()
        };
        let values = lua_main_state_text_values(&play, &text);
        assert_eq!(values[&1], name);
        assert_eq!(values[&3], "RANK AAA");
        assert_eq!(values[&209], "RANK AAA-");
        assert_eq!(values[&210], "RANK MAX-");
        assert_eq!(play_target_name(text.target, Some(name)), name);
    }

    for (target, expected) in [("IR_TOP", ""), ("RANK_AAA", "RANK AAA"), ("NONE", ""), ("", "")] {
        let text = SkinTextState { target, ..Default::default() };
        let values = lua_main_state_text_values(&play, &text);
        assert_eq!(values[&1], expected);
        assert_eq!(values[&3], target_setting_name(target));
        assert_eq!(play_target_name(target, None), expected);
    }

    let select = SkinDrawState { select_screen: true, ..Default::default() };
    let values = lua_main_state_text_values(
        &select,
        &SkinTextState { target: "NONE", rival: "Selected rival", ..Default::default() },
    );
    assert_eq!(values[&1], "Selected rival");
    assert_eq!(values[&3], "NO TARGET");
}

#[test]
fn text_render_item_applies_search_word_alpha_multiplier_for_ref_30() {
    let document: SkinDocument =
        serde_json::from_value(serde_json::json!({ "w": 1920, "h": 1080 })).unwrap();
    let text = SkinTextDef { id: "search".to_string(), ref_id: 30, ..SkinTextDef::default() };
    let frame = ResolvedSkinFrame { w: 100, h: 24, ..ResolvedSkinFrame::default() };
    let state =
        SkinTextState { search_word: "hello", search_word_alpha: 0.5, ..SkinTextState::default() };
    let item = document.text_render_item(&text, frame, &state).unwrap();
    match item {
        SkinRenderItem::Text { style, .. } => {
            // frame.a=255 (1.0) * 0.5 = 0.5
            assert!((style.color.a - 0.5).abs() < 1e-4, "got alpha {}", style.color.a);
        }
        other => panic!("expected SkinRenderItem::Text, got {other:?}"),
    }
}

#[test]
fn text_render_item_keeps_empty_search_word_with_caret() {
    let document: SkinDocument =
        serde_json::from_value(serde_json::json!({ "w": 1920, "h": 1080 })).unwrap();
    let text = SkinTextDef { id: "search".to_string(), ref_id: 30, ..SkinTextDef::default() };
    let frame = ResolvedSkinFrame { w: 100, h: 24, ..ResolvedSkinFrame::default() };
    let state = SkinTextState {
        search_word: "",
        search_caret_byte_index: Some(0),
        ..SkinTextState::default()
    };

    let item = document.text_render_item(&text, frame, &state).unwrap();

    assert!(matches!(
        item,
        SkinRenderItem::Text { text, caret: Some(TextCaret { byte_index: 0, .. }), .. }
            if text.is_empty()
    ));
}

#[test]
fn text_render_item_leaves_alpha_unchanged_for_other_refs() {
    let document: SkinDocument =
        serde_json::from_value(serde_json::json!({ "w": 1920, "h": 1080 })).unwrap();
    let text = SkinTextDef {
        id: "title".to_string(),
        ref_id: 10, // title, not search
        ..SkinTextDef::default()
    };
    let frame = ResolvedSkinFrame { w: 100, h: 24, ..ResolvedSkinFrame::default() };
    let state = SkinTextState {
        title: "song name",
        search_word_alpha: 0.1, // should be ignored for non-search refs
        ..SkinTextState::default()
    };
    let item = document.text_render_item(&text, frame, &state).unwrap();
    match item {
        SkinRenderItem::Text { style, .. } => {
            assert!((style.color.a - 1.0).abs() < 1e-4, "got alpha {}", style.color.a);
        }
        other => panic!("expected SkinRenderItem::Text, got {other:?}"),
    }
}

#[test]
fn text_render_item_separates_bitmap_font_size_from_destination_height() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w": 100,
        "h": 100,
        "font": [
            { "id": "bitmap", "path": "artist.fnt" },
            { "id": "lr2", "path": "artist.lr2font" },
            { "id": "vector", "path": "artist.ttf" }
        ]
    }))
    .unwrap();
    let frame = ResolvedSkinFrame { w: 80, h: 28, ..ResolvedSkinFrame::default() };
    let state = SkinTextState::default();
    let bitmap_text = SkinTextDef {
        id: "artist".to_string(),
        font: "result:bitmap".to_string(),
        size: 17,
        constant_text: "Aoi".to_string(),
        ..SkinTextDef::default()
    };
    let vector_text = SkinTextDef {
        id: "artist_vector".to_string(),
        font: "vector".to_string(),
        size: 17,
        constant_text: "Aoi".to_string(),
        ..SkinTextDef::default()
    };
    let lr2_text = SkinTextDef {
        id: "artist_lr2".to_string(),
        font: "result:lr2".to_string(),
        constant_text: "Aoi".to_string(),
        ..SkinTextDef::default()
    };

    let bitmap_item = document.text_render_item(&bitmap_text, frame, &state).unwrap();
    let lr2_item = document.text_render_item(&lr2_text, frame, &state).unwrap();
    let vector_item = document.text_render_item(&vector_text, frame, &state).unwrap();

    match bitmap_item {
        SkinRenderItem::Text { style, .. } => {
            assert!(approx_eq(style.size, 0.28), "got {}", style.size);
            assert_eq!(style.bitmap_size, Some(0.17));
        }
        other => panic!("expected SkinRenderItem::Text, got {other:?}"),
    }
    match lr2_item {
        SkinRenderItem::Text { style, .. } => {
            assert!(approx_eq(style.size, 0.28), "got {}", style.size);
            assert_eq!(style.bitmap_size, Some(0.28));
        }
        other => panic!("expected SkinRenderItem::Text, got {other:?}"),
    }
    match vector_item {
        SkinRenderItem::Text { style, .. } => {
            assert!(approx_eq(style.size, 0.28), "got {}", style.size);
            assert_eq!(style.bitmap_size, None);
        }
        other => panic!("expected SkinRenderItem::Text, got {other:?}"),
    }
}

#[test]
fn lr2_system_font_uses_destination_height_and_scaled_width() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w": 640, "h": 480,
        "text": [{"id":"text", "constantText":"TITLE", "align":1,
            "lr2SystemFont":{"size":60, "thickness":4, "type":3}}]
    }))
    .unwrap();
    for height in [20, 120] {
        let frame = ResolvedSkinFrame { x: 320, w: 300, h: height, a: 128, ..Default::default() };
        let item =
            document.text_render_item(&document.text[0], frame, &SkinTextState::default()).unwrap();
        let SkinRenderItem::Text { origin, style, .. } = item else { panic!("text") };
        let width = 300.0 / 640.0 * height as f32 / 60.0;
        assert!(approx_eq(style.size, height as f32 / 480.0));
        assert!(approx_eq(style.max_width, width));
        assert!(approx_eq(origin.x, 0.5 - width / 2.0));
        assert!(approx_eq(style.outline.unwrap().width, height as f32 / 60.0 / 480.0));
        assert!(approx_eq(style.outline.unwrap().color.a, 128.0 / 255.0));
    }
}

#[test]
fn skin_state_text_uses_constant_text_over_ref_id() {
    let state = SkinTextState { title: "Ignored", ..SkinTextState::default() };
    let text = SkinTextDef {
        id: "t".to_string(),
        ref_id: 10,
        constant_text: "Hardcoded".to_string(),
        ..SkinTextDef::default()
    };
    assert_eq!(skin_state_text(&text, &state), "Hardcoded");
}

#[test]
fn full_label_handles_empty_components() {
    // both empty
    assert_eq!(full_label("", ""), "");
    // only primary
    assert_eq!(full_label("Title", ""), "Title");
    // only secondary
    assert_eq!(full_label("", "Sub"), "Sub");
    // both present
    assert_eq!(full_label("Title", "Sub"), "Title Sub");
}

#[test]
fn loop_at_cycle_end_holds_final_frame() {
    // loop == cycle（終端へループバック）: 1回再生して最終フレームを保持する。
    // lane-bg(loop:1000,終端1000) や keybeam(loop:100,終端100) の挙動。
    assert_eq!(resolve_loop_elapsed(1000, 500, 1000), 500); // 再生中
    assert_eq!(resolve_loop_elapsed(1000, 1000, 1000), 1000); // 終端
    assert_eq!(resolve_loop_elapsed(1000, 5000, 1000), 1000); // 終端超過 → 保持
    // loop > cycle も終端で停止する。
    assert_eq!(resolve_loop_elapsed(300, 5000, 200), 200);
}

#[test]
fn loop_before_cycle_end_repeats_segment() {
    // loop < cycle: [loop, cycle) 区間を繰り返す。
    assert_eq!(resolve_loop_elapsed(0, 150, 200), 150); // 再生中はそのまま
    assert_eq!(resolve_loop_elapsed(0, 350, 200), 150); // 350 → 150 へループ
    assert_eq!(resolve_loop_elapsed(100, 350, 200), 150); // (350-100)%100+100
}
