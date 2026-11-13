//! Local-asset GPU regression: the original line crop remains visible behind notes.
use super::*;
use bmz_core::lane::Lane;
use bmz_render::snapshot::{NoteVisualKind, RenderSnapshot, VisibleNote};

#[test]
#[ignore = "requires installed ECFN / CR_minimal assets and an offscreen GPU"]
fn installed_play_judge_lines_render_behind_notes() {
    let qw_root =
        PathBuf::from(std::env::var("BMZ_QWILIGHT_UI_DIR").expect("Qwilight UI directory"));
    let output =
        PathBuf::from(std::env::var("BMZ_PLAY_LAYER_PREVIEW_DIR").expect("preview directory"));
    fs::create_dir_all(&output).unwrap();
    let mut library_roots = test_app_paths().skin_library_roots();
    library_roots.push(qw_root.clone());
    for (name, path, mode, number, line_id) in [
        (
            "ecfn",
            test_app_paths().resource_dir.join("skins/ECFN/play/play7.luaskin"),
            KeyMode::K7,
            7,
            "15",
        ),
        ("cr-minimal", qw_root.join("CR_minimal/CR_minimal.yaml"), KeyMode::K6, 6, "qw-paint-10"),
    ] {
        assert!(path.is_file(), "missing {}", path.display());
        let state = LuaLoadRuntimeState {
            number_values: BTreeMap::from([(bmz_render::skin::SKIN_REF_BMZ_KEY_MODE, number)]),
            ..Default::default()
        };
        let mut decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &path,
            kind: SkinKind::Play,
            options: &BTreeMap::new(),
            files: &BTreeMap::new(),
            runtime_state: &state,
            library_roots: &library_roots,
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        // Isolate the original line and note destinations in memory, retaining their order,
        // coordinates, crop and real source pixels. No third-party files are modified.
        decoded.document.destination.retain(|entry| {
            matches!(entry, DestinationListEntry::Single(d) if d.id == line_id || d.id == "notes")
        });
        let line_image = decoded.document.image.iter().find(|i| i.id == line_id).unwrap();
        let line_texture =
            decoded.sources.iter().find(|s| s.source_id == line_image.src).unwrap().texture;
        let mut renderer = Renderer::default();
        renderer.set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
        const WIDTH: u32 = 1920;
        const HEIGHT: u32 = 1080;
        renderer
            .attach_offscreen(bmz_render::renderer::SurfaceSize { width: WIDTH, height: HEIGHT })
            .unwrap();
        install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
        let now = TimeUs(10_000_000);
        let mut snapshot = RenderSnapshot {
            time: now,
            play_elapsed_time: now,
            ready_elapsed_time: Some(now),
            resources_loaded: true,
            key_mode: mode,
            now_bpm: 150.0,
            ..Default::default()
        };
        renderer.render_scene(AppSceneSnapshot::Play(snapshot.clone())).unwrap();
        let line_only = renderer.read_offscreen_rgba().unwrap();
        let (line_index, line_rect) = renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .enumerate()
            .find_map(|(i, c)| match c {
                DrawCommand::Image { texture, rect, .. } if texture.0 == line_texture.0 => {
                    Some((i, *rect))
                }
                _ => None,
            })
            .expect("original judge line draw command");
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: now,
            y: 0.0,
            alpha: 1.0,
            kind: NoteVisualKind::Tap,
            processed_judge: None,
            pms_miss_fall: None,
        });
        renderer.render_scene(AppSceneSnapshot::Play(snapshot)).unwrap();
        let with_note = renderer.read_offscreen_rgba().unwrap();
        let commands = &renderer.last_plan().unwrap().commands;
        assert!(commands.len() > line_index + 1, "{name}: notes must follow the line");
        let mut visible = 0;
        let mut covered = 0;
        let mut unchanged = 0;
        for y in (line_rect.y * HEIGHT as f32).ceil() as u32
            ..((line_rect.y + line_rect.height) * HEIGHT as f32).floor() as u32
        {
            for x in (line_rect.x * WIDTH as f32).ceil() as u32
                ..((line_rect.x + line_rect.width) * WIDTH as f32).floor() as u32
            {
                let offset = ((y * WIDTH + x) * 4) as usize;
                let before = &line_only[offset..offset + 3];
                let after = &with_note[offset..offset + 3];
                if before.iter().any(|v| *v > 32) {
                    visible += 1;
                    if before == after {
                        unchanged += 1;
                    } else {
                        covered += 1;
                    }
                }
            }
        }
        assert!(visible > 100, "{name}: original line crop must remain visible");
        assert!(covered > 100, "{name}: note must visibly overlap the line: {covered}");
        assert!(unchanged > covered, "{name}: uncovered line must remain visible");
        for (suffix, pixels) in [("line", line_only), ("overlap", with_note)] {
            image::save_buffer(
                output.join(format!("{name}-{suffix}.png")),
                &pixels,
                WIDTH,
                HEIGHT,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        eprintln!(
            "{name}: visible line pixels={visible}, covered by note={covered}, uncovered={unchanged}"
        );
    }
}
