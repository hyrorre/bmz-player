use super::*;

#[test]
fn rendering_without_surface_is_skipped() {
    let mut renderer = Renderer::default();
    let scene = AppSceneSnapshot::Select(SelectSnapshot::default());

    assert_eq!(renderer.render_scene_status(scene).unwrap(), RenderSurfaceStatus::SkippedNoSurface);
}

#[test]
fn scene_clear_colors_are_distinct() {
    let select = DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot::default()));
    let play = DrawPlan::from_scene(&AppSceneSnapshot::Play(Default::default()));

    assert_ne!(select.clear, play.clear);
}

#[test]
fn vector_text_caret_rect_uses_font_advance_without_changing_text() {
    let Some(font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 1000, height: 100 };
    let style = TextStyle {
        font_id: None,
        size: 0.1,
        bitmap_size: None,
        color: Color::rgb(1.0, 1.0, 1.0),
        layer: crate::plan::TextLayer::Skin,
        align: TextAlign::Left,
        max_width: 0.0,
        overflow: TextOverflow::Overflow,
        wrapping: false,
        outline: None,
        shadow: None,
    };
    let caret = TextCaret { byte_index: "A".len(), color: Color::rgb(0.8, 0.9, 1.0) };

    let rect =
        vector_text_caret_rect(&Point { x: 0.1, y: 0.2 }, "AB", &style, &font, surface, caret)
            .expect("caret rect");

    let scaled = font.as_scaled(PxScale::from(10.0));
    let expected_x = (100.0 + text_width_px("A", &font, &scaled)) / 1000.0;
    assert_approx(rect.rect.x, expected_x);
    assert_approx(rect.rect.y, 0.2);
    assert_approx(rect.rect.height, 0.1);
    assert_eq!(rect.color, caret.color);
}

#[test]
fn plan_geometry_draws_text_caret_after_text() {
    let mut plan = DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands: vec![sample_text()] };
    let DrawCommand::Text { caret, .. } = &mut plan.commands[0] else {
        unreachable!();
    };
    *caret = Some(TextCaret { byte_index: 1, color: Color::rgb(1.0, 1.0, 1.0) });
    let text_frame = TextFrame {
        command_quad_counts: vec![1],
        command_caret_rects: vec![Some(RectCommand {
            rect: Rect { x: 0.2, y: 0.3, width: 0.01, height: 0.1 },
            color: Color::rgb(1.0, 1.0, 1.0),
        })],
        ..TextFrame::default()
    };

    let geometry = encode_plan_geometry(&plan, &text_frame, test_surface_size());

    assert_eq!(
        geometry.steps,
        vec![
            DrawStep::Text { range: 0..TEXT_INSTANCE_BYTES },
            DrawStep::Rects { range: 0..RECT_INSTANCE_BYTES },
        ]
    );
    assert_eq!(geometry.rects.len(), RECT_INSTANCE_BYTES);
}

#[test]
fn plan_geometry_encodes_one_rect_instance_per_rect_command() {
    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot {
        chart_count: 1,
        ..Default::default()
    }));

    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
    let rect_count =
        plan.commands.iter().filter(|command| matches!(command, DrawCommand::Rect { .. })).count();

    assert_eq!(geometry.rects.len(), rect_count * RECT_INSTANCE_BYTES);
}

#[test]
fn plan_geometry_encodes_rect_batch_instances() {
    let rect = crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::RectBatch {
            rects: std::sync::Arc::from([
                crate::plan::RectCommand { rect, color: Color::rgb(1.0, 0.0, 0.0) },
                crate::plan::RectCommand { rect, color: Color::rgb(0.0, 1.0, 0.0) },
            ]),
            cache: None,
        }],
    };

    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());

    assert_eq!(geometry.rects.len(), RECT_INSTANCE_BYTES * 2);
    assert_eq!(
        geometry.stats(),
        DrawStepStats { steps: 1, rect_steps: 1, rect_instances: 2, ..Default::default() }
    );
}

