use bmz_chart::import::{BmsRandomSource, import_chart, import_chart_with_random_source};
use bmz_chart::model::LongNoteMode;
use bmz_core::lane::Lane;

#[test]
fn hlnobj_overrides_lnobj_regardless_of_header_order_and_preserves_end_sound() {
    for headers in ["#LNOBJ ZZ\n#HLNOBJ ZZ", "#HLNOBJ ZZ\n#LNOBJ ZZ"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("compatible.bms");
        std::fs::write(&path, format!("#TITLE HLN\n#BPM 120\n#WAV01 head.wav\n#WAV02 tail.wav\n{headers}\n#00011:01ZZ\n#00011:0002\n")).unwrap();
        let chart = import_chart(&path, None, false).unwrap().chart;
        assert_eq!(chart.long_notes.len(), 1);
        let pair = &chart.long_notes[0];
        assert_eq!(pair.mode, Some(LongNoteMode::Hln));
        let tail = chart.note_by_id(pair.end_note_id).unwrap();
        let id = tail.sound.unwrap();
        assert!(
            chart.sounds.iter().find(|sound| sound.id == id).unwrap().path.ends_with("tail.wav")
        );
    }
}

#[test]
fn bmson_hln_inherits_type_four_and_respects_per_note_types() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("types.bmson");
    let json = serde_json::json!({
        "version": "1.0.0",
        "info": {"title":"HLN", "artist":"a", "genre":"g", "level":1, "init_bpm":120, "resolution":240, "mode_hint":"beat-7k", "ln_type":4},
        "sound_channels": [{"name":"head.wav", "notes":[
            {"x":1,"y":0,"l":240,"c":false},
            {"x":2,"y":0,"l":240,"c":false,"t":1},
            {"x":3,"y":0,"l":240,"c":false,"t":2},
            {"x":4,"y":0,"l":240,"c":false,"t":3},
            {"x":5,"y":0,"l":240,"c":false,"t":4}
        ]}]
    });
    std::fs::write(&path, json.to_string()).unwrap();
    let chart = import_chart(&path, None, false).unwrap().chart;
    assert_eq!(chart.metadata.long_note_mode, LongNoteMode::Hln);
    for (lane, mode) in [
        (Lane::Key1, LongNoteMode::Hln),
        (Lane::Key2, LongNoteMode::Ln),
        (Lane::Key3, LongNoteMode::Cn),
        (Lane::Key4, LongNoteMode::Hcn),
        (Lane::Key5, LongNoteMode::Hln),
    ] {
        assert_eq!(
            chart.long_notes.iter().find(|pair| pair.lane == lane).unwrap().mode,
            Some(mode)
        );
    }
}

#[test]
fn bmson_type_four_uses_the_same_up_end_sound_rule_as_ln() {
    for up in [false, true] {
        for mode in [1, 4] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("end.bmson");
            let json = serde_json::json!({
                "version":"1.0.0",
                "info":{"title":"HLN","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"beat-7k"},
                "sound_channels":[
                    {"name":"head.wav","notes":[{"x":1,"y":0,"l":240,"c":false,"t":mode}]},
                    {"name":"tail.wav","notes":[{"x":1,"y":240,"l":0,"c":false,"up":up}]}
                ]
            });
            std::fs::write(&path, json.to_string()).unwrap();
            let chart = import_chart(&path, None, false).unwrap().chart;
            let pair = &chart.long_notes[0];
            assert_eq!(
                pair.mode,
                Some(if mode == 4 { LongNoteMode::Hln } else { LongNoteMode::Ln })
            );
            let tail = chart.note_by_id(pair.end_note_id).unwrap();
            assert_eq!(tail.sound.is_some(), up);
            if let Some(id) = tail.sound {
                assert!(
                    chart
                        .sounds
                        .iter()
                        .find(|sound| sound.id == id)
                        .unwrap()
                        .path
                        .ends_with("tail.wav")
                );
            }
        }
    }
}

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
