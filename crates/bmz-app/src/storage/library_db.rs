use std::path::Path;

use anyhow::Result;
use bmz_chart::import::error::ImportWarning;
use bmz_chart::model::{NoteKind, PlayableChart, TimingEventKind};
use rusqlite::{Connection, OptionalExtension, params};

use super::common::configure_connection;

pub const CHART_IMPORT_VERSION: i64 = 1;

pub struct LibraryDatabase {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct ChartImportRecord<'a> {
    pub root_id: Option<i64>,
    pub file_path: &'a Path,
    pub file_size: u64,
    pub modified_at: i64,
    pub scanned_at: i64,
    pub chart: &'a PlayableChart,
}

impl LibraryDatabase {
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

    pub fn upsert_chart_import(&mut self, record: &ChartImportRecord<'_>) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let chart_file_id = upsert_chart_file(&tx, record)?;
        let chart_id = upsert_chart(&tx, record)?;
        tx.execute(
            "INSERT OR IGNORE INTO chart_file_links (chart_id, chart_file_id) VALUES (?1, ?2)",
            params![chart_id, chart_file_id],
        )?;
        tx.commit()?;
        Ok(chart_id)
    }

    pub fn chart_id_by_sha256(&self, sha256: [u8; 32]) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM charts WHERE sha256 = ?1",
                params![sha256.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn chart_file_id_by_path(&self, path: &Path) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM chart_files WHERE path = ?1",
                params![path_to_string(path)],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replace_import_warnings(
        &mut self,
        chart_file_id: i64,
        warnings: &[ImportWarning],
        created_at: i64,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM chart_import_warnings WHERE chart_file_id = ?1",
            params![chart_file_id],
        )?;

        for warning in warnings {
            let (code, message) = warning_details(warning);
            tx.execute(
                "INSERT INTO chart_import_warnings (chart_file_id, code, message, created_at)
                VALUES (?1, ?2, ?3, ?4)",
                params![chart_file_id, code, message, created_at],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn upsert_root(&mut self, path: &Path, enabled: bool, recursive: bool) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO roots (path, enabled, recursive)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(path) DO UPDATE SET
                enabled = excluded.enabled,
                recursive = excluded.recursive",
            params![path_to_string(path), enabled, recursive],
        )?;

        self.conn
            .query_row(
                "SELECT id FROM roots WHERE path = ?1",
                params![path_to_string(path)],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn update_root_scanned_at(&mut self, root_id: i64, scanned_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE roots SET last_scan_at = ?1 WHERE id = ?2",
            params![scanned_at, root_id],
        )?;
        Ok(())
    }
}

fn upsert_chart_file(conn: &Connection, record: &ChartImportRecord<'_>) -> Result<i64> {
    conn.execute(
        "INSERT INTO chart_files (
            root_id,
            path,
            file_size,
            modified_at,
            md5,
            sha256,
            scanned_at,
            parse_status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'Parsed')
        ON CONFLICT(path) DO UPDATE SET
            root_id = excluded.root_id,
            file_size = excluded.file_size,
            modified_at = excluded.modified_at,
            md5 = excluded.md5,
            sha256 = excluded.sha256,
            scanned_at = excluded.scanned_at,
            parse_status = excluded.parse_status",
        params![
            record.root_id,
            path_to_string(record.file_path),
            record.file_size as i64,
            record.modified_at,
            record.chart.identity.file_md5.as_slice(),
            record.chart.identity.file_sha256.as_slice(),
            record.scanned_at,
        ],
    )?;

    conn.query_row(
        "SELECT id FROM chart_files WHERE path = ?1",
        params![path_to_string(record.file_path)],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn upsert_chart(conn: &Connection, record: &ChartImportRecord<'_>) -> Result<i64> {
    let chart = record.chart;
    let stats = ChartStats::from_chart(chart);
    conn.execute(
        "INSERT INTO charts (
            sha256,
            md5,
            title,
            subtitle,
            artist,
            subartist,
            genre,
            difficulty_name,
            play_level,
            mode,
            total_notes,
            initial_bpm,
            min_bpm,
            max_bpm,
            length_ms,
            ln_type,
            has_bga,
            has_long_notes,
            has_mines,
            folder_path,
            stage_file,
            preview_file,
            import_version
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, '7K', ?10, ?11, ?12, ?13,
            ?14, ?15, 0, ?16, ?17, ?18, ?19, ?20, ?21
        )
        ON CONFLICT(sha256) DO UPDATE SET
            md5 = excluded.md5,
            title = excluded.title,
            subtitle = excluded.subtitle,
            artist = excluded.artist,
            subartist = excluded.subartist,
            genre = excluded.genre,
            difficulty_name = excluded.difficulty_name,
            play_level = excluded.play_level,
            mode = excluded.mode,
            total_notes = excluded.total_notes,
            initial_bpm = excluded.initial_bpm,
            min_bpm = excluded.min_bpm,
            max_bpm = excluded.max_bpm,
            length_ms = excluded.length_ms,
            ln_type = excluded.ln_type,
            has_bga = excluded.has_bga,
            has_long_notes = excluded.has_long_notes,
            has_mines = excluded.has_mines,
            folder_path = excluded.folder_path,
            stage_file = excluded.stage_file,
            preview_file = excluded.preview_file,
            import_version = excluded.import_version",
        params![
            chart.identity.file_sha256.as_slice(),
            chart.identity.file_md5.as_slice(),
            chart.metadata.title.as_str(),
            chart.metadata.subtitle.as_str(),
            chart.metadata.artist.as_str(),
            chart.metadata.subartist.as_str(),
            chart.metadata.genre.as_str(),
            chart.metadata.difficulty_name.as_str(),
            chart.metadata.play_level.as_str(),
            chart.total_notes,
            chart.metadata.initial_bpm,
            stats.min_bpm,
            stats.max_bpm,
            chart.end_time.0 / 1_000,
            stats.ln_type,
            stats.has_long_notes,
            stats.has_mines,
            folder_path(record.file_path),
            chart.metadata.stage_file.as_str(),
            chart.metadata.preview_file.as_str(),
            CHART_IMPORT_VERSION,
        ],
    )?;

    conn.query_row(
        "SELECT id FROM charts WHERE sha256 = ?1",
        params![chart.identity.file_sha256.as_slice()],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

#[derive(Debug, Clone, Copy)]
struct ChartStats {
    min_bpm: f64,
    max_bpm: f64,
    ln_type: &'static str,
    has_long_notes: bool,
    has_mines: bool,
}

impl ChartStats {
    fn from_chart(chart: &PlayableChart) -> Self {
        let mut min_bpm = chart.metadata.initial_bpm;
        let mut max_bpm = chart.metadata.initial_bpm;
        for event in &chart.timing_events {
            if let TimingEventKind::BpmChange { bpm } = event.kind {
                min_bpm = min_bpm.min(bpm);
                max_bpm = max_bpm.max(bpm);
            }
        }

        let has_mines = chart
            .lane_notes
            .iter()
            .flat_map(|notes| notes.iter())
            .any(|note| note.kind == NoteKind::Mine);

        Self {
            min_bpm,
            max_bpm,
            ln_type: if chart.long_notes.is_empty() { "" } else { "LongNote" },
            has_long_notes: !chart.long_notes.is_empty(),
            has_mines,
        }
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn folder_path(path: &Path) -> String {
    path.parent().map(path_to_string).unwrap_or_default()
}

fn warning_details(warning: &ImportWarning) -> (&'static str, String) {
    match warning {
        ImportWarning::EncodingFallback => {
            ("EncodingFallback", "decoded chart as Shift_JIS".to_string())
        }
        ImportWarning::TextReplacementOccurred => {
            ("TextReplacementOccurred", "text decoder replaced invalid bytes".to_string())
        }
        ImportWarning::UnsupportedCommand { command } => {
            ("UnsupportedCommand", format!("unsupported command: {command}"))
        }
        ImportWarning::UnsupportedChannel { channel } => {
            ("UnsupportedChannel", format!("unsupported channel: {channel}"))
        }
        ImportWarning::MissingWavDefinition { key } => {
            ("MissingWavDefinition", format!("missing WAV definition: {key}"))
        }
        ImportWarning::MissingSoundFile { path } => {
            ("MissingSoundFile", format!("missing sound file: {}", path_to_string(path)))
        }
        ImportWarning::MissingBpmDefinition { key } => {
            ("MissingBpmDefinition", format!("missing BPM definition: {key}"))
        }
        ImportWarning::MissingStopDefinition { key } => {
            ("MissingStopDefinition", format!("missing STOP definition: {key}"))
        }
        ImportWarning::SuspiciousMeasureLength { measure } => {
            ("SuspiciousMeasureLength", format!("suspicious measure length: {measure}"))
        }
        ImportWarning::LnobjWithoutStart { lane } => {
            ("LnobjWithoutStart", format!("LNOBJ without start on lane {lane:?}"))
        }
        ImportWarning::UnterminatedLongNote { lane } => {
            ("UnterminatedLongNote", format!("unterminated long note on lane {lane:?}"))
        }
        ImportWarning::ConflictingLongNoteSyntax { lane } => {
            ("ConflictingLongNoteSyntax", format!("conflicting long note syntax on lane {lane:?}"))
        }
        ImportWarning::DuplicateDefinition { name } => {
            ("DuplicateDefinition", format!("duplicate definition: {name}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart};
    use bmz_core::time::TimeUs;

    use super::*;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    fn chart(title: &str) -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(title.as_bytes()),
            metadata: ChartMetadata {
                title: title.to_string(),
                artist: "artist".to_string(),
                initial_bpm: 128.0,
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(10_000_000),
        }
    }

    #[test]
    fn upsert_chart_import_persists_file_chart_and_link() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let chart = chart("song");
        let record = ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/song.bms"),
            file_size: 123,
            modified_at: 1_700_000_001,
            scanned_at: 1_700_000_002,
            chart: &chart,
        };

        let chart_id = db.upsert_chart_import(&record).unwrap();

        assert_eq!(db.chart_id_by_sha256(chart.identity.file_sha256).unwrap(), Some(chart_id));
        let (path, parse_status, title, mode, ln_type): (String, String, String, String, String) =
            db.conn()
                .query_row(
                    "SELECT chart_files.path, chart_files.parse_status, charts.title, charts.mode, charts.ln_type
                    FROM chart_file_links
                    JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
                    JOIN charts ON charts.id = chart_file_links.chart_id",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )
                .unwrap();

        assert_eq!(path, "/songs/song.bms");
        assert_eq!(parse_status, "Parsed");
        assert_eq!(title, "song");
        assert_eq!(mode, "7K");
        assert_eq!(ln_type, "");
    }

    #[test]
    fn replace_import_warnings_replaces_previous_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let chart = chart("song");
        let record = ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/song.bms"),
            file_size: 123,
            modified_at: 1,
            scanned_at: 2,
            chart: &chart,
        };
        db.upsert_chart_import(&record).unwrap();
        let chart_file_id = db.chart_file_id_by_path(record.file_path).unwrap().unwrap();

        db.replace_import_warnings(
            chart_file_id,
            &[ImportWarning::UnsupportedChannel { channel: 99 }],
            3,
        )
        .unwrap();
        db.replace_import_warnings(
            chart_file_id,
            &[ImportWarning::MissingWavDefinition { key: 10 }],
            4,
        )
        .unwrap();

        let (count, code): (u32, String) = db
            .conn()
            .query_row("SELECT COUNT(*), MAX(code) FROM chart_import_warnings", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(code, "MissingWavDefinition");
    }

    #[test]
    fn upsert_root_updates_existing_row() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let first = db.upsert_root(Path::new("/songs"), true, true).unwrap();
        let second = db.upsert_root(Path::new("/songs"), false, false).unwrap();
        db.update_root_scanned_at(first, 42).unwrap();

        let (count, enabled, recursive, last_scan_at): (u32, bool, bool, i64) = db
            .conn()
            .query_row("SELECT COUNT(*), enabled, recursive, last_scan_at FROM roots", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(count, 1);
        assert!(!enabled);
        assert!(!recursive);
        assert_eq!(last_scan_at, 42);
    }
}
