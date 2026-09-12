use super::*;

#[test]
fn bundled_default_skin_manifest_defines_play_lane_images() {
    let manifest = default_skin_manifest();
    let note = manifest.play_note_image();
    let receptor = manifest.play_receptor_image();
    let judge_line = manifest.play_judge_line_image();
    let gauge_frame = manifest.play_gauge_frame_image();
    let gauge_fill = manifest.play_gauge_fill_image();
    let combo_panel = manifest.play_combo_panel_image(true);
    let combo_panel_inactive = manifest.play_combo_panel_image(false);

    assert_eq!(note.texture, 1);
    assert_eq!(note.texture_for_lane(Lane::Key1), 1);
    assert_eq!(note.texture_for_lane(Lane::Key2), 2);
    assert_eq!(note.texture_for_lane(Lane::Key4), 2);
    assert_eq!(note.texture_for_lane(Lane::Key6), 2);
    assert_eq!(note.texture_for_lane(Lane::Scratch), 3);
    assert_eq!(note.uv, TextureRegion::default());
    assert_eq!(receptor.texture, 4);
    assert_eq!(receptor.texture_for_lane(Lane::Key1), 4);
    assert_eq!(receptor.texture_for_lane(Lane::Key2), 5);
    assert_eq!(receptor.texture_for_lane(Lane::Key4), 5);
    assert_eq!(receptor.texture_for_lane(Lane::Key6), 5);
    assert_eq!(receptor.texture_for_lane(Lane::Scratch), 6);
    assert_eq!(receptor.uv, TextureRegion::default());
    assert_eq!(judge_line.texture, 7);
    assert_eq!(judge_line.uv, TextureRegion::default());
    assert_eq!(gauge_frame.texture, 8);
    assert_eq!(gauge_frame.scale, SkinImageScale::NineSlice);
    assert_eq!(gauge_frame.source_size, Some(SkinImageSize { width: 12.0, height: 48.0 }));
    assert!(matches!(
        gauge_frame.border,
        Some(SkinImageBorder { unit: SkinImageBorderUnit::Pixels, .. })
    ));
    assert_eq!(gauge_fill.texture, 9);
    assert_eq!(gauge_fill.source_size, Some(SkinImageSize { width: 8.0, height: 48.0 }));
    assert_eq!(combo_panel.texture, 10);
    assert_eq!(combo_panel.scale, SkinImageScale::NineSlice);
    assert_eq!(combo_panel.source_size, Some(SkinImageSize { width: 48.0, height: 16.0 }));
    assert!(matches!(
        combo_panel.border,
        Some(SkinImageBorder { unit: SkinImageBorderUnit::Pixels, .. })
    ));
    assert_eq!(combo_panel_inactive.texture, 11);
    assert_eq!(combo_panel_inactive.scale, SkinImageScale::NineSlice);
    assert_eq!(combo_panel_inactive.source_size, Some(SkinImageSize { width: 48.0, height: 16.0 }));
    assert!(matches!(
        combo_panel_inactive.border,
        Some(SkinImageBorder { unit: SkinImageBorderUnit::Pixels, .. })
    ));
}

#[test]
fn bga_destination_renders_placeholder_only_when_chart_has_bga() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40, "a": 128 }] }
                ]
            }
            "#,
    )
    .unwrap();

    let no_bga_items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState { has_bga: false, ..SkinDrawState::default() },
        &SkinTextState::default(),
    );
    let bga_items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState { has_bga: true, ..SkinDrawState::default() },
        &SkinTextState::default(),
    );

    assert!(no_bga_items.is_empty());
    assert!(matches!(
        bga_items.as_slice(),
        [SkinRenderItem::Rect {
            rect: Rect { x, y, width, height },
            color: Color { r: 0.0, g: 0.0, b: 0.0, a },
            ..
        }] if approx_eq(*x, 0.1)
            && approx_eq(*y, 0.4)
            && approx_eq(*width, 0.3)
            && approx_eq(*height, 0.4)
            && approx_eq(*a, 128.0 / 255.0)
    ));
}

