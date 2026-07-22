use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use bmz_chart::import::error::ImportWarning;
use bmz_chart::model::{LongNoteMode, NoteKind, PlayableChart, TimingEventKind};
use bmz_core::lane::Lane;
use bmz_gameplay::gauge::gauge_total_for_chart;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::ln_policy::{ChartLnCounts, ChartLnProfile, LnPolicySetting, LnScorePolicy};

pub use super::course_db::{StoredCourse, StoredCourseEntry};
pub use super::difficulty_table_db::{
    DifficultyTableEntryRecord, DifficultyTableRecord, TableEntryRow,
};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};

pub const CHART_IMPORT_VERSION: i64 = 6;
pub const CHART_LOUDNESS_ANALYSIS_VERSION: i64 = 1;
const MAX_ANALYSIS_DISTRIBUTION_SECONDS: usize = 10 * 60;

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
    pub subartist: String,
    pub genre: String,
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
    pub judge_rank: Option<i32>,
    /// Raw BMS `#TOTAL` (`model.getTotal()`). Unset charts store `0.0`.
    pub bms_total: f64,
    pub ln_profile: ChartLnProfile,
    pub ln_counts: ChartLnCounts,
}

impl ChartListItem {
    pub const fn scored_total_notes(&self, policy: LnScorePolicy) -> u32 {
        self.ln_counts.scored_total_notes(self.total_notes, policy)
    }

