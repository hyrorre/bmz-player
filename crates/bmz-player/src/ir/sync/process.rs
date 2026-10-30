use super::*;

pub(super) async fn sync_pending_ir_jobs_with_filter(
    network_db: &mut NetworkDatabase,
    score_db_path: &Path,
    profile_root: &Path,
    logs_dir: &Path,
    ir_config: &IrConfig,
    now: i64,
    limit: u32,
    retry: IrJobRetryFilter,
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
            retry != IrJobRetryFilter::RespectBackoff,
        )?,
        Some(IrSyncJobFilter { provider_key, account_id, kind, local_score_id: None }) => {
            network_db.claim_pending_ir_score_jobs_for_kind(
                provider_key,
                account_id,
                kind,
                now,
                limit,
                retry,
            )?
        }
        None => network_db.claim_pending_ir_score_jobs(
            now,
            limit,
            retry != IrJobRetryFilter::RespectBackoff,
        )?,
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
    // アカウント停止・BANを返したアカウント。同じバッチの残りのjobは送らずに拒否として記録する
    // （送っても同じ理由で拒否され、refreshやログインを繰り返すだけになる）。
    let mut restricted_accounts: Vec<(String, String, crate::ir::IrModerationCode, String)> =
        Vec::new();
    for (index, job) in jobs.into_iter().enumerate() {
        let job_now = now.saturating_add(batch_started.elapsed().as_secs() as i64);
        if let Some((_, _, code, message)) =
            restricted_accounts.iter().find(|(provider, account, _, _)| {
                *provider == job.provider && *account == job.account_id
            })
        {
            network_db.mark_ir_score_job_rejected(job.id, job_now, message)?;
            report.record_failure(
                job.id,
                IrSyncFailureClass::Moderated(*code),
                format!("job {}: {message}", job.id),
            );
            continue;
        }
        let Some(provider) = provider_config(ir_config, &job.provider) else {
            network_db.mark_ir_score_job_failed(
                job.id,
                job_now,
                "provider is not configured",
                None,
            )?;
            report.record_failure(
                job.id,
                IrSyncFailureClass::Unauthorized,
                format!("job {}: provider '{}' not configured", job.id, job.provider),
            );
            continue;
        };
        // Keep the checked token for this job: loading it again during submission
        // could pick up a different account after a concurrent login.
        let credentials =
            match credentials_for_job(profile_root, provider, &job.account_id, job_now).await {
                Ok(credentials) => credentials,
                Err(error) => {
                    let message = format!("{error:#}");
                    // token refresh がアカウントのBANで拒否された場合は再ログインを促さない。
                    let class = match crate::ir::http_error::moderation_code_from_error(&error) {
                        Some(code) => IrSyncFailureClass::Moderated(code),
                        None => IrSyncFailureClass::Unauthorized,
                    };
                    if let IrSyncFailureClass::Moderated(code) = class {
                        network_db.mark_ir_score_job_rejected(job.id, job_now, &message)?;
                        restricted_accounts.push((
                            job.provider.clone(),
                            job.account_id.clone(),
                            code,
                            message.clone(),
                        ));
                    } else {
                        network_db.mark_ir_score_job_failed(job.id, job_now, &message, None)?;
                    }
                    report.record_failure(job.id, class, format!("job {}: {message}", job.id));
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
            Err(error) => {
                let account_restriction = crate::ir::http_error::moderation_code_from_error(&error)
                    .filter(|code| code.is_account_level())
                    .map(|code| (code, format!("{error:#}")));
                completion.fail(error, FailureStage::Submission)?;
                if let Some((code, message)) = account_restriction {
                    restricted_accounts.push((
                        job.provider.clone(),
                        job.account_id.clone(),
                        code,
                        message,
                    ));
                }
            }
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
