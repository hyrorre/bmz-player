use super::*;

pub(super) fn skin_offsets_from_session(
    session: &GameSession,
    chart_time: TimeUs,
    play_elapsed: TimeUs,
) -> SkinOffsetValues {
    let mut values = SkinOffsetValues::default();
    for offset in &session.skin_offsets {
        values.set(
            offset.id,
            SkinOffsetValue {
                x: offset.x,
                y: offset.y,
                w: offset.w,
                h: offset.h,
                r: offset.r,
                a: offset.a,
            },
        );
    }
    set_scratch_angle_offsets(
        &mut values,
        session.chart.metadata.key_mode,
        skin_visual_time(chart_time, play_elapsed),
        &session.lane_scratch_angle_delta_ms,
    );
    values
}

pub(super) fn set_scratch_angle_offsets(
    values: &mut SkinOffsetValues,
    key_mode: KeyMode,
    visual_time: TimeUs,
    lane_scratch_angle_delta_ms: &[i64; LANE_COUNT],
) {
    let active_lanes = key_mode.active_lanes();
    if active_lanes.contains(&Lane::Scratch) {
        set_scratch_angle_offset(
            values,
            SCRATCH_ANGLE_OFFSET_1P,
            visual_time,
            lane_scratch_angle_delta_ms[Lane::Scratch.index()],
        );
    }
    if active_lanes.contains(&Lane::Scratch2) {
        set_scratch_angle_offset(
            values,
            SCRATCH_ANGLE_OFFSET_2P,
            visual_time,
            lane_scratch_angle_delta_ms[Lane::Scratch2.index()],
        );
    }
}

pub(super) fn set_scratch_angle_offset(
    values: &mut SkinOffsetValues,
    offset_id: i32,
    visual_time: TimeUs,
    input_delta_ms: i64,
) {
    let mut offset = values.get(offset_id).unwrap_or_default();
    offset.r = scratch_angle_degrees(visual_time, input_delta_ms);
    values.set(offset_id, offset);
}

pub(super) fn scratch_angle_degrees(visual_time: TimeUs, input_delta_ms: i64) -> i32 {
    let elapsed_ms = (visual_time.0.max(0) / 1_000).rem_euclid(SCRATCH_ANGLE_PERIOD_MS);
    let base_ms = (SCRATCH_ANGLE_PERIOD_MS - elapsed_ms).rem_euclid(SCRATCH_ANGLE_PERIOD_MS);
    let angle_ms = (base_ms + input_delta_ms).rem_euclid(SCRATCH_ANGLE_PERIOD_MS);
    // Keep the offset in beatoraja's bottom-origin angle convention. The shared
    // skin renderer converts it to BMZ's top-origin convention together with
    // destination angles after applying all offsets.
    (angle_ms / SCRATCH_ANGLE_DEGREES_DIVISOR) as i32
}

pub(super) fn end_of_note_time(chart: &PlayableChart) -> TimeUs {
    TimeUs(
        chart
            .lane_notes
            .iter()
            .flat_map(|notes| {
                notes.iter().filter_map(|note| {
                    matches!(
                        note.kind,
                        NoteKind::Tap | NoteKind::LongStart | NoteKind::LongEnd | NoteKind::Mine
                    )
                    .then_some(note.time.0)
                })
            })
            .max()
            .unwrap_or(0),
    )
}

pub(super) fn end_of_note_elapsed_ms(render_now: TimeUs, end_time: TimeUs) -> Option<i32> {
    if end_time.0 <= 0 || render_now.0 < end_time.0 {
        return None;
    }
    Some(((render_now.0 - end_time.0) / 1_000).clamp(0, i32::MAX as i64) as i32)
}

/// beatoraja の LaneRenderer が `microtime += judgetiming` とする範囲にだけ
/// 表示オフセットを適用する。判定演出・BGA・skin timer は `chart_now` を使う。
pub(super) fn lane_render_time(session: &GameSession, chart_now: TimeUs) -> TimeUs {
    TimeUs(chart_now.0.saturating_add(session.offsets.visual_offset_us))
}
