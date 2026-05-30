use anyhow::Result;
use bmz_core::clear::ClearType;
use bmz_core::course::{CourseConstraints, CourseDefinition, CourseEntry, CourseTrophy};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, PartialEq)]
pub struct StoredCourseEntry {
    pub position: usize,
    pub entry: CourseEntry,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredCourse {
    pub id: i64,
    pub source: String,
    pub definition: CourseDefinition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseScoreChartRecord {
    pub position: i64,
    pub chart_id: i64,
    pub ex_score: u32,
    pub max_combo: u32,
    pub clear_type: String,
    pub gauge_value: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseReplayRecord {
    pub position: i64,
    pub chart_id: i64,
    pub replay_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseScoreInsert {
    pub course_id: i64,
    pub ex_score: u32,
    pub max_ex_score: u32,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub max_combo: u32,
    pub miss_count: u32,
    pub course_failed: bool,
    pub course_clear: bool,
    pub trophies_json: String,
    pub played_at: i64,
    pub charts: Vec<CourseScoreChartRecord>,
    pub replays: Vec<CourseReplayRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseBestScore {
    pub course_score_id: i64,
    pub course_id: i64,
    pub ex_score: u32,
    pub max_ex_score: u32,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub max_combo: u32,
    pub miss_count: u32,
    pub course_failed: bool,
    pub course_clear: bool,
    pub played_at: i64,
}

pub(super) fn upsert_course(
    conn: &mut Connection,
    source: &str,
    course: &CourseDefinition,
    source_position: i64,
    imported_at: i64,
) -> Result<i64> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO courses (
            source, course_key, title, kind, class_constraint, speed_constraint,
            judge_constraint, gauge_constraint, ln_constraint, source_constraints,
            trophies_json, release, imported_at, source_position
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(source, course_key) DO UPDATE SET
            title = excluded.title,
            kind = excluded.kind,
            class_constraint = excluded.class_constraint,
            speed_constraint = excluded.speed_constraint,
            judge_constraint = excluded.judge_constraint,
            gauge_constraint = excluded.gauge_constraint,
            ln_constraint = excluded.ln_constraint,
            source_constraints = excluded.source_constraints,
            trophies_json = excluded.trophies_json,
            release = excluded.release,
            imported_at = excluded.imported_at,
            source_position = excluded.source_position",
        params![
            source,
            course.key,
            course.title,
            enum_name(course.kind)?,
            enum_name(course.constraints.class)?,
            enum_name(course.constraints.speed)?,
            enum_name(course.constraints.judge)?,
            enum_name(course.constraints.gauge)?,
            enum_name(course.constraints.ln)?,
            serde_json::to_string(&course.constraints.source_constraints)?,
            serde_json::to_string(&course.trophies)?,
            course.release,
            imported_at,
            source_position,
        ],
    )?;

    let course_id: i64 = tx.query_row(
        "SELECT id FROM courses WHERE source = ?1 AND course_key = ?2",
        params![source, course.key],
        |row| row.get(0),
    )?;
    tx.execute("DELETE FROM course_entries WHERE course_id = ?1", params![course_id])?;

    for (position, entry) in course.entries.iter().enumerate() {
        let chart_id = resolve_entry_chart_id(&tx, entry)?;
        tx.execute(
            "INSERT INTO course_entries
             (course_id, position, md5, sha256, title_hint, chart_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                course_id,
                position as i64,
                entry.md5.as_deref().unwrap_or(""),
                entry.sha256.as_deref().unwrap_or(""),
                entry.title_hint,
                chart_id,
            ],
        )?;
    }

    tx.commit()?;
    Ok(course_id)
}

pub(super) fn list_courses(conn: &Connection) -> Result<Vec<StoredCourse>> {
    let mut stmt = conn.prepare(
        "SELECT id, source, course_key, title, kind, class_constraint, speed_constraint,
                judge_constraint, gauge_constraint, ln_constraint, source_constraints,
                trophies_json, release
         FROM courses
         ORDER BY title COLLATE NOCASE, id",
    )?;
    let rows = stmt.query_map([], stored_course_from_row)?;

    let mut courses = Vec::new();
    for row in rows {
        let mut course = row?;
        course.definition.entries =
            list_course_entries(conn, course.id)?.into_iter().map(|entry| entry.entry).collect();
        courses.push(course);
    }
    Ok(courses)
}

pub(super) fn list_courses_by_source(conn: &Connection, source: &str) -> Result<Vec<StoredCourse>> {
    let mut stmt = conn.prepare(
        "SELECT id, source, course_key, title, kind, class_constraint, speed_constraint,
                judge_constraint, gauge_constraint, ln_constraint, source_constraints,
                trophies_json, release
         FROM courses
         WHERE source = ?1
         ORDER BY source_position, id",
    )?;
    let rows = stmt.query_map(rusqlite::params![source], stored_course_from_row)?;

    let mut courses = Vec::new();
    for row in rows {
        let mut course = row?;
        course.definition.entries =
            list_course_entries(conn, course.id)?.into_iter().map(|entry| entry.entry).collect();
        courses.push(course);
    }
    Ok(courses)
}

pub(super) fn list_course_entries(
    conn: &Connection,
    course_id: i64,
) -> Result<Vec<StoredCourseEntry>> {
    let mut stmt = conn.prepare(
        "SELECT position, md5, sha256, title_hint, chart_id
         FROM course_entries
         WHERE course_id = ?1
         ORDER BY position",
    )?;
    let rows = stmt.query_map(params![course_id], |row| {
        let position: i64 = row.get(0)?;
        let md5: String = row.get(1)?;
        let sha256: String = row.get(2)?;
        Ok(StoredCourseEntry {
            position: position.max(0) as usize,
            entry: CourseEntry {
                md5: non_empty(md5),
                sha256: non_empty(sha256),
                title_hint: row.get(3)?,
                chart_id: row.get(4)?,
            },
        })
    })?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

fn stored_course_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCourse> {
    let id = row.get(0)?;
    let source = row.get(1)?;
    let source_constraints_json: String = row.get(10)?;
    let trophies_json: String = row.get(11)?;
    let constraints = CourseConstraints {
        class: enum_from_name(row.get::<_, String>(5)?)?,
        speed: enum_from_name(row.get::<_, String>(6)?)?,
        judge: enum_from_name(row.get::<_, String>(7)?)?,
        gauge: enum_from_name(row.get::<_, String>(8)?)?,
        ln: enum_from_name(row.get::<_, String>(9)?)?,
        source_constraints: serde_json::from_str(&source_constraints_json).unwrap_or_default(),
    };
    let trophies: Vec<CourseTrophy> = serde_json::from_str(&trophies_json).unwrap_or_default();
    Ok(StoredCourse {
        id,
        source,
        definition: CourseDefinition {
            key: row.get(2)?,
            title: row.get(3)?,
            kind: enum_from_name(row.get::<_, String>(4)?)?,
            entries: Vec::new(),
            constraints,
            trophies,
            release: row.get(12)?,
        },
    })
}

fn resolve_entry_chart_id(conn: &Connection, entry: &CourseEntry) -> Result<Option<i64>> {
    if let Some(chart_id) = entry.chart_id {
        return Ok(Some(chart_id));
    }
    if let Some(sha256) = &entry.sha256 {
        let chart_id = conn
            .query_row(
                "SELECT id FROM charts WHERE sha256 = ?1 ORDER BY id LIMIT 1",
                params![sha256],
                |row| row.get(0),
            )
            .optional()?;
        if chart_id.is_some() {
            return Ok(chart_id);
        }
    }
    if let Some(md5) = &entry.md5 {
        return Ok(conn
            .query_row(
                "SELECT id FROM charts WHERE md5 = ?1 ORDER BY id LIMIT 1",
                params![md5],
                |row| row.get(0),
            )
            .optional()?);
    }
    Ok(None)
}

pub(super) fn insert_course_score(
    conn: &mut Connection,
    record: &CourseScoreInsert,
) -> Result<i64> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO course_scores (
            course_id, ex_score, max_ex_score, clear_type, gauge_type, gauge_value,
            max_combo, miss_count, course_failed, course_clear, trophies_json, played_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            record.course_id,
            record.ex_score,
            record.max_ex_score,
            record.clear_type,
            record.gauge_type,
            record.gauge_value,
            record.max_combo,
            record.miss_count,
            record.course_failed,
            record.course_clear,
            record.trophies_json,
            record.played_at,
        ],
    )?;
    let course_score_id = tx.last_insert_rowid();

    for chart in &record.charts {
        tx.execute(
            "INSERT INTO course_score_charts (
                course_score_id, position, chart_id, ex_score, max_combo,
                clear_type, gauge_value
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                course_score_id,
                chart.position,
                chart.chart_id,
                chart.ex_score,
                chart.max_combo,
                chart.clear_type,
                chart.gauge_value,
            ],
        )?;
    }

    for replay in &record.replays {
        // Skip rows with empty replay_path (autoplay or save-disabled).
        if replay.replay_path.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO course_replays (course_score_id, position, chart_id, replay_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                course_score_id,
                replay.position,
                replay.chart_id,
                replay.replay_path,
            ],
        )?;
    }

    tx.commit()?;
    Ok(course_score_id)
}

pub(super) fn best_course_score(
    conn: &Connection,
    course_id: i64,
) -> Result<Option<CourseBestScore>> {
    // Pick the row with the highest ex_score; tie-break by best clear_type
    // rank, then by latest played_at.  ClearType rank uses the same numeric
    // ordering as the enum discriminant (NoPlay=0 .. Max=10).
    let row = conn
        .query_row(
            "SELECT id, course_id, ex_score, max_ex_score, clear_type, gauge_type,
                    gauge_value, max_combo, miss_count, course_failed, course_clear,
                    played_at
             FROM course_scores
             WHERE course_id = ?1
             ORDER BY ex_score DESC,
                      CASE clear_type
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
                      played_at DESC,
                      id DESC
             LIMIT 1",
            params![course_id],
            course_best_score_from_row,
        )
        .optional()?;
    Ok(row)
}

pub(super) fn best_course_clear(conn: &Connection, course_id: i64) -> Result<Option<ClearType>> {
    let value: Option<String> = conn
        .query_row(
            "SELECT clear_type
             FROM course_scores
             WHERE course_id = ?1
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
            params![course_id],
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
        "SELECT position, chart_id, ex_score, max_combo, clear_type, gauge_value
         FROM course_score_charts
         WHERE course_score_id = ?1
         ORDER BY position",
    )?;
    let rows = stmt.query_map(params![course_score_id], |row| {
        Ok(CourseScoreChartRecord {
            position: row.get(0)?,
            chart_id: row.get(1)?,
            ex_score: row.get(2)?,
            max_combo: row.get(3)?,
            clear_type: row.get(4)?,
            gauge_value: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub(super) fn list_course_replays(
    conn: &Connection,
    course_score_id: i64,
) -> Result<Vec<CourseReplayRecord>> {
    let mut stmt = conn.prepare(
        "SELECT position, chart_id, replay_path
         FROM course_replays
         WHERE course_score_id = ?1
         ORDER BY position",
    )?;
    let rows = stmt.query_map(params![course_score_id], |row| {
        Ok(CourseReplayRecord {
            position: row.get(0)?,
            chart_id: row.get(1)?,
            replay_path: row.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn course_best_score_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CourseBestScore> {
    Ok(CourseBestScore {
        course_score_id: row.get(0)?,
        course_id: row.get(1)?,
        ex_score: row.get(2)?,
        max_ex_score: row.get(3)?,
        clear_type: row.get(4)?,
        gauge_type: row.get(5)?,
        gauge_value: row.get(6)?,
        max_combo: row.get(7)?,
        miss_count: row.get(8)?,
        course_failed: row.get(9)?,
        course_clear: row.get(10)?,
        played_at: row.get(11)?,
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

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn enum_name<T: serde::Serialize>(value: T) -> Result<String> {
    Ok(serde_json::to_value(value)?.as_str().unwrap_or_default().to_string())
}

fn enum_from_name<T: for<'de> serde::Deserialize<'de>>(value: String) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

#[cfg(test)]
mod tests {
    use bmz_core::course::{
        CourseClassConstraint, CourseGaugeConstraint, CourseJudgeConstraint, CourseKind,
        CourseLnConstraint, CourseSpeedConstraint,
    };
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    fn open_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        conn
    }

    fn course() -> CourseDefinition {
        CourseDefinition {
            key: "course.json#0".to_string(),
            title: "七段".to_string(),
            kind: CourseKind::Dan,
            constraints: CourseConstraints {
                class: CourseClassConstraint::GradeMirrorAllowed,
                speed: CourseSpeedConstraint::NoSpeed,
                judge: CourseJudgeConstraint::Normal,
                gauge: CourseGaugeConstraint::Keys7,
                ln: CourseLnConstraint::Default,
                source_constraints: vec![
                    "grade_mirror".to_string(),
                    "no_speed".to_string(),
                    "gauge_7k".to_string(),
                ],
            },
            entries: vec![CourseEntry {
                title_hint: "Song A".to_string(),
                md5: Some("00112233445566778899aabbccddeeff".to_string()),
                sha256: Some(
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                ),
                chart_id: None,
            }],
            trophies: vec![CourseTrophy {
                name: "gold".to_string(),
                max_miss_rate: 2.5,
                min_score_rate: 88.0,
            }],
            release: true,
        }
    }

    #[test]
    fn upsert_and_list_course() {
        let mut conn = open_db();
        let course = course();

        let id =
            upsert_course(&mut conn, "course/default.json", &course, 0, 1_700_000_000).unwrap();
        assert!(id > 0);

        let courses = list_courses(&conn).unwrap();
        assert_eq!(courses.len(), 1);
        assert_eq!(courses[0].source, "course/default.json");
        assert_eq!(courses[0].definition.title, "七段");
        assert_eq!(
            courses[0].definition.constraints.class,
            CourseClassConstraint::GradeMirrorAllowed
        );
        assert_eq!(courses[0].definition.constraints.source_constraints[1], "no_speed");
        assert_eq!(courses[0].definition.entries[0].title_hint, "Song A");
        assert_eq!(courses[0].definition.trophies[0].name, "gold");
    }

    #[test]
    fn upsert_replaces_entries() {
        let mut conn = open_db();
        let mut course = course();
        upsert_course(&mut conn, "course/default.json", &course, 0, 1).unwrap();

        course.entries.push(CourseEntry {
            title_hint: "Song B".to_string(),
            md5: None,
            sha256: Some(
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            ),
            chart_id: None,
        });
        upsert_course(&mut conn, "course/default.json", &course, 0, 2).unwrap();

        let courses = list_courses(&conn).unwrap();
        assert_eq!(courses[0].definition.entries.len(), 2);
        assert_eq!(courses[0].definition.entries[1].title_hint, "Song B");
    }

    fn insert_test_course(conn: &mut Connection) -> i64 {
        upsert_course(conn, "course/default.json", &course(), 0, 1_700_000_000).unwrap()
    }

    fn sample_score_insert(course_id: i64, ex_score: u32, clear: &str) -> CourseScoreInsert {
        CourseScoreInsert {
            course_id,
            ex_score,
            max_ex_score: 1000,
            clear_type: clear.to_string(),
            gauge_type: "Normal".to_string(),
            gauge_value: 80.0,
            max_combo: 200,
            miss_count: 5,
            course_failed: clear == "Failed",
            course_clear: clear != "Failed" && clear != "NoPlay",
            trophies_json: "[]".to_string(),
            played_at: 1_700_000_500,
            charts: vec![
                CourseScoreChartRecord {
                    position: 0,
                    chart_id: 1,
                    ex_score: ex_score / 2,
                    max_combo: 100,
                    clear_type: clear.to_string(),
                    gauge_value: 80.0,
                },
                CourseScoreChartRecord {
                    position: 1,
                    chart_id: 2,
                    ex_score: ex_score - ex_score / 2,
                    max_combo: 100,
                    clear_type: clear.to_string(),
                    gauge_value: 80.0,
                },
            ],
            replays: vec![
                CourseReplayRecord {
                    position: 0,
                    chart_id: 1,
                    replay_path: "replay/c1.bzr".to_string(),
                },
                CourseReplayRecord {
                    position: 1,
                    chart_id: 2,
                    replay_path: "replay/c2.bzr".to_string(),
                },
            ],
        }
    }

    #[test]
    fn insert_course_score_persists_charts_and_replays() {
        let mut conn = open_db();
        let course_id = insert_test_course(&mut conn);

        let score_id =
            insert_course_score(&mut conn, &sample_score_insert(course_id, 500, "Normal")).unwrap();
        assert!(score_id > 0);

        let (count, total): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(ex_score), 0) FROM course_score_charts WHERE course_score_id = ?1",
                params![score_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(total, 500);

        let replays = list_course_replays(&conn, score_id).unwrap();
        assert_eq!(replays.len(), 2);
        assert_eq!(replays[0].replay_path, "replay/c1.bzr");
    }

    #[test]
    fn insert_course_score_skips_empty_replay_paths() {
        let mut conn = open_db();
        let course_id = insert_test_course(&mut conn);

        let mut insert = sample_score_insert(course_id, 500, "Normal");
        insert.replays[0].replay_path = String::new();
        let score_id = insert_course_score(&mut conn, &insert).unwrap();

        let replays = list_course_replays(&conn, score_id).unwrap();
        assert_eq!(replays.len(), 1);
        assert_eq!(replays[0].position, 1);
    }

    #[test]
    fn best_course_score_picks_highest_ex_score() {
        let mut conn = open_db();
        let course_id = insert_test_course(&mut conn);

        insert_course_score(&mut conn, &sample_score_insert(course_id, 400, "Normal")).unwrap();
        insert_course_score(&mut conn, &sample_score_insert(course_id, 800, "Easy")).unwrap();
        insert_course_score(&mut conn, &sample_score_insert(course_id, 600, "Hard")).unwrap();

        let best = best_course_score(&conn, course_id).unwrap().unwrap();
        assert_eq!(best.ex_score, 800);
        assert_eq!(best.clear_type, "Easy");
        assert_eq!(best.course_id, course_id);
    }

    #[test]
    fn best_course_score_tiebreaks_by_clear_rank() {
        let mut conn = open_db();
        let course_id = insert_test_course(&mut conn);

        insert_course_score(&mut conn, &sample_score_insert(course_id, 500, "Normal")).unwrap();
        insert_course_score(&mut conn, &sample_score_insert(course_id, 500, "Hard")).unwrap();
        insert_course_score(&mut conn, &sample_score_insert(course_id, 500, "Easy")).unwrap();

        let best = best_course_score(&conn, course_id).unwrap().unwrap();
        assert_eq!(best.clear_type, "Hard");
    }

    #[test]
    fn best_course_clear_returns_highest_rank() {
        let mut conn = open_db();
        let course_id = insert_test_course(&mut conn);
        insert_course_score(&mut conn, &sample_score_insert(course_id, 200, "Failed")).unwrap();
        insert_course_score(&mut conn, &sample_score_insert(course_id, 500, "Normal")).unwrap();

        assert_eq!(best_course_clear(&conn, course_id).unwrap(), Some(ClearType::Normal));
    }

    #[test]
    fn best_course_score_returns_none_when_no_history() {
        let mut conn = open_db();
        let course_id = insert_test_course(&mut conn);
        assert!(best_course_score(&conn, course_id).unwrap().is_none());
        assert!(best_course_clear(&conn, course_id).unwrap().is_none());
    }

    #[test]
    fn deleting_course_cascades_to_scores_and_replays() {
        let mut conn = open_db();
        let course_id = insert_test_course(&mut conn);
        insert_course_score(&mut conn, &sample_score_insert(course_id, 500, "Normal")).unwrap();

        conn.execute("DELETE FROM courses WHERE id = ?1", params![course_id]).unwrap();

        let scores: i64 = conn
            .query_row("SELECT COUNT(*) FROM course_scores", [], |row| row.get(0))
            .unwrap();
        let charts: i64 = conn
            .query_row("SELECT COUNT(*) FROM course_score_charts", [], |row| row.get(0))
            .unwrap();
        let replays: i64 =
            conn.query_row("SELECT COUNT(*) FROM course_replays", [], |row| row.get(0)).unwrap();
        assert_eq!(scores, 0);
        assert_eq!(charts, 0);
        assert_eq!(replays, 0);
    }

    #[test]
    fn list_courses_by_source_orders_by_source_position() {
        let mut conn = open_db();

        // Insert in title-alphabetical order that does NOT match position order.
        let mut zebra = course();
        zebra.key = "z.json#0".to_string();
        zebra.title = "Alpha (pos 5)".to_string();
        upsert_course(&mut conn, "table:url", &zebra, 5, 1).unwrap();

        let mut bravo = course();
        bravo.key = "z.json#1".to_string();
        bravo.title = "Zulu (pos 0)".to_string();
        upsert_course(&mut conn, "table:url", &bravo, 0, 1).unwrap();

        let mut charlie = course();
        charlie.key = "z.json#2".to_string();
        charlie.title = "Mike (pos 2)".to_string();
        upsert_course(&mut conn, "table:url", &charlie, 2, 1).unwrap();

        let courses = list_courses_by_source(&conn, "table:url").unwrap();
        assert_eq!(courses.len(), 3);
        // Order should follow source_position (0, 2, 5), not alphabetical title.
        assert_eq!(courses[0].definition.title, "Zulu (pos 0)");
        assert_eq!(courses[1].definition.title, "Mike (pos 2)");
        assert_eq!(courses[2].definition.title, "Alpha (pos 5)");
    }
}
