use super::*;

#[test]
fn negative_image_destination_size_mirrors_texture_region() {
    let item = skin_image_item_for_frame(
        SkinTextureId(1),
        Rect { x: 0.2, y: 0.3, width: 0.4, height: 0.5 },
        TextureRegion { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
        ResolvedSkinFrame { w: -101, h: -53, ..ResolvedSkinFrame::default() },
        0,
        BlendMode::Normal,
        None,
        false,
    );

    let SkinRenderItem::Image { rect, uv, .. } = item else { panic!() };
    assert!(approx_eq(rect.x, 0.2));
    assert!(approx_eq(rect.width, 0.4));
    assert!(approx_eq(uv.x, 0.4));
    assert!(approx_eq(uv.width, -0.3));
    assert!(approx_eq(uv.y, 0.6));
    assert!(approx_eq(uv.height, -0.4));
}

#[test]
fn positive_image_destination_size_keeps_texture_region_direction() {
    let item = skin_image_item_for_frame(
        SkinTextureId(1),
        Rect { x: 0.2, y: 0.3, width: 0.4, height: 0.5 },
        TextureRegion { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
        ResolvedSkinFrame { w: 101, h: 53, ..ResolvedSkinFrame::default() },
        0,
        BlendMode::Normal,
        None,
        false,
    );

    let SkinRenderItem::Image { uv, .. } = item else { panic!() };
    assert!(approx_eq(uv.x, 0.1));
    assert!(approx_eq(uv.width, 0.3));
    assert!(approx_eq(uv.y, 0.2));
    assert!(approx_eq(uv.height, 0.4));
}

#[test]
fn number_object_resolves_to_padded_text() {
    let object = SkinObject {
        id: SkinObjectId(1),
        source: SkinSource::Number {
            slot: NumberSlot::ExScore,
            style: TextStyle {
                gradient: None,
                font_id: None,
                size: 0.04,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
            digits: 4,
        },
        placements: vec![SkinPlacement {
            phase: SkinPhase::Result,
            time_ms: 0,
            rect: Rect { x: 0.1, y: 0.2, width: 0.2, height: 0.05 },
            alpha: 0.5,
            blend: BlendMode::Normal,
            animation: Animation::none(),
        }],
    };

    let items = object.resolve(SkinPhase::Result, 0, |_| String::new(), |_| 123);

    assert!(matches!(
        &items[0],
        SkinRenderItem::Text { text, style, .. }
            if text == "0123" && style.color.a == 0.5
    ));
}

#[test]
fn skin_definition_resolves_context_values() {
    let skin = SkinDefinition {
        objects: vec![SkinObject {
            id: SkinObjectId(1),
            source: SkinSource::Text {
                slot: TextSlot::Judge,
                style: TextStyle {
                    gradient: None,
                    font_id: None,
                    size: 0.04,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            },
            placements: vec![SkinPlacement {
                phase: SkinPhase::Play,
                time_ms: 0,
                rect: Rect { x: 0.3, y: 0.4, width: 0.2, height: 0.05 },
                alpha: 1.0,
                blend: BlendMode::Normal,
                animation: Animation::none(),
            }],
        }],
    };
    let context = SkinRenderContext {
        phase: SkinPhase::Play,
        elapsed_ms: 12,
        text: &[(TextSlot::Judge, "PGREAT FAST".to_string())],
        numbers: &[],
    };

    let items = skin.resolve(&context);

    assert!(matches!(&items[0], SkinRenderItem::Text { text, .. } if text == "PGREAT FAST"));
}

#[test]
fn append_skin_render_items_emits_image_commands() {
    let mut commands = Vec::new();
    append_skin_render_items(
        &mut commands,
        &[
            SkinRenderItem::Rect {
                rect: Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                color: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
            },
            SkinRenderItem::Image {
                texture: SkinTextureId(1),
                rect: Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                uv: TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Add,
                scale: SkinImageScale::Stretch,
                border: None,
                source_size: None,
                linear_filter: false,
            },
        ],
    );

    assert_eq!(commands.len(), 2);
    assert!(matches!(
        commands[1],
        DrawCommand::Image { texture: TextureId(1), blend: BlendMode::Add, .. }
    ));
}

#[test]
fn append_skin_render_items_keeps_empty_text_with_caret() {
    let mut commands = Vec::new();
    append_skin_render_items(
        &mut commands,
        &[SkinRenderItem::Text {
            origin: Point { x: 0.25, y: 0.5 },
            text: String::new(),
            style: TextStyle {
                gradient: None,
                font_id: None,
                size: 0.04,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
            caret: Some(TextCaret { byte_index: 0, color: Color::rgb(1.0, 1.0, 1.0) }),
            blend: BlendMode::Normal,
            post_scale: Point { x: 1.0, y: 1.0 },
        }],
    );

    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        DrawCommand::Text { text, caret: Some(TextCaret { byte_index: 0, .. }), .. }
            if text.is_empty()
    ));
}

#[test]
fn append_skin_render_items_expands_nine_slice_images() {
    let mut commands = Vec::new();
    append_skin_render_items(
        &mut commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(10),
            rect: Rect { x: 0.1, y: 0.2, width: 0.6, height: 0.3 },
            uv: TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            scale: SkinImageScale::NineSlice,
            border: Some(SkinImageBorder {
                left: 0.1,
                right: 0.2,
                top: 0.25,
                bottom: 0.25,
                unit: SkinImageBorderUnit::Normalized,
            }),
            source_size: None,
            linear_filter: false,
        }],
    );

    assert_eq!(commands.len(), 9);
    assert!(matches!(
        commands[0],
        DrawCommand::Image {
            rect: Rect { x: 0.1, y: 0.2, width, height },
            uv: UvRect { x: 0.0, y: 0.0, width: uv_width, height: uv_height },
            texture: TextureId(10),
            ..
        } if approx_eq(width, 0.06)
            && approx_eq(height, 0.075)
            && approx_eq(uv_width, 0.1)
            && approx_eq(uv_height, 0.25)
    ));
    assert!(matches!(
        commands[4],
        DrawCommand::Image {
            rect: Rect { width, height, .. },
            uv: UvRect { width: uv_width, height: uv_height, .. },
            texture: TextureId(10),
            ..
        } if approx_eq(width, 0.42)
            && approx_eq(height, 0.15)
            && approx_eq(uv_width, 0.7)
            && approx_eq(uv_height, 0.5)
    ));
}

#[test]
fn append_skin_render_items_expands_pixel_based_nine_slice_images() {
    let mut commands = Vec::new();
    append_skin_render_items(
        &mut commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(8),
            rect: Rect { x: 0.2, y: 0.1, width: 0.36, height: 0.48 },
            uv: TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            scale: SkinImageScale::NineSlice,
            border: Some(SkinImageBorder {
                left: 2.0,
                right: 2.0,
                top: 3.0,
                bottom: 3.0,
                unit: SkinImageBorderUnit::Pixels,
            }),
            source_size: Some(SkinImageSize { width: 12.0, height: 48.0 }),
            linear_filter: false,
        }],
    );

    assert_eq!(commands.len(), 9);
    assert!(matches!(
        commands[0],
        DrawCommand::Image {
            rect: Rect { width, height, .. },
            uv: UvRect { width: uv_width, height: uv_height, .. },
            ..
        } if approx_eq(width, 0.06)
            && approx_eq(height, 0.03)
            && approx_eq(uv_width, 2.0 / 12.0)
            && approx_eq(uv_height, 3.0 / 48.0)
    ));
}

