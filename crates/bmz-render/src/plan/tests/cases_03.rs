use super::*;

#[test]
fn lr2_battle_state_projects_live_opponent_without_a_target_score() {
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        serde_json::from_str("{}").unwrap(),
        [],
    );
    let snapshot = RenderSnapshot {
        ex_score: 456,
        gauge: 72.0,
        gauge_type: 3,
        target_ex_score: None,
        autoplay: true,
        opponent: Some(crate::snapshot::OpponentRenderSnapshot {
            ex_score: 789,
            gauge: 38.0,
            gauge_type: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    let state = crate::plan::play::build_play_skin_state(&snapshot, &skin, 100_000);
    assert_eq!(state.ex_score, 456);
    assert_eq!(state.rival_ex_score, Some(789));
    assert_eq!(state.target_ex_score, None);
    assert_eq!(state.opponent_gauge, Some(38.0));
    assert_eq!(state.opponent_gauge_type, Some(1));
}

#[test]
fn play_plan_keeps_bmz_split_scratch_judge_timer_after_recent_window_expires() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
            "type": 0,
            "w": 100,
            "h": 100,
            "source": [{"id": 1, "path": "marker.png"}],
            "image": [{"id": "marker", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
            "destination": [{
                "id": "marker",
                "timer": 19010,
                "op": [19030],
                "loop": -1,
                "dst": [
                    {"time": 0, "x": 0, "y": 0, "w": 10, "h": 10},
                    {"time": 2000}
                ]
            }]
        }"#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        [crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(88),
            source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
        }],
    );
    let mut runtime = crate::skin::DynamicTimerRuntime::default();
    let first = RenderSnapshot {
        time: TimeUs(100_000),
        play_elapsed_time: TimeUs(100_000),
        recent_judgements: vec![DisplayJudgement {
            lane: Lane::Scratch,
            judge: Judge::Great,
            side: Some(TimingSide::Fast),
            text: "GREAT FAST".to_string(),
            combo: 1,
            delta_us: -5_000,
            time: TimeUs(100_000),
            is_miss: false,
            timing_ms_suppressed: false,
        }],
        ..Default::default()
    };
    let first_plan =
        DrawPlan::from_scene_with_skin(&AppSceneSnapshot::Play(first), &skin, &mut runtime);
    assert!(first_plan.commands.iter().any(
        |command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(88))
    ));

    let after_recent_window = RenderSnapshot {
        time: TimeUs(1_200_000),
        play_elapsed_time: TimeUs(1_200_000),
        recent_judgements: Vec::new(),
        ..Default::default()
    };
    let persisted_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(after_recent_window),
        &skin,
        &mut runtime,
    );
    assert!(persisted_plan.commands.iter().any(
        |command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(88))
    ));
}

#[test]
fn play_skin_document_places_hit_timing_note_bottom_on_judge_line() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "note.png"}],
                "image": [{"id": "note", "src": 1, "x": 0, "y": 0, "w": 1, "h": 36}],
                "note": {
                    "note": ["note"],
                    "dst": [
                        { "x": 10, "y": 20, "w": 5, "h": 60 }
                    ]
                }
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(78),
        source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.0,
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
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(78)
                && approx_eq(rect.y + rect.height, 0.8)
                && approx_eq(rect.height, 0.36)
    )));
}

#[test]
fn play_skin_document_applies_notes_alpha_offset_before_note_fade() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [
                    {"id": 1, "path": "note.png"},
                    {"id": 2, "path": "panel.png"}
                ],
                "image": [
                    {"id": "note", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1},
                    {"id": "panel", "src": 2, "x": 0, "y": 0, "w": 1, "h": 1}
                ],
                "note": {
                    "note": ["note"],
                    "dst": [{"x": 10, "y": 20, "w": 5, "h": 60}]
                },
                "destination": [
                    {"id": "notes", "offset": 30},
                    {"id": "panel", "dst": [{"x": 0, "y": 0, "w": 1, "h": 1}]}
                ]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        [
            crate::skin::SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(78),
                source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
            },
            crate::skin::SkinDocumentTexture {
                source_id: "2".to_string(),
                texture: SkinTextureId(79),
                source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
            },
        ],
    );

    let mut snapshot = RenderSnapshot::default();
    snapshot
        .skin_offsets
        .set(30, crate::skin_offset::SkinOffsetValue { a: 128, ..Default::default() });
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 0.5,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot.clone()),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let note_alpha = plan.commands.iter().find_map(|command| match command {
        DrawCommand::Image { texture, tint, .. } if *texture == TextureId(78) => Some(tint.a),
        _ => None,
    });
    assert_eq!(note_alpha, Some(0.5));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == TextureId(79) && approx_eq(tint.a, 1.0)
    )));

    snapshot
        .skin_offsets
        .set(30, crate::skin_offset::SkinOffsetValue { a: -128, ..Default::default() });
    let negative_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let negative_alpha = negative_plan.commands.iter().find_map(|command| match command {
        DrawCommand::Image { texture, tint, .. } if *texture == TextureId(78) => Some(tint.a),
        _ => None,
    });
    assert_eq!(negative_alpha, Some((1.0 - 128.0 / 255.0) * 0.5));
}

