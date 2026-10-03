use super::*;

#[test]
fn system_font_slots_are_independent_of_bitmap_font_slots() {
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(Path::new("skin.lr2skin"), Header::default(), &files);
    for fields in [["#FONT", "20", "4", "2"], ["#FONT", "60", "5", "3"]] {
        builder.add_system_font(&CsvLine {
            command: "FONT".into(),
            fields: fields.into_iter().map(String::from).collect(),
        });
    }
    builder.add_lr2_font("bitmap.lr2font");
    for index in [0, 1] {
        builder.add_text(&CsvLine {
            command: "SRC_TEXT".into(),
            fields: vec![
                "#SRC_TEXT".into(),
                "0".into(),
                index.to_string(),
                "10".into(),
                "1".into(),
            ],
        });
    }
    assert_eq!(builder.texts[0]["font"], "lr2font-0");
    assert!(builder.texts[0]["lr2SystemFont"].is_null());
    assert_eq!(builder.texts[1]["font"], "");
    assert_eq!(builder.texts[1]["size"], 0);
    assert_eq!(builder.texts[1]["lr2SystemFont"], json!({"size":60, "thickness":5, "type":3}));
}

#[test]
fn image_transparency_is_captured_at_each_load() {
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(Path::new("skin.lr2skin"), Header::default(), &files);
    builder.add_source("image.bmp");
    assert_eq!(builder.sources[0]["lr2ColorKey"], json!([0, 255, 0]));
    for (index, command) in ["TRANSCOLOR", "TRANSCLOLR", "TRANSCLOLOR"].iter().enumerate() {
        builder
            .execute(&CsvLine {
                command: (*command).into(),
                fields: vec![format!("#{command}"), "255".into(), "0".into(), index.to_string()],
            })
            .unwrap();
        builder.add_source("image.bmp");
        assert_eq!(builder.sources[index + 1]["lr2ColorKey"], json!([255, 0, index]));
    }
    assert_eq!(builder.sources[0]["lr2ColorKey"], json!([0, 255, 0]));
}

#[test]
fn processor_selects_default_custom_option_branch() {
    let mut ops = HashMap::new();
    ops.insert(900, true);
    ops.insert(901, false);
    let mut processor = Processor::new(ops);
    assert!(!processor.should_execute(&CsvLine {
        command: "IF".into(),
        fields: vec!["#IF".into(), "900".into()],
    }));
    assert!(processor.active());
    assert!(
        !processor
            .should_execute(&CsvLine { command: "ENDIF".into(), fields: vec!["#ENDIF".into()] })
    );
    assert!(processor.active());
}

#[test]
fn processor_keeps_outer_false_branch_inactive_inside_true_nested_if() {
    let mut ops = HashMap::new();
    ops.insert(900, false);
    ops.insert(901, true);
    let mut processor = Processor::new(ops);
    assert!(!processor.should_execute(&CsvLine {
        command: "IF".into(),
        fields: vec!["#IF".into(), "900".into()],
    }));
    assert!(!processor.active());
    assert!(!processor.should_execute(&CsvLine {
        command: "IF".into(),
        fields: vec!["#IF".into(), "901".into()],
    }));
    assert!(!processor.active());
    assert!(
        !processor
            .should_execute(&CsvLine { command: "ENDIF".into(), fields: vec!["#ENDIF".into()] })
    );
    assert!(!processor.active());
    assert!(
        !processor
            .should_execute(&CsvLine { command: "ENDIF".into(), fields: vec!["#ENDIF".into()] })
    );
    assert!(processor.active());
}

#[test]
fn processor_keeps_autoplay_conditions_as_runtime_ops() {
    let ops = HashMap::from([(32, true), (33, false)]);
    let mut processor = Processor::new(ops);
    assert!(!processor.should_execute(&CsvLine {
        command: "IF".into(),
        fields: vec!["#IF".into(), "33".into()],
    }));

    assert!(processor.active());
    assert_eq!(processor.active_runtime_ops(), vec![33]);
}

