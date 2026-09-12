use super::*;

#[test]
fn chart_speed_changes_emits_resume_after_stop() {
    use bmz_chart::model::TimingEventKind;
    let mut c = chart("stop_test");
    // BPM=128 で始まり、2 秒目に 0.5 秒の STOP
    c.timing_events.push(timing_event(2_000_000, TimingEventKind::Stop { duration_us: 500_000 }));
    let changes = chart_speed_changes(&c);
    // 期待: [{128, 0}, {0, 2000}, {128, 2500}, {128, 10000}]
    // STOP 直後の resume (speed=128 at 2500ms) が必須
    let resume = changes.iter().find(|c| c.time_ms == 2_500 && c.speed == 128.0);
    assert!(resume.is_some(), "resume entry after stop must exist: {changes:?}");
    // 末尾エントリが speed=0 になってはいけない
    assert_ne!(
        changes.last().unwrap().speed,
        0.0,
        "last entry must not be stop speed: {changes:?}"
    );
}

#[test]
fn chart_speed_changes_resume_bpm_reflects_change_during_stop() {
    use bmz_chart::model::TimingEventKind;
    let mut c = chart("stop_bpm_change");
    // 1 秒目: STOP 2 秒間 (終了 3 秒)
    c.timing_events.push(timing_event(1_000_000, TimingEventKind::Stop { duration_us: 2_000_000 }));
    // 2 秒目 (STOP 中): BPM 200 に変化
    c.timing_events.push(timing_event(2_000_000, TimingEventKind::BpmChange { bpm: 200.0 }));
    let changes = chart_speed_changes(&c);
    // resume は STOP 終了 (3 秒) に BPM=200 で出るはず
    let resume = changes.iter().find(|c| c.time_ms == 3_000);
    assert!(resume.is_some_and(|r| r.speed == 200.0), "resume must use post-stop BPM: {changes:?}");
}

#[test]
fn upsert_chart_import_persists_file_chart_and_link() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let mut chart = chart("song");
    chart.metadata.has_bga = true;
    let record = ChartImportRecord {
        root_id: None,
        file_path: Path::new("/songs/song.bms"),
        file_size: 123,
        modified_at: 1_700_000_001,
        scanned_at: 1_700_000_002,
        chart: &chart,
    };

    let chart_id = db.upsert_chart_import(&record).unwrap();

    assert_eq!(db.chart_id_by_sha256(chart.identity.file_sha256).unwrap(), Some(chart_id));
    let (path, parse_status, title, mode, ln_type, has_bga): (
            String,
            String,
            String,
            String,
            String,
            bool,
        ) =
            db.conn()
                .query_row(
                    "SELECT chart_files.path, chart_files.parse_status, charts.title, charts.mode, charts.ln_type, charts.has_bga
                    FROM chart_file_links
                    JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
                    JOIN charts ON charts.id = chart_file_links.chart_id",
                    [],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .unwrap();

    assert_eq!(path, "/songs/song.bms");
    assert_eq!(parse_status, "Parsed");
    assert_eq!(title, "song");
    assert_eq!(mode, "7K");
    assert_eq!(ln_type, "");
    assert!(has_bga);

    let analysis = db.chart_analysis_by_chart_id(chart_id).unwrap().unwrap();
    assert_eq!(analysis.total_gauge, 260.0);
    assert_eq!(analysis.main_bpm, 128.0);
}

#[test]
fn upsert_chart_import_preserves_first_seen_time_across_rescan() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let chart = chart("first seen");
    let mut first = record_for_chart("/songs/first-seen.bms", &chart);
    first.scanned_at = 100;
    let chart_id = db.upsert_chart_import(&first).unwrap();

    let mut rescan = record_for_chart("/songs/first-seen.bms", &chart);
    rescan.scanned_at = 200;
    db.upsert_chart_import(&rescan).unwrap();

    let first_seen = db.chart_first_seen_at_by_chart_ids(&[chart_id]).unwrap();
    assert_eq!(first_seen.get(&chart_id), Some(&100));
    let scanned_at: i64 =
        db.conn().query_row("SELECT scanned_at FROM chart_files", [], |row| row.get(0)).unwrap();
    assert_eq!(scanned_at, 200);
}

