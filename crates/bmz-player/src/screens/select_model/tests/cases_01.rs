use super::*;

#[test]
fn load_select_items_in_folder_attaches_best_scores_by_hash() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let alpha = chart("Alpha");
    let beta = chart("Beta");

    library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/beta.bms", &beta)).unwrap();
    score_db.insert_score(&score_for_chart(alpha.identity.file_sha256)).unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
            .unwrap();

    let charts: Vec<_> = items
        .iter()
        .filter_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
        .collect();
    assert_eq!(charts.len(), 2);
    assert_eq!(charts[0].display_title(), "Alpha");
    assert!(charts[0].best_score.is_some());
    assert_eq!(charts[1].display_title(), "Beta");
    assert!(charts[1].best_score.is_none());
}

#[test]
fn virtual_folder_profile_query_filters_library_charts() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let mut low = chart("Low");
    low.metadata.play_level = "5".to_string();
    let mut high = chart("High");
    high.metadata.play_level = "12".to_string();
    library_db.upsert_chart_import(&record_for_chart("/songs/low.bms", &low)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/high.bms", &high)).unwrap();

    let profile_root = std::env::temp_dir().join(format!(
        "bmz-virtual-query-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&profile_root).unwrap();
    std::fs::write(
        profile_root.join(VIRTUAL_FOLDER_CONFIG_FILE),
        r#"
version = 1

[[folders]]
id = "custom"
name = "CUSTOM"
query = "mode == '7K' && level >= 10"
"#,
    )
    .unwrap();

    let items = load_select_items_in_virtual_folder(
        &library_db,
        &score_db,
        &profile_root,
        "bmz-filter:custom",
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
        None,
    )
    .unwrap();
    let titles = items
        .iter()
        .filter_map(|item| match item {
            SelectItem::Chart(row) => Some(row.display_title()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(titles, ["High"]);
    std::fs::remove_dir_all(profile_root).unwrap();
}

#[test]
fn virtual_folder_skips_difficulty_table_enrichment() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let mut mirrorworld = chart("War in the Mirrorworld[ANOTHER]");
    mirrorworld.metadata.play_level = "11".to_string();
    library_db
        .upsert_chart_import(&record_for_chart("/songs/mirrorworld.bms", &mirrorworld))
        .unwrap();
    library_db
        .upsert_difficulty_table(&difficulty_table_for_md5(
            &mirrorworld.identity.file_md5,
            "sl",
            "0",
        ))
        .unwrap();

    let profile_root = std::env::temp_dir().join(format!(
        "bmz-virtual-no-table-enrichment-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&profile_root).unwrap();
    std::fs::write(
        profile_root.join(VIRTUAL_FOLDER_CONFIG_FILE),
        r#"
version = 1

[[folders]]
id = "custom"
name = "CUSTOM"
query = "level == 11"
"#,
    )
    .unwrap();

    let items = load_select_items_in_virtual_folder(
        &library_db,
        &score_db,
        &profile_root,
        "bmz-filter:custom",
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
        None,
    )
    .unwrap();
    let row = items
        .iter()
        .find_map(|item| match item {
            SelectItem::Chart(row) => Some(row),
            _ => None,
        })
        .unwrap();

    assert_eq!(row.chart.as_ref().unwrap().play_level, "11");
    assert!(row.table_level.is_empty());
    assert_eq!(row.table_text, DifficultyTableText::default());
    std::fs::remove_dir_all(profile_root).unwrap();
}

#[test]
fn folder_enrichment_keeps_hash_metadata_for_duplicate_hashes() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let duplicate = chart("Duplicate");
    library_db
        .upsert_chart_import(&record_for_chart("/songs/first/duplicate.bms", &duplicate))
        .unwrap();
    library_db
        .upsert_chart_import(&record_for_chart("/songs/second/duplicate.bms", &duplicate))
        .unwrap();
    library_db
        .upsert_difficulty_table(&difficulty_table_for_md5(&duplicate.identity.file_md5, "★", "3"))
        .unwrap();
    score_db.insert_score(&score_for_chart(duplicate.identity.file_sha256)).unwrap();
    score_db
        .upsert_replay_slot(&crate::storage::score_db::ReplaySlotRecord {
            chart_sha256: duplicate.identity.file_sha256,
            ln_policy: LnScorePolicy::ForceLn,
            double_option: crate::select_options::DoubleOptionScoreBucket::Off,
            rule_mode: RuleMode::Beatoraja,
            slot: 0,
            rule: crate::config::profile_config::ReplaySlotRule::Always,
            replay_path: "replay/duplicate.toml".to_string(),
            played_at: 1_700_000_030,
            ex_score: Some(2),
            bp: Some(0),
            cb: Some(0),
            max_combo: Some(1),
            clear_rank: Some(ClearType::Normal as u8),
            source_kind: crate::storage::score_db::ScoreSourceKind::Local,
            source_path: String::new(),
            source_fingerprint: String::new(),
        })
        .unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
            .unwrap();
    let rows = items
        .iter()
        .filter_map(|item| match item {
            SelectItem::Chart(row) => Some(row),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.best_score.is_some()));
    assert!(rows.iter().all(|row| row.replay_slots == [true, false, false, false]));
    assert_eq!(rows.iter().map(|row| row.table_level.as_str()).collect::<Vec<_>>(), ["★3", "★3"]);
    assert_eq!(
        rows.iter().map(|row| row.table_text.table_level.as_str()).collect::<Vec<_>>(),
        ["★3", "★3"]
    );
}

#[test]
fn virtual_folder_deduplicates_sha256_before_limit() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let duplicate = chart("Duplicate");
    let unique = chart("Unique");
    library_db
        .upsert_chart_import(&record_for_chart("/songs/first/duplicate.bms", &duplicate))
        .unwrap();
    library_db
        .upsert_chart_import(&record_for_chart("/songs/second/duplicate.bms", &duplicate))
        .unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/unique.bms", &unique)).unwrap();
    score_db.insert_score(&score_for_chart(duplicate.identity.file_sha256)).unwrap();
    score_db.insert_score(&score_for_chart(duplicate.identity.file_sha256)).unwrap();
    score_db.insert_score(&score_for_chart(unique.identity.file_sha256)).unwrap();

    let profile_root = std::env::temp_dir().join(format!(
        "bmz-virtual-deduplicate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&profile_root).unwrap();
    std::fs::write(
        profile_root.join(VIRTUAL_FOLDER_CONFIG_FILE),
        r#"
version = 1

[[folders]]
id = "limited"
name = "LIMITED"
[folders.query]
filter = "play_count > 0"
order_by = "play_count desc"
limit = 2
"#,
    )
    .unwrap();

    let items = load_select_items_in_virtual_folder(
        &library_db,
        &score_db,
        &profile_root,
        "bmz-filter:limited",
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
        None,
    )
    .unwrap();
    let titles = items
        .iter()
        .filter_map(|item| match item {
            SelectItem::Chart(row) => Some(row.display_title()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(titles, ["Duplicate", "Unique"]);
    std::fs::remove_dir_all(profile_root).unwrap();
}

#[test]
fn built_in_score_folders_exclude_no_play_scores() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let mut played = chart("Played");
    played.total_notes = 1;
    let mut no_play = chart("No Play");
    no_play.total_notes = 1;
    library_db.upsert_chart_import(&record_for_chart("/songs/played.bms", &played)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/no-play.bms", &no_play)).unwrap();
    score_db.insert_score(&score_for_chart(played.identity.file_sha256)).unwrap();
    let mut no_play_score = score_for_chart(no_play.identity.file_sha256);
    no_play_score.clear_type = ClearType::NoPlay;
    score_db.insert_score(&no_play_score).unwrap();

    let profile_root = std::env::temp_dir().join(format!(
        "bmz-virtual-no-play-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for path in ["bmz-filter:score-rank/aaa", "bmz-filter:my-best"] {
        let items = load_select_items_in_virtual_folder(
            &library_db,
            &score_db,
            &profile_root,
            path,
            LnPolicySetting::AutoLn,
            RuleMode::Beatoraja,
            None,
        )
        .unwrap();
        let titles = items
            .iter()
            .filter_map(|item| match item {
                SelectItem::Chart(row) => Some(row.display_title()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(titles, ["Played"], "unexpected items in {path}");
    }
}

#[test]
fn load_select_items_in_folder_attaches_replay_slots_from_replay_slots_table() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let alpha = chart("Alpha");

    library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
    for slot in 0..4_u8 {
        score_db
            .upsert_replay_slot(&crate::storage::score_db::ReplaySlotRecord {
                chart_sha256: alpha.identity.file_sha256,
                ln_policy: LnScorePolicy::ForceLn,
                double_option: crate::select_options::DoubleOptionScoreBucket::Off,
                rule_mode: RuleMode::Beatoraja,
                slot,
                rule: crate::config::profile_config::ReplaySlotRule::Always,
                replay_path: format!("replay/{slot}.toml"),
                played_at: 1_700_000_030 + slot as i64,
                ex_score: Some(10 * slot as u32),
                bp: Some(0),
                cb: Some(0),
                max_combo: Some(10),
                clear_rank: Some(ClearType::Normal as u8),
                source_kind: crate::storage::score_db::ScoreSourceKind::Local,
                source_path: String::new(),
                source_fingerprint: String::new(),
            })
            .unwrap();
    }

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
            .unwrap();

    let row = items
        .iter()
        .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
        .unwrap();
    assert_eq!(row.replay_slots, [true, true, true, true]);
}

#[test]
fn load_select_items_uses_profile_ln_policy_for_score_lookup() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let mut alpha = chart("Alpha");
    alpha.long_notes.push(undefined_ln_pair());
    library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
    let mut force_ln_score = score_for_chart(alpha.identity.file_sha256);
    force_ln_score.ln_policy = LnScorePolicy::ForceLn;
    force_ln_score.score.judges.slow_pgreat = 50;
    let mut force_cn_score = score_for_chart(alpha.identity.file_sha256);
    force_cn_score.ln_policy = LnScorePolicy::ForceCn;
    force_cn_score.score.judges.slow_pgreat = 100;
    score_db.insert_score(&force_ln_score).unwrap();
    score_db.insert_score(&force_cn_score).unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoCn)
            .unwrap();

    let row = items
        .iter()
        .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
        .unwrap();
    assert_eq!(row.best_score.as_ref().map(|s| s.ln_policy), Some(LnScorePolicy::ForceCn));
    assert_eq!(row.best_score.as_ref().map(|s| s.ex_score), Some(200));
}

#[test]
fn load_select_items_in_folder_flattens_leaf_subfolders() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart_a = chart("A");
    let chart_b = chart("B");

    // chart_b directly in /bms; chart_a is in a leaf sub-folder (no deeper nesting)
    library_db.upsert_chart_import(&record_for_chart("/bms/genre/song_a.bms", &chart_a)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/bms/song_b.bms", &chart_b)).unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/bms", LnPolicySetting::AutoLn)
            .unwrap();

    // genre is a leaf folder so its chart appears directly, not as a Folder entry
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| matches!(i, SelectItem::Chart(_))));
    let titles: Vec<_> = items
        .iter()
        .filter_map(|i| if let SelectItem::Chart(r) = i { Some(r.display_title()) } else { None })
        .collect();
    assert!(titles.contains(&"A"));
    assert!(titles.contains(&"B"));
}

#[test]
fn load_select_items_in_folder_shows_non_leaf_subfolder_as_folder() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart_a = chart("A");
    let chart_b = chart("B");

    // genre/subgenre/song_a — genre has a subfolder so it is non-leaf
    library_db
        .upsert_chart_import(&record_for_chart("/bms/genre/subgenre/song_a.bms", &chart_a))
        .unwrap();
    library_db.upsert_chart_import(&record_for_chart("/bms/song_b.bms", &chart_b)).unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/bms", LnPolicySetting::AutoLn)
            .unwrap();

    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0], SelectItem::Folder { name, .. } if name == "genre"));
    assert!(matches!(&items[1], SelectItem::Chart(r) if r.display_title() == "B"));
}

#[test]
fn load_select_items_in_folder_with_filters_hides_charts_outside_active_roots() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let active = chart("Active Song");
    let stale = chart("Stale Song");
    library_db
        .upsert_chart_import(&record_for_chart("/songs/enabled/active.bms", &active))
        .unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/removed/stale.bms", &stale)).unwrap();

    let active_roots = vec!["/songs/enabled".to_string()];
    let items = load_select_items_in_folder_for_rule_mode_with_filters(
        &library_db,
        &score_db,
        "/songs",
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
        &[],
        Some(&active_roots),
        None,
    )
    .unwrap();

    let titles: Vec<_> = items
        .iter()
        .filter_map(|item| {
            if let SelectItem::Chart(row) = item { Some(row.display_title()) } else { None }
        })
        .collect();
    assert_eq!(titles, vec!["Active Song"]);
}

#[test]
fn select_folder_summary_counts_recursive_folder_lamps() {
    let (mut library_db, mut score_db) = open_in_memory_dbs();
    let normal = chart("Normal");
    let hard = chart("Hard");
    let unplayed = chart("Unplayed");
    let outside = chart("Outside");
    library_db.upsert_chart_import(&record_for_chart("/songs/folder/normal.bms", &normal)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/folder/sub/hard.bms", &hard)).unwrap();
    library_db
        .upsert_chart_import(&record_for_chart("/songs/folder/sub/unplayed.bms", &unplayed))
        .unwrap();
    library_db.upsert_chart_import(&record_for_chart("/songs/outside.bms", &outside)).unwrap();
    score_db.insert_score(&score_for_chart(normal.identity.file_sha256)).unwrap();
    let mut hard_score = score_for_chart(hard.identity.file_sha256);
    hard_score.clear_type = ClearType::Hard;
    score_db.insert_score(&hard_score).unwrap();
    score_db.insert_score(&score_for_chart(outside.identity.file_sha256)).unwrap();

    let summary = select_folder_summary(
        &library_db,
        &score_db,
        "/songs/folder",
        SelectRowKind::Folder,
        LnPolicySetting::AutoLn,
    )
    .unwrap()
    .unwrap();

    assert_eq!(summary.lamp_counts[0], 1);
    assert_eq!(summary.lamp_counts[5], 1);
    assert_eq!(summary.lamp_counts[6], 1);
    assert_eq!(summary.lamp_counts.iter().sum::<u32>(), 3);
    assert_eq!(summary.clear_type(), "");
}

#[test]
fn root_folder_items_returns_folder_per_root() {
    let roots = vec!["/bms/a".to_string(), "/bms/b".to_string()];
    let items = root_folder_items(&roots);

    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0], SelectItem::Folder { name, .. } if name == "a"));
    assert!(matches!(&items[1], SelectItem::Folder { name, .. } if name == "b"));
}