#[test]
fn bga_destination_is_hidden_when_bga_is_disabled() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
    )
    .unwrap();

    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState {
            has_bga: true,
            bga_enabled: false,
            bga_base: Some(SkinBgaFrame {
                texture: SkinTextureId(20000),
                source_size: SkinImageSize { width: 256.0, height: 256.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            ..SkinDrawState::default()
        },
        &SkinTextState::default(),
    );

    assert!(items.is_empty());
}

#[test]
fn bga_option_conditions_still_reflect_song_bga_when_disabled() {
    let disabled = SkinDrawState { has_bga: true, bga_enabled: false, ..SkinDrawState::default() };

    assert!(test_skin_op(40, &[], &disabled));
    assert!(!test_skin_op(41, &[], &disabled));
    assert!(!test_skin_op(170, &[], &disabled));
    assert!(test_skin_op(171, &[], &disabled));

    let enabled_no_song_bga =
        SkinDrawState { has_bga: false, bga_enabled: true, ..SkinDrawState::default() };
    assert!(!test_skin_op(40, &[], &enabled_no_song_bga));
    assert!(test_skin_op(41, &[], &enabled_no_song_bga));
    assert!(test_skin_op(170, &[], &enabled_no_song_bga));
    assert!(!test_skin_op(171, &[], &enabled_no_song_bga));
}

#[test]
fn score_save_and_play_mode_ops_are_scene_scoped() {
    let select = SkinDrawState::default();
    let normal = SkinDrawState {
        play_screen: true,
        score_save_enabled: Some(true),
        ..SkinDrawState::default()
    };
    let replay = SkinDrawState {
        play_screen: true,
        replay_playback: true,
        score_save_enabled: Some(false),
        ..SkinDrawState::default()
    };
    let practice = SkinDrawState {
        play_screen: true,
        practice_mode: true,
        score_save_enabled: Some(false),
        ..SkinDrawState::default()
    };

    assert!(!test_skin_op(60, &[], &select));
    assert!(!test_skin_op(61, &[], &select));
    assert!(!test_skin_op(82, &[], &select));
    assert!(test_skin_op(61, &[], &normal));
    assert!(test_skin_op(82, &[], &normal));
    assert!(!test_skin_op(84, &[], &normal));
    assert!(test_skin_op(60, &[], &replay));
    assert!(!test_skin_op(82, &[], &replay));
    assert!(test_skin_op(84, &[], &replay));
    assert!(test_skin_op(60, &[], &practice));
    assert!(test_skin_op(82, &[], &practice));
    assert!(test_skin_op(1080, &[], &practice));
}

#[test]
fn play_asset_and_loading_ops_reflect_skin_state() {
    let unloaded = SkinDrawState { skin_loaded: false, ..SkinDrawState::default() };
    assert!(test_skin_op(80, &[], &unloaded));
    assert!(!test_skin_op(81, &[], &unloaded));

    let loaded = SkinDrawState::default();
    assert!(!test_skin_op(80, &[], &loaded));
    assert!(test_skin_op(81, &[], &loaded));
    assert!(test_skin_op(190, &[], &loaded));
    assert!(!test_skin_op(191, &[], &loaded));
    assert!(test_skin_op(194, &[], &loaded));
    assert!(!test_skin_op(195, &[], &loaded));

    let with_stagefile = SkinDrawState { has_stagefile: true, ..SkinDrawState::default() };
    assert!(!test_skin_op(190, &[], &with_stagefile));
    assert!(test_skin_op(191, &[], &with_stagefile));

    let with_backbmp = SkinDrawState { has_backbmp: true, ..SkinDrawState::default() };
    assert!(!test_skin_op(194, &[], &with_backbmp));
    assert!(test_skin_op(195, &[], &with_backbmp));
}

#[test]
fn lane_cover_changing_op_is_true_while_lane_cover_is_visible() {
    assert!(!test_skin_op(270, &[], &SkinDrawState::default()));
    assert!(!test_skin_op(
        270,
        &[],
        &SkinDrawState { lane_cover: 0.2, ..SkinDrawState::default() }
    ));
    assert!(test_skin_op(
        270,
        &[],
        &SkinDrawState { lane_cover_changing: true, ..SkinDrawState::default() }
    ));
    assert!(test_skin_op(
        271,
        &[],
        &SkinDrawState { lanecover_enabled: true, ..SkinDrawState::default() }
    ));
}

#[test]
fn play_key_mode_ops_use_play_key_mode() {
    let play_14k = SkinDrawState { key_mode: KeyMode::K14, ..SkinDrawState::default() };

    assert!(test_skin_op(162, &[], &play_14k));
    assert!(!test_skin_op(160, &[], &play_14k));

    let play_6k = SkinDrawState { key_mode: KeyMode::K6, ..SkinDrawState::default() };
    assert!(test_skin_op(SKIN_OPTION_BMZ_KEY_MODE_BASE + 2, &[], &play_6k));
    assert!(test_skin_op(SKIN_OPTION_BMZ_NO_SCRATCH, &[], &play_6k));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_KEY_MODE, &play_6k), Some(6));
}

