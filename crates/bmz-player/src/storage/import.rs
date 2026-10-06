use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use bmz_chart::import::error::ImportWarning;
use bmz_chart::import::import_bms_chart;

use super::library_db::{ChartImportRecord, LibraryDatabase};

#[derive(Debug, Clone)]
pub struct ImportedChart {
    pub chart_id: i64,
    pub chart_file_id: i64,
    pub chart: bmz_chart::model::PlayableChart,
    pub warnings: Vec<ImportWarning>,
}

pub fn import_chart_file(
    db: &mut LibraryDatabase,
    path: &Path,
    root_id: Option<i64>,
    random_seed: Option<u64>,
    scanned_at: i64,
) -> Result<ImportedChart> {
    let locator = crate::chart_source::ChartLocator::parse(path)?;
    let stable_path = locator.to_path_buf();
    let path = stable_path.as_path();
    let metadata = std::fs::metadata(locator.container_path())?;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let (mut import, archive_index, file_size) =
        if let crate::chart_source::ChartLocator::Archive { container, entry } = &locator {
            let read = crate::song_archive::read_chart(container, entry, &Default::default())?;
            let bytes = read.bytes;
            let index = crate::song_archive::inspect(container, &Default::default())?;
            anyhow::ensure!(
                read.generation == index.generation,
                "archive changed during chart import"
            );
            let mut import = bmz_chart::import::import_chart_bytes_with_random_source(
                path,
                &bytes,
                bmz_chart::import::BmsRandomSource::Seed(random_seed),
                false,
            )?;
            import.chart.metadata.preview_file = crate::chart_source::archive_preview_file(
                &index,
                entry,
                &import.chart.metadata.preview_file,
            );
            (import, Some(index), bytes.len() as u64)
        } else {
            (import_bms_chart(path, random_seed, true)?, None, metadata.len())
        };
    let chart = &mut import.chart;
    let record =
        ChartImportRecord { root_id, file_path: path, file_size, modified_at, scanned_at, chart };

    let chart_id = db.upsert_chart_import(&record)?;
    if let Some(folder) = path.parent() {
        db.update_folder_document_flags(&[(
            folder.to_path_buf(),
            match (&locator, &archive_index) {
                (crate::chart_source::ChartLocator::Archive { entry, .. }, Some(index)) => {
                    crate::chart_source::archive_folder_has_document(index, entry)
                }
                _ => super::scan::folder_has_document(folder),
            },
        )])?;
    }
    let chart_file_id =
        db.chart_file_id_by_path(path)?.expect("chart file must exist after import upsert");
    db.replace_import_warnings(chart_file_id, &import.warnings, scanned_at)?;

    Ok(ImportedChart { chart_id, chart_file_id, chart: import.chart, warnings: import.warnings })
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::library_db::LibraryDatabase;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    #[test]
    fn import_chart_file_registers_chart_and_warnings() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let path = write_temp_bms(
            "\
#TITLE Storage Import
#BPM 120
#TOTAL 200
#WAV01 key.wav
#00011:0199
",
        );
        let key_path = path.parent().unwrap().join("key.wav");
        write_file(&key_path, b"");

        let imported = import_chart_file(&mut db, &path, None, None, 1_700_000_010).unwrap();

        assert!(imported.chart_id > 0);
        assert!(imported.chart_file_id > 0);
        assert_eq!(imported.chart.metadata.title, "Storage Import");
        assert!(!imported.warnings.is_empty(), "warnings: {:?}", imported.warnings);

        let title: String =
            db.conn().query_row("SELECT title FROM charts", [], |row| row.get(0)).unwrap();

        assert_eq!(title, "Storage Import");

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(key_path).unwrap();
    }

    #[test]
    fn import_chart_file_honors_bms_random_seed() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let path = write_temp_bms(
            "\
#RANDOM 2
#IF 1
#TITLE Random One
#ENDIF
#IF 2
#TITLE Random Two
#ENDIF
#BPM 120
#00011:01
",
        );
        let expected = import_bms_chart(&path, Some(77), true).unwrap().chart;

        let imported = import_chart_file(&mut db, &path, None, Some(77), 1_700_000_011).unwrap();

        assert_eq!(imported.chart.metadata.title, expected.metadata.title);
        assert_eq!(imported.chart.total_notes, expected.total_notes);

        std::fs::remove_file(path).unwrap();
    }

    fn write_temp_bms(text: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-player-import-{}-{stamp}.bms", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.sync_all().unwrap();
        path
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
}
