use super::conditional::{advance, imported};
use super::*;
use bmz_audio::queue::ScheduledSoundQueue;

#[test]
fn free_zone_replaces_inner_judgements_with_one_score_and_display_event() {
    for body in ["#001SF:01", "#001SF:01\n#00116:01010000"] {
        let mut session = session_with_autoplay(imported(body));
        let mut audio = ScheduledSoundQueue::new();
        advance(&mut session, 5_000_000, &mut audio);
        assert_eq!(session.scored_total_notes, 1);
        assert_eq!(session.score.ex_score(), 2);
        assert_eq!(session.score.combo, 1);
        assert_eq!(session.score.bp(), 0);
        assert_eq!(session.result_judgements.len(), 1);
        let zone = &session.chart.metadata.free_zones[0];
        assert!(session.result_judgements.contains_key(&zone.note_id));
    }
}

#[test]
fn missed_inner_notes_do_not_add_bp_or_play_miss_keysounds() {
    let mut session = session_with_autoplay(imported("#001SF:01\n#00116:01010000"));
    session.autoplay = None;
    session.play_keysound_on_miss = true;
    let mut audio = ScheduledSoundQueue::new();
    advance(&mut session, 5_000_000, &mut audio);
    assert_eq!(session.score.bp(), 1);
    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.result_judgements.len(), 1);
    assert_eq!(audio.drain_all().count(), 0);
}

#[test]
fn scratch_in_empty_zone_gives_good_without_empty_poor_or_fallback_sound() {
    let mut session = session_with_autoplay(imported("#001SF:01\n#00316:01"));
    session.autoplay = None;
    session
        .conditional
        .pending_inputs
        .push(InputEvent { lane: Lane::Scratch, ..human_press(TimeUs(2_200_000)) });
    let mut audio = ScheduledSoundQueue::new();
    advance(&mut session, 3_000_000, &mut audio);
    assert_eq!(session.score.total_good(), 1);
    assert_eq!(session.score.bp(), 0);
    assert_eq!(audio.drain_all().count(), 0);
}

#[test]
fn potion_crossing_uses_input_timestamps_even_in_one_large_frame() {
    for step in [1_000, 3_000_000] {
        let mut session = session_with_autoplay(imported("#001F1:0A\n#00312:01"));
        session.autoplay = None;
        session.gauge.set_initial_value(20.0);
        session
            .conditional
            .pending_inputs
            .extend([human_press(TimeUs(1_900_000)), human_release(TimeUs(2_100_000))]);
        let mut audio = ScheduledSoundQueue::new();
        let mut time = 0;
        while time < 3_000_000 {
            advance(&mut session, time, &mut audio);
            time += step;
        }
        advance(&mut session, 3_000_000, &mut audio);
        assert_eq!(session.gauge.current().value, 30.0);
        assert_eq!(session.score.ex_score(), 0);
        assert_eq!(session.score.past_notes, 0);
        assert_eq!(audio.drain_all().count(), 0);
    }
}

#[test]
fn potion_does_not_revive_zero_gauge_and_autoplay_does_not_collect_it() {
    let mut session = session_with_autoplay(imported("#001F1:0A\n#00312:01"));
    session.gauge.set_initial_value(20.0);
    advance(&mut session, 3_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(session.gauge.current().value, 20.0);
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 1);
    session.gauge.set_initial_value(0.0);
    session.gauge.apply_potion(100.0);
    assert_eq!(session.gauge.current().value, 0.0);
}

#[test]
fn dummy_notes_have_no_scoring_audio_or_hold_gauge() {
    let mut session = session_with_autoplay(imported(
        "#LNMODE 4\n#001H1:01\n#001J2:01\n#002J2:01\n#001L3:01\n#001N4:01",
    ));
    session.gauge.set_initial_value(50.0);
    let mut audio = ScheduledSoundQueue::new();
    advance(&mut session, 6_000_000, &mut audio);
    assert_eq!(session.scored_total_notes, 0);
    assert_eq!(session.score.ex_score(), 0);
    assert_eq!(session.score.bp(), 0);
    assert_eq!(session.gauge.current().value, 50.0);
    assert_eq!(audio.drain_all().count(), 0);
}
