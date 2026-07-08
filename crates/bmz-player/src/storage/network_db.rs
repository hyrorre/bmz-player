use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, params};

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

/// IR ジョブの種別。単曲スコアかコーススコアか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrJobKind {
    Score,
    Course,
}

impl IrJobKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Score => "score",
            Self::Course => "course",
        }
    }

    pub fn from_str_or_score(value: &str) -> Self {
        if value == "course" { Self::Course } else { Self::Score }
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
