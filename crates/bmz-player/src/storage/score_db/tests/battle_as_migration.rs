use super::*;

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
    let before = SCORE_MIGRATIONS.iter().position(|m| m.version == 40).unwrap();
    run_migrations(&mut conn, &SCORE_MIGRATIONS[..before]).unwrap();
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

    run_migrations(&mut db.conn, SCORE_MIGRATIONS).unwrap();

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
    run_migrations(&mut db.conn, SCORE_MIGRATIONS).unwrap();
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
