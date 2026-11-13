use super::*;

fn library() -> (std::path::PathBuf, LibraryDatabase, i64) {
    library_from_bms(
        "#TITLE Extra Note\n#BPM 150\n#WAV01 piano.wav\n#00111:01000100\n#00211:01000100\n#00319:01\n#00101:00010001\n#00201:00010001\n",
    )
}

fn library_from_bms(body: &str) -> (std::path::PathBuf, LibraryDatabase, i64) {
    let path = write_temp_bms(body);
    let imported = import_bms_chart(&path, None, true).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library = LibraryDatabase::from_connection(conn);
    let chart_id = library
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    (path, library, chart_id)
}

#[test]
fn extra_notes_follow_final_arrangement_without_changing_original_notes() {
    let (path, library, id) = library();
    for arrange in [
        ArrangeOption::Normal,
        ArrangeOption::Mirror,
        ArrangeOption::Random,
        ArrangeOption::SRandom,
        ArrangeOption::HRandom,
    ] {
        let options = PlaySessionOptions { arrange, arrange_seed: Some(123), ..Default::default() };
        let base = load_transformed_chart_for_play(&library, id, &options).unwrap();
        let extra = load_transformed_chart_for_play(
            &library,
            id,
            &PlaySessionOptions {
                assist: AssistOptionConfig { extra_note_depth: 2, ..Default::default() },
                ..options
            },
        )
        .unwrap();
        let first = extra.extra_note_start_id.expect("at least one event can be placed");
        for lane in Lane::ALL {
            let original: Vec<_> =
                extra.chart.lane_notes[lane.index()].iter().filter(|n| n.id < first).collect();
            let expected: Vec<_> = base.chart.lane_notes[lane.index()].iter().collect();
            assert_eq!(format!("{original:?}"), format!("{expected:?}"), "{arrange:?} {lane:?}");
        }
        assert_eq!(extra.assist_runtime.level, bmz_gameplay::session::AssistLevel::Assist);
        assert!(
            !extra.score_save_disabled,
            "EXTRA NOTE uses actual assist, not unconditional no-save"
        );
        assert_eq!(
            extra.chart.total_notes as usize,
            extra
                .chart
                .lane_notes
                .iter()
                .flatten()
                .filter(|n| matches!(n.kind, NoteKind::Tap | NoteKind::LongStart))
                .count()
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn extra_notes_with_no_candidates_have_the_same_save_eligibility_as_off() {
    let (path, mut library, _) = library();
    let options = PlaySessionOptions {
        assist: AssistOptionConfig { extra_note_depth: 1, ..Default::default() },
        ..Default::default()
    };
    let no_bgm_path = write_temp_bms("#TITLE No BGM\n#BPM 150\n#00111:01\n#00211:01\n");
    let import = import_bms_chart(&no_bgm_path, None, true).unwrap();
    let id = library
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &no_bgm_path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &import.chart,
        })
        .unwrap();
    let transformed = load_transformed_chart_for_play(&library, id, &options).unwrap();
    assert!(transformed.extra_note_start_id.is_none());
    assert!(transformed.assist_runtime.score_update_enabled());
    assert_eq!(transformed.assist_runtime.effective_mask & crate::assist::EXTRA_NOTE_MASK, 0);
    assert!(!transformed.score_save_disabled);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(no_bgm_path).unwrap();
}

#[test]
fn extra_notes_g_battle_keeps_opponent_display_and_judgement_unmodified() {
    let (path, library, id) = library();
    let base =
        load_transformed_chart_for_play(&library, id, &PlaySessionOptions::default()).unwrap();
    let options = PlaySessionOptions {
        session_mode: SessionMode::GBattle,
        assist: AssistOptionConfig { extra_note_depth: 2, ..Default::default() },
        battle_opponent: Some(BattleOpponentOptions {
            replay_player: None,
            replay_judge_rate_percent: 100,
            gauge: None,
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            arrange_seed: None,
            arrange_seed_2p: None,
            legacy_arrange_seed: false,
            packed_seed: None,
            bms_random_choices: None,
            bms_switch_choices: None,
            arrange_pattern: None,
            s_random_scheme: SRandomScheme::default(),
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
        }),
        ..Default::default()
    };
    let preloaded = preload_play_session_for_chart(&library, id, options, 1.0).unwrap();
    let primary = preloaded.chart.lane_notes[..8].iter().map(Vec::len).sum::<usize>();
    let display = preloaded.chart.lane_notes[8..].iter().map(Vec::len).sum::<usize>();
    assert!(primary > base.chart.total_notes as usize);
    assert_eq!(display, base.chart.total_notes as usize);
    assert_eq!(preloaded.chart.total_notes as usize, primary);
    let opponent = preloaded.opponent_chart.unwrap();
    assert_eq!(format!("{:?}", opponent.lane_notes), format!("{:?}", base.chart.lane_notes));
    assert_eq!(opponent.bgm_events.len(), base.chart.bgm_events.len());
    std::fs::remove_file(path).unwrap();
}

fn battle_library(mode: KeyMode) -> (std::path::PathBuf, LibraryDatabase, i64) {
    let high_key = if mode == KeyMode::K7 { "#00319:01\n" } else { "" };
    library_from_bms(&format!(
        "#TITLE Extra Battle\n#BPM 150\n#WAV01 piano.wav\n#00111:01000100\n#00211:01000100\n#00152:01000100\n#002D3:0100\n#00101:00010001\n#00201:00010001\n{high_key}"
    ))
}

/// Compare complete objects by corresponding SP lane, allowing only cloned IDs.
fn side_objects(chart: &PlayableChart, source: KeyMode, second: bool, mirror: bool) -> Vec<String> {
    let key_count = if source == KeyMode::K5 { 5 } else { 7 };
    source
        .active_lanes()
        .iter()
        .flat_map(|&lane| {
            let key = if mirror && lane != Lane::Scratch {
                key_count + 1 - lane.index()
            } else {
                lane.index()
            };
            let index =
                if second { if key == 0 { Lane::Scratch2.index() } else { key + 7 } } else { key };
            chart.lane_notes[index].iter().map(move |note| {
                let mut note = note.clone();
                note.id = NoteId(0);
                note.lane = lane;
                format!("{note:?}")
            })
        })
        .collect()
}

#[test]
fn extra_notes_dp_battle_duplicates_the_completed_sp_chart_including_hazards_and_lns() {
    for mode in [KeyMode::K5, KeyMode::K7] {
        let (path, library, id) = battle_library(mode);
        for scratch in [false, true] {
            let options = PlaySessionOptions {
                assist: AssistOptionConfig {
                    extra_note_depth: 2,
                    extra_note_scratch: scratch,
                    ..Default::default()
                },
                ..Default::default()
            };
            let sp = load_transformed_chart_for_play(&library, id, &options).unwrap();
            assert_eq!(sp.chart.metadata.key_mode, mode);
            assert!(sp.extra_note_start_id.is_some());
            for double_option in [DoubleOption::Battle, DoubleOption::BattleAutoScratch] {
                let battle = load_transformed_chart_for_play(
                    &library,
                    id,
                    &PlaySessionOptions { double_option, ..options.clone() },
                )
                .unwrap();
                let expected = side_objects(&sp.chart, mode, false, false);
                assert_eq!(side_objects(&battle.chart, mode, false, false), expected);
                assert_eq!(side_objects(&battle.chart, mode, true, false), expected);
                assert_eq!(
                    format!("{:?}", battle.chart.bgm_events),
                    format!("{:?}", sp.chart.bgm_events)
                );
                assert_eq!(battle.chart.total_notes, 2 * sp.chart.total_notes);
                assert_eq!(battle.chart.long_notes.len(), 2 * sp.chart.long_notes.len());
                let ids: std::collections::BTreeSet<_> =
                    battle.chart.lane_notes.iter().flatten().map(|n| n.id).collect();
                assert_eq!(ids.len(), battle.chart.lane_notes.iter().map(Vec::len).sum::<usize>());
                assert_eq!(battle.applied_arrange.double_option, double_option);
                assert_eq!(battle.assist_runtime.effective_mask, sp.assist_runtime.effective_mask);
            }
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn extra_notes_dp_battle_applies_the_selected_second_side_mirror_after_duplication() {
    for mode in [KeyMode::K5, KeyMode::K7] {
        let (path, library, id) = battle_library(mode);
        let options = PlaySessionOptions {
            assist: AssistOptionConfig { extra_note_depth: 2, ..Default::default() },
            ..Default::default()
        };
        let sp = load_transformed_chart_for_play(&library, id, &options).unwrap();
        let battle = load_transformed_chart_for_play(
            &library,
            id,
            &PlaySessionOptions {
                double_option: DoubleOption::Battle,
                arrange_2p: ArrangeOption::Mirror,
                ..options
            },
        )
        .unwrap();
        let expected = side_objects(&sp.chart, mode, false, false);
        assert_eq!(side_objects(&battle.chart, mode, false, false), expected);
        assert_eq!(side_objects(&battle.chart, mode, true, true), expected);
        assert_eq!(battle.applied_arrange.arrange_2p, ArrangeOption::Mirror);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn extra_notes_dp_battle_randomizes_copies_of_the_same_added_events() {
    let (path, library, id) = battle_library(KeyMode::K7);
    let options = PlaySessionOptions {
        assist: AssistOptionConfig { extra_note_depth: 2, ..Default::default() },
        ..Default::default()
    };
    let sp = load_transformed_chart_for_play(&library, id, &options).unwrap();
    assert!(sp.extra_note_start_id.is_some());
    // Discard lanes as well as IDs: independent side settings may move copies.
    let events = |chart: &PlayableChart, second: bool| {
        let mut result: Vec<_> = chart
            .lane_notes
            .iter()
            .enumerate()
            .filter(|(i, _)| (*i >= 8) == second)
            .flat_map(|(_, notes)| {
                notes.iter().map(|note| {
                    let mut note = note.clone();
                    note.id = NoteId(0);
                    note.lane = Lane::Key1;
                    format!("{note:?}")
                })
            })
            .collect();
        result.sort();
        result
    };
    for double_option in [DoubleOption::Battle, DoubleOption::BattleAutoScratch] {
        let battle = load_transformed_chart_for_play(
            &library,
            id,
            &PlaySessionOptions {
                double_option,
                arrange: ArrangeOption::SRandom,
                arrange_2p: ArrangeOption::HRandom,
                arrange_seed: Some(123),
                arrange_seed_2p: Some(456),
                ..options.clone()
            },
        )
        .unwrap();
        assert_eq!(events(&battle.chart, false), events(&sp.chart, false));
        assert_eq!(events(&battle.chart, true), events(&sp.chart, false));
        assert_eq!(battle.applied_arrange.arrange, ArrangeOption::SRandom);
        assert_eq!(battle.applied_arrange.arrange_2p, ArrangeOption::HRandom);
    }
    std::fs::remove_file(path).unwrap();
}
