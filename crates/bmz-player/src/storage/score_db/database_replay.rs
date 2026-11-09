use super::*;

impl ScoreDatabase {
    pub fn replay_slots_for_charts(&self, keys: &[ScoreKey]) -> Result<Vec<ReplaySlotSummary>> {
        let mut seen = HashSet::with_capacity(keys.len());
        let unique_keys = keys.iter().copied().filter(|key| seen.insert(*key)).collect::<Vec<_>>();
        let mut found: HashMap<ScoreKey, [bool; 4]> = HashMap::with_capacity(unique_keys.len());

        for chunk in unique_keys.chunks(SCORE_KEY_LOOKUP_BATCH_SIZE) {
            let placeholders =
                std::iter::repeat_n("(?, ?, ?, ?)", chunk.len()).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT chart_sha256, ln_policy, double_option, rule_mode, slot
                 FROM replay_slots
                 WHERE (chart_sha256, ln_policy, double_option, rule_mode)
                    IN ({placeholders})"
            );
            let params = score_key_query_params(chunk);
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
                let sha256_hex: String = row.get(0)?;
                let key = ScoreKey::with_options(
                    hex_to_hash::<32>(&sha256_hex)?,
                    ln_policy_from_row(row, 1)?,
                    double_option_from_row(row, 2)?,
                    rule_mode_from_row(row, 3)?,
                );
                Ok((key, row.get::<_, u8>(4)?))
            })?;
            for row in rows {
                let (key, slot) = row?;
                let replay_slots = found.entry(key).or_default();
                if (slot as usize) < 4 {
                    replay_slots[slot as usize] = true;
                }
            }
        }

        Ok(keys
            .iter()
            .filter_map(|key| {
                found.get(key).copied().map(|replay_slots| ReplaySlotSummary {
                    chart_sha256: key.chart_sha256,
                    ln_policy: key.ln_policy,
                    double_option: key.double_option,
                    rule_mode: key.rule_mode,
                    replay_slots,
                })
            })
            .collect())
    }

    pub fn replay_slot(&self, key: ScoreKey, slot: u8) -> Result<Option<ReplaySlotRecord>> {
        self.conn
            .query_row(
                "SELECT chart_sha256, ln_policy, double_option, rule_mode, slot, rule, replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank, source_kind, source_path, source_fingerprint
                 FROM replay_slots
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
                   AND rule_mode = ?4 AND slot = ?5",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                    slot,
                ],
                replay_slot_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replay_slots_for_chart(&self, key: ScoreKey) -> Result<[Option<ReplaySlotRecord>; 4]> {
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256, ln_policy, double_option, rule_mode, slot, rule, replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank, source_kind, source_path, source_fingerprint
             FROM replay_slots
             WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
               AND rule_mode = ?4",
        )?;
        let rows = stmt
            .query_map(
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                ],
                replay_slot_record_from_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out: [Option<ReplaySlotRecord>; 4] = [None, None, None, None];
        for record in rows {
            let slot = record.slot as usize;
            if slot < out.len() {
                out[slot] = Some(record);
            }
        }
        Ok(out)
    }

    pub fn upsert_replay_slot(&mut self, record: &ReplaySlotRecord) -> Result<()> {
        upsert_replay_slot_on(&self.conn, record)
    }

    pub fn upsert_replay_slots(&mut self, records: &[ReplaySlotRecord]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for record in records {
            upsert_replay_slot_on(&tx, record)?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn upsert_replay_slot_on(conn: &Connection, record: &ReplaySlotRecord) -> Result<()> {
    if record.slot > 3 {
        bail!("replay slot must be in 0..=3 (got {})", record.slot);
    }
    conn.execute(
            "INSERT INTO replay_slots (
                chart_sha256, ln_policy, double_option, rule_mode, slot, rule, replay_path, played_at,
                ex_score, bp, cb, max_combo, clear_rank, source_kind, source_path, source_fingerprint
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(chart_sha256, ln_policy, double_option, rule_mode, slot) DO UPDATE SET
                rule = excluded.rule,
                replay_path = excluded.replay_path,
                played_at = excluded.played_at,
                ex_score = excluded.ex_score,
                bp = excluded.bp,
                cb = excluded.cb,
                max_combo = excluded.max_combo,
                clear_rank = excluded.clear_rank,
                source_kind = excluded.source_kind,
                source_path = excluded.source_path,
                source_fingerprint = excluded.source_fingerprint",
            params![
                hash_to_hex(&record.chart_sha256),
                record.ln_policy.as_str(),
                record.double_option.as_str(),
                record.rule_mode.as_str(),
                record.slot,
                record.rule.as_str(),
                record.replay_path,
                record.played_at,
                record.ex_score,
                record.bp,
                record.cb,
                record.max_combo,
                record.clear_rank,
                record.source_kind.as_str(),
                record.source_path,
                record.source_fingerprint,
            ],
        )?;
    Ok(())
}
