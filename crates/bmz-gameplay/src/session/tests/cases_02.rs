use super::*;

#[test]
#[ignore = "hold gauge timing on a chart with many long notes"]
fn measure_hold_gauge_updates() {
    let mut chart = chart_with_hcn_long_note();
    let first = chart.long_notes[0].clone();
    for n in 1..10000 {
        let mut pair = first.clone();
        pair.start_time = TimeUs(i64::from(n) * 1_000_000);
        pair.end_time = TimeUs(pair.start_time.0 + 1_000_000);
        pair.start_note_id = NoteId(n * 2 + 1);
        pair.end_note_id = NoteId(n * 2 + 2);
        chart.long_notes.push(pair);
    }
    let mut session = session_with_autoplay(chart);
    session.judge.judged_notes.insert(first.start_note_id, Judge::PGreat);
    update_hcn_lane_timers(&mut session, TimeUs(0));
    assert!(session.lane_hcn_timer[first.lane.index()].is_some());
    let mut times = Vec::new();
    for _ in 0..11 {
        session.last_hcn_gauge_at = Some(TimeUs(0));
        let start = std::time::Instant::now();
        for n in 1..=1000 {
            std::hint::black_box(apply_hcn_gauge(&mut session, TimeUs(n * 100)));
        }
        times.push(start.elapsed());
    }
    times.sort();
    eprintln!("1000 gauge updates median {:?}", times[5]);
}

#[test]
fn advance_session_frame_starts_full_combo_timer_after_last_note() {
    let mut session = session_with_autoplay(chart_with_keysound());
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    assert_eq!(session.full_combo_started_at, Some(TimeUs(0)));
}

#[test]
fn scored_note_count_uses_effective_long_note_mode() {
    let mut chart = chart_with_hcn_long_note();
    assert_eq!(scored_note_count(&chart), 2);

    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    assert_eq!(scored_note_count(&chart), 2);

    chart.long_notes[0].mode = Some(LongNoteMode::Ln);
    assert_eq!(scored_note_count(&chart), 1);

    chart.long_notes[0].mode = None;
    chart.metadata.long_note_mode = LongNoteMode::Hcn;
    assert_eq!(scored_note_count(&chart), 2);
}

#[test]
fn full_combo_timer_waits_for_cn_end_judgement() {
    let mut chart = chart_with_hcn_long_note();
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut session = session_with_autoplay(chart);

    let start_events = apply_judge_outcome(
        &mut session,
        JudgeOutcome { events: vec![judgement_event(Judge::PGreat, 0)], ..Default::default() },
    );
    update_full_combo_timer(&mut session, &start_events);

    assert_eq!(session.scored_total_notes, 2);
    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.full_combo_started_at, None);

    let mut end_event = judgement_event(Judge::PGreat, 0);
    end_event.note_id = Some(NoteId(2));
    end_event.time = TimeUs(1_000_000);
    let end_events = apply_judge_outcome(
        &mut session,
        JudgeOutcome { events: vec![end_event], ..Default::default() },
    );
    update_full_combo_timer(&mut session, &end_events);

    assert_eq!(session.score.past_notes, 2);
    assert_eq!(session.full_combo_started_at, Some(TimeUs(1_000_000)));
}

#[test]
fn hard_gauge_zero_moves_session_to_failed() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.state = PlayState::Playing;
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 1000);
    session
        .gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == bmz_core::clear::GaugeType::Hard)
        .unwrap()
        .value = 0.0;

    update_failed_state_from_gauge(&mut session);

    assert_eq!(session.state, PlayState::Failed);
}

#[test]
fn gauge_increase_timer_starts_when_judge_raises_gauge() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, 200);
    session.gauge.set_initial_value(50.0);

    apply_judge_outcome(
        &mut session,
        JudgeOutcome {
            events: vec![JudgementEvent {
                time: TimeUs(123_000),
                ..judgement_event(Judge::PGreat, 0)
            }],
            ..Default::default()
        },
    );

    assert_eq!(session.gauge_increase_started_at, Some(TimeUs(123_000)));
}

#[test]
fn gauge_increase_timer_stops_when_gauge_reaches_max() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.gauge_increase_started_at = Some(TimeUs(10_000));
    session.gauge.set_initial_value(100.0);

    update_gauge_increase_timer(&mut session, 99.0, TimeUs(123_000));
    assert_eq!(session.gauge_increase_started_at, None);

    update_gauge_max_timer(&mut session, TimeUs(123_000));
    assert_eq!(session.gauge_max_started_at, Some(TimeUs(123_000)));
}

#[test]
fn gauge_max_timer_tracks_current_max_state() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.gauge.set_initial_value(99.0);

    update_gauge_max_timer(&mut session, TimeUs(25_000));
    assert_eq!(session.gauge_max_started_at, None);

    session.gauge.set_initial_value(100.0);
    update_gauge_max_timer(&mut session, TimeUs(50_000));
    assert_eq!(session.gauge_max_started_at, Some(TimeUs(50_000)));

    update_gauge_max_timer(&mut session, TimeUs(75_000));
    assert_eq!(session.gauge_max_started_at, Some(TimeUs(50_000)));

    session.gauge.set_initial_value(99.0);
    update_gauge_max_timer(&mut session, TimeUs(100_000));
    assert_eq!(session.gauge_max_started_at, None);
}