#[test]
fn processor_converts_runtime_else_to_negated_op() {
    let mut processor = Processor::new(HashMap::new());
    assert!(!processor.should_execute(&parse_csv_line("#IF,41").unwrap()));
    assert_eq!(processor.active_runtime_ops(), vec![41]);
    assert!(!processor.should_execute(&parse_csv_line("#ELSE").unwrap()));
    assert_eq!(processor.active_runtime_ops(), vec![-41]);
    assert!(!processor.should_execute(&parse_csv_line("#ENDIF").unwrap()));
    assert!(processor.active_runtime_ops().is_empty());
}

#[test]
fn processor_expands_conditional_setoption_to_its_runtime_source() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(path, Header::default(), &files);
    let lines = [
        parse_csv_line("#IF,41").unwrap(),
        parse_csv_line("#SETOPTION,982,1").unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
        parse_csv_line("#SRC_BGA").unwrap(),
        parse_csv_line("#IF,982").unwrap(),
        parse_csv_line("#DST_BGA,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::new());

    processor.process_lines(&lines, path, &mut builder).unwrap();

    assert_eq!(builder.destinations[0]["op"].as_array().unwrap(), &[json!(41)]);
}

#[test]
fn processor_does_not_leak_setoption_inside_runtime_if() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(path, Header::default(), &files);
    let lines = [
        parse_csv_line("#IF,33").unwrap(),
        parse_csv_line("#SETOPTION,985,1").unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::new());

    processor.process_lines(&lines, path, &mut builder).unwrap();

    assert!(!processor.ops.contains_key(&985));
    assert!(!builder.header.selected_ops.contains_key(&985));
}

#[test]
fn processor_attaches_autoplay_runtime_op_to_destination() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(path, Header::default(), &files);
    let lines = [
        parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#IF,33").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::new());

    processor.process_lines(&lines, path, &mut builder).unwrap();

    let op = builder.destinations[0]["op"].as_array().unwrap();
    assert_eq!(op, &[json!(33)]);
}

#[test]
fn processor_preserves_autoplay_off_on_score_graph_destinations() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut header = Header::default();
    header.selected_ops.extend([(39, true), (900, true)]);
    let mut builder = CsvBuilder::new(path, header, &files);
    let lines = [
        parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#IF,32,900").unwrap(),
        parse_csv_line("#DST_IMAGE,0,1000,546,110,277,798,0,255,255,255,255,1,1,0,0,0,0,32,39,0")
            .unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::from([(39, true), (900, true)]));

    processor.process_lines(&lines, path, &mut builder).unwrap();

    let op = builder.destinations[0]["op"].as_array().unwrap();
    assert_eq!(op, &[json!(32), json!(39)]);
}

#[test]
fn processor_keeps_non_graph_layout_destinations_conditional_on_autoplay_off() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut header = Header::default();
    header.selected_ops.extend([(39, true), (900, true)]);
    let mut builder = CsvBuilder::new(path, header, &files);
    let lines = [
        parse_csv_line("#IF,32,900").unwrap(),
        parse_csv_line("#SRC_BGA").unwrap(),
        parse_csv_line("#DST_BGA,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#SRC_TEXT,0,0,10,1,0").unwrap(),
        parse_csv_line("#DST_TEXT,0,0,100,10,200,30,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::from([(39, true), (900, true)]));

    processor.process_lines(&lines, path, &mut builder).unwrap();

    assert_eq!(builder.destinations[0]["op"].as_array().unwrap(), &[json!(32)]);
    assert_eq!(builder.destinations[1]["op"].as_array().unwrap(), &[json!(32)]);
}

#[test]
fn processor_preserves_autoplay_and_score_graph_layout_conditions() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut header = Header::default();
    header.selected_ops.extend([(39, true), (900, true)]);
    let mut builder = CsvBuilder::new(path, header, &files);
    let lines = [
        parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
        parse_csv_line("#IF,33").unwrap(),
        parse_csv_line("#SRC_BGA").unwrap(),
        parse_csv_line("#DST_BGA,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
        parse_csv_line("#IF,32,900").unwrap(),
        parse_csv_line("#SRC_BGA").unwrap(),
        parse_csv_line("#DST_BGA,0,0,100,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,200,10,20,30,40,0,255,255,255,255,0,0,0,0,0,32,39,0")
            .unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::from([(39, true), (900, true)]));

    processor.process_lines(&lines, path, &mut builder).unwrap();

    assert_eq!(builder.destinations.len(), 3);
    assert_eq!(builder.destinations[0]["id"], json!("bga"));
    assert_eq!(builder.destinations[0]["dst"][0]["x"], json!(0));
    assert_eq!(builder.destinations[0]["op"].as_array().unwrap(), &[json!(33)]);
    assert_eq!(builder.destinations[1]["id"], json!("bga"));
    assert_eq!(builder.destinations[1]["dst"][0]["x"], json!(100));
    assert_eq!(builder.destinations[1]["op"].as_array().unwrap(), &[json!(32)]);
    assert_eq!(builder.destinations[2]["op"].as_array().unwrap(), &[json!(32), json!(39)]);
}

#[test]
fn processor_guards_matching_load_time_else_if_with_negated_runtime_alias() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut header = Header::default();
    header.selected_ops.insert(911, true);
    let mut builder = CsvBuilder::new(path, header, &files);
    let lines = [
        parse_csv_line("#IF,33").unwrap(),
        parse_csv_line("#SETOPTION,910,1").unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
        parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
        parse_csv_line("#IF,910").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#ELSEIF,911").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,100,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0")
            .unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::from([(911, true)]));

    processor.process_lines(&lines, path, &mut builder).unwrap();

    assert_eq!(builder.destinations.len(), 2);
    assert_eq!(builder.destinations[0]["dst"][0]["x"], json!(0));
    assert_eq!(builder.destinations[0]["op"].as_array().unwrap(), &[json!(33)]);
    assert_eq!(builder.destinations[1]["dst"][0]["x"], json!(100));
    assert_eq!(builder.destinations[1]["op"].as_array().unwrap(), &[json!(-33)]);
}

