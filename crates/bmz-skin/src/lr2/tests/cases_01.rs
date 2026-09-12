use super::*;

#[test]
fn lr2_resolution_accepts_presets_and_explicit_dimensions() {
    for (source, expected, has_explicit_dimensions) in [
        ("#RESOLUTION,1", (1280, 720), false),
        ("#RESOLUTION,2,", (1920, 1080), false),
        ("#RESOLUTION,3", (3840, 2160), false),
        ("#RESOLUTION,1920,1080", (1920, 1080), true),
        ("#RESOLUTION,1280,720", (1280, 720), true),
    ] {
        let line = parse_csv_line(source).expect("valid RESOLUTION command");
        assert_eq!(lr2_resolution(&line), expected, "source: {source}");
        assert_eq!(
            lr2_resolution_has_explicit_dimensions(&line),
            has_explicit_dimensions,
            "source: {source}"
        );
    }
}

#[test]
fn explicit_resolution_lr2_effects_follow_open_lr2_note_adjustment() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-openlr2-effects").join("play.lr2skin");
    let mut builder = CsvBuilder::new(
        &skin_path,
        Header { w: 1920, h: 1080, explicit_resolution_dimensions: true, ..Header::default() },
        &files,
    );
    builder.destinations = vec![
        json!({ "id": "bomb", "timer": 50, "offset": 0, "dst": [{ "h": 300 }] }),
        json!({ "id": "ln-bomb", "timer": 89, "offset": 0, "dst": [{ "h": 300 }] }),
        json!({
            "id": "key-beam",
            "timer": 100,
            "offset": 0,
            "dst": [{ "h": 0 }, { "h": 723 }]
        }),
        json!({
            "id": "scratch-key-beam",
            "timer": 110,
            "offset": 1,
            "dst": [{ "h": 0 }, { "h": 723 }]
        }),
        json!({
            "id": "short-key-light",
            "timer": 139,
            "offset": 0,
            "dst": [{ "h": 0 }, { "h": 99 }]
        }),
        json!({ "id": "unrelated", "timer": 49, "offset": 0, "dst": [{ "h": 300 }] }),
        json!({ "id": "custom-offset", "timer": 51, "offset": 32, "dst": [{ "h": 300 }] }),
        json!({
            "id": "offset-list",
            "timer": 101,
            "offset": 0,
            "offsets": [32],
            "dst": [{ "h": 723 }]
        }),
    ];

    builder.complete_open_lr2_note_adjustment_effects();

    for id in ["bomb", "ln-bomb", "key-beam", "scratch-key-beam"] {
        let destination =
            builder.destinations.iter().find(|destination| destination["id"] == id).unwrap();
        assert_eq!(destination["offsets"], json!([LR2_OFFSET_LIFT]), "id: {id}");
    }
    for id in ["short-key-light", "unrelated", "custom-offset", "offset-list"] {
        let destination =
            builder.destinations.iter().find(|destination| destination["id"] == id).unwrap();
        if id == "offset-list" {
            assert_eq!(destination["offsets"], json!([32]));
        } else {
            assert!(destination.get("offsets").is_none(), "id: {id}");
        }
    }
}

#[test]
fn preset_resolution_lr2_effects_do_not_gain_lift_offsets() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-preset-effects").join("play.lr2skin");
    let mut builder =
        CsvBuilder::new(&skin_path, Header { w: 1920, h: 1080, ..Header::default() }, &files);
    builder.destinations = vec![
        json!({ "id": "bomb", "timer": 50, "offset": 0, "dst": [{ "h": 300 }] }),
        json!({
            "id": "key-beam",
            "timer": 100,
            "offset": 0,
            "dst": [{ "h": 0 }, { "h": 723 }]
        }),
    ];

    builder.complete_open_lr2_note_adjustment_effects();

    assert!(builder.destinations.iter().all(|destination| destination.get("offsets").is_none()));
}

#[test]
fn lr2_asset_path_strips_theme_prefix() {
    assert_eq!(
        normalize_lr2_asset_path(r".\LR2files\Theme\WMII_FHD\play\parts\note\*.png"),
        "play/parts/note/*.png"
    );
}

