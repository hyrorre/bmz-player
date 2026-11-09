use std::path::PathBuf;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use super::*;
use crate::config::profile_config::IrProviderConfig;
use crate::ir::backfill::IrLocalUploadOptions;
use crate::ir::sync::IrSyncReport;
use crate::ln_policy::LnScorePolicy;
use crate::storage::common::configure_connection;
use crate::storage::migration::{
    NETWORK_MIGRATIONS, migrate_library_db, migrate_network_db, migrate_score_db, run_migrations,
};
use crate::storage::network_db::{IrJobKind, IrJobRetryFilter, NetworkDatabase, NewIrScoreJob};

use super::sync::{IrManualSyncScope, sync_jobs};
use super::upload::full_upload_stop_reason;

fn temp_dir(name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "bmz-ir-manual-{name}-{}-{}",
        std::process::id(),
        stamp.as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open_network_db() -> NetworkDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, NETWORK_MIGRATIONS).unwrap();
    NetworkDatabase::from_connection(conn)
}

/// 設定に存在しないproviderのjob。送信は通信せずローカルで失敗扱いになる。
fn enqueue_unroutable_job(db: &mut NetworkDatabase, local_score_id: i64) -> i64 {
    db.enqueue_ir_score_job(&NewIrScoreJob {
        provider: "unconfigured".to_string(),
        account_id: "account-1".to_string(),
        kind: IrJobKind::Score,
        local_score_id,
        chart_sha256: [local_score_id as u8; 32],
        ln_policy: LnScorePolicy::ForceLn,
        payload_json: "{}".to_string(),
        now: 1,
    })
    .unwrap()
}

struct Fixture {
    dir: PathBuf,
    score_db: PathBuf,
    config: IrConfig,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = temp_dir(name);
        let score_db = dir.join("score.db");
        crate::storage::migration::migrate_score_db(&score_db).unwrap();
        Self { dir, score_db, config: IrConfig::default() }
    }

    fn ctx(&self) -> IrSyncContext<'_> {
        IrSyncContext {
            score_db: &self.score_db,
            profile_root: &self.dir,
            logs_dir: &self.dir,
            ir_config: &self.config,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn full_upload_distinguishes_failures_from_waiting_for_a_sending_job() {
    assert_eq!(
        full_upload_stop_reason(&IrSyncReport::default()),
        Some(IrManualStop::WaitingForSendingJob)
    );
    assert_eq!(full_upload_stop_reason(&IrSyncReport { submitted: 1, ..Default::default() }), None);
    assert_eq!(
        full_upload_stop_reason(&IrSyncReport { submitted: 1, failed: 2, ..Default::default() }),
        Some(IrManualStop::FailedJobs { failed: 2 })
    );
}

#[tokio::test]
async fn sync_jobs_reports_progress_for_every_job() {
    let fixture = Fixture::new("progress");
    let mut db = open_network_db();
    for id in 1..=2 {
        enqueue_unroutable_job(&mut db, id);
    }
    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let mut events = Vec::new();
    let outcome = sync_jobs(
        &fixture.ctx(),
        &mut db,
        IrManualSyncScope::Any,
        10,
        &mut |event| events.push(event),
        &cancel,
    )
    .await
    .unwrap();

    assert!(!outcome.cancelled);
    assert_eq!(outcome.report.failed, 2);
    assert_eq!(outcome.report.submitted, 0);
    assert!(matches!(events[0], IrManualProgress::SyncStarted { limit: 10, .. }));
    let started: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            IrManualProgress::JobSyncing { index, .. } => Some(*index),
            _ => None,
        })
        .collect();
    assert_eq!(started, [1, 2]);
    let finished = events
        .iter()
        .filter(|event| matches!(event, IrManualProgress::JobFinished { failed: 1, .. }))
        .count();
    assert_eq!(finished, 2);
}

#[tokio::test]
async fn sync_jobs_stops_before_the_next_job_when_cancelled() {
    let fixture = Fixture::new("cancel");
    let mut db = open_network_db();
    for id in 1..=3 {
        enqueue_unroutable_job(&mut db, id);
    }
    let (tx, cancel) = tokio::sync::watch::channel(false);
    let outcome = sync_jobs(
        &fixture.ctx(),
        &mut db,
        IrManualSyncScope::Any,
        10,
        &mut |event| {
            if matches!(event, IrManualProgress::JobFinished { .. }) {
                tx.send_replace(true);
            }
        },
        &cancel,
    )
    .await
    .unwrap();

    assert!(outcome.cancelled);
    // 処理中の1件は完了し、残りは手を付けない。
    assert_eq!(outcome.report.failed, 1);
    let untouched = db.pending_ir_score_jobs(i64::MAX / 2, 10).unwrap();
    assert_eq!(untouched.iter().filter(|job| job.status == "pending").count(), 2);
}

