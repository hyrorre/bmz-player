use super::*;

#[test]
fn result_document_state_maps_canvas_mouse_position_to_skin_pixels() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 7, "w": 200, "h": 100 }"#).unwrap();

    let unavailable = result_skin_draw_state_for_document(&snapshot, &document);
    assert_eq!(unavailable.mouse_x, None);
    assert_eq!(unavailable.mouse_y, None);

    snapshot.mouse_position = Some((0.25, 0.75));
    let hovered = result_skin_draw_state_for_document(&snapshot, &document);
    assert_eq!(hovered.mouse_x, Some(50.0));
    assert_eq!(hovered.mouse_y, Some(25.0));
}

#[test]
fn result_skin_state_exposes_autoplay_options() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };

    let normal = build_result_skin_draw_state(&snapshot, 0);
    assert!(!normal.autoplay);
    assert!(crate::skin::test_skin_ops(&[32], &[], &normal));
    assert!(!crate::skin::test_skin_ops(&[33], &[], &normal));

    snapshot.autoplay = true;
    let autoplay = build_result_skin_draw_state(&snapshot, 0);
    assert!(autoplay.autoplay);
    assert!(!crate::skin::test_skin_ops(&[32], &[], &autoplay));
    assert!(crate::skin::test_skin_ops(&[33], &[], &autoplay));
}

#[test]
fn result_skin_state_keeps_clear_failed_flag_separate_from_clear_type() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    snapshot.clear_type = bmz_core::clear::ClearType::NoPlay;
    snapshot.result_failed = false;

    let state = build_result_skin_draw_state(&snapshot, 0);

    assert_eq!(state.select_clear_index, bmz_core::clear::ClearType::NoPlay as i64);
    assert_eq!(state.result_failed, Some(false));

    snapshot.result_failed = true;
    let failed_state = build_result_skin_draw_state(&snapshot, 0);

    assert_eq!(failed_state.select_clear_index, bmz_core::clear::ClearType::NoPlay as i64);
    assert_eq!(failed_state.result_failed, Some(true));
}

#[test]
fn result_skin_state_falls_back_to_timing_points_for_average_timing() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    std::sync::Arc::make_mut(&mut snapshot.graph).timing_points = vec![
        crate::snapshot::ResultTimingPoint {
            time_ms: 0,
            delta_us: -12_000,
            judge: bmz_core::judge::Judge::Great,
        },
        crate::snapshot::ResultTimingPoint {
            time_ms: 1000,
            delta_us: 20_000,
            judge: bmz_core::judge::Judge::PGreat,
        },
    ];

    let state = build_result_skin_draw_state(&snapshot, 0);

    assert_eq!(state.average_timing_ms, Some(4.0));
    assert_eq!(state.average_duration_us, Some(998_032));
    assert_eq!(state.stddev_timing_ms, Some(16.0));
}

#[test]
fn result_skin_state_uses_precomputed_timing_metrics() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    let graph = std::sync::Arc::make_mut(&mut snapshot.graph);
    graph.timing_points = vec![
        crate::snapshot::ResultTimingPoint {
            time_ms: 0,
            delta_us: -12_000,
            judge: bmz_core::judge::Judge::Great,
        },
        crate::snapshot::ResultTimingPoint {
            time_ms: 1000,
            delta_us: 20_000,
            judge: bmz_core::judge::Judge::PGreat,
        },
    ];
    graph.refresh_timing_metrics();
    graph.timing_points.clear();
    graph.timing_distribution = Default::default();

    let state = build_result_skin_draw_state(&snapshot, 0);

    assert_eq!(state.average_timing_ms, Some(4.0));
    assert_eq!(state.average_duration_us, Some(998_032));
    assert_eq!(state.stddev_timing_ms, Some(16.0));
}

