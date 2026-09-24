use super::*;

impl ScoreDatabase {
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
        let mut seen = HashSet::with_capacity(keys.len());
        let unique_keys = keys.iter().copied().filter(|key| seen.insert(*key)).collect::<Vec<_>>();
        let mut found = HashMap::with_capacity(unique_keys.len());

        for chunk in unique_keys.chunks(SCORE_KEY_LOOKUP_BATCH_SIZE) {
            let placeholders =
                std::iter::repeat_n("(?, ?, ?, ?)", chunk.len()).collect::<Vec<_>>().join(", ");
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
                    h.applied_double_option
                FROM score_best b
                LEFT JOIN score_history h ON h.id = b.best_score_history_id
                WHERE (b.chart_sha256, b.ln_policy, b.double_option, b.rule_mode)
                    IN ({placeholders})"
            );
            let params = score_key_query_params(chunk);
            let mut stmt = self.conn.prepare(&sql)?;
            let rows =
                stmt.query_map(params_from_iter(params.iter()), best_score_summary_from_row)?;
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

        Ok(keys.iter().filter_map(|key| found.get(key).cloned()).collect())
    }
}
