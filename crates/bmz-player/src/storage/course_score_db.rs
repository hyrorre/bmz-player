use anyhow::Result;
use bmz_core::clear::ClearType;
use bmz_core::course::CourseTrophy;
use bmz_gameplay::rule::RuleMode;
use bmz_render::snapshot::{DisplayJudgeCounts, FastSlowJudgeCounts};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::ln_policy::LnScorePolicy;

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
    pub ln_policy: LnScorePolicy,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseReplaySlotSource {
    pub course_score_id: i64,
    pub source_kind: String,
    pub source_path: String,
    pub source_fingerprint: String,
    pub replay_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseReplaySlotRecord {
    pub course_hash: String,
    pub ln_policy: LnScorePolicy,
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
    pub last_played_at: Option<i64>,
    pub course_score_id: i64,
    pub course_hash: String,
    pub ln_policy: LnScorePolicy,
    pub rule_mode: RuleMode,
    pub ex_score: u32,
    pub max_ex_score: u32,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub max_combo: u32,
    pub bp: u32,
    pub cb: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
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
    pub ln_policy: LnScorePolicy,
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
    let course_score_id = insert_course_score_rows(&tx, record, false, "Local", "", "")?;
    tx.commit()?;
    Ok(course_score_id)
}

fn insert_course_score_rows(
    tx: &Transaction<'_>,
    record: &CourseScoreInsert,
    replay_only: bool,
    replay_source_kind: &str,
    replay_source_path: &str,
    replay_source_fingerprint: &str,
) -> Result<i64> {
    tx.execute(
        "INSERT INTO course_scores (
            course_hash, ln_policy, rule_mode, source, course_key, title, kind, constraints_json,
            chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
            gauge_value, max_combo, bp, course_failed, course_clear, arrange,
            trophies_json, played_at, replay_only, replay_source_kind,
            replay_source_path, replay_source_fingerprint
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                   ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        params![
            record.course_hash,
            record.ln_policy.as_str(),
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
            replay_only,
            replay_source_kind,
            replay_source_path,
            replay_source_fingerprint,
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

    Ok(course_score_id)
}

pub(super) fn insert_imported_course_replay(
    conn: &mut Connection,
    record: &CourseScoreInsert,
    slot: u8,
    source_path: &str,
    source_fingerprint: &str,
) -> Result<i64> {
    if slot > 3 {
        anyhow::bail!("course replay slot must be in 0..=3 (got {slot})");
    }
    let tx = conn.transaction()?;
    let previous_import_id = tx
        .query_row(
            "SELECT slots.course_score_id
             FROM course_replay_slots slots
             JOIN course_scores scores ON scores.id = slots.course_score_id
             WHERE slots.course_hash = ?1
               AND slots.ln_policy = ?2
               AND slots.rule_mode = ?3
               AND slots.slot = ?4
               AND scores.replay_only = 1",
            params![
                record.course_hash,
                record.ln_policy.as_str(),
                record.rule_mode.as_str(),
                slot,
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let course_score_id =
        insert_course_score_rows(&tx, record, true, "Beatoraja", source_path, source_fingerprint)?;
    if !course_replay_attempt_is_complete(&tx, course_score_id)? {
        anyhow::bail!("imported course replay is incomplete");
    }
    tx.execute(
        "INSERT INTO course_replay_slots (
            course_hash, ln_policy, rule_mode, slot, rule, course_score_id,
            played_at, ex_score, bp, max_combo, clear_rank
         ) VALUES (?1, ?2, ?3, ?4, 'Always', ?5, ?6, 0, 0, 0, 0)
         ON CONFLICT(course_hash, ln_policy, rule_mode, slot) DO UPDATE SET
            rule = excluded.rule,
            course_score_id = excluded.course_score_id,
            played_at = excluded.played_at,
            ex_score = excluded.ex_score,
            bp = excluded.bp,
            max_combo = excluded.max_combo,
            clear_rank = excluded.clear_rank",
        params![
            record.course_hash,
            record.ln_policy.as_str(),
            record.rule_mode.as_str(),
            slot,
            course_score_id,
            record.played_at,
        ],
    )?;
    if let Some(previous_import_id) = previous_import_id {
        tx.execute("DELETE FROM course_scores WHERE id = ?1", params![previous_import_id])?;
    }
    tx.commit()?;
    Ok(course_score_id)
}

pub(super) fn course_replay_slot_source(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
    slot: u8,
) -> Result<Option<CourseReplaySlotSource>> {
    conn.query_row(
        "SELECT scores.id, scores.replay_source_kind, scores.replay_source_path,
                scores.replay_source_fingerprint, scores.replay_only
         FROM course_replay_slots slots
         JOIN course_scores scores ON scores.id = slots.course_score_id
         WHERE slots.course_hash = ?1
           AND slots.ln_policy = ?2
           AND slots.rule_mode = ?3
           AND slots.slot = ?4",
        params![course_hash, ln_policy.as_str(), rule_mode.as_str(), slot],
        |row| {
            Ok(CourseReplaySlotSource {
                course_score_id: row.get(0)?,
                source_kind: row.get(1)?,
                source_path: row.get(2)?,
                source_fingerprint: row.get(3)?,
                replay_only: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn best_course_score(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
) -> Result<Option<CourseBestScore>> {
    let mut best = conn
        .query_row(
        "SELECT cs.id, cs.course_hash, cs.ln_policy, cs.rule_mode, cs.ex_score, cs.max_ex_score, cs.clear_type, cs.gauge_type,
                cs.gauge_value, cs.max_combo, cs.bp,
                COALESCE((SELECT SUM(sh.cb) FROM score_history sh WHERE sh.course_score_id = cs.id), 0),
                cs.course_failed, cs.course_clear,
                (SELECT COUNT(*) FROM course_scores count_cs
                    WHERE count_cs.course_hash = cs.course_hash
                      AND count_cs.ln_policy = cs.ln_policy
                      AND count_cs.rule_mode = cs.rule_mode
                      AND count_cs.replay_only = 0),
                (SELECT COUNT(*) FROM course_scores clear_cs
                    WHERE clear_cs.course_hash = cs.course_hash
                      AND clear_cs.ln_policy = cs.ln_policy
                      AND clear_cs.rule_mode = cs.rule_mode
                      AND clear_cs.replay_only = 0
                      AND clear_cs.clear_type NOT IN ('', 'NoPlay', 'Failed')),
                cs.played_at,
                (SELECT MAX(last_cs.played_at) FROM course_scores last_cs
                 WHERE last_cs.course_hash = cs.course_hash AND last_cs.ln_policy = cs.ln_policy
                   AND last_cs.rule_mode = cs.rule_mode AND last_cs.replay_only = 0
                   AND last_cs.played_at > 0)
         FROM course_scores cs
         WHERE cs.course_hash = ?1 AND cs.ln_policy = ?2 AND cs.rule_mode = ?3
           AND cs.replay_only = 0
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
        params![course_hash, ln_policy.as_str(), rule_mode.as_str()],
        course_best_score_from_row,
    )
        .optional()?;
    hydrate_course_best_judges(conn, &mut best)?;
    Ok(best)
}

pub(super) fn best_course_clear(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
) -> Result<Option<ClearType>> {
    let value: Option<String> = conn
        .query_row(
            "SELECT clear_type
             FROM course_scores
             WHERE course_hash = ?1 AND ln_policy = ?2 AND rule_mode = ?3
               AND replay_only = 0
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
            params![course_hash, ln_policy.as_str(), rule_mode.as_str()],
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

pub(super) fn achieved_trophy_names_for_definition(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
    trophies: &[CourseTrophy],
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT ex_score, max_ex_score, bp
         FROM course_scores
         WHERE course_hash = ?1
           AND ln_policy = ?2
           AND rule_mode = ?3
           AND replay_only = 0
           AND course_clear = 1
           AND arrange IN ('Normal', 'Mirror', 'Random')",
    )?;
    let attempts = stmt
        .query_map(params![course_hash, ln_policy.as_str(), rule_mode.as_str()], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?, row.get::<_, u32>(2)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut achieved = Vec::new();
    for trophy in trophies {
        if trophy.name.is_empty() || achieved.contains(&trophy.name) {
            continue;
        }
        let matches = attempts.iter().any(|&(ex_score, max_ex_score, bp)| {
            let total_notes = max_ex_score / 2;
            if total_notes == 0 {
                return false;
            }
            let miss_rate = bp as f32 / total_notes as f32 * 100.0;
            let score_rate = ex_score as f32 / max_ex_score as f32 * 100.0;
            miss_rate <= trophy.max_miss_rate && score_rate >= trophy.min_score_rate
        });
        if matches {
            achieved.push(trophy.name.clone());
        }
    }
    Ok(achieved)
}

pub(super) fn best_course_score_for_trophy(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
    trophy_name: &str,
) -> Result<Option<CourseBestScore>> {
    let mut best = conn
        .query_row(
        "SELECT cs.id, cs.course_hash, cs.ln_policy, cs.rule_mode, cs.ex_score, cs.max_ex_score, cs.clear_type,
                cs.gauge_type, cs.gauge_value, cs.max_combo, cs.bp,
                COALESCE((SELECT SUM(sh.cb) FROM score_history sh WHERE sh.course_score_id = cs.id), 0),
                cs.course_failed, cs.course_clear,
                (SELECT COUNT(*) FROM course_scores count_cs
                    WHERE count_cs.course_hash = cs.course_hash
                      AND count_cs.ln_policy = cs.ln_policy
                      AND count_cs.rule_mode = cs.rule_mode
                      AND count_cs.replay_only = 0),
                (SELECT COUNT(*) FROM course_scores clear_cs
                    WHERE clear_cs.course_hash = cs.course_hash
                      AND clear_cs.ln_policy = cs.ln_policy
                      AND clear_cs.rule_mode = cs.rule_mode
                      AND clear_cs.replay_only = 0
                      AND clear_cs.clear_type NOT IN ('', 'NoPlay', 'Failed')),
                cs.played_at,
                (SELECT MAX(last_cs.played_at) FROM course_scores last_cs
                 WHERE last_cs.course_hash = cs.course_hash AND last_cs.ln_policy = cs.ln_policy
                   AND last_cs.rule_mode = cs.rule_mode AND last_cs.replay_only = 0
                   AND last_cs.played_at > 0)
         FROM course_scores cs
         JOIN course_trophy_achievements cta
             ON cta.course_score_id = cs.id
         WHERE cs.course_hash = ?1 AND cs.ln_policy = ?2 AND cs.rule_mode = ?3
           AND cs.replay_only = 0
           AND cta.trophy_name = ?4
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
        params![course_hash, ln_policy.as_str(), rule_mode.as_str(), trophy_name],
        course_best_score_from_row,
    )
        .optional()?;
    hydrate_course_best_judges(conn, &mut best)?;
    Ok(best)
}

fn hydrate_course_best_judges(conn: &Connection, best: &mut Option<CourseBestScore>) -> Result<()> {
    let Some(best) = best else { return Ok(()) };
    let counts = conn.query_row(
        "SELECT
            COALESCE(SUM(fast_pgreat), 0), COALESCE(SUM(slow_pgreat), 0),
            COALESCE(SUM(fast_great), 0), COALESCE(SUM(slow_great), 0),
            COALESCE(SUM(fast_good), 0), COALESCE(SUM(slow_good), 0),
            COALESCE(SUM(fast_bad), 0), COALESCE(SUM(slow_bad), 0),
            COALESCE(SUM(fast_poor), 0), COALESCE(SUM(slow_poor), 0),
            COALESCE(SUM(fast_empty_poor), 0), COALESCE(SUM(slow_empty_poor), 0)
         FROM score_history
         WHERE course_score_id = ?1",
        params![best.course_score_id],
        |row| {
            Ok(FastSlowJudgeCounts {
                fast_pgreat: row.get(0)?,
                slow_pgreat: row.get(1)?,
                fast_great: row.get(2)?,
                slow_great: row.get(3)?,
                fast_good: row.get(4)?,
                slow_good: row.get(5)?,
                fast_bad: row.get(6)?,
                slow_bad: row.get(7)?,
                fast_poor: row.get(8)?,
                slow_poor: row.get(9)?,
                fast_empty_poor: row.get(10)?,
                slow_empty_poor: row.get(11)?,
            })
        },
    )?;
    best.judge_counts = DisplayJudgeCounts {
        pgreat: counts.fast_pgreat.saturating_add(counts.slow_pgreat),
        great: counts.fast_great.saturating_add(counts.slow_great),
        good: counts.fast_good.saturating_add(counts.slow_good),
        bad: counts.fast_bad.saturating_add(counts.slow_bad),
        poor: counts.fast_poor.saturating_add(counts.slow_poor),
        empty_poor: counts.fast_empty_poor.saturating_add(counts.slow_empty_poor),
    };
    best.fast_slow_counts = counts;
    Ok(())
}

pub(super) fn list_recent_course_scores(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
    limit: u32,
    offset: u32,
) -> Result<Vec<CourseScoreEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, course_hash, ln_policy, rule_mode, source, course_key, title, kind, constraints_json,
                chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
                gauge_value, max_combo, bp, course_failed, course_clear, played_at
         FROM course_scores
         WHERE course_hash = ?1 AND ln_policy = ?2 AND rule_mode = ?3
           AND replay_only = 0
         ORDER BY played_at DESC, id DESC
         LIMIT ?4 OFFSET ?5",
    )?;
    let rows = stmt
        .query_map(
            params![course_hash, ln_policy.as_str(), rule_mode.as_str(), limit, offset],
            course_score_entry_base_from_row,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    hydrate_course_score_entries(conn, rows)
}

pub(super) fn list_recent_course_scores_all_contexts(
    conn: &Connection,
    course_hash: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<CourseScoreEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, course_hash, ln_policy, rule_mode, source, course_key, title, kind, constraints_json,
                chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
                gauge_value, max_combo, bp, course_failed, course_clear, played_at
         FROM course_scores
         WHERE course_hash = ?1 AND replay_only = 0
         ORDER BY played_at DESC, id DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt
        .query_map(params![course_hash, limit, offset], course_score_entry_base_from_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    hydrate_course_score_entries(conn, rows)
}

fn hydrate_course_score_entries(
    conn: &Connection,
    rows: Vec<CourseScoreEntry>,
) -> Result<Vec<CourseScoreEntry>> {
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
            "SELECT id, course_hash, ln_policy, rule_mode, source, course_key, title, kind, constraints_json,
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
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM course_scores
         WHERE course_hash = ?1 AND ln_policy = ?2 AND rule_mode = ?3
           AND replay_only = 0
         ORDER BY played_at DESC, id DESC
         LIMIT 1",
        params![course_hash, ln_policy.as_str(), rule_mode.as_str()],
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

pub(super) fn course_replay_attempt_is_complete(
    conn: &Connection,
    course_score_id: i64,
) -> Result<bool> {
    let complete = conn.query_row(
        "SELECT
            EXISTS (
                SELECT 1 FROM course_score_charts
                WHERE course_score_id = ?1
            )
            AND NOT EXISTS (
                SELECT 1
                FROM course_score_charts c
                WHERE c.course_score_id = ?1
                  AND NOT EXISTS (
                      SELECT 1
                      FROM course_replays r
                      WHERE r.course_score_id = c.course_score_id
                        AND r.position = c.position
                        AND r.chart_sha256 = c.chart_sha256
                        AND r.replay_path <> ''
                  )
            )
            AND NOT EXISTS (
                SELECT 1
                FROM course_replays r
                WHERE r.course_score_id = ?1
                  AND NOT EXISTS (
                      SELECT 1
                      FROM course_score_charts c
                      WHERE c.course_score_id = r.course_score_id
                        AND c.position = r.position
                        AND c.chart_sha256 = r.chart_sha256
                  )
            )",
        params![course_score_id],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(complete)
}

pub(super) fn upsert_course_replay_slot(
    conn: &mut Connection,
    record: &CourseReplaySlotRecord,
) -> Result<()> {
    if record.slot > 3 {
        anyhow::bail!("course replay slot must be in 0..=3 (got {})", record.slot);
    }
    if !course_replay_attempt_is_complete(conn, record.course_score_id)? {
        anyhow::bail!(
            "course replay slot requires a complete replay attempt (course_score_id={})",
            record.course_score_id
        );
    }
    conn.execute(
        "INSERT INTO course_replay_slots (
            course_hash, ln_policy, rule_mode, slot, rule, course_score_id,
            played_at, ex_score, bp, max_combo, clear_rank
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(course_hash, ln_policy, rule_mode, slot) DO UPDATE SET
            rule = excluded.rule,
            course_score_id = excluded.course_score_id,
            played_at = excluded.played_at,
            ex_score = excluded.ex_score,
            bp = excluded.bp,
            max_combo = excluded.max_combo,
            clear_rank = excluded.clear_rank",
        params![
            record.course_hash,
            record.ln_policy.as_str(),
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
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
    slot: u8,
) -> Result<Option<CourseReplaySlotRecord>> {
    let record = conn
        .query_row(
            "SELECT course_hash, ln_policy, rule_mode, slot, rule, course_score_id, played_at,
                ex_score, bp, max_combo, clear_rank
         FROM course_replay_slots
         WHERE course_hash = ?1 AND ln_policy = ?2 AND rule_mode = ?3 AND slot = ?4",
            params![course_hash, ln_policy.as_str(), rule_mode.as_str(), slot],
            course_replay_slot_from_row,
        )
        .optional()?;
    match record {
        Some(record) if course_replay_attempt_is_complete(conn, record.course_score_id)? => {
            Ok(Some(record))
        }
        Some(_) | None => Ok(None),
    }
}

pub(super) fn course_replay_slots_for_course(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
) -> Result<[Option<CourseReplaySlotRecord>; 4]> {
    let mut stmt = conn.prepare(
        "SELECT course_hash, ln_policy, rule_mode, slot, rule, course_score_id, played_at,
                ex_score, bp, max_combo, clear_rank
         FROM course_replay_slots
         WHERE course_hash = ?1 AND ln_policy = ?2 AND rule_mode = ?3",
    )?;
    let rows = stmt
        .query_map(
            params![course_hash, ln_policy.as_str(), rule_mode.as_str()],
            course_replay_slot_from_row,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut out: [Option<CourseReplaySlotRecord>; 4] = [None, None, None, None];
    for record in rows {
        let idx = record.slot as usize;
        if idx < out.len() && course_replay_attempt_is_complete(conn, record.course_score_id)? {
            out[idx] = Some(record);
        }
    }
    Ok(out)
}

pub(super) fn course_replay_slot_presence(
    conn: &Connection,
    course_hash: &str,
    ln_policy: LnScorePolicy,
    rule_mode: RuleMode,
) -> Result<[bool; 4]> {
    Ok(course_replay_slots_for_course(conn, course_hash, ln_policy, rule_mode)?
        .map(|slot| slot.is_some()))
}

fn course_replay_slot_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CourseReplaySlotRecord> {
    Ok(CourseReplaySlotRecord {
        course_hash: row.get(0)?,
        ln_policy: ln_score_policy_from_row(row, 1)?,
        rule_mode: rule_mode_from_row(row, 2)?,
        slot: row.get(3)?,
        rule: row.get(4)?,
        course_score_id: row.get(5)?,
        played_at: row.get(6)?,
        ex_score: row.get(7)?,
        bp: row.get(8)?,
        max_combo: row.get(9)?,
        clear_rank: row.get(10)?,
    })
}

fn course_best_score_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CourseBestScore> {
    Ok(CourseBestScore {
        last_played_at: row.get(17)?,
        course_score_id: row.get(0)?,
        course_hash: row.get(1)?,
        ln_policy: ln_score_policy_from_row(row, 2)?,
        rule_mode: rule_mode_from_row(row, 3)?,
        ex_score: row.get(4)?,
        max_ex_score: row.get(5)?,
        clear_type: row.get(6)?,
        gauge_type: row.get(7)?,
        gauge_value: row.get(8)?,
        max_combo: row.get(9)?,
        bp: row.get(10)?,
        cb: row.get(11)?,
        judge_counts: DisplayJudgeCounts::default(),
        fast_slow_counts: FastSlowJudgeCounts::default(),
        course_failed: row.get(12)?,
        course_clear: row.get(13)?,
        play_count: row.get(14)?,
        clear_count: row.get(15)?,
        played_at: row.get(16)?,
    })
}

fn course_score_entry_base_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CourseScoreEntry> {
    Ok(CourseScoreEntry {
        course_score_id: row.get(0)?,
        course_hash: row.get(1)?,
        ln_policy: ln_score_policy_from_row(row, 2)?,
        rule_mode: rule_mode_from_row(row, 3)?,
        source: row.get(4)?,
        course_key: row.get(5)?,
        title: row.get(6)?,
        kind: row.get(7)?,
        constraints_json: row.get(8)?,
        chart_sha256s_json: row.get(9)?,
        ex_score: row.get(10)?,
        max_ex_score: row.get(11)?,
        clear_type: row.get(12)?,
        gauge_type: row.get(13)?,
        gauge_value: row.get(14)?,
        max_combo: row.get(15)?,
        bp: row.get(16)?,
        course_failed: row.get(17)?,
        course_clear: row.get(18)?,
        played_at: row.get(19)?,
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

fn ln_score_policy_from_row(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<LnScorePolicy> {
    let value: String = row.get(index)?;
    LnScorePolicy::from_str_opt(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid LN score policy: {value}").into(),
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
#[path = "course_score_db/tests.rs"]
mod tests;
