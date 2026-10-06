use super::*;

#[test]
fn skin_level_number_extracts_digits_without_requiring_numeric_label() {
    assert_eq!(skin_level_number("12"), 12);
    assert_eq!(skin_level_number("LV 10+"), 10);
    assert_eq!(skin_level_number("no level"), 0);
}

#[test]
fn skin_difficulty_code_matches_numeric_and_case_insensitive_names() {
    assert_eq!(skin_difficulty_code("1"), 1);
    assert_eq!(skin_difficulty_code(" hyper "), 3);
    assert_eq!(skin_difficulty_code("ANOTHER"), 4);
    assert_eq!(skin_difficulty_code("unknown"), 0);
}

#[test]
fn play_skin_state_carries_frozen_rule_and_ln_score_policy() {
    let window = bmz_gameplay::judge::window::judge_windows_for_keymode_and_rule_mode(
        bmz_core::lane::KeyMode::K9,
        bmz_gameplay::rule::RuleMode::Dx,
    )
    .note;
    let snapshot = RenderSnapshot {
        has_long_notes: Some(true),
        rule_mode_index: 2,
        ln_score_policy_index: Some(4),
        note_judge_window: Some(window),
        ..RenderSnapshot::default()
    };

    let state = play::build_play_skin_state(&snapshot, &SkinContext::default(), 0);

    assert_eq!(state.rule_mode_index, 2);
    assert_eq!(state.note_judge_window, Some(window));
    assert_eq!(state.ln_score_policy_index, Some(4));
    assert_eq!(state.ln_policy_setting_index, None);
    assert_eq!(state.chart_has_long_notes, Some(true));
    assert!(crate::skin::test_skin_ops(&[173], &[], &state));
    assert!(!crate::skin::test_skin_ops(&[172], &[], &state));

    let no_ln_state = play::build_play_skin_state(
        &RenderSnapshot { has_long_notes: Some(false), ..RenderSnapshot::default() },
        &SkinContext::default(),
        0,
    );
    assert!(crate::skin::test_skin_ops(&[172], &[], &no_ln_state));
    assert!(!crate::skin::test_skin_ops(&[173], &[], &no_ln_state));
}

#[test]
fn lr2_judgetimer_limits_bomb_judgements() {
    assert!(judge_starts_bomb(Some(0), 1));
    assert!(judge_starts_bomb(Some(1), 1));
    assert!(!judge_starts_bomb(Some(2), 1));
    assert!(judge_starts_bomb(Some(3), 3));
    assert!(!judge_starts_bomb(None, 3));
}

#[test]
fn play_plan_renders_long_note_body() {
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key4,
        mode: bmz_chart::model::LongNoteMode::Ln,
        head_y: 0.1,
        tail_y: 0.7,
        alpha: 1.0,
        body_state: LongBodyState::Inactive,
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    // 胴体は LONG_NOTE_BODY_COLOR の Rect。終端(tail)が始端(head)より画面上方にある。
    let body = plan.commands.iter().find_map(|command| match command {
        DrawCommand::Rect { rect, color } if *color == LONG_NOTE_BODY_COLOR => Some(*rect),
        _ => None,
    });
    let body = body.expect("long note body rect should be present");
    assert!(body.height > NOTE_HEIGHT, "body should be taller than a tap note");
    let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };
    assert!(approx_eq(body.y, play_object_y(board, 0.0, 0.7)));
    assert!(approx_eq(body.y + body.height, play_object_y(board, 0.0, 0.1)));
}

#[test]
fn play_plan_colors_long_note_body_by_mode() {
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key4,
        mode: LongNoteMode::Cn,
        head_y: 0.1,
        tail_y: 0.7,
        alpha: 1.0,
        body_state: LongBodyState::Inactive,
    });
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key6,
        mode: LongNoteMode::Hcn,
        head_y: 0.1,
        tail_y: 0.7,
        alpha: 1.0,
        body_state: LongBodyState::Inactive,
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(
        |command| matches!(command, DrawCommand::Rect { color, .. } if *color == CN_BODY_COLOR)
    ));
    assert!(plan.commands.iter().any(
        |command| matches!(command, DrawCommand::Rect { color, .. } if *color == HCN_BODY_COLOR)
    ));
}

#[test]
fn play_plan_includes_lanes_notes_and_bar_lines() {
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });
    snapshot.bar_lines.push(VisibleBarLine {
        time: TimeUs(900),
        y: 0.25,
        alpha: 1.0,
        label: String::new(),
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.len() >= LANE_COUNT + 3);
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == DEFAULT_NOTE_TEXTURE && *tint == skin_image_tint(Lane::Key1)
    )));
}

