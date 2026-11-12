use super::*;

#[test]
#[ignore = "requires CR_minimal and GPU; set BMZ_QWILIGHT_SKIN_ROOT and BMZ_QWILIGHT_PREVIEW_DIR"]
fn qwilight_cr_minimal_completion_gpu() {
    let root = PathBuf::from(std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("skin root"));
    let output =
        PathBuf::from(std::env::var_os("BMZ_QWILIGHT_PREVIEW_DIR").expect("preview directory"));
    fs::create_dir_all(&output).unwrap();
    let started = std::time::Instant::now();
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
    eprintln!("completion decode/install: {:?}", started.elapsed());
    snapshot.total_notes = 1000;
    snapshot.past_notes = 1000;
    snapshot.combo = 1000;
    snapshot.judge_counts.pgreat = 999;
    snapshot.judge_counts.great = 1;
    snapshot.visible_notes = Default::default();
    snapshot.visible_long_notes.clear();
    snapshot.recent_judgements.clear();
    let baseline = render_qwilight_snapshot(&mut renderer, snapshot.clone());
    let mut previous = baseline.clone();
    for (name, perfect) in [("full-combo", false), ("perfect", true)] {
        snapshot.judge_counts.pgreat = if perfect { 1000 } else { 999 };
        snapshot.judge_counts.great = u32::from(!perfect);
        for elapsed in [0, 500, 1500, 3000, 4500] {
            snapshot.full_combo_elapsed_ms = Some(elapsed);
            let now = std::time::Instant::now();
            let pixels = render_qwilight_snapshot(&mut renderer, snapshot.clone());
            eprintln!("{name} {elapsed}: {:?}", now.elapsed());
            if elapsed == 1500 {
                save_preview(&output, &format!("CR_minimal-{name}.png"), &pixels);
                assert!(
                    changed_pixels(&baseline, &pixels, (0, 1280), (0, 530)) > 500,
                    "missing {name}"
                );
            }
            if elapsed == 500 {
                assert_ne!(pixels, previous);
            }
            previous = pixels;
        }
    }
    snapshot.full_combo_elapsed_ms = None;
    snapshot.combo = 0;
    snapshot.music_end_elapsed_ms = Some(1500);
    snapshot.gauge = 100.0;
    let cleared = render_qwilight_snapshot(&mut renderer, snapshot.clone());
    save_preview(&output, "CR_minimal-clear.png", &cleared);
    assert!(changed_pixels(&baseline, &cleared, (0, 1280), (0, 530)) > 500);
    snapshot.music_end_elapsed_ms = Some(6000);
    let after = render_qwilight_snapshot(&mut renderer, snapshot);
    assert_ne!(cleared, after);
}

