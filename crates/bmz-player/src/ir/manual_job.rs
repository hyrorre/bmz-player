//! ゲーム内設定から起動する手動IR操作（一括送信・スコア取得・送信キューの再送/一覧）。
//!
//! UIは `IrManualHandle` へ要求を積み、`snapshot()` を毎フレーム読む。実行はappが
//! `dispatch` で開始する（DBのpathとIR設定はappだけが持つため）。
//!
//! - 書き込み系（`IrManualOp`）は同時に1つ。専用スレッドの current-thread runtime で動かす
//!   （download の future が `Send` ではないため）。
//! - 要求ごとに世代番号を持ち、世代が合わない結果・進捗は捨てる。
//! - 実行中は `pause_receiver()` が true になる。常駐 `IrSyncWorker` はこれを見て待機し、
//!   終了・失敗・中断のいずれでも解除される（`Drop` guard）。

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::watch;

use crate::config::profile_config::IrConfig;
use crate::i18n::Localizer;
use crate::ir::backfill::{IrLocalUploadOptions, enqueue_local_score_jobs};
use crate::ir::download::{IrScoreDownloadOptions, IrScoreDownloadReport};
use crate::ir::manual::{
    IrManualCancel, IrManualContext, IrManualProgress, IrManualProgressSink, IrManualStop,
    IrManualUploadReport, IrSyncContext, download_scores, now_unix_seconds, resend_failed_jobs,
    resend_job, sleep_or_cancel, upload_local_all,
};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::network_db::{
    IrJobKind, IrScoreJobListFilter, IrScoreJobStatus, IrScoreJobStatusCounts, IrScoreJobSummary,
    NetworkDatabase,
};
use crate::storage::score_db::ScoreDatabase;

/// 送信キュー画面の1ページあたりの行数。
pub const IR_QUEUE_PAGE_SIZE: u32 = 50;
/// 他のsending jobしか残っていないときに再試行する間隔。
///
/// sending jobのleaseは5分。常駐workerは一時停止するので、待つ相手は別プロセスのCLIか、
/// 停止要求前にclaim済みだった1件だけ。短い間隔で見に行き、解消し次第すぐ続きを進める。
const LEASE_RETRY_INTERVAL: Duration = Duration::from_secs(15);
/// 進捗が無いまま待ち続ける上限（lease 5分 + 余裕）。超えたら諦めて停止理由を報告する。
const LEASE_WAIT_LIMIT: Duration = Duration::from_secs(6 * 60);

