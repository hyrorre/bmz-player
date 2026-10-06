use super::*;

const FULL: Rect = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
const CENTER: Rect = Rect { x: 0.25, y: 0.25, width: 0.5, height: 0.5 };
const OUTSIDE: Rect = Rect { x: 2.0, y: 2.0, width: 1.0, height: 1.0 };

fn colored_rect(rect: Rect, color: Color) -> DrawCommand {
    DrawCommand::Rect { rect, color }
}

fn solid_image(texture: u32) -> DrawCommand {
    DrawCommand::Image {
        rect: FULL,
        uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        source_size: None,
        texture: TextureId(texture),
        tint: Color::rgb(1.0, 1.0, 1.0),
        blend: BlendMode::Normal,
        linear_filter: false,
    }
}

fn ambient(color: Color) -> DrawCommand {
    DrawCommand::Ambient {
        rect: FULL,
        blur: 50.0,
        fade_edges: false,
        layers: vec![colored_rect(FULL, color)],
    }
}

#[test]
fn clip_rounds_bottom_origin_components_before_flipping_negative_extents() {
    let size = SurfaceSize { width: 16, height: 16 };
    let viewport = CanvasViewport::from_policy(size, CanvasRenderPolicy::default());
    // x and bottom y are -0.5px; Java rounds these to zero, not -1.
    assert_eq!(
        ScissorRect::from_canvas(
            Rect { x: -0.03125, y: 0.75, width: 0.28125, height: 0.28125 },
            viewport,
            size,
        ),
        Some(ScissorRect { x: 0, y: 11, width: 5, height: 5 })
    );
    // Negative 4.5px extents round to -4 before the global scale is flipped.
    assert_eq!(
        ScissorRect::from_canvas(
            Rect { x: 0.5, y: 0.5, width: -0.28125, height: -0.28125 },
            viewport,
            size,
        ),
        Some(ScissorRect { x: 4, y: 3, width: 4, height: 4 })
    );
    assert_eq!(
        ScissorRect::from_canvas(
            Rect { x: -1.0, y: -1.0, width: 3.0, height: 3.0 },
            viewport,
            size
        ),
        Some(ScissorRect { x: 0, y: 0, width: 16, height: 16 })
    );
    for rect in [OUTSIDE, Rect { width: 0.0, ..FULL }, Rect { x: f32::NAN, ..FULL }] {
        assert_eq!(ScissorRect::from_canvas(rect, viewport, size), None);
    }
    let size = SurfaceSize { width: 100, height: 100 };
    let viewport = CanvasViewport::from_policy(size, CanvasRenderPolicy::default());
    // Half pixels also arise from odd offset widths/heights. Normalize exactly
    // as the destination path does, then project before Java-style rounding.
    for (x, expected_x, expected_width) in [(10.5, 11, 20), (-10.5, 0, 10)] {
        let rect = Rect {
            x: x / 100.0,
            y: (100.0 - 10.5 - 20.0) / 100.0,
            width: 20.0 / 100.0,
            height: 20.0 / 100.0,
        };
        assert_eq!(
            ScissorRect::from_canvas(rect, viewport, size),
            Some(ScissorRect { x: expected_x, y: 69, width: expected_width, height: 20 })
        );
    }
}

#[test]
fn clip_uses_actual_internal_target_and_letterboxed_canvas() {
    let surface = SurfaceSize { width: 256, height: 256 };
    let policy = CanvasRenderPolicy {
        fit_mode: CanvasFitMode::Contain,
        canvas_size: Some(CanvasSize { width: 64, height: 32 }),
    };
    let viewport = CanvasViewport::from_policy(surface, policy);
    assert_eq!(
        ScissorRect::from_canvas(CENTER, viewport, surface),
        Some(ScissorRect { x: 64, y: 96, width: 128, height: 64 })
    );
    let internal = policy.internal_render_size(surface, InternalResolutionMode::Skin).unwrap();
    assert_eq!(internal, SurfaceSize { width: 64, height: 32 });
    assert_eq!(
        ScissorRect::from_canvas(CENTER, CanvasViewport::from_policy(internal, policy), internal),
        Some(ScissorRect { x: 16, y: 8, width: 32, height: 16 })
    );
}

