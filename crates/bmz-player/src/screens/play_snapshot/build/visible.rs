use super::super::projection::PlayfieldView;
use super::*;

pub(in crate::screens::play_snapshot) fn populate_visible_playfield(
    snapshot: &mut RenderSnapshot,
    session: &PlayfieldView<'_>,
    chart_now: TimeUs,
    cache: &PlayRenderSnapshotCache,
    lane_render_now: TimeUs,
) {
    // playstart 中は見た目の基準だけ 0 に固定し、音声時刻は負値のまま維持する。
    if lane_render_now.0 < 0 {
        return;
    }
    let scroll_time = scroll_render_time(lane_render_now);
    let scroll = ScrollContext {
        timing_map: session.timing_map,
        hispeed: session.hispeed,
        visible_lane_fraction: crate::config::play::visible_lane_fraction(0.0, session.lift),
        lookahead_ticks: TICKS_PER_MEASURE as f64,
        scroll_integral: &cache.scroll_integral,
        speed: render_speed(&cache.speed_segments, session.constant_enabled, scroll_time),
    };
    let cursor_tick = scroll.cursor_tick(scroll_time);
    let tick_upper_bound = scroll.simple_tick_upper_bound(cursor_tick);

    if snapshot.judge_area {
        snapshot.judge_area_key_y =
            judge_area_edges(&scroll, cursor_tick, lane_render_now, session.judge.window_set.note);
        snapshot.judge_area_scratch_y = judge_area_edges(
            &scroll,
            cursor_tick,
            lane_render_now,
            session.judge.window_set.scratch,
        );
    }

    populate_visible_notes(
        snapshot,
        session,
        lane_render_now,
        &scroll,
        cursor_tick,
        tick_upper_bound,
    );
    populate_visible_guide_lines(
        snapshot,
        session,
        scroll_time,
        &scroll,
        cursor_tick,
        tick_upper_bound,
    );
    populate_visible_long_notes(
        snapshot,
        session,
        chart_now,
        cache,
        scroll_time,
        &scroll,
        cursor_tick,
        tick_upper_bound,
    );
}

fn populate_visible_notes(
    snapshot: &mut RenderSnapshot,
    session: &PlayfieldView<'_>,
    lane_render_now: TimeUs,
    scroll: &ScrollContext<'_>,
    cursor_tick: f64,
    tick_upper_bound: Option<f64>,
) {
    // 9KEYS は既定 skin も見逃しノーツを落とす。それ以外は skin の DST_NOTE2 がある場合だけ残す。
    let keep_missed_notes = snapshot.key_mode == KeyMode::K9 || session.missed_note_fall;
    let note_lower_time = (!keep_missed_notes).then_some(lane_render_now);
    for lane in Lane::ALL {
        let retention_lower_index =
            session.note_retention.then_some(session.judge.lanes[lane.index()].next_note_index);
        for note in visible_lane_notes(
            session.chart.notes_for_lane(lane),
            note_lower_time,
            retention_lower_index,
            tick_upper_bound,
        ) {
            let Some(alpha) = constant_object_alpha(session, lane_render_now, note.time) else {
                continue;
            };
            let processed_judge = session.judge.judged_notes.get(&note.id).copied();
            // beatoraja LaneRenderer の PMS 見逃し POOR 描画と同じく、state 0 と BAD/POOR を対象にする。
            let pms_missed = keep_missed_notes
                && note.kind == NoteKind::Tap
                && matches!(processed_judge, None | Some(Judge::Bad | Judge::Poor))
                && note.time < lane_render_now;
            let retained_at_judge_line = !pms_missed
                && session.note_retention
                && note.kind == NoteKind::Tap
                && processed_judge.is_none()
                && note.time < lane_render_now;
            if note.time < lane_render_now && !retained_at_judge_line && !pms_missed {
                continue;
            }
            match note.kind {
                NoteKind::Invisible | NoteKind::LongStart | NoteKind::LongEnd => {}
                NoteKind::Mine => {
                    if let Some(y) = scroll.note_y(note.time, cursor_tick) {
                        snapshot.visible_mines[lane.index()].push(VisibleMine {
                            lane,
                            time: note.time,
                            y,
                            alpha,
                            damage: note.damage.unwrap_or(0.0),
                        });
                    }
                }
                NoteKind::Tap => {
                    let pms_miss_fall = if pms_missed {
                        let Some(fall) = pms_missed_tap_fall(session, note, lane_render_now) else {
                            continue;
                        };
                        Some(fall)
                    } else {
                        None
                    };
                    let y = match pms_miss_fall {
                        Some(fall) => Some(-fall),
                        None if retained_at_judge_line => Some(0.0),
                        None => scroll.note_y(note.time, cursor_tick),
                    };
                    if let Some(y) = y {
                        snapshot.visible_notes[lane.index()].push(VisibleNote {
                            lane,
                            time: note.time,
                            y,
                            alpha,
                            kind: NoteVisualKind::Tap,
                            processed_judge,
                            pms_miss_fall,
                        });
                    }
                }
            }
        }
    }
}

