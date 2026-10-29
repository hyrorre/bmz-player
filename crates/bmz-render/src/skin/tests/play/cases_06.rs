use super::*;

#[test]
fn build_judge_region_state_tracks_signed_timing_per_region() {
    use crate::snapshot::DisplayJudgement;
    let judgement = |lane, delta_us, suppressed| DisplayJudgement {
        lane,
        judge: bmz_core::judge::Judge::Great,
        side: Some(bmz_core::judge::TimingSide::Fast),
        text: String::new(),
        combo: 1,
        delta_us,
        time: TimeUs(1_000),
        is_miss: false,
        timing_ms_suppressed: suppressed,
    };
    // 1P 側 FAST 3ms、2P 側 SLOW 7ms。
    let judgements = [judgement(Lane::Key1, -3_000, false), judgement(Lane::Key8, 7_000, false)];
    let state = build_judge_region_state(&judgements, 2_000, KeyMode::K14, 2);
    assert_eq!(state.judge_timing_ms[0], Some(-3));
    assert_eq!(state.judge_timing_ms[1], Some(7));
    assert_eq!(state.judge_timing_ms[2], None);

    // 閾値フィルタで抑制された判定は ±ms を領域ごと隠す。
    let suppressed = [judgement(Lane::Key1, -3_000, true)];
    let state = build_judge_region_state(&suppressed, 2_000, KeyMode::K14, 2);
    assert_eq!(state.judge_timing_ms[0], None);
}

#[test]
fn lane_judge_region_maps_14k_sides() {
    assert_eq!(lane_judge_region(Lane::Scratch, KeyMode::K14, 2), 0);
    assert_eq!(lane_judge_region(Lane::Key7, KeyMode::K14, 2), 0);
    assert_eq!(lane_judge_region(Lane::Key8, KeyMode::K14, 2), 1);
    assert_eq!(lane_judge_region(Lane::Scratch2, KeyMode::K14, 2), 1);
    assert_eq!(lane_judge_region(Lane::Scratch, KeyMode::K10, 2), 0);
    assert_eq!(lane_judge_region(Lane::Key8, KeyMode::K10, 2), 1);
}

#[test]
fn lane_judge_region_splits_9k_into_three_regions() {
    // beatoraja は lane / (9 / 3) で KEY1-3 / KEY4-6 / KEY7-9 に分ける。
    let regions = [
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
        Lane::Key8,
        Lane::Key9,
    ]
    .map(|lane| lane_judge_region(lane, KeyMode::K9, 3));
    assert_eq!(regions, [0, 0, 0, 1, 1, 1, 2, 2, 2]);
    // 1領域は全レーン共通。2領域では 9 / 2 = 4 レーンずつで、余った KEY9 は右へ寄せる。
    assert_eq!(lane_judge_region(Lane::Key9, KeyMode::K9, 1), 0);
    assert_eq!(lane_judge_region(Lane::Key4, KeyMode::K9, 2), 0);
    assert_eq!(lane_judge_region(Lane::Key5, KeyMode::K9, 2), 1);
    assert_eq!(lane_judge_region(Lane::Key9, KeyMode::K9, 2), 1);
    assert_eq!(lane_judge_region(Lane::Scratch, KeyMode::K7, 1), 0);
}

#[test]
fn judge_runtime_lights_the_right_9k_region_timer() {
    use crate::snapshot::DisplayJudgement;

    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 0, "w": 1, "h": 1, "destination": [] }"#).unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();
    runtime.ingest_judge_lane_state(
        &[DisplayJudgement {
            lane: Lane::Key8,
            judge: bmz_core::judge::Judge::Great,
            side: None,
            text: String::new(),
            combo: 3,
            delta_us: 0,
            time: TimeUs(1_000),
            is_miss: false,
            timing_ms_suppressed: false,
        }],
        KeyMode::K9,
        3,
        11_000,
    );
    runtime.advance(&document, &mut state, 11);
    // timer 247 (3P 判定) は領域2の値を読む。
    assert_eq!(state.judge_ms, [None, None, Some(10)]);
    assert_eq!(state.judge_combo[2], 3);
}