fn expanded_fixture() -> (tempfile::TempDir, DecodedSkin) {
    use std::io::Write;
    let (root, _) = fixture();
    let path = root.path().join("skin.yaml");
    let mut yaml=fs::read_to_string(&path).unwrap()
        .replacen("frame:\n","frame:\n  band!-frame: 300\n  last-frame: 300\n  yell!-frame: 300\n  band-frame: 3\n  band-framerate: 10\n  main-frame: 2\n  main-framerate: 10\n  judgment-main-frame: 2\n  judgment-main-framerate: 10\n  main-judgment-meter-frame: 50\n",1)
        .replace("pipeline: ","pipeline: 4, 28, 30, 33, 34, 46, 53, 57, 58, ");
    yaml.push_str(
        r#"
  band!Position0: 100
  band!Position1: 100
  band!Length: 200
  band!Height: 300
  lastPosition0: 100
  lastPosition1: 100
  lastLength: 200
  lastHeight: 300
  yell!Position0: 100
  yell!Position1: 100
  yell!Length: 200
  yell!Height: 300
  enlargeBand: 6
  hitPointsVisualizerPosition0: 350
  hitPointsVisualizerPosition1: 120
  hunterPosition0: 450
  hunterPosition1: 120
  judgmentPointsPosition0: 500
  judgmentPointsPosition1: 150
  judgmentPointsLength: 100
  judgmentPointsHeight: 10
  judgmentPointsSystem: 2
  statusPosition0: 500
  statusPosition1: 170
  statusLength: 100
  statusHeight: 10
  statusSystem: 3
  statusSliderPosition0: 500
  statusSliderPosition1: 170
  statusSliderLength: 100
  statusSliderHeight: 10
  statusSliderSystem: 3
  statusSliderContentsLength: 10
  statusSliderContentsHeight: 10
  audioMultiplierPosition0: 650
  audioMultiplierPosition1: 120
  autoMainPosition0: 500
  autoMainPosition1: 250
  autoMainLength: 100
  autoMainHeight: 20
  mainJudgmentMeterPosition1: 450
  mainJudgmentMeterHeight: 20
"#,
    );
    fs::write(&path, yaml).unwrap();
    let mut names: Vec<String> = [
        "Drawing/Status",
        "Drawing/Status Slider",
        "Drawing/AM 0",
        "Bin/HP %",
        "Bin/HU +",
        "Bin/HU -",
        "Bin/AW",
        "Bin/A .",
        "Main/M 1 1",
        "Main/M 1 2",
        "Main/J 1 1",
        "Main/J 1 2",
        "Main/JM 1 0",
    ]
    .map(str::to_owned)
    .into();
    for category in 9..=11 {
        for frame in [0, 150] {
            names.push(format!("Paint/J {category} {frame}"));
        }
    }
    for i in 0..6 {
        names.push(format!("Drawing/J {i}"));
    }
    for frame in 0..3 {
        for digit in 0..10 {
            names.push(format!("Bin/C {digit} {frame}"));
        }
    }
    let file =
        fs::OpenOptions::new().read(true).write(true).open(root.path().join("skin.zip")).unwrap();
    let mut zip = zip::ZipWriter::new_append(file).unwrap();
    for (i, name) in names.iter().enumerate() {
        zip.start_file(format!("{name}.png"), zip::write::SimpleFileOptions::default()).unwrap();
        let height = if name.starts_with("Main/JM") { 200 } else { 2 };
        zip.write_all(&png_bytes(2, height, [i as u8, 80, 200, 128])).unwrap();
    }
    zip.finish().unwrap();
    let decoded = decode_qwilight(&path, root.path(), 6, BTreeMap::new());
    (root, decoded)
}

#[test]
fn qwilight_signed_lane_frames_fit_gpu_limits_even_with_a_prime_frame_count() {
    let (_root, decoded) = expanded_fixture();
    let meter = decoded.document.image.iter().find(|image| image.id == "qw-lane-timing-0").unwrap();
    let source = decoded
        .sources
        .iter()
        .find(|source| source.source_id == meter.src)
        .expect("101 signed frames of height 200 must not become a 20200px atlas");
    assert_eq!(source.size.height, 2000.0);
    assert_eq!(source.size.width, 22.0);
    assert_eq!(items(&decoded, "qw-lane-timing-0", &SkinDrawState::default()).len(), 1);
}

