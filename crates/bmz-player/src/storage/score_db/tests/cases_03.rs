use super::*;

#[test]
fn history_ir_identity_survives_reopen_and_branches_after_profile_copy() {
    let temp_root =
        std::env::temp_dir().join(crate::ir::payload::new_score_idempotency_key().unwrap());
    std::fs::create_dir_all(&temp_root).unwrap();
    let original_path = temp_root.join("original.db");
    let separate_path = temp_root.join("separate.db");
    let fork_a_path = temp_root.join("fork-a.db");
    let fork_b_path = temp_root.join("fork-b.db");

    crate::storage::migration::migrate_score_db(&original_path).unwrap();
    let mut original = ScoreDatabase::open(&original_path).unwrap();
    let first_id = original.insert_score(&record(40, ClearType::Hard)).unwrap();
    let first_key = original.ir_submission_idempotency_key_for_history(first_id).unwrap();
    let parsed = uuid::Uuid::parse_str(first_key.strip_prefix("bmz-score-v2-").unwrap()).unwrap();
    assert_eq!(parsed.get_version_num(), 4);
    assert_eq!(parsed.get_variant(), uuid::Variant::RFC4122);
    drop(original);

    let reopened = ScoreDatabase::open(&original_path).unwrap();
    assert_eq!(reopened.ir_submission_idempotency_key_for_history(first_id).unwrap(), first_key);
    drop(reopened);

    std::fs::copy(&original_path, &fork_a_path).unwrap();
    std::fs::copy(&original_path, &fork_b_path).unwrap();

    crate::storage::migration::migrate_score_db(&separate_path).unwrap();
    let mut separate = ScoreDatabase::open(&separate_path).unwrap();
    let separate_first_id = separate.insert_score(&record(40, ClearType::Hard)).unwrap();
    assert_eq!(separate_first_id, first_id);
    let separate_key =
        separate.ir_submission_idempotency_key_for_history(separate_first_id).unwrap();
    assert_ne!(separate_key, first_key);
    drop(separate);

    let mut fork_a = ScoreDatabase::open(&fork_a_path).unwrap();
    let mut fork_b = ScoreDatabase::open(&fork_b_path).unwrap();
    let branch_a_id = fork_a.insert_score(&record(42, ClearType::Easy)).unwrap();
    let branch_b_id = fork_b.insert_score(&record(42, ClearType::Easy)).unwrap();
    assert_eq!(branch_a_id, branch_b_id);
    let branch_a_key = fork_a.ir_submission_idempotency_key_for_history(branch_a_id).unwrap();
    let branch_b_key = fork_b.ir_submission_idempotency_key_for_history(branch_b_id).unwrap();
    assert_ne!(branch_a_key, branch_b_key);
    drop(fork_a);
    drop(fork_b);

    std::fs::remove_dir_all(temp_root).unwrap();
}

#[test]
fn score_migration_assigns_idempotency_keys_to_existing_histories() {
    let temp_root =
        std::env::temp_dir().join(crate::ir::payload::new_score_idempotency_key().unwrap());
    std::fs::create_dir_all(&temp_root).unwrap();
    let path = temp_root.join("legacy.db");

    crate::storage::migration::migrate_score_db(&path).unwrap();
    let mut db = ScoreDatabase::open(&path).unwrap();
    let history_id = db.insert_score(&record(40, ClearType::Hard)).unwrap();
    drop(db);

    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE score_kaleid_plays;
         DROP TABLE score_history_ir_identity;
         DROP INDEX idx_score_history_device_best;
         ALTER TABLE score_history DROP COLUMN normal_best_eligible;
         ALTER TABLE score_history DROP COLUMN source_min_bp;
         PRAGMA user_version = 31;",
    )
    .unwrap();
    drop(conn);
    crate::storage::migration::migrate_score_db(&path).unwrap();

    let db = ScoreDatabase::open(&path).unwrap();
    let key = db.ir_submission_idempotency_key_for_history(history_id).unwrap();
    let parsed = uuid::Uuid::parse_str(key.strip_prefix("bmz-score-v2-").unwrap()).unwrap();
    assert_eq!(parsed.get_version_num(), 4);
    assert_eq!(parsed.get_variant(), uuid::Variant::RFC4122);
    drop(db);
    std::fs::remove_dir_all(temp_root).unwrap();
}