#[test]
fn processor_preserves_runtime_else_if_chain_before_load_time_branch() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut header = Header::default();
    header.selected_ops.insert(900, true);
    let mut builder = CsvBuilder::new(path, header, &files);
    let lines = [
        parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
        parse_csv_line("#IF,33").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
        parse_csv_line("#ELSEIF,41").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,50,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0")
            .unwrap(),
        parse_csv_line("#ELSEIF,900").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,100,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0")
            .unwrap(),
        parse_csv_line("#ENDIF").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::from([(900, true)]));

    processor.process_lines(&lines, path, &mut builder).unwrap();

    assert_eq!(builder.destinations.len(), 3);
    assert_eq!(builder.destinations[0]["dst"][0]["x"], json!(0));
    assert_eq!(builder.destinations[0]["op"].as_array().unwrap(), &[json!(33)]);
    assert_eq!(builder.destinations[1]["dst"][0]["x"], json!(50));
    assert_eq!(builder.destinations[1]["op"].as_array().unwrap(), &[json!(-33), json!(41)]);
    assert_eq!(builder.destinations[2]["dst"][0]["x"], json!(100));
    assert_eq!(builder.destinations[2]["op"].as_array().unwrap(), &[json!(-33), json!(-41)]);
}

#[test]
fn consecutive_lr2_destinations_merge_into_keyframes() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(path, Header::default(), &files);
    builder
        .execute(&CsvLine {
            command: "IMAGE".into(),
            fields: vec!["#IMAGE".into(), "parts/frame.png".into()],
        })
        .unwrap();
    builder
        .execute(&CsvLine {
            command: "SRC_IMAGE".into(),
            fields: vec![
                "#SRC_IMAGE".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "10".into(),
                "20".into(),
                "1".into(),
                "1".into(),
                "0".into(),
                "0".into(),
            ],
        })
        .unwrap();
    builder
        .execute(&CsvLine {
            command: "DST_IMAGE".into(),
            fields: vec![
                "#DST_IMAGE".into(),
                "0".into(),
                "0".into(),
                "10".into(),
                "20".into(),
                "30".into(),
                "40".into(),
                "0".into(),
                "0".into(),
                "255".into(),
                "255".into(),
                "255".into(),
                "1".into(),
                "1".into(),
                "0".into(),
                "0".into(),
                "500".into(),
                "0".into(),
                "41".into(),
                "30".into(),
                "0".into(),
            ],
        })
        .unwrap();
    builder
        .execute(&CsvLine {
            command: "DST_IMAGE".into(),
            fields: vec![
                "#DST_IMAGE".into(),
                "0".into(),
                "500".into(),
                "10".into(),
                "20".into(),
                "30".into(),
                "40".into(),
                "0".into(),
                "255".into(),
                "255".into(),
                "255".into(),
                "255".into(),
                "1".into(),
                "1".into(),
            ],
        })
        .unwrap();

    assert_eq!(builder.destinations.len(), 1);
    let frames = builder.destinations[0].get("dst").and_then(JsonValue::as_array).unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["a"], 0);
    assert_eq!(frames[1]["a"], 255);
    assert_eq!(builder.destinations[0]["loop"], 500);
}

