use super::*;
use crate::storage::common::configure_connection;
use crate::storage::migration::{NETWORK_MIGRATIONS, SCORE_MIGRATIONS, run_migrations};
use crate::storage::network_db::{IrJobKind, NewIrScoreJob, NewIrScoreSubmission};
use rusqlite::Connection;

fn open_score_db() -> ScoreDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    ScoreDatabase::from_connection(conn)
}

fn open_network_db() -> NetworkDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, NETWORK_MIGRATIONS).unwrap();
    NetworkDatabase::from_connection(conn)
}

fn score_entry(id: &str) -> IrOwnScoreHistoryEntry {
    IrOwnScoreHistoryEntry {
        auto_scratch_scoring: None,
        score_id: id.to_string(),
        idempotency_key: None,
        chart_sha256: "11".repeat(32),
        clear: "Hard".to_string(),
        ex_score: 21,
        max_combo: 12,
        min_bp: 1,
        min_cb: 1,
        bp: 1,
        cb: 1,
        gauge: "HARD".to_string(),
        ln_policy: "ForceLn".to_string(),
        double_option: "off".to_string(),
        applied_double_option: "flip".to_string(),
        source_kind: "beatoraja".to_string(),
        rule_mode: "Beatoraja".to_string(),
        judges: IrJudgePayload {
            fast: super::super::types::IrJudgeSidePayload {
                pgreat: 10,
                great: 1,
                good: 0,
                bad: 0,
                poor: 0,
                empty_poor: 0,
            },
            slow: super::super::types::IrJudgeSidePayload {
                pgreat: 0,
                great: 0,
                good: 0,
                bad: 1,
                poor: 0,
                empty_poor: 0,
            },
        },
        notes: 12,
        pass_notes: 12,
        duration_ms: Some(123_000),
        device_type: "controller".to_string(),
        arrange_1p: Some("random".to_string()),
        arrange_2p: None,
        random_seed: Some(123),
        seed_scheme: "beatoraja_24bit_v1".to_string(),
        assist_mask: Some(4),
        played_at: Some(1_700_000_000),
        server_received_at: 1_700_000_005,
        verification: "signed".to_string(),
        replay_hash: None,
    }
}

#[test]
fn score_record_from_ir_preserves_judges_and_options() {
    let record = score_record_from_ir_entry(&score_entry("remote-1")).unwrap();

    assert_eq!(record.chart_sha256, [0x11; 32]);
    assert_eq!(record.clear_type, ClearType::Hard);
    assert_eq!(record.gauge_type, Some(GaugeType::Hard));
    assert_eq!(record.gauge_value, None);
    assert_eq!(record.score.ex_score(), 21);
    assert_eq!(record.score.bp(), 1);
    assert!(!record.count_unprocessed_notes);
    assert_eq!(record.random_seed, Some(123));
    assert_eq!(record.playtime_seconds, 123);
    assert_eq!(record.seed_scheme, "beatoraja_24bit_v1");
    assert_eq!(record.arrange, "Random");
    assert_eq!(record.applied_double_option, DoubleOption::Flip);
    assert_eq!(record.source_kind, ScoreSourceKind::Beatoraja);
    assert_eq!(record.assist_mask, 4);
    assert_eq!(record.device_type, InputDeviceKind::Controller);
}

#[test]
fn bmz_history_download_skips_legacy_local_battle_as_and_keeps_valid_scores() {
    let mut db = open_score_db();
    let network = open_network_db();
    let mut legacy = score_entry("old-local");
    legacy.double_option = "battle_auto_scratch".into();
    legacy.applied_double_option.clone_from(&legacy.double_option);
    legacy.source_kind = "local".into();
    let imported = IrOwnScoreHistoryEntry {
        score_id: "imported".into(),
        source_kind: "beatoraja".into(),
        ..legacy.clone()
    };
    let new = IrOwnScoreHistoryEntry {
        score_id: "new-local".into(),
        auto_scratch_scoring: Some("unscored".into()),
        ..legacy.clone()
    };
    let no_source = IrOwnScoreHistoryEntry {
        score_id: "no-source".into(),
        source_kind: String::new(),
        ..legacy.clone()
    };
    let mut report = IrScoreDownloadReport::default();
    import_ir_score_entries(
        &mut db,
        &network,
        "bmz",
        "account",
        &[legacy, imported, new, no_source],
        false,
        1,
        &mut report,
    )
    .unwrap();
    assert_eq!((report.imported, report.failed, report.scanned), (2, 0, 4));
    assert_eq!(report.messages.len(), 2);
    assert_eq!(db.all_best_scores().unwrap().len(), 1);
}

