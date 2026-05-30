use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use bmz_chart::import::error::ImportWarning;
use bmz_chart::model::{NoteKind, PlayableChart, TimingEventKind};
use bmz_gameplay::gauge::gauge_total_for_chart;
use rusqlite::{Connection, OptionalExtension, params};

pub use super::course_db::{StoredCourse, StoredCourseEntry};
pub use super::difficulty_table_db::{
    DifficultyTableEntryRecord, DifficultyTableRecord, TableEntryRow,
};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};

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
    pub stage_file: String,
    pub banner_file: String,
    pub backbmp_file: String,
    pub preview_file: String,
    pub has_long_notes: bool,
    pub has_mines: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableEntryListItem {
    pub level: String,
    pub md5: String,
    pub sha256: String,
    pub title: String,
    pub artist: String,
    pub comment: String,
    pub chart: Option<ChartListItem>,
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
                params![hash_to_hex(&sha256)],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn chart_sha256_by_chart_id(&self, chart_id: i64) -> Result<Option<[u8; 32]>> {
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT sha256 FROM charts WHERE id = ?1 LIMIT 1",
                params![chart_id],
                |row| row.get(0),
            )
            .optional()?;
        match result {
            Some(hex) => Ok(Some(hex_to_hash::<32>(&hex)?)),
            None => Ok(None),
        }
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

    /// Returns the chart id linked to a chart file path, trying common path normalizations.
    pub fn chart_id_by_chart_file_path(&self, path: &Path) -> Result<Option<i64>> {
        for candidate in chart_file_path_candidates(path) {
            let Some(chart_file_id) = self.chart_file_id_by_path(Path::new(&candidate))? else {
                continue;
            };
            let chart_id = self
                .conn
                .query_row(
                    "SELECT chart_id FROM chart_file_links WHERE chart_file_id = ?1 LIMIT 1",
                    params![chart_file_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            if chart_id.is_some() {
                return Ok(chart_id);
            }
        }
        Ok(None)
    }

    /// トランザクションを管理せずにインポート警告を置き換える。
    /// 戻り値は実際に挿入した（重複排除後の）警告行数。
    pub fn write_import_warnings(
        conn: &Connection,
        chart_file_id: i64,
        warnings: &[ImportWarning],
        created_at: i64,
    ) -> Result<usize> {
        conn.prepare_cached("DELETE FROM chart_import_warnings WHERE chart_file_id = ?1")?
            .execute(params![chart_file_id])?;
        // 同一 (code, message) の警告は1行にまとめる。
        // 非対応チャンネル等はオブジェクトごとに警告が出るため、重複排除しないと
        // warnings テーブルが数千行/チャート規模に膨張する。
        let mut seen = std::collections::HashSet::new();
        for warning in warnings {
            let (code, message) = warning_details(warning);
            if !seen.insert((code.clone(), message.clone())) {
                continue;
            }
            conn.prepare_cached(
                "INSERT INTO chart_import_warnings (chart_file_id, code, message, created_at)
                VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(params![chart_file_id, code, message, created_at])?;
        }
        Ok(seen.len())
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
        let chart_file_id: i64 = conn
            .prepare_cached(
                "INSERT INTO chart_files (
                root_id, path, file_size, modified_at, md5, sha256, scanned_at, parse_status
            ) VALUES (?1, ?2, ?3, ?4, '', '', ?5, 'Failed')
            ON CONFLICT(path) DO UPDATE SET
                root_id = excluded.root_id,
                file_size = excluded.file_size,
                modified_at = excluded.modified_at,
                scanned_at = excluded.scanned_at,
                parse_status = excluded.parse_status
            RETURNING id",
            )?
            .query_row(
                params![
                    root_id,
                    path_to_string(file_path),
                    file_size as i64,
                    modified_at,
                    scanned_at
                ],
                |row| row.get(0),
            )?;
        conn.prepare_cached("DELETE FROM chart_import_warnings WHERE chart_file_id = ?1")?
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
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS}
            FROM charts
            ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE
            LIMIT ?1 OFFSET ?2"
        ))?;

        let rows = stmt.query_map(params![limit, offset], chart_list_item_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Returns distinct immediate child folder names directly under `parent_path`.
    /// Only the last path component (name) is returned, not the full path.
    pub fn list_child_folder_names(&self, parent_path: &str) -> Result<Vec<String>> {
        let parent_path = to_folder_key(parent_path);
        // 直下の子だけが欲しいので、子孫を 1 回引いて Rust 側で
        // 直下名を抽出する。range 条件 ( `folder_path >= prefix AND < end` )
        // により idx_charts_folder_path をレンジスキャンで使える。
        let descendants = self.list_descendant_folder_paths_for_key(&parent_path)?;
        let mut names: Vec<String> = Vec::new();
        let prefix_len = parent_path.len() + 1; // including the trailing '/'
        for path in descendants {
            let rest = &path[prefix_len..];
            let name = match rest.find('/') {
                Some(idx) => &rest[..idx],
                None => rest,
            };
            if name.is_empty() {
                continue;
            }
            names.push(name.to_string());
        }
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        Ok(names)
    }

    /// Returns all distinct `folder_path` values that are strict descendants of
    /// `parent_path` (i.e. starting with `parent_path + '/'`).
    ///
    /// Uses a range condition on the indexed `folder_path` column, so it scales
    /// to libraries with tens of thousands of charts without a full table scan.
    pub fn list_descendant_folder_paths(&self, parent_path: &str) -> Result<Vec<String>> {
        let parent_path = to_folder_key(parent_path);
        self.list_descendant_folder_paths_for_key(&parent_path)
    }

    fn list_descendant_folder_paths_for_key(&self, parent_key: &str) -> Result<Vec<String>> {
        // ASCII '/' は 0x2F、'0' は 0x30。`prefix..end` は `prefix` で始まる
        // 文字列だけを範囲指定でき、idx_charts_folder_path を使ったレンジ
        // スキャンになる。
        let prefix = format!("{parent_key}/");
        let end = format!("{parent_key}0");
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT folder_path FROM charts
             WHERE folder_path >= ?1 AND folder_path < ?2",
        )?;
        let rows = stmt.query_map(params![prefix, end], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Returns charts in any of the given folder paths.
    ///
    /// Reuses a single prepared `WHERE folder_path = ?1` statement instead of
    /// expanding to `IN (?,?,...)`, so the SQLite bind-variable limit
    /// (`SQLITE_MAX_VARIABLE_NUMBER`) is never hit even for huge folder sets.
    pub fn list_charts_in_folders(&self, folder_paths: &[&str]) -> Result<Vec<ChartListItem>> {
        if folder_paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS}
            FROM charts
            WHERE folder_path = ?1
            ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE"
        ))?;
        let mut out = Vec::new();
        for path in folder_paths {
            let key = to_folder_key(path);
            let rows = stmt.query_map(params![key], chart_list_item_from_row)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    /// Returns charts whose `chart_id` is one of the given ids.
    /// Order in the returned vector is unspecified.
    pub fn list_charts_by_ids(&self, ids: &[i64]) -> Result<Vec<ChartListItem>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {CHART_LIST_ITEM_COLUMNS} FROM charts WHERE id = ?1"))?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let row = stmt.query_row(params![id], chart_list_item_from_row).ok();
            if let Some(row) = row {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// Returns charts whose `folder_path` exactly matches `folder_path`.
    pub fn list_charts_in_folder(&self, folder_path: &str) -> Result<Vec<ChartListItem>> {
        let folder_path = to_folder_key(folder_path);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS}
            FROM charts
            WHERE folder_path = ?1
            ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE"
        ))?;
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
            Ok((
                path,
                ChartFileFingerprint {
                    file_size: file_size.max(0) as u64,
                    modified_at: row.get(2)?,
                    import_version: row.get(3)?,
                },
            ))
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

    /// Returns every entry of the given difficulty table, including entries that
    /// are not present in the local library.  Matched charts use MD5 first, then
    /// SHA-256.
    pub fn list_table_entries_with_chart(
        &self,
        source_url: &str,
    ) -> Result<Vec<TableEntryListItem>> {
        let rows = super::difficulty_table_db::list_table_entries(&self.conn, source_url)?;
        let md5_refs: Vec<&str> =
            rows.iter().filter(|row| row.md5.len() >= 24).map(|row| row.md5.as_str()).collect();
        let sha256_refs: Vec<&str> = rows
            .iter()
            .filter(|row| row.sha256.len() >= 24)
            .map(|row| row.sha256.as_str())
            .collect();
        let md5_charts = self.charts_by_md5s(&md5_refs)?;
        let sha256_charts = self.charts_by_sha256s(&sha256_refs)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let chart = md5_charts
                    .get(&row.md5)
                    .cloned()
                    .or_else(|| sha256_charts.get(&row.sha256).cloned());
                TableEntryListItem {
                    level: row.level,
                    md5: row.md5,
                    sha256: row.sha256,
                    title: row.title,
                    artist: row.artist,
                    comment: row.comment,
                    chart,
                }
            })
            .collect())
    }

    fn charts_by_md5s(&self, md5s: &[&str]) -> Result<HashMap<String, ChartListItem>> {
        charts_by_hash_column(&self.conn, "md5", md5s)
    }

    fn charts_by_sha256s(&self, sha256s: &[&str]) -> Result<HashMap<String, ChartListItem>> {
        charts_by_hash_column(&self.conn, "sha256", sha256s)
    }

    pub fn upsert_course(
        &mut self,
        source: &str,
        course: &bmz_core::course::CourseDefinition,
        source_position: i64,
        imported_at: i64,
    ) -> Result<i64> {
        super::course_db::upsert_course(
            &mut self.conn,
            source,
            course,
            source_position,
            imported_at,
        )
    }

    pub fn list_courses(&self) -> Result<Vec<StoredCourse>> {
        super::course_db::list_courses(&self.conn)
    }

    pub fn list_courses_by_source(&self, source: &str) -> Result<Vec<StoredCourse>> {
        super::course_db::list_courses_by_source(&self.conn, source)
    }

    pub fn list_course_entries(&self, course_id: i64) -> Result<Vec<StoredCourseEntry>> {
        super::course_db::list_course_entries(&self.conn, course_id)
    }

    /// Returns `(ChartListItem, raw_level)` pairs for charts in the library that
    /// appear in the given difficulty table, matched first by MD5 then by SHA-256.
    /// Charts not present in the local library are omitted.
    ///
    /// Prefer [`Self::list_table_entries_with_chart`] when table entries without a
    /// local chart should be included.
    pub fn list_charts_with_level_in_table(
        &self,
        source_url: &str,
    ) -> Result<Vec<(ChartListItem, String)>> {
        // Use UNION (not UNION ALL) so that a chart matched by both MD5 and SHA-256
        // for the same entry only appears once.
        let sql = format!(
            "
            SELECT {CHART_LIST_ITEM_COLUMNS_C}, dte.level
            FROM difficulty_table_entries dte
            JOIN difficulty_tables dt ON dt.id = dte.table_id
            JOIN charts c ON c.md5 = dte.md5
            WHERE dt.source_url = ?1 AND length(dte.md5) >= 24
            UNION
            SELECT {CHART_LIST_ITEM_COLUMNS_C}, dte.level
            FROM difficulty_table_entries dte
            JOIN difficulty_tables dt ON dt.id = dte.table_id
            JOIN charts c ON c.sha256 = dte.sha256
            WHERE dt.source_url = ?1 AND length(dte.sha256) >= 24"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![source_url], |row| {
            let chart = chart_list_item_from_row(row)?;
            let level: String = row.get(21)?;
            Ok((chart, level))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

const CHART_LIST_ITEM_LOOKUP_SQL: &str = "
    SELECT id, md5, sha256, title, subtitle, artist,
           difficulty_name, play_level, mode, total_notes,
           initial_bpm, COALESCE(min_bpm, initial_bpm),
           COALESCE(max_bpm, initial_bpm), length_ms, folder_path,
           stage_file, banner_file, backbmp_file, preview_file,
           has_long_notes, has_mines
    FROM charts
    WHERE {column} = ?1
    ORDER BY id DESC
    LIMIT 1";

fn charts_by_hash_column(
    conn: &Connection,
    column: &'static str,
    hashes: &[&str],
) -> Result<HashMap<String, ChartListItem>> {
    debug_assert!(matches!(column, "md5" | "sha256"));
    let mut map = HashMap::new();
    if hashes.is_empty() {
        return Ok(map);
    }
    let sql = CHART_LIST_ITEM_LOOKUP_SQL.replace("{column}", column);
    let mut stmt = conn.prepare(&sql)?;
    for hash in hashes {
        if map.contains_key(*hash) {
            continue;
        }
        let chart = stmt.query_row(params![hash], chart_list_item_from_row).optional()?;
        if let Some(chart) = chart {
            map.insert((*hash).to_string(), chart);
        }
    }
    Ok(map)
}

const CHART_LIST_ITEM_COLUMNS: &str = "
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
    folder_path,
    stage_file,
    banner_file,
    backbmp_file,
    preview_file,
    has_long_notes,
    has_mines";

const CHART_LIST_ITEM_COLUMNS_C: &str = "
    c.id,
    c.md5,
    c.sha256,
    c.title,
    c.subtitle,
    c.artist,
    c.difficulty_name,
    c.play_level,
    c.mode,
    c.total_notes,
    c.initial_bpm,
    COALESCE(c.min_bpm, c.initial_bpm),
    COALESCE(c.max_bpm, c.initial_bpm),
    c.length_ms,
    c.folder_path,
    c.stage_file,
    c.banner_file,
    c.backbmp_file,
    c.preview_file,
    c.has_long_notes,
    c.has_mines";

fn chart_list_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChartListItem> {
    let md5_hex: String = row.get(1)?;
    let md5 = hex_to_hash::<16>(&md5_hex)?;
    let sha256_hex: String = row.get(2)?;
    let sha256 = hex_to_hash::<32>(&sha256_hex)?;

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
        stage_file: row.get(15)?,
        banner_file: row.get(16)?,
        backbmp_file: row.get(17)?,
        preview_file: row.get(18)?,
        has_long_notes: row.get(19)?,
        has_mines: row.get(20)?,
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
            hash_to_hex(&record.chart.identity.file_md5),
            hash_to_hex(&record.chart.identity.file_sha256),
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
            has_mines, folder_path, stage_file, preview_file,
            banner_file, backbmp_file, judge_rank, gauge_total, import_version
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
        )",
    )?
    .execute(params![
        hash_to_hex(&chart.identity.file_sha256),
        hash_to_hex(&chart.identity.file_md5),
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
        chart.metadata.banner_file.as_str(),
        chart.metadata.backbmp_file.as_str(),
        chart.metadata.judge_rank,
        gauge_total_for_chart(chart.metadata.total, chart.total_notes),
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
            banner_file = ?23, backbmp_file = ?24, judge_rank = ?25, gauge_total = ?26,
            import_version = ?27
         WHERE id = ?28",
    )?
    .execute(params![
        hash_to_hex(&chart.identity.file_sha256),
        hash_to_hex(&chart.identity.file_md5),
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
        chart.metadata.banner_file.as_str(),
        chart.metadata.backbmp_file.as_str(),
        chart.metadata.judge_rank,
        gauge_total_for_chart(chart.metadata.total, chart.total_notes),
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

fn chart_file_path_candidates(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |value: String| {
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    };
    push(path_to_string(path));
    push(to_folder_key(&path_to_string(path)));
    if let Ok(canonical) = path.canonicalize() {
        push(path_to_string(&canonical));
        push(to_folder_key(&path_to_string(&canonical)));
    }
    out
}

/// `charts.folder_path` はスラッシュ `/` を正準とする。
/// Windows のバックスラッシュ区切りをスラッシュに変換する。
fn to_folder_key(path: &str) -> String {
    path.replace('\\', "/")
}

fn folder_path(path: &Path) -> String {
    to_folder_key(&path.parent().map(path_to_string).unwrap_or_default())
}

fn warning_details(warning: &ImportWarning) -> (String, String) {
    match warning {
        ImportWarning::EncodingFallback => {
            ("EncodingFallback".into(), "decoded chart as Shift_JIS".to_string())
        }
        ImportWarning::TextReplacementOccurred => {
            ("TextReplacementOccurred".into(), "text decoder replaced invalid bytes".to_string())
        }
        ImportWarning::ParserDiagnostic { code, message } => {
            // bms-rs から細分化済みの code をそのまま `chart_import_warnings.code` に保存する。
            (code.clone(), message.clone())
        }
        ImportWarning::UnsupportedCommand { command } => {
            ("UnsupportedCommand".into(), format!("unsupported command: {command}"))
        }
        ImportWarning::UnsupportedChannel { channel } => {
            ("UnsupportedChannel".into(), format!("unsupported channel: {channel}"))
        }
        ImportWarning::MissingWavDefinition { key } => {
            ("MissingWavDefinition".into(), format!("missing WAV definition: {key}"))
        }
        ImportWarning::MissingSoundFile { path } => {
            ("MissingSoundFile".into(), format!("missing sound file: {}", path_to_string(path)))
        }
        ImportWarning::MissingBmpDefinition { key } => {
            ("MissingBmpDefinition".into(), format!("missing BMP definition: {key}"))
        }
        ImportWarning::MissingBmpFile { path } => {
            ("MissingBmpFile".into(), format!("missing BMP file: {}", path_to_string(path)))
        }
        ImportWarning::MissingBpmDefinition { key } => {
            ("MissingBpmDefinition".into(), format!("missing BPM definition: {key}"))
        }
        ImportWarning::MissingStopDefinition { key } => {
            ("MissingStopDefinition".into(), format!("missing STOP definition: {key}"))
        }
        ImportWarning::LnobjWithoutStart { lane } => {
            ("LnobjWithoutStart".into(), format!("LNOBJ without start on lane {lane:?}"))
        }
        ImportWarning::UnterminatedLongNote { lane } => {
            ("UnterminatedLongNote".into(), format!("unterminated long note on lane {lane:?}"))
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

            scroll_events: Vec::new(),

            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
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
    fn chart_id_by_chart_file_path_resolves_linked_chart() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let chart = chart("boot");
        let chart_id = db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: Path::new("/songs/boot.bms"),
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &chart,
            })
            .unwrap();

        assert_eq!(
            db.chart_id_by_chart_file_path(Path::new("/songs/boot.bms")).unwrap(),
            Some(chart_id)
        );
        assert_eq!(db.chart_id_by_chart_file_path(Path::new("/missing.bms")).unwrap(), None);
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
    fn charts_by_md5s_prefers_newest_chart_id() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let same_chart = chart("duplicate");
        let stale_id =
            db.upsert_chart_import(&record_for_chart("/songs/a/track.bms", &same_chart)).unwrap();
        let fresh_id =
            db.upsert_chart_import(&record_for_chart("/songs/b/track.bms", &same_chart)).unwrap();
        assert!(stale_id < fresh_id);

        let md5 = hash_to_hex(&same_chart.identity.file_md5);
        let resolved = db.charts_by_md5s(&[md5.as_str()]).unwrap();

        assert_eq!(resolved.get(&md5).map(|chart| chart.chart_id), Some(fresh_id));
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

    #[test]
    fn folder_navigation_normalizes_backslash_separators() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let chart = chart("song");
        // file_path はスラッシュ区切りで与える（Path::parent() の OS 依存を避ける）。
        // folder_path は "G:/BMS/INSANE/sub" として保存される。
        db.upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("G:/BMS/INSANE/sub/song.bms"),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();

        // バックスラッシュ区切りの引数でも、スラッシュ保存された行が見つかること。
        assert_eq!(db.list_child_folder_names("G:\\BMS\\INSANE").unwrap(), vec!["sub".to_string()]);
        assert_eq!(db.list_charts_in_folder("G:\\BMS\\INSANE\\sub").unwrap().len(), 1);
    }

    #[test]
    fn list_descendant_folder_paths_returns_only_strict_descendants() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        for (i, path) in [
            "G:/BMS/INSANE/a/song.bms",
            "G:/BMS/INSANE/b/c/song.bms",
            "G:/BMS/INSANE/song.bms", // 親そのもの直下: 子孫扱いしない
            "G:/BMS/OTHER/song.bms",  // 別ルート: 含まれない
        ]
        .iter()
        .enumerate()
        {
            let c = chart(&format!("s{i}"));
            db.upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: Path::new(path),
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &c,
            })
            .unwrap();
        }

        let mut got = db.list_descendant_folder_paths("G:/BMS/INSANE").unwrap();
        got.sort();
        assert_eq!(got, vec!["G:/BMS/INSANE/a", "G:/BMS/INSANE/b/c"]);
    }

    #[test]
    fn list_charts_in_folders_collects_charts_across_paths() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        db.upsert_chart_import(&record_for_chart("/songs/a/song.bms", &chart("A"))).unwrap();
        db.upsert_chart_import(&record_for_chart("/songs/b/song.bms", &chart("B"))).unwrap();
        db.upsert_chart_import(&record_for_chart("/songs/c/song.bms", &chart("C"))).unwrap();

        let got = db.list_charts_in_folders(&["/songs/a", "/songs/c"]).unwrap();
        let titles: Vec<_> = got.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["A", "C"]);

        assert!(db.list_charts_in_folders(&[]).unwrap().is_empty());
    }

    #[test]
    fn charts_hash_columns_are_lowercase_hex_text() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let chart = chart("song");
        db.upsert_chart_import(&record_for_chart("/songs/song.bms", &chart)).unwrap();

        let (md5_typeof, sha256_typeof, md5_hex, sha256_hex): (String, String, String, String) = db
            .conn()
            .query_row("SELECT typeof(md5), typeof(sha256), md5, sha256 FROM charts", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap();
        assert_eq!(md5_typeof, "text");
        assert_eq!(sha256_typeof, "text");
        assert_eq!(md5_hex.len(), 32);
        assert_eq!(sha256_hex.len(), 64);
        assert!(md5_hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(sha256_hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

        // chart_files も同様に小文字 hex TEXT。
        let (cf_md5_typeof, cf_sha256_typeof): (String, String) = db
            .conn()
            .query_row("SELECT typeof(md5), typeof(sha256) FROM chart_files", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(cf_md5_typeof, "text");
        assert_eq!(cf_sha256_typeof, "text");
    }

    #[test]
    fn list_charts_with_level_in_table_uses_hash_indexes() {
        // 難易度表結合が `c.md5 = dte.md5` の通常 JOIN になり、`idx_charts_md5` /
        // `idx_charts_sha256` でルックアップされることを EXPLAIN QUERY PLAN で確認する。
        // 関数結合（`lower(hex(c.md5)) = dte.md5`）に戻ると SCAN charts になる。
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();

        let plan: Vec<String> = conn
            .prepare(
                "EXPLAIN QUERY PLAN
                SELECT c.id FROM difficulty_table_entries dte
                JOIN difficulty_tables dt ON dt.id = dte.table_id
                JOIN charts c ON c.md5 = dte.md5
                WHERE dt.source_url = ?1 AND length(dte.md5) >= 24",
            )
            .unwrap()
            .query_map(params!["http://example.com/"], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let combined = plan.join("\n");
        assert!(
            combined.contains("idx_charts_md5"),
            "expected idx_charts_md5 to be used, got:\n{combined}"
        );
        assert!(
            !combined.contains("SCAN c "),
            "expected charts to be searched via index, not full scanned:\n{combined}"
        );
    }
}
