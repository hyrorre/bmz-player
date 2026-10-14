//! BPM change preview added to the separately installed ADFX02 (EC:FN / Starseeker) 7K play skins.
//! These skins must not be redistributed, so the tests are ignored and require local assets.
use super::*;
use bmz_render::skin::{SkinAnimationDef, SkinDestinationDef, SkinDocument, SkinDstEntry};

const DIGIT_W: i32 = 16;
const DIGITS: i32 = 3;
const TYPES: [&str; 4] = [
    "TYPE A (判定文字の上)",
    "TYPE B (判定文字の下)",
    "TYPE C (レーン外・判定文字の横)",
    "TYPE D (レーン外・判定線の横)",
];

#[derive(Clone, Copy)]
enum Skin {
    Ecfn,
    Starseeker,
}

impl Skin {
    fn path(self) -> PathBuf {
        let relative = match self {
            Self::Ecfn => "skins/ADFX02/ECFN/play/play7.luaskin",
            Self::Starseeker => "skins/ADFX02/Starseeker/play/play7.luaskin",
        };
        let path = test_app_paths().resource_dir.join(relative);
        assert!(path.is_file(), "install ADFX02 at {}", path.display());
        path
    }

    fn atlas(self) -> &'static str {
        match self {
            Self::Ecfn => "bg",
            Self::Starseeker => "system",
        }
    }
}

fn load(skin: Skin, options: &[(&str, &str)]) -> SkinDocument {
    let path = skin.path();
    let context = SkinPathContext::new(&path, test_app_paths().skin_library_roots()).unwrap();
    load_skin_document_with_path_context(
        &path,
        SkinKind::Play,
        &options.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
        Some(&context),
    )
    .unwrap()
    .document
}

fn preview_destinations(document: &SkinDocument) -> Vec<SkinDestinationDef> {
    document
        .all_destinations(&document.enabled_options())
        .into_iter()
        .filter(|destination| {
            destination.id.starts_with("num_bpm_preview_")
                || destination.id.starts_with("txt_bpm_preview_arrow_")
        })
        .cloned()
        .collect()
}

fn frame(destination: &SkinDestinationDef) -> &SkinAnimationDef {
    let SkinDstEntry::Frame(frame) = &destination.dst[0] else { panic!("preview frame") };
    frame
}

fn is_arrow(destination: &SkinDestinationDef) -> bool {
    destination.id.starts_with("txt_bpm_preview_arrow")
}

/// `(left, right, bottom, top)`. The arrow box is taller than its glyph, so the
/// vertical range comes from the digits.
fn bounds(destinations: &[SkinDestinationDef]) -> (i32, i32, i32, i32) {
    destinations.iter().fold((i32::MAX, i32::MIN, i32::MAX, i32::MIN), |b, destination| {
        let frame = frame(destination);
        let (x, w) = (frame.x.unwrap(), frame.w.unwrap());
        let (left, right) =
            if is_arrow(destination) { (x - w / 2, x + w / 2) } else { (x, x + w * DIGITS) };
        let (bottom, top) = if is_arrow(destination) {
            (b.2, b.3)
        } else {
            (b.2.min(frame.y.unwrap()), b.3.max(frame.y.unwrap() + frame.h.unwrap()))
        };
        (b.0.min(left), b.1.max(right), bottom, top)
    })
}

#[test]
#[ignore = "requires separately installed ADFX02 assets"]
fn adfx02_bpm_preview_defaults_off_and_adds_bmz_options() {
    for skin in [Skin::Ecfn, Skin::Starseeker] {
        let document = load(skin, &[]);
        assert_eq!(document.bmz_bpm_change_preview, None);
        assert!(preview_destinations(&document).is_empty());
        let names =
            document.property.iter().map(|property| property.name.as_str()).collect::<Vec<_>>();
        assert!(names.contains(&"BPM変化予告 (BMZ)") && names.contains(&"BPM変化予告タイミング"));
    }
}

