use super::*;
use bmz_render::snapshot::{BpmChangePreviewSnapshot, UpcomingBpmChange};

/// Lane-clock time where BPM × SCROLL changes by at least one. STOP is zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BpmChangePoint {
    pub(super) time: TimeUs,
    pub(super) from_bpm: f64,
    pub(super) to_bpm: f64,
}

#[derive(Debug, Clone, Copy)]
enum PreviewEvent {
    Bpm(f64),
    Scroll(f64),
    StopStart,
    StopEnd,
}

/// Merge changes on the same lane clock used by rendering. Timing-map gaps
/// between STOP markers and following segments are the actual stopped intervals.
pub(super) fn build_bpm_change_points(chart: &PlayableChart) -> Vec<BpmChangePoint> {
    let timing_map =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    let mut events = Vec::new();
    for segment in &timing_map.segments {
        events.push((segment.start_time, PreviewEvent::Bpm(segment.bpm)));
    }
    for pair in timing_map.segments.windows(2) {
        let start = pair[0].end_time;
        let end = pair[1].start_time;
        if start < end {
            events.push((start, PreviewEvent::StopStart));
            events.push((end, PreviewEvent::StopEnd));
        }
    }
    for event in &chart.scroll_events {
        // SCROLL switches when the rendering cursor reaches its public tick.
        // This also handles legacy charts whose event.time was left at zero.
        events.push((timing_map.tick_to_time(event.tick), PreviewEvent::Scroll(event.factor)));
    }
    events.sort_by_key(|(time, _)| *time);

    let mut points = Vec::new();
    let mut bpm = timing_map.initial_bpm.max(1.0);
    let mut scroll = 1.0;
    let mut stops = 0usize;
    let effective_bpm = |bpm: f64, scroll: f64, stops: usize| {
        if stops > 0 { 0.0 } else { bpm * scroll }
    };
    let mut index = 0;
    while let Some(&(time, _)) = events.get(index) {
        let from_bpm = effective_bpm(bpm, scroll, stops);
        // Compare the state before and after all simultaneous events, including
        // adjacent STOP end/start pairs and BPM/SCROLL changes that cancel out.
        while let Some(&(event_time, event)) = events.get(index) {
            if event_time != time {
                break;
            }
            match event {
                PreviewEvent::Bpm(value) => bpm = value,
                PreviewEvent::Scroll(value) => scroll = value,
                PreviewEvent::StopStart => stops = stops.saturating_add(1),
                PreviewEvent::StopEnd => stops = stops.saturating_sub(1),
            }
            index += 1;
        }
        let to_bpm = effective_bpm(bpm, scroll, stops);
        // Preview snapshots use f32. Never publish an unrepresentable value,
        // and compare before rounding the numbers shown by skins.
        if (from_bpm as f32).is_finite()
            && (to_bpm as f32).is_finite()
            && (to_bpm - from_bpm).abs() >= 1.0
        {
            points.push(BpmChangePoint { time, from_bpm, to_bpm });
        }
    }
    points
}

/// `lane_now` is the visual-offset lane clock used for `now_bpm`. Durations are
/// converted to real time with the playback rate.
pub(super) fn bpm_change_preview(
    points: &[BpmChangePoint],
    lane_now: TimeUs,
    playback_rate_percent: u16,
) -> BpmChangePreviewSnapshot {
    let rate = i64::from(bmz_audio::clock::clamp_playback_rate_percent(playback_rate_percent));
    let real_ms = |chart_us: i64| {
        (chart_us.saturating_mul(100) / rate / 1_000).clamp(0, i64::from(i32::MAX)) as i32
    };
    let index = points.partition_point(|point| point.time <= lane_now);
    BpmChangePreviewSnapshot {
        next: points.get(index).map(|point| UpcomingBpmChange {
            remaining_ms: real_ms(point.time.0.saturating_sub(lane_now.0)),
            from_bpm: point.from_bpm as f32,
            to_bpm: point.to_bpm as f32,
        }),
        since_last_change_ms: index
            .checked_sub(1)
            .map(|last| real_ms(lane_now.0.saturating_sub(points[last].time.0))),
    }
}