#[tokio::test]
async fn sync_jobs_does_not_start_when_already_cancelled() {
    let fixture = Fixture::new("precancel");
    let mut db = open_network_db();
    enqueue_unroutable_job(&mut db, 1);
    let (_tx, cancel) = tokio::sync::watch::channel(true);
    let outcome =
        sync_jobs(&fixture.ctx(), &mut db, IrManualSyncScope::Any, 10, &mut |_| {}, &cancel)
            .await
            .unwrap();
    assert!(outcome.cancelled);
    assert_eq!(outcome.report.failed, 0);
}

#[tokio::test]
async fn sync_jobs_skips_jobs_another_worker_is_sending() {
    let fixture = Fixture::new("sending");
    let mut db = open_network_db();
    enqueue_unroutable_job(&mut db, 1);
    // 常駐workerなど別の主体がclaim済み（lease内）。
    let claimed = db.claim_pending_ir_score_jobs(now_unix_seconds(), 10, true).unwrap();
    assert_eq!(claimed.len(), 1);

    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let mut events = Vec::new();
    let outcome = sync_jobs(
        &fixture.ctx(),
        &mut db,
        IrManualSyncScope::Any,
        10,
        &mut |event| events.push(event),
        &cancel,
    )
    .await
    .unwrap();

    assert_eq!(outcome.report.submitted, 0);
    assert_eq!(outcome.report.failed, 0);
    assert!(events.iter().any(|event| matches!(event, IrManualProgress::NoQueuedJobs)));
    // 進捗なし（失敗でもない）= lease待ちとして扱われる。
    assert_eq!(full_upload_stop_reason(&outcome.report), Some(IrManualStop::WaitingForSendingJob));
}

struct UploadFixture {
    dir: PathBuf,
    library_db: PathBuf,
    score_db: PathBuf,
    network_db: PathBuf,
    config: IrConfig,
}

impl UploadFixture {
    fn new(name: &str) -> Self {
        let dir = temp_dir(name);
        let library_db = dir.join("library.db");
        let score_db = dir.join("score.db");
        let network_db = dir.join("network.db");
        migrate_library_db(&library_db).unwrap();
        migrate_score_db(&score_db).unwrap();
        migrate_network_db(&network_db).unwrap();

        let mut target = IrProviderConfig::bmz_ir();
        target.provider_key = "target-a".to_string();
        target.enabled = true;
        target.account_id = "account-a".to_string();
        let config = IrConfig {
            primary_provider: "target-a".to_string(),
            providers: vec![target],
            ..IrConfig::default()
        };
        Self { dir, library_db, score_db, network_db, config }
    }

