use super::*;

fn import_bmp00_case(commands: &str, extension: &str, random: Vec<i32>) -> ImportResult {
    let text = format!(
        "#TITLE BMP00 regression\n#BPM 120\n#TOTAL 200\n#WAV01 key.wav\n#00111:01\n{commands}"
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("chart.{extension}"));
    std::fs::write(&path, &text).unwrap();
    let result = import_chart_with_random_source(
        &path,
        BmsRandomSource::Choices { random, switches: Vec::new() },
        false,
    )
    .unwrap();
    assert_eq!(result.chart.identity, compute_chart_identity(text.as_bytes()));
    result
}

fn poor_events(chart: &PlayableChart) -> Vec<(i64, Option<BgaAssetId>)> {
    chart
        .bga_events
        .iter()
        .filter(|event| event.kind == BgaEventKind::Poor)
        .map(|event| (event.time.0, event.asset))
        .collect()
}

#[test]
fn bmp00_defines_initial_poor_resource_in_bms_and_both_pms_layouts() {
    for (extension, lane_command) in [("bms", ""), ("pms", "#00125:01"), ("pms", "#00119:01")] {
        let result =
            import_bmp00_case(&format!("#BMP00 default.png\n{lane_command}\n"), extension, vec![]);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let chart = &result.chart;
        assert!(chart.metadata.has_bga);
        assert_eq!(chart.bga_assets.len(), 1);
        assert_eq!(chart.bga_asset_by_bmp_key[&0], BgaAssetId(0));
        assert!(chart.bga_assets[0].path.ends_with("default.png"));
        assert_eq!(chart.bga_assets[0].kind, BgaAssetKind::Static);
        assert_eq!(poor_events(chart), [(0, Some(BgaAssetId(0)))]);
        assert_eq!(chart.bga_events[0].tick.0, 0);
        assert_eq!(chart.bga_events.len(), 1);
    }
}

#[test]
fn missing_bmp00_does_not_treat_first_asset_id_as_a_default_poor() {
    let result = import_bmp00_case("#BMP01 regular.png\n", "bms", vec![]);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let chart = &result.chart;
    assert!(!chart.metadata.has_bga);
    assert!(!chart.bga_asset_by_bmp_key.contains_key(&0));
    assert_eq!(chart.bga_asset_by_bmp_key[&1], BgaAssetId(0));
    assert!(chart.bga_events.is_empty());
}

#[test]
fn bmp00_yields_to_initial_poor_but_survives_empty_and_later_channels() {
    for (channel, expected_keys) in [
        ("#00006:01", vec![(0, 1)]),
        ("#00106:02", vec![(0, 0), (2_000_000, 2)]),
        ("#00006:00", vec![(0, 0)]),
        ("#00006:0001", vec![(0, 0), (1_000_000, 1)]),
        ("#00006 01", vec![(0, 1)]),
    ] {
        let result = import_bmp00_case(
            &format!("#BMP00 default.png\n#BMP01 custom.png\n#BMP02 later.png\n{channel}\n"),
            "bms",
            vec![],
        );
        assert!(result.warnings.is_empty(), "{channel}: {:?}", result.warnings);
        let chart = &result.chart;
        assert!(chart.metadata.has_bga);
        assert_eq!(chart.bga_assets.len(), 3);
        let expected = expected_keys
            .iter()
            .map(|&(time, key)| (time, Some(chart.bga_asset_by_bmp_key[&key])))
            .collect::<Vec<_>>();
        assert_eq!(poor_events(chart), expected, "{channel}");
        assert_eq!(chart.bga_events.len(), expected.len());
    }
}

#[test]
fn bmp00_does_not_replace_an_explicit_missing_poor_definition() {
    let result = import_bmp00_case("#BMP00 default.png\n#00006:01\n", "bms", vec![]);
    assert!(result.chart.metadata.has_bga);
    assert_eq!(poor_events(&result.chart), [(0, None)]);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| { matches!(warning, ImportWarning::MissingBmpDefinition { key: 1 }) })
    );
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| { matches!(warning, ImportWarning::MissingBmpDefinition { key: 0 }) })
    );
}

#[test]
fn bmp00_coexists_with_all_other_initial_bga_layers() {
    let result = import_bmp00_case(
        "#BMP00 default.png\n#BMP01 base.png\n#BMP02 layer.png\n#BMP03 layer2.png\n\
         #00004:01\n#00007:02\n#0000A:03\n",
        "bms",
        vec![],
    );
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let chart = &result.chart;
    assert!(chart.metadata.has_bga);
    assert_eq!(chart.bga_events.len(), 4);
    for (kind, key) in [
        (BgaEventKind::Base, 1),
        (BgaEventKind::Poor, 0),
        (BgaEventKind::Layer, 2),
        (BgaEventKind::Layer2, 3),
    ] {
        assert!(chart.bga_events.iter().any(|event| {
            event.kind == kind
                && event.time.0 == 0
                && event.asset == Some(chart.bga_asset_by_bmp_key[&key])
        }));
    }
}

#[test]
fn bmp00_and_initial_poor_use_only_the_selected_random_branch() {
    let commands = "#RANDOM 3\n\
        #IF 1\n#BMP00 first.png\n#ENDIF\n\
        #IF 2\n#BMP00 second.png\n#BMP01 override.png\n#00006:01\n#ENDIF\n\
        #IF 3\n#BMP01 unused.png\n#ENDIF\n#ENDRANDOM\n";
    for choice in 1..=3 {
        let result = import_bmp00_case(commands, "bms", vec![choice]);
        assert!(result.warnings.is_empty(), "{choice}: {:?}", result.warnings);
        assert_eq!(result.bms_random_choices, [choice]);
        let chart = &result.chart;
        if choice == 3 {
            assert!(!chart.metadata.has_bga);
            assert!(!chart.bga_asset_by_bmp_key.contains_key(&0));
            assert!(chart.bga_events.is_empty());
        } else {
            assert!(chart.metadata.has_bga);
            let default = &chart.bga_assets[chart.bga_asset_by_bmp_key[&0].0 as usize];
            assert!(default.path.ends_with(if choice == 1 { "first.png" } else { "second.png" }));
            let key = if choice == 1 { 0 } else { 1 };
            assert_eq!(poor_events(chart), [(0, Some(chart.bga_asset_by_bmp_key[&key]))]);
        }
    }
}

#[test]
fn bmson_zero_resource_is_not_an_implicit_poor_event() {
    for explicit_poor in [false, true] {
        let text = serde_json::json!({
            "version": "1.0.0",
            "info": { "title": "zero", "artist": "test", "genre": "test", "level": 1,
                "init_bpm": 120, "resolution": 240, "mode_hint": "beat-7k" },
            "sound_channels": [],
            "bga": {
                "bga_header": [{"id": 0, "name": "zero.png"}],
                "bga_events": [], "layer_events": [],
                "poor_events": if explicit_poor { vec![serde_json::json!({"y": 240, "id": 0})] }
                    else { vec![] },
            }
        });
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("zero.bmson");
        std::fs::write(&path, text.to_string()).unwrap();
        let result = import_chart(&path, None, false).unwrap();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.chart.metadata.has_bga, explicit_poor);
        assert_eq!(result.chart.bga_assets.len(), 1);
        assert_eq!(result.chart.bga_assets[0].id, BgaAssetId(0));
        assert_eq!(
            poor_events(&result.chart),
            if explicit_poor { vec![(500_000, Some(BgaAssetId(0)))] } else { vec![] }
        );
    }
}
