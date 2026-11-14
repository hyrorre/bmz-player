use bmz_core::time::{ChartTick, TimeUs};

pub const TICKS_PER_BEAT: u32 = 960;
pub const BEATS_PER_MEASURE: u32 = 4;
pub const TICKS_PER_MEASURE: u32 = TICKS_PER_BEAT * BEATS_PER_MEASURE;

/// 譜面インポート中だけ使う高精度tickの倍率。
///
/// 公開される `ChartTick` の単位は従来どおりだが、BMS の極端に短い `#02` 小節を
/// 整数tickへ丸める前に、十分な精度で時間を計算するために使う。
pub const IMPORT_TICK_SCALE: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct TimingMap {
    pub initial_bpm: f64,
    pub segments: Vec<TimingSegment>,
    tick_scale: u64,
    bpm_reflections: Vec<TickTimePoint>,
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

/// Reflect a judgement clock around its most recent negative BPM event.
pub fn reflect_time(time: TimeUs, anchor: TimeUs) -> TimeUs {
    TimeUs(anchor.0.saturating_sub(time.0.saturating_sub(anchor.0)))
}

pub fn bpm_reflection_at(points: &[TickTimePoint], time: TimeUs) -> Option<TickTimePoint> {
    let end = points.partition_point(|point| point.time <= time);
    points[..end].last().copied()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TickTimingEventKind {
    StopRaw { value: u64 },
    BmsonStop { duration_pulses: u64, resolution: u64 },
    SetBpm(f64),
    SetNegativeBmsBpm(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickTimingEvent {
    pub tick: ChartTick,
    pub kind: TickTimingEventKind,
}

pub fn ticks_to_us(delta_ticks: u64, bpm: f64) -> i64 {
    ticks_to_us_scaled(delta_ticks, bpm, 1)
}

pub fn ticks_to_us_scaled(delta_ticks: u64, bpm: f64, tick_scale: u64) -> i64 {
    let bpm = sanitize_bpm(bpm);
    let ticks_per_beat = TICKS_PER_BEAT as f64 * tick_scale.max(1) as f64;
    let beats = delta_ticks as f64 / ticks_per_beat;
    let us = beats * 60.0 * 1_000_000.0 / bpm;
    us.round() as i64
}

pub fn us_to_ticks(delta_us: i64, bpm: f64) -> u64 {
    us_to_ticks_scaled(delta_us, bpm, 1)
}

pub fn us_to_ticks_scaled(delta_us: i64, bpm: f64, tick_scale: u64) -> u64 {
    let bpm = sanitize_bpm(bpm);
    let ticks = delta_us as f64 * bpm * TICKS_PER_BEAT as f64 * tick_scale.max(1) as f64
        / 60.0
        / 1_000_000.0;
    ticks.floor().max(0.0) as u64
}

fn sanitize_bpm(bpm: f64) -> f64 {
    if bpm.is_finite() && bpm > 0.0 { bpm } else { 1.0 }
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

/// BMSON STOP の pulse duration を microsecond へ変換する。
pub fn bmson_stop_pulses_to_us(duration_pulses: u64, resolution: u64, bpm: f64) -> i64 {
    let bpm = sanitize_bpm(bpm);
    let beats = duration_pulses as f64 / resolution.max(1) as f64;
    (beats * 60_000_000.0 / bpm).round() as i64
}

fn add_time_us(time: TimeUs, delta_us: i64) -> TimeUs {
    TimeUs(time.0.saturating_add(delta_us))
}

pub fn build_timing_map(initial_bpm: f64, events: Vec<TickTimingEvent>) -> TimingMap {
    build_timing_map_with_tick_scale(initial_bpm, events, 1)
}

pub fn build_timing_map_with_tick_scale(
    initial_bpm: f64,
    mut events: Vec<TickTimingEvent>,
    tick_scale: u64,
) -> TimingMap {
    let tick_scale = tick_scale.max(1);
    events.sort_by_key(|event| (event.tick.0, timing_event_priority(event.kind)));

    let mut segments = Vec::new();
    let mut current_tick = ChartTick(0);
    let mut current_time = TimeUs(0);
    let mut current_bpm = sanitize_bpm(initial_bpm);

    for event in events {
        if event.tick > current_tick {
            let end_time = add_time_us(
                current_time,
                ticks_to_us_scaled(event.tick.0 - current_tick.0, current_bpm, tick_scale),
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
            TickTimingEventKind::StopRaw { value } => {
                segments.push(TimingSegment {
                    start_tick: current_tick,
                    end_tick: current_tick,
                    start_time: current_time,
                    end_time: current_time,
                    bpm: current_bpm,
                });
                current_time = add_time_us(current_time, stop_raw_to_us(value, current_bpm));
            }
            TickTimingEventKind::BmsonStop { duration_pulses, resolution } => {
                segments.push(TimingSegment {
                    start_tick: current_tick,
                    end_tick: current_tick,
                    start_time: current_time,
                    end_time: current_time,
                    bpm: current_bpm,
                });
                current_time = add_time_us(
                    current_time,
                    bmson_stop_pulses_to_us(duration_pulses, resolution, current_bpm),
                );
            }
            TickTimingEventKind::SetBpm(bpm) | TickTimingEventKind::SetNegativeBmsBpm(bpm) => {
                current_bpm = sanitize_bpm(bpm);
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

    TimingMap::new_with_tick_scale(initial_bpm, segments, tick_scale)
}

fn timing_event_priority(kind: TickTimingEventKind) -> u8 {
    match kind {
        // Match beatoraja/bms-rs: BPM changes take effect before a STOP at
        // the same position, so the STOP uses the new BPM.
        TickTimingEventKind::SetBpm(_) | TickTimingEventKind::SetNegativeBmsBpm(_) => 0,
        TickTimingEventKind::StopRaw { .. } | TickTimingEventKind::BmsonStop { .. } => 1,
    }
}

impl TimingMap {
    pub fn new(initial_bpm: f64, segments: Vec<TimingSegment>) -> Self {
        Self::new_with_tick_scale(initial_bpm, segments, 1)
    }

    pub fn new_with_tick_scale(
        initial_bpm: f64,
        segments: Vec<TimingSegment>,
        tick_scale: u64,
    ) -> Self {
        debug_assert!(!segments.is_empty());
        Self { initial_bpm, segments, tick_scale: tick_scale.max(1), bpm_reflections: Vec::new() }
    }

    pub fn tick_to_time(&self, tick: ChartTick) -> TimeUs {
        if let Some(seg) = self.first_segment_starting_at(tick) {
            return seg.start_time;
        }

        let seg = self.find_segment_by_tick(tick);
        let delta = tick.0.saturating_sub(seg.start_tick.0);
        add_time_us(seg.start_time, ticks_to_us_scaled(delta, seg.bpm, self.tick_scale))
    }

    pub fn time_to_tick(&self, time: TimeUs) -> ChartTick {
        let seg = self.find_segment_by_time(time);
        let delta_us = time.0.saturating_sub(seg.start_time.0);
        let delta_ticks = us_to_ticks_scaled(delta_us, seg.bpm, self.tick_scale);
        ChartTick(seg.start_tick.0.saturating_add(delta_ticks))
    }

    pub fn bpm_at_tick(&self, tick: ChartTick) -> f64 {
        self.find_segment_by_tick(tick).bpm
    }

    pub fn bpm_at_time(&self, time: TimeUs) -> f64 {
        self.find_segment_by_time(time).bpm
    }

    pub fn bpm_reflections(&self) -> &[TickTimePoint] {
        &self.bpm_reflections
    }

    pub fn bpm_reflection_at(&self, time: TimeUs) -> Option<TickTimePoint> {
        bpm_reflection_at(&self.bpm_reflections, time)
    }

    pub fn reflected_judge_time(&self, time: TimeUs) -> TimeUs {
        self.bpm_reflection_at(time).map_or(time, |point| reflect_time(time, point.time))
    }

    pub fn find_segment_by_tick(&self, tick: ChartTick) -> &TimingSegment {
        let idx = self.segments.partition_point(|seg| seg.end_tick <= tick);
        &self.segments[idx.min(self.segments.len() - 1)]
    }

    pub fn find_segment_by_time(&self, time: TimeUs) -> &TimingSegment {
        let idx = self.segments.partition_point(|seg| seg.end_time <= time);
        &self.segments[idx.min(self.segments.len() - 1)]
    }

    fn first_segment_starting_at(&self, tick: ChartTick) -> Option<&TimingSegment> {
        let idx = self.segments.partition_point(|seg| seg.start_tick < tick);
        self.segments.get(idx).filter(|seg| seg.start_tick == tick)
    }

    /// `time_to_tick` の連続版。STOP 区間内では止まっている tick を返す。
    /// スクロール位置計算で sub-tick 精度が欲しいときに使う。
    pub fn time_to_tick_f64(&self, time: TimeUs) -> f64 {
        let seg = self.find_segment_by_time(time);
        let delta_us = (time.0 - seg.start_time.0).max(0);
        let delta_ticks =
            delta_us as f64 * seg.bpm * TICKS_PER_BEAT as f64 * self.tick_scale as f64
                / 60_000_000.0;
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
        // Imported charts record the precise event time before their temporary high-resolution
        // ticks are compressed back to the public ChartTick unit. Hand-built legacy charts often
        // leave this field at zero, so retain the old tick-derived path for those charts.
        let use_recorded_times = events.iter().any(|event| event.time.0 != 0);
        let mut sorted: Vec<_> = events.to_vec();
        if use_recorded_times {
            sorted.sort_by_key(|e| (e.time, timing_event_priority_from_chart_kind(e.kind)));
        } else {
            sorted.sort_by_key(|e| (e.tick, timing_event_priority_from_chart_kind(e.kind)));
        }

        let mut segments = Vec::new();
        let mut current_tick = ChartTick(0);
        let mut current_time = TimeUs(0);
        let mut current_bpm = initial_bpm;
        let mut bpm_reflections = Vec::new();
        let mut signed_bpm_position = None;

        for event in sorted {
            let event_time = if use_recorded_times {
                TimeUs(event.time.0.max(current_time.0))
            } else {
                add_time_us(
                    current_time,
                    ticks_to_us(event.tick.0.saturating_sub(current_tick.0), current_bpm),
                )
            };
            if event.tick > current_tick {
                segments.push(TimingSegment {
                    start_tick: current_tick,
                    end_tick: event.tick,
                    start_time: current_time,
                    end_time: event_time,
                    bpm: current_bpm,
                });
                current_time = event_time;
                current_tick = event.tick;
            } else if use_recorded_times && event_time > current_time {
                // Different BMS positions can intentionally share one public ChartTick after
                // compression. Preserve their elapsed time for time-based queries.
                segments.push(TimingSegment {
                    start_tick: current_tick,
                    end_tick: current_tick,
                    start_time: current_time,
                    end_time: event_time,
                    bpm: current_bpm,
                });
                current_time = event_time;
            }

            match event.kind {
                Kind::Stop { duration_us } => {
                    segments.push(TimingSegment {
                        start_tick: current_tick,
                        end_tick: current_tick,
                        start_time: current_time,
                        end_time: current_time,
                        bpm: current_bpm,
                    });
                    current_time = add_time_us(current_time, duration_us.max(0));
                }
                Kind::BpmChange { bpm } => {
                    let is_signed_bpm =
                        signed_bpm_position.take() == Some((event.tick, event.time));
                    current_bpm = if is_signed_bpm { sanitize_bpm(bpm) } else { bpm.max(1.0) };
                }
                Kind::BpmReflection => {
                    signed_bpm_position = Some((event.tick, event.time));
                    // In LR2 the playback origin is also the inactive boundary.
                    if current_time.0 > 0 {
                        bpm_reflections
                            .push(TickTimePoint { tick: current_tick, time: current_time });
                    }
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

        TimingMap { bpm_reflections, ..TimingMap::new(initial_bpm, segments) }
    }
}

fn timing_event_priority_from_chart_kind(kind: crate::model::TimingEventKind) -> u8 {
    match kind {
        crate::model::TimingEventKind::BpmChange { .. }
        | crate::model::TimingEventKind::BpmReflection => 0,
        crate::model::TimingEventKind::Stop { .. } => 1,
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
    fn bmson_stop_pulses_to_us_uses_quarter_note_resolution() {
        assert_eq!(bmson_stop_pulses_to_us(240, 240, 120.0), 500_000);
        assert_eq!(bmson_stop_pulses_to_us(240, 240, 240.0), 250_000);
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
    fn from_chart_events_uses_recorded_times_after_tick_compression() {
        let tick = TICKS_PER_MEASURE as u64;
        let events = vec![
            TimingEvent {
                tick: ChartTick(tick),
                time: TimeUs(2_000_000),
                kind: TimingEventKind::BpmChange { bpm: 1.0 },
            },
            TimingEvent {
                tick: ChartTick(tick),
                time: TimeUs(2_000_225),
                kind: TimingEventKind::Stop { duration_us: 1_250_000 },
            },
        ];

        let map = TimingMap::from_chart_timing_events(120.0, &events);

        assert_eq!(map.bpm_at_time(TimeUs(2_000_100)), 1.0);
        assert!((map.time_to_tick_f64(TimeUs(3_750_225)) - (tick + 8) as f64).abs() < 1e-6);
    }

    #[test]
    fn build_timing_map_saturates_extreme_chart_time() {
        let events = vec![TickTimingEvent {
            tick: ChartTick(u64::MAX),
            kind: TickTimingEventKind::SetBpm(120.0),
        }];

        let map = build_timing_map(1.0, events);

        assert_eq!(map.segments[0].end_time, TimeUs(i64::MAX));
        assert_eq!(map.tick_to_time(ChartTick(u64::MAX)), TimeUs(i64::MAX));
    }

    #[test]
    fn build_timing_map_saturates_extreme_stop_time() {
        let events = vec![TickTimingEvent {
            tick: ChartTick(0),
            kind: TickTimingEventKind::StopRaw { value: u64::MAX },
        }];

        let map = build_timing_map(1.0, events);

        assert_eq!(map.tick_to_time(ChartTick(0)), TimeUs(0));
        assert_eq!(map.segments.last().unwrap().start_time, TimeUs(i64::MAX));
    }

    #[test]
    fn build_timing_map_sanitizes_zero_bpm_changes() {
        let events = vec![TickTimingEvent {
            tick: ChartTick(TICKS_PER_MEASURE as u64),
            kind: TickTimingEventKind::SetBpm(0.0),
        }];

        let map = build_timing_map(120.0, events);

        assert_eq!(map.tick_to_time(ChartTick(TICKS_PER_MEASURE as u64)), TimeUs(2_000_000));
        assert_eq!(map.tick_to_time(ChartTick(TICKS_PER_MEASURE as u64 * 2)), TimeUs(242_000_000));
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

    #[test]
    fn tick_to_time_uses_stop_start_for_same_tick_notes() {
        let stop_tick = TICKS_PER_BEAT as u64 * 4;
        let stop_us = 1_000_000;
        let events = vec![TimingEvent {
            tick: ChartTick(stop_tick),
            time: TimeUs(0),
            kind: TimingEventKind::Stop { duration_us: stop_us },
        }];

        let map = TimingMap::from_chart_timing_events(120.0, &events);

        assert_eq!(map.tick_to_time(ChartTick(stop_tick)), TimeUs(2_000_000));
        assert_eq!(
            map.tick_to_time(ChartTick(stop_tick + TICKS_PER_BEAT as u64)),
            TimeUs(3_500_000)
        );
    }

    #[test]
    fn tick_to_time_uses_bpm_change_start_for_same_tick_notes() {
        let change_tick = TICKS_PER_BEAT as u64 * 4;
        let events = vec![TimingEvent {
            tick: ChartTick(change_tick),
            time: TimeUs(0),
            kind: TimingEventKind::BpmChange { bpm: 0.0 },
        }];

        let map = TimingMap::from_chart_timing_events(120.0, &events);

        assert_eq!(map.tick_to_time(ChartTick(change_tick)), TimeUs(2_000_000));
        assert_eq!(
            map.tick_to_time(ChartTick(change_tick + TICKS_PER_BEAT as u64)),
            TimeUs(62_000_000)
        );
    }

    #[test]
    fn build_timing_map_uses_new_bpm_for_same_tick_stop() {
        let change_tick = TICKS_PER_BEAT as u64 * 4;
        let events = vec![
            TickTimingEvent {
                tick: ChartTick(change_tick),
                kind: TickTimingEventKind::StopRaw { value: 48 },
            },
            TickTimingEvent {
                tick: ChartTick(change_tick),
                kind: TickTimingEventKind::SetBpm(240.0),
            },
        ];

        let map = build_timing_map(120.0, events);

        // Four beats at 120 BPM = 2 seconds. STOP 48 is one beat, so it is
        // 250 ms at the new BPM 240, followed by one beat at 240 BPM.
        assert_eq!(
            map.tick_to_time(ChartTick(change_tick + TICKS_PER_BEAT as u64)),
            TimeUs(2_500_000)
        );
    }
}