#[test]
fn play_skin_document_applies_notes_alpha_to_long_mine_and_processed_fallbacks() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "note.png"}],
                "image": [{"id": "note", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "note": ["note"],
                    "dst": [{"x": 10, "y": 20, "w": 5, "h": 60}]
                },
                "destination": [{"id": "notes", "offset": 30}]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        [crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(78),
            source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
        }],
    );
    let mut snapshot = RenderSnapshot {
        mark_processed_note: true,
        show_ln_tail_cap: true,
        ..RenderSnapshot::default()
    };
    snapshot
        .skin_offsets
        .set(30, crate::skin_offset::SkinOffsetValue { a: -128, ..Default::default() });
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key1,
        mode: LongNoteMode::Ln,
        head_y: 0.2,
        tail_y: 0.7,
        alpha: 0.25,
        body_state: LongBodyState::Inactive,
    });
    snapshot.visible_mines[Lane::Key1.index()].push(VisibleMine {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 0.4,
        damage: 8.0,
    });
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 0.6,
        kind: NoteVisualKind::Tap,
        processed_judge: Some(Judge::PGreat),
    });

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let long_note_images = plan
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                DrawCommand::Image { texture, tint, .. }
                    if *texture == TextureId(78)
                        && approx_eq(tint.a, (127.0 / 255.0) * 0.25)
            )
        })
        .count();
    assert_eq!(long_note_images, 3);
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == DEFAULT_MINE_NOTE_TEXTURE
                && approx_eq(tint.a, (127.0 / 255.0) * 0.4)
    )));
    let processed_fallback_borders = plan.commands.iter().filter(|command| {
        matches!(
            command,
            DrawCommand::Rect { color, .. }
                if approx_eq(color.a, (127.0 / 255.0) * 0.6)
        )
    });
    assert_eq!(processed_fallback_borders.count(), 4);
}

#[test]
fn play_skin_all_offset_transforms_fallback_mine_sprite() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
            "type": 0,
            "w": 100,
            "h": 100,
            "note": {
                "id": "notes",
                "note": ["missing-note"],
                "size": [10],
                "dst": [{ "x": 10, "y": 20, "w": 30, "h": 60 }]
            }
        }"#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_mines[Lane::Key1.index()].push(VisibleMine {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.5,
        alpha: 1.0,
        damage: 8.0,
    });

    let base_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot.clone()),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let base = base_plan
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Image { texture, rect, .. } if *texture == DEFAULT_MINE_NOTE_TEXTURE => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("fallback mine sprite");

    snapshot.skin_offsets.set(
        10,
        crate::skin_offset::SkinOffsetValue { x: 10, y: 20, w: 50, h: -50, ..Default::default() },
    );
    let offset_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let transformed = offset_plan
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Image { texture, rect, .. } if *texture == DEFAULT_MINE_NOTE_TEXTURE => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("transformed fallback mine sprite");

    assert!(approx_eq(transformed.x, base.x * 1.5 + 0.1));
    assert!(approx_eq(transformed.y, base.y * 0.5 + 0.5 - 0.2));
    assert!(approx_eq(transformed.width, base.width * 1.5));
    assert!(approx_eq(transformed.height, base.height * 0.5));
}

