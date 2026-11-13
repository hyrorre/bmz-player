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
            Self::NotCleared => "not_cleared",
            Self::NoChange => "no_change",
            Self::Ineligible => "ineligible",
            Self::Pending => "pending",
            Self::Sending => "sending",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

impl IrScoreJobStatus {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "sending" => Self::Sending,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "ineligible" => Self::Ineligible,
            "no_change" => Self::NoChange,
            "not_cleared" => Self::NotCleared,
            _ => return None,
        })
    }
}

/// job取得時に、`pending` / `failed` jobのどれを再試行の対象にするか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrJobRetryFilter {
    /// `next_attempt_at` に達したjobだけ。
    RespectBackoff,
    /// `failed` jobも再試行時刻に関係なく対象にする。
    IgnoreBackoff,
    /// Explicit resend of one selected job; may restart a confirmed failed import.
    ExplicitResend,
    /// 再試行時刻は無視するが、`updated_at` が指定時刻以降の `failed` jobは除く。
    /// 1回の手動実行で各jobを最大1回だけ送るために使う（失敗時に `updated_at` が進む）。
    FailedBefore(i64),
}

impl IrJobRetryFilter {
    pub const fn from_ignore_backoff(ignore_retry_backoff: bool) -> Self {
        if ignore_retry_backoff { Self::IgnoreBackoff } else { Self::RespectBackoff }
    }

    /// `?1` を現在時刻として参照するSQL条件。
    pub(super) fn sql_condition(self) -> String {
        match self {
            Self::RespectBackoff => {
                "status IN ('pending', 'failed') AND next_attempt_at <= ?1".to_string()
            }
            Self::IgnoreBackoff | Self::ExplicitResend => {
                "status IN ('pending', 'failed')".to_string()
            }
            Self::FailedBefore(since) => {
                format!("status = 'pending' OR (status = 'failed' AND updated_at < {since})")
            }
        }
    }
}

/// 指定provider/account/kindの未完了jobの内訳。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IrScoreJobBacklog {
    /// 今claimできるjob（期限切れの `sending` を含む）。
    pub claimable: u32,
    /// 別の主体がlease中の `sending` job。
    pub leased: u32,
    /// 再試行条件を満たさない `failed` job。
    pub deferred: u32,
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

/// 送信キュー一覧用の軽量レコード。重い `payload_json` は含めない。
#[derive(Debug, Clone)]
pub struct IrScoreJobSummary {
    pub id: i64,
    pub provider: String,
    pub account_id: String,
    pub kind: IrJobKind,
    pub local_score_id: i64,
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub status: String,
    pub attempt_count: u32,
    pub next_attempt_at: i64,
    pub last_error: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// `list_ir_score_jobs` / `count_ir_score_jobs` の絞り込み条件。
#[derive(Debug, Clone, Copy)]
pub struct IrScoreJobListFilter<'a> {
    pub provider: Option<&'a str>,
    pub account_id: Option<&'a str>,
    /// 空なら全kind。
    pub kinds: &'a [IrJobKind],
    /// 空なら `succeeded` / `no_change` 以外（キューに残っているもの）。
    pub statuses: &'a [IrScoreJobStatus],
    pub limit: u32,
    pub offset: u32,
}

impl Default for IrScoreJobListFilter<'_> {
    fn default() -> Self {
        Self { provider: None, account_id: None, kinds: &[], statuses: &[], limit: 100, offset: 0 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IrScoreJobStatusCounts {
    pub pending: u32,
    pub sending: u32,
    pub failed: u32,
    pub succeeded: u32,
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

/// rianIR の軽量ライバル全曲APIから取得した、譜面/LNモード単位のベスト。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRivalScoreRecord {
    pub chart_sha256: [u8; 32],
    pub ln_mode: u8,
    pub ex_score: u32,
    pub clear_type: i32,
    pub max_combo: u32,
    pub min_bp: i32,
    pub play_option: i32,
    pub arrange_1p: String,
    pub arrange_2p: String,
    pub double_option: String,
    pub play_seed: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IrRivalScoreCacheState {
    pub etag: String,
    pub fetched_at: i64,
}
use super::*;