#[test]
fn lr2_destination_converts_top_origin_to_bottom_origin() {
    let mut values = [0; 22];
    values[2] = 100;
    values[3] = 10;
    values[4] = 20;
    values[5] = 30;
    values[6] = 40;
    let frame = destination_frame(&values, 1080);
    assert_eq!(frame["time"], 100);
    assert_eq!(frame["x"], 10);
    assert_eq!(frame["y"], 1020);
    assert_eq!(frame["w"], 30);
    assert_eq!(frame["h"], 40);
}

#[test]
fn lr2_destination_preserves_angle_and_custom_offset_id() {
    let mut values = [0; 22];
    values[14] = -90;
    values[21] = 32;

    let destination = destination_def_with_default_offsets("image", &values, 1080, &[], &[]);

    assert_eq!(destination["dst"][0]["angle"], -90);
    assert_eq!(destination["offset"], 32);
}

#[test]
fn lr2_dst_line_defaults_to_lift_offset() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-dst-line").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("line.png");
    builder
        .execute(&parse_csv_line("#SRC_LINE,0,0,0,0,10,1,1,1,0,0").expect("valid SRC_LINE"))
        .unwrap();
    builder
        .execute(
            &parse_csv_line("#DST_LINE,0,0,10,20,40,2,0,255,255,255,255,0,0,0,0,0,0,0,0,0,")
                .expect("valid DST_LINE"),
        )
        .unwrap();
    builder.complete_play_lines();

    let group = builder.note.group.first().expect("DST_LINE should produce note.group");

    assert_eq!(group["offset"], 0);
    assert_eq!(group["offsets"].as_array().unwrap(), &[json!(LR2_OFFSET_LIFT)]);
    for destinations in [&builder.note.bpm, &builder.note.stop, &builder.note.time] {
        assert_eq!(destinations.len(), 1);
        assert_eq!(destinations[0]["offsets"].as_array().unwrap(), &[json!(LR2_OFFSET_LIFT)]);
    }
    let bpm_frame = builder.note.bpm[0]["dst"].as_array().unwrap().first().unwrap();
    assert_eq!(bpm_frame["h"], json!(4));
    assert_eq!(
        (bpm_frame["r"].as_i64(), bpm_frame["g"].as_i64(), bpm_frame["b"].as_i64()),
        (Some(0), Some(192), Some(0))
    );
    let stop_frame = builder.note.stop[0]["dst"].as_array().unwrap().first().unwrap();
    assert_eq!(
        (stop_frame["r"].as_i64(), stop_frame["g"].as_i64(), stop_frame["b"].as_i64()),
        (Some(192), Some(192), Some(0))
    );
    let time_frame = builder.note.time[0]["dst"].as_array().unwrap().first().unwrap();
    assert_eq!(time_frame["h"], json!(2));
    assert_eq!(
        (time_frame["r"].as_i64(), time_frame["g"].as_i64(), time_frame["b"].as_i64()),
        (Some(64), Some(192), Some(192))
    );
}

#[test]
fn lr2_nowjudge_indices_match_beatoraja_slots() {
    assert_eq!(lr2_judge_slot(5), 0);
    assert_eq!(lr2_judge_slot(4), 1);
    assert_eq!(lr2_judge_slot(3), 2);
    assert_eq!(lr2_judge_slot(2), 3);
    assert_eq!(lr2_judge_slot(1), 4);
    assert_eq!(lr2_judge_slot(0), 5);
    assert_eq!(lr2_judge_slot(6), 6);
}

#[test]
fn lr2_number_ref_preserves_poor_plus_miss() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-number-ref").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("numbers.png");
    builder
        .execute(
            &parse_csv_line("#SRC_NUMBER,0,0,0,0,10,20,1,10,0,0,426,0,4,0,1")
                .expect("valid SRC_NUMBER"),
        )
        .unwrap();

    let value = builder.values.first().unwrap();
    assert_eq!(value["ref"], json!(426));
    assert_eq!(value["digit"], json!(4));
    assert_eq!(value["zeropadding"], json!(0));
    assert_eq!(value["space"], json!(1));
}

