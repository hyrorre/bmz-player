use super::*;

#[test]
fn surface_size_requires_non_zero_dimensions() {
    assert!(SurfaceSize { width: 1, height: 1 }.is_drawable());
    assert!(!SurfaceSize { width: 0, height: 1 }.is_drawable());
    assert!(!SurfaceSize { width: 1, height: 0 }.is_drawable());
}

#[test]
fn canvas_viewport_expand_uses_full_surface() {
    let viewport = CanvasViewport::from_policy(
        SurfaceSize { width: 320, height: 240 },
        CanvasRenderPolicy::default(),
    );

    assert_eq!(viewport.rect, Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 });
    assert!(viewport.is_identity());
    assert_eq!(viewport.content_size(), SurfaceSize { width: 320, height: 240 });
}

#[test]
fn skin_internal_resolution_only_downscales_larger_surface_content() {
    let policy = CanvasRenderPolicy {
        fit_mode: CanvasFitMode::Contain,
        canvas_size: Some(CanvasSize { width: 1920, height: 1080 }),
    };
    let four_k = SurfaceSize { width: 3840, height: 2160 };

    assert_eq!(
        policy.internal_render_size(four_k, InternalResolutionMode::Skin),
        Some(SurfaceSize { width: 1920, height: 1080 })
    );
    assert_eq!(policy.internal_render_size(four_k, InternalResolutionMode::Native), None);
    assert_eq!(
        policy.internal_render_size(
            SurfaceSize { width: 1280, height: 720 },
            InternalResolutionMode::Skin,
        ),
        None
    );
    assert_eq!(
        CanvasRenderPolicy::default().internal_render_size(four_k, InternalResolutionMode::Skin),
        None
    );
}

#[test]
fn skin_internal_resolution_keeps_surface_contain_rect_for_final_upscale() {
    let surface = SurfaceSize { width: 1000, height: 1000 };
    let policy = CanvasRenderPolicy {
        fit_mode: CanvasFitMode::Contain,
        canvas_size: Some(CanvasSize { width: 320, height: 180 }),
    };

    assert_eq!(
        policy.internal_render_size(surface, InternalResolutionMode::Skin),
        Some(SurfaceSize { width: 320, height: 180 })
    );
    let output = CanvasViewport::from_policy(surface, policy);
    assert_approx(output.rect.x, 0.0);
    assert_approx(output.rect.y, 0.21875);
    assert_approx(output.rect.width, 1.0);
    assert_approx(output.rect.height, 0.5625);
    assert!(
        CanvasViewport::from_policy(SurfaceSize { width: 320, height: 180 }, policy).is_identity()
    );
}

#[test]
fn canvas_viewport_contain_same_aspect_uses_identity_transform() {
    let viewport = CanvasViewport::from_policy(
        SurfaceSize { width: 2560, height: 1440 },
        CanvasRenderPolicy {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: 1920, height: 1080 }),
        },
    );

    assert!(viewport.is_identity());
    assert_eq!(viewport.content_size(), SurfaceSize { width: 2560, height: 1440 });
}

#[test]
fn canvas_viewport_contain_letterboxes_tall_surface() {
    let viewport = CanvasViewport::from_policy(
        SurfaceSize { width: 1000, height: 1000 },
        CanvasRenderPolicy {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: 16, height: 9 }),
        },
    );

    assert_approx(viewport.rect.x, 0.0);
    assert_approx(viewport.rect.y, 0.21875);
    assert_approx(viewport.rect.width, 1.0);
    assert_approx(viewport.rect.height, 0.5625);
    assert!(!viewport.is_identity());
    assert_eq!(viewport.content_size(), SurfaceSize { width: 1000, height: 563 });
}

#[test]
fn canvas_viewport_maps_surface_points_back_to_canvas() {
    let viewport = CanvasViewport::from_policy(
        SurfaceSize { width: 1000, height: 1000 },
        CanvasRenderPolicy {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: 16, height: 9 }),
        },
    );

    let (x, y) = viewport.surface_to_canvas_point(0.5, 0.5).unwrap();
    assert_approx(x, 0.5);
    assert_approx(y, 0.5);
    assert!(viewport.surface_to_canvas_point(0.5, 0.1).is_none());
}

