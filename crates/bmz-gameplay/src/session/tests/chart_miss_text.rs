use super::*;

fn event_at(judge: Judge, time_us: i64, affects_score: bool) -> JudgementEvent {
    JudgementEvent { time: TimeUs(time_us), affects_score, ..judgement_event(judge, 0) }
}

#[test]
fn chart_miss_text_tracks_the_latest_poor_or_empty_poor() {
    let mut session = session_with_autoplay(chart_with_keysound());
    let mut update = |events: &[JudgementEvent], now: i64| {
        update_recent_judgements(&mut session, events, TimeUs(now));
        session.last_miss_judgement_time
    };
    assert_eq!(update(&[], 0), None);
    // BAD and better judgements do not show the miss message.
    let hits = [
        event_at(Judge::PGreat, 10_000, true),
        event_at(Judge::Great, 11_000, true),
        event_at(Judge::Good, 12_000, true),
        event_at(Judge::Bad, 13_000, true),
    ];
    assert_eq!(update(&hits, 20_000), None);
    // Display-only events (e.g. the LN-mode start) are ignored.
    assert_eq!(update(&[event_at(Judge::Poor, 21_000, false)], 21_000), None);
    assert_eq!(update(&[event_at(Judge::Poor, 30_000, true)], 30_000), Some(TimeUs(30_000)));
    // The latest miss wins and is kept after the judgement display expires.
    let misses = [event_at(Judge::EmptyPoor, 50_000, true), event_at(Judge::Poor, 40_000, true)];
    assert_eq!(update(&misses, 50_000), Some(TimeUs(50_000)));
    assert_eq!(update(&[], 50_000 + JUDGEMENT_DISPLAY_US * 10), Some(TimeUs(50_000)));
}

#[test]
fn missed_notes_and_empty_poor_inputs_set_the_miss_time() {
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
    assert_eq!(session.last_miss_judgement_time, Some(TimeUs(150_000)));
}
