use std::path::Path;

use bmz_core::lane::KeyMode;

use super::*;

const PMS_HEADER: &str = "\
#TITLE PMS Test
#ARTIST Tester
#BPM 120
#WAV01 key.wav
";

fn pms_note_lines_standard() -> String {
    let mut lines = String::from(PMS_HEADER);
    for (i, channel) in
        ["11", "12", "13", "14", "15", "22", "23", "24", "25"].into_iter().enumerate()
    {
        let measure = i + 1;
        lines.push_str(&format!("#{measure:03}{channel}:01\n"));
    }
    lines
}

fn pms_note_lines_bme() -> String {
    let mut lines = String::from(PMS_HEADER);
    for (i, channel) in
        ["11", "12", "13", "14", "15", "16", "17", "18", "19"].into_iter().enumerate()
    {
        let measure = i + 1;
        lines.push_str(&format!("#{measure:03}{channel}:01\n"));
    }
    lines
}

fn import_pms_text(text: &str) -> IntermediateChart {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pms");
    std::fs::write(&path, text).unwrap();
    std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
    let mut warnings = Vec::new();
    import_pms_to_intermediate(&path, None, &mut warnings).unwrap()
}

fn note_lanes(chart: &IntermediateChart) -> Vec<Lane> {
    chart
        .objects
        .iter()
        .filter_map(|object| match object.kind {
            IntermediateObjectKind::VisibleNote { lane, .. } => Some(lane),
            _ => None,
        })
        .collect()
}

fn playable_lane_counts(chart: &IntermediateChart) -> [usize; bmz_core::lane::LANE_COUNT] {
    let mut counts = [0; bmz_core::lane::LANE_COUNT];
    for object in &chart.objects {
        let lane = match object.kind {
            IntermediateObjectKind::VisibleNote { lane, .. }
            | IntermediateObjectKind::InvisibleNote { lane, .. }
            | IntermediateObjectKind::LongChannelNote { lane, .. }
            | IntermediateObjectKind::MineNote { lane, .. } => lane,
            _ => continue,
        };
        counts[lane.index()] += 1;
    }
    counts
}

#[test]
fn detect_pms_variant_standard_from_p2_upper_channels() {
    let (variant, conflict) = detect_pms_variant(&pms_note_lines_standard());
    assert_eq!(variant, PmsKeyLayout::Standard);
    assert!(!conflict);
}

#[test]
fn detect_pms_variant_ignores_non_message_headers_with_colons() {
    let text = "\
#TITLE 赤 (原曲: 天衣無縫) [9K NORMAL]
#BPM 120
";
    let (variant, conflict) = detect_pms_variant(text);
    assert_eq!(variant, PmsKeyLayout::Standard);
    assert!(!conflict);
}

#[test]
fn detect_pms_variant_bme_from_p1_upper_channels() {
    let (variant, conflict) = detect_pms_variant(&pms_note_lines_bme());
    assert_eq!(variant, PmsKeyLayout::BmeType);
    assert!(!conflict);
}

#[test]
fn pms_standard_9k_maps_key1_through_key9() {
    let chart = import_pms_text(&pms_note_lines_standard());
    assert_eq!(chart.metadata.key_mode, KeyMode::K9);
    let lanes = note_lanes(&chart);
    assert_eq!(lanes.len(), 9);
    for (expected, actual) in [
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
        Lane::Key8,
        Lane::Key9,
    ]
    .into_iter()
    .zip(lanes)
    {
        assert_eq!(expected, actual);
    }
}

#[test]
fn pms_standard_drops_conflicting_bme_upper_channels() {
    let mut text = pms_note_lines_standard();
    text.push_str("#01018:01\n");

    let chart = import_pms_text(&text);

    assert_eq!(note_lanes(&chart).len(), 9);
    assert_eq!(playable_lane_counts(&chart)[Lane::Key8.index()], 1);
}

#[test]
fn pms_bme_9k_maps_key1_through_key9() {
    let chart = import_pms_text(&pms_note_lines_bme());
    assert_eq!(chart.metadata.key_mode, KeyMode::K9);
    let lanes = note_lanes(&chart);
    assert_eq!(lanes.len(), 9);
    assert!(lanes.contains(&Lane::Key9));
}

