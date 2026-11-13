use super::*;
#[path = "qwilight_additions.rs"]
mod additions;

fn fixture() -> (tempfile::TempDir, DecodedSkin) {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("skin.yaml");
    fs::write(
        &path,
        r#"
format: {zip: skin}
frame:
  judgment-paint-frame: 2
  judgment-paint-framerate: 24
  higher-judgment-paint-frame-loop: 48
lambda:
  drawing1: 1, 2, 3, 4, 5, 10
  pipeline: 100, 101, 102, 2, 3, 8, 31, 7, 6, 54, 55, 56, 15, 9, 10
  setJudgmentMainPosition: true
point:
  noteLength: 20
  noteHeight: 12
  judgmentMainPosition: 500
  judgmentMainPosition1: 500
  judgmentMainHeight: 2
  inputPosition1: 500
  inputHeight: 10
  mainWall0Length: 2
  mainWall0Height: 500
  mainWall1Length: 2
  mainWall1Height: 500
  paintProperty0: 0, 0, 1280, 720, 0
  paintProperty1: 200, 100, 20, 20, 0, 2, 10, 2
  paintProperty2: 0, 500, 160, 2, 0, 1, 15
  judgmentPaintPosition0: 200
  judgmentPaintPosition1: 400
  judgmentPaintLength: 100
  judgmentPaintHeight: 30
  binLength: 10
  binHeight: 20
  bandPosition0: 200
  bandPosition1: 430
  judgmentMeterPosition0: 200
  judgmentMeterPosition1: 370
  judgmentMeterFrontDrawingLength: 30
  judgmentMeterUnitDrawingLength: 10
  hitPointsPosition0: 300
  hitPointsPosition1: 100
  hitPointsLength: 10
  hitPointsHeight: 200
  earlyValuePosition0: 320
  earlyValuePosition1: 500
  lateValuePosition0: 320
  lateValuePosition1: 530
  judgmentVSVisualizerPosition0: 320
  judgmentVSVisualizerPosition1: 560
  judgmentVSVisualizerStopPointDrawingLength: 4
"#,
    )
    .unwrap();
    let mut names: Vec<String> = [
        "Note/N 1 0 0.png",
        "Main/J 1 0.png",
        "Main/M 1 0.png",
        "Main/W 0.png",
        "Main/W 1.png",
        "Input/I 1 0.png",
        "Paint/N 1 0.png",
        "Drawing/P 0.png",
        "Drawing/P 1 0.png",
        "Drawing/P 1 1.png",
        "Drawing/P 2.png",
        "Drawing/HP 1 0.png",
        "Bin/JM +.png",
        "Bin/JM -.png",
        "Bin/Millis.png",
        "Bin/..png",
    ]
    .map(str::to_string)
    .into();
    names.extend((0..10).map(|i| format!("Bin/{i}.png")));
    for judge in 0..6 {
        names.extend((0..2).map(|frame| format!("Paint/J {judge} {frame}.png")));
    }
    let mut zip = zip::ZipWriter::new(fs::File::create(root.path().join("skin.zip")).unwrap());
    for (index, name) in names.iter().enumerate() {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&png_bytes(2, 2, [index as u8, 255, 255, 255])).unwrap();
    }
    zip.finish().unwrap();
    let decoded = decode_qwilight(&path, root.path(), 6, BTreeMap::new());
    (root, decoded)
}

fn sources(decoded: &DecodedSkin) -> std::collections::HashMap<String, SkinDocumentTexture> {
    decoded
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
        .collect()
}

#[test]
fn qwilight_judge_line_decoration_follows_pipeline_order() {
    let (root, _) = fixture();
    let path = root.path().join("skin.yaml");
    let yaml = fs::read_to_string(&path).unwrap();
    for (pipeline, after_notes) in [("31, 102, 7", false), ("31, 7, 102", true)] {
        fs::write(
            &path,
            yaml.replace("100, 101, 102, 2, 3, 8, 31, 7, 6, 54, 55, 56, 15, 9, 10", pipeline),
        )
        .unwrap();
        let decoded = decode_qwilight(&path, root.path(), 6, BTreeMap::new());
        let image = decoded.document.image.iter().find(|i| i.id == "qw-paint-2").unwrap();
        let textures = sources(&decoded);
        let texture = textures[&image.src].texture;
        let (behind, front, _) = decoded.document.static_render_items_split(
            &textures,
            &SkinDrawState::default(),
            &SkinTextState::default(),
        );
        let contains_line = |items: &[SkinRenderItem]| {
            items.iter().any(
                |item| matches!(item, SkinRenderItem::Image { texture: t, .. } if *t == texture),
            )
        };
        assert_eq!(contains_line(&behind), !after_notes, "pipeline {pipeline}");
        assert_eq!(contains_line(&front), after_notes, "pipeline {pipeline}");
    }
}

