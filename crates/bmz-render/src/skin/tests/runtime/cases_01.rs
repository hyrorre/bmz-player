use super::*;

fn keylogger_input(sequence: u64, lane: Lane, kind: InputKind, time_us: i64) -> SkinRuntimeEvent {
    SkinRuntimeEvent {
        sequence,
        kind: SkinRuntimeEventKind::Input(InputEvent {
            lane,
            kind,
            time: TimeUs(time_us),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }),
    }
}

fn keylogger_judgement(
    sequence: u64,
    lane: Lane,
    judge: Judge,
    side: TimingSide,
    time_us: i64,
    affects_score: bool,
) -> SkinRuntimeEvent {
    SkinRuntimeEvent {
        sequence,
        kind: SkinRuntimeEventKind::Judgement(bmz_gameplay::judge::model::JudgementEvent {
            note_id: Some(NoteId(sequence as u32)),
            lane,
            judge,
            side,
            delta: TimeUs(0),
            time: TimeUs(time_us),
            affects_score,
        }),
    }
}

#[test]
fn keylogger_runtime_consumes_sequences_and_builds_nps_and_lane_counts() {
    let input = keylogger_input(10, Lane::Key1, InputKind::Press, 500_000);
    let judgement =
        keylogger_judgement(11, Lane::Key1, Judge::Great, TimingSide::Fast, 500_000, true);
    let mut runtime = KeyLoggerRuntime::default();
    runtime.ingest(&[input.clone(), judgement.clone()], KeyMode::K9, 500_000, 50_000);
    runtime.ingest(&[input, judgement], KeyMode::K9, 500_000, 50_000);
    let mut state = SkinDrawState::default();
    runtime.write_state(&mut state, 500);

    // PeacefulPlay は初回更新では計測開始時刻だけを記録し、表示値はまだ0。
    assert_eq!(state.keylogger_nps, 0);
    assert_eq!(state.keylogger_judge_counts[0], [0, 1, 0, 0]);
    assert_eq!(state.keylogger_fast_slow_counts[0], [0, 1, 0]);
    assert_eq!(state.keylogger_event_ms[0][0], Some(0));
    assert!(eval_skin_draw_condition("keylogger_judge(1,1,great)", &state));
    assert!(eval_skin_draw_condition("keylogger_fastslow(1,1,fast)", &state));
    assert!(!eval_skin_draw_condition("keylogger_judge(1,1,bad)", &state));
    let destination: SkinDestinationDef = serde_json::from_str(
        r#"{"id":"keylogger-note-1","timer_expr":"bmz:keylogger_event:1:1","dst":[]}"#,
    )
    .unwrap();
    assert_eq!(destination_timer_elapsed_ms(&destination, &state), Some(0));
    assert!(
        (keylogger_graph_value("bmz:keylogger_graph:judge:1:great", &state).unwrap() - 1.0).abs()
            < f32::EPSILON
    );

    runtime.ingest(&[], KeyMode::K9, 1_499_999, 50_000);
    runtime.write_state(&mut state, 1_499);
    assert_eq!(state.keylogger_nps, 0);

    runtime.ingest(&[], KeyMode::K9, 1_500_000, 50_000);
    runtime.write_state(&mut state, 1_500);
    assert_eq!(state.keylogger_nps, 1);

    // 1秒窓の内容は毎フレーム変わっても、表示は次の1秒更新まで保持する。
    runtime.ingest(&[], KeyMode::K9, 2_000_001, 50_000);
    runtime.write_state(&mut state, 2_000);
    assert_eq!(state.keylogger_nps, 1);
    runtime.ingest(&[], KeyMode::K9, 2_500_000, 50_000);
    runtime.write_state(&mut state, 2_500);
    assert_eq!(state.keylogger_nps, 0);

    let next_session_input = keylogger_input(0, Lane::Key2, InputKind::Press, 0);
    runtime.ingest(&[next_session_input], KeyMode::K9, 0, 50_000);
    runtime.write_state(&mut state, 0);

    assert_eq!(state.keylogger_nps, 0);
    assert_eq!(state.keylogger_judge_counts, [[0; 4]; LANE_COUNT]);
    assert_eq!(state.keylogger_event_ms[0], [None; 16]);
    assert_eq!(state.keylogger_event_ms[1][0], Some(0));
}