#[test]
#[ignore = "requires separately installed ADFX02 assets"]
fn adfx02_bpm_preview_uses_pgreat_ms_digits_and_places_each_type() {
    // Both skins: 1P lane x=76..515, 2P lane x=1413..1852, judge text y=470.., judge line y=357.
    for skin in [Skin::Ecfn, Skin::Starseeker] {
        for (side, lane) in [("1P", (76, 515)), ("2P", (1413, 1852))] {
            for (index, type_label) in TYPES.into_iter().enumerate() {
                let document = load(
                    skin,
                    &[
                        ("プレーサイド", side),
                        ("BPM変化予告 (BMZ)", type_label),
                        ("BPM変化予告タイミング", "1500ms"),
                    ],
                );
                let label = format!("{} {side} {type_label}", skin.atlas());
                assert_eq!(
                    document.bmz_bpm_change_preview.map(|preview| preview.lead_ms),
                    Some(1_500),
                    "{label}"
                );
                let outside = index >= 2;
                // The PGREAT ±ms rows: blue for increase, red for decrease.
                for (key, src_y) in [("inc", 3214), ("dec", 3230)] {
                    for (part, ref_id) in [("from", 19062), ("to", 19061)] {
                        let id = format!("num_bpm_preview_{part}_{key}");
                        let value = document.value.iter().find(|value| value.id == id).unwrap();
                        assert_eq!(value.src, skin.atlas(), "{label} {id}");
                        assert_eq!((value.x, value.y, value.w, value.h), (207, src_y, 160, 16));
                        assert_eq!((value.divx, value.digit, value.ref_id), (10, DIGITS, ref_id));
                    }
                }
                let destinations = preview_destinations(&document);
                assert_eq!(destinations.len(), if outside { 14 } else { 6 }, "{label}");
                let (left, right, bottom, top) = bounds(&destinations);
                assert_eq!(top - bottom, DIGIT_W, "{label}");
                if !outside {
                    let center = (lane.0 + lane.1) / 2;
                    assert!(((left + right) / 2 - center).abs() <= 1, "{label}: {left}..{right}");
                } else if side == "1P" {
                    // The lane-side number starts right next to the lane.
                    assert_eq!(left, lane.1 + 17, "{label}");
                } else {
                    assert_eq!(right, lane.0 - 17, "{label}");
                }
                match index {
                    0 => assert!(bottom >= 576, "{label}: above judge detail at {bottom}"),
                    1 => assert!(bottom > 359 && top <= 470, "{label}: {bottom}..{top}"),
                    2 => assert!(bottom >= 470 && top <= 560, "{label}: {bottom}..{top}"),
                    _ => assert!(bottom < 358 && top > 358, "{label}: {bottom}..{top}"),
                }
            }
        }
    }
}

/// Starseeker places FAST/SLOW and score difference next to the lane; the
/// preview moves 6px above them instead of overlapping or crowding them.
#[test]
#[ignore = "requires separately installed ADFX02 assets"]
fn starseeker_bpm_preview_moves_above_outside_judge_details() {
    let preview_rect =
        |options: &[(&str, &str)]| bounds(&preview_destinations(&load(Skin::Starseeker, options)));
    for side in ["1P", "2P"] {
        let free = preview_rect(&[
            ("プレーサイド", side),
            ("fast/slow", "Off"),
            ("スコア差分", "Off"),
            ("BPM変化予告 (BMZ)", TYPES[2]),
        ]);
        // FAST/SLOW TYPE-B: y=524..542.
        let above_fast_slow = preview_rect(&[
            ("プレーサイド", side),
            ("fast/slow", "EARLY/LATE"),
            ("fast/slow位置", "TYPE-B"),
            ("スコア差分", "Off"),
            ("BPM変化予告 (BMZ)", TYPES[2]),
        ]);
        assert!(free.2 < 542, "{side}: TYPE C overlaps FAST/SLOW TYPE-B by default");
        assert_eq!(above_fast_slow.2, 548, "{side}");
        assert_eq!((above_fast_slow.0, above_fast_slow.1), (free.0, free.1), "{side}");

        // Score difference TYPE-C: y=354..370 next to the judge line.
        let above_score = preview_rect(&[
            ("プレーサイド", side),
            ("fast/slow", "Off"),
            ("スコア差分", "mybest"),
            ("スコア差分位置", "TYPE-C"),
            ("BPM変化予告 (BMZ)", TYPES[3]),
        ]);
        assert_eq!(above_score.2, 376, "{side}");
    }
}

