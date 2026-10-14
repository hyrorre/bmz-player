use super::*;
use bmz_render::skin::{SkinDestinationDef, SkinDocument, SkinDstEntry};

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

fn preview_destinations(document: &SkinDocument) -> Vec<SkinDestinationDef> {
    document
        .all_destinations(&document.enabled_options())
        .into_iter()
        .filter(|destination| destination.id.starts_with("txt_bpm_preview"))
        .cloned()
        .collect()
}

/// Returns `(left, right, bottom, top)` of the preview group in skin pixels.
fn preview_bounds(destinations: &[SkinDestinationDef]) -> (i32, i32, i32, i32) {
    let frames = destinations.iter().map(|destination| {
        let SkinDstEntry::Frame(frame) = &destination.dst[0] else { panic!("preview frame") };
        let (x, w) = (frame.x.unwrap(), frame.w.unwrap());
        // Text x is the alignment anchor: right edge, center, or left edge.
        let left = match destination.id.as_str() {
            "txt_bpm_preview_from" => x - w,
            "txt_bpm_preview_arrow" => x - w / 2,
            _ => x,
        };
        (left, frame.y.unwrap(), w, frame.h.unwrap())
    });
    frames.fold((i32::MAX, i32::MIN, i32::MAX, i32::MIN), |bounds, (x, y, w, h)| {
        (bounds.0.min(x), bounds.1.max(x + w), bounds.2.min(y), bounds.3.max(y + h))
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
    assert!(!document.text.iter().any(|text| text.id.starts_with("txt_bpm_preview")));
}

#[test]
fn antique_bpm_preview_types_place_colored_text_when_available() {
    if !antique_path().is_file() {
        return;
    }
    // Lane geometry for the default layout: lane x=63..576, judge line y=353,
    // judge image y=476..560, FAST/SLOW y=575..595, play frame x=6..593.
    let lane_center = (63 + 576) / 2;
    for (lead_label, lead_ms) in [("500ms", 500), ("2000ms", 2_000), ("4000ms", 4_000)] {
        for (type_label, expected_side) in [
            ("TYPE A (判定文字の上)", "inside"),
            ("TYPE B (判定文字の下)", "inside"),
            ("TYPE C (レーン外・判定文字の横)", "right"),
            ("TYPE D (レーン外・判定線の横)", "right"),
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
            let texts = |id: &str| document.text.iter().find(|text| text.id == id).unwrap();
            assert_eq!(texts("txt_bpm_preview_from").number_ref, Some(19062));
            assert_eq!(texts("txt_bpm_preview_to").number_ref, Some(19061));
            assert_eq!(texts("txt_bpm_preview_arrow").constant_text, ">>");

            let destinations = preview_destinations(&document);
            assert_eq!(destinations.len(), 6, "{type_label}");
            for destination in &destinations {
                assert_eq!(destination.timer, Some(19068));
                assert!(destination.op.contains(&19064));
                let SkinDstEntry::Frame(frame) = &destination.dst[0] else { panic!("frame") };
                let color = (frame.r, frame.g, frame.b);
                if destination.op.contains(&19065) {
                    assert_eq!(color, (Some(80), Some(170), Some(255)), "increase is blue");
                } else {
                    assert!(destination.op.contains(&19066));
                    assert_eq!(color, (Some(255), Some(80), Some(80)), "decrease is red");
                }
            }

            let (left, right, bottom, top) = preview_bounds(&destinations);
            let backing = document
                .all_destinations(&document.enabled_options())
                .into_iter()
                .filter(|destination| destination.id == "pnl_bpm_preview")
                .map(|destination| (destination.timer, destination.op.clone()))
                .collect::<Vec<_>>();
            if expected_side == "inside" {
                assert!(backing.is_empty(), "{type_label}: keep notes visible");
            } else {
                assert_eq!(backing, vec![(Some(19068), vec![19064])], "{type_label}");
            }
            match expected_side {
                "inside" => assert!(
                    (left + right) / 2 - lane_center <= 1 && left >= 63 && right <= 576,
                    "{type_label}: {left}..{right}"
                ),
                _ => assert!(left >= 593, "{type_label} outside the lane: {left}"),
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
    for layout in ["BGA/グラフ/レーン", "グラフ/BGA/レーン", "BGA/レーン/グラフ"]
    {
        for type_label in ["TYPE C (レーン外・判定文字の横)", "TYPE D (レーン外・判定線の横)"]
        {
            let document = load_antique(&[("配置", layout), ("BPM変化予告 (BMZ)", type_label)]);
            let (_, right, _, _) = preview_bounds(&preview_destinations(&document));
            // Right-side play frames start at x=1078 or x=1330.
            let play_x = if layout == "BGA/レーン/グラフ" { 1078 } else { 1330 };
            assert!(right <= play_x, "{layout} {type_label}: {right}");
        }
    }
}

#[test]
#[ignore = "requires a GPU and antique assets; verifies preview text color and placement in pixels"]
fn antique_bpm_preview_gpu_draws_colored_text_only_inside_the_lead_window() {
    use bmz_render::renderer::SurfaceSize;
    use bmz_render::snapshot::{BpmChangePreviewSnapshot, RenderSnapshot, UpcomingBpmChange};
    assert!(antique_path().is_file(), "initialize the mz-select submodule");
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 1920, height: 1080 }).unwrap();
    let preview_dir = std::env::var_os("BMZ_BPM_PREVIEW_DIR").map(PathBuf::from);
    for (name, layout, type_label) in [
        ("type-a", "レーン/グラフ/BGA", "TYPE A (判定文字の上)"),
        ("type-b", "レーン/グラフ/BGA", "TYPE B (判定文字の下)"),
        ("type-c", "レーン/グラフ/BGA", "TYPE C (レーン外・判定文字の横)"),
        ("type-d", "レーン/グラフ/BGA", "TYPE D (レーン外・判定線の横)"),
        ("type-c-right-lane", "BGA/グラフ/レーン", "TYPE C (レーン外・判定文字の横)"),
        ("type-d-right-lane", "BGA/グラフ/レーン", "TYPE D (レーン外・判定線の横)"),
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
        for (case, remaining_ms, from_bpm, to_bpm) in [
            ("before", 2_500, 153.0, 156.0),
            ("increase", 500, 153.0, 156.0),
            ("decrease", 500, 155.0, 149.0),
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
        let count = |frame: &[u8], matches: fn(&[u8]) -> bool| {
            rows.clone()
                .flat_map(|y| columns.clone().map(move |x| (y * 1920 + x) * 4))
                .filter(|&index| matches(&frame[index..index + 3]))
                .count()
        };
        let blue = |pixel: &[u8]| pixel[2] > 200 && pixel[1] > 120 && pixel[0] < 140;
        let red = |pixel: &[u8]| pixel[0] > 200 && pixel[1] < 140 && pixel[2] < 140;
        assert_eq!(count(&frames[0], blue), count(&frames[2], blue), "{name}: no blue before");
        assert!(count(&frames[1], blue) > count(&frames[0], blue) + 50, "{name}: blue text");
        assert!(count(&frames[2], red) > count(&frames[0], red) + 50, "{name}: red text");
        assert_eq!(frames[0].len(), frames[1].len());
    }
}
