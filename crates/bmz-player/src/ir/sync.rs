use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::sync::Mutex;

use crate::config::profile_config::{IrConfig, IrProviderConfig};
use crate::storage::network_db::{
    IrJobKind, IrScoreJobRecord, IrScoreJobStatus, NetworkDatabase, NewIrScoreJob,
    NewIrScoreSubmission,
};
use crate::storage::score_db::ScoreDatabase;

use super::bmz_official::{BmzOfficialIrClient, retry_after_seconds_from_error};
use super::credentials::{IrStoredCredentials, load_credentials, save_credentials};
use super::types::{IrRankingResult, IrRankingScope, IrScoreSubmission, IrSubmitOptions};

static CREDENTIAL_REFRESH_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Default, Clone)]
pub struct IrSyncReport {
    pub submitted: u32,
    pub failed: u32,
    pub messages: Vec<String>,
    pub included_rankings: Vec<IrRankingResult>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct IrReplayJobPayload {
    remote_score_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct IrScoreAttestationJobPayload {
    remote_score_id: String,
}

pub const IR_SYNC_BATCH_LIMIT: u32 = 20;
pub const IR_SYNC_JOB_SPACING_MS: u64 = 3_100;
/// 手動の `ir sync` / local backfill 用。結果画面・常駐同期の待機時間とは分ける。
pub const IR_CLI_SYNC_BATCH_LIMIT: u32 = 100;
pub const IR_CLI_SYNC_JOB_SPACING_MS: u64 = 200;
pub const IR_SYNC_LOOP_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrSyncThrottle {
    pub job_spacing_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct IrSyncJobFilter<'a> {
    pub provider_key: &'a str,
    pub account_id: &'a str,
    pub kind: IrJobKind,
}

impl IrSyncThrottle {
    pub const fn none() -> Self {
        Self { job_spacing_ms: 0 }
    }

    pub const fn rate_limited() -> Self {
        Self { job_spacing_ms: IR_SYNC_JOB_SPACING_MS }
    }

    fn job_delay(self) -> Option<std::time::Duration> {
        if self.job_spacing_ms == 0 {
            None
        } else {
            Some(std::time::Duration::from_millis(self.job_spacing_ms))
        }
    }
}

/// 保存済み credentials を読み、失効が近ければ refresh して保存し直す。
pub async fn ensure_fresh_credentials(
    profile_root: &Path,
    provider_key: &str,
    base_url: &str,
    now: i64,
) -> Result<IrStoredCredentials> {
    let _guard = CREDENTIAL_REFRESH_LOCK.lock().await;
    let Some(credentials) = load_credentials(profile_root, provider_key)? else {
        bail!("not signed in to IR provider '{provider_key}'; run `bmz ir login` first");
    };
    if !credentials.needs_refresh(now) {
        return Ok(credentials);
    }
    let client = BmzOfficialIrClient::anonymous(base_url)?;
    let tokens = client
        .refresh(&credentials.refresh_token)
        .await
        .with_context(|| format!("failed to refresh IR token for '{provider_key}'"))?;
    let refreshed = IrStoredCredentials {
        provider: tokens.provider_key,
        account_id: tokens.player.id,
        display_name: tokens.player.display_name.unwrap_or(credentials.display_name),
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: tokens.expires_at,
    };
    save_credentials(profile_root, &refreshed)?;
    Ok(refreshed)
}

/// pending / failed (retry時刻到達済み) の IR スコアジョブを送信する。
pub async fn sync_pending_ir_jobs(
    network_db: &mut NetworkDatabase,
    score_db_path: &Path,
    profile_root: &Path,
    logs_dir: &Path,
    ir_config: &IrConfig,
    now: i64,
    limit: u32,
    ignore_retry_backoff: bool,
    throttle: IrSyncThrottle,
) -> Result<IrSyncReport> {
    sync_pending_ir_jobs_with_filter(
        network_db,
        score_db_path,
        profile_root,
        logs_dir,
        ir_config,
        now,
        limit,
        ignore_retry_backoff,
        throttle,
        None,
    )
    .await
}

pub async fn sync_pending_ir_jobs_filtered(
    network_db: &mut NetworkDatabase,
    score_db_path: &Path,
    profile_root: &Path,
    logs_dir: &Path,
    ir_config: &IrConfig,
    filter: IrSyncJobFilter<'_>,
    now: i64,
    limit: u32,
    ignore_retry_backoff: bool,
    throttle: IrSyncThrottle,
) -> Result<IrSyncReport> {
    sync_pending_ir_jobs_with_filter(
        network_db,
        score_db_path,
        profile_root,
        logs_dir,
        ir_config,
        now,
        limit,
        ignore_retry_backoff,
        throttle,
        Some(filter),
    )
    .await
}

async fn sync_pending_ir_jobs_with_filter(
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
        Some(IrSyncJobFilter { provider_key, account_id, kind }) => network_db
            .claim_pending_ir_score_jobs_for_kind(
                provider_key,
                account_id,
                kind,
                now,
                limit,
                ignore_retry_backoff,
            )?,
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
        match job.kind {
            IrJobKind::Replay => {
                let replay_result = submit_replay_job(
                    profile_root,
                    provider,
                    &job.payload_json,
                    replay_paths.get(&job.id).and_then(Option::as_deref),
                    job.local_score_id,
                    job_now,
                )
                .await;
                match replay_result {
                    Ok(()) => {
                        network_db.mark_ir_score_job_status(
                            job.id,
                            IrScoreJobStatus::Succeeded,
                            job_now,
                            "",
                        )?;
                        report.submitted += 1;
                    }
                    Err(error) => {
                        let message = format!("replay upload failed: {error:#}");
                        let _ = write_ir_submission_log(
                            logs_dir,
                            &job,
                            "failed",
                            "",
                            job_now,
                            &job.payload_json,
                            "",
                            &message,
                        );
                        network_db.mark_ir_score_job_failed(
                            job.id,
                            job_now,
                            &message,
                            retry_after_seconds_from_error(&error),
                        )?;
                        report.failed += 1;
                        report.messages.push(format!("job {}: {message}", job.id));
                        tracing::warn!(job_id = job.id, provider = job.provider, %message, "IR replay upload failed");
                    }
                }
            }
            IrJobKind::Attestation => {
                let attestation_result = submit_score_attestation_job(
                    profile_root,
                    provider,
                    &job.payload_json,
                    job_now,
                )
                .await;
                match attestation_result {
                    Ok((remote_score_id, request_json, response_json)) => {
                        let _ = write_ir_submission_log(
                            logs_dir,
                            &job,
                            "succeeded",
                            &remote_score_id,
                            job_now,
                            &request_json,
                            &response_json,
                            "",
                        );
                        network_db.mark_ir_score_job_status(
                            job.id,
                            IrScoreJobStatus::Succeeded,
                            job_now,
                            "",
                        )?;
                        report.submitted += 1;
                    }
                    Err(error) => {
                        let message = format!("score attestation failed: {error:#}");
                        let _ = write_ir_submission_log(
                            logs_dir,
                            &job,
                            "failed",
                            "",
                            job_now,
                            &job.payload_json,
                            "",
                            &message,
                        );
                        network_db.mark_ir_score_job_failed(
                            job.id,
                            job_now,
                            &message,
                            retry_after_seconds_from_error(&error),
                        )?;
                        report.failed += 1;
                        report.messages.push(format!("job {}: {message}", job.id));
                        tracing::warn!(job_id = job.id, provider = job.provider, %message, "IR score attestation failed");
                    }
                }
            }
            IrJobKind::Score | IrJobKind::Course => {
                let submit_result = match job.kind {
                    IrJobKind::Score => {
                        submit_job_payload(profile_root, provider, &job.payload_json, job_now).await
                    }
                    IrJobKind::Course => {
                        submit_course_job_payload(
                            profile_root,
                            provider,
                            &job.payload_json,
                            job_now,
                        )
                        .await
                    }
                    IrJobKind::Replay | IrJobKind::Attestation => unreachable!(),
                };
                let Ok((request_json, response_json)) = submit_result else {
                    let error = submit_result.unwrap_err();
                    let message = format!("{error:#}");
                    let _ = write_ir_submission_log(
                        logs_dir,
                        &job,
                        "failed",
                        "",
                        job_now,
                        &job.payload_json,
                        "",
                        &message,
                    );
                    network_db.mark_ir_score_job_failed(
                        job.id,
                        job_now,
                        &message,
                        retry_after_seconds_from_error(&error),
                    )?;
                    report.failed += 1;
                    report.messages.push(format!("job {}: {message}", job.id));
                    tracing::warn!(job_id = job.id, provider = job.provider, %message, "IR score submission failed");
                    if index + 1 < job_count
                        && let Some(delay) = throttle.job_delay()
                    {
                        tokio::time::sleep(delay).await;
                    }
                    continue;
                };
                let parsed_response =
                    serde_json::from_str::<super::types::IrSubmitResponse>(&response_json).ok();
                if let Some(ranking) = parsed_response
                    .as_ref()
                    .and_then(|response| response.rankings.get(&IrRankingScope::Global))
                    .filter(|ranking| ranking.succeeded)
                    .and_then(|ranking| ranking.data.clone())
                {
                    report.included_rankings.push(ranking);
                }
                let remote_score_id = parsed_response
                    .as_ref()
                    .and_then(|response| response.score_id.clone())
                    .or_else(|| {
                        serde_json::from_str::<serde_json::Value>(&response_json).ok().and_then(
                            |value| value.get("course_score_id")?.as_str().map(str::to_string),
                        )
                    })
                    .unwrap_or_default();
                let completion =
                    replay_job_for_score(&job, &remote_score_id, job_now).and_then(|replay_job| {
                        let log_path = write_ir_submission_log(
                            logs_dir,
                            &job,
                            "succeeded",
                            &remote_score_id,
                            job_now,
                            &request_json,
                            &response_json,
                            "",
                        );
                        network_db.complete_ir_score_job(
                            &NewIrScoreSubmission {
                                job_id: job.id,
                                provider: job.provider.clone(),
                                account_id: job.account_id.clone(),
                                kind: job.kind,
                                local_score_id: job.local_score_id,
                                remote_score_id: remote_score_id.clone(),
                                status: "succeeded".to_string(),
                                submitted_at: job_now,
                                log_path,
                                error: String::new(),
                            },
                            replay_job.as_ref(),
                        )?;
                        Ok(())
                    });
                match completion {
                    Ok(()) => {
                        report.submitted += 1;
                    }
                    Err(error) => {
                        let message = format!("failed to complete IR score job: {error:#}");
                        let _ = write_ir_submission_log(
                            logs_dir,
                            &job,
                            "failed",
                            &remote_score_id,
                            job_now,
                            &request_json,
                            &response_json,
                            &message,
                        );
                        network_db.mark_ir_score_job_failed(
                            job.id,
                            job_now,
                            &message,
                            retry_after_seconds_from_error(&error),
                        )?;
                        report.failed += 1;
                        report.messages.push(format!("job {}: {message}", job.id));
                        tracing::warn!(job_id = job.id, provider = job.provider, %message, "IR score completion failed");
                    }
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

fn replay_paths_for_jobs(
    score_db_path: &Path,
    jobs: &[IrScoreJobRecord],
) -> Result<HashMap<i64, Option<String>>> {
    if !jobs.iter().any(|job| job.kind == IrJobKind::Replay) {
        return Ok(HashMap::new());
    }
    let score_db = ScoreDatabase::open(score_db_path)?;
    Ok(jobs
        .iter()
        .filter(|job| job.kind == IrJobKind::Replay)
        .map(|job| {
            let replay_path = match score_db.replay_path_for_history(job.local_score_id) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(
                        job_id = job.id,
                        local_score_id = job.local_score_id,
                        %error,
                        "failed to look up replay path for IR job"
                    );
                    None
                }
            };
            (job.id, replay_path)
        })
        .collect())
}

fn write_ir_submission_log(
    logs_dir: &Path,
    job: &IrScoreJobRecord,
    status: &str,
    remote_score_id: &str,
    submitted_at: i64,
    payload_json: &str,
    response_json: &str,
    error: &str,
) -> String {
    const LOG_FILE: &str = "ir-submissions.jsonl";
    if let Err(write_error) = std::fs::create_dir_all(logs_dir) {
        tracing::warn!(%write_error, "failed to create IR submission log directory");
        return String::new();
    }
    let payload = serde_json::from_str::<serde_json::Value>(payload_json)
        .unwrap_or_else(|_| serde_json::Value::String(payload_json.to_string()));
    let response = if response_json.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str::<serde_json::Value>(response_json)
            .unwrap_or_else(|_| serde_json::Value::String(response_json.to_string()))
    };
    let entry = serde_json::json!({
        "submitted_at": submitted_at,
        "provider": &job.provider,
        "account_id": &job.account_id,
        "kind": job.kind.as_str(),
        "job_id": job.id,
        "local_score_id": job.local_score_id,
        "remote_score_id": remote_score_id,
        "status": status,
        "payload": payload,
        "response": response,
        "error": error,
    });
    let path = logs_dir.join(LOG_FILE);
    let line = match serde_json::to_string(&entry) {
        Ok(line) => line,
        Err(write_error) => {
            tracing::warn!(%write_error, "failed to serialize IR submission log entry");
            return String::new();
        }
    };
    let mut file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(write_error) => {
            tracing::warn!(path = %path.display(), %write_error, "failed to open IR submission log");
            return String::new();
        }
    };
    if let Err(write_error) = writeln!(file, "{line}") {
        tracing::warn!(path = %path.display(), %write_error, "failed to write IR submission log");
        return String::new();
    }
    LOG_FILE.to_string()
}

fn replay_job_for_score(
    job: &IrScoreJobRecord,
    remote_score_id: &str,
    now: i64,
) -> Result<Option<NewIrScoreJob>> {
    if job.kind != IrJobKind::Score {
        return Ok(None);
    }
    let payload: IrScoreSubmission =
        serde_json::from_str(&job.payload_json).context("failed to parse stored IR payload")?;
    if payload.replay.is_none() {
        return Ok(None);
    }
    if remote_score_id.is_empty() {
        bail!("replay declared but remote score id is missing");
    }
    Ok(Some(NewIrScoreJob {
        provider: job.provider.clone(),
        account_id: job.account_id.clone(),
        kind: IrJobKind::Replay,
        local_score_id: job.local_score_id,
        chart_sha256: job.chart_sha256,
        ln_policy: job.ln_policy,
        payload_json: serde_json::to_string(&IrReplayJobPayload {
            remote_score_id: remote_score_id.to_string(),
        })?,
        now,
    }))
}

async fn submit_replay_job(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    replay_path: Option<&str>,
    local_score_id: i64,
    now: i64,
) -> Result<()> {
    let payload: IrReplayJobPayload =
        serde_json::from_str(payload_json).context("failed to parse stored IR replay payload")?;
    let replay_path = replay_path.with_context(|| {
        format!("replay declared but local file path is missing for score {local_score_id}")
    })?;
    if replay_path.is_empty() {
        bail!("replay declared but local file path is empty for score {local_score_id}");
    }
    let replay_path = replay_path.to_string();
    let bytes =
        std::fs::read(profile_root.join(&replay_path)).context("failed to read replay file")?;
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    let credentials =
        ensure_fresh_credentials(profile_root, provider_key, &provider.base_url, now).await?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    let target = client.replay_upload_url(&payload.remote_score_id).await?;
    client.upload_replay(&target.upload_url, bytes).await?;
    let verify = client.verify_replay(&payload.remote_score_id).await?;
    ensure_replay_verified(&verify.status)?;
    tracing::info!(remote_score_id = payload.remote_score_id, status = %verify.status, "IR replay uploaded");
    Ok(())
}

fn ensure_replay_verified(status: &str) -> Result<()> {
    if status != "verified" {
        bail!("IR replay verification returned status '{status}'");
    }
    Ok(())
}

fn provider_config<'a>(
    ir_config: &'a IrConfig,
    provider_key: &str,
) -> Option<&'a IrProviderConfig> {
    crate::ir::provider_key::provider_config_for_key(ir_config, provider_key)
}

