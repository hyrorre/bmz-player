use super::*;

#[test]
fn play_plan_keeps_deprecated_grade_diff_options_on_fixed_next() {
    let document: SkinDocument = serde_json::from_str(
        r##"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "panel": [
                    { "id": "nearest", "color": "#12AB34" },
                    { "id": "next", "color": "#FEDCBA" }
                ],
                "destination": [
                    { "id": "nearest", "op": [1972], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "next", "op": [1973], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "##,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(RenderSnapshot::default()),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let next = Color::rgb(0xFE as f32 / 255.0, 0xDC as f32 / 255.0, 0xBA as f32 / 255.0);
    let nearest = Color::rgb(0x12 as f32 / 255.0, 0xAB as f32 / 255.0, 0x34 as f32 / 255.0);

    assert!(
        plan.commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Rect { color, .. } if *color == next))
    );
    assert!(
        !plan
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Rect { color, .. } if *color == nearest))
    );
}

#[test]
fn play_plan_passes_runtime_stagefile_to_skin_document() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": "-100", "op": [191], "dst": [{ "x": 0, "y": 0, "w": 40, "h": 20 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);
    let AppSceneSnapshot::Play(mut snapshot) = crate::sample::sample_play_scene() else {
        panic!("sample play scene");
    };
    snapshot.stagefile_background = true;
    snapshot.stagefile_image_size = Some(SkinImageSize { width: 400.0, height: 200.0 });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == SELECT_STAGE_TEXTURE
    )));
}

#[test]
fn play_plan_does_not_inject_backbmp_without_skin_destination() {
    let snapshot = RenderSnapshot { backbmp_background: true, ..Default::default() };
    let fallback_plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot.clone()));

    let document: SkinDocument = serde_json::from_str(r#"{ "type": 0 }"#).unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);
    let document_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    for plan in [&fallback_plan, &document_plan] {
        assert!(!plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == PLAY_BACKBMP_TEXTURE
        )));
    }
}

#[test]
fn play_plan_passes_runtime_backbmp_to_skin_document() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": "-101", "op": [195], "dst": [{ "x": 0, "y": 0, "w": 40, "h": 20 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);
    let snapshot = RenderSnapshot { backbmp_background: true, ..Default::default() };

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == PLAY_BACKBMP_TEXTURE
    )));
}

#[test]
fn play_plan_does_not_inject_bms_text() {
    let snapshot = RenderSnapshot { chart_text: "BMS #TEXT MESSAGE".into(), ..Default::default() };
    let fallback_plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot.clone()));

    let document: SkinDocument = serde_json::from_str(r#"{ "type": 0 }"#).unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);
    let document_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    for plan in [&fallback_plan, &document_plan] {
        assert!(!plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text == "BMS #TEXT MESSAGE"
        )));
    }
}

#[test]
fn result_plan_passes_runtime_stagefile_to_skin_document() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": "-100", "op": [191], "dst": [{ "x": 0, "y": 0, "w": 40, "h": 20 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    snapshot.stagefile_background = true;
    snapshot.stagefile_image_size = Some(SkinImageSize { width: 400.0, height: 200.0 });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Result(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == SELECT_STAGE_TEXTURE
    )));
}

#[test]
fn select_plan_renders_all_snapshot_rows() {
    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(crate::scene::SelectSnapshot {
        chart_count: 20,
        rows: select_rows(20),
        ..Default::default()
    }));

    let selected_row_color = Color::rgb(0.22, 0.28, 0.31);
    let row_color = Color::rgb(0.075, 0.09, 0.1);
    let row_count = plan
            .commands
            .iter()
            .filter(|command| matches!(
                command,
                DrawCommand::Rect { color, .. } if *color == selected_row_color || *color == row_color
            ))
            .count();
    assert_eq!(row_count, 20);
    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.contains("DIFFICULTY NORMAL") && text.contains("LEVEL 0")
        )));
}

#[test]
fn select_plan_renders_empty_row_when_no_rows_are_available() {
    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(Default::default()));

    let selected_row_color = Color::rgb(0.22, 0.28, 0.31);
    let row_count = plan
            .commands
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::Rect { color, .. } if *color == selected_row_color)
            })
            .count();
    assert_eq!(row_count, 1);
}

