use super::*;

#[test]
fn load_select_items_in_table_returns_charts_sorted_by_level_order() {
    let (mut library_db, score_db) = open_in_memory_dbs();

    let hard = chart("Hard Song");
    let easy = chart("Easy Song");
    library_db.upsert_chart_import(&record_for_chart("/songs/hard.bms", &hard)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/easy.bms", &easy)).unwrap();

    // Table has level_order ["5", "10"] — easy(5) before hard(10)
    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/table/".to_string(),
        head_url: "https://example.com/table/header.json".to_string(),
        name: "Test Table".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["5".to_string(), "10".to_string()],
        entries: vec![
            FetchedTableEntry {
                level: "10".to_string(),
                md5: hash_to_hex(&hard.identity.file_md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
            FetchedTableEntry {
                level: "5".to_string(),
                md5: hash_to_hex(&easy.identity.file_md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
        ],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = load_select_items_in_table(
        &library_db,
        &score_db,
        "https://example.com/table/",
        LnPolicySetting::AutoLn,
    )
    .unwrap();

    assert_eq!(items.len(), 2);
    let titles: Vec<_> = items
        .iter()
        .filter_map(|i| if let SelectItem::Chart(r) = i { Some(r.display_title()) } else { None })
        .collect();
    assert_eq!(titles[0], "Easy Song");
    assert_eq!(titles[1], "Hard Song");

    // table_level should be formatted as symbol+level
    let levels: Vec<_> =
        items
            .iter()
            .filter_map(|i| {
                if let SelectItem::Chart(r) = i { Some(r.table_level.as_str()) } else { None }
            })
            .collect();
    assert_eq!(levels[0], "★5");
    assert_eq!(levels[1], "★10");
}

#[test]
fn table_source_url_from_context_reads_stack_and_selection() {
    let stack = vec!["bmz-table:https://example.com/t/\n12".to_string()];
    assert_eq!(
        table_source_url_from_context(&stack, None),
        Some("https://example.com/t/".to_string())
    );

    let selected = SelectItem::Folder {
        path: "bmz-table:https://example.com/other/".to_string(),
        name: "[★] Other".to_string(),
        kind: SelectRowKind::TableFolder,
        summary: None,
    };
    assert_eq!(
        table_source_url_from_context(&[], Some(&selected)),
        Some("https://example.com/other/".to_string())
    );

    assert_eq!(table_source_url_from_context(&[], None), None);
}

#[test]
fn song_scan_path_from_context_reads_folder_and_chart() {
    let folder = SelectItem::Folder {
        path: "/music/bms".to_string(),
        name: "bms".to_string(),
        kind: SelectRowKind::Folder,
        summary: None,
    };
    assert_eq!(song_scan_path_from_context(&[], Some(&folder)), Some("/music/bms".to_string()));

    let chart = SelectItem::Chart(SelectChartRow {
        chart: Some(ChartListItem {
            chart_id: 1,
            md5: [0; 16],
            sha256: [0; 32],
            title: "Song".to_string(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            mode: String::new(),
            total_notes: 10,
            initial_bpm: 120.0,
            min_bpm: 120.0,
            max_bpm: 120.0,
            length_ms: 0,
            folder_path: "/music/bms/album".to_string(),
            stage_file: String::new(),
            banner_file: String::new(),
            backbmp_file: String::new(),
            preview_file: String::new(),
            has_document: false,
            has_bga: false,
            has_long_notes: false,
            has_mines: false,
            has_bms_random: false,
            judge_rank: None,
            bms_total: 0.0,
            ln_profile: Default::default(),
            ln_counts: Default::default(),
        }),
        chart_analysis: None,
        has_document: false,
        fallback_title: String::new(),
        fallback_artist: String::new(),
        entry_sha256: None,
        download_metadata: ChartDownloadMetadata::default(),
        best_score: None,
        replay_slots: [false; 4],
        favorite_chart: false,
        favorite_song: false,
        table_level: String::new(),
        table_text: DifficultyTableText::default(),
    });
    assert_eq!(
        song_scan_path_from_context(&[], Some(&chart)),
        Some("/music/bms/album".to_string())
    );
}

#[test]
fn parse_table_path_distinguishes_root_table_and_level() {
    assert_eq!(parse_table_path("bmz-table:"), Some(TablePath::Root));
    assert_eq!(
        parse_table_path("bmz-table:https://example.com/t/"),
        Some(TablePath::Table { source_url: "https://example.com/t/" })
    );
    assert_eq!(
        parse_table_path("bmz-table:https://example.com/t/\n12"),
        Some(TablePath::Level { source_url: "https://example.com/t/", level: "12" })
    );
    assert_eq!(parse_table_path("/songs/folder"), None);
}

#[test]
fn course_contents_path_round_trips_course_id() {
    let path = course_contents_path(42);
    assert_eq!(path, "bmz-course-contents:42");
    assert_eq!(parse_course_contents_path(&path), Some(42));
    assert_eq!(parse_course_contents_path(COURSE_ROOT_PATH), None);
}

#[test]
fn course_contents_preserve_stage_order_and_missing_download_metadata() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let first = chart("First Stage");
    let third = chart("Third Stage");
    let first_id =
        library_db.upsert_chart_import(&record_for_chart("/songs/first.bms", &first)).unwrap();
    let third_id =
        library_db.upsert_chart_import(&record_for_chart("/songs/third.bms", &third)).unwrap();
    let missing_md5 = "0123456789abcdef0123456789abcdef";
    let source_url = "https://example.com/course-table/";

    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    library_db
        .upsert_difficulty_table(&FetchedDifficultyTable {
            source_url: source_url.to_string(),
            head_url: format!("{source_url}header.json"),
            name: "Course Table".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["12".to_string()],
            entries: vec![FetchedTableEntry {
                level: "12".to_string(),
                md5: missing_md5.to_string(),
                sha256: String::new(),
                title: "Missing Stage".to_string(),
                artist: "Missing Artist".to_string(),
                comment: String::new(),
                url: "https://example.com/missing-stage".to_string(),
                append_url: String::new(),
                ipfs: "/ipfs/missing-stage".to_string(),
                append_ipfs: String::new(),
            }],
            courses: Vec::new(),
            fetched_at: 0,
        })
        .unwrap();
    let course_id = library_db
        .upsert_course(
            &format!("table:{source_url}"),
            &bmz_core::course::CourseDefinition {
                key: "ordered-course".to_string(),
                title: "Ordered Course".to_string(),
                kind: bmz_core::course::CourseKind::Course,
                entries: vec![
                    bmz_core::course::CourseEntry {
                        title_hint: "First hint".to_string(),
                        md5: None,
                        sha256: Some(hash_to_hex(&first.identity.file_sha256)),
                        chart_id: Some(first_id),
                    },
                    bmz_core::course::CourseEntry {
                        title_hint: "Missing hint".to_string(),
                        md5: Some(missing_md5.to_string()),
                        sha256: None,
                        chart_id: None,
                    },
                    bmz_core::course::CourseEntry {
                        title_hint: "Third hint".to_string(),
                        md5: None,
                        sha256: Some(hash_to_hex(&third.identity.file_sha256)),
                        chart_id: Some(third_id),
                    },
                ],
                constraints: bmz_core::course::CourseConstraints::default(),
                trophies: Vec::new(),
                release: true,
            },
            0,
            1,
        )
        .unwrap();

    let items = load_select_items_for_course_contents(
        &library_db,
        &score_db,
        course_id,
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
    )
    .unwrap();
    let rows: Vec<&SelectChartRow> = items
        .iter()
        .filter_map(|item| match item {
            SelectItem::Chart(row) => Some(row),
            _ => None,
        })
        .collect();
    assert_eq!(
        rows.iter().map(|row| row.display_title()).collect::<Vec<_>>(),
        ["First Stage", "Missing Stage", "Third Stage",]
    );
    assert!(rows[0].in_library());
    assert!(!rows[1].in_library());
    assert_eq!(rows[1].display_artist(), "Missing Artist");
    assert_eq!(rows[1].download_metadata.url, "https://example.com/missing-stage");
    assert_eq!(rows[1].table_level, "★12");
    assert!(rows[2].in_library());
}

#[test]
fn table_level_folder_items_hide_undefined_levels_but_keep_unowned_levels() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart_a = chart("A");
    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/insane/".to_string(),
        head_url: "https://example.com/insane/header.json".to_string(),
        name: "Insane".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["1".to_string(), "2".to_string(), "25".to_string()],
        entries: vec![FetchedTableEntry {
            level: "2".to_string(),
            md5: hash_to_hex(&chart_a.identity.file_md5),
            sha256: String::new(),
            title: String::new(),
            artist: String::new(),
            comment: String::new(),
            ..FetchedTableEntry::default()
        }],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = table_level_folder_items(
        &library_db,
        &score_db,
        "https://example.com/insane/",
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        SelectItem::Folder { path, name, kind, .. }
        if name == "★2" && path == "bmz-table:https://example.com/insane/\n2" && *kind == SelectRowKind::TableFolder
    ));
}

#[test]
fn load_select_items_in_table_level_filters_by_level() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let easy = chart("Easy Song");
    let hard = chart("Hard Song");
    library_db.upsert_chart_import(&record_for_chart("/songs/easy.bms", &easy)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/hard.bms", &hard)).unwrap();

    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/insane/".to_string(),
        head_url: "https://example.com/insane/header.json".to_string(),
        name: "Insane".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["5".to_string(), "10".to_string()],
        entries: vec![
            FetchedTableEntry {
                level: "5".to_string(),
                md5: hash_to_hex(&easy.identity.file_md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
            FetchedTableEntry {
                level: "10".to_string(),
                md5: hash_to_hex(&hard.identity.file_md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
        ],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = load_select_items_in_table_level(
        &library_db,
        &score_db,
        "https://example.com/insane/",
        "5",
        LnPolicySetting::AutoLn,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0], SelectItem::Chart(r) if r.display_title() == "Easy Song"));
}

#[test]
fn load_select_items_in_table_level_shows_missing_library_entry() {
    let (mut library_db, score_db) = open_in_memory_dbs();

    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/missing/".to_string(),
        head_url: "https://example.com/missing/header.json".to_string(),
        name: "Missing".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["12".to_string()],
        entries: vec![FetchedTableEntry {
            level: "12".to_string(),
            md5: "aabbcc".repeat(5) + "aabb",
            sha256: String::new(),
            title: "Missing Song".to_string(),
            artist: "Missing Artist".to_string(),
            comment: String::new(),
            url: "https://example.com/missing".to_string(),
            append_url: "https://example.com/missing-diff".to_string(),
            ipfs: "/ipfs/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3ktekzrxql4i5f3u".to_string(),
            append_ipfs: String::new(),
        }],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = load_select_items_in_table_level(
        &library_db,
        &score_db,
        "https://example.com/missing/",
        "12",
        LnPolicySetting::AutoLn,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        SelectItem::Chart(row)
        if row.display_title() == "Missing Song"
            && row.display_artist() == "Missing Artist"
            && !row.in_library()
            && row.download_metadata.url == "https://example.com/missing"
            && row.download_metadata.ipfs.starts_with("/ipfs/")
    ));
}

#[test]
fn load_select_items_in_table_level_prefers_library_title_when_registered() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart = chart("Library Title");
    library_db.upsert_chart_import(&record_for_chart("/songs/registered.bms", &chart)).unwrap();

    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/registered/".to_string(),
        head_url: "https://example.com/registered/header.json".to_string(),
        name: "Registered".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["12".to_string()],
        entries: vec![FetchedTableEntry {
            level: "12".to_string(),
            md5: hash_to_hex(&chart.identity.file_md5),
            sha256: String::new(),
            title: "Table Title".to_string(),
            artist: "Table Artist".to_string(),
            comment: String::new(),
            ..FetchedTableEntry::default()
        }],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = load_select_items_in_table_level(
        &library_db,
        &score_db,
        "https://example.com/registered/",
        "12",
        LnPolicySetting::AutoLn,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        SelectItem::Chart(row)
        if row.display_title() == "Library Title" && row.in_library()
    ));
}

#[test]
fn load_select_items_in_table_level_dedupes_matched_chart_and_stale_hash_row() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart = chart("Registered Song");
    library_db.upsert_chart_import(&record_for_chart("/songs/registered.bms", &chart)).unwrap();

    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/dedupe/".to_string(),
        head_url: "https://example.com/dedupe/header.json".to_string(),
        name: "Dedupe".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["12".to_string()],
        entries: vec![
            FetchedTableEntry {
                level: "12".to_string(),
                md5: hash_to_hex(&chart.identity.file_md5),
                sha256: String::new(),
                title: "Registered Song".to_string(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
            FetchedTableEntry {
                level: "12".to_string(),
                md5: "deadbeef".repeat(4),
                sha256: String::new(),
                title: "Registered Song".to_string(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
        ],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = load_select_items_in_table_level(
        &library_db,
        &score_db,
        "https://example.com/dedupe/",
        "12",
        LnPolicySetting::AutoLn,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        SelectItem::Chart(row)
        if row.display_title() == "Registered Song" && row.in_library()
    ));
}

#[test]
fn load_select_items_in_table_level_dedupes_md5_and_sha256_rows_for_same_chart() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart = chart("Dual Hash Song");
    library_db.upsert_chart_import(&record_for_chart("/songs/dual.bms", &chart)).unwrap();

    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/dual/".to_string(),
        head_url: "https://example.com/dual/header.json".to_string(),
        name: "Dual".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["12".to_string()],
        entries: vec![
            FetchedTableEntry {
                level: "12".to_string(),
                md5: hash_to_hex(&chart.identity.file_md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
            FetchedTableEntry {
                level: "12".to_string(),
                md5: String::new(),
                sha256: hash_to_hex(&chart.identity.file_sha256),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
        ],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = load_select_items_in_table_level(
        &library_db,
        &score_db,
        "https://example.com/dual/",
        "12",
        LnPolicySetting::AutoLn,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0], SelectItem::Chart(row) if row.in_library()));
}

#[test]
fn load_select_items_in_table_level_dedupes_duplicate_library_chart_ids() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart = chart("Duplicate Import Song");
    let chart_id_a =
        library_db.upsert_chart_import(&record_for_chart("/songs/a/track.bms", &chart)).unwrap();
    let chart_id_b =
        library_db.upsert_chart_import(&record_for_chart("/songs/b/track.bms", &chart)).unwrap();
    assert_ne!(chart_id_a, chart_id_b);

    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    let table = FetchedDifficultyTable {
        source_url: "https://example.com/dup-import/".to_string(),
        head_url: "https://example.com/dup-import/header.json".to_string(),
        name: "Dup Import".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["12".to_string()],
        entries: vec![
            FetchedTableEntry {
                level: "12".to_string(),
                md5: hash_to_hex(&chart.identity.file_md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
            FetchedTableEntry {
                level: "12".to_string(),
                md5: String::new(),
                sha256: hash_to_hex(&chart.identity.file_sha256),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
                ..FetchedTableEntry::default()
            },
        ],
        courses: Vec::new(),
        fetched_at: 0,
    };
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = load_select_items_in_table_level(
        &library_db,
        &score_db,
        "https://example.com/dup-import/",
        "12",
        LnPolicySetting::AutoLn,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0], SelectItem::Chart(row) if row.in_library()));
}

#[test]
fn parse_search_query_round_trips() {
    assert_eq!(parse_search_query("bmz-search:blue"), Some("blue"));
    assert_eq!(parse_search_query("bmz-search:"), None);
    assert_eq!(parse_search_query("/songs/blue"), None);
    assert_eq!(parse_search_query("bmz-table:foo"), None);
}

#[test]
fn search_history_folder_items_formats_each_entry() {
    let history = vec!["alpha".to_string(), "beta".to_string()];
    let items = search_history_folder_items(&history);
    assert_eq!(items.len(), 2);
    match &items[0] {
        SelectItem::Folder { path, name, kind, summary } => {
            assert_eq!(path, "bmz-search:alpha");
            assert_eq!(name, "検索: 'alpha'");
            assert_eq!(*kind, SelectRowKind::SearchFolder);
            assert_eq!(*summary, None);
        }
        other => panic!("expected folder, got {other:?}"),
    }
    match &items[1] {
        SelectItem::Folder { name, .. } => assert_eq!(name, "検索: 'beta'"),
        other => panic!("expected folder, got {other:?}"),
    }

    let english = search_history_folder_items_for_locale(&history, AppLocale::En);
    assert!(matches!(
        &english[0],
        SelectItem::Folder { name, .. } if name == "Search: 'alpha'"
    ));
}

#[test]
fn load_select_items_for_search_returns_chart_rows_with_best_score() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let mut sky = chart("Blue Sky");
    sky.metadata.artist = "Composer A".to_string();
    let mut unrelated = chart("Sunset");
    unrelated.metadata.artist = "Solo".to_string();

    library_db.upsert_chart_import(&record_for_chart("/songs/a.bms", &sky)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/b.bms", &unrelated)).unwrap();
    score_db.insert_score(&score_for_chart(sky.identity.file_sha256)).unwrap();

    let items =
        load_select_items_for_search(&library_db, &score_db, "blue", LnPolicySetting::AutoLn)
            .unwrap();
    assert_eq!(items.len(), 1);
    let row = match &items[0] {
        SelectItem::Chart(r) => r,
        other => panic!("expected chart row, got {other:?}"),
    };
    assert_eq!(row.display_title(), "Blue Sky");
    assert!(row.best_score.is_some());
}

#[test]
fn load_select_items_for_search_with_filters_hides_removed_song_roots() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let active = chart("Blue Active");
    let stale = chart("Blue Stale");
    library_db
        .upsert_chart_import(&record_for_chart("/songs/enabled/active.bms", &active))
        .unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/removed/stale.bms", &stale)).unwrap();

    let active_roots = vec!["/songs/enabled".to_string()];
    let items = load_select_items_for_search_for_rule_mode_with_filters(
        &library_db,
        &score_db,
        "blue",
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
        &[],
        Some(&active_roots),
        None,
    )
    .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0], SelectItem::Chart(row) if row.display_title() == "Blue Active"));
}