#[test]
fn lr2_note_destination_uses_lane_region_height() {
    let mut values = [0; 22];
    values[2] = 0;
    values[3] = 75;
    values[4] = 704;
    values[5] = 90;
    values[6] = 27;

    let frame = note_destination_frame(&values, 1080);

    assert_eq!(frame["x"], 75);
    assert_eq!(frame["y"], 349);
    assert_eq!(frame["w"], 90);
    assert_eq!(frame["h"], 731);
}

#[test]
fn lr2_gauge_destination_uses_additive_part_span() {
    let mut values = [0; 22];
    values[2] = 1400;
    values[3] = 54;
    values[4] = 897;
    values[5] = 8;
    values[6] = 28;
    values[8] = 255;

    let destination = gauge_destination_def("gauge", &values, 1080, 9, 0, &[]);
    let frame = destination["dst"].as_array().unwrap().first().unwrap();

    assert_eq!(frame["x"], 54);
    assert_eq!(frame["y"], 155);
    assert_eq!(frame["w"], 450);
    assert_eq!(frame["h"], 28);
}

#[test]
fn lr2_gauge_destination_preserves_negative_additive_direction() {
    let mut values = [0; 22];
    values[2] = 1400;
    values[3] = 54;
    values[4] = 897;
    values[5] = 8;
    values[6] = 28;
    values[8] = 255;

    let destination = gauge_destination_def("gauge", &values, 1080, -9, 0, &[]);
    let frame = destination["dst"].as_array().unwrap().first().unwrap();

    assert_eq!(frame["x"], 63);
    assert_eq!(frame["y"], 155);
    assert_eq!(frame["w"], -450);
    assert_eq!(frame["h"], 28);
}

#[test]
fn lr2_gauge_omitted_parts_uses_beatoraja_animation_defaults() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-gauge-defaults").join("play.lr2skin");
    let header = Header { skin_type: 2, ..Header::default() };
    let mut builder = CsvBuilder::new(&skin_path, header, &files);
    builder
        .execute(&parse_csv_line("#IMAGE,gauge.png").expect("valid IMAGE"))
        .expect("IMAGE should load");
    builder
        .execute(
            &parse_csv_line("#SRC_GROOVEGAUGE,0,0,0,0,32,28,4,1,0,0,9,0,,,,,,,,")
                .expect("valid SRC_GROOVEGAUGE"),
        )
        .expect("SRC_GROOVEGAUGE should load");

    let gauge = builder.gauges.first().expect("gauge should be created");
    assert_eq!(gauge["parts"], json!(50));
    assert_eq!(gauge["type"], json!(0));
    assert_eq!(gauge["range"], json!(3));
    assert_eq!(gauge["cycle"], json!(33));
}

#[test]
fn lr2_gauge_nodes_expand_standard_cells_to_beatoraja_slots() {
    let cells = ["red", "green", "back-red", "back-green"].map(|cell| cell.to_string()).to_vec();

    let nodes = lr2_gauge_nodes(&cells, 0, false);

    assert_eq!(nodes.len(), 36);
    assert_eq!(nodes[0], "red");
    assert_eq!(nodes[1], "green");
    assert_eq!(nodes[2], "back-red");
    assert_eq!(nodes[3], "back-green");
    assert_eq!(nodes[4], "red");
    assert_eq!(nodes[5], "green");
    assert_eq!(nodes[18], "red");
    assert_eq!(nodes[24], "red");
    assert_eq!(nodes[34], "red");
    assert_eq!(nodes[35], "green");
}