#[test]
fn result_plan_clamps_ex_score_bar() {
    let judge_counts = DisplayJudgeCounts::default();
    let fast_slow_counts = FastSlowJudgeCounts::default();
    let graph = ResultGraphSnapshot::default();
    let plan = plan_result_fallback(ResultFallbackSummary {
        clear_type: "Normal",
        ex_score: 0,
        ex_score_rate: 1.5,
        max_combo: 0,
        gauge_value: 0.0,
        total_notes: 100,
        judge_counts: &judge_counts,
        fast_slow_counts: &fast_slow_counts,
        graph: &graph,
        score_history_id: 1,
        replay_saved: true,
        difficulty_name: "",
        play_level: "",
        grade_diff: String::new(),
        ir: &crate::scene::ResultIrSnapshot::default(),
    });

    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.width == 0.72 && *color == Color::rgb(0.55, 0.78, 0.86)
        )));
}

#[test]
fn result_plan_includes_extended_summary_text() {
    let judge_counts = DisplayJudgeCounts::default();
    let fast_slow_counts = FastSlowJudgeCounts::default();
    let graph = ResultGraphSnapshot::default();
    let plan = plan_result_fallback(ResultFallbackSummary {
        clear_type: "Normal",
        ex_score: 1500,
        ex_score_rate: 0.75,
        max_combo: 500,
        gauge_value: 82.0,
        total_notes: 1000,
        judge_counts: &judge_counts,
        fast_slow_counts: &fast_slow_counts,
        graph: &graph,
        score_history_id: 42,
        replay_saved: true,
        difficulty_name: "HYPER",
        play_level: "10",
        grade_diff: "AA+56".to_string(),
        ir: &crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Loaded,
            rank: Some(3),
            total_player: Some(42),
            clear_rate: None,
            previous_rank: None,
            ..Default::default()
        },
    });

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { style, .. } if style.color == Color::rgb(0.72, 0.84, 0.86)
    )));
    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.contains("DIFFICULTY HYPER") && text.contains("LEVEL 10")
        )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, .. } if text.contains("GRADE AA+56")
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, .. } if text.contains("IR RANK 3/42")
    )));
    assert_eq!(format_percent(0.754), "75%");
}

#[test]
fn result_plan_includes_stat_detail_panels() {
    let judge_counts =
        DisplayJudgeCounts { pgreat: 12, great: 8, good: 4, bad: 2, poor: 1, empty_poor: 3 };
    let fast_slow_counts = FastSlowJudgeCounts {
        fast_pgreat: 7,
        slow_pgreat: 5,
        fast_great: 3,
        slow_great: 5,
        fast_good: 1,
        slow_good: 3,
        fast_bad: 1,
        slow_bad: 1,
        fast_poor: 0,
        slow_poor: 1,
        fast_empty_poor: 2,
        slow_empty_poor: 1,
    };
    let graph = ResultGraphSnapshot {
        timing_points: vec![
            ResultTimingPoint {
                time_ms: 100,
                delta_us: -12_000,
                judge: bmz_core::judge::Judge::Great,
            },
            ResultTimingPoint {
                time_ms: 200,
                delta_us: 8_000,
                judge: bmz_core::judge::Judge::PGreat,
            },
        ],
        judge_graph_density: vec![1, 3, 2],
        ..ResultGraphSnapshot::default()
    };

    let plan = plan_result_fallback(ResultFallbackSummary {
        clear_type: "Normal",
        ex_score: 1500,
        ex_score_rate: 0.75,
        max_combo: 500,
        gauge_value: 82.0,
        total_notes: 1000,
        judge_counts: &judge_counts,
        fast_slow_counts: &fast_slow_counts,
        graph: &graph,
        score_history_id: 42,
        replay_saved: true,
        difficulty_name: "HYPER",
        play_level: "10",
        grade_diff: "AA+56".to_string(),
        ir: &crate::scene::ResultIrSnapshot::default(),
    });

    for label in ["JUDGE DETAILS", "FAST/SLOW DETAILS", "TIMING DETAILS"] {
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text == label
        )));
    }
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, .. } if text.starts_with("AVG ")
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, .. } if text == "F 7  S 11"
    )));
}