#[test]
fn select_score_lookups_batch_more_keys_than_one_sqlite_variable_chunk() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut first = record(20, ClearType::Normal);
    first.chart_sha256 = [1; 32];
    let mut second = record(10, ClearType::Easy);
    second.chart_sha256 = [2; 32];
    db.insert_score(&first).unwrap();
    db.insert_score(&second).unwrap();
    db.upsert_replay_slot(&sample_slot(0, 20)).unwrap();
    db.upsert_replay_slot(&sample_slot(2, 20)).unwrap();
    let mut second_slot = sample_slot(1, 10);
    second_slot.chart_sha256 = [2; 32];
    db.upsert_replay_slot(&second_slot).unwrap();

    let mut keys = (0..SCORE_KEY_LOOKUP_BATCH_SIZE * 2 + 1)
        .map(|index| {
            let mut sha = [0; 32];
            sha[..8].copy_from_slice(&(index as u64 + 100).to_le_bytes());
            key(sha)
        })
        .collect::<Vec<_>>();
    keys.extend([key([2; 32]), key([1; 32]), key([1; 32])]);

    let scores = db.best_scores_for_charts(&keys).unwrap();
    let slots = db.replay_slots_for_charts(&keys).unwrap();

    assert_eq!(
        scores.iter().map(|score| score.chart_sha256).collect::<Vec<_>>(),
        [[2; 32], [1; 32], [1; 32]]
    );
    assert_eq!(slots.len(), 3);
    assert_eq!(slots[0].chart_sha256, [2; 32]);
    assert_eq!(slots[0].replay_slots, [false, true, false, false]);
    assert_eq!(slots[1].chart_sha256, [1; 32]);
    assert_eq!(slots[1].replay_slots, [true, false, true, false]);
    assert_eq!(slots[2], slots[1]);
}

#[test]
fn upsert_replay_slot_overwrites_same_slot() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
    let mut updated = sample_slot(0, 99);
    updated.replay_path = "replay/updated.toml".to_string();
    db.upsert_replay_slot(&updated).unwrap();

    let record = db.replay_slot(key([1; 32]), 0).unwrap().unwrap();
    assert_eq!(record.ex_score, Some(99));
    assert_eq!(record.replay_path, "replay/updated.toml");
}

#[test]
fn replay_slots_for_chart_returns_all_four_slots() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
    db.upsert_replay_slot(&sample_slot(3, 30)).unwrap();

    let slots = db.replay_slots_for_chart(key([1; 32])).unwrap();

    assert!(slots[0].is_some());
    assert!(slots[1].is_none());
    assert!(slots[2].is_none());
    assert_eq!(slots[3].as_ref().unwrap().ex_score, Some(30));
}

#[test]
fn replay_slots_are_separate_per_ln_policy() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    let mut ln = sample_slot(0, 10);
    ln.ln_policy = LnScorePolicy::ForceLn;
    let mut cn = sample_slot(0, 99);
    cn.ln_policy = LnScorePolicy::ForceCn;

    db.upsert_replay_slot(&ln).unwrap();
    db.upsert_replay_slot(&cn).unwrap();

    let ln_slot = db.replay_slot(key([1; 32]), 0).unwrap().unwrap();
    let cn_slot =
        db.replay_slot(ScoreKey::new([1; 32], LnScorePolicy::ForceCn), 0).unwrap().unwrap();
    assert_eq!(ln_slot.ex_score, Some(10));
    assert_eq!(cn_slot.ex_score, Some(99));
}