#[test]
fn bundled_default_skin_manifest_resolves_relative_texture_paths() {
    let manifest = SkinManifest::bundled_default().with_texture_source_sizes(&default_skin_root());

    let textures = manifest.resolve_textures(Path::new("/skin/default"));

    assert_eq!(textures[0].id, TextureId(1));
    assert_eq!(textures[0].path, PathBuf::from("/skin/default/note.png"));
    assert_eq!(textures[1].id, TextureId(2));
    assert_eq!(textures[1].path, PathBuf::from("/skin/default/note-blue.png"));
    assert_eq!(textures[2].id, TextureId(3));
    assert_eq!(textures[2].path, PathBuf::from("/skin/default/note-red.png"));
    assert_eq!(textures[3].id, TextureId(4));
    assert_eq!(textures[3].path, PathBuf::from("/skin/default/receptor.png"));
    assert_eq!(textures[4].id, TextureId(5));
    assert_eq!(textures[4].path, PathBuf::from("/skin/default/receptor-blue.png"));
    assert_eq!(textures[5].id, TextureId(6));
    assert_eq!(textures[5].path, PathBuf::from("/skin/default/receptor-red.png"));
    assert_eq!(textures[6].id, TextureId(7));
    assert_eq!(textures[6].path, PathBuf::from("/skin/default/judge-line.png"));
    assert_eq!(textures[7].id, TextureId(8));
    assert_eq!(textures[7].path, PathBuf::from("/skin/default/gauge-frame.png"));
    assert_eq!(textures[8].id, TextureId(9));
    assert_eq!(textures[8].path, PathBuf::from("/skin/default/gauge-fill.png"));
    assert_eq!(textures[9].id, TextureId(10));
    assert_eq!(textures[9].path, PathBuf::from("/skin/default/combo-panel.png"));
    assert_eq!(textures[10].id, TextureId(11));
    assert_eq!(textures[10].path, PathBuf::from("/skin/default/combo-panel-inactive.png"));
    assert_eq!(textures[11].id, TextureId(12));
    assert_eq!(textures[11].path, PathBuf::from("/skin/default/note-mine.png"));
    assert_eq!(manifest.play_note_image().texture_for_lane(Lane::Key2), 2);
    assert_eq!(manifest.play_note_image().texture_for_lane(Lane::Scratch), 3);
    assert_eq!(manifest.play_receptor_image().texture_for_lane(Lane::Key2), 5);
    assert_eq!(manifest.play_receptor_image().texture_for_lane(Lane::Scratch), 6);
    assert_eq!(manifest.play_judge_line_image().texture, 7);
    assert_eq!(manifest.play_gauge_frame_image().texture, 8);
    assert_eq!(manifest.play_gauge_frame_image().scale, SkinImageScale::NineSlice);
    assert_eq!(
        manifest.play_gauge_frame_image().source_size,
        Some(SkinImageSize { width: 12.0, height: 48.0 })
    );
    assert_eq!(
        manifest.play_gauge_frame_image().border,
        Some(SkinImageBorder {
            left: 2.0,
            right: 2.0,
            top: 3.0,
            bottom: 3.0,
            unit: SkinImageBorderUnit::Pixels,
        })
    );
    assert_eq!(manifest.play_gauge_fill_image().texture, 9);
    assert_eq!(manifest.play_combo_panel_image(true).texture, 10);
    assert_eq!(manifest.play_combo_panel_image(true).scale, SkinImageScale::NineSlice);
    assert_eq!(manifest.play_combo_panel_image(false).texture, 11);
}