#[test]
fn result_average_duration_uses_absolute_deltas_and_unjudged_penalty() {
    let points = [
        crate::snapshot::ResultTimingPoint {
            time_ms: 0,
            delta_us: -10_000,
            judge: bmz_core::judge::Judge::Great,
        },
        crate::snapshot::ResultTimingPoint {
            time_ms: 1000,
            delta_us: 20_000,
            judge: bmz_core::judge::Judge::PGreat,
        },
    ];

    assert_eq!(result_average_duration_us(&points, 4), Some(507_500));
    assert_eq!(result_average_duration_us(&points, 0), None);
}

#[test]
fn result_display_gauge_uses_selected_graph_history_tail() {
    use crate::snapshot::ResultGaugeGraphPoint;
    use bmz_core::clear::GaugeType;

    let points = [
        ResultGaugeGraphPoint {
            time_ms: 0,
            value: 20.0,
            max: 100.0,
            border: 0.0,
            gauge_type: GaugeType::ExHard as i32,
            course_section_start: false,
        },
        ResultGaugeGraphPoint {
            time_ms: 1_000,
            value: 80.0,
            max: 100.0,
            border: 80.0,
            gauge_type: GaugeType::Normal as i32,
            course_section_start: false,
        },
        ResultGaugeGraphPoint {
            time_ms: 1_000,
            value: 42.0,
            max: 100.0,
            border: 0.0,
            gauge_type: GaugeType::ExHard as i32,
            course_section_start: false,
        },
    ];

    assert_eq!(
        result_display_gauge(&points, GaugeType::ExHard as i32, 80.0, GaugeType::Normal as i32,),
        (42.0, GaugeType::ExHard as i32, 100.0, 0.0)
    );
}

#[test]
fn result_display_gauge_falls_back_when_selected_history_is_missing() {
    use bmz_core::clear::GaugeType;

    assert_eq!(
        result_display_gauge(&[], GaugeType::ExHard as i32, 80.0, GaugeType::Normal as i32,),
        (80.0, GaugeType::Normal as i32, 100.0, 80.0)
    );
}

