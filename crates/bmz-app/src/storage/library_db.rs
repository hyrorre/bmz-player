use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use bmz_chart::import::error::ImportWarning;
use bmz_chart::model::{NoteKind, PlayableChart, TimingEventKind};
use rusqlite::{Connection, OptionalExtension, params};

pub use super::difficulty_table_db::{DifficultyTableEntryRecord, DifficultyTableRecord};

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

#[derive(Debug, Clone, PartialEq)]
pub struct ChartListItem {
    pub chart_id: i64,
    pub md5: [u8; 16],
    pub sha256: [u8; 32],
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub difficulty_name: String,
    pub play_level: String,
    pub mode: String,
    pub total_notes: u32,
    pub initial_bpm: f64,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub length_ms: i64,
    pub folder_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedChartFile {
    pub chart_file_id: i64,
    pub path: String,
    pub message: String,
    pub scanned_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartFileFingerprint {
    pub file_size: u64,
    pub modified_at: i64,
    pub import_version: i64,
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

    /// トランザクションを管理せずにチャートをupsertする。
    /// `conn` にはアクティブなトランザクション（またはコネクション）を渡す。
    /// 戻り値は `(chart_id, chart_file_id)`。
    pub fn write_chart_import(
        conn: &Connection,
        record: &ChartImportRecord<'_>,
    ) -> Result<(i64, i64)> {
        let chart_file_id = upsert_chart_file(conn, record)?;

        let existing_chart_id: Option<i64> = conn
            .query_row(
                "SELECT chart_id FROM chart_file_links WHERE chart_file_id = ?1",
                params![chart_file_id],
                |row| row.get(0),
            )
            .optional()?;

        let chart_id = if let Some(existing_id) = existing_chart_id {
            update_chart(conn, existing_id, record)?;
            existing_id
        } else {
            let new_id = insert_chart(conn, record)?;
            conn.execute(
                "INSERT INTO chart_file_links (chart_id, chart_file_id) VALUES (?1, ?2)",
                params![new_id, chart_file_id],
            )?;
            new_id
        };

        Ok((chart_id, chart_file_id))
    }

    pub fn upsert_chart_import(&mut self, record: &ChartImportRecord<'_>) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let (chart_id, _) = Self::write_chart_import(&tx, record)?;
        tx.commit()?;
        Ok(chart_id)
    }

    pub fn chart_id_by_title(&self, title: &str) -> Result<Option<i64>> {
        self.conn
            .query_row("SELECT id FROM charts WHERE title = ?1 LIMIT 1", params![title], |row| {
                row.get(0)
            })
            .optional()
            .map_err(Into::into)
    }

    pub fn chart_id_by_sha256(&self, sha256: [u8; 32]) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM charts WHERE sha256 = ?1 LIMIT 1",
                params![sha256.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn chart_sha256_by_chart_id(&self, chart_id: i64) -> Result<Option<[u8; 32]>> {
        let result: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT sha256 FROM charts WHERE id = ?1 LIMIT 1",
                params![chart_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.and_then(|blob| {
            if blob.len() == 32 {
                let mut out = [0_u8; 32];
                out.copy_from_slice(&blob);
                Some(out)
            } else {
                None
            }
        }))
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

    /// トランザクションを管理せずにインポート警告を置き換える。
    pub fn write_import_warnings(
        conn: &Connection,
        chart_file_id: i64,
        warnings: &[ImportWarning],
        created_at: i64,
    ) -> Result<()> {
        conn.prepare_cached(
            "DELETE FROM chart_import_warnings WHERE chart_file_id = ?1",
        )?
        .execute(params![chart_file_id])?;
        for warning in warnings {
            let (code, message) = warning_details(warning);
            conn.prepare_cached(
                "INSERT INTO chart_import_warnings (chart_file_id, code, message, created_at)
                VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(params![chart_file_id, code, message, created_at])?;
        }
        Ok(())
    }

    pub fn replace_import_warnings(
        &mut self,
        chart_file_id: i64,
        warnings: &[ImportWarning],
        created_at: i64,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        Self::write_import_warnings(&tx, chart_file_id, warnings, created_at)?;
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_root(&mut self, path: &Path, enabled: bool, recursive: bool) -> Result<i64> {
        self.conn
            .prepare_cached(
                "INSERT INTO roots (path, enabled, recursive)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(path) DO UPDATE SET
                    enabled = excluded.enabled,
                    recursive = excluded.recursive",
            )?
            .execute(params![path_to_string(path), enabled, recursive])?;

        self.conn
            .prepare_cached("SELECT id FROM roots WHERE path = ?1")?
            .query_row(params![path_to_string(path)], |row| row.get(0))
            .map_err(Into::into)
    }

    pub fn update_root_scanned_at(&mut self, root_id: i64, scanned_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE roots SET last_scan_at = ?1 WHERE id = ?2",
            params![scanned_at, root_id],
        )?;
        Ok(())
    }

    /// トランザクションを管理せずに失敗チャートを記録する。戻り値は `chart_file_id`。
    pub fn write_failed_chart(
        conn: &Connection,
        root_id: Option<i64>,
        file_path: &Path,
        file_size: u64,
        modified_at: i64,
        scanned_at: i64,
        message: &str,
    ) -> Result<i64> {
        let chart_file_id: i64 = conn.prepare_cached(
            "INSERT INTO chart_files (
                root_id, path, file_size, modified_at, md5, sha256, scanned_at, parse_status
            ) VALUES (?1, ?2, ?3, ?4, x'', x'', ?5, 'Failed')
            ON CONFLICT(path) DO UPDATE SET
                root_id = excluded.root_id,
                file_size = excluded.file_size,
                modified_at = excluded.modified_at,
                scanned_at = excluded.scanned_at,
                parse_status = excluded.parse_status
            RETURNING id",
        )?
        .query_row(
            params![root_id, path_to_string(file_path), file_size as i64, modified_at, scanned_at],
            |row| row.get(0),
        )?;
        conn.prepare_cached(
            "DELETE FROM chart_import_warnings WHERE chart_file_id = ?1",
        )?
        .execute(params![chart_file_id])?;
        conn.prepare_cached(
            "INSERT INTO chart_import_warnings (chart_file_id, code, message, created_at)
            VALUES (?1, 'ImportFailed', ?2, ?3)",
        )?
        .execute(params![chart_file_id, message, scanned_at])?;
        Ok(chart_file_id)
    }

    pub fn upsert_failed_chart_file(
        &mut self,
        root_id: Option<i64>,
        file_path: &Path,
        file_size: u64,
        modified_at: i64,
        scanned_at: i64,
        message: &str,
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let chart_file_id = Self::write_failed_chart(
            &tx,
            root_id,
            file_path,
            file_size,
            modified_at,
            scanned_at,
            message,
        )?;
        tx.commit()?;
        Ok(chart_file_id)
    }

    pub fn list_charts(&self, limit: u32, offset: u32) -> Result<Vec<ChartListItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                md5,
                sha256,
                title,
                subtitle,
                artist,
                difficulty_name,
                play_level,
                mode,
                total_notes,
                initial_bpm,
                COALESCE(min_bpm, initial_bpm),
                COALESCE(max_bpm, initial_bpm),
                length_ms,
                folder_path
            FROM charts
            ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE
            LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![limit, offset], chart_list_item_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Returns distinct immediate child folder names directly under `parent_path`.
    /// Only the last path component (name) is returned, not the full path.
    pub fn list_child_folder_names(&self, parent_path: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT child_name FROM (
                SELECT CASE
                    WHEN INSTR(rest, '/') > 0 THEN SUBSTR(rest, 1, INSTR(rest, '/') - 1)
                    ELSE rest
                END AS child_name
                FROM (
                    SELECT SUBSTR(folder_path, LENGTH(?1) + 2) AS rest
                    FROM charts
                    WHERE SUBSTR(folder_path, 1, LENGTH(?1) + 1) = ?1 || '/'
                )
            )
            WHERE child_name != ''
            ORDER BY child_name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![parent_path], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Returns charts whose `folder_path` exactly matches `folder_path`.
    pub fn list_charts_in_folder(&self, folder_path: &str) -> Result<Vec<ChartListItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                md5,
                sha256,
                title,
                subtitle,
                artist,
                difficulty_name,
                play_level,
                mode,
                total_notes,
                initial_bpm,
                COALESCE(min_bpm, initial_bpm),
                COALESCE(max_bpm, initial_bpm),
                length_ms,
                folder_path
            FROM charts
            WHERE folder_path = ?1
            ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![folder_path], chart_list_item_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn primary_chart_file_path(&self, chart_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT chart_files.path
                FROM chart_file_links
                JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
                WHERE chart_file_links.chart_id = ?1
                ORDER BY chart_files.path COLLATE NOCASE
                LIMIT 1",
                params![chart_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_failed_chart_files(&self, limit: u32, offset: u32) -> Result<Vec<FailedChartFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                chart_files.id,
                chart_files.path,
                COALESCE(chart_import_warnings.message, ''),
                chart_files.scanned_at
            FROM chart_files
            LEFT JOIN chart_import_warnings
                ON chart_import_warnings.chart_file_id = chart_files.id
            WHERE chart_files.parse_status = 'Failed'
            ORDER BY chart_files.scanned_at DESC, chart_files.path COLLATE NOCASE
            LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(FailedChartFile {
                chart_file_id: row.get(0)?,
                path: row.get(1)?,
                message: row.get(2)?,
                scanned_at: row.get(3)?,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn chart_file_fingerprint(&self, path: &Path) -> Result<Option<ChartFileFingerprint>> {
        self.conn
            .query_row(
                "SELECT chart_files.file_size, chart_files.modified_at, COALESCE(charts.import_version, 0)
                FROM chart_files
                LEFT JOIN chart_file_links
                    ON chart_file_links.chart_file_id = chart_files.id
                LEFT JOIN charts
                    ON charts.id = chart_file_links.chart_id
                WHERE chart_files.path = ?1
                LIMIT 1",
                params![path_to_string(path)],
                |row| {
                    let file_size: i64 = row.get(0)?;
                    Ok(ChartFileFingerprint {
                        file_size: file_size.max(0) as u64,
                        modified_at: row.get(1)?,
                        import_version: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn load_fingerprints_for_root(
        &self,
        root_id: i64,
    ) -> Result<HashMap<String, ChartFileFingerprint>> {
        let mut stmt = self.conn.prepare(
            "SELECT cf.path, cf.file_size, cf.modified_at, COALESCE(c.import_version, 0)
            FROM chart_files cf
            LEFT JOIN chart_file_links cfl ON cfl.chart_file_id = cf.id
            LEFT JOIN charts c ON c.id = cfl.chart_id
            WHERE cf.root_id = ?1",
        )?;
        let rows = stmt.query_map(params![root_id], |row| {
            let path: String = row.get(0)?;
            let file_size: i64 = row.get(1)?;
            Ok((path, ChartFileFingerprint {
                file_size: file_size.max(0) as u64,
                modified_at: row.get(2)?,
                import_version: row.get(3)?,
            }))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (path, fingerprint) = row?;
            map.insert(path, fingerprint);
        }
        Ok(map)
    }

    pub fn upsert_difficulty_table(
        &mut self,
        table: &crate::difficulty_table::FetchedDifficultyTable,
    ) -> Result<i64> {
        super::difficulty_table_db::upsert_difficulty_table(&mut self.conn, table)
    }

    pub fn list_difficulty_tables(&self) -> Result<Vec<DifficultyTableRecord>> {
        super::difficulty_table_db::list_difficulty_tables(&self.conn)
    }

    pub fn list_difficulty_table_entries_by_md5s(
        &self,
        md5s: &[&str],
    ) -> Result<Vec<DifficultyTableEntryRecord>> {
        super::difficulty_table_db::list_entries_by_md5s(&self.conn, md5s)
    }

    pub fn list_difficulty_table_entries_by_sha256s(
        &self,
        sha256s: &[&str],
    ) -> Result<Vec<DifficultyTableEntryRecord>> {
        super::difficulty_table_db::list_entries_by_sha256s(&self.conn, sha256s)
    }

    /// Returns `(ChartListItem, raw_level)` pairs for charts in the library that
    /// appear in the given difficulty table, matched first by MD5 then by SHA-256.
    /// Charts not present in the local library are omitted.
    pub fn list_charts_with_level_in_table(
        &self,
        source_url: &str,
    ) -> Result<Vec<(ChartListItem, String)>> {
        // Use UNION (not UNION ALL) so that a chart matched by both MD5 and SHA-256
        // for the same entry only appears once.
        let sql = "
            SELECT c.id, c.md5, c.sha256, c.title, c.subtitle, c.artist,
                   c.difficulty_name, c.play_level, c.mode, c.total_notes,
                   c.initial_bpm,
                   COALESCE(c.min_bpm, c.initial_bpm),
                   COALESCE(c.max_bpm, c.initial_bpm),
                   c.length_ms, c.folder_path, dte.level
            FROM difficulty_table_entries dte
            JOIN difficulty_tables dt ON dt.id = dte.table_id
            JOIN charts c ON lower(hex(c.md5)) = dte.md5
            WHERE dt.source_url = ?1 AND length(dte.md5) >= 24
            UNION
            SELECT c.id, c.md5, c.sha256, c.title, c.subtitle, c.artist,
                   c.difficulty_name, c.play_level, c.mode, c.total_notes,
                   c.initial_bpm,
                   COALESCE(c.min_bpm, c.initial_bpm),
                   COALESCE(c.max_bpm, c.initial_bpm),
                   c.length_ms, c.folder_path, dte.level
            FROM difficulty_table_entries dte
            JOIN difficulty_tables dt ON dt.id = dte.table_id
            JOIN charts c ON lower(hex(c.sha256)) = dte.sha256
            WHERE dt.source_url = ?1 AND length(dte.sha256) >= 24";

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![source_url], |row| {
            let chart = chart_list_item_from_row(row)?;
            let level: String = row.get(15)?;
            Ok((chart, level))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn chart_list_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChartListItem> {
    let md5_blob: Vec<u8> = row.get(1)?;
    let mut md5 = [0_u8; 16];
    md5.copy_from_slice(&md5_blob[..16]);
    let sha256_blob: Vec<u8> = row.get(2)?;
    let mut sha256 = [0_u8; 32];
    sha256.copy_from_slice(&sha256_blob[..32]);

    Ok(ChartListItem {
        chart_id: row.get(0)?,
        md5,
        sha256,
        title: row.get(3)?,
        subtitle: row.get(4)?,
        artist: row.get(5)?,
        difficulty_name: row.get(6)?,
        play_level: row.get(7)?,
        mode: row.get(8)?,
        total_notes: row.get(9)?,
        initial_bpm: row.get(10)?,
        min_bpm: row.get(11)?,
        max_bpm: row.get(12)?,
        length_ms: row.get(13)?,
        folder_path: row.get(14)?,
    })
}

fn upsert_chart_file(conn: &Connection, record: &ChartImportRecord<'_>) -> Result<i64> {
    conn.prepare_cached(
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
            parse_status = excluded.parse_status
        RETURNING id",
    )?
    .query_row(
        params![
            record.root_id,
            path_to_string(record.file_path),
            record.file_size as i64,
            record.modified_at,
            record.chart.identity.file_md5.as_slice(),
            record.chart.identity.file_sha256.as_slice(),
            record.scanned_at,
        ],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn insert_chart(conn: &Connection, record: &ChartImportRecord<'_>) -> Result<i64> {
    let chart = record.chart;
    let stats = ChartStats::from_chart(chart);
    conn.prepare_cached(
        "INSERT INTO charts (
            sha256, md5, title, subtitle, artist, subartist, genre,
            difficulty_name, play_level, mode, total_notes, initial_bpm,
            min_bpm, max_bpm, length_ms, ln_type, has_bga, has_long_notes,
            has_mines, folder_path, stage_file, preview_file, import_version
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
        )",
    )?
    .execute(params![
        chart.identity.file_sha256.as_slice(),
        chart.identity.file_md5.as_slice(),
        chart.metadata.title.as_str(),
        chart.metadata.subtitle.as_str(),
        chart.metadata.artist.as_str(),
        chart.metadata.subartist.as_str(),
        chart.metadata.genre.as_str(),
        chart.metadata.difficulty_name.as_str(),
        chart.metadata.play_level.as_str(),
        chart.metadata.key_mode.as_str(),
        chart.total_notes,
        chart.metadata.initial_bpm,
        stats.min_bpm,
        stats.max_bpm,
        chart.end_time.0 / 1_000,
        stats.ln_type,
        chart.metadata.has_bga,
        stats.has_long_notes,
        stats.has_mines,
        folder_path(record.file_path),
        chart.metadata.stage_file.as_str(),
        chart.metadata.preview_file.as_str(),
        CHART_IMPORT_VERSION,
    ])?;
    Ok(conn.last_insert_rowid())
}

fn update_chart(conn: &Connection, chart_id: i64, record: &ChartImportRecord<'_>) -> Result<()> {
    let chart = record.chart;
    let stats = ChartStats::from_chart(chart);
    conn.prepare_cached(
        "UPDATE charts SET
            sha256 = ?1, md5 = ?2, title = ?3, subtitle = ?4, artist = ?5,
            subartist = ?6, genre = ?7, difficulty_name = ?8, play_level = ?9,
            mode = ?10, total_notes = ?11, initial_bpm = ?12, min_bpm = ?13, max_bpm = ?14,
            length_ms = ?15, ln_type = ?16, has_bga = ?17, has_long_notes = ?18,
            has_mines = ?19, folder_path = ?20, stage_file = ?21, preview_file = ?22,
            import_version = ?23
         WHERE id = ?24",
    )?
    .execute(params![
        chart.identity.file_sha256.as_slice(),
        chart.identity.file_md5.as_slice(),
        chart.metadata.title.as_str(),
        chart.metadata.subtitle.as_str(),
        chart.metadata.artist.as_str(),
        chart.metadata.subartist.as_str(),
        chart.metadata.genre.as_str(),
        chart.metadata.difficulty_name.as_str(),
        chart.metadata.play_level.as_str(),
        chart.metadata.key_mode.as_str(),
        chart.total_notes,
        chart.metadata.initial_bpm,
        stats.min_bpm,
        stats.max_bpm,
        chart.end_time.0 / 1_000,
        stats.ln_type,
        chart.metadata.has_bga,
        stats.has_long_notes,
        stats.has_mines,
        folder_path(record.file_path),
        chart.metadata.stage_file.as_str(),
        chart.metadata.preview_file.as_str(),
        CHART_IMPORT_VERSION,
        chart_id,
    ])?;
    Ok(())
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
        ImportWarning::MissingBmpDefinition { key } => {
            ("MissingBmpDefinition", format!("missing BMP definition: {key}"))
        }
        ImportWarning::MissingBmpFile { path } => {
            ("MissingBmpFile", format!("missing BMP file: {}", path_to_string(path)))
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

    fn record_for_chart<'a>(path: &'a str, c: &'a PlayableChart) -> ChartImportRecord<'a> {
        ChartImportRecord {
            root_id: None,
            file_path: Path::new(path),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: c,
        }
    }

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
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
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
        let mut chart = chart("song");
        chart.metadata.has_bga = true;
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
        let (path, parse_status, title, mode, ln_type, has_bga): (
            String,
            String,
            String,
            String,
            String,
            bool,
        ) =
            db.conn()
                .query_row(
                    "SELECT chart_files.path, chart_files.parse_status, charts.title, charts.mode, charts.ln_type, charts.has_bga
                    FROM chart_file_links
                    JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
                    JOIN charts ON charts.id = chart_file_links.chart_id",
                    [],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .unwrap();

        assert_eq!(path, "/songs/song.bms");
        assert_eq!(parse_status, "Parsed");
        assert_eq!(title, "song");
        assert_eq!(mode, "7K");
        assert_eq!(ln_type, "");
        assert!(has_bga);
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

    #[test]
    fn list_charts_orders_by_title() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let alpha = chart("Alpha");
        let beta = chart("beta");

        db.upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/beta.bms"),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &beta,
        })
        .unwrap();
        db.upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/alpha.bms"),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &alpha,
        })
        .unwrap();

        let charts = db.list_charts(10, 0).unwrap();

        assert_eq!(charts.len(), 2);
        assert_eq!(charts[0].title, "Alpha");
        assert_eq!(charts[1].title, "beta");
        assert_eq!(charts[0].mode, "7K");
        assert_eq!(charts[0].length_ms, 10_000);
    }

    #[test]
    fn primary_chart_file_path_returns_linked_file() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let chart = chart("song");
        let chart_id = db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: Path::new("/songs/song.bms"),
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &chart,
            })
            .unwrap();

        assert_eq!(
            db.primary_chart_file_path(chart_id).unwrap(),
            Some("/songs/song.bms".to_string())
        );
        assert_eq!(db.primary_chart_file_path(chart_id + 1).unwrap(), None);
    }

    #[test]
    fn upsert_failed_chart_file_records_failure_status_and_warning() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let chart_file_id = db
            .upsert_failed_chart_file(None, Path::new("/songs/broken.bms"), 10, 1, 2, "broken")
            .unwrap();

        let (status, code): (String, String) = db
            .conn()
            .query_row(
                "SELECT chart_files.parse_status, chart_import_warnings.code
                FROM chart_files
                JOIN chart_import_warnings ON chart_import_warnings.chart_file_id = chart_files.id
                WHERE chart_files.id = ?1",
                params![chart_file_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(status, "Failed");
        assert_eq!(code, "ImportFailed");
    }

    #[test]
    fn list_failed_chart_files_returns_failures() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        db.upsert_failed_chart_file(None, Path::new("/songs/broken.bms"), 10, 1, 2, "broken")
            .unwrap();

        let failed = db.list_failed_chart_files(10, 0).unwrap();

        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].path, "/songs/broken.bms");
        assert_eq!(failed[0].message, "broken");
        assert_eq!(failed[0].scanned_at, 2);
    }

    #[test]
    fn upsert_chart_import_updates_chart_in_place_when_content_changes() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let v1 = chart("content-v1");
        let id1 = db.upsert_chart_import(&record_for_chart("/songs/track.bms", &v1)).unwrap();

        let v2 = chart("content-v2");
        let id2 = db.upsert_chart_import(&record_for_chart("/songs/track.bms", &v2)).unwrap();

        assert_eq!(id1, id2, "same path must return the same chart id");

        let count: i64 =
            db.conn().query_row("SELECT COUNT(*) FROM charts", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "re-import of same path must not create an extra chart row");

        let title: String = db
            .conn()
            .query_row("SELECT title FROM charts WHERE id = ?1", params![id2], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "content-v2");

        let link_count: i64 =
            db.conn().query_row("SELECT COUNT(*) FROM chart_file_links", [], |r| r.get(0)).unwrap();
        assert_eq!(link_count, 1);
    }

    #[test]
    fn upsert_chart_import_creates_separate_charts_for_different_paths_with_same_sha256() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let same_chart = chart("duplicate");
        let id_a =
            db.upsert_chart_import(&record_for_chart("/songs/a/track.bms", &same_chart)).unwrap();
        let id_b =
            db.upsert_chart_import(&record_for_chart("/songs/b/track.bms", &same_chart)).unwrap();

        assert_ne!(id_a, id_b, "different paths must produce separate chart records");

        let count: i64 =
            db.conn().query_row("SELECT COUNT(*) FROM charts", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn chart_file_fingerprint_reads_imported_file_metadata() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let chart = chart("song");
        db.upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/song.bms"),
            file_size: 123,
            modified_at: 456,
            scanned_at: 789,
            chart: &chart,
        })
        .unwrap();

        assert_eq!(
            db.chart_file_fingerprint(Path::new("/songs/song.bms")).unwrap(),
            Some(ChartFileFingerprint {
                file_size: 123,
                modified_at: 456,
                import_version: CHART_IMPORT_VERSION,
            })
        );
    }
}