#[test]
fn plan_geometry_skips_invisible_rects_and_images() {
    let visible_rect = crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
    let zero_width_rect = crate::plan::Rect { width: 0.0, ..visible_rect };
    let mut transparent_image = sample_image(0, BlendMode::Normal);
    let DrawCommand::Image { tint, .. } = &mut transparent_image else { panic!() };
    *tint = Color::rgba(1.0, 1.0, 1.0, 0.0);
    let mut zero_size_image = sample_image(1, BlendMode::Normal);
    let DrawCommand::Image { rect, .. } = &mut zero_size_image else { panic!() };
    rect.height = 0.0;

    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            DrawCommand::Rect { rect: visible_rect, color: Color::rgba(1.0, 0.0, 0.0, 0.0) },
            DrawCommand::Rect { rect: zero_width_rect, color: Color::rgb(0.0, 1.0, 0.0) },
            DrawCommand::RectBatch {
                rects: std::sync::Arc::from([
                    crate::plan::RectCommand {
                        rect: visible_rect,
                        color: Color::rgba(1.0, 1.0, 1.0, 0.0),
                    },
                    crate::plan::RectCommand {
                        rect: zero_width_rect,
                        color: Color::rgb(1.0, 1.0, 1.0),
                    },
                ]),
                cache: None,
            },
            transparent_image,
            zero_size_image,
        ],
    };

    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());

    assert!(geometry.rects.is_empty());
    assert!(geometry.images.is_empty());
    assert_eq!(geometry.steps, Vec::new());
    assert_eq!(geometry.stats(), DrawStepStats::default());
}

#[test]
fn plan_geometry_can_replace_cached_rect_batch_with_image_instance() {
    let rect = crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
    let cache =
        crate::plan::RectBatchCache { key: crate::plan::RectBatchCacheKey(42), bounds: rect };
    let texture = TextureId(123);
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::RectBatch {
            rects: std::sync::Arc::from([crate::plan::RectCommand {
                rect,
                color: Color::rgb(1.0, 0.0, 0.0),
            }]),
            cache: Some(cache),
        }],
    };

    let geometry = encode_plan_geometry_with_rect_batch_resolver(
        &plan,
        &TextFrame::default(),
        test_surface_size(),
        CanvasViewport::from_policy(test_surface_size(), CanvasRenderPolicy::default()),
        &mut |_, _| Some(texture),
    );

    assert!(geometry.rects.is_empty());
    assert_eq!(
        geometry.stats(),
        DrawStepStats { steps: 1, image_steps: 1, image_instances: 1, ..Default::default() }
    );
    assert_eq!(
        geometry.steps,
        vec![DrawStep::Image {
            texture,
            blend: BlendMode::Premultiplied,
            linear: false,
            range: 0..IMAGE_INSTANCE_BYTES,
        }]
    );
}

#[test]
fn plan_geometry_groups_consecutive_images_by_texture() {
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            sample_image(0, BlendMode::Normal),
            sample_image(0, BlendMode::Normal),
            sample_image(7, BlendMode::Normal),
        ],
    };

    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
    let image_step_sizes: Vec<_> = geometry
        .steps
        .iter()
        .filter_map(|step| match step {
            DrawStep::Image { range, .. } => Some(range.len()),
            _ => None,
        })
        .collect();

    assert_eq!(image_step_sizes, vec![IMAGE_INSTANCE_BYTES * 2, IMAGE_INSTANCE_BYTES]);
    assert_eq!(geometry.images.len(), IMAGE_INSTANCE_BYTES * 3);
    assert_eq!(
        geometry.stats(),
        DrawStepStats { steps: 2, image_steps: 2, image_instances: 3, ..Default::default() }
    );
}

#[test]
fn plan_geometry_separates_image_blend_modes() {
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![sample_image(0, BlendMode::Normal), sample_image(0, BlendMode::Add)],
    };

    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
    let blends: Vec<_> = geometry
        .steps
        .iter()
        .filter_map(|step| match step {
            DrawStep::Image { blend, .. } => Some(*blend),
            _ => None,
        })
        .collect();

    assert_eq!(blends, vec![BlendMode::Normal, BlendMode::Add]);
}

#[test]
fn additive_image_blend_uses_source_alpha() {
    let blend = image_blend_state(BlendMode::Add);

    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::SrcAlpha);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
}

#[test]
fn multiply_image_blend_modulates_destination_with_source_color() {
    let blend = image_blend_state(BlendMode::Multiply);

    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::Zero);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::Src);
    assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
    assert_eq!(blend.alpha.src_factor, wgpu::BlendFactor::Zero);
    assert_eq!(blend.alpha.dst_factor, wgpu::BlendFactor::Src);
    assert_eq!(blend.alpha.operation, wgpu::BlendOperation::Add);
}

#[test]
fn subtractive_image_blend_darkens_rgb_without_erasing_destination_alpha() {
    let blend = image_blend_state(BlendMode::Subtract);
    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::SrcAlpha);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.color.operation, wgpu::BlendOperation::ReverseSubtract);
    assert_eq!(blend.alpha.src_factor, wgpu::BlendFactor::Zero);
    assert_eq!(blend.alpha.dst_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.alpha.operation, wgpu::BlendOperation::Add);
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![sample_image(0, BlendMode::Normal), sample_image(0, BlendMode::Subtract)],
    };
    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
    assert_eq!(geometry.steps.len(), 2);
    assert!(matches!(geometry.steps[1], DrawStep::Image { blend: BlendMode::Subtract, .. }));
}

