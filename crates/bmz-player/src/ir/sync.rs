use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::profile_config::{IrConfig, IrProviderConfig};
use crate::storage::score_db::{IrScoreJobStatus, NewIrScoreSubmission, ScoreDatabase};

use super::bmz_official::BmzOfficialIrClient;
use super::credentials::{IrStoredCredentials, load_credentials, save_credentials};
use super::types::{IrScoreSubmission, IrSubmitOptions};

#[derive(Debug, Default, Clone)]
pub struct IrSyncReport {
    pub submitted: u32,
    pub failed: u32,
    pub messages: Vec<String>,
}

/// 保存済み credentials を読み、失効が近ければ refresh して保存し直す。
pub async fn ensure_fresh_credentials(
    profile_root: &Path,
    provider: &str,
    base_url: &str,
    now: i64,
) -> Result<IrStoredCredentials> {
    let Some(credentials) = load_credentials(profile_root, provider)? else {
        bail!("not signed in to IR provider '{provider}'; run `bmz ir login` first");
    };
    if !credentials.needs_refresh(now) {
        return Ok(credentials);
    }
    let client = BmzOfficialIrClient::anonymous(base_url)?;
    let tokens = client
        .refresh(&credentials.refresh_token)
        .await
        .with_context(|| format!("failed to refresh IR token for '{provider}'"))?;
    let refreshed = IrStoredCredentials {
        provider: credentials.provider,
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
    score_db: &mut ScoreDatabase,
    profile_root: &Path,
    ir_config: &IrConfig,
    now: i64,
    limit: u32,
) -> Result<IrSyncReport> {
    let mut report = IrSyncReport::default();
    let jobs = score_db.pending_ir_score_jobs(now, limit)?;
    for job in jobs {
        let Some(provider) = provider_config(ir_config, &job.provider) else {
            score_db.mark_ir_score_job_status(
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
        score_db.mark_ir_score_job_status(job.id, IrScoreJobStatus::Sending, now, "")?;
        match submit_job_payload(profile_root, provider, &job.payload_json, now).await {
            Ok(response_json) => {
                let remote_score_id = serde_json::from_str::<serde_json::Value>(&response_json)
                    .ok()
                    .and_then(|value| value.get("score_id")?.as_str().map(str::to_string))
                    .unwrap_or_default();
                score_db.mark_ir_score_job_status(job.id, IrScoreJobStatus::Succeeded, now, "")?;
                score_db.insert_ir_score_submission(&NewIrScoreSubmission {
                    job_id: job.id,
                    provider: job.provider.clone(),
                    account_id: job.account_id.clone(),
                    local_score_id: job.local_score_id,
                    remote_score_id: remote_score_id.clone(),
                    status: "succeeded".to_string(),
                    submitted_at: now,
                    response_json,
                    error: String::new(),
                })?;
                report.submitted += 1;
                // replay upload はスコア送信成功の付随処理。失敗しても job は
                // succeeded のまま (リプレイは best 更新に影響しない)。
                upload_replay_if_declared(
                    score_db,
                    profile_root,
                    provider,
                    &job.payload_json,
                    job.local_score_id,
                    &remote_score_id,
                    now,
                )
                .await;
            }
            Err(error) => {
                let message = format!("{error:#}");
                score_db.mark_ir_score_job_status(
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

/// payload に replay hash を申告済みなら、保存済みリプレイファイルを
/// 署名付き URL でアップロードし、サーバー側 hash 検証まで行う。
async fn upload_replay_if_declared(
    score_db: &mut ScoreDatabase,
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
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
    // rusqlite Connection は Sync でないため、DB 参照は await を跨ぐ前に済ませる。
    let replay_path = match score_db.replay_path_for_history(local_score_id) {
        Ok(Some(path)) => path,
        Ok(None) => {
            tracing::warn!(remote_score_id, "replay declared but local file path is missing");
            return;
        }
        Err(error) => {
            tracing::warn!(remote_score_id, %error, "failed to look up replay path");
            return;
        }
    };
    let result = async {
        let bytes =
            std::fs::read(profile_root.join(&replay_path)).context("failed to read replay file")?;
        let credentials =
            ensure_fresh_credentials(profile_root, &provider.provider, &provider.base_url, now)
                .await?;
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

fn provider_config<'a>(ir_config: &'a IrConfig, provider: &str) -> Option<&'a IrProviderConfig> {
    ir_config
        .providers
        .iter()
        .find(|entry| entry.provider == provider && entry.enabled && !entry.base_url.is_empty())
}

async fn submit_job_payload(
    profile_root: &Path,
    provider: &IrProviderConfig,
    payload_json: &str,
    now: i64,
) -> Result<String> {
    let mut payload: IrScoreSubmission =
        serde_json::from_str(payload_json).context("failed to parse stored IR payload")?;
    let credentials =
        ensure_fresh_credentials(profile_root, &provider.provider, &provider.base_url, now).await?;
    let client = BmzOfficialIrClient::new(&provider.base_url, credentials.access_token)?;
    attach_evidence(profile_root, provider, &client, &mut payload).await;
    let options = IrSubmitOptions { ranking_scopes: Vec::new(), ranking_limit: 0 };
    let response = client.submit_score(&payload, &options).await?;
    Ok(serde_json::to_string(&response)?)
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
        let mut key =
            super::device_key::load_or_create_device_key(profile_root, &provider.provider)?;
        if key.key_id.is_none() {
            let key_id = client.register_device_key(&key.public_key).await?;
            key.key_id = Some(key_id);
            super::device_key::save_device_key(profile_root, &key)?;
        }
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
