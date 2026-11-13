use super::*;

impl NetworkDatabase {
    pub(crate) fn finish_bokutachi_dan(
        &self,
        job_id: i64,
        now: i64,
        status: IrScoreJobStatus,
        message: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE ir_score_jobs SET status = ?2, updated_at = ?3,
            next_attempt_at = ?4, last_error = ?5 WHERE id = ?1",
            params![job_id, status.as_str(), now, IR_JOB_NO_AUTO_RETRY_AT, message],
        )?;
        Ok(())
    }
    pub(crate) fn reject_bokutachi_chart(
        &self,
        job_id: i64,
        now: i64,
        message: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE ir_score_jobs SET status = 'ineligible', updated_at = ?2,
            next_attempt_at = ?3, last_error = ?4, attempt_count = attempt_count + 1 WHERE id = ?1",
            params![job_id, now, IR_JOB_NO_AUTO_RETRY_AT, message],
        )?;
        Ok(())
    }

    pub(crate) fn bokutachi_progress(&self, job_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT progress_json FROM bokutachi_imports WHERE job_id = ?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub(crate) fn save_bokutachi_progress(&self, job_id: i64, progress: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO bokutachi_imports (job_id, progress_json) VALUES (?1, ?2)
            ON CONFLICT(job_id) DO UPDATE SET progress_json = excluded.progress_json",
            params![job_id, progress],
        )?;
        Ok(())
    }

    pub(crate) fn defer_bokutachi_job(
        &self,
        job_id: i64,
        now: i64,
        next: i64,
        message: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE ir_score_jobs SET status = 'pending', updated_at = ?2,
            next_attempt_at = ?3, last_error = ?4 WHERE id = ?1",
            params![job_id, now, next, message],
        )?;
        Ok(())
    }

    /// Atomic across workers. Ordinary posts are spaced to stay within 5/min/IP;
    /// 429 extends the origin-wide reservation instead of sleeping a worker.
    pub(crate) fn reserve_bokutachi_post(&mut self, origin: &str, now: i64) -> Result<Option<i64>> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let next: Option<i64> = tx
            .query_row(
                "SELECT next_post_at FROM bokutachi_rate_limits WHERE origin = ?1",
                [origin],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(next) = next.filter(|next| *next > now) {
            return Ok(Some(next));
        }
        tx.execute(
            "INSERT INTO bokutachi_rate_limits (origin, next_post_at) VALUES (?1, ?2)
            ON CONFLICT(origin) DO UPDATE SET next_post_at = excluded.next_post_at",
            params![origin, now.saturating_add(13)],
        )?;
        tx.commit()?;
        Ok(None)
    }

    pub(crate) fn bokutachi_origin_wait(&self, origin: &str, now: i64) -> Result<Option<i64>> {
        let next: Option<i64> = self
            .conn
            .query_row(
                "SELECT next_post_at FROM bokutachi_rate_limits WHERE origin = ?1",
                [origin],
                |row| row.get(0),
            )
            .optional()?;
        Ok(next.filter(|next| *next > now))
    }

    pub(crate) fn defer_bokutachi_origin(&self, origin: &str, next: i64) -> Result<()> {
        self.conn.execute("INSERT INTO bokutachi_rate_limits (origin, next_post_at) VALUES (?1, ?2)
            ON CONFLICT(origin) DO UPDATE SET next_post_at = MAX(next_post_at, excluded.next_post_at)", params![origin, next])?;
        Ok(())
    }

    pub(crate) fn finish_bokutachi_without_score(
        &self,
        job_id: i64,
        now: i64,
        message: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE ir_score_jobs SET status = 'no_change', updated_at = ?2,
            next_attempt_at = ?3, last_error = ?4 WHERE id = ?1",
            params![job_id, now, IR_JOB_NO_AUTO_RETRY_AT, message],
        )?;
        Ok(())
    }
}