#[test]
fn keylogger_count_graph_counts_once_per_press_and_ignores_non_press_judgements() {
    let events = vec![
        keylogger_input(1, Lane::Key1, InputKind::Press, 1_000),
        keylogger_judgement(2, Lane::Key1, Judge::Bad, TimingSide::Fast, 1_000, true),
        keylogger_judgement(3, Lane::Key1, Judge::Bad, TimingSide::Slow, 1_000, true),
        keylogger_judgement(4, Lane::Key1, Judge::Great, TimingSide::Fast, 1_000, true),
        // 見逃し Poor は押下トランザクション外なので集計しない。
        keylogger_judgement(5, Lane::Key1, Judge::Poor, TimingSide::Slow, 2_000, true),
        keylogger_input(6, Lane::Key1, InputKind::Release, 3_000),
        // LN 解放判定も Release 後なので集計しない。
        keylogger_judgement(7, Lane::Key1, Judge::Bad, TimingSide::Slow, 3_000, true),
        keylogger_input(8, Lane::Key1, InputKind::Press, 4_000),
        keylogger_judgement(9, Lane::Key1, Judge::Bad, TimingSide::Fast, 4_000, true),
        // Traditional LN 開始判定は event_index=8 相当で、直前の候補も無効化する。
        keylogger_judgement(10, Lane::Key1, Judge::Great, TimingSide::Fast, 4_000, false),
        keylogger_input(11, Lane::Key2, InputKind::Press, 5_000),
        keylogger_judgement(12, Lane::Key2, Judge::EmptyPoor, TimingSide::Slow, 5_000, true),
    ];
    let mut runtime = KeyLoggerRuntime::default();
    runtime.ingest(&events, KeyMode::K9, 5_000, 50_000);
    let mut state = SkinDrawState::default();
    runtime.write_state(&mut state, 5);

    assert_eq!(state.keylogger_judge_counts[0], [0, 1, 0, 0]);
    assert_eq!(state.keylogger_fast_slow_counts[0], [0, 1, 0]);
    assert_eq!(state.keylogger_judge_counts[1], [0, 0, 0, 1]);
    assert_eq!(state.keylogger_fast_slow_counts[1], [0, 0, 1]);
    assert_eq!(state.keylogger_event_judge[0][0], 2);
    assert_eq!(state.keylogger_event_judge[0][1], 0);
    assert_eq!(state.keylogger_event_judge[1][0], 4);
}

#[test]
fn keylogger_chattering_uses_selected_threshold_and_two_second_timer() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"{
            "property": [{
                "category": "key_logger_threshold",
                "name": "Threshold",
                "item": [
                    {"name":"10ms","op":11640},
                    {"name":"50ms","op":11644}
                ],
                "def": "50ms"
            }]
        }"#,
    )
    .unwrap();
    assert_eq!(keylogger_chattering_threshold_us(Some(&document)), 50_000);
    document.user_selected_options = Some(vec![11640]);
    assert_eq!(keylogger_chattering_threshold_us(Some(&document)), 10_000);
    assert_eq!(keylogger_chattering_threshold_us(None), 50_000);

    let mut runtime = KeyLoggerRuntime::default();
    runtime.ingest(
        &[
            keylogger_input(1, Lane::Key1, InputKind::Press, 100_000),
            keylogger_input(2, Lane::Key1, InputKind::Press, 109_999),
        ],
        KeyMode::K9,
        109_999,
        10_000,
    );
    let mut state = SkinDrawState::default();
    runtime.write_state(&mut state, 109);
    assert_eq!(state.keylogger_chattering_ms[0], Some(0));
    assert_eq!(state.keylogger_chattering_ms[1], None);

    // 通常間隔の次押下では、発火中の警告を途中解除しない。
    runtime.ingest(
        &[keylogger_input(3, Lane::Key1, InputKind::Press, 200_000)],
        KeyMode::K9,
        200_000,
        10_000,
    );
    runtime.write_state(&mut state, 200);
    assert_eq!(state.keylogger_chattering_ms[0], Some(90));

    // 再び短時間で押すと2秒タイマーを再始動する。
    runtime.ingest(
        &[keylogger_input(4, Lane::Key1, InputKind::Press, 205_000)],
        KeyMode::K9,
        205_000,
        10_000,
    );
    runtime.write_state(&mut state, 205);
    let destination: SkinDestinationDef = serde_json::from_str(
        r#"{"id":"keylogger-chattering-alert-1","timer_expr":"bmz:keylogger_chattering:1","dst":[]}"#,
    )
    .unwrap();
    assert_eq!(destination_timer_elapsed_ms(&destination, &state), Some(0));

    runtime.ingest(&[], KeyMode::K9, 2_205_000, 10_000);
    runtime.write_state(&mut state, 2_205);
    assert_eq!(destination_timer_elapsed_ms(&destination, &state), Some(2_000));
    runtime.ingest(&[], KeyMode::K9, 2_205_001, 10_000);
    runtime.write_state(&mut state, 2_205);
    assert_eq!(destination_timer_elapsed_ms(&destination, &state), None);

    let mut boundary = KeyLoggerRuntime::default();
    boundary.ingest(
        &[
            keylogger_input(1, Lane::Key1, InputKind::Press, 100_000),
            keylogger_input(2, Lane::Key1, InputKind::Press, 110_000),
        ],
        KeyMode::K9,
        110_000,
        10_000,
    );
    boundary.write_state(&mut state, 110);
    assert_eq!(state.keylogger_chattering_ms[0], None);
}