#[test]
fn course_by_id_loads_only_the_requested_definition_with_entries() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let first = course_with_entries(vec![CourseEntry {
        title_hint: "First stage".to_string(),
        md5: None,
        sha256: None,
        chart_id: None,
    }]);
    let mut second = course_with_entries(vec![CourseEntry {
        title_hint: "Second stage".to_string(),
        md5: None,
        sha256: None,
        chart_id: None,
    }]);
    second.key = "table:test#1".to_string();
    second.title = "Other Course".to_string();
    let first_id = db.upsert_course("table:test", &first, 0, 1).unwrap();
    let second_id = db.upsert_course("table:test", &second, 1, 1).unwrap();

    let loaded = db.course_by_id(first_id).unwrap().unwrap();

    assert_eq!(loaded.id, first_id);
    assert_ne!(loaded.id, second_id);
    assert_eq!(loaded.definition.title, "Test Course");
    assert_eq!(loaded.definition.entries.len(), 1);
    assert_eq!(loaded.definition.entries[0].title_hint, "First stage");
    assert!(db.course_by_id(i64::MAX).unwrap().is_none());
}

#[test]
fn upsert_chart_import_backfills_unresolved_course_entries() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let chart = chart("course song");
    let md5 = hash_to_hex(&chart.identity.file_md5);
    let sha256 = hash_to_hex(&chart.identity.file_sha256);
    let course = course_with_entries(vec![
        CourseEntry {
            title_hint: "SHA-256 match".to_string(),
            md5: Some(md5.clone()),
            sha256: Some(sha256),
            chart_id: None,
        },
        CourseEntry {
            title_hint: "MD5 fallback".to_string(),
            md5: Some(md5),
            sha256: Some("f".repeat(64)),
            chart_id: None,
        },
    ]);
    let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
    assert!(
        db.list_course_entries(course_id)
            .unwrap()
            .iter()
            .all(|entry| entry.entry.chart_id.is_none())
    );

    let chart_id = db.upsert_chart_import(&record_for_chart("/songs/course.bms", &chart)).unwrap();

    let entries = db.list_course_entries(course_id).unwrap();
    assert_eq!(entries[0].entry.chart_id, Some(chart_id));
    assert_eq!(entries[1].entry.chart_id, Some(chart_id));
}

#[test]
fn successful_reimport_restores_course_link_after_import_failure() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let chart = chart("recovered course song");
    let path = Path::new("/songs/recovered.bms");
    let record = record_for_chart("/songs/recovered.bms", &chart);
    let original_chart_id = db.upsert_chart_import(&record).unwrap();
    let course = course_with_entries(vec![CourseEntry {
        title_hint: "Recovered song".to_string(),
        md5: Some(hash_to_hex(&chart.identity.file_md5)),
        sha256: Some(hash_to_hex(&chart.identity.file_sha256)),
        chart_id: None,
    }]);
    let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
    assert_eq!(
        db.list_course_entries(course_id).unwrap()[0].entry.chart_id,
        Some(original_chart_id)
    );

    db.upsert_failed_chart_file(None, path, 1, 2, 2, "temporary failure").unwrap();
    assert_eq!(db.list_course_entries(course_id).unwrap()[0].entry.chart_id, None);

    let recovered_chart_id = db.upsert_chart_import(&record).unwrap();
    assert_eq!(
        db.list_course_entries(course_id).unwrap()[0].entry.chart_id,
        Some(recovered_chart_id)
    );
}

#[test]
fn course_backfill_uses_latest_duplicate_chart_id() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let chart = chart("duplicate course song");
    let oldest_chart_id =
        db.upsert_chart_import(&record_for_chart("/songs/first.bms", &chart)).unwrap();
    let course = course_with_entries(vec![CourseEntry {
        title_hint: "Duplicate song".to_string(),
        md5: Some(hash_to_hex(&chart.identity.file_md5)),
        sha256: Some(hash_to_hex(&chart.identity.file_sha256)),
        chart_id: None,
    }]);
    let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
    db.conn()
        .execute(
            "UPDATE course_entries SET chart_id = NULL WHERE course_id = ?1",
            params![course_id],
        )
        .unwrap();

    let duplicate_chart_id =
        db.upsert_chart_import(&record_for_chart("/songs/second.bms", &chart)).unwrap();

    assert!(duplicate_chart_id > oldest_chart_id);
    assert_eq!(
        db.list_course_entries(course_id).unwrap()[0].entry.chart_id,
        Some(duplicate_chart_id)
    );
}

