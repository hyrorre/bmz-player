use super::*;

fn process(commands: &[&str]) -> CsvBuilder<'static> {
    static FILES: BTreeMap<String, String> = BTreeMap::new();
    let mut builder = CsvBuilder::new(Path::new("play.lr2skin"), Header::default(), &FILES);
    let lines: Vec<_> = commands.iter().map(|line| parse_csv_line(line).unwrap()).collect();
    Processor::new(HashMap::new())
        .process_lines(&lines, Path::new("play.lr2skin"), &mut builder)
        .unwrap();
    builder
}

#[test]
fn destinations_keep_pending_sources_separate_by_kind() {
    let builder = process(&[
        "#IMAGE,atlas.png",
        "#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0",
        "#SRC_NUMBER,0,0,0,0,110,10,11,1,0,0,101,0,4",
        "#DST_IMAGE,0,0,20,20,10,10,0,255,255,255,255,1",
        "#DST_NUMBER,0,0,30,20,10,10,0,255,255,255,255,1",
    ]);
    assert_eq!(builder.destinations[0]["id"], builder.images[0]["id"]);
    assert_eq!(builder.destinations[1]["id"], builder.values[0]["id"]);
}

#[test]
fn imageset_source_uses_the_image_destination_channel() {
    let builder = process(&[
        "#IMAGE,atlas.png",
        "#IMAGESET,0,0,0,0,10,10,1,1,0,0",
        "#SRC_IMAGESET,0,0,0,1,0",
        "#SRC_TEXT,0,0,10,1",
        "#DST_IMAGE,0,0,20,20,10,10,0,255,255,255,255,1",
    ]);
    assert_eq!(builder.destinations[0]["id"], builder.imagesets[0]["id"]);
}

#[test]
fn line_and_judge_destinations_do_not_become_image_fallbacks() {
    let builder = process(&[
        "#IMAGE,atlas.png",
        "#SRC_LINE,0,0,0,0,1,1,1,1,0,0",
        "#DST_LINE,0,0,20,20,10,1,0,255,255,255,255,1",
        "#SRC_NOWJUDGE_1P,0,0,0,0,10,10,1,1,0,0",
        "#DST_NOWJUDGE_1P,0,0,20,20,10,10,0,255,255,255,255,1",
        "#IF,154",
        "#SRC_IMAGE,0,0,30,0,10,10,1,1,0,0",
        "#ENDIF",
        "#DST_IMAGE,0,0,20,20,10,10,0,255,255,255,255,1",
    ]);
    assert!(
        builder.destinations.iter().all(|d| !d["id"].as_str().unwrap().starts_with("lr2-line"))
    );
    assert!(
        builder
            .destinations
            .iter()
            .all(|d| !d["id"].as_str().unwrap().starts_with("lr2-judge-image"))
    );
    assert_eq!(builder.destinations.last().unwrap()["id"], builder.images.last().unwrap()["id"]);
}

#[test]
fn conditional_source_replaces_the_default_and_earlier_alternatives() {
    let builder = process(&[
        "#IMAGE,atlas.png",
        "#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0",
        "#IF,154",
        "#SRC_IMAGE,0,0,10,0,10,10,1,1,0,0",
        "#ENDIF",
        "#IF,33,241",
        "#SRC_IMAGE,0,0,20,0,10,10,1,1,0,0",
        "#ENDIF",
        "#DST_IMAGE,0,0,20,20,10,10,0,255,255,255,255,1",
    ]);
    assert_eq!(
        builder.destinations[0]["lr2Not"],
        json!([{"op":[154],"lr2Not":[]}, {"op":[33,241],"lr2Not":[]}])
    );
    assert_eq!(builder.destinations[1]["lr2Not"], json!([{"op":[33,241],"lr2Not":[]}]));
    assert_eq!(builder.destinations[2]["lr2Not"], json!([]));
}

#[test]
fn compound_runtime_else_and_its_source_replacement_are_preserved() {
    let builder = process(&[
        "#IMAGE,atlas.png",
        "#IF,33,154",
        "#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0",
        "#ELSE",
        "#SRC_IMAGE,0,0,10,0,10,10,1,1,0,0",
        "#ENDIF",
        "#DST_IMAGE,0,0,20,20,10,10,0,255,255,255,255,1",
    ]);
    assert_eq!(builder.destinations.len(), 2);
    assert_eq!(builder.destinations[1]["lr2Not"], json!([{"op":[33,154],"lr2Not":[]}]));
    assert_eq!(
        builder.destinations[0]["lr2Not"][0]["lr2Not"],
        json!([{"op":[33,154],"lr2Not":[]}])
    );
}

#[test]
fn invalid_number_source_does_not_retarget_an_image_destination() {
    let builder = process(&[
        "#IMAGE,atlas.png",
        "#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0",
        "#SRC_NUMBER,0,9,0,0,110,10,11,1,0,0,101,0,4",
        "#DST_NUMBER,0,0,30,20,10,10,0,255,255,255,255,1",
        "#DST_IMAGE,0,0,20,20,10,10,0,255,255,255,255,1",
    ]);
    assert_eq!(builder.destinations.len(), 1);
    assert_eq!(builder.destinations[0]["id"], builder.images[0]["id"]);
}

