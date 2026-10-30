use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use crate::ln_policy::LnScorePolicy;

fn claimed_job(db: &mut NetworkDatabase, kind: IrJobKind, provider: &str) -> IrScoreJobRecord {
    db.enqueue_ir_score_job(&NewIrScoreJob {
        provider: provider.into(),
        account_id: "account".into(),
        kind,
        local_score_id: 42,
        chart_sha256: [42; 32],
        ln_policy: LnScorePolicy::AutoLn,
        payload_json: "{}".into(),
        now: 100,
    })
    .unwrap();
    db.claim_pending_ir_score_jobs(100, 1, false).unwrap().remove(0)
}

fn logs(data: &ProfileTestDir) -> Vec<serde_json::Value> {
    std::fs::read_to_string(data.paths.logs_dir.join("ir-submissions.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn failures_preserve_retry_after_and_record_each_job_once() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let mut report = IrSyncReport::default();
    for kind in [IrJobKind::Score, IrJobKind::Course, IrJobKind::Replay, IrJobKind::Attestation] {
        let job = claimed_job(&mut boot.network_db, kind, "bmz");
        let error = crate::ir::http_error::http_response_error(
            "test",
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "{}",
            Some("123"),
        )
        .context("upstream");
        JobCompletion {
            network_db: &mut boot.network_db,
            report: &mut report,
            logs_dir: &data.paths.logs_dir,
            job: &job,
            now: 200,
        }
        .fail(error, FailureStage::Submission)
        .unwrap();
        let saved = boot.network_db.ir_score_jobs_for_local_score(kind, 42).unwrap().remove(0);
        assert_eq!(saved.status, "failed");
        assert_eq!(saved.next_attempt_at, 323);
        assert!(saved.last_error.contains("upstream: test failed: 429"));
        assert_eq!(saved.payload_json, "{}");
    }
    assert_eq!((report.submitted, report.failed, report.messages.len()), (0, 4, 4));
    let logs = logs(&data);
    assert_eq!(logs.len(), 4);
    assert!(logs.iter().all(|entry| entry["status"] == "failed" && entry["response"].is_null()));
    assert!(report.messages[2].contains("replay upload failed:"));
    assert!(report.messages[3].contains("score attestation failed:"));
    assert_eq!(report.failures.len(), 4);
    assert!(report.failures.iter().all(|failure| failure.class
        == IrSyncFailureClass::RateLimited { retry_after_seconds: Some(123) }));
}

#[test]
fn submission_errors_are_classified_for_bulk_upload_decisions() {
    let http = |status: reqwest::StatusCode| {
        crate::ir::http_error::http_response_error("test", status, "{}", None).context("upstream")
    };
    for status in [reqwest::StatusCode::UNAUTHORIZED, reqwest::StatusCode::FORBIDDEN] {
        assert_eq!(classify_submission_error(&http(status)), IrSyncFailureClass::Unauthorized);
    }
    assert_eq!(
        classify_submission_error(&http(reqwest::StatusCode::TOO_MANY_REQUESTS)),
        IrSyncFailureClass::RateLimited { retry_after_seconds: None }
    );
    for status in [
        reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        reqwest::StatusCode::BAD_GATEWAY,
        reqwest::StatusCode::REQUEST_TIMEOUT,
    ] {
        assert_eq!(classify_submission_error(&http(status)), IrSyncFailureClass::Transient);
    }
    for status in [
        reqwest::StatusCode::BAD_REQUEST,
        reqwest::StatusCode::CONFLICT,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
    ] {
        assert_eq!(classify_submission_error(&http(status)), IrSyncFailureClass::Job);
    }
    let local = anyhow::anyhow!("invalid payload").context("failed to parse stored IR payload");
    assert_eq!(classify_submission_error(&local), IrSyncFailureClass::Job);
}

#[tokio::test]
async fn connection_errors_are_transient() {
    // 予約済みのdiscardポートへの接続は即座に拒否される。
    let error = reqwest::Client::new()
        .get("http://127.0.0.1:9/")
        .send()
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from)
        .context("submit")
        .unwrap_err();
    assert_eq!(classify_submission_error(&error), IrSyncFailureClass::Transient);
}

#[test]
fn successful_outcomes_keep_distinct_persistence_and_ranking_identity() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let mut report = IrSyncReport::default();
    let response = serde_json::json!({
        "accepted": true, "score_id": "remote-score", "best_updated": true,
        "rankings": { "global": { "succeeded": true, "previous_rank": 7,
            "data": { "chart": { "sha256": "chart" }, "ranking": { "scope": "global", "entries": [] } }
        } }
    }).to_string();
    for kind in [IrJobKind::Replay, IrJobKind::Attestation, IrJobKind::Score, IrJobKind::Course] {
        let provider = crate::ir::bms_ir::BMS_IR_PROVIDER;
        let job = claimed_job(&mut boot.network_db, kind, provider);
        let outcome = match kind {
            IrJobKind::Replay => SubmittedIrJob::Replay,
            IrJobKind::Attestation => SubmittedIrJob::Attestation(SubmissionReceipt {
                remote_score_id: "remote-attestation".into(),
                request_json: "{}".into(),
                response_json: "{}".into(),
            }),
            IrJobKind::Score | IrJobKind::Course => SubmittedIrJob::Score {
                request_json: "{\"request\":true}".into(),
                response_json: if kind == IrJobKind::Score {
                    response.clone()
                } else {
                    "{\"course_score_id\":\"remote-course\"}".into()
                },
                include_ranking: kind == IrJobKind::Score,
            },
        };
        JobCompletion {
            network_db: &mut boot.network_db,
            report: &mut report,
            logs_dir: &data.paths.logs_dir,
            job: &job,
            now: 200,
        }
        .complete(outcome)
        .unwrap();
        let status: String = boot
            .network_db
            .conn()
            .query_row("SELECT status FROM ir_score_jobs WHERE id = ?1", [job.id], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "succeeded");
    }
    assert_eq!((report.submitted, report.failed), (4, 0));
    assert_eq!(report.included_rankings.len(), 1);
    let included = &report.included_rankings[0];
    assert_eq!(included.account_id, "account");
    assert_eq!(included.local_score_id, 42);
    assert_eq!(included.previous_rank, Some(7));
    let submissions: i64 = boot
        .network_db
        .conn()
        .query_row("SELECT COUNT(*) FROM ir_score_submissions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(submissions, 2);
    let logs = logs(&data);
    assert_eq!(logs.len(), 3); // Replay success has no score submission log.
    assert_eq!(logs[2]["remote_score_id"], "remote-course");
}

#[test]
fn completion_failure_keeps_remote_response_without_counting_success() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let mut report = IrSyncReport::default();
    let job = claimed_job(&mut boot.network_db, IrJobKind::Score, "bmz");
    JobCompletion {
        network_db: &mut boot.network_db,
        report: &mut report,
        logs_dir: &data.paths.logs_dir,
        job: &job,
        now: 200,
    }
    .complete(SubmittedIrJob::Score {
        request_json: "{\"request\":true}".into(),
        response_json: "{\"accepted\":true,\"best_updated\":true,\"score_id\":\"remote\"}".into(),
        include_ranking: false,
    })
    .unwrap();
    // The stored {} payload cannot be decoded to decide whether to enqueue replay.
    assert_eq!((report.submitted, report.failed), (0, 1));
    assert!(report.messages[0].contains("failed to complete IR score job:"));
    let saved = boot.network_db.ir_score_jobs_for_local_score(IrJobKind::Score, 42).unwrap();
    assert_eq!(saved[0].status, "failed");
    assert!(
        boot.network_db
            .latest_ir_score_submission_response("bmz", "account", IrJobKind::Score, 42)
            .unwrap()
            .is_none()
    );
    let logs = logs(&data);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["remote_score_id"], "remote");
    assert_eq!(logs[0]["response"]["accepted"], true);
    assert_eq!(logs[0]["payload"]["request"], true);
}