fn items(decoded: &DecodedSkin, prefix: &str, state: &SkinDrawState) -> Vec<SkinRenderItem> {
    let mut document = decoded.document.clone();
    document.destination.retain(
        |entry| matches!(entry, DestinationListEntry::Single(d) if d.id.starts_with(prefix)),
    );
    document.static_render_items(&sources(decoded), state, &SkinTextState::default())
}

fn digits(decoded: &DecodedSkin, id: &str, state: &SkinDrawState) -> String {
    let src = &decoded.document.value.iter().find(|v| v.id == id).unwrap().src;
    let texture = sources(decoded)[src].texture;
    items(decoded, id, state)
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { texture: t, uv, .. } if *t == texture => {
                Some(char::from(b'0' + (uv.x * 10.0).round() as u8))
            }
            _ => None,
        })
        .collect()
}

fn information_fixture() -> (tempfile::TempDir, DecodedSkin) {
    let (root, _) = fixture();
    let path = root.path().join("skin.yaml");
    let mut yaml = fs::read_to_string(&path).unwrap().replace(
        "pipeline: 100, 101, 102, 2, 3, 8, 31, 7, 6, 54, 55, 56, 15, 9, 10",
        "pipeline: 100, 101, 102, 103, 2, 3, 8, 31, 7, 6, 54, 55, 56, 15, 9, 10, 47, 48, 51, 52, 59, 60, 44, 29, 43, 61, 1",
    );
    yaml.push_str(
        r#"
  titleSystem0: 2
  titlePosition0: 210
  titlePosition1: 16
  titleHeight: 42
  artistPosition0: 210
  genrePosition0: 210
  levelTextPosition0: 760
  levelTextPosition1: 690
  wantLevelPosition0: 650
  wantLevelPosition1: 690
  binInputVisualizerLength: 11
  binInputVisualizerHeight: 15
  inputVisualizerPosition0: 45
  inputVisualizerPosition1: 620
  binHmsLength: 13
  binHmsHeight: 17
  hmsPosition0: 410
  hmsPosition1: 620
  hmsSystem: 2
  judgmentVisualizerPosition0: 450
  judgmentVisualizerPosition1: 470
  judgmentVisualizerLength: 360
  judgmentVisualizerHeight: 14
  judgmentVisualizerSystem: 3
  audioVisualizerPosition0: 450
  audioVisualizerPosition1: 520
  audioVisualizerLength: 360
  audioVisualizerHeight: 200
  judgmentInputVisualizerPosition0: 10
  judgmentInputVisualizerPosition1: 640
  judgmentInputVisualizerLength: 400
  judgmentInputVisualizerHeight: 80
  paintProperty3: 0, 0, 20, 20, 0, 1, 24, 0, 43
font:
  titleLevel: 26
paint:
  title: DeepSkyBlue
"#,
    );
    fs::write(&path, yaml).unwrap();
    let decoded = decode_qwilight(&path, root.path(), 6, BTreeMap::new());
    (root, decoded)
}