#[test]
fn course_combo_carry_extends_display_combo_without_changing_score_max() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.course_combo_carry = 100;
    session.course_combo_carry_active = true;
    session.course_max_combo = 100;

    apply_judge_outcome(
        &mut session,
        JudgeOutcome { events: vec![judgement_event(Judge::PGreat, 0)], ..Default::default() },
    );

    assert_eq!(session.score.combo, 1);
    assert_eq!(session.score.max_combo, 1);
    assert_eq!(session.display_combo(), 101);
    assert_eq!(session.display_max_combo(), 101);
    assert_eq!(session.recent_display_judgements[0].combo, 101);
}

#[test]
fn course_combo_carry_resets_on_combo_break() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.course_combo_carry = 100;
    session.course_combo_carry_active = true;
    session.course_max_combo = 100;

    apply_judge_outcome(
        &mut session,
        JudgeOutcome { events: vec![judgement_event(Judge::PGreat, 0)], ..Default::default() },
    );
    apply_judge_outcome(
        &mut session,
        JudgeOutcome { events: vec![judgement_event(Judge::Bad, 0)], ..Default::default() },
    );
    apply_judge_outcome(
        &mut session,
        JudgeOutcome { events: vec![judgement_event(Judge::Great, 0)], ..Default::default() },
    );

    assert!(!session.course_combo_carry_active);
    assert_eq!(session.score.combo, 1);
    assert_eq!(session.score.max_combo, 1);
    assert_eq!(session.display_combo(), 1);
    assert_eq!(session.display_max_combo(), 101);
    assert_eq!(
        session.recent_display_judgements.iter().map(|event| event.combo).collect::<Vec<_>>(),
        [101, 0, 1]
    );
}

#[test]
fn auto_shift_hard_zero_falls_back_without_failed_state() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.state = PlayState::Playing;
    session.gauge = GaugeState::new_auto_shift(160.0, 1000);

    session.gauge.apply_judge(Judge::Poor, 7.0);
    update_failed_state_from_gauge(&mut session);

    assert_eq!(session.state, PlayState::Playing);
    assert_eq!(session.gauge.selected, bmz_core::clear::GaugeType::Hard);
}

#[test]
fn hcn_gauge_increases_while_passing_and_pressed_until_end() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());
    session.gauge.set_initial_value(50.0);
    session.judge.judged_notes.insert(NoteId(1), Judge::PGreat);
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(0));

    update_hcn_lane_timers(&mut session, TimeUs(0));
    apply_hcn_gauge(&mut session, TimeUs(0));
    update_hcn_lane_timers(&mut session, TimeUs(500_000));
    apply_hcn_gauge(&mut session, TimeUs(500_000));
    let mid = session.gauge.current().value;
    // 終端通過後は passing が外れ、ゲージは変化しない。
    update_hcn_lane_timers(&mut session, TimeUs(2_000_000));
    apply_hcn_gauge(&mut session, TimeUs(2_000_000));
    update_hcn_lane_timers(&mut session, TimeUs(3_000_000));
    apply_hcn_gauge(&mut session, TimeUs(3_000_000));

    assert!(mid > 50.0);
    assert!((session.gauge.current().value - mid).abs() < f32::EPSILON);
}

#[test]
fn overlapping_hcn_recovery_and_damage_follow_their_chart_times() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());
    session.gauge.set_initial_value(100.0);
    session.last_hcn_gauge_at = Some(TimeUs(0));
    session.lane_hcn_timer[Lane::Key1.index()] =
        Some(HcnLaneTimer { inclease: true, since: TimeUs(0), passing_count_us: 190_000 });
    session.lane_hcn_timer[Lane::Key2.index()] =
        Some(HcnLaneTimer { inclease: false, since: TimeUs(0), passing_count_us: -199_500 });
    let mut expected = session.gauge.clone();
    expected.apply_hcn_drain(); // 501us
    expected.apply_hcn_hold(); // 10001us
    expected.apply_hcn_drain(); // 200501us
    apply_hcn_gauge(&mut session, TimeUs(201_000));
    assert_eq!(session.gauge.carry_values(), expected.carry_values());
}

#[test]
fn hcn_head_miss_uses_the_scratch_window_on_mapped_scratch_lanes() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());
    let mut windows = session.judge.window_set;
    windows.note.bad_slow_us = 120_000;
    windows.scratch.bad_slow_us = 180_000;
    session.judge.set_window_set(windows);
    session.judge.set_scratch_lane_mask(std::array::from_fn(|lane| lane == Lane::Key1.index()));
    let mut boundaries = Vec::new();
    session.judge.append_hcn_boundaries(
        &session.chart,
        TimeUs(0),
        TimeUs(300_000),
        &mut boundaries,
    );
    assert_eq!(boundaries, [TimeUs(180_001)]);
}

