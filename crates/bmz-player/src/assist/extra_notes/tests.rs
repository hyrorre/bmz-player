use super::*;
use crate::config::profile_config::AssistOptionConfig;
use bmz_chart::model::{SoundAssetRef, SoundEvent, SoundSlice, TimingEvent};
use bmz_core::ids::NoteId;
use bmz_core::time::ChartTick;
use bmz_gameplay::session::{AssistLevel, AssistRuntime};

fn chart(mode: KeyMode) -> PlayableChart {
    let mut chart = super::super::tests::chart();
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    chart.long_notes.clear();
    chart.timing_events.clear();
    chart.metadata.key_mode = mode;
    chart.metadata.initial_bpm = 150.0;
    chart.total_notes = 0;
    chart
}

fn note(
    chart: &mut PlayableChart,
    lane: Lane,
    at: i64,
    kind: NoteKind,
    sound: Option<u32>,
) -> NoteId {
    let id = super::super::next_note_id(chart);
    chart.lane_notes[lane.index()].push(NoteEvent {
        id,
        lane,
        kind,
        time: TimeUs(at),
        tick: ChartTick(at.max(0) as u64),
        sound: sound.map(SoundId),
        layered_sounds: Vec::new(),
        damage: (kind == NoteKind::Mine).then_some(10.0),
    });
    id
}

fn tap(chart: &mut PlayableChart, lane: Lane, at: i64, sound: u32) {
    note(chart, lane, at, NoteKind::Tap, Some(sound));
}

fn bgm(chart: &mut PlayableChart, at: i64, sound: u32) {
    chart.bgm_events.push(SoundEvent {
        time: TimeUs(at),
        tick: ChartTick(at.max(0) as u64),
        sound: SoundId(sound),
    });
}

fn hold(chart: &mut PlayableChart, lane: Lane, start: i64, end: i64) {
    let mut pair = super::super::tests::chart().long_notes.remove(0);
    pair.start_note_id = note(chart, lane, start, NoteKind::LongStart, Some(1));
    pair.end_note_id = note(chart, lane, end, NoteKind::LongEnd, None);
    pair.lane = lane;
    pair.start_time = TimeUs(start);
    pair.end_time = TimeUs(end);
    pair.start_tick = ChartTick(start as u64);
    pair.end_tick = ChartTick(end as u64);
    chart.long_notes.push(pair);
}

fn sounding_events(chart: &PlayableChart) -> Vec<(TimeUs, SoundId)> {
    let mut events: Vec<_> = chart
        .lane_notes
        .iter()
        .flatten()
        .flat_map(|note| note.sounds().map(|sound| (note.time, sound)))
        .chain(chart.bgm_events.iter().map(|event| (event.time, event.sound)))
        .collect();
    events.sort_unstable();
    events
}

#[test]
fn extra_notes_preserve_all_original_objects_sounds_and_timing() {
    for mode in [
        KeyMode::K4,
        KeyMode::K5,
        KeyMode::K6,
        KeyMode::K7,
        KeyMode::K8,
        KeyMode::K9,
        KeyMode::K10,
        KeyMode::K14,
    ] {
        let mut chart = chart(mode);
        for i in 0..=40 {
            tap(&mut chart, Lane::Key4, i * 200_000, 1);
        }
        hold(&mut chart, Lane::Key1, 1_000_000, 4_000_000);
        note(&mut chart, Lane::Key2, 2_000_000, NoteKind::Mine, None);
        note(&mut chart, Lane::Key3, 2_000_000, NoteKind::Invisible, Some(2));
        chart.lane_notes[Lane::Key4.index()][0].layered_sounds.push(SoundId(2));
        for i in -1..=161 {
            bgm(&mut chart, i * 50_000, 1);
        }
        let before = chart.clone();
        let first = super::super::next_note_id(&chart);
        assert!(add_extra_notes(&mut chart, 16, true, false), "{mode:?}");
        assert_eq!(sounding_events(&chart), sounding_events(&before), "{mode:?}");
        assert_eq!(format!("{:?}", chart.long_notes), format!("{:?}", before.long_notes));
        for lane in Lane::ALL {
            let originals: Vec<_> =
                chart.lane_notes[lane.index()].iter().filter(|note| note.id < first).collect();
            assert_eq!(
                format!("{originals:?}"),
                format!("{:?}", before.lane_notes[lane.index()].iter().collect::<Vec<_>>())
            );
            for added in chart.lane_notes[lane.index()].iter().filter(|note| note.id >= first) {
                assert!((0..=8_000_000).contains(&added.time.0));
                let gap = if is_scratch(lane) { 250_000 } else { 200_000 };
                for original in &before.lane_notes[lane.index()] {
                    assert!(added.time.0.abs_diff(original.time.0) >= gap);
                }
                assert!(!before.long_notes.iter().any(|p| p.lane == lane
                    && p.start_time <= added.time
                    && added.time <= p.end_time));
            }
        }
    }
}

