use super::*;
use bmz_render::skin::{SkinAnimationDef, SkinDestinationDef, SkinDstEntry};
use bmz_skin::LuaSkinOffsetValue;

fn fixture() -> tempfile::TempDir {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("skin.yaml"), r#"
format: {zip: skin}
frame: {}
lambda:
  drawing1: 1, 2, 3, 4, 5, 10
  pipeline: 0, 1, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 28, 29, 30, 31, 33, 34, 35, 36, 37, 38, 39, 40, 41, 43, 44, 45, 46, 47, 48, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 100
  alt-multiplier: 2
  alt-hit-points: 0
  alt-status: 0
  alt-status-slider: 0
point:
  noteLength: 20
  noteHeight: 12
  p2Position: 30
  binLength: 10
  binHeight: 20
  mainPosition1: 0
  mainHeight: 500
  judgmentMainPosition: 500
  judgmentMainPosition1: 500
  judgmentMainHeight: 5
  inputPosition1: 300
  inputHeight: 200
  hitNotePaintPosition1: 0
  hitNotePaintLength: 40
  hitNotePaintHeight: 60
  mainJudgmentMeterPosition1: 400
  mainJudgmentMeterHeight: 12
  judgmentPaintPosition0: 300
  judgmentPaintPosition1: 300
  judgmentPaintLength: 100
  judgmentPaintHeight: 30
  bandPosition0: 400
  standPosition0: 200
  bpmPosition0: 200
  highestBandPosition0: 200
  earlyValuePosition0: 200
  lateValuePosition0: 200
  highestJudgmentValuePosition0: 200
  higherJudgmentValuePosition0: 200
  highJudgmentValuePosition0: 200
  lowJudgmentValuePosition0: 200
  lowerJudgmentValuePosition0: 200
  lowestJudgmentValuePosition0: 200
  highestJudgmentValuePosition1: 100
  higherJudgmentValuePosition1: 120
  highJudgmentValuePosition1: 140
  lowJudgmentValuePosition1: 160
  lowerJudgmentValuePosition1: 180
  lowestJudgmentValuePosition1: 200
  multiplierPosition0: 200
  multiplierPosition1: 40
  multiplierSystem: 1
  multiplierUnitDrawingLength: 10
  pointPosition0: 200
  audioMultiplierPosition0: 200
  judgmentVSVisualizerPosition0: 200
  judgmentMeterPosition0: 200
  judgmentMeterPosition1: 70
  hunterPosition0: 200
  hitPointsVisualizerPosition0: 200
  inputVisualizerPosition0: 200
  binInputVisualizerLength: 10
  binInputVisualizerHeight: 20
  hmsPosition0: 200
  binHmsLength: 10
  binHmsHeight: 20
  mediaPosition0: 20
  mediaPosition1: 40
  mediaLength: 100
  mediaHeight: 80
  audioVisualizerPosition0: 20
  audioVisualizerLength: 100
  audioVisualizerHeight: 100
  judgmentVisualizerPosition0: 20
  judgmentVisualizerLength: 100
  judgmentVisualizerHeight: 20
  judgmentInputVisualizerPosition0: 20
  judgmentInputVisualizerLength: 100
  judgmentInputVisualizerHeight: 100
  hitPointsPosition0: 600
  hitPointsLength: 20
  hitPointsHeight: 200
  statusPosition0: 650
  statusPosition1: 20
  statusLength: 20
  statusHeight: 200
  statusSliderPosition0: 650
  statusSliderPosition1: 20
  statusSliderLength: 20
  statusSliderHeight: 200
  statusSliderContentsLength: 20
  statusSliderContentsHeight: 10
  judgmentPointsPosition0: 700
  judgmentPointsLength: 20
  judgmentPointsHeight: 100
  titlePosition0: 400
  titleSystem0: 2
  titleLength: 200
  artistPosition0: 200
  genrePosition0: 200
  levelTextPosition0: 200
  wantLevelPosition0: 200
  levelPosition0: 200
  levelLength: 80
  levelHeight: 20
  autoMainPosition0: 100
  autoMainLength: 100
  autoMainHeight: 20
  band!Length: 80
  band!Height: 40
  paintProperty0: 0, 0, 1280, 720, 0, 1, 60
font: {titleLevel: 20}
"#).unwrap();
    let mut names = vec!["Note/N 1 0 0.png".into()];
    names.extend((0..10).map(|i| format!("Bin/{i}.png")));
    names.extend((0..6).map(|i| format!("Paint/J {i} 0.png")));
    names.extend((0..6).map(|i| format!("Drawing/J {i}.png")));
    for material in [1, 2, 3, 4, 5, 10] {
        names.extend([
            format!("Main/M {material} 0.png"),
            format!("Main/J {material} 0.png"),
            format!("Input/I {material} 0.png"),
            format!("Paint/N {material} 0.png"),
        ]);
    }
    names.extend(
        [
            "Main/M 0.png",
            "Main/J 0.png",
            "Input/I 0.png",
            "Paint/N 0.png",
            "Main/JM 0 -1.png",
            "Main/JM 0 1.png",
            "Drawing/HP 1.png",
            "Drawing/Status.png",
            "Drawing/Status Slider.png",
            "Drawing/AM.png",
            "Level/L 0 0.png",
            "Drawing/P 0 0.png",
            "Bin/..png",
            "Bin/X.png",
            "Bin/%.png",
            "Bin/MW.png",
        ]
        .map(str::to_string),
    );
    let mut zip = zip::ZipWriter::new(fs::File::create(root.path().join("skin.zip")).unwrap());
    for name in names {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&png_bytes(2, 2, [255; 4])).unwrap();
    }
    zip.finish().unwrap();
    root
}