fn moderation_error(code: &str) -> anyhow::Error {
    crate::ir::http_error::http_response_error(
        "BMZ IR score submission",
        reqwest::StatusCode::FORBIDDEN,
        &serde_json::json!({
            "statusCode": 403,
            "statusMessage": "Score submission is blocked for this chart",
            "data": { "code": code },
        })
        .to_string(),
        None,
    )
    .context("failed to submit IR score")
}

#[test]
fn moderation_rejections_are_permanent_and_not_login_errors() {
    use crate::ir::IrModerationCode;
    for (code, expected) in [
        ("ACCOUNT_SUSPENDED", IrModerationCode::AccountSuspended),
        ("ACCOUNT_BANNED", IrModerationCode::AccountBanned),
        ("CHART_HIDDEN", IrModerationCode::ChartHidden),
        ("CHART_BLOCKED", IrModerationCode::ChartBlocked),
    ] {
        let error = moderation_error(code);
        assert_eq!(classify_submission_error(&error), IrSyncFailureClass::Moderated(expected));
        // 保存されるエラー文字列からも制限コードを読み戻せる。
        assert_eq!(IrModerationCode::from_error_message(&format!("{error:#}")), Some(expected));
    }
    // 未知のコード・403以外・他provider形式のbodyは従来どおりの分類。
    assert_eq!(
        classify_submission_error(&moderation_error("SOMETHING_ELSE")),
        IrSyncFailureClass::Unauthorized
    );
    let not_forbidden = crate::ir::http_error::http_response_error(
        "test",
        reqwest::StatusCode::BAD_REQUEST,
        r#"{"data":{"code":"CHART_BLOCKED"}}"#,
        None,
    );
    assert_eq!(classify_submission_error(&not_forbidden), IrSyncFailureClass::Job);
    assert_eq!(IrModerationCode::from_error_message("403 Forbidden [maybe]"), None);
}

