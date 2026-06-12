use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE;
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::ScoreState;
use bmz_render::snapshot::{DisplayJudgeCounts, FastSlowJudgeCounts};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rusqlite::{Connection, OptionalExtension, params};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};
use crate::config::profile_config::ReplaySlotRule;
use crate::ln_policy::LnScorePolicy;
use crate::select_options::DoubleOptionScoreBucket;

pub struct ScoreDatabase {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInfo {
    pub player_uuid: String,
    pub display_name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStats {
    pub play_count: u64,
    pub clear_count: u64,
    pub max_combo: u32,
    pub fast_pgreat: u64,
    pub slow_pgreat: u64,
    pub fast_great: u64,
    pub slow_great: u64,
    pub fast_good: u64,
    pub slow_good: u64,
    pub fast_bad: u64,
    pub slow_bad: u64,
    pub fast_poor: u64,
    pub slow_poor: u64,
    pub fast_empty_poor: u64,
    pub slow_empty_poor: u64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScoreKey {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
}

impl ScoreKey {
    pub const fn new(chart_sha256: [u8; 32], ln_policy: LnScorePolicy) -> Self {
        Self { chart_sha256, ln_policy, double_option: DoubleOptionScoreBucket::Off }
    }

    pub const fn with_double_option(
        chart_sha256: [u8; 32],
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
    ) -> Self {
        Self { chart_sha256, ln_policy, double_option }
    }
}

#[derive(Debug, Clone)]
pub struct ScoreRecord {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub played_at: i64,
    pub clear_type: ClearType,
    pub gauge_type: Option<GaugeType>,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub score: ScoreState,
    pub random_seed: Option<i64>,
    pub arrange: String,
    pub gauge_option: String,
    pub rule_mode: String,
    pub assist_mask: u32,
    pub autoplay: bool,
    pub device_type: InputDeviceKind,
    pub replay_path: String,
}

#[derive(Debug, Clone)]
pub struct ScoreRecordMetadata {
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub played_at: i64,
    pub random_seed: Option<i64>,
    pub arrange: String,
    pub gauge_option: String,
    pub rule_mode: String,
    pub assist_mask: u32,
    pub device_type: InputDeviceKind,
    pub replay_path: String,
}

impl ScoreRecord {
    pub fn from_play_result(result: &PlayResult, metadata: ScoreRecordMetadata) -> Self {
        let ScoreRecordMetadata {
            ln_policy,
            double_option,
            played_at,
            random_seed,
            arrange,
            gauge_option,
            rule_mode,
            assist_mask,
            device_type,
            replay_path,
        } = metadata;

        Self {
            chart_sha256: result.chart_sha256,
            ln_policy,
            double_option,
            played_at,
            clear_type: result.clear_type,
            gauge_type: Some(result.gauge_type),
            gauge_value: result.gauge_value,
            total_notes: result.total_notes,
            score: result.score.clone(),
            random_seed,
            arrange,
            gauge_option,
            rule_mode,
            assist_mask,
            autoplay: result.autoplay,
            device_type,
            replay_path,
        }
    }
}

impl ScoreRecordMetadata {
    pub fn new(
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        played_at: i64,
        random_seed: Option<i64>,
        arrange: impl Into<String>,
        gauge_option: impl Into<String>,
        rule_mode: impl Into<String>,
        assist_mask: u32,
        device_type: InputDeviceKind,
        replay_path: impl Into<String>,
    ) -> Self {
        Self {
            ln_policy,
            double_option,
            played_at,
            random_seed,
            arrange: arrange.into(),
            gauge_option: gauge_option.into(),
            rule_mode: rule_mode.into(),
            assist_mask,
            device_type,
            replay_path: replay_path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BestScoreSummary {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    pub play_count: u32,
    pub clear_count: u32,
    pub device_type: InputDeviceKind,
    pub played_at: i64,
    pub replay_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScoreBestRank {
    ex_score: u32,
    clear_rank: u8,
    bp: u32,
    cb: u32,
    max_combo: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousBestSnapshot {
    pub clear_type: String,
    pub ex_score: u32,
    pub max_combo: u32,
    pub bp: u32,
    pub cb: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaySlotSummary {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub replay_slots: [bool; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaySlotRecord {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub slot: u8,
    pub rule: ReplaySlotRule,
    pub replay_path: String,
    pub played_at: i64,
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub clear_rank: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreHistoryEntry {
    pub id: i64,
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub played_at: i64,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub autoplay: bool,
    pub device_type: InputDeviceKind,
    pub replay_path: String,
    /// `library.db`'s `course_scores.id` if this chart play happened as part
    /// of a course attempt, otherwise `None`.  No cross-database FK is
    /// enforced — callers can join against `library.db.course_scores` if
    /// they need the attempt details.
    pub course_score_id: Option<i64>,
    pub previous_best: Option<PreviousBestSnapshot>,
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
    pub local_score_id: i64,
    pub remote_score_id: String,
    pub status: String,
    pub submitted_at: i64,
    pub response_json: String,
    pub error: String,
}

impl ScoreDatabase {
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
        let tx = self.conn.transaction()?;
        let previous_best = previous_best_snapshot(
            &tx,
            ScoreKey::with_double_option(
                record.chart_sha256,
                record.ln_policy,
                record.double_option,
            ),
        )?;
        insert_score_history(&tx, record, previous_best.as_ref())?;
        let history_id = tx.last_insert_rowid();
        upsert_score_best(&tx, record)?;
        update_player_stats(&tx, record)?;
        tx.commit()?;
        Ok(history_id)
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

    pub fn best_ex_score(&self, key: ScoreKey) -> Result<Option<u32>> {
        self.conn
            .query_row(
                "SELECT ex_score FROM score_best
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
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
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
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
        let mut out = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT
                chart_sha256,
                ln_policy,
                double_option,
                clear_type,
                gauge_type,
                gauge_value,
                ex_score,
                bp,
                cb,
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
                play_count,
                clear_count,
                device_type,
                played_at,
                replay_path
            FROM score_best
            WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3",
        )?;

        for key in keys {
            if let Some(summary) = stmt
                .query_row(
                    params![
                        hash_to_hex(&key.chart_sha256),
                        key.ln_policy.as_str(),
                        key.double_option.as_str(),
                    ],
                    best_score_summary_from_row,
                )
                .optional()?
            {
                out.push(summary);
            }
        }

        Ok(out)
    }

    pub fn replay_slots_for_charts(&self, keys: &[ScoreKey]) -> Result<Vec<ReplaySlotSummary>> {
        let mut out = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT slot FROM replay_slots
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3",
        )?;

        for key in keys {
            let slots: Vec<u8> = stmt
                .query_map(
                    params![
                        hash_to_hex(&key.chart_sha256),
                        key.ln_policy.as_str(),
                        key.double_option.as_str(),
                    ],
                    |row| row.get::<_, u8>(0),
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if slots.is_empty() {
                continue;
            }
            let mut replay_slots = [false; 4];
            for slot in slots {
                if (slot as usize) < replay_slots.len() {
                    replay_slots[slot as usize] = true;
                }
            }
            out.push(ReplaySlotSummary {
                chart_sha256: key.chart_sha256,
                ln_policy: key.ln_policy,
                double_option: key.double_option,
                replay_slots,
            });
        }

        Ok(out)
    }

    pub fn replay_slot(&self, key: ScoreKey, slot: u8) -> Result<Option<ReplaySlotRecord>> {
        self.conn
            .query_row(
                "SELECT chart_sha256, ln_policy, double_option, slot, rule, replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank
                 FROM replay_slots
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3 AND slot = ?4",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    slot,
                ],
                replay_slot_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replay_slots_for_chart(&self, key: ScoreKey) -> Result<[Option<ReplaySlotRecord>; 4]> {
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256, ln_policy, double_option, slot, rule, replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank
             FROM replay_slots
             WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3",
        )?;
        let rows = stmt
            .query_map(
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
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
        if record.slot > 3 {
            bail!("replay slot must be in 0..=3 (got {})", record.slot);
        }
        self.conn.execute(
            "INSERT INTO replay_slots (
                chart_sha256, ln_policy, double_option, slot, rule, replay_path, played_at,
                ex_score, bp, cb, max_combo, clear_rank
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(chart_sha256, ln_policy, double_option, slot) DO UPDATE SET
                rule = excluded.rule,
                replay_path = excluded.replay_path,
                played_at = excluded.played_at,
                ex_score = excluded.ex_score,
                bp = excluded.bp,
                cb = excluded.cb,
                max_combo = excluded.max_combo,
                clear_rank = excluded.clear_rank",
            params![
                hash_to_hex(&record.chart_sha256),
                record.ln_policy.as_str(),
                record.double_option.as_str(),
                record.slot,
                record.rule.as_str(),
                record.replay_path,
                record.played_at,
                record.ex_score,
                record.bp,
                record.cb,
                record.max_combo,
                record.clear_rank,
            ],
        )?;
        Ok(())
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
        const SENDING_STALE_AFTER_SECONDS: i64 = 300;
        let mut stmt = self.conn.prepare(
            "SELECT id, provider, account_id, local_score_id, chart_sha256, ln_policy,
                payload_json, status, attempt_count, next_attempt_at, last_error,
                created_at, updated_at, kind
             FROM ir_score_jobs
             WHERE (status IN ('pending', 'failed') AND next_attempt_at <= ?1)
                OR (status = 'sending' AND updated_at <= ?1 - ?3)
             ORDER BY next_attempt_at ASC, id ASC
             LIMIT ?2",
        )?;
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
                job_id, provider, account_id, local_score_id, remote_score_id,
                status, submitted_at, response_json, error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.job_id,
                record.provider,
                record.account_id,
                record.local_score_id,
                record.remote_score_id,
                record.status,
                record.submitted_at,
                record.response_json,
                record.error,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Tag the given `score_history` rows with a course attempt id.
    ///
    /// `course_score_id` references `library.db`'s `course_scores.id`.  No FK
    /// is enforced because the two databases are separate; the caller is
    /// responsible for passing a real id.
    pub fn tag_score_history_with_course(
        &mut self,
        score_history_ids: &[i64],
        course_score_id: i64,
    ) -> Result<usize> {
        if score_history_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut total = 0_usize;
        {
            let mut stmt =
                tx.prepare("UPDATE score_history SET course_score_id = ?1 WHERE id = ?2")?;
            for id in score_history_ids {
                total += stmt.execute(params![course_score_id, id])?;
            }
        }
        tx.commit()?;
        Ok(total)
    }

    /// IR replay upload 用に、score_history 行の replay_path を引く。
    /// 行が無い / 空文字なら None。
    pub fn replay_path_for_history(&self, score_history_id: i64) -> Result<Option<String>> {
        let path: Option<String> = self
            .conn
            .query_row(
                "SELECT replay_path FROM score_history WHERE id = ?1",
                params![score_history_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(path.filter(|path| !path.is_empty()))
    }

    pub fn recent_history(&self, limit: u32, offset: u32) -> Result<Vec<ScoreHistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                chart_sha256,
                played_at,
                clear_type,
                gauge_type,
                gauge_value,
                total_notes,
                ex_score,
                bp,
                cb,
                max_combo,
                autoplay,
                replay_path,
                course_score_id,
                ln_policy,
                old_clear_type,
                old_ex_score,
                old_max_combo,
                old_bp,
                old_cb,
                device_type
            FROM score_history
            ORDER BY played_at DESC, id DESC
            LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], score_history_entry_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn best_score_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BestScoreSummary> {
    let sha256_hex: String = row.get(0)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let ln_policy = ln_policy_from_row(row, 1)?;
    let double_option = double_option_from_row(row, 2)?;

    Ok(BestScoreSummary {
        chart_sha256,
        ln_policy,
        double_option,
        clear_type: row.get(3)?,
        gauge_type: row.get(4)?,
        gauge_value: row.get(5)?,
        ex_score: row.get(6)?,
        bp: row.get(7)?,
        cb: row.get(8)?,
        max_combo: row.get(9)?,
        judge_counts: DisplayJudgeCounts {
            pgreat: row.get::<_, u32>(10)? + row.get::<_, u32>(11)?,
            great: row.get::<_, u32>(12)? + row.get::<_, u32>(13)?,
            good: row.get::<_, u32>(14)? + row.get::<_, u32>(15)?,
            bad: row.get::<_, u32>(16)? + row.get::<_, u32>(17)?,
            poor: row.get::<_, u32>(18)? + row.get::<_, u32>(19)?,
            empty_poor: row.get::<_, u32>(20)? + row.get::<_, u32>(21)?,
        },
        fast_slow_counts: FastSlowJudgeCounts {
            fast_pgreat: row.get(10)?,
            slow_pgreat: row.get(11)?,
            fast_great: row.get(12)?,
            slow_great: row.get(13)?,
            fast_good: row.get(14)?,
            slow_good: row.get(15)?,
            fast_bad: row.get(16)?,
            slow_bad: row.get(17)?,
            fast_poor: row.get(18)?,
            slow_poor: row.get(19)?,
            fast_empty_poor: row.get(20)?,
            slow_empty_poor: row.get(21)?,
        },
        play_count: row.get(22)?,
        clear_count: row.get(23)?,
        device_type: device_type_from_row(row, 24)?,
        played_at: row.get(25)?,
        replay_path: row.get(26)?,
    })
}

fn replay_slot_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReplaySlotRecord> {
    let sha256_hex: String = row.get(0)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let ln_policy = ln_policy_from_row(row, 1)?;
    let double_option = double_option_from_row(row, 2)?;
    let rule_str: String = row.get(4)?;
    let rule = ReplaySlotRule::from_str_opt(&rule_str).unwrap_or(ReplaySlotRule::Always);

    Ok(ReplaySlotRecord {
        chart_sha256,
        ln_policy,
        double_option,
        slot: row.get(3)?,
        rule,
        replay_path: row.get(5)?,
        played_at: row.get(6)?,
        ex_score: row.get(7)?,
        bp: row.get(8)?,
        cb: row.get(9)?,
        max_combo: row.get(10)?,
        clear_rank: row.get(11)?,
    })
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

fn score_history_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScoreHistoryEntry> {
    let sha256_hex: String = row.get(1)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let old_clear_type: Option<String> = row.get(15)?;
    let previous_best = if let Some(clear_type) = old_clear_type {
        Some(PreviousBestSnapshot {
            clear_type,
            ex_score: row.get(16)?,
            max_combo: row.get(17)?,
            bp: row.get(18)?,
            cb: row.get(19)?,
        })
    } else {
        None
    };

    Ok(ScoreHistoryEntry {
        id: row.get(0)?,
        chart_sha256,
        ln_policy: ln_policy_from_row(row, 14)?,
        played_at: row.get(2)?,
        clear_type: row.get(3)?,
        gauge_type: row.get(4)?,
        gauge_value: row.get(5)?,
        total_notes: row.get(6)?,
        ex_score: row.get(7)?,
        bp: row.get(8)?,
        cb: row.get(9)?,
        max_combo: row.get(10)?,
        autoplay: row.get(11)?,
        replay_path: row.get(12)?,
        course_score_id: row.get(13)?,
        device_type: device_type_from_row(row, 20)?,
        previous_best,
    })
}

fn player_stats_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlayerStats> {
    Ok(PlayerStats {
        play_count: row.get(0)?,
        clear_count: row.get(1)?,
        max_combo: row.get(2)?,
        fast_pgreat: row.get(3)?,
        slow_pgreat: row.get(4)?,
        fast_great: row.get(5)?,
        slow_great: row.get(6)?,
        fast_good: row.get(7)?,
        slow_good: row.get(8)?,
        fast_bad: row.get(9)?,
        slow_bad: row.get(10)?,
        fast_poor: row.get(11)?,
        slow_poor: row.get(12)?,
        fast_empty_poor: row.get(13)?,
        slow_empty_poor: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn device_type_from_row(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<InputDeviceKind> {
    let value: String = row.get(index)?;
    match value.as_str() {
        "keyboard" => Ok(InputDeviceKind::Keyboard),
        "controller" => Ok(InputDeviceKind::Controller),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid input device type: {value}").into(),
        )),
    }
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

fn double_option_from_row(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<DoubleOptionScoreBucket> {
    let value: String = row.get(index)?;
    Ok(DoubleOptionScoreBucket::from_str_or_off(&value))
}

fn previous_best_snapshot(
    conn: &Connection,
    key: ScoreKey,
) -> Result<Option<PreviousBestSnapshot>> {
    conn.query_row(
        "SELECT clear_type, ex_score, max_combo, bp, cb
         FROM score_best
         WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3",
        params![
            hash_to_hex(&key.chart_sha256),
            key.ln_policy.as_str(),
            key.double_option.as_str(),
        ],
        |row| {
            Ok(PreviousBestSnapshot {
                clear_type: row.get(0)?,
                ex_score: row.get(1)?,
                max_combo: row.get(2)?,
                bp: row.get(3)?,
                cb: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn insert_score_history(
    conn: &Connection,
    record: &ScoreRecord,
    previous_best: Option<&PreviousBestSnapshot>,
) -> Result<()> {
    let judges = &record.score.judges;
    let ghost = encode_beatoraja_ghost(&record.score.ghost)?;
    let bp = score_record_bp(record);
    let cb = score_record_cb(record);
    conn.execute(
        "INSERT INTO score_history (
            chart_sha256,
            ln_policy,
            double_option,
            played_at,
            clear_type,
            gauge_type,
            gauge_value,
            total_notes,
            ex_score,
            bp,
            cb,
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
            random_seed,
            arrange,
            gauge_option,
            rule_mode,
            assist_mask,
            autoplay,
            device_type,
            replay_path,
            ghost,
            old_clear_type,
            old_ex_score,
            old_max_combo,
            old_bp,
            old_cb
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
            ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38
        )",
        params![
            hash_to_hex(&record.chart_sha256),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
            record.played_at,
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.total_notes,
            record.score.ex_score(),
            bp,
            cb,
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.random_seed,
            record.arrange.as_str(),
            record.gauge_option.as_str(),
            record.rule_mode.as_str(),
            record.assist_mask,
            record.autoplay,
            record.device_type.as_str(),
            record.replay_path.as_str(),
            ghost,
            previous_best.map(|best| best.clear_type.as_str()),
            previous_best.map(|best| best.ex_score),
            previous_best.map(|best| best.max_combo),
            previous_best.map(|best| best.bp),
            previous_best.map(|best| best.cb),
        ],
    )?;
    Ok(())
}

fn update_player_stats(conn: &Connection, record: &ScoreRecord) -> Result<()> {
    let judges = &record.score.judges;
    let clear_increment = u32::from(is_counted_clear(record.clear_type));
    conn.execute(
        "INSERT INTO player_stats (
            id,
            play_count,
            clear_count,
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
        ) VALUES (
            1, 1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
        )
        ON CONFLICT(id) DO UPDATE SET
            play_count = play_count + 1,
            clear_count = clear_count + excluded.clear_count,
            max_combo = max(max_combo, excluded.max_combo),
            fast_pgreat = fast_pgreat + excluded.fast_pgreat,
            slow_pgreat = slow_pgreat + excluded.slow_pgreat,
            fast_great = fast_great + excluded.fast_great,
            slow_great = slow_great + excluded.slow_great,
            fast_good = fast_good + excluded.fast_good,
            slow_good = slow_good + excluded.slow_good,
            fast_bad = fast_bad + excluded.fast_bad,
            slow_bad = slow_bad + excluded.slow_bad,
            fast_poor = fast_poor + excluded.fast_poor,
            slow_poor = slow_poor + excluded.slow_poor,
            fast_empty_poor = fast_empty_poor + excluded.fast_empty_poor,
            slow_empty_poor = slow_empty_poor + excluded.slow_empty_poor,
            updated_at = max(updated_at, excluded.updated_at)",
        params![
            clear_increment,
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.played_at,
        ],
    )?;
    Ok(())
}

fn upsert_score_best(conn: &Connection, record: &ScoreRecord) -> Result<()> {
    let judges = &record.score.judges;
    let ghost = encode_beatoraja_ghost(&record.score.ghost)?;
    let clear_increment = u32::from(is_counted_clear(record.clear_type));
    let bp = score_record_bp(record);
    let cb = score_record_cb(record);
    let inserted = conn.execute(
        "INSERT INTO score_best (
            chart_sha256,
            ln_policy,
            double_option,
            clear_type,
            gauge_type,
            gauge_value,
            ex_score,
            bp,
            cb,
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
            played_at,
            replay_path,
            ghost,
            device_type,
            play_count,
            clear_count
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
        )
        ON CONFLICT(chart_sha256, ln_policy, double_option) DO NOTHING",
        params![
            hash_to_hex(&record.chart_sha256),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.score.ex_score(),
            bp,
            cb,
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.played_at,
            record.replay_path.as_str(),
            ghost,
            record.device_type.as_str(),
            1_u32,
            clear_increment,
        ],
    )?;
    if inserted > 0 {
        return Ok(());
    }

    let chart_sha256 = hash_to_hex(&record.chart_sha256);
    conn.execute(
        "UPDATE score_best
         SET play_count = play_count + 1,
             clear_count = clear_count + ?2
         WHERE chart_sha256 = ?1 AND ln_policy = ?3 AND double_option = ?4",
        params![
            chart_sha256,
            clear_increment,
            record.ln_policy.as_str(),
            record.double_option.as_str(),
        ],
    )?;

    let current = conn.query_row(
        "SELECT ex_score, clear_type, bp, cb, max_combo
         FROM score_best
         WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3",
        params![
            hash_to_hex(&record.chart_sha256),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
        ],
        |row| {
            let clear_type: String = row.get(1)?;
            Ok(ScoreBestRank {
                ex_score: row.get(0)?,
                clear_rank: clear_rank_from_name(&clear_type),
                bp: row.get(2)?,
                cb: row.get(3)?,
                max_combo: row.get(4)?,
            })
        },
    )?;
    if !score_best_should_update(record, current) {
        conn.execute(
            "UPDATE score_best SET
                bp = min(bp, ?2),
                cb = min(cb, ?3),
                max_combo = max(max_combo, ?4)
             WHERE chart_sha256 = ?1 AND ln_policy = ?5 AND double_option = ?6",
            params![
                hash_to_hex(&record.chart_sha256),
                bp,
                cb,
                record.score.max_combo,
                record.ln_policy.as_str(),
                record.double_option.as_str(),
            ],
        )?;
        return Ok(());
    }

    conn.execute(
        "UPDATE score_best SET
            clear_type = ?2,
            gauge_type = ?3,
            gauge_value = ?4,
            ex_score = ?5,
            bp = ?6,
            cb = ?7,
            max_combo = ?8,
            fast_pgreat = ?9,
            slow_pgreat = ?10,
            fast_great = ?11,
            slow_great = ?12,
            fast_good = ?13,
            slow_good = ?14,
            fast_bad = ?15,
            slow_bad = ?16,
            fast_poor = ?17,
            slow_poor = ?18,
            fast_empty_poor = ?19,
            slow_empty_poor = ?20,
            played_at = ?21,
            replay_path = ?22,
            ghost = ?23,
            device_type = ?24
         WHERE chart_sha256 = ?1 AND ln_policy = ?25 AND double_option = ?26",
        params![
            hash_to_hex(&record.chart_sha256),
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.score.ex_score(),
            bp,
            cb,
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.played_at,
            record.replay_path.as_str(),
            ghost,
            record.device_type.as_str(),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
        ],
    )?;
    Ok(())
}

fn gauge_type_str(gauge_type: Option<GaugeType>) -> &'static str {
    gauge_type.map(GaugeType::as_str).unwrap_or("")
}

fn is_counted_clear(clear_type: ClearType) -> bool {
    !matches!(clear_type, ClearType::NoPlay | ClearType::Failed)
}

fn score_best_should_update(record: &ScoreRecord, current: ScoreBestRank) -> bool {
    let next = ScoreBestRank {
        ex_score: record.score.ex_score(),
        clear_rank: record.clear_type as u8,
        bp: score_record_bp(record),
        cb: score_record_cb(record),
        max_combo: record.score.max_combo,
    };
    (
        next.ex_score,
        next.clear_rank,
        std::cmp::Reverse(next.bp),
        std::cmp::Reverse(next.cb),
        next.max_combo,
    ) > (
        current.ex_score,
        current.clear_rank,
        std::cmp::Reverse(current.bp),
        std::cmp::Reverse(current.cb),
        current.max_combo,
    )
}

fn score_record_bp(record: &ScoreRecord) -> u32 {
    if record.clear_type == ClearType::Failed {
        record.score.bp_with_unprocessed_notes(record.total_notes)
    } else {
        record.score.bp()
    }
}

fn score_record_cb(record: &ScoreRecord) -> u32 {
    if record.clear_type == ClearType::Failed {
        record.score.cb_with_unprocessed_notes(record.total_notes)
    } else {
        record.score.cb()
    }
}

fn clear_rank_from_name(value: &str) -> u8 {
    match value {
        "NoPlay" => ClearType::NoPlay as u8,
        "Failed" => ClearType::Failed as u8,
        "AssistEasy" => ClearType::AssistEasy as u8,
        "LightAssistEasy" => ClearType::LightAssistEasy as u8,
        "Easy" => ClearType::Easy as u8,
        "Normal" => ClearType::Normal as u8,
        "Hard" => ClearType::Hard as u8,
        "ExHard" => ClearType::ExHard as u8,
        "FullCombo" => ClearType::FullCombo as u8,
        "Perfect" => ClearType::Perfect as u8,
        "Max" => ClearType::Max as u8,
        _ => ClearType::NoPlay as u8,
    }
}

pub fn encode_beatoraja_ghost(ghost: &[u8]) -> Result<String> {
    if ghost.is_empty() {
        return Ok(String::new());
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(ghost)?;
    Ok(URL_SAFE.encode(encoder.finish()?))
}

pub fn decode_beatoraja_ghost(encoded: &str, total_notes: u32) -> Result<Vec<u8>> {
    let expected_len = total_notes as usize;
    if encoded.is_empty() {
        return Ok(vec![4; expected_len]);
    }

    let compressed = URL_SAFE.decode(encoded)?;
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut decoded = Vec::with_capacity(expected_len);
    decoder.read_to_end(&mut decoded)?;
    if decoded.len() < expected_len {
        decoded.resize(expected_len, 4);
    } else if decoded.len() > expected_len {
        decoded.truncate(expected_len);
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::ids::NoteId;
    use bmz_core::input::InputDeviceKind;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::judge::model::JudgementEvent;
    use bmz_gameplay::result::PlayResult;
    use bmz_gameplay::score::ScoreState;

    use super::*;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

    fn score_with_ex_score(ex_score: u32) -> ScoreState {
        let mut score = ScoreState::default();
        for index in 0..(ex_score / 2) {
            score.apply(&JudgementEvent {
                note_id: Some(NoteId(index)),
                lane: Lane::Key1,
                judge: Judge::PGreat,
                side: TimingSide::Slow,
                delta: TimeUs(0),
                time: TimeUs(index as i64),
            });
        }
        score
    }

    fn record(ex_score: u32, clear_type: ClearType) -> ScoreRecord {
        ScoreRecord {
            chart_sha256: [7; 32],
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            played_at: 1_700_000_000,
            clear_type,
            gauge_type: Some(GaugeType::Normal),
            gauge_value: 82.0,
            total_notes: ex_score / 2,
            score: score_with_ex_score(ex_score),
            random_seed: None,
            arrange: "Normal".to_string(),
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            autoplay: false,
            device_type: InputDeviceKind::Keyboard,
            replay_path: String::new(),
        }
    }

    fn key(sha: [u8; 32]) -> ScoreKey {
        ScoreKey::new(sha, LnScorePolicy::ForceLn)
    }

    #[test]
    fn insert_score_persists_enum_strings_and_empty_values() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut record = record(20, ClearType::Normal);
        record.gauge_type = None;
        record.rule_mode = "Dx".to_string();
        record.arrange = "Random".to_string();
        record.device_type = InputDeviceKind::Controller;
        db.insert_score(&record).unwrap();

        let (clear_type, gauge_type, gauge_option, rule_mode, arrange, device_type, replay_path): (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ) = db
            .conn()
            .query_row(
                "SELECT clear_type, gauge_type, gauge_option, rule_mode, arrange, device_type, replay_path FROM score_history",
                [],
                |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
                },
            )
            .unwrap();

        assert_eq!(clear_type, "Normal");
        assert_eq!(gauge_type, "");
        assert_eq!(gauge_option, "");
        assert_eq!(rule_mode, "Dx");
        assert_eq!(arrange, "Random");
        assert_eq!(device_type, "controller");
        assert_eq!(replay_path, "");
    }

    #[test]
    fn best_score_keeps_higher_ex_score() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        db.insert_score(&record(10, ClearType::Hard)).unwrap();
        db.insert_score(&record(30, ClearType::Easy)).unwrap();

        assert_eq!(db.best_ex_score(key([7; 32])).unwrap(), Some(30));
    }

    #[test]
    fn score_best_is_separate_per_double_option() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        let mut battle = record(60, ClearType::Hard);
        battle.double_option = DoubleOptionScoreBucket::Battle;
        db.insert_score(&battle).unwrap();

        let off_key = key([7; 32]);
        let battle_key = ScoreKey::with_double_option(
            [7; 32],
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Battle,
        );

        assert_eq!(db.best_ex_score(off_key).unwrap(), Some(20));
        assert_eq!(db.best_ex_score(battle_key).unwrap(), Some(60));

        let summaries = db.best_scores_for_charts(&[off_key, battle_key]).unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].double_option, DoubleOptionScoreBucket::Off);
        assert_eq!(summaries[0].ex_score, 20);
        assert_eq!(summaries[1].double_option, DoubleOptionScoreBucket::Battle);
        assert_eq!(summaries[1].ex_score, 60);
    }

    #[test]
    fn best_score_tiebreaks_by_lower_bp_then_lower_cb() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut high_bp = record(20, ClearType::Normal);
        high_bp.score.judges.fast_bad = 3;
        high_bp.score.judges.fast_empty_poor = 2;
        db.insert_score(&high_bp).unwrap();

        let mut lower_bp = record(20, ClearType::Normal);
        lower_bp.score.judges.fast_bad = 2;
        lower_bp.score.judges.fast_empty_poor = 2;
        db.insert_score(&lower_bp).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.bp, 4);
        assert_eq!(best.cb, 2);

        let mut higher_cb = record(20, ClearType::Normal);
        higher_cb.score.judges.fast_bad = 4;
        higher_cb.score.judges.fast_empty_poor = 1;
        db.insert_score(&higher_cb).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.bp, 4);
        assert_eq!(best.cb, 2);

        let mut lower_cb = record(20, ClearType::Normal);
        lower_cb.score.judges.fast_great = 6;
        lower_cb.score.judges.slow_good = 5;
        lower_cb.score.judges.fast_bad = 1;
        lower_cb.score.judges.fast_empty_poor = 3;
        db.insert_score(&lower_cb).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.bp, 4);
        assert_eq!(best.cb, 1);
        assert_eq!(best.judge_counts.pgreat, 10);
        assert_eq!(best.judge_counts.great, 6);
        assert_eq!(best.judge_counts.good, 5);
        assert_eq!(best.judge_counts.bad, 1);
        assert_eq!(best.judge_counts.empty_poor, 3);
        assert_eq!(best.fast_slow_counts.fast_great, 6);
        assert_eq!(best.fast_slow_counts.slow_good, 5);
        assert_eq!(best.fast_slow_counts.fast_empty_poor, 3);
        assert_eq!(best.play_count, 4);
        assert_eq!(best.clear_count, 4);
    }

    #[test]
    fn score_best_counts_every_play_but_only_clear_results() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        let mut failed = record(10, ClearType::Failed);
        failed.played_at = 2;
        db.insert_score(&failed).unwrap();
        let mut clear = record(10, ClearType::Easy);
        clear.played_at = 3;
        db.insert_score(&clear).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 20);
        assert_eq!(best.clear_type, "Normal");
        assert_eq!(best.play_count, 3);
        assert_eq!(best.clear_count, 2);
    }

    #[test]
    fn score_best_keeps_independent_bp_cb_and_max_combo_records() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut best_score = record(20, ClearType::Normal);
        best_score.score.max_combo = 30;
        best_score.score.judges.fast_bad = 4;
        db.insert_score(&best_score).unwrap();

        let mut lower_score = record(10, ClearType::Failed);
        lower_score.played_at = 2;
        lower_score.score.max_combo = 80;
        lower_score.score.judges.fast_bad = 2;
        db.insert_score(&lower_score).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 20);
        assert_eq!(best.clear_type, "Normal");
        assert_eq!(best.max_combo, 80);
        assert_eq!(best.bp, 2);
        assert_eq!(best.cb, 2);
        assert_eq!(best.play_count, 2);
        assert_eq!(best.clear_count, 1);
    }

    #[test]
    fn failed_score_counts_unprocessed_notes_for_bp_and_cb_records() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut failed = record(0, ClearType::Failed);
        failed.total_notes = 100;
        db.insert_score(&failed).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        assert_eq!(history[0].bp, 100);
        assert_eq!(history[0].cb, 100);

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.clear_type, "Failed");
        assert_eq!(best.bp, 100);
        assert_eq!(best.cb, 100);
    }

    #[test]
    fn score_best_is_separate_per_ln_policy() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut ln = record(20, ClearType::Normal);
        ln.ln_policy = LnScorePolicy::ForceLn;
        let mut cn = record(40, ClearType::Hard);
        cn.ln_policy = LnScorePolicy::ForceCn;

        db.insert_score(&ln).unwrap();
        db.insert_score(&cn).unwrap();

        assert_eq!(db.best_ex_score(key([7; 32])).unwrap(), Some(20));
        assert_eq!(
            db.best_ex_score(ScoreKey::new([7; 32], LnScorePolicy::ForceCn)).unwrap(),
            Some(40)
        );
    }

    #[test]
    fn player_info_is_created_and_display_name_updates() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let info = db.player_info().unwrap();
        assert_eq!(info.player_uuid.len(), 32);
        assert!(info.player_uuid.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(info.display_name, "");

        db.set_player_display_name("hyrorre", 1_700_000_099).unwrap();

        let info = db.player_info().unwrap();
        assert_eq!(info.display_name, "hyrorre");
        assert_eq!(info.updated_at, 1_700_000_099);
    }

    #[test]
    fn player_stats_accumulates_profile_wide_scores() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut first = record(20, ClearType::Normal);
        first.played_at = 10;
        first.score.judges.fast_great = 3;
        first.score.judges.slow_bad = 2;
        let mut failed = record(10, ClearType::Failed);
        failed.played_at = 20;
        failed.score.max_combo = 99;
        failed.score.judges.fast_empty_poor = 4;

        db.insert_score(&first).unwrap();
        db.insert_score(&failed).unwrap();

        let stats = db.player_stats().unwrap();
        assert_eq!(stats.play_count, 2);
        assert_eq!(stats.clear_count, 1);
        assert_eq!(stats.max_combo, 99);
        assert_eq!(stats.fast_pgreat, 0);
        assert_eq!(stats.slow_pgreat, 15);
        assert_eq!(stats.fast_great, 3);
        assert_eq!(stats.slow_bad, 2);
        assert_eq!(stats.fast_empty_poor, 4);
        assert_eq!(stats.updated_at, 20);
    }

    #[test]
    fn player_stats_migration_backfills_existing_history() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, &SCORE_MIGRATIONS[..7]).unwrap();
        conn.execute(
            "INSERT INTO score_history (
                chart_sha256, ln_policy, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, rule_mode, assist_mask, autoplay,
                replay_path, ghost
            ) VALUES (
                ?1, 'ForceLn', 10, 'Normal', 'Normal', 80.0,
                10, 20, 1, 1, 8,
                1, 2, 3, 4,
                5, 6, 7, 8,
                9, 10, 11, 12,
                NULL, '', 'Beatoraja', 0, 0,
                '', ''
            )",
            params![hash_to_hex(&[1; 32])],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO score_history (
                chart_sha256, ln_policy, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, rule_mode, assist_mask, autoplay,
                replay_path, ghost
            ) VALUES (
                ?1, 'ForceLn', 20, 'Failed', 'Normal', 20.0,
                10, 10, 5, 5, 12,
                2, 3, 4, 5,
                6, 7, 8, 9,
                10, 11, 12, 13,
                NULL, '', 'Beatoraja', 0, 0,
                '', ''
            )",
            params![hash_to_hex(&[2; 32])],
        )
        .unwrap();

        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let db = ScoreDatabase { conn };

        let stats = db.player_stats().unwrap();
        assert_eq!(stats.play_count, 2);
        assert_eq!(stats.clear_count, 1);
        assert_eq!(stats.max_combo, 12);
        assert_eq!(stats.fast_pgreat, 3);
        assert_eq!(stats.slow_empty_poor, 25);
        assert_eq!(stats.updated_at, 20);
    }

    #[test]
    fn score_history_records_previous_best_snapshot() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut first = record(20, ClearType::Normal);
        first.played_at = 10;
        first.score.max_combo = 15;
        first.score.judges.fast_bad = 2;
        db.insert_score(&first).unwrap();

        let mut second = record(30, ClearType::Hard);
        second.played_at = 20;
        db.insert_score(&second).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        assert_eq!(history[1].previous_best, None);
        assert_eq!(
            history[0].previous_best,
            Some(PreviousBestSnapshot {
                clear_type: "Normal".to_string(),
                ex_score: 20,
                max_combo: 15,
                bp: 2,
                cb: 2,
            })
        );
    }

    #[test]
    fn score_history_previous_best_is_separate_per_ln_policy() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut ln = record(20, ClearType::Normal);
        ln.ln_policy = LnScorePolicy::ForceLn;
        ln.played_at = 10;
        let mut cn_first = record(40, ClearType::Hard);
        cn_first.ln_policy = LnScorePolicy::ForceCn;
        cn_first.played_at = 20;
        let mut cn_second = record(10, ClearType::Easy);
        cn_second.ln_policy = LnScorePolicy::ForceCn;
        cn_second.played_at = 30;

        db.insert_score(&ln).unwrap();
        db.insert_score(&cn_first).unwrap();
        db.insert_score(&cn_second).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        assert_eq!(
            history[0].previous_best.as_ref().map(|best| (best.clear_type.as_str(), best.ex_score)),
            Some(("Hard", 40))
        );
        assert_eq!(history[1].previous_best, None);
        assert_eq!(history[2].previous_best, None);
    }