    pub fn scored_total_notes_for_setting(&self, setting: LnPolicySetting) -> u32 {
        self.ln_counts.scored_total_notes_for_setting(self.total_notes, setting)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartAnalysis {
    pub normal_notes: u32,
    pub long_notes: u32,
    pub scratch_notes: u32,
    pub long_scratch_notes: u32,
    pub density: f64,
    pub peak_density: f64,
    pub end_density: f64,
    pub total_gauge: f64,
    pub main_bpm: f64,
    pub distribution: Vec<ChartDistributionSecond>,
    pub speed_changes: Vec<ChartSpeedChange>,
    pub lane_notes: Vec<ChartLaneNotes>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartAnalysisSummary {
    pub normal_notes: u32,
    pub long_notes: u32,
    pub scratch_notes: u32,
    pub long_scratch_notes: u32,
    pub density: f64,
    pub peak_density: f64,
    pub end_density: f64,
    pub total_gauge: f64,
    pub main_bpm: f64,
    pub speed_changes: Vec<ChartSpeedChange>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartNormalizationAnalysis {
    pub loudness_lufs: f32,
    pub normalization_gain: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartDistributionSecond {
    pub scratch_long_heads: u16,
    pub scratch_long_bodies: u16,
    pub scratch_taps: u16,
    pub key_long_heads: u16,
    pub key_long_bodies: u16,
    pub key_taps: u16,
    pub mines: u16,
}

impl ChartDistributionSecond {
    fn playable_notes(self) -> u32 {
        u32::from(self.scratch_long_heads)
            + u32::from(self.scratch_long_bodies)
            + u32::from(self.scratch_taps)
            + u32::from(self.key_long_heads)
            + u32::from(self.key_long_bodies)
            + u32::from(self.key_taps)
    }

    fn is_empty(self) -> bool {
        self.scratch_long_heads == 0
            && self.scratch_long_bodies == 0
            && self.scratch_taps == 0
            && self.key_long_heads == 0
            && self.key_long_bodies == 0
            && self.key_taps == 0
            && self.mines == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChartSpeedChange {
    pub speed: f64,
    pub time_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartLaneNotes {
    pub lane_index: u8,
    pub normal_notes: u32,
    pub long_notes: u32,
    pub mines: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableEntryListItem {
    pub level: String,
    pub md5: String,
    pub sha256: String,
    pub title: String,
    pub artist: String,
    pub comment: String,
    pub url: String,
    pub append_url: String,
    pub ipfs: String,
    pub append_ipfs: String,
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

        write_chart_analysis(conn, chart_id, record.chart)?;
        super::course_db::backfill_unresolved_course_entries_for_chart(
            conn,
            &hash_to_hex(&record.chart.identity.file_sha256),
            &hash_to_hex(&record.chart.identity.file_md5),
        )?;

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

    /// Returns every chart row with the given file SHA-256.
    ///
    /// BMS collections often contain the same file in multiple folders; callers
    /// that resolve user collection state should keep those folder contexts.
    pub fn list_charts_by_sha256(&self, sha256: [u8; 32]) -> Result<Vec<ChartListItem>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS}
             FROM charts
             WHERE sha256 = ?1
             ORDER BY folder_path COLLATE NOCASE, title COLLATE NOCASE, play_level COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map(params![hash_to_hex(&sha256)], chart_list_item_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn chart_sha256_by_md5(&self, md5: [u8; 16]) -> Result<Option<[u8; 32]>> {
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT sha256 FROM charts WHERE md5 = ?1 ORDER BY id DESC LIMIT 1",
                params![hash_to_hex(&md5)],
                |row| row.get(0),
            )
            .optional()?;
        match result {
            Some(hex) => Ok(Some(hex_to_hash::<32>(&hex)?)),
            None => Ok(None),
        }
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
        let previous_chart_id: Option<i64> = conn
            .query_row(
                "SELECT chart_id FROM chart_file_links WHERE chart_file_id = ?1",
                params![chart_file_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(chart_id) = previous_chart_id {
            conn.prepare_cached("DELETE FROM chart_file_links WHERE chart_file_id = ?1")?
                .execute(params![chart_file_id])?;
            conn.prepare_cached("UPDATE course_entries SET chart_id = NULL WHERE chart_id = ?1")?
                .execute(params![chart_id])?;
            conn.prepare_cached(
                "DELETE FROM charts
                 WHERE id = ?1
                   AND NOT EXISTS (SELECT 1 FROM chart_file_links WHERE chart_id = ?1)",
            )?
            .execute(params![chart_id])?;
        }
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

    pub fn chart_analysis_by_chart_id(&self, chart_id: i64) -> Result<Option<ChartAnalysis>> {
        self.conn
            .query_row(
                "SELECT normal_notes, long_notes, scratch_notes, long_scratch_notes,
                    density, peak_density, end_density, total_gauge, main_bpm,
                    distribution_json, speed_changes_json, lane_notes_json
                 FROM chart_analysis
                 WHERE chart_id = ?1",
                params![chart_id],
                chart_analysis_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn chart_normalization_analysis_by_chart_id(
        &self,
        chart_id: i64,
    ) -> Result<Option<ChartNormalizationAnalysis>> {
        self.conn
            .query_row(
                "SELECT loudness_lufs, normalization_gain
                 FROM chart_analysis
                 WHERE chart_id = ?1
                    AND loudness_analysis_version = ?2
                    AND loudness_lufs IS NOT NULL
                    AND normalization_gain IS NOT NULL",
                params![chart_id, CHART_LOUDNESS_ANALYSIS_VERSION],
                |row| {
                    let loudness_lufs: f32 = row.get(0)?;
                    let normalization_gain: f32 = row.get(1)?;
                    Ok(ChartNormalizationAnalysis { loudness_lufs, normalization_gain })
                },
            )
            .optional()
            .map(|value| {
                value.filter(|analysis| {
                    analysis.loudness_lufs.is_finite()
                        && analysis.normalization_gain.is_finite()
                        && analysis.normalization_gain > 0.0
                })
            })
            .map_err(Into::into)
    }

    pub fn write_chart_normalization_analysis(
        &self,
        chart_id: i64,
        analysis: ChartNormalizationAnalysis,
    ) -> Result<()> {
        self.conn
            .prepare_cached(
                "UPDATE chart_analysis
             SET loudness_lufs = ?2,
                 normalization_gain = ?3,
                 loudness_analysis_version = ?4
             WHERE chart_id = ?1",
            )?
            .execute(params![
                chart_id,
                analysis.loudness_lufs,
                analysis.normalization_gain,
                CHART_LOUDNESS_ANALYSIS_VERSION,
            ])?;
        Ok(())
    }

    pub fn chart_analyses_by_chart_ids(&self, ids: &[i64]) -> Result<HashMap<i64, ChartAnalysis>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT chart_id, normal_notes, long_notes, scratch_notes, long_scratch_notes,
                density, peak_density, end_density, total_gauge, main_bpm,
                distribution_json, speed_changes_json, lane_notes_json
             FROM chart_analysis
             WHERE chart_id = ?1",
        )?;
        let mut out = HashMap::with_capacity(ids.len());
        for id in ids {
            if let Some((chart_id, analysis)) = stmt
                .query_row(params![id], |row| {
                    Ok((row.get(0)?, chart_analysis_from_row_with_offset(row, 1)?))
                })
                .optional()?
            {
                out.insert(chart_id, analysis);
            }
        }
        Ok(out)
    }

    pub fn chart_analysis_summaries_by_chart_ids(
        &self,
        ids: &[i64],
    ) -> Result<HashMap<i64, ChartAnalysisSummary>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut unique_ids = ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        let mut out = HashMap::with_capacity(ids.len());
        for chunk in unique_ids.chunks(CHART_ANALYSIS_LOOKUP_BATCH_SIZE) {
            let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT chart_id, normal_notes, long_notes, scratch_notes, long_scratch_notes,
                    density, peak_density, end_density, total_gauge, main_bpm, speed_changes_json
                 FROM chart_analysis
                 WHERE chart_id IN ({placeholders})"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
                    Ok((row.get(0)?, chart_analysis_summary_from_row_with_offset(row, 1)?))
                })?;
            for row in rows {
                let (chart_id, summary) = row?;
                out.insert(chart_id, summary);
            }
        }
        Ok(out)
    }

    pub fn chart_distributions_by_chart_ids(
        &self,
        ids: &[i64],
    ) -> Result<HashMap<i64, Vec<ChartDistributionSecond>>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT chart_id, distribution_json
             FROM chart_analysis
             WHERE chart_id = ?1",
        )?;
        let mut out = HashMap::with_capacity(ids.len());
        for id in ids {
            if let Some((chart_id, distribution_json)) = stmt
                .query_row(params![id], |row| Ok((row.get(0)?, row.get::<_, String>(1)?)))
                .optional()?
            {
                let distribution = decode_distribution(&distribution_json);
                out.insert(chart_id, distribution);
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

    /// Returns charts whose title / subtitle / artist / subartist / genre contain
    /// `query` as a case-insensitive substring. Equivalent to beatoraja
    /// `SQLiteSongDatabaseAccessor.getSongDatasByText`.
    pub fn search_charts(&self, query: &str) -> Result<Vec<ChartListItem>> {
        let pattern = format!("%{}%", escape_like(query));
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS}
            FROM charts
            WHERE title LIKE ?1 ESCAPE '\\'
               OR subtitle LIKE ?1 ESCAPE '\\'
               OR artist LIKE ?1 ESCAPE '\\'
               OR subartist LIKE ?1 ESCAPE '\\'
               OR genre LIKE ?1 ESCAPE '\\'
            GROUP BY sha256
            ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map(params![pattern], chart_list_item_from_row)?;
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
        self.list_table_entries_with_chart_at_level(source_url, None)
    }

    pub fn list_table_entries_with_chart_at_level(
        &self,
        source_url: &str,
        level: Option<&str>,
    ) -> Result<Vec<TableEntryListItem>> {
        let rows = match level {
            Some(level) => super::difficulty_table_db::list_table_entries_at_level(
                &self.conn, source_url, level,
            )?,
            None => super::difficulty_table_db::list_table_entries(&self.conn, source_url)?,
        };
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
                    url: row.url,
                    append_url: row.append_url,
                    ipfs: row.ipfs,
                    append_ipfs: row.append_ipfs,
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
            let level: String = row.get(33)?;
            Ok((chart, level))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// Keep batched hash lookups below SQLite's historical 999-variable default.
const CHART_HASH_LOOKUP_BATCH_SIZE: usize = 500;
const CHART_ANALYSIS_LOOKUP_BATCH_SIZE: usize = 500;

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

    let mut unique_hashes = hashes.to_vec();
    unique_hashes.sort_unstable();
    unique_hashes.dedup();

    for chunk in unique_hashes.chunks(CHART_HASH_LOOKUP_BATCH_SIZE) {
        let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS_C}, latest.lookup_hash
             FROM charts c
             JOIN (
                 SELECT {column} AS lookup_hash, MAX(id) AS chart_id
                 FROM charts
                 WHERE {column} IN ({placeholders})
                 GROUP BY {column}
             ) latest ON latest.chart_id = c.id"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
            Ok((row.get::<_, String>(33)?, chart_list_item_from_row(row)?))
        })?;
        for row in rows {
            let (hash, chart) = row?;
            map.insert(hash, chart);
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
    has_mines,
    judge_rank,
    has_undefined_ln,
    has_defined_ln,
    has_defined_cn,
    has_defined_hcn,
    subartist,
    genre,
    COALESCE(bms_total, 0),
    undefined_ln_pairs,
    defined_ln_pairs,
    defined_cn_pairs,
    defined_hcn_pairs";

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
    c.has_mines,
    c.judge_rank,
    c.has_undefined_ln,
    c.has_defined_ln,
    c.has_defined_cn,
    c.has_defined_hcn,
    c.subartist,
    c.genre,
    COALESCE(c.bms_total, 0),
    c.undefined_ln_pairs,
    c.defined_ln_pairs,
    c.defined_cn_pairs,
    c.defined_hcn_pairs";

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
        judge_rank: row.get(21)?,
        ln_profile: ChartLnProfile {
            has_undefined_ln: row.get(22)?,
            has_defined_ln: row.get(23)?,
            has_defined_cn: row.get(24)?,
            has_defined_hcn: row.get(25)?,
        },
        subartist: row.get(26)?,
        genre: row.get(27)?,
        bms_total: row.get(28)?,
        ln_counts: ChartLnCounts {
            undefined_ln_pairs: row.get(29)?,
            defined_ln_pairs: row.get(30)?,
            defined_cn_pairs: row.get(31)?,
            defined_hcn_pairs: row.get(32)?,
        },
    })
}

fn chart_bms_total(metadata_total: Option<f64>) -> f64 {
    metadata_total.unwrap_or(0.0)
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
            banner_file, backbmp_file, judge_rank, gauge_total, bms_total,
            has_undefined_ln, has_defined_ln, has_defined_cn, has_defined_hcn,
            undefined_ln_pairs, defined_ln_pairs, defined_cn_pairs, defined_hcn_pairs,
            has_bms_random, source_url, append_url, headers_json, import_version
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27,
            ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40
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
        gauge_total_for_chart(
            chart.metadata.total,
            stats.ln_counts.canonical_total_notes(chart.total_notes),
        ),
        chart_bms_total(chart.metadata.total),
        stats.ln_profile.has_undefined_ln,
        stats.ln_profile.has_defined_ln,
        stats.ln_profile.has_defined_cn,
        stats.ln_profile.has_defined_hcn,
        stats.ln_counts.undefined_ln_pairs,
        stats.ln_counts.defined_ln_pairs,
        stats.ln_counts.defined_cn_pairs,
        stats.ln_counts.defined_hcn_pairs,
        chart.metadata.has_bms_random,
        chart.metadata.source_url.as_str(),
        chart.metadata.append_url.as_str(),
        chart_headers_json(),
        CHART_IMPORT_VERSION,
    ])?;
    Ok(conn.last_insert_rowid())
}

fn chart_headers_json() -> &'static str {
    // Header values needed by the library have dedicated columns.  Do not
    // persist the raw header map: some BMS channel identifiers use Base62 and
    // were historically mistaken for headers, retaining complete note lines.
    "{}"
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
            bms_total = ?27, has_undefined_ln = ?28, has_defined_ln = ?29,
            has_defined_cn = ?30, has_defined_hcn = ?31,
            undefined_ln_pairs = ?32, defined_ln_pairs = ?33,
            defined_cn_pairs = ?34, defined_hcn_pairs = ?35, has_bms_random = ?36,
            source_url = ?37, append_url = ?38, headers_json = ?39,
            import_version = ?40
         WHERE id = ?41",
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
        gauge_total_for_chart(
            chart.metadata.total,
            stats.ln_counts.canonical_total_notes(chart.total_notes),
        ),
        chart_bms_total(chart.metadata.total),
        stats.ln_profile.has_undefined_ln,
        stats.ln_profile.has_defined_ln,
        stats.ln_profile.has_defined_cn,
        stats.ln_profile.has_defined_hcn,
        stats.ln_counts.undefined_ln_pairs,
        stats.ln_counts.defined_ln_pairs,
        stats.ln_counts.defined_cn_pairs,
        stats.ln_counts.defined_hcn_pairs,
        chart.metadata.has_bms_random,
        chart.metadata.source_url.as_str(),
        chart.metadata.append_url.as_str(),
        chart_headers_json(),
        CHART_IMPORT_VERSION,
        chart_id,
    ])?;
    Ok(())
}

fn write_chart_analysis(conn: &Connection, chart_id: i64, chart: &PlayableChart) -> Result<()> {
    let analysis = ChartAnalysis::from_chart(chart);
    conn.prepare_cached(
        "INSERT INTO chart_analysis (
            chart_id, normal_notes, long_notes, scratch_notes, long_scratch_notes,
            density, peak_density, end_density, total_gauge, main_bpm,
            distribution_json, speed_changes_json, lane_notes_json, analysis_version
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
        )
        ON CONFLICT(chart_id) DO UPDATE SET
            normal_notes = excluded.normal_notes,
            long_notes = excluded.long_notes,
            scratch_notes = excluded.scratch_notes,
            long_scratch_notes = excluded.long_scratch_notes,
            density = excluded.density,
            peak_density = excluded.peak_density,
            end_density = excluded.end_density,
            total_gauge = excluded.total_gauge,
            main_bpm = excluded.main_bpm,
            distribution_json = excluded.distribution_json,
            speed_changes_json = excluded.speed_changes_json,
            lane_notes_json = excluded.lane_notes_json,
            loudness_lufs = NULL,
            normalization_gain = NULL,
            loudness_analysis_version = 0,
            analysis_version = excluded.analysis_version",
    )?
    .execute(params![
        chart_id,
        analysis.normal_notes,
        analysis.long_notes,
        analysis.scratch_notes,
        analysis.long_scratch_notes,
        analysis.density,
        analysis.peak_density,
        analysis.end_density,
        analysis.total_gauge,
        analysis.main_bpm,
        encode_distribution_compact(&analysis.distribution),
        serde_json::to_string(&analysis.speed_changes)?,
        serde_json::to_string(&analysis.lane_notes)?,
        CHART_IMPORT_VERSION,
    ])?;
    Ok(())
}