#[test]
fn plan_geometry_applies_canvas_viewport_to_images() {
    let surface = SurfaceSize { width: 1000, height: 1000 };
    let viewport = CanvasViewport::from_policy(
        surface,
        CanvasRenderPolicy {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: 16, height: 9 }),
        },
    );
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Image {
            rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: TextureId(0),
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            linear_filter: false,
        }],
    };

    let geometry = encode_plan_geometry_with_rect_batch_resolver(
        &plan,
        &TextFrame::default(),
        surface,
        viewport,
        &mut |_, _| None,
    );
    let floats: Vec<f32> =
        geometry.images.as_chunks::<4>().0.iter().map(|bytes| f32::from_le_bytes(*bytes)).collect();

    assert_approx(floats[0], 0.0);
    assert_approx(floats[1], 0.21875);
    assert_approx(floats[2], 1.0);
    assert_approx(floats[3], 0.5625);
}

#[test]
fn sampling_uv_insets_subregions_by_half_texel() {
    let uv = sampling_uv_with_half_texel_inset(
        UvRect { x: 0.25, y: 0.5, width: 0.125, height: 0.25 },
        Some(SkinImageSize { width: 256.0, height: 128.0 }),
    );

    assert_approx(uv.x, 0.25 + 0.5 / 256.0);
    assert_approx(uv.y, 0.5 + 0.5 / 128.0);
    assert_approx(uv.width, 0.125 - 1.0 / 256.0);
    assert_approx(uv.height, 0.25 - 1.0 / 128.0);
}

#[test]
fn sampling_uv_keeps_full_texture_axes_unchanged() {
    let uv = sampling_uv_with_half_texel_inset(
        UvRect { x: 0.0, y: 0.25, width: 1.0, height: 0.5 },
        Some(SkinImageSize { width: 256.0, height: 128.0 }),
    );

    assert_approx(uv.x, 0.0);
    assert_approx(uv.width, 1.0);
    assert_approx(uv.y, 0.25 + 0.5 / 128.0);
    assert_approx(uv.height, 0.5 - 1.0 / 128.0);
}

#[test]
fn sampling_uv_does_not_collapse_single_texel_regions() {
    let uv = sampling_uv_with_half_texel_inset(
        UvRect { x: 0.25, y: 0.5, width: 1.0 / 256.0, height: 1.0 / 128.0 },
        Some(SkinImageSize { width: 256.0, height: 128.0 }),
    );

    assert_approx(uv.x, 0.25);
    assert_approx(uv.y, 0.5);
    assert_approx(uv.width, 1.0 / 256.0);
    assert_approx(uv.height, 1.0 / 128.0);
}

#[test]
fn text_instances_are_transformed_into_canvas_viewport() {
    let viewport = CanvasViewport::from_policy(
        SurfaceSize { width: 1000, height: 1000 },
        CanvasRenderPolicy {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: 16, height: 9 }),
        },
    );
    let mut instances = Vec::new();
    for value in [0.1_f32, 0.2, 0.3, 0.4, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0] {
        instances.extend_from_slice(&value.to_le_bytes());
    }

    viewport.transform_text_instances(&mut instances);
    let floats: Vec<f32> =
        instances.as_chunks::<4>().0.iter().map(|bytes| f32::from_le_bytes(*bytes)).collect();

    assert_approx(floats[0], 0.1);
    assert_approx(floats[1], 0.33125);
    assert_approx(floats[2], 0.3);
    assert_approx(floats[3], 0.225);
    assert_approx(floats[4], 0.0);
    assert_approx(floats[8], 1.0);
}

#[test]
fn text_wrapping_splits_lines_by_max_width() {
    let Some(font) = load_default_font() else { return };
    let scale = PxScale::from(16.0);
    let scaled_font = font.as_scaled(scale);
    let one_char_width = text_width_px("W", &font, &scaled_font);
    let lines = wrap_text_to_width("WWW", &font, &scaled_font, one_char_width * 1.5);

    assert_eq!(lines, vec!["W", "W", "W"]);
}

#[test]
fn text_shadow_emits_extra_text_instances() {
    let Some(font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 320, height: 240 };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "A".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
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
                shadow: Some(crate::plan::TextShadow {
                    color: Color::rgba(0.0, 0.0, 0.0, 0.5),
                    offset: Point { x: 0.01, y: 0.01 },
                }),
            },
        }],
    };
    let frame = build_text_frame(&plan, &font, &HashMap::new(), &HashMap::new(), surface);

    assert_eq!(frame.instances.len(), TEXT_INSTANCE_BYTES * 2);
}

