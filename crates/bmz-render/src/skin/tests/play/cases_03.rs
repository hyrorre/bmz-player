use super::*;

#[test]
fn hidden_cover_clips_at_disappear_line() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "cover.png" }],
                "hiddenCover": [
                    { "id": "hidden-cover", "src": 12, "x": 0, "y": 0, "w": 390, "h": 580, "disapearLine": 140 }
                ],
                "destination": [
                    { "id": "hidden-cover", "dst": [{ "x": 20, "y": -440, "w": 390, "h": 580 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "12".to_string(),
        SkinDocumentTexture {
            source_id: "12".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 390.0, height: 580.0 },
        },
    )]);

    let flush = document.static_image_render_items(
        &sources,
        &SkinDrawState { hidden_cover: 1.0, ..SkinDrawState::default() },
    );
    // beatoraja は上端 (skin y=140) が disappearLine と一致する場合も描画しない。
    assert!(flush.is_empty());

    let clipped = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            hidden_cover: 1.0,
            offset_hidden_cover_px: 300,
            ..SkinDrawState::default()
        },
    );
    let SkinRenderItem::Image { rect: clipped_rect, uv: clipped_uv, .. } = &clipped[0] else {
        panic!("expected image");
    };
    // offset で上げた分、判定線より下を切り、上側 300px だけ残す
    assert!(approx_eq(clipped_rect.y, 280.0 / 720.0));
    assert!(approx_eq(clipped_rect.height, 300.0 / 720.0));
    assert!(approx_eq(1.0 - clipped_uv.height, 280.0 / 580.0));
}

#[test]
fn lift_cover_hides_at_minimum_lift() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "lift.png" }],
                "image": [
                    { "id": "liftcover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723 }
                ],
                "hiddenCover": [
                    { "id": "hiddencover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357 }
                ],
                "destination": [
                    { "id": "liftcover", "offset": 3, "dst": [{ "x": 20, "y": -366, "w": 431, "h": 723 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "12".to_string(),
        SkinDocumentTexture {
            source_id: "12".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 431.0, height: 723.0 },
        },
    )]);

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() },
    );
    assert!(items.is_empty());
}

#[test]
fn lift_cover_clips_at_disappear_line() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "lift.png" }],
                "image": [
                    { "id": "liftcover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723 }
                ],
                "hiddenCover": [
                    { "id": "hiddencover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357 }
                ],
                "destination": [
                    { "id": "liftcover", "offset": 3, "dst": [{ "x": 20, "y": -366, "w": 431, "h": 723 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "12".to_string(),
        SkinDocumentTexture {
            source_id: "12".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 431.0, height: 723.0 },
        },
    )]);

    let clipped = document.static_image_render_items(
        &sources,
        &SkinDrawState { offset_lift_px: 200, ..SkinDrawState::default() },
    );
    let SkinRenderItem::Image { rect, uv, .. } = &clipped[0] else {
        panic!("expected clipped lift cover image");
    };
    assert!(approx_eq(rect.height, 200.0 / 720.0));
    assert!(approx_eq(uv.height, 200.0 / 723.0));
}

#[test]
fn lift_hidden_cover_clips_with_its_own_disappear_line() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "lift.png" }],
                "hiddenCover": [
                    { "id": "lr2-liftcover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357, "isDisapearLineLinkLift": false }
                ],
                "destination": [
                    { "id": "lr2-liftcover", "offset": 3, "dst": [{ "x": 20, "y": -366, "w": 431, "h": 723 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "12".to_string(),
        SkinDocumentTexture {
            source_id: "12".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 431.0, height: 723.0 },
        },
    )]);

    let no_lift = document.static_image_render_items(
        &sources,
        &SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() },
    );
    assert!(no_lift.is_empty());

    let lifted = document.static_image_render_items(
        &sources,
        &SkinDrawState { offset_lift_px: 200, ..SkinDrawState::default() },
    );
    let SkinRenderItem::Image { rect, uv, tint, .. } = &lifted[0] else {
        panic!("expected clipped lift hidden cover image");
    };
    assert!(approx_eq(rect.height, 200.0 / 720.0));
    assert!(approx_eq(uv.height, 200.0 / 723.0));
    assert!(tint.a > 0.5);
}

