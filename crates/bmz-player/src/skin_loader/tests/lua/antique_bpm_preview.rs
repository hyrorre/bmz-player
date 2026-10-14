use super::*;
use bmz_render::skin::{SkinAnimationDef, SkinDestinationDef, SkinDocument, SkinDstEntry};

/// FAST/SLOW digit cell width (drawn at 1x) and maximum shown digits.
const DIGIT_W: i32 = 13;
const DIGITS: i32 = 3;

fn antique_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin")
}

fn load_antique(options: &[(&str, &str)]) -> SkinDocument {
    load_skin_document(
        &antique_path(),
        SkinKind::Play,
        &options.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
    )
    .unwrap()
    .document
}

fn is_preview_part(id: &str) -> bool {
    id.starts_with("num_bpm_preview_") || id.starts_with("txt_bpm_preview_arrow_")
}

fn preview_destinations(document: &SkinDocument) -> Vec<SkinDestinationDef> {
    document
        .all_destinations(&document.enabled_options())
        .into_iter()
        .filter(|destination| is_preview_part(&destination.id))
        .cloned()
        .collect()
}

fn frame(destination: &SkinDestinationDef) -> &SkinAnimationDef {
    let SkinDstEntry::Frame(frame) = &destination.dst[0] else { panic!("preview frame") };
    frame
}

/// `(left, right)` of one part. Numbers span three digit cells; the arrow text is centered.
fn part_span(destination: &SkinDestinationDef) -> (i32, i32) {
    let frame = frame(destination);
    let (x, w) = (frame.x.unwrap(), frame.w.unwrap());
    if destination.id.starts_with("txt_bpm_preview_arrow") {
        (x - w / 2, x + w / 2)
    } else {
        (x, x + w * DIGITS)
    }
}

/// Returns `(left, right, bottom, top)` of the preview group in skin pixels.
/// The vertical range comes from the digits: the arrow text box is taller than its glyph.
fn preview_bounds(destinations: &[SkinDestinationDef]) -> (i32, i32, i32, i32) {
    destinations.iter().fold((i32::MAX, i32::MIN, i32::MAX, i32::MIN), |bounds, destination| {
        let (left, right) = part_span(destination);
        let (bottom, top) = if destination.id.starts_with("txt_bpm_preview_arrow") {
            (bounds.2, bounds.3)
        } else {
            let frame = frame(destination);
            let (y, h) = (frame.y.unwrap(), frame.h.unwrap());
            (bounds.2.min(y), bounds.3.max(y + h))
        };
        (bounds.0.min(left), bounds.1.max(right), bottom, top)
    })
}

#[test]
fn antique_bpm_preview_defaults_off_without_declaration_when_available() {
    if !antique_path().is_file() {
        return;
    }
    let document = load_antique(&[]);
    assert_eq!(document.bmz_bpm_change_preview, None);
    assert!(preview_destinations(&document).is_empty());
    assert!(!document.value.iter().any(|value| value.id.starts_with("num_bpm_preview_")));
}