#[test]
fn judge_runtime_keeps_each_dp_combo_after_recent_history_expires() {
    use crate::snapshot::DisplayJudgement;

    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 0, "w": 1, "h": 1, "destination": [] }"#).unwrap();
    let judgement = |lane, combo, time| DisplayJudgement {
        lane,
        judge: bmz_core::judge::Judge::PGreat,
        side: None,
        text: String::new(),
        combo,
        delta_us: 0,
        time: TimeUs(time),
        is_miss: false,
        timing_ms_suppressed: false,
    };
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();
    runtime.ingest_judge_lane_state(
        &[judgement(Lane::Key1, 5, 1_000), judgement(Lane::Key8, 6, 2_000)],
        KeyMode::K14,
        2,
        3_000,
    );
    runtime.advance(&document, &mut state, 3);
    assert_eq!(&state.judge_combo[..2], &[5, 6]);

    let mut break_judgement = judgement(Lane::Key8, 0, 4_000);
    break_judgement.judge = bmz_core::judge::Judge::Bad;
    runtime.ingest_judge_lane_state(&[break_judgement], KeyMode::K14, 2, 5_000);
    runtime.advance(&document, &mut state, 5);
    assert_eq!(&state.judge_combo[..2], &[5, 0]);

    // session 側の recent_judgements (800ms) が空になっても、SkinJudge の
    // destination timer が終わるまで左右の最新値を保持する。
    runtime.ingest_judge_lane_state(&[], KeyMode::K14, 2, 1_000_000);
    runtime.advance(&document, &mut state, 1_000);
    assert_eq!(&state.judge_combo[..2], &[5, 0]);
    assert_eq!(state.judge_ms[0], Some(999));
    assert_eq!(state.judge_ms[1], Some(996));
}

