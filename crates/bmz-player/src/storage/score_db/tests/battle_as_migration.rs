use super::*;
use crate::storage::migration::run_score_migrations;

fn battle_as_key() -> ScoreKey {
    ScoreKey::with_options(
        [7; 32],
        LnScorePolicy::ForceLn,
        DoubleOptionScoreBucket::BattleAutoScratch,
        RuleMode::Beatoraja,
    )
}

fn battle_as_record(ex_score: u32, clear_type: ClearType, played_at: i64) -> ScoreRecord {
    ScoreRecord {
        double_option: DoubleOptionScoreBucket::BattleAutoScratch,
        applied_double_option: DoubleOption::BattleAutoScratch,
        played_at,
        ..record(ex_score, clear_type)
    }
}

fn database_before_battle_as_migration() -> ScoreDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    ScoreDatabase { conn }
}

#[test]
fn old_battle_as_scores_stay_in_history_but_leave_bests_lamps_and_slots() {
    let mut db = database_before_battle_as_migration();
    let old = db.insert_score(&battle_as_record(100, ClearType::Hard, 1_700_000_000)).unwrap();
    let normal = db.insert_score(&record(50, ClearType::Normal)).unwrap();
    db.upsert_replay_slot(&ReplaySlotRecord {
        chart_sha256: [7; 32],
        double_option: DoubleOptionScoreBucket::BattleAutoScratch,
        rule: ReplaySlotRule::ScoreUpdate,
        ex_score: Some(100),
        ..sample_slot(0, 100)
    })
    .unwrap();

    run_score_migrations(&mut db.conn).unwrap();

    let eligible = |id: i64| -> i64 {
        db.conn()
            .query_row(
                "SELECT normal_best_eligible FROM score_history WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap()
    };
    assert_eq!(eligible(old), 0, "the old play remains in history");
    assert_eq!(eligible(normal), 1);
    let bests: Vec<String> = db
        .conn()
        .prepare("SELECT double_option FROM score_best ORDER BY double_option")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(bests, ["Off"]);
    let slot = db.replay_slot(battle_as_key(), 0).unwrap().expect("the replay stays playable");
    assert_eq!(slot.replay_path, "replay/0.toml");
    assert_eq!((slot.ex_score, slot.bp, slot.max_combo, slot.clear_rank), (None, None, None, None));
    assert!(db.clear_option_history(battle_as_key()).unwrap().is_empty());

    // A new play starts the bucket again, even below the old inflated score.
    db.insert_score(&battle_as_record(40, ClearType::Easy, 1_800_000_000)).unwrap();
    let best = db.best_scores_for_charts(&[battle_as_key()]).unwrap().remove(0);
    assert_eq!(best.ex_score, 40);
    assert_eq!(best.clear_type, "Easy");
    let play_count: u32 = db
        .conn()
        .query_row(
            "SELECT play_count FROM score_best WHERE double_option = 'BattleAutoScratch'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(play_count, 1);
    assert_eq!(db.clear_option_history(battle_as_key()).unwrap().len(), 1);
}

#[test]
fn clear_only_battle_as_after_migration_does_not_turn_old_history_into_a_bp_best() {
    let mut db = database_before_battle_as_migration();
    db.insert_score(&battle_as_record(100, ClearType::Hard, 1_700_000_000)).unwrap();
    run_score_migrations(&mut db.conn).unwrap();
    // An assisted BATTLE AS clear (H-RANDOM etc.) only records the lamp.
    db.update_score_clear_only(&battle_as_record(0, ClearType::LightAssistEasy, 1_800_000_000))
        .unwrap();

    let mut play = battle_as_record(40, ClearType::Easy, 1_900_000_000);
    play.score = play_with_bp(40, 7);
    db.insert_score(&play).unwrap();

    let best = db.best_scores_for_charts(&[battle_as_key()]).unwrap().remove(0);
    assert_eq!((best.ex_score, best.bp), (40, 7));
}

fn play_with_bp(ex_score: u32, bp: u32) -> ScoreState {
    let mut score = score_with_ex_score(ex_score);
    for index in 0..bp {
        score.apply(&JudgementEvent {
            note_id: Some(NoteId(10_000 + index)),
            lane: Lane::Key1,
            judge: Judge::Poor,
            side: TimingSide::Slow,
            delta: TimeUs(1),
            time: TimeUs(10_000 + index as i64),
            affects_score: true,
            advances_score: true,
        });
    }
    score
}

#[test]
fn battle_as_migration_rebuilds_bests_from_beatoraja_history() {
    for previous_v40 in [false, true] {
        let mut db = database_before_battle_as_migration();
        let old_local = db.insert_score(&battle_as_record(400, ClearType::Max, 1)).unwrap();
        let imported = ScoreRecord {
            source_kind: ScoreSourceKind::Beatoraja,
            total_notes: 128,
            score: play_with_bp(256, 0),
            ..battle_as_record(256, ClearType::FullCombo, 2)
        };
        let import_id = db.insert_score(&imported).unwrap();
        let mut import_only = imported.clone();
        import_only.chart_sha256 = [8; 32];
        db.insert_score(&import_only).unwrap();
        if previous_v40 {
            db.conn()
                .execute_batch(
                    "UPDATE score_history SET normal_best_eligible = 0;
                     DELETE FROM score_best;
                     PRAGMA user_version = 40;",
                )
                .unwrap();
            db.insert_score(&battle_as_record(100, ClearType::Easy, 3)).unwrap();
            db.update_score_clear_only(&battle_as_record(0, ClearType::Max, 4)).unwrap();
        }
        let stats_before = db.player_stats().unwrap();
        for _ in 0..2 {
            run_score_migrations(&mut db.conn).unwrap();
        }
        assert_eq!(db.player_stats().unwrap(), stats_before);
        for (id, expected) in [(old_local, 0), (import_id, 1)] {
            let eligible: i64 = db
                .conn()
                .query_row(
                    "SELECT normal_best_eligible FROM score_history WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(eligible, expected);
        }
        let key = battle_as_key();
        let best = db.best_scores_for_charts(&[key]).unwrap().remove(0);
        assert_eq!(best.ex_score, 256);
        assert_eq!(best.clear_type, if previous_v40 { "Max" } else { "FullCombo" });
        let counts: (u32, u32) = db
            .conn()
            .query_row(
                "SELECT play_count, clear_count FROM score_best WHERE chart_sha256 = ?1",
                [crate::storage::common::hash_to_hex(&key.chart_sha256)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, if previous_v40 { (3, 3) } else { (1, 1) });
        let import_key = ScoreKey { chart_sha256: import_only.chart_sha256, ..key };
        assert_eq!(db.best_scores_for_charts(&[import_key]).unwrap()[0].ex_score, 256);
        // Imported ghosts are stored on the best row. A re-import can fill a lost ghost.
        assert_eq!(
            db.reconcile_imported_score(&imported).unwrap(),
            ImportedScoreReconciliation::Corrected
        );
        assert_eq!(
            db.reconcile_imported_score(&imported).unwrap(),
            ImportedScoreReconciliation::Unchanged
        );
    }
}

#[test]
fn deleting_excluded_battle_as_duplicates_preserves_new_play_counts() {
    let mut db = database_before_battle_as_migration();
    let legacy = battle_as_record(100, ClearType::Hard, 1);
    let old_id = db.insert_score(&legacy).unwrap();
    db.insert_score(&legacy).unwrap();
    run_score_migrations(&mut db.conn).unwrap();
    db.insert_score(&battle_as_record(40, ClearType::Easy, 2)).unwrap();
    assert_eq!(db.purge_score_history_ids_and_rebuild(&[old_id]).unwrap(), 1);
    let counts: (u32, u32) = db.conn().query_row(
        "SELECT play_count, clear_count FROM score_best WHERE double_option = 'BattleAutoScratch'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(counts, (1, 1));
    let stats = db.player_stats().unwrap();
    assert_eq!((stats.play_count, stats.clear_count), (2, 2));
}
