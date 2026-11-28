use super::*;

impl ScoreDatabase {
    /// Read a consistent snapshot for local rivals without modifying the source DB.
    pub fn all_best_scores(&self) -> Result<Vec<BestScoreSummary>> {
        let transaction = self.conn.unchecked_transaction()?;
        let keys = {
            let mut statement = transaction.prepare(
                "SELECT chart_sha256, ln_policy, double_option, rule_mode FROM score_best",
            )?;
            statement
                .query_map([], |row| {
                    Ok(ScoreKey::with_options(
                        hex_to_hash::<32>(&row.get::<_, String>(0)?)?,
                        ln_policy_from_row(row, 1)?,
                        double_option_from_row(row, 2)?,
                        rule_mode_from_row(row, 3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let scores = self.best_scores_for_charts(&keys)?;
        transaction.commit()?;
        Ok(scores)
    }

    /// Latest saved attempt, independent of the EX-score source. Batch this at
    /// list loading time; callbacks/rendering must never query score.db.
    pub fn last_played_times_for_charts(
        &self,
        keys: &[ScoreKey],
    ) -> Result<HashMap<ScoreKey, i64>> {
        let mut found = HashMap::new();
        let mut seen = HashSet::new();
        let keys: Vec<_> = keys.iter().copied().filter(|key| seen.insert(*key)).collect();
        for chunk in keys.chunks(SCORE_KEY_LOOKUP_BATCH_SIZE) {
            let placeholders =
                std::iter::repeat_n("(?, ?, ?, ?)", chunk.len()).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "WITH requested(chart_sha256, ln_policy, double_option, rule_mode) AS (VALUES {placeholders})
                 SELECT r.chart_sha256, r.ln_policy, r.double_option, r.rule_mode,
                    MAX(COALESCE((SELECT MAX(h.played_at) FROM score_history h
                        WHERE h.chart_sha256 = r.chart_sha256 AND h.ln_policy = r.ln_policy
                          AND h.double_option = r.double_option
                          AND CASE h.rule_mode WHEN 'Lr2Oraja' THEN 'Lr2Oraja'
                              WHEN 'Dx' THEN 'Dx' WHEN 'Legacy' THEN 'Legacy' ELSE 'Beatoraja' END = r.rule_mode
                          AND h.autoplay = 0), 0), COALESCE(u.played_at, 0))
                 FROM requested r LEFT JOIN score_unrecorded_plays u
                 ON u.chart_sha256 = r.chart_sha256 AND u.ln_policy = r.ln_policy
                    AND u.double_option = r.double_option AND u.rule_mode = r.rule_mode"
            );
            let mut stmt = self.conn.prepare_cached(&sql)?;
            let rows = stmt.query_map(params_from_iter(score_key_query_params(chunk)), |row| {
                let key = ScoreKey::with_options(
                    hex_to_hash::<32>(&row.get::<_, String>(0)?)?,
                    ln_policy_from_row(row, 1)?,
                    double_option_from_row(row, 2)?,
                    rule_mode_from_row(row, 3)?,
                );
                Ok((key, row.get::<_, i64>(4)?))
            })?;
            for row in rows {
                let (key, date) = row?;
                if date > 0 {
                    found.insert(key, date);
                }
            }
        }
        Ok(found)
    }

    pub fn best_ex_score(&self, key: ScoreKey) -> Result<Option<u32>> {
        self.conn
            .query_row(
                "SELECT ex_score FROM score_best
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
                   AND rule_mode = ?4",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                ],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Clear type, gauge type, arrange and assist mask of every non-autoplay clear
    /// of `key`, for LR2's clear-option history (select option 118..145).
    pub fn clear_option_history(&self, key: ScoreKey) -> Result<Vec<ClearOptionHistoryRow>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT clear_type, gauge_type, arrange, assist_mask FROM score_history
             WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
               AND CASE rule_mode WHEN 'Lr2Oraja' THEN 'Lr2Oraja'
                   WHEN 'Dx' THEN 'Dx' WHEN 'Legacy' THEN 'Legacy' ELSE 'Beatoraja' END = ?4
               AND autoplay = 0 AND normal_best_eligible = 1
               AND clear_type NOT IN ('NoPlay', 'Failed')",
        )?;
        let rows = stmt.query_map(
            params![
                hash_to_hex(&key.chart_sha256),
                key.ln_policy.as_str(),
                key.double_option.as_str(),
                key.rule_mode.as_str(),
            ],
            |row| {
                Ok(ClearOptionHistoryRow {
                    clear_type: row.get(0)?,
                    gauge_type: row.get(1)?,
                    arrange: row.get(2)?,
                    assist_mask: row.get(3)?,
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn best_ghost(&self, key: ScoreKey, total_notes: u32) -> Result<Option<Vec<u8>>> {
        let Some(ghost) = self
            .conn
            .query_row(
                "SELECT ghost FROM score_best
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
                   AND rule_mode = ?4",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        else {
            return Ok(None);
        };
        if ghost.is_empty() {
            return Ok(None);
        }
        decode_beatoraja_ghost(&ghost, total_notes).map(Some)
    }

    pub fn best_scores_for_charts(&self, keys: &[ScoreKey]) -> Result<Vec<BestScoreSummary>> {
        let found = self.best_score_map_for_charts(keys)?;
        Ok(keys.iter().filter_map(|key| found.get(key).cloned()).collect())
    }

    pub(crate) fn best_score_map_for_charts(
        &self,
        keys: &[ScoreKey],
    ) -> Result<HashMap<ScoreKey, BestScoreSummary>> {
        let mut seen = HashSet::with_capacity(keys.len());
        let unique_keys = keys.iter().copied().filter(|key| seen.insert(*key)).collect::<Vec<_>>();
        let mut found = HashMap::new();

        for chunk in unique_keys.chunks(SCORE_KEY_LOOKUP_BATCH_SIZE) {
            let placeholders =
                std::iter::repeat_n("(?, ?, ?, ?)", chunk.len()).collect::<Vec<_>>().join(", ");
            let eligible_prior = eligible_prior_condition(&self.conn);
            let sql = format!(
                "SELECT
                    b.chart_sha256,
                    b.ln_policy,
                    b.double_option,
                    b.rule_mode,
                    b.clear_type,
                    b.gauge_type,
                    b.gauge_value,
                    b.ex_score,
                    b.bp,
                    b.cb,
                    b.max_combo,
                    b.fast_pgreat,
                    b.slow_pgreat,
                    b.fast_great,
                    b.slow_great,
                    b.fast_good,
                    b.slow_good,
                    b.fast_bad,
                    b.slow_bad,
                    b.fast_poor,
                    b.slow_poor,
                    b.fast_empty_poor,
                    b.slow_empty_poor,
                    b.play_count,
                    b.clear_count,
                    b.device_type,
                    b.played_at,
                    b.replay_path,
                    h.arrange,
                    h.arrange_2p,
                    h.applied_double_option,
                    CASE WHEN b.best_score_history_id IS NOT NULL
                    OR b.ex_score != 0 OR b.bp != 0 OR b.cb != 0 OR b.max_combo != 0
                    THEN 1 ELSE EXISTS (
                        SELECT 1 FROM score_history AS prior
                        WHERE prior.chart_sha256 = b.chart_sha256
                          AND prior.ln_policy = b.ln_policy
                          AND prior.double_option = b.double_option
                          AND CASE prior.rule_mode WHEN 'Lr2Oraja' THEN 'Lr2Oraja'
                              WHEN 'Dx' THEN 'Dx' WHEN 'Legacy' THEN 'Legacy' ELSE 'Beatoraja' END = b.rule_mode
                          {eligible_prior}
                    ) END
                FROM score_best b
                LEFT JOIN score_history h ON h.id = b.best_score_history_id
                WHERE (b.chart_sha256, b.ln_policy, b.double_option, b.rule_mode)
                    IN ({placeholders})"
            );
            let mut stmt = self.conn.prepare_cached(&sql)?;
            let rows = stmt.query_map(
                params_from_iter(score_key_query_params(chunk)),
                best_score_summary_from_row,
            )?;
            for row in rows {
                let summary = row?;
                let key = ScoreKey::with_options(
                    summary.chart_sha256,
                    summary.ln_policy,
                    summary.double_option,
                    summary.rule_mode,
                );
                found.insert(key, summary);
            }
        }

        Ok(found)
    }
}
