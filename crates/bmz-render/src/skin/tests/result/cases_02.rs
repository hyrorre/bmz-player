use super::*;

#[test]
fn skin_ops_map_gauge_ranges_and_result_judge_existence() {
    let play = SkinDrawState {
        play_screen: true,
        gauge: 45.0,
        gauge_max: 100.0,
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(234, &[], &play));
    assert!(!test_skin_op(233, &[], &play));
    assert!(test_skin_op(240, &[], &SkinDrawState { gauge: 100.0, ..play.clone() }));
    assert!(test_skin_op(
        234,
        &[],
        &SkinDrawState { ready_timer_ms: None, play_timer_ms: None, ..play.clone() }
    ));
    assert!(!test_skin_op(234, &[], &SkinDrawState { play_screen: false, ..play.clone() }));

    let result = SkinDrawState {
        result_failed: Some(false),
        judge_counts: DisplayJudgeCounts {
            pgreat: 1,
            good: 2,
            poor: 3,
            ..DisplayJudgeCounts::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(2241, &[], &result));
    assert!(!test_skin_op(2242, &[], &result));
    assert!(test_skin_op(2243, &[], &result));
    assert!(!test_skin_op(2244, &[], &result));
    assert!(test_skin_op(2245, &[], &result));
    assert!(!test_skin_op(2246, &[], &result));
    assert!(!test_skin_op(2241, &[], &SkinDrawState::default()));
}

#[test]
fn skin_state_number_maps_result_chart_detail_refs() {
    let state = SkinDrawState {
        result_failed: Some(false),
        now_bpm: 128.0,
        min_bpm: 100.0,
        max_bpm: 180.0,
        main_bpm: 150.0,
        total_duration_ms: 200_000,
        duration_green_ms: Some(120_000),
        select_chart_total_gauge: 200.0,
        judge_rank: Some(2),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(160, &state), Some(128));
    assert_eq!(skin_state_number(91, &state), Some(100));
    assert_eq!(skin_state_number(90, &state), Some(180));
    assert_eq!(skin_state_number(92, &state), Some(150));
    assert_eq!(skin_state_number(312, &state), Some(200_000));
    assert_eq!(skin_state_number(313, &state), Some(120_000));
    assert_eq!(skin_state_number(368, &state), Some(200));
    assert_eq!(skin_state_number(400, &state), Some(2));
}

#[test]
fn result_skin_state_maps_arrange_ops() {
    let cases = [
        (0, 126),
        (1, 127),
        (2, 128),
        (3, 1128),
        (4, 129),
        (5, 1129),
        (6, 130),
        (7, 131),
        (8, 1130),
        (9, 1131),
    ];
    for (index, op) in cases {
        let state = SkinDrawState {
            result_failed: Some(false),
            result_arrange_index: index,
            ..SkinDrawState::default()
        };
        assert!(test_skin_op(op, &[], &state), "op {op} should match index {index}");
        for (_, other_op) in cases {
            if other_op != op {
                assert!(
                    !test_skin_op(other_op, &[], &state),
                    "op {other_op} should not match index {index}"
                );
            }
        }
    }

    assert!(!test_skin_op(
        1131,
        &[],
        &SkinDrawState { result_arrange_index: 9, ..SkinDrawState::default() }
    ));
}

#[test]
fn result_timers_follow_result_state() {
    let inactive = SkinDrawState::default();
    assert_eq!(skin_timer_elapsed_ms(Some(150), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(151), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(152), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(172), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(173), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(174), &inactive), None);

    let active = SkinDrawState {
        result_graph_begin_ms: Some(120),
        result_graph_end_ms: Some(120),
        result_update_score_ms: Some(40),
        ir_ranking: crate::scene::ResultIrSnapshot {
            connect_begin_ms: Some(180),
            connect_success_ms: Some(90),
            connect_fail_ms: Some(30),
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert_eq!(skin_timer_elapsed_ms(Some(150), &active), Some(120));
    assert_eq!(skin_timer_elapsed_ms(Some(151), &active), Some(120));
    assert_eq!(skin_timer_elapsed_ms(Some(152), &active), Some(40));
    assert_eq!(skin_timer_elapsed_ms(Some(172), &active), Some(180));
    assert_eq!(skin_timer_elapsed_ms(Some(173), &active), Some(90));
    assert_eq!(skin_timer_elapsed_ms(Some(174), &active), Some(30));
}

#[test]
fn ir_skin_properties_map_loaded_ranking() {
    let loaded = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Loaded,
            rank: Some(3),
            total_player: Some(42),
            clear_rate: Some(85),
            previous_rank: Some(7),
            entries: [
                crate::scene::ResultIrRankingEntrySnapshot {
                    rank: Some(1),
                    ex_score: Some(2000),
                    clear_index: Some(8),
                    player_name: crate::scene::ResultIrRankingName::from_display_name("Alice"),
                },
                crate::scene::ResultIrRankingEntrySnapshot {
                    rank: Some(2),
                    ex_score: Some(1900),
                    clear_index: Some(6),
                    player_name: crate::scene::ResultIrRankingName::from_display_name("Bob"),
                },
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
        },
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(179, &loaded), Some(3));
    assert_eq!(skin_state_number(180, &loaded), Some(42));
    assert_eq!(skin_state_number(200, &loaded), Some(42));
    assert_eq!(skin_state_number(181, &loaded), Some(85));
    assert_eq!(skin_state_number(182, &loaded), Some(7));
    assert_eq!(skin_state_number(226, &loaded), None);
    assert_eq!(skin_state_number(227, &loaded), Some(85));
    assert_eq!(skin_state_number(241, &loaded), None);
    assert_eq!(skin_state_number(380, &loaded), Some(2000));
    assert_eq!(skin_state_number(381, &loaded), Some(1900));
    assert_eq!(skin_state_number(390, &loaded), Some(1));
    assert_eq!(skin_state_number(391, &loaded), Some(2));
    assert_eq!(skin_image_index_number(390, &loaded), Some(8));
    assert_eq!(skin_image_index_number(391, &loaded), Some(6));
    assert_eq!(skin_state_number(382, &loaded), None);
    assert!(!test_skin_op(601, &[], &loaded));
    assert!(test_skin_op(602, &[], &loaded));
    assert!(!test_skin_op(603, &[], &loaded));
    assert!(!test_skin_op(604, &[], &loaded));

    let loading = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Loading,
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(601, &[], &loading));
    assert!(!test_skin_op(602, &[], &loading));
    assert!(!test_skin_op(606, &[], &loading));

    let waiting = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Waiting,
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(606, &[], &waiting));
    assert!(!test_skin_op(601, &[], &waiting));

    let failed = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Failed,
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(604, &[], &failed));
    assert!(test_skin_op(608, &[], &failed));

    let no_player = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Loaded,
            total_player: Some(0),
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(603, &[], &no_player));
}