#[test]
fn dual_judge_regions_render_combo_at_separate_positions() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "judgen-pg", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 3 }
                ],
                "judge": [
                    {
                        "id": "judge",
                        "index": 0,
                        "images": [
                            { "id": "judgef-pg", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                        ],
                        "numbers": [
                            { "id": "judgen-pg", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                        ]
                    },
                    {
                        "id": "judge1",
                        "index": 1,
                        "images": [
                            { "id": "judgef-pg", "dst": [{ "time": 0, "x": 60, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                        ],
                        "numbers": [
                            { "id": "judgen-pg", "dst": [{ "time": 0, "x": 70, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                        ]
                    }
                ],
                "destination": [
                    { "id": "judge" },
                    { "id": "judge1" }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    assert_eq!(document.judge_region_count(), 2);
    let state = SkinDrawState {
        judge_ms: {
            let mut ms = [None; MAX_JUDGE_REGIONS];
            ms[0] = Some(100);
            ms[1] = Some(100);
            ms
        },
        judge_index: {
            let mut idx = [None; MAX_JUDGE_REGIONS];
            idx[0] = Some(0);
            idx[1] = Some(0);
            idx
        },
        judge_combo: {
            let mut combo = [0; MAX_JUDGE_REGIONS];
            combo[0] = 42;
            combo[1] = 42;
            combo
        },
        combo: 42,
        ..SkinDrawState::default()
    };
    let left = document
        .judge_render_items_for_def(&document.judge[0], 0, 42, 100, &sources, &state)
        .unwrap();
    let right = document
        .judge_render_items_for_def(&document.judge[1], 0, 42, 100, &sources, &state)
        .unwrap();
    let left_digit = match &left[1] {
        SkinRenderItem::Image { rect, .. } => rect.x,
        _ => panic!("expected digit image"),
    };
    let right_digit = match &right[1] {
        SkinRenderItem::Image { rect, .. } => rect.x,
        _ => panic!("expected digit image"),
    };
    assert!(
        right_digit > left_digit + 0.2,
        "right region digit x={right_digit} should be right of left x={left_digit}"
    );

    let static_items = document.static_render_items(&sources, &state, &SkinTextState::default());
    assert_eq!(static_items.len(), 6);
    let static_left = match &static_items[1] {
        SkinRenderItem::Image { rect, .. } => rect.x,
        _ => panic!(),
    };
    let static_right = match &static_items[4] {
        SkinRenderItem::Image { rect, .. } => rect.x,
        _ => panic!(),
    };
    assert!(static_right > static_left + 0.2);
}

#[test]
fn judge_uses_max_gauge_variant_and_hides_combo_for_bad() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 0,
            "w": 100,
            "h": 100,
            "source": [{ "id": 1, "path": "judge.png" }],
            "image": [
                { "id": "i0", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "i1", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "i2", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "i3", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "i4", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "i5", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "i6", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
            ],
            "value": [
                { "id": "n0", "src": 1, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 3 },
                { "id": "n1", "src": 1, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 3 },
                { "id": "n2", "src": 1, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 3 },
                { "id": "n3", "src": 1, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 3 },
                { "id": "n4", "src": 1, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 3 },
                { "id": "n5", "src": 1, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 3 },
                { "id": "n6", "src": 1, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 3 }
            ],
            "judge": [{
                "id": "judge",
                "index": 0,
                "images": [
                    { "id": "i0", "dst": [{ "time": 0, "x": 10, "y": 10, "w": 10, "h": 10 }, { "time": 500 }] },
                    { "id": "i1", "dst": [{ "time": 0, "x": 20, "y": 10, "w": 10, "h": 10 }, { "time": 500 }] },
                    { "id": "i2", "dst": [{ "time": 0, "x": 30, "y": 10, "w": 10, "h": 10 }, { "time": 500 }] },
                    { "id": "i3", "dst": [{ "time": 0, "x": 40, "y": 10, "w": 10, "h": 10 }, { "time": 500 }] },
                    { "id": "i4", "dst": [{ "time": 0, "x": 50, "y": 10, "w": 10, "h": 10 }, { "time": 500 }] },
                    { "id": "i5", "dst": [{ "time": 0, "x": 60, "y": 10, "w": 10, "h": 10 }, { "time": 500 }] },
                    { "id": "i6", "dst": [{ "time": 0, "x": 70, "y": 10, "w": 10, "h": 10 }, { "time": 500 }] }
                ],
                "numbers": [
                    { "id": "n0", "dst": [{ "time": 0, "x": 10, "y": 0, "w": 5, "h": 10 }, { "time": 500 }] },
                    { "id": "n1", "dst": [{ "time": 0, "x": 20, "y": 0, "w": 5, "h": 10 }, { "time": 500 }] },
                    { "id": "n2", "dst": [{ "time": 0, "x": 30, "y": 0, "w": 5, "h": 10 }, { "time": 500 }] },
                    { "id": "n3", "dst": [{ "time": 0, "x": 40, "y": 0, "w": 5, "h": 10 }, { "time": 500 }] },
                    { "id": "n4", "dst": [{ "time": 0, "x": 50, "y": 0, "w": 5, "h": 10 }, { "time": 500 }] },
                    { "id": "n5", "dst": [{ "time": 0, "x": 60, "y": 0, "w": 5, "h": 10 }, { "time": 500 }] },
                    { "id": "n6", "dst": [{ "time": 0, "x": 70, "y": 0, "w": 5, "h": 10 }, { "time": 500 }] }
                ]
            }],
            "destination": [{ "id": "judge" }]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let max_state = SkinDrawState { gauge: 100.0, gauge_max: 100.0, ..SkinDrawState::default() };

    let max_items = document
        .judge_render_items_for_def(&document.judge[0], 0, 42, 100, &sources, &max_state)
        .unwrap();
    let max_x = match &max_items[0] {
        SkinRenderItem::Image { rect, .. } => rect.x,
        _ => panic!(),
    };
    assert!(approx_eq(max_x, 0.7));
    assert!(max_items.len() > 1);

    let bad_items = document
        .judge_render_items_for_def(
            &document.judge[0],
            3,
            42,
            100,
            &sources,
            &SkinDrawState::default(),
        )
        .unwrap();
    assert_eq!(bad_items.len(), 1);
}

#[test]
fn judge_child_destination_uses_its_own_timer() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 0, "w": 100, "h": 100,
            "source": [{ "id": 1, "path": "judge.png" }],
            "image": [{ "id": "pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
            "judge": [{
                "id": "judge", "index": 0,
                "images": [{
                    "id": "pg", "timer": 0, "loop": -1,
                    "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }, { "time": 500 }]
                }]
            }],
            "destination": [{ "id": "judge" }]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let state = SkinDrawState { elapsed_ms: 600, ..SkinDrawState::default() };

    assert!(
        document
            .judge_render_items_for_def(&document.judge[0], 0, 0, 100, &sources, &state)
            .is_none()
    );
}

#[test]
fn judge_outer_destination_conditions_are_applied() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 0, "w": 100, "h": 100,
            "source": [{ "id": 1, "path": "judge.png" }],
            "image": [{ "id": "pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
            "judge": [{
                "id": "judge", "index": 0,
                "images": [{
                    "id": "pg",
                    "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }, { "time": 500 }]
                }]
            }],
            "destination": [{
                "id": "judge", "op": [999],
                "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }]
            }]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let mut state = SkinDrawState::default();
    state.judge_ms[0] = Some(100);
    state.judge_index[0] = Some(0);

    assert!(document.static_render_items(&sources, &state, &SkinTextState::default()).is_empty());
}

#[test]
fn skin_document_hides_judge_combo_when_region_combo_is_zero() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judge-poor", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "combo", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 3 }
                ],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judge-poor", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                    ],
                    "numbers": [
                        { "id": "combo", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                    ]
                }],
                "destination": [{ "id": "judge" }]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let state = SkinDrawState {
        combo: 123,
        judge_ms: judge_region_state(0, 100, 0).judge_ms,
        judge_index: judge_region_state(0, 100, 0).judge_index,
        judge_combo: [0; MAX_JUDGE_REGIONS],
        ..SkinDrawState::default()
    };

    let items = document.static_render_items(&sources, &state, &SkinTextState::default());

    assert_eq!(items.len(), 1);
}

#[test]
fn skin_draw_options_match_judge_fast_slow_regions() {
    let fast = SkinDrawState {
        judge_index: [Some(1), None, None],
        judge_timing_sign: [Some(1), None, None],
        ..SkinDrawState::default()
    };
    let slow = SkinDrawState {
        judge_index: [Some(1), None, None],
        judge_timing_sign: [Some(-1), None, None],
        ..SkinDrawState::default()
    };
    // Auto モード: PGREAT は apply_fast_slow_display_filter で side=None にされるため
    // judge_timing_sign=None となり、op 1242/1243 は false になる。
    let perfect_auto = SkinDrawState {
        judge_index: [Some(0), None, None],
        judge_timing_sign: [None, None, None],
        ..SkinDrawState::default()
    };
    // ThresholdMs モード(threshold=0)で PGREAT の side が残っても、beatoraja と
    // 同じく PERFECT と EARLY/LATE は同時成立しない。
    let perfect_threshold = SkinDrawState {
        judge_index: [Some(0), None, None],
        judge_timing_sign: [Some(1), None, None],
        ..SkinDrawState::default()
    };

    assert!(test_skin_op(1242, &[], &fast));
    assert!(!test_skin_op(1243, &[], &fast));
    assert!(test_skin_op(1243, &[], &slow));
    assert!(!test_skin_op(1242, &[], &slow));
    assert!(test_skin_op(241, &[], &perfect_auto));
    assert!(!test_skin_op(1242, &[], &perfect_auto));
    assert!(test_skin_op(241, &[], &perfect_threshold));
    assert!(!test_skin_op(1242, &[], &perfect_threshold));

    for (judge_index, op) in (241..=246).enumerate() {
        let state = SkinDrawState {
            judge_index: [Some(judge_index), None, None],
            ..SkinDrawState::default()
        };
        assert!(test_skin_op(op, &[], &state), "1P op {op}");
        assert!(!test_skin_op(261 + judge_index as i32, &[], &state), "2P op");
    }

    for (region, base_op) in [(1, 261), (2, 361)] {
        for judge_index in 0..=5 {
            let mut judge_indices = [None; MAX_JUDGE_REGIONS];
            judge_indices[region] = Some(judge_index);
            let state = SkinDrawState { judge_index: judge_indices, ..SkinDrawState::default() };
            let op = base_op + judge_index as i32;
            assert!(test_skin_op(op, &[], &state), "region {region}, op {op}");
        }
    }
}

#[test]
fn play_judgement_existence_options_gate_full_combo_effects() {
    let clean = SkinDrawState { play_screen: true, ..SkinDrawState::default() };
    assert!(!test_skin_op(2244, &[], &clean));
    assert!(!test_skin_op(2245, &[], &clean));
    assert!(test_skin_op(-2244, &[], &clean));
    assert!(test_skin_op(-2245, &[], &clean));

    let failed = SkinDrawState {
        play_screen: true,
        judge_counts: DisplayJudgeCounts {
            bad: 1,
            poor: 2,
            empty_poor: 3,
            ..DisplayJudgeCounts::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(2244, &[], &failed));
    assert!(test_skin_op(2245, &[], &failed));
    assert!(test_skin_op(2246, &[], &failed));
    assert!(!test_skin_op(-2244, &[], &failed));
    assert!(!test_skin_op(-2245, &[], &failed));
}

#[test]
fn skin_draw_options_match_live_score_ranks() {
    let base = SkinDrawState { total_notes: 100, ..SkinDrawState::default() };
    let aaa = SkinDrawState { ex_score: 178, ..base.clone() };
    let aa = SkinDrawState { ex_score: 156, ..base.clone() };
    let a = SkinDrawState { ex_score: 134, ..base.clone() };
    let f = SkinDrawState { ex_score: 0, ..base };

    assert!(test_skin_op(220, &[], &aaa));
    assert!(test_skin_op(221, &[], &aaa));
    assert!(!test_skin_op(220, &[], &aa));
    assert!(test_skin_op(221, &[], &aa));
    assert!(test_skin_op(222, &[], &a));
    assert!(test_skin_op(223, &[], &a));
    assert!(test_skin_op(227, &[], &f));
    assert!(!test_skin_op(226, &[], &f));
    assert!(!test_skin_op(220, &[], &SkinDrawState::default()));
}

#[test]
fn skin_document_shifts_judge_combo_numbers_beatoraja_style() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "judgen-pg", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 6 }
                ],
                "judge": [{
                    "id": 2010,
                    "shift": true,
                    "images": [
                        { "id": "judgef-pg", "dst": [{ "time": 0, "x": 30, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                    ],
                    "numbers": [
                        { "id": "judgen-pg", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                    ]
                }],
                "destination": [
                    { "id": 2010 }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let items = document.static_render_items(
        &sources,
        &SkinDrawState {
            judge_ms: judge_region_state(0, 100, 0).judge_ms,
            judge_index: judge_region_state(0, 100, 0).judge_index,
            judge_combo: {
                let mut combo = [0; MAX_JUDGE_REGIONS];
                combo[0] = 123;
                combo
            },
            ..Default::default()
        },
        &SkinTextState::default(),
    );

    assert_eq!(items.len(), 4);
    assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.23)));
    // dst x 20 - w*6/2 = 5, align=2, shiftbase=3, judge x 30 - length/2 = 23
    assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.43)));
    assert!(matches!(items[2], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.48)));
    assert!(matches!(items[3], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.53)));
}

#[test]
fn skin_document_resolves_lane_imageset_effects() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "effect.png" }],
                "image": [
                    { "id": "normal", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "pgreat", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 },
                    { "id": "good", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 }
                ],
                "imageset": [
                    { "id": "beam1", "ref": 501, "images": ["normal", "pgreat"] },
                    { "id": "bomb1", "ref": 501, "images": ["normal", "pgreat", "good"] },
                    { "id": "beam2", "ref": 502, "images": ["normal", "pgreat"] }
                ],
                "destination": [
                    { "id": "beam1", "timer": 51, "loop": -1, "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] },
                    { "id": "bomb1", "timer": 51, "loop": -1, "dst": [{ "time": 0, "x": 30, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] },
                    { "id": "beam2", "timer": 52, "loop": -1, "dst": [{ "time": 0, "x": 50, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 100.0, height: 100.0 },
        },
    )]);

    // Key1 (timer 51 = bomb_ms[1]) でボムタイマー進行中、直近判定 PGREAT
    let pgreat_state = SkinDrawState {
        bomb_ms: {
            let mut a = [None; LANE_COUNT];
            a[1] = Some(50);
            a
        },
        lane_judge: {
            let mut a = [None; LANE_COUNT];
            a[1] = Some(0);
            a
        },
        ..SkinDrawState::default()
    };
    let pgreat = document.static_render_items(&sources, &pgreat_state, &SkinTextState::default());
    // GOOD 判定
    let good_state = SkinDrawState {
        bomb_ms: {
            let mut a = [None; LANE_COUNT];
            a[1] = Some(50);
            a
        },
        lane_judge: {
            let mut a = [None; LANE_COUNT];
            a[1] = Some(2);
            a
        },
        ..SkinDrawState::default()
    };
    let good = document.static_render_items(&sources, &good_state, &SkinTextState::default());
    // タイマーがアニメーション終端を超過 → loop:-1 で非表示
    let expired_state = SkinDrawState {
        bomb_ms: {
            let mut a = [None; LANE_COUNT];
            a[1] = Some(150);
            a
        },
        lane_judge: {
            let mut a = [None; LANE_COUNT];
            a[1] = Some(0);
            a
        },
        ..SkinDrawState::default()
    };
    let expired = document.static_render_items(&sources, &expired_state, &SkinTextState::default());

    // beam1 と bomb1 のみ描画される (beam2 は timer 52 非アクティブ)
    assert_eq!(pgreat.len(), 2);
    // beam1: 2枚構成 + PGREAT → "pgreat" 画像 (u=0.1), rect x=0.1
    assert!(matches!(pgreat[0], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.1) && approx_eq(u, 0.1)));
    // beam1: 2枚構成 + GOOD → "normal" 画像 (u=0.0)
    assert!(matches!(good[0], SkinRenderItem::Image {
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(u, 0.0)));
    // bomb1: 3枚構成 + GOOD(index2) → "good" 画像 (u=0.2), rect x=0.3
    assert!(matches!(good[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.3) && approx_eq(u, 0.2)));
    assert!(expired.is_empty());
}

