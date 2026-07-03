use anyhow::Result;
use bmz_core::clear::ClearType;
use bmz_gameplay::rule::RuleMode;
use rusqlite::{Connection, OptionalExtension, params};

use super::common::{hash_to_hex, hex_to_hash};

#[derive(Debug, Clone, PartialEq)]
pub struct CourseScoreChartRecord {
    pub position: i64,
    pub chart_sha256: [u8; 32],
    pub ex_score: u32,
    pub max_combo: u32,
    pub clear_type: String,
    pub gauge_value: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseReplayRecord {
    pub position: i64,
    pub chart_sha256: [u8; 32],
    pub replay_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseScoreInsert {
    pub course_hash: String,
    pub rule_mode: RuleMode,
    pub source: String,
    pub course_key: String,
    pub title: String,
    pub kind: String,
    pub constraints_json: String,
    pub chart_sha256s_json: String,
    pub ex_score: u32,
    pub max_ex_score: u32,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub max_combo: u32,
    pub bp: u32,
    pub course_failed: bool,
    pub course_clear: bool,
    pub arrange: String,
    pub trophies_json: String,
    pub played_at: i64,
    pub charts: Vec<CourseScoreChartRecord>,
    pub replays: Vec<CourseReplayRecord>,
    pub achieved_trophies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseReplaySlotRecord {
    pub course_hash: String,
    pub rule_mode: RuleMode,
    pub slot: u8,
    pub rule: String,
    pub course_score_id: i64,
    pub played_at: i64,
    pub ex_score: u32,
    pub bp: u32,
    pub max_combo: u32,
    pub clear_rank: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseBestScore {
    pub course_score_id: i64,
    pub course_hash: String,
    pub rule_mode: RuleMode,
    pub ex_score: u32,
    pub max_ex_score: u32,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub max_combo: u32,
    pub bp: u32,
    pub cb: u32,
    pub course_failed: bool,
    pub course_clear: bool,
    pub play_count: u32,
    pub clear_count: u32,
    pub played_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseScoreEntry {
    pub course_score_id: i64,
    pub course_hash: String,
    pub rule_mode: RuleMode,
    pub source: String,
    pub course_key: String,
    pub title: String,
    pub kind: String,
    pub constraints_json: String,
    pub chart_sha256s_json: String,
    pub ex_score: u32,
    pub max_ex_score: u32,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub max_combo: u32,
    pub bp: u32,
    pub course_failed: bool,
    pub course_clear: bool,
    pub played_at: i64,
    pub achieved_trophies: Vec<String>,
}

pub(super) fn insert_course_score(
    conn: &mut Connection,
    record: &CourseScoreInsert,
) -> Result<i64> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO course_scores (
            course_hash, rule_mode, source, course_key, title, kind, constraints_json,
            chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
            gauge_value, max_combo, bp, course_failed, course_clear, arrange,
            trophies_json, played_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                   ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            record.course_hash,
            record.rule_mode.as_str(),
            record.source,
            record.course_key,
            record.title,
            record.kind,
            record.constraints_json,
            record.chart_sha256s_json,
            record.ex_score,
            record.max_ex_score,
            record.clear_type,
            record.gauge_type,
            record.gauge_value,
            record.max_combo,
            record.bp,
            record.course_failed,
            record.course_clear,
            record.arrange,
            record.trophies_json,
            record.played_at,
        ],
    )?;
    let course_score_id = tx.last_insert_rowid();

    for chart in &record.charts {
        tx.execute(
            "INSERT INTO course_score_charts (
                course_score_id, position, chart_sha256, ex_score, max_combo,
                clear_type, gauge_value
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                course_score_id,
                chart.position,
                hash_to_hex(&chart.chart_sha256),
                chart.ex_score,
                chart.max_combo,
                chart.clear_type,
                chart.gauge_value,
            ],
        )?;
    }

    for replay in &record.replays {
        if replay.replay_path.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO course_replays (
                course_score_id, position, chart_sha256, replay_path
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                course_score_id,
                replay.position,
                hash_to_hex(&replay.chart_sha256),
                replay.replay_path,
            ],
        )?;
    }

    for trophy_name in &record.achieved_trophies {
        if trophy_name.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO course_trophy_achievements
                 (course_score_id, course_hash, trophy_name)
             VALUES (?1, ?2, ?3)",
            params![course_score_id, record.course_hash, trophy_name],
        )?;
    }

