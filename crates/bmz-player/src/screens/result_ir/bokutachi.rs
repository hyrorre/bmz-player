use std::path::Path;

use tokio::sync::{oneshot, watch};

use super::task::{
    fetch_ranking_and_send, fetch_result_ranking, now_unix_seconds, submission_result_from_jobs,
};
use super::*;
use crate::storage::network_db::{
    IR_JOB_NO_AUTO_RETRY_AT, IR_JOB_SENDING_STALE_AFTER_SECONDS, IrJobRetryFilter, NetworkDatabase,
};

const RANKING_RETRY_DELAYS: [u64; 3] = [2, 4, 8];

pub(super) fn spawn(
    network_db_path: &Path,
    score_db_path: &Path,
    logs_dir: &Path,
    ir_config: &IrConfig,
    query: &ResultIrTaskQuery,
    targets: &[ResultIrSubmissionTarget],
    sender: &Sender<ResultIrEvent>,
    fetch_ranking: bool,
) -> Option<watch::Sender<()>> {
    let (lifetime, closed) = watch::channel(());
    let mut started = false;
    for target in targets.iter().filter(|t| crate::ir::bokutachi::is_provider(&t.provider)) {
        started = true;
        let submission = Submission {
            network_db_path: network_db_path.into(),
            score_db_path: score_db_path.into(),
            logs_dir: logs_dir.into(),
            ir_config: ir_config.clone(),
            query: query.clone(),
            target: target.clone(),
            sender: sender.clone(),
        };
        let (confirmed_tx, confirmed_rx) = oneshot::channel();
        if fetch_ranking
            && target.provider == query.provider
            && target.account_id == query.account_id
        {
            let (query, sender, mut closed) = (query.clone(), sender.clone(), closed.clone());
            tokio::spawn(async move {
                tokio::select! {
                    _ = closed.changed() => {}
                    _ = update_ranking(&query, &sender, confirmed_rx) => {}
                }
            });
        }
        let closed = closed.clone();
        tokio::spawn(async move {
            match submission.follow(closed).await {
                Ok(true) => {
                    let _ = confirmed_tx.send(());
                }
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(%error, "failed to follow Bokutachi submission from Result");
                    let _ = submission.sender.send(ResultIrEvent::Submit {
                        provider: submission.target.provider.clone(),
                        submitted: 0,
                        failed: 1,
                        message: Some(format!("{error:#}")),
                        status: None,
                    });
                }
            }
        });
    }
    started.then_some(lifetime)
}

struct Submission {
    network_db_path: PathBuf,
    score_db_path: PathBuf,
    logs_dir: PathBuf,
    ir_config: IrConfig,
    query: ResultIrTaskQuery,
    target: ResultIrSubmissionTarget,
    sender: Sender<ResultIrEvent>,
}

