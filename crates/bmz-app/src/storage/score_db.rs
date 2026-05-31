use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE;
use bmz_core::clear::{ClearType, GaugeType};
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::ScoreState;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rusqlite::{Connection, OptionalExtension, params};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};
use crate::config::profile_config::ReplaySlotRule;

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
pub struct ReplaySlotRecord {
    pub chart_sha256: [u8; 32],
    pub slot: u8,
    pub rule: ReplaySlotRule,
    pub replay_path: String,
    pub played_at: i64,
    pub ex_score: u32,
    pub miss_count: u32,
    pub max_combo: u32,
    pub clear_rank: u8,
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
    /// `library.db`'s `course_scores.id` if this chart play happened as part
    /// of a course attempt, otherwise `None`.  No cross-database FK is
    /// enforced — callers can join against `library.db.course_scores` if
    /// they need the attempt details.
    pub course_score_id: Option<i64>,
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
                params![hash_to_hex(&chart_sha256)],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn best_ghost(&self, chart_sha256: [u8; 32], total_notes: u32) -> Result<Option<Vec<u8>>> {
        let Some(ghost) = self
            .conn
            .query_row(
                "SELECT ghost FROM score_best WHERE chart_sha256 = ?1",
                params![hash_to_hex(&chart_sha256)],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        else {
            return Ok(None);
        };
        if ghost.is_empty() {
            return Ok(None);
        }
        decode_beatoraja_ghost(&ghost, total_notes).map(Some)
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
                .query_row(params![hash_to_hex(sha256)], best_score_summary_from_row)
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
        let mut stmt =
            self.conn.prepare("SELECT slot FROM replay_slots WHERE chart_sha256 = ?1")?;

        for sha256 in chart_sha256s {
            let slots: Vec<u8> = stmt
                .query_map(params![hash_to_hex(sha256)], |row| row.get::<_, u8>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if slots.is_empty() {
                continue;
            }
            let mut replay_slots = [false; 4];
            for slot in slots {
                if (slot as usize) < replay_slots.len() {
                    replay_slots[slot as usize] = true;
                }
            }
            out.push(ReplaySlotSummary { chart_sha256: *sha256, replay_slots });
        }

        Ok(out)
    }

    pub fn replay_slot(
        &self,
        chart_sha256: [u8; 32],
        slot: u8,
    ) -> Result<Option<ReplaySlotRecord>> {
        self.conn
            .query_row(
                "SELECT chart_sha256, slot, rule, replay_path, played_at, ex_score, miss_count, max_combo, clear_rank
                 FROM replay_slots
                 WHERE chart_sha256 = ?1 AND slot = ?2",
                params![hash_to_hex(&chart_sha256), slot],
                replay_slot_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replay_slots_for_chart(
        &self,
        chart_sha256: [u8; 32],
    ) -> Result<[Option<ReplaySlotRecord>; 4]> {
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256, slot, rule, replay_path, played_at, ex_score, miss_count, max_combo, clear_rank
             FROM replay_slots
             WHERE chart_sha256 = ?1",
        )?;
        let rows = stmt
            .query_map(params![hash_to_hex(&chart_sha256)], replay_slot_record_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out: [Option<ReplaySlotRecord>; 4] = [None, None, None, None];
        for record in rows {
            let slot = record.slot as usize;
            if slot < out.len() {
                out[slot] = Some(record);
            }
        }
        Ok(out)
    }

    pub fn upsert_replay_slot(&mut self, record: &ReplaySlotRecord) -> Result<()> {
        if record.slot > 3 {
            bail!("replay slot must be in 0..=3 (got {})", record.slot);
        }
        self.conn.execute(
            "INSERT INTO replay_slots (
                chart_sha256, slot, rule, replay_path, played_at,
                ex_score, miss_count, max_combo, clear_rank
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(chart_sha256, slot) DO UPDATE SET
                rule = excluded.rule,
                replay_path = excluded.replay_path,
                played_at = excluded.played_at,
                ex_score = excluded.ex_score,
                miss_count = excluded.miss_count,
                max_combo = excluded.max_combo,
                clear_rank = excluded.clear_rank",
            params![
                hash_to_hex(&record.chart_sha256),
                record.slot,
                record.rule.as_str(),
                record.replay_path,
                record.played_at,
                record.ex_score,
                record.miss_count,
                record.max_combo,
                record.clear_rank,
            ],
        )?;
        Ok(())
    }

    /// Tag the given `score_history` rows with a course attempt id.
    ///
    /// `course_score_id` references `library.db`'s `course_scores.id`.  No FK
    /// is enforced because the two databases are separate; the caller is
    /// responsible for passing a real id.
    pub fn tag_score_history_with_course(
        &mut self,
        score_history_ids: &[i64],
        course_score_id: i64,
    ) -> Result<usize> {
        if score_history_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut total = 0_usize;
        {
            let mut stmt =
                tx.prepare("UPDATE score_history SET course_score_id = ?1 WHERE id = ?2")?;
            for id in score_history_ids {
                total += stmt.execute(params![course_score_id, id])?;
            }
        }
        tx.commit()?;
        Ok(total)
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
                replay_path,
                course_score_id
            FROM score_history
            ORDER BY played_at DESC, id DESC
            LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], score_history_entry_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn best_score_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BestScoreSummary> {
    let sha256_hex: String = row.get(0)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;

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

fn replay_slot_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReplaySlotRecord> {
    let sha256_hex: String = row.get(0)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let rule_str: String = row.get(2)?;
    let rule = ReplaySlotRule::from_str_opt(&rule_str).unwrap_or(ReplaySlotRule::Always);

    Ok(ReplaySlotRecord {
        chart_sha256,
        slot: row.get(1)?,
        rule,
        replay_path: row.get(3)?,
        played_at: row.get(4)?,
        ex_score: row.get(5)?,
        miss_count: row.get(6)?,
        max_combo: row.get(7)?,
        clear_rank: row.get(8)?,
    })
}

fn score_history_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScoreHistoryEntry> {
    let sha256_hex: String = row.get(1)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;

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
        course_score_id: row.get(11)?,
    })
}

fn insert_score_history(conn: &Connection, record: &ScoreRecord) -> Result<()> {
    let judges = &record.score.judges;
    let ghost = encode_beatoraja_ghost(&record.score.ghost)?;
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
            replay_path,
            ghost
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26
        )",
        params![
            hash_to_hex(&record.chart_sha256),
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
            ghost,
        ],
    )?;
    Ok(())
}

fn upsert_score_best(conn: &Connection, record: &ScoreRecord) -> Result<()> {
    let judges = &record.score.judges;
    let ghost = encode_beatoraja_ghost(&record.score.ghost)?;
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
            replay_path,
            ghost
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
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
            replay_path = excluded.replay_path,
            ghost = excluded.ghost
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
            hash_to_hex(&record.chart_sha256),
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
            ghost,
        ],
    )?;
    Ok(())
}

