use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::ids::NoteId;
use bmz_core::input::InputDeviceKind;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::ScoreState;

use super::*;
use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

fn score_with_ex_score(ex_score: u32) -> ScoreState {
    let mut score = ScoreState::default();
    for index in 0..(ex_score / 2) {
        score.apply(&JudgementEvent {
            note_id: Some(NoteId(index)),
            lane: Lane::Key1,
            judge: Judge::PGreat,
            side: TimingSide::Slow,
            delta: TimeUs(0),
            time: TimeUs(index as i64),
            affects_score: true,
        });
    }
    score
}

fn record(ex_score: u32, clear_type: ClearType) -> ScoreRecord {
    ScoreRecord {
        source_min_bp: None,
        chart_sha256: [7; 32],
        ln_policy: LnScorePolicy::ForceLn,
        double_option: DoubleOptionScoreBucket::Off,
        applied_double_option: DoubleOption::Off,
        played_at: 1_700_000_000,
        clear_type,
        gauge_type: Some(GaugeType::Normal),
        gauge_value: Some(82.0),
        total_notes: ex_score / 2,
        playtime_seconds: 0,
        score: score_with_ex_score(ex_score),
        count_unprocessed_notes: clear_type == ClearType::Failed,
        random_seed: None,
        seed_scheme: String::new(),
        arrange: "Normal".to_string(),
        arrange_2p: "Normal".to_string(),
        gauge_option: String::new(),
        rule_mode: String::new(),
        assist_mask: 0,
        autoplay: false,
        device_type: InputDeviceKind::Keyboard,
        replay_path: String::new(),
        source_kind: ScoreSourceKind::Local,
    }
}

#[test]
fn bokutachi_pms_device_best_aggregates_independent_metrics_before_current_history() {
    for (own, other) in [
        (InputDeviceKind::Controller, InputDeviceKind::Keyboard),
        (InputDeviceKind::Keyboard, InputDeviceKind::Controller),
    ] {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        let mut past = record(200, ClearType::Max);
        past.rule_mode = "Beatoraja".into();
        past.device_type = other;
        db.insert_score(&past).unwrap();
        let mut result = PlayResult {
            chart_sha256: [7; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 80.0,
            total_notes: 100,
            score: score_with_ex_score(20),
            autoplay: false,
        };
        assert!(db.improves_device_best(key([7; 32]), own, i64::MAX, &result).unwrap());
        past.device_type = own;
        past.clear_type = ClearType::Normal;
        past.score = score_with_ex_score(20);
        let first = db.insert_score(&past).unwrap();
        db.conn
            .execute("UPDATE score_history SET bp=10,cb=10,max_combo=10 WHERE id=?1", [first])
            .unwrap();
        past.score = score_with_ex_score(10);
        let second = db.insert_score(&past).unwrap();
        db.conn
            .execute(
                "UPDATE score_history SET bp=2,cb=2,max_combo=15,clear_type='Hard' WHERE id=?1",
                [second],
            )
            .unwrap();
        result.score.max_combo = 15;
        result.clear_type = ClearType::Hard;
        result.score.judges.fast_bad = 3;
        // Best EX's row has BP=10, but independent BP=2 is the comparison.
        assert!(!db.improves_device_best(key([7; 32]), own, i64::MAX, &result).unwrap());
        result.score.judges.fast_bad = 1;
        assert!(db.improves_device_best(key([7; 32]), own, i64::MAX, &result).unwrap());
        result.score.judges.fast_bad = 2;
        for metric in ["lamp", "combo", "ex", "cb"] {
            let mut improved = result.clone();
            match metric {
                "lamp" => improved.clear_type = ClearType::ExHard,
                "combo" => improved.score.max_combo = 16,
                "ex" => improved.score = score_with_ex_score(22),
                "cb" => {
                    improved.score.judges.fast_bad = 1;
                    improved.score.judges.fast_empty_poor = 1;
                }
                _ => unreachable!(),
            }
            assert!(
                db.improves_device_best(key([7; 32]), own, i64::MAX, &improved).unwrap(),
                "{metric}"
            );
        }
        past.score = score_with_ex_score(22);
        past.clear_type = ClearType::Max;
        let current = db.insert_score(&past).unwrap();
        result.score = score_with_ex_score(22);
        result.clear_type = ClearType::Max;
        assert!(db.improves_device_best(key([7; 32]), own, current, &result).unwrap());
        assert!(!db.improves_device_best(key([7; 32]), own, current + 1, &result).unwrap());
        // History-only imports do not become a new comparison baseline.
        past.score = score_with_ex_score(200);
        db.insert_score_with_mode(&past, ScoreInsertMode::HistoryOnly).unwrap();
        result.score = score_with_ex_score(24);
        assert!(db.improves_device_best(key([7; 32]), own, i64::MAX, &result).unwrap());
    }
}

#[test]
fn bokutachi_pms_device_best_excludes_external_imports_and_downloads() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    let mut imported = record(200, ClearType::Max);
    imported.rule_mode = "Beatoraja".into();
    // Imported PMS scores may have the default Keyboard device even when no
    // device information was available. They must not suppress a local play.
    for source in [ScoreSourceKind::Beatoraja, ScoreSourceKind::Lr2, ScoreSourceKind::Lr2Oraja] {
        imported.source_kind = source;
        db.insert_score(&imported).unwrap();
    }
    let download = ScoreHistorySourceRecord {
        key: ScoreHistorySourceKey {
            source: "bmz_ir".into(),
            provider: "bmz-official".into(),
            account_id: "player".into(),
            remote_score_id: "imported-pms".into(),
        },
        verification: String::new(),
        server_received_at: 1,
        imported_at: 2,
    };
    imported.source_kind = ScoreSourceKind::Beatoraja;
    db.insert_score_with_source(&imported, &download).unwrap();
    let result = PlayResult {
        chart_sha256: [7; 32],
        clear_type: ClearType::Normal,
        gauge_type: GaugeType::Normal,
        gauge_value: 80.0,
        total_notes: 100,
        score: score_with_ex_score(20),
        autoplay: false,
    };
    for device in [InputDeviceKind::Keyboard, InputDeviceKind::Controller] {
        assert!(db.improves_device_best(key([7; 32]), device, i64::MAX, &result).unwrap());
        let mut local = record(20, ClearType::Normal);
        local.rule_mode = "Beatoraja".into();
        local.device_type = device;
        let current = db.insert_score(&local).unwrap();
        assert!(db.improves_device_best(key([7; 32]), device, current, &result).unwrap());
        assert!(!db.improves_device_best(key([7; 32]), device, current + 1, &result).unwrap());
    }
    // This partition does not change local BEST or the other providers.
    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().remove(0);
    assert_eq!(best.ex_score, 200);
    assert_eq!(best.clear_type, "Max");
}

