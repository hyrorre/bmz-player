use super::*;

impl ScoreDatabase {
    pub fn open_read_only(path: &Path) -> Result<Self> {
        Ok(Self {
            conn: Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?,
        })
    }
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

    pub fn insert_score(&mut self, record: &ScoreRecord) -> Result<i64> {
        self.insert_score_with_mode(record, ScoreInsertMode::Full)
    }

    pub fn insert_score_with_mode(
        &mut self,
        record: &ScoreRecord,
        mode: ScoreInsertMode,
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let previous_best = previous_best_snapshot(
            &tx,
            ScoreKey::with_options(
                record.chart_sha256,
                record.ln_policy,
                record.double_option,
                record_rule_mode(record),
            ),
        )?;
        insert_score_history(&tx, record, previous_best.as_ref())?;
        let history_id = tx.last_insert_rowid();
        if mode == ScoreInsertMode::Full {
            upsert_score_best(&tx, record, history_id)?;
            update_player_stats(&tx, record)?;
        }
        tx.commit()?;
        Ok(history_id)
    }

    /// beatoraja の `updateScore=false` 相当。
    ///
    /// score history と数値ベストは更新せず、曲別のプレイ回数・クリア回数・
    /// クリアランプ、および profile 全体のプレイヤー統計を更新する。
    pub fn update_score_clear_only(&mut self, record: &ScoreRecord) -> Result<()> {
        let tx = self.conn.transaction()?;
        upsert_score_best_clear_only(&tx, record)?;
        update_player_stats(&tx, record)?;
        tx.commit()?;
        Ok(())
    }

    /// Returns whether an imported score with the same persisted score contents
    /// and provenance already exists. `played_at` is intentionally excluded:
    /// LR2 does not retain a per-score timestamp, and re-importing a source
    /// database must not create duplicates merely because the import time changed.
    pub fn has_same_score_from_source(&self, record: &ScoreRecord) -> Result<bool> {
        Ok(source_score_history_match(&self.conn, record)?
            .is_some_and(|existing| existing.device_type == record.device_type))
    }

    /// 同一出所の既存履歴へ、再インポートで得られた補完可能な値を反映する。
    ///
    /// 入力デバイスは履歴を常に補正し、集計側はその履歴がEXスコアの出所である場合だけ
    /// 更新する。ghost は現在のMYBESTに紐づく履歴かつ保存値が空の場合だけ補完し、
    /// 既存ghostや別履歴由来のMYBESTは上書きしない。
    pub fn reconcile_imported_score(
        &mut self,
        record: &ScoreRecord,
    ) -> Result<ImportedScoreReconciliation> {
        if record.source_kind == ScoreSourceKind::Local {
            return Ok(ImportedScoreReconciliation::Missing);
        }
        let Some(existing) = source_score_history_match(&self.conn, record)? else {
            return Ok(ImportedScoreReconciliation::Missing);
        };
        let device_changed = existing.device_type != record.device_type;
        let ghost_needs_backfill = !record.score.ghost.is_empty()
            && self.conn.query_row(
                "SELECT EXISTS (
                    SELECT 1 FROM score_best
                    WHERE best_score_history_id = ?1 AND ghost = ''
                )",
                params![existing.history_id],
                |row| row.get(0),
            )?;
        if !device_changed && !ghost_needs_backfill {
            return Ok(ImportedScoreReconciliation::Unchanged);
        }
        let ghost = ghost_needs_backfill
            .then(|| encode_beatoraja_ghost(&record.score.ghost))
            .transpose()?;

