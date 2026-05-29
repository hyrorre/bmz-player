use bmz_core::time::TimeUs;

use crate::model::ChartVolumeEvent;

/// BMS チャネル #97 / #98 の既定音量 (0xFF = 原音)。
pub const DEFAULT_CHART_CHANNEL_VOLUME: u8 = 255;

/// `#VOLWAV` の百分率 (100 = 原音) をゲイン係数へ変換する。
pub fn volwav_factor(percent: u8) -> f32 {
    percent as f32 / 100.0
}

/// チャネル音量 (0x01..=0xFF) を再生係数へ変換する。
pub fn chart_channel_volume_factor(value: u8) -> f32 {
    (value as f32 / 255.0).clamp(0.0, 1.0)
}

/// 指定時刻時点のチャネル音量 (1..=255) を返す。
pub fn chart_volume_at_time(events: &[ChartVolumeEvent], now: TimeUs) -> u8 {
    let mut value = DEFAULT_CHART_CHANNEL_VOLUME;
    for event in events {
        if event.time <= now {
            value = event.value;
        } else {
            break;
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;
    use crate::model::ChartVolumeEvent;

    #[test]
    fn volwav_factor_scales_percent() {
        assert_eq!(volwav_factor(100), 1.0);
        assert_eq!(volwav_factor(50), 0.5);
    }

    #[test]
    fn chart_channel_volume_factor_maps_ff_to_one() {
        assert_eq!(chart_channel_volume_factor(255), 1.0);
        assert!((chart_channel_volume_factor(128) - 128.0 / 255.0).abs() < 0.001);
    }

    #[test]
    fn chart_volume_at_time_steps_with_events() {
        let events = vec![
            ChartVolumeEvent { tick: ChartTick(0), time: TimeUs(1_000_000), value: 128 },
            ChartVolumeEvent { tick: ChartTick(1), time: TimeUs(2_000_000), value: 64 },
        ];
        assert_eq!(chart_volume_at_time(&events, TimeUs(0)), 255);
        assert_eq!(chart_volume_at_time(&events, TimeUs(1_500_000)), 128);
        assert_eq!(chart_volume_at_time(&events, TimeUs(3_000_000)), 64);
    }
}
