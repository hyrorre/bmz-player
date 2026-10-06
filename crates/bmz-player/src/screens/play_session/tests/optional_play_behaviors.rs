use super::*;
use bmz_chart::model::{BgaEvent, BgaEventKind};
use bmz_core::time::ChartTick;

#[test]
fn optional_play_behaviors_reach_session_without_changing_score_chart_or_practice_range() {
    let mut chart = chart();
    chart.bgm_events.push(SoundEvent {
        tick: ChartTick(0),
        time: TimeUs(7_000_000),
        sound: SoundId(1),
    });
    chart.bga_events = vec![
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(8_000_000),
            asset: None,
            kind: BgaEventKind::Base,
        },
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(9_000_000),
            asset: None,
            kind: BgaEventKind::Layer,
        },
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(10_000_000),
            asset: None,
            kind: BgaEventKind::Layer2,
        },
        BgaEvent {
            tick: ChartTick(0),
            time: TimeUs(99_000_000),
            asset: None,
            kind: BgaEventKind::Poor,
        },
    ];
    let chart = Arc::new(chart);
    for enabled in [false, true] {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.play.wait_all_notes_result = enabled;
        profile.play.hide_misslayer_on_good = enabled;
        profile.play.play_keysound_on_miss = enabled;
        for practice in [false, true] {
            for replay in [false, true] {
                for autoplay in [false, true] {
                    let options = PlaySessionOptions {
                        session_mode: if practice {
                            SessionMode::Practice
                        } else {
                            SessionMode::Normal
                        },
                        autoplay,
                        replay_player: replay.then(|| ReplayPlayer {
                            events: Vec::new(),
                            next_index: 0,
                            next_scoring_time: None,
                        }),
                        initial_gauge_value: Some(50.0),
                        ..Default::default()
                    };
                    let session = build_game_session(chart.clone(), &profile, options);
                    assert_eq!(
                        session.result_wait_end_time,
                        (enabled && !practice).then_some(TimeUs(10_000_000))
                    );
                    assert_eq!(session.hide_misslayer_on_good, enabled);
                    assert_eq!(session.play_keysound_on_miss, enabled);
                    assert_eq!(session.chart.identity, chart.identity);
                    assert_eq!(session.chart.end_time, chart.end_time);
                    assert_eq!(session.assist.level, bmz_gameplay::session::AssistLevel::None);
                    assert_eq!(session.gauge.current().value, 50.0);
                }
            }
        }
    }
}

#[test]
fn result_wait_uses_bgm_and_hidden_notes_and_handles_empty_chart() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    profile.play.wait_all_notes_result = true;
    let mut chart = chart();
    assert_eq!(
        build_game_session(Arc::new(chart.clone()), &profile, Default::default())
            .result_wait_end_time,
        Some(TimeUs(0))
    );
    chart.bgm_events.push(SoundEvent {
        tick: ChartTick(0),
        time: TimeUs(7_000_000),
        sound: SoundId(1),
    });
    assert_eq!(
        build_game_session(Arc::new(chart.clone()), &profile, Default::default())
            .result_wait_end_time,
        Some(TimeUs(7_000_000))
    );
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(99),
        lane: Lane::Key1,
        kind: NoteKind::Invisible,
        tick: ChartTick(0),
        time: TimeUs(9_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });
    assert_eq!(
        build_game_session(Arc::new(chart), &profile, Default::default()).result_wait_end_time,
        Some(TimeUs(9_000_000))
    );
}