#[test]
fn extra_notes_enforce_pairwise_gaps_density_and_depth_with_dense_bgm() {
    let mut chart = chart(KeyMode::K7);
    for i in 0..=80 {
        tap(&mut chart, Lane::Key4, i * 100_000, 1);
    }
    for i in 0..=800 {
        for sound in 1..=3 {
            bgm(&mut chart, i * 10_000, sound);
        }
    }
    let before = chart.clone();
    let first = super::super::next_note_id(&chart);
    assert!(add_extra_notes(&mut chart, 2, false, false));
    let mut added_times = Vec::new();
    let mut chords = BTreeMap::<TimeUs, usize>::new();
    let tempo = Tempo::new(&before);
    for lane in Lane::ALL {
        let added: Vec<_> =
            chart.lane_notes[lane.index()].iter().filter(|note| note.id >= first).collect();
        if is_scratch(lane) {
            assert!(added.is_empty());
        }
        for pair in added.windows(2) {
            assert!(
                pair[1].time.0 - pair[0].time.0
                    >= tempo.gap(pair[0].time, lane).max(tempo.gap(pair[1].time, lane))
            );
        }
        for note in added {
            added_times.push(note.time);
            *chords.entry(note.time).or_default() += 1;
        }
    }
    added_times.sort_unstable();
    assert!(added_times.len() > 10);
    assert!(chords.values().all(|&count| count <= 2));
    for &time in &added_times {
        let recent = added_times.iter().filter(|at| time.0 - SECOND < at.0 && **at <= time).count();
        assert!(recent <= 8);
    }
    assert_eq!(sounding_events(&chart), sounding_events(&before));
}

#[test]
fn extra_notes_limit_sparse_charts_to_the_source_density() {
    let mut chart = chart(KeyMode::K7);
    for i in 0..=16 {
        tap(&mut chart, Lane::Key1, i * 500_000, 1);
    }
    for i in 0..=800 {
        bgm(&mut chart, i * 10_000, 2);
    }
    let original: Vec<_> = chart.lane_notes.iter().flatten().map(|note| note.time).collect();
    let first = super::super::next_note_id(&chart);
    assert!(add_extra_notes(&mut chart, 16, false, false));
    let added: Vec<_> =
        chart.lane_notes.iter().flatten().filter(|n| n.id >= first).map(|n| n.time).collect();
    for &time in &added {
        let count =
            original.iter().filter(|at| time.0 - SECOND <= at.0 && at.0 < time.0 + SECOND).count();
        let recent = added.iter().filter(|at| time.0 - SECOND < at.0 && **at <= time).count();
        assert!(recent <= count.div_ceil(2).min(8));
    }
}