#[test]
fn antique_bpm_preview_types_use_fast_slow_digits_and_place_parts_when_available() {
    if !antique_path().is_file() {
        return;
    }
    // Lane geometry for the default layout: lane x=63..576, judge line y=353,
    // judge image y=476..560, FAST/SLOW y=575..595, play frame x=6..593.
    let lane_center = (63 + 576) / 2;
    for (lead_label, lead_ms) in [("500ms", 500), ("2000ms", 2_000), ("4000ms", 4_000)] {
        for (type_label, inside) in [
            ("TYPE A (判定文字の上)", true),
            ("TYPE B (判定文字の下)", true),
            ("TYPE C (レーン外・判定文字の横)", false),
            ("TYPE D (レーン外・判定線の横)", false),
        ] {
            let document = load_antique(&[
                ("BPM変化予告 (BMZ)", type_label),
                ("BPM変化予告タイミング", lead_label),
            ]);
            assert_eq!(
                document.bmz_bpm_change_preview.map(|preview| preview.lead_ms),
                Some(lead_ms),
                "{type_label} {lead_label}"
            );
            // Same atlas rows as the FAST (y=26) / SLOW (y=39) judge counts.
            for (key, src_y) in [("inc", 26), ("dec", 39)] {
                for (part, ref_id, align) in
                    [("from", 19062, if inside { 0 } else { 1 }), ("to", 19061, 1)]
                {
                    let id = format!("num_bpm_preview_{part}_{key}");
                    let value = document.value.iter().find(|value| value.id == id).unwrap();
                    assert_eq!(value.src, "src_number_common_s", "{id}");
                    assert_eq!((value.y, value.divx, value.digit), (src_y, 10, DIGITS), "{id}");
                    assert_eq!((value.ref_id, value.align), (ref_id, align), "{id}");
                }
            }
            // The arrow outline uses the digit color to thicken the glyph.
            for (key, outline) in [("inc", "ccccffff"), ("dec", "ffccccff")] {
                let id = format!("txt_bpm_preview_arrow_{key}");
                let arrow = document.text.iter().find(|text| text.id == id).unwrap();
                assert_eq!(arrow.constant_text, "»");
                assert_eq!(arrow.outline_color, outline);
            }

            let destinations = preview_destinations(&document);
            // Inside: three parts per color. Outside: lane-side number plus
            // arrow / far number for each digit count of the lane-side number.
            assert_eq!(destinations.len(), if inside { 6 } else { 14 }, "{type_label}");
            for destination in &destinations {
                assert_eq!(destination.timer, Some(19068));
                assert!(destination.op.contains(&19064));
                let increase = destination.op.contains(&19065);
                assert!(increase || destination.op.contains(&19066));
                let key = if increase { "_inc" } else { "_dec" };
                assert!(destination.id.ends_with(key), "{}", destination.id);
                if destination.id.starts_with("txt_bpm_preview_arrow") {
                    let color = (frame(destination).r, frame(destination).g, frame(destination).b);
                    let expected = if increase { (204, 204, 255) } else { (255, 204, 204) };
                    assert_eq!(color, (Some(expected.0), Some(expected.1), Some(expected.2)));
                }
            }

            let (left, right, bottom, top) = preview_bounds(&destinations);
            // Like FAST/SLOW and the score difference, the preview has no backing panel.
            assert!(document.panel.iter().all(|panel| panel.id != "pnl_bpm_preview"));
            if inside {
                assert!(
                    ((left + right) / 2 - lane_center).abs() <= 1 && left >= 63 && right <= 576,
                    "{type_label}: {left}..{right}"
                );
            } else {
                // The lane-side number starts right next to the play frame.
                let from = destinations
                    .iter()
                    .filter(|destination| destination.id.starts_with("num_bpm_preview_from"))
                    .map(|destination| part_span(destination).0)
                    .collect::<Vec<_>>();
                assert_eq!(from, vec![601, 601], "{type_label}");
                for destination in &destinations {
                    let draw = destination.draw.as_str();
                    if destination.id.starts_with("num_bpm_preview_from") {
                        assert!(draw.is_empty());
                    } else {
                        assert!(draw.contains("number(19062)"), "{}: {draw}", destination.id);
                    }
                }
            }
            match type_label.split_whitespace().nth(1).unwrap() {
                "A" => assert!(bottom >= 595, "above FAST/SLOW: {bottom}"),
                "B" => assert!(bottom > 365 && top <= 476, "below judge: {bottom}..{top}"),
                "C" => assert!(bottom >= 476 && top <= 560, "beside judge: {bottom}..{top}"),
                "D" => assert!(bottom < 359 && top > 359, "beside judge line: {bottom}..{top}"),
                other => panic!("{other}"),
            }
        }
    }
}