#[test]
fn skin_lane_height_uses_document_note_area_for_lane_cover_offsets() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "note": {
                    "dst": [
                        { "x": 100, "y": 357, "w": 10, "h": 723 },
                        { "x": 110, "y": 357, "w": 10, "h": 723 },
                        { "x": 120, "y": 357, "w": 10, "h": 723 },
                        { "x": 130, "y": 357, "w": 10, "h": 723 },
                        { "x": 140, "y": 357, "w": 10, "h": 723 },
                        { "x": 150, "y": 357, "w": 10, "h": 723 },
                        { "x": 160, "y": 357, "w": 10, "h": 723 },
                        { "x": 170, "y": 357, "w": 10, "h": 723 }
                    ]
                }
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let skin = SkinContext::from_manifest_and_document(manifest, document, []);

    assert!(approx_eq(skin_lane_height_px(&skin, KeyMode::K7, 1080.0), 723.0));
}

#[test]
fn play_skin_lift_offsets_use_lane_height() {
    let lane_h = 723.0;

    assert_eq!(skin_lift_offset_px(0.3, lane_h), 217);
    assert_eq!(skin_lanecover_offset_px(0.5, 0.0, lane_h), -362);
    assert_eq!(skin_lanecover_offset_px(0.5, 0.25, lane_h), -362);
    assert!(approx_eq(lane_cover_bottom_progress(0.25, 0.0), 0.75));
    assert!(approx_eq(lane_cover_bottom_progress(0.25, 0.2), 0.6875));
    assert!(approx_eq(lane_cover_bottom_progress(0.9, 0.2), 0.0));
    assert_eq!(skin_hidden_cover_offset_px(0.3, 0.25, lane_h), 127);
}

#[test]
fn play_skin_ready_timer_starts_after_load_timers() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "loadstart": 500,
                "loadend": 3000,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [
                    {"id": "panel", "op": [80], "dst": [
                        {"time": 0, "x": 80, "y": 0, "w": 10, "h": 10}
                    ]},
                    {"id": "panel", "op": [81], "dst": [
                        {"time": 0, "x": 60, "y": 0, "w": 10, "h": 10}
                    ]},
                    {"id": "panel", "timer": 40, "dst": [
                        {"time": 0, "x": 0, "y": 0, "w": 10, "h": 10},
                        {"time": 1000, "x": 50, "y": 0, "w": 10, "h": 10}
                    ]}
                ]
            }"#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(99),
        source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let before_ready = RenderSnapshot {
        time: TimeUs(-1_000_000),
        play_elapsed_time: TimeUs(3_000_000),
        ready_elapsed_time: None,
        ..Default::default()
    };
    let after_ready = RenderSnapshot {
        time: TimeUs(-1_000_000),
        play_elapsed_time: TimeUs(4_000_000),
        ready_elapsed_time: Some(TimeUs(500_000)),
        resources_loaded: true,
        ..Default::default()
    };
    let seamless = RenderSnapshot {
        time: TimeUs(12_000_000),
        play_elapsed_time: TimeUs(4_000_000),
        ready_elapsed_time: None,
        seamless_play_entry: true,
        resources_loaded: true,
        ..Default::default()
    };

    let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
    let before_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(before_ready),
        &skin,
        &mut dynamic_timers,
    );
    let after_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(after_ready),
        &skin,
        &mut dynamic_timers,
    );
    let seamless_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(seamless),
        &skin,
        &mut dynamic_timers,
    );

    assert!(before_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.8)
    )));
    assert!(after_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.25)
    )));
    assert!(!after_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.8)
    )));
    assert!(seamless_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.6)
    )));
    assert!(!seamless_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && (approx_eq(rect.x, 0.8) || approx_eq(rect.x, 0.0))
    )));
}

#[test]
fn seamless_play_entry_does_not_restart_start_input_animation() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "input": 1000,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [{"id": "panel", "timer": 1, "loop": 1000, "dst": [
                    {"time": 0, "x": 0, "y": 0, "w": 10, "h": 10},
                    {"time": 1000, "x": 50}
                ]}]
            }"#,
    )
    .unwrap();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(99),
        source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
    };
    let skin = SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        [source_texture],
    );
    let normal = RenderSnapshot { play_elapsed_time: TimeUs(5_000_000), ..Default::default() };
    let seamless = RenderSnapshot {
        play_elapsed_time: TimeUs(5_000_000),
        seamless_play_entry: true,
        ..Default::default()
    };

    let normal_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(normal),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let seamless_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(seamless),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    assert!(normal_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.0)
    )));
    assert!(seamless_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.5)
    )));
}

