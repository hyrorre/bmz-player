use super::*;

#[test]
fn lr2_2p_bomb_destination_uses_play_key_mode_op() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "bomb.png" }],
                "image": [{ "id": "bomb-img", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "divx": 16, "cycle": 251 }],
                "destination": [
                    { "id": "bomb-img", "timer": 61, "op": [162], "loop": -1, "dst": [
                        { "time": 0, "x": 10, "y": 10, "w": 10, "h": 10 },
                        { "time": 250, "x": 10, "y": 10, "w": 10, "h": 10 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(9),
            source_size: SkinImageSize { width: 160.0, height: 10.0 },
        },
    )]);
    let bomb_ms = {
        let mut a = [None; LANE_COUNT];
        a[Lane::Key8.index()] = Some(0);
        a
    };

    let active_14k = SkinDrawState { key_mode: KeyMode::K14, bomb_ms, ..Default::default() };
    let inactive_7k = SkinDrawState { key_mode: KeyMode::K7, bomb_ms, ..Default::default() };

    assert_eq!(document.static_image_render_items(&sources, &active_14k).len(), 1);
    assert!(document.static_image_render_items(&sources, &inactive_7k).is_empty());
}

#[test]
fn note_rect_for_progress_shifts_with_lift() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 720, "h": 720,
                "image": [
                    { "id": "n1", "src": 1, "x": 0, "y": 0, "w": 50, "h": 12 }
                ],
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "dst": [{ "time": 0, "x": 10, "y": 140, "w": 50, "h": 580 }]
                }
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let note_height = 12.0 / 720.0;
    let state_no_lift = SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() };
    let state_lifted = SkinDrawState { offset_lift_px: 72, ..SkinDrawState::default() };

    let rect_no_lift = skin
        .note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.0, note_height, &state_no_lift)
        .unwrap();
    let rect_lifted = skin
        .note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.0, note_height, &state_lifted)
        .unwrap();

    let judge_no_lift = 580.0 / 720.0;
    let judge_lifted = judge_no_lift - 72.0 / 720.0;
    assert!(approx_eq(rect_no_lift.y + note_height, judge_no_lift));
    assert!(approx_eq(rect_lifted.y + note_height, judge_lifted));
    assert!(
        rect_lifted.y < rect_no_lift.y,
        "expected lifted note higher on screen, got no_lift={} lifted={}",
        rect_no_lift.y,
        rect_lifted.y
    );
}

#[test]
fn pms_note_expansion_uses_quarter_note_elapsed_time() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 100, "h": 100,
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "expansionrate": [150, 80],
                    "dst": [{ "time": 0, "x": 10, "y": 20, "w": 30, "h": 60 }]
                },
                "destination": [{ "id": "notes", "offset": 30 }]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);

    let peak = skin.document_note_expansion_scale(&SkinDrawState {
        quarter_note_elapsed_ms: Some(9),
        ..SkinDrawState::default()
    });
    let finished = skin.document_note_expansion_scale(&SkinDrawState {
        quarter_note_elapsed_ms: Some(159),
        ..SkinDrawState::default()
    });

    assert!(approx_eq(peak.0, 1.5));
    assert!(approx_eq(peak.1, 0.8));
    assert!(approx_eq(finished.0, 1.0));
    assert!(approx_eq(finished.1, 1.0));
}

#[test]
fn pms_missed_note_falls_toward_dst2() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 100, "h": 100,
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "size": [10],
                    "dst2": 90,
                    "dst": [{ "time": 0, "x": 10, "y": 20, "w": 30, "h": 60 }]
                }
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let state = SkinDrawState::default();

    let start = skin.missed_note_rect_for_fall(Lane::Key1, KeyMode::K9, 0.0, 0.1, &state).unwrap();
    let end = skin.missed_note_rect_for_fall(Lane::Key1, KeyMode::K9, 1.0, 0.1, &state).unwrap();

    assert!(approx_eq(start.y + start.height, 0.8));
    assert!(approx_eq(end.y + end.height, 0.1));
}