#[test]
fn wmii_fhd_lr2skin_parse_has_no_unsupported_command_warnings_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !path.is_file() {
        return;
    }

    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert!(
        loaded
            .warnings
            .iter()
            .all(|warning| !warning.message.contains("unsupported lr2 csv command")),
        "unexpected warnings: {:?}",
        loaded.warnings
    );
    assert!(
        loaded.warnings.iter().all(|warning| !warning.message.contains("source index 101")
            && !warning.message.contains("source index 110")
            && !warning.message.contains("source index 111")),
        "unexpected reference source warnings: {:?}",
        loaded.warnings
    );
    assert_eq!(loaded.value["name"], "WMII FHD play AC");
    assert!(loaded.value["destination"].as_array().unwrap().len() > 100);
    assert!(!loaded.value["note"]["group"].as_array().unwrap().is_empty());
}

#[test]
fn openlr2_wmii_bombs_and_key_beams_follow_lift_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD_LR2/play/FHDPLAY_AC_Battle.lr2skin");
    if !path.is_file() {
        return;
    }

    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    let destinations = loaded.value["destination"].as_array().expect("destination array");
    let mut bomb_count = 0;
    let mut key_beam_count = 0;
    for destination in destinations {
        let timer = destination["timer"].as_i64().unwrap_or(0);
        let follows_lift = destination["offset"] == LR2_OFFSET_LIFT
            || destination["offsets"]
                .as_array()
                .is_some_and(|offsets| offsets.contains(&json!(LR2_OFFSET_LIFT)));
        if (50..90).contains(&timer) {
            bomb_count += 1;
            assert!(follows_lift, "bomb destination should follow lift: {destination}");
        } else if (100..140).contains(&timer) {
            let frames = destination["dst"].as_array().expect("destination frames");
            let first_height = frames.first().and_then(|frame| frame["h"].as_i64()).unwrap_or(0);
            let last_height = frames.last().and_then(|frame| frame["h"].as_i64()).unwrap_or(0);
            if first_height.saturating_abs() >= 100 || last_height.saturating_abs() >= 100 {
                key_beam_count += 1;
                assert!(follows_lift, "key beam destination should follow lift: {destination}");
            } else {
                assert!(
                    !follows_lift,
                    "short key light should keep its LR2 position: {destination}"
                );
            }
        }
    }

    assert!(bomb_count > 0, "expected WMII bomb destinations");
    assert!(key_beam_count > 0, "expected WMII key beam destinations");
}

#[test]
fn wmii_fhd_lr2skin_dp_keeps_internal_setoption_ops_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC_DP.lr2skin");
    if !path.is_file() {
        return;
    }

    let options = BTreeMap::from([
        ("Displayjudge".to_string(), "ON".to_string()),
        ("GRAPH SIDE".to_string(), "RIGHT".to_string()),
        ("Score Graph".to_string(), "On".to_string()),
    ]);
    let loaded = load_lr2_csv_skin_value(&path, &options, &BTreeMap::new()).unwrap();

    assert!(
        loaded.internal_enabled_options.contains(&983),
        "expected WMII DP judge detail right-side op983 to be kept internally"
    );
    assert!(
        !loaded.internal_enabled_options.contains(&980),
        "custom property option 980 should remain user-selectable instead of internal"
    );
}

#[test]
fn wmii_fhd_lr2skin_dp_uses_default_gauge_animation_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC_DP.lr2skin");
    if !path.is_file() {
        return;
    }

    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    let gauges = loaded.value["gauges"].as_array().expect("gauges array");

    assert!(!gauges.is_empty(), "expected WMII DP gauge objects");
    for gauge in gauges {
        assert_eq!(gauge["type"], json!(0));
        assert_eq!(gauge["range"], json!(3));
        assert_eq!(gauge["cycle"], json!(33));
    }
}

#[test]
fn wmii_fhd_lr2skin_keeps_gauge_sources_separate_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !path.is_file() {
        return;
    }

    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    let gauges = loaded.value["gauges"].as_array().expect("gauges array");

    assert!(gauges.len() >= 4, "expected WMII gauge objects, got {gauges:?}");
    for gauge in gauges.iter().take(4) {
        let nodes = gauge["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 36);
    }
    assert_ne!(gauges[0]["id"], gauges[1]["id"]);
}