#[test]
fn extra_notes_share_affinity_only_for_the_same_asset_and_slice() {
    let mut chart = chart(KeyMode::K7);
    for at in [0, 1_000_000, 2_000_000] {
        tap(&mut chart, Lane::Key3, at, 1);
    }
    chart.sounds = vec![
        SoundAssetRef {
            id: SoundId(1),
            path: "sounds/./piano.wav".into(),
            slice: Some(SoundSlice { start_us: 100, duration_us: Some(200) }),
        },
        SoundAssetRef {
            id: SoundId(2),
            path: "sounds\\piano.wav".into(),
            slice: Some(SoundSlice { start_us: 100, duration_us: Some(200) }),
        },
        SoundAssetRef {
            id: SoundId(3),
            path: "sounds/piano.wav".into(),
            slice: Some(SoundSlice { start_us: 300, duration_us: Some(200) }),
        },
    ];
    let keys = sound_keys(&chart);
    assert!(keys[&1] == keys[&2]);
    assert!(keys[&1] != keys[&3]);
    bgm(&mut chart, 500_000, 2);
    assert!(add_extra_notes(&mut chart, 1, false, false));
    assert!(
        chart.lane_notes[Lane::Key3.index()]
            .iter()
            .any(|n| n.time == TimeUs(500_000) && n.sound == Some(SoundId(2)))
    );
}

#[test]
fn extra_notes_scratch_option_and_250ms_boundary() {
    let mut original = chart(KeyMode::K7);
    for at in [0, 1_000_000, 2_000_000] {
        tap(&mut original, Lane::Scratch, at, 1);
    }
    bgm(&mut original, 500_000, 1);
    let mut off = original.clone();
    assert!(add_extra_notes(&mut off, 1, false, false));
    assert_eq!(off.lane_notes[Lane::Scratch.index()].len(), 3);
    assert!(add_extra_notes(&mut original, 1, true, false));
    assert_eq!(original.lane_notes[Lane::Scratch.index()].len(), 4);
    let tempo = Tempo::new(&original);
    let index = LaneIndex::new(&original, Lane::Scratch, &tempo);
    assert!(index.slack(TimeUs(249_999), 250_000).is_none());
    assert!(index.slack(TimeUs(250_000), 250_000).is_some());
}

#[test]
fn extra_notes_check_distant_slow_bpm_notes_and_invalid_tempo() {
    let mut chart = chart(KeyMode::K7);
    chart.metadata.initial_bpm = -30.0;
    tap(&mut chart, Lane::Key1, 0, 1);
    tap(&mut chart, Lane::Key1, 500_000, 1);
    chart.timing_events = vec![
        TimingEvent {
            time: TimeUs(400_000),
            tick: ChartTick(0),
            kind: TimingEventKind::BpmChange { bpm: 300.0 },
        },
        TimingEvent {
            time: TimeUs(600_000),
            tick: ChartTick(0),
            kind: TimingEventKind::BpmChange { bpm: f64::NAN },
        },
    ];
    let tempo = Tempo::new(&chart);
    assert_eq!(tempo.gap(TimeUs(0), Lane::Key1), 1_000_000);
    assert_eq!(tempo.gap(TimeUs(800_000), Lane::Key1), 125_000);
    let index = LaneIndex::new(&chart, Lane::Key1, &tempo);
    assert!(index.slack(TimeUs(999_999), 125_000).is_none());
    assert!(index.slack(TimeUs(1_000_000), 125_000).is_some());
    chart.metadata.initial_bpm = 0.0;
    assert_eq!(Tempo::new(&chart).gap(TimeUs(0), Lane::Key1), 250_000);
}

#[test]
fn extra_notes_count_held_lns_in_the_chord_cap_and_keep_mines() {
    let mut chart = chart(KeyMode::K7);
    for lane in [Lane::Key1, Lane::Key2, Lane::Key3] {
        hold(&mut chart, lane, 0, 2_000_000);
    }
    tap(&mut chart, Lane::Key4, 1_000_000, 1);
    let mine = note(&mut chart, Lane::Key5, 1_000_000, NoteKind::Mine, None);
    for sound in 1..=16 {
        bgm(&mut chart, 1_000_000, sound);
    }
    assert!(!add_extra_notes(&mut chart, 16, true, false));
    assert_eq!(chart.bgm_events.len(), 16);
    assert!(
        chart.lane_notes[Lane::Key5.index()]
            .iter()
            .any(|n| n.id == mine && n.kind == NoteKind::Mine)
    );
}

