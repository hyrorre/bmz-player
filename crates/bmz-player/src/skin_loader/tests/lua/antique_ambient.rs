use super::*;

fn antique_bga_brightness_state(front: i32, background: i32) -> LuaLoadRuntimeState {
    LuaLoadRuntimeState {
        offset_id_values: BTreeMap::from([
            (54, bmz_skin::LuaSkinOffsetValue { a: front, ..Default::default() }),
            (65, bmz_skin::LuaSkinOffsetValue { a: background, ..Default::default() }),
        ]),
        ..Default::default()
    }
}

#[test]
fn antique_bga_brightness_is_separate_for_foreground_background_and_fallback_when_available() {
    use bmz_render::skin::SkinDstEntry;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !path.is_file() {
        return;
    }
    for size in ["x2(512x512)", "背景(1920x1080)"] {
        for (ambient, mode) in [("OFF", "全体"), ("ON", "全体"), ("ON", "Spread")] {
            for (front, background) in [(0, 0), (-128, -64), (-500, 100)] {
                let document = load_skin_document(
                    &path,
                    SkinKind::Play,
                    &BTreeMap::from([
                        ("BGAサイズ".into(), size.into()),
                        ("Ambient".into(), ambient.into()),
                        ("Ambient表示方式".into(), mode.into()),
                    ]),
                    &BTreeMap::new(),
                    &antique_bga_brightness_state(front, background),
                    None,
                )
                .unwrap()
                .document;
                for d in document.all_destinations(&document.enabled_options()) {
                    if matches!(d.id.as_str(), "bga" | "img_bga_bgi") {
                        let brightness = if d.ambient || size.starts_with("背景") || d.stretch == 3
                        {
                            background
                        } else {
                            front
                        };
                        let SkinDstEntry::Frame(frame) = &d.dst[0] else { panic!("BGA frame") };
                        assert_eq!(
                            [frame.r, frame.g, frame.b],
                            [Some((255 + brightness).clamp(0, 255)); 3]
                        );
                        assert_eq!(
                            frame.a.unwrap_or(255),
                            if d.ambient {
                                210
                            } else if d.stretch == 3 {
                                64
                            } else {
                                255
                            }
                        );
                    }
                    if d.id == "-110" && d.loop_time == Some(500) && d.timer.is_none() {
                        assert!(
                            matches!(d.dst.last(), Some(SkinDstEntry::Frame(frame)) if frame.a == Some(0)),
                            "old dim overlay must finish transparent"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU and antique assets; verifies independent brightness in rendered pixels"]
fn antique_bga_brightness_gpu_changes_only_its_own_visible_region() {
    use bmz_render::plan::TextureId;
    use bmz_render::renderer::SurfaceSize;
    use bmz_render::snapshot::{DisplayBgaFrame, RenderSnapshot};
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    assert!(path.is_file(), "initialize the mz-select submodule");
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 1920, height: 1080 }).unwrap();
    renderer
        .upsert_rgba_texture_ref(
            TextureId(700_001),
            640,
            360,
            &[240, 160, 80, 255].repeat(640 * 360),
        )
        .unwrap();
    for (name, ambient, mode, size, blur) in [
        ("off", "OFF", "全体", "x2(512x512)", "50%"),
        ("full", "ON", "全体", "x2(512x512)", "50%"),
        ("spread", "ON", "Spread", "x2(512x512)", "50%"),
        ("spread-zero", "ON", "Spread", "x2(512x512)", "0%"),
        ("background-off", "OFF", "全体", "背景(1920x1080)", "50%"),
        ("background-full", "ON", "全体", "背景(1920x1080)", "50%"),
        ("background-spread", "ON", "Spread", "背景(1920x1080)", "50%"),
    ] {
        let mut frames = Vec::new();
        for (setting, front, background) in
            [("normal", 0, 0), ("front-dark", -255, 0), ("background-dark", 0, -255)]
        {
            let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
                pinned_sources: None,
                skin_path: &path,
                kind: SkinKind::Play,
                options: &BTreeMap::from([
                    ("Ambient".into(), ambient.into()),
                    ("Ambient表示方式".into(), mode.into()),
                    ("BGAサイズ".into(), size.into()),
                    ("Ambientぼかし度 (%)".into(), blur.into()),
                ]),
                files: &BTreeMap::new(),
                runtime_state: &antique_bga_brightness_state(front, background),
                library_roots: &[Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins")],
                document_cache: None,
                source_cache: None,
                texture_cache: None,
                font_cache: None,
                installed_fonts: None,
            })
            .unwrap();
            install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
            renderer.prepare_scene(AppSceneSnapshot::Play(RenderSnapshot {
                time: TimeUs(10_000_000),
                play_elapsed_time: TimeUs(15_000_000),
                ready_elapsed_time: Some(TimeUs(12_000_000)),
                resources_loaded: true,
                key_mode: KeyMode::K7,
                has_bga: true,
                bga_enabled: true,
                bga_base: Some(DisplayBgaFrame::opaque(700_001, 640.0, 360.0)),
                ..Default::default()
            }));
            renderer.render_last_plan().unwrap();
            let pixels = renderer.read_offscreen_rgba().unwrap();
            if let Some(directory) = std::env::var_os("BMZ_AMBIENT_PREVIEW_DIR") {
                let directory = PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                image::save_buffer(
                    directory.join(format!("brightness-{name}-{setting}.png")),
                    &pixels,
                    1920,
                    1080,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            frames.push(pixels);
        }
        let pixel = |frame: usize, x: usize, y: usize| {
            &frames[frame][(y * 1920 + x) * 4..(y * 1920 + x) * 4 + 3]
        };
        assert!(pixel(0, 1380, 540)[0] > 30, "visible BGA: {name}");
        if size.starts_with("背景") {
            assert_eq!(
                frames[0], frames[1],
                "foreground setting is inactive for background size: {name}"
            );
            assert_eq!(pixel(2, 1380, 540), &[0, 0, 0], "background dims the whole BGA: {name}");
        } else {
            assert_eq!(pixel(1, 1380, 540), &[0, 0, 0], "foreground stays opaque: {name}");
            assert_eq!(
                pixel(0, 1380, 540),
                pixel(2, 1380, 540),
                "background must not dim foreground: {name}"
            );
            // Actual fit-inside image is x=1124..1636, y=396..684; outside includes letterbox and Ambient.
            for y in 0..1080 {
                for x in 0..1920 {
                    if !(1123..1637).contains(&x) || !(395..685).contains(&y) {
                        assert_eq!(
                            pixel(0, x, y),
                            pixel(1, x, y),
                            "front brightness leaked at {x},{y}: {name}"
                        );
                    }
                }
            }
            let background_y = if blur == "0%" { 375 } else { 360 };
            assert!(pixel(0, 1380, background_y)[0] > 2, "visible background: {name}");
            assert_eq!(
                pixel(2, 1380, background_y),
                &[0, 0, 0],
                "background is adjustable outside front: {name}"
            );
        }
    }
}

#[test]
fn antique_ambient_defaults_off_and_changes_only_panel_opacity_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !path.is_file() {
        return;
    }
    let load = |options: BTreeMap<String, String>| {
        load_skin_document(
            &path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
        )
        .unwrap()
        .document
    };
    let first_alpha = |document: &SkinDocument, id: &str| {
        document
            .all_destinations(&document.enabled_options())
            .iter()
            .find(|destination| destination.id == id)
            .and_then(|destination| destination.dst.first())
            .and_then(|frame| match frame {
                bmz_render::skin::SkinDstEntry::Frame(frame) => Some(frame.a.unwrap_or(255)),
                _ => None,
            })
            .unwrap()
    };
    let defaults = load(BTreeMap::new());
    assert!(
        defaults
            .property
            .iter()
            .any(|property| property.name == "Ambient" && property.def == "OFF")
    );
    assert!(defaults.all_destinations(&defaults.enabled_options()).iter().all(|d| !d.ambient));
    assert_eq!(first_alpha(&defaults, "img_frame_play1p"), 255);
    for (mode, transparency, expected) in [
        ("OFF", "100% (透明)", 255),
        ("ON", "0% (不透明)", 255),
        ("ON", "40%", 153),
        ("ON", "100% (透明)", 0),
    ] {
        let document = load(BTreeMap::from([
            ("Ambient".into(), mode.into()),
            ("Ambientパネル透明度".into(), transparency.into()),
        ]));
        let destinations = document.all_destinations(&document.enabled_options());
        assert_eq!(
            destinations.iter().filter(|d| d.ambient).count(),
            if mode == "ON" { 3 } else { 0 }
        );
        let regular_bga: Vec<_> = destinations
            .iter()
            .filter(|d| !d.ambient && matches!(d.id.as_str(), "bga" | "img_bga_bgi"))
            .collect();
        assert_eq!(regular_bga.len(), if mode == "ON" { 3 } else { 6 });
        if mode == "ON" {
            assert!(regular_bga.iter().all(|d| d.stretch == 1), "no dim cover copy");
        }
        for id in ["img_frame_play1p", "img_frame_graph1p"] {
            assert_eq!(first_alpha(&document, id), expected, "{mode} {transparency} {id}");
        }
        assert_eq!(
            first_alpha(&document, "img_frame_lane1p"),
            255,
            "opaque lane {mode} {transparency}"
        );
        for id in ["num_gauge", "img_judgeline", "sld_progress_song"] {
            assert_eq!(first_alpha(&document, id), first_alpha(&defaults, id), "foreground {id}");
        }
    }
}

#[test]
fn antique_ambient_modes_and_sizes_preserve_only_the_sharp_foreground_when_available() {
    use bmz_render::skin::SkinAmbientMode;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !path.is_file() {
        return;
    }
    for size in [
        "FULL(1080x1080)",
        "x4(1024x1024)",
        "x3(768x768)",
        "x2(512x512)",
        "x1(256x256)",
        "背景(1920x1080)",
    ] {
        for (mode, spread, blur) in
            [("全体", "20%", "50%"), ("Spread", "0%", "0%"), ("Spread", "200%", "100%")]
        {
            let document = load_skin_document(
                &path,
                SkinKind::Play,
                &BTreeMap::from([
                    ("Ambient".into(), "ON".into()),
                    ("Ambient表示方式".into(), mode.into()),
                    ("Spread範囲 (%)".into(), spread.into()),
                    ("Ambientぼかし度 (%)".into(), blur.into()),
                    ("BGAサイズ".into(), size.into()),
                ]),
                &BTreeMap::new(),
                &LuaLoadRuntimeState::default(),
                None,
            )
            .unwrap()
            .document;
            let destinations = document.all_destinations(&document.enabled_options());
            let ambient: Vec<_> = destinations.iter().filter(|d| d.ambient).collect();
            assert_eq!(ambient.len(), 3, "{size} {mode}");
            for d in ambient {
                assert_eq!(
                    d.ambient_mode,
                    if mode == "Spread" { SkinAmbientMode::Spread } else { SkinAmbientMode::Full }
                );
                assert_eq!(d.ambient_spread, spread.trim_end_matches('%').parse::<f32>().unwrap());
                assert_eq!(d.ambient_blur, blur.trim_end_matches('%').parse::<f32>().unwrap());
                assert_eq!(d.stretch, if mode == "Spread" { 1 } else { 3 });
            }
            let regular: Vec<_> = destinations
                .iter()
                .filter(|d| !d.ambient && matches!(d.id.as_str(), "bga" | "img_bga_bgi"))
                .collect();
            assert_eq!(regular.len(), if size.starts_with("背景") { 0 } else { 3 });
            assert!(regular.iter().all(|d| d.stretch == 1));
        }
    }
}

#[test]
#[ignore = "requires a GPU and the antique submodule assets; writes optional review images"]
fn antique_ambient_gpu_preview() {
    use bmz_render::plan::TextureId;
    use bmz_render::renderer::SurfaceSize;
    use bmz_render::snapshot::{DisplayBgaFrame, RenderSnapshot};
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    assert!(path.is_file(), "initialize the mz-select submodule");
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 1920, height: 1080 }).unwrap();
    // High-frequency edges make it possible to distinguish actual blur from alpha/scaling.
    let pixels: Vec<u8> = (0..256)
        .flat_map(|y| {
            (0..256).flat_map(move |x| {
                let grid = if (x / 16 + y / 16) % 2 == 0 { 0.55 } else { 1.0 };
                [
                    (x as f32 * grid) as u8,
                    (y as f32 * grid) as u8,
                    ((255 - x) as f32 * grid) as u8,
                    255,
                ]
            })
        })
        .collect();
    renderer.upsert_rgba_texture_ref(TextureId(700_000), 256, 256, &pixels).unwrap();
    let mut opaque_lane_pixel = None;
    for (name, mode, alpha, size, effect, spread, blur, aspect) in [
        ("off", "OFF", "40%", "FULL(1080x1080)", "全体", "20%", "50%", 1.0),
        ("on-40", "ON", "40%", "FULL(1080x1080)", "全体", "20%", "50%", 1.0),
        ("on-80", "ON", "80%", "FULL(1080x1080)", "全体", "20%", "50%", 1.0),
        ("on-background", "ON", "40%", "背景(1920x1080)", "全体", "20%", "50%", 1.0),
        ("on-fallback", "ON", "40%", "FULL(1080x1080)", "全体", "20%", "50%", 1.0),
        ("spread-square", "ON", "40%", "x2(512x512)", "Spread", "20%", "50%", 1.0),
        ("spread-wide", "ON", "40%", "x2(512x512)", "Spread", "20%", "50%", 16.0 / 9.0),
        ("spread-portrait", "ON", "40%", "x2(512x512)", "Spread", "20%", "50%", 9.0 / 16.0),
        ("spread-large", "ON", "80%", "x2(512x512)", "Spread", "200%", "100%", 16.0 / 9.0),
        ("spread-no-blur", "ON", "40%", "x2(512x512)", "Spread", "20%", "0%", 16.0 / 9.0),
        ("spread-low-blur", "ON", "40%", "x2(512x512)", "Spread", "20%", "10%", 16.0 / 9.0),
        ("spread-background", "ON", "40%", "背景(1920x1080)", "Spread", "20%", "50%", 16.0 / 9.0),
        ("spread-fallback", "ON", "40%", "x2(512x512)", "Spread", "20%", "50%", 1.0),
    ] {
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &path,
            kind: SkinKind::Play,
            options: &BTreeMap::from([
                ("Ambient".into(), mode.into()),
                ("Ambientパネル透明度".into(), alpha.into()),
                ("BGAサイズ".into(), size.into()),
                ("Ambient表示方式".into(), effect.into()),
                ("Spread範囲 (%)".into(), spread.into()),
                ("Ambientぼかし度 (%)".into(), blur.into()),
            ]),
            files: &BTreeMap::new(),
            runtime_state: &LuaLoadRuntimeState::default(),
            // Match the app's library boundary; customize/ is beside system/.
            library_roots: &[Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins")],
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        assert!(decoded.sources.iter().any(|source| source.source_id == "src_bga_bgi"));
        install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
        let snapshot = RenderSnapshot {
            time: TimeUs(10_000_000),
            play_elapsed_time: TimeUs(15_000_000),
            ready_elapsed_time: Some(TimeUs(12_000_000)),
            resources_loaded: true,
            key_mode: KeyMode::K7,
            has_bga: !name.ends_with("fallback"),
            bga_enabled: true,
            bga_base: (!name.ends_with("fallback"))
                .then(|| DisplayBgaFrame::opaque(700_000, 256.0 * aspect, 256.0)),
            title: "Ambient preview".into(),
            artist: "BMZ".into(),
            gauge: 80.0,
            gauge_max: 100.0,
            gauge_border: 80.0,
            total_notes: 1000,
            past_notes: 200,
            ex_score: 380,
            now_bpm: 150.0,
            min_bpm: 150.0,
            max_bpm: 150.0,
            hispeed: 2.5,
            ..Default::default()
        };
        renderer.prepare_scene(AppSceneSnapshot::Play(snapshot));
        let count = renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::Ambient { .. }))
            .count();
        assert_eq!(count, if mode == "ON" && blur != "0%" { 1 } else { 0 }, "{name}");
        renderer.render_last_plan().unwrap();
        let pixels = renderer.read_offscreen_rgba().unwrap();
        let lane_pixel = &pixels[(100 * 1920 + 100) * 4..(100 * 1920 + 100) * 4 + 4];
        if let Some(expected) = &opaque_lane_pixel {
            assert_eq!(lane_pixel, expected, "Ambient must not show through the lane: {name}");
        } else {
            opaque_lane_pixel = Some(lane_pixel.to_vec());
        }
        if let Some(directory) = std::env::var_os("BMZ_AMBIENT_PREVIEW_DIR") {
            let directory = PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            image::save_buffer(
                directory.join(format!("antique-{name}.png")),
                &pixels,
                1920,
                1080,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