fn decode(root: &Path, offsets: BTreeMap<String, LuaSkinOffsetValue>) -> DecodedSkin {
    decode_qwilight_with_offsets(&root.join("skin.yaml"), root, 14, BTreeMap::new(), offsets)
}

fn destination<'a>(decoded: &'a DecodedSkin, id: &str) -> &'a SkinDestinationDef {
    decoded
        .document
        .destination
        .iter()
        .find_map(|entry| match entry {
            DestinationListEntry::Single(d) if d.id == id => Some(d),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {id}"))
}

fn frame(d: &SkinDestinationDef, i: usize) -> &SkinAnimationDef {
    match &d.dst[i] {
        SkinDstEntry::Frame(f) => f,
        _ => panic!("not a frame"),
    }
}

#[test]
fn qwilight_element_offsets_bind_requested_controls_and_preserve_exclusions() {
    let root = fixture();
    let base = decode(root.path(), BTreeMap::new());
    let adjusted: BTreeMap<_, _> = base
        .document
        .offset
        .iter()
        .filter(|o| o.id >= 1100)
        .map(|o| {
            assert!(o.x && o.w && o.h && o.a && !o.r);
            assert_eq!(o.y, o.name != "Qwilight Judgment line offset");
            (o.name.clone(), LuaSkinOffsetValue { x: 30, y: 20, w: 40, h: 10, a: -64, r: 0 })
        })
        .collect();
    assert_eq!(adjusted.len(), 34);
    assert!(!adjusted.keys().any(|n| {
        ["Completion", "Auto", "Background", "Frame", "Decoration"].iter().any(|s| n.contains(s))
    }));
    let moved = decode(root.path(), adjusted);
    for id in [
        "qw-bga",
        "qw-audioVisualizer",
        "qw-hitPoints-1",
        "qw-number-band",
        "qw-judgment-meter",
        "qw-number-stand",
        "qw-number-point",
        "qw-number-bpm",
        "qw-number-multiplier",
        "qw-status-0",
        "qw-time-minutes",
        "qw-judgmentPoints-0",
        "qw-number-audioMultiplier",
        "qw-hitPointsVisualizer",
        "qw-number-highestJudgmentValue",
        "qw-number-lowestJudgmentValue",
        "qw-number-highestBand",
        "qw-judgmentVisualizer",
        "qw-nps",
        "qw-hunter",
        "qw-text-title",
        "qw-text-artist",
        "qw-text-genre",
        "qw-text-levelText",
        "qw-number-earlyValue",
        "qw-number-lateValue",
        "qw-number-judgmentVSVisualizer",
        "qw-text-wantLevel",
        "qw-level-0",
        "qw-judgmentInputVisualizer",
        "qw-status-thumb",
    ] {
        assert_ne!(frame(destination(&base, id), 0), frame(destination(&moved, id), 0), "{id}");
    }
    assert_ne!(base.document.judge[0].images, moved.document.judge[0].images);
    assert_eq!(base.document.note, moved.document.note);
    for id in ["qw-lane-8-0", "qw-hit-false-0", "qw-lane-timing-0"] {
        assert_ne!(frame(destination(&base, id), 0), frame(destination(&moved, id), 0), "{id}");
        assert_eq!(destination(&moved, id).offset, 3);
    }
    let line_before = frame(destination(&base, "qw-lane-31-0"), 0);
    let line_after = frame(destination(&moved, "qw-lane-31-0"), 0);
    assert_eq!(
        line_before.y.unwrap() + line_before.h.unwrap(),
        line_after.y.unwrap() + line_after.h.unwrap(),
        "line top remains at the same hit position even with saved y"
    );
    assert_eq!(
        destination(&base, "qw-lane-3-0"),
        destination(&moved, "qw-lane-3-0"),
        "lane background is excluded"
    );
    assert_eq!(destination(&base, "qw-auto-main"), destination(&moved, "qw-auto-main"));
    assert_eq!(
        base.document
            .destination
            .iter()
            .filter_map(|e| match e {
                DestinationListEntry::Single(d) => Some(&d.id),
                _ => None,
            })
            .collect::<Vec<_>>(),
        moved
            .document
            .destination
            .iter()
            .filter_map(|e| match e {
                DestinationListEntry::Single(d) => Some(&d.id),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
    let context = SkinPathContext::for_entry(&root.path().join("skin.yaml")).unwrap();
    let hidden = bmz_skin::load_qwilight_play_header_with_options(
        &context,
        &BTreeMap::from([("Qwilight show HS".into(), "Off".into())]),
    )
    .unwrap();
    assert_eq!(
        base.document.offset, hidden.offset,
        "hiding a display must retain its saved controls"
    );
}

#[test]
#[ignore = "requires installed Qwilight skins and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_installed_element_offsets_gpu() {
    let root = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set BMZ_QWILIGHT_SKIN_ROOT"),
    );
    let output = PathBuf::from(
        std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("set BMZ_QWILIGHT_PREVIEW_DIR"),
    );
    fs::create_dir_all(&output).unwrap();
    let offset = |x, y, w, h, a| LuaSkinOffsetValue { x, y, w, h, a, r: 0 };
    let adjusted = BTreeMap::from([
        ("Qwilight BGA offset".into(), offset(10, 0, -40, -40, -80)),
        ("Qwilight Title offset".into(), offset(0, 10, -100, 6, -20)),
        ("Qwilight Timing ms offset".into(), offset(0, 30, 30, 8, -30)),
        ("Qwilight Score offset".into(), offset(10, 10, 30, 8, 0)),
        ("Qwilight HS offset".into(), offset(10, 20, 30, 8, 0)),
        ("Qwilight Gauge offset".into(), offset(20, 0, 8, -60, -30)),
        ("Qwilight Judgment counts offset".into(), offset(20, 10, 10, -40, -30)),
        ("Qwilight Spectrum offset".into(), offset(0, 0, 0, -30, 200)),
        ("Qwilight Progress offset".into(), offset(-20, 0, 10, -40, 0)),
    ]);
    for (name, mode, number) in [("CR_minimal", KeyMode::K6, 6), ("Default", KeyMode::K14, 14)] {
        let base = render_installed_qwilight(
            &root,
            std::slice::from_ref(&root),
            name,
            mode,
            number,
            &BTreeMap::new(),
            &BTreeMap::new(),
            0.0,
        );
        let moved = render_installed_qwilight(
            &root,
            std::slice::from_ref(&root),
            name,
            mode,
            number,
            &BTreeMap::new(),
            &adjusted,
            0.0,
        );
        save_preview(&output, &format!("{name}-{number}k-elements-base.png"), &base);
        save_preview(&output, &format!("{name}-{number}k-elements-adjusted.png"), &moved);
        let changed = changed_pixels(&base, &moved, (0, 1280), (0, 720));
        assert!(changed > 1000, "{name}: expected visible offset changes, got {changed}");
        eprintln!("{name} {number}K element offsets: {changed} pixels changed");
    }
}

#[test]
fn qwilight_element_offsets_resize_compound_digits_without_scaling_dp_spacing() {
    let root = fixture();
    let base = decode(root.path(), BTreeMap::new());
    let moved = decode(
        root.path(),
        BTreeMap::from([(
            "Qwilight HS offset".into(),
            LuaSkinOffsetValue { x: 20, y: 10, w: 60, h: 20, a: -55, r: 0 },
        )]),
    );
    let render = |decoded: &DecodedSkin, hs| {
        let mut view = decoded.document.clone();
        view.destination.retain(|e| matches!(e, DestinationListEntry::Single(d) if d.id.starts_with("qw-number-multiplier")));
        let sources = decoded
            .sources
            .iter()
            .map(|s| {
                (
                    s.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: s.source_id.clone(),
                        texture: s.texture,
                        source_size: s.size,
                    },
                )
            })
            .collect();
        view.static_render_items(
            &sources,
            &SkinDrawState { hispeed: hs, ..Default::default() },
            &SkinTextState::default(),
        )
    };
    for hs in [2.34, 12.34, 123.45] {
        let a = render(&base, hs);
        let b = render(&moved, hs);
        let rects = |items: &[SkinRenderItem]| {
            items
                .iter()
                .filter_map(|i| match i {
                    SkinRenderItem::Image { rect, tint, .. } => Some((*rect, tint.a)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let a = rects(&a);
        let b = rects(&b);
        assert_eq!(a.len(), b.len());
        assert!(!a.is_empty());
        let per_side = a.len() / 2;
        for i in 0..per_side {
            let gap_a = a[i + per_side].0.x - a[i].0.x;
            let gap_b = b[i + per_side].0.x - b[i].0.x;
            assert!((gap_a - gap_b).abs() < 0.001, "DP copies must not spread apart");
            assert!((b[i].0.height * 720.0 - 40.0).abs() < 0.01);
            assert!((b[i].1 - 200.0 / 255.0).abs() < 0.001);
        }
        let bounds = |r: &[(bmz_render::plan::Rect, f32)]| {
            let l = r[..per_side].iter().map(|v| v.0.x).fold(f32::INFINITY, f32::min);
            let right =
                r[..per_side].iter().map(|v| v.0.x + v.0.width).fold(f32::NEG_INFINITY, f32::max);
            (l * 1280.0, right * 1280.0)
        };
        let (al, ar) = bounds(&a);
        let (bl, br) = bounds(&b);
        assert!(((br - bl) - (ar - al) - 60.0).abs() < 3.0, "whole display grows by 60 pixels");
        assert!(((bl + br - al - ar) / 2.0 - 20.0).abs() < 2.0);
    }
}

#[test]
fn qwilight_element_offsets_keep_slider_travel_fade_and_text_size_consistent() {
    let root = fixture();
    let base = decode(root.path(), BTreeMap::new());
    let moved = decode(
        root.path(),
        BTreeMap::from([
            (
                "Qwilight Progress offset".into(),
                LuaSkinOffsetValue { h: 100, ..Default::default() },
            ),
            (
                "Qwilight Timing ms offset".into(),
                LuaSkinOffsetValue { h: -10, a: -100, ..Default::default() },
            ),
            (
                "Qwilight Title offset".into(),
                LuaSkinOffsetValue { w: 100, h: -10, ..Default::default() },
            ),
            (
                "Qwilight Spectrum offset".into(),
                LuaSkinOffsetValue { a: 230, ..Default::default() },
            ),
        ]),
    );
    let slider = |d: &DecodedSkin| {
        d.document.slider.iter().find(|s| s.id == "qw-status-thumb").unwrap().range
    };
    assert_eq!(slider(&base), 200);
    assert_eq!(slider(&moved), 300);
    let bar = frame(destination(&moved, "qw-status-0"), 0);
    let thumb = frame(destination(&moved, "qw-status-thumb"), 0);
    assert_eq!(bar.h, Some(300));
    assert_eq!(thumb.h, Some(15));
    let meter = destination(&moved, "qw-judgment-meter");
    assert_eq!(frame(meter, 0).a, Some(155));
    assert_eq!(frame(meter, 1).a, Some(0));
    let title = moved.document.text.iter().find(|t| t.id == "qw-text-title").unwrap();
    assert_eq!(title.size, 10);
    assert_eq!(
        frame(destination(&moved, "qw-text-title"), 0).x,
        frame(destination(&base, "qw-text-title"), 0).x,
        "centered title retains anchor when resized"
    );
    assert_eq!(
        moved
            .document
            .qwilight_visualizer
            .iter()
            .find(|v| v.id == "qw-audioVisualizer")
            .unwrap()
            .opacity,
        Some(1.0)
    );
    for (decoded, expected) in [(&base, 0.1), (&moved, 1.0)] {
        let mut view = decoded.document.clone();
        view.destination
            .retain(|e| matches!(e,DestinationListEntry::Single(d) if d.id=="qw-audioVisualizer"));
        let items = view.static_render_items(
            &Default::default(),
            &SkinDrawState { audio_spectrum: [0.5; 32], ..Default::default() },
            &SkinTextState::default(),
        );
        let alphas: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                SkinRenderItem::Rect { color, .. } => Some(color.a),
                _ => None,
            })
            .collect();
        assert_eq!(alphas.len(), 32);
        assert!(alphas.iter().all(|a| (a - expected).abs() < 0.001));
    }
}
