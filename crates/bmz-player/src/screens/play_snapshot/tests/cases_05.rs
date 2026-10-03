use super::*;

#[test]
fn snapshot_auto_note_lanes_exclude_full_autoplay_and_display_only_opponent() {
    use bmz_gameplay::autoplay::AutoplayController;
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    for full in [false, true] {
        session.autoplay = Some(if full {
            AutoplayController::default()
        } else {
            AutoplayController::for_lanes(&[Lane::Scratch, Lane::Key8])
        });
        session.display_only_lane_mask[Lane::Key8.index()] = true;
        let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
        assert_eq!(snapshot.auto_note_lanes[Lane::Scratch.index()], !full);
        assert!(!snapshot.auto_note_lanes[Lane::Key8.index()]);
        assert!(!snapshot.auto_note_lanes[Lane::Key1.index()]);
    }
}

#[test]
fn prepared_chart_populates_play_skin_data_without_marking_media_ready() {
    use bmz_chart::model::{TimingEvent, TimingEventKind};

    let mut chart = chart();
    chart.metadata.title = "Prepared title".to_string();
    chart.metadata.subtitle = "Prepared subtitle".to_string();
    chart.metadata.artist = "Prepared artist".to_string();
    chart.metadata.has_bga = true;
    chart.metadata.judge_rank = Some(75);
    chart.end_time = TimeUs(3_000_000);
    chart.timing_events.push(TimingEvent {
        tick: ChartTick(960),
        time: TimeUs(250_000),
        kind: TimingEventKind::Stop { duration_us: 125_000 },
    });
    let cache = PlayRenderSnapshotCache::from_chart(&chart);
    let mut snapshot = bmz_render::snapshot::RenderSnapshot {
        resources_loaded: false,
        resource_load_progress: 0.25,
        hispeed: 2.0,
        ..Default::default()
    };

    apply_prepared_chart_to_render_snapshot(&mut snapshot, &chart, &cache, false);

    assert_eq!(snapshot.title, "Prepared title");
    assert_eq!(snapshot.subtitle, "Prepared subtitle");
    assert_eq!(snapshot.artist, "Prepared artist");
    assert_eq!(snapshot.duration, TimeUs(3_000_000));
    assert_eq!(snapshot.total_notes, 1);
    assert_eq!(snapshot.chart_total_gauge, 160.0);
    assert_eq!(snapshot.now_bpm, 120.0);
    assert_eq!(snapshot.min_bpm, 120.0);
    assert_eq!(snapshot.max_bpm, 120.0);
    assert!(snapshot.has_bga);
    assert_eq!(snapshot.has_long_notes, Some(false));
    assert!(snapshot.has_bpm_stop);
    assert!(!snapshot.judge_graph_density.is_empty());
    assert!(!snapshot.bpm_graph_segments.is_empty());
    assert!(!snapshot.resources_loaded);
    assert_eq!(snapshot.resource_load_progress, 0.25);
}

#[test]
fn prepared_battle_chart_keeps_7k_as_the_skin_key_mode() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K14;
    chart.lane_notes[Lane::Key8.index()].push(tap_note(2, Lane::Key8, 0, 1_000_000));
    chart.total_notes = 1;
    let cache = PlayRenderSnapshotCache::from_chart(&chart);
    let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();

    apply_prepared_chart_to_render_snapshot(&mut snapshot, &chart, &cache, true);

    assert_eq!(snapshot.key_mode, KeyMode::K14);
    assert_eq!(snapshot.skin_attempt.effective_key_mode, Some(KeyMode::K7));
    assert_eq!(snapshot.total_notes, 1);
}

#[test]
fn refresh_play_skin_visuals_with_input_elapsed_tracks_short_pre_ready_keybeam() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(100_000));
    let mut snapshot = build_render_snapshot(&session, TimeUs(-100_000), &[], None);

    refresh_play_skin_visuals_with_input_elapsed(&mut snapshot, &session, TimeUs(150_000));

    assert_eq!(snapshot.keyon_ms[Lane::Key1.index()], Some(50));
}

#[test]
fn refresh_play_skin_visuals_hides_stale_pre_ready_keybeam_after_chart_start() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(3_500_000));
    session.lane_keyoff_started_at[Lane::Key2.index()] = Some(TimeUs(3_550_000));
    let mut snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
    snapshot.play_elapsed_time = TimeUs(4_000_000);

    refresh_play_skin_visuals(&mut snapshot, &session);

    assert_eq!(snapshot.keyon_ms[Lane::Key1.index()], None);
    assert_eq!(snapshot.keyoff_ms[Lane::Key2.index()], None);
}

