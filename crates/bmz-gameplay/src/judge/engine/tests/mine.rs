use super::*;

#[test]
fn mine_hit_emits_event_with_damage() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = JudgeEngine::new(windows());

    let input = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    assert!(input.mine_hits.is_empty());
    let mut held = [None; LANE_COUNT];
    held[Lane::Key1.index()] = Some(TimeUs(1_000_000));
    let outcome = engine.process_mine_passes(&chart, TimeUs(1_000_000), &held);

    assert_eq!(outcome.mine_hits.len(), 1);
    assert_eq!(outcome.mine_hits[0].damage, 8.0);
    assert_eq!(outcome.mine_hits[0].note_id, NoteId(7));
    // Mine ヒットは通常判定とは別ベクタに入る。スコア対象ノーツが無いので
    // events は空、consumed_input も false のまま。
    assert!(outcome.events.is_empty());
    assert!(!outcome.consumed_input);
}

#[test]
fn mine_does_not_hit_outside_window() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = JudgeEngine::new(windows());

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));
    assert!(outcome.mine_hits.is_empty());
}

#[test]
fn mine_hit_does_not_double_fire() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = JudgeEngine::new(windows());

    let mut held = [None; LANE_COUNT];
    held[Lane::Key1.index()] = Some(TimeUs(999_999));
    let first = engine.process_mine_passes(&chart, TimeUs(1_000_000), &held);
    let second = engine.process_mine_passes(&chart, TimeUs(1_000_001), &held);

    assert_eq!(first.mine_hits.len(), 1);
    assert!(second.mine_hits.is_empty(), "same Mine must not fire twice");
}

#[test]
fn mine_pass_hits_when_lane_is_held() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = JudgeEngine::new(windows());
    let mut lane_keyon_started_at = [None; LANE_COUNT];
    lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));

    let outcome = engine.process_mine_passes(&chart, TimeUs(1_000_000), &lane_keyon_started_at);

    assert_eq!(outcome.mine_hits.len(), 1);
    assert_eq!(outcome.mine_hits[0].note_id, NoteId(7));
    assert_eq!(outcome.mine_hits[0].damage, 8.0);
}

#[test]
fn mine_pass_without_pressed_lane_is_skipped() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = JudgeEngine::new(windows());
    let lane_keyon_started_at = [None; LANE_COUNT];

    let outcome = engine.process_mine_passes(&chart, TimeUs(1_000_000), &lane_keyon_started_at);

    assert!(outcome.mine_hits.is_empty());
}

#[test]
fn absorbed_bss_press_skips_only_the_mine_on_its_edge() {
    let lane = Lane::Scratch;
    let mut chart = chart_with_lane_long_start(lane, TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    for (id, time) in [(7, 2_001_000), (8, 2_001_001), (9, 2_001_001)] {
        chart.lane_notes[lane.index()].push(NoteEvent {
            id: NoteId(id),
            lane,
            kind: NoteKind::Mine,
            tick: Default::default(),
            time: TimeUs(time),
            sound: None,
            layered_sounds: Vec::new(),
            damage: Some(3.0),
        });
    }
    let mut engine = JudgeEngine::new(windows());
    for input in [press_lane_at(lane, TimeUs(1_000_000)), release_lane_at(lane, TimeUs(2_000_000))]
    {
        engine.process_input(
            &chart,
            InputEvent { scratch_direction: Some(ScratchDirection::Down), ..input },
        );
    }
    let at = TimeUs(2_001_000);
    let reversed = engine.process_input(
        &chart,
        InputEvent { scratch_direction: Some(ScratchDirection::Up), ..press_lane_at(lane, at) },
    );
    assert!(reversed.events.is_empty() && reversed.keysounds.is_empty() && reversed.consumed_input);
    let mut held = [None; LANE_COUNT];
    held[lane.index()] = Some(at);
    assert!(engine.process_mine_passes(&chart, at, &held).mine_hits.is_empty());
    let later = TimeUs(at.0 + 1);
    let crossed = engine.process_mine_passes(&chart, later, &held);
    assert_eq!(
        crossed.mine_hits.iter().map(|mine| mine.note_id).collect::<Vec<_>>(),
        [NoteId(8), NoteId(9)]
    );
    assert!(engine.process_mine_passes(&chart, later, &held).mine_hits.is_empty());
}

#[test]
fn mine_pass_ignores_key_pressed_after_mine_time() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = JudgeEngine::new(windows());
    let mut lane_keyon_started_at = [None; LANE_COUNT];
    lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(1_050_000));

    let outcome = engine.process_mine_passes(&chart, TimeUs(1_100_000), &lane_keyon_started_at);

    assert!(outcome.mine_hits.is_empty());
}

#[test]
fn mine_does_not_hit_after_it_already_passed_unpressed() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = JudgeEngine::new(windows());
    let lane_keyon_started_at = [None; LANE_COUNT];
    engine.process_mine_passes(&chart, TimeUs(1_000_000), &lane_keyon_started_at);

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));

    assert!(outcome.mine_hits.is_empty());
}

#[test]
fn miss_is_reported_after_bad_window() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(windows());

    let still_candidate = engine.process_misses(&chart, TimeUs(1_110_000));
    let missed = engine.process_misses(&chart, TimeUs(1_130_000));

    assert!(still_candidate.events.is_empty());
    assert_eq!(missed.events.len(), 1);
    assert_eq!(missed.events[0].judge, Judge::Poor);
    assert_eq!(missed.events[0].side, TimingSide::Slow);
    assert_eq!(missed.events[0].note_id, Some(NoteId(1)));
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
}