#[test]
fn pms_5k_still_reports_k9_key_mode() {
    let mut text = String::from(PMS_HEADER);
    for (i, channel) in ["11", "12", "13", "14", "15"].into_iter().enumerate() {
        let measure = i + 1;
        text.push_str(&format!("#{measure:03}{channel}:01\n"));
    }
    let chart = import_pms_text(&text);
    assert_eq!(chart.metadata.key_mode, KeyMode::K9);
    assert_eq!(note_lanes(&chart).len(), 5);
}

fn import_bms_text(text: &str) -> IntermediateChart {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.bms");
    std::fs::write(&path, text).unwrap();
    std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
    let mut warnings = Vec::new();
    import_bms_to_intermediate(&path, None, &mut warnings).unwrap()
}

#[test]
fn presentation_headers_use_selected_random_branch() {
    let chart = import_bms_text(
        "#BPM 120\n#LOADINGFILE fallback.png\n#SETRANDOM 1\n#IF 1\n#loadingfile images/ロード image.gif\n#READYFILE ready.gif\n#ENDIF\n#IF 2\n#LOADINGFILE wrong.gif\n#READYFILE wrong.gif\n#ENDIF\n#ENDRANDOM\n",
    );
    assert_eq!(chart.metadata.loading_file, "images/ロード image.gif");
    assert_eq!(chart.metadata.ready_file, "ready.gif");
    assert_eq!(chart.metadata.bms_headers.get("READYFILE").map(String::as_str), Some("wrong.gif"));
}

#[test]
fn presentation_headers_are_independent_and_default_empty() {
    let plain = import_bms_text("#BPM 120\n#STAGEFILE stage.gif\n");
    assert!(plain.metadata.loading_file.is_empty());
    assert!(plain.metadata.ready_file.is_empty());
    let loading = import_bms_text("#BPM 120\n#LOADINGFILE first.gif\n#LOADINGFILE second.png\n");
    assert_eq!(loading.metadata.loading_file, "second.png");
    assert!(loading.metadata.ready_file.is_empty());
}

fn import_bms_text_with_warnings(text: &str) -> (IntermediateChart, Vec<ImportWarning>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.bms");
    std::fs::write(&path, text).unwrap();
    std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
    let mut warnings = Vec::new();
    let chart = import_bms_to_intermediate(&path, None, &mut warnings).unwrap();
    (chart, warnings)
}

fn import_bms_text_with_control_choices(
    text: &str,
    random: Vec<i32>,
    switches: Vec<u64>,
) -> (IntermediateChart, Vec<ImportWarning>, Vec<i32>, Vec<u64>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.bms");
    std::fs::write(&path, text).unwrap();
    std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
    let mut warnings = Vec::new();
    let mut applied_random = Vec::new();
    let mut applied_switches = Vec::new();
    let chart = import_bms_to_intermediate_with_random_source(
        &path,
        &BmsRandomSource::Choices { random, switches },
        &mut applied_random,
        &mut applied_switches,
        &mut warnings,
    )
    .unwrap();
    (chart, warnings, applied_random, applied_switches)
}

const BMS_HEADER: &str = "\
#TITLE BMS Test
#ARTIST Tester
#BPM 120
#WAV01 key.wav
";

fn ue_8k_note_lines() -> String {
    let mut lines = String::from(BMS_HEADER);
    for (i, channel) in ["16", "11", "12", "13", "14", "15", "18", "19"].into_iter().enumerate() {
        let measure = i + 1;
        lines.push_str(&format!("#{measure:03}{channel}:01\n"));
    }
    lines
}

