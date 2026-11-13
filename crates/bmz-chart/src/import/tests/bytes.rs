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

#[test]
fn declared_asset_lookup_keeps_declared_bga_paths_without_probing_files() {
    let dir = tempfile::tempdir().unwrap();
    let chart = dir.path().join("song.bms");
    // Only the same-stem alternatives exist, so resolving picks them up.
    std::fs::write(dir.path().join("movie.mp4"), b"mp4").unwrap();
    std::fs::write(dir.path().join("still.png"), b"png").unwrap();
    let text = "#TITLE Assets\n#BPM 120\n#BMP01 movie.mpg\n#BMP02 still.bmp\n\
                #BMP03 missing.jpg\n#00004:010203\n#00011:01\n";
    std::fs::write(&chart, text).unwrap();
    let asset = |result: &ImportResult, key: u16| {
        let id = result.chart.bga_asset_by_bmp_key[&key];
        let asset = result.chart.bga_assets.iter().find(|asset| asset.id == id).unwrap();
        (asset.path.file_name().unwrap().to_string_lossy().into_owned(), asset.kind)
    };

    let resolved = import_chart_with_assets(
        &chart,
        BmsRandomSource::Seed(None),
        AssetLookup::Resolve { warn_missing: true },
    )
    .unwrap();
    assert_eq!(asset(&resolved, 1), ("movie.mp4".to_string(), BgaAssetKind::Video));
    assert_eq!(asset(&resolved, 2), ("still.png".to_string(), BgaAssetKind::Static));
    assert!(resolved.warnings.iter().any(|warning| matches!(
        warning,
        ImportWarning::MissingBmpFile { path } if path.ends_with("missing.jpg")
    )));

    let declared =
        import_chart_with_assets(&chart, BmsRandomSource::Seed(None), AssetLookup::Declared)
            .unwrap();
    assert_eq!(asset(&declared, 1), ("movie.mpg".to_string(), BgaAssetKind::Video));
    assert_eq!(asset(&declared, 2), ("still.bmp".to_string(), BgaAssetKind::Static));
    assert!(
        !declared
            .warnings
            .iter()
            .any(|warning| matches!(warning, ImportWarning::MissingBmpFile { .. }))
    );
    // Apart from the asset paths, the chart (all a library scan stores) is the same.
    let without_assets = |result: &ImportResult| {
        let mut chart = result.chart.clone();
        chart.bga_assets.clear();
        // A HashMap prints in arbitrary order; it is compared on its own below.
        chart.bga_asset_by_bmp_key.clear();
        format!("{chart:?}")
    };
    assert_eq!(without_assets(&declared), without_assets(&resolved));
    assert_eq!(declared.chart.bga_asset_by_bmp_key, resolved.chart.bga_asset_by_bmp_key);
}

#[test]
fn conditional_branches_reuse_import_asset_paths_after_files_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("song.bmc");
    let movie = dir.path().join("movie.mp4");
    std::fs::write(&movie, b"mp4").unwrap();
    let text = b"#BPM 120\n#BMP01 movie.mpg\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00204:01\n#DEFAULT\n#00304:01\n#ENDCONDITIONAL\n";
    let import = |assets| {
        import_chart_bytes_with_assets(&path, text, BmsRandomSource::Seed(None), assets)
            .unwrap()
            .chart
    };
    let resolved = import(AssetLookup::Resolve { warn_missing: true });
    let declared = import(AssetLookup::Declared);
    assert_eq!(resolved.bga_assets[0].path, movie);
    assert_eq!(declared.bga_assets[0].path, dir.path().join("movie.mpg"));
    // A branch must neither repeat disk probes nor change the preloaded BGA's path.
    std::fs::remove_file(&movie).unwrap();
    for chart in [resolved, declared] {
        let program = chart.metadata.conditional.as_ref().unwrap();
        for choice in [None, Some(0), Some(1)] {
            let branch = program.materialize(&[choice]).unwrap();
            assert_eq!(branch.bga_assets[0].path, chart.bga_assets[0].path);
            assert_eq!(branch.bga_assets[0].kind, chart.bga_assets[0].kind);
            assert_eq!(branch.bga_asset_by_bmp_key, chart.bga_asset_by_bmp_key);
        }
    }
}