#[test]
fn play_plan_includes_judge_line_gauge_and_combo_panel() {
    let snapshot = RenderSnapshot {
        combo: 1234,
        max_combo: 1234,
        ex_score: 2000,
        total_notes: 1200,
        past_notes: 900,
        gauge: 82.0,
        difficulty_name: "ANOTHER".to_string(),
        play_level: "12".to_string(),
        ..Default::default()
    };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == DEFAULT_JUDGE_LINE_TEXTURE && *tint == skin_image_tint(Lane::Key1)
    )));
    // デフォルトスキンではグルーブゲージを描画しない。
    assert!(!plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == DEFAULT_GAUGE_FRAME_TEXTURE
    )));
    assert!(!plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == DEFAULT_GAUGE_FILL_TEXTURE
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == DEFAULT_COMBO_PANEL_TEXTURE && *tint == Color::rgb(1.0, 1.0, 1.0)
    )));
    assert_eq!(
        plan.commands
            .iter()
            .filter(|command| matches!(
                command,
                DrawCommand::Image { texture, .. } if *texture == DEFAULT_COMBO_PANEL_TEXTURE
            ))
            .count(),
        9
    );
    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.x == 0.05 && rect.width == 0.11 && *color == Color::rgb(0.035, 0.04, 0.044)
        )));
    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.x == 0.05 && rect.y == 0.36 && *color == Color::rgb(0.032, 0.036, 0.04)
        )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == DEFAULT_KEY_EVEN_RECEPTOR_TEXTURE && *tint == skin_image_tint(Lane::Key2)
    )));
    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_SCRATCH_RECEPTOR_TEXTURE && *tint == skin_image_tint(Lane::Scratch)
        )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, style, .. }
            if text == "1234" && style.layer == TextLayer::Skin
    )));
    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.contains("DIFFICULTY ANOTHER") && text.contains("LEVEL 12")
        )));
}

#[test]
fn play_plan_uses_snapshot_2p_arrange_for_skin_imageset() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "arrange.png" }],
                "image": [
                    { "id": "normal", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "mirror", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 },
                    { "id": "random", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 }
                ],
                "imageset": [
                    { "id": "arrange", "ref": 43, "images": ["normal", "mirror", "random"] }
                ],
                "destination": [
                    { "id": "arrange", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: crate::skin::SkinTextureId(77),
        source_size: crate::skin::SkinImageSize { width: 30.0, height: 10.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let snapshot = RenderSnapshot {
        arrange: "NORMAL".to_string(),
        arrange_2p: "RANDOM".to_string(),
        ..Default::default()
    };

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, uv, .. }
            if *texture == TextureId(77) && (uv.x - 20.0 / 30.0).abs() < 0.001
    )));
}

#[test]
fn play_plan_uses_beatoraja_target_list_index_for_skin_imageset() {
    for ref_id in [41, 77] {
        let document_json =
                r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "target.png" }],
                "image": [
                    { "id": "target", "src": 1, "x": 0, "y": 0, "w": 10, "h": 110, "divy": 11, "len": 11, "ref": REF_ID }
                ],
                "destination": [
                    { "id": "target", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }] }
                ]
            }
            "#
                .replace("REF_ID", &ref_id.to_string());
        let document: crate::skin::SkinDocument = serde_json::from_str(&document_json).unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: crate::skin::SkinTextureId(78),
            source_size: crate::skin::SkinImageSize { width: 10.0, height: 110.0 },
        };
        let skin = SkinContext::from_manifest_and_document(
            SkinManifest::default(),
            document,
            [source_texture],
        );
        let snapshot = RenderSnapshot { target: "RANK_AAA".to_string(), ..Default::default() };

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        // beatoraja の11段階では AAA は 7 番目。BMZ の選択肢に A+/AA+/AAA+
        // がなくても、その分を詰めずに元の画像行を選ぶ。
        assert!(
            plan.commands.iter().any(|command| matches!(
                command,
                DrawCommand::Image { texture, uv, .. }
                    if *texture == TextureId(78) && (uv.y - 70.0 / 110.0).abs() < 0.001
            )),
            "target ref {ref_id} must select the AAA row"
        );
    }
}