#[test]
fn hcn_passing_waits_until_the_head_is_judged() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());

    update_hcn_lane_timers(&mut session, TimeUs(100_000));
    assert!(session.lane_hcn_timer[Lane::Key1.index()].is_none());

    session.judge.judged_notes.insert(NoteId(1), Judge::Poor);
    update_hcn_lane_timers(&mut session, TimeUs(100_000));
    assert!(session.lane_hcn_timer[Lane::Key1.index()].is_some());
}

#[test]
fn hcn_gauge_recovers_when_pressed_after_missed_start() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());
    session.gauge.set_initial_value(50.0);
    // 始端を見逃し (POOR) て離している → 減衰。
    session.judge.judged_notes.insert(NoteId(1), Judge::Poor);

    update_hcn_lane_timers(&mut session, TimeUs(100_000));
    apply_hcn_gauge(&mut session, TimeUs(100_000));
    // 250ms 経過 → mpassingcount が -200ms を下回り減衰 1 tick。
    update_hcn_lane_timers(&mut session, TimeUs(350_000));
    apply_hcn_gauge(&mut session, TimeUs(350_000));
    let drained = session.gauge.current().value;
    assert!(drained < 50.0);

    // 途中から押し直すと回復に転じる (beatoraja passing ベース)。
    // カウンタは反転でリセットされないため、残り -50ms を打ち消して
    // +200ms を超えるまで押し続けると回復 1 tick が入る。
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(350_000));
    update_hcn_lane_timers(&mut session, TimeUs(800_000));
    apply_hcn_gauge(&mut session, TimeUs(800_000));

    assert!(session.gauge.current().value > drained);
}

#[test]
fn hcn_keysound_mutes_on_release_after_early_end_judge() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());
    session.judge.judged_notes.insert(NoteId(1), Judge::PGreat);
    // 早離しで終端が BAD 判定済み、キーは離している。
    session.judge.judged_notes.insert(NoteId(2), Judge::Bad);

    update_hcn_lane_timers(&mut session, TimeUs(500_000));
    assert_eq!(session.pending_keysound_volumes, vec![(SoundId(7), 0.0)]);

    // 押し直すと元の音量へ復帰する。同一状態の継続では再送しない。
    session.pending_keysound_volumes.clear();
    update_hcn_lane_timers(&mut session, TimeUs(600_000));
    assert!(session.pending_keysound_volumes.is_empty());
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(700_000));
    update_hcn_lane_timers(&mut session, TimeUs(700_000));
    assert_eq!(session.pending_keysound_volumes.len(), 1);
    assert_eq!(session.pending_keysound_volumes[0].0, SoundId(7));
    assert!(session.pending_keysound_volumes[0].1 > 0.0);
}

#[test]
fn hcn_keysound_volume_untouched_while_end_unjudged() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());
    session.judge.judged_notes.insert(NoteId(1), Judge::PGreat);
    // 終端未判定で離していても音量は触らない (beatoraja: pair state > 3 のみ)。
    update_hcn_lane_timers(&mut session, TimeUs(500_000));
    assert!(session.pending_keysound_volumes.is_empty());
}

#[test]
fn ln_mode_scores_once_at_long_note_end() {
    let mut chart = chart_with_hcn_long_note();
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.long_notes[0].mode = Some(LongNoteMode::Ln);
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;

    let press = session.judge.process_input(
        &session.chart,
        InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(0),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        },
    );
    apply_judge_outcome(&mut session, press);
    assert_eq!(session.score.past_notes, 0);
    assert_eq!(session.score.combo, 0);

    let end = session.judge.process_misses(&session.chart, TimeUs(1_000_001));
    apply_judge_outcome(&mut session, end);

    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.score.combo, 1);
    assert_eq!(session.score.ex_score(), 2);
}

#[test]
fn process_mine_passes_applies_damage_for_held_human_lane() {
    let mut session = session_with_autoplay(chart_with_mine(TimeUs(1_000_000), 8.0));
    session.autoplay = None;
    session.gauge.set_initial_value(50.0);
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));

    let events = process_mine_passes(&mut session, TimeUs(1_000_000));

    assert!(events.is_empty());
    assert_eq!(session.pending_mine_hits.len(), 1);
    assert_eq!(session.pending_mine_hits[0].note_id, NoteId(7));
    assert!((session.gauge.current().value - 42.0).abs() < f32::EPSILON);
}

#[test]
fn process_mine_passes_preserves_fractional_damage() {
    let mut session = session_with_autoplay(chart_with_mine(TimeUs(1_000_000), 8.5));
    session.autoplay = None;
    session.gauge.set_initial_value(50.0);
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));

    process_mine_passes(&mut session, TimeUs(1_000_000));

    assert_eq!(session.pending_mine_hits[0].damage, 8.5);
    assert!((session.gauge.current().value - 41.5).abs() < f32::EPSILON);
}
