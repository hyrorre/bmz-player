use super::*;

impl ScoreDatabase {
    pub fn note_count_today(&self) -> Result<NoteCountAggregate> {
        self.conn
            .query_row(
                "SELECT
                    strftime('%Y-%m-%d', 'now', 'localtime'),
                    COUNT(*),
                    COALESCE(SUM(total_notes), 0)
                 FROM score_history
                 WHERE source_kind = 'Local'
                   AND autoplay = 0
                   AND date(played_at, 'unixepoch', 'localtime') = date('now', 'localtime')",
                [],
                note_count_aggregate_from_row,
            )
            .map_err(Into::into)
    }

    pub fn monthly_note_counts(&self, limit: u32) -> Result<Vec<NoteCountAggregate>> {
        self.note_counts_grouped_by("%Y-%m", limit)
    }

    pub fn yearly_note_counts(&self, limit: u32) -> Result<Vec<NoteCountAggregate>> {
        self.note_counts_grouped_by("%Y", limit)
    }

    fn note_counts_grouped_by(
        &self,
        format: &'static str,
        limit: u32,
    ) -> Result<Vec<NoteCountAggregate>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                strftime(?1, played_at, 'unixepoch', 'localtime') AS period,
                COUNT(*),
                COALESCE(SUM(total_notes), 0)
             FROM score_history
             WHERE source_kind = 'Local'
               AND autoplay = 0
             GROUP BY period
             ORDER BY period DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![format, limit], note_count_aggregate_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Returns local-calendar day ranges, newest first.
    ///
    /// Unlike `current_daily_statistics_range`, these ranges intentionally do
    /// not honor the manual daily-statistics reset marker: virtual folders
    /// describe calendar history rather than the resettable stats panel.
    pub fn recent_local_day_ranges(&self, day_count: usize) -> Result<Vec<(i64, i64)>> {
        let mut ranges = Vec::with_capacity(day_count);
        for offset in 0..day_count {
            let modifier = format!("-{offset} days");
            let range = self.conn.query_row(
                "SELECT
                    CAST(strftime('%s', date('now', 'localtime', ?1), 'utc') AS INTEGER),
                    CAST(strftime(
                        '%s', date('now', 'localtime', ?1), '+1 day', 'utc'
                    ) AS INTEGER)",
                params![modifier],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            ranges.push(range);
        }
        Ok(ranges)
    }

    /// Finds local, non-autoplay score/lamp improvements since `start_at`.
    pub fn chart_update_times_since(
        &self,
        keys: &[ScoreKey],
        start_at: i64,
    ) -> Result<HashMap<ScoreKey, ChartUpdateTimes>> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let accepted: HashSet<ScoreKey> = keys.iter().copied().collect();
        let mut updates = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT
                chart_sha256, ln_policy, double_option, rule_mode, played_at,
                clear_type, old_clear_type, ex_score, old_ex_score
             FROM score_history
             WHERE source_kind = 'Local'
               AND autoplay = 0
               AND played_at >= ?1
             ORDER BY played_at ASC, id ASC",
        )?;
        let mut rows = stmt.query(params![start_at])?;
        while let Some(row) = rows.next()? {
            let chart_sha256 = hex_to_hash::<32>(&row.get::<_, String>(0)?)?;
            let Some(ln_policy) = LnScorePolicy::from_str_opt(&row.get::<_, String>(1)?) else {
                continue;
            };
            let double_option = DoubleOptionScoreBucket::from_str_or_off(&row.get::<_, String>(2)?);
            let rule_mode =
                RuleMode::from_str_opt(&row.get::<_, String>(3)?).unwrap_or(RuleMode::Beatoraja);
            let key = ScoreKey::with_options(chart_sha256, ln_policy, double_option, rule_mode);
            if !accepted.contains(&key) {
                continue;
            }

            let played_at: i64 = row.get(4)?;
            let clear_type: String = row.get(5)?;
            let old_clear_type: Option<String> = row.get(6)?;
            let ex_score: u32 = row.get(7)?;
            let old_ex_score: Option<u32> = row.get(8)?;
            let entry = updates.entry(key).or_insert_with(ChartUpdateTimes::default);
            if old_clear_type.as_deref().is_none_or(|old| {
                ClearType::rank_from_label(&clear_type) > ClearType::rank_from_label(old)
            }) {
                entry.lamp.push(played_at);
            }
            if old_ex_score.is_none_or(|old| ex_score > old) {
                entry.score.push(played_at);
            }
        }
        Ok(updates)
    }

    pub fn player_info(&self) -> Result<PlayerInfo> {
        self.conn
            .query_row(
                "SELECT player_uuid, display_name, created_at, updated_at
                 FROM player_info
                 WHERE id = 1",
                [],
                |row| {
                    Ok(PlayerInfo {
                        player_uuid: row.get(0)?,
                        display_name: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn set_player_display_name(&mut self, display_name: &str, updated_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE player_info
             SET display_name = ?1, updated_at = ?2
             WHERE id = 1",
            params![display_name, updated_at],
        )?;
        Ok(())
    }

    pub fn player_stats(&self) -> Result<PlayerStats> {
        self.conn
            .query_row(
                "SELECT
                    play_count,
                    clear_count,
                    playtime_seconds,
                    max_combo,
                    fast_pgreat,
                    slow_pgreat,
                    fast_great,
                    slow_great,
                    fast_good,
                    slow_good,
                    fast_bad,
                    slow_bad,
                    fast_poor,
                    slow_poor,
                    fast_empty_poor,
                    slow_empty_poor,
                    updated_at
                 FROM player_stats
                 WHERE id = 1",
                [],
                player_stats_from_row,
            )
            .map_err(Into::into)
    }

    /// Aggregate locally played score history inside `[start_at, end_at)`.
    pub fn daily_player_stats_between(
        &self,
        start_at: i64,
        end_at: i64,
    ) -> Result<DailyPlayerStats> {
        self.conn
            .query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(CASE
                        WHEN clear_type NOT IN ('NoPlay', 'Failed') THEN 1 ELSE 0
                    END), 0),
                    COALESCE(SUM(fast_pgreat + slow_pgreat), 0),
                    COALESCE(SUM(fast_great + slow_great), 0),
                    COALESCE(SUM(fast_good + slow_good), 0),
                    COALESCE(SUM(fast_bad + slow_bad), 0),
                    COALESCE(SUM(fast_poor + slow_poor), 0),
                    COALESCE(SUM(fast_empty_poor + slow_empty_poor), 0),
                    COALESCE(SUM(CASE
                        WHEN old_ex_score IS NULL OR ex_score > old_ex_score THEN 1 ELSE 0
                    END), 0),
                    COALESCE(SUM(CASE WHEN old_clear_type IS NULL OR
                        CASE clear_type
                            WHEN 'Failed' THEN 1 WHEN 'AssistEasy' THEN 2
                            WHEN 'LightAssistEasy' THEN 3 WHEN 'Easy' THEN 4
                            WHEN 'Normal' THEN 5 WHEN 'Hard' THEN 6
                            WHEN 'ExHard' THEN 7 WHEN 'FullCombo' THEN 8
                            WHEN 'Perfect' THEN 9 WHEN 'Max' THEN 10 ELSE 0 END
                        > CASE old_clear_type
                            WHEN 'Failed' THEN 1 WHEN 'AssistEasy' THEN 2
                            WHEN 'LightAssistEasy' THEN 3 WHEN 'Easy' THEN 4
                            WHEN 'Normal' THEN 5 WHEN 'Hard' THEN 6
                            WHEN 'ExHard' THEN 7 WHEN 'FullCombo' THEN 8
                            WHEN 'Perfect' THEN 9 WHEN 'Max' THEN 10 ELSE 0 END
                        THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE
                        WHEN old_bp IS NULL OR fast_bad + slow_bad + fast_poor + slow_poor < old_bp
                        THEN 1 ELSE 0 END), 0)
                 FROM score_history
                 WHERE source_kind = 'Local'
                   AND autoplay = 0
                   AND played_at >= ?1
                   AND played_at < ?2",
                params![start_at, end_at],
                daily_player_stats_from_row,
            )
            .map_err(Into::into)
    }

    /// Aggregate the current calendar day using the host's local timezone.
    pub fn current_local_day_player_stats(&self) -> Result<DailyPlayerStats> {
        self.current_local_day_player_stats_with_start_hour(0)
    }

    pub fn current_local_day_player_stats_with_start_hour(
        &self,
        day_start_hour: u8,
    ) -> Result<DailyPlayerStats> {
        let (start_at, end_at) = self.current_daily_statistics_range(day_start_hour)?;
        self.daily_player_stats_between(start_at, end_at)
    }

    pub fn current_daily_statistics_range(&self, day_start_hour: u8) -> Result<(i64, i64)> {
        let hour = day_start_hour.min(23);
        let shift_to_day = format!("-{hour} hours");
        let shift_from_day = format!("+{hour} hours");
        let (calendar_start, end_at): (i64, i64) = self.conn.query_row(
            "SELECT
                CAST(strftime('%s', date('now', 'localtime', ?1), ?2, 'utc') AS INTEGER),
                CAST(strftime('%s', date('now', 'localtime', ?1), '+1 day', ?2, 'utc') AS INTEGER)",
            params![shift_to_day, shift_from_day],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let reset_at: i64 = self.conn.query_row(
            "SELECT reset_at FROM daily_statistics_state WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        Ok((calendar_start.max(reset_at).min(end_at), end_at))
    }

    pub fn reset_daily_statistics(&self, reset_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE daily_statistics_state SET reset_at = ?1 WHERE id = 1",
            params![reset_at],
        )?;
        Ok(())
    }

    pub fn daily_recent_chart_sha256s_between(
        &self,
        start_at: i64,
        end_at: i64,
        limit: usize,
    ) -> Result<Vec<[u8; 32]>> {
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256
             FROM score_history
             WHERE source_kind = 'Local'
               AND autoplay = 0
               AND played_at >= ?1
               AND played_at < ?2
             ORDER BY played_at DESC, id DESC",
        )?;
        let mut rows = stmt.query(params![start_at, end_at])?;
        let mut hashes = Vec::with_capacity(limit);
        let mut previous_hex = None;
        while hashes.len() < limit {
            let Some(row) = rows.next()? else { break };
            let hex: String = row.get(0)?;
            if previous_hex.as_deref() == Some(hex.as_str()) {
                continue;
            }
            previous_hex = Some(hex.clone());
            hashes.push(hex_to_hash::<32>(&hex)?);
        }
        Ok(hashes)
    }
}
