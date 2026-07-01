use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::sync::Mutex;

use crate::config::profile_config::{IrConfig, IrProviderConfig};
use crate::storage::network_db::{
    IrJobKind, IrScoreJobRecord, IrScoreJobStatus, NetworkDatabase, NewIrScoreSubmission,
};
use crate::storage::score_db::ScoreDatabase;

use super::bmz_official::BmzOfficialIrClient;
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
) -> Result<IrSyncReport> {
    let mut report = IrSyncReport::default();
    let jobs = network_db.pending_ir_score_jobs(now, limit)?;
    let replay_paths = replay_paths_for_jobs(score_db_path, &jobs)?;
    for job in jobs {
        let Some(provider) = provider_config(ir_config, &job.provider) else {
            network_db.mark_ir_score_job_status(
                job.id,
                IrScoreJobStatus::Failed,
                now,
                "provider is not configured",
            )?;
            report.failed += 1;
            report
                .messages
                .push(format!("job {}: provider '{}' not configured", job.id, job.provider));
            continue;
        };
        network_db.mark_ir_score_job_status(job.id, IrScoreJobStatus::Sending, now, "")?;
        let submit_result = match job.kind {
            IrJobKind::Score => {
                submit_job_payload(profile_root, provider, &job.payload_json, now).await
            }
            IrJobKind::Course => {
                submit_course_job_payload(profile_root, provider, &job.payload_json, now).await
            }
        };
        match submit_result {
            Ok((request_json, response_json)) => {
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
                let log_path = write_ir_submission_log(
                    logs_dir,
                    &job,
                    "succeeded",
                    &remote_score_id,
                    now,
                    &request_json,
                    &response_json,
                    "",
                );
                network_db.insert_ir_score_submission(&NewIrScoreSubmission {
                    job_id: job.id,
                    provider: job.provider.clone(),
                    account_id: job.account_id.clone(),
                    kind: job.kind,
                    local_score_id: job.local_score_id,
                    remote_score_id: remote_score_id.clone(),
                    status: "succeeded".to_string(),
                    submitted_at: now,
                    log_path,
                    error: String::new(),
                })?;
                report.submitted += 1;
                // replay upload はスコア送信成功の付随処理。失敗しても job は
                // succeeded のまま (リプレイは best 更新に影響しない)。
                // コースジョブにはリプレイ申告がないため no-op になる。
                upload_replay_if_declared(
                    profile_root,
                    provider,
                    &job.payload_json,
                    replay_paths.get(&job.id).and_then(Option::as_deref),
                    job.local_score_id,
                    &remote_score_id,
                    now,
                )
                .await;
                network_db.mark_ir_score_job_status(
                    job.id,
                    IrScoreJobStatus::Succeeded,
                    now,
                    "",
                )?;
            }
            Err(error) => {
                let message = format!("{error:#}");
                let _ = write_ir_submission_log(
                    logs_dir,
                    &job,
                    "failed",
                    "",
                    now,
                    &job.payload_json,
                    "",
                    &message,
                );
                network_db.mark_ir_score_job_status(
                    job.id,
                    IrScoreJobStatus::Failed,
                    now,
                    &message,
                )?;
                report.failed += 1;
                report.messages.push(format!("job {}: {message}", job.id));
                tracing::warn!(job_id = job.id, provider = job.provider, %message, "IR score submission failed");
            }
        }
    }
    Ok(report)
}

fn replay_paths_for_jobs(
    score_db_path: &Path,
    jobs: &[IrScoreJobRecord],
) -> Result<HashMap<i64, Option<String>>> {
    if !jobs.iter().any(|job| job.kind == IrJobKind::Score) {
        return Ok(HashMap::new());
    }
    let score_db = ScoreDatabase::open(score_db_path)?;
    Ok(jobs
        .iter()
        .filter(|job| job.kind == IrJobKind::Score)
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

/// payload に replay hash を申告済みなら、保存済みリプレイファイルを
/// 署名付き URL でアップロードし、サーバー側 hash 検証まで行う。
async fn upload_replay_if_declared(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    replay_path: Option<&str>,
    local_score_id: i64,
    remote_score_id: &str,
    now: i64,
) {
    let declared = serde_json::from_str::<IrScoreSubmission>(payload_json)
        .ok()
        .and_then(|payload| payload.replay)
        .is_some();
    if !declared || remote_score_id.is_empty() {
        return;
    }
    let Some(replay_path) = replay_path else {
        tracing::warn!(
            remote_score_id,
            local_score_id,
            "replay declared but local file path is missing"
        );
        return;
    };
    if replay_path.is_empty() {
        tracing::warn!(
            remote_score_id,
            local_score_id,
            "replay declared but local file path is empty"
        );
        return;
    }
    let replay_path = replay_path.to_string();
    let result = async {
        let bytes =
            std::fs::read(profile_root.join(&replay_path)).context("failed to read replay file")?;
        let provider_key = crate::ir::provider_key::configured_provider_key(provider)
            .context("IR provider key is not set; log in again")?;
        let credentials =
            ensure_fresh_credentials(profile_root, provider_key, &provider.base_url, now).await?;
        let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
        let target = client.replay_upload_url(remote_score_id).await?;
        client.upload_replay(&target.upload_url, bytes).await?;
        let verify = client.verify_replay(remote_score_id).await?;
        anyhow::Ok(verify.status)
    }
    .await;
    match result {
        Ok(status) => {
            tracing::info!(remote_score_id, %status, "IR replay uploaded");
        }
        Err(error) => {
            tracing::warn!(remote_score_id, %error, "IR replay upload failed");
        }
    }
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
}
