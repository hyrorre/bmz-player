use super::*;

#[test]
fn imports_mixed_space_and_colon_channel_data_into_playable_chart() {
    let text = "\
#TITLE Mixed Channel Separators
#BPM 120
#TOTAL 200
#WAV01 key.wav
#BMP01 base.png
#BMP02 poor.png
#BMP03 layer.png
#BMP04 layer2.png
#BPM01 180
#STOP01 192
#00002 0.5
#00004 01
#00006: 02
#00007\t03
#0000A   04
#00001 01
#00111  : 0101\x20\x20
#00112\t0101\t
#00133 01
#00208 01
#00209 01
#00311 01
";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.bms");
    std::fs::write(&path, text).unwrap();
    for name in ["key.wav", "base.png", "poor.png", "layer.png", "layer2.png"] {
        write_temp_file(&dir.path().join(name));
    }

    let result = import_bms_chart(&path, None, true).unwrap();
    let chart = &result.chart;
    assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
    assert_eq!(chart.identity, compute_chart_identity(text.as_bytes()));
    assert_eq!(chart.total_notes, 5);
    assert_eq!(chart.notes_for_lane(Lane::Key1)[0].time.0, 1_000_000);
    assert_eq!(chart.notes_for_lane(Lane::Key2)[1].time.0, 2_000_000);
    assert_eq!(chart.bgm_events.len(), 1);
    assert_eq!(chart.notes_for_lane(Lane::Key3).len(), 1);
    assert_eq!(chart.notes_for_lane(Lane::Key3)[0].kind, NoteKind::Invisible);
    assert!(chart.metadata.has_bga);
    assert_eq!(chart.bga_events.len(), 4);
    for kind in [BgaEventKind::Base, BgaEventKind::Poor, BgaEventKind::Layer, BgaEventKind::Layer2]
    {
        assert!(chart.bga_events.iter().any(|event| event.kind == kind && event.time.0 == 0));
    }
    assert!(
        chart
            .timing_events
            .iter()
            .any(|event| matches!(event.kind, TimingEventKind::BpmChange { bpm: 180.0 })
                && event.time.0 == 3_000_000)
    );
    assert!(
        chart
            .timing_events
            .iter()
            .any(|event| matches!(event.kind, TimingEventKind::Stop { duration_us: 1_333_333 })
                && event.time.0 == 3_000_000)
    );
}

#[test]
fn imports_basic_7k_bms_into_playable_chart() {
    let text = "\
#TITLE Integration Song
#ARTIST Test Artist
#BPM 120
#TOTAL 200
#WAV01 key.wav
#WAV02 bgm.wav
#BMP01 bga.png
#BPM01 180
#STOP01 192
#00001:0002
#00004:0001
#00011:0100
#00013:0001
#00108:0100
#00109:0100
#00111:01
";
    let path = write_temp_bms(text);
    let base_dir = path.parent().unwrap().to_path_buf();
    write_temp_file(&base_dir.join("key.wav"));
    write_temp_file(&base_dir.join("bgm.wav"));
    write_temp_file(&base_dir.join("bga.png"));

    let result = import_bms_chart(&path, None, true).unwrap();
    let expected_identity = compute_chart_identity(text.as_bytes());

    assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
    assert_eq!(result.chart.identity, expected_identity);
    assert_eq!(result.chart.metadata.title, "Integration Song");
    assert_eq!(result.chart.metadata.artist, "Test Artist");
    assert_eq!(result.chart.total_notes, 3);
    assert_eq!(result.chart.sounds.len(), 2);
    assert_eq!(result.chart.bga_assets.len(), 1);
    assert_eq!(result.chart.bgm_events.len(), 1);
    assert_eq!(result.chart.bga_events.len(), 1);
    assert_eq!(result.chart.bga_events[0].kind, BgaEventKind::Base);
    assert_eq!(result.chart.notes_for_lane(Lane::Key1).len(), 2);
    assert_eq!(result.chart.notes_for_lane(Lane::Key3).len(), 1);

    let first = &result.chart.notes_for_lane(Lane::Key1)[0];
    assert_eq!(first.kind, NoteKind::Tap);
    assert_eq!(first.time.0, 0);
    assert!(first.sound.is_some());

    let second = &result.chart.notes_for_lane(Lane::Key3)[0];
    assert_eq!(second.kind, NoteKind::Tap);
    assert_eq!(second.time.0, 1_000_000);

    assert!(result.chart.timing_events.iter().any(|event| matches!(
        event.kind,
        TimingEventKind::BpmChange { bpm } if bpm == 180.0
    )));
    // 同じ位置のBPM変更が先に適用されるため、STOPはBPM 180で計算する。
    assert!(result.chart.timing_events.iter().any(|event| matches!(
        event.kind,
        TimingEventKind::Stop { duration_us } if duration_us == 1_333_333
    )));

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(base_dir.join("key.wav")).unwrap();
    std::fs::remove_file(base_dir.join("bgm.wav")).unwrap();
    std::fs::remove_file(base_dir.join("bga.png")).unwrap();
}

