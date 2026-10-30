use super::*;

/// 消滅位置はrendererが決める。snapshotには従来どおり最大8小節ぶんだけ残す。
pub(super) const MAX_MISSED_NOTE_FALL: f32 = 8.0;

#[derive(Debug, Clone)]
pub(super) struct MissedNoteFallCache {
    segments: Arc<[MovingTimeSegment]>,
    lanes: [Arc<[MissedNoteStart]>; LANE_COUNT],
}

#[derive(Debug, Clone, Copy)]
struct MovingTimeSegment {
    start_us: i64,
    end_us: i64,
    measures_at_start: f64,
    measures_per_us: f64,
}

#[derive(Debug, Clone, Copy)]
struct MissedNoteStart {
    stop_end_us: i64,
    /// STOP終了とnote.timeのprefix最大値。時刻とtickがずれたchartでも未来を除かない。
    prefix_max_start_us: i64,
}

/// 時刻ごとの累積値は1回だけ求め、ノーツ側の始点との差を取る。
/// 判定幅はcacheへ焼き込まず、Practice等の現在のwindowを使う。
#[derive(Clone, Copy)]
pub(super) struct MissedNoteFallFrame<'a> {
    cache: &'a MissedNoteFallCache,
    measures_now: f64,
    delay_us: i64,
}

impl MissedNoteFallCache {
    pub(super) fn new(chart: &PlayableChart, timing: &TimingMap) -> Self {
        let mut measures = 0.0;
        let segments = timing
            .segments
            .iter()
            .map(|segment| {
                let bpm =
                    if segment.bpm.is_finite() && segment.bpm > 0.0 { segment.bpm } else { 1.0 };
                let cached = MovingTimeSegment {
                    start_us: segment.start_time.0.max(0),
                    end_us: segment.end_time.0,
                    measures_at_start: measures,
                    measures_per_us: bpm / 240_000_000.0,
                };
                // segment間の空白がSTOP。そこでは累積値を増やさない。
                measures += cached.end_us.saturating_sub(cached.start_us).max(0) as f64
                    * cached.measures_per_us;
                cached
            })
            .collect::<Vec<_>>()
            .into();
        let lanes = std::array::from_fn(|lane| {
            let mut prefix_max_start_us = i64::MIN;
            chart.lane_notes[lane]
                .iter()
                .map(|note| {
                    // 同じtickの最後のsegmentは、その位置の連続STOPがすべて終わった時刻。
                    let index = timing.segments.partition_point(|seg| seg.start_tick <= note.tick);
                    let stop_end_us = index
                        .checked_sub(1)
                        .and_then(|index| timing.segments.get(index))
                        .filter(|segment| segment.start_tick == note.tick)
                        .map_or(note.time.0, |segment| segment.start_time.0);
                    prefix_max_start_us = prefix_max_start_us.max(stop_end_us).max(note.time.0);
                    MissedNoteStart { stop_end_us, prefix_max_start_us }
                })
                .collect::<Vec<_>>()
                .into()
        });
        Self { segments, lanes }
    }

    fn measures_at(&self, time_us: i64) -> f64 {
        let time_us = time_us.max(0);
        let index = self.segments.partition_point(|segment| segment.end_us <= time_us);
        let segment = &self.segments[index.min(self.segments.len() - 1)];
        segment.measures_at_start
            + time_us.saturating_sub(segment.start_us).max(0) as f64 * segment.measures_per_us
    }

    pub(super) fn frame(&self, now: TimeUs, delay_us: i64) -> MissedNoteFallFrame<'_> {
        MissedNoteFallFrame {
            cache: self,
            measures_now: self.measures_at(now.0),
            delay_us: delay_us.max(0),
        }
    }
}

impl MissedNoteFallFrame<'_> {
    fn progress_from_stop_end(&self, stop_end_us: i64) -> f32 {
        let start = stop_end_us.saturating_add(self.delay_us);
        (self.measures_now - self.cache.measures_at(start)).max(0.0) as f32
    }

    pub(super) fn first_note(&self, lane: Lane) -> usize {
        self.cache.lanes[lane.index()].partition_point(|note| {
            self.progress_from_stop_end(note.prefix_max_start_us) > MAX_MISSED_NOTE_FALL
        })
    }

    pub(super) fn progress(&self, lane: Lane, note_index: usize) -> Option<f32> {
        let fall =
            self.progress_from_stop_end(self.cache.lanes[lane.index()][note_index].stop_end_us);
        (fall <= MAX_MISSED_NOTE_FALL).then_some(fall)
    }
}
