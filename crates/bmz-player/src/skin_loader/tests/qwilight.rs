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
fn qwilight_judges_and_fractional_hud_use_bmz_values_with_aligned_digits() {
    use std::io::{Cursor, Write};
    for align in 0..=2 {
        let root = unique_test_dir("qwilight-numeric-hud");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("skin.yaml");
        fs::write(
            &path,
            format!(
                r#"
format: {{zip: skin}}
frame: {{}}
lambda:
  drawing1: 1, 2, 3, 4, 5, 10
  pipeline: 7, 12, 14, 15, 39, 40
  alt-multiplier: 2
  alt-point: 2
point:
  noteLength: 20
  noteHeight: 12
  judgmentMainPosition: 500
  p2Position: 30
  binLength: 10
  binHeight: 20
  stopPointDrawingLength: 4
  multiplierSystem: {align}
  multiplierPosition0: 200
  multiplierPosition1: 20
  multiplierUnitDrawingLength: 8
  pointSystem: {align}
  pointPosition0: 200
  pointPosition1: 80
  pointUnitDrawingLength: 8
"#
            ),
        )
        .unwrap();
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut names = vec!["Note/N 1 0 0.png".to_string()];
        names.extend((0..10).map(|i| format!("Bin/{i}.png")));
        names.extend((0..6).map(|i| format!("Paint/J {i} 0.png")));
        // Specific HS glyphs and generic percentage glyphs exercise both paths.
        names.extend(["Bin/M ..png", "Bin/MW.png", "Bin/..png", "Bin/%.png"].map(str::to_string));
        let mut zip = zip::ZipWriter::new(fs::File::create(root.join("skin.zip")).unwrap());
        for name in names {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(png.get_ref()).unwrap();
        }
        zip.finish().unwrap();
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &path,
            kind: SkinKind::Play,
            options: &BTreeMap::new(),
            files: &BTreeMap::new(),
            runtime_state: &LuaLoadRuntimeState {
                number_values: BTreeMap::from([(bmz_render::skin::SKIN_REF_BMZ_KEY_MODE, 14)]),
                ..Default::default()
            },
            library_roots: &[root.to_path_buf()],
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        let document = &decoded.document;
        for (index, qw) in [(0, 0), (1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 0)] {
            let image =
                document.image.iter().find(|v| v.id == format!("qw-judge-{index}")).unwrap();
            let source = document.source.iter().find(|v| v.id == image.src).unwrap();
            assert!(source.path.ends_with(&format!("Paint/J {qw} 0.png")));
        }
        for (name, reference) in [("lowerJudgmentValue", 114), ("lowestJudgmentValue", 420)] {
            assert_eq!(
                document.value.iter().find(|v| v.id == format!("qw-number-{name}")).unwrap().ref_id,
                reference
            );
        }
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: source.size,
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for (hs, displayed_hs, score, past, hs_text, rate_text) in [
            (0.05, None, 0, 0, "0.05", "100.00"),
            (2.299_999_5, None, 7, 100, "2.30", "3.50"),
            (9.999, None, 1999, 1000, "10.00", "99.95"),
            (1.0, Some(12.34), 2000, 1000, "12.34", "100.00"),
            (20.0, None, 101, 300, "20.00", "16.83"),
        ] {
            let state = SkinDrawState {
                hispeed: hs,
                hispeed_display: displayed_hs,
                ex_score: score,
                past_notes: past,
                total_notes: 2000,
                ..Default::default()
            };
            for (name, expected, suffix) in
                [("multiplier", hs_text, false), ("point", rate_text, true)]
            {
                let id = format!("qw-number-{name}");
                let mut view = document.clone();
                view.destination.retain(|entry| matches!(entry, DestinationListEntry::Single(d) if d.id.starts_with(&id)));
                let items = view.static_render_items(&sources, &state, &SkinTextState::default());
                let digits_texture =
                    sources[&view.value.iter().find(|v| v.id == id).unwrap().src].texture;
                let digits: Vec<_> = items
                    .iter()
                    .filter_map(|item| match item {
                        SkinRenderItem::Image { texture, rect, uv, .. }
                            if *texture == digits_texture =>
                        {
                            Some((rect, (uv.x * 10.0).round() as u8))
                        }
                        _ => None,
                    })
                    .collect();
                let expected_digits: Vec<_> =
                    expected.bytes().filter(|c| *c != b'.').map(|c| c - b'0').collect();
                assert_eq!(
                    digits.iter().map(|(_, d)| *d).collect::<Vec<_>>(),
                    expected_digits.repeat(2),
                    "{name} {expected}"
                );
                let int_digits = expected.find('.').unwrap();
                let number_width = (expected_digits.len() * 10 + 4) as f32;
                let aligned_width = number_width + if suffix { 8.0 } else { 0.0 };
                let left = 200.0
                    - match align {
                        1 => aligned_width / 2.0,
                        2 => aligned_width,
                        _ => 0.0,
                    };
                for (i, (rect, _)) in digits.iter().enumerate() {
                    let column = i % expected_digits.len();
                    let side = i / expected_digits.len();
                    let x = left
                        + side as f32 * 190.0
                        + column as f32 * 10.0
                        + if column >= int_digits { 4.0 } else { 0.0 };
                    assert!(
                        (rect.x * 1280.0 - x).abs() < 0.01,
                        "{name} {expected} align={align} column={column}"
                    );
                }
                for (part, position) in [
                    ("dot", left + int_digits as f32 * 10.0),
                    ("unit", if suffix { left + number_width } else { left - 8.0 }),
                ] {
                    let texture = sources
                        [&view.image.iter().find(|v| v.id == format!("{id}-{part}")).unwrap().src]
                        .texture;
                    let positions: Vec<_> = items
                        .iter()
                        .filter_map(|item| match item {
                            SkinRenderItem::Image { texture: t, rect, .. } if *t == texture => {
                                Some(rect.x * 1280.0)
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(positions.len(), 2);
                    for (actual, expected) in positions.iter().zip([position, position + 190.0]) {
                        assert!((actual - expected).abs() < 0.01, "{name} {part}");
                    }
                }
            }
        }
    }
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
            hispeed: 2.3,
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