#[test]
fn premultiplied_image_blend_does_not_apply_source_alpha_twice() {
    let blend = image_blend_state(BlendMode::Premultiplied);

    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
    assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
    assert_eq!(blend.alpha.src_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.alpha.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
    assert_eq!(blend.alpha.operation, wgpu::BlendOperation::Add);
}

#[test]
fn plan_geometry_splits_image_steps_around_other_commands() {
    // 同じテクスチャの画像でも、間に rect を挟めば別ステップになる。
    // rect が2枚の画像の「間」に描かれ、commands の順序が保たれることの回帰テスト。
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            sample_image(1, BlendMode::Normal),
            sample_rect(),
            sample_image(1, BlendMode::Normal),
        ],
    };

    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());

    assert_eq!(geometry.steps.len(), 3);
    assert!(matches!(geometry.steps[0], DrawStep::Image { .. }));
    assert!(matches!(geometry.steps[1], DrawStep::Rects { .. }));
    assert!(matches!(geometry.steps[2], DrawStep::Image { .. }));
}

#[test]
fn plan_geometry_orders_text_steps_by_command_position() {
    // Text コマンドが Image より前にあれば、描画ステップも Image より前 (背面) になる。
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![sample_text(), sample_image(1, BlendMode::Normal)],
    };
    // sample_text が 2 quad を生成したと仮定したテキストフレーム。
    let text_frame = TextFrame { command_quad_counts: vec![2], ..TextFrame::default() };

    let geometry = encode_plan_geometry(&plan, &text_frame, test_surface_size());

    assert_eq!(geometry.steps.len(), 2);
    assert_eq!(geometry.steps[0], DrawStep::Text { range: 0..TEXT_INSTANCE_BYTES * 2 });
    assert!(matches!(geometry.steps[1], DrawStep::Image { .. }));
    assert_eq!(
        geometry.stats(),
        DrawStepStats {
            steps: 2,
            image_steps: 1,
            text_steps: 1,
            image_instances: 1,
            text_instances: 2,
            ..Default::default()
        }
    );
}

#[test]
fn plan_geometry_writes_rotation_instance_data() {
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::RotatedImage {
            rect: crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
            uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: crate::plan::TextureId(0),
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            linear_filter: false,
            angle_rad: 1.25,
            center: Point { x: 0.0, y: 1.0 },
            post_scale: Point { x: 1.0, y: 1.0 },
        }],
    };

    let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
    let floats: Vec<f32> =
        geometry.images.as_chunks::<4>().0.iter().map(|bytes| f32::from_le_bytes(*bytes)).collect();

    assert_eq!(floats.len(), IMAGE_INSTANCE_FLOATS);
    assert_eq!(floats[12], 1.25);
    assert_eq!(floats[13], 0.0);
    assert_eq!(floats[14], 1.0);
    assert!((floats[15] - 16.0 / 9.0).abs() < f32::EPSILON);
    assert_eq!(floats[16], 1.0);
    assert_eq!(floats[17], 1.0);
}

#[test]
fn all_offset_post_scale_is_applied_to_text_quads_and_caret() {
    let origin = Point { x: 0.2, y: 0.3 };
    let scale = Point { x: 1.5, y: 0.5 };
    let mut quads = [TextQuad {
        x: 0.3,
        y: 0.5,
        width: 0.2,
        height: 0.4,
        atlas_origin: (0, 0),
        glyph_width: 1,
        glyph_height: 1,
        color: Color::rgb(1.0, 1.0, 1.0),
    }];
    scale_text_quads(&mut quads, origin, scale);
    assert_approx(quads[0].x, 0.35);
    assert_approx(quads[0].y, 0.4);
    assert_approx(quads[0].width, 0.3);
    assert_approx(quads[0].height, 0.2);

    let caret = scale_text_rect_command(
        RectCommand {
            rect: Rect { x: 0.4, y: 0.5, width: 0.02, height: 0.1 },
            color: Color::rgb(1.0, 1.0, 1.0),
        },
        origin,
        scale,
    );
    assert_approx(caret.rect.x, 0.5);
    assert_approx(caret.rect.y, 0.4);
    assert_approx(caret.rect.width, 0.03);
    assert_approx(caret.rect.height, 0.05);
}

#[test]
fn validate_rgba_texture_rejects_invalid_payloads() {
    assert!(validate_rgba_texture(1, 1, &[255, 255, 255, 255]).is_ok());
    assert!(validate_rgba_texture(0, 1, &[255, 255, 255, 255]).is_err());
    assert!(validate_rgba_texture(1, 1, &[255]).is_err());
}
