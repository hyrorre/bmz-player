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

pub fn stop_raw_to_us(value: u64, bpm: f64) -> i64 {
    ticks_to_us(value, bpm)
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
}