#[test]
fn replay_slots_are_separate_per_rule_mode() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    let mut beatoraja = sample_slot(0, 10);
    beatoraja.rule_mode = RuleMode::Beatoraja;
    let mut dx = sample_slot(0, 99);
    dx.rule_mode = RuleMode::Dx;

    db.upsert_replay_slot(&beatoraja).unwrap();
    db.upsert_replay_slot(&dx).unwrap();

    let beatoraja_slot =
        db.replay_slot(key([1; 32]).with_rule_mode(RuleMode::Beatoraja), 0).unwrap().unwrap();
    let dx_slot = db.replay_slot(key([1; 32]).with_rule_mode(RuleMode::Dx), 0).unwrap().unwrap();
    assert_eq!(beatoraja_slot.ex_score, Some(10));
    assert_eq!(dx_slot.ex_score, Some(99));
}

#[test]
fn score_record_can_be_built_from_play_result() {
    let result = PlayResult {
        chart_sha256: [9; 32],
        clear_type: ClearType::Normal,
        gauge_type: GaugeType::Hard,
        gauge_value: 76.5,
        total_notes: 1,
        score: score_with_ex_score(2),
        autoplay: true,
    };

    let record = ScoreRecord::from_play_result(
        &result,
        ScoreRecordMetadata::new(
            LnScorePolicy::ForceCn,
            DoubleOptionScoreBucket::Battle,
            1_700_000_040,
            Some(123),
            "Normal",
            "Hard",
            "Lr2Oraja",
            0,
            InputDeviceKind::Controller,
            "",
        )
        .with_arrange_2p("Mirror")
        .with_source_kind(ScoreSourceKind::Lr2Oraja),
    );

    assert_eq!(record.chart_sha256, [9; 32]);
    assert_eq!(record.ln_policy, LnScorePolicy::ForceCn);
    assert_eq!(record.double_option, DoubleOptionScoreBucket::Battle);
    assert_eq!(record.played_at, 1_700_000_040);
    assert_eq!(record.clear_type, ClearType::Normal);
    assert_eq!(record.gauge_type, Some(GaugeType::Hard));
    assert_eq!(record.gauge_value, Some(76.5));
    assert_eq!(record.device_type, InputDeviceKind::Controller);
    assert_eq!(record.arrange_2p, "Mirror");
    assert_eq!(record.source_kind, ScoreSourceKind::Lr2Oraja);
    assert_eq!(record.score.ex_score(), 2);
    assert!(record.autoplay);
    assert_eq!(record.gauge_option, "Hard");
    assert_eq!(record.rule_mode, "Lr2Oraja");
    assert_eq!(record.replay_path, "");
}

#[test]
fn tag_score_history_with_course_updates_only_given_rows() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase::from_connection(conn);

    let mut r1 = record(20, ClearType::Normal);
    r1.chart_sha256 = [1; 32];
    let mut r2 = record(30, ClearType::Easy);
    r2.chart_sha256 = [2; 32];
    let mut r3 = record(10, ClearType::Failed);
    r3.chart_sha256 = [3; 32];
    let id1 = db.insert_score(&r1).unwrap();
    let id2 = db.insert_score(&r2).unwrap();
    let id3 = db.insert_score(&r3).unwrap();

    let course_score_id = insert_test_course_score(&mut db, "course-a");

    // Tag the first two with a real course score, leave r3 untouched.
    let updated = db.tag_score_history_with_course(&[id1, id2], course_score_id).unwrap();
    assert_eq!(updated, 2);

    let rows: Vec<(i64, Option<i64>)> = db
        .conn()
        .prepare("SELECT id, course_score_id FROM score_history ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(rows, vec![(id1, Some(course_score_id)), (id2, Some(course_score_id)), (id3, None)]);
}