impl Submission {
    async fn follow(&self, mut closed: watch::Receiver<()>) -> anyhow::Result<bool> {
        let (kind, local_score_id) = self.query.target.submission_job();
        let mut displayed_accepted = false;
        loop {
            if closed.has_changed().is_err() {
                return Ok(false);
            }
            let mut db = NetworkDatabase::open(&self.network_db_path)?;
            let job =
                db.ir_score_jobs_for_local_score(kind, local_score_id)?.into_iter().find(|job| {
                    job.provider == self.target.provider && job.account_id == self.target.account_id
                });
            let Some(job) = job else {
                self.publish_result(&[]);
                return Ok(false);
            };
            let status = IrScoreJobStatus::parse(&job.status);
            // Publish receipt state while another worker may still own the lease.
            let accepted_import = kind == IrJobKind::Score
                && awaiting_submission(status, job.next_attempt_at, true)
                && crate::ir::bokutachi::sync::has_accepted_import(&db, job.id)?;
            if !awaiting_submission(status, job.next_attempt_at, accepted_import) {
                self.publish_result(&[job]);
                return Ok(status == Some(IrScoreJobStatus::Succeeded));
            }
            if accepted_import && !displayed_accepted {
                if self
                    .sender
                    .send(ResultIrEvent::BokutachiAccepted {
                        provider: self.target.provider.clone(),
                    })
                    .is_err()
                {
                    return Ok(false);
                }
                displayed_accepted = true;
            }
            let now = now_unix_seconds();
            let due = if status == Some(IrScoreJobStatus::Sending) {
                job.updated_at.saturating_add(IR_JOB_SENDING_STALE_AFTER_SECONDS)
            } else {
                job.next_attempt_at
            };
            if due <= now {
                // Finish an in-flight claim even if the Result closes. Cancelling
                // after the POST could leave an accepted import without a receipt.
                let report = sync_pending_ir_jobs_filtered(
                    &mut db,
                    &self.score_db_path,
                    &self.query.profile_root,
                    &self.logs_dir,
                    &self.ir_config,
                    IrSyncJobFilter {
                        provider_key: &self.target.provider,
                        account_id: &self.target.account_id,
                        kind,
                        local_score_id: Some(local_score_id),
                    },
                    now,
                    1,
                    IrJobRetryFilter::RespectBackoff,
                    IrSyncThrottle::none(),
                )
                .await?;
                if report.processed > 0 {
                    drop(db);
                    tokio::task::yield_now().await;
                    continue;
                }
            }
            drop(db);
            tokio::select! {
                _ = closed.changed() => return Ok(false),
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }

    fn publish_result(&self, jobs: &[IrScoreJobRecord]) {
        let Some((submitted, failed, message)) = submission_result_from_jobs(jobs) else {
            return;
        };
        let status = jobs.first().and_then(|job| IrScoreJobStatus::parse(&job.status));
        let _ = self.sender.send(ResultIrEvent::Submit {
            provider: self.target.provider.clone(),
            submitted,
            failed,
            message,
            status,
        });
    }
}

fn awaiting_submission(
    status: Option<IrScoreJobStatus>,
    next_attempt_at: i64,
    accepted: bool,
) -> bool {
    matches!(status, Some(IrScoreJobStatus::Pending | IrScoreJobStatus::Sending))
        || (status == Some(IrScoreJobStatus::Failed)
            && accepted
            && next_attempt_at < IR_JOB_NO_AUTO_RETRY_AT)
}

async fn update_ranking(
    query: &ResultIrTaskQuery,
    sender: &Sender<ResultIrEvent>,
    mut confirmed: oneshot::Receiver<()>,
) {
    let scope = IrRankingScope::Global;
    let initial = fetch_ranking_and_send(query, scope, sender);
    tokio::pin!(initial);
    let registered = tokio::select! {
        biased;
        _ = &mut initial => confirmed.await.is_ok(),
        result = &mut confirmed => {
            if result.is_err() {
                initial.await;
            }
            result.is_ok()
        }
    };
    if !registered {
        return;
    }

    for attempt in 0..=RANKING_RETRY_DELAYS.len() {
        match fetch_result_ranking(query, scope).await {
            Ok(ranking) => {
                let _ = sender.send(ResultIrEvent::Ranking {
                    provider: query.provider.clone(),
                    scope,
                    result: Ok(ranking),
                });
                return;
            }
            Err(error) => {
                let Some(delay) = RANKING_RETRY_DELAYS.get(attempt) else {
                    let _ = sender.send(ResultIrEvent::Ranking {
                        provider: query.provider.clone(),
                        scope,
                        result: Err(format!("{error:#}")),
                    });
                    return;
                };
                tracing::debug!(%error, attempt, "retrying Bokutachi ranking refresh");
                let retry_after = crate::ir::bmz_official::retry_after_seconds_from_error(&error);
                tokio::time::sleep(Duration::from_secs(retry_after.unwrap_or(0).max(*delay))).await;
            }
        }
    }
}