#[test]
fn result_plan_renders_gaugegraph_from_result_graph_data() {
    use crate::scene::ResultSnapshot;
    use crate::snapshot::{FastSlowJudgeCounts, ResultGaugeGraphPoint, ResultGraphSnapshot};
    use bmz_core::clear::ClearType;

    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "name": "test",
                "w": 100,
                "h": 100,
                "gaugegraph": [{
                    "id": "gg",
                    "color": [
                        "010101", "ff0000", "00ff00", "0000ff",
                        "010101", "010101", "010101", "010101",
                        "010101", "010101", "010101", "010101",
                        "010101", "010101", "010101", "010101",
                        "010101", "010101", "010101", "010101",
                        "010101", "010101", "010101", "010101"
                    ]
                }],
                "destination": [
                    {"id": "gg", "dst": [{"x": 0, "y": 0, "w": 100, "h": 100}]}
                ]
            }"#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        std::iter::empty(),
    );
    let snapshot = ResultSnapshot {
        screen_size: [0, 0],
        last_played_at: None,
        lr2_progress: Default::default(),
        operating_time_ms: 0,
        player_name: String::new(),
        target_name: String::new(),
        target: Default::default(),
        current_fps: 0,
        skin_input: Default::default(),
        skin_attempt: Default::default(),
        skin_offsets: Default::default(),
        mouse_position: None,
        hispeed_auto_adjust: false,
        assist_flags: [false; 7],
        assist_extra_note_depth: 0,
        assist_mine_mode: 0,
        assist_scroll_mode: 0,
        assist_long_note_mode: 0,
        clear_type: ClearType::Normal,
        result_failed: false,
        autoplay: false,
        arrange: "NORMAL".to_string(),
        arrange_2p: "NORMAL".to_string(),
        double_option: "OFF".to_string(),
        lane_shuffle_pattern: Vec::new(),
        ex_score: 100,
        ex_score_rate: 0.5,
        max_combo: 50,
        bp: 0,
        cb: 0,
        gauge_value: 80.0,
        gauge_type: bmz_core::clear::GaugeType::Normal as i32,
        total_notes: 100,
        duration_ms: 0,
        note_display_duration_ms: None,
        initial_bpm: 0.0,
        min_bpm: 0.0,
        max_bpm: 0.0,
        main_bpm: 0.0,
        total_gauge: 0.0,
        judge_rank: None,
        note_judge_window: None,
        key_mode: bmz_core::lane::KeyMode::default(),
        has_long_notes: false,
        ln_mode_index: 0,
        rule_mode_index: 0,
        ln_score_policy_index: Some(0),
        result_gauge_graph_type: bmz_core::clear::GaugeType::AssistEasy as i32,
        result_panel: 0,
        favorite_chart: false,
        judge_counts: DisplayJudgeCounts::default(),
        fast_slow_counts: FastSlowJudgeCounts::default(),
        score_save_enabled: false,
        score_history_id: 0,
        replay_saved: false,
        replay_slots: [false; 4],
        saved_replay_slots: [false; 4],
        best_ex_score: None,
        best_clear_type: None,
        target_ex_score: None,
        best_max_combo: None,
        target_max_combo: None,
        best_bp: None,
        target_bp: None,
        previous_best_ex_score: None,
        previous_best_clear_type: None,
        previous_best_max_combo: None,
        previous_best_bp: None,
        target_clear_type: None,
        elapsed_time: TimeUs(2_000_000),
        fadeout_elapsed: None,
        title: String::new(),
        subtitle: String::new(),
        artist: String::new(),
        subartist: String::new(),
        genre: String::new(),
        difficulty_name: String::new(),
        play_level: String::new(),
        table_text_primary: String::new(),
        table_text_secondary: String::new(),
        table_text_fallback: String::new(),
        stagefile_background: false,
        stagefile_image_size: None,
        course_titles: Default::default(),
        course_result: Default::default(),
        graph: std::sync::Arc::new(ResultGraphSnapshot {
            gauge_points: vec![
                ResultGaugeGraphPoint {
                    time_ms: 0,
                    value: 20.0,
                    max: 100.0,
                    border: 60.0,
                    gauge_type: bmz_core::clear::GaugeType::AssistEasy as i32,
                    course_section_start: false,
                },
                ResultGaugeGraphPoint {
                    time_ms: 1_000,
                    value: 90.0,
                    max: 100.0,
                    border: 60.0,
                    gauge_type: bmz_core::clear::GaugeType::AssistEasy as i32,
                    course_section_start: false,
                },
            ],
            ..ResultGraphSnapshot::default()
        }),
        overlay: crate::snapshot::OverlaySnapshot::default(),
        ir: crate::scene::ResultIrSnapshot::default(),
        player_stats: crate::scene::PlayerStatsSnapshot::default(),
    };

    let draw_state = result_skin_draw_state(&snapshot, 0);
    assert_eq!(draw_state.gauge, 90.0);
    assert_eq!(draw_state.gauge_type, bmz_core::clear::GaugeType::AssistEasy as i32);
    assert_eq!(draw_state.gauge_max, 100.0);
    assert_eq!(draw_state.gauge_border, 60.0);

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Result(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| {
        draw_command_has_rect(command, |_, Color { r, g, b, .. }| {
            (*r - 0.0).abs() < 0.01 && (*g - 1.0).abs() < 0.01 && (*b - 0.0).abs() < 0.01
        })
    }));
    assert!(plan.commands.iter().any(|command| {
        draw_command_has_rect(command, |rect, Color { r, g, b, .. }| {
            (*r - 1.0).abs() < 0.01 && *g < 0.01 && *b < 0.01 && (rect.height - 0.4).abs() < 0.01
        })
    }));
}

