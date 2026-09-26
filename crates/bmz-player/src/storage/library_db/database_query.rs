use super::*;

impl LibraryDatabase {
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
                params![path_key(path)],
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
        let path = path_key(path);
        self.conn
            .prepare_cached(
                "INSERT INTO roots (path, enabled, recursive)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(path) DO UPDATE SET
                    enabled = excluded.enabled,
                    recursive = excluded.recursive",
            )?
            .execute(params![path, enabled, recursive])?;

        self.conn
            .prepare_cached("SELECT id FROM roots WHERE path = ?1")?
            .query_row(params![path], |row| row.get(0))
            .map_err(Into::into)
    }

    pub fn update_root_scanned_at(&mut self, root_id: i64, scanned_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE roots SET last_scan_at = ?1 WHERE id = ?2",
            params![scanned_at, root_id],
        )?;
        Ok(())
    }

    pub fn update_folder_document_flags(&mut self, folder_flags: &[(PathBuf, bool)]) -> Result<()> {
        if folder_flags.is_empty() {
            return Ok(());
        }

        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare_cached("UPDATE charts SET has_document = ?1 WHERE folder_path = ?2")?;
            for (folder, has_document) in folder_flags {
                stmt.execute(params![has_document, to_folder_key(&path_to_string(folder))])?;
            }
        }
        tx.commit()?;
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
                root_id, path, file_size, modified_at, md5, sha256, scanned_at,
                first_seen_at, parse_status
            ) VALUES (?1, ?2, ?3, ?4, '', '', ?5, ?5, 'Failed')
            ON CONFLICT(path) DO UPDATE SET
                root_id = excluded.root_id,
                file_size = excluded.file_size,
                modified_at = excluded.modified_at,
                scanned_at = excluded.scanned_at,
                parse_status = excluded.parse_status
            RETURNING id",
            )?
            .query_row(
                params![root_id, path_key(file_path), file_size as i64, modified_at, scanned_at],
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

    pub fn list_all_charts(&self) -> Result<Vec<ChartListItem>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS}
             FROM charts
             ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([], chart_list_item_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Returns the first time each chart was discovered through any linked file.
    pub fn chart_first_seen_at_by_chart_ids(&self, chart_ids: &[i64]) -> Result<HashMap<i64, i64>> {
        if chart_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut unique_ids = chart_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        let mut out = HashMap::with_capacity(unique_ids.len());
        for chunk in unique_ids.chunks(CHART_ANALYSIS_LOOKUP_BATCH_SIZE) {
            let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT cfl.chart_id, MIN(cf.first_seen_at)
                 FROM chart_file_links cfl
                 JOIN chart_files cf ON cf.id = cfl.chart_file_id
                 WHERE cfl.chart_id IN ({placeholders})
                 GROUP BY cfl.chart_id"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            for row in rows {
                let (chart_id, first_seen_at) = row?;
                out.insert(chart_id, first_seen_at);
            }
        }
        Ok(out)
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

    /// Returns the duration recorded by the library scan for a chart.
    pub fn chart_length_ms_by_id(&self, chart_id: i64) -> Result<Option<i64>> {
        self.conn
            .query_row("SELECT length_ms FROM charts WHERE id = ?1", params![chart_id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(Into::into)
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
                "SELECT loudness_lufs, short_term_lufs, sample_peak
                 FROM chart_analysis
                 WHERE chart_id = ?1
                    AND loudness_analysis_version = ?2
                    AND loudness_lufs IS NOT NULL
                    AND short_term_lufs IS NOT NULL
                    AND sample_peak IS NOT NULL",
                params![chart_id, CHART_LOUDNESS_ANALYSIS_VERSION],
                |row| {
                    let loudness_lufs: f32 = row.get(0)?;
                    let short_term_lufs: f32 = row.get(1)?;
                    let sample_peak: f32 = row.get(2)?;
                    Ok(ChartNormalizationAnalysis { loudness_lufs, short_term_lufs, sample_peak })
                },
            )
            .optional()
            .map(|value| {
                value.filter(|analysis| {
                    analysis.loudness_lufs.is_finite()
                        && analysis.short_term_lufs.is_finite()
                        && analysis.sample_peak.is_finite()
                        && analysis.sample_peak > 0.0
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
                 short_term_lufs = ?3,
                 sample_peak = ?4,
                 loudness_analysis_version = ?5
             WHERE chart_id = ?1",
            )?
            .execute(params![
                chart_id,
                analysis.loudness_lufs,
                analysis.short_term_lufs,
                analysis.sample_peak,
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
                    density, peak_density, end_density, main_bpm
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

    pub fn chart_graphs_by_chart_ids(&self, ids: &[i64]) -> Result<HashMap<i64, ChartGraphData>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT chart_id, distribution_json, speed_changes_json
             FROM chart_analysis
             WHERE chart_id = ?1",
        )?;
        let mut out = HashMap::with_capacity(ids.len());
        for id in ids {
            if let Some((chart_id, distribution_json, speed_changes_json)) = stmt
                .query_row(params![id], |row| {
                    Ok((row.get(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))
                })
                .optional()?
            {
                let distribution = decode_distribution(&distribution_json);
                let speed_changes = speed_changes_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok())
                    .unwrap_or_default();
                out.insert(chart_id, ChartGraphData { distribution, speed_changes });
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
        self.search_charts_with_limit(query, None, None)
    }

    pub fn search_charts_in_roots(
        &self,
        query: &str,
        roots: Option<&[String]>,
    ) -> Result<Vec<ChartListItem>> {
        self.search_charts_with_limit(query, None, roots)
    }

    /// Searches chart metadata while limiting the rows materialized by SQLite.
    /// UI callers should use this instead of collecting every match and truncating it.
    pub fn search_charts_limited(&self, query: &str, limit: u32) -> Result<Vec<ChartListItem>> {
        self.search_charts_with_limit(query, Some(limit), None)
    }

    fn search_charts_with_limit(
        &self,
        query: &str,
        limit: Option<u32>,
        roots: Option<&[String]>,
    ) -> Result<Vec<ChartListItem>> {
        let pattern = format!("%{}%", escape_like(query));
        let mut values = vec![rusqlite::types::Value::Text(pattern)];
        let root_clause = if let Some(roots) = roots {
            if roots.is_empty() {
                return Ok(Vec::new());
            }
            let predicates: Vec<_> = roots.iter().map(|root| {
                let root = library_path_key(Path::new(root));
                values.push(rusqlite::types::Value::Text(root.trim_end_matches('/').to_string()));
                let index = values.len();
                format!("(folder_path = ?{index} OR substr(folder_path, 1, length(?{index}) + 1) = ?{index} || '/')")
            }).collect();
            format!(" AND ({})", predicates.join(" OR "))
        } else {
            String::new()
        };
        let limit_clause = if let Some(limit) = limit {
            values.push(rusqlite::types::Value::Integer(i64::from(limit)));
            format!(" LIMIT ?{}", values.len())
        } else {
            String::new()
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS}
            FROM charts
            WHERE id IN (
            SELECT MIN(id) FROM charts
            WHERE (title LIKE ?1 ESCAPE '\\'
               OR subtitle LIKE ?1 ESCAPE '\\'
               OR artist LIKE ?1 ESCAPE '\\'
               OR subartist LIKE ?1 ESCAPE '\\'
               OR genre LIKE ?1 ESCAPE '\\')
            {root_clause}
            GROUP BY sha256)
            ORDER BY title COLLATE NOCASE, artist COLLATE NOCASE, play_level COLLATE NOCASE
            {limit_clause}"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), chart_list_item_from_row)?;
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
                params![path_key(path)],
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
}