#[test]
fn detect_key_mode_from_headers_parses_qwilight_tags() {
    use bms_rs::bms::command::channel::mapper::KeyLayoutBeat;
    use bms_rs::bms::{default_config, parse_bms};

    let parse =
        |text: &str| parse_bms::<KeyLayoutBeat, _, _, _>(text, default_config()).bms.unwrap();

    assert_eq!(
        detect_key_mode_from_bms_headers(&parse("#4K\n"), ChartKeyLayout::beat()),
        Some(KeyMode::K4),
    );
    assert_eq!(
        detect_key_mode_from_bms_headers(&parse("#6K\n"), ChartKeyLayout::beat()),
        Some(KeyMode::K6),
    );
    assert_eq!(
        detect_key_mode_from_bms_headers(&parse("#8K\n"), ChartKeyLayout::beat()),
        Some(KeyMode::K8),
    );
    assert_eq!(
        detect_key_mode_from_bms_headers(&parse("* EXPANSION\n#6K\n#8K\n"), ChartKeyLayout::beat(),),
        Some(KeyMode::K8),
    );
    assert_eq!(
        detect_key_mode_from_bms_headers(&parse("#TITLE x\n"), ChartKeyLayout::beat()),
        None,
    );
    assert_eq!(
        detect_key_mode_from_bms_headers(
            &parse("#8K\n"),
            ChartKeyLayout::pms(PmsKeyLayout::Standard),
        ),
        None,
    );
}

#[test]
fn bms_8k_header_overrides_lane_detected_k7() {
    let mut text = ue_8k_note_lines();
    text.push_str("#8K\n");
    let chart = import_bms_text(&text);
    assert_eq!(chart.metadata.key_mode, KeyMode::K8);
}

#[test]
fn bms_8k_header_maps_ue_channels_to_eight_key_lanes() {
    let mut text = ue_8k_note_lines();
    text.push_str("#8K\n");

    let chart = import_bms_text(&text);

    assert_eq!(chart.metadata.key_mode, KeyMode::K8);
    assert_eq!(
        note_lanes(&chart),
        vec![
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
            Lane::Key6,
            Lane::Key7,
            Lane::Key8,
        ],
    );
}

#[test]
fn bms_without_qwilight_header_uses_lane_detect() {
    let chart = import_bms_text(&ue_8k_note_lines());
    assert_eq!(chart.metadata.key_mode, KeyMode::K7);
}

#[test]
fn bms_4k_and_6k_headers_set_key_mode() {
    let mut text = ue_8k_note_lines();
    text.push_str("#4K\n");
    assert_eq!(import_bms_text(&text).metadata.key_mode, KeyMode::K4);

    let mut text = ue_8k_note_lines();
    text.push_str("#6K\n");
    assert_eq!(import_bms_text(&text).metadata.key_mode, KeyMode::K6);
}

#[test]
fn bms_4k_header_maps_ue_channels_to_four_key_lanes() {
    let mut text = String::from(BMS_HEADER);
    text.push_str("#4K\n");
    for (i, channel) in ["11", "12", "14", "15"].into_iter().enumerate() {
        let measure = i + 1;
        text.push_str(&format!("#{measure:03}{channel}:01\n"));
    }

    let chart = import_bms_text(&text);

    assert_eq!(chart.metadata.key_mode, KeyMode::K4);
    assert_eq!(note_lanes(&chart), vec![Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4],);
}

#[test]
fn bms_6k_header_maps_ue_channels_to_six_key_lanes() {
    let mut text = String::from(BMS_HEADER);
    text.push_str("#6K\n");
    for (i, channel) in ["11", "12", "13", "15", "18", "19"].into_iter().enumerate() {
        let measure = i + 1;
        text.push_str(&format!("#{measure:03}{channel}:01\n"));
    }

    let chart = import_bms_text(&text);

    assert_eq!(chart.metadata.key_mode, KeyMode::K6);
    assert_eq!(
        note_lanes(&chart),
        vec![Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6],
    );
}

#[test]
#[ignore = "requires local 6K U_E FULL PACK sample data"]
fn bms_6k_full_pack_sample_uses_six_active_lanes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../data/songs/6K U_E FULL PACK 3.1/234 [HAPPY HARDCORE] Blue-White Crazybits/crazybits6bit.bms",
    );
    assert!(path.exists(), "missing sample chart: {}", path.display());

    let mut warnings = Vec::new();
    let chart = import_bms_to_intermediate(&path, None, &mut warnings).unwrap();
    let counts = playable_lane_counts(&chart);

    assert_eq!(chart.metadata.key_mode, KeyMode::K6);
    for lane in [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6] {
        assert!(counts[lane.index()] > 0, "{lane:?} has no playable objects");
    }
    assert_eq!(counts[Lane::Scratch.index()], 0);
    assert_eq!(counts[Lane::Key7.index()], 0);
}

