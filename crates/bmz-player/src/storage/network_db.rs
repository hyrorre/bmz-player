use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params, params_from_iter};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};
use crate::ln_policy::LnScorePolicy;

const SUCCEEDED_IR_SCORE_JOB_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
const SUCCEEDED_IR_SCORE_JOB_RETAIN_RECENT_COUNT: u32 = 500;

pub struct NetworkDatabase {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrScoreJobStatus {
    Pending,
    Sending,
    Succeeded,
    Failed,
}

/// IR ジョブの種別。単曲スコア、コーススコア、リプレイ、または既送信scoreへの署名。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrJobKind {
    Score,
    Course,
    Replay,
    Attestation,
}

impl IrJobKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Score => "score",
            Self::Course => "course",
            Self::Replay => "replay",
            Self::Attestation => "attestation",
        }
    }

    pub fn from_str_or_score(value: &str) -> Self {
        match value {
            "course" => Self::Course,
            "replay" => Self::Replay,
            "attestation" => Self::Attestation,
            _ => Self::Score,
        }
    }
}

impl IrScoreJobStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sending => "sending",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrScoreJobRecord {
    pub id: i64,
    pub provider: String,
    pub account_id: String,
    pub kind: IrJobKind,
    pub local_score_id: i64,
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub payload_json: String,
    pub status: String,
    pub attempt_count: u32,
    pub next_attempt_at: i64,
    pub last_error: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewIrScoreJob {
    pub provider: String,
    pub account_id: String,
    pub kind: IrJobKind,
    pub local_score_id: i64,
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub payload_json: String,
    pub now: i64,
}

#[derive(Debug, Clone)]
pub struct NewIrScoreSubmission {
    pub job_id: i64,
    pub provider: String,
    pub account_id: String,
    pub kind: IrJobKind,
    pub local_score_id: i64,
    pub remote_score_id: String,
    pub status: String,
    pub submitted_at: i64,
    pub log_path: String,
    pub error: String,
}

/// ローカル score_history と対応する、受理済み IR score の記録。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrSubmittedScoreLink {
    pub provider: String,
    pub account_id: String,
    pub local_score_id: i64,
    pub remote_score_id: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IrLocalScoreCleanupReport {
    pub removed_jobs: u32,
    pub removed_submissions: u32,
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

    pub fn enqueue_ir_score_job(&mut self, job: &NewIrScoreJob) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO ir_score_jobs (
                provider, account_id, kind, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8, '', ?8, ?8)
            ON CONFLICT(provider, account_id, kind, local_score_id) DO UPDATE SET
                payload_json = excluded.payload_json,
                status = 'pending',
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
            ],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM ir_score_jobs
             WHERE provider = ?1 AND account_id = ?2 AND kind = ?3 AND local_score_id = ?4",
            params![job.provider, job.account_id, job.kind.as_str(), job.local_score_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(id)
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
                 FROM ir_score_submissions
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND kind = 'score'
                   AND status = 'succeeded'
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
            if self.has_ir_score_job(
                provider,
                account_id,
                IrJobKind::Attestation,
                local_score_id,
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

    pub fn insert_ir_score_submission(&mut self, record: &NewIrScoreSubmission) -> Result<i64> {
        self.conn.execute(
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
        Ok(self.conn.last_insert_rowid())
    }

    pub fn complete_ir_score_job(
        &mut self,
        record: &NewIrScoreSubmission,
        replay_job: Option<&NewIrScoreJob>,
    ) -> Result<()> {
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
        if let Some(job) = replay_job {
            tx.execute(
                "INSERT INTO ir_score_jobs (
                    provider, account_id, kind, local_score_id, chart_sha256, ln_policy,
                    payload_json, status, attempt_count, next_attempt_at, last_error,
                    created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8, '', ?8, ?8)
                ON CONFLICT(provider, account_id, kind, local_score_id) DO UPDATE SET
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

    pub fn local_score_id_for_remote_score(
        &self,
        provider: &str,
        account_id: &str,
        remote_score_id: &str,
    ) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT local_score_id
                 FROM ir_score_submissions
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND kind = 'score'
                   AND remote_score_id = ?3
                   AND status = 'succeeded'
                 ORDER BY submitted_at DESC, id DESC
                 LIMIT 1",
                params![provider, account_id, remote_score_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn prune_succeeded_ir_score_jobs(&mut self, now: i64) -> Result<usize> {
        self.prune_succeeded_ir_score_jobs_with_policy(
            now,
            SUCCEEDED_IR_SCORE_JOB_RETENTION_SECONDS,
            SUCCEEDED_IR_SCORE_JOB_RETAIN_RECENT_COUNT,
        )
    }

    fn prune_succeeded_ir_score_jobs_with_policy(
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
                 FROM ir_score_submissions
                 WHERE kind = 'score'
                   AND status = 'succeeded'
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

fn sql_placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count).collect::<Vec<_>>().join(", ")
}

fn ir_score_job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IrScoreJobRecord> {
    let chart_sha256: String = row.get(4)?;
    let kind: String = row.get(13)?;
    Ok(IrScoreJobRecord {
        id: row.get(0)?,
        provider: row.get(1)?,
        account_id: row.get(2)?,
        kind: IrJobKind::from_str_or_score(&kind),
        local_score_id: row.get(3)?,
        chart_sha256: hex_to_hash(&chart_sha256)?,
        ln_policy: ln_policy_from_row(row, 5)?,
        payload_json: row.get(6)?,
        status: row.get(7)?,
        attempt_count: row.get(8)?,
        next_attempt_at: row.get(9)?,
        last_error: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn ln_policy_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<LnScorePolicy> {
    let value: String = row.get(index)?;
    LnScorePolicy::from_str_opt(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid LN score policy: {value}").into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{NETWORK_MIGRATIONS, run_migrations};

    fn open_network_db() -> NetworkDatabase {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, NETWORK_MIGRATIONS).unwrap();
        NetworkDatabase::from_connection(conn)
    }

    fn enqueue_test_job(db: &mut NetworkDatabase, local_score_id: i64, now: i64) -> i64 {
        db.enqueue_ir_score_job(&NewIrScoreJob {
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id,
            chart_sha256: [local_score_id as u8; 32],
            ln_policy: LnScorePolicy::ForceLn,
            payload_json: "{}".to_string(),
            now,
        })
        .unwrap()
    }

    #[test]
    fn ir_score_jobs_round_trip_and_dedupe_by_provider_account_kind_score() {
        let mut db = open_network_db();
        let job = NewIrScoreJob {
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            chart_sha256: [7; 32],
            ln_policy: LnScorePolicy::ForceLn,
            payload_json: "{\"score\":1}".to_string(),
            now: 100,
        };
        let first_id = db.enqueue_ir_score_job(&job).unwrap();
        let mut updated = job.clone();
        updated.payload_json = "{\"score\":2}".to_string();
        updated.now = 200;
        let second_id = db.enqueue_ir_score_job(&updated).unwrap();

        assert_eq!(first_id, second_id);
        let pending = db.pending_ir_score_jobs(200, 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].payload_json, "{\"score\":2}");
        assert_eq!(pending[0].ln_policy, LnScorePolicy::ForceLn);

        let log_path = "ir-submissions.jsonl".to_string();
        let submission_id = db
            .insert_ir_score_submission(&NewIrScoreSubmission {
                job_id: first_id,
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: 42,
                remote_score_id: "sc_remote".to_string(),
                status: "succeeded".to_string(),
                submitted_at: 220,
                log_path: log_path.clone(),
                error: String::new(),
            })
            .unwrap();
        assert!(submission_id > 0);
        let stored_log_path: String = db
            .conn()
            .query_row(
                "SELECT log_path FROM ir_score_submissions WHERE id = ?1",
                [submission_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_log_path, log_path);

        db.mark_ir_score_job_status(first_id, IrScoreJobStatus::Succeeded, 230, "").unwrap();
        assert!(db.pending_ir_score_jobs(300, 10).unwrap().is_empty());
        let payload_json: String = db
            .conn()
            .query_row("SELECT payload_json FROM ir_score_jobs WHERE id = ?1", [first_id], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(payload_json.is_empty());
    }

    #[test]
    fn cleanup_removes_selected_provider_records_for_removed_local_scores() {
        let mut db = open_network_db();
        let score_job = db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "bmz".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: 42,
                chart_sha256: [1; 32],
                ln_policy: LnScorePolicy::AutoLn,
                payload_json: "{}".to_string(),
                now: 100,
            })
            .unwrap();
        db.enqueue_ir_score_job(&NewIrScoreJob {
            provider: "bmz".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Attestation,
            local_score_id: 42,
            chart_sha256: [0; 32],
            ln_policy: LnScorePolicy::AutoLn,
            payload_json: "{}".to_string(),
            now: 100,
        })
        .unwrap();
        db.insert_ir_score_submission(&NewIrScoreSubmission {
            job_id: score_job,
            provider: "bmz".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            remote_score_id: "remote-42".to_string(),
            status: "succeeded".to_string(),
            submitted_at: 101,
            log_path: String::new(),
            error: String::new(),
        })
        .unwrap();
        db.enqueue_ir_score_job(&NewIrScoreJob {
            provider: "other".to_string(),
            account_id: "account-2".to_string(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            chart_sha256: [2; 32],
            ln_policy: LnScorePolicy::AutoLn,
            payload_json: "{}".to_string(),
            now: 100,
        })
        .unwrap();

        assert_eq!(
            db.successful_ir_score_submissions_for_local_scores(&[42]).unwrap(),
            vec![IrSubmittedScoreLink {
                provider: "bmz".to_string(),
                account_id: "account-1".to_string(),
                local_score_id: 42,
                remote_score_id: "remote-42".to_string(),
            }]
        );
        assert_eq!(
            db.purge_ir_records_for_local_scores("bmz", "account-1", &[42]).unwrap(),
            IrLocalScoreCleanupReport { removed_jobs: 2, removed_submissions: 1 }
        );
        let remaining: Vec<(String, String)> = db
            .conn()
            .prepare("SELECT provider, account_id FROM ir_score_jobs ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(remaining, vec![("other".to_string(), "account-2".to_string())]);
    }

    #[test]
    fn attempt_job_lookup_keeps_kind_and_local_score_isolated() {
        let mut db = open_network_db();
        for (kind, local_score_id) in
            [(IrJobKind::Score, 42), (IrJobKind::Course, 42), (IrJobKind::Score, 43)]
        {
            db.enqueue_ir_score_job(&NewIrScoreJob {
                provider: format!("provider-{local_score_id}-{}", kind.as_str()),
                account_id: "account".to_string(),
                kind,
                local_score_id,
                chart_sha256: [local_score_id as u8; 32],
                ln_policy: LnScorePolicy::AutoLn,
                payload_json: "{}".to_string(),
                now: 100,
            })
            .unwrap();
        }

        let jobs = db.ir_score_jobs_for_local_score(IrJobKind::Score, 42).unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, IrJobKind::Score);
        assert_eq!(jobs[0].local_score_id, 42);
    }

    #[test]
    fn completing_score_job_atomically_enqueues_replay_job() {
        let mut db = open_network_db();
        let score_job_id = enqueue_test_job(&mut db, 42, 100);
        let replay_job = NewIrScoreJob {
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Replay,
            local_score_id: 42,
            chart_sha256: [42; 32],
            ln_policy: LnScorePolicy::ForceLn,
            payload_json: r#"{"remote_score_id":"remote-42"}"#.to_string(),
            now: 200,
        };

        db.complete_ir_score_job(
            &NewIrScoreSubmission {
                job_id: score_job_id,
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: 42,
                remote_score_id: "remote-42".to_string(),
                status: "succeeded".to_string(),
                submitted_at: 200,
                log_path: "ir-submissions.jsonl".to_string(),
                error: String::new(),
            },
            Some(&replay_job),
        )
        .unwrap();

        let (score_status, score_payload): (String, String) = db
            .conn()
            .query_row(
                "SELECT status, payload_json FROM ir_score_jobs WHERE id = ?1",
                [score_job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(score_status, "succeeded");
        assert!(score_payload.is_empty());

        let replay_jobs = db.pending_ir_score_jobs(200, 10).unwrap();
        assert_eq!(replay_jobs.len(), 1);
        assert_eq!(replay_jobs[0].kind, IrJobKind::Replay);
        assert_eq!(replay_jobs[0].payload_json, replay_job.payload_json);

        let submissions: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM ir_score_submissions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(submissions, 1);
    }

    #[test]
    fn ir_score_job_failures_back_off_progressively() {
        let mut db = open_network_db();
        let job_id = db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: 42,
                chart_sha256: [7; 32],
                ln_policy: LnScorePolicy::ForceLn,
                payload_json: "{}".to_string(),
                now: 0,
            })
            .unwrap();

        // docs/ir.md: 1分 → 5分 → 30分 → 2時間 → 24時間 (以降は 24時間維持)。
        let expected_delays = [60, 300, 1800, 7200, 86_400, 86_400];
        for (attempt, delay) in expected_delays.into_iter().enumerate() {
            let now = (attempt as i64 + 1) * 1_000_000;
            db.mark_ir_score_job_status(job_id, IrScoreJobStatus::Failed, now, "boom").unwrap();
            let (attempt_count, next_attempt_at): (u32, i64) = db
                .conn()
                .query_row(
                    "SELECT attempt_count, next_attempt_at FROM ir_score_jobs WHERE id = ?1",
                    [job_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(attempt_count, attempt as u32 + 1);
            assert_eq!(next_attempt_at, now + delay, "attempt {attempt}");
        }
    }

    #[test]
    fn ir_score_job_failure_honors_retry_after() {
        let mut db = open_network_db();
        let job_id = enqueue_test_job(&mut db, 42, 100);

        db.mark_ir_score_job_failed(job_id, 200, "rate limited", Some(777)).unwrap();

        let (status, attempt_count, next_attempt_at): (String, u32, i64) = db
            .conn()
            .query_row(
                "SELECT status, attempt_count, next_attempt_at
                 FROM ir_score_jobs
                 WHERE id = ?1",
                [job_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(attempt_count, 1);
        assert_eq!(next_attempt_at, 977);
    }

    #[test]
    fn claiming_ir_jobs_marks_the_whole_batch_sending() {
        let mut db = open_network_db();
        let first = enqueue_test_job(&mut db, 1, 100);
        let second = enqueue_test_job(&mut db, 2, 100);

        let claimed = db.claim_pending_ir_score_jobs(100, 20, false).unwrap();
        assert_eq!(claimed.iter().map(|job| job.id).collect::<Vec<_>>(), vec![first, second]);
        assert!(db.claim_pending_ir_score_jobs(100, 20, false).unwrap().is_empty());

        let statuses = db
            .conn()
            .prepare("SELECT status FROM ir_score_jobs ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(statuses, vec!["sending", "sending"]);
    }

    #[test]
    fn claiming_ir_jobs_for_kind_keeps_other_jobs_pending() {
        let mut db = open_network_db();
        let score = enqueue_test_job(&mut db, 1, 100);
        let attestation = db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Attestation,
                local_score_id: 2,
                chart_sha256: [0; 32],
                ln_policy: LnScorePolicy::AutoLn,
                payload_json: r#"{"remote_score_id":"remote-2"}"#.to_string(),
                now: 100,
            })
            .unwrap();
        db.enqueue_ir_score_job(&NewIrScoreJob {
            provider: "bmz-official".to_string(),
            account_id: "account-2".to_string(),
            kind: IrJobKind::Attestation,
            local_score_id: 3,
            chart_sha256: [0; 32],
            ln_policy: LnScorePolicy::AutoLn,
            payload_json: r#"{"remote_score_id":"remote-3"}"#.to_string(),
            now: 100,
        })
        .unwrap();

        let pending = db
            .pending_ir_score_jobs_for_kind(
                "bmz-official",
                "account-1",
                IrJobKind::Attestation,
                100,
                10,
                true,
            )
            .unwrap();
        assert_eq!(pending.iter().map(|job| job.id).collect::<Vec<_>>(), vec![attestation]);

        let claimed = db
            .claim_pending_ir_score_jobs_for_kind(
                "bmz-official",
                "account-1",
                IrJobKind::Attestation,
                100,
                10,
                true,
            )
            .unwrap();
        assert_eq!(claimed.iter().map(|job| job.id).collect::<Vec<_>>(), vec![attestation]);

        let score_status: String = db
            .conn()
            .query_row("SELECT status FROM ir_score_jobs WHERE id = ?1", [score], |row| row.get(0))
            .unwrap();
        assert_eq!(score_status, "pending");
    }

    #[test]
    fn existing_ir_job_is_detected_before_backfill_reenqueue() {
        let mut db = open_network_db();
        enqueue_test_job(&mut db, 42, 100);

        assert!(db.has_ir_score_job("bmz-official", "account-1", IrJobKind::Score, 42).unwrap());
        assert!(!db.has_ir_score_job("bmz-official", "account-2", IrJobKind::Score, 42).unwrap());
        assert_eq!(
            db.unfinished_ir_score_job_count_for_kind(
                "bmz-official",
                "account-1",
                IrJobKind::Score,
            )
            .unwrap(),
            1
        );

        let job_id = db
            .conn()
            .query_row("SELECT id FROM ir_score_jobs WHERE local_score_id = 42", [], |row| {
                row.get(0)
            })
            .unwrap();
        db.mark_ir_score_job_status(job_id, IrScoreJobStatus::Succeeded, 200, "").unwrap();
        assert_eq!(
            db.unfinished_ir_score_job_count_for_kind(
                "bmz-official",
                "account-1",
                IrJobKind::Score,
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn submitted_scores_enqueue_one_attestation_job() {
        let mut db = open_network_db();
        let score_job_id = enqueue_test_job(&mut db, 42, 100);
        db.insert_ir_score_submission(&NewIrScoreSubmission {
            job_id: score_job_id,
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            remote_score_id: "remote-42".to_string(),
            status: "succeeded".to_string(),
            submitted_at: 200,
            log_path: "ir-submissions.jsonl".to_string(),
            error: String::new(),
        })
        .unwrap();

        assert_eq!(
            db.enqueue_ir_score_attestation_jobs("bmz-official", "account-1", 300).unwrap(),
            1
        );
        assert_eq!(
            db.enqueue_ir_score_attestation_jobs("bmz-official", "account-1", 301).unwrap(),
            0
        );

        let jobs = db.pending_ir_score_jobs(300, 10).unwrap();
        let job = jobs.iter().find(|job| job.kind == IrJobKind::Attestation).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap();
        assert_eq!(job.local_score_id, 42);
        assert_eq!(payload["remote_score_id"], "remote-42");
    }

    #[test]
    fn manual_ir_sync_can_ignore_retry_backoff() {
        let mut db = open_network_db();
        let job_id = db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: 42,
                chart_sha256: [7; 32],
                ln_policy: LnScorePolicy::ForceLn,
                payload_json: "{}".to_string(),
                now: 100,
            })
            .unwrap();

        db.mark_ir_score_job_status(job_id, IrScoreJobStatus::Failed, 200, "boom").unwrap();

        assert!(db.pending_ir_score_jobs(201, 10).unwrap().is_empty());
        let pending = db.pending_ir_score_jobs_ignoring_backoff(201, 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, job_id);
        assert_eq!(pending[0].status, "failed");
    }

    #[test]
    fn stale_sending_ir_score_jobs_are_retried() {
        let mut db = open_network_db();
        let job_id = db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: 42,
                chart_sha256: [7; 32],
                ln_policy: LnScorePolicy::ForceLn,
                payload_json: "{}".to_string(),
                now: 100,
            })
            .unwrap();

        db.mark_ir_score_job_status(job_id, IrScoreJobStatus::Sending, 200, "").unwrap();

        assert!(db.pending_ir_score_jobs(499, 10).unwrap().is_empty());
        let pending = db.pending_ir_score_jobs(500, 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, job_id);
        assert_eq!(pending[0].status, "sending");
    }

    #[test]
    fn prune_succeeded_ir_score_jobs_keeps_recent_and_unfinished_jobs() {
        let mut db = open_network_db();
        let stale_a = enqueue_test_job(&mut db, 1, 0);
        let stale_b = enqueue_test_job(&mut db, 2, 0);
        let retained_by_count = enqueue_test_job(&mut db, 3, 0);
        let retained_by_age = enqueue_test_job(&mut db, 4, 9_500);
        let failed = enqueue_test_job(&mut db, 5, 0);

        db.mark_ir_score_job_status(stale_a, IrScoreJobStatus::Succeeded, 100, "").unwrap();
        db.mark_ir_score_job_status(stale_b, IrScoreJobStatus::Succeeded, 200, "").unwrap();
        db.mark_ir_score_job_status(retained_by_count, IrScoreJobStatus::Succeeded, 300, "")
            .unwrap();
        db.mark_ir_score_job_status(retained_by_age, IrScoreJobStatus::Succeeded, 9_500, "")
            .unwrap();
        db.mark_ir_score_job_status(failed, IrScoreJobStatus::Failed, 100, "boom").unwrap();

        db.insert_ir_score_submission(&NewIrScoreSubmission {
            job_id: stale_a,
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: 1,
            remote_score_id: "remote-a".to_string(),
            status: "succeeded".to_string(),
            submitted_at: 100,
            log_path: "ir-submissions.jsonl".to_string(),
            error: String::new(),
        })
        .unwrap();

        let deleted = db.prune_succeeded_ir_score_jobs_with_policy(10_000, 1_000, 2).unwrap();
        assert_eq!(deleted, 2);

        let remaining: Vec<i64> = db
            .conn()
            .prepare("SELECT id FROM ir_score_jobs ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(remaining, vec![retained_by_count, retained_by_age, failed]);

        let stale_submission_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM ir_score_submissions WHERE job_id = ?1",
                [stale_a],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stale_submission_count, 0);
    }
}
