//! BMS `#TEXT` / channel #99 state exposed to skins.
//!
//! Derived only from the chart clock and the latest miss judgement, so PAUSE,
//! seek, replay and autoplay all read the same state for the same chart time.
//! Lookups are binary searches and strings are shared with the chart.
use super::*;
use bmz_chart::text::chart_text_event_at_time;

/// Session inputs for the `#TEXT00` miss message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ChartMissTextClock {
    /// Chart time of the latest POOR / empty POOR.
    pub(super) last_miss: Option<TimeUs>,
    /// Profile misslayer duration. `0` never shows the miss message.
    pub(super) duration_us: i64,
}

impl ChartMissTextClock {
    pub(super) fn from_session(session: &GameSession) -> Self {
        Self {
            last_miss: session.last_miss_judgement_time,
            duration_us: session.poor_bga_duration_us,
        }
    }

    /// Elapsed time while the miss message is shown at `now`.
    fn elapsed_us(self, now: TimeUs) -> Option<i64> {
        let elapsed = now.0.checked_sub(self.last_miss?.0)?;
        (0..self.duration_us).contains(&elapsed).then_some(elapsed)
    }
}

/// Sets text 20100 / 20103, timer 20102 / 20105 and the miss state at `chart_now`.
///
/// Timer 20102 always follows channel #99; a miss replaces only the displayed
/// text, and the current channel #99 text returns when the miss display ends.
pub(super) fn apply_chart_text(
    snapshot: &mut RenderSnapshot,
    chart: &PlayableChart,
    chart_now: TimeUs,
    miss: ChartMissTextClock,
) {
    let event = chart_text_event_at_time(&chart.text_events, chart_now);
    snapshot.chart_text_event_elapsed_ms =
        event.map(|event| elapsed_ms(chart_now.0 - event.time.0));
    let miss_text = chart.metadata.miss_text.as_ref();
    set_shared_text(&mut snapshot.chart_miss_text, miss_text);
    let miss_elapsed = miss_text.and(miss.elapsed_us(chart_now));
    snapshot.chart_miss_text_elapsed_ms = miss_elapsed.map(elapsed_ms);
    let displayed = if miss_elapsed.is_some() { miss_text } else { event.map(|event| &event.text) };
    set_shared_text(&mut snapshot.chart_text, displayed);
}

fn set_shared_text(target: &mut Arc<str>, value: Option<&Arc<str>>) {
    match value {
        Some(value) => {
            if !Arc::ptr_eq(target, value) {
                *target = value.clone();
            }
        }
        None => {
            if !target.is_empty() {
                *target = Arc::from("");
            }
        }
    }
}

fn elapsed_ms(elapsed_us: i64) -> i32 {
    (elapsed_us / 1_000).clamp(0, i64::from(i32::MAX)) as i32
}
