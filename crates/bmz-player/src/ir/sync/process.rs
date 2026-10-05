use super::*;

pub(super) async fn sync_pending_ir_jobs_with_filter(
    network_db: &mut NetworkDatabase,
    score_db_path: &Path,
    profile_root: &Path,
    logs_dir: &Path,
    ir_config: &IrConfig,
    now: i64,
    limit: u32,
    ignore_retry_backoff: bool,
    throttle: IrSyncThrottle,
    filter: Option<IrSyncJobFilter<'_>>,
) -> Result<IrSyncReport> {
    let mut report = IrSyncReport::default();
    let jobs = match filter {
        Some(IrSyncJobFilter {
            provider_key,
            account_id,
            kind,
            local_score_id: Some(local_score_id),
        }) => network_db.claim_pending_ir_score_job_for_local_score(
            provider_key,
            account_id,
            kind,
            local_score_id,
            now,
            ignore_retry_backoff,
        )?,
        Some(IrSyncJobFilter { provider_key, account_id, kind, local_score_id: None }) => {
            network_db.claim_pending_ir_score_jobs_for_kind(
                provider_key,
                account_id,
                kind,
                now,
                limit,
                ignore_retry_backoff,
            )?
        }
        None => network_db.claim_pending_ir_score_jobs(now, limit, ignore_retry_backoff)?,
    };
    let job_count = jobs.len();
    let replay_paths = match replay_paths_for_jobs(score_db_path, &jobs) {
        Ok(paths) => paths,
        Err(error) => {
            let message = format!("failed to resolve replay paths: {error:#}");
            for job in &jobs {
                network_db.mark_ir_score_job_failed(job.id, now, &message, None)?;
            }
            return Err(error);
        }
    };
    let batch_started = std::time::Instant::now();
    for (index, job) in jobs.into_iter().enumerate() {
        let job_now = now.saturating_add(batch_started.elapsed().as_secs() as i64);
        let Some(provider) = provider_config(ir_config, &job.provider) else {
            network_db.mark_ir_score_job_failed(
                job.id,
                job_now,
                "provider is not configured",
                None,
            )?;
            report.failed += 1;
            report
                .messages
                .push(format!("job {}: provider '{}' not configured", job.id, job.provider));
            continue;
        };
        // Keep the checked token for this job: loading it again during submission
        // could pick up a different account after a concurrent login.
        let credentials =
            match credentials_for_job(profile_root, provider, &job.account_id, job_now).await {
                Ok(credentials) => credentials,
                Err(error) => {
                    let message = format!("{error:#}");
                    network_db.mark_ir_score_job_failed(job.id, job_now, &message, None)?;
                    report.failed += 1;
                    report.messages.push(format!("job {}: {message}", job.id));
                    continue;
                }
            };
        let include_ranking = job.kind == IrJobKind::Score
            && score_submission_includes_ranking(ir_config, &job.provider);
        let submitted = submit_ir_job(
            profile_root,
            provider,
            &job,
            replay_paths.get(&job.id).and_then(Option::as_deref),
            credentials,
            include_ranking,
        )
        .await;
        let mut completion =
            JobCompletion { network_db, report: &mut report, logs_dir, job: &job, now: job_now };
        match submitted {
            Ok(submitted) => completion.complete(submitted)?,
            Err(error) => completion.fail(error, FailureStage::Submission)?,
        }
        if index + 1 < job_count
            && let Some(delay) = throttle.job_delay()
        {
            tokio::time::sleep(delay).await;
        }
    }
    let finished_at = now.saturating_add(batch_started.elapsed().as_secs() as i64);
    let pruned = network_db.prune_succeeded_ir_score_jobs(finished_at)?;
    if pruned > 0 {
        tracing::debug!(pruned, "pruned succeeded IR score jobs");
    }
    Ok(report)
}