#[test]
#[ignore = "requires ADFX02 assets and a GPU; set BMZ_BPM_PREVIEW_DIR to save images"]
fn adfx02_bpm_preview_gpu_draws_colored_digits_next_to_the_lane() {
    use bmz_render::renderer::SurfaceSize;
    use bmz_render::snapshot::{BpmChangePreviewSnapshot, RenderSnapshot, UpcomingBpmChange};
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 1920, height: 1080 }).unwrap();
    let preview_dir = std::env::var_os("BMZ_BPM_PREVIEW_DIR").map(PathBuf::from);
    for skin in [Skin::Ecfn, Skin::Starseeker] {
        for (side, type_index, lane_edge) in
            [("1P", 0, None), ("1P", 1, None), ("1P", 2, Some(532)), ("2P", 3, Some(1396))]
        {
            let option_pairs = [("プレーサイド", side), ("BPM変化予告 (BMZ)", TYPES[type_index])];
            let name = format!("{}-{side}-{}", skin.atlas(), (b'a' + type_index as u8) as char);
            let (left, right, bottom, top) =
                bounds(&preview_destinations(&load(skin, &option_pairs)));
            let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
                pinned_sources: None,
                skin_path: &skin.path(),
                kind: SkinKind::Play,
                options: &option_pairs
                    .iter()
                    .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                    .collect(),
                files: &BTreeMap::new(),
                runtime_state: &LuaLoadRuntimeState::default(),
                library_roots: &test_app_paths().skin_library_roots(),
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
            // Skin y is bottom-up; image rows are top-down. Include the arrow glyph margin.
            let rows = (1080 - top - 4).max(0) as usize..(1080 - bottom + 4).min(1080) as usize;
            let columns = left.max(0) as usize..right.min(1920) as usize;
            let matching = |frame: &[u8], matches: fn(&[u8]) -> bool| {
                let mut found = Vec::new();
                for x in columns.clone() {
                    for y in rows.clone() {
                        let index = (y * 1920 + x) * 4;
                        if matches(&frame[index..index + 3]) {
                            found.push((x as i32, y as i32));
                        }
                    }
                }
                found
            };
            // Digit cores are about (70,149,218) and (238,76,76).
            let blue = |p: &[u8]| p[2] > 180 && p[0] < 120 && p[2] as i32 - p[0] as i32 > 80;
            let red = |p: &[u8]| p[0] > 200 && p[2] < 120 && p[0] as i32 - p[2] as i32 > 80;
            assert!(matching(&frames[0], blue).is_empty(), "{name}: hidden before lead");
            assert!(matching(&frames[0], red).is_empty(), "{name}: hidden before lead");
            for (frame, matches, color) in
                [(&frames[1], blue as fn(&[u8]) -> bool, "blue"), (&frames[2], red, "red")]
            {
                let pixels = matching(frame, matches);
                assert!(pixels.len() > 60, "{name}: {color} preview drawn");
                let first = pixels.iter().map(|p| p.0).min().unwrap();
                let last = pixels.iter().map(|p| p.0).max().unwrap();
                // Narrow glyphs such as "1" sit inside their 16px cell.
                match lane_edge {
                    Some(edge) if edge < 960 => {
                        assert!(
                            (edge..edge + 8).contains(&first),
                            "{name} {color}: starts at {first}"
                        )
                    }
                    Some(edge) => {
                        assert!((edge - 8..edge).contains(&last), "{name} {color}: ends at {last}")
                    }
                    None => {
                        // The arrow is centered on the lane; digits sit on both sides.
                        let mid = (left + right) / 2;
                        let center = |selected: &dyn Fn(i32) -> bool| {
                            let ys = pixels.iter().filter(|p| selected(p.0)).map(|p| p.1);
                            let (min, max) =
                                ys.fold((i32::MAX, i32::MIN), |r, y| (r.0.min(y), r.1.max(y)));
                            (min + max) as f32 / 2.0
                        };
                        let arrow = center(&|x| (mid - 10..mid + 10).contains(&x));
                        let digits = center(&|x| !(mid - 10..mid + 10).contains(&x));
                        assert!(
                            (arrow - digits).abs() <= 1.0,
                            "{name} {color}: digits {digits}, arrow {arrow}"
                        );
                    }
                }
            }
        }
    }
}