#[test]
fn antique_bpm_preview_outside_lane_moves_left_for_right_lane_layouts_when_available() {
    if !antique_path().is_file() {
        return;
    }
    for (layout, play_x) in
        [("BGA/グラフ/レーン", 1330), ("グラフ/BGA/レーン", 1330), ("BGA/レーン/グラフ", 1078)]
    {
        for type_label in ["TYPE C (レーン外・判定文字の横)", "TYPE D (レーン外・判定線の横)"]
        {
            let document = load_antique(&[("配置", layout), ("BPM変化予告 (BMZ)", type_label)]);
            let destinations = preview_destinations(&document);
            let (_, right, _, _) = preview_bounds(&destinations);
            assert!(right <= play_x, "{layout} {type_label}: {right}");
            // The lane-side number is right-aligned and ends right next to the play frame.
            for key in ["inc", "dec"] {
                let id = format!("num_bpm_preview_to_{key}");
                assert_eq!(document.value.iter().find(|value| value.id == id).unwrap().align, 0);
            }
            let to = destinations
                .iter()
                .filter(|destination| destination.id.starts_with("num_bpm_preview_to"))
                .map(|destination| part_span(destination).1)
                .collect::<Vec<_>>();
            assert_eq!(to, vec![play_x - 8, play_x - 8], "{layout} {type_label}");
            assert!(destinations.iter().all(|destination| {
                destination.id.starts_with("num_bpm_preview_to")
                    || destination.draw.contains("number(19061)")
            }));
        }
    }
}