/// 見逃しノーツを snapshot に残す落下量の上限。消える位置は skin の `DST_NOTE2` で
/// 決まるため、ここでは十分に落ち切ったノーツだけを除く。
const PMS_MISSED_NOTE_MAX_FALL_MEASURES: f32 = 8.0;

fn pms_missed_tap_fall(
    session: &PlayfieldView<'_>,
    note: &NoteEvent,
    lane_render_now: TimeUs,
) -> Option<f32> {
    // beatoraja は遅い側の空POOR境界 (PMS 175ms) まで判定ライン上に留め、その後に落とす。
    let fall = pms_missed_note_fall_progress(
        session.timing_map,
        note.tick,
        note.time,
        session.judge.window_set.note.empty_poor_slow_us.max(0),
        lane_render_now,
    );
    (fall <= PMS_MISSED_NOTE_MAX_FALL_MEASURES).then_some(fall)
}

fn populate_visible_guide_lines(
    snapshot: &mut RenderSnapshot,
    session: &PlayfieldView<'_>,
    scroll_time: TimeUs,
    scroll: &ScrollContext<'_>,
    cursor_tick: f64,
    tick_upper_bound: Option<f64>,
) {
    let tick_range = tick_upper_bound.map(|upper| (cursor_tick, upper));
    for bar in visible_bar_lines(&session.chart.bar_lines, scroll_time, tick_range) {
        let Some(alpha) = constant_object_alpha(session, scroll_time, bar.time) else {
            continue;
        };
        if let Some(y) = scroll.note_y(bar.time, cursor_tick) {
            snapshot.bar_lines.push(VisibleBarLine {
                time: bar.time,
                y,
                alpha,
                label: String::new(),
            });
        }
    }
    for event in visible_timing_events(&session.chart.timing_events, scroll_time, tick_range) {
        let Some(alpha) = constant_object_alpha(session, scroll_time, event.time) else {
            continue;
        };
        let Some(y) = scroll.note_y(event.time, cursor_tick) else {
            continue;
        };
        let label = match event.kind {
            TimingEventKind::BpmChange { bpm } => format!("BPM{}", bpm as i32),
            TimingEventKind::Stop { duration_us } => format!("STOP {}ms", duration_us / 1_000),
        };
        let line = VisibleBarLine { time: event.time, y, alpha, label };
        match event.kind {
            TimingEventKind::BpmChange { .. } => snapshot.bpm_lines.push(line),
            TimingEventKind::Stop { .. } => snapshot.stop_lines.push(line),
        }
    }

    let end_second = (session.chart.end_time.0.max(0) / 1_000_000).min(21_600);
    for second in
        visible_time_line_seconds(session.timing_map, end_second, scroll_time, tick_upper_bound)
    {
        let time = TimeUs(second.saturating_mul(1_000_000));
        let Some(alpha) = constant_object_alpha(session, scroll_time, time) else {
            continue;
        };
        if let Some(y) = scroll.note_y(time, cursor_tick) {
            snapshot.time_lines.push(VisibleBarLine {
                time,
                y,
                alpha,
                label: format!("{:.1}s", time.0 as f64 / 1_000_000.0),
            });
        }
    }
}

fn judge_area_edges(
    scroll: &ScrollContext<'_>,
    cursor_tick: f64,
    now: TimeUs,
    window: bmz_gameplay::judge::model::JudgeWindow,
) -> [f32; 5] {
    [
        window.pgreat_us,
        window.great_us,
        window.good_us,
        window.bad_fast_us,
        window.empty_poor_fast_us.max(window.bad_fast_us),
    ]
    .map(|offset| {
        scroll
            .note_y(TimeUs(now.0.saturating_add(offset)), cursor_tick)
            .unwrap_or_default()
            .clamp(0.0, 1.0)
    })
}