#[test]
fn judge_timing_value_omits_sign_when_numeric_digits_fill_all_cells() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "value": [{ "id": "judgetiming", "src": 1, "x": 0, "y": 0, "w": 120, "h": 20, "divx": 12, "divy": 2, "digit": 2, "ref": 12 }],
                "destination": [{ "id": "judgetiming", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 120.0, 40.0);
    let state = SkinDrawState { judge_timing_offset_ms: 12, ..SkinDrawState::default() };

    let items = document.static_image_render_items(&sources, &state);
    let digit_uvs: Vec<f32> = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { uv, .. } => Some(uv.x),
            _ => None,
        })
        .collect();

    assert_eq!(digit_uvs.len(), 2);
    assert!(approx_eq(digit_uvs[0], 1.0 / 12.0), "first cell should be tens digit");
    assert!(approx_eq(digit_uvs[1], 2.0 / 12.0), "second cell should be ones digit");
}

#[test]
fn lane_cover_numbers_render_before_ready_while_changing() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "number.png" }],
                "value": [
                    { "id": "white", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 14 },
                    { "id": "green", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 313 },
                    { "id": "combo", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 104 }
                ],
                "destination": [
                    { "id": "white", "timer": 40, "op": [270], "dst": [{ "x": 10, "y": 20, "w": 5, "h": 10 }] },
                    { "id": "green", "timer": 40, "op": [270], "dst": [{ "x": 10, "y": 30, "w": 5, "h": 10 }] },
                    { "id": "combo", "timer": 40, "op": [270], "dst": [{ "x": 10, "y": 40, "w": 5, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 100.0, height: 100.0 },
        },
    )]);

    let inactive = document.static_image_render_items(
        &sources,
        &SkinDrawState { ready_timer_ms: None, ..SkinDrawState::default() },
    );
    assert!(inactive.is_empty());

    let active = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            ready_timer_ms: None,
            lane_cover_changing: true,
            lane_cover: 0.25,
            total_duration_ms: 300,
            combo: 123,
            ..SkinDrawState::default()
        },
    );
    assert_eq!(active.len(), 6);
}