    #[test]
    fn beatoraja_ghost_round_trips_as_gzip_urlsafe_base64() {
        let ghost = vec![0, 1, 2, 3, 4];

        let encoded = encode_beatoraja_ghost(&ghost).unwrap();
        let decoded = decode_beatoraja_ghost(&encoded, ghost.len() as u32).unwrap();

        assert_eq!(decoded, ghost);
    }

    #[test]
    fn insert_score_persists_best_ghost_for_current_best() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        db.insert_score(&record(10, ClearType::Hard)).unwrap();

        assert_eq!(db.best_ghost(key([7; 32]), 10).unwrap(), Some(vec![0; 10]));
    }

    #[test]
    fn class_gauge_types_round_trip_via_score_history_and_best() {
        // 段位ゲージで終わったプレイが score_history / score_best 経由で
        // `"Class" / "ExClass" / "ExHardClass"` の文字列として正しく永続化・
        // 復元されることを担保する。
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let cases = [
            ([10u8; 32], GaugeType::Class, "Class"),
            ([11u8; 32], GaugeType::ExClass, "ExClass"),
            ([12u8; 32], GaugeType::ExHardClass, "ExHardClass"),
        ];

        // ex_score は (sha[0], 段位ごと) で順に上げ、score_best が上書きされて
        // 残ることを保証する。
        for (i, (sha, gauge, _)) in cases.iter().enumerate() {
            let mut rec = record(20 + i as u32 * 10, ClearType::Hard);
            rec.chart_sha256 = *sha;
            rec.gauge_type = Some(*gauge);
            rec.gauge_value = 42.0 + i as f32;
            db.insert_score(&rec).unwrap();
        }

        // score_history: GaugeType::as_str() の文字列で素直に入る。
        let history = db.recent_history(10, 0).unwrap();
        let mut history_map: std::collections::HashMap<[u8; 32], String> =
            history.into_iter().map(|entry| (entry.chart_sha256, entry.gauge_type)).collect();
        for (sha, _, expected) in &cases {
            assert_eq!(history_map.remove(sha).as_deref(), Some(*expected), "history {sha:?}");
        }

        // score_best: 同じく文字列でラウンドトリップ、gauge_value も保持される。
        let keys: Vec<_> = cases.iter().map(|(sha, _, _)| key(*sha)).collect();
        let best = db.best_scores_for_charts(&keys).unwrap();
        assert_eq!(best.len(), 3);
        let mut by_sha: std::collections::HashMap<_, _> =
            best.into_iter().map(|s| (s.chart_sha256, s)).collect();
        for (i, (sha, _, expected_label)) in cases.iter().enumerate() {
            let summary = by_sha.remove(sha).expect("best entry exists");
            assert_eq!(summary.gauge_type, *expected_label);
            assert_eq!(summary.gauge_value, 42.0 + i as f32);
        }
    }