#[test]
fn note_body_rect_shifts_with_lift() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 720, "h": 720,
                "image": [
                    { "id": "n1", "src": 1, "x": 0, "y": 0, "w": 50, "h": 12 }
                ],
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "dst": [{ "time": 0, "x": 10, "y": 140, "w": 50, "h": 580 }]
                }
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let state_no_lift = SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() };
    let state_lifted = SkinDrawState { offset_lift_px: 72, ..SkinDrawState::default() };

    let rect_no_lift =
        skin.note_body_rect(Lane::Key1, KeyMode::K7, 0.0, 0.5, &state_no_lift).unwrap();
    let rect_lifted =
        skin.note_body_rect(Lane::Key1, KeyMode::K7, 0.0, 0.5, &state_lifted).unwrap();

    // beatoraja 座標系（y-up）での body 位置:
    //   body.y      = tail_bottom = area.height * (1 - tail_y) = 580/720 * 0.5 = 290/720
    //   body.height = head_top - tail_bottom = (head_bottom - note_height) - tail_bottom
    //               = (580/720 - 12/720) - 290/720 = 278/720
    assert!(approx_eq(rect_no_lift.y, (580.0 * 0.5) / 720.0));
    assert!(approx_eq(rect_no_lift.height, (580.0 * 0.5 - 12.0) / 720.0));
    assert!(
        rect_lifted.y < rect_no_lift.y,
        "expected lifted long body higher on screen, got no_lift={} lifted={}",
        rect_no_lift.y,
        rect_lifted.y
    );
    assert!(rect_lifted.height <= rect_no_lift.height + 0.0001);
}

#[test]
fn notes_offset_keeps_long_note_caps_joined_to_body() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 100, "h": 100,
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "size": [10],
                    "dst": [{ "time": 0, "x": 10, "y": 20, "w": 30, "h": 60 }]
                },
                "destination": [{ "id": "notes", "offset": 30 }]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let note_height = skin.document_note_height(Lane::Key1, KeyMode::K7).unwrap();

    for offset_h in [-4, 6] {
        let mut offsets = SkinOffsetValues::default();
        offsets.set(
            OFFSET_NOTES_1P,
            crate::skin_offset::SkinOffsetValue { h: offset_h, ..Default::default() },
        );
        let state = SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() };
        let head =
            skin.note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.0, note_height, &state).unwrap();
        let tail =
            skin.note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.5, note_height, &state).unwrap();
        let body = skin.note_body_rect(Lane::Key1, KeyMode::K7, 0.0, 0.5, &state).unwrap();

        assert!(approx_eq(body.y, tail.y + tail.height));
        assert!(approx_eq(body.y + body.height, head.y));
        assert!(approx_eq(head.y + head.height, 0.8));
        assert!(approx_eq(tail.y + tail.height, 0.5));
        assert!(approx_eq(head.height, note_height + offset_h as f32 / 100.0));
        assert!(approx_eq(body.height, 0.2 - offset_h as f32 / 100.0));
    }
}