#[test]
fn score_record_from_ir_reconstructs_unprocessed_failed_bp() {
    let mut entry = score_entry("remote-1");
    entry.clear = "Failed".to_string();
    entry.gauge = "HARD".to_string();
    entry.notes = 20;
    entry.pass_notes = 12;
    entry.min_bp = 9;
    entry.min_cb = 9;

    let record = score_record_from_ir_entry(&entry).unwrap();

    assert!(record.count_unprocessed_notes);
    assert_eq!(record.score.past_notes, 12);
    assert_eq!(record.score.bp_with_unprocessed_notes(record.total_notes), 9);
    assert_eq!(record.score.cb_with_unprocessed_notes(record.total_notes), 9);
}

#[test]
fn score_record_from_ir_recovers_legacy_failed_backfill_pass_notes_strictly() {
    let mut entry = score_entry("legacy-failed-backfill");
    entry.clear = "Failed".to_string();
    entry.ex_score = 21;
    entry.notes = 20;
    entry.pass_notes = 20;
    entry.bp = 1;
    entry.cb = 1;
    entry.min_bp = 9;
    entry.min_cb = 9;

    let record = score_record_from_ir_entry(&entry).unwrap();
    assert!(record.count_unprocessed_notes);
    assert_eq!(record.score.past_notes, 12);
    assert_eq!(record.score.bp_with_unprocessed_notes(20), 9);
    assert_eq!(record.score.cb_with_unprocessed_notes(20), 9);

    let mut mismatched_judges = entry.clone();
    mismatched_judges.judges.fast.pgreat = 9;
    mismatched_judges.ex_score = 19;
    assert!(score_record_from_ir_entry(&mismatched_judges).is_err());

    let mut asymmetric_delta = entry.clone();
    asymmetric_delta.min_cb = 8;
    assert!(score_record_from_ir_entry(&asymmetric_delta).is_err());

    let mut non_failed = entry.clone();
    non_failed.clear = "Hard".to_string();
    assert!(score_record_from_ir_entry(&non_failed).is_err());

    let mut pass_notes_not_total = entry;
    pass_notes_not_total.pass_notes = 19;
    assert!(score_record_from_ir_entry(&pass_notes_not_total).is_err());
}

#[test]
fn import_ir_score_entries_inserts_once_and_skips_source_duplicate() {
    let mut score_db = open_score_db();
    let network_db = open_network_db();
    let mut report = IrScoreDownloadReport::default();

    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &[score_entry("remote-1")],
        false,
        1_800_000_000,
        &mut report,
    )
    .unwrap();
    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &[score_entry("remote-1")],
        false,
        1_800_000_000,
        &mut report,
    )
    .unwrap();

    assert_eq!(report.imported, 1);
    assert_eq!(report.skipped_existing, 1);
    let count: i64 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let history_gauge: Option<f32> = score_db
        .conn()
        .query_row("SELECT gauge_value FROM score_history", [], |row| row.get(0))
        .unwrap();
    let best_gauge: Option<f32> = score_db
        .conn()
        .query_row("SELECT gauge_value FROM score_best", [], |row| row.get(0))
        .unwrap();
    assert_eq!(history_gauge, None);
    assert_eq!(best_gauge, None);
}