#[test]
fn extra_notes_dp_density_budgets_are_independent_and_battle_excludes_opponent() {
    for primary_only in [false, true] {
        let mut chart = chart(KeyMode::K14);
        for at in [0, 1_000_000, 2_000_000] {
            tap(&mut chart, Lane::Key1, at, 1);
        }
        // Only 2P has this sound affinity, but 2P's original density is zero near 1s.
        tap(&mut chart, Lane::Key8, 10_000_000, 2);
        bgm(&mut chart, 1_000_000, 2);
        let first = super::super::next_note_id(&chart);
        assert!(add_extra_notes(&mut chart, 1, false, primary_only));
        let added = chart.lane_notes.iter().flatten().find(|n| n.id >= first).unwrap();
        assert!(added.lane.index() < 8);
    }
}

#[test]
fn extra_notes_depth_zero_is_exactly_inert_and_unplaceable_is_not_assist() {
    let mut chart = chart(KeyMode::K7);
    tap(&mut chart, Lane::Key1, 0, 1);
    tap(&mut chart, Lane::Key1, 2_000_000, 1);
    bgm(&mut chart, -1, 1);
    bgm(&mut chart, 2_000_001, 1);
    let original = format!("{chart:?}");
    assert!(!add_extra_notes(&mut chart, 0, true, false));
    assert_eq!(format!("{chart:?}"), original);
    let mut runtime = AssistRuntime::default();
    assert!(
        super::super::apply_extra_note_assist(
            &mut chart,
            AssistOptionConfig { extra_note_depth: 1, ..Default::default() },
            &mut runtime,
            false
        )
        .is_none()
    );
    assert_eq!(runtime.level, AssistLevel::None);
    assert_eq!(runtime.effective_mask, 0);
    assert_eq!(chart.bgm_events.len(), 2);
}

#[test]
fn extra_notes_are_deterministic_and_cannot_reuse_exhausted_ids() {
    let mut chart = chart(KeyMode::K7);
    for at in [0, 1_000_000, 2_000_000] {
        tap(&mut chart, Lane::Key1, at, 1);
    }
    bgm(&mut chart, 500_000, 42);
    let mut same = chart.clone();
    assert!(add_extra_notes(&mut chart, 1, false, false));
    assert!(add_extra_notes(&mut same, 1, false, false));
    assert_eq!(format!("{chart:?}"), format!("{same:?}"));
    bgm(&mut chart, 1_500_000, 42);
    chart.lane_notes[Lane::Key1.index()][0].id = NoteId(u32::MAX);
    let before = format!("{chart:?}");
    assert!(!add_extra_notes(&mut chart, 1, false, false));
    assert_eq!(format!("{chart:?}"), before);
}

#[test]
fn extra_notes_are_independent_of_the_installation_directory() {
    let mut original = chart(KeyMode::K7);
    for at in [0, 1_000_000, 2_000_000] {
        tap(&mut original, Lane::Key1, at, 1);
    }
    original.sounds =
        vec![SoundAssetRef { id: SoundId(2), path: "C:/songs/chart/a.wav".into(), slice: None }];
    bgm(&mut original, 500_000, 2);
    let mut relocated = original.clone();
    relocated.sounds[0].path = "/music/chart/a.wav".into();
    assert!(add_extra_notes(&mut original, 1, false, false));
    assert!(add_extra_notes(&mut relocated, 1, false, false));
    assert_eq!(format!("{:?}", original.lane_notes), format!("{:?}", relocated.lane_notes));
}

#[test]
fn extra_notes_depth_groups_equal_times_even_when_ticks_differ() {
    let mut chart = chart(KeyMode::K7);
    for at in [0, 1_000_000, 2_000_000] {
        tap(&mut chart, Lane::Key1, at, 1);
    }
    bgm(&mut chart, 500_000, 2);
    bgm(&mut chart, 500_000, 3);
    chart.bgm_events[1].tick = ChartTick(123);
    let first = super::super::next_note_id(&chart);
    assert!(add_extra_notes(&mut chart, 1, false, false));
    assert_eq!(chart.lane_notes.iter().flatten().filter(|n| n.id >= first).count(), 1);
    assert_eq!(chart.bgm_events.len(), 1);
}