#[test]
fn preserves_fractional_measure_time_before_tick_compression() {
    let text = "\
#TITLE Fractional Measure Timing
#BPM 120
#BPM01 1
#STOP01 1
#WAV01 key.wav
#00102:0.000001875
#00108:01
#00109:0001
#00211:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    let note = &result.chart.notes_for_lane(Lane::Key1)[0];
    assert_eq!(note.tick, bmz_core::time::ChartTick(3_840));
    // Measure 1 contributes 450us at BPM 1. The STOP starts 225us into it and lasts 1.25s.
    assert_eq!(note.time, bmz_core::time::TimeUs(3_250_450));
    assert!(result.chart.timing_events.iter().any(|event| {
        matches!(event.kind, TimingEventKind::Stop { duration_us: 1_250_000 })
            && event.time == bmz_core::time::TimeUs(2_000_225)
    }));

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_data_song_bga_compat_fixture() {
    let path = repo_root().join("data/songs/bga-compat/bga-compat.bms");

    let result = import_bms_chart(&path, None, true).unwrap();

    assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
    assert_eq!(result.chart.metadata.title, "BMZ BGA Compatibility");
    assert_eq!(result.chart.metadata.stage_file, "stage.bmp");
    assert_eq!(result.chart.metadata.banner_file, "banner.jpg");
    assert_eq!(result.chart.metadata.backbmp_file, "back.bmp");
    assert_eq!(result.chart.sounds.len(), 1);
    assert_eq!(result.chart.bga_assets.len(), 5);
    assert_eq!(result.chart.bga_events.len(), 5);

    assert_eq!(
        [1, 2, 3, 4, 5]
            .into_iter()
            .map(|key| bga_asset_path_for_key(&result.chart, key))
            .collect::<Vec<_>>(),
        vec![
            (BgaAssetKind::Static, "data/songs/bga-compat/small.png".to_string()),
            (BgaAssetKind::Video, "data/songs/bga-compat/movie.webm".to_string()),
            (BgaAssetKind::Static, "data/songs/bga-compat/still.gif".to_string()),
            (BgaAssetKind::Static, "data/songs/bga-compat/tga_only.tga".to_string()),
            (BgaAssetKind::Static, "data/songs/bga-compat/animated.gif".to_string()),
        ]
    );
}

#[test]
fn import_sound_existence_check_uses_beatoraja_extension_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sound-fallback.bms");
    let text = "\
#TITLE Sound Fallback
#BPM 120
#TOTAL 200
#WAV01 key.wav
#00111:01
";
    std::fs::write(&path, text).unwrap();
    write_temp_file(&dir.path().join("key.flac"));

    let result = import_bms_chart(&path, None, true).unwrap();

    assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
    assert_eq!(result.chart.sounds.len(), 1);
    assert_eq!(result.chart.sounds[0].path, dir.path().join("key.wav"));
}