#[test]
fn difficulty_ops_reflect_chart_difficulty_code() {
    let unknown = SkinDrawState::default();
    let normal = SkinDrawState { difficulty: 2, ..SkinDrawState::default() };
    let insane = SkinDrawState { difficulty: 5, ..SkinDrawState::default() };

    assert!(test_skin_op(150, &[], &unknown));
    assert!(!test_skin_op(150, &[], &normal));
    assert!(test_skin_op(152, &[], &normal));
    assert!(!test_skin_op(153, &[], &normal));
    assert!(test_skin_op(155, &[], &insane));
}

#[test]
fn folded_constant_draw_condition_number_zero_is_true() {
    assert!(eval_skin_draw_condition("number(0) >= 0", &SkinDrawState::default()));
    assert!(!eval_skin_draw_condition("number(0) < 0", &SkinDrawState::default()));
}

#[test]
fn removed_grade_diff_mode_refs_have_fixed_next_compatibility_values() {
    let state = SkinDrawState::default();

    assert_eq!(skin_state_number(SKIN_REF_BMZ_GRADE_DIFF_DISPLAY, &state), Some(1));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_GRADE_DIFF_NEAREST, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_GRADE_DIFF_NEXT, &[], &state));
}

#[test]
fn bmz_rule_and_ln_policy_refs_keep_setting_score_key_and_effective_mode_separate() {
    let state = SkinDrawState {
        rule_mode_index: 2,
        ln_policy_setting_index: Some(4),
        ln_score_policy_index: Some(1),
        result_ln_mode_index: Some(2),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(SKIN_REF_BMZ_RULE_MODE, &state), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LN_POLICY_SETTING, &state), Some(4));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LN_SCORE_POLICY, &state), Some(1));
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_RULE_MODE, &state), 2);
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_LN_POLICY_SETTING, &state), 4);
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_LN_SCORE_POLICY, &state), 1);
    assert_eq!(skin_state_imageset_index(SKIN_REF_BMZ_RULE_MODE, &state), Some(2));
    assert_eq!(skin_state_imageset_index(SKIN_REF_BMZ_LN_POLICY_SETTING, &state), Some(4));
    assert_eq!(skin_state_imageset_index(SKIN_REF_BMZ_LN_SCORE_POLICY, &state), Some(1));
    // Existing ref 308 remains the effective rendered LN kind.
    assert_eq!(skin_state_event_index(308, &state), 2);

    assert!(test_skin_op(1990, &[], &state));
    assert!(!test_skin_op(1988, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_LN_POLICY_SETTING_BASE + 4, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_LN_POLICY_SETTING_FORCE, &[], &state));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_LN_POLICY_SETTING_AUTO, &[], &state));
    assert!(test_skin_op(19_152, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_LN_SCORE_POLICY_AUTO, &[], &state));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_LN_SCORE_POLICY_FORCE, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_LN_SCORE_POLICY_AVAILABLE, &[], &state));

    let text = SkinTextState::default();
    assert_eq!(skin_main_state_text(SKIN_REF_BMZ_RULE_MODE, Some(&state), &text), "DX");
    assert_eq!(
        skin_main_state_text(SKIN_REF_BMZ_LN_POLICY_SETTING, Some(&state), &text),
        "FORCE(CN)"
    );
    assert_eq!(skin_main_state_text(SKIN_REF_BMZ_LN_SCORE_POLICY, Some(&state), &text), "AUTO(CN)");
    let lua_text = lua_main_state_text_values(&state, &text);
    assert_eq!(lua_text.get(&SKIN_REF_BMZ_RULE_MODE).map(String::as_str), Some("DX"));
    assert_eq!(lua_text.get(&SKIN_REF_BMZ_LN_SCORE_POLICY).map(String::as_str), Some("AUTO(CN)"));
}

