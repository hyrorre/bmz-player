use std::path::Path;

use anyhow::Result;
use bmz_core::clear::{ClearType, GaugeType};
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::ScoreState;
use rusqlite::{Connection, OptionalExtension, params};

use super::common::configure_connection;

pub struct ScoreDatabase {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct ScoreRecord {
    pub chart_sha256: [u8; 32],
    pub played_at: i64,
    pub clear_type: ClearType,
    pub gauge_type: Option<GaugeType>,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub score: ScoreState,
    pub random_seed: Option<i64>,
    pub gauge_option: String,
    pub assist_mask: u32,
    pub autoplay: bool,
    pub replay_path: String,
}

impl ScoreRecord {
    pub fn from_play_result(
        result: &PlayResult,
        played_at: i64,
        random_seed: Option<i64>,
        gauge_option: impl Into<String>,
        assist_mask: u32,
        replay_path: impl Into<String>,
    ) -> Self {
        Self {
            chart_sha256: result.chart_sha256,
            played_at,
            clear_type: result.clear_type,
            gauge_type: Some(result.gauge_type),
            gauge_value: result.gauge_value,
            total_notes: result.total_notes,
            score: result.score.clone(),
            random_seed,
            gauge_option: gauge_option.into(),
            assist_mask,
            autoplay: result.autoplay,
            replay_path: replay_path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BestScoreSummary {
    pub chart_sha256: [u8; 32],
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub ex_score: u32,
    pub max_combo: u32,
    pub played_at: i64,
    pub replay_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaySlotSummary {
    pub chart_sha256: [u8; 32],
    pub replay_slots: [bool; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreHistoryEntry {
    pub id: i64,
    pub chart_sha256: [u8; 32],
    pub played_at: i64,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub ex_score: u32,
    pub max_combo: u32,
    pub autoplay: bool,
    pub replay_path: String,
}

impl ScoreDatabase {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        configure_connection(&conn)?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub(crate) fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn insert_score(&mut self, record: &ScoreRecord) -> Result<i64> {
        let tx = self.conn.transaction()?;
        insert_score_history(&tx, record)?;
        let history_id = tx.last_insert_rowid();
        upsert_score_best(&tx, record)?;
        tx.commit()?;
        Ok(history_id)
    }

    pub fn best_ex_score(&self, chart_sha256: [u8; 32]) -> Result<Option<u32>> {
        self.conn
            .query_row(
                "SELECT ex_score FROM score_best WHERE chart_sha256 = ?1",
                params![chart_sha256.as_slice()],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn best_scores_for_charts(
        &self,
        chart_sha256s: &[[u8; 32]],
    ) -> Result<Vec<BestScoreSummary>> {
        let mut out = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT
                chart_sha256,
                clear_type,
                gauge_type,
                gauge_value,
                ex_score,
                max_combo,
                played_at,
                replay_path
            FROM score_best
            WHERE chart_sha256 = ?1",
        )?;

        for sha256 in chart_sha256s {
            if let Some(summary) = stmt
                .query_row(params![sha256.as_slice()], best_score_summary_from_row)
                .optional()?
            {
                out.push(summary);
            }
        }

        Ok(out)
    }

    pub fn replay_slots_for_charts(
        &self,
        chart_sha256s: &[[u8; 32]],
    ) -> Result<Vec<ReplaySlotSummary>> {
        let mut out = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT replay_path
            FROM score_history
            WHERE chart_sha256 = ?1 AND replay_path <> ''
            ORDER BY played_at DESC, id DESC
            LIMIT 4",
        )?;

        for sha256 in chart_sha256s {
            let paths = stmt
                .query_map(params![sha256.as_slice()], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if paths.is_empty() {
                continue;
            }
            let mut replay_slots = [false; 4];
            for slot in replay_slots.iter_mut().take(paths.len()) {
                *slot = true;
            }
            out.push(ReplaySlotSummary { chart_sha256: *sha256, replay_slots });
        }

        Ok(out)
    }

    pub fn recent_history(&self, limit: u32, offset: u32) -> Result<Vec<ScoreHistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                chart_sha256,
                played_at,
                clear_type,
                gauge_type,
                gauge_value,
                total_notes,
                ex_score,
                max_combo,
                autoplay,
                replay_path
            FROM score_history
            ORDER BY played_at DESC, id DESC
            LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], score_history_entry_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn best_score_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BestScoreSummary> {
    let sha256_blob: Vec<u8> = row.get(0)?;
    let mut chart_sha256 = [0_u8; 32];
    chart_sha256.copy_from_slice(&sha256_blob[..32]);

    Ok(BestScoreSummary {
        chart_sha256,
        clear_type: row.get(1)?,
        gauge_type: row.get(2)?,
        gauge_value: row.get(3)?,
        ex_score: row.get(4)?,
        max_combo: row.get(5)?,
        played_at: row.get(6)?,
        replay_path: row.get(7)?,
    })
}

fn score_history_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScoreHistoryEntry> {
    let sha256_blob: Vec<u8> = row.get(1)?;
    let mut chart_sha256 = [0_u8; 32];
    chart_sha256.copy_from_slice(&sha256_blob[..32]);

    Ok(ScoreHistoryEntry {
        id: row.get(0)?,
        chart_sha256,
        played_at: row.get(2)?,
        clear_type: row.get(3)?,
        gauge_type: row.get(4)?,
        gauge_value: row.get(5)?,
        total_notes: row.get(6)?,
        ex_score: row.get(7)?,
        max_combo: row.get(8)?,
        autoplay: row.get(9)?,
        replay_path: row.get(10)?,
    })
}

fn insert_score_history(conn: &Connection, record: &ScoreRecord) -> Result<()> {
    let judges = &record.score.judges;
    conn.execute(
        "INSERT INTO score_history (
            chart_sha256,
            played_at,
            clear_type,
            gauge_type,
            gauge_value,
            total_notes,
            ex_score,
            max_combo,
            fast_pgreat,
            slow_pgreat,
            fast_great,
            slow_great,
            fast_good,
            slow_good,
            fast_bad,
            slow_bad,
            fast_poor,
            slow_poor,
            fast_empty_poor,
            slow_empty_poor,
            random_seed,
            gauge_option,
            assist_mask,
            autoplay,
            replay_path
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
        )",
        params![
            record.chart_sha256.as_slice(),
            record.played_at,
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.total_notes,
            record.score.ex_score(),
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.random_seed,
            record.gauge_option.as_str(),
            record.assist_mask,
            record.autoplay,
            record.replay_path.as_str(),
        ],
    )?;
    Ok(())
}

fn upsert_score_best(conn: &Connection, record: &ScoreRecord) -> Result<()> {
    let judges = &record.score.judges;
    conn.execute(
        "INSERT INTO score_best (
            chart_sha256,
            clear_type,
            gauge_type,
            gauge_value,
            ex_score,
            max_combo,
            fast_pgreat,
            slow_pgreat,
            fast_great,
            slow_great,
            fast_good,
            slow_good,
            fast_bad,
            slow_bad,
            fast_poor,
            slow_poor,
            fast_empty_poor,
            slow_empty_poor,
            played_at,
            replay_path
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20
        )
        ON CONFLICT(chart_sha256) DO UPDATE SET
            clear_type = excluded.clear_type,
            gauge_type = excluded.gauge_type,
            gauge_value = excluded.gauge_value,
            ex_score = excluded.ex_score,
            max_combo = excluded.max_combo,
            fast_pgreat = excluded.fast_pgreat,
            slow_pgreat = excluded.slow_pgreat,
            fast_great = excluded.fast_great,
            slow_great = excluded.slow_great,
            fast_good = excluded.fast_good,
            slow_good = excluded.slow_good,
            fast_bad = excluded.fast_bad,
            slow_bad = excluded.slow_bad,
            fast_poor = excluded.fast_poor,
            slow_poor = excluded.slow_poor,
            fast_empty_poor = excluded.fast_empty_poor,
            slow_empty_poor = excluded.slow_empty_poor,
            played_at = excluded.played_at,
            replay_path = excluded.replay_path
        WHERE
            excluded.ex_score > score_best.ex_score
            OR (
                excluded.ex_score = score_best.ex_score
                AND CASE excluded.clear_type
                    WHEN 'NoPlay' THEN 0
                    WHEN 'Failed' THEN 1
                    WHEN 'AssistEasy' THEN 2
                    WHEN 'LightAssistEasy' THEN 3
                    WHEN 'Easy' THEN 4
                    WHEN 'Normal' THEN 5
                    WHEN 'Hard' THEN 6
                    WHEN 'ExHard' THEN 7
                    WHEN 'FullCombo' THEN 8
                    WHEN 'Perfect' THEN 9
                    WHEN 'Max' THEN 10
                    ELSE 0
                END > CASE score_best.clear_type
                    WHEN 'NoPlay' THEN 0
                    WHEN 'Failed' THEN 1
                    WHEN 'AssistEasy' THEN 2
                    WHEN 'LightAssistEasy' THEN 3
                    WHEN 'Easy' THEN 4
                    WHEN 'Normal' THEN 5
                    WHEN 'Hard' THEN 6
                    WHEN 'ExHard' THEN 7
                    WHEN 'FullCombo' THEN 8
                    WHEN 'Perfect' THEN 9
                    WHEN 'Max' THEN 10
                    ELSE 0
                END
            )
            OR (
                excluded.ex_score = score_best.ex_score
                AND excluded.clear_type = score_best.clear_type
                AND excluded.max_combo > score_best.max_combo
            )",
        params![
            record.chart_sha256.as_slice(),
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.score.ex_score(),
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.played_at,
            record.replay_path.as_str(),
        ],
    )?;
    Ok(())
}

fn gauge_type_str(gauge_type: Option<GaugeType>) -> &'static str {
    gauge_type.map(GaugeType::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::ids::NoteId;
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
            });
        }
        score
    }

    fn record(ex_score: u32, clear_type: ClearType) -> ScoreRecord {
        ScoreRecord {
            chart_sha256: [7; 32],
            played_at: 1_700_000_000,
            clear_type,
            gauge_type: Some(GaugeType::Normal),
            gauge_value: 82.0,
            total_notes: ex_score / 2,
            score: score_with_ex_score(ex_score),
            random_seed: None,
            gauge_option: String::new(),
            assist_mask: 0,
            autoplay: false,
            replay_path: String::new(),
        }
    }

    #[test]
    fn insert_score_persists_enum_strings_and_empty_values() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut record = record(20, ClearType::Normal);
        record.gauge_type = None;
        db.insert_score(&record).unwrap();

        let (clear_type, gauge_type, gauge_option, replay_path): (String, String, String, String) =
            db.conn()
                .query_row(
                    "SELECT clear_type, gauge_type, gauge_option, replay_path FROM score_history",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .unwrap();

        assert_eq!(clear_type, "Normal");
        assert_eq!(gauge_type, "");
        assert_eq!(gauge_option, "");
        assert_eq!(replay_path, "");
    }

    #[test]
    fn best_score_keeps_higher_ex_score() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        db.insert_score(&record(10, ClearType::Hard)).unwrap();
        db.insert_score(&record(30, ClearType::Easy)).unwrap();

        assert_eq!(db.best_ex_score([7; 32]).unwrap(), Some(30));
    }

