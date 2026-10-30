//! 設定UIやCLIから手動で実行するIR一括処理。
//!
//! 出力は一切行わず、進捗は `IrManualProgress` のコールバックで通知する。
//! 中断は `tokio::sync::watch::Receiver<bool>` が true になった時点で、
//! 処理中の1件を終えてから止める（job間・batch間・送信間隔のsleep中に確認する）。

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::sync::watch;

use crate::config::profile_config::{IrConfig, IrProviderConfig};
use crate::ir::backfill::IrLocalUploadReport;
use crate::ir::download::IrScoreDownloadReport;

mod download;
mod resend;
mod sync;
mod upload;

pub use download::download_scores;
pub use resend::{IrManualResendReport, resend_failed_jobs, resend_job};
pub use sync::{
    IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT, IrFailureBreaker, IrManualSyncOutcome, IrManualSyncRetry,
    IrManualSyncScope, sync_jobs, sync_jobs_with,
};
pub use upload::{upload_local_all, upload_local_batch};

pub type IrManualCancel = watch::Receiver<bool>;
pub type IrManualProgressSink<'a> = &'a mut (dyn FnMut(IrManualProgress) + Send);

/// job同期に必要なpath群。
#[derive(Debug, Clone, Copy)]
pub struct IrSyncContext<'a> {
    pub score_db: &'a Path,
    pub profile_root: &'a Path,
    pub logs_dir: &'a Path,
    pub ir_config: &'a IrConfig,
}

/// 手動IR処理に必要なpath群。DBは各関数が開く（呼び出し側でmigration済みであること）。
#[derive(Debug, Clone, Copy)]
pub struct IrManualContext<'a> {
    pub library_db: &'a Path,
    pub network_db: &'a Path,
    pub sync: IrSyncContext<'a>,
}

#[derive(Debug, Clone)]
pub enum IrManualProgress {
    /// `--all` の何回目のbatchか。
    FullUploadBatch {
        batch: u32,
    },
    /// 未完了のscore jobがあるため、追加enqueueの前に同期する。
    ExistingQueued {
        provider_key: String,
        account_id: String,
        queued: u32,
    },
    /// `limit` はbatch上限（dry-runの "would enqueue" 表示用）。
    Enqueued {
        report: IrLocalUploadReport,
        dry_run: bool,
        all: bool,
        limit: u32,
    },
    /// `--all` で上限に達したが続行する。
    LimitReachedContinuing,
    SyncStarted {
        limit: u32,
        estimated_seconds: u64,
    },
    /// `index` は1始まり。
    JobSyncing {
        index: u32,
        limit: u32,
    },
    JobFinished {
        submitted: u32,
        failed: u32,
    },
    JobError,
    NoQueuedJobs,
    SyncSummary {
        provider_key: String,
        account_id: String,
        submitted: u32,
        failed: u32,
        remaining: u32,
        messages: Vec<String>,
    },
    FullUploadComplete {
        enqueued: u32,
        submitted: u32,
    },
    DownloadStarted {
        provider_key: String,
        account_id: String,
    },
    /// ページを取り込むたびの累計。
    DownloadPage(IrScoreDownloadReport),
}

/// 手動処理が止まった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrManualStop {
    #[default]
    Completed,
    Cancelled,
    /// 送信に失敗したjobが残った。`--all` では残りのjobを処理し終えてから、
    /// それ以外では失敗した時点でこの状態になる。失敗jobは送信キューに残り再送できる。
    FailedJobs {
        failed: u32,
    },
    /// 送信が `consecutive` 件続けて失敗したため打ち切った（サーバー障害・通信断の疑い）。
    ConsecutiveFailures {
        consecutive: u32,
    },
    /// 認証・ログイン状態の問題で、以降のjobも送れないため打ち切った。
    Unauthorized,
    /// IRがアカウントの送信停止（`banned == false`）またはBANを返したため打ち切った。
    /// 再ログインでは解消しない。ローカルスコアと送信キューは残る。
    AccountRestricted {
        banned: bool,
    },
    /// サーバーが送信を制限している（429）ため打ち切った。
    RateLimited {
        retry_after_seconds: Option<u64>,
    },
    /// 進捗が無かった。他プロセス・常駐workerがsending中のjobを持っており、
    /// lease（5分）が切れるか完了するまで待てば再試行できる。
    WaitingForSendingJob,
}

