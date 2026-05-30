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
        let kind = match course.definition.kind {
            bmz_core::course::CourseKind::Course => "course",
            bmz_core::course::CourseKind::Dan => "dan",
        };
        println!(
            "[{}] {} — {} chart(s), {} missing ({})",
            kind,
            course.definition.title,
            course.definition.entries.len(),
            missing,
            course.source
        );
    }
    Ok(())
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
    fn course_json_files_returns_single_file() {
        let file =
            std::env::temp_dir().join(format!("bmz-course-files-{}.json", std::process::id()));
        std::fs::write(&file, "{}").unwrap();

        let files = course_json_files(&file).unwrap();

        assert_eq!(files, vec![file.clone()]);
        std::fs::remove_file(file).unwrap();
    }
}