#[test]
fn skin_state_number_maps_play_value_refs() {
    let state = SkinDrawState {
        combo: 12,
        max_combo: 45,
        ex_score: 167,
        total_notes: 100,
        past_notes: 100,
        judge_counts: DisplayJudgeCounts {
            pgreat: 30,
            great: 20,
            good: 10,
            bad: 4,
            poor: 3,
            empty_poor: 2,
        },
        gauge: 78.6,
        fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 10,
            slow_pgreat: 11,
            fast_great: 12,
            slow_great: 13,
            fast_good: 14,
            slow_good: 15,
            fast_bad: 16,
            slow_bad: 17,
            fast_poor: 18,
            slow_poor: 19,
            fast_empty_poor: 20,
            slow_empty_poor: 21,
        }),
        best_ex_score: Some(123),
        target_ex_score: Some(145),
        judge_rank: Some(1),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(71, &state), Some(167));
    assert_eq!(skin_state_number(72, &state), Some(200));
    assert_eq!(skin_state_number(74, &state), Some(100));
    assert_eq!(skin_state_number(75, &state), Some(45));
    assert_eq!(skin_state_number(105, &state), Some(45));
    assert_eq!(skin_state_number(76, &state), Some(7));
    assert_eq!(skin_state_number(102, &state), Some(83));
    assert_eq!(skin_state_number(103, &state), Some(50));
    assert_eq!(skin_state_number(104, &state), Some(12));
    assert_eq!(skin_state_number(107, &state), Some(78));
    assert_eq!(skin_state_number(407, &state), Some(6));
    assert_eq!(skin_state_number(110, &state), Some(30));
    assert_eq!(skin_state_number(111, &state), Some(20));
    assert_eq!(skin_state_number(112, &state), Some(10));
    assert_eq!(skin_state_number(113, &state), Some(4));
    assert_eq!(skin_state_number(114, &state), Some(3));
    assert_eq!(skin_state_number(122, &state), Some(72));
    assert_eq!(skin_state_number(123, &state), Some(50));
    assert_eq!(skin_state_number(183, &state), Some(61));
    assert_eq!(skin_state_number(184, &state), Some(50));
    assert_eq!(skin_state_number(400, &state), Some(1));
    assert_eq!(skin_state_number(420, &state), Some(2));
    assert_eq!(skin_state_number(423, &state), Some(80));
    assert_eq!(skin_state_number(424, &state), Some(85));
    assert_eq!(skin_state_number(425, &state), Some(7));
    assert_eq!(skin_state_number(426, &state), Some(5));
    assert_eq!(skin_state_number(427, &state), Some(9));
    assert!(test_skin_op(181, &[], &state));
    assert!(!test_skin_op(182, &[], &state));
}

#[test]
fn autoplay_pgreat_fast_slow_refs_are_neutral() {
    let state = SkinDrawState {
        autoplay: true,
        judge_counts: DisplayJudgeCounts { pgreat: 30, ..DisplayJudgeCounts::default() },
        fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 10,
            slow_pgreat: 11,
            fast_great: 12,
            slow_great: 13,
            ..crate::snapshot::FastSlowJudgeCounts::default()
        }),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(410, &state), Some(0));
    assert_eq!(skin_state_number(411, &state), Some(0));
    assert_eq!(skin_state_number(412, &state), Some(12));
    assert_eq!(skin_state_number(413, &state), Some(13));
    assert!(eval_skin_draw_condition(
        "number(110) > number(410) and number(110) > number(411)",
        &state
    ));
}