#[test]
fn play_plan_applies_constant_fade_alpha_to_notes() {
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 0.25,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == DEFAULT_NOTE_TEXTURE && approx_eq(tint.a, 0.25)
    )));
}

#[test]
fn play_plan_renders_judge_area_and_processed_note_fallback() {
    let mut snapshot = RenderSnapshot {
        judge_area: true,
        judge_area_key_y: [0.02, 0.04, 0.08, 0.12, 0.2],
        judge_area_scratch_y: [0.03, 0.06, 0.1, 0.15, 0.25],
        mark_processed_note: true,
        ..RenderSnapshot::default()
    };
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: Some(Judge::PGreat),
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color, .. } if *color == Color::rgba(0.0, 0.0, 1.0, 0.125)
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color, .. } if *color == Color::rgb(0.0, 1.0, 1.0)
    )));
}

#[test]
fn play_plan_uses_note_textures_by_lane() {
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_notes[Lane::Scratch.index()].push(VisibleNote {
        lane: Lane::Scratch,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });
    snapshot.visible_notes[Lane::Key2.index()].push(VisibleNote {
        lane: Lane::Key2,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == DEFAULT_SCRATCH_NOTE_TEXTURE
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == DEFAULT_NOTE_TEXTURE
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == DEFAULT_KEY_EVEN_NOTE_TEXTURE
    )));
}

#[test]
fn result_plan_uses_skin_document_for_result_and_course_result_types() {
    use crate::scene::ResultSnapshot;
    use crate::skin::SkinTextureId;
    use crate::snapshot::{FastSlowJudgeCounts, ResultGaugeGraphPoint, ResultGraphSnapshot};
    use bmz_core::clear::ClearType;

    for skin_type in [7, 15] {
        let json = r#"{
                "type": __TYPE__,
                "name": "test",
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "x.png"}],
                "image": [{"id": "logo", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8}],
                "gaugegraph": [{"id": "graph"}],
                "destination": [
                    {"id": "logo", "dst": [{"x": 0, "y": 0, "w": 8, "h": 8}]},
                    {"id": "graph", "dst": [{"x": 0, "y": 8, "w": 100, "h": 80}]}
                ]
            }"#
        .replace("__TYPE__", &skin_type.to_string());
        let document: crate::skin::SkinDocument = serde_json::from_str(&json).unwrap();
        let manifest: SkinManifest = SkinManifest::default();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(99),
            source_size: crate::skin::SkinImageSize { width: 64.0, height: 64.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
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
            elapsed_time: TimeUs(1_500_000),
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
                        value: 20.0,
                        max: 100.0,
                        border: 80.0,
                        gauge_type: bmz_core::clear::GaugeType::Normal as i32,
                        ..Default::default()
                    },
                    ResultGaugeGraphPoint {
                        value: 80.0,
                        max: 100.0,
                        border: 80.0,
                        gauge_type: bmz_core::clear::GaugeType::Normal as i32,
                        ..Default::default()
                    },
                ],
                ..Default::default()
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
            DrawCommand::Image { texture, .. } if *texture == TextureId(99)
        )));
        assert!(
            plan.commands
                .iter()
                .any(|command| matches!(command, DrawCommand::RectBatch { cache: Some(_), .. }))
        );
    }
}

#[test]
fn result_snapshot_custom_offset_adjusts_destination_geometry_and_alpha() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "offset": 42, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 30, "h": 40, "a": 200 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        [SkinDocumentTexture {
            source_id: "src".to_string(),
            texture: SkinTextureId(99),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        }],
    );
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!()
    };
    snapshot
        .skin_offsets
        .set(42, crate::skin_offset::SkinOffsetValue { x: 6, y: 8, w: 10, h: 12, r: 0, a: -50 });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Result(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    let command = plan
            .commands
            .iter()
            .find(|command| {
                matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(99))
            })
            .expect("custom result destination should render");
    let DrawCommand::Image { rect, tint, .. } = command else { unreachable!() };
    assert!((rect.x - 0.11).abs() < 0.0001);
    assert!((rect.y - 0.26).abs() < 0.0001);
    assert!((rect.width - 0.4).abs() < 0.0001);
    assert!((rect.height - 0.52).abs() < 0.0001);
    assert!((tint.a - 150.0 / 255.0).abs() < 0.0001);
}

#[test]
fn course_result_plan_supplies_course_titles_to_skin_document() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
                "type": 15,
                "name": "test",
                "w": 100,
                "h": 100,
                "text": [{"id": "course1", "size": 10, "ref": 150}],
                "destination": [
                    {"id": "course1", "dst": [{"x": 0, "y": 0, "w": 100, "h": 10}]}
                ]
            }"#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        std::iter::empty(),
    );
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    snapshot.course_titles[0] = "Stage One".to_string();

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Result(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(
        plan.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Stage One")
        )
    );
}