#[test]
fn skin_state_number_bpm_lanecover_duration_timing() {
    let state = SkinDrawState {
        now_bpm: 148.7,
        min_bpm: 80.0,
        max_bpm: 200.3,
        lane_cover: 0.25,
        total_duration_ms: 305_000,
        duration_green_ms: Some(183_000),
        judge_timing_ms: [Some(-3), Some(7), None],
        ..SkinDrawState::default()
    };
    // NUMBER_NOWBPM (160) = round(148.7) = 149
    assert_eq!(skin_state_number(160, &state), Some(149));
    // NUMBER_MINBPM (91) = round(80.0) = 80
    assert_eq!(skin_state_number(91, &state), Some(80));
    // NUMBER_MAXBPM (90) = round(200.3) = 200
    assert_eq!(skin_state_number(90, &state), Some(200));
    // NUMBER_LANECOVER1 (14) = round(0.25 * 1000) = 250
    assert_eq!(skin_state_number(14, &state), Some(250));
    // NUMBER_LIFT1 (314) = round(0.42 * 1000) = 420
    let lifted = SkinDrawState { lift: 0.42, ..state.clone() };
    assert_eq!(skin_state_number(314, &lifted), Some(420));
    let capped_cover = SkinDrawState { lane_cover: 0.9, lift: 0.2, ..state.clone() };
    assert_eq!(skin_state_number(14, &capped_cover), Some(800));
    // float_number(113) tracks BARGRAPH_BESTSCORERATE
    let best_rate =
        SkinDrawState { total_notes: 100, best_ex_score: Some(150), ..SkinDrawState::default() };
    assert!((skin_state_float_number(113, &best_rate).unwrap() - 0.75).abs() < 0.001);
    assert!(!eval_skin_draw_condition("float_number(113) == 0", &best_rate));
    assert!(eval_skin_draw_condition(
        "float_number(113) == 0",
        &SkinDrawState { total_notes: 100, best_ex_score: Some(0), ..SkinDrawState::default() }
    ));
    // BMZ keeps the green number in SkinDrawState and exposes beatoraja's duration as green*5/3.
    assert_eq!(skin_state_number(312, &state), Some(305_000));
    // NUMBER_DURATION_GREEN (313) = green number.
    assert_eq!(skin_state_number(313, &state), Some(183_000));
    assert_eq!(
        skin_state_number(
            313,
            &SkinDrawState { duration_green_ms: Some(183_001), ..state.clone() }
        ),
        Some(183_001)
    );
    let duration_state = SkinDrawState {
        now_bpm: 100.0,
        main_bpm: 100.0,
        min_bpm: 50.0,
        max_bpm: 200.0,
        hispeed: 2.0,
        lane_cover: 0.25,
        total_duration_ms: 900,
        duration_green_ms: Some(540),
        ..SkinDrawState::default()
    };
    // 1312..=1327 are lane-cover duration variants:
    // current/main/min/max BPM x cover on/off x normal/green.
    // Current-BPM variants use SkinDrawState's real note display duration; main/min/max variants
    // are theoretical values derived from their BPM.
    assert_eq!(skin_state_number(1312, &duration_state), Some(900));
    assert_eq!(skin_state_number(1313, &duration_state), Some(540));
    assert_eq!(skin_state_number(1314, &duration_state), Some(1_200));
    assert_eq!(skin_state_number(1315, &duration_state), Some(720));
    assert_eq!(skin_state_number(1317, &duration_state), Some(540));
    assert_eq!(skin_state_number(1321, &duration_state), Some(1_080));
    assert_eq!(skin_state_number(1325, &duration_state), Some(270));
    let changed_now_bpm = SkinDrawState {
        now_bpm: 150.0,
        duration_green_ms: Some(777),
        total_duration_ms: 1_295,
        ..duration_state.clone()
    };
    // WMII uses the main/min/max variants.  They should stay stable across BPM changes and
    // current-duration rounding; current-BPM variants follow the runtime display duration.
    assert_eq!(skin_state_number(1312, &changed_now_bpm), Some(1_295));
    assert_eq!(skin_state_number(1313, &changed_now_bpm), Some(777));
    assert_eq!(skin_state_number(1317, &changed_now_bpm), Some(540));
    assert_eq!(skin_state_number(1321, &changed_now_bpm), Some(1_080));
    assert_eq!(skin_state_number(1325, &changed_now_bpm), Some(270));
    let faster = SkinDrawState { hispeed: 3.0, ..duration_state.clone() };
    assert_eq!(skin_state_number(1317, &faster), Some(360));
    let lower_cover = SkinDrawState { lane_cover: 0.5, ..duration_state.clone() };
    assert_eq!(skin_state_number(1317, &lower_cover), Some(360));
    let lifted_cover = SkinDrawState {
        lift: 0.2,
        total_duration_ms: 660,
        duration_green_ms: Some(396),
        ..duration_state.clone()
    };
    assert_eq!(skin_state_number(1312, &lifted_cover), Some(660));
    assert_eq!(skin_state_number(1313, &lifted_cover), Some(396));
    assert_eq!(skin_state_number(1314, &lifted_cover), Some(960));
    // VALUE_JUDGE_1P_DURATION (525) = -(-3) = 3 (FAST 3ms は beatoraja 規約で正)
    assert_eq!(skin_state_number(525, &state), Some(3));
    // VALUE_JUDGE_2P_DURATION (526): SLOW 7ms (delta=+7) は beatoraja 規約で負
    assert_eq!(skin_state_number(526, &state), Some(-7));
    // VALUE_JUDGE_3P_DURATION (527): 領域に判定が無ければ None
    assert_eq!(skin_state_number(527, &state), None);
    // SLOW 5ms (delta=+5) は beatoraja 規約で負
    let slow = SkinDrawState { judge_timing_ms: [Some(5), None, None], ..state.clone() };
    assert_eq!(skin_state_number(525, &slow), Some(-5));
    // When no recent judgement, 525 returns None
    let no_judge = SkinDrawState { judge_timing_ms: [None; MAX_JUDGE_REGIONS], ..state.clone() };
    assert_eq!(skin_state_number(525, &no_judge), None);
}