#[test]
fn bmz_result_ir_scope_refs_and_options_follow_snapshot() {
    let state = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            scope: crate::scene::ResultIrScope::Rival,
            global_scope_supported: true,
            rival_scope_supported: true,
            total_player: Some(7),
            ..Default::default()
        },
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(SKIN_REF_BMZ_RESULT_IR_SCOPE, &state), Some(1));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_RESULT_IR_SCOPE_TOTAL, &state), Some(7));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_RESULT_IR_SCOPE_GLOBAL, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_RESULT_IR_SCOPE_RIVAL, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_RESULT_IR_SCOPE_GLOBAL_SUPPORTED, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_RESULT_IR_SCOPE_RIVAL_SUPPORTED, &[], &state));

    let text_state = SkinTextState::default();
    assert_eq!(
        skin_main_state_text(SKIN_REF_BMZ_RESULT_IR_SCOPE, Some(&state), &text_state),
        "RIVAL"
    );
}

#[test]
fn wmii_ir_score_graph_and_user_highlight_use_ranking_snapshot() {
    let state = SkinDrawState {
        total_notes: 100,
        ir_ranking: crate::scene::ResultIrSnapshot {
            online: true,
            state: crate::scene::ResultIrState::Loaded,
            user_name: crate::scene::ResultIrRankingName::from_display_name("Alice"),
            entries: [
                crate::scene::ResultIrRankingEntrySnapshot {
                    rank: Some(1),
                    ex_score: Some(155),
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
        },
        ..SkinDrawState::default()
    };

    assert_eq!(skin_builtin_value_f32("bmz:ir_score_rate:1", &state), Some(0.775));
    assert_eq!(skin_builtin_value_i64("bmz:ir_score_rate_integer:1", &state), Some(77));
    assert_eq!(skin_builtin_value_i64("bmz:ir_score_rate_fraction:1", &state), Some(50));
    assert!(eval_skin_draw_condition("ir_score_rate_band(1,6,7)", &state));
    assert!(!eval_skin_draw_condition("ir_score_rate_band(1,7,8)", &state));
    assert!(eval_skin_draw_condition("option(51) and ir_score_rate_range(1,666,777)", &state));
    assert!(!eval_skin_draw_condition("option(51) and ir_score_rate_range(1,777,888)", &state));
    assert!(eval_skin_draw_condition("ir_ranking_user(1)", &state));
    assert!(!eval_skin_draw_condition("ir_ranking_user(2)", &state));
}

#[test]
fn wmii_ir_score_diff_uses_best_of_old_and_current_score() {
    let mut entries =
        std::array::from_fn(|_| crate::scene::ResultIrRankingEntrySnapshot::default());
    entries[0] = crate::scene::ResultIrRankingEntrySnapshot {
        rank: Some(1),
        ex_score: Some(2293),
        clear_index: Some(9),
        player_name: crate::scene::ResultIrRankingName::from_display_name("Alice"),
    };
    let mut state = SkinDrawState {
        ex_score: 2284,
        total_notes: 1155,
        past_notes: 1155,
        previous_best_ex_score: Some(2293),
        result_failed: Some(false),
        ir_ranking: crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Loaded,
            entries,
            ..Default::default()
        },
        ..SkinDrawState::default()
    };

    assert_eq!(skin_builtin_value_i64("bmz:ir_score_diff:1", &state), Some(0));

    state.previous_best_ex_score = Some(2200);
    assert_eq!(skin_builtin_value_i64("bmz:ir_score_diff:1", &state), Some(-9));

    state.previous_best_ex_score = Some(2293);
    state.ir_ranking.entries[0].ex_score = Some(2300);
    assert_eq!(skin_builtin_value_i64("bmz:ir_score_diff:1", &state), Some(-7));
}

#[test]
fn ir_skin_properties_use_offline_defaults() {
    let state = SkinDrawState::default();

    assert!(test_skin_op(50, &[], &state));
    assert!(!test_skin_op(51, &[], &state));
    for op in 601..=608 {
        assert!(!test_skin_op(op, &[], &state), "IR option {op} should be false offline");
    }

    for ref_id in [179, 180, 181, 182, 200, 201, 202, 220, 226, 227, 241, 242, 380, 390] {
        assert_eq!(skin_state_number(ref_id, &state), None, "IR number {ref_id}");
    }
}

#[test]
fn ir_online_property_is_independent_from_ranking_state() {
    let state = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            online: true,
            state: crate::scene::ResultIrState::Offline,
            ..Default::default()
        },
        ..SkinDrawState::default()
    };

    assert!(!test_skin_op(50, &[], &state));
    assert!(test_skin_op(51, &[], &state));
}