fn gauge_type_str(gauge_type: Option<GaugeType>) -> &'static str {
    gauge_type.map(GaugeType::as_str).unwrap_or("")
}

pub fn encode_beatoraja_ghost(ghost: &[u8]) -> Result<String> {
    if ghost.is_empty() {
        return Ok(String::new());
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(ghost)?;
    Ok(URL_SAFE.encode(encoder.finish()?))
}

pub fn decode_beatoraja_ghost(encoded: &str, total_notes: u32) -> Result<Vec<u8>> {
    let expected_len = total_notes as usize;
    if encoded.is_empty() {
        return Ok(vec![4; expected_len]);
    }

    let compressed = URL_SAFE.decode(encoded)?;
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut decoded = Vec::with_capacity(expected_len);
    decoder.read_to_end(&mut decoded)?;
    if decoded.len() < expected_len {
        decoded.resize(expected_len, 4);
    } else if decoded.len() > expected_len {
        decoded.truncate(expected_len);
    }
    Ok(decoded)
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
    fn beatoraja_ghost_round_trips_as_gzip_urlsafe_base64() {
        let ghost = vec![0, 1, 2, 3, 4];

        let encoded = encode_beatoraja_ghost(&ghost).unwrap();
        let decoded = decode_beatoraja_ghost(&encoded, ghost.len() as u32).unwrap();

        assert_eq!(decoded, ghost);
    }

    #[test]
    fn insert_score_persists_best_ghost_for_current_best() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        db.insert_score(&record(10, ClearType::Hard)).unwrap();

        assert_eq!(db.best_ghost([7; 32], 10).unwrap(), Some(vec![0; 10]));
    }

    #[test]
    fn class_gauge_types_round_trip_via_score_history_and_best() {
        // 段位ゲージで終わったプレイが score_history / score_best 経由で
        // `"Class" / "ExClass" / "ExHardClass"` の文字列として正しく永続化・
        // 復元されることを担保する。
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let cases = [
            ([10u8; 32], GaugeType::Class, "Class"),
            ([11u8; 32], GaugeType::ExClass, "ExClass"),
            ([12u8; 32], GaugeType::ExHardClass, "ExHardClass"),
        ];

        // ex_score は (sha[0], 段位ごと) で順に上げ、score_best が上書きされて
        // 残ることを保証する。
        for (i, (sha, gauge, _)) in cases.iter().enumerate() {
            let mut rec = record(20 + i as u32 * 10, ClearType::Hard);
            rec.chart_sha256 = *sha;
            rec.gauge_type = Some(*gauge);
            rec.gauge_value = 42.0 + i as f32;
            db.insert_score(&rec).unwrap();
        }

        // score_history: GaugeType::as_str() の文字列で素直に入る。
        let history = db.recent_history(10, 0).unwrap();
        let mut history_map: std::collections::HashMap<[u8; 32], String> =
            history.into_iter().map(|entry| (entry.chart_sha256, entry.gauge_type)).collect();
        for (sha, _, expected) in &cases {
            assert_eq!(history_map.remove(sha).as_deref(), Some(*expected), "history {sha:?}");
        }

        // score_best: 同じく文字列でラウンドトリップ、gauge_value も保持される。
        let shas: Vec<_> = cases.iter().map(|(sha, _, _)| *sha).collect();
        let best = db.best_scores_for_charts(&shas).unwrap();
        assert_eq!(best.len(), 3);
        let mut by_sha: std::collections::HashMap<_, _> =
            best.into_iter().map(|s| (s.chart_sha256, s)).collect();
        for (i, (sha, _, expected_label)) in cases.iter().enumerate() {
            let summary = by_sha.remove(sha).expect("best entry exists");
            assert_eq!(summary.gauge_type, *expected_label);
            assert_eq!(summary.gauge_value, 42.0 + i as f32);
        }
    }

    #[test]
    fn gauge_type_str_matches_enum_display_for_class_gauges() {
        assert_eq!(gauge_type_str(Some(GaugeType::Class)), "Class");
        assert_eq!(gauge_type_str(Some(GaugeType::ExClass)), "ExClass");
        assert_eq!(gauge_type_str(Some(GaugeType::ExHardClass)), "ExHardClass");
        // sanity: 非段位ゲージも従来通り。
        assert_eq!(gauge_type_str(Some(GaugeType::Normal)), "Normal");
        assert_eq!(gauge_type_str(None), "");
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

    fn sample_slot(slot: u8, ex_score: u32) -> ReplaySlotRecord {
        ReplaySlotRecord {
            chart_sha256: [1; 32],
            slot,
            rule: ReplaySlotRule::Always,
            replay_path: format!("replay/{slot}.toml"),
            played_at: 1_700_000_000 + slot as i64,
            ex_score,
            miss_count: 0,
            max_combo: ex_score,
            clear_rank: ClearType::Normal as u8,
        }
    }

    #[test]
    fn replay_slots_for_charts_reports_slot_presence_from_new_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
        db.upsert_replay_slot(&sample_slot(2, 30)).unwrap();

        let slots = db.replay_slots_for_charts(&[[2; 32], [1; 32]]).unwrap();

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].chart_sha256, [1; 32]);
        assert_eq!(slots[0].replay_slots, [true, false, true, false]);
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

        let record = db.replay_slot([1; 32], 0).unwrap().unwrap();
        assert_eq!(record.ex_score, 99);
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

        let slots = db.replay_slots_for_chart([1; 32]).unwrap();

        assert!(slots[0].is_some());
        assert!(slots[1].is_none());
        assert!(slots[2].is_none());
        assert_eq!(slots[3].as_ref().unwrap().ex_score, 30);
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

        // Tag the first two with course_score_id=99, leave r3 untouched.
        let updated = db.tag_score_history_with_course(&[id1, id2], 99).unwrap();
        assert_eq!(updated, 2);

        let rows: Vec<(i64, Option<i64>)> = db
            .conn()
            .prepare("SELECT id, course_score_id FROM score_history ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![(id1, Some(99)), (id2, Some(99)), (id3, None)]);
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

        // Tag the course-attempt row only.
        db.tag_score_history_with_course(&[course_play_id], 77).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        let by_id: std::collections::HashMap<i64, &ScoreHistoryEntry> =
            history.iter().map(|h| (h.id, h)).collect();
        assert_eq!(by_id.get(&solo_id).unwrap().course_score_id, None);
        assert_eq!(by_id.get(&course_play_id).unwrap().course_score_id, Some(77));
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
}