#[test]
fn lr2_play_number_refs_map_modified_lr2_fast_slow_extension() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-fast-slow-refs").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("numbers.png");

    for ref_id in [210, 212, 214] {
        builder
            .execute(
                &parse_csv_line(&format!("#SRC_NUMBER,0,0,0,0,110,20,11,1,0,0,{ref_id},0,4,0,1"))
                    .expect("valid SRC_NUMBER"),
            )
            .unwrap();
    }

    assert_eq!(builder.values[0]["ref"], json!(SKIN_REF_BMZ_LR2_FAST_SLOW_1P));
    assert_eq!(builder.values[1]["ref"], json!(423));
    assert_eq!(builder.values[2]["ref"], json!(424));
}

#[test]
fn lr2_non_play_number_refs_keep_beatoraja_meanings() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-non-play-fast-slow-refs").join("select.lr2skin");
    let header = Header { skin_type: 5, ..Header::default() };
    let mut builder = CsvBuilder::new(&skin_path, header, &files);
    builder.add_source("numbers.png");
    builder
        .execute(
            &parse_csv_line("#SRC_NUMBER,0,0,0,0,110,20,11,1,0,0,210,0,1,0,1")
                .expect("valid SRC_NUMBER"),
        )
        .unwrap();

    assert_eq!(builder.values[0]["ref"], json!(210));
}

#[test]
fn lr2_signed_number_reserves_sign_digit_and_defaults_to_blank_padding() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-signed-number").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("numbers.png");
    builder
        .execute(
            &parse_csv_line("#SRC_NUMBER,0,0,0,0,168,30,12,2,0,0,12,0,2,,,,,,,,")
                .expect("valid SRC_NUMBER"),
        )
        .unwrap();

    let value = builder.values.first().unwrap();
    assert_eq!(value["digit"], json!(3));
    assert_eq!(value["zeropadding"], json!(2));
    assert_eq!(value["space"], json!(0));
}

#[test]
fn lr2_unsigned_number_ignores_explicit_zero_padding_like_beatoraja() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-unsigned-number").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("numbers.png");
    builder
        .execute(
            &parse_csv_line("#SRC_NUMBER,0,0,0,0,100,20,10,1,0,0,100,0,4,2,0")
                .expect("valid SRC_NUMBER"),
        )
        .unwrap();

    let value = builder.values.first().unwrap();
    assert_eq!(value["digit"], json!(4));
    assert_eq!(value["zeropadding"], json!(0));
}

#[test]
fn lr2_button_keeps_state_reference_separate_from_clickability() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-button").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("button.png");
    builder
        .execute(
            &parse_csv_line("#SRC_BUTTON,0,0,0,0,20,10,2,1,0,0,77,0,0,-1,3")
                .expect("valid SRC_BUTTON"),
        )
        .unwrap();

    let image = builder.images.first().unwrap();
    assert_eq!(image["act"], json!(77));
    assert_eq!(image["clickable"], json!(false));
    assert_eq!(image["click"], json!(1));
    assert_eq!(image["len"], json!(3));
}

#[test]
fn lr2_play_bridges_keep_battle_values_and_gauge_display_separate() {
    let files = BTreeMap::new();
    let path = unique_test_dir("bmz-lr2-battle-values").join("play.lr2skin");
    for skin_type in [0, 5, 12, 13] {
        let mut builder = CsvBuilder::new(&path, Header { skin_type, ..Header::default() }, &files);
        builder.add_source("parts.png");
        for ref_id in [10, 11, 121, 127] {
            builder
                .execute(
                    &parse_csv_line(&format!("#SRC_NUMBER,0,0,0,0,100,20,10,1,0,0,{ref_id},0,3"))
                        .unwrap(),
                )
                .unwrap();
        }
        let expected = if skin_type == 5 {
            [10, 11, 121, 127]
        } else {
            [
                SKIN_REF_BMZ_LR2_HISPEED,
                SKIN_REF_BMZ_LR2_HISPEED,
                if skin_type >= 12 { 271 } else { 121 },
                SKIN_REF_BMZ_LR2_GAUGE_2P,
            ]
        };
        for (value, expected) in builder.values.iter().zip(expected) {
            assert_eq!(value["ref"], json!(expected));
        }
        for event_id in [40, 41, 55] {
            builder
                .execute(
                    &parse_csv_line(&format!(
                        "#SRC_BUTTON,0,0,0,0,128,84,1,6,0,0,{event_id},0,0,0"
                    ))
                    .unwrap(),
                )
                .unwrap();
        }
        for (image, event_id) in builder.images.iter().zip([40, 41, 55]) {
            assert_eq!(image["act"], json!(event_id));
            assert_eq!(image["clickable"], json!(false));
            if skin_type != 5 && event_id != 55 {
                assert_eq!(
                    image["ref"],
                    json!(if event_id == 40 {
                        SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P
                    } else {
                        SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P
                    })
                );
            } else {
                assert!(image.get("ref").is_none());
            }
        }
    }
}

