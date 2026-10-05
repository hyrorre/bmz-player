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