    tx.commit()?;
    Ok(course_score_id)
}

pub(super) fn best_course_score(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
) -> Result<Option<CourseBestScore>> {
    conn.query_row(
        "SELECT cs.id, cs.course_hash, cs.rule_mode, cs.ex_score, cs.max_ex_score, cs.clear_type, cs.gauge_type,
                cs.gauge_value, cs.max_combo, cs.bp,
                COALESCE((SELECT SUM(sh.cb) FROM score_history sh WHERE sh.course_score_id = cs.id), 0),
                cs.course_failed, cs.course_clear,
                (SELECT COUNT(*) FROM course_scores count_cs
                    WHERE count_cs.course_hash = cs.course_hash
                      AND count_cs.rule_mode = cs.rule_mode),
                (SELECT COUNT(*) FROM course_scores clear_cs
                    WHERE clear_cs.course_hash = cs.course_hash
                      AND clear_cs.rule_mode = cs.rule_mode
                      AND clear_cs.clear_type NOT IN ('', 'NoPlay', 'Failed')),
                cs.played_at
         FROM course_scores cs
         WHERE cs.course_hash = ?1 AND cs.rule_mode = ?2
         ORDER BY cs.ex_score DESC,
                  CASE cs.clear_type
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
                  END DESC,
                  cs.bp ASC,
                  cs.max_combo DESC,
                  cs.played_at DESC,
                  cs.id DESC
         LIMIT 1",
        params![course_hash, rule_mode.as_str()],
        course_best_score_from_row,
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn best_course_clear(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
) -> Result<Option<ClearType>> {
    let value: Option<String> = conn
        .query_row(
            "SELECT clear_type
             FROM course_scores
             WHERE course_hash = ?1 AND rule_mode = ?2
             ORDER BY CASE clear_type
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
                      END DESC
             LIMIT 1",
            params![course_hash, rule_mode.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value.and_then(|s| clear_type_from_name(&s)))
}

pub(super) fn list_course_score_charts(
    conn: &Connection,
    course_score_id: i64,
) -> Result<Vec<CourseScoreChartRecord>> {
    let mut stmt = conn.prepare(
        "SELECT position, chart_sha256, ex_score, max_combo, clear_type, gauge_value
         FROM course_score_charts
         WHERE course_score_id = ?1
         ORDER BY position",
    )?;
    let rows = stmt.query_map(params![course_score_id], |row| {
        let sha256_hex: String = row.get(1)?;
        Ok(CourseScoreChartRecord {
            position: row.get(0)?,
            chart_sha256: hex_to_hash(&sha256_hex)?,
            ex_score: row.get(2)?,
            max_combo: row.get(3)?,
            clear_type: row.get(4)?,
            gauge_value: row.get(5)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn achieved_trophy_names_for_course(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT cta.trophy_name
         FROM course_trophy_achievements cta
         JOIN course_scores cs ON cs.id = cta.course_score_id
         WHERE cta.course_hash = ?1
           AND cs.rule_mode = ?2
           AND cs.arrange IN ('Normal', 'Mirror', 'Random')
         ORDER BY cta.trophy_name",
    )?;
    let rows =
        stmt.query_map(params![course_hash, rule_mode.as_str()], |row| row.get::<_, String>(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn best_course_score_for_trophy(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
    trophy_name: &str,
) -> Result<Option<CourseBestScore>> {
    conn.query_row(
        "SELECT cs.id, cs.course_hash, cs.rule_mode, cs.ex_score, cs.max_ex_score, cs.clear_type,
                cs.gauge_type, cs.gauge_value, cs.max_combo, cs.bp,
                COALESCE((SELECT SUM(sh.cb) FROM score_history sh WHERE sh.course_score_id = cs.id), 0),
                cs.course_failed, cs.course_clear,
                (SELECT COUNT(*) FROM course_scores count_cs
                    WHERE count_cs.course_hash = cs.course_hash
                      AND count_cs.rule_mode = cs.rule_mode),
                (SELECT COUNT(*) FROM course_scores clear_cs
                    WHERE clear_cs.course_hash = cs.course_hash
                      AND clear_cs.rule_mode = cs.rule_mode
                      AND clear_cs.clear_type NOT IN ('', 'NoPlay', 'Failed')),
                cs.played_at
         FROM course_scores cs
         JOIN course_trophy_achievements cta
             ON cta.course_score_id = cs.id
         WHERE cs.course_hash = ?1 AND cs.rule_mode = ?2 AND cta.trophy_name = ?3
         ORDER BY cs.ex_score DESC,
                  CASE cs.clear_type
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
                  END DESC,
                  cs.bp ASC,
                  cs.max_combo DESC,
                  cs.played_at DESC,
                  cs.id DESC
         LIMIT 1",
        params![course_hash, rule_mode.as_str(), trophy_name],
        course_best_score_from_row,
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn list_recent_course_scores(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
    limit: u32,
    offset: u32,
) -> Result<Vec<CourseScoreEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, course_hash, rule_mode, source, course_key, title, kind, constraints_json,
                chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
                gauge_value, max_combo, bp, course_failed, course_clear, played_at
         FROM course_scores
         WHERE course_hash = ?1 AND rule_mode = ?2
         ORDER BY played_at DESC, id DESC
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt
        .query_map(
            params![course_hash, rule_mode.as_str(), limit, offset],
            course_score_entry_base_from_row,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut trophy_stmt = conn.prepare(
        "SELECT trophy_name
         FROM course_trophy_achievements
         WHERE course_score_id = ?1
         ORDER BY trophy_name",
    )?;
    let mut out = Vec::with_capacity(rows.len());
    for mut entry in rows {
        entry.achieved_trophies = trophy_stmt
            .query_map(params![entry.course_score_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        out.push(entry);
    }
    Ok(out)
}

pub(super) fn course_score_entry_by_id(
    conn: &Connection,
    course_score_id: i64,
) -> Result<Option<CourseScoreEntry>> {
    let Some(mut entry) = conn
        .query_row(
            "SELECT id, course_hash, rule_mode, source, course_key, title, kind, constraints_json,
                    chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
                    gauge_value, max_combo, bp, course_failed, course_clear, played_at
             FROM course_scores
             WHERE id = ?1",
            params![course_score_id],
            course_score_entry_base_from_row,
        )
        .optional()?
    else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT trophy_name
         FROM course_trophy_achievements
         WHERE course_score_id = ?1
         ORDER BY trophy_name",
    )?;
    entry.achieved_trophies = stmt
        .query_map(params![course_score_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(Some(entry))
}

pub(super) fn latest_course_score_id(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM course_scores
         WHERE course_hash = ?1 AND rule_mode = ?2
         ORDER BY played_at DESC, id DESC
         LIMIT 1",
        params![course_hash, rule_mode.as_str()],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn list_course_replays(
    conn: &Connection,
    course_score_id: i64,
) -> Result<Vec<CourseReplayRecord>> {
    let mut stmt = conn.prepare(
        "SELECT position, chart_sha256, replay_path
         FROM course_replays
         WHERE course_score_id = ?1
         ORDER BY position",
    )?;
    let rows = stmt.query_map(params![course_score_id], |row| {
        let sha256_hex: String = row.get(1)?;
        Ok(CourseReplayRecord {
            position: row.get(0)?,
            chart_sha256: hex_to_hash(&sha256_hex)?,
            replay_path: row.get(2)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn upsert_course_replay_slot(
    conn: &mut Connection,
    record: &CourseReplaySlotRecord,
) -> Result<()> {
    if record.slot > 3 {
        anyhow::bail!("course replay slot must be in 0..=3 (got {})", record.slot);
    }
    conn.execute(
        "INSERT INTO course_replay_slots (
            course_hash, rule_mode, slot, rule, course_score_id, played_at,
            ex_score, bp, max_combo, clear_rank
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(course_hash, rule_mode, slot) DO UPDATE SET
            rule = excluded.rule,
            course_score_id = excluded.course_score_id,
            played_at = excluded.played_at,
            ex_score = excluded.ex_score,
            bp = excluded.bp,
            max_combo = excluded.max_combo,
            clear_rank = excluded.clear_rank",
        params![
            record.course_hash,
            record.rule_mode.as_str(),
            record.slot,
            record.rule,
            record.course_score_id,
            record.played_at,
            record.ex_score,
            record.bp,
            record.max_combo,
            record.clear_rank,
        ],
    )?;
    Ok(())
}

pub(super) fn course_replay_slot(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
    slot: u8,
) -> Result<Option<CourseReplaySlotRecord>> {
    conn.query_row(
        "SELECT course_hash, rule_mode, slot, rule, course_score_id, played_at,
                ex_score, bp, max_combo, clear_rank
         FROM course_replay_slots
         WHERE course_hash = ?1 AND rule_mode = ?2 AND slot = ?3",
        params![course_hash, rule_mode.as_str(), slot],
        course_replay_slot_from_row,
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn course_replay_slots_for_course(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
) -> Result<[Option<CourseReplaySlotRecord>; 4]> {
    let mut stmt = conn.prepare(
        "SELECT course_hash, rule_mode, slot, rule, course_score_id, played_at,
                ex_score, bp, max_combo, clear_rank
         FROM course_replay_slots
         WHERE course_hash = ?1 AND rule_mode = ?2",
    )?;
    let rows = stmt
        .query_map(params![course_hash, rule_mode.as_str()], course_replay_slot_from_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut out: [Option<CourseReplaySlotRecord>; 4] = [None, None, None, None];
    for record in rows {
        let idx = record.slot as usize;
        if idx < out.len() {
            out[idx] = Some(record);
        }
    }
    Ok(out)
}

pub(super) fn course_replay_slot_presence(
    conn: &Connection,
    course_hash: &str,
    rule_mode: RuleMode,
) -> Result<[bool; 4]> {
    let mut stmt = conn.prepare(
        "SELECT slot FROM course_replay_slots WHERE course_hash = ?1 AND rule_mode = ?2",
    )?;
    let mut out = [false; 4];
    let rows =
        stmt.query_map(params![course_hash, rule_mode.as_str()], |row| row.get::<_, u8>(0))?;
    for row in rows {
        let slot = row? as usize;
        if slot < out.len() {
            out[slot] = true;
        }
    }
    Ok(out)
}

fn course_replay_slot_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CourseReplaySlotRecord> {
    Ok(CourseReplaySlotRecord {
        course_hash: row.get(0)?,
        rule_mode: rule_mode_from_row(row, 1)?,
        slot: row.get(2)?,
        rule: row.get(3)?,
        course_score_id: row.get(4)?,
        played_at: row.get(5)?,
        ex_score: row.get(6)?,
        bp: row.get(7)?,
        max_combo: row.get(8)?,
        clear_rank: row.get(9)?,
    })
}

fn course_best_score_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CourseBestScore> {
    Ok(CourseBestScore {
        course_score_id: row.get(0)?,
        course_hash: row.get(1)?,
        rule_mode: rule_mode_from_row(row, 2)?,
        ex_score: row.get(3)?,
        max_ex_score: row.get(4)?,
        clear_type: row.get(5)?,
        gauge_type: row.get(6)?,
        gauge_value: row.get(7)?,
        max_combo: row.get(8)?,
        bp: row.get(9)?,
        cb: row.get(10)?,
        course_failed: row.get(11)?,
        course_clear: row.get(12)?,
        play_count: row.get(13)?,
        clear_count: row.get(14)?,
        played_at: row.get(15)?,
    })
}

fn course_score_entry_base_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CourseScoreEntry> {
    Ok(CourseScoreEntry {
        course_score_id: row.get(0)?,
        course_hash: row.get(1)?,
        rule_mode: rule_mode_from_row(row, 2)?,
        source: row.get(3)?,
        course_key: row.get(4)?,
        title: row.get(5)?,
        kind: row.get(6)?,
        constraints_json: row.get(7)?,
        chart_sha256s_json: row.get(8)?,
        ex_score: row.get(9)?,
        max_ex_score: row.get(10)?,
        clear_type: row.get(11)?,
        gauge_type: row.get(12)?,
        gauge_value: row.get(13)?,
        max_combo: row.get(14)?,
        bp: row.get(15)?,
        course_failed: row.get(16)?,
        course_clear: row.get(17)?,
        played_at: row.get(18)?,
        achieved_trophies: Vec::new(),
    })
}

fn rule_mode_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<RuleMode> {
    let value: String = row.get(index)?;
    RuleMode::from_str_opt(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid rule mode: {value}").into(),
        )
    })
}

fn clear_type_from_name(name: &str) -> Option<ClearType> {
    match name {
        "NoPlay" => Some(ClearType::NoPlay),
        "Failed" => Some(ClearType::Failed),
        "AssistEasy" => Some(ClearType::AssistEasy),
        "LightAssistEasy" => Some(ClearType::LightAssistEasy),
        "Easy" => Some(ClearType::Easy),
        "Normal" => Some(ClearType::Normal),
        "Hard" => Some(ClearType::Hard),
        "ExHard" => Some(ClearType::ExHard),
        "FullCombo" => Some(ClearType::FullCombo),
        "Perfect" => Some(ClearType::Perfect),
        "Max" => Some(ClearType::Max),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

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

        let best = best_course_score(&conn, "course-a", RuleMode::Beatoraja).unwrap().unwrap();
        assert_eq!(best.course_score_id, score_id);
        assert_eq!(best.course_hash, "course-a");
        assert_eq!(best.ex_score, 500);
        assert_eq!(best.cb, 10);
        assert_eq!(best.play_count, 1);
        assert_eq!(best.clear_count, 1);

        let charts = list_course_score_charts(&conn, score_id).unwrap();
        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].chart_sha256, [1; 32]);

        let replays = list_course_replays(&conn, score_id).unwrap();
        assert_eq!(replays.len(), 1);
        assert_eq!(replays[0].replay_path, "replay/course.toml");

        assert_eq!(
            achieved_trophy_names_for_course(&conn, "course-a", RuleMode::Beatoraja).unwrap(),
            vec!["gold".to_string()]
        );
        let entry = course_score_entry_by_id(&conn, score_id).unwrap().unwrap();
        assert_eq!(entry.title, "Dan 1");
        assert_eq!(entry.achieved_trophies, vec!["gold".to_string()]);
    }

    #[test]
    fn best_and_latest_are_scoped_by_course_hash() {
        let mut conn = open_conn();
        insert_course_score(&mut conn, &sample_score("course-a", 400, "Hard", 10)).unwrap();
        let newer =
            insert_course_score(&mut conn, &sample_score("course-a", 300, "Normal", 20)).unwrap();
        insert_course_score(&mut conn, &sample_score("course-b", 900, "Normal", 30)).unwrap();

        let best = best_course_score(&conn, "course-a", RuleMode::Beatoraja).unwrap().unwrap();
        assert_eq!(best.ex_score, 400);
        assert_eq!(best.play_count, 2);
        assert_eq!(best.clear_count, 2);
        assert_eq!(
            latest_course_score_id(&conn, "course-a", RuleMode::Beatoraja).unwrap(),
            Some(newer)
        );
        assert_eq!(
            best_course_clear(&conn, "course-a", RuleMode::Beatoraja).unwrap(),
            Some(ClearType::Hard)
        );
    }

    #[test]
    fn replay_slots_are_keyed_by_course_hash() {
        let mut conn = open_conn();
        let score_id =
            insert_course_score(&mut conn, &sample_score("course-a", 500, "Normal", 10)).unwrap();
        upsert_course_replay_slot(&mut conn, &sample_slot("course-a", 0, score_id, 500)).unwrap();
        upsert_course_replay_slot(&mut conn, &sample_slot("course-a", 3, score_id, 700)).unwrap();

        let slot = course_replay_slot(&conn, "course-a", RuleMode::Beatoraja, 3).unwrap().unwrap();
        assert_eq!(slot.ex_score, 700);
        assert_eq!(
            course_replay_slot_presence(&conn, "course-a", RuleMode::Beatoraja).unwrap(),
            [true, false, false, true]
        );
        assert_eq!(
            course_replay_slot_presence(&conn, "course-b", RuleMode::Beatoraja).unwrap(),
            [false; 4]
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

        let beatoraja = best_course_score(&conn, "course-a", RuleMode::Beatoraja).unwrap().unwrap();
        let dx = best_course_score(&conn, "course-a", RuleMode::Dx).unwrap().unwrap();

        assert_eq!(beatoraja.course_score_id, beatoraja_id);
        assert_eq!(beatoraja.ex_score, 400);
        assert_eq!(dx.course_score_id, dx_id);
        assert_eq!(dx.ex_score, 900);
        assert_eq!(
            latest_course_score_id(&conn, "course-a", RuleMode::Beatoraja).unwrap(),
            Some(beatoraja_id)
        );
        assert_eq!(latest_course_score_id(&conn, "course-a", RuleMode::Dx).unwrap(), Some(dx_id));
    }

    #[test]
    fn course_replay_slots_are_separate_per_rule_mode() {
        let mut conn = open_conn();
        let beatoraja_id =
            insert_course_score(&mut conn, &sample_score("course-a", 500, "Normal", 10)).unwrap();
        let mut dx = sample_score("course-a", 700, "Normal", 20);
        dx.rule_mode = RuleMode::Dx;
        let dx_id = insert_course_score(&mut conn, &dx).unwrap();
        upsert_course_replay_slot(&mut conn, &sample_slot("course-a", 0, beatoraja_id, 500))
            .unwrap();
        let mut dx_slot = sample_slot("course-a", 0, dx_id, 700);
        dx_slot.rule_mode = RuleMode::Dx;
        upsert_course_replay_slot(&mut conn, &dx_slot).unwrap();

        let beatoraja =
            course_replay_slot(&conn, "course-a", RuleMode::Beatoraja, 0).unwrap().unwrap();
        let dx = course_replay_slot(&conn, "course-a", RuleMode::Dx, 0).unwrap().unwrap();

        assert_eq!(beatoraja.course_score_id, beatoraja_id);
        assert_eq!(beatoraja.ex_score, 500);
        assert_eq!(dx.course_score_id, dx_id);
        assert_eq!(dx.ex_score, 700);
        assert_eq!(
            course_replay_slot_presence(&conn, "course-a", RuleMode::Beatoraja).unwrap(),
            [true, false, false, false]
        );
        assert_eq!(
            course_replay_slot_presence(&conn, "course-a", RuleMode::Dx).unwrap(),
            [true, false, false, false]
        );
    }
}