#[test]
fn mixed_valid_and_invalid_rows_keep_valid_imports_and_retry_without_duplicates() {
    let mut score_db = open_score_db();
    let network_db = open_network_db();
    let mut invalid = score_entry("invalid-row");
    invalid.clear = "unknown-clear".to_string();
    let entries = [score_entry("valid-row"), invalid];

    let mut first = IrScoreDownloadReport::default();
    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &entries,
        false,
        1_800_000_000,
        &mut first,
    )
    .unwrap();
    assert_eq!(first.imported, 1);
    assert_eq!(first.failed, 1);

    let mut retry = IrScoreDownloadReport::default();
    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &entries,
        false,
        1_800_000_001,
        &mut retry,
    )
    .unwrap();
    assert_eq!(retry.imported, 0);
    assert_eq!(retry.skipped_existing, 1);
    assert_eq!(retry.failed, 1);

    let count: i64 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn import_ir_score_entries_links_previously_uploaded_local_score() {
    let mut score_db = open_score_db();
    let mut network_db = open_network_db();
    let mut local_record = score_record_from_ir_entry(&score_entry("local")).unwrap();
    local_record.gauge_value = Some(64.0);
    let local_history_id = score_db.insert_score(&local_record).unwrap();
    let job_id = network_db
        .enqueue_ir_score_job(&NewIrScoreJob {
            provider: "provider-1".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: local_history_id,
            chart_sha256: [0x11; 32],
            ln_policy: LnScorePolicy::ForceLn,
            payload_json: "{}".to_string(),
            now: 1,
        })
        .unwrap();
    network_db
        .insert_ir_score_submission(&NewIrScoreSubmission {
            job_id,
            provider: "provider-1".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: local_history_id,
            remote_score_id: "remote-1".to_string(),
            status: "succeeded".to_string(),
            submitted_at: 2,
            log_path: String::new(),
            error: String::new(),
        })
        .unwrap();
    let mut report = IrScoreDownloadReport::default();

    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &[score_entry("remote-1")],
        false,
        1_800_000_000,
        &mut report,
    )
    .unwrap();

    assert_eq!(report.imported, 0);
    assert_eq!(report.linked_existing, 1);
    let count: i64 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn pruning_old_success_keeps_upload_and_remote_link_identity() {
    let mut score_db = open_score_db();
    let mut network_db = open_network_db();
    let local_record = score_record_from_ir_entry(&score_entry("local")).unwrap();
    let local_history_id = score_db.insert_score(&local_record).unwrap();

    for index in 0..501_i64 {
        let local_score_id = if index == 0 { local_history_id } else { 10_000 + index };
        let job_id = network_db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "provider-1".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id,
                chart_sha256: [0x11; 32],
                ln_policy: LnScorePolicy::ForceLn,
                payload_json: format!("{{\"idempotency_key\":\"submission-{index}\"}}"),
                now: index,
            })
            .unwrap();
        let remote_score_id = if index == 0 { "remote-old".to_string() } else { String::new() };
        network_db
            .complete_ir_score_job(
                &NewIrScoreSubmission {
                    job_id,
                    provider: "provider-1".to_string(),
                    account_id: "account-1".to_string(),
                    kind: IrJobKind::Score,
                    local_score_id,
                    remote_score_id,
                    status: "succeeded".to_string(),
                    submitted_at: index,
                    log_path: String::new(),
                    error: String::new(),
                },
                None,
                "{}",
            )
            .unwrap();
    }

    assert_eq!(network_db.prune_succeeded_ir_score_jobs(31 * 24 * 60 * 60 + 1).unwrap(), 1);
    assert!(
        network_db
            .has_successful_ir_score_submission(
                "provider-1",
                "account-1",
                IrJobKind::Score,
                local_history_id
            )
            .unwrap()
    );
    assert_eq!(
        network_db
            .local_score_id_for_remote_score("provider-1", "account-1", "remote-old")
            .unwrap(),
        Some(local_history_id)
    );

    let play_count_before: i64 = score_db
        .conn()
        .query_row("SELECT play_count FROM score_best", [], |row| row.get(0))
        .unwrap();

    let mut report = IrScoreDownloadReport::default();
    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &[score_entry("remote-old")],
        false,
        100_001,
        &mut report,
    )
    .unwrap();

    assert_eq!(report.linked_existing, 1);
    assert_eq!(report.imported, 0);
    let score_count: i64 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(score_count, 1);
    let play_count_after: i64 = score_db
        .conn()
        .query_row("SELECT play_count FROM score_best", [], |row| row.get(0))
        .unwrap();
    assert_eq!(play_count_after, play_count_before);
}

