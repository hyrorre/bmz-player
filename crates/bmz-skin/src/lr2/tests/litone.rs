use super::*;

#[test]
fn lr2_customfile_tags_preserve_extensions_and_child_paths() {
    for (definition, asset, selected, expected) in [
        ("parts/*.png|1P|", "parts/*.png|1P|", "parts/blue.png", "parts/blue.png"),
        ("parts/*|1P|.png", "parts/*|1P|.png", "parts/blue.png", "parts/blue.png"),
        ("parts/*|1P|", "parts/*|1P|/part.csv", "parts/blue", "parts/blue/part.csv"),
    ] {
        assert!(same_wildcard_prefix(definition, asset));
        assert_eq!(substitute_wildcard(asset, definition, selected), expected);
        assert_eq!(substitute_wildcard_default(asset, definition, "blue"), expected);
    }
    assert!(!same_wildcard_prefix("parts/*|1P|", "parts/*|2P|/part.csv"));
}

fn tagged_directory_skin() -> (crate::tests::TestDir, PathBuf) {
    let root = unique_test_dir("lr2-tagged-directories");
    let play = root.join("Play");
    for choice in ["OFF", "A", "B"] {
        let directory = play.join("parts/graphs").join(choice);
        fs::create_dir_all(&directory).unwrap();
        for side in ["1P", "2P"] {
            for part in ["top", "bottom"] {
                fs::write(
                    directory.join(format!("{part}_{side}.csv")),
                    format!("#IMAGE,{choice}-{side}-{part}.png\n"),
                )
                .unwrap();
            }
        }
    }
    let path = play.join("play.lr2skin");
    fs::write(
        &path,
        concat!(
            "#INFORMATION,0,test,test\n",
            "#CUSTOMFILE,Side1,parts/graphs/*|1P|,OFF\n",
            "#CUSTOMFILE,Side2,parts/graphs/*|2P|,OFF\n",
            "#INCLUDE,parts/graphs/*|1P|/top_1P.csv\n",
            "#INCLUDE,parts/graphs/*|1P|/bottom_1P.csv\n",
            "#INCLUDE,parts/graphs/*|2P|/top_2P.csv\n",
            "#INCLUDE,parts/graphs/*|2P|/bottom_2P.csv\n",
        ),
    )
    .unwrap();
    (root, path)
}

#[test]
fn lr2_tagged_customfile_directories_keep_independent_selections() {
    let (_root, path) = tagged_directory_skin();
    for (files, expected) in [
        (BTreeMap::new(), ["OFF", "OFF"]),
        (BTreeMap::from([("Side1".into(), "A".into()), ("Side2".into(), "B".into())]), ["A", "B"]),
        (
            BTreeMap::from([
                ("Side1".into(), "parts/graphs/B".into()),
                ("Side2".into(), "parts/graphs/A".into()),
            ]),
            ["B", "A"],
        ),
        (
            BTreeMap::from([
                ("Side1".into(), "missing".into()),
                ("Side2".into(), "../parts/graphs/A".into()),
            ]),
            ["OFF", "OFF"],
        ),
    ] {
        let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &files).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let sources = loaded.value["source"].as_array().unwrap();
        assert_eq!(sources.len(), 4);
        for (index, (choice, side, part)) in [
            (expected[0], "1P", "top"),
            (expected[0], "1P", "bottom"),
            (expected[1], "2P", "top"),
            (expected[1], "2P", "bottom"),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(sources[index]["path"], format!("{choice}-{side}-{part}.png"));
        }
        assert_eq!(loaded.dependencies.files, BTreeSet::from(["Side1".into(), "Side2".into()]));
    }
}

#[test]
fn lr2_tagged_customfile_random_directory_is_shared_by_its_includes() {
    let (_root, path) = tagged_directory_skin();
    let files =
        BTreeMap::from([("Side1".into(), "Random".into()), ("Side2".into(), "Random".into())]);
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &files).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert!(loaded.dependencies.opaque);
    let sources = loaded.value["source"].as_array().unwrap();
    assert_eq!(sources.len(), 4);
    for (index, side) in [(0, "1P"), (2, "2P")] {
        let top = sources[index]["path"].as_str().unwrap();
        let choice = top.strip_suffix(&format!("-{side}-top.png")).unwrap();
        assert!(["OFF", "A", "B"].contains(&choice));
        assert_eq!(sources[index + 1]["path"], format!("{choice}-{side}-bottom.png"));
    }
}