#[test]
fn lr2_fast_slow_number_bridge_maps_latest_non_pgreat_side() {
    let fast = SkinDrawState {
        judge_index: [Some(1), None, None],
        judge_timing_sign: [Some(1), None, None],
        ..SkinDrawState::default()
    };
    let slow = SkinDrawState {
        judge_index: [Some(4), None, None],
        judge_timing_sign: [Some(-1), None, None],
        ..SkinDrawState::default()
    };
    let pgreat = SkinDrawState {
        judge_index: [Some(0), None, None],
        judge_timing_sign: [Some(1), None, None],
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(SKIN_REF_BMZ_LR2_FAST_SLOW_1P, &fast), Some(1));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LR2_FAST_SLOW_1P, &slow), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LR2_FAST_SLOW_1P, &pgreat), Some(0));
    assert_eq!(
        skin_state_number(SKIN_REF_BMZ_LR2_FAST_SLOW_1P, &SkinDrawState::default()),
        Some(0)
    );
}

#[test]
fn lr2_battle_gauge_bridges_use_each_active_gauge_without_changing_standard_refs() {
    let mut state = SkinDrawState {
        gauge_type: 3,
        opponent_gauge_type: Some(1),
        gauge: 72.9,
        opponent_gauge: Some(38.7),
        select_gauge_index: 2,
        select_target_index: 4,
        ..Default::default()
    };
    assert_eq!(skin_image_ref_number(SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P, &state), Some(1));
    assert_eq!(skin_image_ref_number(SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P, &state), Some(3));
    assert_eq!(skin_state_number(107, &state), Some(72));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LR2_GAUGE_2P, &state), Some(38));
    assert_eq!(skin_image_ref_number(40, &state), Some(3));
    assert_eq!(skin_image_ref_number(41, &state), Some(4));
    for (gauge, row) in [(0, 3), (1, 3), (2, 0), (3, 1), (4, 1), (5, 2), (6, 1), (7, 1), (8, 1)] {
        state.gauge_type = gauge;
        assert_eq!(skin_image_ref_number(SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P, &state), Some(row));
    }
    state.opponent_gauge = Some(0.0);
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LR2_GAUGE_2P, &state), Some(0));
    state.opponent_gauge = None;
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LR2_GAUGE_2P, &state), None);
}

#[test]
fn lr2_hispeed_bridge_scales_both_sides_without_changing_decimal_refs() {
    for (hispeed, expected) in [(0.01, 1), (2.5, 250), (9.99, 999), (10.0, 1000), (20.0, 2000)] {
        let state = SkinDrawState { hispeed, ..Default::default() };
        assert_eq!(skin_state_number(SKIN_REF_BMZ_LR2_HISPEED, &state), Some(expected));
    }
    let state = SkinDrawState { hispeed: 2.5, ..Default::default() };
    assert_eq!(skin_state_number(310, &state), Some(2));
    assert_eq!(skin_state_number(311, &state), Some(50));
}

#[test]
fn display_number_digits_uses_absolute_value_like_beatoraja_skin_number() {
    assert_eq!(display_number_digits(-34, 2, NumberPadding::Zero), vec![3, 4]);
    assert_eq!(display_number_digits(-34, 4, NumberPadding::Blank), vec![10, 10, 3, 4]);
}

#[test]
fn skin_state_event_index_maps_lane_judge_values() {
    let mut lane_judge = [None; LANE_COUNT];
    lane_judge[Lane::Key1.index()] = Some(0);
    lane_judge[Lane::Key2.index()] = Some(1);
    lane_judge[Lane::Key3.index()] = Some(2);
    lane_judge[Lane::Key4.index()] = Some(3);
    lane_judge[Lane::Key5.index()] = Some(4);
    lane_judge[Lane::Key6.index()] = Some(5);
    lane_judge[Lane::Key8.index()] = Some(0);
    let state = SkinDrawState { lane_judge, ..SkinDrawState::default() };

    assert_eq!(skin_state_event_index(501, &state), 1);
    assert_eq!(skin_state_event_index(502, &state), 2);
    assert_eq!(skin_state_event_index(503, &state), 4);
    assert_eq!(skin_state_event_index(504, &state), 6);
    assert_eq!(skin_state_event_index(505, &state), 7);
    assert_eq!(skin_state_event_index(506, &state), 8);
    assert_eq!(skin_state_event_index(507, &state), 0);
    assert_eq!(skin_state_event_index(511, &state), 1);
}