/// Arrow and far-number positions follow the digit count of the lane-side number.
#[test]
fn antique_bpm_preview_digit_variants_are_exclusive_when_available() {
    if !antique_path().is_file() {
        return;
    }
    let document = load_antique(&[("BPM変化予告 (BMZ)", "TYPE C (レーン外・判定文字の横)")]);
    let arrows = preview_destinations(&document)
        .into_iter()
        .filter(|destination| destination.id == "txt_bpm_preview_arrow_inc")
        .filter(|destination| destination.op.contains(&19065))
        .map(|destination| (destination.draw.clone(), frame(&destination).x.unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        arrows,
        vec![
            // x = lane edge + digits * width + gap (9) + half arrow box (10).
            ("number(19062) < 10".to_string(), 601 + DIGIT_W + 19),
            ("number(19062) >= 10 && number(19062) < 100".to_string(), 601 + DIGIT_W * 2 + 19),
            ("number(19062) >= 100".to_string(), 601 + DIGIT_W * 3 + 19),
        ]
    );
}

#[test]
#[ignore = "requires a GPU and antique assets; verifies preview color and placement in pixels"]
fn antique_bpm_preview_gpu_draws_colored_digits_only_inside_the_lead_window() {
    use bmz_render::renderer::SurfaceSize;
    use bmz_render::snapshot::{BpmChangePreviewSnapshot, RenderSnapshot, UpcomingBpmChange};
    assert!(antique_path().is_file(), "initialize the mz-select submodule");
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 1920, height: 1080 }).unwrap();
    let preview_dir = std::env::var_os("BMZ_BPM_PREVIEW_DIR").map(PathBuf::from);
    // `lane_edge` is the play-frame side the preview must hug, if outside the lane.
    for (name, layout, type_label, lane_edge) in [
        ("type-a", "レーン/グラフ/BGA", "TYPE A (判定文字の上)", None),
        ("type-b", "レーン/グラフ/BGA", "TYPE B (判定文字の下)", None),
        ("type-c", "レーン/グラフ/BGA", "TYPE C (レーン外・判定文字の横)", Some(601)),
        ("type-d", "レーン/グラフ/BGA", "TYPE D (レーン外・判定線の横)", Some(601)),
        ("type-c-right-lane", "BGA/グラフ/レーン", "TYPE C (レーン外・判定文字の横)", Some(1322)),
        ("type-d-right-lane", "BGA/グラフ/レーン", "TYPE D (レーン外・判定線の横)", Some(1322)),
    ] {
        let options = BTreeMap::from([
            ("配置".to_string(), layout.to_string()),
            ("BPM変化予告 (BMZ)".to_string(), type_label.to_string()),
            ("BPM変化予告タイミング".to_string(), "2000ms".to_string()),
        ]);
        let document = load_antique(&[
            ("配置", layout),
            ("BPM変化予告 (BMZ)", type_label),
            ("BPM変化予告タイミング", "2000ms"),
        ]);
        let (left, right, bottom, top) = preview_bounds(&preview_destinations(&document));
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &antique_path(),
            kind: SkinKind::Play,
            options: &options,
            files: &BTreeMap::new(),
            runtime_state: &LuaLoadRuntimeState::default(),
            library_roots: &[Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins")],
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
        let mut frames = Vec::new();
        // The lane-side number has 2 / 3 digits on the right of the lane (from)
        // and 3 / 2 digits on the left of the lane (to).
        for (case, remaining_ms, from_bpm, to_bpm) in [
            ("before", 2_500, 153.0, 156.0),
            ("increase", 500, 98.0, 150.0),
            ("decrease", 500, 155.0, 85.0),
        ] {
            renderer.prepare_scene(AppSceneSnapshot::Play(RenderSnapshot {
                time: TimeUs(10_000_000),
                play_elapsed_time: TimeUs(15_000_000),
                ready_elapsed_time: Some(TimeUs(12_000_000)),
                resources_loaded: true,
                key_mode: KeyMode::K7,
                now_bpm: from_bpm,
                bpm_change_preview: BpmChangePreviewSnapshot {
                    next: Some(UpcomingBpmChange { remaining_ms, from_bpm, to_bpm }),
                    since_last_change_ms: None,
                },
                ..Default::default()
            }));
            renderer.render_last_plan().unwrap();
            let pixels = renderer.read_offscreen_rgba().unwrap();
            if let Some(directory) = &preview_dir {
                std::fs::create_dir_all(directory).unwrap();
                image::save_buffer(
                    directory.join(format!("{name}-{case}.png")),
                    &pixels,
                    1920,
                    1080,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            frames.push(pixels);
        }
        // Skin y is bottom-up; image rows are top-down.
        let rows = (1080 - top).max(0) as usize..(1080 - bottom).min(1080) as usize;
        let columns = left.max(0) as usize..right.min(1920) as usize;
        let matching_columns = |frame: &[u8], matches: fn(&[u8]) -> bool| {
            columns
                .clone()
                .filter(|&x| {
                    rows.clone().any(|y| {
                        let index = (y * 1920 + x) * 4;
                        matches(&frame[index..index + 3])
                    })
                })
                .collect::<Vec<_>>()
        };
        // FAST digits are (204,204,255), SLOW digits are (255,204,204).
        let blue = |pixel: &[u8]| pixel[2] > 230 && pixel[0] < 225 && pixel[2] - pixel[0] > 30;
        let red = |pixel: &[u8]| pixel[0] > 230 && pixel[2] < 225 && pixel[0] - pixel[2] > 30;
        assert!(matching_columns(&frames[0], blue).is_empty(), "{name}: hidden before lead");
        assert!(matching_columns(&frames[0], red).is_empty(), "{name}: hidden before lead");
        for (frame, matches, color) in
            [(&frames[1], blue as fn(&[u8]) -> bool, "blue"), (&frames[2], red, "red")]
        {
            let columns = matching_columns(frame, matches);
            assert!(columns.len() > 30, "{name}: {color} preview drawn");
            match lane_edge {
                Some(edge) if edge < 960 => {
                    let first = columns[0] as i32;
                    assert!((edge..edge + 8).contains(&first), "{name} {color}: starts at {first}");
                }
                Some(edge) => {
                    let last = *columns.last().unwrap() as i32;
                    assert!((edge - 8..edge).contains(&last), "{name} {color}: ends at {last}");
                }
                None => {
                    // The arrow box is centered on the lane; digits sit on both sides.
                    let arrow = (left + right) / 2 - 10..(left + right) / 2 + 10;
                    let digit_rows = row_center(frame, matches, &|x| {
                        (left..right).contains(&x) && !arrow.contains(&x)
                    });
                    let arrow_rows = row_center(frame, matches, &|x| arrow.contains(&x));
                    assert!(
                        (digit_rows - arrow_rows).abs() <= 0.5,
                        "{name} {color}: digits {digit_rows}, arrow {arrow_rows}"
                    );
                }
            }
        }
    }
}

/// Vertical center of the matching pixels in the selected columns, in image rows.
fn row_center(frame: &[u8], matches: fn(&[u8]) -> bool, column: &dyn Fn(i32) -> bool) -> f32 {
    let mut rows = (0..1080).filter(|&y| {
        (0..1920).filter(|&x| column(x)).any(|x| {
            let index = (y * 1920 + x as usize) * 4;
            matches(&frame[index..index + 3])
        })
    });
    let first = rows.next().expect("matching pixels") as f32;
    let last = rows.last().map_or(first, |row| row as f32);
    (first + last) / 2.0
}

fn load_antique_with_offset(
    options: &[(&str, &str)],
    offset: bmz_skin::LuaSkinOffsetValue,
) -> SkinDocument {
    load_skin_document(
        &antique_path(),
        SkinKind::Play,
        &options.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            offset_values: BTreeMap::from([("BPM変化予告の表示位置".to_string(), offset)]),
            offset_id_values: BTreeMap::from([(66, offset)]),
            ..Default::default()
        },
        None,
    )
    .unwrap()
    .document
}

/// TYPE A sits above FAST/SLOW and follows the same offsets (LIFT, judge, judge detail).
#[test]
fn antique_bpm_preview_offset_ids_follow_neighbors_when_available() {
    if !antique_path().is_file() {
        return;
    }
    for (type_label, expected) in [
        ("TYPE A (判定文字の上)", vec![3, 32, 33]),
        ("TYPE B (判定文字の下)", vec![3, 32]),
        ("TYPE C (レーン外・判定文字の横)", vec![3, 32]),
        ("TYPE D (レーン外・判定線の横)", vec![3]),
    ] {
        let document = load_antique(&[("BPM変化予告 (BMZ)", type_label)]);
        for destination in preview_destinations(&document) {
            assert_eq!(destination.offsets, expected, "{type_label} {}", destination.id);
        }
    }
}

/// x/y move the preview; w/h resize each digit, the arrow and the gaps; a changes alpha.
#[test]
fn antique_bpm_preview_custom_offset_adjusts_position_size_and_alpha_when_available() {
    if !antique_path().is_file() {
        return;
    }
    let options = [("BPM変化予告 (BMZ)", "TYPE A (判定文字の上)")];
    let base = load_antique(&options);
    let offset =
        bmz_skin::LuaSkinOffsetValue { x: 10, y: -5, w: 13, h: 13, a: -55, ..Default::default() };
    let adjusted = load_antique_with_offset(&options, offset);
    assert!(adjusted.property.iter().all(|property| property.name != "BPM変化予告の表示位置"));
    let frames = |document: &SkinDocument| {
        preview_destinations(document)
            .into_iter()
            .filter(|destination| destination.op.contains(&19066))
            .map(|destination| {
                let first = *frame(&destination);
                let SkinDstEntry::Frame(last) = destination.dst.last().unwrap() else {
                    panic!("frame")
                };
                (destination.id.clone(), first, last.a)
            })
            .collect::<Vec<_>>()
    };
    let base_frames = frames(&base);
    let adjusted_frames = frames(&adjusted);
    let find = |frames: &[(String, SkinAnimationDef, Option<i32>)], id: &str| {
        frames.iter().find(|(frame_id, _, _)| frame_id == id).cloned().unwrap()
    };
    // Digits double in size: 13px -> 26px per digit, alpha 255 - 55.
    let (_, base_to, _) = find(&base_frames, "num_bpm_preview_to_dec");
    let (_, to, to_alpha) = find(&adjusted_frames, "num_bpm_preview_to_dec");
    assert_eq!((to.w, to.h, to_alpha), (Some(26), Some(26), Some(200)));
    assert_eq!(to.y, base_to.y.map(|y| y - 5));
    // The arrow stays centered on the lane, shifted by x, and grows with h.
    let (_, base_arrow, _) = find(&base_frames, "txt_bpm_preview_arrow_dec");
    let (_, arrow, _) = find(&adjusted_frames, "txt_bpm_preview_arrow_dec");
    assert_eq!(arrow.x, base_arrow.x.map(|x| x + 10));
    assert_eq!(arrow.w, base_arrow.w.map(|w| w * 2));
    let size = |document: &SkinDocument| {
        document.text.iter().find(|text| text.id == "txt_bpm_preview_arrow_dec").unwrap().size
    };
    assert_eq!(size(&adjusted), size(&base) * 2);
    // The far number is laid out after the doubled arrow and gap.
    assert_eq!(to.x.unwrap() - arrow.x.unwrap(), (base_to.x.unwrap() - base_arrow.x.unwrap()) * 2);
    let offset_def =
        base.offset.iter().find(|offset| offset.name == "BPM変化予告の表示位置").unwrap();
    assert_eq!(offset_def.id, 66);
}

/// In the draw plan, LIFT and the judge / judge detail offsets move the preview
/// exactly as far as FAST (which uses offsets 3, 32 and 33 in antique).
#[test]
fn antique_bpm_preview_moves_with_lift_and_judge_offsets_when_available() {
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_render::skin_offset::{SkinOffsetValue, SkinOffsetValues};
    use bmz_render::snapshot::{
        BpmChangePreviewSnapshot, DisplayJudgement, RenderSnapshot, UpcomingBpmChange,
    };
    if !antique_path().is_file() {
        return;
    }
    let positions = |type_label: &str, lift: f32, offsets: &[(i32, i32)]| {
        let options = BTreeMap::from([
            ("FAST/SLOW".to_string(), "ON".to_string()),
            ("BPM変化予告 (BMZ)".to_string(), type_label.to_string()),
        ]);
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &antique_path(),
            kind: SkinKind::Play,
            options: &options,
            files: &BTreeMap::new(),
            runtime_state: &LuaLoadRuntimeState::default(),
            library_roots: &[Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins")],
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        let mut renderer = Renderer::default();
        install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
        let mut skin_offsets = SkinOffsetValues::default();
        for &(id, y) in offsets {
            skin_offsets.set(id, SkinOffsetValue { y, ..Default::default() });
        }
        renderer.prepare_scene(AppSceneSnapshot::Play(RenderSnapshot {
            time: TimeUs(10_000_000),
            play_elapsed_time: TimeUs(15_000_000),
            ready_elapsed_time: Some(TimeUs(12_000_000)),
            resources_loaded: true,
            key_mode: KeyMode::K7,
            lift,
            skin_offsets,
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key1,
                judge: Judge::Great,
                side: Some(TimingSide::Fast),
                text: "GREAT".into(),
                combo: 1,
                delta_us: 20_000,
                time: TimeUs(9_870_000),
                is_miss: false,
                timing_ms_suppressed: false,
            }],
            bpm_change_preview: BpmChangePreviewSnapshot {
                next: Some(UpcomingBpmChange { remaining_ms: 500, from_bpm: 155.0, to_bpm: 85.0 }),
                since_last_change_ms: None,
            },
            ..Default::default()
        }));
        let commands = &renderer.last_plan().unwrap().commands;
        let arrow = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text { text, origin, .. } if text == "»" => Some(origin.y * 1080.0),
                _ => None,
            })
            .expect("preview arrow");
        // FAST image: 72x20 skin pixels.
        let fast = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Image { rect, .. }
                    if (rect.width * 1920.0 - 72.0).abs() < 0.5
                        && (rect.height * 1080.0 - 20.0).abs() < 0.5 =>
                {
                    Some(rect.y * 1080.0)
                }
                _ => None,
            })
            .expect("FAST image");
        (arrow, fast)
    };
    for type_label in [
        "TYPE A (判定文字の上)",
        "TYPE B (判定文字の下)",
        "TYPE C (レーン外・判定文字の横)",
        "TYPE D (レーン外・判定線の横)",
    ] {
        let (arrow, fast) = positions(type_label, 0.0, &[]);
        // TYPE A follows all FAST offsets; B/C do not follow the judge detail
        // offset; D follows LIFT only.
        let cases: &[(f32, &[(i32, i32)], bool)] =
            match type_label.split_whitespace().nth(1).unwrap() {
                "A" => &[(0.1, &[(32, 20), (33, 30)], true)],
                "B" | "C" => &[(0.1, &[(32, 20)], true), (0.0, &[(33, 30)], false)],
                _ => &[(0.1, &[], true), (0.0, &[(32, 20), (33, 30)], false)],
            };
        for &(lift, offsets, follows_fast) in cases {
            let (moved_arrow, moved_fast) = positions(type_label, lift, offsets);
            let arrow_delta = moved_arrow - arrow;
            let fast_delta = moved_fast - fast;
            assert!(fast_delta.abs() > 1.0, "{type_label}: FAST moved by {fast_delta}");
            if follows_fast {
                assert!(
                    (arrow_delta - fast_delta).abs() < 0.5,
                    "{type_label} {offsets:?}: {arrow_delta} vs {fast_delta}"
                );
            } else {
                assert!(
                    arrow_delta.abs() < 0.5,
                    "{type_label} {offsets:?}: moved by {arrow_delta}"
                );
            }
        }
    }
}
