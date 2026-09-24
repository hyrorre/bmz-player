use super::*;
use bmz_render::snapshot::SkinBestScoreOptions;

fn database() -> ScoreDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    ScoreDatabase { conn }
}

#[test]
fn best_score_options_follow_score_history_including_imports_and_flip() {
    let mut db = database();
    let mut best = record(20, ClearType::Normal);
    best.arrange = "Random".into();
    best.arrange_2p = "Mirror".into();
    best.applied_double_option = DoubleOption::Flip;
    best.source_kind = ScoreSourceKind::Beatoraja;
    db.insert_score(&best).unwrap();
    let expected = Some(SkinBestScoreOptions { arrange_1p: 2, arrange_2p: 1, double_option: 1 });
    assert_eq!(
        db.best_scores_for_charts(&[key(best.chart_sha256)]).unwrap()[0].play_options,
        expected
    );

    // A better lamp from a lower-scoring play must not supply the score's options.
    db.insert_score(&record(18, ClearType::Hard)).unwrap();
    let saved = db.best_scores_for_charts(&[key(best.chart_sha256)]).unwrap().remove(0);
    assert_eq!(saved.ex_score, 20);
    assert_eq!(saved.clear_type, "Hard");
    assert_eq!(saved.play_options, expected);

    // An exact tie retains the representative history. An accepted same-score
    // combo improvement changes it, following the existing score update policy.
    let mut equal = record(20, ClearType::Normal);
    db.insert_score(&equal).unwrap();
    assert_eq!(
        db.best_scores_for_charts(&[key(best.chart_sha256)]).unwrap()[0].play_options,
        expected
    );
    equal.score.max_combo += 1;
    equal.arrange = "FRandom".into();
    equal.arrange_2p = "MFRandom".into();
    db.insert_score(&equal).unwrap();
    assert_eq!(
        db.best_scores_for_charts(&[key(best.chart_sha256)]).unwrap()[0].play_options,
        Some(SkinBestScoreOptions { arrange_1p: 10, arrange_2p: 11, double_option: 0 })
    );
}

#[test]
fn best_score_options_respect_score_keys() {
    let mut db = database();
    let variants = [
        (LnScorePolicy::ForceLn, DoubleOptionScoreBucket::Off, RuleMode::Beatoraja),
        (LnScorePolicy::ForceCn, DoubleOptionScoreBucket::Off, RuleMode::Beatoraja),
        (LnScorePolicy::ForceLn, DoubleOptionScoreBucket::Battle, RuleMode::Beatoraja),
        (LnScorePolicy::ForceLn, DoubleOptionScoreBucket::Off, RuleMode::Dx),
    ];
    let mut keys = Vec::new();
    for (index, (ln, double, rule)) in variants.into_iter().enumerate() {
        let mut entry = record(20, ClearType::Normal);
        entry.ln_policy = ln;
        entry.double_option = double;
        entry.rule_mode = rule.as_str().into();
        entry.arrange =
            crate::select_options::ArrangeOption::VALUES[index].to_persistent_str().into();
        db.insert_score(&entry).unwrap();
        keys.push(ScoreKey::with_options(entry.chart_sha256, ln, double, rule));
    }
    for (index, best) in db.best_scores_for_charts(&keys).unwrap().into_iter().enumerate() {
        assert_eq!(best.play_options.unwrap().arrange_1p, index);
    }
}

#[test]
fn best_score_options_distinguish_zero_score_normal_from_missing_history() {
    let mut db = database();
    let key = key([7; 32]);
    assert!(db.best_scores_for_charts(&[key]).unwrap().is_empty());
    db.insert_score(&record(0, ClearType::Failed)).unwrap();
    assert_eq!(
        db.best_scores_for_charts(&[key]).unwrap()[0].play_options,
        Some(SkinBestScoreOptions { arrange_1p: 0, arrange_2p: 0, double_option: 0 })
    );
    db.conn.execute("UPDATE score_best SET best_score_history_id = NULL", []).unwrap();
    let best = db.best_scores_for_charts(&[key]).unwrap().remove(0);
    assert_eq!(best.ex_score, 0);
    assert_eq!(best.play_options, None);
    db.conn.execute("UPDATE score_best SET best_score_history_id = 99999", []).unwrap();
    assert_eq!(db.best_scores_for_charts(&[key]).unwrap()[0].play_options, None);
}
