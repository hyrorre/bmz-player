use super::*;
use bmz_core::judge::Judge;
use bmz_core::lane::Lane;
use bmz_render::snapshot::{
    DisplayJudgement, LongBodyState, NoteVisualKind, RenderSnapshot, VisibleLongNote, VisibleNote,
};

#[path = "qwilight_visuals.rs"]
mod visual_regressions;

#[test]
fn qwilight_unequal_frame_sizes_fill_each_atlas_cell() {
    use std::io::Write;
    let root = unique_test_dir("qwilight-unequal-frames");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.yaml");
    fs::write(&path, "format: {zip: skin}\nframe: {main-frame: 2}\nlambda:\n  drawing1: 1, 2, 3, 4, 5, 10\n  pipeline: 3, 7\npoint: {noteLength: 20}\n").unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(root.join("skin.zip")).unwrap());
    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    for (entry, width, height, color) in [
        ("Note/N 1 0 0.png", 2, 2, [255; 4]),
        ("Main/M 1 0.png", 4, 2, red),
        ("Main/M 1 1.png", 2, 4, blue),
    ] {
        zip.start_file(entry, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&png_bytes(width, height, color)).unwrap();
    }
    zip.finish().unwrap();
    let decoded = decode_qwilight(&path, &root, 7, BTreeMap::new());
    let beam = decoded.document.image.iter().find(|image| image.id == "qw-lane-3-0").unwrap();
    let source = decoded.sources.iter().find(|source| source.source_id == beam.src).unwrap();
    let asset = source.asset.as_ref().unwrap();
    assert_eq!((beam.divx, beam.divy), (2, 2));
    assert_eq!((asset.width, asset.height), (8, 8));
    for y in 0..8 {
        for x in 0..8 {
            let i = (y * 8 + x) * 4;
            let expected = if x < 4 && y < 4 { red } else { blue };
            assert_eq!(&asset.pixels[i..i + 4], &expected, "pixel ({x}, {y})");
        }
    }
}

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
        assert_eq!((asset.width, asset.height), (4, 2));
        assert_eq!(&asset.pixels[..4], &[255, 0, 0, 255]);
        assert_eq!(&asset.pixels[8..12], &[0, 0, 255, 255]);
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

fn png_bytes(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(width, height, image::Rgba(color))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    png.into_inner()
}

fn decode_qwilight(
    path: &Path,
    root: &Path,
    mode: i32,
    options: BTreeMap<String, String>,
) -> DecodedSkin {
    decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: path,
        kind: SkinKind::Play,
        options: &options,
        files: &BTreeMap::new(),
        runtime_state: &LuaLoadRuntimeState {
            number_values: BTreeMap::from([(bmz_render::skin::SKIN_REF_BMZ_KEY_MODE, mode)]),
            ..Default::default()
        },
        library_roots: &[root.to_path_buf()],
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
    })
    .unwrap()
}