    #[test]
    fn gauge_type_str_matches_enum_display_for_class_gauges() {
        assert_eq!(gauge_type_str(Some(GaugeType::Class)), "Class");
        assert_eq!(gauge_type_str(Some(GaugeType::ExClass)), "ExClass");
        assert_eq!(gauge_type_str(Some(GaugeType::ExHardClass)), "ExHardClass");
        // sanity: 非段位ゲージも従来通り。
        assert_eq!(gauge_type_str(Some(GaugeType::Normal)), "Normal");
        assert_eq!(gauge_type_str(None), "");
    }

    #[test]
    fn best_scores_for_charts_returns_existing_scores() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        let mut first = record(20, ClearType::Normal);
        first.chart_sha256 = [1; 32];
        first.replay_path = "replay/one.bzr".to_string();
        let mut second = record(10, ClearType::Easy);
        second.chart_sha256 = [2; 32];
        second.gauge_type = None;

        db.insert_score(&first).unwrap();
        db.insert_score(&second).unwrap();

        let scores =
            db.best_scores_for_charts(&[key([2; 32]), key([3; 32]), key([1; 32])]).unwrap();

        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].chart_sha256, [2; 32]);
        assert_eq!(scores[0].gauge_type, "");
        assert_eq!(scores[1].chart_sha256, [1; 32]);
        assert_eq!(scores[1].replay_path, "replay/one.bzr");
    }

    fn sample_slot(slot: u8, ex_score: u32) -> ReplaySlotRecord {
        ReplaySlotRecord {
            chart_sha256: [1; 32],
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            slot,
            rule: ReplaySlotRule::Always,
            replay_path: format!("replay/{slot}.toml"),
            played_at: 1_700_000_000 + slot as i64,
            ex_score,
            bp: 0,
            cb: 0,
            max_combo: ex_score,
            clear_rank: ClearType::Normal as u8,
        }
    }

    #[test]
    fn replay_slots_for_charts_reports_slot_presence_from_new_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
        db.upsert_replay_slot(&sample_slot(2, 30)).unwrap();

        let slots = db.replay_slots_for_charts(&[key([2; 32]), key([1; 32])]).unwrap();

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].chart_sha256, [1; 32]);
        assert_eq!(slots[0].replay_slots, [true, false, true, false]);
    }

    #[test]
    fn upsert_replay_slot_overwrites_same_slot() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
        let mut updated = sample_slot(0, 99);
        updated.replay_path = "replay/updated.toml".to_string();
        db.upsert_replay_slot(&updated).unwrap();

        let record = db.replay_slot(key([1; 32]), 0).unwrap().unwrap();
        assert_eq!(record.ex_score, 99);
        assert_eq!(record.replay_path, "replay/updated.toml");
    }

    #[test]
    fn replay_slots_for_chart_returns_all_four_slots() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
        db.upsert_replay_slot(&sample_slot(3, 30)).unwrap();

        let slots = db.replay_slots_for_chart(key([1; 32])).unwrap();

        assert!(slots[0].is_some());
        assert!(slots[1].is_none());
        assert!(slots[2].is_none());
        assert_eq!(slots[3].as_ref().unwrap().ex_score, 30);
    }

    #[test]
    fn replay_slots_are_separate_per_ln_policy() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        let mut ln = sample_slot(0, 10);
        ln.ln_policy = LnScorePolicy::ForceLn;
        let mut cn = sample_slot(0, 99);
        cn.ln_policy = LnScorePolicy::ForceCn;

        db.upsert_replay_slot(&ln).unwrap();
        db.upsert_replay_slot(&cn).unwrap();

        let ln_slot = db.replay_slot(key([1; 32]), 0).unwrap().unwrap();
        let cn_slot =
            db.replay_slot(ScoreKey::new([1; 32], LnScorePolicy::ForceCn), 0).unwrap().unwrap();
        assert_eq!(ln_slot.ex_score, 10);
        assert_eq!(cn_slot.ex_score, 99);
    }

    #[test]
    fn score_record_can_be_built_from_play_result() {
        let result = PlayResult {
            chart_sha256: [9; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Hard,
            gauge_value: 76.5,
            total_notes: 1,
            score: score_with_ex_score(2),
            autoplay: true,
        };

        let record = ScoreRecord::from_play_result(
            &result,
            ScoreRecordMetadata::new(
                LnScorePolicy::ForceCn,
                DoubleOptionScoreBucket::Battle,
                1_700_000_040,
                Some(123),
                "Normal",
                "Hard",
                "Lr2Oraja",
                0,
                InputDeviceKind::Controller,
                "",
            ),
        );

        assert_eq!(record.chart_sha256, [9; 32]);
        assert_eq!(record.ln_policy, LnScorePolicy::ForceCn);
        assert_eq!(record.double_option, DoubleOptionScoreBucket::Battle);
        assert_eq!(record.played_at, 1_700_000_040);
        assert_eq!(record.clear_type, ClearType::Normal);
        assert_eq!(record.gauge_type, Some(GaugeType::Hard));
        assert_eq!(record.gauge_value, 76.5);
        assert_eq!(record.device_type, InputDeviceKind::Controller);
        assert_eq!(record.score.ex_score(), 2);
        assert!(record.autoplay);
        assert_eq!(record.gauge_option, "Hard");
        assert_eq!(record.rule_mode, "Lr2Oraja");
        assert_eq!(record.replay_path, "");
    }

    #[test]
    fn tag_score_history_with_course_updates_only_given_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);

        let mut r1 = record(20, ClearType::Normal);
        r1.chart_sha256 = [1; 32];
        let mut r2 = record(30, ClearType::Easy);
        r2.chart_sha256 = [2; 32];
        let mut r3 = record(10, ClearType::Failed);
        r3.chart_sha256 = [3; 32];
        let id1 = db.insert_score(&r1).unwrap();
        let id2 = db.insert_score(&r2).unwrap();
        let id3 = db.insert_score(&r3).unwrap();

        // Tag the first two with course_score_id=99, leave r3 untouched.
        let updated = db.tag_score_history_with_course(&[id1, id2], 99).unwrap();
        assert_eq!(updated, 2);

        let rows: Vec<(i64, Option<i64>)> = db
            .conn()
            .prepare("SELECT id, course_score_id FROM score_history ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![(id1, Some(99)), (id2, Some(99)), (id3, None)]);
    }

    #[test]
    fn tag_score_history_with_course_no_op_on_empty_list() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        assert_eq!(db.tag_score_history_with_course(&[], 1).unwrap(), 0);
    }

    #[test]
    fn recent_history_returns_newest_scores_first() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        let mut older = record(20, ClearType::Normal);
        older.played_at = 1;
        older.chart_sha256 = [1; 32];
        let mut newer = record(10, ClearType::Easy);
        newer.played_at = 2;
        newer.chart_sha256 = [2; 32];
        newer.autoplay = true;

        db.insert_score(&older).unwrap();
        db.insert_score(&newer).unwrap();

        let history = db.recent_history(10, 0).unwrap();

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].chart_sha256, [2; 32]);
        assert_eq!(history[0].played_at, 2);
        assert!(history[0].autoplay);
        assert_eq!(history[1].chart_sha256, [1; 32]);
    }

    #[test]
    fn recent_history_exposes_course_score_id_when_tagged() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);

        let mut solo = record(20, ClearType::Normal);
        solo.chart_sha256 = [1; 32];
        let solo_id = db.insert_score(&solo).unwrap();

        let mut course_play = record(30, ClearType::Easy);
        course_play.chart_sha256 = [2; 32];
        let course_play_id = db.insert_score(&course_play).unwrap();

        // Tag the course-attempt row only.
        db.tag_score_history_with_course(&[course_play_id], 77).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        let by_id: std::collections::HashMap<i64, &ScoreHistoryEntry> =
            history.iter().map(|h| (h.id, h)).collect();
        assert_eq!(by_id.get(&solo_id).unwrap().course_score_id, None);
        assert_eq!(by_id.get(&course_play_id).unwrap().course_score_id, Some(77));
    }

    #[test]
    fn ir_score_jobs_round_trip_and_dedupe_by_provider_account_score() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        let local_score_id = db.insert_score(&record(20, ClearType::Normal)).unwrap();

        let job = NewIrScoreJob {
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id,
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

        db.mark_ir_score_job_status(first_id, IrScoreJobStatus::Succeeded, 210, "").unwrap();
        assert!(db.pending_ir_score_jobs(300, 10).unwrap().is_empty());

        let submission_id = db
            .insert_ir_score_submission(&NewIrScoreSubmission {
                job_id: first_id,
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                local_score_id,
                remote_score_id: "sc_remote".to_string(),
                status: "succeeded".to_string(),
                submitted_at: 220,
                response_json: "{\"accepted\":true}".to_string(),
                error: String::new(),
            })
            .unwrap();
        assert!(submission_id > 0);
    }

    #[test]
    fn ir_score_job_failures_back_off_progressively() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        let local_score_id = db.insert_score(&record(20, ClearType::Normal)).unwrap();
        let job_id = db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id,
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
                    params![job_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(attempt_count, attempt as u32 + 1);
            assert_eq!(next_attempt_at, now + delay, "attempt {attempt}");
        }
    }

    #[test]
    fn stale_sending_ir_score_jobs_are_retried() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        let local_score_id = db.insert_score(&record(20, ClearType::Normal)).unwrap();
        let job_id = db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id,
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
    fn chart_sha256_columns_are_lowercase_hex_text() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        db.insert_score(&record(20, ClearType::Normal)).unwrap();

        let (hist_typeof, best_typeof, best_hex): (String, String, String) = db
            .conn()
            .query_row(
                "SELECT
                    (SELECT typeof(chart_sha256) FROM score_history LIMIT 1),
                    (SELECT typeof(chart_sha256) FROM score_best LIMIT 1),
                    (SELECT chart_sha256 FROM score_best LIMIT 1)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(hist_typeof, "text");
        assert_eq!(best_typeof, "text");
        assert_eq!(best_hex.len(), 64);
        assert!(best_hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }
}