#[test]
fn cached_text_frame_only_marks_new_glyphs_dirty() {
    let Some(font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 320, height: 240 };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "FPS 20".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
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
            },
        }],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

    let first = build_text_frame_with_cache(
        &plan,
        &font,
        &HashMap::new(),
        &HashMap::new(),
        surface,
        &mut atlas,
    );
    let second = build_text_frame_with_cache(
        &plan,
        &font,
        &HashMap::new(),
        &HashMap::new(),
        surface,
        &mut atlas,
    );

    assert!(!first.instances.is_empty());
    assert!(!first.dirty_regions.is_empty());
    assert_eq!(second.instances.len(), first.instances.len());
    assert!(second.dirty_regions.is_empty());
}

#[test]
fn cached_text_frame_reuses_static_text_layouts() {
    let Some(font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 320, height: 240 };
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
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "STATIC".to_string(),
                caret: None,
                post_scale: Point { x: 1.0, y: 1.0 },
                style: style.clone(),
            },
            DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "STATIC".to_string(),
                caret: None,
                post_scale: Point { x: 1.0, y: 1.0 },
                style,
            },
        ],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

    let first = build_text_frame_with_cache(
        &plan,
        &font,
        &HashMap::new(),
        &HashMap::new(),
        surface,
        &mut atlas,
    );
    let layout_count = atlas.layouts.len();
    let second = build_text_frame_with_cache(
        &plan,
        &font,
        &HashMap::new(),
        &HashMap::new(),
        surface,
        &mut atlas,
    );

    assert!(!first.instances.is_empty());
    assert_eq!(layout_count, 1);
    assert_eq!(second.instances, first.instances);
    assert!(second.dirty_regions.is_empty());
}

#[test]
fn cached_decorated_skin_text_tracks_alpha_for_vector_and_bitmap_fonts() {
    use crate::skin::{SkinDocumentRenderExt, SkinDrawState, SkinRenderItem, SkinTextState};
    let font = load_default_font().expect("font required for vector regression test");
    let bitmap = test_bitmap_font();
    let surface = SurfaceSize { width: 320, height: 240 };
    for bitmap_mode in [false, true] {
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let mut document: bmz_skin_document::SkinDocument = serde_json::from_str(
            r#"{
            "w":100,"h":100,
            "text":[{"id":"label","constantText":"A","outlineWidth":1,
                "outlineColor":"ff000080","shadowColor":"00ff0040"}],
            "destination":[{"id":"label","dst":[{"h":10,"a":255}]}]
        }"#,
        )
        .unwrap();
        if bitmap_mode {
            document.text[0].font = "bitmap".into();
        }
        let mut original = None;
        for a in [255, 128, 0, 255] {
            let bmz_skin_document::DestinationListEntry::Single(destination) =
                &mut document.destination[0]
            else {
                panic!("destination")
            };
            destination.dst[0] = serde_json::from_value(serde_json::json!({"h":10,"a":a})).unwrap();
            let items = document.static_render_items(
                &HashMap::new(),
                &SkinDrawState::default(),
                &SkinTextState::default(),
            );
            let SkinRenderItem::Text { origin, text, style, .. } = &items[0] else {
                panic!("text")
            };
            atlas.begin_frame();
            let mut builder = CachedTextFrameBuilder::new(&mut atlas);
            if bitmap_mode {
                builder.push_bitmap_text(origin, text, style.clone(), "bitmap", &bitmap, surface);
            } else {
                builder.push_text(
                    origin,
                    text,
                    style.clone(),
                    VectorFontSet::Single { cache_id: "vector", font: &font },
                    surface,
                );
            }
            let quads = builder.quads.clone();
            let alpha = a as f32 / 255.0;
            assert_eq!(quads.len(), if a == 0 { 1 } else { 10 });
            assert_approx(quads.last().unwrap().color.a, alpha);
            if a != 0 {
                assert_approx(quads[0].color.a, 64.0 / 255.0 * alpha);
                for quad in &quads[1..9] {
                    assert_eq!((quad.color.r, quad.color.g, quad.color.b), (1.0, 0.0, 0.0));
                    assert_approx(quad.color.a, 128.0 / 255.0 * alpha);
                }
            }
            if a == 255 {
                let values = quads
                    .iter()
                    .map(|quad| (quad.x, quad.y, quad.width, quad.height, quad.color))
                    .collect::<Vec<_>>();
                if let Some(first) = &original {
                    assert_eq!(&values, first);
                } else {
                    original = Some(values);
                }
            }
        }
    }
}
