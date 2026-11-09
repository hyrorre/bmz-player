use rusqlite::Connection;

use super::*;
use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

const LN_POLICY: LnScorePolicy = LnScorePolicy::ForceLn;

#[test]
fn last_play_course_tracks_non_best_failed_attempts_and_excludes_replay_only() {
    let mut conn = open_conn();
    assert!(best_course_score(&conn, "date", LN_POLICY, RuleMode::Beatoraja).unwrap().is_none());
    insert_course_score(&mut conn, &sample_score("date", 500, "Normal", 100)).unwrap();
    insert_course_score(&mut conn, &sample_score("date", 100, "Failed", 200)).unwrap();
    let replay = insert_course_score(&mut conn, &sample_score("date", 900, "Normal", 300)).unwrap();
    conn.execute("UPDATE course_scores SET replay_only = 1 WHERE id = ?1", [replay]).unwrap();
    let mut other = sample_score("date", 800, "Normal", 400);
    other.ln_policy = LnScorePolicy::ForceCn;
    insert_course_score(&mut conn, &other).unwrap();
    other.ln_policy = LN_POLICY;
    other.rule_mode = RuleMode::Dx;
    insert_course_score(&mut conn, &other).unwrap();
    let best = best_course_score(&conn, "date", LN_POLICY, RuleMode::Beatoraja).unwrap().unwrap();
    assert_eq!(best.played_at, 100);
    assert_eq!(best.last_played_at, Some(200));
    let best = best_course_score_for_trophy(&conn, "date", LN_POLICY, RuleMode::Beatoraja, "gold")
        .unwrap()
        .unwrap();
    assert_eq!(best.last_played_at, Some(200));
}

fn open_conn() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    super::super::common::configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    conn
}

fn sample_score(
    course_hash: &str,
    ex_score: u32,
    clear: &str,
    played_at: i64,
) -> CourseScoreInsert {
    CourseScoreInsert {
        course_hash: course_hash.to_string(),
        ln_policy: LN_POLICY,
        rule_mode: RuleMode::Beatoraja,
        source: "table:x".to_string(),
        course_key: "dan-1".to_string(),
        title: "Dan 1".to_string(),
        kind: "dan".to_string(),
        constraints_json: r#"{"gauge":"Lr2"}"#.to_string(),
        chart_sha256s_json: r#"["1111"]"#.to_string(),
        ex_score,
        max_ex_score: 1_000,
        clear_type: clear.to_string(),
        gauge_type: "Normal".to_string(),
        gauge_value: 82.5,
        max_combo: 123,
        bp: 7,
        course_failed: clear == "Failed",
        course_clear: clear != "Failed",
        arrange: "Normal".to_string(),
        trophies_json: r#"["gold"]"#.to_string(),
        played_at,
        charts: vec![CourseScoreChartRecord {
            position: 0,
            chart_sha256: [1; 32],
            ex_score,
            max_combo: 123,
            clear_type: clear.to_string(),
            gauge_value: 82.5,
        }],
        replays: vec![CourseReplayRecord {
            position: 0,
            chart_sha256: [1; 32],
            replay_path: "replay/course.toml".to_string(),
        }],
        achieved_trophies: vec!["gold".to_string()],
    }
}

fn sample_slot(
    course_hash: &str,
    slot: u8,
    course_score_id: i64,
    ex_score: u32,
) -> CourseReplaySlotRecord {
    CourseReplaySlotRecord {
        course_hash: course_hash.to_string(),
        ln_policy: LN_POLICY,
        rule_mode: RuleMode::Beatoraja,
        slot,
        rule: "score".to_string(),
        course_score_id,
        played_at: 1_700_000_000,
        ex_score,
        bp: 7,
        max_combo: 123,
        clear_rank: ClearType::Normal as u8,
    }
}