#[test]
fn placement_uses_latest_animation_keyframe() {
    let placement = SkinPlacement {
        phase: SkinPhase::Play,
        time_ms: 0,
        rect: Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
        alpha: 1.0,
        blend: BlendMode::Normal,
        animation: Animation {
            keyframes: vec![
                Keyframe {
                    time_ms: 0,
                    rect: Rect { x: 0.1, y: 0.0, width: 0.1, height: 0.1 },
                    alpha: 1.0,
                },
                Keyframe {
                    time_ms: 100,
                    rect: Rect { x: 0.2, y: 0.0, width: 0.1, height: 0.1 },
                    alpha: 0.8,
                },
            ],
        },
    };

    assert_eq!(placement.resolve(120).rect.x, 0.2);
}

#[test]
fn judge_line_with_lift_offset_still_renders_at_minimum_lift() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "line.png" }],
                "image": [{ "id": "judge_line", "src": 12, "w": 431, "h": 8 }],
                "destination": [
                    { "id": "judge_line", "offset": 3, "dst": [{ "time": 0, "x": 20, "y": 357, "w": 431, "h": 8, "a": 255 }] }
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
            source_size: SkinImageSize { width: 431.0, height: 8.0 },
        },
    )]);

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() },
    );
    assert_eq!(items.len(), 1, "judge_line must not be skipped with liftcover skip logic");
}

#[test]
fn skin_document_evaluates_timer_draw_conditions() {
    assert!(eval_skin_draw_condition("timer(46) == timer_off", &SkinDrawState::default()));
    assert!(eval_skin_draw_condition(
        "timer(46) != timer_off",
        &SkinDrawState { judge_ms: judge_region_state(0, 120, 0).judge_ms, ..Default::default() }
    ));
    assert!(eval_skin_draw_condition(
        "timer(46) > 0 and option(197)",
        &SkinDrawState {
            judge_ms: judge_region_state(0, 120, 0).judge_ms,
            select_replay_slots: [true, false, false, false],
            ..Default::default()
        }
    ));
    let eon_shadow_draw = "timer(143) == timer_off and number(106)-number(110)-number(111)-number(112)-number(113)-number(114) == 0";
    assert!(eval_skin_draw_condition(
        eon_shadow_draw,
        &SkinDrawState {
            total_notes: 5,
            judge_counts: DisplayJudgeCounts { pgreat: 5, ..Default::default() },
            ..Default::default()
        }
    ));
    assert!(!eval_skin_draw_condition(
        eon_shadow_draw,
        &SkinDrawState {
            total_notes: 5,
            judge_counts: DisplayJudgeCounts { pgreat: 5, ..Default::default() },
            end_of_note_ms: Some(0),
            ..Default::default()
        }
    ));
    let ir_wait_draw = "timer(173) == timer_off and timer(174) == timer_off";
    assert!(eval_skin_draw_condition(ir_wait_draw, &SkinDrawState::default()));
    assert!(!eval_skin_draw_condition(
        ir_wait_draw,
        &SkinDrawState {
            ir_ranking: crate::scene::ResultIrSnapshot {
                connect_begin_ms: Some(500),
                connect_success_ms: Some(100),
                ..Default::default()
            },
            ..Default::default()
        }
    ));
}

