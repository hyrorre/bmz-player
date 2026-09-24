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
use crate::ir::types::{IrRankingResult, IrRankingScope, IrScoreSubmission, IrSubmitOptions};

static CREDENTIAL_REFRESH_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Default, Clone)]
pub struct IrSyncReport {
    pub submitted: u32,
    pub failed: u32,
    pub messages: Vec<String>,
    /// 送信レスポンスに同梱されたランキングと、そのレスポンスを返したローカル job。
    ///
    /// 同じ譜面を複数回送信するバッチでは chart hash だけで ranking を選ぶと、
    /// 古い試行の応答を今回のリザルトへ表示してしまう。Result 側が今回の
    /// score_history_id と照合できるよう、job の識別子を一緒に保持する。
    pub included_rankings: Vec<IrIncludedRanking>,
}

#[derive(Debug, Clone)]
pub struct IrIncludedRanking {
    pub provider: String,
    pub account_id: String,
    pub kind: IrJobKind,
    pub local_score_id: i64,
    pub previous_rank: Option<u32>,
    pub ranking: IrRankingResult,
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
    /// `Some` の場合は、その Result attempt の job だけを同期する。
    pub local_score_id: Option<i64>,
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

async fn credentials_for_job(
    profile_root: &Path,
    provider: &IrProviderConfig,
    account_id: &str,
    now: i64,
) -> Result<IrStoredCredentials> {
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    let credentials =
        ensure_fresh_credentials(profile_root, provider_key, &provider.base_url, now).await?;
    if credentials.account_id != account_id {
        bail!("IR job belongs to account '{account_id}'; sign in to that account to retry");
    }
    Ok(credentials)
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

mod evidence;
mod payload;
mod process;
mod replay;

use evidence::*;
use payload::*;
use process::*;
use replay::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ln_policy::LnScorePolicy;

    #[tokio::test]
    async fn queued_credentials_require_the_original_account() {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root =
            std::env::temp_dir().join(format!("bmz-ir-job-account-{}-{stamp}", std::process::id()));
        let mut provider = IrProviderConfig::bmz_ir();
        provider.provider_key = "bmz-test".into();
        let key = crate::ir::provider_key::configured_provider_key(&provider).unwrap();
        let mut credentials = IrStoredCredentials {
            provider: key.to_string(),
            account_id: "account-b".into(),
            display_name: String::new(),
            access_token: "token-b".into(),
            refresh_token: String::new(),
            expires_at: None,
        };
        save_credentials(&root, &credentials).unwrap();
        let error = credentials_for_job(&root, &provider, "account-a", 100).await.unwrap_err();
        assert!(error.to_string().contains("belongs to account 'account-a'"));
        credentials.account_id = "account-a".into();
        credentials.access_token = "token-a".into();
        save_credentials(&root, &credentials).unwrap();
        let checked = credentials_for_job(&root, &provider, "account-a", 100).await.unwrap();
        credentials.account_id = "account-b".into();
        save_credentials(&root, &credentials).unwrap();
        assert_eq!(checked.account_id, "account-a");
        assert_eq!(checked.access_token, "token-a");
        std::fs::remove_dir_all(root).unwrap();
    }

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
    fn score_submission_requests_the_default_ranking_limit() {
        let options = score_submit_options(true);

        assert_eq!(options.ranking_scopes, vec![IrRankingScope::Global]);
        assert_eq!(options.ranking_limit, crate::ir::types::default_ranking_limit());
        assert_eq!(options.ranking_limit, 100);
    }

    #[test]
    fn submit_only_score_submission_does_not_request_ranking() {
        let options = score_submit_options(false);

        assert!(options.ranking_scopes.is_empty());
        assert_eq!(options.ranking_limit, crate::ir::types::default_ranking_limit());
    }

    #[test]
    fn score_submission_requests_ranking_only_from_configured_primary() {
        let mut primary = IrProviderConfig::bms_ir();
        primary.provider_key = crate::ir::bms_ir::BMS_IR_PROVIDER.to_string();
        primary.enabled = true;
        let mut submit_only = IrProviderConfig::rian_ir();
        submit_only.provider_key = crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string();
        submit_only.enabled = true;
        let config = IrConfig {
            primary_provider: crate::ir::bms_ir::BMS_IR_PROVIDER.to_string(),
            providers: vec![primary, submit_only],
            ..IrConfig::default()
        };

        assert!(score_submission_includes_ranking(&config, crate::ir::bms_ir::BMS_IR_PROVIDER));
        assert!(!score_submission_includes_ranking(&config, crate::ir::rian_ir::RIAN_IR_PROVIDER));
    }

    #[test]
    fn replay_verification_rejects_non_verified_status() {
        assert!(ensure_replay_verified("rejected").is_err());
        assert!(ensure_replay_verified("verified").is_ok());
    }

    #[test]
    fn queued_local_backfill_is_blocked_for_submit_only_legacy_providers() {
        let payload: IrScoreSubmission = serde_json::from_value(serde_json::json!({
            "client": { "name": "BMZ", "version": "test", "platform": "test" },
            "chart": {
                "sha256": "00",
                "ln_profile": {
                    "has_undefined_ln": false,
                    "has_defined_ln": false,
                    "has_defined_cn": true,
                    "has_defined_hcn": false
                },
                "mode": "7K",
                "notes": { "total": 0, "ln": 0, "cn": 0, "hcn": 0, "mine": 0 },
                "features": {
                    "random": false,
                    "stop": false,
                    "ln": false,
                    "cn": true,
                    "hcn": false,
                    "mine": false
                }
            },
            "rule": {
                "play_mode": "single",
                "key_mode": "7K",
                "gauge": "Hard",
                "ln_policy": "ForceCn",
                "effective_ln_mode": "cn",
                "judge_algorithm": "bmz_v1",
                "scoring": "bms_ex_score_v1"
            },
            "result": {
                "clear": "Hard",
                "played_at": 0,
                "judges": {
                    "fast": { "pgreat": 0, "great": 0, "good": 0, "bad": 0, "poor": 0, "empty_poor": 0 },
                    "slow": { "pgreat": 0, "great": 0, "good": 0, "bad": 0, "poor": 0, "empty_poor": 0 }
                },
                "ex_score": 0,
                "max_combo": 0,
                "notes": 0,
                "min_bp": 0,
                "min_cb": 0
            },
            "play_options": { "submission_source": "local_backfill" },
            "idempotency_key": "test"
        }))
        .unwrap();
        let rian = IrProviderConfig::rian_ir();
        let bms_ir = IrProviderConfig::bms_ir();
        let bmz = IrProviderConfig::bmz_ir();

        let error = ensure_score_payload_allowed(&rian, &payload).unwrap_err();
        assert_eq!(error.to_string(), "rianIR local score backfill is disabled");
        let error = ensure_score_payload_allowed(&bms_ir, &payload).unwrap_err();
        assert_eq!(error.to_string(), "BMS-IR local score backfill is disabled");
        assert!(ensure_score_payload_allowed(&bmz, &payload).is_ok());

        let mut payload = payload;
        payload.play_options.clear();
        payload.play_options.insert("conditional".into(), serde_json::Value::Bool(true));
        for provider in [rian, bms_ir, bmz] {
            let error = ensure_score_payload_allowed(&provider, &payload).unwrap_err();
            assert_eq!(
                error.to_string(),
                "dynamic conditional charts are not supported by IR providers yet"
            );
        }
    }

    #[test]
    fn bms_ir_score_completion_never_enqueues_replay_upload() {
        let job = IrScoreJobRecord {
            id: 7,
            provider: crate::ir::bms_ir::BMS_IR_PROVIDER.to_string(),
            account_id: "123".to_string(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            chart_sha256: [1; 32],
            ln_policy: LnScorePolicy::AutoLn,
            payload_json: "not parsed for submit-only provider".to_string(),
            status: "sending".to_string(),
            attempt_count: 0,
            next_attempt_at: 0,
            last_error: String::new(),
            created_at: 100,
            updated_at: 100,
        };

        assert!(replay_job_for_score(&job, "remote-score", 123).unwrap().is_none());
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