#[test]
fn result_gauge_type_image_index_uses_applied_gauge() {
    let state = SkinDrawState {
        select_screen: false,
        select_gauge_index: bmz_core::clear::GaugeType::Normal as usize,
        gauge_type: bmz_core::clear::GaugeType::ExHard as i32,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };

    assert_eq!(
        skin_state_imageset_index(40, &state),
        Some(bmz_core::clear::GaugeType::ExHard as usize)
    );
    assert_eq!(skin_image_ref_number(40, &state), Some(bmz_core::clear::GaugeType::ExHard as i64));
}

#[test]
fn arrange_ref_uses_result_arrange_on_result_screen() {
    let state = SkinDrawState {
        select_arrange_index: 2,
        select_arrange_2p_index: 3,
        result_arrange_index: 8,
        result_arrange_2p_index: 1,
        result_extended_arrange_index: 11,
        result_extended_arrange_2p_index: 10,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_imageset_index(42, &state), Some(8));
    assert_eq!(skin_state_imageset_index(43, &state), Some(1));
    assert_eq!(skin_image_ref_number(42, &state), Some(8));
    assert_eq!(skin_image_ref_number(43, &state), Some(1));
    assert_eq!(skin_state_number(42, &state), Some(8));
    assert_eq!(skin_state_number(43, &state), Some(1));
    assert_eq!(skin_state_event_index(42, &state), 8);
    assert_eq!(skin_state_event_index(43, &state), 1);
    assert_eq!(skin_state_imageset_index(344, &state), Some(11));
    assert_eq!(skin_state_imageset_index(345, &state), Some(10));
    assert_eq!(skin_state_number(344, &state), Some(11));
    assert_eq!(skin_state_number(345, &state), Some(10));
    assert_eq!(skin_state_event_index(344, &state), 11);
    assert_eq!(skin_state_event_index(345, &state), 10);
}