#[test]
fn qwilight_lane_timing_follows_fast_slow_mode_and_threshold_in_play_and_replay() {
    use crate::config::profile_config::FastSlowDisplayScope::{Auto, ThresholdMs};
    use crate::screens::play_snapshot::apply_fast_slow_display_filter;
    use bmz_core::judge::TimingSide;
    use bmz_gameplay::judge::model::JudgementEvent;
    use bmz_gameplay::session::{SkinRuntimeEvent, SkinRuntimeEventKind};

    let (_root, mut decoded) = expanded_fixture();
    let meter = decoded.document.image.iter().find(|image| image.id == "qw-lane-timing-0").unwrap();
    assert_eq!(meter.qwilight_animation.as_ref().unwrap().lane, 1);
    let texture =
        decoded.sources.iter().find(|source| source.source_id == meter.src).unwrap().texture;
    let (columns, rows) = (meter.divx, meter.divy);
    decoded.document.destination.retain(
        |entry| matches!(entry, DestinationListEntry::Single(d) if d.id == "qw-lane-timing-0"),
    );
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, SkinManifest::default()).unwrap();
    // Frames 0 / 50 / 100 are FAST / neutral / SLOW in this signed atlas.
    let cases = [
        (Auto, 0, Judge::PGreat, -20_000, 50),
        (Auto, 5, Judge::PGreat, 20_000, 50),
        (ThresholdMs, 5, Judge::PGreat, -4_999, 50),
        (ThresholdMs, 5, Judge::PGreat, 4_999, 50),
        (ThresholdMs, 5, Judge::PGreat, -5_000, 0),
        (ThresholdMs, 5, Judge::PGreat, 5_000, 100),
        (ThresholdMs, 5, Judge::PGreat, -5_001, 0),
        (ThresholdMs, 5, Judge::PGreat, 5_001, 100),
        (ThresholdMs, 0, Judge::PGreat, -1, 0),
        (ThresholdMs, 0, Judge::PGreat, 1, 100),
        (ThresholdMs, 0, Judge::PGreat, 0, 50),
        (Auto, 5, Judge::Great, -1_000, 0),
        (Auto, 5, Judge::Good, 1_000, 100),
        (Auto, 5, Judge::Bad, -1_000, 0),
        (ThresholdMs, 5, Judge::Great, 1_000, 100),
        (ThresholdMs, 5, Judge::Good, -1_000, 0),
        (ThresholdMs, 5, Judge::Bad, 1_000, 100),
        (ThresholdMs, 0, Judge::Poor, -10_000, 50),
        (ThresholdMs, 0, Judge::EmptyPoor, 10_000, 50),
    ];
    for replay_playback in [false, true] {
        for (scope, threshold, judge, delta_us, frame) in cases {
            // Each case begins a new attempt, with no preceding lane effect.
            renderer.prepare_scene(AppSceneSnapshot::Play(RenderSnapshot::default()));
            let time = TimeUs(1_000_000);
            let side = if delta_us < 0 { TimingSide::Fast } else { TimingSide::Slow };
            let mut snapshot = RenderSnapshot {
                time,
                play_elapsed_time: time,
                key_mode: KeyMode::K6,
                replay_playback,
                recent_judgements: vec![DisplayJudgement {
                    lane: Lane::Key1,
                    judge,
                    side: Some(side),
                    delta_us,
                    time,
                    text: String::new(),
                    combo: 1,
                    is_miss: judge == Judge::Poor,
                    timing_ms_suppressed: false,
                }],
                // Raw events must not bypass the display filter.
                skin_events: vec![SkinRuntimeEvent {
                    sequence: 1,
                    kind: SkinRuntimeEventKind::Judgement(JudgementEvent {
                        note_id: None,
                        lane: Lane::Key1,
                        judge,
                        side,
                        delta: TimeUs(delta_us),
                        time,
                        affects_score: true,
                    }),
                }],
                ..Default::default()
            };
            apply_fast_slow_display_filter(&mut snapshot, threshold, scope);
            renderer.prepare_scene(AppSceneSnapshot::Play(snapshot));
            let uv = renderer
                .last_plan()
                .unwrap()
                .commands
                .iter()
                .find_map(|command| match command {
                    DrawCommand::Image { texture: id, uv, .. } if id.0 == texture.0 => Some(*uv),
                    _ => None,
                })
                .expect("lane timing image");
            let actual_frame = (uv.y * rows as f32).round() as i32 * columns
                + (uv.x * columns as f32).round() as i32;
            assert_eq!(
                actual_frame, frame,
                "{scope:?} {threshold}ms {judge:?} {delta_us}us replay={replay_playback}"
            );
        }
    }
}