async fn submit_job_payload(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    now: i64,
) -> Result<(String, String)> {
    let mut payload: IrScoreSubmission =
        serde_json::from_str(payload_json).context("failed to parse stored IR payload")?;
    normalize_legacy_score_seed_options(&mut payload);
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    let credentials =
        ensure_fresh_credentials(profile_root, provider_key, &provider.base_url, now).await?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    attach_evidence(profile_root, provider, &client, &mut payload).await;
    let request_json = serde_json::to_string(&payload)?;
    let options =
        IrSubmitOptions { ranking_scopes: vec![IrRankingScope::Global], ranking_limit: 20 };
    let response = client.submit_score(&payload, &options).await?;
    Ok((request_json, serde_json::to_string(&response)?))
}

async fn submit_score_attestation_job(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    now: i64,
) -> Result<(String, String, String)> {
    const ATTESTATION_PURPOSE: &str = "score_attestation";
    const ATTESTATION_SCHEMA: &str = "bmz-score-attestation-v1";

    let payload: IrScoreAttestationJobPayload = serde_json::from_str(payload_json)
        .context("failed to parse stored IR attestation payload")?;
    if payload.remote_score_id.is_empty() {
        bail!("stored IR attestation payload has no remote score id");
    }
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    let credentials =
        ensure_fresh_credentials(profile_root, provider_key, &provider.base_url, now).await?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    let unsigned = serde_json::json!({
        "score_id": &payload.remote_score_id,
        "purpose": ATTESTATION_PURPOSE,
    });
    let key = super::device_key::ensure_registered_device_key(profile_root, provider_key, &client)
        .await?;
    let evidence =
        super::device_key::build_evidence_for_value(&key, &unsigned, ATTESTATION_SCHEMA)?;
    let request = serde_json::json!({
        "score_id": &payload.remote_score_id,
        "purpose": ATTESTATION_PURPOSE,
        "evidence": evidence,
    });
    let response = client.attest_score(&payload.remote_score_id, &request).await?;
    Ok((
        payload.remote_score_id,
        serde_json::to_string(&request)?,
        serde_json::to_string(&response)?,
    ))
}