#[test]
fn result_plan_renders_timing_distribution_from_result_graph_data() {
    use crate::scene::ResultSnapshot;
    use crate::snapshot::{
        FastSlowJudgeCounts, ResultGraphSnapshot, ResultTimingDistribution, ResultTimingPoint,
    };
    use bmz_core::clear::ClearType;
    use bmz_core::judge::Judge;

    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "name": "test",
                "w": 100,
                "h": 100,
                "timingdistributiongraph": [{"id": "td", "graphColor": "00FF00FF"}],
                "destination": [
                    {"id": "td", "dst": [{"x": 0, "y": 0, "w": 100, "h": 50}]}
                ]
            }"#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        std::iter::empty(),
    );
    let mut timing_distribution = ResultTimingDistribution::default();
    timing_distribution.add(-12);
    timing_distribution.add(8);
    let snapshot = ResultSnapshot {
        screen_size: [0, 0],
        last_played_at: None,
        lr2_progress: Default::default(),
        operating_time_ms: 0,
        player_name: String::new(),
        target_name: String::new(),
        target: Default::default(),
        current_fps: 0,
        skin_input: Default::default(),
        skin_attempt: Default::default(),
        skin_offsets: Default::default(),
        mouse_position: None,
        hispeed_auto_adjust: false,
        assist_flags: [false; 7],
        assist_extra_note_depth: 0,
        assist_mine_mode: 0,
        assist_scroll_mode: 0,
        assist_long_note_mode: 0,
        clear_type: ClearType::Normal,
        result_failed: false,
        autoplay: false,
        arrange: "NORMAL".to_string(),
        arrange_2p: "NORMAL".to_string(),
        double_option: "OFF".to_string(),
        lane_shuffle_pattern: Vec::new(),
        ex_score: 100,
        ex_score_rate: 0.5,
        max_combo: 50,
        bp: 0,
        cb: 0,
        gauge_value: 80.0,
        gauge_type: bmz_core::clear::GaugeType::Normal as i32,
        total_notes: 100,
        duration_ms: 0,
        note_display_duration_ms: None,
        initial_bpm: 0.0,
        min_bpm: 0.0,
        max_bpm: 0.0,
        main_bpm: 0.0,
        total_gauge: 0.0,
        judge_rank: None,
        note_judge_window: None,
        key_mode: bmz_core::lane::KeyMode::default(),
        has_long_notes: false,
        ln_mode_index: 0,
        rule_mode_index: 0,
        ln_score_policy_index: Some(0),
        result_gauge_graph_type: bmz_core::clear::GaugeType::Normal as i32,
        result_panel: 0,
        favorite_chart: false,
        judge_counts: DisplayJudgeCounts::default(),
        fast_slow_counts: FastSlowJudgeCounts::default(),
        score_save_enabled: false,
        score_history_id: 0,
        replay_saved: false,
        replay_slots: [false; 4],
        saved_replay_slots: [false; 4],
        best_ex_score: None,
        best_clear_type: None,
        target_ex_score: None,
        best_max_combo: None,
        target_max_combo: None,
        best_bp: None,
        target_bp: None,
        previous_best_ex_score: None,
        previous_best_clear_type: None,
        previous_best_max_combo: None,
        previous_best_bp: None,
        target_clear_type: None,
        elapsed_time: TimeUs(0),
        fadeout_elapsed: None,
        title: String::new(),
        subtitle: String::new(),
        artist: String::new(),
        subartist: String::new(),
        genre: String::new(),
        difficulty_name: String::new(),
        play_level: String::new(),
        table_text_primary: String::new(),
        table_text_secondary: String::new(),
        table_text_fallback: String::new(),
        stagefile_background: false,
        stagefile_image_size: None,
        course_titles: Default::default(),
        course_result: Default::default(),
        graph: std::sync::Arc::new(ResultGraphSnapshot {
            timing_distribution,
            timing_points: vec![
                ResultTimingPoint { time_ms: 0, delta_us: -12_000, judge: Judge::Great },
                ResultTimingPoint { time_ms: 100, delta_us: 8_000, judge: Judge::PGreat },
            ],
            ..ResultGraphSnapshot::default()
        }),
        overlay: crate::snapshot::OverlaySnapshot::default(),
        ir: crate::scene::ResultIrSnapshot::default(),
        player_stats: crate::scene::PlayerStatsSnapshot::default(),
    };

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Result(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color: Color { r, g, b, .. }, .. }
            if (*r - 0.0).abs() < 0.01 && (*g - 1.0).abs() < 0.01 && (*b - 0.0).abs() < 0.01
    )));
}