#[test]
fn lr2_imageset_combines_registered_source_sets() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-imageset").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("set.png");
    builder
        .execute(&parse_csv_line("#IMAGESET,0,0,0,0,20,10,2,1,0,0").expect("valid IMAGESET"))
        .unwrap();
    builder
        .execute(&parse_csv_line("#SRC_IMAGESET,100,0,88,1,0").expect("valid SRC_IMAGESET"))
        .unwrap();

    let imageset = builder.imagesets.first().unwrap();
    assert_eq!(imageset["ref"], json!(88));
    assert_eq!(imageset["images"].as_array().unwrap().len(), 1);
    assert_eq!(builder.images.last().unwrap()["cycle"], json!(100));
}

#[test]
fn lr2_play_headers_and_stretch_are_preserved() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(path, Header::default(), &files);
    let lines = [
        parse_csv_line("#STARTINPUT,350").unwrap(),
        parse_csv_line("#SCENETIME,90000").unwrap(),
        parse_csv_line("#JUDGETIMER,3").unwrap(),
        parse_csv_line("#IMAGE,parts/frame.png").unwrap(),
        parse_csv_line("#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0").unwrap(),
        parse_csv_line("#STRETCH,2").unwrap(),
        parse_csv_line("#DST_IMAGE,0,0,0,10,20,30,40,0,255,255,255,255,0,0,0,0,0,0,0,0,0").unwrap(),
    ];
    let mut processor = Processor::new(HashMap::new());

    processor.process_lines(&lines, path, &mut builder).unwrap();

    assert_eq!(builder.header.input, 350);
    assert_eq!(builder.header.scene, 90_000);
    assert_eq!(builder.header.judgetimer, 3);
    assert_eq!(builder.destinations[0]["stretch"], json!(2));
}

#[test]
fn lr2_bga_stretch_distinguishes_unspecified_from_explicit_full() {
    let path = Path::new("skin/play/test.lr2skin");
    let files = BTreeMap::new();
    for (directive, expected) in [(None, -1), (Some(0), 0), (Some(1), 1), (Some(8), 8)] {
        let mut builder = CsvBuilder::new(path, Header::default(), &files);
        if let Some(stretch) = directive {
            builder.execute(&parse_csv_line(&format!("#STRETCH,{stretch}")).unwrap()).unwrap();
        }
        for line in [
            "#IMAGE,parts/frame.png",
            "#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0",
            "#DST_IMAGE,0,0,0,0,100,100,0,255,255,255,255,0,0,0,0,0,0,0,0,0",
            "#SRC_BGA,0",
            "#DST_BGA,0,0,0,0,100,100,0,255,255,255,255,0,0,0,0,0,0,0,0,0",
        ] {
            builder.execute(&parse_csv_line(line).unwrap()).unwrap();
        }
        assert_eq!(builder.destinations.len(), 2);
        for destination in &builder.destinations {
            assert_eq!(destination["stretch"], json!(expected));
        }
    }
}

#[test]
fn lr2_bargraph_preserves_negative_fill_direction() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-negative-graph").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("graph.png");
    builder
        .execute(
            &parse_csv_line("#SRC_BARGRAPH,0,0,0,0,100,10,1,1,0,0,0,0")
                .expect("valid SRC_BARGRAPH"),
        )
        .unwrap();
    builder
        .execute(
            &parse_csv_line("#DST_BARGRAPH,0,0,50,20,-30,8,0,255,255,255,255,0,0,0,0,0,0,0,0,0")
                .expect("valid DST_BARGRAPH"),
        )
        .unwrap();

    let frame = builder.destinations[0]["dst"].as_array().unwrap().first().unwrap();
    assert_eq!(frame["x"], json!(50));
    assert_eq!(frame["w"], json!(-30));
}