#[test]
fn skin_image_index_number_maps_replay_slot_rules() {
    let state = SkinDrawState {
        select_replay_slot_rule_indices: [10, 1, 3, 0],
        ..SkinDrawState::default()
    };
    assert_eq!(skin_image_index_number(321, &state), Some(10));
    assert_eq!(skin_image_index_number(322, &state), Some(1));
    assert_eq!(skin_image_index_number(323, &state), Some(3));
    assert_eq!(skin_image_index_number(324, &state), Some(0));
}

#[test]
fn timing_judge_areas_follow_beatoraja_mode_windows() {
    let areas = beatoraja_timing_judge_areas(&SkinDrawState {
        key_mode: KeyMode::K7,
        judge_rank: None,
        ..SkinDrawState::default()
    });

    assert_eq!(areas[0], TimingJudgeArea { late_ms: -20.0, early_ms: 20.0 });
    assert_eq!(areas[1], TimingJudgeArea { late_ms: -60.0, early_ms: 60.0 });
    assert_eq!(areas[2], TimingJudgeArea { late_ms: -150.0, early_ms: 150.0 });
    assert_eq!(areas[3], TimingJudgeArea { late_ms: -220.0, early_ms: 280.0 });
    assert_eq!(areas[4], TimingJudgeArea { late_ms: -500.0, early_ms: 150.0 });
}

#[test]
fn timing_judge_areas_apply_pms_rank_rule() {
    let areas = beatoraja_timing_judge_areas(&SkinDrawState {
        key_mode: KeyMode::K9,
        judge_rank: Some(0),
        ..SkinDrawState::default()
    });

    assert_eq!(areas[0], TimingJudgeArea { late_ms: -20.0, early_ms: 20.0 });
    assert_eq!(areas[1], TimingJudgeArea { late_ms: -20.0, early_ms: 20.0 });
    assert_eq!(areas[2], TimingJudgeArea { late_ms: -38.61, early_ms: 38.61 });
    assert_eq!(areas[3], TimingJudgeArea { late_ms: -183.0, early_ms: 183.0 });
    assert_eq!(areas[4], TimingJudgeArea { late_ms: -500.0, early_ms: 175.0 });
}

#[test]
fn timing_judge_bands_use_effective_window_instead_of_rank_and_rule() {
    let window = bmz_gameplay::judge::model::JudgeWindow {
        bad_fast_us: 100_000,
        bad_slow_us: 125_000,
        ..bmz_gameplay::judge::model::JudgeWindow::symmetric(
            25_000, 50_000, 87_500, 100_000, 500_000, 112_500, 0,
        )
    };
    let state = SkinDrawState {
        key_mode: KeyMode::K7,
        judge_rank: Some(0),
        rule_mode_index: 1,
        note_judge_window: Some(window),
        ..Default::default()
    };
    let areas = timing_judge_areas(&state);
    assert_eq!(areas[0], TimingJudgeArea { late_ms: -25.0, early_ms: 25.0 });
    assert_eq!(areas[3], TimingJudgeArea { late_ms: -100.0, early_ms: 125.0 });
    let items = timing_judge_band_items(
        Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        200.0,
        1.0,
        BlendMode::Normal,
        [Color::rgba(1.0, 1.0, 1.0, 1.0); 5],
        &state,
    );
    assert!(items.len() >= 2, "both sides of the PGREAT band must be drawn");
    for (item, expected_x) in items.iter().take(2).zip([0.4375, 0.5]) {
        let SkinRenderItem::Rect { rect, .. } = item else { panic!("expected judge band") };
        assert!((rect.x - expected_x).abs() < 0.000_1);
        assert!((rect.width - 0.0625).abs() < 0.000_1);
    }
}