#[test]
fn qwilight_lane_cover_follows_sudden_with_skin_or_added_images() {
    use std::io::Write;
    let root = unique_test_dir("qwilight-lane-cover");
    let skin = root.join("Skin");
    fs::create_dir_all(&skin).unwrap();
    let path = skin.join("skin.yaml");
    // No VeilDrawing (50) in the pipeline: Qwilight draws the veil right after the notes.
    fs::write(&path, "format: {zip: skin}\nframe: {}\nlambda:\n  drawing1: 1, 2, 3, 4, 5, 10\n  pipeline: 7, 15, 9\npoint:\n  mainPosition: 100\n  noteLength: 20\n  noteHeight: 12\n  judgmentMainPosition: 500\n  p2Position: 30\n  binLength: 10\n  binHeight: 20\n  bandPosition0: 300\n  bandPosition1: 40\n").unwrap();
    // Without `_GetVeil`, Qwilight looks for `Drawing/Veil`; skins name their own file.
    fs::write(skin.join("skin.lua"), "function _GetVeil() return 'Veil 0' end").unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(skin.join("skin.zip")).unwrap());
    let mut entries = vec![("Note/N 1 0 0.png".to_string(), png_bytes(2, 2, [255; 4]))];
    entries.push(("Drawing/Veil 0.png".into(), png_bytes(4, 8, [0, 255, 0, 255])));
    entries.extend((0..10).map(|i| (format!("Bin/{i}.png"), png_bytes(2, 2, [255; 4]))));
    entries.extend((0..6).map(|i| (format!("Paint/J {i} 0.png"), png_bytes(2, 2, [255; 4]))));
    for (name, bytes) in entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    let added = root.join(bmz_skin::QWILIGHT_LANE_COVER_DIR);
    fs::create_dir_all(&added).unwrap();
    fs::write(added.join("mine.png"), png_bytes(8, 8, [255, 0, 0, 255])).unwrap();

    let cover = |options: BTreeMap<String, String>, mode| {
        let decoded = decode_qwilight(&path, &root, mode, options);
        let document = decoded.document.clone();
        let slider = document.slider.iter().find(|s| s.id == "qw-lane-cover").unwrap().clone();
        let source = document
            .source
            .iter()
            .find(|s| s.id == slider.src)
            .unwrap_or_else(|| panic!("{} not in {:?}", slider.src, document.source))
            .path
            .clone();
        (decoded, slider, source)
    };
    let property = bmz_skin::QWILIGHT_LANE_COVER_PROPERTY.to_string();
    for (selected, expected, aspect) in [
        (None, "Drawing/Veil 0.png", 2.0),
        (Some("mine.png"), "mine.png", 1.0),
        // A removed image falls back to the skin's own lane cover.
        (Some("missing.png"), "Drawing/Veil 0.png", 2.0),
    ] {
        let options: BTreeMap<_, _> =
            selected.map(|name| (property.clone(), name.to_string())).into_iter().collect();
        let (decoded, slider, source) = cover(options, 7);
        assert!(source.replace('\\', "/").ends_with(expected), "{selected:?}: {source}");
        assert_eq!((slider.slider_type, slider.angle, slider.range), (4, 2, 500));
        let document = &decoded.document;
        let ids: Vec<_> = document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                DestinationListEntry::Single(d) => Some(d.id.as_str()),
                _ => None,
            })
            .collect();
        let notes = ids.iter().position(|id| *id == "notes").unwrap();
        assert_eq!(ids[notes + 1], "qw-lane-cover", "lane cover follows the notes");
        let DestinationListEntry::Single(destination) = document
            .destination
            .iter()
            .find(
                |entry| matches!(entry, DestinationListEntry::Single(d) if d.id == "qw-lane-cover"),
            )
            .unwrap()
        else {
            unreachable!()
        };
        // 1P width is 8 lanes x 20 px; the image keeps its aspect and starts above the canvas.
        let bmz_render::skin::SkinDstEntry::Frame(frame) = &destination.dst[0] else {
            panic!("lane cover frame")
        };
        let height = (160.0 * aspect) as i32;
        assert_eq!(
            (frame.x, frame.y, frame.w, frame.h),
            (Some(100), Some(720), Some(160), Some(height))
        );
        let sources: std::collections::HashMap<_, _> = decoded
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
            .collect();
        let texture = sources[&slider.src].texture;
        let mut view = document.clone();
        view.destination.retain(
            |entry| matches!(entry, DestinationListEntry::Single(d) if d.id == "qw-lane-cover"),
        );
        let drawn = |lane_cover| {
            let state = SkinDrawState { lane_cover, ..Default::default() };
            view.static_render_items(&sources, &state, &SkinTextState::default())
                .into_iter()
                .filter_map(|item| match item {
                    SkinRenderItem::Image { texture: t, rect, .. } if t == texture => Some(rect),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(drawn(0.0).is_empty(), "SUDDEN+ off hides the lane cover");
        let rect = drawn(0.5)[0];
        // The bottom edge sits halfway down the 500 px lane (top-down canvas coordinates).
        assert!(((rect.y + rect.height) * 720.0 - 250.0).abs() < 0.5, "{rect:?}");
        assert!((rect.width * 1280.0 - 160.0).abs() < 0.5);
    }
    let (decoded, _, _) = cover(BTreeMap::from([(property.clone(), "mine.png".into())]), 14);
    let sides = decoded
        .document
        .destination
        .iter()
        .filter(|entry| matches!(entry, DestinationListEntry::Single(d) if d.id == "qw-lane-cover"))
        .count();
    assert_eq!(sides, 2, "DP draws the lane cover on both sides");

    // beatoraja's Notes / Judge offsets are attached to the converted elements.
    let decoded = decode_qwilight(&path, &root, 7, BTreeMap::new());
    let document = &decoded.document;
    let offset_of = |id: &str| {
        document.destination.iter().find_map(|entry| match entry {
            DestinationListEntry::Single(d) if d.id == id => Some(
                d.offsets
                    .iter()
                    .copied()
                    .chain((d.offset != 0).then_some(d.offset))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
    };
    assert_eq!(offset_of("notes"), Some(vec![30]));
    assert_eq!(offset_of("qw-number-band"), Some(vec![3, 32]));
    assert!(document.judge[0].images.iter().all(|image| image.offsets == [3, 32]));
    assert_eq!(
        document.property.last().unwrap().item.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["Skin", "Random", "mine.png"]
    );
}

/// Offscreen GPU preview of an installed Qwilight skin with BMZ play values.
#[allow(clippy::too_many_arguments)]
fn render_installed_qwilight(
    root: &Path,
    library_roots: &[PathBuf],
    name: &str,
    mode: KeyMode,
    number: i32,
    options: &BTreeMap<String, String>,
    offsets: &BTreeMap<String, bmz_skin::LuaSkinOffsetValue>,
    lane_cover: f32,
) -> Vec<u8> {
    let (mut renderer, snapshot) = prepare_installed_qwilight(
        root,
        library_roots,
        name,
        mode,
        number,
        options,
        offsets,
        lane_cover,
    );
    render_qwilight_snapshot(&mut renderer, snapshot)
}

#[allow(clippy::too_many_arguments)]
fn prepare_installed_qwilight(
    root: &Path,
    library_roots: &[PathBuf],
    name: &str,
    mode: KeyMode,
    number: i32,
    options: &BTreeMap<String, String>,
    offsets: &BTreeMap<String, bmz_skin::LuaSkinOffsetValue>,
    lane_cover: f32,
) -> (Renderer, RenderSnapshot) {
    let path = root.join(name).join(format!("{name}.yaml"));
    let state = LuaLoadRuntimeState {
        number_values: BTreeMap::from([(bmz_render::skin::SKIN_REF_BMZ_KEY_MODE, number)]),
        offset_values: offsets.clone(),
        ..Default::default()
    };
    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: &path,
        kind: SkinKind::Play,
        options,
        files: &BTreeMap::new(),
        runtime_state: &state,
        library_roots,
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
        lane_cover,
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
    (renderer, snapshot)
}

fn render_qwilight_snapshot(renderer: &mut Renderer, snapshot: RenderSnapshot) -> Vec<u8> {
    renderer.render_scene(AppSceneSnapshot::Play(snapshot)).unwrap();
    let rgba = renderer.read_offscreen_rgba().unwrap();
    assert!(
        rgba.as_chunks::<4>().0.iter().filter(|p| p[0] > 30 || p[1] > 30 || p[2] > 30).count()
            > 10000,
        "empty preview"
    );
    rgba
}

fn save_preview(output: &Path, file: &str, rgba: &[u8]) {
    image::save_buffer(output.join(file), rgba, 1280, 720, image::ColorType::Rgba8).unwrap();
}

/// Pixels that differ, within `x0..x1` and `y0..y1` of a 1280x720 frame.
fn changed_pixels(a: &[u8], b: &[u8], (x0, x1): (usize, usize), (y0, y1): (usize, usize)) -> usize {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (y * 1280 + x) * 4))
        .filter(|&i| a[i..i + 3].iter().zip(&b[i..i + 3]).any(|(p, q)| p.abs_diff(*q) > 24))
        .count()
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
    let roots = [root.clone()];
    let (none, unadjusted) = (BTreeMap::new(), BTreeMap::new());
    for (name, mode, number) in
        [("Default", KeyMode::K7, 7), ("CR_DX", KeyMode::K7, 7), ("CR_simple", KeyMode::K14, 14)]
    {
        let rgba =
            render_installed_qwilight(&root, &roots, name, mode, number, &none, &unadjusted, 0.0);
        save_preview(&output, &format!("{name}-{number}k.png"), &rgba);
    }
}

/// Non-default Qwilight adjustments, SUDDEN+ with the skin's and an added
/// lane cover, rendered offscreen. The added image is copied to a temporary
/// library root; the installed Qwilight folder is not modified.
#[test]
#[ignore = "requires installed Qwilight skins and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_installed_customization_gpu() {
    let root = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set BMZ_QWILIGHT_SKIN_ROOT"),
    );
    let output = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("set BMZ_QWILIGHT_PREVIEW_DIR"),
    );
    fs::create_dir_all(&output).unwrap();
    let added = tempfile::tempdir().unwrap();
    let cover_dir = added.path().join(bmz_skin::QWILIGHT_LANE_COVER_DIR);
    fs::create_dir_all(&cover_dir).unwrap();
    // CR_DX's loose Cover_DX.png equals its built-in veil, so use another skin's cover.
    fs::copy(root.join("CR_natural/Cover_Red.png"), cover_dir.join("Cover_Red.png")).unwrap();
    let roots = [root.clone(), added.path().to_path_buf()];
    let (none, unadjusted) = (BTreeMap::new(), BTreeMap::new());
    let value = |x, y, w, h| bmz_skin::LuaSkinOffsetValue { x, y, w, h, r: 0, a: 0 };
    let adjusted = BTreeMap::from([
        ("Qwilight lane position".to_string(), value(120, 0, 0, 0)),
        ("Qwilight note width".to_string(), value(0, 0, 8, 0)),
        ("Qwilight combo position".to_string(), value(0, 60, 0, 0)),
    ]);
    let user_cover = BTreeMap::from([(
        bmz_skin::QWILIGHT_LANE_COVER_PROPERTY.to_string(),
        "Cover_Red.png".to_string(),
    )]);
    for (name, mode, number) in [("CR_DX", KeyMode::K7, 7), ("Default", KeyMode::K14, 14)] {
        let render = |options, offsets, cover| {
            render_installed_qwilight(&root, &roots, name, mode, number, options, offsets, cover)
        };
        let base = render(&none, &unadjusted, 0.0);
        let moved = render(&none, &adjusted, 0.0);
        let covered = render(&none, &unadjusted, 0.4);
        let user = render(&user_cover, &unadjusted, 0.4);
        for (suffix, rgba) in
            [("base", &base), ("adjusted", &moved), ("sudden", &covered), ("added-cover", &user)]
        {
            save_preview(&output, &format!("{name}-{number}k-{suffix}.png"), rgba);
        }
        let moved_pixels = changed_pixels(&base, &moved, (0, 1280), (0, 720));
        // SUDDEN+ 0.4 covers the upper part of the lanes only.
        let upper = changed_pixels(&base, &covered, (0, 1280), (0, 150));
        let lower = changed_pixels(&base, &covered, (0, 1280), (500, 720));
        let skin_vs_added = changed_pixels(&covered, &user, (0, 1280), (0, 150));
        eprintln!(
            "{name} {number}K: adjusted {moved_pixels}, cover upper {upper} / lower {lower}, \
             added vs skin cover {skin_vs_added}"
        );
        assert!(moved_pixels > 10_000, "{name}: adjustments must move the play area");
        assert!(upper > 5_000, "{name}: SUDDEN+ must draw the lane cover");
        assert!(lower < upper / 10, "{name}: the lane cover must stay above its edge");
        assert!(skin_vs_added > 5_000, "{name}: the added image must replace the skin's");
    }
}

#[test]
#[ignore = "requires installed Qwilight skins and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_cr_minimal_hidden_turntable_gpu() {
    let root = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set BMZ_QWILIGHT_SKIN_ROOT"),
    );
    let output = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("set BMZ_QWILIGHT_PREVIEW_DIR"),
    );
    fs::create_dir_all(&output).unwrap();
    let roots = [root.clone()];
    let options = BTreeMap::from([("ターンテーブルの隠し設定".into(), "Hide Turntable".into())]);
    let shown = render_installed_qwilight(
        &root,
        &roots,
        "CR_minimal",
        KeyMode::K7,
        7,
        &BTreeMap::new(),
        &BTreeMap::new(),
        0.0,
    );
    let hidden = render_installed_qwilight(
        &root,
        &roots,
        "CR_minimal",
        KeyMode::K7,
        7,
        &options,
        &BTreeMap::new(),
        0.0,
    );
    assert!(changed_pixels(&shown, &hidden, (0, 1280), (0, 720)) > 1000);
    save_preview(&output, "CR_minimal-hide-turntable.png", &hidden);
}