#[test]
fn imports_simultaneous_base_and_poor_bga_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("simultaneous-bga.bms");
    let text = "\
#TITLE Simultaneous BGA
#BPM 120
#TOTAL 200
#WAV01 key.wav
#BMP01 base.bmp
#BMP02 poor.bmp
#00004:01
#00006:02
#00111:01
";
    std::fs::write(&path, text).unwrap();
    write_temp_file(&dir.path().join("key.wav"));
    write_temp_file(&dir.path().join("base.bmp"));
    write_temp_file(&dir.path().join("poor.bmp"));

    let result = import_bms_chart(&path, None, true).unwrap();

    assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
    assert_eq!(result.chart.bga_events.len(), 2);
    assert!(result.chart.bga_events.iter().any(|event| event.kind == BgaEventKind::Base));
    assert!(result.chart.bga_events.iter().any(|event| event.kind == BgaEventKind::Poor));
    assert!(result.chart.bga_events.iter().all(|event| event.tick.0 == 0 && event.time.0 == 0));
}

#[test]
fn bga_asset_ids_are_stable_across_repeated_imports_and_definition_order() {
    let dir = tempfile::tempdir().unwrap();
    let ordered_path = dir.path().join("ordered.bms");
    let reversed_path = dir.path().join("reversed.bms");
    let ordered = "\
#TITLE Stable BGA Assets
#BPM 120
#BMP01 base.png
#BMP02 poor.webm
#BMP03 layer.jpg
#00004:01
#00006:02
#00007:03
";
    let reversed = "\
#TITLE Stable BGA Assets
#BPM 120
#BMP03 layer.jpg
#BMP02 poor.webm
#BMP01 base.png
#00004:01
#00006:02
#00007:03
";
    std::fs::write(&ordered_path, ordered).unwrap();
    std::fs::write(&reversed_path, reversed).unwrap();

    let expected = vec![
        (1, BgaAssetId(0), dir.path().join("base.png"), BgaAssetKind::Static),
        (2, BgaAssetId(1), dir.path().join("poor.webm"), BgaAssetKind::Video),
        (3, BgaAssetId(2), dir.path().join("layer.jpg"), BgaAssetKind::Static),
    ];

    for path in [&ordered_path, &ordered_path, &reversed_path] {
        let result = import_bms_chart(path, None, false).unwrap();
        assert_eq!(bga_asset_manifest(&result.chart), expected);
    }
}