#[derive(Debug, Clone)]
pub struct IrManualEnv {
    pub library_db: PathBuf,
    pub score_db: PathBuf,
    pub network_db: PathBuf,
    pub profile_root: PathBuf,
    pub logs_dir: PathBuf,
    pub ir_config: IrConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrManualOp {
    UploadLocalAll,
    DownloadScores,
    ResendJob { provider_key: String, account_id: String, kind: IrJobKind, local_score_id: i64 },
    ResendFailed { provider_key: String, account_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrQueueStatusFilter {
    #[default]
    Failed,
    Pending,
    Sending,
    /// 成功済み以外すべて。
    Unfinished,
}

impl IrQueueStatusFilter {
    fn statuses(self) -> &'static [IrScoreJobStatus] {
        match self {
            Self::Failed => &[IrScoreJobStatus::Failed],
            Self::Pending => &[IrScoreJobStatus::Pending],
            Self::Sending => &[IrScoreJobStatus::Sending],
            Self::Unfinished => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrQueueQuery {
    pub provider_key: String,
    pub account_id: String,
    pub status: IrQueueStatusFilter,
    /// 0始まり。
    pub page: u32,
}

#[derive(Debug, Clone)]
pub enum IrManualRequest {
    Run(IrManualOp),
    /// 一括送信の対象件数を数える（dry-run相当）。
    EstimateUpload,
    LoadQueue(IrQueueQuery),
}

/// 積まれた要求。`generation` は要求した時点の世代で、結果の適用可否の判定に使う。
#[derive(Debug, Clone)]
pub struct IrManualQueued {
    pub generation: u64,
    pub request: IrManualRequest,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IrManualProgressView {
    pub batch: u32,
    pub enqueued: u32,
    pub submitted: u32,
    pub failed: u32,
    /// 送信待ちの残りscore job数（一括送信）。
    pub remaining: u32,
    pub skipped_busy: u32,
    pub missing_replays: u32,
    /// 現在のsync内の位置（1始まり）と上限。
    pub job_index: u32,
    pub job_limit: u32,
    pub download_pages: u32,
    pub download_scanned: u32,
    pub download_imported: u32,
    pub download_linked: u32,
    pub download_skipped: u32,
    pub download_failed: u32,
    /// lease待ちで再試行を待っている。
    pub waiting_for_lease: bool,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrManualOutcomeStatus {
    Completed,
    /// Score download finished with one or more score import failures.
    DownloadFailed {
        failed: u32,
    },
    Cancelled,
    FailedJobs {
        failed: u32,
    },
    /// 送信が続けて失敗したため一括送信を打ち切った。
    ConsecutiveFailures {
        consecutive: u32,
    },
    /// IRの認証に失敗したため一括送信を打ち切った。
    Unauthorized,
    /// IRアカウントが送信停止またはBANされているため一括送信を打ち切った。
    AccountRestricted {
        banned: bool,
    },
    /// IRサーバーが送信を制限したため一括送信を打ち切った。
    RateLimited,
    /// 他のsending jobが解消せず、待機の上限に達した。
    WaitingTimedOut,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct IrManualOutcome {
    pub op: IrManualOp,
    pub status: IrManualOutcomeStatus,
    pub progress: IrManualProgressView,
    pub messages: Vec<String>,
}

impl IrManualOutcome {
    /// Download が新しいscoreを保存した場合、Selectとplayer statsの読込更新が必要。
    /// 完了・中断・エラーのどれでも、保存済み件数を進捗が保持していれば更新する。
    pub fn imported_scores_need_refresh(&self) -> bool {
        self.op == IrManualOp::DownloadScores && self.progress.download_imported > 0
    }
}

#[derive(Debug, Clone)]
pub struct IrManualRunning {
    pub op: IrManualOp,
    pub progress: IrManualProgressView,
}

#[derive(Debug, Clone, Default)]
pub enum IrEstimateState {
    #[default]
    Idle,
    Counting,
    Ready(IrUploadEstimate),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrUploadEstimate {
    pub provider_key: String,
    pub account_id: String,
    /// これから新しくenqueueされる件数。
    pub candidates: u32,
    /// 既にキューにある未完了score job（これも送られる）。
    pub already_queued: u32,
}

impl IrUploadEstimate {
    pub fn total_jobs(&self) -> u32 {
        self.candidates.saturating_add(self.already_queued)
    }

    /// 送信間隔だけから見積もった最短時間。通信時間は含まない。
    pub fn minimum_duration(&self) -> Duration {
        Duration::from_millis(
            u64::from(self.total_jobs())
                .saturating_mul(crate::ir::sync::IR_CLI_SYNC_JOB_SPACING_MS),
        )
    }
}

#[derive(Debug, Clone)]
pub struct IrQueueRow {
    pub job: IrScoreJobSummary,
    /// library.db から引いた曲名。見つからなければ `None`。
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IrQueueView {
    pub query: IrQueueQuery,
    pub counts: IrScoreJobStatusCounts,
    /// 現在のstatus絞り込みでの総件数。
    pub total: u32,
    pub rows: Vec<IrQueueRow>,
}

#[derive(Debug, Clone, Default)]
pub enum IrQueueState {
    #[default]
    Idle,
    Loading,
    Ready(IrQueueView),
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct IrManualSnapshot {
    pub running: Option<IrManualRunning>,
    pub last_outcome: Option<IrManualOutcome>,
    pub estimate: IrEstimateState,
    pub queue: IrQueueState,
}

#[derive(Default)]
struct State {
    requests: VecDeque<IrManualQueued>,
    /// 書き込み系・estimate・queue それぞれの世代。要求のたびに進める。
    run_generation: u64,
    estimate_generation: u64,
    queue_generation: u64,
    running: Option<(u64, IrManualRunning)>,
    cancel: Option<watch::Sender<bool>>,
    last_outcome: Option<IrManualOutcome>,
    /// appがtoastで通知するまで保持する。
    pending_notice: Option<IrManualOutcome>,
    estimate: IrEstimateState,
    queue: IrQueueState,
    last_queue_query: Option<IrQueueQuery>,
}

struct Shared {
    state: Mutex<State>,
    pause: watch::Sender<bool>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct IrManualHandle {
    shared: Arc<Shared>,
}

impl Default for IrManualHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl IrManualHandle {
    pub fn new() -> Self {
        let (pause, _) = watch::channel(false);
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                pause,
                wake: Mutex::new(None),
                thread: Mutex::new(None),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 状態が変わったときに呼ばれる。appがevent loopを起こすために使う。
    pub fn set_wake(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        *self.shared.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(wake);
    }

    fn wake(&self) {
        let wake = self.shared.wake.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(wake) = wake {
            wake();
        }
    }

    /// 常駐IRワーカーの一時停止信号。true の間はジョブを送らない。
    pub fn pause_receiver(&self) -> watch::Receiver<bool> {
        self.shared.pause.subscribe()
    }

    pub fn is_busy(&self) -> bool {
        self.lock().running.is_some()
    }

    pub fn snapshot(&self) -> IrManualSnapshot {
        let state = self.lock();
        IrManualSnapshot {
            running: state.running.as_ref().map(|(_, running)| running.clone()),
            last_outcome: state.last_outcome.clone(),
            estimate: state.estimate.clone(),
            queue: state.queue.clone(),
        }
    }

    /// UIから呼ぶ。実行はappの次回pollで始まる。
    pub fn request(&self, request: IrManualRequest) {
        let mut state = self.lock();
        let generation = match &request {
            // 実行中の書き込みと競合する要求はここで捨てる（UIも無効化している）。
            IrManualRequest::Run(_) if state.running.is_some() => return,
            IrManualRequest::EstimateUpload => {
                state.estimate_generation += 1;
                state.estimate = IrEstimateState::Counting;
                state.estimate_generation
            }
            IrManualRequest::LoadQueue(query) => {
                state.queue_generation += 1;
                state.last_queue_query = Some(query.clone());
                if !matches!(state.queue, IrQueueState::Ready(_)) {
                    state.queue = IrQueueState::Loading;
                }
                state.queue_generation
            }
            IrManualRequest::Run(_) => state.run_generation,
        };
        state.requests.push_back(IrManualQueued { generation, request });
        drop(state);
        self.wake();
    }

    /// 実行中の操作を中断する。処理中の1件は完了させてから止まる。
    pub fn cancel(&self) {
        let mut state = self.lock();
        let Some((_, running)) = state.running.as_mut() else {
            return;
        };
        running.progress.cancel_requested = true;
        if let Some(cancel) = &state.cancel {
            cancel.send_replace(true);
        }
        drop(state);
        self.wake();
    }

    /// 見積もりをやり直せるよう破棄する（確認ダイアログを閉じたとき）。
    pub fn clear_estimate(&self) {
        let mut state = self.lock();
        state.estimate_generation += 1;
        state.estimate = IrEstimateState::Idle;
    }

    pub fn clear_outcome(&self) {
        self.lock().last_outcome = None;
    }

    /// profile切替後に旧profileの状態を全て捨てる。実行中の結果も世代で無効になる。
    pub fn reset_for_profile(&self) {
        let mut state = self.lock();
        state.requests.clear();
        state.run_generation += 1;
        state.estimate_generation += 1;
        state.queue_generation += 1;
        state.running = None;
        state.last_outcome = None;
        state.pending_notice = None;
        state.estimate = IrEstimateState::Idle;
        state.queue = IrQueueState::Idle;
        state.last_queue_query = None;
        if let Some(cancel) = state.cancel.take() {
            cancel.send_replace(true);
        }
    }

    /// 完了通知（toast用）を1度だけ取り出す。
    pub fn take_notice(&self) -> Option<IrManualOutcome> {
        self.lock().pending_notice.take()
    }

    /// 積まれた要求を取り出す。app専用。
    pub fn drain_requests(&self) -> Vec<IrManualQueued> {
        self.lock().requests.drain(..).collect()
    }

    /// アプリ終了時: 中断を要求し、処理中の1件が終わるのを少しだけ待つ。
    /// 待ち切れなくてもjobは `sending` のまま残り、5分のleaseで再取得される（DBは壊れない）。
    pub fn shutdown(&self, wait: Duration) {
        self.cancel();
        let handle = self.shared.thread.lock().unwrap_or_else(|p| p.into_inner()).take();
        let Some(handle) = handle else { return };
        let deadline = Instant::now() + wait;
        while !handle.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if handle.is_finished() {
            let _ = handle.join();
        }
    }

    /// 要求を実行する。app専用。
    pub fn dispatch(&self, env: IrManualEnv, queued: IrManualQueued) {
        let IrManualQueued { generation, request } = queued;
        match request {
            IrManualRequest::Run(op) => self.start_run(env, op),
            IrManualRequest::EstimateUpload => self.start_estimate(env, generation),
            IrManualRequest::LoadQueue(query) => self.start_queue_load(env, query, generation),
        }
    }

    fn start_run(&self, env: IrManualEnv, op: IrManualOp) {
        let (generation, cancel_rx) = {
            let mut state = self.lock();
            if state.running.is_some() {
                return;
            }
            state.run_generation += 1;
            let generation = state.run_generation;
            let (cancel_tx, cancel_rx) = watch::channel(false);
            state.cancel = Some(cancel_tx);
            state.last_outcome = None;
            state.running = Some((
                generation,
                IrManualRunning { op: op.clone(), progress: IrManualProgressView::default() },
            ));
            (generation, cancel_rx)
        };
        self.shared.pause.send_replace(true);
        let handle = self.clone();
        let thread_op = op.clone();
        let spawned = std::thread::Builder::new().name("ir-manual".to_string()).spawn(move || {
            let guard = RunGuard { handle: handle.clone(), generation };
            let outcome = run_op(&handle, generation, &env, &thread_op, &cancel_rx);
            guard.finish(outcome);
        });
        match spawned {
            Ok(join) => *self.shared.thread.lock().unwrap_or_else(|p| p.into_inner()) = Some(join),
            Err(error) => {
                RunGuard { handle: self.clone(), generation }.finish(IrManualOutcome {
                    op,
                    status: IrManualOutcomeStatus::Error(error.to_string()),
                    progress: IrManualProgressView::default(),
                    messages: Vec::new(),
                });
            }
        }
        self.wake();
    }

    fn update_progress(&self, generation: u64, update: impl FnOnce(&mut IrManualProgressView)) {
        {
            let mut state = self.lock();
            let Some((current, running)) = state.running.as_mut() else { return };
            if *current != generation {
                return;
            }
            update(&mut running.progress);
        }
        self.wake();
    }

    fn start_estimate(&self, env: IrManualEnv, generation: u64) {
        let handle = self.clone();
        std::thread::Builder::new()
            .name("ir-manual-estimate".to_string())
            .spawn(move || {
                let result = estimate_upload(&env).map_err(|error| format!("{error:#}"));
                {
                    let mut state = handle.lock();
                    if state.estimate_generation != generation {
                        return;
                    }
                    state.estimate = match result {
                        Ok(estimate) => IrEstimateState::Ready(estimate),
                        Err(error) => IrEstimateState::Failed(error),
                    };
                }
                handle.wake();
            })
            .ok();
    }

    fn start_queue_load(&self, env: IrManualEnv, query: IrQueueQuery, generation: u64) {
        let handle = self.clone();
        std::thread::Builder::new()
            .name("ir-manual-queue".to_string())
            .spawn(move || {
                let result = load_queue(&env, &query).map_err(|error| format!("{error:#}"));
                {
                    let mut state = handle.lock();
                    if state.queue_generation != generation {
                        return;
                    }
                    state.queue = match result {
                        Ok(view) => IrQueueState::Ready(view),
                        Err(error) => IrQueueState::Failed(error),
                    };
                }
                handle.wake();
            })
            .ok();
    }
}

/// 書き込み系スレッドの後始末。panicしても一時停止とrunning状態を解除する。
struct RunGuard {
    handle: IrManualHandle,
    generation: u64,
}

impl RunGuard {
    fn finish(self, outcome: IrManualOutcome) {
        let handle = &self.handle;
        {
            let mut state = handle.lock();
            if state.running.as_ref().is_some_and(|(current, _)| *current == self.generation) {
                state.running = None;
                state.cancel = None;
                state.last_outcome = Some(outcome.clone());
                state.pending_notice = Some(outcome);
                // 完了後は表示中のキュー一覧を最新にする。
                if let Some(query) = state.last_queue_query.clone() {
                    state.queue_generation += 1;
                    let generation = state.queue_generation;
                    state.requests.push_back(IrManualQueued {
                        generation,
                        request: IrManualRequest::LoadQueue(query),
                    });
                }
            }
        }
        // Dropで解除する。ここで二重に解除しても害は無い。
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        let handle = &self.handle;
        let still_running = {
            let mut state = handle.lock();
            // finish されずに落ちた（panic）場合。
            if state.running.as_ref().is_some_and(|(current, _)| *current == self.generation) {
                state.running = None;
                state.cancel = None;
            }
            // 切替で破棄された旧世代が、後続の実行の一時停止を解除しないようにする。
            state.running.is_some()
        };
        if !still_running {
            handle.shared.pause.send_replace(false);
        }
        handle.wake();
    }
}

fn run_op(
    handle: &IrManualHandle,
    generation: u64,
    env: &IrManualEnv,
    op: &IrManualOp,
    cancel: &IrManualCancel,
) -> IrManualOutcome {
    let finish = |status, progress: IrManualProgressView, messages| IrManualOutcome {
        op: op.clone(),
        status,
        progress,
        messages,
    };
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            return finish(
                IrManualOutcomeStatus::Error(error.to_string()),
                IrManualProgressView::default(),
                Vec::new(),
            );
        }
    };
    let mut sink = |event: IrManualProgress| apply_progress(handle, generation, event);
    let result = runtime.block_on(run_op_async(handle, generation, env, op, &mut sink, cancel));
    match result {
        Ok(outcome) => outcome,
        Err(error) => {
            let progress = handle
                .lock()
                .running
                .as_ref()
                .filter(|(current, _)| *current == generation)
                .map(|(_, running)| running.progress.clone())
                .unwrap_or_default();
            finish(IrManualOutcomeStatus::Error(format!("{error:#}")), progress, Vec::new())
        }
    }
}

fn apply_progress(handle: &IrManualHandle, generation: u64, event: IrManualProgress) {
    handle.update_progress(generation, |progress| match event {
        IrManualProgress::FullUploadBatch { batch } => progress.batch = batch,
        IrManualProgress::Enqueued { report, .. } => {
            progress.enqueued = progress.enqueued.saturating_add(report.enqueued);
            progress.missing_replays =
                progress.missing_replays.saturating_add(report.missing_replays);
        }
        IrManualProgress::SyncStarted { limit, .. } => {
            progress.job_limit = limit;
            progress.job_index = 0;
        }
        IrManualProgress::JobSyncing { index, limit } => {
            progress.job_index = index;
            progress.job_limit = limit;
        }
        IrManualProgress::SyncSummary { remaining, .. } => progress.remaining = remaining,
        IrManualProgress::JobFinished { submitted, failed } => {
            progress.submitted = progress.submitted.saturating_add(submitted);
            progress.failed = progress.failed.saturating_add(failed);
        }
        IrManualProgress::DownloadPage(report) => {
            progress.download_pages = report.pages;
            progress.download_scanned = report.scanned;
            progress.download_imported = report.imported;
            progress.download_linked = report.linked_existing;
            progress.download_skipped = report.skipped_existing;
            progress.download_failed = report.failed;
        }
        IrManualProgress::ExistingQueued { .. }
        | IrManualProgress::LimitReachedContinuing
        | IrManualProgress::JobError
        | IrManualProgress::NoQueuedJobs
        | IrManualProgress::FullUploadComplete { .. }
        | IrManualProgress::DownloadStarted { .. } => {}
    });
}

async fn run_op_async(
    handle: &IrManualHandle,
    generation: u64,
    env: &IrManualEnv,
    op: &IrManualOp,
    sink: IrManualProgressSink<'_>,
    cancel: &IrManualCancel,
) -> Result<IrManualOutcome> {
    crate::storage::migration::migrate_network_db(&env.network_db)?;
    let sync_ctx = IrSyncContext {
        score_db: &env.score_db,
        profile_root: &env.profile_root,
        logs_dir: &env.logs_dir,
        ir_config: &env.ir_config,
    };
    let current_progress = || {
        handle
            .lock()
            .running
            .as_ref()
            .filter(|(current, _)| *current == generation)
            .map(|(_, running)| running.progress.clone())
            .unwrap_or_default()
    };
    let outcome = |status, messages| IrManualOutcome {
        op: op.clone(),
        status,
        progress: {
            let mut progress = current_progress();
            progress.waiting_for_lease = false;
            progress
        },
        messages,
    };

    match op {
        IrManualOp::UploadLocalAll => {
            let ctx = IrManualContext {
                library_db: &env.library_db,
                network_db: &env.network_db,
                sync: sync_ctx,
            };
            // include_replay / include_course_stages はCLIの既定値（どちらもfalse）。
            let options = IrLocalUploadOptions::default();
            let mut messages = Vec::new();
            let mut waiting_since = Instant::now();
            // lease待ちから再開しても、この実行で失敗したjobを再送しない。
            let run_started_at = now_unix_seconds();
            loop {
                let report: IrManualUploadReport =
                    upload_local_all(&ctx, &options, run_started_at, &mut *sink, cancel).await?;
                messages.extend(report.messages.iter().cloned());
                match report.stop {
                    IrManualStop::Completed => {
                        return Ok(outcome(IrManualOutcomeStatus::Completed, messages));
                    }
                    IrManualStop::Cancelled => {
                        return Ok(outcome(IrManualOutcomeStatus::Cancelled, messages));
                    }
                    IrManualStop::FailedJobs { failed } => {
                        return Ok(outcome(IrManualOutcomeStatus::FailedJobs { failed }, messages));
                    }
                    IrManualStop::ConsecutiveFailures { consecutive } => {
                        return Ok(outcome(
                            IrManualOutcomeStatus::ConsecutiveFailures { consecutive },
                            messages,
                        ));
                    }
                    IrManualStop::Unauthorized => {
                        return Ok(outcome(IrManualOutcomeStatus::Unauthorized, messages));
                    }
                    IrManualStop::AccountRestricted { banned } => {
                        return Ok(outcome(
                            IrManualOutcomeStatus::AccountRestricted { banned },
                            messages,
                        ));
                    }
                    IrManualStop::RateLimited { .. } => {
                        return Ok(outcome(IrManualOutcomeStatus::RateLimited, messages));
                    }
                    IrManualStop::WaitingForSendingJob => {
                        // 進捗があった試行は待機時間を数え直す。
                        if report.submitted > 0 {
                            waiting_since = Instant::now();
                        }
                        if waiting_since.elapsed() >= LEASE_WAIT_LIMIT {
                            return Ok(outcome(IrManualOutcomeStatus::WaitingTimedOut, messages));
                        }
                        handle.update_progress(generation, |p| p.waiting_for_lease = true);
                        if sleep_or_cancel(LEASE_RETRY_INTERVAL, cancel).await {
                            return Ok(outcome(IrManualOutcomeStatus::Cancelled, messages));
                        }
                        handle.update_progress(generation, |p| p.waiting_for_lease = false);
                    }
                }
            }
        }
        IrManualOp::DownloadScores => {
            // CLIの既定(200件)ではなく全件。重複はDB側で除外されるので再実行しても続きから進む。
            let options =
                IrScoreDownloadOptions { provider: None, limit: u32::MAX, dry_run: false };
            let report: IrScoreDownloadReport =
                download_scores(&sync_ctx, &env.network_db, &options, sink, cancel).await?;
            sink(IrManualProgress::DownloadPage(report.clone()));
            let status = download_status(&report);
            Ok(outcome(status, report.messages))
        }
        IrManualOp::ResendJob { provider_key, account_id, kind, local_score_id } => {
            let mut network_db = NetworkDatabase::open(&env.network_db)?;
            let report = resend_job(
                &sync_ctx,
                &mut network_db,
                provider_key,
                account_id,
                *kind,
                *local_score_id,
            )
            .await?;
            apply_resend(handle, generation, &report);
            Ok(outcome(resend_status(&report), report.messages))
        }
        IrManualOp::ResendFailed { provider_key, account_id } => {
            let mut network_db = NetworkDatabase::open(&env.network_db)?;
            let report = resend_failed_jobs(
                &sync_ctx,
                &mut network_db,
                provider_key,
                account_id,
                &mut *sink,
                cancel,
            )
            .await?;
            // JobFinished の積算と二重にならないよう、skipped_busy だけ反映する。
            handle.update_progress(generation, |p| p.skipped_busy = report.skipped_busy);
            let status = if report.cancelled {
                IrManualOutcomeStatus::Cancelled
            } else {
                IrManualOutcomeStatus::Completed
            };
            Ok(outcome(status, report.messages))
        }
    }
}

fn download_status(report: &IrScoreDownloadReport) -> IrManualOutcomeStatus {
    if report.cancelled {
        IrManualOutcomeStatus::Cancelled
    } else if report.failed > 0 {
        IrManualOutcomeStatus::DownloadFailed { failed: report.failed }
    } else {
        IrManualOutcomeStatus::Completed
    }
}

fn apply_resend(
    handle: &IrManualHandle,
    generation: u64,
    report: &crate::ir::manual::IrManualResendReport,
) {
    handle.update_progress(generation, |p| {
        p.submitted = report.submitted;
        p.failed = report.failed;
        p.skipped_busy = report.skipped_busy;
    });
}

fn resend_status(report: &crate::ir::manual::IrManualResendReport) -> IrManualOutcomeStatus {
    if report.failed > 0 {
        IrManualOutcomeStatus::FailedJobs { failed: report.failed }
    } else {
        IrManualOutcomeStatus::Completed
    }
}

/// 対象のprovider/accountは、実際の一括送信と同じ解決規則（primary）で決める。
fn estimate_upload(env: &IrManualEnv) -> Result<IrUploadEstimate> {
    crate::storage::migration::migrate_network_db(&env.network_db)?;
    let library_db = LibraryDatabase::open(&env.library_db)?;
    let score_db = ScoreDatabase::open(&env.score_db)?;
    let mut network_db = NetworkDatabase::open(&env.network_db)?;
    let options = IrLocalUploadOptions { dry_run: true, ..IrLocalUploadOptions::default() };
    let report = enqueue_local_score_jobs(
        &env.profile_root,
        &env.ir_config,
        &score_db,
        &library_db,
        &mut network_db,
        &options,
        now_unix_seconds(),
    )?;
    let already_queued = network_db.unfinished_ir_score_job_count_for_kind(
        &report.provider_key,
        &report.account_id,
        IrJobKind::Score,
    )?;
    Ok(IrUploadEstimate {
        provider_key: report.provider_key,
        account_id: report.account_id,
        candidates: report.candidates,
        already_queued,
    })
}

fn load_queue(env: &IrManualEnv, query: &IrQueueQuery) -> Result<IrQueueView> {
    crate::storage::migration::migrate_network_db(&env.network_db)?;
    let network_db = NetworkDatabase::open(&env.network_db)?;
    let kinds: &[IrJobKind] = &[];
    let filter = IrScoreJobListFilter {
        provider: Some(&query.provider_key),
        account_id: Some(&query.account_id),
        kinds,
        statuses: query.status.statuses(),
        limit: IR_QUEUE_PAGE_SIZE,
        offset: query.page.saturating_mul(IR_QUEUE_PAGE_SIZE),
    };
    let jobs = network_db.list_ir_score_jobs(&filter)?;
    let total = network_db.count_ir_score_jobs(&filter)?;
    let counts = network_db
        .ir_score_job_status_counts(Some(&query.provider_key), Some(&query.account_id))?;
    // library.db を引けない（未作成など）場合も一覧は出す。曲名はsha256の短縮表示に落ちる。
    let library_db = LibraryDatabase::open(&env.library_db).ok();
    let mut titles: HashMap<[u8; 32], Option<String>> = HashMap::new();
    let rows = jobs
        .into_iter()
        .map(|job| {
            let title = titles
                .entry(job.chart_sha256)
                .or_insert_with(|| {
                    library_db
                        .as_ref()
                        .and_then(|db| db.list_charts_by_sha256(job.chart_sha256).ok())
                        .and_then(|charts| charts.into_iter().next())
                        .map(|chart| {
                            if chart.subtitle.is_empty() {
                                chart.title
                            } else {
                                format!("{} {}", chart.title, chart.subtitle)
                            }
                        })
                })
                .clone();
            IrQueueRow { job, title }
        })
        .collect();
    Ok(IrQueueView { query: query.clone(), counts, total, rows })
}

/// toast・設定画面の結果表示に使う、操作結果の1行説明。
pub fn outcome_message(text: Localizer, outcome: &IrManualOutcome) -> String {
    use crate::i18n::FluentArgs;

    let progress = &outcome.progress;
    let resend =
        matches!(outcome.op, IrManualOp::ResendJob { .. } | IrManualOp::ResendFailed { .. });
    let mut args = FluentArgs::new();
    match &outcome.status {
        IrManualOutcomeStatus::Cancelled if matches!(outcome.op, IrManualOp::DownloadScores) => {
            set_download_args(&mut args, progress);
            text.format("toast-ir-download-cancelled", &args)
        }
        IrManualOutcomeStatus::Cancelled => text.text("toast-ir-manual-cancelled"),
        IrManualOutcomeStatus::DownloadFailed { failed } => {
            args.set("imported", i64::from(progress.download_imported));
            args.set("linked", i64::from(progress.download_linked));
            args.set("failed", i64::from(*failed));
            text.format("toast-ir-download-partial-failure", &args)
        }
        IrManualOutcomeStatus::Error(error) if matches!(outcome.op, IrManualOp::DownloadScores) => {
            args.set("error", error.clone());
            set_download_args(&mut args, progress);
            text.format("toast-ir-download-error", &args)
        }
        IrManualOutcomeStatus::Error(error) => {
            args.set("error", error.clone());
            text.format("toast-ir-manual-error", &args)
        }
        IrManualOutcomeStatus::WaitingTimedOut => text.text("toast-ir-manual-waiting-timeout"),
        IrManualOutcomeStatus::ConsecutiveFailures { consecutive } => {
            args.set("consecutive", i64::from(*consecutive));
            args.set("submitted", i64::from(progress.submitted));
            text.format("toast-ir-manual-consecutive-failures", &args)
        }
        IrManualOutcomeStatus::Unauthorized => text.text("toast-ir-manual-unauthorized"),
        IrManualOutcomeStatus::AccountRestricted { banned: false } => {
            text.text("toast-ir-manual-account-suspended")
        }
        IrManualOutcomeStatus::AccountRestricted { banned: true } => {
            text.text("toast-ir-manual-account-banned")
        }
        IrManualOutcomeStatus::RateLimited => text.text("toast-ir-manual-rate-limited"),
        IrManualOutcomeStatus::FailedJobs { failed } if !resend => {
            args.set("failed", i64::from(*failed));
            text.format("toast-ir-manual-failed-jobs", &args)
        }
        IrManualOutcomeStatus::Completed | IrManualOutcomeStatus::FailedJobs { .. } => {
            match outcome.op {
                IrManualOp::UploadLocalAll => {
                    args.set("submitted", i64::from(progress.submitted));
                    args.set("failed", i64::from(progress.failed));
                    text.format("toast-ir-upload-completed", &args)
                }
                IrManualOp::DownloadScores => {
                    set_download_args(&mut args, progress);
                    text.format("toast-ir-download-completed", &args)
                }
                IrManualOp::ResendJob { .. } | IrManualOp::ResendFailed { .. } => {
                    args.set("submitted", i64::from(progress.submitted));
                    args.set("failed", i64::from(progress.failed));
                    text.format("toast-ir-resend-completed", &args)
                }
            }
        }
    }
}

fn set_download_args(args: &mut crate::i18n::FluentArgs, progress: &IrManualProgressView) {
    args.set("imported", i64::from(progress.download_imported));
    args.set("linked", i64::from(progress.download_linked));
    args.set("failed", i64::from(progress.download_failed));
}

#[cfg(test)]
mod tests;