#[test]
fn insert_course_score_round_trips_score_children_and_trophies() {
    let mut conn = open_conn();
    let score_id =
        insert_course_score(&mut conn, &sample_score("course-a", 500, "Normal", 10)).unwrap();
    conn.execute(
        "INSERT INTO score_history (
            chart_sha256, played_at, clear_type, gauge_type, gauge_value,
            total_notes, ex_score, bp, cb, max_combo,
            fast_pgreat, slow_pgreat, fast_great, slow_great,
            fast_good, slow_good, fast_bad, slow_bad,
            fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
            random_seed, gauge_option, assist_mask, autoplay, replay_path, course_score_id
        ) VALUES (
            ?1, 10, 'Normal', 'Normal', 82.5,
            100, 500, 7, 4, 123,
            0, 0, 0, 0,
            0, 0, 0, 0,
            0, 0, 0, 0,
            NULL, '', 0, 0, '', ?2
        )",
        params![hash_to_hex(&[1; 32]), score_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO score_history (
            chart_sha256, played_at, clear_type, gauge_type, gauge_value,
            total_notes, ex_score, bp, cb, max_combo,
            fast_pgreat, slow_pgreat, fast_great, slow_great,
            fast_good, slow_good, fast_bad, slow_bad,
            fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
            random_seed, gauge_option, assist_mask, autoplay, replay_path, course_score_id
        ) VALUES (
            ?1, 10, 'Normal', 'Normal', 82.5,
            100, 500, 7, 6, 123,
            0, 0, 0, 0,
            0, 0, 0, 0,
            0, 0, 0, 0,
            NULL, '', 0, 0, '', ?2
        )",
        params![hash_to_hex(&[2; 32]), score_id],
    )
    .unwrap();

    conn.execute(
        "UPDATE score_history
         SET fast_pgreat = 3, slow_pgreat = 4,
             fast_great = 5, slow_great = 6,
             fast_good = 7, slow_good = 8,
             fast_bad = 9, slow_bad = 10,
             fast_poor = 11, slow_poor = 12,
             fast_empty_poor = 13, slow_empty_poor = 14
         WHERE course_score_id = ?1",
        params![score_id],
    )
    .unwrap();

    let best =
        best_course_score(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap().unwrap();
    assert_eq!(best.course_score_id, score_id);
    assert_eq!(best.course_hash, "course-a");
    assert_eq!(best.ex_score, 500);
    assert_eq!(best.bp, 7);
    assert_eq!(best.cb, 10);
    assert_eq!(best.play_count, 1);
    assert_eq!(best.clear_count, 1);
    assert_eq!(best.judge_counts.pgreat, 14);
    assert_eq!(best.judge_counts.great, 22);
    assert_eq!(best.judge_counts.good, 30);
    assert_eq!(best.judge_counts.bad, 38);
    assert_eq!(best.judge_counts.poor, 46);
    assert_eq!(best.judge_counts.empty_poor, 54);
    assert_eq!(best.fast_slow_counts.fast_pgreat, 6);
    assert_eq!(best.fast_slow_counts.slow_empty_poor, 28);

    let charts = list_course_score_charts(&conn, score_id).unwrap();
    assert_eq!(charts.len(), 1);
    assert_eq!(charts[0].chart_sha256, [1; 32]);

    let replays = list_course_replays(&conn, score_id).unwrap();
    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].replay_path, "replay/course.toml");

    assert_eq!(
        achieved_trophy_names_for_definition(
            &conn,
            "course-a",
            LN_POLICY,
            RuleMode::Beatoraja,
            &[CourseTrophy { name: "gold".to_string(), max_miss_rate: 2.0, min_score_rate: 50.0 }],
        )
        .unwrap(),
        vec!["gold".to_string()]
    );
    let entry = course_score_entry_by_id(&conn, score_id).unwrap().unwrap();
    assert_eq!(entry.title, "Dan 1");
    assert_eq!(entry.bp, 7);
    assert_eq!(entry.achieved_trophies, vec!["gold".to_string()]);
}

