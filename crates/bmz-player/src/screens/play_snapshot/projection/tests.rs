use super::*;
use crate::config::profile_config::ProfileConfig;
use crate::screens::play_session::{PlaySessionOptions, build_game_session};
use bmz_audio::clock::AudioClock;
use bmz_chart::model::{LongNotePair, LongNoteStyle, ScrollEvent, SpeedEvent, TimingEvent};
use bmz_gameplay::judge::model::{ActiveLongNote, LongNoteEndRef};
use std::sync::atomic::AtomicU64;

fn session(chart: PlayableChart) -> GameSession {
    let profile = ProfileConfig::new_default("test", "Test", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;
    session.lift = 0.0;
    session.offsets.visual_offset_us = 0;
    session.audio_clock =
        AudioClock::with_position(1_000_000, 0, 0, Arc::new(AtomicU64::new(0)), true);
    session
}

fn chart() -> PlayableChart {
    let mut chart = super::super::tests::chart();
    chart.lane_notes[Lane::Key1.index()][0].tick = ChartTick(1920);
    chart
}

fn base(session: &GameSession, time: TimeUs, cache: &PlayRenderSnapshotCache) -> RenderSnapshot {
    build::build_render_state_with_target_and_bga_frames_cached(
        session,
        time,
        &[],
        None,
        None,
        None,
        &BgaFrameCatalog::new(),
        cache,
    )
}

fn assert_playfield_eq(actual: &RenderSnapshot, expected: &RenderSnapshot) {
    assert_eq!(actual.visible_notes, expected.visible_notes);
    assert_eq!(actual.visible_mines, expected.visible_mines);
    assert_eq!(actual.visible_long_notes, expected.visible_long_notes);
    assert_eq!(actual.bar_lines, expected.bar_lines);
    assert_eq!(actual.bpm_lines, expected.bpm_lines);
    assert_eq!(actual.stop_lines, expected.stop_lines);
    assert_eq!(actual.time_lines, expected.time_lines);
    assert_eq!(actual.judge_area_key_y, expected.judge_area_key_y);
    assert_eq!(actual.judge_area_scratch_y, expected.judge_area_scratch_y);
    assert_eq!(actual.now_bpm, expected.now_bpm);
    assert_eq!(actual.note_display_duration_ms, expected.note_display_duration_ms);
    assert_eq!(actual.adjusted_cover_progress, expected.adjusted_cover_progress);
    assert_eq!(actual.adjusted_rate, expected.adjusted_rate);
}

#[test]
fn motion_tracks_render_time_for_all_publication_phases_and_stalls() {
    let session = session(chart());
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    for fps in [120, 240] {
        for phase in [0, 417, 1_000, 1_999, 7_999] {
            for stall_us in [0, 16_000, 33_000, 100_000, 250_000] {
                let mut pool = PlayfieldProjection::pool(&session, &cache);
                let projection = Arc::get_mut(&mut pool[0]).unwrap();
                let mut clock = ProjectionClock::default();
                let mut previous: Option<(i64, f32)> = None;
                let mut source = base(&session, TimeUs(0), &cache);
                let mut last_publication = 0;
                for frame in 0..=fps / 2 {
                    let now = i64::from(frame) * 1_000_000 / i64::from(fps);
                    // Producer deadlines and consumer deadlines have independent phases.
                    let published = ((now - phase).max(0) / 8_000 * 8_000 + phase).min(now);
                    if published > last_publication {
                        projection.update(&session, TimeUs(published));
                        source = base(&session, TimeUs(published), &cache);
                        last_publication = published;
                    }
                    if now >= 100_000 && now < 100_000 + stall_us {
                        continue;
                    }
                    let mut rendered = source.clone();
                    projection.project(&mut rendered, TimeUs(now), &mut clock);
                    let y = rendered.visible_notes[Lane::Key1.index()][0].y;
                    // Independent analytic oracle: 120 BPM, four-beat lane, hispeed 1.
                    let expected = (1_000_000 - now) as f32 / 2_000_000.0;
                    assert!(
                        (y - expected).abs() < 1e-6,
                        "fps={fps}, phase={phase}, stall={stall_us}, now={now}"
                    );
                    assert_eq!(rendered.time, TimeUs(now));
                    if let Some((previous_time, previous_y)) = previous {
                        assert!(y < previous_y, "repeated or reversed note position");
                        assert!(
                            (previous_y - y - (now - previous_time) as f32 / 2_000_000.0).abs()
                                < 1e-6
                        );
                    }
                    previous = Some((now, y));
                }
            }
        }
    }
}

fn mixed_chart() -> PlayableChart {
    let mut chart = chart();
    let template = chart.lane_notes[Lane::Key1.index()][0].clone();
    chart.lane_notes[Lane::Key2.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key2,
        kind: NoteKind::Mine,
        damage: Some(4.0),
        ..template.clone()
    });
    for (index, (lane, mode)) in [
        (Lane::Key3, LongNoteMode::Ln),
        (Lane::Key4, LongNoteMode::Cn),
        (Lane::Key5, LongNoteMode::Hcn),
    ]
    .into_iter()
    .enumerate()
    {
        let id = 3 + index as u32 * 2;
        chart.lane_notes[lane.index()].extend([
            NoteEvent {
                id: NoteId(id),
                lane,
                kind: NoteKind::LongStart,
                tick: ChartTick(960),
                time: TimeUs(500_000),
                ..template.clone()
            },
            NoteEvent {
                id: NoteId(id + 1),
                lane,
                kind: NoteKind::LongEnd,
                tick: ChartTick(3456),
                time: TimeUs(1_800_000),
                ..template.clone()
            },
        ]);
        chart.long_notes.push(LongNotePair {
            lane,
            style: LongNoteStyle::ChannelPair,
            mode: Some(mode),
            start_note_id: NoteId(id),
            end_note_id: NoteId(id + 1),
            start_tick: ChartTick(960),
            end_tick: ChartTick(3456),
            start_time: TimeUs(500_000),
            end_time: TimeUs(1_800_000),
            sound: None,
        });
    }
    chart.bar_lines.push(BarLine { measure: 1, tick: ChartTick(1920), time: TimeUs(1_000_000) });
    chart.end_time = TimeUs(3_000_000);
    chart
}

