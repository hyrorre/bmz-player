use super::*;
use crate::text::chart_text_at_time;
use bmz_core::time::TimeUs;

const SECOND: i64 = 1_000_000;

fn import_bytes(bytes: &[u8], random_source: BmsRandomSource) -> ImportResult {
    import_chart_bytes_with_random_source(Path::new("text.bms"), bytes, random_source, false)
        .unwrap()
}

fn import_text(text: &str) -> ImportResult {
    import_bytes(text.as_bytes(), BmsRandomSource::Seed(Some(1)))
}

fn text_events(result: &ImportResult) -> Vec<(i64, String)> {
    result.chart.text_events.iter().map(|event| (event.time.0, event.text.to_string())).collect()
}

// BPM 120: one measure is two seconds.
const HEADER: &str = "#TITLE Text\n#BPM 120\n#00111:01\n";

#[test]
fn text_events_switch_hold_and_clear_with_empty_definitions() {
    let result = import_text(&format!(
        "{HEADER}#TEXT01 \"Hello World\"\n#TEXT02 \"Second message\"\n#TEXT03 \"\"\n\
         #00199:01000200\n#00299:03000000\n"
    ));
    assert_eq!(
        text_events(&result),
        [
            (2 * SECOND, "Hello World".to_string()),
            (3 * SECOND, "Second message".to_string()),
            (4 * SECOND, String::new()),
        ]
    );
    let events = &result.chart.text_events;
    assert_eq!(chart_text_at_time(events, TimeUs(0)), "");
    assert_eq!(chart_text_at_time(events, TimeUs(2 * SECOND + 999_999)), "Hello World");
    // `00` slots are empty objects and keep the current text.
    assert_eq!(chart_text_at_time(events, TimeUs(3 * SECOND + 999_999)), "Second message");
    assert_eq!(chart_text_at_time(events, TimeUs(60 * SECOND)), "");
}

#[test]
fn undefined_references_clear_text_without_dropping_the_rest_of_the_message() {
    let result = import_text(&format!("{HEADER}#TEXT01 A\n#TEXT02 B\n#00199:01ZZ0201\n"));
    assert_eq!(
        text_events(&result),
        [
            (2 * SECOND, "A".to_string()),
            (2 * SECOND + SECOND / 2, String::new()),
            (3 * SECOND, "B".to_string()),
            (3 * SECOND + SECOND / 2, "A".to_string()),
        ]
    );
}

#[test]
fn simultaneous_events_and_repeated_definitions_keep_source_order() {
    let result = import_text(&format!(
        "{HEADER}#TEXT01 First\n#TEXT02 Second\n#00199:01\n#00199:02\n#00299:02\n"
    ));
    assert_eq!(
        text_events(&result),
        [
            (2 * SECOND, "First".to_string()),
            (2 * SECOND, "Second".to_string()),
            (4 * SECOND, "Second".to_string()),
        ]
    );
    assert_eq!(chart_text_at_time(&result.chart.text_events, TimeUs(2 * SECOND)), "Second");

    let redefined = import_text(&format!("{HEADER}#TEXT01 Old\n#text01 New\n#00199:01\n"));
    assert_eq!(text_events(&redefined), [(2 * SECOND, "New".to_string())]);
}

#[test]
fn text_values_keep_japanese_inner_quotes_long_lines_and_crlf() {
    let long = "長".repeat(2_000);
    let source = format!(
        "#TITLE Text\r\n#BPM 120\r\n#00111:01\r\n#TEXT01 \"日本語のテキスト\"\r\n\
         #TEXT02 He said \"hi\"\r\n#TEXT03 \"\"quoted\"\"\r\n#TEXT04 {long}\r\n\
         #TEXT05   \"  padded  \"  \r\n#00199:0102030405\r\n"
    );
    let (bytes, _, unmappable) = encoding_rs::SHIFT_JIS.encode(&source);
    assert!(!unmappable);
    let result = import_bytes(&bytes, BmsRandomSource::Seed(Some(1)));
    let texts: Vec<_> = text_events(&result).into_iter().map(|(_, text)| text).collect();
    assert_eq!(
        texts,
        ["日本語のテキスト", "He said \"hi\"", "\"quoted\"", long.as_str(), "  padded  "]
    );
}

#[test]
fn random_branches_select_definitions_and_messages() {
    let source = format!(
        "{HEADER}#TEXT01 Shared\n#RANDOM 2\n#IF 1\n#TEXT02 One\n#00199:02\n#ENDIF\n\
         #IF 2\n#TEXT02 Two\n#00199:0201\n#ENDIF\n#ENDRANDOM\n"
    );
    for (choice, expected) in [
        (1, vec![(2 * SECOND, "One".to_string())]),
        (2, vec![(2 * SECOND, "Two".to_string()), (3 * SECOND, "Shared".to_string())]),
    ] {
        let result = import_bytes(
            source.as_bytes(),
            BmsRandomSource::Choices { random: vec![choice], switches: Vec::new() },
        );
        assert_eq!(text_events(&result), expected, "choice {choice}");
    }
}

#[test]
fn base62_object_ids_are_case_sensitive_only_with_base62() {
    let base36 = import_text(&format!("{HEADER}#TEXTaa lower\n#TEXTAA upper\n#00199:aaAA\n"));
    assert_eq!(
        text_events(&base36),
        [(2 * SECOND, "upper".to_string()), (3 * SECOND, "upper".to_string())]
    );

    let base62 =
        import_text(&format!("#BASE 62\n{HEADER}#TEXTaa lower\n#TEXTAA upper\n#00199:aaAA\n"));
    assert_eq!(
        text_events(&base62),
        [(2 * SECOND, "lower".to_string()), (3 * SECOND, "upper".to_string())]
    );
}