#[test]
fn bmz_ln_policy_refs_are_unavailable_without_a_selected_or_running_score_key() {
    let state = SkinDrawState::default();

    assert_eq!(skin_state_number(SKIN_REF_BMZ_LN_POLICY_SETTING, &state), None);
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LN_SCORE_POLICY, &state), None);
    assert_eq!(skin_state_imageset_index(SKIN_REF_BMZ_LN_POLICY_SETTING, &state), None);
    assert_eq!(skin_state_imageset_index(SKIN_REF_BMZ_LN_SCORE_POLICY, &state), None);
    assert!(!test_skin_op(SKIN_OPTION_BMZ_LN_POLICY_SETTING_BASE, &[], &state));
    assert!(!test_skin_op(19_151, &[], &state));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_LN_SCORE_POLICY_AVAILABLE, &[], &state));
    assert_eq!(
        skin_main_state_text(SKIN_REF_BMZ_LN_SCORE_POLICY, Some(&state), &SkinTextState::default()),
        ""
    );
}

#[test]
fn score_grade_refs_use_exact_official_borders() {
    let state = SkinDrawState {
        select_screen: true,
        select_ex_score: Some(63),
        select_total_notes: 102,
        select_play_count: 1,
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(154, &state), Some(5));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_CURRENT, &state), Some(1));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT, &state), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEAREST, &state), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_CURRENT_DIFF, &state), Some(17));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT_DIFF, &state), Some(-5));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEAREST_DIFF, &state), Some(-5));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEAREST_ABS, &state), Some(5));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_CURRENT, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_NEXT, &[], &state));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_EXACT, &[], &state));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_TIE, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_AVAILABLE, &[], &state));
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_SCORE_GRADE_NEAREST, &state), 2);
    assert_eq!(skin_state_imageset_index(SKIN_REF_BMZ_SCORE_GRADE_NEAREST, &state), Some(2));
    assert_eq!(
        skin_main_state_text(
            SKIN_REF_BMZ_SCORE_GRADE_CURRENT,
            Some(&state),
            &SkinTextState::default()
        ),
        "E"
    );
    assert_eq!(
        skin_main_state_text(
            SKIN_REF_BMZ_SCORE_GRADE_NEXT,
            Some(&state),
            &SkinTextState::default()
        ),
        "D"
    );
    assert_eq!(
        skin_main_state_text(
            SKIN_REF_BMZ_SCORE_GRADE_NEAREST,
            Some(&state),
            &SkinTextState::default()
        ),
        "D"
    );
}