#[test]
fn result_plan_supplies_result_judge_graph_data_to_skin_document() {
    use crate::scene::ResultSnapshot;
    use crate::snapshot::{FastSlowJudgeCounts, ResultGraphSnapshot, ResultJudgeGraphBucket};
    use bmz_core::clear::ClearType;

    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "name": "test",
                "w": 100,
                "h": 100,
                "judgegraph": [{"id": "jg", "type": 1, "backTexOff": 1, "noGap": 1}],
                "destination": [
                    {"id": "jg", "dst": [{"x": 0, "y": 0, "w": 100, "h": 50}]}
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
            judge_graph_density: vec![1, 3, 2],
            judge_graph_buckets: vec![ResultJudgeGraphBucket { values: [0, 0, 1, 0, 0, 0] }],
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

    assert!(plan.commands.iter().any(|command| {
        draw_command_has_rect_color(command, |Color { r, g, b, .. }| {
            (*r - 0.0).abs() < 0.01 && (*g - 1.0).abs() < 0.01 && (*b - 0.53).abs() < 0.01
        })
    }));
}

#[test]
fn result_skin_state_sets_beatoraja_result_timers() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    snapshot.elapsed_time = TimeUs(500_000);

    let pending = build_result_skin_draw_state(&snapshot, 1000);
    assert_eq!(pending.result_graph_begin_ms, Some(500));
    assert_eq!(pending.result_graph_end_ms, Some(500));
    assert_eq!(pending.result_update_score_ms, None);

    snapshot.elapsed_time = TimeUs(1_500_000);
    let active = build_result_skin_draw_state(&snapshot, 1000);
    assert_eq!(active.result_update_score_ms, Some(500));

    let immediate = build_result_skin_draw_state(&snapshot, 0);
    assert_eq!(immediate.result_update_score_ms, Some(1500));
}

#[test]
fn result_skin_state_maps_arrange_option() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    snapshot.arrange = "S-RANDOM-EX".to_string();
    snapshot.arrange_2p = "MF-RANDOM".to_string();
    snapshot.double_option = "FLIP".to_string();

    let state = build_result_skin_draw_state(&snapshot, 0);

    assert_eq!(state.select_arrange_index, 9);
    assert_eq!(state.select_arrange_2p_index, 2);
    assert_eq!(state.select_double_option_index, 1);
    assert_eq!(state.result_arrange_index, 9);
    assert_eq!(state.result_arrange_2p_index, 2);
    assert_eq!(state.select_extended_arrange_index, 9);
    assert_eq!(state.select_extended_arrange_2p_index, 11);
    assert_eq!(state.result_extended_arrange_index, 9);
    assert_eq!(state.result_extended_arrange_2p_index, 11);
}

#[test]
fn result_skin_state_maps_favorite_chart_as_two_state_index() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };

    let not_favorite = build_result_skin_draw_state(&snapshot, 0);
    assert_eq!(not_favorite.result_favorite_chart, Some(false));

    snapshot.favorite_chart = true;
    let favorite = build_result_skin_draw_state(&snapshot, 0);
    assert_eq!(favorite.result_favorite_chart, Some(true));
}

#[test]
fn result_skin_state_maps_random_lane_pattern() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    let mut pattern = (0..bmz_core::lane::LANE_COUNT as u8).collect::<Vec<_>>();
    pattern[bmz_core::lane::Lane::Key1.index()] = bmz_core::lane::Lane::Key7.index() as u8;
    snapshot.arrange = "RANDOM".to_string();
    snapshot.lane_shuffle_pattern = pattern;

    let state = build_result_skin_draw_state(&snapshot, 0);

    assert_eq!(state.result_arrange_index, 2);
    assert_eq!(state.random_lane_refs[0], 7);
}

#[test]
fn result_skin_state_maps_effective_long_note_state() {
    let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
        panic!("sample result scene");
    };
    snapshot.has_long_notes = true;
    snapshot.ln_mode_index = 2;
    snapshot.rule_mode_index = 1;
    snapshot.ln_score_policy_index = Some(5);
    let window = bmz_gameplay::judge::model::JudgeWindow::symmetric(
        8_333, 16_666, 58_333, 100_000, 500_000, 100_000, 0,
    );
    snapshot.note_judge_window = Some(window);

    let state = build_result_skin_draw_state(&snapshot, 0);

    assert_eq!(state.chart_has_long_notes, Some(true));
    assert_eq!(state.result_ln_mode_index, Some(2));
    assert_eq!(state.rule_mode_index, 1);
    assert_eq!(state.ln_score_policy_index, Some(5));
    assert_eq!(state.note_judge_window, Some(window));
}