#[test]
#[ignore = "requires local 4K U_E FULL PACK sample data"]
fn bms_4k_full_pack_sample_uses_four_active_lanes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/songs/4K U_E FULL PACK 2.1/[kozato] Marion/_Marion_4Pursuit.bml");
    assert!(path.exists(), "missing sample chart: {}", path.display());

    let mut warnings = Vec::new();
    let chart = import_bms_to_intermediate(&path, None, &mut warnings).unwrap();
    let counts = playable_lane_counts(&chart);

    assert_eq!(chart.metadata.key_mode, KeyMode::K4);
    for lane in [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4] {
        assert!(counts[lane.index()] > 0, "{lane:?} has no playable objects");
    }
    assert_eq!(counts[Lane::Scratch.index()], 0);
    assert_eq!(counts[Lane::Key5.index()], 0);
}

#[test]
fn bms_random_zero_is_clamped_to_one_for_beatoraja_compatibility() {
    let (chart, warnings) = import_bms_text_with_warnings(
        "\
#TITLE Random Zero
#BPM 120
#WAV01 key.wav
#RANDOM 0
#IF 1
#00111:01
#ENDIF
#ENDRANDOM
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key1]);
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. } if code == "RandomZeroClamped"
    )));
}

#[test]
fn bms_random_control_is_flattened_like_beatoraja() {
    let (chart, _warnings) = import_bms_text_with_warnings(
        "\
#TITLE Random Flatten
#BPM 120
#WAV01 key.wav
#RANDOM 1
#IF 2
#00111:01
#ENDIF
#IF 1
#00212:01
#ENDIF
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key2]);
}

#[test]
fn bms_random_else_after_matched_if_is_included_like_beatoraja() {
    // beatoraja (jbms-parser BMSDecoder) は #ELSE を予約語として扱わない。
    // #IF が一致した場合、#ELSE 以降のブロックもそのまま取り込まれる。
    let (chart, warnings) = import_bms_text_with_warnings(
        "\
#TITLE Else Matched
#BPM 120
#WAV01 key.wav
#RANDOM 1
#IF 1
#00111:01
#ELSE
#00212:01
#ENDIF
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key1, Lane::Key2]);
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. }
            if code == "BeatorajaRandomUnsupportedElse"
    )));
}

#[test]
fn bms_random_else_after_unmatched_if_stays_skipped_like_beatoraja() {
    // #IF が不一致の場合、#ELSE は skip 状態を反転させないため
    // #ELSE 以降のブロックも beatoraja と同じく skip される。
    let (chart, _warnings) = import_bms_text_with_warnings(
        "\
#TITLE Else Unmatched
#BPM 120
#WAV01 key.wav
#RANDOM 1
#IF 2
#00111:01
#ELSE
#00212:01
#ENDIF
#00313:01
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key3]);
}

#[test]
fn bms_random_elseif_is_ignored_like_beatoraja() {
    // #ELSEIF も同様に無視され、直前の #IF の skip 状態が継続する。
    let (chart, warnings) = import_bms_text_with_warnings(
        "\
#TITLE ElseIf Ignored
#BPM 120
#WAV01 key.wav
#RANDOM 1
#IF 1
#00111:01
#ELSEIF 2
#00212:01
#ENDIF
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key1, Lane::Key2]);
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. }
            if code == "BeatorajaRandomUnsupportedElse"
    )));
}

#[test]
fn bms_random_sections_set_has_bms_random_metadata() {
    let (with_random, _) = import_bms_text_with_warnings(
        "\
#TITLE Random Song
#BPM 120
#WAV01 key.wav
#RANDOM 1
#IF 1
#00111:01
#ENDIF
",
    );
    let (without_random, _) = import_bms_text_with_warnings(
        "\
#TITLE Plain Song
#BPM 120
#WAV01 key.wav
#00111:01
",
    );

    assert!(with_random.metadata.has_bms_random);
    assert!(!without_random.metadata.has_bms_random);
}

