use super::*;
use bmz_chart::model::{ScrollEvent, SpeedEvent};
use bmz_chart::timing::TimingMap;
use bmz_core::time::ChartTick;

fn add_note(chart: &mut PlayableChart, id: u32, tick: u64, kind: NoteKind) {
    let timing =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    let mut event = note(id, Lane::Key1, timing.tick_to_time(ChartTick(tick)).0);
    event.tick = ChartTick(tick);
    event.kind = kind;
    chart.lane_notes[Lane::Key1.index()].push(event);
}

fn base_bpm(chart: &PlayableChart, option: HsFixOption) -> f64 {
    let timing =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    hsfix_base_bpm_for_chart(chart, &timing, option)
}

#[test]
fn hsfix_combines_scroll_and_interpolated_speed_at_notes() {
    let mut chart = chart();
    chart.scroll_events = vec![
        ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 },
        ScrollEvent { tick: ChartTick(48), time: TimeUs(25_000), factor: 0.5 },
    ];
    chart.speed_events = vec![
        SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 1.0 },
        SpeedEvent { tick: ChartTick(96), time: TimeUs(50_000), factor: 3.0 },
    ];
    for (id, tick) in [0, 24, 48, 72, 96, 120].into_iter().enumerate() {
        add_note(&mut chart, id as u32, tick, NoteKind::Tap);
    }
    assert_eq!(base_bpm(&chart, HsFixOption::MinBpm), 120.0);
    assert_eq!(base_bpm(&chart, HsFixOption::MaxBpm), 360.0);
    assert_eq!(base_bpm(&chart, HsFixOption::MainBpm), 180.0);
    assert_eq!(base_bpm(&chart, HsFixOption::Off), 120.0);
    assert_eq!(base_bpm(&chart, HsFixOption::StartBpm), 120.0);
}

#[test]
fn hsfix_counts_long_starts_once_and_excludes_ends_mines_and_invisible_notes() {
    let mut chart = chart();
    chart.scroll_events = vec![
        ScrollEvent { tick: ChartTick(48), time: TimeUs(25_000), factor: 2.0 },
        ScrollEvent { tick: ChartTick(96), time: TimeUs(50_000), factor: 4.0 },
    ];
    add_note(&mut chart, 1, 0, NoteKind::LongStart);
    add_note(&mut chart, 2, 0, NoteKind::LongStart);
    add_note(&mut chart, 3, 48, NoteKind::Tap);
    add_note(&mut chart, 4, 96, NoteKind::LongEnd);
    add_note(&mut chart, 5, 96, NoteKind::LongEnd);
    add_note(&mut chart, 6, 96, NoteKind::Mine);
    add_note(&mut chart, 7, 96, NoteKind::Invisible);
    for (start, end) in [(1, 4), (2, 5)] {
        chart.long_notes.push(bmz_chart::model::LongNotePair {
            lane: Lane::Key1,
            style: bmz_chart::model::LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(start),
            end_note_id: NoteId(end),
            start_tick: ChartTick(0),
            end_tick: ChartTick(96),
            start_time: TimeUs(0),
            end_time: TimeUs(50_000),
            sound: None,
        });
    }
    assert_eq!(base_bpm(&chart, HsFixOption::MinBpm), 120.0);
    assert_eq!(base_bpm(&chart, HsFixOption::MaxBpm), 240.0);
    assert_eq!(base_bpm(&chart, HsFixOption::MainBpm), 120.0);

    // LN始点がlane_notes側に無い場合もlong_notesから一度だけ集計する。
    chart.lane_notes[Lane::Key1.index()].retain(|note| note.kind != NoteKind::LongStart);
    assert_eq!(base_bpm(&chart, HsFixOption::MainBpm), 120.0);
}

