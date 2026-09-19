use bmz_chart::import::{BmsRandomSource, import_chart, import_chart_with_random_source};
use bmz_chart::model::{LongNoteMode, NoteKind, PlayableChart};
use bmz_core::lane::Lane;

fn import(body: &str) -> PlayableChart {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long-end.bms");
    std::fs::write(&path, format!("#TITLE End\n#BPM 120\n#TOTAL 200\n#WAV01 head.wav\n#WAV02 tail2.wav\n#WAV03 tail3.wav\n#WAVZZ marker.wav\n{body}\n")).unwrap();
    import_chart(&path, None, false).unwrap().chart
}

fn tail_paths(chart: &PlayableChart) -> Vec<String> {
    let end = chart.note_by_id(chart.long_notes[0].end_note_id).unwrap();
    end.sounds()
        .map(|id| {
            chart
                .sounds
                .iter()
                .find(|asset| asset.id == id)
                .unwrap()
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn charge_end_layers_are_order_independent_and_deduplicate_wav_ids() {
    let rows = ["#00011:00ZZ", "#00011:00000200", "#00011:00000300"];
    for command in ["CNOBJ", "HCNOBJ", "HLNOBJ"] {
        for order in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            let body = order.map(|i| rows[i]).join("\n");
            let chart = import(&format!("#{command} ZZ\n#00011:01\n{body}\n#00011:00000200"));
            assert_eq!(chart.long_notes.len(), 1);
            assert_eq!(tail_paths(&chart), ["tail2.wav", "tail3.wav"], "{command}, {order:?}");
            assert_eq!(chart.notes_for_lane(Lane::Key1).len(), 2);
            assert!(chart.bgm_events.is_empty());
        }
    }
}

#[test]
fn typed_markers_override_lnmode_and_lnobj_and_last_typed_definition_wins() {
    for (headers, expected) in [
        ("#LNMODE 3\n#CNOBJ ZZ\n#LNOBJ ZZ", LongNoteMode::Cn),
        ("#LNMODE 1\n#HCNOBJ ZZ\n#LNOBJ ZZ", LongNoteMode::Hcn),
        ("#HCNOBJ ZZ\n#CNOBJ ZZ", LongNoteMode::Cn),
        ("#CNOBJ ZZ\n#HCNOBJ ZZ", LongNoteMode::Hcn),
        ("#HLNOBJ ZZ\n#CNOBJ ZZ", LongNoteMode::Cn),
        ("#CNOBJ ZZ\n#HLNOBJ ZZ", LongNoteMode::Hln),
    ] {
        let chart = import(&format!("{headers}\n#00011:01ZZ\n#00011:0002"));
        assert_eq!(chart.long_notes[0].mode, Some(expected), "{headers}");
        assert_eq!(tail_paths(&chart), ["tail2.wav"]);
    }
}

#[test]
fn collocated_distinct_markers_choose_last_defined_type_and_never_become_sound() {
    for rows in ["#00011:00ZZ\n#00011:00YY", "#00011:00YY\n#00011:00ZZ"] {
        let chart = import(&format!("#CNOBJ ZZ\n#HCNOBJ YY\n#00011:01\n{rows}\n#00011:0002"));
        assert_eq!(chart.long_notes.len(), 1);
        assert_eq!(chart.long_notes[0].mode, Some(LongNoteMode::Hcn));
        assert_eq!(tail_paths(&chart), ["tail2.wav"]);
    }
}

#[test]
fn marker_redefinition_releases_old_id_and_invalid_definition_does_not_erase_valid_one() {
    let chart = import("#CNOBJ 03\n#CNOBJ ZZ\n#CNOBJ invalid\n#00011:01ZZ\n#00011:0003");
    assert_eq!(chart.long_notes[0].mode, Some(LongNoteMode::Cn));
    assert_eq!(tail_paths(&chart), ["tail3.wav"]);
}

#[test]
fn all_obj_types_coexist_and_generic_marker_inherits_lnmode() {
    let chart =
        import("#LNMODE 2\n#LNOBJ WW\n#CNOBJ XX\n#HCNOBJ YY\n#HLNOBJ ZZ\n#00011:01WW01XX01YY01ZZ");
    assert_eq!(
        chart.long_notes.iter().map(|pair| pair.mode).collect::<Vec<_>>(),
        [
            Some(LongNoteMode::Cn),
            Some(LongNoteMode::Cn),
            Some(LongNoteMode::Hcn),
            Some(LongNoteMode::Hln)
        ]
    );
    assert!(
        chart
            .lane_notes
            .iter()
            .flatten()
            .filter(|note| note.kind == NoteKind::LongEnd)
            .all(|note| note.sounds().next().is_none())
    );
}

#[test]
fn ordinary_duplicate_notes_keep_existing_last_wins_rule() {
    let chart = import("#CNOBJ ZZ\n#00011:0100ZZ00\n#00012:0200\n#00012:0300");
    let tap = &chart.notes_for_lane(Lane::Key2)[0];
    assert_eq!(tap.kind, NoteKind::Tap);
    assert_eq!(tap.sounds().count(), 1);
    assert!(
        chart
            .sounds
            .iter()
            .find(|asset| Some(asset.id) == tap.sound)
            .unwrap()
            .path
            .ends_with("tail3.wav")
    );
}

#[test]
fn typed_markers_respect_random_selection_and_base62() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("random.bms");
    std::fs::write(&path, "#TITLE End\n#BPM 120\n#TOTAL 200\n#BASE 62\n#WAV01 head.wav\n#WAVaA tail.wav\n#RANDOM 2\n#IF 1\n#CNOBJ zz\n#ENDIF\n#IF 2\n#HCNOBJ zz\n#ENDIF\n#ENDRANDOM\n#00011:01zz\n#00011:00aA").unwrap();
    for (branch, expected) in [(1, LongNoteMode::Cn), (2, LongNoteMode::Hcn)] {
        let result = import_chart_with_random_source(
            &path,
            BmsRandomSource::Choices { random: vec![branch], switches: vec![] },
            false,
        )
        .unwrap();
        assert_eq!(result.chart.long_notes[0].mode, Some(expected));
        assert_eq!(tail_paths(&result.chart), ["tail.wav"]);
    }
}

#[test]
fn same_wav_at_head_and_tail_has_independent_volume_control() {
    let chart = import("#HCNOBJ ZZ\n#00011:01ZZ\n#00011:0001");
    let pair = &chart.long_notes[0];
    let head = chart.note_by_id(pair.start_note_id).unwrap().sound.unwrap();
    let tail = chart.note_by_id(pair.end_note_id).unwrap().sound.unwrap();
    assert_ne!(head, tail);
    assert_eq!(tail_paths(&chart), ["head.wav"]);
}

#[test]
fn dedicated_long_channel_blocks_marker_pairs_and_merges_end_layers() {
    let chart = import("#LNMODE 3\n#CNOBJ ZZ\n#00051:0101\n#00011:01ZZ\n#00011:0002");
    assert_eq!(chart.long_notes.len(), 1);
    assert_eq!(chart.long_notes[0].mode, Some(LongNoteMode::Hcn));
    assert_eq!(tail_paths(&chart), ["tail2.wav"]);
    let chart = import("#CNOBJ ZZ\n#00011:010000ZZ\n#00051:00010200");
    assert_eq!(chart.long_notes.len(), 1);
    assert_eq!(chart.long_notes[0].mode, None);
}

#[test]
fn orphan_marker_and_its_layers_do_not_become_tap_notes() {
    let chart = import("#CNOBJ ZZ\n#00011:ZZ\n#00011:02");
    assert!(chart.long_notes.is_empty());
    assert!(chart.notes_for_lane(Lane::Key1).is_empty());
    assert!(chart.bgm_events.is_empty());
}