#[test]
fn skin_document_resolves_hidden_cover_destinations() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 12, "path": "cover.png" }],
                "hiddenCover": [
                    { "id": "hidden-cover", "src": 12, "x": 10, "y": 20, "w": 30, "h": 40 }
                ],
                "destination": [
                    { "id": "hidden-cover", "blend": 2, "dst": [{ "x": 20, "y": -40, "w": 30, "h": 40, "a": 128 }] }
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
            source_size: SkinImageSize { width: 100.0, height: 100.0 },
        },
    )]);

    let hidden = document.static_image_render_items(&sources, &SkinDrawState::default());
    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { hidden_enabled: true, ..SkinDrawState::default() },
    );

    assert!(matches!(hidden.as_slice(), [SkinRenderItem::Image { tint, .. }] if tint.a == 0.0));
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uw, height: uh },
                tint: Color { a, .. },
                blend,
                ..
            } if approx_eq(x, 0.2)
                && approx_eq(y, 1.0)
                && approx_eq(width, 0.3)
                && approx_eq(height, 0.4)
                && approx_eq(u, 0.1)
                && approx_eq(v, 0.2)
                && approx_eq(uw, 0.3)
                && approx_eq(uh, 0.4)
                && approx_eq(a, 128.0 / 255.0)
                && blend == BlendMode::Add));
    assert_eq!(document.hidden_cover[0].disappear_line, 0);
    assert!(document.hidden_cover[0].is_disappear_line_link_lift);
}