#[test]
fn qwilight_completion_uses_sparse_lossless_frames_and_bmz_outcomes() {
    let (_root, decoded) = expanded_fixture();
    assert_eq!(decoded.document.deferred_sources.len(), 6);
    for source in decoded.sources.iter().filter(|source| source.deferred.is_some()) {
        assert!(source.asset.is_none());
        assert_eq!(source.deferred.as_ref().unwrap().decode().unwrap().pixels[3], 128);
    }
    let mut state = SkinDrawState {
        total_notes: 10,
        full_combo_ms: Some(0),
        gauge: 100.0,
        ..Default::default()
    };
    state.judge_counts.pgreat = 8;
    let first = items(&decoded, "qw-completion-9", &state);
    assert_eq!(first.len(), 1);
    assert!(items(&decoded, "qw-completion-11", &state).is_empty());
    state.full_combo_ms = Some(2499);
    assert_eq!(first, items(&decoded, "qw-completion-9", &state));
    state.full_combo_ms = Some(2500);
    assert_ne!(first, items(&decoded, "qw-completion-9", &state));
    state.judge_counts.pgreat = 10;
    assert!(items(&decoded, "qw-completion-9", &state).is_empty());
    assert_eq!(items(&decoded, "qw-completion-11", &state).len(), 1);
    state.full_combo_ms = Some(5001);
    assert!(items(&decoded, "qw-completion-", &state).is_empty());
    state.full_combo_ms = None;
    state.music_end_ms = Some(0);
    assert_eq!(items(&decoded, "qw-completion-10", &state).len(), 1);
    state.gauge = 0.0;
    state.gauge_border = 80.0;
    assert!(items(&decoded, "qw-completion-10", &state).is_empty());
}

#[test]
fn qwilight_additional_hud_tracks_live_values_and_progress() {
    let (_root, decoded) = expanded_fixture();
    let mut state = SkinDrawState {
        gauge: 75.0,
        ex_score: 50,
        total_notes: 100,
        past_notes: 50,
        target_ex_score: Some(80),
        freq_semitones: 12,
        play_progress: 0.25,
        ..Default::default()
    };
    state.judge_counts.pgreat = 3;
    state.judge_counts.great = 1;
    assert_eq!(digits(&decoded, "qw-hitPointsVisualizer", &state), "75");
    assert_eq!(digits(&decoded, "qw-hunter", &state), "10");
    assert_eq!(digits(&decoded, "qw-number-audioMultiplier", &state), "200");
    let bars = items(&decoded, "qw-judgmentPoints-", &state);
    assert_eq!(bars.len(), 2);
    let SkinRenderItem::Image { rect, .. } = bars[0] else { panic!("ratio") };
    assert!((rect.width * 1280.0 - 75.0).abs() < 0.01);
    let first = items(&decoded, "qw-status-thumb", &state);
    state.play_progress = 0.75;
    assert_ne!(first, items(&decoded, "qw-status-thumb", &state));
    assert!(items(&decoded, "qw-auto-main", &state).is_empty());
    state.autoplay = true;
    assert_eq!(items(&decoded, "qw-auto-main", &state).len(), 1);
}

#[test]
fn qwilight_key_release_beat_and_combo_animations_follow_timers() {
    let (_root, decoded) = expanded_fixture();
    let mut state = SkinDrawState {
        combo: 3,
        judge_ms: [Some(0), None, None],
        quarter_note_elapsed_ms: Some(0),
        ..Default::default()
    };
    let mut runtime = bmz_render::skin::DynamicTimerRuntime::default();
    runtime.advance(&decoded.document, &mut state, 0);
    let idle = items(&decoded, "qw-lane-3-0", &state);
    state.keyon_ms[Lane::Key1.index()] = Some(0);
    runtime.advance(&decoded.document, &mut state, 0);
    runtime.advance(&decoded.document, &mut state, 100);
    let middle = items(&decoded, "qw-lane-3-0", &state);
    assert_ne!(idle, middle);
    runtime.advance(&decoded.document, &mut state, 200);
    assert_ne!(middle, items(&decoded, "qw-lane-3-0", &state));
    state.keyon_ms[Lane::Key1.index()] = None;
    runtime.advance(&decoded.document, &mut state, 300);
    assert_eq!(middle, items(&decoded, "qw-lane-3-0", &state));
    runtime.advance(&decoded.document, &mut state, 400);
    assert_eq!(idle, items(&decoded, "qw-lane-3-0", &state));
    let pulse = items(&decoded, "qw-lane-31-0", &state);
    let combo = items(&decoded, "qw-number-band", &state);
    state.quarter_note_elapsed_ms = Some(300);
    state.judge_ms[0] = Some(300);
    assert_ne!(pulse, items(&decoded, "qw-lane-31-0", &state));
    assert_ne!(combo, items(&decoded, "qw-number-band", &state));
}