#[test]
fn timing_judge_areas_without_session_follow_rule_mode() {
    for (key_mode, pgreat, good) in [(KeyMode::K7, 16.666, 116.666), (KeyMode::K9, 25.0, 87.5)] {
        for rank in [None, Some(0), Some(4), Some(150)] {
            let areas = timing_judge_areas(&SkinDrawState {
                key_mode,
                judge_rank: rank,
                rule_mode_index: 2,
                ..Default::default()
            });
            assert_eq!(areas[0], TimingJudgeArea { late_ms: -pgreat, early_ms: pgreat });
            assert_eq!(areas[2], TimingJudgeArea { late_ms: -good, early_ms: good });
        }
    }
    for rank in [None, Some(2), Some(4)] {
        let areas = timing_judge_areas(&SkinDrawState {
            judge_rank: rank,
            rule_mode_index: 1,
            ..Default::default()
        });
        assert_eq!(areas[0], TimingJudgeArea { late_ms: -18.0, early_ms: 18.0 });
        assert_eq!(areas[2], TimingJudgeArea { late_ms: -100.0, early_ms: 100.0 });
    }
    let unknown = SkinDrawState { rule_mode_index: usize::MAX, ..Default::default() };
    assert_eq!(timing_judge_areas(&unknown), beatoraja_timing_judge_areas(&unknown));
}

#[test]
fn skin_state_text_formats_bmz_judge_region_extension() {
    let text = SkinTextDef {
        id: "judge_text".to_string(),
        judge_region: Some(0),
        ..SkinTextDef::default()
    };
    let state = SkinDrawState {
        judge_ms: [Some(120), None, None],
        judge_index: [Some(0), None, None],
        judge_timing_sign: [Some(1), None, None],
        ..SkinDrawState::default()
    };

    assert_eq!(
        skin_state_text_with_draw_state(&text, Some(&state), &SkinTextState::default()),
        "PGREAT"
    );

    let expired = SkinDrawState {
        judge_ms: [None, None, None],
        judge_index: [Some(1), None, None],
        ..SkinDrawState::default()
    };
    assert_eq!(
        skin_state_text_with_draw_state(&text, Some(&expired), &SkinTextState::default()),
        ""
    );
}

#[test]
fn skin_state_text_formats_bmz_judge_timing_region_extension() {
    let text = SkinTextDef {
        id: "judge_timing".to_string(),
        judge_timing_region: Some(0),
        ..SkinTextDef::default()
    };
    let fast = SkinDrawState {
        judge_ms: [Some(120), None, None],
        judge_timing_sign: [Some(1), None, None],
        ..SkinDrawState::default()
    };
    let slow = SkinDrawState {
        judge_ms: [Some(120), None, None],
        judge_timing_sign: [Some(-1), None, None],
        ..SkinDrawState::default()
    };
    let just = SkinDrawState {
        judge_ms: [Some(120), None, None],
        judge_timing_sign: [None, None, None],
        ..SkinDrawState::default()
    };

    assert_eq!(
        skin_state_text_with_draw_state(&text, Some(&fast), &SkinTextState::default()),
        "FAST"
    );
    assert_eq!(
        skin_state_text_with_draw_state(&text, Some(&slow), &SkinTextState::default()),
        "SLOW"
    );
    assert_eq!(skin_state_text_with_draw_state(&text, Some(&just), &SkinTextState::default()), "");
}