#[test]
fn play_skin_stays_loading_after_load_delay_until_ready_timer_starts() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "loadstart": 500,
                "loadend": 3000,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [
                    {"id": "panel", "op": [80], "dst": [
                        {"time": 0, "x": 80, "y": 0, "w": 10, "h": 10}
                    ]},
                    {"id": "panel", "op": [81], "dst": [
                        {"time": 0, "x": 20, "y": 0, "w": 10, "h": 10}
                    ]}
                ]
            }"#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(99),
        source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let loaded_before_ready = RenderSnapshot {
        time: TimeUs(-1_000_000),
        play_elapsed_time: TimeUs(3_500_000),
        ready_elapsed_time: None,
        resources_loaded: true,
        ..Default::default()
    };

    let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(loaded_before_ready),
        &skin,
        &mut dynamic_timers,
    );

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.8)
    )));
    assert!(!plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == TextureId(99) && approx_eq(rect.x, 0.2)
    )));
}

#[test]
fn play_skin_untimed_intro_uses_scene_elapsed_without_loadend_offset() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "loadend": 3000,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [{"id": "panel", "loop": 1600, "dst": [
                    {"time": 1400, "x": 0, "y": 0, "w": 10, "h": 10, "a": 0},
                    {"time": 1600, "a": 255}
                ]}]
            }"#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(99),
        source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let before_intro = RenderSnapshot {
        time: TimeUs(-1_000_000),
        play_elapsed_time: TimeUs(0),
        ..Default::default()
    };
    let during_intro = RenderSnapshot {
        time: TimeUs(-1_000_000),
        play_elapsed_time: TimeUs(1_500_000),
        ..Default::default()
    };

    let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
    let before_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(before_intro),
        &skin,
        &mut dynamic_timers,
    );
    let during_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(during_intro),
        &skin,
        &mut dynamic_timers,
    );

    assert!(!before_plan.commands.iter().any(
        |command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(99))
    ));
    assert!(during_plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, tint, .. }
            if *texture == TextureId(99) && approx_eq(tint.a, 128.0 / 255.0)
    )));
}

#[test]
fn play_skin_play_timer_is_inactive_before_chart_start() {
    let document: crate::skin::SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [{"id": "panel", "timer": 41, "dst": [
                    {"time": 0, "x": 0, "y": 0, "w": 10, "h": 10}
                ]}]
            }"#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let source_texture = crate::skin::SkinDocumentTexture {
        source_id: "1".to_string(),
        texture: SkinTextureId(99),
        source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
    };
    let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
    let before_start = RenderSnapshot {
        time: TimeUs(-1),
        play_elapsed_time: TimeUs(500_000),
        ..Default::default()
    };
    let after_start = RenderSnapshot {
        time: TimeUs(0),
        play_elapsed_time: TimeUs(500_000),
        ..Default::default()
    };

    let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
    let before_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(before_start),
        &skin,
        &mut dynamic_timers,
    );
    let after_plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(after_start),
        &skin,
        &mut dynamic_timers,
    );

    assert!(!before_plan.commands.iter().any(
        |command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(99))
    ));
    assert!(after_plan.commands.iter().any(
        |command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(99))
    ));
}

#[test]
fn play_plan_maps_normalized_note_y_to_distinct_screen_positions() {
    let mut snapshot = RenderSnapshot::default();
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(1_000),
        y: 0.75,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(2_000),
        y: 0.25,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));
    let note_ys: Vec<f32> = plan
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { rect, texture, .. } if *texture == DEFAULT_NOTE_TEXTURE => {
                Some(rect.y)
            }
            _ => None,
        })
        .collect();

    assert!(note_ys.iter().any(|y| approx_eq(*y, 0.2255)));
    assert!(note_ys.iter().any(|y| approx_eq(*y, 0.6125)));
}

#[test]
fn play_plan_places_hit_timing_note_on_judge_line() {
    let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };

    assert!(approx_eq(note_rect_y(board, 0.0, 0.0) + NOTE_HEIGHT, judge_line_y(board, 0.0)));
}