#[test]
fn random_lane_refs_are_available_outside_result_screen() {
    let mut refs = [0; SKIN_RANDOM_LANE_REF_COUNT];
    refs[0] = 7;
    let state = SkinDrawState { random_lane_refs: refs, ..SkinDrawState::default() };

    assert_eq!(skin_state_imageset_index(450, &state), Some(7));
    assert_eq!(skin_state_event_index(450, &state), 7);
    assert_eq!(skin_state_number(450, &state), Some(7));
}

#[test]
fn result_judge_pie_segments_use_runtime_judge_counts() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 200, "h": 200,
                "source": [{ "id": "src", "path": "jud_detail.png" }],
                "image": [
                    { "id": "judge_graph", "src": "src", "x": 574, "y": 1, "w": 140, "h": 8 }
                ],
                "destination": [
                    { "id": "judge_graph", "dst": [{ "x": 41, "y": 241, "w": 140, "h": 8, "r": 8, "g": 179, "b": 239, "angle": 91 }] },
                    { "id": "judge_graph", "dst": [{ "x": 41, "y": 241, "w": 140, "h": 8, "r": 8, "g": 179, "b": 239, "angle": 100 }] },
                    { "id": "judge_graph", "dst": [{ "x": 41, "y": 241, "w": 140, "h": 8, "r": 8, "g": 179, "b": 239, "angle": 120 }] },
                    { "id": "judge_graph", "dst": [{ "x": 41, "y": 241, "w": 140, "h": 8, "r": 8, "g": 179, "b": 239, "angle": 150 }] },
                    { "id": "judge_graph", "dst": [{ "x": 41, "y": 241, "w": 140, "h": 8, "r": 8, "g": 179, "b": 239, "angle": 290 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("src", 800.0, 800.0);
    let state = SkinDrawState {
        result_failed: Some(false),
        judge_counts: DisplayJudgeCounts {
            pgreat: 70,
            great: 20,
            good: 5,
            bad: 3,
            poor: 2,
            empty_poor: 0,
        },
        ..SkinDrawState::default()
    };
    let items = document.static_image_render_items(&sources, &state);

    let segments = items
        .iter()
        .map(|item| match item {
            SkinRenderItem::RotatedImage { tint, angle_deg, .. } => (
                (
                    (tint.r * 255.0).round() as i32,
                    (tint.g * 255.0).round() as i32,
                    (tint.b * 255.0).round() as i32,
                ),
                *angle_deg as i32,
            ),
            _ => panic!("expected rotated judge pie segment"),
        })
        .collect::<Vec<_>>();
    let colors = segments.iter().map(|(color, _)| *color).collect::<Vec<_>>();
    assert_eq!(
        colors,
        vec![(217, 68, 35), (226, 135, 42), (240, 190, 15), (240, 239, 10), (8, 179, 239),]
    );
    let angles = segments.iter().map(|(_, angle)| *angle).collect::<Vec<_>>();
    assert_eq!(angles, vec![-91, -100, -120, -150, -290]);

    let mut cache = ResultRenderCache::default();
    let text = SkinTextState::default();
    for (pgreat, great, texture, width) in
        [(70, 20, 1, 800.0), (0, 90, 1, 800.0), (70, 20, 2, 1600.0)]
    {
        let mut state = state.clone();
        state.judge_counts.pgreat = pgreat;
        state.judge_counts.great = great;
        let mut sources = sources.clone();
        let source = sources.get_mut("src").unwrap();
        source.texture = SkinTextureId(texture);
        source.source_size.width = width;
        for elapsed in [0, 100, 1000] {
            state.elapsed_ms = elapsed;
            let actual = document.static_render_items_with_graphs_cached(
                &sources,
                &state,
                &text,
                SkinRuntimeGraphs::from_document(&document),
                Some(&mut cache),
            );
            assert_eq!(actual, document.static_image_render_items(&sources, &state));
        }
    }
}

#[test]
fn result_judge_pie_cache_keeps_animated_and_offset_segments_live() {
    let document: SkinDocument = serde_json::from_str(r#"{
        "w":200,"h":200,
        "image":[{"id":"judge_graph","src":"src","w":140,"h":8}],
        "destination":[
            {"id":"judge_graph","offset":30,"dst":[{"w":140,"h":8,"angle":120}]},
            {"id":"judge_graph","timer":150,"dst":[{"w":140,"h":8,"angle":150}]},
            {"id":"judge_graph","dst":[{"time":0,"w":140,"h":8,"angle":290,"a":0},{"time":100,"a":255}]}
        ]
    }"#).unwrap();
    let sources = mock_source("src", 800.0, 800.0);
    let mut cache = ResultRenderCache::default();
    for elapsed in [0, 25, 50, 100] {
        let mut state =
            SkinDrawState { elapsed_ms: elapsed, result_failed: Some(false), ..Default::default() };
        state.judge_counts.pgreat = 80;
        state.judge_counts.great = 20;
        state.result_graph_begin_ms = (elapsed > 25).then_some(elapsed - 25);
        state
            .skin_offsets
            .set(30, SkinOffsetValue { x: elapsed, a: -elapsed, ..Default::default() });
        let actual = document.static_render_items_with_graphs_cached(
            &sources,
            &state,
            &SkinTextState::default(),
            SkinRuntimeGraphs::from_document(&document),
            Some(&mut cache),
        );
        assert_eq!(actual, document.static_image_render_items(&sources, &state));
    }
}

#[test]
fn skin_image_index_number_result_favorite_ref_has_only_two_states() {
    let not_favorite =
        SkinDrawState { result_favorite_chart: Some(false), ..SkinDrawState::default() };
    assert_eq!(skin_image_index_number(90, &not_favorite), Some(0));

    let favorite = SkinDrawState { result_favorite_chart: Some(true), ..SkinDrawState::default() };
    assert_eq!(skin_image_index_number(90, &favorite), Some(1));
}

#[test]
fn wmii_nearest_rank_diff_value_uses_absolute_runtime_difference() {
    let state = SkinDrawState { ex_score: 155, total_notes: 100, ..Default::default() };
    let value =
        SkinValueDef { value_expr: "bmz:nearest_rank_diff_abs".to_string(), ..Default::default() };
    assert_eq!(skin_value_number(&value, &state), Some(1));
}

#[test]
fn wmii_next_rank_diff_value_uses_forward_lua_boundary() {
    let state = SkinDrawState { ex_score: 160, total_notes: 100, ..Default::default() };
    let value =
        SkinValueDef { value_expr: "bmz:wmii_next_rank_diff".to_string(), ..Default::default() };
    assert_eq!(skin_value_number(&value, &state), Some(18));
}

#[test]
fn wmii_next_rank_diff_without_max_minus_targets_max_directly() {
    let state = SkinDrawState { ex_score: 188, total_notes: 100, ..Default::default() };
    let value = SkinValueDef {
        value_expr: "bmz:wmii_next_rank_diff_no_max_minus".to_string(),
        ..Default::default()
    };
    assert_eq!(skin_value_number(&value, &state), Some(12));
}