#[test]
fn text_render_item_colors_bmz_judge_region_by_category() {
    let document: SkinDocument =
        serde_json::from_value(serde_json::json!({ "w": 1920, "h": 1080 })).unwrap();
    let text = SkinTextDef {
        id: "judge".to_string(),
        judge_region: Some(0),
        judge_color: true,
        ..SkinTextDef::default()
    };
    let frame = ResolvedSkinFrame {
        w: 100,
        h: 24,
        a: 128,
        r: 255,
        g: 255,
        b: 255,
        ..ResolvedSkinFrame::default()
    };
    let color_for = |index| {
        let draw_state = SkinDrawState {
            judge_ms: [Some(100), None, None],
            judge_index: [Some(index), None, None],
            ..SkinDrawState::default()
        };
        match document
            .text_render_item_with_draw_state(
                &text,
                frame,
                Some(&draw_state),
                &SkinTextState::default(),
            )
            .unwrap()
        {
            SkinRenderItem::Text { style, .. } => style.color,
            other => panic!("expected SkinRenderItem::Text, got {other:?}"),
        }
    };

    let pgreat = color_for(0);
    assert!(approx_eq(pgreat.r, 112.0 / 255.0));
    assert!(approx_eq(pgreat.g, 224.0 / 255.0));
    assert!(approx_eq(pgreat.b, 1.0));
    assert!(approx_eq(pgreat.a, 128.0 / 255.0));

    let good = color_for(2);
    assert!(approx_eq(good.r, 1.0));
    assert!(approx_eq(good.g, 224.0 / 255.0));
    assert!(approx_eq(good.b, 80.0 / 255.0));

    let poor = color_for(4);
    assert!(approx_eq(poor.r, 1.0));
    assert!(approx_eq(poor.g, 88.0 / 255.0));
    assert!(approx_eq(poor.b, 82.0 / 255.0));
}

#[test]
fn text_render_item_colors_bmz_judge_timing_region_by_side() {
    let document: SkinDocument =
        serde_json::from_value(serde_json::json!({ "w": 1920, "h": 1080 })).unwrap();
    let text = SkinTextDef {
        id: "judge_timing".to_string(),
        judge_timing_region: Some(0),
        judge_timing_color: true,
        ..SkinTextDef::default()
    };
    let frame = ResolvedSkinFrame {
        w: 100,
        h: 24,
        a: 128,
        r: 255,
        g: 255,
        b: 255,
        ..ResolvedSkinFrame::default()
    };
    let color_for = |sign| {
        let draw_state = SkinDrawState {
            judge_ms: [Some(100), None, None],
            judge_timing_sign: [Some(sign), None, None],
            ..SkinDrawState::default()
        };
        match document
            .text_render_item_with_draw_state(
                &text,
                frame,
                Some(&draw_state),
                &SkinTextState::default(),
            )
            .unwrap()
        {
            SkinRenderItem::Text { style, .. } => style.color,
            other => panic!("expected SkinRenderItem::Text, got {other:?}"),
        }
    };

    let fast = color_for(1);
    assert!(approx_eq(fast.r, 72.0 / 255.0));
    assert!(approx_eq(fast.g, 176.0 / 255.0));
    assert!(approx_eq(fast.b, 1.0));
    assert!(approx_eq(fast.a, 128.0 / 255.0));

    let slow = color_for(-1);
    assert!(approx_eq(slow.r, 1.0));
    assert!(approx_eq(slow.g, 88.0 / 255.0));
    assert!(approx_eq(slow.b, 82.0 / 255.0));
}

