use super::*;

#[test]
#[ignore = "requires a GPU; run explicitly for ambient rendering changes"]
fn ambient_gpu_blurs_composited_layers_updates_video_and_releases_targets() {
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 256, height: 144 }).unwrap();
    let full = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
    let source = |id, tint, blend| DrawCommand::Image {
        rect: full,
        uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        source_size: None,
        texture: TextureId(id),
        tint,
        blend,
        linear_filter: true,
    };
    let pixels: Vec<u8> = (0..144)
        .flat_map(|_| {
            (0..256).flat_map(|x| if x < 128 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })
        })
        .collect();
    renderer.upsert_rgba_texture_ref(TextureId(900), 256, 144, &pixels).unwrap();
    renderer.upsert_rgba_texture_ref(TextureId(901), 1, 1, &[0, 0, 0, 255]).unwrap();
    let layers = vec![
        source(900, Color::rgb(1.0, 1.0, 1.0), BlendMode::Normal),
        source(901, Color::rgb(1.0, 1.0, 1.0), BlendMode::LayerMask),
    ];
    renderer.last_plan = Some(DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            DrawCommand::Ambient { rect: full, layers },
            DrawCommand::Rect {
                rect: Rect { x: 0.1, y: 0.1, width: 0.1, height: 0.1 },
                color: Color::rgb(0.0, 1.0, 0.0),
            },
        ],
    });
    renderer.render_last_plan().unwrap();
    let pixels = renderer.read_offscreen_rgba().unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 256 + x) * 4..(y * 256 + x + 1) * 4];
    assert_eq!(pixel(38, 21), &[0, 255, 0, 255], "foreground stays sharp");
    assert!(pixel(125, 72)[0] > 20 && pixel(125, 72)[2] > 20, "blur crosses the source edge");
    assert!(pixel(131, 72)[0] > 20 && pixel(131, 72)[2] > 20);
    assert!(pixel(10, 72)[0] > 245, "black-keyed layer preserves the base");
    // Updating the same texture twice must use the last uploaded video frame in this submission.
    for color in [[255, 255, 0, 255], [0, 255, 255, 255]] {
        renderer
            .upsert_rgba_texture_ref(TextureId(900), 256, 144, &color.repeat(256 * 144))
            .unwrap();
    }
    renderer.render_last_plan().unwrap();
    let pixels = renderer.read_offscreen_rgba().unwrap();
    assert_eq!(&pixels[(72 * 256 + 128) * 4..(72 * 256 + 128) * 4 + 4], &[0, 255, 255, 255]);
    // Half-transparent red is composited just once (premultiplied blur output).
    renderer.upsert_rgba_texture_ref(TextureId(900), 1, 1, &[255, 0, 0, 255]).unwrap();
    renderer.last_plan.as_mut().unwrap().commands = vec![DrawCommand::Ambient {
        rect: full,
        layers: vec![source(900, Color::rgba(1.0, 1.0, 1.0, 0.5), BlendMode::Normal)],
    }];
    renderer.render_last_plan().unwrap();
    let pixels = renderer.read_offscreen_rgba().unwrap();
    assert!(
        (125..=130).contains(&pixels[0]),
        "half red is composited once into the unorm offscreen target: {}",
        pixels[0]
    );
    // Shrink/change aspect, then disable: targets must resize and disappear.
    if let DrawCommand::Ambient { rect, .. } = &mut renderer.last_plan.as_mut().unwrap().commands[0]
    {
        rect.width = 0.25;
    }
    renderer.render_last_plan().unwrap();
    assert_eq!(renderer.gpu.as_ref().unwrap().image_textures[&ambient_texture_id(0)].height, 128);
    renderer.last_plan.as_mut().unwrap().commands.clear();
    renderer.render_last_plan().unwrap();
    let gpu = renderer.gpu.as_ref().unwrap();
    assert!(gpu.ambient.is_none());
    assert!(!gpu.image_textures.contains_key(&ambient_texture_id(0)));
}
