use super::*;

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
        "Drawing/HP 2 0.png",
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
        let gauge_items = items(&decoded, "qw-gauge", &state);
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