#[test]
fn moved_chart_import_repairs_existing_course_path() {
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root =
        std::env::temp_dir().join(format!("bmz-course-moved-chart-{}-{stamp}", std::process::id()));
    let old_path = root.join("before/song.bms");
    let new_path = root.join("after/song.bms");
    std::fs::create_dir_all(old_path.parent().unwrap()).unwrap();
    std::fs::write(&old_path, b"#TITLE Moved Course Song\n#BPM 120\n").unwrap();

    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let chart = chart("moved course song");
    let old_path_text = old_path.to_string_lossy().into_owned();
    let old_chart_id = db.upsert_chart_import(&record_for_chart(&old_path_text, &chart)).unwrap();
    let course = course_with_entries(vec![CourseEntry {
        title_hint: "Moved course song".to_string(),
        md5: Some(hash_to_hex(&chart.identity.file_md5)),
        sha256: Some(hash_to_hex(&chart.identity.file_sha256)),
        chart_id: None,
    }]);
    let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
    assert_eq!(db.list_course_entries(course_id).unwrap()[0].entry.chart_id, Some(old_chart_id));

    std::fs::create_dir_all(new_path.parent().unwrap()).unwrap();
    std::fs::rename(&old_path, &new_path).unwrap();
    let new_path_text = new_path.to_string_lossy().into_owned();
    let new_chart_id = db.upsert_chart_import(&record_for_chart(&new_path_text, &chart)).unwrap();

    assert_ne!(new_chart_id, old_chart_id);
    assert_eq!(db.list_course_entries(course_id).unwrap()[0].entry.chart_id, Some(new_chart_id));
    assert_eq!(
        db.primary_chart_file_path(new_chart_id).unwrap(),
        Some(library_path_key(&new_path))
    );

    db.conn()
        .execute(
            "UPDATE course_entries SET chart_id = ?1 WHERE course_id = ?2",
            params![old_chart_id, course_id],
        )
        .unwrap();
    crate::storage::course_db::repair_course_entry_chart_links_for_course(db.conn(), course_id)
        .unwrap();
    assert_eq!(db.list_course_entries(course_id).unwrap()[0].entry.chart_id, Some(new_chart_id));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn upsert_chart_import_persists_bms_total_separately_from_gauge_total() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let mut chart = chart("bms total");
    chart.metadata.total = Some(320.0);
    chart.total_notes = 500;
    let record = ChartImportRecord {
        root_id: None,
        file_path: Path::new("/songs/total.bms"),
        file_size: 123,
        modified_at: 1_700_000_001,
        scanned_at: 1_700_000_002,
        chart: &chart,
    };
    let chart_id = db.upsert_chart_import(&record).unwrap();

    let stored: f64 = db
        .conn
        .query_row("SELECT bms_total FROM charts WHERE id = ?1", params![chart_id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(stored, 320.0);

    let listed = db.list_charts_by_ids(&[chart_id]).unwrap().pop().unwrap();
    assert_eq!(listed.bms_total, 320.0);
}

#[test]
fn upsert_chart_import_persists_ln_profile_and_pair_counts() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let mut chart = chart("defined cn");
    chart.metadata.long_note_mode = LongNoteMode::Cn;
    chart.metadata.long_note_mode_defined = true;
    for (index, mode) in
        [None, Some(LongNoteMode::Ln), Some(LongNoteMode::Cn), Some(LongNoteMode::Hcn)]
            .into_iter()
            .enumerate()
    {
        chart.long_notes.push(LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::ChannelPair,
            mode,
            start_note_id: NoteId((index * 2 + 1) as u32),
            end_note_id: NoteId((index * 2 + 2) as u32),
            start_tick: ChartTick(0),
            end_tick: ChartTick(192),
            start_time: TimeUs(1_000_000),
            end_time: TimeUs(2_000_000),
            sound: None,
        });
    }

    let chart_id =
        db.upsert_chart_import(&record_for_chart("/songs/defined-cn.bms", &chart)).unwrap();
    let row = db.list_charts_by_ids(&[chart_id]).unwrap().pop().unwrap();

    assert!(row.ln_profile.has_undefined_ln);
    assert!(row.ln_profile.has_defined_ln);
    assert!(row.ln_profile.has_defined_cn);
    assert!(row.ln_profile.has_defined_hcn);
    assert_eq!(
        row.ln_counts,
        ChartLnCounts {
            undefined_ln_pairs: 1,
            defined_ln_pairs: 1,
            defined_cn_pairs: 1,
            defined_hcn_pairs: 1,
        }
    );
    assert_eq!(row.scored_total_notes(LnScorePolicy::ForceCn), 4);

    chart.long_notes.truncate(1);
    let updated_id =
        db.upsert_chart_import(&record_for_chart("/songs/defined-cn.bms", &chart)).unwrap();
    assert_eq!(updated_id, chart_id);
    let updated = db.list_charts_by_ids(&[chart_id]).unwrap().pop().unwrap();
    assert_eq!(updated.ln_counts.undefined_ln_pairs, 1);
    assert_eq!(updated.ln_counts.defined_ln_pairs, 0);
    assert_eq!(updated.ln_counts.defined_cn_pairs, 0);
    assert_eq!(updated.ln_counts.defined_hcn_pairs, 0);
}

