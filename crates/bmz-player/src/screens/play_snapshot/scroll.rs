use super::*;

/// ノーツ / 小節線 / ロングノートのスクロール計算に使う時刻。
/// playstart 中は beatoraja と同く 0 扱いにする。
pub(super) fn scroll_render_time(render_now: TimeUs) -> TimeUs {
    TimeUs(render_now.0.max(0))
}

/// BPM 変化と STOP に追従した tick ベースのスクロール計算ヘルパ。
///
/// beatoraja の LaneRenderer と同じく 4 拍ぶんの tick 幅を基準にし、現在カーソル
/// tick との差分でノートの y を出す。これにより BPM が上がれば見かけのスクロール
/// 速度も上がり、STOP 中はカーソル tick が停止する。
pub(super) struct ScrollContext<'a> {
    pub(super) timing_map: &'a TimingMap,
    pub(super) hispeed: f32,
    pub(super) visible_lane_fraction: f32,
    pub(super) lookahead_ticks: f64,
    /// SCROLL イベント (tick 昇順)。`(tick, factor)`。
    /// 区間ごとに factor を掛けて scroll 位置を畳む。空なら factor 1.0 固定。
    pub(super) scroll_integral: &'a ScrollIntegralCache,
    /// SPEED イベント (tick 昇順)。隣接イベント間を線形補間し、note 位置時点の値を
    /// 倍率として掛ける。
    pub(super) speed_segments: &'a [(f64, f64)],
}

impl<'a> ScrollContext<'a> {
    pub(super) fn cursor_tick(&self, render_now: TimeUs) -> f64 {
        self.timing_map.time_to_tick_f64(render_now)
    }

    pub(super) fn simple_tick_upper_bound(&self, cursor_tick: f64) -> Option<f64> {
        if !self.scroll_integral.is_empty() || !self.speed_segments.is_empty() {
            return None;
        }
        let hispeed = self.hispeed.max(crate::config::play::HISPEED_MIN) as f64;
        let visible = self.visible_lane_fraction.max(f32::EPSILON) as f64;
        Some(cursor_tick + self.lookahead_ticks * visible / hispeed + f64::EPSILON)
    }

    /// ノートの正規化進捗（0.0=判定ライン, 1.0=画面上端）。判定ラインより手前 (delta<0)
    /// と画面上端より奥のノートは `None`。SCROLL / SPEED 倍率を畳み込む。
    pub(super) fn note_y(&self, note_time: TimeUs, cursor_tick: f64) -> Option<f32> {
        let note_tick = self.timing_map.time_to_tick_f64(note_time);
        let delta = self.scroll_delta(cursor_tick, note_tick);
        if delta < 0.0 {
            return None;
        }
        let progress = self.progress_from_delta(delta);
        (progress <= 1.0).then_some(progress)
    }

    /// `note_y` と同じ進捗のクランプしない生値。ロングノートの始端/終端で使う。
    pub(super) fn note_progress(&self, note_time: TimeUs, cursor_tick: f64) -> f32 {
        let note_tick = self.timing_map.time_to_tick_f64(note_time);
        let delta = self.scroll_delta(cursor_tick, note_tick);
        self.progress_from_delta(delta)
    }

    pub(super) fn progress_from_delta(&self, delta: f64) -> f32 {
        let visible = self.visible_lane_fraction.max(f32::EPSILON) as f64;
        (delta / (self.lookahead_ticks * visible)) as f32 * self.hispeed
    }

    /// `from..to` の tick 区間にわたって SCROLL の factor を畳み込み、note 位置の
    /// SPEED 倍率を掛けた「見かけの距離」を返す。factor が負だと delta も負になり、
    /// note_y は `None` に倒れる(= 逆スクロール時は画面外として描画対象外)。
    pub(super) fn scroll_delta(&self, from_tick: f64, to_tick: f64) -> f64 {
        self.scroll_integral.delta(from_tick, to_tick) * speed_at(self.speed_segments, to_tick)
    }
}

pub(super) fn visible_lane_notes(
    notes: &[NoteEvent],
    lower_time: Option<TimeUs>,
    lower_index: Option<usize>,
    upper_tick: Option<f64>,
) -> &[NoteEvent] {
    let time_start =
        lower_time.map_or(0, |lower_time| notes.partition_point(|note| note.time < lower_time));
    let start = lower_index.map_or(time_start, |lower_index| time_start.min(lower_index));
    let end = upper_tick.map_or(notes.len(), |upper_tick| {
        notes.partition_point(|note| (note.tick.0 as f64) <= upper_tick)
    });
    &notes[start.min(end)..end]
}

