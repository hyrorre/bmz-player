use anyhow::Result;
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

        let id = upsert_course(&mut conn, "course/default.json", &course, 0, 1_700_000_000).unwrap();
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