#[test]
fn hsfix_uses_exact_note_tick_at_scroll_boundary() {
    let mut chart = chart();
    chart.metadata.initial_bpm = 180.0;
    let time = TimingMap::from_chart_timing_events(180.0, &[]).tick_to_time(ChartTick(1));
    chart.scroll_events.push(ScrollEvent { tick: ChartTick(1), time, factor: 2.0 });
    add_note(&mut chart, 1, 1, NoteKind::Tap);
    assert_eq!(base_bpm(&chart, HsFixOption::MaxBpm), 360.0);
}

#[test]
fn hsfix_uses_absolute_scroll_and_skips_zero_or_invalid_candidates() {
    let mut chart = chart();
    for (index, factor) in [0.0, -2.0, f64::NAN, f64::INFINITY].into_iter().enumerate() {
        let tick = index as u64 * 48;
        chart.scroll_events.push(ScrollEvent {
            tick: ChartTick(tick),
            time: TimeUs(index as i64 * 25_000),
            factor,
        });
        add_note(&mut chart, index as u32, tick, NoteKind::Tap);
    }
    for option in [HsFixOption::MinBpm, HsFixOption::MaxBpm, HsFixOption::MainBpm] {
        assert_eq!(base_bpm(&chart, option), 240.0);
    }
}

#[test]
fn hsfix_falls_back_to_bpm_when_all_scroll_candidates_are_zero() {
    let mut chart = chart();
    chart.scroll_events.push(ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 0.0 });
    chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: ChartTick(48),
        time: TimeUs(25_000),
        kind: TimingEventKind::BpmChange { bpm: 240.0 },
    });
    for (id, tick) in [0, 48, 96].into_iter().enumerate() {
        add_note(&mut chart, id as u32, tick, NoteKind::Tap);
    }
    assert_eq!(base_bpm(&chart, HsFixOption::MinBpm), 120.0);
    assert_eq!(base_bpm(&chart, HsFixOption::MaxBpm), 240.0);
    assert_eq!(base_bpm(&chart, HsFixOption::MainBpm), 240.0);
}

#[test]
fn hsfix_preserves_sub_one_bpm_and_applies_bpm_changes_at_stops() {
    let mut chart = chart();
    chart.metadata.initial_bpm = 0.5;
    chart.scroll_events.push(ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 });
    chart.timing_events = vec![
        bmz_chart::model::TimingEvent {
            tick: ChartTick(960),
            time: TimeUs(120_000_000),
            kind: TimingEventKind::BpmChange { bpm: 0.96 },
        },
        bmz_chart::model::TimingEvent {
            tick: ChartTick(960),
            time: TimeUs(120_000_000),
            kind: TimingEventKind::Stop { duration_us: 1_000_000 },
        },
    ];
    for (id, tick) in [0, 960, 1920].into_iter().enumerate() {
        add_note(&mut chart, id as u32, tick, NoteKind::Tap);
    }
    assert_eq!(base_bpm(&chart, HsFixOption::MinBpm), 1.0);
    assert_eq!(base_bpm(&chart, HsFixOption::MaxBpm), 1.92);
    assert_eq!(base_bpm(&chart, HsFixOption::MainBpm), 1.92);
}

#[test]
fn hsfix_initial_hispeed_applies_scroll_once_for_normal_and_floating() {
    let mut chart = chart();
    chart.scroll_events.push(ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 });
    add_note(&mut chart, 1, 48, NoteKind::Tap);
    for preset in [HispeedConfigPreset::Normal, HispeedConfigPreset::Floating] {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.set_hispeed_config(preset);
        profile.lane.normal_hispeed_level = 18;
        profile.lane.target_green_number = 300;
        for hs_fix in [
            HsFixOption::MinBpm,
            HsFixOption::MaxBpm,
            HsFixOption::MainBpm,
            HsFixOption::StartBpm,
            HsFixOption::Off,
        ] {
            let session = build_game_session(
                Arc::new(chart.clone()),
                &profile,
                PlaySessionOptions { hs_fix, ..Default::default() },
            );
            assert!(
                (session.hispeed - 2.0).abs() < 0.0001,
                "{preset:?} {hs_fix:?}: {}",
                session.hispeed
            );
        }
    }
}