    fn context(&self) -> IrManualContext<'_> {
        IrManualContext {
            library_db: &self.library_db,
            network_db: &self.network_db,
            sync: IrSyncContext {
                score_db: &self.score_db,
                profile_root: &self.dir,
                logs_dir: &self.dir,
                ir_config: &self.config,
            },
        }
    }

    fn seed_target_sending_and_unrelated_pending(&self) -> Vec<(i64, &'static str)> {
        let mut db = NetworkDatabase::open(&self.network_db).unwrap();
        let now = now_unix_seconds();
        let target_score =
            enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Score, 1, now);
        let claimed = db
            .claim_pending_ir_score_jobs_for_kind(
                "target-a",
                "account-a",
                IrJobKind::Score,
                now,
                1,
                IrJobRetryFilter::IgnoreBackoff,
            )
            .unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, target_score);

        let other_provider =
            enqueue_upload_job(&mut db, "unconfigured", "other", IrJobKind::Score, 2, now);
        let other_account =
            enqueue_upload_job(&mut db, "target-a", "account-b", IrJobKind::Score, 3, now);
        let replay =
            enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Replay, 4, now);
        let course =
            enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Course, 5, now);
        vec![
            (target_score, "sending"),
            (other_provider, "pending"),
            (other_account, "pending"),
            (replay, "pending"),
            (course, "pending"),
        ]
    }

    fn seed_only_unrelated_pending(&self) -> Vec<(i64, &'static str)> {
        let mut db = NetworkDatabase::open(&self.network_db).unwrap();
        let now = now_unix_seconds();
        let other_provider =
            enqueue_upload_job(&mut db, "unconfigured", "other", IrJobKind::Score, 12, now);
        let other_account =
            enqueue_upload_job(&mut db, "target-a", "account-b", IrJobKind::Score, 13, now);
        let replay =
            enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Replay, 14, now);
        let course =
            enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Course, 15, now);
        vec![
            (other_provider, "pending"),
            (other_account, "pending"),
            (replay, "pending"),
            (course, "pending"),
        ]
    }

    fn assert_job_statuses(&self, expected: &[(i64, &str)]) {
        let db = NetworkDatabase::open(&self.network_db).unwrap();
        for (job_id, expected_status) in expected {
            let status: String = db
                .conn()
                .query_row("SELECT status FROM ir_score_jobs WHERE id = ?1", [job_id], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(&status, expected_status, "job {job_id}");
        }
    }

    fn seed_local_history(&self, id: i64, chart_hash: [u8; 32], played_at: i64) {
        let conn = Connection::open(&self.score_db).unwrap();
        conn.execute(
            "INSERT INTO score_history (
                id, chart_sha256, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great, fast_good, slow_good,
                fast_bad, slow_bad, fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                gauge_option, replay_path
             ) VALUES (?1, ?2, ?3, 'Hard', 'Hard', 100.0, 2, 2, 0, 0, 2,
                       2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 'Hard', '')",
            rusqlite::params![id, crate::storage::common::hash_to_hex(&chart_hash), played_at],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO score_history_ir_identity (score_history_id, idempotency_key)
             VALUES (?1, ?2)",
            rusqlite::params![id, format!("backfill-key-{id}")],
        )
        .unwrap();
    }
}

impl Drop for UploadFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn enqueue_upload_job(
    db: &mut NetworkDatabase,
    provider: &str,
    account_id: &str,
    kind: IrJobKind,
    local_score_id: i64,
    now: i64,
) -> i64 {
    db.enqueue_ir_score_job(&NewIrScoreJob {
        provider: provider.to_string(),
        account_id: account_id.to_string(),
        kind,
        local_score_id,
        chart_sha256: [local_score_id as u8; 32],
        ln_policy: LnScorePolicy::ForceLn,
        payload_json: "{}".to_string(),
        now,
    })
    .unwrap()
}

#[tokio::test]
async fn upload_local_all_waits_for_its_sending_score_without_touching_other_jobs() {
    let fixture = UploadFixture::new("upload-all-scope");
    let expected = fixture.seed_target_sending_and_unrelated_pending();
    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let report = upload_local_all(
        &fixture.context(),
        &IrLocalUploadOptions::default(),
        now_unix_seconds(),
        &mut |_| {},
        &cancel,
    )
    .await
    .unwrap();

    assert_eq!(report.stop, IrManualStop::WaitingForSendingJob);
    assert_eq!(report.remaining, 1);
    fixture.assert_job_statuses(&expected);
}

#[tokio::test]
async fn upload_local_batch_sync_waits_for_its_sending_score_without_touching_other_jobs() {
    let fixture = UploadFixture::new("upload-batch-scope");
    let expected = fixture.seed_target_sending_and_unrelated_pending();
    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let report = upload_local_batch(
        &fixture.context(),
        &IrLocalUploadOptions::default(),
        true,
        &mut |_| {},
        &cancel,
    )
    .await
    .unwrap();

    assert_eq!(report.stop, IrManualStop::WaitingForSendingJob);
    assert_eq!(report.remaining, 1);
    fixture.assert_job_statuses(&expected);
}

#[tokio::test]
async fn empty_upload_local_batch_completes_without_touching_other_jobs() {
    let fixture = UploadFixture::new("upload-batch-empty");
    let expected = fixture.seed_only_unrelated_pending();
    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let report = upload_local_batch(
        &fixture.context(),
        &IrLocalUploadOptions::default(),
        true,
        &mut |_| {},
        &cancel,
    )
    .await
    .unwrap();

    assert_eq!(report.stop, IrManualStop::Completed);
    assert_eq!(report.remaining, 0);
    fixture.assert_job_statuses(&expected);
}