/// コーススコアジョブの送信。署名 evidence を付けて
/// `POST /api/v1/course-scores` へ送る。
async fn submit_course_job_payload(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    now: i64,
) -> Result<(String, String)> {
    let mut payload: serde_json::Value =
        serde_json::from_str(payload_json).context("failed to parse stored IR course payload")?;
    normalize_legacy_course_payload(&mut payload);
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    let credentials =
        ensure_fresh_credentials(profile_root, provider_key, &provider.base_url, now).await?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    let evidence = async {
        let key =
            super::device_key::ensure_registered_device_key(profile_root, provider_key, &client)
                .await?;
        super::device_key::build_evidence_for_value(&key, &payload, "bmz-course-score-evidence-v1")
    }
    .await;
    match evidence {
        Ok(evidence) => {
            if let Some(object) = payload.as_object_mut() {
                object.insert("evidence".to_string(), serde_json::json!(evidence));
            }
        }
        Err(error) => {
            tracing::warn!(provider = provider.provider, %error, "failed to attach IR course evidence; sending unsigned");
        }
    }
    let request_json = serde_json::to_string(&payload)?;
    let response = client.submit_course_score(&payload).await?;
    Ok((request_json, serde_json::to_string(&response)?))
}

fn normalize_legacy_course_payload(payload: &mut serde_json::Value) {
    normalize_legacy_seed_options_value(payload);
    let Some(rule) = payload.get_mut("rule").and_then(serde_json::Value::as_object_mut) else {
        return;
    };
    let needs_default = match rule.get("rule_mode") {
        Some(serde_json::Value::String(value)) => value.trim().is_empty(),
        Some(serde_json::Value::Null) | None => true,
        Some(_) => false,
    };
    if needs_default {
        rule.insert("rule_mode".to_string(), serde_json::json!("Beatoraja"));
    }
}