#[test]
fn play_plan_uses_supplied_skin_context() {
    let manifest = SkinManifest {
        play: crate::skin::SkinPlayManifest {
            note: Some(crate::skin::SkinImageManifest {
                texture: 42,
                key_even_texture: None,
                scratch_texture: None,
                source_size: None,
                uv: crate::skin::TextureRegion::default(),
                scale: crate::skin::SkinImageScale::Stretch,
                border: None,
            }),
            ..crate::skin::SkinPlayManifest::default()
        },
        ..SkinManifest::default()
    };
    let skin = SkinContext::from_manifest(manifest);
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == TextureId(42)
    )));
}

fn target_text_skin(skin_type: i32) -> SkinContext {
    let mut document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "text": [
                    { "id": "rival", "size": 12, "ref": 1 },
                    { "id": "target", "size": 12, "ref": 3 },
                    { "id": "sentinel", "size": 12, "constantText": "Target name test" }
                ],
                "destination": [
                    { "id": "rival", "dst": [{ "x": 10, "y": 40, "w": 60, "h": 12 }] },
                    { "id": "target", "dst": [{ "x": 10, "y": 20, "w": 60, "h": 12 }] },
                    { "id": "sentinel", "dst": [{ "x": 10, "y": 60, "w": 60, "h": 12 }] }
                ]
            }
            "#,
    )
    .unwrap();
    document.skin_type = skin_type;
    SkinContext::from_manifest_and_document(SkinManifest::default(), document, [])
}

fn assert_target_text(plan: &DrawPlan, name: &str, setting: &str) {
    let texts: Vec<_> = plan
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let expected: Vec<_> =
        [name, setting, "Target name test"].into_iter().filter(|text| !text.is_empty()).collect();
    assert_eq!(texts, expected);
}

#[test]
fn play_skin_document_receives_target_text() {
    let skin = target_text_skin(0);
    for (target, resolved_name, expected) in [
        ("IR_TOP", None, ""),
        ("RANK_AAA", None, "RANK AAA"),
        ("", None, ""),
        ("NONE", None, ""),
        ("RIVAL_2", Some("ライバル_A"), "ライバル_A"),
        ("RIVAL_2", Some("AAA"), "AAA"),
        ("IR_TOP", Some("NONE"), "NONE"),
        ("IR_TOP", Some("RIVAL_2"), "RIVAL_2"),
        ("RANK_AAA", Some(""), ""),
    ] {
        let snapshot = RenderSnapshot {
            target: target.to_string(),
            resolved_target_name: resolved_name.map(str::to_string),
            ..RenderSnapshot::default()
        };
        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );
        assert_target_text(&plan, expected, &crate::skin::target_setting_name(target));
    }
}

#[test]
fn select_skin_document_separates_rival_name_from_target_setting() {
    let skin = target_text_skin(5);
    for (target, rival, resolved, expected, setting) in [
        ("NONE", "", None, "", "NO TARGET"),
        ("NONE", "未プレイ_A", None, "未プレイ_A", "NO TARGET"),
        ("RANK_AAA", "", None, "RANK AAA", "RANK AAA"),
        ("IR_TOP", "", None, "", "IR TOP"),
        ("IR_TOP", "", Some("AAA"), "AAA", "IR TOP"),
        ("IR_TOP", "Selected", Some("TopPlayer"), "Selected", "IR TOP"),
    ] {
        let AppSceneSnapshot::Select(mut snapshot) = crate::sample::sample_select_scene() else {
            unreachable!()
        };
        snapshot.target = target.to_string();
        snapshot.rival_name = rival.to_string();
        snapshot.resolved_target_name = resolved.map(str::to_string);
        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Select(snapshot),
            &skin,
            &mut Default::default(),
        );
        assert_target_text(&plan, expected, setting);
    }
}