#[test]
fn arrange_refs_use_each_sides_arrange_on_play_screen() {
    let state = SkinDrawState {
        select_arrange_index: 2,
        select_arrange_2p_index: 1,
        select_extended_arrange_index: 11,
        select_extended_arrange_2p_index: 10,
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_imageset_index(42, &state), Some(2));
    assert_eq!(skin_state_imageset_index(43, &state), Some(1));
    assert_eq!(skin_state_number(42, &state), Some(2));
    assert_eq!(skin_state_number(43, &state), Some(1));
    assert_eq!(skin_state_event_index(42, &state), 2);
    assert_eq!(skin_state_event_index(43, &state), 1);
    assert_eq!(skin_state_imageset_index(344, &state), Some(11));
    assert_eq!(skin_state_imageset_index(345, &state), Some(10));
    assert_eq!(skin_state_number(344, &state), Some(11));
    assert_eq!(skin_state_number(345, &state), Some(10));
    assert_eq!(skin_state_event_index(344, &state), 11);
    assert_eq!(skin_state_event_index(345, &state), 10);
}

#[test]
fn random_lane_refs_map_beatoraja_pattern_numbers() {
    let mut pattern = (0..LANE_COUNT as u8).collect::<Vec<_>>();
    pattern[Lane::Key1.index()] = Lane::Key7.index() as u8;
    pattern[Lane::Key2.index()] = Lane::Key3.index() as u8;
    pattern[Lane::Key3.index()] = Lane::Key1.index() as u8;

    let refs = fixed_random_lane_refs(&pattern, KeyMode::K7, "RANDOM", "NORMAL");
    let state = SkinDrawState {
        result_arrange_index: 2,
        random_lane_refs: refs,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_event_index(42, &state), 2);
    assert_eq!(skin_state_imageset_index(450, &state), Some(7));
    assert_eq!(skin_state_imageset_index(451, &state), Some(3));
    assert_eq!(skin_state_imageset_index(452, &state), Some(1));
    assert_eq!(skin_state_imageset_index(457, &state), Some(0));
    assert_eq!(skin_state_imageset_index(459, &state), Some(0));
    assert_eq!(skin_state_event_index(450, &state), 7);
    assert_eq!(skin_state_event_index(451, &state), 3);
    assert_eq!(skin_state_event_index(452, &state), 1);
    assert_eq!(skin_state_event_index(457, &state), 0);
    assert_eq!(skin_state_event_index(459, &state), 0);
    assert_eq!(skin_state_number(450, &state), Some(7));
    assert_eq!(skin_state_number(466, &state), Some(0));
    assert_eq!(skin_state_number(467, &state), None);
    assert_eq!(skin_state_number(468, &state), None);
    assert_eq!(skin_state_event_index(467, &state), 0);
    assert_eq!(skin_state_event_index(468, &state), 0);
}

#[test]
fn random_lane_refs_hide_for_non_fixed_random() {
    let refs = fixed_random_lane_refs(
        &(0..LANE_COUNT as u8).collect::<Vec<_>>(),
        KeyMode::K7,
        "S-RANDOM",
        "NORMAL",
    );
    let state = SkinDrawState {
        result_arrange_index: 4,
        random_lane_refs: refs,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_event_index(42, &state), 4);
    assert_eq!(skin_state_imageset_index(450, &state), Some(0));
}

