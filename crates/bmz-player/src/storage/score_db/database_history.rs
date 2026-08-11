use super::*;

impl ScoreDatabase {
    /// IR replay upload 用に、score_history 行の replay_path を引く。
    /// 行が無い / 空文字なら None。
    pub fn replay_path_for_history(&self, score_history_id: i64) -> Result<Option<String>> {
        let path: Option<String> = self
            .conn
            .query_row(
                "SELECT replay_path FROM score_history WHERE id = ?1",
                params![score_history_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(path.filter(|path| !path.is_empty()))
    }

    pub fn recent_history(&self, limit: u32, offset: u32) -> Result<Vec<ScoreHistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                chart_sha256,
                played_at,
                clear_type,
                gauge_type,
                gauge_value,
                total_notes,
                ex_score,
                bp,
                cb,
                max_combo,
                autoplay,
                replay_path,
                course_score_id,
                ln_policy,
                old_clear_type,
                old_ex_score,
                old_max_combo,
                old_bp,
                old_cb,
                device_type,
                source_kind,
                applied_double_option
            FROM score_history
            ORDER BY played_at DESC, id DESC
            LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], score_history_entry_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn recent_history_by_local_day(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ScoreHistoryDayEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                chart_sha256,
                played_at,
                clear_type,
                gauge_type,
                gauge_value,
                total_notes,
                ex_score,
                bp,
                cb,
                max_combo,
                autoplay,
                replay_path,
                course_score_id,
                ln_policy,
                old_clear_type,
                old_ex_score,
                old_max_combo,
                old_bp,
                old_cb,
                device_type,
                source_kind,
                applied_double_option,
                date(played_at, 'unixepoch', 'localtime'),
                CAST(strftime('%Y', played_at, 'unixepoch', 'localtime') AS INTEGER) || '/' ||
                    CAST(strftime('%m', played_at, 'unixepoch', 'localtime') AS INTEGER) || '/' ||
                    CAST(strftime('%d', played_at, 'unixepoch', 'localtime') AS INTEGER) || ' ' ||
                    strftime('%H:%M', played_at, 'unixepoch', 'localtime')
            FROM score_history
            ORDER BY played_at DESC, id DESC
            LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(ScoreHistoryDayEntry {
                entry: score_history_entry_from_row(row)?,
                local_day: row.get(23)?,
                local_minute: row.get(24)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn local_history_since(&self, played_at: i64) -> Result<Vec<ScoreHistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                chart_sha256,
                played_at,
                clear_type,
                gauge_type,
                gauge_value,
                total_notes,
                ex_score,
                bp,
                cb,
                max_combo,
                autoplay,
                replay_path,
                course_score_id,
                ln_policy,
                old_clear_type,
                old_ex_score,
                old_max_combo,
                old_bp,
                old_cb,
                device_type,
                source_kind,
                applied_double_option
            FROM score_history
            WHERE played_at >= ?1
              AND source_kind = 'Local'
              AND autoplay = 0
              AND course_score_id IS NULL
            ORDER BY played_at ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![played_at], score_history_entry_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}
