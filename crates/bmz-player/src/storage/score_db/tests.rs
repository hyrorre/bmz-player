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
