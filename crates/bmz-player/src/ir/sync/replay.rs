use super::*;

pub(super) fn replay_paths_for_jobs(
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

pub(super) fn write_ir_submission_log(
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

pub(super) fn replay_job_for_score(
    job: &IrScoreJobRecord,
    remote_score_id: &str,
    now: i64,
) -> Result<Option<NewIrScoreJob>> {
    if job.kind != IrJobKind::Score
        || crate::ir::rian_ir::is_rian_ir_provider(&job.provider)
        || crate::ir::bms_ir::is_bms_ir_provider(&job.provider)
    {
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

pub(super) async fn submit_replay_job(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    replay_path: Option<&str>,
    local_score_id: i64,
    credentials: IrStoredCredentials,
) -> Result<()> {
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        bail!("BMS-IR replay upload is not supported");
    }
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
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    let target = client.replay_upload_url(&payload.remote_score_id).await?;
    client.upload_replay(&target.upload_url, bytes).await?;
    let verify = client.verify_replay(&payload.remote_score_id).await?;
    ensure_replay_verified(&verify.status)?;
    tracing::info!(remote_score_id = payload.remote_score_id, status = %verify.status, "IR replay uploaded");
    Ok(())
}

pub(super) fn ensure_replay_verified(status: &str) -> Result<()> {
    if status != "verified" {
        bail!("IR replay verification returned status '{status}'");
    }
    Ok(())
}

pub(super) fn provider_config<'a>(
    ir_config: &'a IrConfig,
    provider_key: &str,
) -> Option<&'a IrProviderConfig> {
    crate::ir::provider_key::provider_config_for_key(ir_config, provider_key)
}

pub(super) fn score_submission_includes_ranking(ir_config: &IrConfig, provider_key: &str) -> bool {
    crate::ir::provider_key::primary_provider_config(ir_config)
        .and_then(crate::ir::provider_key::configured_provider_key)
        .is_some_and(|primary_key| primary_key == provider_key)
}

pub(super) async fn submit_job_payload(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    credentials: IrStoredCredentials,
    include_ranking: bool,
) -> Result<(String, String)> {
    let mut payload: IrScoreSubmission =
        serde_json::from_str(payload_json).context("failed to parse stored IR payload")?;
    normalize_legacy_score_seed_options(&mut payload);
    ensure_score_payload_allowed(provider, &payload)?;
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        let client = crate::ir::bms_ir::BmsIrClient::new(&provider.base_url)?;
        let outcome = client
            .submit_score(
                &payload,
                &credentials.account_id,
                &credentials.access_token,
                include_ranking,
            )
            .await?;
        return Ok((outcome.redacted_request_json, outcome.response_json));
    }
    if crate::ir::rian_ir::is_rian_ir_config(provider) {
        let client = crate::ir::rian_ir::RianIrClient::new(&provider.base_url)?;
        let outcome = client
            .submit_score(
                &payload,
                &credentials.account_id,
                &credentials.access_token,
                include_ranking,
            )
            .await?;
        return Ok((outcome.redacted_request_json, outcome.response_json));
    }
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    attach_evidence(profile_root, provider, &client, &mut payload).await;
    let request_json = serde_json::to_string(&payload)?;
    let options = score_submit_options(include_ranking);
    let response = client.submit_score(&payload, &options).await?;
    Ok((request_json, serde_json::to_string(&response)?))
}

pub(super) fn score_submit_options(include_ranking: bool) -> IrSubmitOptions {
    IrSubmitOptions {
        ranking_scopes: if include_ranking { vec![IrRankingScope::Global] } else { Vec::new() },
        ranking_limit: crate::ir::types::default_ranking_limit(),
    }
}

pub(super) fn ensure_score_payload_allowed(
    provider: &IrProviderConfig,
    payload: &IrScoreSubmission,
) -> Result<()> {
    if payload.play_options.get("conditional").and_then(serde_json::Value::as_bool) == Some(true) {
        bail!("dynamic conditional charts are not supported by IR providers yet");
    }
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        return crate::ir::bms_ir::ensure_score_payload_supported(payload);
    }
    if crate::ir::rian_ir::is_rian_ir_config(provider)
        && crate::ir::backfill::is_local_backfill_submission(payload)
    {
        bail!("rianIR local score backfill is disabled");
    }
    Ok(())
}