#[test]
fn bms_switch_large_range_is_flattened_before_bms_rs() {
    let text = "\
#TITLE Large Switch
#BPM 120
#TOTAL 200
#WAV01 key.wav
#SWITCH 2000000000000
#CASE 1
#00111:01
#SKIP
#CASE21
#00212:01
#SKIP
#DEF
#00313:01
#ENDSW
";
    let (chart, warnings, random, switches) =
        import_bms_text_with_control_choices(text, Vec::new(), vec![2_000_000_000_000]);

    assert_eq!(note_lanes(&chart), vec![Lane::Key3]);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert!(random.is_empty());
    assert_eq!(switches, vec![2_000_000_000_000]);
    assert!(chart.metadata.has_bms_random);
}

#[test]
fn bms_switch_parses_case_value_without_space() {
    let text = "\
#TITLE Direct Case
#BPM 120
#TOTAL 200
#WAV01 key.wav
#SWITCH 2000000000000
#CASE 1
#00111:01
#SKIP
#CASE21
#00212:01
#SKIP
#DEF
#00313:01
#ENDSW
";
    let (chart, warnings, _, switches) =
        import_bms_text_with_control_choices(text, Vec::new(), vec![21]);

    assert_eq!(note_lanes(&chart), vec![Lane::Key2]);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(switches, vec![21]);
}

#[test]
fn bms_setswitch_falls_through_until_skip_without_recording_choice() {
    let text = "\
#TITLE Set Switch
#BPM 120
#TOTAL 200
#WAV01 key.wav
#SETSWITCH 1
#CASE 1
#00111:01
#CASE 2
#00212:01
#SKIP
#DEF
#00313:01
#ENDSW
";
    let (chart, warnings, random, switches) =
        import_bms_text_with_control_choices(text, Vec::new(), Vec::new());

    assert_eq!(note_lanes(&chart), vec![Lane::Key1, Lane::Key2]);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert!(random.is_empty());
    assert!(switches.is_empty());
}

#[test]
fn bms_random_parses_if_value_without_space() {
    let (chart, warnings) = import_bms_text_with_warnings(
        "\
#TITLE Direct If
#BPM 120
#TOTAL 200
#WAV01 key.wav
#SETRANDOM 21
#IF21
#00111:01
#ENDIF
#ENDRANDOM
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key1]);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
}

#[test]
fn bms_headers_capture_url_and_metadata_commands() {
    let (chart, _) = import_bms_text_with_warnings(
        "\
#TITLE Example Song
#ARTIST Alice
#URL http://example.com/bms
#URL-WAV http://example.com/append
#BPM 120
#WAV01 key.wav
#00111:01
",
    );

    assert_eq!(chart.metadata.source_url, "http://example.com/bms");
    assert_eq!(chart.metadata.append_url, "http://example.com/append");
    assert_eq!(chart.metadata.bms_headers.get("TITLE"), Some(&"Example Song".to_string()));
    assert_eq!(chart.metadata.bms_headers.get("URL"), Some(&"http://example.com/bms".to_string()));
    assert_eq!(
        chart.metadata.bms_headers.get("URL-WAV"),
        Some(&"http://example.com/append".to_string())
    );
    assert!(!chart.metadata.bms_headers.contains_key("00111"));
}

#[test]
fn bms_headers_exclude_base62_channel_commands() {
    let headers = extract_bms_headers_from_text("#002D9:000102\n#TITLE Example");

    assert!(!headers.contains_key("002D9"));
    assert_eq!(headers.get("TITLE"), Some(&"Example".to_string()));
}

#[test]
fn beatoraja_colon_separated_bpm_and_stop_definitions_are_imported() {
    let (chart, warnings) = import_bms_text_with_warnings(
        "\
#TITLE Colon Definitions
#BPM 120
#BPM01:240
#STOP01:192
#WAV01 key.wav
#00103:01
#00109:01
#00111:01
",
    );

    assert_eq!(
        chart
            .resources
            .bpm_table
            .iter()
            .find(|definition| definition.key == 1)
            .map(|definition| definition.bpm),
        Some(240.0)
    );
    assert_eq!(
        chart
            .resources
            .stop_table
            .iter()
            .find(|definition| definition.key == 1)
            .map(|definition| definition.value),
        Some(192)
    );
    assert!(!warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. }
            if code == "ParseSyntaxError" || code == "ParseUndefinedObject"
    )));
}