#[test]
fn random_lane_refs_use_each_sides_arrange() {
    let mut pattern = (0..LANE_COUNT as u8).collect::<Vec<_>>();
    pattern[Lane::Key1.index()] = Lane::Key7.index() as u8;
    pattern[Lane::Key8.index()] = Lane::Key10.index() as u8;
    let refs = fixed_random_lane_refs(&pattern, KeyMode::K14, "NORMAL", "RANDOM");
    let p2_random = SkinDrawState {
        result_arrange_index: 0,
        result_arrange_2p_index: 2,
        random_lane_refs: refs,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_imageset_index(450, &p2_random), Some(0));
    assert_eq!(skin_state_imageset_index(460, &p2_random), Some(3));

    let p1_random = SkinDrawState {
        result_arrange_index: 2,
        result_arrange_2p_index: 0,
        random_lane_refs: fixed_random_lane_refs(&pattern, KeyMode::K14, "RANDOM", "NORMAL"),
        ..p2_random
    };
    assert_eq!(skin_state_imageset_index(450, &p1_random), Some(7));
    assert_eq!(skin_state_imageset_index(460, &p1_random), Some(0));
}

#[test]
fn play_target_image_index_matches_beatoraja_default_target_list() {
    assert_eq!(play_target_image_index("RANK_A"), 1);
    assert_eq!(play_target_image_index("RANK_AA-"), 3);
    assert_eq!(play_target_image_index("RANK_AA"), 4);
    assert_eq!(play_target_image_index("RANK_AAA-"), 6);
    assert_eq!(play_target_image_index("RANK_AAA"), 7);
    assert_eq!(play_target_image_index("RANK_MAX-"), 9);
    assert_eq!(play_target_image_index("MAX"), 10);
    assert_eq!(play_target_image_index("IR_TOP"), 0);
}

#[test]
fn bundled_beatoraja_default_play7_json_loads_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/beatoraja/skin/default/play7.json");
    if !path.is_file() {
        return;
    }

    let document = SkinDocument::load_beatoraja_json(&path).unwrap();

    assert_eq!(document.name, "beatoraja default");
    assert_eq!(document.w, 1280);
    assert_eq!(document.h, 720);
    assert!(document.source_map().contains_key("7"));
    assert!(document.image_map().contains_key("note-w"));
    assert_eq!(document.note.as_ref().unwrap().id, "notes");
    assert!(!document.destination.is_empty());
}

#[test]
fn local_ecfn_converted_play7_json_loads_when_available() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7-1p.json");
    if !path.is_file() {
        return;
    }

    let document = SkinDocument::load_beatoraja_json(&path).unwrap();

    assert!(!document.destination.is_empty());
}

#[test]
fn stretch_applied_to_judge_destination() {
    // stretch=9 (resize_about_center) should resize the image to its source dimensions
    // centered on the destination rect.
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "effect.png" }],
                "image": [{ "id": "judge-pg", "src": 1, "x": 0, "y": 0, "w": 50, "h": 20 }],
                "judge": [{
                    "id": "judge-1p",
                    "index": 0,
                    "images": [
                        { "id": "judge-pg", "stretch": 9, "dst": [
                            { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100 }
                        ]}
                    ]
                }]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(5),
            source_size: SkinImageSize { width: 50.0, height: 20.0 },
        },
    )]);

    let items = document.judge_render_items("PGREAT", 0, 0, &sources).unwrap();

    // stretch=9: resize_about_center places the 50x20 source centered in 100x100 destination.
    // In normalized coords (canvas 100x100):
    //   dest rect: x=0/100=0, y=0/100=0, w=100/100=1, h=100/100=1
    //   source size: 50x20 pixels → w=50/100=0.5, h=20/100=0.2
    //   centered: x = 0 + (1 - 0.5)*0.5 = 0.25, y = 0 + (1 - 0.2)*0.5 = 0.4
    assert!(matches!(
        items[0],
        SkinRenderItem::Image {
            rect: Rect { x, y, width, height },
            ..
        } if approx_eq(x, 0.25)
            && approx_eq(y, 0.4)
            && approx_eq(width, 0.5)
            && approx_eq(height, 0.2)
    ));
}
