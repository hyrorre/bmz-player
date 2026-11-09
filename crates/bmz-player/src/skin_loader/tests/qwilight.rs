use super::*;
use bmz_core::judge::Judge;
use bmz_core::lane::Lane;
use bmz_render::snapshot::{
    DisplayJudgement, LongBodyState, NoteVisualKind, RenderSnapshot, VisibleLongNote, VisibleNote,
};

#[test]
fn qwilight_zip_frames_decode_without_song_registry_and_options_change_layout() {
    use std::io::{Cursor, Write};
    let root = unique_test_dir("qwilight-synthetic");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.yaml");
    fs::write(&path,"format: {zip: skin, lua: skin}\nframe: {note-frame: 2, note-framerate: 10}\nlambda:\n  configure-comment-0: Width\n  configure-0: Narrow, Wide\n  drawing1: 1, 2, 3, 4, 5, 10\n  pipeline: 3, 7, 15, 9\npoint:\n  noteLength: Width()\n  noteHeight: 12\n  judgmentMainPosition: 500\n  longNoteFrontEdgeHeight: 24\n  longNoteFrontEdgePosition: -12\n").unwrap();
    fs::write(root.join("skin.lua"), "function Width() return 20 + 10 * configures[1] end")
        .unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(root.join("skin.zip")).unwrap());
    for (frame, color) in [(0, [255, 0, 0, 255]), (1, [0, 0, 255, 255])] {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba(color))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        zip.start_file(format!("Note/N 1 {frame} 0.png"), zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(png.get_ref()).unwrap();
    }
    zip.finish().unwrap();
    let mut state = LuaLoadRuntimeState::default();
    state.number_values.insert(bmz_render::skin::SKIN_REF_BMZ_KEY_MODE, 7);
    for (option, width) in [("Narrow", 20), ("Wide", 30)] {
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &path,
            kind: SkinKind::Play,
            options: &BTreeMap::from([("Width".into(), option.into())]),
            files: &BTreeMap::new(),
            runtime_state: &state,
            library_roots: &[root.to_path_buf()],
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        let note_image =
            decoded.document.image.iter().find(|image| image.id == "qw-note-0").unwrap();
        let source =
            decoded.sources.iter().find(|source| source.source_id == note_image.src).unwrap();
        for id in ["qw-lnstart-0", "qw-lnend-0", "qw-lnbody-0", "qw-lnbodyActive-0"] {
            let part = decoded.document.image.iter().find(|image| image.id == id).unwrap();
            assert_eq!(
                part.src, note_image.src,
                "missing LN parts must not alias another tap frame"
            );
        }
        let asset = source.asset.as_ref().unwrap();
        assert_eq!((asset.width, asset.height), (2, 4));
        assert_eq!(&asset.pixels[..4], &[255, 0, 0, 255]);
        assert_eq!(&asset.pixels[16..20], &[0, 0, 255, 255]);
        let note = decoded.document.note.as_ref().unwrap();
        let bmz_render::skin::SkinDstEntry::Frame(key) = &note.dst[0] else { panic!("key") };
        let bmz_render::skin::SkinDstEntry::Frame(scratch) = &note.dst[7] else {
            panic!("scratch")
        };
        assert_eq!(key.x, Some(width));
        assert_eq!(scratch.x, Some(0));
        assert_eq!(key.w, Some(width));
        assert_eq!(note.qwilight_geometry[0].front_height, 24.0);
    }
    assert!(decode_beatoraja_skin(&path, SkinKind::Select).is_err());
    assert!(!root.join("Note").exists());
}

#[test]
#[ignore = "requires installed Qwilight skins and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_installed_play_skins_gpu() {
    let root = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set BMZ_QWILIGHT_SKIN_ROOT"),
    );
    let output = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("set BMZ_QWILIGHT_PREVIEW_DIR"),
    );
    fs::create_dir_all(&output).unwrap();
    for (name, mode, number) in
        [("Default", KeyMode::K7, 7), ("CR_DX", KeyMode::K7, 7), ("CR_simple", KeyMode::K14, 14)]
    {
        let path = root.join(name).join(format!("{name}.yaml"));
        let state = LuaLoadRuntimeState {
            number_values: BTreeMap::from([(bmz_render::skin::SKIN_REF_BMZ_KEY_MODE, number)]),
            ..Default::default()
        };
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &path,
            kind: SkinKind::Play,
            options: &BTreeMap::new(),
            files: &BTreeMap::new(),
            runtime_state: &state,
            library_roots: std::slice::from_ref(&root),
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        let missing: Vec<_> = decoded
            .document
            .source
            .iter()
            .filter(|s| !decoded.sources.iter().any(|loaded| loaded.source_id == s.id))
            .map(|s| s.path.as_str())
            .collect();
        assert!(missing.is_empty(), "{name} missing sources: {missing:?}");
        eprintln!("{name}: decoded {} source atlases", decoded.sources.len());
        let mut renderer = Renderer::default();
        renderer.set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
        renderer
            .attach_offscreen(bmz_render::renderer::SurfaceSize { width: 1280, height: 720 })
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
            ex_score: 1234,
            combo: 123,
            max_combo: 345,
            total_notes: 1000,
            past_notes: 700,
            gauge: 70.0,
            gauge_max: 100.0,
            title: "Qwilight play skin / BMZ preview".into(),
            artist: "Compatibility test".into(),
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key4,
                judge: Judge::Great,
                side: None,
                text: "GREAT".into(),
                combo: 123,
                delta_us: 0,
                time: TimeUs(now.0 - 100_000),
                is_miss: false,
                timing_ms_suppressed: false,
            }],
            visible_long_notes: vec![VisibleLongNote {
                lane: Lane::Key2,
                mode: bmz_chart::model::LongNoteMode::Ln,
                head_y: 0.2,
                tail_y: 0.75,
                alpha: 1.0,
                body_state: LongBodyState::Inactive,
            }],
            ..Default::default()
        };
        snapshot.keyon_ms[Lane::Key4.index()] = Some(100);
        for (i, &lane) in mode.active_lanes().iter().enumerate() {
            snapshot.visible_notes[lane.index()].push(VisibleNote {
                lane,
                time: now,
                y: 0.15 + (i % 6) as f32 * 0.11,
                alpha: 1.0,
                kind: NoteVisualKind::Tap,
                processed_judge: None,
                pms_miss_fall: None,
            });
        }
        renderer.render_scene(AppSceneSnapshot::Play(snapshot)).unwrap();
        let rgba = renderer.read_offscreen_rgba().unwrap();
        assert!(
            rgba.as_chunks::<4>().0.iter().filter(|p| p[0] > 30 || p[1] > 30 || p[2] > 30).count()
                > 10000,
            "empty preview"
        );
        image::save_buffer(
            output.join(format!("{name}-{number}k.png")),
            &rgba,
            1280,
            720,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
