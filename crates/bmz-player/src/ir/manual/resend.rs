//! 送信キューからの再送信。
//!
//! 状態をSQLで直接書き換えず、通常の送信と同じclaim経由（`sync_pending_ir_jobs_filtered`）で
//! 送る。claimは `Immediate` トランザクション内で `sending` へ更新するため、
//! 常駐workerや別プロセスが同じjobを送信中なら二重に送らない（その場合は skipped_busy）。

use anyhow::Result;

use crate::ir::sync::{
    IR_CLI_SYNC_JOB_SPACING_MS, IrSyncJobFilter, IrSyncThrottle, sync_pending_ir_jobs_filtered,
};
use crate::storage::network_db::{
    IrJobKind, IrJobRetryFilter, IrScoreJobListFilter, IrScoreJobStatus, NetworkDatabase,
};

use super::{
    IrManualCancel, IrManualProgress, IrManualProgressSink, IrSyncContext, is_cancelled,
    now_unix_seconds, sleep_or_cancel,
};

#[derive(Debug, Clone, Default)]
pub struct IrManualResendReport {
    /// 再送対象にしたjob数。
    pub targeted: u32,
    pub submitted: u32,
    pub failed: u32,
    /// claimできなかったjob数（別の主体が送信中、または既に成功済み・他のjobに置き換わった）。
    pub skipped_busy: u32,
    pub messages: Vec<String>,
    pub cancelled: bool,
}

/// job 1件を、再試行待ち時刻を無視して再送する。
pub async fn resend_job(
    ctx: &IrSyncContext<'_>,
    network_db: &mut NetworkDatabase,
    provider_key: &str,
    account_id: &str,
    kind: IrJobKind,
    local_score_id: i64,
) -> Result<IrManualResendReport> {
    let report = sync_pending_ir_jobs_filtered(
        network_db,
        ctx.score_db,
        ctx.profile_root,
        ctx.logs_dir,
        ctx.ir_config,
        IrSyncJobFilter { provider_key, account_id, kind, local_score_id: Some(local_score_id) },
        now_unix_seconds(),
        1,
        IrJobRetryFilter::IgnoreBackoff,
        IrSyncThrottle::none(),
    )
    .await?;
    let claimed = report.processed > 0;
    Ok(IrManualResendReport {
        targeted: 1,
        submitted: report.submitted,
        failed: report.failed,
        skipped_busy: u32::from(!claimed),
        messages: report.messages,
        cancelled: false,
    })
}

/// 指定provider/accountの `failed` job全件を再送する。
///
/// 開始時点のfailed一覧を対象に固定する（再送中に新しく失敗したjobは含めない）。
/// 送信間隔は `IR_CLI_SYNC_JOB_SPACING_MS`。
pub async fn resend_failed_jobs(
    ctx: &IrSyncContext<'_>,
    network_db: &mut NetworkDatabase,
    provider_key: &str,
    account_id: &str,
    progress: IrManualProgressSink<'_>,
    cancel: &IrManualCancel,
) -> Result<IrManualResendReport> {
    let failed_count =
        network_db.ir_score_job_status_counts(Some(provider_key), Some(account_id))?.failed;
    let targets = network_db.list_ir_score_jobs(&IrScoreJobListFilter {
        provider: Some(provider_key),
        account_id: Some(account_id),
        kinds: &[],
        statuses: &[IrScoreJobStatus::Failed],
        limit: failed_count,
        offset: 0,
    })?;
    let limit = u32::try_from(targets.len()).unwrap_or(u32::MAX);
    let estimated_seconds = u64::from(limit.saturating_sub(1))
        .saturating_mul(IR_CLI_SYNC_JOB_SPACING_MS)
        .div_ceil(1_000);
    progress(IrManualProgress::SyncStarted { limit, estimated_seconds });

    let mut total = IrManualResendReport { targeted: limit, ..IrManualResendReport::default() };
    for (index, job) in targets.iter().enumerate() {
        if is_cancelled(cancel) {
            total.cancelled = true;
            break;
        }
        if index > 0
            && sleep_or_cancel(std::time::Duration::from_millis(IR_CLI_SYNC_JOB_SPACING_MS), cancel)
                .await
        {
            total.cancelled = true;
            break;
        }
        progress(IrManualProgress::JobSyncing {
            index: u32::try_from(index + 1).unwrap_or(u32::MAX),
            limit,
        });
        match resend_job(
            ctx,
            network_db,
            &job.provider,
            &job.account_id,
            job.kind,
            job.local_score_id,
        )
        .await
        {
            Ok(report) => {
                progress(IrManualProgress::JobFinished {
                    submitted: report.submitted,
                    failed: report.failed,
                });
                total.submitted = total.submitted.saturating_add(report.submitted);
                total.failed = total.failed.saturating_add(report.failed);
                total.skipped_busy = total.skipped_busy.saturating_add(report.skipped_busy);
                total.messages.extend(report.messages);
            }
            Err(error) => {
                // 個別jobの準備段階の失敗（リプレイ解決など）は、残りの再送を止めない。
                progress(IrManualProgress::JobError);
                total.failed = total.failed.saturating_add(1);
                total.messages.push(format!("job {}: {error:#}", job.id));
            }
        }
    }
    Ok(total)
}