#[test]
fn imports_missing_bmp_bga_as_clear_event() {
    let text = "\
#TITLE BGA Clear Song
#BPM 120
#TOTAL 200
#BMP01 layer.png
#00007:01
#00107:02
#00011:01
";
    let path = write_temp_bms(text);

    let result = import_bms_chart(&path, None, false).unwrap();

    assert_eq!(result.chart.bga_assets.len(), 1);
    assert_eq!(result.chart.bga_events.len(), 2);
    assert_eq!(result.chart.bga_events[0].kind, BgaEventKind::Layer);
    assert_eq!(result.chart.bga_events[0].asset, Some(result.chart.bga_assets[0].id));
    assert_eq!(result.chart.bga_events[1].kind, BgaEventKind::Layer);
    assert_eq!(result.chart.bga_events[1].asset, None);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| matches!(warning, ImportWarning::MissingBmpDefinition { key: 2 }))
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_mine_notes_with_damage() {
    let text = "\
#TITLE Mine Song
#BPM 120
#TOTAL 200
#001D1:0008000C
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    let mines: Vec<_> = result
        .chart
        .notes_for_lane(Lane::Key1)
        .iter()
        .filter(|n| n.kind == NoteKind::Mine)
        .collect();
    assert_eq!(mines.len(), 2);
    assert_eq!(mines[0].damage, Some(8.0));
    assert_eq!(mines[1].damage, Some(12.0));
    // total_notes は Tap/LongStart のみ。Mine はスコア対象外。
    assert_eq!(result.chart.total_notes, 0);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn invisible_notes_do_not_become_bgm_events() {
    let text = "\
#TITLE Invisible Song
#BPM 120
#TOTAL 200
#WAV01 bgm.wav
#WAV02 hidden.wav
#00001:01
#00031:02
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    assert_eq!(result.chart.bgm_events.len(), 1);
    let lane = result.chart.notes_for_lane(Lane::Key1);
    assert_eq!(lane.len(), 1);
    assert_eq!(lane[0].kind, NoteKind::Invisible);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn ignores_zero_objects_in_legacy_bpm_channel() {
    let text = "\
#TITLE Legacy BPM Zero Placeholders
#BPM 97.5
#TOTAL 200
#00803:00005F00005A
#00811:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    let bpm_changes: Vec<_> = result
        .chart
        .timing_events
        .iter()
        .filter_map(|event| match event.kind {
            TimingEventKind::BpmChange { bpm } => Some(bpm),
            _ => None,
        })
        .collect();
    assert!(!bpm_changes.is_empty());
    assert!(bpm_changes.iter().all(|bpm| *bpm > 0.0), "bpm changes: {bpm_changes:?}");

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_random_branch_with_deterministic_seed() {
    // RANDOM 2 / IF 1 を含むので、seed=1 で同じ結果になることを確認する。
    let text = "\
#TITLE Random Song
#BPM 120
#TOTAL 200
#00011:01010101
#RANDOM 2
#IF 1
#00211:01010101
#ENDIF
#ENDRANDOM
";
    let path = write_temp_bms(text);
    let result_a = import_bms_chart(&path, Some(1), false).unwrap();
    let result_b = import_bms_chart(&path, Some(1), false).unwrap();
    assert_eq!(
        result_a.chart.notes_for_lane(Lane::Key1).len(),
        result_b.chart.notes_for_lane(Lane::Key1).len(),
        "fixed seed should give identical note count"
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn returns_bms_random_choices_without_setrandom_values() {
    let text = "\
#TITLE Random Choices
#BPM 120
#TOTAL 200
#RANDOM 2
#IF 1
#00111:01
#ENDIF
#IF 2
#00112:01
#ENDIF
#ENDRANDOM
#SETRANDOM 2
#IF 2
#00213:01
#ENDIF
#ENDRANDOM
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, Some(1), false).unwrap();

    assert_eq!(result.bms_random_choices.len(), 1);
    assert!((1..=2).contains(&result.bms_random_choices[0]));

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn recorded_bms_random_choices_replay_the_same_branch() {
    let text = "\
#TITLE Random Replay
#BPM 120
#TOTAL 200
#RANDOM 2
#IF 1
#00111:01
#ENDIF
#IF 2
#00112:01
#ENDIF
#ENDRANDOM
";
    let path = write_temp_bms(text);
    let seeded = import_bms_chart(&path, Some(1), false).unwrap();
    let replayed = import_bms_chart_with_random_source(
        &path,
        BmsRandomSource::Choices {
            random: seeded.bms_random_choices.clone(),
            switches: seeded.bms_switch_choices.clone(),
        },
        false,
    )
    .unwrap();

    assert_eq!(
        replayed.bms_random_choices, seeded.bms_random_choices,
        "recorded choices must take priority over random seed selection"
    );
    assert_eq!(
        replayed.chart.notes_for_lane(Lane::Key1).len(),
        seeded.chart.notes_for_lane(Lane::Key1).len()
    );
    assert_eq!(
        replayed.chart.notes_for_lane(Lane::Key2).len(),
        seeded.chart.notes_for_lane(Lane::Key2).len()
    );

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn recorded_bms_switch_choices_replay_large_switch_branch() {
    let text = "\
#TITLE Switch Replay
#BPM 120
#TOTAL 200
#SWITCH 2000000000000
#CASE 1
#00111:01
#SKIP
#CASE21
#00112:01
#SKIP
#DEF
#00113:01
#ENDSW
";
    let path = write_temp_bms(text);
    let selected = import_bms_chart_with_random_source(
        &path,
        BmsRandomSource::Choices { random: Vec::new(), switches: vec![21] },
        false,
    )
    .unwrap();
    let replayed = import_bms_chart_with_random_source(
        &path,
        BmsRandomSource::Choices {
            random: selected.bms_random_choices.clone(),
            switches: selected.bms_switch_choices.clone(),
        },
        false,
    )
    .unwrap();

    assert!(selected.bms_random_choices.is_empty());
    assert_eq!(selected.bms_switch_choices, vec![21]);
    assert_eq!(selected.chart.notes_for_lane(Lane::Key1).len(), 0);
    assert_eq!(selected.chart.notes_for_lane(Lane::Key2).len(), 1);
    assert_eq!(selected.chart.notes_for_lane(Lane::Key3).len(), 0);
    assert_eq!(replayed.bms_switch_choices, selected.bms_switch_choices);
    assert_eq!(replayed.chart.notes_for_lane(Lane::Key2).len(), 1);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn invalid_bms_random_choice_is_clamped_with_a_warning() {
    let text = "\
#TITLE Invalid Random Choice
#BPM 120
#TOTAL 200
#RANDOM 2
#IF 1
#00111:01
#ENDIF
#IF 2
#00112:01
#ENDIF
#ENDRANDOM
";
    let path = write_temp_bms(text);
    let result = import_bms_chart_with_random_source(
        &path,
        BmsRandomSource::Choices { random: vec![99], switches: Vec::new() },
        false,
    )
    .unwrap();

    assert_eq!(result.bms_random_choices, vec![2]);
    assert_eq!(result.chart.notes_for_lane(Lane::Key1).len(), 0);
    assert_eq!(result.chart.notes_for_lane(Lane::Key2).len(), 1);
    assert!(result.warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. } if code == "BmsRandomChoiceOutOfRange"
    )));

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn deduplicates_visible_note_overlapping_random_branch() {
    let text = "\
#TITLE Random Duplicate
#BPM 120
#TOTAL 200
#WAV01 branch.wav
#WAV02 main.wav
#RANDOM 1
#IF 1
#00111:0000000000010000000000000000000000000000000000000000000000000000
#ENDIF
#ENDRANDOM
#00111:0000000000020000000000000000000000000000000000000000000000000000
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, Some(1), false).unwrap();

    let lane = result.chart.notes_for_lane(Lane::Key1);
    assert_eq!(lane.len(), 1);
    assert_eq!(result.chart.total_notes, 1);
    assert_eq!(
        lane[0].sound.map(|id| result.chart.sounds[id.0 as usize].path.file_name().unwrap()),
        Some(std::ffi::OsStr::new("main.wav")),
        "the later main-data definition should replace the RANDOM branch note"
    );

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn lnobj_without_marker_is_handled_outside_bms_rs() {
    // LNOBJ is stripped before bms-rs parse and resolved during lane normalization.
    let text = "\
#TITLE Diagnostic
#BPM 120
#TOTAL 200
#LNOBJ ZZ
#00011:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    let has_undefined = result.warnings.iter().any(|w| {
        matches!(
            w,
            crate::import::error::ImportWarning::ParserDiagnostic { code, .. }
                if code == "ParseUndefinedObject"
        )
    });
    assert!(!has_undefined, "warnings: {:?}", result.warnings);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_forward_declared_lnobj_as_long_note() {
    let text = "\
#TITLE LNOBJ Song
#BPM 120
#TOTAL 200
#WAV01 key.wav
#WAVZZ marker.wav
#LNOBJ ZZ
#00111:01ZZ
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    assert!(
        result.warnings.iter().all(|w| {
            !matches!(
                w,
                crate::import::error::ImportWarning::ParserDiagnostic { code, .. }
                    if code == "ParseUndefinedObject"
            )
        }),
        "warnings: {:?}",
        result.warnings
    );

    let lane = result.chart.notes_for_lane(Lane::Key1);
    assert_eq!(lane.len(), 2, "lane notes: {lane:?}");
    assert_eq!(lane[0].kind, NoteKind::LongStart);
    assert_eq!(lane[1].kind, NoteKind::LongEnd);
    assert!(lane[0].sound.is_some());
    assert!(lane[1].sound.is_none());
    assert_eq!(result.chart.long_notes.len(), 1);
    assert_eq!(result.chart.long_notes[0].style, LongNoteStyle::LnObj);
    assert_eq!(result.chart.total_notes, 1);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_long_channel_pair_with_matching_end_wav_as_silent_end() {
    let text = "\
#TITLE Long End Marker
#BPM 120
#TOTAL 200
#WAV01 key.wav
#00151:0101
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    let lane = result.chart.notes_for_lane(Lane::Key1);
    assert_eq!(lane.len(), 2, "lane notes: {lane:?}");
    assert_eq!(lane[0].kind, NoteKind::LongStart);
    assert_eq!(lane[1].kind, NoteKind::LongEnd);
    assert!(lane[0].sound.is_some());
    assert_eq!(lane[1].sound, None);
    assert_eq!(result.chart.long_notes.len(), 1);
    assert_eq!(result.chart.long_notes[0].style, LongNoteStyle::ChannelPair);
    assert_eq!(result.chart.total_notes, 1);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn merges_visible_note_at_long_channel_start() {
    let text = "\
#TITLE Layered Long Start
#BPM 120
#TOTAL 200
#WAV01 key.wav
#WAV02 end.wav
#00111:01
#00151:0102
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    let lane = result.chart.notes_for_lane(Lane::Key1);
    assert_eq!(lane.len(), 2, "lane notes: {lane:?}");
    assert_eq!(lane[0].kind, NoteKind::LongStart);
    assert_eq!(lane[1].kind, NoteKind::LongEnd);
    assert_eq!(result.chart.long_notes.len(), 1);
    assert_eq!(result.chart.total_notes, 1);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn merges_long_channel_start_without_disturbing_lnobj_pair() {
    let text = "\
#TITLE Layered Long Start With LNOBJ
#BPM 120
#TOTAL 200
#WAV01 layered.wav
#WAV02 lnobj-start.wav
#WAV03 long-end.wav
#WAVZZ marker.wav
#LNOBJ ZZ
#00111:0102ZZ
#00151:010003
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    assert_eq!(result.chart.long_notes.len(), 2);
    assert_eq!(result.chart.total_notes, 2);
    assert_eq!(
        result
            .chart
            .notes_for_lane(Lane::Key1)
            .iter()
            .filter(|note| note.kind == NoteKind::Tap)
            .count(),
        0
    );

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn removes_visible_notes_covered_by_long_channel_pair() {
    let text = "\
#TITLE Notes Inside Long Note
#BPM 120
#TOTAL 200
#WAV01 start.wav
#WAV02 inside.wav
#WAV03 inside.wav
#WAV04 end.wav
#00111:01020304
#00151:01000004
#00211:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();

    let lane = result.chart.notes_for_lane(Lane::Key1);
    assert_eq!(result.chart.long_notes.len(), 1);
    assert_eq!(result.chart.total_notes, 2);
    assert_eq!(
        lane.iter().filter(|note| note.kind == NoteKind::Tap).count(),
        1,
        "lane notes: {lane:?}"
    );

    std::fs::remove_file(&path).unwrap();
}