#[test]
fn result_skin_document_receives_resolved_target_text() {
    let skin = target_text_skin(7);
    for name in ["ライバル_A", "AAA", "NONE", "RIVAL_2", "RANK AAA", ""] {
        let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
            panic!("sample result scene");
        };
        snapshot.target_name = name.to_string();
        snapshot.target = "IR_TOP".to_string();
        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Result(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );
        assert_target_text(&plan, name, "IR TOP");
    }
}

#[test]
fn play_skin_document_renders_bar_lines_in_note_area() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [
                        { "x": 10, "y": 20, "w": 5, "h": 60 },
                        { "x": 15, "y": 20, "w": 5, "h": 60 },
                        { "x": 20, "y": 20, "w": 5, "h": 60 },
                        { "x": 25, "y": 20, "w": 5, "h": 60 },
                        { "x": 30, "y": 20, "w": 5, "h": 60 },
                        { "x": 35, "y": 20, "w": 5, "h": 60 },
                        { "x": 40, "y": 20, "w": 5, "h": 60 },
                        { "x": 45, "y": 20, "w": 5, "h": 60 }
                    ],
                    "group": [
                        {
                            "id": "section-line",
                            "dst": [
                                { "x": 10, "y": 25, "w": 40, "h": 2, "r": 64, "g": 128, "b": 255, "a": 200 }
                            ]
                        }
                    ]
                }
            }
            "#,
        )
        .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(77),
        source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let mut snapshot = RenderSnapshot::default();
    snapshot.bar_lines.push(VisibleBarLine {
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        label: String::new(),
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, tint, .. }
            if *texture == TextureId(77)
                && approx_eq(rect.x, 0.1)
                && approx_eq(rect.y + rect.height, 0.45)
                && approx_eq(rect.width, 0.4)
                && approx_eq(rect.height, 0.02)
                && approx_eq(tint.r, 64.0 / 255.0)
                && approx_eq(tint.g, 128.0 / 255.0)
                && approx_eq(tint.b, 1.0)
                && approx_eq(tint.a, 200.0 / 255.0)
    )));
}

#[test]
fn play_skin_document_moves_bar_lines_in_same_direction_as_notes() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [
                    {"id": "note", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1},
                    {"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}
                ],
                "note": {
                    "id": "notes",
                    "note": ["note", "note", "note", "note", "note", "note", "note", "note"],
                    "dst": [{ "x": 10, "y": 20, "w": 40, "h": 60 }],
                    "group": [{
                        "id": "section-line",
                        "dst": [{ "x": 10, "y": 20, "w": 40, "h": 2 }]
                    }]
                }
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(77),
        source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let note_height = skin.document_note_height(Lane::Key1, KeyMode::K7).unwrap();
    let state = crate::skin::SkinDrawState::default();
    let early_note =
        skin.note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.5, note_height, &state).unwrap();
    let later_note =
        skin.note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.25, note_height, &state).unwrap();

    let bar_y = |progress| {
        let items = skin.document_bar_line_items(progress, KeyMode::K7, &state);
        let Some(SkinRenderItem::Image { rect, .. }) = items.first() else { panic!() };
        rect.y
    };
    let early_bar_y = bar_y(0.5);
    let later_bar_y = bar_y(0.25);

    assert!(later_note.y > early_note.y);
    assert!(later_bar_y > early_bar_y);
}

