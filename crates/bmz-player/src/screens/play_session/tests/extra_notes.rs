use super::*;

fn library() -> (std::path::PathBuf, LibraryDatabase, i64) {
    let path = write_temp_bms(
        "#TITLE Extra Note\n#BPM 150\n#WAV01 piano.wav\n#00111:01000100\n#00211:01000100\n#00319:01\n#00101:00010001\n#00201:00010001\n",
    );
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