#[test]
fn qwilight_information_uses_point_anchored_text_and_live_numbers() {
    let (_root, decoded) = information_fixture();
    let document = &decoded.document;
    let title = document.text.iter().find(|t| t.id == "qw-text-title").unwrap();
    assert_eq!((title.ref_id, title.align, title.size), (12, 1, 26));
    let text = document.static_render_items(
        &sources(&decoded),
        &SkinDrawState::default(),
        &SkinTextState {
            title: "曲名",
            subtitle: "[追加情報]",
            artist: "作曲者",
            genre: "GENRE",
            play_level: "18.5",
            table_level: "6UE19",
            table_text_fallback: "6UE19 表の名前",
            ..Default::default()
        },
    );
    let rendered = format!("{text:?}");
    for expected in ["曲名", "追加情報", "作曲者", "GENRE", "LV 18.5", "6UE19"] {
        assert!(rendered.contains(expected), "missing {expected}");
    }
    let state = SkinDrawState {
        keylogger_nps: 19,
        play_timer_ms: Some(66_000),
        chart_length_ms: Some(158_000),
        play_screen: true,
        ..Default::default()
    };
    assert_eq!(digits(&decoded, "qw-nps", &state), "19");
    assert_eq!(digits(&decoded, "qw-time-minutes", &state), "1");
    assert_eq!(digits(&decoded, "qw-time-seconds", &state), "06");
    assert_eq!(digits(&decoded, "qw-time-total-minutes", &state), "2");
    assert_eq!(digits(&decoded, "qw-time-total-seconds", &state), "38");
    let state =
        SkinDrawState { play_timer_ms: Some(600_000), chart_length_ms: Some(3_601_000), ..state };
    assert_eq!(digits(&decoded, "qw-time-minutes", &state), "10");
    assert_eq!(digits(&decoded, "qw-time-total-minutes", &state), "60");
}

#[test]
fn qwilight_visibility_removes_each_element_and_its_linked_decoration() {
    let (root, decoded) = information_fixture();
    let options = decoded
        .document
        .property
        .iter()
        .filter(|p| p.name.starts_with("Qwilight show "))
        .map(|p| (p.name.clone(), "Off".into()))
        .collect();
    let hidden = decode_qwilight(&root.path().join("skin.yaml"), root.path(), 6, options);
    assert!(hidden.document.qwilight_visualizer.is_empty());
    assert!(hidden.document.destination.iter().all(|entry| !matches!(entry,DestinationListEntry::Single(d)
        if d.id.starts_with("qw-text-") || d.id.starts_with("qw-time-") || d.id=="qw-nps" || d.id.starts_with("qw-paint-3") || d.id=="qw-judge" || d.id=="qw-gauge")));
    assert!(hidden.document.note.is_some());
    let restored = decode_qwilight(
        &root.path().join("skin.yaml"),
        root.path(),
        6,
        BTreeMap::from([("Qwilight show Spectrum".into(), "On".into())]),
    );
    assert_eq!(restored.document.qwilight_visualizer.len(), 3);
}

#[test]
fn qwilight_timing_and_spectrum_follow_the_current_state() {
    let (_root, decoded) = information_fixture();
    let mut state = SkinDrawState {
        qwilight_timing: vec![(-50.0, 0, 1), (50.0, 400, 2)],
        ..Default::default()
    };
    let feedback = items(&decoded, "qw-judgmentVisualizer", &state);
    let rects: Vec<_> = feedback
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Rect { rect, color, .. } => Some((rect, color)),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 2);
    assert!(rects[0].0.x < rects[1].0.x);
    assert!(rects[0].1.a > rects[1].1.a);
    state.qwilight_timing[0].1 = 800;
    state.qwilight_timing[1].1 = 800;
    assert!(items(&decoded, "qw-judgmentVisualizer", &state).is_empty());
    state.audio_spectrum[10] = 0.5;
    assert_eq!(items(&decoded, "qw-audioVisualizer", &state).len(), 1);
    state.audio_spectrum.fill(0.0);
    assert!(items(&decoded, "qw-audioVisualizer", &state).is_empty());
}