fn populate_visible_long_notes(
    snapshot: &mut RenderSnapshot,
    session: &PlayfieldView<'_>,
    chart_now: TimeUs,
    cache: &PlayRenderSnapshotCache,
    scroll_time: TimeUs,
    scroll: &ScrollContext<'_>,
    cursor_tick: f64,
    tick_upper_bound: Option<f64>,
) {
    let tick_range = tick_upper_bound.map(|upper| (cursor_tick, upper));
    for (pair_index, long) in visible_long_notes(
        &session.chart.long_notes,
        &cache.long_note_prefix_max_end_times,
        scroll_time,
        tick_range,
    ) {
        let Some(alpha) = constant_object_alpha(session, scroll_time, long.start_time) else {
            continue;
        };
        let head = scroll.note_progress(long.start_time, cursor_tick);
        let tail = scroll.note_progress(long.end_time, cursor_tick);
        if tail < 0.0 || head > 1.0 {
            continue;
        }
        let mode = long.mode.unwrap_or(session.chart.metadata.long_note_mode);
        snapshot.visible_long_notes.push(VisibleLongNote {
            lane: long.lane,
            mode,
            head_y: head.clamp(0.0, 1.0),
            tail_y: tail.clamp(0.0, 1.0),
            alpha,
            body_state: long_body_state(session, chart_now, pair_index, long, mode),
        });
    }
}

pub(super) fn constant_object_alpha(
    session: &PlayfieldView<'_>,
    render_now: TimeUs,
    object_time: TimeUs,
) -> Option<f32> {
    if !session.constant_enabled {
        return Some(1.0);
    }
    let duration_ms =
        crate::config::play::duration_ms_from_green_number(session.target_green_number.max(1));
    let target = render_now.0.saturating_add(i64::from(duration_ms).saturating_mul(1_000));
    let difference = object_time.0.saturating_sub(target);
    let fade_us = i64::from(session.constant_fade_ms).saturating_mul(1_000);
    if fade_us >= 0 {
        if difference < 0 {
            Some(1.0)
        } else if fade_us > 0 && difference < fade_us {
            Some(1.0 - difference as f32 / fade_us as f32)
        } else {
            None
        }
    } else if difference >= 0 {
        None
    } else if difference > fade_us {
        Some((-difference) as f32 / (-fade_us) as f32)
    } else {
        Some(1.0)
    }
}

fn long_body_state(
    session: &PlayfieldView<'_>,
    chart_now: TimeUs,
    pair_index: usize,
    long: &bmz_chart::model::LongNotePair,
    mode: LongNoteMode,
) -> LongBodyState {
    let lane_index = long.lane.index();
    if session.judge.lanes[lane_index]
        .active_long
        .is_some_and(|active| active.pair_index == pair_index)
    {
        return LongBodyState::Processing;
    }
    if mode == LongNoteMode::Hcn
        && chart_now.0 >= long.start_time.0
        && chart_now.0 < long.end_time.0
        && let Some(timer) = session.lane_hcn_timer[lane_index]
    {
        if timer.inclease { LongBodyState::HcnActive } else { LongBodyState::HcnDamage }
    } else {
        LongBodyState::Inactive
    }
}

#[cfg(test)]
mod constant_tests {
    use std::sync::Arc;

    use crate::config::profile_config::ProfileConfig;
    use crate::screens::play_session::{PlaySessionOptions, build_game_session};

    use super::*;

    #[test]
    fn constant_window_supports_positive_and_negative_fades() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session = build_game_session(
            Arc::new(crate::screens::play_snapshot::tests::chart()),
            &profile,
            PlaySessionOptions::default(),
        );
        session.constant_enabled = true;
        session.target_green_number = 300;

        session.constant_fade_ms = 100;
        let view = PlayfieldView::from(&session);
        assert_eq!(constant_object_alpha(&view, TimeUs(0), TimeUs(499_000)), Some(1.0));
        assert_eq!(constant_object_alpha(&view, TimeUs(0), TimeUs(550_000)), Some(0.5));
        assert_eq!(constant_object_alpha(&view, TimeUs(0), TimeUs(600_000)), None);

        session.constant_fade_ms = -100;
        let view = PlayfieldView::from(&session);
        assert_eq!(constant_object_alpha(&view, TimeUs(0), TimeUs(399_000)), Some(1.0));
        assert_eq!(constant_object_alpha(&view, TimeUs(0), TimeUs(450_000)), Some(0.5));
        assert_eq!(constant_object_alpha(&view, TimeUs(0), TimeUs(500_000)), None);
    }
}