#[derive(Debug, Clone, Default)]
pub struct IrManualUploadReport {
    pub provider_key: String,
    pub account_id: String,
    pub batches: u32,
    pub scanned: u32,
    pub candidates: u32,
    pub enqueued: u32,
    pub submitted: u32,
    pub failed: u32,
    /// 最後に確認した未完了score job数。
    pub remaining: u32,
    pub skipped_already_submitted: u32,
    pub skipped_already_queued: u32,
    pub skipped_resend_payload_unavailable: u32,
    pub skipped_busy: u32,
    pub skipped_missing_chart: u32,
    pub skipped_course_stage: u32,
    pub skipped_autoplay: u32,
    pub missing_replays: u32,
    /// 読み取り・payload作成に失敗して飛ばした履歴数。詳細は `messages`。
    pub skipped_invalid: u32,
    /// 最後のenqueueがbatch上限に達したか。
    pub limit_reached: bool,
    pub messages: Vec<String>,
    pub stop: IrManualStop,
}

impl IrManualUploadReport {
    pub fn cancelled(&self) -> bool {
        self.stop == IrManualStop::Cancelled
    }
}

pub fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn is_cancelled(cancel: &IrManualCancel) -> bool {
    *cancel.borrow()
}

/// `duration` 待つ。待機中に中断要求が来たら即座に戻り true を返す。
pub(crate) async fn sleep_or_cancel(duration: Duration, cancel: &IrManualCancel) -> bool {
    let mut watcher = cancel.clone();
    let sleep = tokio::time::sleep(duration);
    tokio::pin!(sleep);
    tokio::select! {
        _ = &mut sleep => is_cancelled(cancel),
        requested = async { watcher.wait_for(|requested| *requested).await.is_ok() } => {
            if requested {
                true
            } else {
                // 送信側が破棄された（中断されることは無い）。残りを普通に待つ。
                sleep.await;
                false
            }
        }
    }
}

/// provider指定（なければprimary、それも無ければ最初の有効なprovider）を解決する。
pub fn selected_provider<'a>(
    ir_config: &'a IrConfig,
    requested: Option<&str>,
) -> Result<&'a IrProviderConfig> {
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        let provider = ir_config
            .providers
            .iter()
            .find(|entry| {
                entry.provider == requested
                    || crate::ir::provider_key::configured_provider_key(entry)
                        .is_some_and(|provider_key| provider_key == requested)
            })
            .with_context(|| format!("IR provider is not configured: {requested}"))?;
        if !provider.enabled {
            bail!("IR provider '{}' is disabled", provider.provider);
        }
        if provider.base_url.trim().is_empty() {
            bail!("IR provider '{}' has no base_url; run `bmz ir login` first", provider.provider);
        }
        if crate::ir::provider_key::configured_provider_key(provider).is_none() {
            bail!(
                "IR provider '{}' has no provider key; run `bmz ir login` first",
                provider.provider
            );
        }
        return Ok(provider);
    }

    let provider_name = if ir_config.primary_provider.is_empty() {
        ir_config
            .providers
            .iter()
            .find(|entry| {
                entry.enabled && crate::ir::provider_key::configured_provider_key(entry).is_some()
            })
            .and_then(crate::ir::provider_key::configured_provider_key)
            .map(str::to_string)
            .unwrap_or_default()
    } else {
        ir_config.primary_provider.clone()
    };
    ir_config
        .providers
        .iter()
        .find(|entry| {
            !entry.base_url.is_empty()
                && crate::ir::provider_key::configured_provider_key(entry)
                    .is_some_and(|provider_key| provider_key == provider_name)
        })
        .context("no read-capable IR provider configured")
}

#[cfg(test)]
mod tests;