#[test]
fn projection_matches_existing_playfield_rules_and_published_judgements() {
    for key_mode in [KeyMode::K7, KeyMode::K9] {
        for fade in [-100, 0, 100] {
            for constant in [false, true] {
                let mut chart = mixed_chart();
                chart.metadata.key_mode = key_mode;
                chart.timing_events = vec![
                    TimingEvent {
                        tick: ChartTick(1152),
                        time: TimeUs(600_000),
                        kind: TimingEventKind::BpmChange { bpm: 180.0 },
                    },
                    TimingEvent {
                        tick: ChartTick(2016),
                        time: TimeUs(900_000),
                        kind: TimingEventKind::Stop { duration_us: 200_000 },
                    },
                ];
                chart.scroll_events.push(ScrollEvent {
                    tick: ChartTick(1000),
                    time: TimeUs(520_833),
                    factor: 0.75,
                });
                chart.speed_events.push(SpeedEvent {
                    tick: ChartTick(1000),
                    time: TimeUs(520_833),
                    factor: 1.5,
                });
                let mut session = session(chart);
                session.constant_enabled = constant;
                session.constant_fade_ms = fade;
                session.target_green_number = 300;
                session.note_retention = true;
                session.assist.judge_area = true;
                session.lift = 0.15;
                session.lane_cover = 0.25;
                session.hidden_enabled = true;
                session.offsets.visual_offset_us = 15_000;
                session.judge.judged_notes.insert(NoteId(1), Judge::Poor);
                let long = &session.chart.long_notes[0];
                session.judge.lanes[long.lane.index()].active_long = Some(ActiveLongNote {
                    pair_index: 0,
                    mode: LongNoteMode::Ln,
                    start_note_id: long.start_note_id,
                    start_judge: Judge::PGreat,
                    start_delta: TimeUs(0),
                    end: LongNoteEndRef {
                        end_note_id: long.end_note_id,
                        end_tick: long.end_tick,
                        end_time: long.end_time,
                    },
                    started_at: long.start_time,
                    scratch_direction: None,
                    pending_release: None,
                });
                session.lane_hcn_timer[Lane::Key5.index()] = Some(HcnLaneTimer {
                    inclease: false,
                    since: TimeUs(500_000),
                    passing_count_us: 0,
                });
                let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
                let pool = PlayfieldProjection::pool(&session, &cache);
                let mut clock = ProjectionClock::default();
                for now in [0, 450_000, 650_000, 910_000, 990_000, 1_050_000, 1_250_000, 1_850_000]
                {
                    let mut actual = base(&session, TimeUs(0), &cache);
                    pool[0].project(&mut actual, TimeUs(now), &mut clock);
                    let expected = build_render_snapshot(&session, TimeUs(now), &[], None);
                    assert_playfield_eq(&actual, &expected);
                }
            }
        }
    }
}

