use super::*;
use bmz_render::renderer::SurfaceSize;
use bmz_render::skin::SkinDstEntry;
use bmz_render::snapshot::RenderSnapshot;

fn antique_play_skin_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin")
}

fn decode_antique_play_skin(skin_path: &Path) -> DecodedSkin {
    let library_roots = test_app_paths().skin_library_roots();
    decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files: &BTreeMap::new(),
        runtime_state: &LuaLoadRuntimeState::default(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: &library_roots,
    })
    .expect("decode Antique skin with the app skin-library root")
}

#[test]
fn antique_progress_slider_resolves_its_source_and_moves_with_play_progress_when_available() {
    let skin_path = antique_play_skin_path();
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_antique_play_skin(&skin_path);
    let slider = decoded
        .document
        .slider
        .iter()
        .find(|slider| slider.id == "sld_progress_song")
        .expect("Antique song progress slider");
    assert_eq!(slider.src, "src_progress_song");
    assert_eq!((slider.x, slider.y, slider.w, slider.h), (0, 0, -1, -1));
    assert_eq!((slider.angle, slider.range, slider.slider_type), (2, 617, 6));

    let source = decoded
        .sources
        .iter()
        .find(|source| source.source_id == slider.src)
        .expect("Antique progress image source should resolve within the skin library");
    assert!(source.path.is_file(), "resolved progress image: {}", source.path.display());
    assert!(source.size.width > 0.0 && source.size.height > 0.0);
    let texture = source.texture;
    let document_sources = decoded
        .sources
        .iter()
        .map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: source.size,
        })
        .map(|source| (source.source_id.clone(), source))
        .collect::<HashMap<_, _>>();

    for (progress, expected_y) in [(0.0, 1014), (0.5, 705), (1.0, 397)] {
        let items = decoded.document.static_image_render_items(
            &document_sources,
            &SkinDrawState {
                play_progress: progress,
                rhythm_timer_ms: Some(0),
                ..SkinDrawState::default()
            },
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                SkinRenderItem::Image { texture: item_texture, rect, uv, .. }
                    if *item_texture == texture
                        && uv.width > 0.0 && uv.height > 0.0
                        && (rect.y - 1.0 + expected_y as f32 / 1080.0 + 42.0 / 1080.0).abs() < 0.001
            )),
            "Antique progress slider should have positive UVs and move to y={expected_y} at {progress}"
        );
    }
}

#[test]
#[ignore = "requires an offscreen GPU adapter and the bundled Antique skin"]
fn antique_loading_progress_keeps_the_label_and_percentage_visible_on_gpu() {
    let skin_path = antique_play_skin_path();
    assert!(skin_path.is_file(), "initialize the mz-select submodule");
    let decoded = decode_antique_play_skin(&skin_path);
    let document = &decoded.document;
    let size = SurfaceSize { width: document.w, height: document.h };
    let loading = document
        .destination
        .iter()
        .find_map(|entry| match entry {
            DestinationListEntry::Single(dst) if dst.id == "img_loading" => dst.dst.first(),
            _ => None,
        })
        .expect("Antique loading label destination");
    let SkinDstEntry::Frame(loading) = loading else { panic!("expected a plain loading frame") };
    let left = loading.x.unwrap() as usize;
    let top = (document.h as i32 - loading.y.unwrap() - loading.h.unwrap()) as usize;
    let loading_texture =
        decoded.sources.iter().find(|source| source.source_id == "src_loading").unwrap().texture;

    let mut renderer = Renderer::default();
    renderer.attach_offscreen(size).unwrap();
    install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
    let mut render = |progress, ready_elapsed_time| {
        renderer.prepare_scene(AppSceneSnapshot::Play(RenderSnapshot {
            time: TimeUs(-1_000_000),
            play_elapsed_time: TimeUs(500_000),
            ready_elapsed_time,
            resource_load_progress: progress,
            key_mode: KeyMode::K7,
            ..RenderSnapshot::default()
        }));
        renderer.render_last_plan().unwrap();
        renderer.read_offscreen_rgba().unwrap()
    };
    let channel = |pixels: &[u8], x, y| pixels[((top + y) * size.width as usize + left + x) * 4];

    let start = render(0.0, None);
    let halfway = render(0.5, None);
    let complete = render(1.0, None);
    // These neighboring pixels are the "N" and the background of the label.
    assert!(channel(&start, 8, 17) > 160 && channel(&start, 4, 17) < 20);
    for pixels in [&halfway, &complete] {
        assert!(channel(pixels, 8, 17) < 90 && channel(pixels, 4, 17) > 160);
    }
    // At 50% the fill has not reached the number. Pixel changes here come from its digits.
    assert!((5..22).any(|y| (145..228).any(|x| channel(&start, x, y) != channel(&halfway, x, y))));
    assert!(channel(&halfway, 229, 20) < 20 && channel(&complete, 229, 20) > 160);

    render(1.0, Some(TimeUs(250_000)));
    assert!(!renderer.last_plan().unwrap().commands.iter().any(|command| matches!(
        command,
        DrawCommand::Image { texture, .. } | DrawCommand::RotatedImage { texture, .. }
            if texture.0 == loading_texture.0
    )));
}