#[test]
fn skin_document_applies_declared_14k_turntable_offsets_with_beatoraja_rotation() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 2,
                "w": 100,
                "h": 100,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "turntable", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    {
                        "id": "turntable",
                        "offset": 1,
                        "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }]
                    },
                    {
                        "id": "turntable",
                        "offset": 1,
                        "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }]
                    },
                    {
                        "id": "turntable",
                        "offset": 2,
                        "dst": [{ "x": 40, "y": 0, "w": 10, "h": 10 }]
                    }
                ]
            }
            "#,
    )
    .unwrap();
    let mut skin_offsets = SkinOffsetValues::default();
    skin_offsets.set(1, crate::skin_offset::SkinOffsetValue { r: 30, ..Default::default() });
    skin_offsets.set(2, crate::skin_offset::SkinOffsetValue { r: 70, ..Default::default() });
    let state = SkinDrawState { key_mode: KeyMode::K14, skin_offsets, ..SkinDrawState::default() };

    let angles = document
        .static_image_render_items(&mock_source("src", 10.0, 10.0), &state)
        .iter()
        .map(|item| match item {
            SkinRenderItem::RotatedImage { angle_deg, .. } => *angle_deg as i32,
            _ => panic!("turntable should be rotated"),
        })
        .collect::<Vec<_>>();

    assert_eq!(angles, vec![-30, -30, -70]);
}

#[test]
fn skin_document_samples_and_repeats_destination_keyframes_by_elapsed_time() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                        { "time": 100, "x": 30, "a": 128 },
                        { "time": 200, "x": 60, "w": 20 }
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
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    let early = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 50, ..SkinDrawState::default() },
    );
    let middle = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 150, ..SkinDrawState::default() },
    );
    let late = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 250, ..SkinDrawState::default() },
    );

    assert!(
        matches!(early[0], SkinRenderItem::Image { rect: Rect { x, width, .. }, tint: Color { a, .. }, .. }
                if approx_eq(x, 0.15) && approx_eq(width, 0.1) && approx_eq(a, 192.0 / 255.0))
    );
    assert!(
        matches!(middle[0], SkinRenderItem::Image { rect: Rect { x, width, .. }, tint: Color { a, .. }, .. }
                if approx_eq(x, 0.45) && approx_eq(width, 0.15) && approx_eq(a, 128.0 / 255.0))
    );
    assert!(
        matches!(late[0], SkinRenderItem::Image { rect: Rect { x, width, .. }, tint: Color { a, .. }, .. }
                if approx_eq(x, 0.15) && approx_eq(width, 0.1) && approx_eq(a, 192.0 / 255.0))
    );
}

#[test]
fn skin_document_loops_destination_keyframes() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "loop": 100, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                        { "time": 100, "x": 30 },
                        { "time": 200, "x": 60 }
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
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    // loop=100, 終端=200。elapsed=350 は終端超過なので [100, 200) 区間へループバック:
    // (350 - 100) % (200 - 100) + 100 = 150 → time 150 は keyframe 100(x=30)/200(x=60) の中間
    // x = 45 → 正規化 0.45
    let wrapped = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 350, ..SkinDrawState::default() },
    );

    assert!(matches!(wrapped[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                if approx_eq(x, 0.45)));
}

#[test]
fn play_destination_negative_image_id_renders_runtime_stagefile_source() {
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
    let context = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let state = SkinDrawState {
        has_stagefile: true,
        stagefile_image_size: Some(SkinImageSize { width: 400.0, height: 200.0 }),
        ..SkinDrawState::default()
    };

    let (behind, front, overlay) = context.static_document_play_items_split_for_state_and_text(
        &state,
        &SkinTextState::default(),
        &[],
        &[],
    );

    assert!(behind.iter().chain(&front).chain(&overlay).any(|item| matches!(
        item,
        SkinRenderItem::Image {
            texture,
            source_size: Some(SkinImageSize { width: 400.0, height: 200.0 }),
            ..
        } if *texture == SkinTextureId(SELECT_STAGE_TEXTURE.0)
    )));
}

