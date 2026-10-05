use super::*;

pub(super) struct SubmissionReceipt {
    pub(super) remote_score_id: String,
    pub(super) request_json: String,
    pub(super) response_json: String,
}

pub(super) enum SubmittedIrJob {
    Replay,
    Attestation(SubmissionReceipt),
    Score { request_json: String, response_json: String, include_ranking: bool },
}

pub(super) enum FailureStage<'a> {
    Submission,
    Completion(&'a SubmissionReceipt),
}

/// Checked credentials are consumed once by the selected submission path.
pub(super) async fn submit_ir_job(
    profile_root: &Path,
    provider: &IrProviderConfig,
    job: &IrScoreJobRecord,
    replay_path: Option<&str>,
    credentials: IrStoredCredentials,
    include_ranking: bool,
) -> Result<SubmittedIrJob> {
    match job.kind {
        IrJobKind::Replay => {
            submit_replay_job(
                profile_root,
                provider,
                &job.payload_json,
                replay_path,
                job.local_score_id,
                credentials,
            )
            .await?;
            Ok(SubmittedIrJob::Replay)
        }
        IrJobKind::Attestation => {
            let (remote_score_id, request_json, response_json) = submit_score_attestation_job(
                profile_root,
                provider,
                &job.payload_json,
                credentials,
            )
            .await?;
            Ok(SubmittedIrJob::Attestation(SubmissionReceipt {
                remote_score_id,
                request_json,
                response_json,
            }))
        }
        IrJobKind::Score | IrJobKind::Course => {
            let (request_json, response_json) = if job.kind == IrJobKind::Score {
                submit_job_payload(
                    profile_root,
                    provider,
                    &job.payload_json,
                    credentials,
                    include_ranking,
                )
                .await?
            } else {
                submit_course_job_payload(profile_root, provider, &job.payload_json, credentials)
                    .await?
            };
            Ok(SubmittedIrJob::Score { request_json, response_json, include_ranking })
        }
    }
}

/// Persistence and diagnostics for one claimed job. Network submission has no DB access.
pub(super) struct JobCompletion<'a> {
    pub(super) network_db: &'a mut NetworkDatabase,
    pub(super) report: &'a mut IrSyncReport,
    pub(super) logs_dir: &'a Path,
    pub(super) job: &'a IrScoreJobRecord,
    pub(super) now: i64,
}

impl JobCompletion<'_> {
    pub(super) fn fail(&mut self, error: anyhow::Error, stage: FailureStage<'_>) -> Result<()> {
        let (prefix, event, receipt) = match stage {
            FailureStage::Completion(receipt) => {
                ("failed to complete IR score job: ", "IR score completion failed", Some(receipt))
            }
            FailureStage::Submission => match self.job.kind {
                IrJobKind::Replay => ("replay upload failed: ", "IR replay upload failed", None),
                IrJobKind::Attestation => {
                    ("score attestation failed: ", "IR score attestation failed", None)
                }
                IrJobKind::Score | IrJobKind::Course => ("", "IR score submission failed", None),
            },
        };
        let message = format!("{prefix}{error:#}");
        let (remote_id, request, response) =
            receipt.map_or(("", self.job.payload_json.as_str(), ""), |receipt| {
                (
                    receipt.remote_score_id.as_str(),
                    receipt.request_json.as_str(),
                    receipt.response_json.as_str(),
                )
            });
        let _ = write_ir_submission_log(
            self.logs_dir,
            self.job,
            "failed",
            remote_id,
            self.now,
            request,
            response,
            &message,
        );
        self.network_db.mark_ir_score_job_failed(
            self.job.id,
            self.now,
            &message,
            retry_after_seconds_from_error(&error),
        )?;
        self.report.failed += 1;
        self.report.messages.push(format!("job {}: {message}", self.job.id));
        tracing::warn!(job_id = self.job.id, provider = self.job.provider, %message, "{event}");
        Ok(())
    }

    pub(super) fn complete(&mut self, submitted: SubmittedIrJob) -> Result<()> {
        match submitted {
            SubmittedIrJob::Replay => self.mark_succeeded()?,
            SubmittedIrJob::Attestation(receipt) => {
                self.log_success(&receipt);
                self.mark_succeeded()?;
            }
            SubmittedIrJob::Score { request_json, response_json, include_ranking } => {
                let parsed =
                    serde_json::from_str::<crate::ir::types::IrSubmitResponse>(&response_json).ok();
                if include_ranking
                    && let Some(ranking_response) = parsed
                        .as_ref()
                        .and_then(|response| response.rankings.get(&IrRankingScope::Global))
                        .filter(|ranking| ranking.succeeded)
                    && let Some(ranking) = ranking_response.data.clone()
                {
                    self.report.included_rankings.push(IrIncludedRanking {
                        provider: self.job.provider.clone(),
                        account_id: self.job.account_id.clone(),
                        kind: self.job.kind,
                        local_score_id: self.job.local_score_id,
                        previous_rank: ranking_response.previous_rank,
                        ranking,
                    });
                }
                let remote_score_id = parsed
                    .as_ref()
                    .and_then(|response| response.score_id.clone())
                    .or_else(|| {
                        serde_json::from_str::<serde_json::Value>(&response_json).ok().and_then(
                            |value| value.get("course_score_id")?.as_str().map(str::to_string),
                        )
                    })
                    .unwrap_or_default();
                let receipt = SubmissionReceipt { remote_score_id, request_json, response_json };
                // Completing the score and scheduling its replay stay in one DB transaction.
                if let Err(error) = self.complete_score(&receipt) {
                    return self.fail(error, FailureStage::Completion(&receipt));
                }
            }
        }
        self.report.submitted += 1;
        Ok(())
    }

    fn mark_succeeded(&mut self) -> Result<()> {
        self.network_db.mark_ir_score_job_status(
            self.job.id,
            IrScoreJobStatus::Succeeded,
            self.now,
            "",
        )
    }

    fn log_success(&self, receipt: &SubmissionReceipt) -> String {
        write_ir_submission_log(
            self.logs_dir,
            self.job,
            "succeeded",
            &receipt.remote_score_id,
            self.now,
            &receipt.request_json,
            &receipt.response_json,
            "",
        )
    }

    fn complete_score(&mut self, receipt: &SubmissionReceipt) -> Result<()> {
        let replay_job = replay_job_for_score(self.job, &receipt.remote_score_id, self.now)?;
        let log_path = self.log_success(receipt);
        self.network_db.complete_ir_score_job(
            &NewIrScoreSubmission {
                job_id: self.job.id,
                provider: self.job.provider.clone(),
                account_id: self.job.account_id.clone(),
                kind: self.job.kind,
                local_score_id: self.job.local_score_id,
                remote_score_id: receipt.remote_score_id.clone(),
                status: "succeeded".to_string(),
                submitted_at: self.now,
                log_path,
                error: String::new(),
            },
            replay_job.as_ref(),
            &receipt.response_json,
        )
    }
}

#[cfg(test)]
mod tests;
