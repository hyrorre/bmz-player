//! BMS `#TEXT` / channel #99 state exposed to skins.
//!
//! Derived only from the chart clock, so PAUSE, seek, replay and autoplay all
//! read the same state for the same chart time. Lookups are binary searches.
use super::*;
use bmz_chart::text::chart_text_event_at_time;

/// Sets the text reached by `chart_now` (text 20100) and the elapsed time since
/// that event (timer 20102). The text is shared with the chart, not copied.
pub(super) fn apply_chart_text(
    snapshot: &mut RenderSnapshot,
    chart: &PlayableChart,
    chart_now: TimeUs,
) {
    let event = chart_text_event_at_time(&chart.text_events, chart_now);
    match event {
        Some(event) => {
            if !Arc::ptr_eq(&snapshot.chart_text, &event.text) {
                snapshot.chart_text = event.text.clone();
            }
        }
        None => {
            if !snapshot.chart_text.is_empty() {
                snapshot.chart_text = Arc::from("");
            }
        }
    }
    snapshot.chart_text_event_elapsed_ms = event.map(|event| elapsed_ms(chart_now, event.time));
}

fn elapsed_ms(now: TimeUs, since: TimeUs) -> i32 {
    (now.0.saturating_sub(since.0) / 1_000).clamp(0, i64::from(i32::MAX)) as i32
}