#[test]
fn presentation_source_overrides_only_play_and_updates_dimensions() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
        "type":0,"w":100,"h":100,"destination":[
            {"id":"-100","op":[191],"dst":[{"x":0,"y":0,"w":40,"h":20}]}
        ]
    }"#,
    )
    .unwrap();
    let context = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    for (play, width, texture) in [
        (true, 32.0, crate::plan::PLAY_PRESENTATION_TEXTURE.0),
        (true, 64.0, crate::plan::PLAY_PRESENTATION_TEXTURE.0),
        (false, 400.0, SELECT_STAGE_TEXTURE.0),
    ] {
        let state = SkinDrawState {
            play_screen: play,
            has_stagefile: true,
            stagefile_image_size: (!play).then_some(SkinImageSize { width: 400.0, height: 200.0 }),
            stagefile_override: Some(SkinBgaFrame::opaque(
                SkinTextureId(crate::plan::PLAY_PRESENTATION_TEXTURE.0),
                SkinImageSize { width, height: 16.0 },
            )),
            ..SkinDrawState::default()
        };
        let (behind, front, overlay) = context.static_document_play_items_split_for_state_and_text(
            &state,
            &SkinTextState::default(),
            &[],
            &[],
        );
        assert!(behind.iter().chain(&front).chain(&overlay).any(|item| matches!(
            item, SkinRenderItem::Image { texture: actual, source_size: Some(size), .. }
                if actual.0 == texture && size.width == width
        )));
    }
}

#[test]
fn skin_document_resolves_end_of_note_timer_destinations() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "marker", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "marker", "timer": 143, "dst": [{ "x": 10, "y": 20, "w": 5, "h": 6 }] }
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

    let hidden = document.static_image_render_items(
        &sources,
        &SkinDrawState { end_of_note: false, ..SkinDrawState::default() },
    );
    let visible = document.static_image_render_items(
        &sources,
        &SkinDrawState { end_of_note: true, end_of_note_ms: Some(0), ..SkinDrawState::default() },
    );

    assert!(hidden.is_empty());
    assert_eq!(visible.len(), 1);
    assert!(matches!(visible[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(x, 0.1)
                && approx_eq(y, 0.74)
                && approx_eq(width, 0.05)
                && approx_eq(height, 0.06)));
}

#[test]
fn skin_document_resolves_full_combo_timer_destinations() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "fc", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "fc", "timer": 48, "loop": -1, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 5, "h": 6, "a": 255 },
                        { "time": 1000, "a": 0 }
                    ] }
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

    let hidden = document.static_image_render_items(
        &sources,
        &SkinDrawState { full_combo_ms: None, ..SkinDrawState::default() },
    );
    let visible = document.static_image_render_items(
        &sources,
        &SkinDrawState { full_combo_ms: Some(500), ..SkinDrawState::default() },
    );

    assert!(hidden.is_empty());
    assert_eq!(visible.len(), 1);
    assert!(matches!(visible[0], SkinRenderItem::Image {
                tint: Color { a, .. },
                ..
            } if approx_eq(a, 128.0 / 255.0)));
}