#[test]
fn lr2_play_chart_sources_keep_beatoraja_fields_and_destination_size() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-play-chart").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder
        .execute(
            &parse_csv_line("#SRC_NOTECHART_1P,2,0,0,0,0,0,0,0,0,0,300,120,0,0,15,1,1,1,1")
                .expect("valid SRC_NOTECHART_1P"),
        )
        .unwrap();
    builder
        .execute(
            &parse_csv_line("#DST_NOTECHART_1P,0,0,50,200,0,0,0,255,255,255,255,0,0,0,0,0,0,0,0,0")
                .expect("valid DST_NOTECHART_1P"),
        )
        .unwrap();

    let graph = builder.judge_graphs.first().unwrap();
    assert_eq!(graph["type"], json!(2));
    assert_eq!(graph["delay"], json!(15));
    assert_eq!(graph["backTexOff"], json!(1));
    assert_eq!(graph["orderReverse"], json!(1));
    assert_eq!(graph["noGap"], json!(1));
    assert_eq!(graph["noGapX"], json!(1));
    let frame = builder.destinations[0]["dst"].as_array().unwrap().first().unwrap();
    assert_eq!(frame["x"], json!(50));
    assert_eq!(frame["y"], json!(520));
    assert_eq!(frame["w"], json!(300));
    assert_eq!(frame["h"], json!(120));
}

