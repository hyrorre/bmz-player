use super::*;

/// Turntable offset is driven by scene elapsed time, like beatoraja's scratch angle offset.
pub fn skin_visual_time(_chart_time: TimeUs, play_elapsed: TimeUs) -> TimeUs {
    play_elapsed
}

/// chart 時刻と大きく乖離した wall-clock 系 key 時刻を検出する。
pub(super) fn is_wall_clock_key_time(started_at: TimeUs, chart_time: TimeUs) -> bool {
    bmz_gameplay::session::is_wall_clock_lane_key_time(started_at, chart_time)
}

pub(super) fn lane_key_timer_now(
    started_at: TimeUs,
    chart_time: TimeUs,
    play_elapsed: TimeUs,
) -> Option<TimeUs> {
    if is_wall_clock_key_time(started_at, chart_time) {
        if chart_time.0 >= 0 { None } else { Some(play_elapsed) }
    } else {
        Some(chart_time)
    }
}

pub(super) fn skin_timer_elapsed_ms(now: TimeUs, started_at: TimeUs) -> i32 {
    ((now.0 - started_at.0) / 1_000).clamp(0, i32::MAX as i64) as i32
}

pub(super) fn optional_skin_timer_elapsed_ms(
    now: TimeUs,
    started_at: Option<TimeUs>,
) -> Option<i32> {
    started_at.map(|started_at| skin_timer_elapsed_ms(now, started_at))
}

pub(super) fn rhythm_timer_elapsed_ms(
    timing_map: &TimingMap,
    bar_lines: &[BarLine],
    now: TimeUs,
) -> Option<i32> {
    if now.0 < 0 {
        return None;
    }
    let section_start = bar_lines
        .partition_point(|bar| bar.time <= now)
        .checked_sub(1)
        .map(|index| bar_lines[index].time)
        .unwrap_or(TimeUs(0));
    let elapsed_us = rhythm_timer_elapsed_us(timing_map, section_start, now);
    Some((elapsed_us / 1_000.0).floor().clamp(0.0, i32::MAX as f64) as i32)
}

/// LR2互換の TIMER_RHYTHM 更新式を、フレーム刻みではなく区間積分で再現する。
///
/// 通常のプレイ速度は百分率の 100 なので、timer の経過値は実時間の
/// `bpm / 60` 倍になる。
pub(super) fn rhythm_timer_elapsed_us(timing_map: &TimingMap, start: TimeUs, end: TimeUs) -> f64 {
    let mut cursor = start.0.max(0).min(end.0);
    let end = end.0.max(cursor);
    let mut elapsed_us = 0.0;
    while cursor < end {
        let segment = timing_map.find_segment_by_time(TimeUs(cursor));
        let boundary =
            if segment.start_time.0 > cursor { segment.start_time.0 } else { segment.end_time.0 };
        let next = end.min(boundary.max(cursor.saturating_add(1)));
        let bpm = if segment.bpm.is_finite() && segment.bpm > 0.0 { segment.bpm } else { 1.0 };
        elapsed_us += (next - cursor) as f64 * bpm / 60.0;
        cursor = next;
    }
    elapsed_us
}

pub(super) fn quarter_note_elapsed_ms(
    timing_map: &TimingMap,
    bar_lines: &[BarLine],
    now: TimeUs,
) -> Option<i32> {
    if now.0 < 0 {
        return None;
    }
    let section_index = bar_lines.partition_point(|bar| bar.time <= now).checked_sub(1);
    let section_tick = section_index.map_or(0, |index| bar_lines[index].tick.0);
    let next_tick = section_index
        .and_then(|index| bar_lines.get(index + 1))
        .map_or_else(|| section_tick.saturating_add(TICKS_PER_MEASURE as u64), |bar| bar.tick.0);
    let section_length = next_tick.saturating_sub(section_tick).max(1);
    let cursor_tick = timing_map.time_to_tick_f64(now).max(section_tick as f64);
    let quarter_width = section_length as f64 / 4.0;
    let quarter_index = ((cursor_tick - section_tick as f64) / quarter_width).floor().max(0.0);
    let quarter_tick = section_tick
        .saturating_add((quarter_index * quarter_width).round().clamp(0.0, u64::MAX as f64) as u64);
    let quarter_time = timing_map.tick_to_time(ChartTick(quarter_tick));
    Some(((now.0 - quarter_time.0) / 1_000).clamp(0, i32::MAX as i64) as i32)
}

/// 実時間を区間 BPM / 60 倍して進める。STOP 中 (次の区間の開始より前) は進めない。
fn bpm_normalized_moving_elapsed_us(timing_map: &TimingMap, start: TimeUs, end: TimeUs) -> f64 {
    let mut cursor = start.0.max(0).min(end.0);
    let end = end.0.max(cursor);
    let mut elapsed_us = 0.0;
    while cursor < end {
        let segment = timing_map.find_segment_by_time(TimeUs(cursor));
        let in_stop = segment.start_time.0 > cursor;
        let boundary = if in_stop { segment.start_time.0 } else { segment.end_time.0 };
        let next = end.min(boundary.max(cursor.saturating_add(1)));
        if !in_stop {
            let bpm = if segment.bpm.is_finite() && segment.bpm > 0.0 { segment.bpm } else { 1.0 };
            elapsed_us += (next - cursor) as f64 * bpm / 60.0;
        }
        cursor = next;
    }
    elapsed_us
}

