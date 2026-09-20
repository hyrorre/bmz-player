use super::*;

#[test]
#[ignore = "requires a GPU; run explicitly for texture upload changes"]
fn reusable_texture_uploads_preserve_pixels_order_and_resize() {
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 257, height: 129 }).unwrap();
    let id = TextureId(900);
    renderer.last_plan = Some(DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Image {
            rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: id,
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            linear_filter: false,
        }],
    });
    // Odd-width rows, slot exhaustion before a draw, subsequent reuse, and
    // same-ID resize must all retain the last submitted pixels.
    for (width, height) in [(257, 129), (640, 360), (257, 129), (1, 1)] {
        for cycle in 0..2 {
            for color in [
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [255, 255, 0, 255],
                [0, 255, 255, 255],
            ] {
                renderer
                    .upsert_rgba_texture_ref(
                        id,
                        width,
                        height,
                        &color.repeat((width * height) as usize),
                    )
                    .unwrap();
            }
            renderer.render_last_plan().unwrap();
            assert_eq!(
                renderer.read_offscreen_rgba().unwrap(),
                [0, 255, 255, 255].repeat(257 * 129),
                "size {width}x{height}, cycle {cycle}"
            );
        }
    }
    for phase in 0..2 {
        let pixels: Vec<_> = (0..129)
            .flat_map(|y| {
                (0..257).flat_map(move |x| {
                    [
                        ((x + phase) % 2 * 255) as u8,
                        (y % 2 * 255) as u8,
                        ((x + y) % 2 * 255) as u8,
                        255,
                    ]
                })
            })
            .collect();
        renderer.upsert_rgba_texture_ref(id, 257, 129, &pixels).unwrap();
        renderer.render_last_plan().unwrap();
        assert_eq!(renderer.read_offscreen_rgba().unwrap(), pixels);
    }
}

#[test]
fn gpu_texture_validation_rejects_oversized_images_before_allocation() {
    assert!(validate_rgba_texture_for_device(4, 5, 1, &[0; 20]).is_err());
    assert!(validate_rgba_texture_for_device(4, 1, 5, &[0; 20]).is_err());
    assert!(validate_rgba_texture_for_device(4, 4, 4, &[0; 64]).is_ok());
    assert!(validate_rgba_texture_for_device(4, 0, 1, &[]).is_err());
    assert!(validate_rgba_texture_for_device(4, 1, 1, &[0; 3]).is_err());
}

#[test]
fn renderer_queues_texture_assets_before_surface_attach() {
    let mut renderer = Renderer::default();
    let asset = crate::assets::RgbaImageAsset { width: 1, height: 1, pixels: vec![255, 0, 0, 255] };

    renderer.upsert_image_asset(crate::plan::TextureId(9), &asset).unwrap();

    assert_eq!(renderer.pending_textures.len(), 1);
    assert_eq!(renderer.pending_textures[0].id, crate::plan::TextureId(9));
}

#[test]
fn installing_vector_font_replaces_stale_bitmap_font_with_same_id() {
    let Some(font) = load_default_font() else { return };
    let mut renderer = Renderer::default();

    renderer.insert_bitmap_font_entry("play:0".to_string(), test_bitmap_font());
    renderer.insert_vector_font("play:0".to_string(), font);

    assert!(renderer.fonts.contains_key("play:0"));
    assert!(!renderer.bitmap_fonts.contains_key("play:0"));
}

#[test]
fn installing_bitmap_font_replaces_stale_vector_font_with_same_id() {
    let Some(font) = load_default_font() else { return };
    let mut renderer = Renderer::default();

    renderer.insert_vector_font("play:0".to_string(), font);
    renderer.insert_bitmap_font_entry("play:0".to_string(), test_bitmap_font());

    assert!(renderer.bitmap_fonts.contains_key("play:0"));
    assert!(!renderer.fonts.contains_key("play:0"));
}