    #[test]
    fn best_scores_for_charts_returns_existing_scores() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        let mut first = record(20, ClearType::Normal);
        first.chart_sha256 = [1; 32];
        first.replay_path = "replay/one.bzr".to_string();
        let mut second = record(10, ClearType::Easy);
        second.chart_sha256 = [2; 32];
        second.gauge_type = None;

        db.insert_score(&first).unwrap();
        db.insert_score(&second).unwrap();

        let scores = db.best_scores_for_charts(&[[2; 32], [3; 32], [1; 32]]).unwrap();

        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].chart_sha256, [2; 32]);
        assert_eq!(scores[0].gauge_type, "");
        assert_eq!(scores[1].chart_sha256, [1; 32]);
        assert_eq!(scores[1].replay_path, "replay/one.bzr");
    }

    #[test]
    fn replay_slots_for_charts_returns_recent_non_empty_replays() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut empty_replay = record(10, ClearType::Normal);
        empty_replay.chart_sha256 = [1; 32];
        empty_replay.played_at = 1_700_000_000;
        db.insert_score(&empty_replay).unwrap();
        for index in 0..5 {
            let mut with_replay = record(20 + index, ClearType::Normal);
            with_replay.chart_sha256 = [1; 32];
            with_replay.played_at = 1_700_000_010 + index as i64;
            with_replay.replay_path = format!("replay/{index}.toml");
            db.insert_score(&with_replay).unwrap();
        }

        let slots = db.replay_slots_for_charts(&[[2; 32], [1; 32]]).unwrap();

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].chart_sha256, [1; 32]);
        assert_eq!(slots[0].replay_slots, [true, true, true, true]);
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

        let record =
            ScoreRecord::from_play_result(&result, 1_700_000_040, Some(123), "Hard", 0, "");

        assert_eq!(record.chart_sha256, [9; 32]);
        assert_eq!(record.played_at, 1_700_000_040);
        assert_eq!(record.clear_type, ClearType::Normal);
        assert_eq!(record.gauge_type, Some(GaugeType::Hard));
        assert_eq!(record.gauge_value, 76.5);
        assert_eq!(record.score.ex_score(), 2);
        assert!(record.autoplay);
        assert_eq!(record.gauge_option, "Hard");
        assert_eq!(record.replay_path, "");
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
}
