use super::*;
use crate::ln_policy::ChartLnProfile;

fn database(trusted: bool) -> ScoreDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let db = ScoreDatabase::from_connection(conn);
    if trusted {
        db.register_ln_profile(
            [7; 32],
            ChartLnProfile { has_undefined_ln: true, has_defined_ln: true, ..Default::default() },
        )
        .unwrap();
    }
    db
}

fn attempt(
    policy: LnScorePolicy,
    ex: u32,
    bp: u32,
    cb: u32,
    combo: u32,
    clear: ClearType,
) -> ScoreRecord {
    let mut r = record(ex, clear);
    r.ln_policy = policy;
    r.rule_mode = RuleMode::Beatoraja.as_str().into();
    r.score.judges.fast_bad = cb;
    r.score.judges.fast_empty_poor = bp - cb;
    r.score.max_combo = combo;
    r.replay_path = format!("{ex}-{bp}-{cb}.toml");
    r
}

fn best(db: &ScoreDatabase) -> BestScoreSummary {
    db.best_scores_for_charts(&[key([7; 32])]).unwrap().remove(0)
}

#[test]
fn old_only_new_only_and_unproven_profiles_are_separate() {
    for trusted in [false, true] {
        let mut db = database(trusted);
        let old = attempt(LnScorePolicy::AutoLn, 200, 10, 5, 80, ClearType::Hard);
        db.insert_score(&old).unwrap();
        assert_eq!(db.best_ex_score(key([7; 32])).unwrap(), trusted.then_some(200));
        let new = attempt(LnScorePolicy::ForceLn, 100, 20, 8, 50, ClearType::Normal);
        let id = db.insert_score(&new).unwrap();
        let b = best(&db);
        assert_eq!(b.ex_score, if trusted { 200 } else { 100 });
        assert_eq!(b.play_count, if trusted { 2 } else { 1 });
        let prior: Option<u32> = db
            .conn
            .query_row("SELECT old_ex_score FROM score_history WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(prior, trusted.then_some(200));
        assert_eq!(db.recent_history(10, 0).unwrap().len(), 2);
        assert_eq!(
            db.conn
                .query_row(
                    "SELECT COUNT(*) FROM score_history WHERE ln_policy = 'ForceLn'",
                    [],
                    |r| r.get::<_, u32>(0)
                )
                .unwrap(),
            1
        );
    }
    let mut db = database(true);
    db.insert_score(&attempt(LnScorePolicy::ForceLn, 100, 20, 8, 50, ClearType::Normal)).unwrap();
    assert_eq!(best(&db).ex_score, 100);
}

#[test]
fn merges_independent_bests_and_uses_real_representative_metrics() {
    let mut db = database(true);
    let mut a = attempt(LnScorePolicy::AutoLn, 200, 20, 12, 80, ClearType::Normal);
    a.score.ghost = vec![1; a.total_notes as usize];
    db.insert_score(&a).unwrap();
    let b = attempt(LnScorePolicy::AutoLn, 100, 2, 1, 90, ClearType::Hard);
    db.insert_score(&b).unwrap();
    let mut c = attempt(LnScorePolicy::ForceLn, 200, 10, 7, 75, ClearType::Easy);
    c.score.ghost = vec![2; c.total_notes as usize];
    db.insert_score(&c).unwrap();
    let d = attempt(LnScorePolicy::ForceLn, 80, 15, 3, 95, ClearType::ExHard);
    db.insert_score(&d).unwrap();
    let result = best(&db);
    assert_eq!((result.ex_score, result.bp, result.cb, result.max_combo), (200, 2, 1, 95));
    assert_eq!(result.clear_type, "ExHard");
    assert_eq!(result.play_count, 4);
    assert_eq!(result.replay_path, c.replay_path); // Actual BP 10 beats 20, despite aggregate BP 2.
    assert_eq!(db.best_ghost(key([7; 32]), c.total_notes).unwrap(), Some(c.score.ghost));
    assert_eq!(db.clear_option_history(key([7; 32])).unwrap().len(), 4);
    db.conn
        .execute(
            "UPDATE score_best SET best_score_history_id = NULL WHERE ln_policy = 'AutoLn'",
            [],
        )
        .unwrap();
    assert_eq!(best(&db).replay_path, a.replay_path); // Unknown tie-breaks keep old metadata.
    assert_eq!(db.best_ghost(key([7; 32]), a.total_notes).unwrap(), Some(a.score.ghost));
    let clear = attempt(LnScorePolicy::ForceLn, 0, 0, 0, 0, ClearType::Max);
    db.update_score_clear_only(&clear).unwrap();
    assert_eq!(best(&db).clear_type, "Max");
    assert_eq!(best(&db).play_count, 5);
}

#[test]
fn clear_only_is_preserved_without_treating_neutral_bp_as_a_record() {
    let mut db = database(true);
    db.update_score_clear_only(&attempt(LnScorePolicy::AutoLn, 0, 0, 0, 0, ClearType::Hard))
        .unwrap();
    db.insert_score(&attempt(LnScorePolicy::ForceLn, 100, 8, 4, 50, ClearType::Normal)).unwrap();
    let b = best(&db);
    assert_eq!((b.bp, b.cb, b.play_count, b.clear_count), (8, 4, 2, 2));
    assert_eq!(b.clear_type, "Hard");
    let mut older_key = attempt(LnScorePolicy::AutoLn, 0, 0, 0, 0, ClearType::ExHard);
    older_key.played_at += 500;
    db.update_score_clear_only(&older_key).unwrap();
    let mut canonical = older_key.clone();
    canonical.ln_policy = LnScorePolicy::ForceLn;
    canonical.played_at -= 100;
    db.update_score_clear_only(&canonical).unwrap();
    assert_eq!(
        db.last_played_times_for_charts(&[key([7; 32])]).unwrap()[&key([7; 32])],
        older_key.played_at
    );
    assert_eq!(best(&db).play_count, 4);
}

#[test]
fn slots_prefer_canonical_and_legacy_replays_keep_bytes_hash_and_policy() {
    use crate::select_options::ArrangeOption;
    use crate::storage::replay::{
        ReplayFile, load_replay_for_chart_and_policy, save_replay_with_hash,
    };
    let mut db = database(true);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("old.toml");
    let replay = ReplayFile::new_with_policy(
        [7; 32],
        LnScorePolicy::AutoLn,
        DoubleOptionScoreBucket::Off,
        100,
        None,
        ArrangeOption::Normal,
        ArrangeOption::Normal,
        None,
        None,
        vec![],
    );
    let hash = save_replay_with_hash(&path, &replay).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for slot in [0, 1] {
        let mut old = sample_slot(slot, 200);
        old.chart_sha256 = [7; 32];
        old.ln_policy = LnScorePolicy::AutoLn;
        old.replay_path = path.to_string_lossy().into_owned();
        db.upsert_replay_slot(&old).unwrap();
    }
    let mut new = sample_slot(0, 100);
    new.chart_sha256 = [7; 32];
    db.upsert_replay_slot(&new).unwrap();
    let slots = db.replay_slots_for_chart(key([7; 32])).unwrap();
    assert_eq!(slots[0].as_ref().unwrap().ex_score, Some(100));
    let old = slots[1].as_ref().unwrap();
    assert_eq!(old.ln_policy, LnScorePolicy::AutoLn);
    assert!(load_replay_for_chart_and_policy(&path, [7; 32], old.ln_policy).is_ok());
    assert!(load_replay_for_chart_and_policy(&path, [7; 32], LnScorePolicy::ForceCn).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        bmz_chart::hash::compute_chart_identity(&bytes).file_sha256,
        crate::storage::common::hex_to_hash::<32>(&hash).unwrap()
    );
    assert_eq!(
        db.replay_slots_for_charts(&[key([7; 32])]).unwrap()[0].replay_slots,
        [true, true, false, false]
    );
}

#[test]
fn reimport_matches_old_policy_and_only_supplements_source_bp() {
    let mut db = database(true);
    let mut old = attempt(LnScorePolicy::AutoLn, 200, 20, 10, 80, ClearType::Hard);
    old.source_kind = ScoreSourceKind::Beatoraja;
    let id = db.insert_imported_aggregate_score(&old).unwrap();
    let identity = db.ir_submission_idempotency_key_for_history(id).unwrap();
    let mut incoming = old.clone();
    incoming.ln_policy = LnScorePolicy::ForceLn;
    incoming.source_min_bp = Some(3);
    assert_eq!(
        db.reconcile_imported_aggregate_score(&incoming).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    assert_eq!(
        db.reconcile_imported_aggregate_score(&incoming).unwrap(),
        ImportedScoreReconciliation::Unchanged
    );
    assert_eq!((best(&db).bp, best(&db).play_count), (3, 1));
    assert_eq!(db.recent_history(10, 0).unwrap().len(), 1);
    assert_eq!(db.recent_history(10, 0).unwrap()[0].ln_policy, LnScorePolicy::AutoLn);
    assert_eq!(db.ir_submission_idempotency_key_for_history(id).unwrap(), identity);
}

#[test]
fn library_evidence_rejects_old_random_missing_and_changed_files() {
    use crate::storage::library_db::{CHART_IMPORT_VERSION, ChartImportRecord, LibraryDatabase};
    use crate::storage::migration::LIBRARY_MIGRATIONS;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("mixed.bmson");
    let text = r#"{"version":"1.0.0","info":{"title":"mixed","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240,"mode_hint":"beat-7k"},"sound_channels":[{"name":"key.wav","notes":[{"x":1,"y":0,"l":240,"c":false},{"x":2,"y":0,"l":240,"c":false,"t":2}]}]}"#;
    std::fs::write(&path, text).unwrap();
    let chart = bmz_chart::import::import_chart(&path, None, false).unwrap().chart;
    assert_eq!(
        ChartLnProfile::from_chart(&chart),
        ChartLnProfile { has_undefined_ln: true, has_defined_cn: true, ..Default::default() }
    );
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library = LibraryDatabase::from_connection(conn);
    library
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: text.len() as u64,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();
    let mut db = database(false);
    let mut old = record(100, ClearType::Normal);
    old.chart_sha256 = chart.identity.file_sha256;
    old.ln_policy = LnScorePolicy::AutoCn;
    db.insert_score(&old).unwrap();
    let key = ScoreKey::new(old.chart_sha256, LnScorePolicy::ForceCn);
    db.refresh_ln_compatibility(&library).unwrap();
    assert_eq!(db.best_ex_score(key).unwrap(), Some(100));
    for (version, random) in [(0, false), (CHART_IMPORT_VERSION, true)] {
        library
            .conn()
            .execute(
                "UPDATE charts SET import_version = ?1, has_bms_random = ?2",
                params![version, random],
            )
            .unwrap();
        db.refresh_ln_compatibility(&library).unwrap();
        assert_eq!(db.best_ex_score(key).unwrap(), None);
    }
    library.conn().execute("UPDATE charts SET has_bms_random = 0", []).unwrap();
    std::fs::write(&path, "#TITLE changed").unwrap();
    db.refresh_ln_compatibility(&library).unwrap();
    assert_eq!(db.best_ex_score(key).unwrap(), None);
    std::fs::remove_file(&path).unwrap();
    db.refresh_ln_compatibility(&library).unwrap();
    assert_eq!(db.best_ex_score(key).unwrap(), None);
    assert_eq!(db.recent_history(10, 0).unwrap()[0].ln_policy, LnScorePolicy::AutoCn);
}

#[test]
fn course_aliases_combine_attempts_and_slots_but_require_all_stage_evidence() {
    let mut db = database(false);
    let a = [1; 32];
    let b = [2; 32];
    db.register_ln_profile(a, ChartLnProfile { has_undefined_ln: true, ..Default::default() })
        .unwrap();
    db.register_ln_profile(b, ChartLnProfile { has_defined_cn: true, ..Default::default() })
        .unwrap();
    let hashes = serde_json::to_string(&[hash_to_hex(&a), hash_to_hex(&b)]).unwrap();
    let old_id = insert_test_course_score(&mut db, "course");
    db.conn.execute("UPDATE course_scores SET ln_policy = 'AutoCn', chart_sha256s_json = ?1,
        constraints_json = '{\"ln\":\"cn\"}', ex_score = 200, clear_type = 'Normal', played_at = 100 WHERE id = ?2",
        params![hashes, old_id]).unwrap();
    let new_id = insert_test_course_score(&mut db, "course");
    db.conn.execute("UPDATE course_scores SET ln_policy = 'ForceCn', chart_sha256s_json = ?1,
        constraints_json = '{\"ln\":\"cn\"}', ex_score = 100, clear_type = 'Hard', played_at = 200 WHERE id = ?2",
        params![hashes, new_id]).unwrap();
    let mode = RuleMode::Beatoraja;
    let canonical = LnScorePolicy::ForceCn;
    for id in [old_id, new_id] {
        for (position, sha) in [a, b].into_iter().enumerate() {
            db.conn.execute("INSERT INTO course_score_charts
                (course_score_id, position, chart_sha256, ex_score, max_combo, clear_type, gauge_value)
                VALUES (?1, ?2, ?3, 100, 50, 'Normal', 80)", params![id, position as i64, hash_to_hex(&sha)]).unwrap();
            db.conn.execute("INSERT INTO course_replays (course_score_id, position, chart_sha256, replay_path)
                VALUES (?1, ?2, ?3, 'old-stage.toml')", params![id, position as i64, hash_to_hex(&sha)]).unwrap();
        }
    }
    let best = db.best_course_score("course", canonical, mode).unwrap().unwrap();
    assert_eq!(
        (best.ex_score, best.play_count, best.clear_count, best.last_played_at),
        (200, 2, 2, Some(200))
    );
    assert_eq!(db.best_course_clear("course", canonical, mode).unwrap(), Some(ClearType::Hard));
    db.conn
        .execute(
            "UPDATE course_scores SET course_clear = 1, arrange = 'Normal', max_ex_score = 400",
            [],
        )
        .unwrap();
    let trophies = [bmz_core::course::CourseTrophy {
        name: "legacy".into(),
        max_miss_rate: 5.0,
        min_score_rate: 40.0,
    }];
    assert_eq!(
        db.achieved_trophy_names_for_definition("course", canonical, mode, &trophies).unwrap(),
        ["legacy"]
    );
    assert_eq!(db.list_recent_course_scores("course", canonical, mode, 10, 0).unwrap().len(), 2);
    assert_eq!(
        db.course_score_entry_by_id(old_id).unwrap().unwrap().ln_policy,
        LnScorePolicy::AutoCn
    );
    for slot in [0, 1] {
        db.upsert_course_replay_slot(&CourseReplaySlotRecord {
            course_hash: "course".into(),
            ln_policy: LnScorePolicy::AutoCn,
            rule_mode: mode,
            slot,
            rule: "Always".into(),
            course_score_id: old_id,
            played_at: 100,
            ex_score: 200,
            bp: 0,
            max_combo: 100,
            clear_rank: 5,
        })
        .unwrap();
    }
    let mut new = db.course_replay_slot("course", canonical, mode, 0).unwrap().unwrap();
    new.ln_policy = canonical;
    new.course_score_id = new_id;
    db.upsert_course_replay_slot(&new).unwrap();
    let slots = db.course_replay_slots_for_course("course", canonical, mode).unwrap();
    assert_eq!(slots[0].as_ref().unwrap().course_score_id, new_id);
    assert_eq!(slots[1].as_ref().unwrap().ln_policy, LnScorePolicy::AutoCn);
    assert_eq!(
        db.course_replay_slot_source("course", canonical, mode, 1)
            .unwrap()
            .unwrap()
            .course_score_id,
        old_id
    );
    db.conn
        .execute(
            "UPDATE course_scores SET constraints_json = '{\"ln\":\"hcn\"}' WHERE id = ?1",
            [old_id],
        )
        .unwrap();
    assert_eq!(db.best_course_score("course", canonical, mode).unwrap().unwrap().play_count, 1);
    db.conn
        .execute(
            "UPDATE course_scores SET constraints_json = '{\"ln\":\"cn\"}' WHERE id = ?1",
            [old_id],
        )
        .unwrap();
    db.conn
        .execute("DELETE FROM ln_compatible_profiles WHERE sha = ?1", [hash_to_hex(&a)])
        .unwrap();
    assert_eq!(db.best_course_score("course", canonical, mode).unwrap().unwrap().play_count, 1);
    assert!(db.course_replay_slot("course", canonical, mode, 1).unwrap().is_none());
}
