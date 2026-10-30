use anyhow::Context;

const IR_JOB_SENDING_STALE_AFTER_SECONDS: i64 = 300;

// Payloads without a submission identifier are legacy/internal jobs.
fn job_submission_key(job: &NewIrScoreJob) -> String {
    serde_json::from_str::<serde_json::Value>(&job.payload_json)
        .ok()
        .and_then(|value| {
            value
                .get("idempotency_key")
                .or_else(|| value.get("remote_score_id"))
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| format!("local:{}", job.local_score_id))
}

fn upsert_ir_score_success_ledger(
    tx: &rusqlite::Transaction<'_>,
    record: &NewIrScoreSubmission,
) -> Result<()> {
    if record.status != "succeeded" {
        return Ok(());
    }
    let (submission_key, payload_json, chart_sha256, ln_policy): (String, String, String, String) = tx.query_row(
        "SELECT submission_key, payload_json, chart_sha256, ln_policy FROM ir_score_jobs WHERE id = ?1",
        [record.job_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    tx.execute(
        "INSERT INTO ir_score_success_ledger (
            provider, account_id, kind, submission_key, local_score_id,
            remote_score_id, submitted_at, submission_payload_json, chart_sha256, ln_policy
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(provider, account_id, kind, submission_key) DO UPDATE SET
            local_score_id = excluded.local_score_id,
            remote_score_id = excluded.remote_score_id,
            submitted_at = excluded.submitted_at,
            submission_payload_json = CASE WHEN ir_score_success_ledger.submission_payload_json = '' THEN excluded.submission_payload_json ELSE ir_score_success_ledger.submission_payload_json END,
            chart_sha256 = CASE WHEN ir_score_success_ledger.chart_sha256 = '' THEN excluded.chart_sha256 ELSE ir_score_success_ledger.chart_sha256 END,
            ln_policy = CASE WHEN ir_score_success_ledger.ln_policy = '' THEN excluded.ln_policy ELSE ir_score_success_ledger.ln_policy END",
        params![
            record.provider,
            record.account_id,
            record.kind.as_str(),
            submission_key,
            record.local_score_id,
            record.remote_score_id,
            record.submitted_at,
            payload_json,
            chart_sha256,
            ln_policy,
        ],
    )?;
    Ok(())
}

impl NetworkDatabase {
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

    pub fn rival_score_cache_state(
        &self,
        provider: &str,
        rival_id: &str,
        body: &str,
    ) -> Result<IrRivalScoreCacheState> {
        Ok(self
            .conn
            .query_row(
                "SELECT etag, fetched_at FROM ir_rival_score_sync
                 WHERE provider = ?1 AND rival_id = ?2 AND body = ?3",
                params![provider, rival_id, body],
                |row| Ok(IrRivalScoreCacheState { etag: row.get(0)?, fetched_at: row.get(1)? }),
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn rival_scores(
        &self,
        provider: &str,
        rival_id: &str,
        body: &str,
    ) -> Result<Vec<IrRivalScoreRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT chart_sha256, ln_mode, ex_score, clear_type, max_combo, min_bp,
                    play_option, arrange_1p, arrange_2p, double_option, play_seed
             FROM ir_rival_scores
             WHERE provider = ?1 AND rival_id = ?2 AND body = ?3
             ORDER BY chart_sha256, ln_mode",
        )?;
        statement
            .query_map(params![provider, rival_id, body], |row| {
                let hash: String = row.get(0)?;
                let chart_sha256 = hex_to_hash::<32>(&hash).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(IrRivalScoreRecord {
                    chart_sha256,
                    ln_mode: row.get::<_, i64>(1)?.clamp(0, u8::MAX as i64) as u8,
                    ex_score: row.get::<_, i64>(2)?.clamp(0, u32::MAX as i64) as u32,
                    clear_type: row.get(3)?,
                    max_combo: row.get::<_, i64>(4)?.clamp(0, u32::MAX as i64) as u32,
                    min_bp: row.get::<_, i64>(5)?.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                    play_option: row.get(6)?,
                    arrange_1p: row.get(7)?,
                    arrange_2p: row.get(8)?,
                    double_option: row.get(9)?,
                    play_seed: row.get(10)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// サーバー応答は全件 snapshot なので、同じ rival/body の旧キャッシュを
    /// 1 transaction で置き換える。304 の場合は呼ばない。
    pub fn replace_rival_scores(
        &mut self,
        provider: &str,
        rival_id: &str,
        body: &str,
        records: &[IrRivalScoreRecord],
        etag: &str,
        fetched_at: i64,
    ) -> Result<()> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM ir_rival_scores
             WHERE provider = ?1 AND rival_id = ?2 AND body = ?3",
            params![provider, rival_id, body],
        )?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO ir_rival_scores (
                    provider, rival_id, body, chart_sha256, ln_mode, ex_score,
                    clear_type, max_combo, min_bp, play_option, arrange_1p,
                    arrange_2p, double_option, play_seed, fetched_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;
            for record in records {
                insert.execute(params![
                    provider,
                    rival_id,
                    body,
                    hash_to_hex(&record.chart_sha256),
                    record.ln_mode,
                    record.ex_score,
                    record.clear_type,
                    record.max_combo,
                    record.min_bp,
                    record.play_option,
                    record.arrange_1p,
                    record.arrange_2p,
                    record.double_option,
                    record.play_seed,
                    fetched_at,
                ])?;
            }
        }
        tx.execute(
            "INSERT INTO ir_rival_score_sync (provider, rival_id, body, etag, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(provider, rival_id, body) DO UPDATE SET
                 etag = excluded.etag,
                 fetched_at = excluded.fetched_at",
            params![provider, rival_id, body, etag, fetched_at],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn enqueue_ir_score_job(&mut self, job: &NewIrScoreJob) -> Result<i64> {
        let submission_key = job_submission_key(job);
        self.conn.execute(
            "INSERT INTO ir_score_jobs (
                provider, account_id, kind, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, submission_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8, '', ?8, ?8, ?9)
            ON CONFLICT(provider, account_id, kind, submission_key) DO UPDATE SET
                payload_json = excluded.payload_json,
                status = 'pending',
                next_attempt_at = excluded.next_attempt_at,
                last_error = '',
                updated_at = excluded.updated_at
            WHERE ir_score_jobs.submission_key LIKE 'local:%'
              AND ir_score_jobs.chart_sha256 = excluded.chart_sha256
              AND ir_score_jobs.ln_policy = excluded.ln_policy",
            params![
                job.provider,
                job.account_id,
                job.kind.as_str(),
                job.local_score_id,
                hash_to_hex(&job.chart_sha256),
                job.ln_policy.as_str(),
                job.payload_json,
                job.now,
                submission_key,
            ],
        )?;
        let (id, chart, ln_policy, payload): (i64, String, String, String) = self.conn.query_row(
            "SELECT id, chart_sha256, ln_policy, payload_json FROM ir_score_jobs
             WHERE provider = ?1 AND account_id = ?2 AND kind = ?3 AND submission_key = ?4",
            params![job.provider, job.account_id, job.kind.as_str(), submission_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        anyhow::ensure!(
            chart == hash_to_hex(&job.chart_sha256)
                && ln_policy == job.ln_policy.as_str()
                && (payload.is_empty() || payload == job.payload_json),
            "IR submission identifier collision for job {id}"
        );
        Ok(id)
    }

    /// Backfill-only resend path. Existing payloads and idempotency keys are kept byte-for-byte;
    /// a succeeded job whose payload was already discarded can only be retried from its snapshot.
    pub fn enqueue_backfill_score_resend(
        &mut self,
        provider: &str,
        account_id: &str,
        local_score_id: i64,
        now: i64,
        fallback: Option<&NewIrScoreJob>,
        preview: bool,
    ) -> Result<IrBackfillResendOutcome> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active: Option<(String, i64)> = tx.query_row(
            "SELECT status, updated_at FROM ir_score_jobs
             WHERE provider = ?1 AND account_id = ?2 AND kind = 'score' AND local_score_id = ?3
               AND status IN ('pending', 'sending')
             ORDER BY CASE status WHEN 'pending' THEN 0 ELSE 1 END, updated_at DESC, id DESC LIMIT 1",
            params![provider, account_id, local_score_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        if let Some((status, updated_at)) = active {
            if status == "pending" {
                tx.commit()?;
                return Ok(IrBackfillResendOutcome::AlreadyPending);
            }
            if updated_at > now.saturating_sub(IR_JOB_SENDING_STALE_AFTER_SECONDS) {
                tx.commit()?;
                return Ok(IrBackfillResendOutcome::Busy);
            }
        }
        let existing: Option<(i64, String, String, String)> = tx
            .query_row(
                "SELECT id, status, payload_json, submission_key FROM ir_score_jobs
                 WHERE provider = ?1 AND account_id = ?2 AND kind = 'score' AND local_score_id = ?3
                 ORDER BY updated_at DESC, id DESC LIMIT 1",
                params![provider, account_id, local_score_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        // pendingと有効なleaseは上で返している。ここに来るjobは failed / succeeded /
        // leaseの切れた sending のいずれか。
        if let Some((job_id, status, payload, submission_key)) = existing {
            let (payload, chart, ln): (String, String, String) = if status == "succeeded" {
                tx.query_row(
                    "SELECT submission_payload_json, submission_key, chart_sha256, ln_policy
                     FROM ir_score_success_ledger
                     WHERE provider = ?1 AND account_id = ?2 AND kind = 'score' AND local_score_id = ?3
                       AND submission_key = ?4
                     ORDER BY submitted_at DESC, submission_key DESC LIMIT 1",
                    params![provider, account_id, local_score_id, submission_key],
                    |row| Ok((row.get(0)?, row.get(2)?, row.get(3)?)),
                ).optional()?.unwrap_or_default()
            } else {
                let (chart, ln): (String, String) = tx.query_row(
                    "SELECT chart_sha256, ln_policy FROM ir_score_jobs WHERE id = ?1",
                    [job_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                (payload, chart, ln)
            };
            if payload.is_empty() {
                tx.commit()?;
                return Ok(IrBackfillResendOutcome::PayloadUnavailable);
            }
            if preview {
                tx.commit()?;
                return Ok(IrBackfillResendOutcome::Enqueued);
            }
            // 明示的な再送なので、次に失敗したときのbackoffは初回と同じ長さから数え直す。
            tx.execute(
                "UPDATE ir_score_jobs SET payload_json = ?2, status = 'pending',
                    attempt_count = 0, next_attempt_at = ?3, last_error = '', updated_at = ?3,
                    chart_sha256 = ?4, ln_policy = ?5
                 WHERE id = ?1",
                params![job_id, payload, now, chart, ln],
            )?;
            tx.commit()?;
            return Ok(IrBackfillResendOutcome::Enqueued);
        }

        let snapshot: Option<(String, String, String, String)> = tx
            .query_row(
                "SELECT submission_payload_json, submission_key, chart_sha256, ln_policy
                 FROM ir_score_success_ledger
                 WHERE provider = ?1 AND account_id = ?2 AND kind = 'score' AND local_score_id = ?3
                 ORDER BY submitted_at DESC, submission_key DESC LIMIT 1",
                params![provider, account_id, local_score_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let (job, key) = if let Some((payload, key, chart, ln)) = snapshot {
            if payload.is_empty() {
                tx.commit()?;
                return Ok(IrBackfillResendOutcome::PayloadUnavailable);
            }
            (
                NewIrScoreJob {
                    provider: provider.to_owned(),
                    account_id: account_id.to_owned(),
                    kind: IrJobKind::Score,
                    local_score_id,
                    chart_sha256: hex_to_hash(&chart)?,
                    ln_policy: LnScorePolicy::from_str_opt(&ln)
                        .context("invalid LN policy in IR success ledger")?,
                    payload_json: payload,
                    now,
                },
                key,
            )
        } else if let Some(job) = fallback {
            (job.clone(), job_submission_key(job))
        } else {
            tx.commit()?;
            return Ok(IrBackfillResendOutcome::NeedsPayload);
        };
        if preview {
            tx.commit()?;
            return Ok(IrBackfillResendOutcome::Enqueued);
        }
        tx.execute(
            "INSERT INTO ir_score_jobs (
                provider, account_id, kind, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, submission_key
             ) VALUES (?1, ?2, 'score', ?3, ?4, ?5, ?6, 'pending', 0, ?7, '', ?7, ?7, ?8)",
            params![
                job.provider,
                job.account_id,
                job.local_score_id,
                hash_to_hex(&job.chart_sha256),
                job.ln_policy.as_str(),
                job.payload_json,
                now,
                key
            ],
        )?;
        tx.commit()?;
        Ok(IrBackfillResendOutcome::Enqueued)
    }

    pub fn pending_ir_score_jobs(&self, now: i64, limit: u32) -> Result<Vec<IrScoreJobRecord>> {
        self.pending_ir_score_jobs_with_backoff_policy(now, limit, false)
    }

    pub fn pending_ir_score_jobs_ignoring_backoff(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<Vec<IrScoreJobRecord>> {
        self.pending_ir_score_jobs_with_backoff_policy(now, limit, true)
    }

    /// 指定した今回のプレイに紐付く IR ジョブを返す。
    ///
    /// 結果画面は常駐同期と同じ DB を共有するため、送信バッチ全体の集計ではなく
    /// この attempt の状態を監視して skin の IR 送信タイマーを更新する。
    pub fn ir_score_jobs_for_local_score(
        &self,
        kind: IrJobKind,
        local_score_id: i64,
    ) -> Result<Vec<IrScoreJobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE kind = ?1 AND local_score_id = ?2
               AND id IN (SELECT MAX(id) FROM ir_score_jobs
                          WHERE kind = ?1 AND local_score_id = ?2
                          GROUP BY provider, account_id)
             ORDER BY id ASC",
        )?;
        stmt.query_map(params![kind.as_str(), local_score_id], ir_score_job_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn pending_ir_score_jobs_for_kind(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        now: i64,
        limit: u32,
        retry: IrJobRetryFilter,
    ) -> Result<Vec<IrScoreJobRecord>> {
        const SENDING_STALE_AFTER_SECONDS: i64 = 300;
        let retry_filter = retry.sql_condition();
        let sql = format!(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE provider = ?4
               AND account_id = ?5
               AND kind = ?6
               AND (({retry_filter})
                    OR (status = 'sending' AND updated_at <= ?1 - ?3))
             ORDER BY next_attempt_at ASC, id ASC
             LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        stmt.query_map(
            params![now, limit, SENDING_STALE_AFTER_SECONDS, provider, account_id, kind.as_str()],
            ir_score_job_from_row,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
    }

    pub fn claim_pending_ir_score_jobs(
        &mut self,
        now: i64,
        limit: u32,
        ignore_retry_backoff: bool,
    ) -> Result<Vec<IrScoreJobRecord>> {
        const SENDING_STALE_AFTER_SECONDS: i64 = 300;
        let retry_filter = if ignore_retry_backoff {
            "status IN ('pending', 'failed')"
        } else {
            "status IN ('pending', 'failed') AND next_attempt_at <= ?1"
        };
        let sql = format!(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE ({retry_filter})
                OR (status = 'sending' AND updated_at <= ?1 - ?3)
             ORDER BY next_attempt_at ASC, id ASC
             LIMIT ?2"
        );
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let jobs = {
            let mut stmt = tx.prepare(&sql)?;
            stmt.query_map(params![now, limit, SENDING_STALE_AFTER_SECONDS], ir_score_job_from_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for job in &jobs {
            tx.execute(
                "UPDATE ir_score_jobs
                 SET status = 'sending', updated_at = ?2
                 WHERE id = ?1",
                params![job.id, now],
            )?;
        }
        tx.commit()?;
        Ok(jobs)
    }

    pub fn claim_pending_ir_score_jobs_for_kind(
        &mut self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        now: i64,
        limit: u32,
        retry: IrJobRetryFilter,
    ) -> Result<Vec<IrScoreJobRecord>> {
        const SENDING_STALE_AFTER_SECONDS: i64 = 300;
        let retry_filter = retry.sql_condition();
        let sql = format!(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE provider = ?4
               AND account_id = ?5
               AND kind = ?6
               AND (({retry_filter})
                    OR (status = 'sending' AND updated_at <= ?1 - ?3))
             ORDER BY next_attempt_at ASC, id ASC
             LIMIT ?2"
        );
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let jobs = {
            let mut stmt = tx.prepare(&sql)?;
            stmt.query_map(
                params![
                    now,
                    limit,
                    SENDING_STALE_AFTER_SECONDS,
                    provider,
                    account_id,
                    kind.as_str()
                ],
                ir_score_job_from_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for job in &jobs {
            tx.execute(
                "UPDATE ir_score_jobs
                 SET status = 'sending', updated_at = ?2
                 WHERE id = ?1",
                params![job.id, now],
            )?;
        }
        tx.commit()?;
        Ok(jobs)
    }

    /// Result で表示中の attempt に対応する指定provider/accountのjobだけをclaimする。
    ///
    /// 通常のバッチ順に依存すると、古いpending jobが上限を埋めた場合や別taskと
    /// claimが競合した場合に、Resultが今回の送信完了を待ち切れない。
    pub fn claim_pending_ir_score_job_for_local_score(
        &mut self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        local_score_id: i64,
        now: i64,
        ignore_retry_backoff: bool,
    ) -> Result<Vec<IrScoreJobRecord>> {
        const SENDING_STALE_AFTER_SECONDS: i64 = 300;
        let retry_filter = if ignore_retry_backoff {
            "status IN ('pending', 'failed')"
        } else {
            "status IN ('pending', 'failed') AND next_attempt_at <= ?1"
        };
        let sql = format!(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE provider = ?3
               AND account_id = ?4
               AND kind = ?5
               AND local_score_id = ?6
               AND id = (SELECT MAX(id) FROM ir_score_jobs
                         WHERE provider = ?3 AND account_id = ?4
                           AND kind = ?5 AND local_score_id = ?6)
               AND (({retry_filter})
                    OR (status = 'sending' AND updated_at <= ?1 - ?2))
             LIMIT 1"
        );
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let jobs = {
            let mut stmt = tx.prepare(&sql)?;
            stmt.query_map(
                params![
                    now,
                    SENDING_STALE_AFTER_SECONDS,
                    provider,
                    account_id,
                    kind.as_str(),
                    local_score_id,
                ],
                ir_score_job_from_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for job in &jobs {
            tx.execute(
                "UPDATE ir_score_jobs
                 SET status = 'sending', updated_at = ?2
                 WHERE id = ?1",
                params![job.id, now],
            )?;
        }
        tx.commit()?;
        Ok(jobs)
    }

    pub fn has_ir_score_job(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        local_score_id: i64,
    ) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1
                 FROM ir_score_jobs
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND kind = ?3
                   AND local_score_id = ?4
                 LIMIT 1",
                params![provider, account_id, kind.as_str(), local_score_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn unfinished_ir_score_job_count_for_kind(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
    ) -> Result<u32> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM ir_score_jobs
             WHERE provider = ?1
               AND account_id = ?2
               AND kind = ?3
               AND status != 'succeeded'",
            params![provider, account_id, kind.as_str()],
            |row| row.get(0),
        )?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// 未完了jobを、今claimできるもの・lease中・再試行待ちに分けて数える。
    ///
    /// claimできる条件は `claim_pending_ir_score_jobs_for_kind` と同じ。
    pub fn ir_score_job_backlog_for_kind(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        now: i64,
        retry: IrJobRetryFilter,
    ) -> Result<IrScoreJobBacklog> {
        let retry_filter = retry.sql_condition();
        let sql = format!(
            "SELECT
                COALESCE(SUM(CASE WHEN ({retry_filter})
                                    OR (status = 'sending' AND updated_at <= ?1 - ?2)
                             THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'sending' AND updated_at > ?1 - ?2
                             THEN 1 ELSE 0 END), 0),
                COUNT(*)
             FROM ir_score_jobs
             WHERE provider = ?3
               AND account_id = ?4
               AND kind = ?5
               AND status != 'succeeded'"
        );
        let (claimable, leased, total): (i64, i64, i64) = self.conn.query_row(
            &sql,
            params![now, IR_JOB_SENDING_STALE_AFTER_SECONDS, provider, account_id, kind.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let count = |value: i64| u32::try_from(value.max(0)).unwrap_or(u32::MAX);
        Ok(IrScoreJobBacklog {
            claimable: count(claimable),
            leased: count(leased),
            deferred: count(total - claimable - leased),
        })
    }

    pub fn unfinished_ir_score_local_ids(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
    ) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(
            "SELECT DISTINCT local_score_id FROM ir_score_jobs
             WHERE provider = ?1 AND account_id = ?2 AND kind = ?3
               AND status IN ('pending', 'sending', 'failed')",
        )?;
        statement
            .query_map(params![provider, account_id, kind.as_str()], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// 送信キューの一覧。`payload_json` を読まない軽量版。
    ///
    /// 並びは status の重要度（failed → sending → pending → succeeded）、
    /// 次に `updated_at` の新しい順、最後に `id` の新しい順。
    /// ユーザーが対処すべき失敗を先頭に固定し、同順位内は直近に動いたものを上にする。
    /// `updated_at` が同値でも `id` で全順序になるため、offset によるページングで
    /// 行の重複・欠落が起きない。
    pub fn list_ir_score_jobs(
        &self,
        filter: &IrScoreJobListFilter<'_>,
    ) -> Result<Vec<IrScoreJobSummary>> {
        let (where_sql, mut values) = ir_score_job_filter_sql(filter);
        let sql = format!(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                status, attempt_count, next_attempt_at, last_error, created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE {where_sql}
             ORDER BY CASE status
                          WHEN 'failed' THEN 0
                          WHEN 'sending' THEN 1
                          WHEN 'pending' THEN 2
                          ELSE 3
                      END ASC,
                      updated_at DESC, id DESC
             LIMIT ? OFFSET ?"
        );
        values.push(rusqlite::types::Value::Integer(i64::from(filter.limit)));
        values.push(rusqlite::types::Value::Integer(i64::from(filter.offset)));
        let mut stmt = self.conn.prepare(&sql)?;
        stmt.query_map(params_from_iter(values), ir_score_job_summary_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// `list_ir_score_jobs` と同じ絞り込みの総件数（limit / offset は無視する）。
    pub fn count_ir_score_jobs(&self, filter: &IrScoreJobListFilter<'_>) -> Result<u32> {
        let (where_sql, values) = ir_score_job_filter_sql(filter);
        let sql = format!("SELECT COUNT(*) FROM ir_score_jobs WHERE {where_sql}");
        let count: i64 = self.conn.query_row(&sql, params_from_iter(values), |row| row.get(0))?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// status別のjob数。`provider` / `account_id` が `None` なら絞り込まない。
    pub fn ir_score_job_status_counts(
        &self,
        provider: Option<&str>,
        account_id: Option<&str>,
    ) -> Result<IrScoreJobStatusCounts> {
        let mut stmt = self.conn.prepare(
            "SELECT status, COUNT(*)
             FROM ir_score_jobs
             WHERE (?1 IS NULL OR provider = ?1)
               AND (?2 IS NULL OR account_id = ?2)
             GROUP BY status",
        )?;
        let rows = stmt.query_map(params![provider, account_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counts = IrScoreJobStatusCounts::default();
        for row in rows {
            let (status, count) = row?;
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            match status.as_str() {
                "pending" => counts.pending = count,
                "sending" => counts.sending = count,
                "failed" => counts.failed = count,
                "succeeded" => counts.succeeded = count,
                _ => {}
            }
        }
        Ok(counts)
    }

    /// 成功済み単曲scoreのremote idから、後付け署名用jobを重複なく投入する。
    pub fn enqueue_ir_score_attestation_jobs(
        &mut self,
        provider: &str,
        account_id: &str,
        now: i64,
    ) -> Result<u32> {
        let submitted = {
            let mut statement = self.conn.prepare(
                "SELECT DISTINCT local_score_id, remote_score_id
                 FROM ir_score_success_ledger
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND kind = 'score'
                   AND remote_score_id != ''
                 ORDER BY local_score_id ASC",
            )?;
            statement
                .query_map(params![provider, account_id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        let mut enqueued = 0;
        for (local_score_id, remote_score_id) in submitted {
            if self.has_successful_ir_score_submission_for_remote_score(
                provider,
                account_id,
                IrJobKind::Attestation,
                local_score_id,
                &remote_score_id,
            )? {
                continue;
            }
            if self.has_ir_score_job_for_remote_score(
                provider,
                account_id,
                IrJobKind::Attestation,
                local_score_id,
                &remote_score_id,
            )? {
                continue;
            }
            let payload_json = serde_json::to_string(&serde_json::json!({
                "remote_score_id": remote_score_id,
            }))?;
            self.enqueue_ir_score_job(&NewIrScoreJob {
                provider: provider.to_string(),
                account_id: account_id.to_string(),
                kind: IrJobKind::Attestation,
                local_score_id,
                chart_sha256: [0; 32],
                ln_policy: LnScorePolicy::AutoLn,
                payload_json,
                now,
            })?;
            enqueued += 1;
        }
        Ok(enqueued)
    }

    fn pending_ir_score_jobs_with_backoff_policy(
        &self,
        now: i64,
        limit: u32,
        ignore_retry_backoff: bool,
    ) -> Result<Vec<IrScoreJobRecord>> {
        const SENDING_STALE_AFTER_SECONDS: i64 = 300;
        let retry_filter = if ignore_retry_backoff {
            "status IN ('pending', 'failed')"
        } else {
            "status IN ('pending', 'failed') AND next_attempt_at <= ?1"
        };
        let sql = format!(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE ({retry_filter})
                OR (status = 'sending' AND updated_at <= ?1 - ?3)
             ORDER BY next_attempt_at ASC, id ASC
             LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        stmt.query_map(params![now, limit, SENDING_STALE_AFTER_SECONDS], ir_score_job_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn mark_ir_score_job_status(
        &mut self,
        job_id: i64,
        status: IrScoreJobStatus,
        now: i64,
        last_error: &str,
    ) -> Result<()> {
        // 失敗時は失敗回数に応じた段階的バックオフで次回試行時刻を決める
        // (docs/ir.md: 1分 → 5分 → 30分 → 2時間 → 24時間)。
        // attempt_count はこの UPDATE 内でインクリメントする前の値を参照する。
        self.conn.execute(
            "UPDATE ir_score_jobs
             SET status = ?2,
                 attempt_count = attempt_count + CASE WHEN ?2 = 'failed' THEN 1 ELSE 0 END,
                 next_attempt_at = CASE WHEN ?2 = 'failed'
                     THEN ?3 + CASE
                         WHEN attempt_count <= 0 THEN 60
                         WHEN attempt_count = 1 THEN 300
                         WHEN attempt_count = 2 THEN 1800
                         WHEN attempt_count = 3 THEN 7200
                         ELSE 86400
                     END
                     ELSE next_attempt_at END,
                 payload_json = CASE WHEN ?2 = 'succeeded' THEN '' ELSE payload_json END,
                 last_error = ?4,
                 updated_at = ?3
             WHERE id = ?1",
            params![job_id, status.as_str(), now, last_error],
        )?;
        Ok(())
    }

    pub fn mark_ir_score_job_failed(
        &mut self,
        job_id: i64,
        now: i64,
        last_error: &str,
        retry_after_seconds: Option<u64>,
    ) -> Result<()> {
        let retry_at = retry_after_seconds
            .map(|seconds| now.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX)));
        self.conn.execute(
            "UPDATE ir_score_jobs
             SET status = 'failed',
                 attempt_count = attempt_count + 1,
                 next_attempt_at = COALESCE(
                     ?4,
                     ?2 + CASE
                         WHEN attempt_count <= 0 THEN 60
                         WHEN attempt_count = 1 THEN 300
                         WHEN attempt_count = 2 THEN 1800
                         WHEN attempt_count = 3 THEN 7200
                         ELSE 86400
                     END
                 ),
                 last_error = ?3,
                 updated_at = ?2
             WHERE id = ?1",
            params![job_id, now, last_error, retry_at],
        )?;
        Ok(())
    }

    /// サーバーが恒久的に拒否したjob（アカウント停止・BAN、譜面の非公開・ブロック）を記録する。
    ///
    /// ローカルスコアとjobは残すが、`next_attempt_at` を自動再試行が届かない時刻にして
    /// 常駐同期やリザルト画面からの再送を止める。制限解除後は手動の再送で送り直せる。
    pub fn mark_ir_score_job_rejected(
        &mut self,
        job_id: i64,
        now: i64,
        last_error: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE ir_score_jobs
             SET status = 'failed',
                 attempt_count = attempt_count + 1,
                 next_attempt_at = ?3,
                 last_error = ?4,
                 updated_at = ?2
             WHERE id = ?1",
            params![job_id, now, IR_JOB_NO_AUTO_RETRY_AT, last_error],
        )?;
        Ok(())
    }

    pub fn insert_ir_score_submission(&mut self, record: &NewIrScoreSubmission) -> Result<i64> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO ir_score_submissions (
                job_id, provider, account_id, kind, local_score_id, remote_score_id,
                status, submitted_at, log_path, error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.job_id,
                record.provider,
                record.account_id,
                record.kind.as_str(),
                record.local_score_id,
                record.remote_score_id,
                record.status,
                record.submitted_at,
                record.log_path,
                record.error,
            ],
        )?;
        let id = tx.last_insert_rowid();
        upsert_ir_score_success_ledger(&tx, record)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn complete_ir_score_job(
        &mut self,
        record: &NewIrScoreSubmission,
        replay_job: Option<&NewIrScoreJob>,
        response_json: &str,
    ) -> Result<()> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO ir_score_submissions (
                job_id, provider, account_id, kind, local_score_id, remote_score_id,
                status, submitted_at, log_path, error, response_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.job_id,
                record.provider,
                record.account_id,
                record.kind.as_str(),
                record.local_score_id,
                record.remote_score_id,
                record.status,
                record.submitted_at,
                record.log_path,
                record.error,
                response_json,
            ],
        )?;
        upsert_ir_score_success_ledger(&tx, record)?;
        if let Some(job) = replay_job {
            tx.execute(
                "INSERT INTO ir_score_jobs (
                    provider, account_id, kind, local_score_id, chart_sha256, ln_policy,
                    payload_json, status, attempt_count, next_attempt_at, last_error,
                    created_at, updated_at, submission_key
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8, '', ?8, ?8, ?9)
                ON CONFLICT(provider, account_id, kind, submission_key) DO UPDATE SET
                    payload_json = excluded.payload_json,
                    status = 'pending',
                    attempt_count = 0,
                    next_attempt_at = excluded.next_attempt_at,
                    last_error = '',
                    updated_at = excluded.updated_at",
                params![
                    job.provider,
                    job.account_id,
                    job.kind.as_str(),
                    job.local_score_id,
                    hash_to_hex(&job.chart_sha256),
                    job.ln_policy.as_str(),
                    job.payload_json,
                    job.now,
                    job_submission_key(job),
                ],
            )?;
        }
        tx.execute(
            "UPDATE ir_score_jobs
             SET status = 'succeeded', payload_json = '', last_error = '', updated_at = ?2
             WHERE id = ?1",
            params![record.job_id, record.submitted_at],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Result表示用に、別taskが完了した送信の応答を同じattemptから取得する。
    pub fn latest_ir_score_submission_response(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        local_score_id: i64,
    ) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT response_json
                 FROM ir_score_submissions
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND kind = ?3
                   AND local_score_id = ?4
                   AND status = 'succeeded'
                   AND response_json != ''
                   AND job_id = (SELECT MAX(id) FROM ir_score_jobs
                                 WHERE provider = ?1 AND account_id = ?2
                                   AND kind = ?3 AND local_score_id = ?4)
                 ORDER BY submitted_at DESC, id DESC
                 LIMIT 1",
                params![provider, account_id, kind.as_str(), local_score_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn local_score_id_for_remote_score(
        &self,
        provider: &str,
        account_id: &str,
        remote_score_id: &str,
    ) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT local_score_id
                 FROM ir_score_success_ledger
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND kind = 'score'
                   AND remote_score_id = ?3
                 ORDER BY submitted_at DESC
                 LIMIT 1",
                params![provider, account_id, remote_score_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// 投稿時のidempotency keyから、そのscoreを送ったローカル履歴を引く。
    ///
    /// 成功台帳を優先し、応答を受け取れず台帳に無い送信（失敗・送信中のjob）も
    /// job本体の `submission_key` から辿る。
    pub fn local_score_id_for_submission_key(
        &self,
        provider: &str,
        account_id: &str,
        submission_key: &str,
    ) -> Result<Option<i64>> {
        if submission_key.is_empty() {
            return Ok(None);
        }
        let ledger = self
            .conn
            .query_row(
                "SELECT local_score_id FROM ir_score_success_ledger
                 WHERE provider = ?1 AND account_id = ?2 AND kind = 'score'
                   AND submission_key = ?3
                 ORDER BY submitted_at DESC
                 LIMIT 1",
                params![provider, account_id, submission_key],
                |row| row.get(0),
            )
            .optional()?;
        if ledger.is_some() {
            return Ok(ledger);
        }
        self.conn
            .query_row(
                "SELECT local_score_id FROM ir_score_jobs
                 WHERE provider = ?1 AND account_id = ?2 AND kind = 'score'
                   AND submission_key = ?3
                 ORDER BY updated_at DESC, id DESC
                 LIMIT 1",
                params![provider, account_id, submission_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn has_successful_ir_score_submission(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        local_score_id: i64,
    ) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM ir_score_success_ledger
             WHERE provider = ?1 AND account_id = ?2 AND kind = ?3 AND local_score_id = ?4
             LIMIT 1",
                params![provider, account_id, kind.as_str(), local_score_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    fn has_successful_ir_score_submission_for_remote_score(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        local_score_id: i64,
        remote_score_id: &str,
    ) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM ir_score_success_ledger
                 WHERE provider = ?1 AND account_id = ?2 AND kind = ?3
                   AND local_score_id = ?4 AND remote_score_id = ?5
                 LIMIT 1",
                params![provider, account_id, kind.as_str(), local_score_id, remote_score_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    fn has_ir_score_job_for_remote_score(
        &self,
        provider: &str,
        account_id: &str,
        kind: IrJobKind,
        local_score_id: i64,
        remote_score_id: &str,
    ) -> Result<bool> {
        let mut statement = self.conn.prepare(
            "SELECT payload_json FROM ir_score_jobs
             WHERE provider = ?1 AND account_id = ?2 AND kind = ?3
               AND local_score_id = ?4 AND status IN ('pending', 'sending', 'failed')",
        )?;
        let payloads = statement
            .query_map(params![provider, account_id, kind.as_str(), local_score_id], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(payloads.into_iter().any(|payload| {
            serde_json::from_str::<serde_json::Value>(&payload)
                .ok()
                .and_then(|value| value.get("remote_score_id")?.as_str().map(str::to_owned))
                .as_deref()
                == Some(remote_score_id)
        }))
    }

    pub fn prune_succeeded_ir_score_jobs(&mut self, now: i64) -> Result<usize> {
        self.prune_succeeded_ir_score_jobs_with_policy(
            now,
            SUCCEEDED_IR_SCORE_JOB_RETENTION_SECONDS,
            SUCCEEDED_IR_SCORE_JOB_RETAIN_RECENT_COUNT,
        )
    }

    pub(super) fn prune_succeeded_ir_score_jobs_with_policy(
        &mut self,
        now: i64,
        retention_seconds: i64,
        retain_recent_count: u32,
    ) -> Result<usize> {
        let cutoff = now.saturating_sub(retention_seconds);
        let deleted = self.conn.execute(
            "DELETE FROM ir_score_jobs
             WHERE status = 'succeeded'
               AND updated_at < ?1
               AND id NOT IN (
                    SELECT id
                    FROM ir_score_jobs
                    WHERE status = 'succeeded'
                    ORDER BY updated_at DESC, id DESC
                    LIMIT ?2
               )",
            params![cutoff, retain_recent_count],
        )?;
        Ok(deleted)
    }

    /// 指定した local score id に紐付く、全 provider の受理済み単曲 score を返す。
    ///
    /// cleanup 実行前に、選択した provider 以外へ送信済みの履歴を見落とさないために
    /// provider / account を絞らない。
    pub fn successful_ir_score_submissions_for_local_scores(
        &self,
        local_score_ids: &[i64],
    ) -> Result<Vec<IrSubmittedScoreLink>> {
        const QUERY_CHUNK_SIZE: usize = 500;
        let mut links = Vec::new();
        for ids in local_score_ids.chunks(QUERY_CHUNK_SIZE) {
            let placeholders = sql_placeholders(ids.len());
            let sql = format!(
                "SELECT DISTINCT provider, account_id, local_score_id, remote_score_id
                 FROM ir_score_success_ledger
                 WHERE kind = 'score'
                   AND remote_score_id != ''
                   AND local_score_id IN ({placeholders})
                 ORDER BY provider, account_id, local_score_id, remote_score_id"
            );
            let mut statement = self.conn.prepare(&sql)?;
            let rows = statement
                .query_map(params_from_iter(ids.iter()), |row| {
                    Ok(IrSubmittedScoreLink {
                        provider: row.get(0)?,
                        account_id: row.get(1)?,
                        local_score_id: row.get(2)?,
                        remote_score_id: row.get(3)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            links.extend(rows);
        }
        Ok(links)
    }

    /// 指定 provider/account の古い local score に紐付く送信台帳とジョブを削除する。
    ///
    /// IR 本体の削除に成功した後で呼び出す。score/replay/attestation を含め、消える
    /// score_history への参照を残さない。
    pub fn purge_ir_records_for_local_scores(
        &mut self,
        provider: &str,
        account_id: &str,
        local_score_ids: &[i64],
    ) -> Result<IrLocalScoreCleanupReport> {
        const DELETE_CHUNK_SIZE: usize = 500;
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut report = IrLocalScoreCleanupReport::default();
        for ids in local_score_ids.chunks(DELETE_CHUNK_SIZE) {
            let placeholders = sql_placeholders(ids.len());
            let parameters = || {
                std::iter::once(&provider as &dyn rusqlite::ToSql)
                    .chain(std::iter::once(&account_id as &dyn rusqlite::ToSql))
                    .chain(ids.iter().map(|id| id as &dyn rusqlite::ToSql))
            };
            let submissions_sql = format!(
                "DELETE FROM ir_score_submissions
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND local_score_id IN ({placeholders})"
            );
            report.removed_submissions = report.removed_submissions.saturating_add(
                tx.execute(&submissions_sql, params_from_iter(parameters()))? as u32,
            );
            let ledger_sql = format!(
                "DELETE FROM ir_score_success_ledger
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND local_score_id IN ({placeholders})"
            );
            tx.execute(&ledger_sql, params_from_iter(parameters()))?;
            let jobs_sql = format!(
                "DELETE FROM ir_score_jobs
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND local_score_id IN ({placeholders})"
            );
            report.removed_jobs = report
                .removed_jobs
                .saturating_add(tx.execute(&jobs_sql, params_from_iter(parameters()))? as u32);
        }
        tx.commit()?;
        Ok(report)
    }
}
use super::rows::{ir_score_job_from_row, ir_score_job_summary_from_row, sql_placeholders};
use super::*;

fn ir_score_job_filter_sql(
    filter: &IrScoreJobListFilter<'_>,
) -> (String, Vec<rusqlite::types::Value>) {
    use rusqlite::types::Value;

    let mut clauses = Vec::new();
    let mut values = Vec::new();
    if let Some(provider) = filter.provider {
        clauses.push("provider = ?".to_string());
        values.push(Value::Text(provider.to_string()));
    }
    if let Some(account_id) = filter.account_id {
        clauses.push("account_id = ?".to_string());
        values.push(Value::Text(account_id.to_string()));
    }
    if !filter.kinds.is_empty() {
        clauses.push(format!("kind IN ({})", sql_placeholders(filter.kinds.len())));
        values.extend(filter.kinds.iter().map(|kind| Value::Text(kind.as_str().to_string())));
    }
    if filter.statuses.is_empty() {
        clauses.push("status != 'succeeded'".to_string());
    } else {
        clauses.push(format!("status IN ({})", sql_placeholders(filter.statuses.len())));
        values
            .extend(filter.statuses.iter().map(|status| Value::Text(status.as_str().to_string())));
    }
    (clauses.join(" AND "), values)
}
