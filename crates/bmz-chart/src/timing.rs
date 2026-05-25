use bmz_core::time::{ChartTick, TimeUs};

pub const TICKS_PER_BEAT: u32 = 960;
pub const BEATS_PER_MEASURE: u32 = 4;
pub const TICKS_PER_MEASURE: u32 = TICKS_PER_BEAT * BEATS_PER_MEASURE;

#[derive(Debug, Clone)]
pub struct TimingMap {
    pub initial_bpm: f64,
    pub segments: Vec<TimingSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingSegment {
    pub start_tick: ChartTick,
    pub end_tick: ChartTick,
    pub start_time: TimeUs,
    pub end_time: TimeUs,
    pub bpm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickTimePoint {
    pub tick: ChartTick,
    pub time: TimeUs,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TickTimingEventKind {
    StopRaw { value: u64 },
    SetBpm(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickTimingEvent {
    pub tick: ChartTick,
    pub kind: TickTimingEventKind,
}

pub fn ticks_to_us(delta_ticks: u64, bpm: f64) -> i64 {
    let beats = delta_ticks as f64 / TICKS_PER_BEAT as f64;
    let us = beats * 60.0 * 1_000_000.0 / bpm;
    us.round() as i64
}

pub fn us_to_ticks(delta_us: i64, bpm: f64) -> u64 {
    let ticks = delta_us as f64 * bpm * TICKS_PER_BEAT as f64 / 60.0 / 1_000_000.0;
    ticks.floor().max(0.0) as u64
}

/// BMS `#STOPxx` の raw 値 (1/192 measure 単位) を microsecond duration へ変換する。
///
/// beatoraja 準拠の式: `duration_us = raw * 60_000_000 * 4 / (192 * bpm)`
/// 例) `#STOP01 192` (= 1 measure) を BPM 120 で停止 → 2_000_000us = 2 秒。
///
/// 内部的には `raw * 20` を ticks 換算して使うのと等価
/// (TICKS_PER_MEASURE / 192 = 3840 / 192 = 20)。
pub fn stop_raw_to_us(value: u64, bpm: f64) -> i64 {
    ticks_to_us(value.saturating_mul(TICKS_PER_MEASURE as u64 / 192), bpm)
}

pub fn build_timing_map(initial_bpm: f64, mut events: Vec<TickTimingEvent>) -> TimingMap {
    events.sort_by_key(|event| (event.tick.0, timing_event_priority(event.kind)));

    let mut segments = Vec::new();
    let mut current_tick = ChartTick(0);
    let mut current_time = TimeUs(0);
    let mut current_bpm = initial_bpm;

    for event in events {
        if event.tick > current_tick {
            let end_time =
                TimeUs(current_time.0 + ticks_to_us(event.tick.0 - current_tick.0, current_bpm));

            segments.push(TimingSegment {
                start_tick: current_tick,
                end_tick: event.tick,
                start_time: current_time,
                end_time,
                bpm: current_bpm,
            });

            current_time = end_time;
            current_tick = event.tick;
        }

        match event.kind {
            TickTimingEventKind::StopRaw { value } => {
                current_time = TimeUs(current_time.0 + stop_raw_to_us(value, current_bpm));
            }
            TickTimingEventKind::SetBpm(bpm) => {
                current_bpm = bpm;
            }
        }
    }

    segments.push(TimingSegment {
        start_tick: current_tick,
        end_tick: ChartTick(u64::MAX),
        start_time: current_time,
        end_time: TimeUs(i64::MAX),
        bpm: current_bpm,
    });

    TimingMap::new(initial_bpm, segments)
}

fn timing_event_priority(kind: TickTimingEventKind) -> u8 {
    match kind {
        TickTimingEventKind::StopRaw { .. } => 0,
        TickTimingEventKind::SetBpm(_) => 1,
    }
}

impl TimingMap {
    pub fn new(initial_bpm: f64, segments: Vec<TimingSegment>) -> Self {
        debug_assert!(!segments.is_empty());
        Self { initial_bpm, segments }
    }

    pub fn tick_to_time(&self, tick: ChartTick) -> TimeUs {
        let seg = self.find_segment_by_tick(tick);
        let delta = tick.0.saturating_sub(seg.start_tick.0);
        TimeUs(seg.start_time.0 + ticks_to_us(delta, seg.bpm))
    }

    pub fn time_to_tick(&self, time: TimeUs) -> ChartTick {
        let seg = self.find_segment_by_time(time);
        let delta_us = time.0.saturating_sub(seg.start_time.0);
        let delta_ticks = us_to_ticks(delta_us, seg.bpm);
        ChartTick(seg.start_tick.0 + delta_ticks)
    }

    pub fn bpm_at_tick(&self, tick: ChartTick) -> f64 {
        self.find_segment_by_tick(tick).bpm
    }

    pub fn bpm_at_time(&self, time: TimeUs) -> f64 {
        self.find_segment_by_time(time).bpm
    }

    pub fn find_segment_by_tick(&self, tick: ChartTick) -> &TimingSegment {
        let idx = self.segments.partition_point(|seg| seg.end_tick <= tick);
        &self.segments[idx.min(self.segments.len() - 1)]
    }

    pub fn find_segment_by_time(&self, time: TimeUs) -> &TimingSegment {
        let idx = self.segments.partition_point(|seg| seg.end_time <= time);
        &self.segments[idx.min(self.segments.len() - 1)]
    }

    /// `time_to_tick` の連続版。STOP 区間内では止まっている tick を返す。
    /// スクロール位置計算で sub-tick 精度が欲しいときに使う。
    pub fn time_to_tick_f64(&self, time: TimeUs) -> f64 {
        let seg = self.find_segment_by_time(time);
        let delta_us = (time.0 - seg.start_time.0).max(0);
        let delta_ticks = delta_us as f64 * seg.bpm * TICKS_PER_BEAT as f64 / 60_000_000.0;
        seg.start_tick.0 as f64 + delta_ticks
    }

    /// 譜面の `TimingEvent` から TimingMap を再構築する。STOP の duration_us は
    /// 既に解決済みの値を使うため `build_timing_map` の `StopRaw` 経路は通らない。
    pub fn from_chart_timing_events(
        initial_bpm: f64,
        events: &[crate::model::TimingEvent],
    ) -> TimingMap {
        use crate::model::TimingEventKind as Kind;

        let initial_bpm = initial_bpm.max(1.0);
        let mut sorted: Vec<_> = events.to_vec();
        sorted.sort_by_key(|e| {
            (
                e.tick,
                match e.kind {
                    Kind::Stop { .. } => 0,
                    Kind::BpmChange { .. } => 1,
                },
            )
        });

        let mut segments = Vec::new();
        let mut current_tick = ChartTick(0);
        let mut current_time = TimeUs(0);
        let mut current_bpm = initial_bpm;

        for event in sorted {
            if event.tick > current_tick {
                let end_time = TimeUs(
                    current_time.0 + ticks_to_us(event.tick.0 - current_tick.0, current_bpm),
                );
                segments.push(TimingSegment {
                    start_tick: current_tick,
                    end_tick: event.tick,
                    start_time: current_time,
                    end_time,
                    bpm: current_bpm,
                });
                current_time = end_time;
                current_tick = event.tick;
            }

            match event.kind {
                Kind::Stop { duration_us } => {
                    current_time = TimeUs(current_time.0 + duration_us.max(0));
                }
                Kind::BpmChange { bpm } => {
                    current_bpm = bpm.max(1.0);
                }
            }
        }

        segments.push(TimingSegment {
            start_tick: current_tick,
            end_tick: ChartTick(u64::MAX),
            start_time: current_time,
            end_time: TimeUs(i64::MAX),
            bpm: current_bpm,
        });

        TimingMap::new(initial_bpm, segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TimingEvent, TimingEventKind};

    #[test]
    fn stop_raw_to_us_matches_beatoraja_formula() {
        // `#STOP01 192` = 1 measure (4 beats) を BPM 120 で停止 → 2_000_000us
        assert_eq!(stop_raw_to_us(192, 120.0), 2_000_000);
        // 48 = 1 beat。BPM 120 で 500_000us。
        assert_eq!(stop_raw_to_us(48, 120.0), 500_000);
        // BPM 240 にすると同じ raw 値で半分の時間。
        assert_eq!(stop_raw_to_us(192, 240.0), 1_000_000);
        // 0 はゼロ秒。
        assert_eq!(stop_raw_to_us(0, 120.0), 0);
    }

    #[test]
    fn from_chart_events_matches_build_timing_map_for_bpm_change() {
        let ticks = TICKS_PER_BEAT as u64 * 4;
        let events = vec![TimingEvent {
            tick: ChartTick(ticks),
            time: TimeUs(0), // unused by builder
            kind: TimingEventKind::BpmChange { bpm: 240.0 },
        }];

        let map = TimingMap::from_chart_timing_events(120.0, &events);

        // 0..ticks at 120 BPM = 2 seconds
        assert_eq!(map.tick_to_time(ChartTick(ticks)), TimeUs(2_000_000));
        // After change, 4 beats at 240 BPM = 1 second
        assert_eq!(map.tick_to_time(ChartTick(ticks * 2)), TimeUs(3_000_000));
        assert_eq!(map.bpm_at_tick(ChartTick(ticks - 1)), 120.0);
        assert_eq!(map.bpm_at_tick(ChartTick(ticks)), 240.0);
    }

    #[test]
    fn time_to_tick_f64_freezes_during_stop() {
        let stop_tick = TICKS_PER_BEAT as u64 * 4;
        let stop_us = 1_000_000;
        let events = vec![TimingEvent {
            tick: ChartTick(stop_tick),
            time: TimeUs(0),
            kind: TimingEventKind::Stop { duration_us: stop_us },
        }];

        let map = TimingMap::from_chart_timing_events(120.0, &events);

        // 2 seconds of play at 120 BPM = 4 beats, lands at stop_tick exactly.
        let before_stop = map.time_to_tick_f64(TimeUs(2_000_000));
        assert!((before_stop - stop_tick as f64).abs() < 1e-6);

        // Halfway through the stop, tick stays put.
        let mid_stop = map.time_to_tick_f64(TimeUs(2_500_000));
        assert!((mid_stop - stop_tick as f64).abs() < 1e-6);

        // After the stop completes, scrolling resumes.
        let after_stop = map.time_to_tick_f64(TimeUs(2_000_000 + stop_us + 500_000));
        let expected = stop_tick as f64 + 0.5 * 120.0 * TICKS_PER_BEAT as f64 / 60.0;
        assert!((after_stop - expected).abs() < 1e-3);
    }
}