/// Simple scroll では tick と画面上の順序が一致するため、二分探索で可視範囲へ
/// 絞れる。SCROLL/SPEED 使用時は負値・補間で単調性を仮定できないので、既存どおり
/// 全候補を返して表示互換を優先する。
pub(super) fn visible_bar_lines(
    bar_lines: &[BarLine],
    lower_time: TimeUs,
    tick_range: Option<(f64, f64)>,
) -> &[BarLine] {
    let Some((lower_tick, upper_tick)) = tick_range else {
        return bar_lines;
    };
    // STOP 中の開始点と、互換テスト/不正規化チャートの time/tick 不整合を安全に残す。
    let start_by_time = bar_lines.partition_point(|bar| bar.time < lower_time);
    let start_by_tick = bar_lines.partition_point(|bar| (bar.tick.0 as f64) < lower_tick);
    let start = start_by_time.min(start_by_tick);
    let end = bar_lines.partition_point(|bar| (bar.tick.0 as f64) <= upper_tick);
    &bar_lines[start.min(end)..end]
}

pub(super) fn visible_timing_events(
    events: &[bmz_chart::model::TimingEvent],
    lower_time: TimeUs,
    tick_range: Option<(f64, f64)>,
) -> &[bmz_chart::model::TimingEvent] {
    let Some((lower_tick, upper_tick)) = tick_range else {
        return events;
    };
    let start_by_time = events.partition_point(|event| event.time < lower_time);
    let start_by_tick = events.partition_point(|event| (event.tick.0 as f64) < lower_tick);
    let start = start_by_time.min(start_by_tick);
    let end = events.partition_point(|event| (event.tick.0 as f64) <= upper_tick);
    &events[start.min(end)..end]
}