#[test]
fn collection_cache_tracks_local_changes_library_updates_and_profile_reset() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let mut collection_db = open_in_memory_collection_db();
    let shared = chart("Cached favorite");
    library_db.upsert_chart_import(&record_for_chart("/pack-a/song/shared.bms", &shared)).unwrap();
    let mut items = load_select_items_in_folder(
        &library_db,
        &score_db,
        "/pack-a/song",
        LnPolicySetting::AutoLn,
    )
    .unwrap();
    let mut cache = SelectCollectionCache::default();
    cache.apply(&library_db, &collection_db, &mut items).unwrap();
    let flags = |items: &[SelectItem]| match &items[0] {
        SelectItem::Chart(row) => (row.favorite_chart, row.favorite_song),
        _ => panic!("expected chart"),
    };
    assert_eq!(flags(&items), (false, false));
    let hints =
        crate::storage::collection_db::FavoriteHints::new("Cached favorite", "", "/pack-a/song");
    collection_db.toggle_favorite_chart(shared.identity.file_sha256, &hints, 1).unwrap();
    collection_db.upsert_favorite_song(shared.identity.file_sha256, &hints, 1).unwrap();
    cache.apply(&library_db, &collection_db, &mut items).unwrap();
    assert_eq!(flags(&items), (true, true));
    // New duplicate folders must join the cached song favorite after a scan/import.
    library_db.upsert_chart_import(&record_for_chart("/pack-b/song/shared.bms", &shared)).unwrap();
    let mut duplicate = load_select_items_in_folder(
        &library_db,
        &score_db,
        "/pack-b/song",
        LnPolicySetting::AutoLn,
    )
    .unwrap();
    cache.apply(&library_db, &collection_db, &mut duplicate).unwrap();
    assert_eq!(flags(&duplicate), (true, true));
    collection_db.toggle_favorite_chart(shared.identity.file_sha256, &hints, 2).unwrap();
    cache.apply(&library_db, &collection_db, &mut items).unwrap();
    assert_eq!(flags(&items), (false, true));
    cache.invalidate();
    cache.apply(&library_db, &open_in_memory_collection_db(), &mut items).unwrap();
    assert_eq!(flags(&items), (false, false));
}

