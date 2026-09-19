use bmz_chart::import::{BmsRandomSource, import_chart, import_chart_with_random_source};
use bmz_chart::model::LongNoteMode;
use bmz_core::lane::Lane;

#[test]
fn hln_header_pairs_and_keysound_ids_are_preserved_and_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hln.bmc");
    std::fs::write(&path, "#TITLE HLN\n#BPM 120\n#TOTAL 200\n#LNMODE 4\n#WAV01 key.wav\n#WAV02 tail.wav\n#00051:0102\n#00052:0101\n#00013:01\n#00001:01\n").unwrap();
    let result = import_chart(&path, None, false).unwrap();
    let chart = &result.chart;
    assert_eq!(chart.metadata.long_note_mode, LongNoteMode::Hln);
    assert_eq!(chart.long_notes.len(), 2);
    assert!(chart.long_notes.iter().all(|pair| pair.mode == Some(LongNoteMode::Hln)));
    assert_eq!(chart.total_notes, 3);
    let first = chart.long_notes[0].sound.unwrap();
    let second = chart.long_notes[1].sound.unwrap();
    assert_ne!(first, second);
    assert_ne!(first, chart.bgm_events[0].sound);
    assert_ne!(second, chart.notes_for_lane(Lane::Key3)[0].sound.unwrap());
    let first_asset = chart.sounds.iter().find(|sound| sound.id == first).unwrap();
    let second_asset = chart.sounds.iter().find(|sound| sound.id == second).unwrap();
    assert_eq!(first_asset.path, second_asset.path);
    assert!(chart.note_by_id(chart.long_notes[0].end_note_id).unwrap().sound.is_some());
    assert!(chart.note_by_id(chart.long_notes[1].end_note_id).unwrap().sound.is_none());
}

#[test]
fn hlnobj_and_lnobj_can_coexist_without_turning_markers_into_sounds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.bms");
    std::fs::write(&path, "#TITLE Mixed\n#BPM 120\n#TOTAL 200\n#WAV01 key.wav\n#LNOBJ ZZ\n#HLNOBJ YY\n#00011:01YY01ZZ\n").unwrap();
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.long_notes.len(), 2);
    assert_eq!(result.chart.long_notes[0].mode, Some(LongNoteMode::Hln));
    assert_eq!(result.chart.long_notes[1].mode, None);
    assert_eq!(result.chart.total_notes, 2);
    for pair in &result.chart.long_notes {
        assert!(result.chart.note_by_id(pair.end_note_id).unwrap().sound.is_none());
    }
}

#[test]
fn hln_headers_follow_selected_random_branch_and_last_valid_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("random.bms");
    let text = "#TITLE Random\n#BPM 120\n#TOTAL 200\n#WAV01 key.wav\n#RANDOM 2\n#IF 1\n#LNMODE 4\n#ENDIF\n#IF 2\n#LNMODE 3\n#ENDIF\n#ENDRANDOM\n#00051:0101\n";
    std::fs::write(&path, text).unwrap();
    for (choice, expected) in [(1, LongNoteMode::Hln), (2, LongNoteMode::Hcn)] {
        let result = import_chart_with_random_source(
            &path,
            BmsRandomSource::Choices { random: vec![choice], switches: vec![] },
            false,
        )
        .unwrap();
        assert_eq!(result.chart.long_notes[0].mode, Some(expected));
    }
    std::fs::write(&path, format!("{text}\n#LNMODE 2\n")).unwrap();
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.long_notes[0].mode, Some(LongNoteMode::Cn));
}