pub(super) fn visible_long_notes<'a>(
    long_notes: &'a [bmz_chart::model::LongNotePair],
    prefix_max_end_times: &[i64],
    lower_time: TimeUs,
    tick_range: Option<(f64, f64)>,
) -> impl Iterator<Item = (usize, &'a bmz_chart::model::LongNotePair)> + 'a {
    let range = tick_range.map_or(0..long_notes.len(), |(_, upper_tick)| {
        debug_assert_eq!(long_notes.len(), prefix_max_end_times.len());
        // prefix max は単調なので、この位置より前の LN はすべて終端が判定線より前。
        let start = prefix_max_end_times.partition_point(|&end_time| end_time < lower_time.0);
        // 既存実装も start tick 順を前提に break していたので、その順序を保つ。
        let end = long_notes.partition_point(|long| (long.start_tick.0 as f64) <= upper_tick);
        start.min(end)..end
    });
    long_notes[range.clone()]
        .iter()
        .enumerate()
        .map(move |(offset, long)| (range.start + offset, long))
}

/// `1..=end_second` のうち、単純スクロール画面に入り得る秒線を返す。
/// tick は時間に対して単調非減少なので、STOP があっても二分探索できる。
pub(super) fn visible_time_line_seconds(
    timing_map: &TimingMap,
    end_second: i64,
    render_now: TimeUs,
    simple_tick_upper_bound: Option<f64>,
) -> Range<i64> {
    if end_second < 1 {
        return 1..1;
    }
    let Some(upper_tick) = simple_tick_upper_bound else {
        // SCROLL/SPEED は負値を含み得るため、過去の秒線が再度画面に入る。
        // この経路は単調性を仮定せず、従来どおり全候補を評価する。
        return 1..end_second.saturating_add(1);
    };
    let start = ((render_now.0.max(0).saturating_add(999_999)) / 1_000_000).clamp(1, end_second);
    let end = {
        let mut low = start;
        let mut high = end_second.saturating_add(1);
        while low < high {
            let middle = low + (high - low) / 2;
            let time = TimeUs(middle.saturating_mul(1_000_000));
            if timing_map.time_to_tick_f64(scroll_render_time(time)) <= upper_tick {
                low = middle.saturating_add(1);
            } else {
                high = middle;
            }
        }
        low.saturating_sub(1)
    };
    start..end.saturating_add(1)
}

/// `segments` を階段関数として `from..to` の区間積分を返す。factor は次のイベントまで
/// 一定。`from > to` の場合は対称に負値を返す。
#[cfg(test)]
pub(super) fn accumulate_scroll(segments: &[(f64, f64)], from_tick: f64, to_tick: f64) -> f64 {
    if (from_tick - to_tick).abs() < f64::EPSILON {
        return 0.0;
    }
    let (lo, hi, sign) =
        if from_tick <= to_tick { (from_tick, to_tick, 1.0) } else { (to_tick, from_tick, -1.0) };
    let mut acc = 0.0;
    let mut prev = lo;
    let mut factor = factor_before(segments, lo);
    for &(tick, next_factor) in segments {
        if tick <= lo {
            continue;
        }
        if tick >= hi {
            break;
        }
        acc += (tick - prev) * factor;
        prev = tick;
        factor = next_factor;
    }
    acc += (hi - prev) * factor;
    acc * sign
}

pub(crate) fn current_scroll_multiplier(
    chart: &PlayableChart,
    timing_map: &TimingMap,
    render_now: TimeUs,
) -> f32 {
    let cursor_tick = timing_map.time_to_tick_f64(scroll_render_time(render_now));
    current_scroll_multiplier_for_tick(chart, cursor_tick)
}

pub(super) fn current_scroll_multiplier_for_tick(chart: &PlayableChart, cursor_tick: f64) -> f32 {
    scroll_multiplier_at_tick(chart, cursor_tick) as f32
}

pub(crate) fn scroll_multiplier_at_tick(chart: &PlayableChart, cursor_tick: f64) -> f64 {
    let scroll_index =
        chart.scroll_events.partition_point(|event| event.tick.0 as f64 <= cursor_tick);
    let scroll = scroll_index.checked_sub(1).map_or(1.0, |index| chart.scroll_events[index].factor);
    let speed = current_speed_factor_for_tick(&chart.speed_events, cursor_tick);
    scroll * speed
}

pub(super) fn current_scroll_multiplier_from_segments(
    scroll_integral: &ScrollIntegralCache,
    speed_segments: &[(f64, f64)],
    cursor_tick: f64,
) -> f32 {
    (scroll_integral.factor_at(cursor_tick) * speed_at(speed_segments, cursor_tick)) as f32
}

#[cfg(test)]
pub(super) fn factor_before(segments: &[(f64, f64)], tick: f64) -> f64 {
    segments
        .partition_point(|(event_tick, _)| *event_tick <= tick)
        .checked_sub(1)
        .map_or(1.0, |index| segments[index].1)
}

pub(super) fn current_speed_factor_for_tick(
    events: &[bmz_chart::model::SpeedEvent],
    tick: f64,
) -> f64 {
    if events.is_empty() {
        return 1.0;
    }

    let next_index = events.partition_point(|event| event.tick.0 as f64 <= tick);
    let prev = next_index.checked_sub(1).map(|index| {
        let event = &events[index];
        (event.tick.0 as f64, event.factor)
    });
    let next = events.get(next_index).map(|event| {
        let event_tick = event.tick.0 as f64;
        (event_tick, event.factor)
    });
    interpolate_speed(prev, next, tick)
}

/// 指定 tick における SPEED の現在値を返す。beatoraja 仕様に合わせ、隣接イベント間は
/// 線形補間。最初のイベント前は 1.0、最後のイベント以降はその値で固定。
pub(super) fn speed_at(segments: &[(f64, f64)], tick: f64) -> f64 {
    if segments.is_empty() {
        return 1.0;
    }
    // tick を挟む直前 (prev) / 直後 (next) のイベントを探す。
    let next_index = segments.partition_point(|(event_tick, _)| *event_tick <= tick);
    let prev = next_index.checked_sub(1).map(|index| segments[index]);
    let next = segments.get(next_index).copied();
    interpolate_speed(prev, next, tick)
}

pub(super) fn interpolate_speed(
    prev: Option<(f64, f64)>,
    next: Option<(f64, f64)>,
    tick: f64,
) -> f64 {
    match (prev, next) {
        (None, _) => 1.0,
        (Some((_, f)), None) => f,
        (Some((t0, f0)), Some((t1, f1))) => {
            let span = t1 - t0;
            if span <= f64::EPSILON {
                return f1;
            }
            let ratio = ((tick - t0) / span).clamp(0.0, 1.0);
            f0 + (f1 - f0) * ratio
        }
    }
}