#[test]
fn note_lane_area_resolves_flat_frame_dst_after_expansion() {
    // load_beatoraja_json が expand_json_skin_value で条件ブロックを展開すると
    // note.dst はレーン順の Frame エントリ列になる。全レーンが正しく解決されること。
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 1280, "h": 720,
                "note": {
                    "dst": [
                        {"x": 90, "y": 140, "w": 50, "h": 580},
                        {"x": 140, "y": 140, "w": 40, "h": 580},
                        {"x": 180, "y": 140, "w": 50, "h": 580},
                        {"x": 230, "y": 140, "w": 40, "h": 580},
                        {"x": 270, "y": 140, "w": 50, "h": 580},
                        {"x": 320, "y": 140, "w": 40, "h": 580},
                        {"x": 360, "y": 140, "w": 50, "h": 580},
                        {"x": 20, "y": 140, "w": 70, "h": 580}
                    ]
                }
            }
            "#,
    )
    .unwrap();

    let enabled: Vec<i32> = vec![];
    // Key1 is index 0 → first Frame
    let area = document.note_lane_area(Lane::Key1, KeyMode::K7, &enabled).unwrap();
    assert!(approx_eq(area.x, 90.0 / 1280.0));
    assert!(approx_eq(area.y, 0.0));
    assert!(approx_eq(area.width, 50.0 / 1280.0));
    assert!(approx_eq(area.height, 580.0 / 720.0));
    // Key2 is index 1 → second Frame
    let area2 = document.note_lane_area(Lane::Key2, KeyMode::K7, &enabled).unwrap();
    assert!(approx_eq(area2.x, 140.0 / 1280.0));
    assert!(approx_eq(area2.width, 40.0 / 1280.0));
    // Scratch is index 7 → eighth Frame
    let scratch = document.note_lane_area(Lane::Scratch, KeyMode::K7, &enabled).unwrap();
    assert!(approx_eq(scratch.x, 20.0 / 1280.0));
    assert!(approx_eq(scratch.width, 70.0 / 1280.0));
}

#[test]
fn note_lane_area_resolves_conditional_dst_for_enabled_option() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 1280, "h": 720,
                "note": {
                    "dst": [
                        {
                            "if": [920],
                            "values": [
                                {"x": 90, "y": 140, "w": 50, "h": 580},
                                {"x": 140, "y": 140, "w": 40, "h": 580},
                                {"x": 180, "y": 140, "w": 50, "h": 580},
                                {"x": 230, "y": 140, "w": 40, "h": 580},
                                {"x": 270, "y": 140, "w": 50, "h": 580},
                                {"x": 320, "y": 140, "w": 40, "h": 580},
                                {"x": 360, "y": 140, "w": 50, "h": 580},
                                {"x": 20, "y": 140, "w": 70, "h": 580}
                            ]
                        }
                    ]
                }
            }
            "#,
    )
    .unwrap();

    let enabled = vec![920];
    // Key1 is index 0
    let area = document.note_lane_area(Lane::Key1, KeyMode::K7, &enabled).unwrap();
    assert!(approx_eq(area.x, 90.0 / 1280.0));
    assert!(approx_eq(area.y, 0.0));
    assert!(approx_eq(area.width, 50.0 / 1280.0));
    assert!(approx_eq(area.height, 580.0 / 720.0));

    // Scratch is index 7
    let scratch_area = document.note_lane_area(Lane::Scratch, KeyMode::K7, &enabled).unwrap();
    assert!(approx_eq(scratch_area.x, 20.0 / 1280.0));
    assert!(approx_eq(scratch_area.width, 70.0 / 1280.0));

    // Without the required option, returns None
    assert!(document.note_lane_area(Lane::Key1, KeyMode::K7, &[]).is_none());
}

#[test]
fn beatoraja_note_index_maps_6k_lanes_without_scratch() {
    assert_eq!(beatoraja_note_index(Lane::Key1, KeyMode::K6), 0);
    assert_eq!(beatoraja_note_index(Lane::Key2, KeyMode::K6), 1);
    assert_eq!(beatoraja_note_index(Lane::Key3, KeyMode::K6), 2);
    assert_eq!(beatoraja_note_index(Lane::Key4, KeyMode::K6), 3);
    assert_eq!(beatoraja_note_index(Lane::Key5, KeyMode::K6), 4);
    assert_eq!(beatoraja_note_index(Lane::Key6, KeyMode::K6), 5);
    assert_eq!(beatoraja_note_index(Lane::Scratch, KeyMode::K6), 5);
}
