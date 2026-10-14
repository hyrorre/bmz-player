use super::*;
use bmz_render::snapshot::{BpmChangePreviewSnapshot, UpcomingBpmChange};

/// Lane-clock time where the effective BPM changes. STOP markers and repeated
/// BPM values are excluded so skins only see changes in scroll speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BpmChangePoint {
    pub(super) time: TimeUs,
    pub(super) from_bpm: f64,
    pub(super) to_bpm: f64,
}

/// Uses the same timing map construction as `GameSession`, so the change time
/// matches the moment `now_bpm` switches.
pub(super) fn build_bpm_change_points(chart: &PlayableChart) -> Vec<BpmChangePoint> {
    let timing_map =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    let mut points: Vec<BpmChangePoint> = Vec::new();
    let mut current_bpm = timing_map.initial_bpm.max(1.0);
    for segment in &timing_map.segments {
        if segment.bpm == current_bpm {
            continue;
        }
        match points.last_mut() {
            // Zero-length segments can stack several values on one instant.
            Some(last) if last.time == segment.start_time => {
                last.to_bpm = segment.bpm;
                if last.to_bpm == last.from_bpm {
                    points.pop();
                }
            }
            _ => points.push(BpmChangePoint {
                time: segment.start_time,
                from_bpm: current_bpm,
                to_bpm: segment.bpm,
            }),
        }
        current_bpm = segment.bpm;
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
