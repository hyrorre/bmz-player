use bmz_core::time::TimeUs;

use crate::model::ChartTextEvent;

/// 指定時刻までに到達した最後の `#TEXT` イベントを返す。
///
/// `events` は時刻順 (同時刻は原文順) で並んでいる前提。二分探索なので、
/// シークや巻き戻しでも直前の状態を持ち越さない。
pub fn chart_text_event_at_time(events: &[ChartTextEvent], now: TimeUs) -> Option<&ChartTextEvent> {
    let reached = events.partition_point(|event| event.time <= now);
    reached.checked_sub(1).map(|index| &events[index])
}

/// 指定時刻時点で表示すべき `#TEXT` 文字列を返す。最初のイベント前は空文字。
pub fn chart_text_at_time(events: &[ChartTextEvent], now: TimeUs) -> &str {
    chart_text_event_at_time(events, now).map_or("", |event| &event.text)
}

#[cfg(test)]
mod tests {
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;
    use crate::model::ChartTextEvent;

    fn event(tick: u64, time_us: i64, text: &str) -> ChartTextEvent {
        ChartTextEvent { tick: ChartTick(tick), time: TimeUs(time_us), text: text.into() }
    }

    #[test]
    fn chart_text_at_time_steps_with_events() {
        let events = vec![event(0, 1_000_000, "Hello"), event(1, 2_000_000, "World")];
        assert_eq!(chart_text_at_time(&events, TimeUs(0)), "");
        assert_eq!(chart_text_at_time(&events, TimeUs(1_000_000)), "Hello");
        assert_eq!(chart_text_at_time(&events, TimeUs(1_500_000)), "Hello");
        assert_eq!(chart_text_at_time(&events, TimeUs(3_000_000)), "World");
        // Seeking backwards never keeps the later state.
        assert_eq!(chart_text_at_time(&events, TimeUs(999_999)), "");
    }

    #[test]
    fn empty_text_event_clears_and_same_time_events_use_the_last_one() {
        let events = vec![
            event(0, 1_000, "A"),
            event(1, 2_000, "B"),
            event(1, 2_000, "C"),
            event(2, 3_000, ""),
        ];
        assert_eq!(chart_text_at_time(&events, TimeUs(2_000)), "C");
        assert_eq!(
            chart_text_event_at_time(&events, TimeUs(2_000)).map(|e| e.tick),
            Some(ChartTick(1))
        );
        assert_eq!(chart_text_at_time(&events, TimeUs(3_000)), "");
        assert!(chart_text_event_at_time(&events, TimeUs(3_000)).is_some());
        assert!(chart_text_event_at_time(&[], TimeUs(3_000)).is_none());
    }
}