#[test]
fn play_plan_uses_snapshot_extended_2p_arrange_for_ref_image() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "arrange.png" }],
                "image": [
                    { "id": "arrange", "src": 1, "x": 0, "y": 0, "w": 10, "h": 120, "divy": 12, "len": 12, "ref": 345 }
                ],
                "destination": [
                    { "id": "arrange", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: crate::skin::SkinTextureId(77),
        source_size: crate::skin::SkinImageSize { width: 10.0, height: 120.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let snapshot = RenderSnapshot {
        arrange: "NORMAL".to_string(),
        arrange_2p: "MF-RANDOM".to_string(),
        ..Default::default()
    };

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, uv, .. }
            if *texture == TextureId(77) && (uv.y - 110.0 / 120.0).abs() < 0.001
    )));
}

#[test]
fn play_plan_routes_recent_judge_text_through_default_skin() {
    let snapshot = RenderSnapshot {
        time: TimeUs(1_000_000),
        recent_judgements: vec![DisplayJudgement {
            lane: Lane::Key2,
            judge: Judge::PGreat,
            side: Some(TimingSide::Fast),
            text: "PGREAT FAST".to_string(),
            combo: 1,
            delta_us: -3_000,
            time: TimeUs(920_000),
            is_miss: false,
            timing_ms_suppressed: false,
        }],
        ..Default::default()
    };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, style, .. }
            if text == "PGREAT FAST" && style.layer == TextLayer::Skin
    )));
}

#[test]
fn play_plan_includes_judge_count_text() {
    let snapshot = RenderSnapshot {
        judge_counts: DisplayJudgeCounts {
            pgreat: 2,
            great: 1,
            good: 1,
            bad: 1,
            poor: 1,
            empty_poor: 3,
        },
        ..Default::default()
    };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { style, .. } if style.color == Color::rgb(0.66, 0.92, 0.98)
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { style, .. } if style.color == Color::rgb(0.96, 0.4, 0.44)
    )));
}

#[test]
fn play_plan_flashes_recent_judgement_lane() {
    let snapshot = RenderSnapshot {
        time: TimeUs(1_000_000),
        recent_judgements: vec![DisplayJudgement {
            lane: Lane::Key2,
            judge: Judge::PGreat,
            side: Some(TimingSide::Fast),
            text: "PGREAT FAST".to_string(),
            combo: 1,
            delta_us: -3_000,
            time: TimeUs(920_000),
            is_miss: false,
            timing_ms_suppressed: false,
        }],
        ..Default::default()
    };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color, .. } if *color == judge_flash_color("PGREAT FAST", 0.35)
    )));
}

#[test]
fn play_plan_includes_recent_judgement_history_panel() {
    let snapshot = RenderSnapshot {
        time: TimeUs(1_000_000),
        recent_judgements: vec![DisplayJudgement {
            lane: Lane::Key2,
            judge: Judge::EmptyPoor,
            side: Some(TimingSide::Slow),
            text: "EMPTY POOR SLOW".to_string(),
            combo: 0,
            delta_us: 50_000,
            time: TimeUs(980_000),
            is_miss: false,
            timing_ms_suppressed: false,
        }],
        ..Default::default()
    };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.x == 0.885 && rect.y == 0.17 && *color == Color::rgb(0.03, 0.035, 0.038)
        )));
}

#[test]
fn lane_flash_expires_old_judgements() {
    let snapshot = RenderSnapshot {
        time: TimeUs(1_000_000),
        recent_judgements: vec![DisplayJudgement {
            lane: Lane::Key2,
            judge: Judge::Bad,
            side: Some(TimingSide::Slow),
            text: "BAD SLOW".to_string(),
            combo: 0,
            delta_us: 88_000,
            time: TimeUs(700_000),
            is_miss: false,
            timing_ms_suppressed: false,
        }],
        ..Default::default()
    };

    assert_eq!(lane_flash_color(&snapshot, Lane::Key2), None);
}