impl ChartAnalysis {
    pub fn from_chart(chart: &PlayableChart) -> Self {
        let canonical_total_notes =
            ChartLnCounts::from_chart(chart).canonical_total_notes(chart.total_notes);
        let seconds = analysis_distribution_seconds(chart);
        let mut distribution = vec![ChartDistributionSecond::default(); seconds.max(1)];
        let mut lane_notes = Lane::ALL
            .iter()
            .map(|lane| ChartLaneNotes { lane_index: lane.index() as u8, ..Default::default() })
            .collect::<Vec<_>>();

        for pair in &chart.long_notes {
            let start_sec = second_index(pair.start_time.0);
            let end_sec = second_index(pair.end_time.0).min(distribution.len().saturating_sub(1));
            for second in distribution.iter_mut().take(end_sec + 1).skip(start_sec) {
                add_long_body(second, pair.lane, 1);
            }
        }

        let long_end_modes = chart
            .long_notes
            .iter()
            .map(|pair| (pair.end_note_id, pair.mode.unwrap_or(chart.metadata.long_note_mode)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut bpm_note_counts: Vec<(f64, u32)> = Vec::new();
        let mut total_countdown = canonical_total_notes as i64
            - gauge_border_note_count(chart.metadata.total, canonical_total_notes);
        let mut border_sec = 0usize;

        for notes in &chart.lane_notes {
            for note in notes {
                let sec = second_index(note.time.0);
                let Some(slot) = distribution.get_mut(sec) else {
                    continue;
                };
                let lane = note.lane;
                let lane_slot = &mut lane_notes[lane.index()];
                match note.kind {
                    NoteKind::Tap => {
                        if is_scratch_lane(lane) {
                            slot.scratch_taps = slot.scratch_taps.saturating_add(1);
                            lane_slot.normal_notes = lane_slot.normal_notes.saturating_add(1);
                        } else {
                            slot.key_taps = slot.key_taps.saturating_add(1);
                            lane_slot.normal_notes = lane_slot.normal_notes.saturating_add(1);
                        }
                        total_countdown -= 1;
                        add_bpm_note_count(&mut bpm_note_counts, bpm_at(chart, note.time.0), 1);
                    }
                    NoteKind::LongStart => {
                        add_long_head(slot, lane);
                        add_long_body(slot, lane, -1);
                        lane_slot.long_notes = lane_slot.long_notes.saturating_add(1);
                        total_countdown -= 1;
                        add_bpm_note_count(&mut bpm_note_counts, bpm_at(chart, note.time.0), 1);
                    }
                    NoteKind::LongEnd
                        if long_end_modes
                            .get(&note.id)
                            .is_some_and(|mode| *mode != LongNoteMode::Ln) =>
                    {
                        add_long_head(slot, lane);
                        lane_slot.long_notes = lane_slot.long_notes.saturating_add(1);
                        total_countdown -= 1;
                        add_bpm_note_count(&mut bpm_note_counts, bpm_at(chart, note.time.0), 1);
                    }
                    NoteKind::LongEnd => {}
                    NoteKind::Invisible => {}
                    NoteKind::Mine => {
                        slot.mines = slot.mines.saturating_add(1);
                        lane_slot.mines = lane_slot.mines.saturating_add(1);
                    }
                }
                if total_countdown == 0 {
                    border_sec = sec;
                }
            }
        }

        let peak_density =
            distribution.iter().map(|second| second.playable_notes()).max().unwrap_or(0) as f64;
        let threshold = canonical_total_notes as usize / distribution.len().max(1) / 4;
        let mut density_sum = 0u32;
        let mut density_count = 0u32;
        for notes in distribution.iter().map(|second| second.playable_notes()) {
            if notes as usize >= threshold {
                density_sum = density_sum.saturating_add(notes);
                density_count = density_count.saturating_add(1);
            }
        }
        let density: f64 = if density_count == 0 {
            0.0_f64
        } else {
            f64::from(density_sum) / f64::from(density_count)
        };

        let end_window = 5usize.min(distribution.len().saturating_sub(border_sec + 1));
        let mut end_density: f64 = 0.0;
        if end_window > 0 {
            for start in border_sec..distribution.len().saturating_sub(end_window) {
                let notes = (0..end_window)
                    .map(|offset| distribution[start + offset].playable_notes())
                    .sum::<u32>();
                end_density = end_density.max(f64::from(notes) / end_window as f64);
            }
        }

        let main_bpm: f64 = bpm_note_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(bpm, _)| bpm)
            .unwrap_or(chart.metadata.initial_bpm);

        let normal_notes = lane_notes.iter().map(|lane| lane.normal_notes).sum();
        let long_notes = lane_notes.iter().map(|lane| lane.long_notes).sum();
        let scratch_notes = lane_notes
            .iter()
            .filter(|lane| {
                lane.lane_index == Lane::Scratch.index() as u8
                    || lane.lane_index == Lane::Scratch2.index() as u8
            })
            .map(|lane| lane.normal_notes)
            .sum();
        let long_scratch_notes = lane_notes
            .iter()
            .filter(|lane| {
                lane.lane_index == Lane::Scratch.index() as u8
                    || lane.lane_index == Lane::Scratch2.index() as u8
            })
            .map(|lane| lane.long_notes)
            .sum();

        trim_trailing_empty_distribution(&mut distribution);

        Self {
            normal_notes,
            long_notes,
            scratch_notes,
            long_scratch_notes,
            density,
            peak_density,
            end_density,
            total_gauge: gauge_total_for_chart(chart.metadata.total, canonical_total_notes),
            main_bpm,
            distribution,
            speed_changes: chart_speed_changes(chart),
            lane_notes,
        }
    }
}

fn analysis_distribution_seconds(chart: &PlayableChart) -> usize {
    let last_note_us = chart
        .lane_notes
        .iter()
        .flat_map(|notes| notes.iter().map(|note| note.time.0))
        .max()
        .unwrap_or(0);
    let last_long_us = chart.long_notes.iter().map(|pair| pair.end_time.0).max().unwrap_or(0);
    let last_us = last_note_us.max(last_long_us).max(0);
    ((last_us / 1_000_000) as usize + 2).clamp(1, MAX_ANALYSIS_DISTRIBUTION_SECONDS)
}

fn trim_trailing_empty_distribution(distribution: &mut Vec<ChartDistributionSecond>) {
    while distribution.len() > 1
        && distribution.last().copied().is_some_and(ChartDistributionSecond::is_empty)
    {
        distribution.pop();
    }
}

fn encode_distribution_compact(distribution: &[ChartDistributionSecond]) -> String {
    let mut out = String::with_capacity(1 + distribution.len() * 14);
    out.push('#');
    for second in distribution {
        for value in [
            second.scratch_long_heads,
            second.scratch_long_bodies,
            second.scratch_taps,
            second.key_long_heads,
            second.key_long_bodies,
            second.key_taps,
            second.mines,
        ] {
            push_base36_2(&mut out, value);
        }
    }
    out
}

fn decode_distribution(value: &str) -> Vec<ChartDistributionSecond> {
    if let Some(compact) = value.strip_prefix('#') {
        return decode_distribution_compact(compact).unwrap_or_default();
    }
    serde_json::from_str(value).unwrap_or_default()
}

fn decode_distribution_compact(value: &str) -> Option<Vec<ChartDistributionSecond>> {
    if !value.len().is_multiple_of(14) || !value.is_ascii() {
        return None;
    }
    let mut out = Vec::with_capacity(value.len() / 14);
    for chunk in value.as_bytes().chunks_exact(14) {
        out.push(ChartDistributionSecond {
            scratch_long_heads: parse_base36_2(&chunk[0..2])?,
            scratch_long_bodies: parse_base36_2(&chunk[2..4])?,
            scratch_taps: parse_base36_2(&chunk[4..6])?,
            key_long_heads: parse_base36_2(&chunk[6..8])?,
            key_long_bodies: parse_base36_2(&chunk[8..10])?,
            key_taps: parse_base36_2(&chunk[10..12])?,
            mines: parse_base36_2(&chunk[12..14])?,
        });
    }
    Some(out)
}

fn push_base36_2(out: &mut String, value: u16) {
    let value = value.min(36 * 36 - 1);
    out.push(base36_digit((value / 36) as u8));
    out.push(base36_digit((value % 36) as u8));
}

fn parse_base36_2(bytes: &[u8]) -> Option<u16> {
    Some(u16::from(parse_base36_digit(bytes[0])?) * 36 + u16::from(parse_base36_digit(bytes[1])?))
}

fn base36_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=35 => (b'a' + value - 10) as char,
        _ => unreachable!("base36 digit out of range"),
    }
}