#[test]
fn clip_stack_preserves_hidden_geometry_text_cursors_and_effect_indices() {
    let size = SurfaceSize { width: 64, height: 64 };
    let white = Color::rgb(1.0, 1.0, 1.0);
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            DrawCommand::PushClip { rect: CENTER },
            colored_rect(FULL, white),
            sample_text(),
            solid_image(7),
            DrawCommand::PushClip { rect: OUTSIDE },
            colored_rect(FULL, white),
            sample_text(),
            solid_image(8),
            DrawCommand::RectBatch {
                rects: std::sync::Arc::from([RectCommand { rect: FULL, color: white }]),
                cache: Some(RectBatchCache { key: RectBatchCacheKey(1), bounds: FULL }),
            },
            ambient(white),
            DrawCommand::PopClip,
            solid_image(9),
            DrawCommand::PopClip,
            solid_image(10),
            sample_text(),
            ambient(white),
        ],
    };
    let text_frame = TextFrame {
        command_quad_counts: vec![2, 3, 1],
        command_caret_rects: vec![Some(RectCommand { rect: CENTER, color: white }); 3],
        ..Default::default()
    };
    let mut batch_calls = 0;
    let geometry = encode_plan_geometry_with_rect_batch_resolver(
        &plan,
        &text_frame,
        size,
        CanvasViewport::from_policy(size, CanvasRenderPolicy::default()),
        &mut |rects, _| {
            batch_calls += 1;
            assert_eq!(rects[0].rect, FULL, "offscreen sources remain uncropped");
            Some(TextureId(900))
        },
    );
    assert_eq!(batch_calls, 1);
    let clips: Vec<_> = geometry
        .steps
        .iter()
        .filter_map(|step| match step {
            DrawStep::Scissor { rect } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(
        clips,
        [
            Some(ScissorRect { x: 16, y: 16, width: 32, height: 32 }),
            None,
            Some(ScissorRect { x: 16, y: 16, width: 32, height: 32 }),
            Some(ScissorRect { x: 0, y: 0, width: 64, height: 64 }),
        ]
    );
    let images: Vec<_> = geometry
        .steps
        .iter()
        .filter_map(|step| match step {
            DrawStep::Image { texture, .. } => Some(*texture),
            _ => None,
        })
        .collect();
    assert_eq!(
        images,
        [
            TextureId(7),
            TextureId(8),
            TextureId(900),
            ambient_texture_id(0),
            TextureId(9),
            TextureId(10),
            ambient_texture_id(1)
        ]
    );
    let text: Vec<_> = geometry
        .steps
        .iter()
        .filter_map(|step| match step {
            DrawStep::Text { range } => Some(range.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        text,
        [
            0..2 * TEXT_INSTANCE_BYTES,
            2 * TEXT_INSTANCE_BYTES..5 * TEXT_INSTANCE_BYTES,
            5 * TEXT_INSTANCE_BYTES..6 * TEXT_INSTANCE_BYTES
        ]
    );
    assert_eq!(geometry.stats().rect_instances, 5, "all three text carets remain indexed");
    assert_eq!(geometry.stats().image_instances, 7);
}

#[test]
fn clip_boundaries_split_batches_and_do_not_leak_when_geometry_is_reused() {
    let size = SurfaceSize { width: 16, height: 16 };
    let viewport = CanvasViewport::from_policy(size, CanvasRenderPolicy::default());
    let mut geometry = PlanGeometry::default();
    let mut plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            solid_image(1),
            DrawCommand::PushClip { rect: CENTER },
            solid_image(1),
            DrawCommand::PopClip,
            solid_image(1),
        ],
    };
    encode_plan_geometry_into(
        &plan,
        &TextFrame::default(),
        size,
        viewport,
        &mut |_, _| None,
        &mut geometry,
    );
    assert_eq!(geometry.stats().image_steps, 3);
    plan.commands = vec![DrawCommand::PushClip { rect: OUTSIDE }, solid_image(1)];
    encode_plan_geometry_into(
        &plan,
        &TextFrame::default(),
        size,
        viewport,
        &mut |_, _| None,
        &mut geometry,
    );
    assert!(matches!(geometry.steps[0], DrawStep::Scissor { rect: None }));
    plan.commands = vec![solid_image(1)];
    encode_plan_geometry_into(
        &plan,
        &TextFrame::default(),
        size,
        viewport,
        &mut |_, _| None,
        &mut geometry,
    );
    assert!(matches!(geometry.steps.as_slice(), [DrawStep::Image { .. }]));
}

fn full_bitmap_text(color: Color) -> DrawCommand {
    let mut text = sample_text();
    let DrawCommand::Text { origin, text: value, style, .. } = &mut text else { unreachable!() };
    *origin = Point { x: 0.0, y: 0.0 };
    *value = "A".into();
    style.font_id = Some("clip-font".into());
    style.size = 1.0;
    style.color = color;
    text
}

fn pixel(pixels: &[u8], width: usize, x: usize, y: usize) -> &[u8] {
    &pixels[(y * width + x) * 4..(y * width + x + 1) * 4]
}

#[test]
#[ignore = "requires a GPU; run explicitly for clipping changes"]
fn clip_gpu_applies_to_every_draw_kind_and_restores_later_objects() {
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 64, height: 64 }).unwrap();
    eprintln!("clip GPU adapter: {:?}", renderer.gpu.as_ref().unwrap().adapter_info);
    renderer.upsert_rgba_texture_ref(TextureId(900), 1, 1, &[255, 0, 0, 255]).unwrap();
    renderer.upsert_rgba_texture_ref(TextureId(901), 1, 1, &[255, 255, 255, 255]).unwrap();
    let mut font = test_bitmap_font();
    font.size = 1;
    font.line_height = 1;
    // This one-pixel glyph should cover the whole target from its top edge.
    // The shared fixture's ascent=7 would place it 448px above this 64px target.
    font.base = 0;
    font.ascent = 0.0;
    renderer.install_bitmap_font("clip-font", font);
    let white = Color::rgb(1.0, 1.0, 1.0);
    let red = Color::rgb(1.0, 0.0, 0.0);
    renderer.last_plan = Some(DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![full_bitmap_text(white)],
    });
    renderer.render_last_plan().unwrap();
    let unclipped_text = renderer.read_offscreen_rgba().unwrap();
    for (x, y) in [(12, 32), (32, 12), (32, 32), (52, 32), (32, 52)] {
        assert_eq!(
            pixel(&unclipped_text, 64, x, y),
            &[255, 255, 255, 255],
            "unclipped text fixture: {x},{y}"
        );
    }
    let batch = |cache| DrawCommand::RectBatch {
        rects: std::sync::Arc::from([RectCommand { rect: FULL, color: white }]),
        cache,
    };
    let kinds = [
        colored_rect(FULL, white),
        solid_image(901),
        DrawCommand::RotatedImage {
            rect: FULL,
            uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: TextureId(901),
            tint: white,
            blend: BlendMode::Normal,
            linear_filter: false,
            angle_rad: std::f32::consts::FRAC_PI_4,
            center: Point { x: 0.5, y: 0.5 },
            post_scale: Point { x: 1.0, y: 1.0 },
        },
        full_bitmap_text(white),
        batch(None),
        batch(Some(RectBatchCache { key: RectBatchCacheKey(98), bounds: FULL })),
        ambient(white),
    ];
    for (index, command) in kinds.into_iter().enumerate() {
        renderer.last_plan = Some(DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![
                DrawCommand::PushClip { rect: OUTSIDE },
                solid_image(900),
                full_bitmap_text(red),
                ambient(red),
                DrawCommand::PopClip,
                DrawCommand::PushClip { rect: CENTER },
                DrawCommand::PushClip { rect: Rect { x: 0.5, y: 0.0, width: 0.5, height: 1.0 } },
                command,
                DrawCommand::PopClip,
                DrawCommand::PopClip,
                colored_rect(
                    Rect { x: 0.0, y: 0.0, width: 0.125, height: 0.125 },
                    Color::rgb(0.0, 1.0, 0.0),
                ),
            ],
        });
        renderer.render_last_plan().unwrap();
        let pixels = renderer.read_offscreen_rgba().unwrap();
        assert_eq!(pixel(&pixels, 64, 32, 32), &[255, 255, 255, 255], "draw kind {index}");
        assert_eq!(pixel(&pixels, 64, 12, 32), &[0, 0, 0, 255], "left clip: {index}");
        assert_eq!(pixel(&pixels, 64, 20, 32), &[0, 0, 0, 255], "nested clip: {index}");
        assert_eq!(pixel(&pixels, 64, 52, 32), &[0, 0, 0, 255], "right clip: {index}");
        assert_eq!(pixel(&pixels, 64, 32, 12), &[0, 0, 0, 255], "top clip: {index}");
        assert_eq!(pixel(&pixels, 64, 32, 52), &[0, 0, 0, 255], "bottom clip: {index}");
        assert_eq!(pixel(&pixels, 64, 4, 4), &[0, 255, 0, 255], "pop restores: {index}");
    }
    renderer.last_plan.as_mut().unwrap().commands =
        vec![DrawCommand::PushClip { rect: OUTSIDE }, solid_image(900)];
    renderer.render_last_plan().unwrap();
    renderer.last_plan.as_mut().unwrap().commands = vec![solid_image(901)];
    renderer.render_last_plan().unwrap();
    assert_eq!(
        pixel(&renderer.read_offscreen_rgba().unwrap(), 64, 4, 4),
        &[255, 255, 255, 255],
        "a new frame resets even an unmatched push"
    );
}