#[test]
fn play_skin_document_applies_bar_line_offset_height_and_alpha() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }],
                    "group": [{
                        "id": "section-line",
                        "dst": [{ "x": 10, "y": 20, "w": 40, "h": 2, "a": 200 }]
                    }]
                }
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(77),
        source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let mut snapshot = RenderSnapshot::default();
    snapshot.skin_offsets.set(
        SKIN_OFFSET_BAR_LINE,
        crate::skin_offset::SkinOffsetValue { h: 3, a: -50, ..Default::default() },
    );
    snapshot.bar_lines.push(VisibleBarLine {
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        label: String::new(),
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, tint, .. }
            if *texture == TextureId(77)
                && approx_eq(rect.height, 0.05)
                && approx_eq(tint.a, 150.0 / 255.0)
    )));
}

#[test]
fn default_play_bar_line_applies_height_and_alpha_offset() {
    let mut snapshot = RenderSnapshot::default();
    snapshot.skin_offsets.set(
        SKIN_OFFSET_BAR_LINE,
        crate::skin_offset::SkinOffsetValue { h: 4, a: -128, ..Default::default() },
    );
    snapshot.bar_lines.push(VisibleBarLine {
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        label: String::new(),
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { rect, color }
            if approx_eq(rect.height, 0.004 + 4.0 / 1080.0)
                && approx_eq(color.a, 127.0 / 255.0)
    )));
}

#[test]
fn play_skin_document_applies_declared_notes_offset_to_bar_lines() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }],
                    "group": [{
                        "id": "section-line",
                        "offset": 30,
                        "dst": [{ "x": 10, "y": 20, "w": 40, "h": 2, "a": 200 }]
                    }]
                }
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(77),
        source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let mut snapshot = RenderSnapshot::default();
    snapshot
        .skin_offsets
        .set(30, crate::skin_offset::SkinOffsetValue { h: 20, ..Default::default() });
    snapshot.skin_offsets.set(
        SKIN_OFFSET_BAR_LINE,
        crate::skin_offset::SkinOffsetValue { h: 5, a: -50, ..Default::default() },
    );
    snapshot.bar_lines.push(VisibleBarLine {
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        label: String::new(),
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, tint, .. }
            if *texture == TextureId(77)
                && approx_eq(rect.height, 0.27)
                && approx_eq(tint.a, 150.0 / 255.0)
    )));
}

#[test]
fn play_skin_document_without_group_does_not_fallback_to_bar_line_rect() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }]
                }
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let skin = SkinContext::from_manifest_and_document(manifest, document, []);
    let mut snapshot = RenderSnapshot::default();
    snapshot.skin_offsets.set(
        SKIN_OFFSET_BAR_LINE,
        crate::skin_offset::SkinOffsetValue { h: 4, a: -128, ..Default::default() },
    );
    snapshot.bar_lines.push(VisibleBarLine {
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        label: String::new(),
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(
        !plan.commands.iter().any(|command| matches!(command, DrawCommand::Rect { .. })),
        "skin documents without note.group should not receive default bar line fallback"
    );
}

#[test]
fn play_skin_document_applies_bar_line_alpha_after_global_offset() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }],
                    "group": [{
                        "id": "section-line",
                        "dst": [{ "x": 10, "y": 20, "w": 40, "h": 2, "a": 255 }]
                    }]
                }
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(77),
        source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let mut snapshot = RenderSnapshot::default();
    snapshot
        .skin_offsets
        .set(10, crate::skin_offset::SkinOffsetValue { w: 20, ..Default::default() });
    snapshot.skin_offsets.set(
        SKIN_OFFSET_BAR_LINE,
        crate::skin_offset::SkinOffsetValue { a: -64, ..Default::default() },
    );
    snapshot.bar_lines.push(VisibleBarLine {
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        label: String::new(),
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == TextureId(77) && approx_eq(tint.a, 191.0 / 255.0)
    )));
}
