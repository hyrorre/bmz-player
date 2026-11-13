use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::BokutachiClient;
use super::client::{PollResult, ResolvedChart};
use super::convert::{ImportRequest, context, convert};
use crate::config::profile_config::IrProviderConfig;
use crate::ir::sync::{IrSyncFailureClass, IrSyncReport};
use crate::ir::types::IrScoreSubmission;
use crate::storage::network_db::{
    IrJobKind, IrScoreJobRecord, NetworkDatabase, NewIrScoreSubmission,
};

#[derive(Debug, Default, Serialize, Deserialize)]
struct Progress {
    origin: String,
    account_id: String,
    submission_key: String,
    resolved: Option<ResolvedChart>,
    request: Option<ImportRequest>,
    import_id: Option<String>,
    accepted_at: Option<i64>,
    phase: String,
    #[serde(default)]
    retired_import_ids: Vec<String>,
    #[serde(default)]
    import_failure: Option<String>,
}

pub(crate) async fn process(
    db: &mut NetworkDatabase,
    profile_root: &Path,
    provider: &IrProviderConfig,
    job: &IrScoreJobRecord,
    now: i64,
    explicit_resend: bool,
    report: &mut IrSyncReport,
) -> Result<()> {
    let result = step(db, profile_root, provider, job, now, explicit_resend, report).await;
    if let Err(error) = result {
        if error.downcast_ref::<super::client::IneligibleChart>().is_some() {
            let message = format!("{error:#}");
            db.reject_bokutachi_chart(job.id, now, &message)?;
            report.messages.push(message);
            return Ok(());
        }
        let status = crate::ir::http_error::status_code_from_error(&error);
        let message = if status.is_some_and(|s| matches!(s.as_u16(), 401 | 403)) {
            format!("{error:#}; check the API key and submit_score permission in IR settings")
        } else {
            format!("{error:#}")
        };
        let transient = error.chain().any(|e| e.is::<reqwest::Error>())
            || status
                .is_some_and(|s| s.is_server_error() || s == reqwest::StatusCode::REQUEST_TIMEOUT);
        let class = if status == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
            let delay = crate::ir::http_error::retry_after_seconds_from_error(&error)
                .unwrap_or(60 + (job.id.unsigned_abs() % 7));
            let origin = crate::ir::credentials::credential_origin(&provider.base_url)?;
            db.defer_bokutachi_origin(
                &origin,
                now.saturating_add(i64::try_from(delay).unwrap_or(i64::MAX)),
            )?;
            db.mark_ir_score_job_failed(job.id, now, &message, Some(delay))?;
            IrSyncFailureClass::RateLimited { retry_after_seconds: Some(delay) }
        } else if transient {
            db.mark_ir_score_job_failed(job.id, now, &message, None)?;
            IrSyncFailureClass::Transient
        } else {
            // Terminal until explicit retry; import IDs and prepared requests remain intact.
            db.mark_ir_score_job_rejected(job.id, now, &message)?;
            if status.is_some_and(|s| matches!(s.as_u16(), 401 | 403)) {
                IrSyncFailureClass::Unauthorized
            } else {
                IrSyncFailureClass::Job
            }
        };
        report.record_failure(job.id, class, message);
    }
    Ok(())
}