#[test]
fn skin_context_reports_timer_animation_duration() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "fc", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "fc", "timer": 48, "loop": -1, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 5, "h": 6 },
                        { "time": 1966, "a": 0 }
                    ] },
                    { "id": "other", "timer": 2, "dst": [{ "time": 3000 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let context =
        SkinContext::from_manifest_and_document(default_skin_manifest(), document, Vec::new());

    assert_eq!(context.timer_animation_duration_ms(48), 1966);
    assert_eq!(context.timer_animation_duration_ms(49), 0);
}

#[test]
fn skin_document_resolves_fadeout_timer_destinations() {
    // timer=2 (TIMER_FADEOUT) はシーン終了アニメーション用。
    // fadeout_ms=None なら非アクティブで描画されず、Some なら経過 ms で
    // keyframe アニメーションが進行する。
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "curtain", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "curtain", "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 0 },
                        { "time": 200, "x": 0, "y": 0, "w": 100, "h": 100 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(7),
            source_size: SkinImageSize { width: 100.0, height: 100.0 },
        },
    )]);

    let inactive = document.static_image_render_items(
        &sources,
        &SkinDrawState { fadeout_ms: None, ..SkinDrawState::default() },
    );
    let mid = document.static_image_render_items(
        &sources,
        &SkinDrawState { fadeout_ms: Some(100), ..SkinDrawState::default() },
    );

    assert!(inactive.is_empty(), "fadeout timer is inactive when fadeout_ms is None");
    assert_eq!(mid.len(), 1);
    assert!(matches!(mid[0], SkinRenderItem::Image {
                rect: Rect { height, .. },
                ..
            } if approx_eq(height, 0.5)));
}

#[test]
fn skin_document_resolves_failed_timer_destinations() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -111, "timer": 3, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100, "a": 0 },
                        { "time": 100, "a": 255 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();

    let inactive = document.static_image_render_items(
        &HashMap::new(),
        &SkinDrawState { failed_ms: None, ..SkinDrawState::default() },
    );
    let active = document.static_image_render_items(
        &HashMap::new(),
        &SkinDrawState { failed_ms: Some(50), ..SkinDrawState::default() },
    );

    assert!(inactive.is_empty());
    assert_eq!(active.len(), 1);
    assert!(matches!(active[0], SkinRenderItem::Rect {
                color: Color { r, g, b, a },
                ..
            } if approx_eq(r, 1.0)
                && approx_eq(g, 1.0)
                && approx_eq(b, 1.0)
                && approx_eq(a, 128.0 / 255.0)));
}

#[test]
fn lift_cover_schema_applies_lift_offset_once() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "lift.png" }],
                "liftCover": [
                    { "id": "lift", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357 }
                ],
                "destination": [
                    { "id": "lift", "dst": [{ "x": 20, "y": -366, "w": 431, "h": 723 }] }
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

    let hidden = document.static_image_render_items(
        &sources,
        &SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() },
    );
    assert!(hidden.is_empty());

    let lifted = document.static_image_render_items(
        &sources,
        &SkinDrawState { offset_lift_px: 200, ..SkinDrawState::default() },
    );
    let SkinRenderItem::Image { rect, uv, .. } = &lifted[0] else {
        panic!("expected lift cover image");
    };
    assert!(approx_eq(rect.height, 200.0 / 720.0));
    assert!(approx_eq(uv.height, 200.0 / 723.0));
}

#[test]
fn hidden_cover_destination_applies_lift_and_hidden_offsets() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 12, "path": "cover.png" }],
                "hiddenCover": [
                    { "id": "hidden-cover", "src": 12, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "hidden-cover", "dst": [{ "x": 20, "y": -40, "w": 30, "h": 40 }] }
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

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            hidden_cover: 0.5,
            offset_lift_px: 10,
            offset_hidden_cover_px: 20,
            ..SkinDrawState::default()
        },
    );

    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
    assert!(
        approx_eq(rect.y, (100 - (-40 + 10 + 20) - 40) as f32 / 100.0),
        "expected hidden cover to use automatic lift and hidden offsets, got {}",
        rect.y
    );
}