#[test]
fn moderated_jobs_keep_local_data_and_leave_automatic_retry() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let mut report = IrSyncReport::default();
    let job = claimed_job(&mut boot.network_db, IrJobKind::Score, "bmz");
    JobCompletion {
        network_db: &mut boot.network_db,
        report: &mut report,
        logs_dir: &data.paths.logs_dir,
        job: &job,
        now: 200,
    }
    .fail(moderation_error("CHART_BLOCKED"), FailureStage::Submission)
    .unwrap();

    let saved = boot.network_db.ir_score_jobs_for_local_score(IrJobKind::Score, 42).unwrap();
    assert_eq!(saved[0].status, "failed");
    assert_eq!(saved[0].payload_json, "{}");
    assert_eq!(saved[0].next_attempt_at, crate::storage::network_db::IR_JOB_NO_AUTO_RETRY_AT);
    assert!(saved[0].last_error.ends_with("[CHART_BLOCKED]"));
    assert_eq!(
        report.failures[0].class,
        IrSyncFailureClass::Moderated(crate::ir::IrModerationCode::ChartBlocked)
    );
    // 常駐同期・リザルト画面の自動送信（backoff尊重）は、何年経っても取り直さない。
    let far_future = 200 + 100 * 365 * 24 * 60 * 60;
    assert!(boot.network_db.claim_pending_ir_score_jobs(far_future, 10, false).unwrap().is_empty());
    // 制限解除後の手動再送（backoff無視）では送り直せる。
    let retried = boot.network_db.claim_pending_ir_score_jobs(201, 10, true).unwrap();
    assert_eq!(retried.iter().map(|job| job.id).collect::<Vec<_>>(), vec![job.id]);
}

/// refreshに `ACCOUNT_BANNED` を1回だけ返し、その後は接続を受け付けないIRサーバー。
fn spawn_banned_refresh_server() -> (String, std::thread::JoinHandle<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut request = [0; 8192];
        let read = stream.read(&mut request).unwrap();
        let body = r#"{"statusCode":403,"statusMessage":"This account is banned from BMZ IR","data":{"code":"ACCOUNT_BANNED"}}"#;
        write!(
            stream,
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8_lossy(&request[..read]).into_owned()
    });
    (format!("http://{address}"), server)
}

#[tokio::test]
async fn banned_account_stops_the_batch_without_repeating_refresh() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let (base_url, server) = spawn_banned_refresh_server();
    let mut provider = IrProviderConfig::bmz_ir();
    provider.provider_key = "bmz-moderation-test".into();
    provider.base_url = base_url;
    provider.enabled = true;
    provider.account_id = "account".into();
    let provider_key =
        crate::ir::provider_key::configured_provider_key(&provider).unwrap().to_string();
    let profile_root = boot.profile_paths.root_dir.clone();
    save_credentials(
        &profile_root,
        &IrStoredCredentials {
            provider: provider_key.clone(),
            account_id: "account".into(),
            display_name: String::new(),
            access_token: "expired".into(),
            refresh_token: "refresh".into(),
            expires_at: Some(1),
        },
    )
    .unwrap();
    for local_score_id in [1, 2, 3] {
        boot.network_db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: provider_key.clone(),
                account_id: "account".into(),
                kind: IrJobKind::Score,
                local_score_id,
                chart_sha256: [7; 32],
                ln_policy: LnScorePolicy::AutoLn,
                payload_json: "{}".into(),
                now: 100,
            })
            .unwrap();
    }
    let ir_config = IrConfig {
        primary_provider: provider_key.clone(),
        providers: vec![provider],
        ..IrConfig::default()
    };

    let report = sync_pending_ir_jobs(
        &mut boot.network_db,
        &boot.profile_paths.score_db,
        &profile_root,
        &data.paths.logs_dir,
        &ir_config,
        1_000,
        10,
        false,
        IrSyncThrottle::none(),
    )
    .await
    .unwrap();

    let request = server.join().unwrap();
    assert!(request.starts_with("POST /api/v1/auth/refresh "));
    assert_eq!((report.submitted, report.failed), (0, 3));
    assert!(report.failures.iter().all(|failure| failure.class
        == IrSyncFailureClass::Moderated(crate::ir::IrModerationCode::AccountBanned)));
    let breaker_stop = crate::ir::manual::IrFailureBreaker::default().record(&report);
    assert_eq!(
        breaker_stop,
        Some(crate::ir::manual::IrManualStop::AccountRestricted { banned: true })
    );
    for local_score_id in [1, 2, 3] {
        let job = boot
            .network_db
            .ir_score_jobs_for_local_score(IrJobKind::Score, local_score_id)
            .unwrap()
            .remove(0);
        assert_eq!(job.status, "failed");
        assert_eq!(job.next_attempt_at, crate::storage::network_db::IR_JOB_NO_AUTO_RETRY_AT);
        assert!(job.last_error.ends_with("[ACCOUNT_BANNED]"), "{}", job.last_error);
    }
}
