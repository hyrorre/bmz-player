use bmz_core::time::TimeUs;

use crate::model::ChartTextEvent;

/// 指定時刻時点で表示すべき `#TEXT` 文字列を返す。
pub fn chart_text_at_time(events: &[ChartTextEvent], now: TimeUs) -> &str {
    let mut current = "";
    for event in events {
        if event.time <= now {
            current = event.text.as_str();
        } else {
            break;
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;
    use crate::model::ChartTextEvent;

    #[test]
    fn chart_text_at_time_steps_with_events() {
        let events = vec![
            ChartTextEvent {
                tick: ChartTick(0),
                time: TimeUs(1_000_000),
                text: "Hello".to_string(),
            },
            ChartTextEvent {
                tick: ChartTick(1),
                time: TimeUs(2_000_000),
                text: "World".to_string(),
            },
        ];
        assert_eq!(chart_text_at_time(&events, TimeUs(0)), "");
        assert_eq!(chart_text_at_time(&events, TimeUs(1_500_000)), "Hello");
        assert_eq!(chart_text_at_time(&events, TimeUs(3_000_000)), "World");
    }
}