#[test]
fn upsert_chart_import_persists_source_url_without_raw_headers() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let mut chart = chart("url song");
    chart.metadata.source_url = "http://example.com/bms".to_string();
    chart.metadata.append_url = "http://example.com/append".to_string();
    chart.metadata.bms_headers.insert("TITLE".to_string(), "url song".to_string());
    chart.metadata.bms_headers.insert("URL".to_string(), "http://example.com/bms".to_string());
    let record = ChartImportRecord {
        root_id: None,
        file_path: Path::new("/songs/url.bms"),
        file_size: 123,
        modified_at: 1_700_000_001,
        scanned_at: 1_700_000_002,
        chart: &chart,
    };

    let chart_id = db.upsert_chart_import(&record).unwrap();
    let (source_url, append_url, headers_json): (String, String, String) = db
        .conn()
        .query_row(
            "SELECT source_url, append_url, headers_json FROM charts WHERE id = ?1",
            params![chart_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(source_url, "http://example.com/bms");
    assert_eq!(append_url, "http://example.com/append");
    assert_eq!(headers_json, "{}");
}

#[test]
fn upsert_chart_import_persists_chart_analysis_distribution() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let mut chart = chart("analysis");
    chart.total_notes = 3;
    chart.end_time = TimeUs(3_000_000);
    chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, NoteKind::Tap, 100_000));
    chart.lane_notes[Lane::Scratch.index()].push(note(
        2,
        Lane::Scratch,
        NoteKind::LongStart,
        1_000_000,
    ));
    chart.lane_notes[Lane::Scratch.index()].push(note(
        3,
        Lane::Scratch,
        NoteKind::LongEnd,
        2_000_000,
    ));
    chart.lane_notes[Lane::Key2.index()].push(note(4, Lane::Key2, NoteKind::Mine, 1_500_000));
    chart.long_notes.push(LongNotePair {
        lane: Lane::Scratch,
        style: LongNoteStyle::ChannelPair,
        mode: None,
        start_note_id: NoteId(2),
        end_note_id: NoteId(3),
        start_tick: ChartTick(0),
        end_tick: ChartTick(192),
        start_time: TimeUs(1_000_000),
        end_time: TimeUs(2_000_000),
        sound: None,
    });

    let chart_id =
        db.upsert_chart_import(&record_for_chart("/songs/analysis.bms", &chart)).unwrap();
    let analysis = db.chart_analysis_by_chart_id(chart_id).unwrap().unwrap();

    assert_eq!(analysis.normal_notes, 1);
    assert_eq!(analysis.long_notes, 1);
    assert_eq!(analysis.scratch_notes, 0);
    assert_eq!(analysis.long_scratch_notes, 1);
    assert_eq!(analysis.distribution[0].key_taps, 1);
    assert_eq!(analysis.distribution[1].scratch_long_heads, 1);
    assert_eq!(analysis.distribution[1].scratch_long_bodies, 0);
    assert_eq!(analysis.distribution[1].mines, 1);
    assert_eq!(analysis.distribution[2].scratch_long_bodies, 1);
    assert_eq!(analysis.lane_notes[Lane::Scratch.index()].long_notes, 1);
    assert_eq!(analysis.lane_notes[Lane::Key2.index()].mines, 1);
    assert_eq!(analysis.peak_density, 1.0);

    let stored_distribution: String = db
        .conn()
        .query_row(
            "SELECT distribution_json FROM chart_analysis WHERE chart_id = ?1",
            params![chart_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(stored_distribution.starts_with('#'));
    assert_eq!(stored_distribution.len(), 1 + analysis.distribution.len() * 14);
}

#[test]
fn chart_normalization_analysis_roundtrips_and_rescan_clears_it() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let chart = chart("normalization");

    let chart_id =
        db.upsert_chart_import(&record_for_chart("/songs/normalization.bms", &chart)).unwrap();
    assert!(db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().is_none());

    db.write_chart_normalization_analysis(
        chart_id,
        ChartNormalizationAnalysis {
            loudness_lufs: -10.5,
            short_term_lufs: -8.0,
            sample_peak: 0.75,
        },
    )
    .unwrap();
    let stored = db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().unwrap();
    assert_eq!(stored.loudness_lufs, -10.5);
    assert_eq!(stored.short_term_lufs, -8.0);
    assert_eq!(stored.sample_peak, 0.75);

    db.conn().execute(
        "UPDATE chart_analysis SET loudness_analysis_version = 3, sample_peak = 6405.997 WHERE chart_id = ?1",
        params![chart_id],
    ).unwrap();
    assert!(db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().is_none());
    db.write_chart_normalization_analysis(chart_id, stored).unwrap();
    assert_eq!(
        db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().unwrap().sample_peak,
        0.75
    );

    db.upsert_chart_import(&record_for_chart("/songs/normalization.bms", &chart)).unwrap();
    assert!(db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().is_none());
}

