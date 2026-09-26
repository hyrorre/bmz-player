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
    let course_id =
        upsert_course_in_transaction(&tx, source, course, source_position, imported_at)?;
    tx.commit()?;
    Ok(course_id)
}

pub(super) fn upsert_course_in_transaction(
    conn: &Connection,
    source: &str,
    course: &CourseDefinition,
    source_position: i64,
    imported_at: i64,
) -> Result<i64> {
    conn.execute(
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

    let course_id: i64 = conn.query_row(
        "SELECT id FROM courses WHERE source = ?1 AND course_key = ?2",
        params![source, course.key],
        |row| row.get(0),
    )?;
    conn.execute("DELETE FROM course_entries WHERE course_id = ?1", params![course_id])?;

    for (position, entry) in course.entries.iter().enumerate() {
        let chart_id = resolve_entry_chart_id(conn, entry)?;
        conn.execute(
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

pub(super) fn course_by_id(conn: &Connection, course_id: i64) -> Result<Option<StoredCourse>> {
    let mut course = conn
        .query_row(
            "SELECT id, source, course_key, title, kind, class_constraint, speed_constraint,
                    judge_constraint, gauge_constraint, ln_constraint, source_constraints,
                    trophies_json, release
             FROM courses
             WHERE id = ?1",
            params![course_id],
            stored_course_from_row,
        )
        .optional()?;
    if let Some(course) = &mut course {
        course.definition.entries =
            list_course_entries(conn, course.id)?.into_iter().map(|entry| entry.entry).collect();
    }
    Ok(course)
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

pub(super) fn delete_courses_by_source(conn: &Connection, source: &str) -> Result<usize> {
    Ok(conn.execute("DELETE FROM courses WHERE source = ?1", params![source])?)
}

pub(super) fn delete_course(conn: &Connection, course_id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM courses WHERE id = ?1", params![course_id])? != 0)
}

pub(super) fn delete_table_courses_by_source_prefix(
    conn: &Connection,
    source_prefix: &str,
) -> Result<usize> {
    let escaped = format!("table:{source_prefix}")
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    Ok(conn.execute(
        "DELETE FROM courses WHERE source LIKE ?1 ESCAPE '\\'",
        params![format!("{escaped}%")],
    )?)
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

/// Resolves course entries related to a chart that has just been imported.
///
/// Existing links remain stable while their file is available.  If the linked
/// file was moved or removed, the newest existing file with the same SHA-256
/// (or MD5 fallback) replaces it.
pub(super) fn refresh_course_entries_for_chart(
    conn: &Connection,
    sha256: &str,
    md5: &str,
) -> Result<usize> {
    let entries = course_entries_for_link_repair(
        conn,
        "WHERE course_entries.sha256 = ?1 OR course_entries.md5 = ?2",
        params![sha256, md5],
    )?;
    repair_course_entry_rows(conn, entries)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CourseLinkRepairStats {
    pub(super) scanned_entries: usize,
    pub(super) repaired_entries: usize,
}

pub(super) fn repair_course_entry_chart_links_with_stats(
    conn: &Connection,
) -> Result<CourseLinkRepairStats> {
    let entries = course_entries_for_link_repair(conn, "", [])?;
    let scanned_entries = entries.len();
    let repaired_entries = repair_course_entry_rows(conn, entries)?;
    Ok(CourseLinkRepairStats { scanned_entries, repaired_entries })
}

pub(super) fn repair_course_entry_chart_links_for_course(
    conn: &Connection,
    course_id: i64,
) -> Result<usize> {
    let entries = course_entries_for_link_repair(
        conn,
        "WHERE course_entries.course_id = ?1",
        params![course_id],
    )?;
    repair_course_entry_rows(conn, entries)
}

fn stored_course_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCourse> {
    let id = row.get(0)?;
    let source = row.get(1)?;
    let source_constraints_json: String = row.get(10)?;
    let trophies_json: String = row.get(11)?;
    let mut constraints = CourseConstraints {
        class: enum_from_name(row.get::<_, String>(5)?)?,
        speed: enum_from_name(row.get::<_, String>(6)?)?,
        judge: enum_from_name(row.get::<_, String>(7)?)?,
        gauge: enum_from_name(row.get::<_, String>(8)?)?,
        ln: enum_from_name(row.get::<_, String>(9)?)?,
        source_constraints: serde_json::from_str(&source_constraints_json).unwrap_or_default(),
    };
    constraints.source_constraints =
        constraints.canonical_names().into_iter().map(str::to_string).collect();
    let trophies: Vec<CourseTrophy> = serde_json::from_str(&trophies_json).unwrap_or_default();
    let kind = CourseDefinition::derive_kind_from_constraints(&constraints);
    Ok(StoredCourse {
        id,
        source,
        definition: CourseDefinition {
            key: row.get(2)?,
            title: row.get(3)?,
            kind,
            entries: Vec::new(),
            constraints,
            trophies,
            release: row.get(12)?,
        },
    })
}

fn resolve_entry_chart_id(conn: &Connection, entry: &CourseEntry) -> Result<Option<i64>> {
    // Prefer the same active/readable copy as playback, but retain metadata links
    // for partial replays whose unplayed stages no longer have usable files.
    // Legacy metadata-only databases retain their historical linking behavior.
    if super::library_db::configured_song_roots(conn)?.is_some() {
        let sha = if let Some(sha) = &entry.sha256 {
            Some(sha.clone())
        } else if let Some(id) = entry.chart_id {
            conn.query_row("SELECT sha256 FROM charts WHERE id = ?1", [id], |row| row.get(0))
                .optional()?
        } else {
            None
        };
        if let Some(sha) = sha {
            return resolve_configured_entry_chart_id(conn, "sha256", &sha, entry.chart_id);
        }
        if let Some(md5) = &entry.md5 {
            return resolve_configured_entry_chart_id(conn, "md5", md5, entry.chart_id);
        }
        return Ok(None);
    }
    if let Some(chart_id) = entry.chart_id
        && chart_id_has_existing_file(conn, chart_id)?
    {
        return Ok(Some(chart_id));
    }
    if let Some(sha256) = &entry.sha256 {
        let candidates = chart_candidates_by_hash(conn, "sha256", sha256)?;
        if let Some(chart_id) = candidates.existing {
            return Ok(Some(chart_id));
        }
        if candidates.latest.is_some() {
            return Ok(entry.chart_id.or(candidates.latest));
        }
    }
    if let Some(md5) = &entry.md5 {
        let candidates = chart_candidates_by_hash(conn, "md5", md5)?;
        if let Some(chart_id) = candidates.existing {
            return Ok(Some(chart_id));
        }
        if candidates.latest.is_some() {
            return Ok(entry.chart_id.or(candidates.latest));
        }
    }
    Ok(entry.chart_id)
}

fn resolve_configured_entry_chart_id(
    conn: &Connection,
    column: &str,
    hash: &str,
    preferred: Option<i64>,
) -> Result<Option<i64>> {
    if let Some(id) = super::library_db::available_chart_id_for_hash(conn, column, hash, preferred)?
    {
        return Ok(Some(id));
    }
    // A link describes chart identity, not permission to play its file. Launch
    // separately verifies every played stage. This also restores links cleared
    // by older repairs, without substituting metadata from a different hash.
    debug_assert!(matches!(column, "sha256" | "md5"));
    Ok(conn
        .query_row(
            &format!(
                "SELECT id FROM charts WHERE {column} = ?1
                 ORDER BY (id = ?2) DESC, import_version DESC, id DESC LIMIT 1"
            ),
            params![hash, preferred],
            |row| row.get(0),
        )
        .optional()?)
}

#[derive(Debug)]
struct CourseEntryLinkRow {
    course_id: i64,
    position: i64,
    entry: CourseEntry,
}

fn course_entries_for_link_repair<P>(
    conn: &Connection,
    filter: &str,
    params: P,
) -> Result<Vec<CourseEntryLinkRow>>
where
    P: rusqlite::Params,
{
    let sql = format!(
        "SELECT course_id, position, md5, sha256, title_hint, chart_id
         FROM course_entries
         {filter}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, |row| {
        let md5: String = row.get(2)?;
        let sha256: String = row.get(3)?;
        Ok(CourseEntryLinkRow {
            course_id: row.get(0)?,
            position: row.get(1)?,
            entry: CourseEntry {
                md5: non_empty(md5),
                sha256: non_empty(sha256),
                title_hint: row.get(4)?,
                chart_id: row.get(5)?,
            },
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

fn repair_course_entry_rows(conn: &Connection, entries: Vec<CourseEntryLinkRow>) -> Result<usize> {
    let mut repaired = 0;
    let mut update = conn.prepare_cached(
        "UPDATE course_entries
         SET chart_id = ?1
         WHERE course_id = ?2 AND position = ?3",
    )?;
    for row in entries {
        let chart_id = resolve_entry_chart_id(conn, &row.entry)?;
        if chart_id == row.entry.chart_id {
            continue;
        }
        repaired += update.execute(params![chart_id, row.course_id, row.position])?;
    }
    Ok(repaired)
}

#[derive(Debug, Default)]
struct ChartHashCandidates {
    existing: Option<i64>,
    latest: Option<i64>,
}

fn chart_candidates_by_hash(
    conn: &Connection,
    column: &'static str,
    hash: &str,
) -> Result<ChartHashCandidates> {
    debug_assert!(matches!(column, "sha256" | "md5"));
    if hash.is_empty() {
        return Ok(ChartHashCandidates::default());
    }
    let sql = format!(
        "SELECT charts.id, chart_files.path
         FROM charts
         LEFT JOIN chart_file_links ON chart_file_links.chart_id = charts.id
         LEFT JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
         WHERE charts.{column} = ?1
         ORDER BY charts.id DESC, chart_files.path COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![hash])?;
    let mut candidates = ChartHashCandidates::default();
    while let Some(row) = rows.next()? {
        let chart_id: i64 = row.get(0)?;
        let path: Option<String> = row.get(1)?;
        candidates.latest.get_or_insert(chart_id);
        if candidates.existing.is_none()
            && path.as_deref().is_some_and(|path| std::path::Path::new(path).is_file())
        {
            candidates.existing = Some(chart_id);
        }
    }
    Ok(candidates)
}

fn chart_id_has_existing_file(conn: &Connection, chart_id: i64) -> Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT chart_files.path
         FROM chart_file_links
         JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
         WHERE chart_file_links.chart_id = ?1",
    )?;
    let paths = stmt
        .query_map(params![chart_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(paths.iter().any(|path| std::path::Path::new(path).is_file()))
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
    fn class_constraint_is_the_source_of_truth_for_course_kind() {
        let mut conn = open_db();
        let course = course();
        upsert_course(&mut conn, "course/default.json", &course, 0, 1_700_000_000).unwrap();
        conn.execute("UPDATE courses SET kind = 'course'", []).unwrap();

        let courses = list_courses(&conn).unwrap();

        assert_eq!(courses[0].definition.kind, CourseKind::Dan);
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
    fn library_migrations_drop_legacy_course_score_tables() {
        let conn = open_db();
        for table in [
            "course_scores",
            "course_score_charts",
            "course_replays",
            "course_replay_slots",
            "course_trophy_achievements",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "{table} should not remain in library.db");
        }
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