#[test]
fn import_limit_skips_known_scores_before_importing_next_candidate() {
    let mut score_db = open_score_db();
    let network_db = open_network_db();
    let entries = [
        score_entry("remote-1"),
        score_entry("remote-2"),
        score_entry("remote-3"),
        score_entry("remote-4"),
    ];
    let mut initial_report = IrScoreDownloadReport::default();
    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &entries[..2],
        false,
        1_800_000_000,
        &mut initial_report,
    )
    .unwrap();
    let mut next_report = IrScoreDownloadReport::default();

    let consumed = import_ir_score_entries_up_to(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &entries,
        true,
        false,
        1_800_000_001,
        &mut next_report,
        Some(1),
    )
    .unwrap();

    assert_eq!(consumed, 3);
    assert_eq!(next_report.scanned, 3);
    assert_eq!(next_report.skipped_existing, 2);
    assert_eq!(next_report.imported, 1);
    let count: i64 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 3);
}

#[test]
fn dry_run_reports_invalid_scores_without_counting_them_as_candidates() {
    let mut score_db = open_score_db();
    let network_db = open_network_db();
    let mut invalid = score_entry("remote-invalid");
    invalid.ex_score += 1;
    let mut report = IrScoreDownloadReport::default();

    import_ir_score_entries(
        &mut score_db,
        &network_db,
        "provider-1",
        "account-1",
        &[invalid],
        true,
        1_800_000_000,
        &mut report,
    )
    .unwrap();

    assert_eq!(report.candidates, 0);
    assert_eq!(report.failed, 1);
    assert_eq!(report.messages.len(), 1);
    let count: i64 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

fn keyed_entry(id: &str, key: &str) -> IrOwnScoreHistoryEntry {
    IrOwnScoreHistoryEntry { idempotency_key: Some(key.to_string()), ..score_entry(id) }
}

fn history_count(score_db: &ScoreDatabase) -> i64 {
    score_db.conn().query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0)).unwrap()
}

fn import_one(
    score_db: &mut ScoreDatabase,
    network_db: &NetworkDatabase,
    entry: IrOwnScoreHistoryEntry,
) -> IrScoreDownloadReport {
    let mut report = IrScoreDownloadReport::default();
    import_ir_score_entries(
        score_db,
        network_db,
        "provider-1",
        "account-1",
        &[entry],
        false,
        1_800_000_000,
        &mut report,
    )
    .unwrap();
    report
}

#[test]
fn score_sent_without_a_recorded_response_links_by_submission_key() {
    // 送信は届いたが応答を失いjobはfailedのまま（成功台帳にremote IDが無い）。
    let mut score_db = open_score_db();
    let mut network_db = open_network_db();
    let local_history_id =
        score_db.insert_score(&score_record_from_ir_entry(&score_entry("local")).unwrap()).unwrap();
    let job_id = network_db
        .enqueue_ir_score_job(&NewIrScoreJob {
            provider: "provider-1".to_string(),
            account_id: "account-1".to_string(),
            kind: IrJobKind::Score,
            local_score_id: local_history_id,
            chart_sha256: [0x11; 32],
            ln_policy: LnScorePolicy::ForceLn,
            payload_json: r#"{"idempotency_key":"play-key-1"}"#.to_string(),
            now: 1,
        })
        .unwrap();
    network_db.mark_ir_score_job_failed(job_id, 2, "response lost", None).unwrap();

    let report = import_one(&mut score_db, &network_db, keyed_entry("remote-1", "play-key-1"));

    assert_eq!((report.imported, report.linked_existing), (0, 1));
    assert_eq!(history_count(&score_db), 1);
    let linked = score_db
        .score_history_id_for_source(&ScoreHistorySourceKey {
            source: IR_SCORE_DOWNLOAD_SOURCE.to_string(),
            provider: "provider-1".to_string(),
            account_id: "account-1".to_string(),
            remote_score_id: "remote-1".to_string(),
        })
        .unwrap();
    assert_eq!(linked, Some(local_history_id));
}

