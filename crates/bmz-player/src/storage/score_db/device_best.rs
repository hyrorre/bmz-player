use super::*;

impl ScoreDatabase {
    /// Only Bokutachi PMS UpdateScore uses this partition. Local BEST and all
    /// other IRs retain their existing ScoreKey and independent best metrics.
    #[cfg(test)]
    pub(crate) fn improves_device_best(
        &self,
        key: ScoreKey,
        device: InputDeviceKind,
        before_id: i64,
        result: &PlayResult,
    ) -> Result<bool> {
        self.improves_device_best_with_lamp(key, device, before_id, result, result.clear_type)
    }

    pub(crate) fn improves_device_best_with_lamp(
        &self,
        key: ScoreKey,
        device: InputDeviceKind,
        before_id: i64,
        result: &PlayResult,
        clear_type: bmz_core::clear::ClearType,
    ) -> Result<bool> {
        let (ex, lamp, combo, bp, cb): (
            Option<u32>,
            Option<i32>,
            Option<u32>,
            Option<u32>,
            Option<u32>,
        ) = self.conn.query_row(
            "SELECT MAX(ex_score), MAX(CASE clear_type
                WHEN 'NoPlay' THEN 0 WHEN 'Failed' THEN 1 WHEN 'AssistEasy' THEN 2
                WHEN 'LightAssistEasy' THEN 3 WHEN 'Easy' THEN 4 WHEN 'Normal' THEN 5
                WHEN 'Hard' THEN 6 WHEN 'ExHard' THEN 7 WHEN 'FullCombo' THEN 8
                WHEN 'Perfect' THEN 9 WHEN 'Max' THEN 10 END), MAX(max_combo), MIN(bp), MIN(cb)
             FROM score_history WHERE chart_sha256 = ?1 AND ln_policy = ?2
                AND double_option = ?3 AND rule_mode = ?4 AND device_type = ?5
                AND id < ?6 AND normal_best_eligible = 1",
            params![
                hash_to_hex(&key.chart_sha256),
                key.ln_policy.as_str(),
                key.double_option.as_str(),
                key.rule_mode.as_str(),
                device.as_str(),
                before_id
            ],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        Ok(ex.is_none_or(|ex| result.score.ex_score() > ex)
            || lamp.is_some_and(|lamp| clear_type as i32 > lamp)
            || combo.is_some_and(|combo| result.score.max_combo > combo)
            || bp.is_some_and(|bp| result.record_bp() < bp)
            || cb.is_some_and(|cb| result.record_cb() < cb))
    }
}
