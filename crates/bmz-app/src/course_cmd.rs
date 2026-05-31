use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::CourseCommand;
use crate::paths::resolve_app_paths;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::migrate_library_db;

pub fn run_course_command(cmd: CourseCommand) -> Result<()> {
    match cmd {
        CourseCommand::Import { path } => import_courses(Path::new(&path)),
        CourseCommand::List => list_courses(),
        CourseCommand::History { course_id, limit } => course_history(course_id, limit),
    }
}

fn import_courses(path: &Path) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;
    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;
    let now = unix_now();

    let files = course_json_files(path)?;
    if files.is_empty() {
        bail!("no .json course files found at {}", path.display());
    }

    let mut imported_files = 0usize;
    let mut imported_courses = 0usize;
    let mut failed_files = 0usize;

    for file in files {
        let source = file.to_string_lossy().replace('\\', "/");
        match std::fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))
            .and_then(|json| crate::course::parse_beatoraja_course_json(&source, &json))
        {
            Ok(courses) => {
                for (position, course) in courses.iter().enumerate() {
                    library_db.upsert_course(&source, course, position as i64, now)?;
                }
                println!("Imported {} course(s) from {}", courses.len(), file.display());
                imported_files += 1;
                imported_courses += courses.len();
            }
            Err(err) => {
                println!("FAILED {}: {err}", file.display());
                failed_files += 1;
            }
        }
    }

    println!(
        "\n{imported_courses} course(s) imported from {imported_files} file(s), {failed_files} failed."
    );
    Ok(())
}

fn list_courses() -> Result<()> {
    let app_paths = resolve_app_paths()?;
    migrate_library_db(&app_paths.library_db)?;
    let library_db = LibraryDatabase::open(&app_paths.library_db)?;
    let courses = library_db.list_courses()?;

    if courses.is_empty() {
        println!("No courses stored. Use `course import <PATH>` to import beatoraja course JSON.");
        return Ok(());
    }

    for course in courses {
        let missing =
            course.definition.entries.iter().filter(|entry| entry.chart_id.is_none()).count();
        println!(
            "[{}] {} — {} chart(s), {} missing ({})",
            kind_label(course.definition.kind),
            course.definition.title,
            course.definition.entries.len(),
            missing,
            course.source
        );
    }
    Ok(())
}

fn course_history(course_id: i64, limit: u32) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    migrate_library_db(&app_paths.library_db)?;
    let library_db = LibraryDatabase::open(&app_paths.library_db)?;

    let course = library_db
        .list_courses()?
        .into_iter()
        .find(|c| c.id == course_id)
        .ok_or_else(|| anyhow::anyhow!("course id {course_id} not found"))?;

    let entries = library_db.list_recent_course_scores(course_id, limit, 0)?;
    if entries.is_empty() {
        println!(
            "No attempts stored for course [{}] {} (id {}).",
            kind_label(course.definition.kind),
            course.definition.title,
            course_id,
        );
        return Ok(());
    }

    println!(
        "[{}] {} — {} stored attempt(s) (showing up to {}):",
        kind_label(course.definition.kind),
        course.definition.title,
        entries.len(),
        limit,
    );
    println!(
        "  {:<5}  {:<19}  {:>7}  {:>7}  {:<12}  {:>8}  TROPHIES",
        "ID", "PLAYED AT (UTC)", "EX", "MAX EX", "CLEAR", "MAXCOMBO",
    );
    for entry in entries {
        let played_at = format_unix_utc(entry.played_at);
        let trophies = if entry.achieved_trophies.is_empty() {
            "-".to_string()
        } else {
            entry.achieved_trophies.join(",")
        };
        println!(
            "  {:<5}  {:<19}  {:>7}  {:>7}  {:<12}  {:>8}  {}",
            entry.course_score_id,
            played_at,
            entry.ex_score,
            entry.max_ex_score,
            entry.clear_type,
            entry.max_combo,
            trophies,
        );
    }
    Ok(())
}

fn kind_label(kind: bmz_core::course::CourseKind) -> &'static str {
    match kind {
        bmz_core::course::CourseKind::Course => "course",
        bmz_core::course::CourseKind::Dan => "dan",
    }
}

/// Format a Unix-seconds timestamp as `YYYY-MM-DD HH:MM:SS` in UTC.
/// Pure Rust to avoid pulling in chrono just for one CLI line.
fn format_unix_utc(secs: i64) -> String {
    // Days since 1970-01-01 and remaining seconds in the day.
    let days = secs.div_euclid(86_400);
    let day_secs = secs.rem_euclid(86_400) as u32;
    let h = day_secs / 3_600;
    let m = (day_secs % 3_600) / 60;
    let s = day_secs % 60;

    // Convert days since epoch into (year, month, day).  Howard Hinnant's
    // civil_from_days algorithm; correct for any i64 day count in range.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m_civ = if mp < 10 { mp + 3 } else { mp.wrapping_sub(9) };
    let y = if m_civ <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m_civ, d, h, m, s)
}

fn course_json_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        bail!("path does not exist or is not readable: {}", path.display());
    }

    let mut files = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_path = entry.path();
        if file_path.is_file()
            && file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            files.push(file_path);
        }
    }
    files.sort();
    Ok(files)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_unix_utc_matches_known_timestamps() {
        // 0 = epoch start.
        assert_eq!(format_unix_utc(0), "1970-01-01 00:00:00");
        // 2024-01-01 00:00:00 UTC = 1_704_067_200.
        assert_eq!(format_unix_utc(1_704_067_200), "2024-01-01 00:00:00");
        // Leap-year Feb 29 2024.
        assert_eq!(format_unix_utc(1_709_175_900), "2024-02-29 03:05:00");
        // A pre-epoch instant (1969-12-31 23:59:59) stays well-formed.
        assert_eq!(format_unix_utc(-1), "1969-12-31 23:59:59");
    }

    #[test]
    fn course_json_files_returns_single_file() {
        let file =
            std::env::temp_dir().join(format!("bmz-course-files-{}.json", std::process::id()));
        std::fs::write(&file, "{}").unwrap();

        let files = course_json_files(&file).unwrap();

        assert_eq!(files, vec![file.clone()]);
        std::fs::remove_file(file).unwrap();
    }
}