fn parse_base36_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'z' => Some(value - b'a' + 10),
        b'A'..=b'Z' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn chart_analysis_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChartAnalysis> {
    chart_analysis_from_row_with_offset(row, 0)
}

fn chart_analysis_from_row_with_offset(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<ChartAnalysis> {
    let distribution_json: String = row.get(offset + 9)?;
    let speed_changes_json: String = row.get(offset + 10)?;
    let lane_notes_json: String = row.get(offset + 11)?;
    Ok(ChartAnalysis {
        normal_notes: row.get(offset)?,
        long_notes: row.get(offset + 1)?,
        scratch_notes: row.get(offset + 2)?,
        long_scratch_notes: row.get(offset + 3)?,
        density: row.get(offset + 4)?,
        peak_density: row.get(offset + 5)?,
        end_density: row.get(offset + 6)?,
        total_gauge: row.get(offset + 7)?,
        main_bpm: row.get(offset + 8)?,
        distribution: decode_distribution(&distribution_json),
        speed_changes: serde_json::from_str(&speed_changes_json).unwrap_or_default(),
        lane_notes: serde_json::from_str(&lane_notes_json).unwrap_or_default(),
    })
}

fn chart_analysis_summary_from_row_with_offset(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<ChartAnalysisSummary> {
    Ok(ChartAnalysisSummary {
        normal_notes: row.get(offset)?,
        long_notes: row.get(offset + 1)?,
        scratch_notes: row.get(offset + 2)?,
        long_scratch_notes: row.get(offset + 3)?,
        density: row.get(offset + 4)?,
        peak_density: row.get(offset + 5)?,
        end_density: row.get(offset + 6)?,
        total_gauge: row.get(offset + 7)?,
        main_bpm: row.get(offset + 8)?,
        speed_changes: row
            .get::<_, Option<String>>(offset + 9)?
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default(),
    })
}

fn second_index(time_us: i64) -> usize {
    (time_us / 1_000_000).max(0) as usize
}

fn is_scratch_lane(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::Scratch2)
}

fn add_long_head(second: &mut ChartDistributionSecond, lane: Lane) {
    if is_scratch_lane(lane) {
        second.scratch_long_heads = second.scratch_long_heads.saturating_add(1);
    } else {
        second.key_long_heads = second.key_long_heads.saturating_add(1);
    }
}

fn add_long_body(second: &mut ChartDistributionSecond, lane: Lane, amount: i16) {
    let value = if is_scratch_lane(lane) {
        &mut second.scratch_long_bodies
    } else {
        &mut second.key_long_bodies
    };
    if amount >= 0 {
        *value = value.saturating_add(amount as u16);
    } else {
        *value = value.saturating_sub((-amount) as u16);
    }
}

fn gauge_border_note_count(total: Option<f64>, total_notes: u32) -> i64 {
    let total = total.unwrap_or(0.0);
    if total <= 0.0 {
        return 0;
    }
    (f64::from(total_notes) * (1.0 - 100.0 / total)) as i64
}

fn add_bpm_note_count(counts: &mut Vec<(f64, u32)>, bpm: f64, notes: u32) {
    if let Some((_, count)) =
        counts.iter_mut().find(|(candidate, _)| (*candidate - bpm).abs() < 0.0001)
    {
        *count = count.saturating_add(notes);
    } else {
        counts.push((bpm, notes));
    }
}

fn bpm_at(chart: &PlayableChart, time_us: i64) -> f64 {
    let mut bpm = chart.metadata.initial_bpm;
    for event in &chart.timing_events {
        if event.time.0 > time_us {
            break;
        }
        if let TimingEventKind::BpmChange { bpm: event_bpm } = event.kind {
            bpm = event_bpm;
        }
    }
    bpm
}

fn chart_speed_changes(chart: &PlayableChart) -> Vec<ChartSpeedChange> {
    let mut out = vec![ChartSpeedChange { speed: chart.metadata.initial_bpm, time_ms: 0 }];
    let mut current = chart.metadata.initial_bpm;
    // STOP 区間の終了時刻。Some の間は STOP 区間内とみなし、BPM 変化のみ先読みする。
    let mut stop_end_us: Option<i64> = None;
    for event in &chart.timing_events {
        let event_us = event.time.0;
        // STOP 区間を抜けたら resume エントリを出力してペンディングを解除する。
        if let Some(end_us) = stop_end_us {
            if event_us >= end_us {
                if end_us < chart.end_time.0 {
                    out.push(ChartSpeedChange { speed: current, time_ms: end_us / 1_000 });
                }
                stop_end_us = None;
                // fall through: このイベントを通常通り処理する
            } else {
                // STOP 区間内: BPM 変化だけ current に反映して次へ
                if let TimingEventKind::BpmChange { bpm } = event.kind {
                    current = bpm;
                }
                continue;
            }
        }
        match event.kind {
            TimingEventKind::Stop { duration_us } => {
                out.push(ChartSpeedChange { speed: 0.0, time_ms: event_us / 1_000 });
                // current は STOP 前の BPM のまま保持し、
                // STOP 区間内の BPM 変化を先読みするため stop_end_us をセットする。
                stop_end_us = Some(event_us + duration_us);
            }
            TimingEventKind::BpmChange { bpm } => {
                if (bpm - current).abs() > f64::EPSILON {
                    out.push(ChartSpeedChange { speed: bpm, time_ms: event_us / 1_000 });
                    current = bpm;
                }
            }
        }
    }
    // ループ終了後も STOP がペンディングなら resume エントリを出力する。
    if let Some(end_us) = stop_end_us
        && end_us < chart.end_time.0
    {
        out.push(ChartSpeedChange { speed: current, time_ms: end_us / 1_000 });
    }
    if out.last().is_some_and(|last| last.time_ms != chart.end_time.0 / 1_000) {
        out.push(ChartSpeedChange { speed: current, time_ms: chart.end_time.0 / 1_000 });
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct ChartStats {
    min_bpm: f64,
    max_bpm: f64,
    ln_type: &'static str,
    has_long_notes: bool,
    has_mines: bool,
    ln_profile: ChartLnProfile,
    ln_counts: ChartLnCounts,
}

impl ChartStats {
    fn from_chart(chart: &PlayableChart) -> Self {
        let mut min_bpm: f64 = chart.metadata.initial_bpm;
        let mut max_bpm: f64 = chart.metadata.initial_bpm;
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
        let ln_counts = ChartLnCounts::from_chart(chart);
        let ln_profile = ln_counts.profile();

        Self {
            min_bpm,
            max_bpm,
            ln_type: if chart.long_notes.is_empty() {
                ""
            } else if chart.metadata.long_note_mode_defined {
                match chart.metadata.long_note_mode {
                    LongNoteMode::Ln => "Ln",
                    LongNoteMode::Cn => "Cn",
                    LongNoteMode::Hcn => "Hcn",
                }
            } else {
                "LongNote"
            },
            has_long_notes: !chart.long_notes.is_empty(),
            has_mines,
            ln_profile,
            ln_counts,
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

/// Escapes SQL LIKE wildcards so user input is matched literally.
/// Pair with `LIKE ? ESCAPE '\\'`.
fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
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
        ImportWarning::UnsupportedPmsPlayerSide { side } => {
            ("UnsupportedPmsPlayerSide".into(), format!("unsupported PMS player side: {side}"))
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
    use bmz_chart::model::{ChartMetadata, LongNotePair, LongNoteStyle, NoteEvent, PlayableChart};
    use bmz_core::course::{CourseConstraints, CourseDefinition, CourseEntry, CourseKind};
    use bmz_core::ids::NoteId;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

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

    fn course_with_entries(entries: Vec<CourseEntry>) -> CourseDefinition {
        CourseDefinition {
            key: "table:test#0".to_string(),
            title: "Test Course".to_string(),
            kind: CourseKind::Dan,
            entries,
            constraints: CourseConstraints::default(),
            trophies: Vec::new(),
            release: true,
        }
    }

    fn note(id: u32, lane: Lane, kind: NoteKind, time_us: i64) -> NoteEvent {
        NoteEvent {
            id: NoteId(id),
            lane,
            kind,
            tick: ChartTick(0),
            time: TimeUs(time_us),
            sound: None,
            damage: None,
        }
    }

    fn timing_event(
        time_us: i64,
        kind: bmz_chart::model::TimingEventKind,
    ) -> bmz_chart::model::TimingEvent {
        bmz_chart::model::TimingEvent { tick: ChartTick(0), time: TimeUs(time_us), kind }
    }

    /// STOP 後の resume エントリが正しく追加されるか確認。
    /// 修正前は STOP 後も speed=0 のまま曲末まで続いていた。
    #[test]
    fn chart_speed_changes_emits_resume_after_stop() {
        use bmz_chart::model::TimingEventKind;
        let mut c = chart("stop_test");
        // BPM=128 で始まり、2 秒目に 0.5 秒の STOP
        c.timing_events
            .push(timing_event(2_000_000, TimingEventKind::Stop { duration_us: 500_000 }));
        let changes = chart_speed_changes(&c);
        // 期待: [{128, 0}, {0, 2000}, {128, 2500}, {128, 10000}]
        // STOP 直後の resume (speed=128 at 2500ms) が必須
        let resume = changes.iter().find(|c| c.time_ms == 2_500 && c.speed == 128.0);
        assert!(resume.is_some(), "resume entry after stop must exist: {changes:?}");
        // 末尾エントリが speed=0 になってはいけない
        assert_ne!(
            changes.last().unwrap().speed,
            0.0,
            "last entry must not be stop speed: {changes:?}"
        );
    }

    /// STOP 区間内に BPM 変化がある場合、STOP 終了後に新 BPM で再開すること。
    #[test]
    fn chart_speed_changes_resume_bpm_reflects_change_during_stop() {
        use bmz_chart::model::TimingEventKind;
        let mut c = chart("stop_bpm_change");
        // 1 秒目: STOP 2 秒間 (終了 3 秒)
        c.timing_events
            .push(timing_event(1_000_000, TimingEventKind::Stop { duration_us: 2_000_000 }));
        // 2 秒目 (STOP 中): BPM 200 に変化
        c.timing_events.push(timing_event(2_000_000, TimingEventKind::BpmChange { bpm: 200.0 }));
        let changes = chart_speed_changes(&c);
        // resume は STOP 終了 (3 秒) に BPM=200 で出るはず
        let resume = changes.iter().find(|c| c.time_ms == 3_000);
        assert!(
            resume.is_some_and(|r| r.speed == 200.0),
            "resume must use post-stop BPM: {changes:?}"
        );
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

        let analysis = db.chart_analysis_by_chart_id(chart_id).unwrap().unwrap();
        assert_eq!(analysis.total_gauge, 260.0);
        assert_eq!(analysis.main_bpm, 128.0);
    }

    #[test]
    fn upsert_chart_import_backfills_unresolved_course_entries() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let chart = chart("course song");
        let md5 = hash_to_hex(&chart.identity.file_md5);
        let sha256 = hash_to_hex(&chart.identity.file_sha256);
        let course = course_with_entries(vec![
            CourseEntry {
                title_hint: "SHA-256 match".to_string(),
                md5: Some(md5.clone()),
                sha256: Some(sha256),
                chart_id: None,
            },
            CourseEntry {
                title_hint: "MD5 fallback".to_string(),
                md5: Some(md5),
                sha256: Some("f".repeat(64)),
                chart_id: None,
            },
        ]);
        let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
        assert!(
            db.list_course_entries(course_id)
                .unwrap()
                .iter()
                .all(|entry| entry.entry.chart_id.is_none())
        );

        let chart_id =
            db.upsert_chart_import(&record_for_chart("/songs/course.bms", &chart)).unwrap();

        let entries = db.list_course_entries(course_id).unwrap();
        assert_eq!(entries[0].entry.chart_id, Some(chart_id));
        assert_eq!(entries[1].entry.chart_id, Some(chart_id));
    }

    #[test]
    fn successful_reimport_restores_course_link_after_import_failure() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let chart = chart("recovered course song");
        let path = Path::new("/songs/recovered.bms");
        let record = record_for_chart("/songs/recovered.bms", &chart);
        let original_chart_id = db.upsert_chart_import(&record).unwrap();
        let course = course_with_entries(vec![CourseEntry {
            title_hint: "Recovered song".to_string(),
            md5: Some(hash_to_hex(&chart.identity.file_md5)),
            sha256: Some(hash_to_hex(&chart.identity.file_sha256)),
            chart_id: None,
        }]);
        let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
        assert_eq!(
            db.list_course_entries(course_id).unwrap()[0].entry.chart_id,
            Some(original_chart_id)
        );

        db.upsert_failed_chart_file(None, path, 1, 2, 2, "temporary failure").unwrap();
        assert_eq!(db.list_course_entries(course_id).unwrap()[0].entry.chart_id, None);

        let recovered_chart_id = db.upsert_chart_import(&record).unwrap();
        assert_eq!(
            db.list_course_entries(course_id).unwrap()[0].entry.chart_id,
            Some(recovered_chart_id)
        );
    }

    #[test]
    fn course_backfill_uses_oldest_duplicate_chart_id() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let chart = chart("duplicate course song");
        let oldest_chart_id =
            db.upsert_chart_import(&record_for_chart("/songs/first.bms", &chart)).unwrap();
        let course = course_with_entries(vec![CourseEntry {
            title_hint: "Duplicate song".to_string(),
            md5: Some(hash_to_hex(&chart.identity.file_md5)),
            sha256: Some(hash_to_hex(&chart.identity.file_sha256)),
            chart_id: None,
        }]);
        let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
        db.conn()
            .execute(
                "UPDATE course_entries SET chart_id = NULL WHERE course_id = ?1",
                params![course_id],
            )
            .unwrap();

        let duplicate_chart_id =
            db.upsert_chart_import(&record_for_chart("/songs/second.bms", &chart)).unwrap();

        assert!(duplicate_chart_id > oldest_chart_id);
        assert_eq!(
            db.list_course_entries(course_id).unwrap()[0].entry.chart_id,
            Some(oldest_chart_id)
        );
    }

    #[test]
    fn upsert_chart_import_persists_bms_total_separately_from_gauge_total() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let mut chart = chart("bms total");
        chart.metadata.total = Some(320.0);
        chart.total_notes = 500;
        let record = ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/total.bms"),
            file_size: 123,
            modified_at: 1_700_000_001,
            scanned_at: 1_700_000_002,
            chart: &chart,
        };
        let chart_id = db.upsert_chart_import(&record).unwrap();

        let stored: f64 = db
            .conn
            .query_row("SELECT bms_total FROM charts WHERE id = ?1", params![chart_id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored, 320.0);

        let listed = db.list_charts_by_ids(&[chart_id]).unwrap().pop().unwrap();
        assert_eq!(listed.bms_total, 320.0);
    }

    #[test]
    fn upsert_chart_import_persists_ln_profile_and_pair_counts() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let mut chart = chart("defined cn");
        chart.metadata.long_note_mode = LongNoteMode::Cn;
        chart.metadata.long_note_mode_defined = true;
        for (index, mode) in
            [None, Some(LongNoteMode::Ln), Some(LongNoteMode::Cn), Some(LongNoteMode::Hcn)]
                .into_iter()
                .enumerate()
        {
            chart.long_notes.push(LongNotePair {
                lane: Lane::Key1,
                style: LongNoteStyle::ChannelPair,
                mode,
                start_note_id: NoteId((index * 2 + 1) as u32),
                end_note_id: NoteId((index * 2 + 2) as u32),
                start_tick: ChartTick(0),
                end_tick: ChartTick(192),
                start_time: TimeUs(1_000_000),
                end_time: TimeUs(2_000_000),
                sound: None,
            });
        }

        let chart_id =
            db.upsert_chart_import(&record_for_chart("/songs/defined-cn.bms", &chart)).unwrap();
        let row = db.list_charts_by_ids(&[chart_id]).unwrap().pop().unwrap();

        assert!(row.ln_profile.has_undefined_ln);
        assert!(row.ln_profile.has_defined_ln);
        assert!(row.ln_profile.has_defined_cn);
        assert!(row.ln_profile.has_defined_hcn);
        assert_eq!(
            row.ln_counts,
            ChartLnCounts {
                undefined_ln_pairs: 1,
                defined_ln_pairs: 1,
                defined_cn_pairs: 1,
                defined_hcn_pairs: 1,
            }
        );
        assert_eq!(row.scored_total_notes(LnScorePolicy::ForceCn), 4);

        chart.long_notes.truncate(1);
        let updated_id =
            db.upsert_chart_import(&record_for_chart("/songs/defined-cn.bms", &chart)).unwrap();
        assert_eq!(updated_id, chart_id);
        let updated = db.list_charts_by_ids(&[chart_id]).unwrap().pop().unwrap();
        assert_eq!(updated.ln_counts.undefined_ln_pairs, 1);
        assert_eq!(updated.ln_counts.defined_ln_pairs, 0);
        assert_eq!(updated.ln_counts.defined_cn_pairs, 0);
        assert_eq!(updated.ln_counts.defined_hcn_pairs, 0);
    }

    #[test]
    fn upsert_chart_import_persists_source_url_without_raw_headers() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let mut chart = chart("url song");
        chart.metadata.source_url = "http://example.com/bms".to_string();
        chart.metadata.append_url = "http://example.com/append".to_string();
        chart.metadata.bms_headers.insert("TITLE".to_string(), "url song".to_string());
        chart.metadata.bms_headers.insert("URL".to_string(), "http://example.com/bms".to_string());
        let record = ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/url.bms"),
            file_size: 123,
            modified_at: 1_700_000_001,
            scanned_at: 1_700_000_002,
            chart: &chart,
        };

        let chart_id = db.upsert_chart_import(&record).unwrap();
        let (source_url, append_url, headers_json): (String, String, String) = db
            .conn()
            .query_row(
                "SELECT source_url, append_url, headers_json FROM charts WHERE id = ?1",
                params![chart_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(source_url, "http://example.com/bms");
        assert_eq!(append_url, "http://example.com/append");
        assert_eq!(headers_json, "{}");
    }

    #[test]
    fn upsert_chart_import_persists_chart_analysis_distribution() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let mut chart = chart("analysis");
        chart.total_notes = 3;
        chart.end_time = TimeUs(3_000_000);
        chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, NoteKind::Tap, 100_000));
        chart.lane_notes[Lane::Scratch.index()].push(note(
            2,
            Lane::Scratch,
            NoteKind::LongStart,
            1_000_000,
        ));
        chart.lane_notes[Lane::Scratch.index()].push(note(
            3,
            Lane::Scratch,
            NoteKind::LongEnd,
            2_000_000,
        ));
        chart.lane_notes[Lane::Key2.index()].push(note(4, Lane::Key2, NoteKind::Mine, 1_500_000));
        chart.long_notes.push(LongNotePair {
            lane: Lane::Scratch,
            style: LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(2),
            end_note_id: NoteId(3),
            start_tick: ChartTick(0),
            end_tick: ChartTick(192),
            start_time: TimeUs(1_000_000),
            end_time: TimeUs(2_000_000),
            sound: None,
        });

        let chart_id =
            db.upsert_chart_import(&record_for_chart("/songs/analysis.bms", &chart)).unwrap();
        let analysis = db.chart_analysis_by_chart_id(chart_id).unwrap().unwrap();

        assert_eq!(analysis.normal_notes, 1);
        assert_eq!(analysis.long_notes, 1);
        assert_eq!(analysis.scratch_notes, 0);
        assert_eq!(analysis.long_scratch_notes, 1);
        assert_eq!(analysis.distribution[0].key_taps, 1);
        assert_eq!(analysis.distribution[1].scratch_long_heads, 1);
        assert_eq!(analysis.distribution[1].scratch_long_bodies, 0);
        assert_eq!(analysis.distribution[1].mines, 1);
        assert_eq!(analysis.distribution[2].scratch_long_bodies, 1);
        assert_eq!(analysis.lane_notes[Lane::Scratch.index()].long_notes, 1);
        assert_eq!(analysis.lane_notes[Lane::Key2.index()].mines, 1);
        assert_eq!(analysis.peak_density, 1.0);

        let stored_distribution: String = db
            .conn()
            .query_row(
                "SELECT distribution_json FROM chart_analysis WHERE chart_id = ?1",
                params![chart_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored_distribution.starts_with('#'));
        assert_eq!(stored_distribution.len(), 1 + analysis.distribution.len() * 14);
    }

    #[test]
    fn chart_normalization_analysis_roundtrips_and_rescan_clears_it() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let chart = chart("normalization");

        let chart_id =
            db.upsert_chart_import(&record_for_chart("/songs/normalization.bms", &chart)).unwrap();
        assert!(db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().is_none());

        db.write_chart_normalization_analysis(
            chart_id,
            ChartNormalizationAnalysis { loudness_lufs: -10.5, normalization_gain: 0.75 },
        )
        .unwrap();
        let stored = db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().unwrap();
        assert_eq!(stored.loudness_lufs, -10.5);
        assert_eq!(stored.normalization_gain, 0.75);

        db.upsert_chart_import(&record_for_chart("/songs/normalization.bms", &chart)).unwrap();
        assert!(db.chart_normalization_analysis_by_chart_id(chart_id).unwrap().is_none());
    }

    #[test]
    fn chart_analysis_counts_defined_cn_long_end_independently_of_chart_default() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase { conn };
        let mut chart = chart("mixed analysis");
        chart.metadata.long_note_mode = LongNoteMode::Ln;
        chart.total_notes = 1;
        chart.end_time = TimeUs(2_000_000);
        chart.lane_notes[Lane::Key1.index()].push(note(
            1,
            Lane::Key1,
            NoteKind::LongStart,
            1_000_000,
        ));
        chart.lane_notes[Lane::Key1.index()].push(note(
            2,
            Lane::Key1,
            NoteKind::LongEnd,
            2_000_000,
        ));
        chart.long_notes.push(LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::ChannelPair,
            mode: Some(LongNoteMode::Cn),
            start_note_id: NoteId(1),
            end_note_id: NoteId(2),
            start_tick: ChartTick(0),
            end_tick: ChartTick(192),
            start_time: TimeUs(1_000_000),
            end_time: TimeUs(2_000_000),
            sound: None,
        });

        let chart_id =
            db.upsert_chart_import(&record_for_chart("/songs/mixed-analysis.bms", &chart)).unwrap();
        let analysis = db.chart_analysis_by_chart_id(chart_id).unwrap().unwrap();

        assert_eq!(analysis.long_notes, 2);
        assert_eq!(analysis.lane_notes[Lane::Key1.index()].long_notes, 2);
        assert_eq!(analysis.distribution[2].key_long_heads, 1);
        assert_eq!(analysis.total_gauge, gauge_total_for_chart(None, 2));
    }

    #[test]
    fn chart_analysis_caps_extreme_distribution_length() {
        let mut chart = chart("long analysis");
        chart.end_time = TimeUs(i64::MAX);
        chart.long_notes.push(LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(1),
            end_note_id: NoteId(2),
            start_tick: ChartTick(0),
            end_tick: ChartTick(0),
            start_time: TimeUs(0),
            end_time: TimeUs(i64::MAX),
            sound: None,
        });

        let analysis = ChartAnalysis::from_chart(&chart);

        assert_eq!(analysis.distribution.len(), MAX_ANALYSIS_DISTRIBUTION_SECONDS);
    }

    #[test]
    fn chart_analysis_trims_distribution_to_last_note_second() {
        let mut chart = chart("trim analysis");
        chart.end_time = TimeUs(i64::MAX);
        chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, NoteKind::Tap, 2_000_000));

        let analysis = ChartAnalysis::from_chart(&chart);

        assert_eq!(analysis.distribution.len(), 3);
        assert_eq!(analysis.distribution[2].key_taps, 1);
    }

    #[test]
    fn chart_analysis_excludes_invisible_notes_from_density() {
        let mut chart = chart("invisible analysis");
        chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, NoteKind::Tap, 0));
        chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, NoteKind::Invisible, 0));
        chart.total_notes = 1;

        let analysis = ChartAnalysis::from_chart(&chart);

        assert_eq!(analysis.normal_notes, 1);
        assert_eq!(analysis.distribution[0].key_taps, 1);
    }

    #[test]
    fn compact_distribution_round_trips_and_accepts_legacy_json() {
        let distribution = vec![
            ChartDistributionSecond {
                scratch_long_heads: 1,
                scratch_long_bodies: 2,
                scratch_taps: 3,
                key_long_heads: 4,
                key_long_bodies: 5,
                key_taps: 6,
                mines: 7,
            },
            ChartDistributionSecond { key_taps: 36 * 36, ..Default::default() },
        ];

        let compact = encode_distribution_compact(&distribution);
        assert_eq!(compact.len(), 1 + distribution.len() * 14);
        let decoded = decode_distribution(&compact);
        assert_eq!(decoded[0], distribution[0]);
        assert_eq!(decoded[1].key_taps, 36 * 36 - 1);

        let legacy_json = serde_json::to_string(&distribution).unwrap();
        assert_eq!(decode_distribution(&legacy_json), distribution);
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
    fn search_charts_matches_substring_across_metadata_fields_case_insensitively() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let mut by_title = chart("Blue Sky");
        by_title.metadata.artist = "Composer A".to_string();
        by_title.metadata.genre = "Trance".to_string();
        let mut by_artist = chart("untitled");
        by_artist.metadata.artist = "DJ Blueprint".to_string();
        let mut by_genre = chart("other");
        by_genre.metadata.artist = "Nobody".to_string();
        by_genre.metadata.genre = "Drum & Bass (BLUE mix)".to_string();
        let mut unrelated = chart("Sunset");
        unrelated.metadata.artist = "Solo".to_string();
        unrelated.metadata.genre = "Ambient".to_string();

        for (path, c) in [
            ("/songs/a.bms", &by_title),
            ("/songs/b.bms", &by_artist),
            ("/songs/c.bms", &by_genre),
            ("/songs/d.bms", &unrelated),
        ] {
            db.upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: Path::new(path),
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: c,
            })
            .unwrap();
        }

        let hits = db.search_charts("blue").unwrap();
        let titles: Vec<&str> = hits.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles.len(), 3, "expected three matches, got {titles:?}");
        assert!(titles.contains(&"Blue Sky"));
        assert!(titles.contains(&"untitled"));
        assert!(titles.contains(&"other"));

        assert!(db.search_charts("nonexistent_query_xyz").unwrap().is_empty());
    }

    #[test]
    fn search_charts_treats_like_wildcards_as_literal() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let mut with_percent = chart("100% pure");
        with_percent.metadata.artist = "p".to_string();
        let mut without = chart("zero");
        without.metadata.artist = "z".to_string();

        for (path, c) in [("/songs/a.bms", &with_percent), ("/songs/b.bms", &without)] {
            db.upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: Path::new(path),
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: c,
            })
            .unwrap();
        }

        let hits = db.search_charts("%").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "100% pure");
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
    fn failed_reimport_removes_previous_chart_link_and_orphan() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let path = Path::new("/songs/unsupported.bmson");
        db.upsert_chart_import(&record_for_chart(path.to_str().unwrap(), &chart("old"))).unwrap();

        db.upsert_failed_chart_file(None, path, 10, 2, 3, "unsupported chart mode").unwrap();

        assert_eq!(db.chart_id_by_chart_file_path(path).unwrap(), None);
        let chart_count: i64 =
            db.conn().query_row("SELECT COUNT(*) FROM charts", [], |row| row.get(0)).unwrap();
        assert_eq!(chart_count, 0);
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
    fn charts_by_md5s_batches_more_hashes_than_one_sqlite_variable_chunk() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let first = chart("batch-first");
        let second = chart("batch-second");
        let first_id =
            db.upsert_chart_import(&record_for_chart("/songs/first.bms", &first)).unwrap();
        let second_id =
            db.upsert_chart_import(&record_for_chart("/songs/second.bms", &second)).unwrap();
        let first_md5 = hash_to_hex(&first.identity.file_md5);
        let second_md5 = hash_to_hex(&second.identity.file_md5);

        let mut hashes = (0..CHART_HASH_LOOKUP_BATCH_SIZE * 2 + 1)
            .map(|index| format!("{index:032x}"))
            .collect::<Vec<_>>();
        hashes.push(first_md5.clone());
        hashes.push(second_md5.clone());
        hashes.push(first_md5.clone());
        let hash_refs = hashes.iter().map(String::as_str).collect::<Vec<_>>();

        let resolved = db.charts_by_md5s(&hash_refs).unwrap();

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved.get(&first_md5).map(|chart| chart.chart_id), Some(first_id));
        assert_eq!(resolved.get(&second_md5).map(|chart| chart.chart_id), Some(second_id));
    }

    #[test]
    fn chart_analysis_summaries_batch_more_ids_than_one_sqlite_variable_chunk() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);

        let first = chart("analysis-batch-first");
        let second = chart("analysis-batch-second");
        let first_id =
            db.upsert_chart_import(&record_for_chart("/songs/first.bms", &first)).unwrap();
        let second_id =
            db.upsert_chart_import(&record_for_chart("/songs/second.bms", &second)).unwrap();
        let mut ids =
            (10_000..10_000 + CHART_ANALYSIS_LOOKUP_BATCH_SIZE as i64 * 2 + 1).collect::<Vec<_>>();
        ids.push(first_id);
        ids.push(second_id);
        ids.push(first_id);

        let summaries = db.chart_analysis_summaries_by_chart_ids(&ids).unwrap();

        assert_eq!(summaries.len(), 2);
        assert!(summaries.contains_key(&first_id));
        assert!(summaries.contains_key(&second_id));
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