#[test]
fn qwilight_timing_coordinates_use_fixed_milliseconds_in_every_direction() {
    use bmz_gameplay::judge::model::JudgeWindow;
    let (_root, mut decoded) = information_fixture();
    for system in 0..4 {
        let visualizer = decoded
            .document
            .qwilight_visualizer
            .iter_mut()
            .find(|v| v.id == "qw-judgmentVisualizer")
            .unwrap();
        visualizer.system = system;
        visualizer.line_width = 1;
        visualizer.line_height = 1;
        for (fast, slow, offset) in
            [(220_000, 280_000, 0), (280_000, 220_000, -80), (50_000, 50_000, 100)]
        {
            let state = SkinDrawState {
                note_judge_window: Some(JudgeWindow {
                    bad_fast_us: fast,
                    bad_slow_us: slow,
                    ..JudgeWindow::symmetric(10_000, 30_000, 40_000, 50_000, 500_000, 500_000, 0)
                }),
                judge_timing_offset_ms: offset,
                qwilight_timing: [-500.0, -250.0, -125.0, 0.0, 125.0, 250.0, 500.0]
                    .map(|ms| (ms, 0, 0))
                    .into(),
                ..Default::default()
            };
            let feedback = items(&decoded, "qw-judgmentVisualizer", &state);
            assert_eq!(feedback.len(), 7);
            for (item, expected) in feedback.iter().zip([0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0]) {
                let SkinRenderItem::Rect { rect, .. } = item else {
                    panic!("expected timing mark")
                };
                let actual = if system < 2 {
                    (rect.y * 720.0 - 470.0) / 13.0
                } else {
                    (rect.x * 1280.0 - 450.0) / 359.0
                };
                let expected = if matches!(system, 0 | 2) { 1.0 - expected } else { expected };
                assert!(
                    (actual - expected).abs() < 0.00001,
                    "system={system} window={fast}/{slow} offset={offset}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn qwilight_play_only_import_waits_for_the_entry_fade() {
    let (_root, decoded) = fixture();
    assert_eq!(decoded.document.bmz_play_fadein, 500);
    assert_eq!(decoded.document.bmz_play_fadeout, 500);
    // J 10 (result settlement) holds the exit fade only for Qwilight conversions.
    assert!(decoded.document.bmz_play_clear_completion);
    assert_eq!(decoded.document.loadstart, 500);
    assert_eq!(decoded.document.playstart, 1000);
    assert_eq!(decoded.document.loadend, 0);
    assert_eq!(decoded.document.fadeout, 500);
    assert_eq!(decoded.document.close, 500);
}

#[test]
fn qwilight_counters_ratio_and_timing_use_bmz_values() {
    let (_root, decoded) = fixture();
    for (label, filename) in [("fast", "Bin/JM -.png"), ("slow", "Bin/JM +.png")] {
        let image = decoded
            .document
            .image
            .iter()
            .find(|image| image.id == format!("qw-judgment-meter-{label}-label"))
            .unwrap();
        let source = decoded.document.source.iter().find(|source| source.id == image.src).unwrap();
        assert!(source.path.ends_with(filename), "{label}: {}", source.path);
    }
    let mut state = SkinDrawState {
        play_timer_ms: Some(1000),
        judge_ms: [Some(100), None, None],
        judge_timing_ms: [Some(-25), None, None],
        fast_slow_counts: Some(bmz_render::snapshot::FastSlowJudgeCounts {
            fast_great: 12,
            slow_good: 3,
            ..Default::default()
        }),
        ..Default::default()
    };
    state.judge_counts.pgreat = 19;
    state.judge_counts.great = 2;
    state.judge_counts.empty_poor = 1;
    assert_eq!(digits(&decoded, "qw-number-earlyValue", &state), "12");
    assert_eq!(digits(&decoded, "qw-number-lateValue", &state), "3");
    assert_eq!(digits(&decoded, "qw-number-judgmentVSVisualizer", &state), "633");
    for delta in [-125, -9, 0, 9, 125] {
        state.judge_timing_ms[0] = Some(delta);
        assert_eq!(digits(&decoded, "qw-judgment-meter", &state), delta.abs().to_string());
        let fast = items(&decoded, "qw-judgment-meter-fast-label", &state);
        let slow = items(&decoded, "qw-judgment-meter-slow-label", &state);
        assert_eq!(!fast.is_empty(), delta < 0);
        assert_eq!(!slow.is_empty(), delta > 0);
    }
    state.judge_timing_ms[0] = None;
    assert!(
        items(&decoded, "qw-judgment-meter", &state).is_empty(),
        "respect BMZ timing display filter"
    );
    state.judge_timing_ms[0] = Some(12);
    state.judge_ms[0] = Some(1001);
    assert!(items(&decoded, "qw-judgment-meter", &state).is_empty());
    state.judge_counts = Default::default();
    assert_eq!(digits(&decoded, "qw-number-judgmentVSVisualizer", &state), "000");
    state.judge_counts.pgreat = 1;
    assert!(!items(&decoded, "qw-number-judgmentVSVisualizer-infinity", &state).is_empty());
    assert_eq!(digits(&decoded, "qw-number-judgmentVSVisualizer", &state), "");
}

#[test]
fn qwilight_background_defaults_once_animation_and_gauge_are_visible() {
    let (_root, decoded) = fixture();
    let mut state = SkinDrawState { play_timer_ms: Some(50), gauge: 50.0, ..Default::default() };
    assert_eq!(items(&decoded, "qw-paint-0", &state).len(), 1, "five-field static background");
    assert_eq!(
        items(&decoded, "qw-wall-", &state).len(),
        2,
        "walls live in Main and use mainWall fields"
    );
    assert_eq!(items(&decoded, "qw-paint-1-0", &state).len(), 1);
    state.play_timer_ms = Some(250);
    assert!(items(&decoded, "qw-paint-1-0", &state).is_empty());
    assert_eq!(items(&decoded, "qw-paint-1-1", &state).len(), 1);
    state.play_timer_ms = Some(50_000);
    assert_eq!(items(&decoded, "qw-paint-1-1", &state).len(), 1, "mode 2 holds its final frame");
    for gauge in [0.0, 20.0, 80.0, 100.0] {
        state.gauge = gauge;
        let gauge_items = items(&decoded, "qw-hitPoints-", &state);
        if gauge == 0.0 {
            assert!(gauge_items.is_empty());
            continue;
        }
        let SkinRenderItem::Image { rect, uv, .. } = &gauge_items[0] else { panic!("gauge") };
        assert!((rect.height * 720.0 - gauge * 2.0).abs() < 0.01);
        assert!((uv.height - gauge / 100.0).abs() < 0.001);
        assert!((rect.y * 720.0 - (300.0 - gauge * 2.0)).abs() < 0.01);
    }
}

#[test]
fn qwilight_judgment_repeats_and_lift_move_the_line_and_feedback() {
    let (_root, decoded) = fixture();
    assert!(!decoded.document.offset.iter().any(|o| o.id == 1005));
    let mut state = SkinDrawState {
        play_timer_ms: Some(2000),
        judge_ms: [Some(500), None, None],
        judge_index: [Some(1), None, None],
        judge_timing_ms: [Some(-25), None, None],
        combo: 3,
        ..Default::default()
    };
    state.bomb_ms[bmz_core::lane::Lane::Key1.index()] = Some(0);
    let mut lifted = state.clone();
    lifted.offset_lift_px = 80;
    for prefix in [
        "qw-lane-31-",
        "qw-lane-8-",
        "qw-hit-false-0",
        "qw-paint-2",
        "qw-judge",
        "qw-number-band",
        "qw-judgment-meter",
    ] {
        let a = items(&decoded, prefix, &state);
        let b = items(&decoded, prefix, &lifted);
        assert!(!a.is_empty(), "missing {prefix}");
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(&b) {
            if let (SkinRenderItem::Image { rect: a, .. }, SkinRenderItem::Image { rect: b, .. }) =
                (a, b)
            {
                assert!(((a.y - b.y) * 720.0 - 80.0).abs() < 0.01, "{prefix}: {a:?} -> {b:?}");
            }
        }
    }
    let mut at = state;
    at.judge_ms[0] = Some(3999);
    assert!(
        !items(&decoded, "qw-judge", &at).is_empty(),
        "2 frames * 48 repeats / 24 fps = 4 seconds"
    );
    at.judge_ms[0] = Some(4001);
    assert!(items(&decoded, "qw-judge", &at).is_empty());
}

#[test]
#[ignore = "requires installed CR_minimal and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_cr_minimal_feedback_gpu() {
    let root = PathBuf::from(std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set skin root"));
    let output =
        PathBuf::from(std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("set preview dir"));
    fs::create_dir_all(&output).unwrap();
    let options = BTreeMap::from([("プレイサイド".into(), "Center".into())]);
    let (mut renderer, mut snapshot) = prepare_installed_qwilight(
        &root,
        std::slice::from_ref(&root),
        "CR_minimal",
        KeyMode::K6,
        6,
        &options,
        &BTreeMap::new(),
        0.0,
    );
    snapshot.judge_counts.pgreat = 19;
    snapshot.judge_counts.great = 2;
    snapshot.judge_counts.empty_poor = 1;
    snapshot.fast_slow_counts.fast_great = 12;
    snapshot.fast_slow_counts.slow_good = 3;
    snapshot.recent_judgements[0].delta_us = -25_000;
    snapshot.recent_judgements[0].side = Some(bmz_core::judge::TimingSide::Fast);
    let base = render_qwilight_snapshot(&mut renderer, snapshot.clone());
    save_preview(&output, "CR_minimal-6k-feedback.png", &base);
    let mut slow = snapshot.clone();
    slow.recent_judgements[0].delta_us = 25_000;
    slow.recent_judgements[0].side = Some(bmz_core::judge::TimingSide::Slow);
    let slow = render_qwilight_snapshot(&mut renderer, slow);
    save_preview(&output, "CR_minimal-6k-slow.png", &slow);
    assert!(changed_pixels(&base, &slow, (660, 735), (325, 350)) > 100);
    snapshot.lift = 0.15;
    let lifted = render_qwilight_snapshot(&mut renderer, snapshot.clone());
    save_preview(&output, "CR_minimal-6k-lift.png", &lifted);
    assert!(changed_pixels(&base, &lifted, (430, 850), (300, 530)) > 2000);
    // Continue after gameplay has discarded its 800 ms recent-judgement list.
    // The renderer's existing latch must keep YELL until the skin's 4 s deadline.
    snapshot.lift = 0.0;
    snapshot.recent_judgements.clear();
    snapshot.time.0 += 1_000_000;
    snapshot.play_elapsed_time.0 += 1_000_000;
    let held = render_qwilight_snapshot(&mut renderer, snapshot.clone());
    save_preview(&output, "CR_minimal-6k-judge-1100ms.png", &held);
    snapshot.time.0 += 3_000_000;
    snapshot.play_elapsed_time.0 += 3_000_000;
    let expired = render_qwilight_snapshot(&mut renderer, snapshot);
    save_preview(&output, "CR_minimal-6k-judge-4100ms.png", &expired);
    assert!(
        changed_pixels(&held, &expired, (460, 640), (350, 400)) > 1000,
        "YELL must remain after 800 ms and disappear after 4 seconds"
    );
}

#[test]
#[ignore = "requires CR_minimal and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_cr_minimal_fades_gpu() {
    let root = PathBuf::from(std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set skin root"));
    let output =
        PathBuf::from(std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("set preview dir"));
    fs::create_dir_all(&output).unwrap();
    let (mut renderer, mut snapshot) = prepare_installed_qwilight(
        &root,
        std::slice::from_ref(&root),
        "CR_minimal",
        KeyMode::K6,
        6,
        &BTreeMap::new(),
        &BTreeMap::new(),
        0.0,
    );
    let mut render = |snapshot: RenderSnapshot, name: &str| {
        renderer.render_scene(AppSceneSnapshot::Play(snapshot)).unwrap();
        let rgba = renderer.read_offscreen_rgba().unwrap();
        save_preview(&output, name, &rgba);
        rgba.as_chunks::<4>()
            .0
            .iter()
            .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
            .sum::<u64>()
    };
    snapshot.play_elapsed_time = TimeUs(0);
    assert_eq!(render(snapshot.clone(), "CR_minimal-fade-entry-0ms.png"), 0);
    snapshot.play_elapsed_time = TimeUs(250_000);
    let half = render(snapshot.clone(), "CR_minimal-fade-entry-250ms.png");
    snapshot.play_elapsed_time = TimeUs(500_000);
    let clear = render(snapshot.clone(), "CR_minimal-fade-entry-500ms.png");
    assert!(half > 0 && clear > half);
    snapshot.fadeout_elapsed_ms = Some(500);
    assert_eq!(render(snapshot.clone(), "CR_minimal-fade-exit-500ms.png"), 0);
    snapshot.fadeout_elapsed_ms = None;
    snapshot.failed_elapsed_ms = Some(250);
    let failed_half = render(snapshot.clone(), "CR_minimal-fade-failed-250ms.png");
    assert!(failed_half > 0 && failed_half < clear);
    snapshot.failed_elapsed_ms = Some(500);
    assert_eq!(render(snapshot.clone(), "CR_minimal-fade-failed-500ms.png"), 0);
    snapshot.failed_elapsed_ms = None;
    snapshot.play_elapsed_time = TimeUs(0);
    snapshot.seamless_play_entry = true;
    assert!(render(snapshot, "CR_minimal-fade-seamless.png") > half);
}

#[test]
#[ignore = "requires CR_minimal and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_cr_minimal_information_gpu() {
    use bmz_core::input::{InputEvent, InputKind, InputSource};
    use bmz_gameplay::session::{SkinRuntimeEvent, SkinRuntimeEventKind};
    let root = PathBuf::from(std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set skin root"));
    let output =
        PathBuf::from(std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("set preview dir"));
    fs::create_dir_all(&output).unwrap();
    let options = BTreeMap::from([("プレイサイド".into(), "Center".into())]);
    let prepare = |options: &BTreeMap<String, String>| {
        prepare_installed_qwilight(
            &root,
            std::slice::from_ref(&root),
            "CR_minimal",
            KeyMode::K6,
            6,
            options,
            &BTreeMap::new(),
            0.0,
        )
    };
    let (mut renderer, mut snapshot) = prepare(&options);
    snapshot.title = "情報表示の確認".into();
    snapshot.subtitle = "[6K TEST]".into();
    snapshot.artist = "BMZ Player".into();
    snapshot.subartist = "PCM spectrum / timing".into();
    snapshot.genre = "#TEST SONG".into();
    snapshot.play_level = "18".into();
    snapshot.difficulty_name = "INSANE".into();
    snapshot.table_text_secondary = "6UE19".into();
    snapshot.table_text_fallback = "6UE19 表の名前".into();
    snapshot.skin_chart_length_ms = Some(158_000);
    snapshot.duration = TimeUs(158_000_000);
    snapshot.time = TimeUs(65_000_000);
    snapshot.play_elapsed_time = snapshot.time;
    render_qwilight_snapshot(&mut renderer, snapshot.clone());
    snapshot.time = TimeUs(66_000_000);
    snapshot.play_elapsed_time = snapshot.time;
    snapshot.recent_judgements[0].time = TimeUs(65_900_000);
    snapshot.recent_judgements[0].delta_us = -35_000;
    snapshot.skin_events = (0..9)
        .map(|i| SkinRuntimeEvent {
            sequence: i + 1,
            kind: SkinRuntimeEventKind::Input(InputEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(65_100_000 + i as i64 * 100_000),
                source: InputSource::Replay,
                device_kind: Default::default(),
                scratch_direction: None,
            }),
        })
        .collect();
    snapshot.play_note_graphs.judge = (0..158)
        .map(|i| bmz_render::snapshot::ResultJudgeGraphBucket {
            values: if i < 66 { [0, 8 + i % 8, 2, 1, 0, 0] } else { [12, 0, 0, 0, 0, 0] },
        })
        .collect::<Vec<_>>()
        .into();
    snapshot.play_note_graphs.judge_revision = 1;
    let mut audio = bmz_audio::engine::AudioEngine::new(48_000);
    audio.spectrum().set_enabled(true);
    let pcm = (0..4096)
        .map(|i| {
            [100.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0]
                .into_iter()
                .map(|hz| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin() * 0.06)
                .sum()
        })
        .collect();
    audio.insert_sample(
        bmz_core::ids::SoundId(1),
        bmz_audio::sample::DecodedSample { channels: 1, sample_rate: 48_000, frames: pcm },
    );
    audio.play_now(bmz_core::ids::SoundId(1), 1.0, false);
    audio.render_stereo(0, &mut vec![0.0; 8192]);
    snapshot.audio_spectrum = audio.spectrum().bands();
    assert!(snapshot.audio_spectrum.iter().any(|value| *value > 0.5));
    let full = render_qwilight_snapshot(&mut renderer, snapshot.clone());
    save_preview(&output, "CR_minimal-6k-information.png", &full);
    let mut hidden = options;
    for name in [
        "Title",
        "Artist",
        "Genre",
        "NPS",
        "Time",
        "Level",
        "Table",
        "Difficulty",
        "Timing",
        "Judgment graph",
        "Spectrum",
    ] {
        hidden.insert(format!("Qwilight show {name}"), "Off".into());
    }
    let (mut hidden_renderer, _) = prepare(&hidden);
    let disabled = render_qwilight_snapshot(&mut hidden_renderer, snapshot);
    save_preview(&output, "CR_minimal-6k-information-off.png", &disabled);
    assert!(changed_pixels(&full, &disabled, (0, 420), (0, 70)) > 1000, "song information");
    assert!(changed_pixels(&full, &disabled, (0, 420), (610, 720)) > 1000, "NPS, time, history");
    assert!(changed_pixels(&full, &disabled, (450, 820), (520, 720)) > 1000, "spectrum and level");
}
