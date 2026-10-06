use super::*;

#[test]
fn byte_import_matches_file_import_and_keeps_raw_identity_for_all_formats() {
    let fixtures = [
        (
            "bms",
            "#RANDOM 2\n#IF 1\n#TITLE First\n#ENDIF\n#IF 2\n#TITLE Second\n#ENDIF\n#BPM 120\n#00011:01\n",
        ),
        ("pms", "#TITLE Pop\n#BPM 120\n#00011:01\n#00019:01\n"),
        (
            "bmson",
            r#"{"version":"1.0.0","info":{"title":"Bytes","artist":"Test","genre":"Test","level":5,"init_bpm":120.0,"judge_rank":100.0,"total":200.0,"resolution":240},"sound_channels":[]}"#,
        ),
    ];
    for (extension, text) in fixtures {
        let path = write_temp_file_with_ext(text, extension);
        let from_file = import_chart(&path, Some(12), false).unwrap();
        std::fs::remove_file(&path).unwrap();
        let from_bytes = import_chart_bytes_with_random_source(
            &path,
            text.as_bytes(),
            BmsRandomSource::Seed(Some(12)),
            false,
        )
        .unwrap();
        assert_eq!(from_bytes.chart.identity, compute_chart_identity(text.as_bytes()));
        assert_eq!(from_bytes.chart.identity, from_file.chart.identity);
        assert_eq!(from_bytes.chart.metadata.title, from_file.chart.metadata.title);
        assert_eq!(from_bytes.chart.metadata.key_mode, from_file.chart.metadata.key_mode);
        assert_eq!(from_bytes.chart.metadata.source_format, from_file.chart.metadata.source_format);
        assert_eq!(from_bytes.chart.total_notes, from_file.chart.total_notes);
        assert_eq!(from_bytes.bms_random_choices, from_file.bms_random_choices);
    }
}
