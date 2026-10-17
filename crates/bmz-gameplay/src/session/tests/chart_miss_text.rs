use super::*;

fn event_at(judge: Judge, time_us: i64, affects_score: bool) -> JudgementEvent {
    JudgementEvent { time: TimeUs(time_us), affects_score, ..judgement_event(judge, 0) }
}

#[test]
fn chart_miss_text_tracks_the_latest_misslayer_judgement() {
    let mut session = session_with_autoplay(chart_with_keysound());
    let mut update = |events: &[JudgementEvent], now: i64| {
        update_recent_judgements(&mut session, events, TimeUs(now));
        session.last_miss_judgement_time
    };
    assert_eq!(update(&[], 0), None);
    // GOOD and better, and empty POOR, do not show the misslayer nor the miss message.
    let ignored = [
        event_at(Judge::PGreat, 10_000, true),
        event_at(Judge::Great, 11_000, true),
        event_at(Judge::Good, 12_000, true),
        event_at(Judge::EmptyPoor, 13_000, true),
    ];
    assert_eq!(update(&ignored, 20_000), None);
    // Display-only events (e.g. the LN-mode start) are ignored.
    assert_eq!(update(&[event_at(Judge::Bad, 21_000, false)], 21_000), None);
    assert_eq!(update(&[event_at(Judge::Bad, 30_000, true)], 30_000), Some(TimeUs(30_000)));
    // The latest miss wins and is kept after the judgement display expires.
    let misses = [event_at(Judge::Poor, 50_000, true), event_at(Judge::Bad, 40_000, true)];
    assert_eq!(update(&misses, 50_000), Some(TimeUs(50_000)));
    assert_eq!(update(&[event_at(Judge::EmptyPoor, 60_000, true)], 60_000), Some(TimeUs(50_000)));
    assert_eq!(update(&[], 50_000 + JUDGEMENT_DISPLAY_US * 10), Some(TimeUs(50_000)));
}

#[test]
fn chart_miss_text_tracks_the_latest_good_or_better_for_hiding() {
    let mut session = session_with_autoplay(chart_with_keysound());
    let mut update = |events: &[JudgementEvent], now: i64| {
        update_recent_judgements(&mut session, events, TimeUs(now));
        session.last_good_judgement_time
    };
    assert_eq!(update(&[event_at(Judge::Bad, 10_000, true)], 10_000), None);
    assert_eq!(update(&[event_at(Judge::EmptyPoor, 11_000, true)], 11_000), None);
    // Display-only events are ignored, as for the miss time.
    assert_eq!(update(&[event_at(Judge::Good, 12_000, false)], 12_000), None);
    let hits = [event_at(Judge::Great, 30_000, true), event_at(Judge::PGreat, 20_000, true)];
    assert_eq!(update(&hits, 30_000), Some(TimeUs(30_000)));
    assert_eq!(update(&[event_at(Judge::Good, 40_000, true)], 40_000), Some(TimeUs(40_000)));
}

#[test]
fn chart_miss_text_uses_the_misslayer_judgements() {
    for (judge, expected) in [
        (Judge::PGreat, false),
        (Judge::Great, false),
        (Judge::Good, false),
        (Judge::Bad, true),
        (Judge::Poor, true),
        (Judge::EmptyPoor, false),
    ] {
        assert_eq!(is_misslayer_judge(judge), expected, "{judge:?}");
    }
}

#[test]
fn missed_notes_set_the_miss_time_but_empty_poor_inputs_do_not() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.autoplay = None;
    let events = process_misses(&mut session, TimeUs(130_000));
    assert_eq!(events[0].judge, Judge::Poor);
    update_recent_judgements(&mut session, &events, TimeUs(130_000));
    assert_eq!(session.last_miss_judgement_time, Some(events[0].time));

    let mut session = session_with_autoplay(chart_with_keysound());
    session.autoplay = None;
    let events = process_session_input(&mut session, human_press(TimeUs(150_000)));
    assert_eq!(events[0].judge, Judge::EmptyPoor);
    update_recent_judgements(&mut session, &events, TimeUs(150_000));
    assert_eq!(session.last_miss_judgement_time, None);
}