#[test]
fn chart_analysis_counts_defined_cn_long_end_independently_of_chart_default() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let mut chart = chart("mixed analysis");
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.total_notes = 1;
    chart.end_time = TimeUs(2_000_000);
    chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, NoteKind::LongStart, 1_000_000));
    chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, NoteKind::LongEnd, 2_000_000));
    chart.long_notes.push(LongNotePair {
        lane: Lane::Key1,
        style: LongNoteStyle::ChannelPair,
        mode: Some(LongNoteMode::Cn),
        start_note_id: NoteId(1),
        end_note_id: NoteId(2),
        start_tick: ChartTick(0),
        end_tick: ChartTick(192),
        start_time: TimeUs(1_000_000),
        end_time: TimeUs(2_000_000),
        sound: None,
    });

    let chart_id =
        db.upsert_chart_import(&record_for_chart("/songs/mixed-analysis.bms", &chart)).unwrap();
    let analysis = db.chart_analysis_by_chart_id(chart_id).unwrap().unwrap();

    assert_eq!(analysis.long_notes, 2);
    assert_eq!(analysis.lane_notes[Lane::Key1.index()].long_notes, 2);
    assert_eq!(analysis.distribution[2].key_long_heads, 1);
    assert_eq!(analysis.total_gauge, gauge_total_for_chart(None, 2));
}

#[test]
fn chart_analysis_caps_extreme_distribution_length() {
    let mut chart = chart("long analysis");
    chart.end_time = TimeUs(i64::MAX);
    chart.long_notes.push(LongNotePair {
        lane: Lane::Key1,
        style: LongNoteStyle::ChannelPair,
        mode: None,
        start_note_id: NoteId(1),
        end_note_id: NoteId(2),
        start_tick: ChartTick(0),
        end_tick: ChartTick(0),
        start_time: TimeUs(0),
        end_time: TimeUs(i64::MAX),
        sound: None,
    });

    let analysis = ChartAnalysis::from_chart(&chart);

    assert_eq!(analysis.distribution.len(), MAX_ANALYSIS_DISTRIBUTION_SECONDS);
}

#[test]
fn chart_analysis_trims_distribution_to_last_note_second() {
    let mut chart = chart("trim analysis");
    chart.end_time = TimeUs(i64::MAX);
    chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, NoteKind::Tap, 2_000_000));

    let analysis = ChartAnalysis::from_chart(&chart);

    assert_eq!(analysis.distribution.len(), 3);
    assert_eq!(analysis.distribution[2].key_taps, 1);
}

#[test]
fn chart_analysis_excludes_invisible_notes_from_density() {
    let mut chart = chart("invisible analysis");
    chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, NoteKind::Tap, 0));
    chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, NoteKind::Invisible, 0));
    chart.total_notes = 1;

    let analysis = ChartAnalysis::from_chart(&chart);

    assert_eq!(analysis.normal_notes, 1);
    assert_eq!(analysis.distribution[0].key_taps, 1);
}

#[test]
fn compact_distribution_round_trips_and_accepts_legacy_json() {
    let distribution = vec![
        ChartDistributionSecond {
            scratch_long_heads: 1,
            scratch_long_bodies: 2,
            scratch_taps: 3,
            key_long_heads: 4,
            key_long_bodies: 5,
            key_taps: 6,
            mines: 7,
        },
        ChartDistributionSecond { key_taps: 36 * 36, ..Default::default() },
    ];

    let compact = encode_distribution_compact(&distribution);
    assert_eq!(compact.len(), 1 + distribution.len() * 14);
    let decoded = decode_distribution(&compact);
    assert_eq!(decoded[0], distribution[0]);
    assert_eq!(decoded[1].key_taps, 36 * 36 - 1);

    let legacy_json = serde_json::to_string(&distribution).unwrap();
    assert_eq!(decode_distribution(&legacy_json), distribution);
}