#[test]
fn start_overlay_label_covers_opening_window() {
    assert_eq!(start_overlay_label(TimeUs(0)), Some("READY"));
    assert_eq!(start_overlay_label(TimeUs(999_999)), Some("READY"));
    assert_eq!(start_overlay_label(TimeUs(1_000_000)), Some("GO"));
    assert_eq!(start_overlay_label(TimeUs(1_599_999)), Some("GO"));
    assert_eq!(start_overlay_label(TimeUs(1_600_000)), None);
}

#[test]
fn play_plan_includes_ready_overlay_at_start() {
    let snapshot = RenderSnapshot { time: TimeUs(0), ..Default::default() };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { style, .. } if style.color == Color::rgb(0.74, 0.88, 0.9)
    )));
}

#[test]
fn default_play_plan_includes_failed_overlay() {
    let snapshot = RenderSnapshot { failed_elapsed_ms: Some(500), ..Default::default() };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, .. } if text == "FAILED"
    )));
    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color, .. } if color.a > 0.0
    )));
}

#[test]
fn play_plan_falls_back_to_black_fade_without_timer_two_destination() {
    let snapshot = RenderSnapshot { fadeout_elapsed_ms: Some(250), ..RenderSnapshot::default() };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect {
            rect: Rect { x, y, width, height },
            color: Color { r, g, b, a },
        } if approx_eq(*x, 0.0)
            && approx_eq(*y, 0.0)
            && approx_eq(*width, 1.0)
            && approx_eq(*height, 1.0)
            && approx_eq(*r, 0.0)
            && approx_eq(*g, 0.0)
            && approx_eq(*b, 0.0)
            && approx_eq(*a, 0.5)
    )));
}

#[test]
fn play_skin_timer_two_destination_disables_default_black_fade() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -110, "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100, "a": 0 },
                        { "time": 500, "a": 255 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);

    assert!(skin.has_timer_destination(2));

    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(RenderSnapshot {
            fadeout_elapsed_ms: Some(250),
            ..RenderSnapshot::default()
        }),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    let half_alpha_black_rects = plan
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                DrawCommand::Rect {
                    rect: Rect { x, y, width, height },
                    color: Color { r, g, b, a },
                } if approx_eq(*x, 0.0)
                    && approx_eq(*y, 0.0)
                    && approx_eq(*width, 1.0)
                    && approx_eq(*height, 1.0)
                    && approx_eq(*r, 0.0)
                    && approx_eq(*g, 0.0)
                    && approx_eq(*b, 0.0)
                    && approx_eq(*a, 0.5)
            )
        })
        .count();
    assert_eq!(half_alpha_black_rects, 0, "skin timer=2 should own the fadeout");
}

#[test]
fn select_plan_has_non_empty_commands() {
    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(Default::default()));

    assert!(!plan.commands.is_empty());
}

#[test]
fn decide_plan_activates_fadeout_timer_destinations() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 6,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -110, "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100, "a": 0 },
                        { "time": 200, "a": 255 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let skin = SkinContext::from_manifest_and_document(manifest, document, Vec::new());
    let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();

    let inactive = plan_decide(&RenderSnapshot::default(), &skin, &mut dynamic_timers);
    let active = plan_decide(
        &RenderSnapshot { fadeout_elapsed_ms: Some(100), ..RenderSnapshot::default() },
        &skin,
        &mut dynamic_timers,
    );

    assert!(!inactive.commands.iter().any(|command| {
        matches!(
            command,
            DrawCommand::Rect {
                rect: Rect { x, y, width, height },
                color: Color { r, g, b, a },
            } if approx_eq(*x, 0.0)
                && approx_eq(*y, 0.0)
                && approx_eq(*width, 1.0)
                && approx_eq(*height, 1.0)
                && approx_eq(*r, 0.0)
                && approx_eq(*g, 0.0)
                && approx_eq(*b, 0.0)
                && approx_eq(*a, 128.0 / 255.0)
        )
    }));
    assert!(active.commands.iter().any(|command| {
        matches!(
            command,
            DrawCommand::Rect {
                rect: Rect { x, y, width, height },
                color: Color { r, g, b, a },
            } if approx_eq(*x, 0.0)
                && approx_eq(*y, 0.0)
                && approx_eq(*width, 1.0)
                && approx_eq(*height, 1.0)
                && approx_eq(*r, 0.0)
                && approx_eq(*g, 0.0)
                && approx_eq(*b, 0.0)
                && approx_eq(*a, 128.0 / 255.0)
        )
    }));
}