#[test]
fn lr2_single_custom_offset_loads_as_document() {
    let root = unique_test_dir("lr2-single-custom-offset");
    let path = root.join("select.lr2skin");
    fs::write(&path, "#INFORMATION,5,test,test\n#CUSTOMOFFSET,Position,40,1,1,0,0,0,0\n").unwrap();
    let loaded = crate::load_lr2_csv_skin(
        &path,
        crate::SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(loaded.document.offset.len(), 1);
    let offset = &loaded.document.offset[0];
    assert_eq!(offset.id, 40);
    assert!(offset.x && offset.y);
    assert!(!offset.w && !offset.h && !offset.r && !offset.a);
}

#[test]
fn litone5_installed_definitions_load_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/LITONE5");
    let mut failures = Vec::new();
    for (relative, kind) in [
        ("Select/select.lr2skin", crate::SkinKind::Select),
        ("Decide/decide.lr2skin", crate::SkinKind::Decide),
        ("Result/result.lr2skin", crate::SkinKind::Result),
        ("Result/courseresult.lr2skin", crate::SkinKind::Result),
        ("Play/AC7.lr2skin", crate::SkinKind::Play),
        ("Play/CS7.lr2skin", crate::SkinKind::Play),
        ("Play/HALFWIDE7.lr2skin", crate::SkinKind::Play),
        ("Play/AC9.lr2skin", crate::SkinKind::Play),
        ("Play/AC14.lr2skin", crate::SkinKind::Play),
    ] {
        let path = root.join(relative);
        if !path.is_file() {
            eprintln!("SKIP missing {}", path.display());
            continue;
        }
        match crate::load_lr2_csv_skin(&path, kind, &BTreeMap::new(), &BTreeMap::new()) {
            Ok(loaded) => {
                eprintln!(
                    "LITONE5 {relative}: {} images, {} destinations, {} warnings",
                    loaded.document.image.len(),
                    loaded.document.destination.len(),
                    loaded.warnings.len()
                );
                for warning in &loaded.warnings {
                    eprintln!("  {}", warning.message);
                }
                assert!(loaded.document.lr2);
                assert!(!loaded.document.destination.is_empty(), "{relative}");
                // The installed AC9 OFF directories contain no CSVs.
                assert!(loaded.warnings.iter().all(|warning| !warning.message.contains('|')));
            }
            Err(error) => failures.push(format!("{relative}: {error:#}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn litone5_ac9_installed_graph_selections_load_when_available() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/LITONE5/Play/AC9.lr2skin");
    if !path.is_file() {
        eprintln!("SKIP missing {}", path.display());
        return;
    }
    let load = |files: &BTreeMap<String, String>| {
        crate::load_lr2_csv_skin(&path, crate::SkinKind::Play, &BTreeMap::new(), files).unwrap()
    };
    let defaults = load(&BTreeMap::new());
    let mut files = BTreeMap::new();
    for (side, choice) in [
        ("1P", "判定分布(JUDGE DISTRIBUTION)"),
        ("2P", "FAST SLOW分布&BPM推移(FAST SLOW DISTRIBUTION & BPM CHART)"),
    ] {
        let definition = defaults
            .document
            .filepath
            .iter()
            .find(|file| file.path.contains(&format!("side_graph/*|{side}|")))
            .unwrap();
        files.insert(definition.name.clone(), format!("parts/9keys/side_graph/{choice}"));
    }
    let loaded = load(&files);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    for directory in files.values() {
        assert!(
            loaded.dependencies.loaded_files.keys().any(|path| {
                path.to_string_lossy().replace('\\', "/").contains(&format!("{directory}/"))
            }),
            "missing {directory}"
        );
    }
    assert!(!loaded.document.judgegraph.is_empty());
    assert!(!loaded.document.bpmgraph.is_empty());
    eprintln!("LITONE5 AC9: distinct 1P / 2P graph directories loaded without warnings");
}