fn key(sha: [u8; 32]) -> ScoreKey {
    ScoreKey::new(sha, LnScorePolicy::ForceLn)
}

fn insert_test_course_score(db: &mut ScoreDatabase, course_hash: &str) -> i64 {
    db.conn_mut()
        .execute(
            "INSERT INTO course_scores (
                    course_hash, source, course_key, title, kind, constraints_json,
                    chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
                    gauge_value, max_combo, bp, course_failed, course_clear, arrange,
                    trophies_json, played_at, rule_mode
                 ) VALUES (
                    ?1, '', '', '', '', '{}',
                    '[]', 0, 0, 'NoPlay', '',
                    0.0, 0, 0, 0, 0, 'Normal',
                    '[]', 0, 'Beatoraja'
                 )",
            params![course_hash],
        )
        .unwrap();
    db.conn().last_insert_rowid()
}

fn sample_slot(slot: u8, ex_score: u32) -> ReplaySlotRecord {
    ReplaySlotRecord {
        chart_sha256: [1; 32],
        ln_policy: LnScorePolicy::ForceLn,
        double_option: DoubleOptionScoreBucket::Off,
        rule_mode: RuleMode::Beatoraja,
        slot,
        rule: ReplaySlotRule::Always,
        replay_path: format!("replay/{slot}.toml"),
        played_at: 1_700_000_000 + slot as i64,
        ex_score: Some(ex_score),
        bp: Some(0),
        cb: Some(0),
        max_combo: Some(ex_score),
        clear_rank: Some(ClearType::Normal as u8),
        source_kind: ScoreSourceKind::Local,
        source_path: String::new(),
        source_fingerprint: String::new(),
    }
}

#[path = "tests/best_score_options.rs"]
mod best_score_options;
#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/independent_bests.rs"]
mod independent_bests;
#[path = "tests/last_play.rs"]
mod last_play;
#[path = "tests/source_min_bp.rs"]
mod source_min_bp;
