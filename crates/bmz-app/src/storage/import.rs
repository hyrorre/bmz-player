use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use bmz_chart::import::error::ImportWarning;
use bmz_chart::import::{ImportResult, import_bms_chart};

use super::library_db::{ChartImportRecord, LibraryDatabase};

#[derive(Debug, Clone)]
pub struct ImportedChart {
    pub chart_id: i64,
    pub chart_file_id: i64,
    pub warnings: Vec<ImportWarning>,
}

pub fn import_chart_file(
    db: &mut LibraryDatabase,
    path: &Path,
    root_id: Option<i64>,
    scanned_at: i64,
) -> Result<ImportedChart> {
    let metadata = std::fs::metadata(path)?;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let ImportResult { chart, warnings } = import_bms_chart(path, None)?;
    let record = ChartImportRecord {
        root_id,
        file_path: path,
        file_size: metadata.len(),
        modified_at,
        scanned_at,
        chart: &chart,
    };

    let chart_id = db.upsert_chart_import(&record)?;
    let chart_file_id =
        db.chart_file_id_by_path(path)?.expect("chart file must exist after import upsert");
    db.replace_import_warnings(chart_file_id, &warnings, scanned_at)?;

    Ok(ImportedChart { chart_id, chart_file_id, warnings })
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
#WAV01 key.wav
#00011:01
#00099:01
",
        );

        let imported = import_chart_file(&mut db, &path, None, 1_700_000_010).unwrap();

        assert!(imported.chart_id > 0);
        assert!(imported.chart_file_id > 0);
        assert_eq!(imported.warnings.len(), 1);

        let title: String =
            db.conn().query_row("SELECT title FROM charts", [], |row| row.get(0)).unwrap();
        let warning_code: String = db
            .conn()
            .query_row("SELECT code FROM chart_import_warnings", [], |row| row.get(0))
            .unwrap();

        assert_eq!(title, "Storage Import");
        assert_eq!(warning_code, "UnsupportedChannel");

        std::fs::remove_file(path).unwrap();
    }

    fn write_temp_bms(text: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-app-import-{}-{stamp}.bms", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.sync_all().unwrap();
        path
    }
}