#[test]
fn refresh_play_skin_visuals_keeps_playstart_keybeam_after_chart_start() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(-500_000));
    let mut snapshot = build_render_snapshot(&session, TimeUs(250_000), &[], None);
    snapshot.play_elapsed_time = TimeUs(4_000_000);

    refresh_play_skin_visuals(&mut snapshot, &session);

    assert_eq!(snapshot.keyon_ms[Lane::Key1.index()], Some(750));
}

#[test]
fn build_render_snapshot_selects_current_bga_frames() {
    use bmz_chart::model::{BgaArgbEvent, BgaAssetKind, BgaAssetRef, BgaEvent, BgaOpacityEvent};

    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut chart = chart();
    chart.metadata.has_bga = true;
    chart.bga_assets = vec![
        BgaAssetRef { id: BgaAssetId(0), path: "base-a.png".into(), kind: BgaAssetKind::Static },
        BgaAssetRef { id: BgaAssetId(1), path: "base-b.png".into(), kind: BgaAssetKind::Static },
        BgaAssetRef { id: BgaAssetId(2), path: "layer.png".into(), kind: BgaAssetKind::Static },
        BgaAssetRef { id: BgaAssetId(3), path: "poor.png".into(), kind: BgaAssetKind::Static },
    ];
    chart.bga_events = vec![
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            asset: Some(BgaAssetId(0)),
            kind: BgaEventKind::Base,
        },
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(500_000),
            asset: Some(BgaAssetId(1)),
            kind: BgaEventKind::Base,
        },
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(250_000),
            asset: Some(BgaAssetId(2)),
            kind: BgaEventKind::Layer,
        },
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(700_000),
            asset: None,
            kind: BgaEventKind::Layer,
        },
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(300_000),
            asset: Some(BgaAssetId(3)),
            kind: BgaEventKind::Poor,
        },
    ];
    chart.bga_opacity_events = vec![BgaOpacityEvent {
        tick: ChartTick(0),
        time: TimeUs(200_000),
        layer: BgaEventKind::Layer,
        opacity: 128,
    }];
    chart.bga_argb_events = vec![BgaArgbEvent {
        tick: ChartTick(0),
        time: TimeUs(200_000),
        layer: BgaEventKind::Layer,
        alpha: 255,
        red: 255,
        green: 32,
        blue: 16,
    }];
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.poor_bga_duration_us = 250_000;
    // レーン表示オフセットが BGA イベント選択へ漏れないことも同時に検証する。
    session.offsets.visual_offset_us = 500_000;
    let bga_frames = BgaFrameCatalog::from([
        (BgaAssetId(0), display_bga_frame(BgaAssetId(0), 256, 256)),
        (BgaAssetId(1), display_bga_frame(BgaAssetId(1), 640, 480)),
        (BgaAssetId(2), display_bga_frame(BgaAssetId(2), 1280, 720)),
        (BgaAssetId(3), display_bga_frame(BgaAssetId(3), 320, 240)),
    ]);
    let poor_judgements = [JudgementEvent {
        note_id: Some(NoteId(1)),
        lane: Lane::Key1,
        judge: Judge::Poor,
        side: TimingSide::Slow,
        delta: TimeUs(0),
        time: TimeUs(400_000),
        affects_score: true,
    }];

    let early =
        build_render_snapshot_with_bga_frames(&session, TimeUs(100_000), &[], None, &bga_frames);
    let late =
        build_render_snapshot_with_bga_frames(&session, TimeUs(600_000), &[], None, &bga_frames);
    let poor_active = build_render_snapshot_with_bga_frames(
        &session,
        TimeUs(600_000),
        &poor_judgements,
        None,
        &bga_frames,
    );
    let poor_expired = build_render_snapshot_with_bga_frames(
        &session,
        TimeUs(651_000),
        &poor_judgements,
        None,
        &bga_frames,
    );
    let layer_cleared =
        build_render_snapshot_with_bga_frames(&session, TimeUs(800_000), &[], None, &bga_frames);

    assert_eq!(early.bga_base.unwrap().texture_id, bga_texture_id(BgaAssetId(0)));
    assert!(early.bga_layer.is_none());
    assert_eq!(late.bga_base.unwrap(), display_bga_frame(BgaAssetId(1), 640, 480));
    let late_layer = late.bga_layer.unwrap();
    assert_eq!(late_layer.texture_id, bga_texture_id(BgaAssetId(2)));
    assert!((late_layer.tint_r - 1.0).abs() < 0.01);
    assert!((late_layer.tint_g - 32.0 / 255.0).abs() < 0.01);
    assert!((late_layer.tint_b - 16.0 / 255.0).abs() < 0.01);
    assert!((late_layer.tint_a - 128.0 / 255.0).abs() < 0.01);
    assert_eq!(poor_active.bga_poor.unwrap(), display_bga_frame(BgaAssetId(3), 320, 240));
    assert!(poor_expired.bga_poor.is_none());
    assert_eq!(layer_cleared.bga_base.unwrap(), display_bga_frame(BgaAssetId(1), 640, 480));
    assert!(layer_cleared.bga_layer.is_none());
}