fn complete_pending_target_scores(network_path: &std::path::Path, observed: &mut Vec<i64>) {
    let mut db = NetworkDatabase::open(network_path).unwrap();
    let now = i64::MAX / 2;
    let jobs = db.pending_ir_score_jobs_ignoring_backoff(now, 100).unwrap();
    for job in jobs.into_iter().filter(|job| {
        job.provider == "target-a" && job.account_id == "account-a" && job.kind == IrJobKind::Score
    }) {
        observed.push(job.local_score_id);
        db.complete_ir_score_job(
            &crate::storage::network_db::NewIrScoreSubmission {
                job_id: job.id,
                provider: job.provider,
                account_id: job.account_id,
                kind: job.kind,
                local_score_id: job.local_score_id,
                remote_score_id: format!("mock-remote-{}", job.local_score_id),
                status: "succeeded".into(),
                submitted_at: now,
                log_path: String::new(),
                error: String::new(),
            },
            None,
            "{}",
        )
        .unwrap();
    }
}

#[tokio::test]
async fn upload_local_all_resend_finishes_two_histories_once_with_preexisting_queue() {
    let fixture = UploadFixture::new("upload-all-resend-finite");
    let first_id = 101;
    let second_id = 102;
    fixture.seed_local_history(first_id, [101; 32], 100);
    fixture.seed_local_history(second_id, [102; 32], 200);

    let mut network_db = NetworkDatabase::open(&fixture.network_db).unwrap();
    let now = now_unix_seconds();
    let pending_id = enqueue_upload_job(
        &mut network_db,
        "target-a",
        "account-a",
        IrJobKind::Score,
        first_id,
        now,
    );
    let successful_id = enqueue_upload_job(
        &mut network_db,
        "target-a",
        "account-a",
        IrJobKind::Score,
        second_id,
        now,
    );
    network_db
        .complete_ir_score_job(
            &crate::storage::network_db::NewIrScoreSubmission {
                job_id: successful_id,
                provider: "target-a".into(),
                account_id: "account-a".into(),
                kind: IrJobKind::Score,
                local_score_id: second_id,
                remote_score_id: "mock-remote-102".into(),
                status: "succeeded".into(),
                submitted_at: now,
                log_path: String::new(),
                error: String::new(),
            },
            None,
            "{}",
        )
        .unwrap();
    drop(network_db);

    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let mut observed = Vec::new();
    let mut batches_seen = 0;
    let mut progress = |event| {
        if let IrManualProgress::FullUploadBatch { batch } = &event {
            batches_seen = *batch;
            assert!(*batch <= 4, "upload_local_all did not finish in a bounded number of batches");
        }
        if matches!(event, IrManualProgress::ExistingQueued { .. })
            || matches!(event, IrManualProgress::Enqueued { all: true, .. })
        {
            complete_pending_target_scores(&fixture.network_db, &mut observed);
        }
    };
    let report = tokio::time::timeout(
        Duration::from_secs(5),
        upload_local_all(
            &fixture.context(),
            &IrLocalUploadOptions {
                provider: Some("target-a".into()),
                resend: true,
                limit: 1,
                ..Default::default()
            },
            now_unix_seconds(),
            &mut progress,
            &cancel,
        ),
    )
    .await
    .expect("full resend should terminate")
    .unwrap();

    assert_eq!(report.stop, IrManualStop::Completed);
    assert!(batches_seen <= 4);
    assert_eq!(report.enqueued, 1);
    assert_eq!(observed, [first_id, second_id]);
    fixture.assert_job_statuses(&[(pending_id, "succeeded"), (successful_id, "succeeded")]);
}

#[tokio::test]
async fn sleep_or_cancel_returns_early_on_cancel() {
    let (tx, cancel) = tokio::sync::watch::channel(false);
    let started = Instant::now();
    let sleeper =
        tokio::spawn(async move { sleep_or_cancel(Duration::from_secs(30), &cancel).await });
    tokio::time::sleep(Duration::from_millis(30)).await;
    tx.send_replace(true);
    assert!(sleeper.await.unwrap());
    assert!(started.elapsed() < Duration::from_secs(5));

    let (_tx, cancel) = tokio::sync::watch::channel(false);
    assert!(!sleep_or_cancel(Duration::from_millis(5), &cancel).await);
}