/// Writes screenshots of every preview type next to each FAST/SLOW and score
/// difference position, with a judgement on screen so those details are drawn.
#[test]
#[ignore = "requires ADFX02 assets and a GPU; writes screenshots to BMZ_BPM_PREVIEW_DIR"]
fn adfx02_bpm_preview_judge_detail_screenshots() {
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_render::renderer::SurfaceSize;
    use bmz_render::snapshot::{
        BpmChangePreviewSnapshot, DisplayJudgement, RenderSnapshot, UpcomingBpmChange,
    };
    let directory =
        PathBuf::from(std::env::var_os("BMZ_BPM_PREVIEW_DIR").expect("set BMZ_BPM_PREVIEW_DIR"));
    std::fs::create_dir_all(&directory).unwrap();

    let mut cases: Vec<(Skin, String, Vec<(&str, &str)>)> = Vec::new();
    for side in ["1P", "2P"] {
        for (type_index, type_label) in TYPES.into_iter().enumerate() {
            let type_name = (b'a' + type_index as u8) as char;
            for fast_slow in ["TYPE-A", "TYPE-B", "TYPE-C"] {
                for score in ["TYPE-A", "TYPE-B", "TYPE-C"] {
                    cases.push((
                        Skin::Starseeker,
                        format!("starseeker-{side}-{type_name}-fs{fast_slow}-diff{score}"),
                        vec![
                            ("プレーサイド", side),
                            ("fast/slow", "EARLY/LATE"),
                            ("fast/slow位置", fast_slow),
                            ("スコア差分", "mybest"),
                            ("スコア差分位置", score),
                            // Starseeker defines the score difference in the graph module.
                            ("スコアグラフ", "On"),
                            ("BPM変化予告 (BMZ)", type_label),
                        ],
                    ));
                }
            }
            cases.push((
                Skin::Ecfn,
                format!("ecfn-{side}-{type_name}"),
                vec![
                    ("プレーサイド", side),
                    ("判定タイミング表示", "EARLY/LATE"),
                    ("スコア差分", "mybest"),
                    ("BPM変化予告 (BMZ)", type_label),
                ],
            ));
        }
    }
    // Mirror a normal (non-autoplay) play start with a saved best, as the app passes
    // to play skins; score difference draw callbacks read NUMBER_HIGHSCORE (150).
    let runtime_state = LuaLoadRuntimeState {
        runtime_mode: bmz_skin::LuaSkinRuntimeMode::Compat,
        number_values: BTreeMap::from([(150, 1_200)]),
        option_values: BTreeMap::from([(32, true), (33, false), (61, true), (82, true)]),
        ..Default::default()
    };
    for (skin, name, options) in cases {
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            pinned_sources: None,
            skin_path: &skin.path(),
            kind: SkinKind::Play,
            options: &options.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect(),
            files: &BTreeMap::new(),
            runtime_state: &runtime_state,
            library_roots: &test_app_paths().skin_library_roots(),
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap();
        // A fresh renderer per screenshot: skins reuse texture IDs between installs.
        let mut renderer = Renderer::default();
        renderer.attach_offscreen(SurfaceSize { width: 1920, height: 1080 }).unwrap();
        install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
        // The judge image blinks between a text frame and a blank frame every 30ms;
        // 130ms after the judgement shows the text frame.
        let now = TimeUs(10_000_000);
        renderer.prepare_scene(AppSceneSnapshot::Play(RenderSnapshot {
            time: now,
            play_elapsed_time: TimeUs(15_000_000),
            ready_elapsed_time: Some(TimeUs(12_000_000)),
            resources_loaded: true,
            key_mode: KeyMode::K7,
            now_bpm: 155.0,
            ex_score: 1_234,
            past_notes: 700,
            total_notes: 1_000,
            best_ex_score: Some(1_200),
            projected_best_ex_score: Some(1_100),
            target_ex_score: Some(1_300),
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key4,
                judge: Judge::Great,
                side: Some(TimingSide::Fast),
                text: "GREAT".into(),
                combo: 345,
                delta_us: 23_000,
                time: TimeUs(now.0 - 130_000),
                is_miss: false,
                timing_ms_suppressed: false,
            }],
            bpm_change_preview: BpmChangePreviewSnapshot {
                next: Some(UpcomingBpmChange { remaining_ms: 500, from_bpm: 155.0, to_bpm: 202.0 }),
                since_last_change_ms: None,
            },
            ..Default::default()
        }));
        renderer.render_last_plan().unwrap();
        let pixels = renderer.read_offscreen_rgba().unwrap();
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &pixels,
            1920,
            1080,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
