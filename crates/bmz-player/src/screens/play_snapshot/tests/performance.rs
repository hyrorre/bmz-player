use super::*;
use std::hint::black_box;
use std::time::Instant;

fn large_chart(count: u32) -> PlayableChart {
    let mut chart = chart();
    let timing = bmz_chart::timing::build_timing_map(120.0, Vec::new());
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    for index in 0..count {
        let lane = KeyMode::K7.active_lanes()[1 + index as usize % 7];
        let tick = u64::from(index) * 96;
        chart.lane_notes[lane.index()].push(tap_note(
            index + 1,
            lane,
            tick,
            timing.tick_to_time(ChartTick(tick)).0,
        ));
    }
    chart.total_notes = count;
    chart.end_time = timing.tick_to_time(ChartTick(u64::from(count) * 96));
    chart
}

fn measure(label: &str, samples: usize, mut run: impl FnMut()) {
    let mut times = Vec::with_capacity(samples);
    for iteration in 0..samples + 10 {
        let start = Instant::now();
        run();
        if iteration >= 10 {
            times.push(start.elapsed().as_secs_f64() * 1_000_000.0);
        }
    }
    times.sort_by(f64::total_cmp);
    eprintln!(
        "{label} median_us={:.3} p95_us={:.3} max_us={:.3}",
        times[samples / 2],
        times[samples * 95 / 100],
        times[samples - 1]
    );
}

#[test]
#[ignore = "release CPU probe; run alone with --ignored --nocapture"]
fn profile_positive_scroll_snapshot() -> anyhow::Result<()> {
    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    for count in [10_000, 50_000] {
        let mut chart = large_chart(count);
        chart.scroll_events = vec![bmz_chart::model::ScrollEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            factor: 1.0,
        }];
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
        let bga = BgaFrameCatalog::new();
        for fraction in [0.1, 0.9] {
            let now = TimeUs((session.chart.end_time.0 as f64 * fraction) as i64);
            measure(&format!("scroll notes={count} progress={fraction}"), 200, || {
                black_box(build_render_snapshot_with_target_and_bga_frames_cached(
                    &session,
                    now,
                    &[],
                    None,
                    None,
                    None,
                    &bga,
                    &cache,
                ));
            });
        }
    }
    Ok(())
}

#[test]
#[ignore = "release CPU probe; run alone with --ignored --nocapture"]
fn profile_judged_projection_copy() -> anyhow::Result<()> {
    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    for count in [10_000, 50_000] {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session = build_game_session(
            Arc::new(large_chart(count)),
            &profile,
            PlaySessionOptions::default(),
        );
        session.judge.judged_notes.extend((1..=count).map(|id| (NoteId(id), Judge::PGreat)));
        let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
        let mut pool = PlayfieldProjection::pool(&session, &cache);
        let projection = Arc::get_mut(&mut pool[0]).unwrap();
        measure(&format!("projection judged={count}"), 1000, || {
            projection.update(black_box(&session), TimeUs(0));
            black_box(&projection);
        });
    }
    Ok(())
}

#[test]
#[ignore = "release CPU probe; run alone with --ignored --nocapture"]
fn profile_bga_channel_selection() -> anyhow::Result<()> {
    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    use bmz_chart::model::{BgaAssetId, BgaEvent, BgaEventKind};
    for count in [10_000, 50_000] {
        let mut chart = large_chart(1);
        chart.bga_events = (0..count)
            .map(|index| BgaEvent {
                tick: ChartTick(index as u64),
                time: TimeUs(index * 1_000),
                kind: BgaEventKind::Base,
                asset: Some(BgaAssetId(1)),
            })
            .collect();
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        let observed = crate::gameplay_runtime::PlaySessionObservation::from_session(&session);
        let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
        let now = TimeUs(count * 100);
        measure(&format!("bga events={count} kinds=base"), 1000, || {
            black_box(crate::video_bga::active_bga_channels(&observed, now, &cache));
        });
    }
    Ok(())
}

#[test]
fn indexed_bga_channels_keep_clears_duplicate_order_and_poor_origin() {
    use bmz_chart::model::{BgaAssetId, BgaEvent, BgaEventKind};
    let mut chart = chart();
    chart.bga_events = [
        (100, BgaEventKind::Base, Some(1)),
        (100, BgaEventKind::Base, Some(2)),
        (100, BgaEventKind::Poor, Some(3)),
        (200, BgaEventKind::Base, None),
    ]
    .into_iter()
    .map(|(time, kind, asset)| BgaEvent {
        tick: ChartTick(time as u64),
        time: TimeUs(time),
        kind,
        asset: asset.map(BgaAssetId),
    })
    .collect();
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.poor_bga_duration_us = 100;
    session.recent_judgements.push(JudgementEvent {
        note_id: Some(NoteId(1)),
        lane: Lane::Key1,
        judge: Judge::Bad,
        side: TimingSide::Slow,
        delta: TimeUs(0),
        time: TimeUs(150),
        affects_score: true,
    });
    let observed = crate::gameplay_runtime::PlaySessionObservation::from_session(&session);
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let selected = |time| crate::video_bga::active_bga_channels(&observed, TimeUs(time), &cache);
    assert!(selected(99).is_empty());
    assert_eq!(selected(100)[0].asset_id, BgaAssetId(2));
    let poor = selected(200);
    assert_eq!(poor.len(), 1);
    assert_eq!(poor[0].kind, BgaEventKind::Poor);
    assert_eq!(poor[0].origin, TimeUs(150));
    assert!(selected(250).is_empty());
}
