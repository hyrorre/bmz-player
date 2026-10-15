//! BMS `#TEXT` / channel #99 state exposed to skins.
//!
//! Derived only from the chart clock, so PAUSE, seek, replay and autoplay all
//! read the same state for the same chart time. Lookups are binary searches.
use super::*;
use bmz_chart::text::chart_text_event_at_time;

/// Points `target` at the text reached by `chart_now` without copying the string.
pub(super) fn apply_chart_text(target: &mut Arc<str>, chart: &PlayableChart, chart_now: TimeUs) {
    match chart_text_event_at_time(&chart.text_events, chart_now) {
        Some(event) => {
            if !Arc::ptr_eq(target, &event.text) {
                *target = event.text.clone();
            }
        }
        None => {
            if !target.is_empty() {
                *target = Arc::from("");
            }
        }
    }
}