async fn step(
    db: &mut NetworkDatabase,
    root: &Path,
    provider: &IrProviderConfig,
    job: &IrScoreJobRecord,
    now: i64,
    explicit_resend: bool,
    report: &mut IrSyncReport,
) -> Result<()> {
    if job.kind == IrJobKind::Course {
        return super::dan::step(db, root, provider, job, now, report).await;
    }
    ensure!(job.kind == IrJobKind::Score, "{}", super::unsupported());
    let payload: IrScoreSubmission =
        serde_json::from_str(&job.payload_json).context("Bokutachi: invalid stored play")?;
    let ctx = super::convert::saved_context(&payload)?;
    ensure!(
        ctx.origin == crate::ir::credentials::credential_origin(&provider.base_url)?,
        "Bokutachi: job belongs to another origin"
    );
    let mut progress: Progress = match db.bokutachi_progress(job.id)? {
        Some(json) => {
            serde_json::from_str(&json).context("Bokutachi: invalid saved import progress")?
        }
        None => Progress {
            origin: ctx.origin.clone(),
            account_id: job.account_id.clone(),
            submission_key: payload.idempotency_key.clone(),
            phase: "resolve".into(),
            ..Progress::default()
        },
    };
    ensure!(
        progress.origin == ctx.origin
            && progress.account_id == job.account_id
            && progress.submission_key == payload.idempotency_key,
        "Bokutachi: saved import account/origin differs"
    );
    let receipt_path = root.join("ir").join("bokutachi-receipts").join(format!("{}.json", job.id));
    // An fsynced receipt also recovers an accepted POST if SQLite completion failed.
    if progress.import_id.is_none() && receipt_path.exists() {
        let receipt: Progress = serde_json::from_slice(&std::fs::read(&receipt_path)?)?;
        ensure!(
            receipt.origin == ctx.origin
                && receipt.account_id == job.account_id
                && receipt.import_id.is_some()
                && receipt.submission_key == payload.idempotency_key,
            "Bokutachi: invalid recovery receipt"
        );
        if !progress.retired_import_ids.contains(receipt.import_id.as_ref().unwrap()) {
            progress = receipt;
            db.save_bokutachi_progress(job.id, &serde_json::to_string(&progress)?)?;
        }
    }
    if progress.phase == "poll_failed" {
        context(&payload)?;
        ensure!(
            explicit_resend,
            "Bokutachi: import failed; select this job and resend to restart registration: {}",
            progress.import_failure.as_deref().unwrap_or("unknown import failure")
        );
        let credentials =
            crate::ir::credentials::load_credentials(root, &job.provider, &ctx.origin)?
                .context("Bokutachi: validate the API key before restarting registration")?;
        ensure!(
            credentials.account_id == job.account_id,
            "Bokutachi: restore this account's API key"
        );
        let import_id = progress.import_id.take().context("Bokutachi: missing failed import ID")?;
        progress.retired_import_ids.push(import_id);
        progress.accepted_at = None;
        progress.phase = "prepared".into();
        // Commit the retired ID before removing its receipt or issuing a POST.
        // A failed unlink must never recover that obsolete receipt on restart.
        db.save_bokutachi_progress(job.id, &serde_json::to_string(&progress)?)?;
        let _ = std::fs::remove_file(&receipt_path);
    }
    let client = BokutachiClient::new(&ctx.origin)?;
    if let Some(import_id) = &progress.import_id {
        let poll = match client.poll(import_id, &job.account_id).await {
            Err(error)
                if crate::ir::http_error::status_code_from_error(&error)
                    == Some(reqwest::StatusCode::NOT_FOUND)
                    && now.saturating_sub(progress.accepted_at.unwrap_or(0)) < 120 =>
            {
                db.defer_bokutachi_job(
                    job.id,
                    now,
                    now.saturating_add(10),
                    "Bokutachi: accepted; waiting for import status",
                )?;
                return Ok(());
            }
            result => result.context(
                "Bokutachi: import confirmation failed; the accepted import ID is retained",
            )?,
        };
        match poll {
            PollResult::Failed { status: 404, .. }
                if now.saturating_sub(progress.accepted_at.unwrap_or(0)) < 120 =>
            {
                db.defer_bokutachi_job(
                    job.id,
                    now,
                    now.saturating_add(10),
                    "Bokutachi: accepted; waiting for import status",
                )?;
            }
            PollResult::Failed { status, description } => {
                let message = format!(
                    "Bokutachi: import failed ({status}): {description}; select this job and resend to restart registration"
                );
                progress.phase = "poll_failed".into();
                progress.import_failure = Some(message.clone());
                db.save_bokutachi_progress(job.id, &serde_json::to_string(&progress)?)?;
                db.mark_ir_score_job_rejected(job.id, now, &message)?;
                report.record_failure(job.id, IrSyncFailureClass::Job, message);
            }
            PollResult::Ongoing => {
                db.defer_bokutachi_job(
                    job.id,
                    now,
                    now.saturating_add(10),
                    "Bokutachi: accepted; registration is still in progress",
                )?;
                report.messages.push("Bokutachi: registration pending".into());
            }
            PollResult::Completed { score_id, errors } => {
                let message = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.kind, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                progress.phase = "completed".into();
                db.save_bokutachi_progress(job.id, &serde_json::to_string(&progress)?)?;
                // Keep the receipt/import ID until the transaction below succeeds.
                if let Some(remote_score_id) = score_id {
                    db.complete_ir_score_job(
                        &NewIrScoreSubmission {
                            job_id: job.id,
                            provider: job.provider.clone(),
                            account_id: job.account_id.clone(),
                            kind: job.kind,
                            local_score_id: job.local_score_id,
                            remote_score_id,
                            status: "succeeded".into(),
                            submitted_at: now,
                            log_path: String::new(),
                            error: message.clone(),
                        },
                        None,
                        &serde_json::to_string(&errors)?,
                    )?;
                    report.submitted += 1;
                    if !message.is_empty() {
                        report
                            .messages
                            .push(format!("Bokutachi: registered with warnings: {message}"));
                    }
                } else if errors.is_empty() {
                    let message =
                        "Bokutachi: no new score was confirmed (duplicates or other reasons)";
                    db.finish_bokutachi_without_score(job.id, now, message)?;
                    report.messages.push(message.into());
                } else {
                    db.mark_ir_score_job_rejected(job.id, now, &message)?;
                    report.record_failure(job.id, IrSyncFailureClass::Job, message);
                }
                let _ = std::fs::remove_file(&receipt_path);
            }
        }
        return Ok(());
    }
    context(&payload)?;
    let credentials = crate::ir::credentials::load_credentials(root, &job.provider, &ctx.origin)?
        .context("Bokutachi: configure and validate the API key to resume")?;
    ensure!(
        credentials.account_id == job.account_id,
        "Bokutachi: this job belongs to another account; restore that account's API key"
    );
    if let Some(next) = db.bokutachi_origin_wait(&ctx.origin, now)? {
        db.defer_bokutachi_job(
            job.id,
            now,
            next,
            "Bokutachi: waiting for the submission rate limit",
        )?;
        report.messages.push("Bokutachi: registration pending (rate limit)".into());
        return Ok(());
    }
    if progress.request.is_none() {
        let resolved = client.resolve(ctx.game, &payload.chart.sha256, payload.result.notes).await
            .map_err(|error| {
                if crate::ir::http_error::status_code_from_error(&error) == Some(reqwest::StatusCode::NOT_FOUND) {
                    anyhow::Error::new(super::client::IneligibleChart("Bokutachi: chart is not registered; retry after chart registration".into()))
                } else { error }
            })
            .context("Bokutachi: chart verification stopped submission; retry manually after chart registration or correction")?;
        progress.request = Some(convert(&payload, resolved.matcher.clone())?);
        progress.resolved = Some(resolved);
        progress.phase = "prepared".into();
        // Freeze the resolved PMS song ID and wire payload before any import POST.
        db.save_bokutachi_progress(job.id, &serde_json::to_string(&progress)?)?;
    }
    if let Some(next) = db.reserve_bokutachi_post(&ctx.origin, now)? {
        db.defer_bokutachi_job(
            job.id,
            now,
            next,
            "Bokutachi: waiting for the submission rate limit",
        )?;
        report.messages.push("Bokutachi: registration pending (rate limit)".into());
        return Ok(());
    }
    let request = progress.request.as_ref().context("Bokutachi: missing prepared request")?;
    let resolved = progress.resolved.as_ref().context("Bokutachi: missing verified chart")?;
    ensure!(
        serde_json::to_value(request)?
            == serde_json::to_value(convert(&payload, resolved.matcher.clone())?)?,
        "Bokutachi: prepared request differs from the saved play; submission stopped"
    );
    let id = client.import(&credentials.access_token, request).await?;
    progress.import_id = Some(id);
    progress.accepted_at = Some(now);
    progress.phase = "poll".into();
    let json = serde_json::to_string(&progress)?;
    let receipt_result = (|| -> Result<()> {
        std::fs::create_dir_all(receipt_path.parent().context("receipt directory")?)?;
        use std::io::Write;
        let temporary = receipt_path.with_extension("tmp");
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(temporary, &receipt_path)?;
        Ok(())
    })();
    db.save_bokutachi_progress(job.id, &json)
        .context("Bokutachi: failed to save accepted import; recovery receipt retained")?;
    if let Err(error) = receipt_result {
        tracing::warn!(%error, "Bokutachi receipt backup failed; accepted import saved in database");
    }
    db.defer_bokutachi_job(
        job.id,
        now,
        now.saturating_add(5),
        "Bokutachi: accepted; awaiting registration confirmation",
    )?;
    report.messages.push("Bokutachi: registration pending".into());
    Ok(())
}