/// beatoraja LaneRenderer の PMS 見逃し POOR と同じく、ノーツの STOP 終了から
/// `delay_us` 後に、ハイスピードによらず1小節でレーン高さぶん落とす。STOP 中は止める。
pub(super) fn pms_missed_note_fall_progress(
    timing_map: &TimingMap,
    note_tick: ChartTick,
    note_time: TimeUs,
    delay_us: i64,
    now: TimeUs,
) -> f32 {
    let stop_end = timing_map
        .segments
        .iter()
        .filter(|segment| segment.start_tick == note_tick)
        .map(|segment| segment.start_time.0)
        .max()
        .unwrap_or(note_time.0);
    let fall_start = TimeUs(stop_end.saturating_add(delay_us.max(0)));
    (bpm_normalized_moving_elapsed_us(timing_map, fall_start, now) / 4_000_000.0) as f32
}

pub(super) fn lane_key_timer_ms(
    started_at: Option<TimeUs>,
    chart_time: TimeUs,
    play_elapsed: TimeUs,
) -> Option<i32> {
    let started_at = started_at?;
    let now = lane_key_timer_now(started_at, chart_time, play_elapsed)?;
    Some(skin_timer_elapsed_ms(now, started_at))
}

pub(super) fn lane_keyon_ms(
    session: &GameSession,
    chart_time: TimeUs,
    play_elapsed: TimeUs,
) -> [Option<i32>; LANE_COUNT] {
    std::array::from_fn(|lane_index| {
        lane_key_timer_ms(session.lane_keyon_started_at[lane_index], chart_time, play_elapsed)
    })
}

pub(super) fn lane_keyoff_ms(
    session: &GameSession,
    chart_time: TimeUs,
    play_elapsed: TimeUs,
) -> [Option<i32>; LANE_COUNT] {
    std::array::from_fn(|lane_index| {
        lane_key_timer_ms(session.lane_keyoff_started_at[lane_index], chart_time, play_elapsed)
    })
}

/// `play_elapsed_time` 更新後に keybeam / turntable 向け snapshot フィールドを再計算する。
pub fn refresh_play_skin_visuals(snapshot: &mut RenderSnapshot, session: &GameSession) {
    snapshot.skin_offsets =
        skin_offsets_from_session(session, snapshot.time, snapshot.play_elapsed_time);
    snapshot.keyon_ms = lane_keyon_ms(session, snapshot.time, snapshot.play_elapsed_time);
    snapshot.keyoff_ms = lane_keyoff_ms(session, snapshot.time, snapshot.play_elapsed_time);
    snapshot.gauge_increase_elapsed_ms =
        optional_skin_timer_elapsed_ms(snapshot.time, session.gauge_increase_started_at);
    snapshot.gauge_max_elapsed_ms =
        optional_skin_timer_elapsed_ms(snapshot.time, session.gauge_max_started_at);
}

/// 通常アニメーション用の `play_elapsed_time` と、押下エフェクト用の実経過時間が
/// 異なる pre-READY 待機中に keybeam / turntable 向けフィールドを再計算する。
pub fn refresh_play_skin_visuals_with_input_elapsed(
    snapshot: &mut RenderSnapshot,
    session: &GameSession,
    input_elapsed: TimeUs,
) {
    snapshot.skin_offsets = skin_offsets_from_session(session, snapshot.time, input_elapsed);
    snapshot.keyon_ms = std::array::from_fn(|lane_index| {
        session.lane_keyon_started_at[lane_index]
            .map(|started_at| skin_timer_elapsed_ms(input_elapsed, started_at))
    });
    snapshot.keyoff_ms = std::array::from_fn(|lane_index| {
        session.lane_keyoff_started_at[lane_index]
            .map(|started_at| skin_timer_elapsed_ms(input_elapsed, started_at))
    });
    snapshot.gauge_increase_elapsed_ms =
        optional_skin_timer_elapsed_ms(snapshot.time, session.gauge_increase_started_at);
    snapshot.gauge_max_elapsed_ms =
        optional_skin_timer_elapsed_ms(snapshot.time, session.gauge_max_started_at);
}

pub fn refresh_pending_play_input_visuals(
    snapshot: &mut RenderSnapshot,
    key_mode: KeyMode,
    lane_keyon_started_at: [Option<TimeUs>; LANE_COUNT],
    lane_keyoff_started_at: [Option<TimeUs>; LANE_COUNT],
    lane_scratch_angle_delta_ms: [i64; LANE_COUNT],
    input_elapsed: TimeUs,
) {
    snapshot.keyon_ms = std::array::from_fn(|lane_index| {
        lane_keyon_started_at[lane_index]
            .map(|started_at| skin_timer_elapsed_ms(input_elapsed, started_at))
    });
    snapshot.keyoff_ms = std::array::from_fn(|lane_index| {
        lane_keyoff_started_at[lane_index]
            .map(|started_at| skin_timer_elapsed_ms(input_elapsed, started_at))
    });
    let active_lanes = key_mode.active_lanes();
    if active_lanes.contains(&Lane::Scratch) {
        set_scratch_angle_offset(
            &mut snapshot.skin_offsets,
            SCRATCH_ANGLE_OFFSET_1P,
            input_elapsed,
            lane_scratch_angle_delta_ms[Lane::Scratch.index()],
        );
    }
    if active_lanes.contains(&Lane::Scratch2) {
        set_scratch_angle_offset(
            &mut snapshot.skin_offsets,
            SCRATCH_ANGLE_OFFSET_2P,
            input_elapsed,
            lane_scratch_angle_delta_ms[Lane::Scratch2.index()],
        );
    }
}