#[test]
fn best_and_latest_are_scoped_by_course_hash() {
    let mut conn = open_conn();
    insert_course_score(&mut conn, &sample_score("course-a", 400, "Hard", 10)).unwrap();
    let newer =
        insert_course_score(&mut conn, &sample_score("course-a", 300, "Normal", 20)).unwrap();
    insert_course_score(&mut conn, &sample_score("course-b", 900, "Normal", 30)).unwrap();

    let best =
        best_course_score(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap().unwrap();
    assert_eq!(best.ex_score, 400);
    assert_eq!(best.play_count, 2);
    assert_eq!(best.clear_count, 2);
    assert_eq!(
        latest_course_score_id(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap(),
        Some(newer)
    );
    assert_eq!(
        best_course_clear(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap(),
        Some(ClearType::Hard)
    );
}

#[test]
fn assist_course_record_updates_clear_without_overwriting_numeric_best() {
    let mut conn = open_conn();
    let mut baseline = sample_score("course-assist", 500, "Failed", 10);
    baseline.trophies_json = "[]".to_owned();
    baseline.achieved_trophies.clear();
    baseline.replays.clear();
    insert_course_score(&mut conn, &baseline).unwrap();

    let mut assist = sample_score("course-assist", 0, "LightAssistEasy", 20);
    assist.max_combo = 0;
    assist.bp = 0;
    assist.course_clear = false;
    assist.trophies_json = "[]".to_owned();
    assist.achieved_trophies.clear();
    assist.replays.clear();
    for chart in &mut assist.charts {
        chart.ex_score = 0;
        chart.max_combo = 0;
    }
    let assist_id = insert_course_score(&mut conn, &assist).unwrap();

    let best_score =
        best_course_score(&conn, "course-assist", LN_POLICY, RuleMode::Beatoraja).unwrap().unwrap();
    assert_eq!(best_score.ex_score, 500);
    assert_eq!(best_score.clear_type, "Failed");

    assert_eq!(
        best_course_clear(&conn, "course-assist", LN_POLICY, RuleMode::Beatoraja,).unwrap(),
        Some(ClearType::LightAssistEasy)
    );
    let assist_entry = course_score_entry_by_id(&conn, assist_id).unwrap().unwrap();
    assert_eq!(assist_entry.clear_type, "LightAssistEasy");
    assert_eq!(assist_entry.ex_score, 0);
    assert_eq!(assist_entry.max_combo, 0);
    assert_eq!(assist_entry.bp, 0);
    assert!(!assist_entry.course_clear);
    assert!(assist_entry.achieved_trophies.is_empty());

    let charts = list_course_score_charts(&conn, assist_id).unwrap();
    assert!(charts.iter().all(|chart| chart.ex_score == 0 && chart.max_combo == 0));
    assert!(list_course_replays(&conn, assist_id).unwrap().is_empty());
}

#[test]
fn replay_slots_are_keyed_by_course_hash() {
    let mut conn = open_conn();
    let score_id =
        insert_course_score(&mut conn, &sample_score("course-a", 500, "Normal", 10)).unwrap();
    upsert_course_replay_slot(&mut conn, &sample_slot("course-a", 0, score_id, 500)).unwrap();
    upsert_course_replay_slot(&mut conn, &sample_slot("course-a", 3, score_id, 700)).unwrap();

    let slot =
        course_replay_slot(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja, 3).unwrap().unwrap();
    assert_eq!(slot.ex_score, 700);
    assert_eq!(slot.bp, 7);
    assert_eq!(
        course_replay_slot_presence(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap(),
        [true, false, false, true]
    );
    assert_eq!(
        course_replay_slot_presence(&conn, "course-b", LN_POLICY, RuleMode::Beatoraja).unwrap(),
        [false; 4]
    );
}

#[test]
fn course_scores_are_separate_per_ln_policy() {
    let mut conn = open_conn();
    let force_ln_id =
        insert_course_score(&mut conn, &sample_score("course-a", 400, "Hard", 10)).unwrap();
    let mut force_cn = sample_score("course-a", 900, "Normal", 20);
    force_cn.ln_policy = LnScorePolicy::ForceCn;
    force_cn.trophies_json = r#"["silver"]"#.to_string();
    force_cn.achieved_trophies = vec!["silver".to_string()];
    let force_cn_id = insert_course_score(&mut conn, &force_cn).unwrap();

    let force_ln =
        best_course_score(&conn, "course-a", LnScorePolicy::ForceLn, RuleMode::Beatoraja)
            .unwrap()
            .unwrap();
    let force_cn =
        best_course_score(&conn, "course-a", LnScorePolicy::ForceCn, RuleMode::Beatoraja)
            .unwrap()
            .unwrap();

    assert_eq!(force_ln.course_score_id, force_ln_id);
    assert_eq!(force_ln.ex_score, 400);
    assert_eq!(force_ln.play_count, 1);
    assert_eq!(force_ln.clear_count, 1);
    assert_eq!(force_cn.course_score_id, force_cn_id);
    assert_eq!(force_cn.ex_score, 900);
    assert_eq!(force_cn.play_count, 1);
    assert_eq!(force_cn.clear_count, 1);
    assert_eq!(
        best_course_clear(&conn, "course-a", LnScorePolicy::ForceLn, RuleMode::Beatoraja,).unwrap(),
        Some(ClearType::Hard)
    );
    assert_eq!(
        achieved_trophy_names_for_definition(
            &conn,
            "course-a",
            LnScorePolicy::ForceLn,
            RuleMode::Beatoraja,
            &[CourseTrophy { name: "gold".to_string(), max_miss_rate: 2.0, min_score_rate: 40.0 }],
        )
        .unwrap(),
        vec!["gold".to_string()]
    );
    assert_eq!(
        achieved_trophy_names_for_definition(
            &conn,
            "course-a",
            LnScorePolicy::ForceCn,
            RuleMode::Beatoraja,
            &[CourseTrophy {
                name: "silver".to_string(),
                max_miss_rate: 2.0,
                min_score_rate: 90.0,
            }],
        )
        .unwrap(),
        vec!["silver".to_string()]
    );
    assert_eq!(
        latest_course_score_id(&conn, "course-a", LnScorePolicy::ForceLn, RuleMode::Beatoraja,)
            .unwrap(),
        Some(force_ln_id)
    );
    let history = list_recent_course_scores(
        &conn,
        "course-a",
        LnScorePolicy::ForceCn,
        RuleMode::Beatoraja,
        10,
        0,
    )
    .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].course_score_id, force_cn_id);
    assert_eq!(history[0].ln_policy, LnScorePolicy::ForceCn);
}

#[test]
fn achieved_trophies_are_recalculated_from_current_definition() {
    let mut conn = open_conn();
    insert_course_score(&mut conn, &sample_score("course-a", 500, "Normal", 10)).unwrap();

    let strict =
        [CourseTrophy { name: "gold".to_string(), max_miss_rate: 2.0, min_score_rate: 60.0 }];
    assert!(
        achieved_trophy_names_for_definition(
            &conn,
            "course-a",
            LN_POLICY,
            RuleMode::Beatoraja,
            &strict,
        )
        .unwrap()
        .is_empty()
    );

    let renamed = [
        CourseTrophy { name: "bronze".to_string(), max_miss_rate: 2.0, min_score_rate: 50.0 },
        CourseTrophy { name: "bronze".to_string(), max_miss_rate: 100.0, min_score_rate: 0.0 },
    ];
    assert_eq!(
        achieved_trophy_names_for_definition(
            &conn,
            "course-a",
            LN_POLICY,
            RuleMode::Beatoraja,
            &renamed,
        )
        .unwrap(),
        vec!["bronze".to_string()]
    );
    assert!(
        achieved_trophy_names_for_definition(
            &conn,
            "course-a",
            LN_POLICY,
            RuleMode::Beatoraja,
            &[],
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn all_context_course_history_includes_ln_and_rule_variants() {
    let mut conn = open_conn();
    let force_ln_id =
        insert_course_score(&mut conn, &sample_score("course-a", 400, "Hard", 10)).unwrap();
    let mut force_cn_dx = sample_score("course-a", 900, "Normal", 20);
    force_cn_dx.ln_policy = LnScorePolicy::ForceCn;
    force_cn_dx.rule_mode = RuleMode::Dx;
    let force_cn_dx_id = insert_course_score(&mut conn, &force_cn_dx).unwrap();
    insert_course_score(&mut conn, &sample_score("course-b", 700, "Normal", 30)).unwrap();

    let history = list_recent_course_scores_all_contexts(&conn, "course-a", 10, 0).unwrap();
    assert_eq!(
        history.iter().map(|entry| entry.course_score_id).collect::<Vec<_>>(),
        vec![force_cn_dx_id, force_ln_id]
    );
    assert_eq!(history[0].ln_policy, LnScorePolicy::ForceCn);
    assert_eq!(history[0].rule_mode, RuleMode::Dx);
    assert_eq!(history[1].ln_policy, LnScorePolicy::ForceLn);
    assert_eq!(history[1].rule_mode, RuleMode::Beatoraja);
}

#[test]
fn replay_slots_require_complete_matching_course_replays() {
    let mut conn = open_conn();
    let complete_id =
        insert_course_score(&mut conn, &sample_score("complete", 500, "Normal", 10)).unwrap();
    assert!(course_replay_attempt_is_complete(&conn, complete_id).unwrap());

    let mut missing = sample_score("missing", 500, "Normal", 20);
    missing.replays.clear();
    let missing_id = insert_course_score(&mut conn, &missing).unwrap();
    assert!(!course_replay_attempt_is_complete(&conn, missing_id).unwrap());
    assert!(
        upsert_course_replay_slot(&mut conn, &sample_slot("missing", 0, missing_id, 500)).is_err()
    );

    let mut mismatched = sample_score("mismatched", 500, "Normal", 30);
    mismatched.replays[0].chart_sha256 = [2; 32];
    let mismatched_id = insert_course_score(&mut conn, &mismatched).unwrap();
    assert!(!course_replay_attempt_is_complete(&conn, mismatched_id).unwrap());

    conn.execute(
        "INSERT INTO course_replay_slots (
            course_hash, ln_policy, rule_mode, slot, rule, course_score_id,
            played_at, ex_score, bp, max_combo, clear_rank
         ) VALUES ('missing', 'ForceLn', 'Beatoraja', 0, 'Always', ?1, 20, 500, 7, 123, ?2)",
        params![missing_id, ClearType::Normal as u8],
    )
    .unwrap();
    assert!(
        course_replay_slot(&conn, "missing", LN_POLICY, RuleMode::Beatoraja, 0).unwrap().is_none()
    );
    assert_eq!(
        course_replay_slot_presence(&conn, "missing", LN_POLICY, RuleMode::Beatoraja).unwrap(),
        [false; 4]
    );
}

#[test]
fn course_replay_slots_are_separate_per_ln_policy() {
    let mut conn = open_conn();
    let force_ln_id =
        insert_course_score(&mut conn, &sample_score("course-a", 500, "Normal", 10)).unwrap();
    let mut force_cn = sample_score("course-a", 700, "Normal", 20);
    force_cn.ln_policy = LnScorePolicy::ForceCn;
    let force_cn_id = insert_course_score(&mut conn, &force_cn).unwrap();

    upsert_course_replay_slot(&mut conn, &sample_slot("course-a", 0, force_ln_id, 500)).unwrap();
    let mut force_cn_slot = sample_slot("course-a", 0, force_cn_id, 700);
    force_cn_slot.ln_policy = LnScorePolicy::ForceCn;
    upsert_course_replay_slot(&mut conn, &force_cn_slot).unwrap();

    let force_ln =
        course_replay_slot(&conn, "course-a", LnScorePolicy::ForceLn, RuleMode::Beatoraja, 0)
            .unwrap()
            .unwrap();
    let force_cn =
        course_replay_slot(&conn, "course-a", LnScorePolicy::ForceCn, RuleMode::Beatoraja, 0)
            .unwrap()
            .unwrap();

    assert_eq!(force_ln.course_score_id, force_ln_id);
    assert_eq!(force_cn.course_score_id, force_cn_id);
    assert_eq!(
        course_replay_slot_presence(
            &conn,
            "course-a",
            LnScorePolicy::ForceLn,
            RuleMode::Beatoraja,
        )
        .unwrap(),
        [true, false, false, false]
    );
    assert_eq!(
        course_replay_slot_presence(
            &conn,
            "course-a",
            LnScorePolicy::ForceCn,
            RuleMode::Beatoraja,
        )
        .unwrap(),
        [true, false, false, false]
    );
}

#[test]
fn course_scores_are_separate_per_rule_mode() {
    let mut conn = open_conn();
    let beatoraja_id =
        insert_course_score(&mut conn, &sample_score("course-a", 400, "Hard", 10)).unwrap();
    let mut dx = sample_score("course-a", 900, "Normal", 20);
    dx.rule_mode = RuleMode::Dx;
    let dx_id = insert_course_score(&mut conn, &dx).unwrap();

    let beatoraja =
        best_course_score(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap().unwrap();
    let dx = best_course_score(&conn, "course-a", LN_POLICY, RuleMode::Dx).unwrap().unwrap();

    assert_eq!(beatoraja.course_score_id, beatoraja_id);
    assert_eq!(beatoraja.ex_score, 400);
    assert_eq!(dx.course_score_id, dx_id);
    assert_eq!(dx.ex_score, 900);
    assert_eq!(
        latest_course_score_id(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap(),
        Some(beatoraja_id)
    );
    assert_eq!(
        latest_course_score_id(&conn, "course-a", LN_POLICY, RuleMode::Dx).unwrap(),
        Some(dx_id)
    );
}

#[test]
fn course_replay_slots_are_separate_per_rule_mode() {
    let mut conn = open_conn();
    let beatoraja_id =
        insert_course_score(&mut conn, &sample_score("course-a", 500, "Normal", 10)).unwrap();
    let mut dx = sample_score("course-a", 700, "Normal", 20);
    dx.rule_mode = RuleMode::Dx;
    let dx_id = insert_course_score(&mut conn, &dx).unwrap();
    upsert_course_replay_slot(&mut conn, &sample_slot("course-a", 0, beatoraja_id, 500)).unwrap();
    let mut dx_slot = sample_slot("course-a", 0, dx_id, 700);
    dx_slot.rule_mode = RuleMode::Dx;
    upsert_course_replay_slot(&mut conn, &dx_slot).unwrap();

    let beatoraja =
        course_replay_slot(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja, 0).unwrap().unwrap();
    let dx = course_replay_slot(&conn, "course-a", LN_POLICY, RuleMode::Dx, 0).unwrap().unwrap();

    assert_eq!(beatoraja.course_score_id, beatoraja_id);
    assert_eq!(beatoraja.ex_score, 500);
    assert_eq!(dx.course_score_id, dx_id);
    assert_eq!(dx.ex_score, 700);
    assert_eq!(
        course_replay_slot_presence(&conn, "course-a", LN_POLICY, RuleMode::Beatoraja).unwrap(),
        [true, false, false, false]
    );
    assert_eq!(
        course_replay_slot_presence(&conn, "course-a", LN_POLICY, RuleMode::Dx).unwrap(),
        [true, false, false, false]
    );
}