#[test]
fn score_grade_boundaries_skip_one_ninth_and_advance_when_exact() {
    let below_e = SkinDrawState { ex_score: 2, total_notes: 9, ..SkinDrawState::default() };
    assert_eq!(skin_state_number(154, &below_e), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT_DIFF, &below_e), Some(-2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT, &below_e), Some(1));

    let exact_d = SkinDrawState { ex_score: 68, total_notes: 102, ..SkinDrawState::default() };
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_CURRENT, &exact_d), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT, &exact_d), Some(3));
    assert_eq!(skin_state_number(154, &exact_d), Some(23));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT_DIFF, &exact_d), Some(-23));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEAREST_DIFF, &exact_d), Some(0));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_CURRENT, &[], &exact_d));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_EXACT, &[], &exact_d));
}

#[test]
fn score_grade_next_diff_keeps_the_observed_max_minus_value() {
    let state = SkinDrawState { ex_score: 2996, total_notes: 1670, ..Default::default() };

    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_CURRENT, &state), Some(7));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT, &state), Some(8));
    assert_eq!(skin_state_number(154, &state), Some(344));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT_DIFF, &state), Some(-344));
}

#[test]
fn score_grade_nearest_tie_prefers_current_border() {
    let state = SkinDrawState { ex_score: 5, total_notes: 9, ..SkinDrawState::default() };

    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_CURRENT, &state), Some(1));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT, &state), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEAREST, &state), Some(1));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEAREST_DIFF, &state), Some(1));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_CURRENT, &[], &state));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_NEXT, &[], &state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_TIE, &[], &state));
}

#[test]
fn score_grade_handles_tiny_charts_max_and_unavailable_scores() {
    let tiny = SkinDrawState { ex_score: 1, total_notes: 1, ..SkinDrawState::default() };
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_CURRENT, &tiny), Some(3));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_NEXT, &tiny), Some(4));

    let max = SkinDrawState { ex_score: 204, total_notes: 102, ..SkinDrawState::default() };
    for ref_id in [
        SKIN_REF_BMZ_SCORE_GRADE_CURRENT,
        SKIN_REF_BMZ_SCORE_GRADE_NEXT,
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST,
    ] {
        assert_eq!(skin_state_number(ref_id, &max), Some(8));
    }
    assert_eq!(skin_state_number(154, &max), Some(0));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_EXACT, &[], &max));

    let unavailable = SkinDrawState {
        select_screen: true,
        select_ex_score: Some(0),
        select_total_notes: 102,
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(154, &unavailable), None);
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SCORE_GRADE_CURRENT, &unavailable), None);
    assert!(!test_skin_op(SKIN_OPTION_BMZ_SCORE_GRADE_AVAILABLE, &[], &unavailable));
    assert_eq!(skin_state_number(154, &SkinDrawState::default()), None);
}

#[test]
fn score_grade_facts_are_identical_across_select_play_and_result() {
    let select = SkinDrawState {
        select_screen: true,
        select_ex_score: Some(63),
        select_total_notes: 102,
        select_play_count: 1,
        ..SkinDrawState::default()
    };
    let play = SkinDrawState { ex_score: 63, total_notes: 102, ..SkinDrawState::default() };
    let result = SkinDrawState {
        result_failed: Some(false),
        ex_score: 63,
        total_notes: 102,
        ..Default::default()
    };

    for ref_id in 154..=154 {
        assert_eq!(skin_state_number(ref_id, &select), skin_state_number(ref_id, &play));
        assert_eq!(skin_state_number(ref_id, &play), skin_state_number(ref_id, &result));
    }
    for ref_id in SKIN_REF_BMZ_SCORE_GRADE_CURRENT..=SKIN_REF_BMZ_SCORE_GRADE_NEAREST_ABS {
        assert_eq!(skin_state_number(ref_id, &select), skin_state_number(ref_id, &play));
        assert_eq!(skin_state_number(ref_id, &play), skin_state_number(ref_id, &result));
    }
}