#[test]
fn backfilled_score_links_by_history_identity_without_network_records() {
    // network.dbを失った後でも、local backfillが送ったkeyは履歴のidentityに残る。
    let mut score_db = open_score_db();
    let network_db = open_network_db();
    let local_history_id =
        score_db.insert_score(&score_record_from_ir_entry(&score_entry("local")).unwrap()).unwrap();
    let key = score_db.ir_submission_idempotency_key_for_history(local_history_id).unwrap();

    let report = import_one(&mut score_db, &network_db, keyed_entry("remote-1", &key));

    assert_eq!((report.imported, report.linked_existing), (0, 1));
    assert_eq!(history_count(&score_db), 1);
}

#[test]
fn submission_key_for_a_different_chart_is_not_linked() {
    let mut score_db = open_score_db();
    let network_db = open_network_db();
    let local_history_id =
        score_db.insert_score(&score_record_from_ir_entry(&score_entry("local")).unwrap()).unwrap();
    let key = score_db.ir_submission_idempotency_key_for_history(local_history_id).unwrap();
    let entry =
        IrOwnScoreHistoryEntry { chart_sha256: "22".repeat(32), ..keyed_entry("remote-1", &key) };

    let report = import_one(&mut score_db, &network_db, entry);

    assert_eq!((report.imported, report.linked_existing), (1, 0));
    assert_eq!(history_count(&score_db), 2);
}

#[test]
fn own_score_history_entry_reads_optional_idempotency_key() {
    let mut value = serde_json::to_value(score_entry("remote-1")).unwrap();
    value.as_object_mut().unwrap().remove("idempotency_key");
    let without: IrOwnScoreHistoryEntry = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(without.idempotency_key, None);
    value["idempotency_key"] = serde_json::json!("key-1");
    let with: IrOwnScoreHistoryEntry = serde_json::from_value(value).unwrap();
    assert_eq!(with.idempotency_key.as_deref(), Some("key-1"));
}

/// 1ページ目は1件と次のcursorを返し、2ページ目は500を返すIRサーバー。
fn spawn_failing_second_page_server() -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let first_page = serde_json::json!({
        "scores": [score_entry("remote-1")],
        "pagination": {
            "limit": 100,
            "offset": 0,
            "total": 2,
            "has_more": true,
            "next_cursor": { "server_received_at_ms": 1, "score_id": "remote-1" }
        }
    })
    .to_string();
    let server = std::thread::spawn(move || {
        for body in [Some(first_page), None] {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            let (status, body) = match body {
                Some(body) => ("200 OK", body),
                None => ("500 Internal Server Error", r#"{"statusMessage":"boom"}"#.to_string()),
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    (format!("http://{address}"), server)
}

#[tokio::test]
async fn later_page_failure_keeps_and_reports_pages_already_imported() {
    let (base_url, server) = spawn_failing_second_page_server();
    let client =
        IrOwnScoreHistoryClient::BmzOfficial(BmzOfficialIrClient::new(&base_url, "token").unwrap());
    let mut score_db = open_score_db();
    let network_db = open_network_db();
    let mut pages = Vec::new();

    let error = download_ir_scores_with_hooks(
        &client,
        "provider-1",
        "account-1",
        &mut score_db,
        &network_db,
        &IrScoreDownloadOptions::default(),
        1_800_000_000,
        &mut |report| pages.push(report.clone()),
        None,
    )
    .await
    .unwrap_err();
    server.join().unwrap();

    assert!(format!("{error:#}").contains("500"));
    // 呼び出し側（CLIの集計表示・GUIの一覧更新）は、失敗前のページ通知から保存済み件数を知る。
    assert_eq!(pages.len(), 1);
    assert_eq!((pages[0].pages, pages[0].imported), (1, 1));
    assert_eq!(
        score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
