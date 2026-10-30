use anyhow::{Result, bail};

use crate::ir::sync::{
    IR_CLI_SYNC_JOB_SPACING_MS, IrSyncFailureClass, IrSyncJobFilter, IrSyncReport, IrSyncThrottle,
    sync_pending_ir_jobs, sync_pending_ir_jobs_filtered,
};
use crate::storage::network_db::{IrJobKind, IrJobRetryFilter, NetworkDatabase};

use super::{
    IrManualCancel, IrManualProgress, IrManualProgressSink, IrManualStop, IrSyncContext,
    is_cancelled, now_unix_seconds, sleep_or_cancel,
};

/// 一括送信で、この件数だけ続けて失敗したら止める。
///
/// サーバー障害や通信断の最中に送り続けると、全jobの試行回数が増えて長いbackoffに入る。
/// 止めるまでに巻き込むjobをこの件数に抑える。
pub const IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT: u32 = 10;

#[derive(Debug, Clone, Copy)]
pub enum IrManualSyncScope<'a> {
    /// 全provider・全kindのjob。
    Any,
    Kind {
        provider_key: &'a str,
        account_id: &'a str,
        kind: IrJobKind,
    },
}

/// `sync_jobs_with` の再試行方針。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrManualSyncRetry {
    /// 失敗が出るまでは再試行待ち時刻を無視し、失敗後は尊重する（同じ失敗jobを連打しない）。
    UntilFirstFailure,
    /// 1回の実行で各jobを最大1回だけ送る。再試行待ち時刻は無視するが、
    /// `started_at` 以降に失敗したjobは取り直さない。`IrManualSyncScope::Kind` 専用。
    OncePerRun { started_at: i64 },
}

/// 送信失敗の続き方から、一括送信を打ち切るか判定する。
///
/// 認証エラー・アカウント停止/BAN・429は以降のjobも同じ理由で失敗するため即座に止める。
/// それ以外の失敗は次のjobへ進み、`IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT` 件続いたら止める。
#[derive(Debug, Clone, Default)]
pub struct IrFailureBreaker {
    consecutive: u32,
}

impl IrFailureBreaker {
    /// 1件分の送信結果を記録し、止めるべきなら理由を返す。
    pub fn record(&mut self, report: &IrSyncReport) -> Option<IrManualStop> {
        if report.submitted > 0 {
            self.consecutive = 0;
        }
        for failure in &report.failures {
            match failure.class {
                IrSyncFailureClass::Unauthorized => return Some(IrManualStop::Unauthorized),
                IrSyncFailureClass::Moderated(code) if code.is_account_level() => {
                    return Some(IrManualStop::AccountRestricted {
                        banned: code == crate::ir::IrModerationCode::AccountBanned,
                    });
                }
                // 譜面単位の拒否はサーバー障害ではないため、連続失敗には数えない。
                IrSyncFailureClass::Moderated(_) => {}
                IrSyncFailureClass::RateLimited { retry_after_seconds } => {
                    return Some(IrManualStop::RateLimited { retry_after_seconds });
                }
                IrSyncFailureClass::Job | IrSyncFailureClass::Transient => {
                    self.consecutive = self.consecutive.saturating_add(1);
                }
            }
        }
        (self.consecutive >= IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT)
            .then_some(IrManualStop::ConsecutiveFailures { consecutive: self.consecutive })
    }
}

#[derive(Debug, Default)]
pub struct IrManualSyncOutcome {
    pub report: IrSyncReport,
    pub cancelled: bool,
    /// `IrFailureBreaker` が送信を打ち切った理由。
    pub stop: Option<IrManualStop>,
}

/// 未送信jobを1件ずつclaimして最大 `limit` 件送る。
///
/// 送信間隔は `IR_CLI_SYNC_JOB_SPACING_MS`。失敗が出たら以降は再試行待ち時刻を尊重する
/// （同じ失敗jobを連打しない）。
pub async fn sync_jobs(
    ctx: &IrSyncContext<'_>,
    network_db: &mut NetworkDatabase,
    scope: IrManualSyncScope<'_>,
    limit: u32,
    progress: IrManualProgressSink<'_>,
    cancel: &IrManualCancel,
) -> Result<IrManualSyncOutcome> {
    sync_jobs_with(
        ctx,
        network_db,
        scope,
        limit,
        IrManualSyncRetry::UntilFirstFailure,
        None,
        progress,
        cancel,
    )
    .await
}