#[test]
fn lr2_nowjudge_adds_beatoraja_judge_detail_objects() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-judge-detail").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.add_source("judge.png");
    builder
        .execute(
            &parse_csv_line("#SRC_NOWJUDGE_1P,5,0,0,0,100,20,1,1,0,0,0")
                .expect("valid SRC_NOWJUDGE_1P"),
        )
        .unwrap();
    builder
        .execute(
            &parse_csv_line(
                "#DST_NOWJUDGE_1P,0,0,100,200,120,24,0,255,255,255,255,0,0,0,0,0,0,0,0,0",
            )
            .expect("valid DST_NOWJUDGE_1P"),
        )
        .unwrap();

    assert!(builder.sources.iter().any(|source| source["path"] == "bmz://lr2/judgedetail"));
    let detail_destinations = builder
        .destinations
        .iter()
        .filter(|destination| {
            destination["op"].as_array().is_some_and(|ops| {
                ops.iter().any(|op| {
                    op.as_i64() == Some(i64::from(SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_EARLY_LATE))
                        || op.as_i64() == Some(i64::from(SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_MS))
                })
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(detail_destinations.len(), 4);
    assert!(detail_destinations.iter().all(|destination| {
        destination["offsets"]
            .as_array()
            .is_some_and(|offsets| offsets == &[json!(33), json!(LR2_OFFSET_LIFT)])
    }));
}

#[test]
fn lr2_text_defaults_to_shrink_overflow() {
    let files = BTreeMap::new();
    let skin_path = unique_test_dir("bmz-lr2-text-shrink").join("play.lr2skin");
    let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
    builder.execute(&parse_csv_line("#SRC_TEXT,0,0,10,1,0").expect("valid SRC_TEXT")).unwrap();
    builder
        .execute(
            &parse_csv_line("#DST_TEXT,0,0,10,20,120,30,0,255,255,255,255,0,0,0,0,0,0,0,0,0,0")
                .expect("valid DST_TEXT"),
        )
        .unwrap();

    let text = builder.texts.first().expect("SRC_TEXT should produce text");
    assert_eq!(text["overflow"], json!(1));

    let destination = builder.destinations.first().expect("DST_TEXT should produce destination");
    let frame = destination["dst"].as_array().unwrap().first().unwrap();
    assert_eq!(frame["w"], json!(120));
    assert_eq!(frame["h"], json!(30));
}

#[test]
fn lr2_ln_body_keeps_animation_only_while_held() {
    let files = BTreeMap::new();

    for command in ["SRC_LN_BODY", "SRC_AUTO_LN_BODY"] {
        let skin_path = unique_test_dir("bmz-lr2-ln-body").join("play.lr2skin");
        let mut builder = CsvBuilder::new(&skin_path, Header::default(), &files);
        builder.execute(&parse_csv_line("#IMAGE,notes.png").expect("valid IMAGE")).unwrap();
        builder
            .execute(
                &parse_csv_line(&format!("#{command},0,0,0,0,10,20,4,6,266,123"))
                    .expect("valid LN body source"),
            )
            .unwrap();

        let inactive = &builder.images[0];
        let active = &builder.images[1];
        assert_eq!(inactive["id"], json!(builder.note.lnbody[7]));
        assert_eq!(active["id"], json!(builder.note.lnbody_active[7]));
        assert_eq!(inactive["cycle"], json!(0), "{command} inactive body");
        assert!(inactive["timer"].is_null(), "{command} inactive body");
        assert_eq!(active["cycle"], json!(266), "{command} active body");
        assert_eq!(active["timer"], json!(123), "{command} active body");
    }
}

#[test]
fn lr2_customfile_default_replaces_wildcard_once() {
    assert_eq!(
        substitute_wildcard_default("parts/note/*.png", "parts/note/*.png", "photon"),
        "parts/note/photon.png"
    );
}

#[test]
fn lr2_customfile_selection_uses_existing_skin_file() {
    let root = unique_test_dir("bmz-lr2-customfile");
    let play_dir = root.join("play");
    std::fs::create_dir_all(play_dir.join("parts/gauge")).unwrap();
    std::fs::write(play_dir.join("parts/gauge/default.png"), []).unwrap();
    std::fs::write(play_dir.join("parts/gauge/blue.png"), []).unwrap();
    let skin_path = play_dir.join("FHDPLAY_AC.lr2skin");
    std::fs::write(&skin_path, []).unwrap();
    let mut header = Header::default();
    header.files.push(CustomFile {
        name: "GAUGE COLOR".to_string(),
        path: "parts/gauge/*.png".to_string(),
        default: "default".to_string(),
    });
    let files = BTreeMap::from([("GAUGE COLOR".to_string(), "parts/gauge/blue.png".to_string())]);
    let mut builder = CsvBuilder::new(&skin_path, header, &files);

    assert_eq!(
        builder.resolve_source_path(r".\LR2files\Theme\WMII_FHD\play\parts\gauge\*.png"),
        "parts/gauge/blue.png"
    );
}

#[test]
fn lr2_customfile_selection_accepts_legacy_basename_selection() {
    let root = unique_test_dir("bmz-lr2-customfile-basename");
    let play_dir = root.join("play");
    std::fs::create_dir_all(play_dir.join("parts/gauge")).unwrap();
    std::fs::write(play_dir.join("parts/gauge/default.png"), []).unwrap();
    std::fs::write(play_dir.join("parts/gauge/blue.png"), []).unwrap();
    let skin_path = play_dir.join("FHDPLAY_AC.lr2skin");
    std::fs::write(&skin_path, []).unwrap();
    let mut header = Header::default();
    header.files.push(CustomFile {
        name: "GAUGE COLOR".to_string(),
        path: "parts/gauge/*.png".to_string(),
        default: "default".to_string(),
    });
    let files = BTreeMap::from([("GAUGE COLOR".to_string(), "blue.png".to_string())]);
    let mut builder = CsvBuilder::new(&skin_path, header, &files);

    assert_eq!(
        builder.resolve_source_path(r".\LR2files\Theme\WMII_FHD\play\parts\gauge\*.png"),
        "parts/gauge/blue.png"
    );
}

#[test]
fn lr2_customfile_selection_falls_back_when_saved_file_is_missing() {
    let root = unique_test_dir("bmz-lr2-customfile-missing");
    let play_dir = root.join("play");
    std::fs::create_dir_all(play_dir.join("parts/gauge")).unwrap();
    std::fs::write(play_dir.join("parts/gauge/default.png"), []).unwrap();
    let skin_path = play_dir.join("FHDPLAY_AC.lr2skin");
    std::fs::write(&skin_path, []).unwrap();
    let mut header = Header::default();
    header.files.push(CustomFile {
        name: "GAUGE COLOR".to_string(),
        path: "parts/gauge/*.png".to_string(),
        default: "default".to_string(),
    });
    let files =
        BTreeMap::from([("GAUGE COLOR".to_string(), "parts/gauge/missing.png".to_string())]);
    let mut builder = CsvBuilder::new(&skin_path, header, &files);

    assert_eq!(
        builder.resolve_source_path(r".\LR2files\Theme\WMII_FHD\play\parts\gauge\*.png"),
        "parts/gauge/default.png"
    );
}