        let tx = self.conn.transaction()?;
        let mut corrected = false;
        if device_changed {
            tx.execute(
                "UPDATE score_history
                 SET device_type = ?1
                 WHERE id = ?2 AND device_type = ?3",
                params![
                    record.device_type.as_str(),
                    existing.history_id,
                    existing.device_type.as_str()
                ],
            )?;
            update_score_best_device_type_from_history(
                &tx,
                existing.history_id,
                record.device_type,
            )?;
            corrected = true;
        }
        if let Some(ghost) = ghost {
            corrected |= tx.execute(
                "UPDATE score_best
                 SET ghost = ?1
                 WHERE best_score_history_id = ?2 AND ghost = ''",
                params![ghost, existing.history_id],
            )? > 0;
        }
        tx.commit()?;
        Ok(if corrected {
            ImportedScoreReconciliation::Corrected
        } else {
            ImportedScoreReconciliation::Unchanged
        })
    }

    /// source_kind 導入前に Local として保存された beatoraja import 候補を調べる。
    ///
    /// Local の通常プレイと完全に区別する情報は失われているため、呼び出し側で
    /// dry-run の結果を確認してから [`Self::purge_legacy_beatoraja_imports`] を実行する。
    pub fn legacy_beatoraja_cleanup_plan(&self) -> Result<LegacyBeatorajaCleanupPlan> {
        Ok(LegacyBeatorajaCleanupPlan {
            legacy_history_ids: legacy_beatoraja_matching_history_ids(&self.conn, "legacy")?,
            retained_beatoraja_history_ids: legacy_beatoraja_matching_history_ids(
                &self.conn, "imported",
            )?,
        })
    }

    /// 同じ source_kind 内で、譜面・プレイ日時・スコア内訳・seed が完全一致する
    /// 通常プレイ履歴を返す。course stage は重複整理の対象にしない。
    pub fn same_source_duplicate_history_ids(&self, history_id: i64) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(
            "SELECT duplicate.id
             FROM score_history AS target
             JOIN score_history AS duplicate
               ON duplicate.id != target.id
              AND duplicate.source_kind = target.source_kind
              AND duplicate.course_score_id IS NULL
              AND duplicate.chart_sha256 = target.chart_sha256
              AND duplicate.played_at = target.played_at
              AND duplicate.ex_score = target.ex_score
              AND duplicate.bp = target.bp
              AND duplicate.cb = target.cb
              AND duplicate.max_combo = target.max_combo
              AND duplicate.fast_pgreat = target.fast_pgreat
              AND duplicate.slow_pgreat = target.slow_pgreat
              AND duplicate.fast_great = target.fast_great
              AND duplicate.slow_great = target.slow_great
              AND duplicate.fast_good = target.fast_good
              AND duplicate.slow_good = target.slow_good
              AND duplicate.fast_bad = target.fast_bad
              AND duplicate.slow_bad = target.slow_bad
              AND duplicate.fast_poor = target.fast_poor
              AND duplicate.slow_poor = target.slow_poor
              AND duplicate.fast_empty_poor = target.fast_empty_poor
              AND duplicate.slow_empty_poor = target.slow_empty_poor
              AND duplicate.random_seed IS target.random_seed
             WHERE target.id = ?1
               AND target.course_score_id IS NULL
             ORDER BY duplicate.id",
        )?;
        statement
            .query_map(params![history_id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// 指定した通常プレイ履歴を削除し、残存履歴から集計を再構築する。
    pub fn purge_score_history_ids_and_rebuild(&mut self, history_ids: &[i64]) -> Result<u32> {
        if history_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        preserve_cleanup_aggregates(&tx, history_ids)?;
        let removed_history = delete_score_history_ids(&tx, history_ids)?;
        rebuild_score_aggregates(&tx)?;
        restore_cleanup_aggregates(&tx)?;
        tx.commit()?;
        Ok(removed_history)
    }

    /// 指定した旧 Local 候補を削除し、通常譜面の score_best と player_stats を
    /// 残存履歴から再集計し、履歴外の実績とコース stage の実績を保全する。
    pub fn purge_legacy_beatoraja_imports(
        &mut self,
        plan: &LegacyBeatorajaCleanupPlan,
    ) -> Result<LegacyBeatorajaCleanupReport> {
        let legacy_history_ids = &plan.legacy_history_ids;
        if legacy_history_ids.is_empty() {
            return Ok(LegacyBeatorajaCleanupReport {
                retained_beatoraja_history: plan.retained_beatoraja_history_ids.len() as u32,
                ..LegacyBeatorajaCleanupReport::default()
            });
        }

        let removed_legacy_history =
            self.purge_score_history_ids_and_rebuild(legacy_history_ids)?;
        Ok(LegacyBeatorajaCleanupReport {
            removed_legacy_history,
            retained_beatoraja_history: plan.retained_beatoraja_history_ids.len() as u32,
        })
    }

    pub fn score_history_id_for_source(&self, key: &ScoreHistorySourceKey) -> Result<Option<i64>> {
        score_history_id_for_source(&self.conn, key)
    }

    pub fn attach_score_history_source(
        &mut self,
        score_history_id: i64,
        source: &ScoreHistorySourceRecord,
    ) -> Result<bool> {
        let inserted = insert_score_history_source(&self.conn, score_history_id, source, true)?;
        Ok(inserted > 0)
    }

    pub fn insert_score_with_source(
        &mut self,
        record: &ScoreRecord,
        source: &ScoreHistorySourceRecord,
    ) -> Result<ScoreSourceInsertOutcome> {
        if let Some(history_id) = self.score_history_id_for_source(&source.key)? {
            return Ok(ScoreSourceInsertOutcome::Duplicate { history_id });
        }

        let tx = self.conn.transaction()?;
        let previous_best = previous_best_snapshot(
            &tx,
            ScoreKey::with_options(
                record.chart_sha256,
                record.ln_policy,
                record.double_option,
                record_rule_mode(record),
            ),
        )?;
        insert_score_history(&tx, record, previous_best.as_ref())?;
        let history_id = tx.last_insert_rowid();
        upsert_score_best(&tx, record, history_id)?;
        update_player_stats(&tx, record)?;
        insert_score_history_source(&tx, history_id, source, false)?;
        tx.commit()?;
        Ok(ScoreSourceInsertOutcome::Inserted { history_id })
    }
}