/// `sync_jobs` に再試行方針と打ち切り判定を指定できる版。
#[allow(clippy::too_many_arguments)]
pub async fn sync_jobs_with(
    ctx: &IrSyncContext<'_>,
    network_db: &mut NetworkDatabase,
    scope: IrManualSyncScope<'_>,
    limit: u32,
    retry: IrManualSyncRetry,
    mut breaker: Option<&mut IrFailureBreaker>,
    progress: IrManualProgressSink<'_>,
    cancel: &IrManualCancel,
) -> Result<IrManualSyncOutcome> {
    if matches!(scope, IrManualSyncScope::Any)
        && matches!(retry, IrManualSyncRetry::OncePerRun { .. })
    {
        bail!("once-per-run IR sync requires a provider/account scope");
    }
    let estimated_seconds = u64::from(limit.saturating_sub(1))
        .saturating_mul(IR_CLI_SYNC_JOB_SPACING_MS)
        .div_ceil(1_000);
    progress(IrManualProgress::SyncStarted { limit, estimated_seconds });

    let mut outcome = IrManualSyncOutcome::default();
    for index in 0..limit {
        if is_cancelled(cancel) {
            outcome.cancelled = true;
            break;
        }
        let retry_filter = match retry {
            IrManualSyncRetry::UntilFirstFailure => {
                IrJobRetryFilter::from_ignore_backoff(outcome.report.failed == 0)
            }
            IrManualSyncRetry::OncePerRun { started_at } => {
                IrJobRetryFilter::FailedBefore(started_at)
            }
        };
        let ignore_retry_backoff = retry_filter != IrJobRetryFilter::RespectBackoff;
        if index > 0 {
            let now = now_unix_seconds();
            let has_next = match scope {
                IrManualSyncScope::Kind { provider_key, account_id, kind } => !network_db
                    .pending_ir_score_jobs_for_kind(
                        provider_key,
                        account_id,
                        kind,
                        now,
                        1,
                        retry_filter,
                    )?
                    .is_empty(),
                IrManualSyncScope::Any if ignore_retry_backoff => {
                    !network_db.pending_ir_score_jobs_ignoring_backoff(now, 1)?.is_empty()
                }
                IrManualSyncScope::Any => !network_db.pending_ir_score_jobs(now, 1)?.is_empty(),
            };
            if !has_next {
                break;
            }
            if sleep_or_cancel(std::time::Duration::from_millis(IR_CLI_SYNC_JOB_SPACING_MS), cancel)
                .await
            {
                outcome.cancelled = true;
                break;
            }
        }
        progress(IrManualProgress::JobSyncing { index: index + 1, limit });
        let sync_result = match scope {
            IrManualSyncScope::Kind { provider_key, account_id, kind } => {
                sync_pending_ir_jobs_filtered(
                    network_db,
                    ctx.score_db,
                    ctx.profile_root,
                    ctx.logs_dir,
                    ctx.ir_config,
                    IrSyncJobFilter { provider_key, account_id, kind, local_score_id: None },
                    now_unix_seconds(),
                    1,
                    retry_filter,
                    IrSyncThrottle::none(),
                )
                .await
            }
            IrManualSyncScope::Any => {
                sync_pending_ir_jobs(
                    network_db,
                    ctx.score_db,
                    ctx.profile_root,
                    ctx.logs_dir,
                    ctx.ir_config,
                    now_unix_seconds(),
                    1,
                    ignore_retry_backoff,
                    IrSyncThrottle::none(),
                )
                .await
            }
        };
        let report = match sync_result {
            Ok(report) => report,
            Err(error) => {
                progress(IrManualProgress::JobError);
                return Err(error);
            }
        };
        if report.submitted.saturating_add(report.failed) == 0 {
            progress(IrManualProgress::NoQueuedJobs);
            break;
        }
        progress(IrManualProgress::JobFinished {
            submitted: report.submitted,
            failed: report.failed,
        });
        let stop = breaker.as_deref_mut().and_then(|breaker| breaker.record(&report));
        let total = &mut outcome.report;
        total.submitted = total.submitted.saturating_add(report.submitted);
        total.failed = total.failed.saturating_add(report.failed);
        total.messages.extend(report.messages);
        total.included_rankings.extend(report.included_rankings);
        total.failures.extend(report.failures);
        if stop.is_some() {
            outcome.stop = stop;
            break;
        }
    }
    Ok(outcome)
}