fn normalize_legacy_score_seed_options(payload: &mut IrScoreSubmission) {
    for key in ["seed", "random_seed"] {
        let Some(value) = payload.play_options.get_mut(key) else {
            continue;
        };
        normalize_integer_value_to_string(value);
    }
}

fn normalize_legacy_seed_options_value(payload: &mut serde_json::Value) {
    let Some(play_options) =
        payload.get_mut("play_options").and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    for key in ["seed", "random_seed"] {
        if let Some(value) = play_options.get_mut(key) {
            normalize_integer_value_to_string(value);
        }
    }
}

fn normalize_integer_value_to_string(value: &mut serde_json::Value) {
    let integer = value
        .as_i64()
        .map(|value| value.to_string())
        .or_else(|| value.as_u64().map(|value| value.to_string()));
    if let Some(integer) = integer {
        *value = serde_json::Value::String(integer);
    }
}

/// device key で payload に署名 evidence を付ける。
///
/// 公開鍵が未登録なら先にサーバーへ登録して key_id を保存する。
/// evidence の付与に失敗してもスコア送信自体は止めない (unverified で送る)。
async fn attach_evidence(
    profile_root: &Path,
    provider: &IrProviderConfig,
    client: &BmzOfficialIrClient,
    payload: &mut IrScoreSubmission,
) {
    let result = async {
        let provider_key = crate::ir::provider_key::configured_provider_key(provider)
            .context("IR provider key is not set; log in again")?;
        let key =
            super::device_key::ensure_registered_device_key(profile_root, provider_key, client)
                .await?;
        super::device_key::build_evidence(&key, payload)
    }
    .await;
    match result {
        Ok(evidence) => payload.evidence = evidence,
        Err(error) => {
            tracing::warn!(provider = provider.provider, %error, "failed to attach IR evidence; sending unsigned");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ln_policy::LnScorePolicy;

    #[test]
    fn ir_sync_throttles_keep_background_and_cli_budgets_separate() {
        assert_eq!(IR_SYNC_BATCH_LIMIT, 20);
        assert_eq!(IR_SYNC_JOB_SPACING_MS, 3_100);
        assert_eq!(IR_CLI_SYNC_BATCH_LIMIT, 100);
        assert_eq!(IR_CLI_SYNC_JOB_SPACING_MS, 200);
        assert_eq!(IR_SYNC_LOOP_INTERVAL_SECS, 30);
        assert_eq!(
            IrSyncThrottle::rate_limited().job_delay(),
            Some(std::time::Duration::from_millis(3_100))
        );
        assert_eq!(IrSyncThrottle::none().job_delay(), None);
    }

    #[test]
    fn replay_verification_rejects_non_verified_status() {
        assert!(ensure_replay_verified("rejected").is_err());
        assert!(ensure_replay_verified("verified").is_ok());
    }

    #[test]
    fn ir_submission_log_is_jsonl_under_logs_dir() {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let logs_dir = std::env::temp_dir()
            .join(format!("bmz-player-ir-submission-log-{}-{stamp}", std::process::id()));
        let job = IrScoreJobRecord {
            id: 7,
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            chart_sha256: [1; 32],
            ln_policy: LnScorePolicy::ForceLn,
            payload_json: String::new(),
            status: "sending".to_string(),
            attempt_count: 0,
            next_attempt_at: 0,
            last_error: String::new(),
            created_at: 100,
            updated_at: 100,
        };

        let log_path = write_ir_submission_log(
            &logs_dir,
            &job,
            "succeeded",
            "remote-1",
            123,
            "{\"score\":1}",
            "{\"accepted\":true}",
            "",
        );

        assert_eq!(log_path, "ir-submissions.jsonl");
        let line = std::fs::read_to_string(logs_dir.join(&log_path)).unwrap();
        let value: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(value["provider"], "bmz-official");
        assert_eq!(value["kind"], "score");
        assert_eq!(value["payload"]["score"], 1);
        assert_eq!(value["response"]["accepted"], true);

        let _ = std::fs::remove_dir_all(logs_dir);
    }

    #[test]
    fn legacy_course_payload_defaults_missing_rule_mode() {
        let mut payload = serde_json::json!({
            "play_options": {
                "seed": 1783820891178268800_i64,
                "random_seed": 42
            },
            "rule": {
                "gauge": "Class",
                "ln_policy": "AutoLn",
                "scoring": "bms_ex_score_v1"
            }
        });

        normalize_legacy_course_payload(&mut payload);

        assert_eq!(payload["rule"]["rule_mode"], "Beatoraja");
        assert_eq!(payload["play_options"]["seed"], "1783820891178268800");
        assert_eq!(payload["play_options"]["random_seed"], "42");
    }

    #[test]
    fn legacy_integer_seed_value_becomes_decimal_string() {
        let mut seed = serde_json::json!(1783820891178268800_i64);

        normalize_integer_value_to_string(&mut seed);

        assert_eq!(seed, "1783820891178268800");
    }

    #[test]
    fn legacy_course_payload_keeps_existing_rule_mode() {
        let mut payload = serde_json::json!({
            "rule": {
                "rule_mode": "Dx"
            }
        });

        normalize_legacy_course_payload(&mut payload);

        assert_eq!(payload["rule"]["rule_mode"], "Dx");
    }
}
