use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params, params_from_iter};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};
use crate::ln_policy::LnScorePolicy;

const SUCCEEDED_IR_SCORE_JOB_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
const SUCCEEDED_IR_SCORE_JOB_RETAIN_RECENT_COUNT: u32 = 500;
/// 恒久拒否されたjobの `next_attempt_at`（9999-12-31T23:59:59Z）。自動再試行はここに届かない。
pub const IR_JOB_NO_AUTO_RETRY_AT: i64 = 253_402_300_799;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrBackfillResendOutcome {
    Enqueued,
    AlreadyPending,
    Busy,
    PayloadUnavailable,
    NeedsPayload,
}

mod model;
mod operations;
mod rows;

pub use model::{
    IrJobKind, IrJobRetryFilter, IrLocalScoreCleanupReport, IrRivalScoreCacheState,
    IrRivalScoreRecord, IrScoreJobBacklog, IrScoreJobListFilter, IrScoreJobRecord,
    IrScoreJobStatusCounts, IrScoreJobSummary, IrSubmittedScoreLink, NewIrScoreJob,
    NewIrScoreSubmission,
};

#[cfg(test)]
#[path = "network_db/tests.rs"]
mod tests;