#[test]
fn empty_trailing_metadata_does_not_clear_previous_values() {
    let (chart, _) = import_bms_text_with_warnings(
        "\
#TITLE Sakura Fubuki
#ARTIST Street
#GENRE Drumstep
#BPM 175
#PLAYLEVEL 12
#TOTAL 440
#STAGEFILE
#WAV01 key.wav
#00111:01
#GENRE
#TITLE
#ARTIST
#TOTAL
",
    );

    assert_eq!(chart.metadata.title, "Sakura Fubuki");
    assert_eq!(chart.metadata.artist, "Street");
    assert_eq!(chart.metadata.genre, "Drumstep");
    assert_eq!(chart.metadata.play_level, "12");
    assert_eq!(chart.metadata.initial_bpm, 175.0);
    assert_eq!(chart.metadata.total, Some(440.0));
    assert_eq!(chart.metadata.stage_file, "");
    assert_eq!(chart.metadata.bms_headers.get("TITLE"), Some(&"Sakura Fubuki".to_string()));
    assert_eq!(chart.metadata.bms_headers.get("TOTAL"), Some(&"440".to_string()));
}

#[test]
fn bms_random_orphan_if_warns_and_continues_like_beatoraja() {
    let (chart, warnings) = import_bms_text_with_warnings(
        "\
#TITLE Orphan If
#BPM 120
#WAV01 key.wav
#IF 1
#00111:01
#ENDIF
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key1]);
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. }
            if code == "BeatorajaRandomIfWithoutRandom"
    )));
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. }
            if code == "BeatorajaRandomEndifWithoutIf"
    )));
}

#[test]
fn bms_end_if_typo_is_ignored_like_beatoraja() {
    let (chart, warnings) = import_bms_text_with_warnings(
        "\
#TITLE End If Typo
#BPM 120
#WAV01 key.wav
#SETRANDOM 2
#IF 1
#00111:01
#end if
#IF 2
#00212:01
#end if
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key2]);
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::ParserDiagnostic { code, .. }
            if code == "BeatorajaRandomIgnoredTypoControl"
    )));
}

#[test]
fn bms_setrandom_is_flattened_with_fixed_condition() {
    let (chart, _warnings) = import_bms_text_with_warnings(
        "\
#TITLE SetRandom
#BPM 120
#WAV01 key.wav
#SETRANDOM 2
#IF 1
#00111:01
#ENDIF
#IF 2
#00212:01
#ENDIF
#ENDRANDOM
",
    );

    assert_eq!(note_lanes(&chart), vec![Lane::Key2]);
}

#[test]
fn bms_8k_ue_sample_reports_k8_when_present() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/songs/8K U_E FULL PACK 1.1/[r] Baby/_baby_8K_Hard.bms"
    ));
    if !path.exists() {
        return;
    }
    let mut warnings = Vec::new();
    let chart = import_bms_to_intermediate(path, None, &mut warnings).unwrap();
    let counts = playable_lane_counts(&chart);
    assert_eq!(chart.metadata.key_mode, KeyMode::K8);
    for lane in [
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
        Lane::Key8,
    ] {
        assert!(counts[lane.index()] > 0, "{lane:?} has no playable objects");
    }
    assert_eq!(counts[Lane::Scratch.index()], 0);
}

#[test]
fn pms_18k_player2_notes_are_dropped_with_warning() {
    let mut text = String::from(PMS_HEADER);
    text.push_str("#00121:01\n");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.pms");
    std::fs::write(&path, &text).unwrap();
    std::fs::write(dir.path().join("key.wav"), b"wav").unwrap();
    let mut warnings = Vec::new();
    let chart = import_pms_to_intermediate(&path, None, &mut warnings).unwrap();
    assert!(note_lanes(&chart).is_empty());
    assert!(
        warnings
            .iter()
            .any(|warning| matches!(warning, ImportWarning::UnsupportedPmsPlayerSide { side: 2 }))
    );
}