#[test]
#[ignore = "requires a GPU; run explicitly for clipping changes"]
fn clip_gpu_respects_letterboxing_internal_resolution_and_uncropped_ambient_source() {
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 256, height: 256 }).unwrap();
    eprintln!("clip GPU adapter: {:?}", renderer.gpu.as_ref().unwrap().adapter_info);
    renderer.last_plan_canvas_policy = CanvasRenderPolicy {
        fit_mode: CanvasFitMode::Contain,
        canvas_size: Some(CanvasSize { width: 64, height: 32 }),
    };
    for mode in [InternalResolutionMode::Native, InternalResolutionMode::Skin] {
        renderer.set_internal_resolution_mode(mode);
        renderer.last_plan = Some(DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![
                DrawCommand::PushClip { rect: CENTER },
                colored_rect(FULL, Color::rgb(1.0, 1.0, 1.0)),
                DrawCommand::PopClip,
            ],
        });
        renderer.render_last_plan().unwrap();
        let pixels = renderer.read_offscreen_rgba().unwrap();
        assert_eq!(pixel(&pixels, 256, 128, 128), &[255, 255, 255, 255], "{mode:?}");
        for (x, y) in [(128, 32), (128, 80), (32, 128), (224, 128), (128, 176)] {
            assert_eq!(pixel(&pixels, 256, x, y), &[0, 0, 0, 255], "{mode:?}: {x},{y}");
        }
    }
    renderer.set_internal_resolution_mode(InternalResolutionMode::Native);
    renderer.last_plan_canvas_policy = CanvasRenderPolicy::default();
    let effect = DrawCommand::Ambient {
        rect: CENTER,
        blur: 100.0,
        fade_edges: true,
        layers: vec![colored_rect(CENTER, Color::rgb(1.0, 0.0, 0.0))],
    };
    renderer.last_plan =
        Some(DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands: vec![effect.clone()] });
    renderer.render_last_plan().unwrap();
    let original = renderer.read_offscreen_rgba().unwrap();
    let clip = Rect { x: 0.0, y: 0.0, width: 0.25, height: 1.0 };
    renderer.last_plan.as_mut().unwrap().commands =
        vec![DrawCommand::PushClip { rect: clip }, effect, DrawCommand::PopClip];
    renderer.render_last_plan().unwrap();
    let clipped = renderer.read_offscreen_rgba().unwrap();
    assert!(pixel(&original, 256, 60, 128)[0] > 0, "blur reaches outside source content");
    assert_eq!(
        pixel(&clipped, 256, 60, 128),
        pixel(&original, 256, 60, 128),
        "outer clip does not crop the blur input"
    );
    assert_eq!(pixel(&clipped, 256, 128, 128), &[0, 0, 0, 255]);
}