#[test]
fn play_attempt_refs_use_frozen_source_and_effective_state() {
    let source_ln_bits = SKIN_SOURCE_LN_UNDEFINED_BIT | SKIN_SOURCE_LN_DEFINED_CN_BIT;
    let state = SkinDrawState {
        play_screen: true,
        chart_has_long_notes: Some(true),
        min_bpm: 150.0,
        max_bpm: 150.0,
        skin_attempt: SkinAttemptState {
            source_key_mode: Some(KeyMode::K7),
            effective_key_mode: Some(KeyMode::K6),
            seven_to_six: true,
            seven_to_nine_pattern: 5,
            seven_to_nine_type: 2,
            source_ln_profile_bits: Some(source_ln_bits),
            session_mode_index: Some(3),
            double_option_index: Some(1),
            hsfix_index: Some(4),
            gauge_auto_shift_index: Some(2),
            bottom_shiftable_gauge_index: Some(1),
            judge_algorithm_index: Some(2),
            ln_mode_index: Some(2),
            has_bga: Some(false),
            has_random_sequence: Some(true),
        },
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(SKIN_REF_BMZ_KEY_MODE, &state), Some(6));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SOURCE_KEY_MODE, &state), Some(7));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SOURCE_LN_PROFILE, &state), Some(5));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SELECT_SESSION_MODE, &state), Some(3));
    assert_eq!(skin_state_number(54, &state), Some(1));
    assert_eq!(skin_state_number(55, &state), Some(4));
    assert_eq!(skin_state_number(78, &state), Some(2));
    assert_eq!(skin_state_number(308, &state), Some(2));
    assert_eq!(skin_state_number(340, &state), Some(2));
    assert_eq!(skin_state_number(341, &state), Some(1));
    assert_eq!(skin_state_number(360, &state), Some(5));
    assert_eq!(skin_state_number(361, &state), Some(2));
    assert_eq!(skin_image_index_number(308, &state), Some(2));
    assert_eq!(skin_image_index_number(360, &state), Some(5));
    assert_eq!(skin_image_index_number(361, &state), Some(2));
    assert_eq!(skin_state_event_index(360, &state), 5);
    assert_eq!(skin_state_event_index(361, &state), 2);
    assert_eq!(skin_state_event_index(308, &state), 2);

    assert!(test_skin_op(SKIN_OPTION_BMZ_KEY_MODE_BASE + 2, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SOURCE_KEY_MODE_BASE + 3, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SEVEN_TO_SIX, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SOURCE_LN_UNDEFINED, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_CN, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SOURCE_LN_MIXED, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SOURCE_LN_PROFILE_AVAILABLE, &[], &state));
    assert!(test_skin_op(170, &[], &state));
    assert!(test_skin_op(176, &[], &state));
    assert!(test_skin_op(179, &[], &state));
}

#[test]
fn play_rank_ops_reflect_current_ex_score() {
    let aa_state = SkinDrawState {
        ex_score: 1556,
        total_notes: 1000,
        past_notes: 1000,
        ..SkinDrawState::default()
    };
    let aaa_state = SkinDrawState {
        ex_score: 1800,
        total_notes: 1000,
        past_notes: 1000,
        ..SkinDrawState::default()
    };
    let current_aaa_state = SkinDrawState {
        ex_score: 90,
        total_notes: 1000,
        past_notes: 50,
        ..SkinDrawState::default()
    };
    let before_first_note_state = SkinDrawState { total_notes: 1000, ..SkinDrawState::default() };

    assert!(test_skin_op(201, &[], &aa_state));
    assert!(!test_skin_op(200, &[], &aa_state));
    assert!(test_skin_op(200, &[], &aaa_state));
    assert!(test_skin_op(200, &[], &current_aaa_state));
    assert!(test_skin_op(200, &[], &before_first_note_state));
}