#[test]
fn collection_cache_observes_other_connection_commits() {
    let (library_db, _) = open_in_memory_dbs();
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir =
        std::env::temp_dir().join(format!("bmz-collection-cache-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("collection.db");
    let mut connection = Connection::open(&path).unwrap();
    run_migrations(&mut connection, COLLECTION_MIGRATIONS).unwrap();
    drop(connection);
    let collection = CollectionDatabase::open(&path).unwrap();
    let mut writer = CollectionDatabase::open(&path).unwrap();
    let mut cache = SelectCollectionCache::default();
    let mut row = missing_favorite_chart_item_for_test();
    cache.apply(&library_db, &collection, std::slice::from_mut(&mut row)).unwrap();
    writer
        .toggle_favorite_chart(
            [7; 32],
            &crate::storage::collection_db::FavoriteHints::new("Favorite", "", ""),
            1,
        )
        .unwrap();
    cache.apply(&library_db, &collection, std::slice::from_mut(&mut row)).unwrap();
    assert!(matches!(&row, SelectItem::Chart(row) if row.favorite_chart));
    drop(writer);
    drop(collection);
    std::fs::remove_dir_all(dir).unwrap();
}

fn missing_favorite_chart_item_for_test() -> SelectItem {
    SelectItem::Chart(SelectChartRow {
        chart: None,
        chart_analysis: None,
        has_document: false,
        fallback_title: "Favorite".into(),
        fallback_artist: String::new(),
        entry_sha256: Some([7; 32]),
        download_metadata: Default::default(),
        best_score: None,
        replay_slots: [false; 4],
        favorite_chart: false,
        favorite_song: false,
        table_level: String::new(),
        table_text: Default::default(),
    })
}

#[test]
fn favorite_song_resolves_all_duplicate_sha256_folders() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let mut collection_db = open_in_memory_collection_db();
    let shared = chart("Shared");
    library_db.upsert_chart_import(&record_for_chart("/pack-a/song/shared.bms", &shared)).unwrap();
    library_db.upsert_chart_import(&record_for_chart("/pack-b/song/shared.bms", &shared)).unwrap();
    collection_db
        .upsert_favorite_song(
            shared.identity.file_sha256,
            &crate::storage::collection_db::FavoriteHints::new("Shared", "artist", "/pack-a/song"),
            10,
        )
        .unwrap();

    assert_eq!(
        favorite_song_representatives_for_folder(&library_db, &collection_db, "/pack-a/song")
            .unwrap(),
        vec![shared.identity.file_sha256]
    );
    assert_eq!(
        favorite_song_representatives_for_folder(&library_db, &collection_db, "/pack-b/song")
            .unwrap(),
        vec![shared.identity.file_sha256]
    );

    let items = load_select_items_for_favorite_song(
        &library_db,
        &score_db,
        &collection_db,
        shared.identity.file_sha256,
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
        &[],
        None,
        None,
    )
    .unwrap();
    let folders: HashSet<String> = items
        .iter()
        .filter_map(|item| match item {
            SelectItem::Chart(row) => row.chart.as_ref().map(|chart| chart.folder_path.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(folders.len(), 2);
    assert!(folders.contains("/pack-a/song"));
    assert!(folders.contains("/pack-b/song"));
    assert!(items.iter().all(|item| match item {
        SelectItem::Chart(row) => row.favorite_song,
        _ => true,
    }));
}

#[test]
fn load_select_items_attaches_table_level_via_md5() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let alpha = chart("Alpha");
    library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();

    let table = difficulty_table_for_md5(&alpha.identity.file_md5, "★", "3");
    library_db.upsert_difficulty_table(&table).unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
            .unwrap();

    let row = items
        .iter()
        .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
        .unwrap();
    assert_eq!(row.table_level, "★3");
    assert_eq!(row.table_text.table_name, "Table");
    assert_eq!(row.table_text.table_level, "★3");
    assert_eq!(row.table_text.table_full, "★3Table");
}

#[test]
fn load_select_items_joins_multiple_table_levels_with_slash() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let alpha = chart("Alpha");
    library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();

    library_db
        .upsert_difficulty_table(&difficulty_table_for_md5(&alpha.identity.file_md5, "★", "3"))
        .unwrap();
    library_db
        .upsert_difficulty_table(&difficulty_table_for_md5(&alpha.identity.file_md5, "☆", "5"))
        .unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
            .unwrap();

    let row = items
        .iter()
        .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
        .unwrap();
    assert!(row.table_level.contains("★3"), "got: {}", row.table_level);
    assert!(row.table_level.contains("☆5"), "got: {}", row.table_level);
    assert!(row.table_level.contains('/'), "got: {}", row.table_level);
}

#[test]
fn load_select_items_falls_back_to_sha256_when_no_md5_match() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let alpha = chart("Alpha");
    library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();

    let table = difficulty_table_for_sha256(&alpha.identity.file_sha256, "◆", "7");
    library_db.upsert_difficulty_table(&table).unwrap();

    let items =
        load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
            .unwrap();

    let row = items
        .iter()
        .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
        .unwrap();
    assert_eq!(row.table_level, "◆7");
}

#[test]
fn table_folder_items_returns_one_folder_per_table() {
    let (mut library_db, _) = open_in_memory_dbs();
    let alpha = chart("Alpha");
    // Register table using md5 so there's at least one entry (content does not matter here)
    let table = difficulty_table_for_md5(&alpha.identity.file_md5, "★", "1");
    library_db.upsert_difficulty_table(&table).unwrap();

    let items = table_folder_items(&library_db, &[]).unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        SelectItem::Folder { path, name, kind, .. }
        if path.starts_with(TABLE_ROOT_PATH) && name == "Table" && *kind == SelectRowKind::TableFolder
    ));
}