#[test]
fn display_signed_number_digits_uses_sign_cell_and_row_offset() {
    // divx=12, divy=2 のレイアウト想定
    // beatoraja の digit=5 は符号を含むため、数値部分は4枠。
    let positive = display_signed_number_digits(12, 5, NumberPadding::Zero, 12);
    assert_eq!(positive, vec![11, 0, 0, 1, 2]);
    assert!(positive.iter().all(|&d| d < 12), "positive digits should be in row 0");

    // 負数 -12 (max_digits=5): row 1 (offset=12)
    let negative = display_signed_number_digits(-12, 5, NumberPadding::Zero, 12);
    assert_eq!(negative, vec![23, 12, 12, 13, 14]);
    assert!(negative.iter().all(|&d| d >= 12), "negative digits should be in row 1");

    // 0 は正側
    let zero = display_signed_number_digits(0, 5, NumberPadding::Zero, 12);
    assert_eq!(zero, vec![11, 0, 0, 0, 0]);
    assert!(zero.iter().all(|&d| d < 12));

    // WMII IR差分: digit=5, zeropadding=4 は符号1枠 + 数値4枠。
    assert_eq!(
        display_signed_number_digits(2284, 5, NumberPadding::Zero, 12),
        vec![11, 2, 2, 8, 4]
    );
    assert_eq!(
        display_signed_number_digits(-9, 5, NumberPadding::Zero, 12),
        vec![23, 12, 12, 12, 21]
    );

    // LR2の符号付き数値は省略されたzeropaddingを2として扱い、符号を先頭に固定する。
    assert_eq!(display_signed_number_digits(9, 3, NumberPadding::Blank, 12), vec![11, 10, 9]);
    assert_eq!(display_signed_number_digits(-9, 3, NumberPadding::Blank, 12), vec![23, 22, 21]);
    assert_eq!(display_signed_number_digits(13, 3, NumberPadding::Blank, 12), vec![11, 1, 3]);
    assert_eq!(display_signed_number_digits(-13, 3, NumberPadding::Blank, 12), vec![23, 13, 15]);

    // ゼロ埋めなしで全枠が数値に埋まる場合、beatoraja同様に符号枠は出ない。
    assert_eq!(
        display_signed_number_digits(12_345, 5, NumberPadding::None, 12),
        vec![1, 2, 3, 4, 5]
    );

    // ref=154 is positive; signed mimage helpers remain available for other refs.
    assert_eq!(display_signed_number_digits(-34, 4, NumberPadding::None, 12), vec![23, 15, 16]);
    assert!(!ref_id_is_signed(154));
    assert_eq!(display_signed_number_digits(34, 4, NumberPadding::None, 12), vec![11, 3, 4]);
    assert_eq!(display_signed_number_digits(0, 4, NumberPadding::None, 12), vec![11, 0]);
    assert_eq!(
        display_signed_number_digits_with_row_order(
            -34,
            4,
            NumberPadding::None,
            12,
            SignedNumberRowOrder::NegativeFirst
        ),
        vec![11, 3, 4]
    );
    assert_eq!(
        display_signed_number_digits_with_row_order(
            0,
            4,
            NumberPadding::None,
            12,
            SignedNumberRowOrder::NegativeFirst
        ),
        vec![23, 12]
    );

    let score_diff_value = SkinValueDef {
        id: "score_diff_mybest".to_string(),
        src: "num".to_string(),
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        divx: 12,
        divy: 2,
        timer: None,
        cycle: 0,
        align: 0,
        judge_align: None,
        digit: 5,
        padding: 0,
        zeropadding: 1,
        space: 0,
        ref_id: 152,
        expr: String::new(),
        value_expr: String::new(),
        offset: Vec::new(),
    };
    let score_diff_padding = number_padding(&score_diff_value);
    assert!(score_diff_padding.is_zero_padding());
    assert_eq!(signed_value_padding(&score_diff_value, score_diff_padding), NumberPadding::Zero);
    assert_eq!(display_signed_number_digits(16, 5, NumberPadding::Zero, 12), vec![11, 0, 0, 1, 6]);

    let select_detail =
        SkinDrawState { select_screen: true, select_option_panel: 3, ..Default::default() };
    let select_normal = SkinDrawState { select_screen: true, ..Default::default() };
    assert!(value_ref_is_signed_for_state(12, &select_detail));
    assert!(!value_ref_is_signed_for_state(12, &select_normal));

    // mz-select は `MAX-` 等を別textで描き、11セル数値は絶対値だけを使う。
    let unsigned_next_diff = SkinValueDef {
        id: "default_playerdata_diff_count".to_string(),
        divx: 11,
        divy: 1,
        digit: 4,
        ref_id: 154,
        ..SkinValueDef::default()
    };
    assert_eq!(number_padding(&unsigned_next_diff), NumberPadding::Blank);
    assert_eq!(
        signed_number_render_for_value(&unsigned_next_diff, &select_normal),
        SignedNumberRender::Unsigned
    );
    assert_eq!(
        display_number_digits(-344, 4, number_padding(&unsigned_next_diff)),
        vec![10, 3, 4, 4]
    );
}