#[test]
fn skin_document_resolves_static_image_destinations() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 1280,
                "h": 720,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 16, "y": 32, "w": 64, "h": 128 }],
                "destination": [
                    { "id": "panel", "blend": 2, "dst": [
                        { "x": 128, "y": 72, "w": 256, "h": 144, "a": 128, "r": 64 }
                    ]},
                    { "id": "panel", "timer": 1, "dst": [{ "x": 0, "y": 0, "w": 1, "h": 1 }] }
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
            source_size: SkinImageSize { width: 256.0, height: 512.0 },
        },
    )]);

    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(items.len(), 1);
    assert!(matches!(
        items[0],
        SkinRenderItem::Image {
            texture: SkinTextureId(42),
            rect: Rect { x, y, width, height },
            uv: TextureRegion { x: u, y: v, width: uv_width, height: uv_height },
            tint: Color { r, a, .. },
            blend: BlendMode::Add,
            ..
        } if approx_eq(x, 0.1)
            && approx_eq(y, 0.7)
            && approx_eq(width, 0.2)
            && approx_eq(height, 0.2)
            && approx_eq(u, 16.0 / 256.0)
            && approx_eq(v, 32.0 / 512.0)
            && approx_eq(uv_width, 64.0 / 256.0)
            && approx_eq(uv_height, 128.0 / 512.0)
            && approx_eq(r, 64.0 / 255.0)
            && approx_eq(a, 128.0 / 255.0)
    ));
}

#[test]
fn skin_document_applies_destination_stretch_to_static_images() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "wide", "src": 1, "x": 0, "y": 0, "w": 200, "h": 100 }],
                "destination": [
                    { "id": "wide", "stretch": 1, "dst": [{ "x": 10, "y": 10, "w": 40, "h": 40 }] },
                    { "id": "wide", "stretch": 3, "dst": [{ "x": 10, "y": 60, "w": 40, "h": 40 }] },
                    { "id": "wide", "stretch": 9, "dst": [{ "x": 70, "y": 70, "w": 20, "h": 20 }] }
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
            source_size: SkinImageSize { width: 200.0, height: 100.0 },
        },
    )]);

    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(items.len(), 3);
    assert!(matches!(
        items[0],
        SkinRenderItem::Image {
            rect: Rect { x, y, width, height },
            uv: TextureRegion { x: u, width: uv_width, .. },
            ..
        } if approx_eq(x, 0.1)
            && approx_eq(y, 0.6)
            && approx_eq(width, 0.4)
            && approx_eq(height, 0.2)
            && approx_eq(u, 0.0)
            && approx_eq(uv_width, 1.0)
    ));
    assert!(matches!(
        items[1],
        SkinRenderItem::Image {
            rect: Rect { x, y, width, height },
            uv: TextureRegion { x: u, width: uv_width, .. },
            ..
        } if approx_eq(x, 0.1)
            && approx_eq(y, 0.0)
            && approx_eq(width, 0.4)
            && approx_eq(height, 0.4)
            && approx_eq(u, 0.25)
            && approx_eq(uv_width, 0.5)
    ));
    assert!(matches!(
        items[2],
        SkinRenderItem::Image {
            rect: Rect { x, y, width, height },
            ..
        } if approx_eq(x, -0.2)
            && approx_eq(y, -0.3)
            && approx_eq(width, 2.0)
            && approx_eq(height, 1.0)
    ));
}

#[test]
fn skin_document_evaluates_number_draw_conditions() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "draw": "number(425) > 0", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "number(425) == 0", "dst": [{ "x": 10, "y": 0, "w": 10, "h": 10 }] }
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

    let no_miss = document.static_image_render_items(&sources, &SkinDrawState::default());
    let miss = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            judge_counts: DisplayJudgeCounts { bad: 1, poor: 2, ..Default::default() },
            ..SkinDrawState::default()
        },
    );

    assert!(
        matches!(no_miss[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.1))
    );
    assert!(
        matches!(miss[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.0))
    );
    assert!(eval_skin_draw_condition(
        "number(410) == number(411) or number(110) == number(410)",
        &SkinDrawState {
            judge_counts: DisplayJudgeCounts { pgreat: 300, ..Default::default() },
            fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
                fast_pgreat: 300,
                slow_pgreat: 0,
                ..Default::default()
            }),
            ..Default::default()
        }
    ));
    assert!(eval_skin_draw_condition(
        "number(410) > number(411) and number(411) >= 1",
        &SkinDrawState {
            fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
                fast_pgreat: 120,
                slow_pgreat: 20,
                ..Default::default()
            }),
            ..Default::default()
        }
    ));
}

#[test]
fn skin_document_evaluates_option_draw_conditions() {
    assert!(eval_skin_draw_condition(
        "option(197)",
        &SkinDrawState { select_replay_slots: [true, false, false, false], ..Default::default() }
    ));
    assert!(eval_skin_draw_condition("!option(197)", &SkinDrawState::default()));
    assert!(!eval_skin_draw_condition(
        "!option(197)",
        &SkinDrawState { select_replay_slots: [true, false, false, false], ..Default::default() }
    ));
}