#[test]
fn decide_plan_selects_loading_or_runtime_stagefile_destination() {
    let document: SkinDocument = serde_json::from_str(
        r##"
            {
                "type": 6,
                "w": 100,
                "h": 100,
                "panel": [
                    { "id": "loading", "color": "#123456" }
                ],
                "destination": [
                    { "id": "loading", "op": [190], "dst": [
                        { "x": 0, "y": 0, "w": 100, "h": 100 }
                    ] },
                    { "id": -100, "op": [191], "dst": [
                        { "x": 0, "y": 0, "w": 100, "h": 100 }
                    ] }
                ]
            }
            "##,
    )
    .unwrap();
    let skin =
        SkinContext::from_manifest_and_document(SkinManifest::default(), document, Vec::new());

    let without_stagefile = plan_decide(
        &RenderSnapshot::default(),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    assert!(without_stagefile.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color: Color { r, g, b, .. }, .. }
            if approx_eq(*r, 0x12 as f32 / 255.0)
                && approx_eq(*g, 0x34 as f32 / 255.0)
                && approx_eq(*b, 0x56 as f32 / 255.0)
    )));
    assert!(!without_stagefile.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == SELECT_STAGE_TEXTURE
    )));

    let with_stagefile = plan_decide(
        &RenderSnapshot {
            stagefile_background: true,
            stagefile_image_size: Some(SkinImageSize { width: 400.0, height: 200.0 }),
            ..RenderSnapshot::default()
        },
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    assert!(!with_stagefile.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color: Color { r, g, b, .. }, .. }
            if approx_eq(*r, 0x12 as f32 / 255.0)
                && approx_eq(*g, 0x34 as f32 / 255.0)
                && approx_eq(*b, 0x56 as f32 / 255.0)
    )));
    assert!(with_stagefile.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } if *texture == SELECT_STAGE_TEXTURE
    )));
}

#[test]
fn decide_plan_hides_non_course_destinations_during_course_mode() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 6,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -111, "dst": [
                        { "x": 0, "y": 0, "w": 50, "h": 100 }
                    ] },
                    { "id": -110, "op": [-290], "dst": [
                        { "x": 50, "y": 0, "w": 50, "h": 100 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();
    let skin =
        SkinContext::from_manifest_and_document(SkinManifest::default(), document, Vec::new());

    let normal = plan_decide(
        &RenderSnapshot::default(),
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    let course = plan_decide(
        &RenderSnapshot {
            course_stage: Some(crate::snapshot::CourseStageMarker::Stage1),
            ..RenderSnapshot::default()
        },
        &skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );

    let has_non_course_rect = |plan: &DrawPlan| {
        plan.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::Rect {
                    rect: Rect { x, width, .. },
                    color: Color { r, g, b, .. },
                } if approx_eq(*x, 0.5)
                    && approx_eq(*width, 0.5)
                    && approx_eq(*r, 0.0)
                    && approx_eq(*g, 0.0)
                    && approx_eq(*b, 0.0)
            )
        })
    };

    assert!(has_non_course_rect(&normal));
    assert!(!has_non_course_rect(&course));
}

#[test]
fn select_detail_panel_shows_gas_state() {
    let snapshot = crate::scene::SelectSnapshot {
        option_panel: 3,
        gauge_auto_shift: "BEST CLEAR".to_string(),
        ..Default::default()
    };

    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(snapshot));

    assert!(plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, .. } if text == "GAS      BEST CLEAR"
    )));
}

#[test]
fn custom_select_skin_does_not_force_stagefile_fullscreen_fallback() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [{ "id": "panel", "dst": [{ "x": 10, "y": 10, "w": 10, "h": 10 }] }]
            }
            "#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    let skin = SkinContext::from_manifest_and_document(
        manifest,
        document,
        [SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        }],
    );
    let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Select(crate::scene::SelectSnapshot {
            stage_background: true,
            stage_image_size: Some(SkinImageSize { width: 640.0, height: 480.0 }),
            ..Default::default()
        }),
        &skin,
        &mut dynamic_timers,
    );

    assert!(!plan.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, rect, .. }
            if *texture == SELECT_STAGE_TEXTURE
                && approx_eq(rect.x, 0.0)
                && approx_eq(rect.y, 0.0)
                && approx_eq(rect.width, 1.0)
                && approx_eq(rect.height, 1.0)
    )));
}