#[test]
fn table_folder_items_follow_config_source_order() {
    let (mut library_db, _) = open_in_memory_dbs();
    let chart = chart("Table Song");
    let table_a = difficulty_table_for_md5(&chart.identity.file_md5, "A", "1");
    let table_b = difficulty_table_for_md5(&chart.identity.file_md5, "B", "1");
    library_db.upsert_difficulty_table(&table_a).unwrap();
    library_db.upsert_difficulty_table(&table_b).unwrap();

    let items = table_folder_items(
        &library_db,
        &["https://example.com/B/".to_string(), "https://example.com/A/".to_string()],
    )
    .unwrap();

    let folders: Vec<_> = items
        .iter()
        .filter_map(|item| {
            if let SelectItem::Folder { path, name, .. } = item {
                Some((path.as_str(), name.as_str()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        folders,
        vec![
            ("bmz-table:https://example.com/B/", "Table"),
            ("bmz-table:https://example.com/A/", "Table"),
        ]
    );
}

#[test]
fn table_folder_items_with_active_sources_hides_removed_tables() {
    let (mut library_db, _) = open_in_memory_dbs();
    let chart = chart("Table Song");
    let table_a = difficulty_table_for_md5(&chart.identity.file_md5, "A", "1");
    let table_b = difficulty_table_for_md5(&chart.identity.file_md5, "B", "1");
    library_db.upsert_difficulty_table(&table_a).unwrap();
    library_db.upsert_difficulty_table(&table_b).unwrap();

    let active_sources = vec!["https://example.com/B/".to_string()];
    let items =
        table_folder_items_for_active_sources(&library_db, &active_sources, Some(&active_sources))
            .unwrap();

    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        SelectItem::Folder { path, .. } if path == "bmz-table:https://example.com/B/"
    ));
}

#[test]
fn chart_enrichment_with_filters_hides_removed_table_levels() {
    let (mut library_db, score_db) = open_in_memory_dbs();
    let chart = chart("Table Song");
    library_db.upsert_chart_import(&record_for_chart("/songs/table.bms", &chart)).unwrap();
    library_db
        .upsert_difficulty_table(&difficulty_table_for_md5(&chart.identity.file_md5, "A", "1"))
        .unwrap();
    library_db
        .upsert_difficulty_table(&difficulty_table_for_md5(&chart.identity.file_md5, "B", "2"))
        .unwrap();

    let active_roots = vec!["/songs".to_string()];
    let active_sources = vec!["https://example.com/B/".to_string()];
    let items = load_select_items_in_folder_for_rule_mode_with_filters(
        &library_db,
        &score_db,
        "/songs",
        LnPolicySetting::AutoLn,
        RuleMode::Beatoraja,
        &active_sources,
        Some(&active_roots),
        Some(&active_sources),
    )
    .unwrap();

    let row = items
        .iter()
        .find_map(|item| if let SelectItem::Chart(row) = item { Some(row) } else { None })
        .unwrap();
    assert_eq!(row.table_level, "B2");
    assert_eq!(row.table_text.table_level, "B2");
}