#[test]
fn bga_destination_renders_current_bga_images() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "stretch": 1, "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40, "a": 128 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState {
            has_bga: true,
            bga_base: Some(SkinBgaFrame {
                texture: SkinTextureId(20000),
                source_size: SkinImageSize { width: 256.0, height: 128.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            bga_layer: Some(SkinBgaFrame {
                texture: SkinTextureId(20001),
                source_size: SkinImageSize { width: 256.0, height: 256.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            ..SkinDrawState::default()
        },
        &SkinTextState::default(),
    );

    assert!(matches!(
        items.as_slice(),
        [
            SkinRenderItem::Image {
                texture: SkinTextureId(20000),
                rect: Rect { x, y, width, height },
                tint: Color { a, .. },
                ..
            },
            SkinRenderItem::Image { texture: SkinTextureId(20001), .. },
        ] if approx_eq(*x, 0.1)
            && approx_eq(*y, 0.525)
            && approx_eq(*width, 0.3)
            && approx_eq(*height, 0.15)
            && approx_eq(*a, 128.0 / 255.0)
    ));
}

#[test]
fn bga_destination_renders_poor_bga_instead_of_base_and_layer() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
    )
    .unwrap();

    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState {
            has_bga: true,
            bga_base: Some(SkinBgaFrame {
                texture: SkinTextureId(20000),
                source_size: SkinImageSize { width: 256.0, height: 256.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            bga_layer: Some(SkinBgaFrame {
                texture: SkinTextureId(20001),
                source_size: SkinImageSize { width: 256.0, height: 256.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            bga_poor: Some(SkinBgaFrame {
                texture: SkinTextureId(20002),
                source_size: SkinImageSize { width: 256.0, height: 256.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            ..SkinDrawState::default()
        },
        &SkinTextState::default(),
    );

    assert!(matches!(
        items.as_slice(),
        [SkinRenderItem::Image { texture: SkinTextureId(20002), .. }]
    ));
}

#[test]
fn bga_destination_uses_profile_stretch_when_destination_omits_stretch() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
    )
    .unwrap();

    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState {
            has_bga: true,
            bga_base: Some(SkinBgaFrame {
                texture: SkinTextureId(20000),
                source_size: SkinImageSize { width: 256.0, height: 128.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            bga_stretch: 1,
            ..SkinDrawState::default()
        },
        &SkinTextState::default(),
    );

    assert!(matches!(
        items.as_slice(),
        [SkinRenderItem::Image {
            texture: SkinTextureId(20000),
            rect: Rect { x, y, width, height },
            ..
        }] if approx_eq(*x, 0.1)
            && approx_eq(*y, 0.525)
            && approx_eq(*width, 0.3)
            && approx_eq(*height, 0.15)
    ));
}

#[test]
fn lr2_unspecified_stretch_keeps_ordinary_image_geometry() {
    let rect = Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
    let uv = TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
    let source = SkinImageSize { width: 256.0, height: 128.0 };
    for stretch in [-1, 0] {
        let (actual_rect, actual_uv) =
            stretch_skin_image_geometry(stretch, rect, uv, source, 100, 100);
        assert_eq!(actual_rect, rect);
        assert_eq!(actual_uv, uv);
    }
}

#[test]
fn bga_destination_stretch_overrides_profile_stretch() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "stretch": 0, "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
    )
    .unwrap();

    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState {
            has_bga: true,
            bga_base: Some(SkinBgaFrame {
                texture: SkinTextureId(20000),
                source_size: SkinImageSize { width: 256.0, height: 128.0 },
                tint_r: 1.0,
                tint_g: 1.0,
                tint_b: 1.0,
                tint_a: 1.0,
                is_video: false,
            }),
            bga_stretch: 1,
            ..SkinDrawState::default()
        },
        &SkinTextState::default(),
    );

    assert!(matches!(
        items.as_slice(),
        [SkinRenderItem::Image {
            texture: SkinTextureId(20000),
            rect: Rect { x, y, width, height },
            ..
        }] if approx_eq(*x, 0.1)
            && approx_eq(*y, 0.4)
            && approx_eq(*width, 0.3)
            && approx_eq(*height, 0.4)
    ));
}

#[test]
fn song_bga_options_are_evaluated_from_draw_state() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": "src", "path": "dummy.png" }],
                "image": [
                    { "id": "no-bga", "src": "src", "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "has-bga", "src": "src", "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "no-bga", "op": [170], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "has-bga", "op": [171], "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "src".to_string(),
        SkinDocumentTexture {
            source_id: "src".to_string(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    let no_bga_items = document.static_image_render_items(
        &sources,
        &SkinDrawState { has_bga: false, ..SkinDrawState::default() },
    );
    let bga_items = document.static_image_render_items(
        &sources,
        &SkinDrawState { has_bga: true, ..SkinDrawState::default() },
    );

    assert!(matches!(
        no_bga_items.as_slice(),
        [SkinRenderItem::Image { rect: Rect { x, .. }, .. }] if approx_eq(*x, 0.0)
    ));
    assert!(matches!(
        bga_items.as_slice(),
        [SkinRenderItem::Image { rect: Rect { x, .. }, .. }] if approx_eq(*x, 0.2)
    ));
}

#[test]
fn static_render_items_split_at_notes_marker() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [
                    { "id": "behind", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "cover", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "frame", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 }
                ],
                "destination": [
                    { "id": "behind", "dst": [{ "x": 0, "y": 0, "w": 100, "h": 100 }] },
                    { "id": "notes" },
                    { "id": "cover", "dst": [{ "x": 10, "y": 10, "w": 20, "h": 20 }] },
                    { "id": "frame", "dst": [{ "x": 5, "y": 5, "w": 90, "h": 90 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = mock_source("1", 8.0, 8.0);

    let (behind, front, failed_overlay) = document.static_render_items_split(
        &sources,
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );

    // `{"id":"notes"}` マーカーより前の destination は背面、後ろは前面に入る。
    assert_eq!(behind.len(), 1, "behind = destinations before the notes marker");
    assert_eq!(front.len(), 2, "front = destinations after the notes marker");
    assert!(failed_overlay.is_empty());
    // 結合版 static_render_items は behind→front→failed の順で全アイテムを返す。
    let all = document.static_render_items(
        &sources,
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );
    assert_eq!(all.len(), 3);
}

#[test]
fn pre_notes_lift_line_at_note_origin_renders_in_front() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [
                    { "id": "backdrop", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": 15, "src": 1, "x": 16, "y": 0, "w": 8, "h": 8 },
                    { "id": "note", "src": 1, "x": 0, "y": 0, "w": 51, "h": 36 }
                ],
                "destination": [
                    { "id": "backdrop", "dst": [{ "x": 0, "y": 0, "w": 720, "h": 720 }] },
                    { "id": 15, "offset": 3, "dst": [{ "x": 76, "y": 357, "w": 431, "h": 8 }] },
                    { "id": "notes" }
                ],
                "note": {
                    "id": "notes",
                    "note": ["note"],
                    "dst": [{ "x": 168, "y": 345, "w": 51, "h": 723 }]
                }
            }
            "#,
    )
    .unwrap();
    let sources = mock_source("1", 720.0, 720.0);

    let (behind, front, failed_overlay) = document.static_render_items_split(
        &sources,
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );

    assert_eq!(behind.len(), 1, "ordinary pre-notes items stay behind notes");
    assert_eq!(front.len(), 1, "ECFN-style judge line is drawn in front of notes");
    assert!(failed_overlay.is_empty());
    assert!(matches!(
        front.first(),
        Some(SkinRenderItem::Image { rect, .. })
            if approx_eq(rect.y, 355.0 / 720.0)
                && approx_eq(rect.height, 8.0 / 720.0)
    ));
}