#[test]
fn stale_publication_discovers_new_notes_mines_and_long_notes_after_stall() {
    let mut chart = mixed_chart();
    for notes in &mut chart.lane_notes {
        for note in notes {
            note.time.0 += 1_600_000;
            note.tick.0 += 3072;
        }
    }
    for long in &mut chart.long_notes {
        long.start_time.0 += 1_600_000;
        long.end_time.0 += 1_600_000;
        long.start_tick.0 += 3072;
        long.end_tick.0 += 3072;
    }
    let session = session(chart);
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut clock = ProjectionClock::default();
    let mut snapshot = base(&session, TimeUs(0), &cache);
    pool[0].project(&mut snapshot, TimeUs(0), &mut clock);
    assert!(snapshot.visible_long_notes.is_empty());
    pool[0].project(&mut snapshot, TimeUs(250_000), &mut clock);
    assert_eq!(snapshot.visible_long_notes.len(), 3);
    pool[0].project(&mut snapshot, TimeUs(750_000), &mut clock);
    assert_eq!(snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    assert_eq!(snapshot.visible_mines[Lane::Key2.index()].len(), 1);
    assert_playfield_eq(&snapshot, &build_render_snapshot(&session, TimeUs(750_000), &[], None));
}

#[test]
fn projection_clock_clamps_offset_changes_but_new_generation_can_reset() {
    let mut clock = ProjectionClock::default();
    assert_eq!(clock.advance(TimeUs(-100_000), 0), (TimeUs(-100_000), TimeUs(-100_000)));
    assert_eq!(clock.advance(TimeUs(100_000), 20_000), (TimeUs(100_000), TimeUs(120_000)));
    assert_eq!(clock.advance(TimeUs(101_000), 0), (TimeUs(101_000), TimeUs(120_000)));
    assert_eq!(clock.advance(TimeUs(99_000), 0), (TimeUs(101_000), TimeUs(120_000)));
    assert_eq!(clock.advance(TimeUs(121_000), 0), (TimeUs(121_000), TimeUs(121_000)));
    let mut retry = ProjectionClock::default();
    assert_eq!(retry.advance(TimeUs(-100_000), 0), (TimeUs(-100_000), TimeUs(-100_000)));
}

#[test]
fn published_buffers_are_immutable_and_reuse_map_capacity() {
    let mut session = session(chart());
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let mut pool = PlayfieldProjection::pool(&session, &cache);
    let observed = pool[0].clone();
    assert!(Arc::get_mut(&mut pool[0]).is_none());
    let free = Arc::get_mut(&mut pool[1]).unwrap();
    let capacity = free.judged_notes.capacity();
    for judge in [Judge::PGreat, Judge::Poor, Judge::Good] {
        session.judge.judged_notes.insert(NoteId(1), judge);
        free.update(&session, TimeUs(500_000));
        assert_eq!(free.judged_notes[&NoteId(1)], judge);
        assert_eq!(free.judged_notes.capacity(), capacity);
        assert!(observed.judged_notes.is_empty());
    }
}

#[test]
fn stop_freezes_positions_and_ready_time_remains_hidden() {
    let mut chart = chart();
    chart.timing_events.push(TimingEvent {
        tick: ChartTick(960),
        time: TimeUs(500_000),
        kind: TimingEventKind::Stop { duration_us: 200_000 },
    });
    let mut session = session(chart);
    session.audio_clock.pause_at(TimeUs(-500_000));
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut clock = ProjectionClock::default();
    let mut snapshot = base(&session, TimeUs(-500_000), &cache);
    pool[0].project(&mut snapshot, TimeUs(-100_000), &mut clock);
    assert_eq!(snapshot.time, TimeUs(-100_000));
    assert!(snapshot.visible_notes.iter().all(Vec::is_empty));
    pool[0].project(&mut snapshot, TimeUs(550_000), &mut clock);
    let stopped_y = snapshot.visible_notes[Lane::Key1.index()][0].y;
    pool[0].project(&mut snapshot, TimeUs(650_000), &mut clock);
    assert_eq!(snapshot.visible_notes[Lane::Key1.index()][0].y, stopped_y);
    pool[0].project(&mut snapshot, TimeUs(750_000), &mut clock);
    assert!(snapshot.visible_notes[Lane::Key1.index()][0].y < stopped_y);
}

fn chart_with_texts(texts: &[(i64, &str)]) -> PlayableChart {
    let mut chart = chart();
    chart.text_events = texts
        .iter()
        .enumerate()
        .map(|(index, &(time, text))| bmz_chart::model::ChartTextEvent {
            tick: ChartTick(index as u64),
            time: TimeUs(time),
            text: text.into(),
        })
        .collect();
    chart
}

#[test]
fn chart_text_follows_chart_time_in_build_and_projection() {
    let session = session(chart_with_texts(&[
        (1_000_000, "Hello World"),
        (2_000_000, "Second message"),
        (3_000_000, ""),
    ]));
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    // READY and before the first event: empty.
    assert_eq!(&*base(&session, TimeUs(-500_000), &cache).chart_text, "");
    assert_eq!(&*base(&session, TimeUs(999_999), &cache).chart_text, "");
    assert_eq!(&*base(&session, TimeUs(1_000_000), &cache).chart_text, "Hello World");
    assert_eq!(&*base(&session, TimeUs(2_500_000), &cache).chart_text, "Second message");
    assert_eq!(&*base(&session, TimeUs(9_000_000), &cache).chart_text, "");

    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut clock = ProjectionClock::default();
    let mut snapshot = base(&session, TimeUs(0), &cache);
    for (now, expected) in
        [(500_000, ""), (1_500_000, "Hello World"), (2_000_000, "Second message"), (3_000_000, "")]
    {
        pool[0].project(&mut snapshot, TimeUs(now), &mut clock);
        assert_eq!(&*snapshot.chart_text, expected, "{now}");
    }
    // The text is shared with the chart event, not copied per frame.
    pool[0].project(&mut snapshot, TimeUs(1_200_000), &mut ProjectionClock::default());
    assert!(Arc::ptr_eq(&snapshot.chart_text, &session.chart.text_events[0].text));
}

#[test]
fn chart_text_restores_the_state_at_seek_and_pause_positions() {
    let session = session(chart_with_texts(&[(1_000_000, "A"), (2_000_000, "B")]));
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut snapshot = base(&session, TimeUs(0), &cache);
    // A seek starts a new projection clock. Seeking back never keeps "B".
    for (now, expected) in [(2_500_000, "B"), (1_500_000, "A"), (500_000, ""), (2_000_000, "B")] {
        pool[0].project(&mut snapshot, TimeUs(now), &mut ProjectionClock::default());
        assert_eq!(&*snapshot.chart_text, expected, "{now}");
    }
    // While paused the clock repeats the same time and the text does not move.
    let mut clock = ProjectionClock::default();
    for _ in 0..3 {
        pool[0].project(&mut snapshot, TimeUs(1_999_999), &mut clock);
        assert_eq!(&*snapshot.chart_text, "A");
    }
}

#[test]
fn chart_text_event_timer_restarts_on_every_event_and_follows_seek() {
    let session = session(chart_with_texts(&[
        (1_000_000, "Same"),
        (2_000_000, "Same"),
        (3_000_000, ""),
        (3_000_000, "Last"),
    ]));
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let at = |now: i64| {
        let snapshot = base(&session, TimeUs(now), &cache);
        (snapshot.chart_text.to_string(), snapshot.chart_text_event_elapsed_ms)
    };
    // READY and before the first event the timer is OFF.
    assert_eq!(at(-1_000_000), (String::new(), None));
    assert_eq!(at(999_999), (String::new(), None));
    assert_eq!(at(1_000_000), ("Same".to_string(), Some(0)));
    assert_eq!(at(1_999_000), ("Same".to_string(), Some(999)));
    // Re-specifying the same text is an event: the timer restarts.
    assert_eq!(at(2_000_000), ("Same".to_string(), Some(0)));
    assert_eq!(at(2_400_000), ("Same".to_string(), Some(400)));
    // Simultaneous events reset once and show the later one.
    assert_eq!(at(3_250_000), ("Last".to_string(), Some(250)));
    // The text never expires by itself.
    assert_eq!(at(600_000_000), ("Last".to_string(), Some(597_000)));

    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut snapshot = base(&session, TimeUs(0), &cache);
    let mut clock = ProjectionClock::default();
    pool[0].project(&mut snapshot, TimeUs(2_500_000), &mut clock);
    assert_eq!(snapshot.chart_text_event_elapsed_ms, Some(500));
    // PAUSE repeats the same audio time; the timer does not advance.
    pool[0].project(&mut snapshot, TimeUs(2_500_000), &mut clock);
    assert_eq!(snapshot.chart_text_event_elapsed_ms, Some(500));
    // Seeking backwards restores the earlier event instead of carrying the later one.
    pool[0].project(&mut snapshot, TimeUs(1_500_000), &mut ProjectionClock::default());
    assert_eq!(snapshot.chart_text_event_elapsed_ms, Some(500));
    assert_eq!(&*snapshot.chart_text, "Same");
    pool[0].project(&mut snapshot, TimeUs(500_000), &mut ProjectionClock::default());
    assert_eq!(
        (snapshot.chart_text.is_empty(), snapshot.chart_text_event_elapsed_ms),
        (true, None)
    );
}

#[test]
fn chart_text_state_resets_when_the_chart_changes() {
    let first = session(chart_with_texts(&[(1_000_000, "Stage 1")]));
    let second = session(chart_with_texts(&[(4_000_000, "Stage 2")]));
    let first_cache = PlayRenderSnapshotCache::from_chart(&first.chart);
    let second_cache = PlayRenderSnapshotCache::from_chart(&second.chart);
    let first_snapshot = base(&first, TimeUs(3_000_000), &first_cache);
    assert_eq!(&*first_snapshot.chart_text, "Stage 1");
    // The next course stage starts from its own chart, with no state from the previous one.
    let next = base(&second, TimeUs(3_000_000), &second_cache);
    assert_eq!((next.chart_text.is_empty(), next.chart_text_event_elapsed_ms), (true, None));
    let pool = PlayfieldProjection::pool(&second, &second_cache);
    let mut snapshot = first_snapshot;
    pool[0].project(&mut snapshot, TimeUs(3_000_000), &mut ProjectionClock::default());
    assert_eq!(
        (snapshot.chart_text.is_empty(), snapshot.chart_text_event_elapsed_ms),
        (true, None)
    );
}

fn miss_session(miss_text: Option<&str>, duration_us: i64) -> GameSession {
    let mut chart = chart_with_texts(&[(1_000_000, "Normal"), (2_000_000, "Next")]);
    chart.metadata.miss_text = miss_text.map(Into::into);
    let mut session = session(chart);
    session.poor_bga_duration_us = duration_us;
    session
}

fn miss_state(snapshot: &RenderSnapshot) -> (String, Option<i32>, String, Option<i32>) {
    (
        snapshot.chart_text.to_string(),
        snapshot.chart_miss_text_elapsed_ms,
        snapshot.chart_miss_text.to_string(),
        snapshot.chart_text_event_elapsed_ms,
    )
}

#[test]
fn text00_replaces_the_displayed_text_only_during_the_miss_window() {
    let mut session = miss_session(Some("MISS!!"), 500_000);
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let at = |session: &GameSession, now: i64| miss_state(&base(session, TimeUs(now), &cache));
    let text = |text: &str| text.to_string();
    // No miss yet: the definition is visible through 20103 only.
    assert_eq!(at(&session, 1_500_000), (text("Normal"), None, text("MISS!!"), Some(500)));

    session.last_miss_judgement_time = Some(TimeUs(1_800_000));
    assert_eq!(at(&session, 1_800_000), (text("MISS!!"), Some(0), text("MISS!!"), Some(800)));
    // Channel #99 keeps advancing under the miss message and timer 20102 is not reset.
    assert_eq!(at(&session, 2_100_000), (text("MISS!!"), Some(300), text("MISS!!"), Some(100)));
    // After the window the current channel #99 text returns.
    assert_eq!(at(&session, 2_300_000), (text("Next"), None, text("MISS!!"), Some(300)));

    // A consecutive miss restarts the window.
    session.last_miss_judgement_time = Some(TimeUs(2_200_000));
    assert_eq!(at(&session, 2_300_000), (text("MISS!!"), Some(100), text("MISS!!"), Some(300)));
    assert_eq!(at(&session, 2_699_999), (text("MISS!!"), Some(499), text("MISS!!"), Some(699)));
    assert_eq!(at(&session, 2_700_000), (text("Next"), None, text("MISS!!"), Some(700)));

    // Projection uses the same window; PAUSE repeats the time and keeps the state.
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut snapshot = base(&session, TimeUs(0), &cache);
    let mut clock = ProjectionClock::default();
    for _ in 0..2 {
        pool[0].project(&mut snapshot, TimeUs(2_450_000), &mut clock);
        assert_eq!(miss_state(&snapshot), (text("MISS!!"), Some(250), text("MISS!!"), Some(450)));
    }
    pool[0].project(&mut snapshot, TimeUs(3_000_000), &mut clock);
    assert_eq!(miss_state(&snapshot), (text("Next"), None, text("MISS!!"), Some(1_000)));
}

#[test]
fn undefined_text00_or_zero_duration_never_changes_the_text() {
    for (miss_text, duration) in [(None, 500_000), (Some("MISS"), 0)] {
        let mut session = miss_session(miss_text, duration);
        session.last_miss_judgement_time = Some(TimeUs(1_500_000));
        let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
        let state = miss_state(&base(&session, TimeUs(1_600_000), &cache));
        assert_eq!(
            state,
            ("Normal".to_string(), None, miss_text.unwrap_or_default().to_string(), Some(600)),
            "{miss_text:?} {duration}"
        );
    }
}

#[test]
fn miss_before_the_first_event_returns_to_empty_text() {
    let mut session = miss_session(Some("MISS"), 200_000);
    session.last_miss_judgement_time = Some(TimeUs(100_000));
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let during = base(&session, TimeUs(150_000), &cache);
    assert_eq!((&*during.chart_text, during.chart_miss_text_elapsed_ms), ("MISS", Some(50)));
    assert_eq!(during.chart_text_event_elapsed_ms, None);
    let after = base(&session, TimeUs(300_000), &cache);
    assert_eq!((&*after.chart_text, after.chart_miss_text_elapsed_ms), ("", None));
}