#[test]
fn play_include_selects_one_autoplay_layout_and_records_its_dependency() {
    let root = unique_test_dir("bmz-lr2-play-include-mode");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.lr2skin");
    fs::write(&path,"#INFORMATION,0,test,test\n#CUSTOMOPTION,Side,900,Left,Right\n#IF,900,32\n#INCLUDE,normal.csv\n#ENDIF\n#IF,900,33\n#INCLUDE,auto.csv\n#ENDIF\n").unwrap();
    for (name, x) in [("normal", 10), ("auto", 90)] {
        fs::write(root.join(format!("{name}.csv")),format!("#IMAGE,{name}.png\n#IF,900\n#SRC_NUMBER,0,0,0,0,110,10,11,1,0,0,100,0,4\n#DST_NUMBER,0,0,{x},20,10,10,0,255,255,255,255,1\n#ENDIF\n")).unwrap();
    }
    for (autoplay, expected) in [(false, 10), (true, 90)] {
        let loaded = load_lr2_csv_skin_value_with_runtime_options(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::from([(32, !autoplay), (33, autoplay)]),
        )
        .unwrap();
        assert_eq!(loaded.value["source"].as_array().unwrap().len(), 1);
        assert_eq!(loaded.value["value"].as_array().unwrap().len(), 1);
        assert_eq!(loaded.value["destination"][0]["dst"][0]["x"], json!(expected));
        assert_eq!(loaded.dependencies.option_values[&32], !autoplay);
        assert_eq!(loaded.dependencies.option_values[&33], autoplay);
        let unused =
            root.join(if autoplay { "normal.csv" } else { "auto.csv" }).canonicalize().unwrap();
        assert!(!loaded.dependencies.loaded_files.contains_key(&unused));
    }
}

#[test]
fn result_fixed_conditions_keep_conditional_image_indices_stable() {
    let root = unique_test_dir("bmz-lr2-result-fixed-images");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("result.lr2skin");
    fs::write(&path, "#INFORMATION,7,test,test\n#IMAGE,frame.png\n#IF,290\n#ELSEIF,182,43\n#SETOPTION,978,1\n#ENDIF\n#IF,978\n#IMAGE,yellow.png\n#ELSE\n#IMAGE,red.png\n#ENDIF\n#IMAGE,rank.png\n").unwrap();
    for (gauge, expected) in
        [(None, "red.png"), (Some(true), "yellow.png"), (Some(false), "red.png")]
    {
        let mut runtime = BTreeMap::from([(182, true), (290, false)]);
        if let Some(enabled) = gauge {
            runtime.insert(43, enabled);
        }
        let loaded = load_lr2_csv_skin_value_with_runtime_options(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime,
        )
        .unwrap();
        assert_eq!(loaded.value["source"].as_array().unwrap().len(), 3);
        assert_eq!(loaded.value["source"][1]["path"], expected);
        assert_eq!(loaded.value["source"][2]["path"], "rank.png");
        assert_eq!(loaded.dependencies.option_values[&43], gauge.unwrap_or(false));
    }
}

#[test]
fn random_include_reuses_the_choice_for_canvas_and_conversion() {
    let root = unique_test_dir("bmz-lr2-random-include");
    fs::create_dir_all(root.join("motions")).unwrap();
    let path = root.join("play.lr2skin");
    fs::write(&path,"#INFORMATION,0,test,test\n#IMAGE,atlas.png\n#SETOPTION,930,1\n#IF,930\n#INCLUDE,motions/TYPE-*.csv\n#ENDIF\n").unwrap();
    for (name, w, h) in [("TYPE-1.csv", 640, 480), ("TYPE-2.CSV", 1280, 720)] {
        fs::write(root.join("motions").join(name),format!("#SRC_IMAGE,0,0,0,0,{w},{h},1,1,0,0\n#DST_IMAGE,0,0,0,0,{w},{h},0,255,255,255,255,1\n")).unwrap();
    }
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert!(loaded.dependencies.opaque);
    assert_eq!(loaded.value["w"], loaded.value["destination"][0]["dst"][0]["w"]);
    assert_eq!(loaded.value["h"], loaded.value["destination"][0]["dst"][0]["h"]);
    assert_eq!(
        loaded.dependencies.loaded_files.len(),
        1,
        "only the selected include; the entry is in the cache key"
    );
    assert!(
        !loaded.dependencies.option_values.contains_key(&930),
        "SETOPTION is a file dependency, not a user option"
    );
}

#[test]
fn custom_include_selection_is_fixed_and_missing_glob_reports_its_command() {
    let root = unique_test_dir("bmz-lr2-custom-include");
    fs::create_dir_all(root.join("preset")).unwrap();
    let path = root.join("play.lr2skin");
    fs::write(&path,"#INFORMATION,0,test,test\n#CUSTOMFILE,PRESET,preset/*.csv,A\n#INCLUDE,preset/*.csv\n#INCLUDE,missing-*.csv\n").unwrap();
    fs::write(root.join("preset/A.csv"), "#IMAGE,A.png\n").unwrap();
    fs::write(root.join("preset/B.csv"), "#IMAGE,B.png\n").unwrap();
    let files = BTreeMap::from([("PRESET".to_string(), "preset/B.csv".to_string())]);
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &files).unwrap();
    assert_eq!(loaded.value["source"][0]["path"], "B.png");
    assert!(!loaded.dependencies.opaque);
    assert_eq!(loaded.warnings.len(), 1);
    assert!(loaded.warnings[0].message.contains("play.lr2skin: command"));
}