#[test]
fn current_bpm_returns_initial_bpm_before_first_change() {
    let chart = chart_with_bpm_changes();
    // At time 0, before any BPM change
    assert_eq!(current_bpm(&chart, TimeUs(0)), 120.0);
}

#[test]
fn current_bpm_returns_changed_bpm_after_event() {
    let chart = chart_with_bpm_changes();
    // BPM changes to 180 at t=500_000 µs
    assert_eq!(current_bpm(&chart, TimeUs(500_000)), 180.0);
    // BPM changes to 90 at t=1_000_000 µs
    assert_eq!(current_bpm(&chart, TimeUs(1_000_000)), 90.0);
    // After last change
    assert_eq!(current_bpm(&chart, TimeUs(2_000_000)), 90.0);
}

#[test]
fn chart_min_bpm_returns_minimum_across_all_events() {
    let chart = chart_with_bpm_changes();
    // initial=120, events: 180, 90 → min=90
    assert_eq!(chart_min_bpm(&chart), 90.0);
}

#[test]
fn chart_max_bpm_returns_maximum_across_all_events() {
    let chart = chart_with_bpm_changes();
    // initial=120, events: 180, 90 → max=180
    assert_eq!(chart_max_bpm(&chart), 180.0);
}

#[test]
fn bpm_helpers_use_initial_bpm_when_no_timing_events() {
    let chart = chart(); // no timing_events
    assert_eq!(current_bpm(&chart, TimeUs(0)), 120.0);
    assert_eq!(chart_min_bpm(&chart), 120.0);
    assert_eq!(chart_max_bpm(&chart), 120.0);
}

#[test]
fn build_render_snapshot_emits_visible_long_note() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(
        Arc::new(chart_with_long_note()),
        &profile,
        PlaySessionOptions::default(),
    );
    session.hispeed = 1.0;

    // render_now=0: start 500ms→0.25, end 1500ms→0.75 (lookahead 2s)
    let upcoming = build_render_snapshot(&session, TimeUs(0), &[], None);
    assert_eq!(upcoming.has_long_notes, Some(true));
    assert_eq!(upcoming.visible_long_notes.len(), 1);
    assert_eq!(upcoming.visible_long_notes[0].lane, Lane::Key1);
    assert_eq!(upcoming.visible_long_notes[0].head_y, 0.25);
    assert_eq!(upcoming.visible_long_notes[0].tail_y, 0.75);
}

#[test]
fn build_render_snapshot_clamps_held_long_note_head_to_judge_line() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(
        Arc::new(chart_with_long_note()),
        &profile,
        PlaySessionOptions::default(),
    );
    session.hispeed = 1.0;

    // render_now=1_000_000: 始端は判定ライン通過済み(負値→0.0)、終端は 0.25
    let held = build_render_snapshot(&session, TimeUs(1_000_000), &[], None);
    assert_eq!(held.visible_long_notes.len(), 1);
    assert_eq!(held.visible_long_notes[0].head_y, 0.0);
    assert_eq!(held.visible_long_notes[0].tail_y, 0.25);

    // 終端も通過したら非表示
    let passed = build_render_snapshot(&session, TimeUs(2_000_000), &[], None);
    assert!(passed.visible_long_notes.is_empty());
}

#[test]
fn cached_snapshot_remains_correct_after_time_rewind() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(
        Arc::new(chart_with_long_note()),
        &profile,
        PlaySessionOptions::default(),
    );
    session.hispeed = 1.0;
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let frames = BgaFrameCatalog::new();

    let later = build_render_snapshot_with_target_and_bga_frames_cached(
        &session,
        TimeUs(1_000_000),
        &[],
        None,
        None,
        None,
        &frames,
        &cache,
    );
    let earlier = build_render_snapshot_with_target_and_bga_frames_cached(
        &session,
        TimeUs(0),
        &[],
        None,
        None,
        None,
        &frames,
        &cache,
    );

    assert_eq!(later.visible_long_notes.len(), 1);
    assert_eq!(later.visible_long_notes[0].head_y, 0.0);
    assert_eq!(later.visible_long_notes[0].tail_y, 0.25);
    assert_eq!(earlier.visible_long_notes.len(), 1);
    assert_eq!(earlier.visible_long_notes[0].head_y, 0.25);
    assert_eq!(earlier.visible_long_notes[0].tail_y, 0.75);
}