#[test]
fn tag_score_history_with_course_no_op_on_empty_list() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase::from_connection(conn);
    assert_eq!(db.tag_score_history_with_course(&[], 1).unwrap(), 0);
}

#[test]
fn recent_history_returns_newest_scores_first() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase::from_connection(conn);
    let mut older = record(20, ClearType::Normal);
    older.played_at = 1;
    older.chart_sha256 = [1; 32];
    let mut newer = record(10, ClearType::Easy);
    newer.played_at = 2;
    newer.chart_sha256 = [2; 32];
    newer.autoplay = true;

    db.insert_score(&older).unwrap();
    db.insert_score(&newer).unwrap();

    let history = db.recent_history(10, 0).unwrap();

    assert_eq!(history.len(), 2);
    assert_eq!(history[0].chart_sha256, [2; 32]);
    assert_eq!(history[0].played_at, 2);
    assert!(history[0].autoplay);
    assert_eq!(history[1].chart_sha256, [1; 32]);
}

#[test]
fn recent_history_exposes_course_score_id_when_tagged() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase::from_connection(conn);

    let mut solo = record(20, ClearType::Normal);
    solo.chart_sha256 = [1; 32];
    let solo_id = db.insert_score(&solo).unwrap();

    let mut course_play = record(30, ClearType::Easy);
    course_play.chart_sha256 = [2; 32];
    let course_play_id = db.insert_score(&course_play).unwrap();

    let course_score_id = insert_test_course_score(&mut db, "course-a");

    // Tag the course-attempt row only.
    db.tag_score_history_with_course(&[course_play_id], course_score_id).unwrap();

    let history = db.recent_history(10, 0).unwrap();
    let by_id: std::collections::HashMap<i64, &ScoreHistoryEntry> =
        history.iter().map(|h| (h.id, h)).collect();
    assert_eq!(by_id.get(&solo_id).unwrap().course_score_id, None);
    assert_eq!(by_id.get(&course_play_id).unwrap().course_score_id, Some(course_score_id));
}

#[test]
fn deleting_course_score_nulls_history_course_link() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase::from_connection(conn);

    let mut course_play = record(30, ClearType::Easy);
    course_play.chart_sha256 = [2; 32];
    let course_play_id = db.insert_score(&course_play).unwrap();
    let course_score_id = insert_test_course_score(&mut db, "course-a");
    db.tag_score_history_with_course(&[course_play_id], course_score_id).unwrap();

    db.conn_mut()
        .execute("DELETE FROM course_scores WHERE id = ?1", params![course_score_id])
        .unwrap();

    let course_score_id_after_delete: Option<i64> = db
        .conn()
        .query_row(
            "SELECT course_score_id FROM score_history WHERE id = ?1",
            params![course_play_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(course_score_id_after_delete, None);
}

#[test]
fn score_db_migrations_do_not_leave_ir_tables() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
                 FROM sqlite_master
                 WHERE type = 'table' AND name LIKE 'ir_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn score_db_migrations_drop_redundant_prefix_indexes() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();

    for index in
        ["idx_score_best_chart", "idx_replay_slots_chart", "idx_score_course_replay_slots_hash"]
    {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                params![index],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "{index} should be covered by a PRIMARY KEY prefix");
    }
}

#[test]
fn chart_sha256_columns_are_lowercase_hex_text() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase::from_connection(conn);
    db.insert_score(&record(20, ClearType::Normal)).unwrap();

    let (hist_typeof, best_typeof, best_hex): (String, String, String) = db
        .conn()
        .query_row(
            "SELECT
                    (SELECT typeof(chart_sha256) FROM score_history LIMIT 1),
                    (SELECT typeof(chart_sha256) FROM score_best LIMIT 1),
                    (SELECT chart_sha256 FROM score_best LIMIT 1)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(hist_typeof, "text");
    assert_eq!(best_typeof, "text");
    assert_eq!(best_hex.len(), 64);
    assert!(best_hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}