#[tokio::test]
async fn resend_job_ignores_backoff_and_skips_a_job_being_sent() {
    let fixture = Fixture::new("resend-one");
    let mut db = open_network_db();
    let failed = enqueue_unroutable_job(&mut db, 1);
    let busy = enqueue_unroutable_job(&mut db, 2);
    db.mark_ir_score_job_failed(failed, now_unix_seconds(), "boom", None).unwrap();
    // 失敗直後なので通常のclaimでは待機中。
    assert!(db.claim_pending_ir_score_jobs(now_unix_seconds(), 1, false).unwrap()[0].id == busy);

    let report =
        resend_job(&fixture.ctx(), &mut db, "unconfigured", "account-1", IrJobKind::Score, 1)
            .await
            .unwrap();
    // 再送が実際に試行された（未設定providerなので再度失敗扱い）。
    assert_eq!((report.targeted, report.failed, report.skipped_busy), (1, 1, 0));
    let counts = db.ir_score_job_status_counts(None, None).unwrap();
    assert_eq!(counts.failed, 1);

    // sending中のjobは二重に送らない。
    let report =
        resend_job(&fixture.ctx(), &mut db, "unconfigured", "account-1", IrJobKind::Score, 2)
            .await
            .unwrap();
    assert_eq!((report.failed, report.skipped_busy), (0, 1));
}