#[test]
fn sparse_text_messages_are_imported_from_source() {
    let mut payload = "01".to_string();
    payload.push_str(&"00".repeat(9_000));
    payload.push_str("02");
    let result = import_text(&format!("{HEADER}#TEXT01 Start\n#TEXT02 End\n#00199:{payload}\n"));
    let events = text_events(&result);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], (2 * SECOND, "Start".to_string()));
    assert_eq!(events[1].1, "End");
    assert!(events[1].0 > 3 * SECOND && events[1].0 < 4 * SECOND);
}

#[test]
fn malformed_text_commands_do_not_fail_import() {
    let result = import_text(&format!(
        "{HEADER}#TEXT\n#TEXT1 short\n#TEXT001 long\n#TEXT0! bad\n#TEXT01 ok\n\
         #00199:0\n#00199:!!01\n#99999999999999999999999:01\n"
    ));
    assert_eq!(text_events(&result), [(3 * SECOND, "ok".to_string())]);
}

#[test]
fn legacy_song_definitions_are_text_definitions() {
    let result = import_text(&format!("{HEADER}#SONG01 \"Legacy Message\"\n#00199:0100\n"));
    assert_eq!(text_events(&result), [(2 * SECOND, "Legacy Message".to_string())]);
    assert_eq!(result.chart.metadata.miss_text, None);
}

#[test]
fn text_definitions_win_over_song_definitions_regardless_of_order() {
    for source in [
        "#SONG01 \"Old\"\n#TEXT01 \"New\"\n",
        "#TEXT01 \"New\"\n#SONG01 \"Old\"\n",
        "#song01 Old\n#TEXT01 Other\n#SONG01 Old2\n#text01 New\n",
    ] {
        let result = import_text(&format!("{HEADER}{source}#00199:0100\n"));
        assert_eq!(text_events(&result), [(2 * SECOND, "New".to_string())], "{source}");
    }
    // An empty #TEXT definition still wins over #SONG.
    let empty = import_text(&format!("{HEADER}#TEXT01 \"\"\n#SONG01 Old\n#00199:01\n"));
    assert_eq!(text_events(&empty), [(2 * SECOND, String::new())]);
    // Other IDs still fall back to #SONG, and repeated #SONG uses the later line.
    let mixed = import_text(&format!(
        "{HEADER}#TEXT01 Text\n#SONG02 Song A\n#SONG02 Song B\n#00199:0102\n"
    ));
    assert_eq!(
        text_events(&mixed),
        [(2 * SECOND, "Text".to_string()), (3 * SECOND, "Song B".to_string())]
    );
}

#[test]
fn song_definitions_follow_random_branches_and_base62() {
    let source = format!(
        "#BASE 62\n{HEADER}#SONGab lower\n#RANDOM 2\n#IF 1\n#TEXTab chosen\n#ENDIF\n\
         #IF 2\n#SONGAB upper\n#ENDIF\n#ENDRANDOM\n#00199:abAB\n"
    );
    let one = import_bytes(
        source.as_bytes(),
        BmsRandomSource::Choices { random: vec![1], switches: Vec::new() },
    );
    assert_eq!(
        text_events(&one),
        [(2 * SECOND, "chosen".to_string()), (3 * SECOND, String::new())]
    );
    let two = import_bytes(
        source.as_bytes(),
        BmsRandomSource::Choices { random: vec![2], switches: Vec::new() },
    );
    assert_eq!(
        text_events(&two),
        [(2 * SECOND, "lower".to_string()), (3 * SECOND, "upper".to_string())]
    );
}

#[test]
fn text00_is_the_miss_message_and_not_a_channel_99_event() {
    let result = import_text(&format!(
        "{HEADER}#TEXT00 \"MISS!!\"\n#TEXT01 \"Normal message\"\n#00199:0100\n"
    ));
    assert_eq!(result.chart.metadata.miss_text.as_deref(), Some("MISS!!"));
    assert_eq!(text_events(&result), [(2 * SECOND, "Normal message".to_string())]);

    let without = import_text(&format!("{HEADER}#TEXT01 A\n#00199:01\n"));
    assert_eq!(without.chart.metadata.miss_text, None);
    // #SONG00 is outside the compatible range and is ignored.
    let song00 = import_text(&format!("{HEADER}#SONG00 MISS\n#00199:01\n"));
    assert_eq!(song00.chart.metadata.miss_text, None);
    assert_eq!(text_events(&song00), [(2 * SECOND, String::new())]);
    let empty = import_text(&format!("{HEADER}#TEXT00 \"\"\n#TEXT00 Later\n"));
    assert_eq!(empty.chart.metadata.miss_text.as_deref(), Some("Later"));
}

#[test]
fn text_import_keeps_the_raw_chart_identity() {
    let source = format!("{HEADER}#SONG01 Old\n#TEXT01 New\n#TEXT00 MISS\n#00199:01\n");
    let result = import_text(&source);
    assert_eq!(result.chart.identity, compute_chart_identity(source.as_bytes()));
    assert_eq!(result.chart.metadata.title, "Text");
    assert_eq!(result.chart.total_notes, 1);
}

#[test]
fn documented_chart_text_sample_imports() {
    let path = repo_root().join("docs/examples/chart-text/chart-text.bms");
    let result = import_bms_chart(&path, Some(1), false).unwrap();
    assert_eq!(result.chart.metadata.miss_text.as_deref(), Some("MISS!!"));
    assert_eq!(
        text_events(&result),
        [
            (2 * SECOND, "Hello World".to_string()),
            (3 * SECOND, "Second message".to_string()),
            (4 * SECOND, "Hello World".to_string()),
            (5 * SECOND, String::new()),
            (6 * SECOND, "Legacy message".to_string()),
        ]
    );
}