#[tokio::test]
async fn resend_failed_jobs_only_targets_failed_jobs_and_can_be_cancelled() {
    let fixture = Fixture::new("resend-bulk");
    let mut db = open_network_db();
    let failed_ids: Vec<i64> = (1..=3).map(|id| enqueue_unroutable_job(&mut db, id)).collect();
    let pending = enqueue_unroutable_job(&mut db, 9);
    for id in &failed_ids {
        db.mark_ir_score_job_failed(*id, now_unix_seconds(), "boom", None).unwrap();
    }

    let (tx, cancel) = tokio::sync::watch::channel(false);
    let mut finished = 0;
    let report = resend_failed_jobs(
        &fixture.ctx(),
        &mut db,
        "unconfigured",
        "account-1",
        &mut |event| {
            if matches!(event, IrManualProgress::JobFinished { .. }) {
                finished += 1;
                if finished == 2 {
                    tx.send_replace(true);
                }
            }
        },
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(report.targeted, 3);
    assert!(report.cancelled);
    assert_eq!(report.failed, 2);

    // pendingのjobは触らない。
    let jobs = db.pending_ir_score_jobs(i64::MAX / 2, 10).unwrap();
    assert!(jobs.iter().any(|job| job.id == pending && job.status == "pending"));

    // 中断なしなら残りも含めて全件が対象になる。
    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let report = resend_failed_jobs(
        &fixture.ctx(),
        &mut db,
        "unconfigured",
        "account-1",
        &mut |_| {},
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!((report.targeted, report.failed, report.cancelled), (3, 3, false));
}

#[tokio::test]
async fn resend_legacy_auto_outbox_retains_gameplay_and_idempotency_key() {
    let root = tempfile::tempdir().unwrap();
    let score_path = root.path().join("score.db");
    migrate_score_db(&score_path).unwrap();
    let config = IrConfig::default();
    let ctx = IrSyncContext {
        score_db: &score_path,
        profile_root: root.path(),
        logs_dir: root.path(),
        ir_config: &config,
    };
    let mut db = open_network_db();
    let original = r#"{"rule":{"ln_policy":"AutoCn","effective_ln_mode":"cn"},"idempotency_key":"legacy-auto-cn","result":{"ex_score":100}}"#;
    let id = db
        .enqueue_ir_score_job(&NewIrScoreJob {
            provider: "unconfigured".into(),
            account_id: "account-1".into(),
            kind: IrJobKind::Score,
            local_score_id: 1,
            chart_sha256: [7; 32],
            ln_policy: LnScorePolicy::AutoCn,
            payload_json: original.into(),
            now: 1,
        })
        .unwrap();
    let snapshot =
        |db: &NetworkDatabase| {
            db.conn().query_row(
        "SELECT payload_json, submission_key, ln_policy FROM ir_score_jobs WHERE id = ?1", [id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).unwrap()
        };
    let before = snapshot(&db);
    db.mark_ir_score_job_failed(id, 1, "offline", None).unwrap();
    let report =
        resend_job(&ctx, &mut db, "unconfigured", "account-1", IrJobKind::Score, 1).await.unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(snapshot(&db), before);
    assert_eq!(before.0, original);
    assert_eq!(before.2, "AutoCn");
}

impl UploadFixture {
    fn sign_in(&self) {
        crate::ir::credentials::save_credentials(
            &self.dir,
            &crate::ir::credentials::IrStoredCredentials {
                provider: "target-a".into(),
                origin: crate::ir::credentials::credential_origin(
                    &self.config.providers[0].base_url,
                )
                .unwrap(),
                account_id: "account-a".into(),
                display_name: String::new(),
                access_token: "token".into(),
                refresh_token: String::new(),
                expires_at: None,
            },
        )
        .unwrap();
    }

    /// ライブラリに譜面を登録し、そのhashを返す（local backfillの候補にするため）。
    fn seed_chart(&self, title: &str) -> [u8; 32] {
        let chart = crate::ir::backfill::tests::test_playable_chart(title);
        let mut library =
            crate::storage::library_db::LibraryDatabase::open(&self.library_db).unwrap();
        library
            .upsert_chart_import(&crate::storage::library_db::ChartImportRecord {
                root_id: None,
                file_path: std::path::Path::new("/fixture/song.bms"),
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &chart,
            })
            .unwrap();
        chart.identity.file_sha256
    }

    fn job_state(&self, job_id: i64) -> (String, u32) {
        let db = NetworkDatabase::open(&self.network_db).unwrap();
        db.conn()
            .query_row(
                "SELECT status, attempt_count FROM ir_score_jobs WHERE id = ?1",
                [job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }
}

/// 対象provider/accountのpending score jobだけを、送信せずに成功扱いにする。
fn complete_only_pending_target_scores(network_path: &std::path::Path, observed: &mut Vec<i64>) {
    let mut db = NetworkDatabase::open(network_path).unwrap();
    let now = now_unix_seconds();
    let jobs = db.pending_ir_score_jobs_ignoring_backoff(now, 100).unwrap();
    for job in jobs.into_iter().filter(|job| {
        job.status == "pending"
            && job.provider == "target-a"
            && job.account_id == "account-a"
            && job.kind == IrJobKind::Score
    }) {
        observed.push(job.local_score_id);
        db.complete_ir_score_job(
            &crate::storage::network_db::NewIrScoreSubmission {
                job_id: job.id,
                provider: job.provider,
                account_id: job.account_id,
                kind: job.kind,
                local_score_id: job.local_score_id,
                remote_score_id: format!("mock-remote-{}", job.local_score_id),
                status: "succeeded".into(),
                submitted_at: now,
                log_path: String::new(),
                error: String::new(),
            },
            None,
            "{}",
        )
        .unwrap();
    }
}

#[tokio::test]
async fn upload_local_all_sends_new_scores_past_failing_jobs_and_tries_each_once() {
    let fixture = UploadFixture::new("upload-all-past-failures");
    fixture.sign_in();
    let chart = fixture.seed_chart("upload-all-past-failures");
    fixture.seed_local_history(301, chart, 100);
    fixture.seed_local_history(302, chart, 200);
    let now = now_unix_seconds();
    let mut db = NetworkDatabase::open(&fixture.network_db).unwrap();
    // 解析できないpayloadは送信前にローカルで失敗する（内容起因の失敗）。
    let old_failure =
        enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Score, 201, now);
    db.mark_ir_score_job_failed(old_failure, now - 100, "old", Some(86_400)).unwrap();
    let pending_failure =
        enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Score, 202, now);
    drop(db);

    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let mut observed = Vec::new();
    let mut progress = |event| {
        if let IrManualProgress::FullUploadBatch { batch } = &event {
            assert!(*batch <= 5, "upload_local_all did not finish in a bounded number of batches");
        }
        if matches!(event, IrManualProgress::Enqueued { all: true, .. }) {
            complete_only_pending_target_scores(&fixture.network_db, &mut observed);
        }
    };
    let report = tokio::time::timeout(
        Duration::from_secs(10),
        upload_local_all(
            &fixture.context(),
            &IrLocalUploadOptions::default(),
            now,
            &mut progress,
            &cancel,
        ),
    )
    .await
    .expect("full upload should terminate")
    .unwrap();

    assert_eq!(report.stop, IrManualStop::FailedJobs { failed: 2 });
    assert_eq!(report.failed, 2);
    assert_eq!(report.enqueued, 2);
    assert_eq!(observed, [301, 302]);
    // 長いbackoff中の既存失敗jobも1回だけ再送し、同じ実行内では取り直さない。
    assert_eq!(fixture.job_state(old_failure), ("failed".to_string(), 2));
    assert_eq!(fixture.job_state(pending_failure), ("failed".to_string(), 1));

    // 失敗jobは残したまま、次の実行では再び1回ずつ試す。
    // 前回の失敗を次の実行開始より前の時刻にする（秒単位の時計を待たないため）。
    NetworkDatabase::open(&fixture.network_db)
        .unwrap()
        .conn()
        .execute(
            "UPDATE ir_score_jobs SET updated_at = updated_at - 10 WHERE status = 'failed'",
            [],
        )
        .unwrap();
    let report = upload_local_all(
        &fixture.context(),
        &IrLocalUploadOptions::default(),
        now_unix_seconds(),
        &mut |_| {},
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(report.stop, IrManualStop::FailedJobs { failed: 2 });
    assert_eq!(report.enqueued, 0);
    assert_eq!(fixture.job_state(old_failure), ("failed".to_string(), 3));
}

#[tokio::test]
async fn upload_local_all_stops_after_consecutive_failures() {
    let fixture = UploadFixture::new("upload-all-consecutive");
    fixture.sign_in();
    let now = now_unix_seconds();
    let mut db = NetworkDatabase::open(&fixture.network_db).unwrap();
    let jobs = (1..=IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT + 1)
        .map(|id| {
            enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Score, id.into(), now)
        })
        .collect::<Vec<_>>();
    drop(db);

    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let report = upload_local_all(
        &fixture.context(),
        &IrLocalUploadOptions::default(),
        now,
        &mut |_| {},
        &cancel,
    )
    .await
    .unwrap();

    assert_eq!(
        report.stop,
        IrManualStop::ConsecutiveFailures { consecutive: IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT }
    );
    assert_eq!(report.failed, IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT);
    let statuses = jobs.iter().map(|job| fixture.job_state(*job).0).collect::<Vec<_>>();
    assert_eq!(statuses.iter().filter(|status| *status == "failed").count(), 10);
    assert_eq!(statuses.iter().filter(|status| *status == "pending").count(), 1);
}

#[tokio::test]
async fn upload_local_all_stops_immediately_when_not_signed_in() {
    let fixture = UploadFixture::new("upload-all-unauthorized");
    let now = now_unix_seconds();
    let mut db = NetworkDatabase::open(&fixture.network_db).unwrap();
    let first = enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Score, 1, now);
    let second = enqueue_upload_job(&mut db, "target-a", "account-a", IrJobKind::Score, 2, now);
    drop(db);

    let (_tx, cancel) = tokio::sync::watch::channel(false);
    let report = upload_local_all(
        &fixture.context(),
        &IrLocalUploadOptions::default(),
        now,
        &mut |_| {},
        &cancel,
    )
    .await
    .unwrap();

    assert_eq!(report.stop, IrManualStop::Unauthorized);
    assert_eq!(report.failed, 1);
    let mut statuses = [fixture.job_state(first).0, fixture.job_state(second).0];
    statuses.sort();
    assert_eq!(statuses, ["failed".to_string(), "pending".to_string()]);
}

#[test]
fn failure_breaker_resets_on_success_and_trips_on_fatal_classes() {
    use crate::ir::sync::{IrSyncFailure, IrSyncFailureClass};
    let failed = |class| IrSyncReport {
        failed: 1,
        failures: vec![IrSyncFailure { job_id: 1, class }],
        ..Default::default()
    };
    let succeeded = IrSyncReport { submitted: 1, ..Default::default() };

    let mut breaker = IrFailureBreaker::default();
    for _ in 1..IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT {
        assert_eq!(breaker.record(&failed(IrSyncFailureClass::Transient)), None);
    }
    assert_eq!(breaker.record(&succeeded), None);
    for _ in 1..IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT {
        assert_eq!(breaker.record(&failed(IrSyncFailureClass::Job)), None);
    }
    assert_eq!(
        breaker.record(&failed(IrSyncFailureClass::Job)),
        Some(IrManualStop::ConsecutiveFailures {
            consecutive: IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT
        })
    );

    let mut breaker = IrFailureBreaker::default();
    assert_eq!(
        breaker.record(&failed(IrSyncFailureClass::Unauthorized)),
        Some(IrManualStop::Unauthorized)
    );
    assert_eq!(
        breaker.record(&failed(IrSyncFailureClass::RateLimited { retry_after_seconds: Some(30) })),
        Some(IrManualStop::RateLimited { retry_after_seconds: Some(30) })
    );
}
