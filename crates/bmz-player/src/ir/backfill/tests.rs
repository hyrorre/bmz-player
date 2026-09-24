use rusqlite::Connection;

use super::*;
use crate::storage::common::configure_connection;
use crate::storage::migration::{NETWORK_MIGRATIONS, run_migrations};
use crate::storage::network_db::NewIrScoreSubmission;

fn test_network_db() -> NetworkDatabase {
    let mut connection = Connection::open_in_memory().unwrap();
    configure_connection(&connection).unwrap();
    run_migrations(&mut connection, NETWORK_MIGRATIONS).unwrap();
    NetworkDatabase::from_connection(connection)
}

#[test]
fn local_upload_defaults_use_the_fast_cli_batch_size() {
    assert_eq!(DEFAULT_UPLOAD_LOCAL_LIMIT, 200);
    assert_eq!(IrLocalUploadOptions::default().limit, 200);
}

#[test]
fn rian_ir_local_backfill_target_is_disabled() {
    let mut provider = IrProviderConfig::rian_ir();
    provider.enabled = true;
    provider.provider_key = "rian-ir".to_string();
    provider.account_id = "player".to_string();

    let error = target_from_provider(&provider).unwrap_err();
    assert_eq!(error.to_string(), "rianIR local score backfill is disabled");
}

fn test_row() -> BackfillScoreRow {
    BackfillScoreRow {
        id: 42,
        chart_sha256: [2; 32],
        ln_policy: LnScorePolicy::ForceCn,
        double_option: DoubleOptionScoreBucket::Battle,
        applied_double_option: DoubleOption::Battle,
        played_at: 1234,
        clear_type: "Hard".to_string(),
        gauge_type: "Hard".to_string(),
        total_notes: 1000,
        ex_score: 1500,
        bp: 12,
        cb: 3,
        max_combo: 456,
        fast_pgreat: 10,
        slow_pgreat: 20,
        fast_great: 30,
        slow_great: 40,
        fast_good: 5,
        slow_good: 6,
        fast_bad: 7,
        slow_bad: 8,
        fast_poor: 9,
        slow_poor: 10,
        fast_empty_poor: 11,
        slow_empty_poor: 12,
        random_seed: Some(99),
        seed_scheme: crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1.to_string(),
        arrange: ArrangeOption::Random,
        arrange_2p: ArrangeOption::Mirror,
        gauge_option: String::new(),
        rule_mode: "Beatoraja".to_string(),
        assist_mask: 4,
        autoplay: false,
        device_type: InputDeviceKind::Controller,
        replay_path: None,
        course_score_id: None,
        source_kind: ScoreSourceKind::Beatoraja,
    }
}

fn test_chart() -> ChartListItem {
    ChartListItem {
        chart_id: 7,
        md5: [1; 16],
        sha256: [2; 32],
        title: "title".to_string(),
        subtitle: "sub".to_string(),
        artist: "artist".to_string(),
        subartist: "subartist".to_string(),
        genre: "genre".to_string(),
        difficulty_name: "Another".to_string(),
        play_level: "12".to_string(),
        mode: "7K".to_string(),
        total_notes: 1000,
        initial_bpm: 150.0,
        min_bpm: 100.0,
        max_bpm: 200.0,
        length_ms: 120_000,
        folder_path: "/songs".to_string(),
        stage_file: String::new(),
        banner_file: String::new(),
        backbmp_file: String::new(),
        preview_file: String::new(),
        has_document: false,
        has_bga: false,
        has_long_notes: true,
        has_mines: true,
        has_bms_random: false,
        judge_rank: Some(100),
        bms_total: 300.0,
        ln_profile: crate::ln_policy::ChartLnProfile {
            has_undefined_ln: false,
            has_defined_ln: false,
            has_defined_cn: true,
            has_defined_hcn: false,
            has_defined_hln: false,
        },
        ln_counts: crate::ln_policy::ChartLnCounts { defined_cn_pairs: 50, ..Default::default() },
    }
}

fn test_analysis() -> ChartAnalysis {
    ChartAnalysis {
        normal_notes: 950,
        long_notes: 50,
        scratch_notes: 10,
        long_scratch_notes: 0,
        density: 1.0,
        peak_density: 2.0,
        end_density: 0.5,
        total_gauge: 300.0,
        main_bpm: 150.0,
        distribution: Vec::new(),
        speed_changes: Vec::new(),
        lane_notes: vec![crate::storage::library_db::ChartLaneNotes {
            lane_index: 0,
            normal_notes: 1,
            long_notes: 2,
            mines: 3,
        }],
    }
}

#[test]
fn local_backfill_payload_marks_submission_source_and_omits_evidence() {
    let payload =
        build_local_score_submission(&test_row(), &test_chart(), Some(&test_analysis()), None);

    assert!(is_local_backfill_submission(&payload));
    assert!(payload.evidence.is_empty());
    assert_eq!(payload.idempotency_key, "bmz-score-42");
    assert_eq!(payload.play_options["submission_source"], LOCAL_BACKFILL_SOURCE);
    assert_eq!(payload.play_options["local_score_history_id"], 42);
}

#[test]
fn local_backfill_payload_uses_history_counts_and_options() {
    let payload =
        build_local_score_submission(&test_row(), &test_chart(), Some(&test_analysis()), None);

    assert_eq!(payload.rule.ln_policy, LnScorePolicy::ForceCn);
    assert_eq!(payload.rule.effective_ln_mode, IrEffectiveLnMode::Cn);
    assert_eq!(payload.rule.gauge, "Hard");
    assert_eq!(payload.result.ex_score, 1500);
    assert_eq!(payload.result.duration_ms, None);
    assert!(serde_json::to_value(&payload).unwrap()["result"].get("duration_ms").is_none());
    assert_eq!(payload.result.judges.fast.pgreat, 10);
    assert_eq!(payload.result.judges.slow.empty_poor, 12);
    assert_eq!(payload.play_options["arrange_1p"], "random");
    assert_eq!(payload.play_options["arrange_2p"], "mirror");
    assert_eq!(payload.play_options["double_option"], "battle");
    assert_eq!(payload.play_options["applied_double_option"], "battle");
    assert_eq!(payload.play_options["source_kind"], "beatoraja");
    assert_eq!(payload.play_options["device_type"], "controller");
    assert_eq!(payload.play_options["assist_mask"], 4);
}

#[test]
fn local_backfill_payload_resolves_mixed_auto_mode_from_library_profile() {
    let mut row = test_row();
    row.ln_policy = LnScorePolicy::AutoLn;
    let mut chart = test_chart();
    chart.ln_profile.has_defined_ln = true;

    let payload = build_local_score_submission(&row, &chart, Some(&test_analysis()), None);

    assert_eq!(payload.rule.ln_policy, LnScorePolicy::AutoLn);
    assert_eq!(payload.rule.effective_ln_mode, IrEffectiveLnMode::Cn);
    assert!(payload.chart.ln_profile.has_defined_ln);
    assert!(payload.chart.ln_profile.has_defined_cn);
}

#[test]
fn local_backfill_payload_keeps_flip_separate_from_off_score_bucket() {
    let mut row = test_row();
    row.double_option = DoubleOptionScoreBucket::Off;
    row.applied_double_option = DoubleOption::Flip;

    let payload = build_local_score_submission(&row, &test_chart(), Some(&test_analysis()), None);

    assert_eq!(payload.play_options["double_option"], "off");
    assert_eq!(payload.play_options["applied_double_option"], "flip");
}

#[test]
fn local_backfill_chart_payload_uses_library_metadata() {
    let payload =
        build_local_score_submission(&test_row(), &test_chart(), Some(&test_analysis()), None);

    assert_eq!(payload.chart.sha256, hash_to_hex(&[2; 32]));
    assert_eq!(payload.chart.level, Some(12));
    assert_eq!(payload.chart.notes.total, 2100);
    assert_eq!(payload.result.notes, 2100);
    assert_eq!(payload.chart.notes.ln, 50);
    assert_eq!(payload.chart.notes.cn, 50);
    assert_eq!(payload.chart.notes.mine, 3);
    assert!(payload.chart.features.cn);
    assert!(payload.chart.features.mine);
}

#[test]
fn existing_queue_is_distinct_from_successful_submission() {
    let mut network_db = test_network_db();
    let target = TargetProvider {
        provider_key: "bmz-official".to_string(),
        account_id: "account-1".to_string(),
    };
    let job_id = network_db
        .enqueue_ir_score_job(&NewIrScoreJob {
            provider: target.provider_key.clone(),
            account_id: target.account_id.clone(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            chart_sha256: [2; 32],
            ln_policy: LnScorePolicy::ForceCn,
            payload_json: "{}".to_string(),
            now: 100,
        })
        .unwrap();

    assert_eq!(existing_score_state(&network_db, &target, 42).unwrap(), ExistingScoreState::Queued);

    network_db
        .insert_ir_score_submission(&NewIrScoreSubmission {
            job_id,
            provider: target.provider_key.clone(),
            account_id: target.account_id.clone(),
            kind: IrJobKind::Score,
            local_score_id: 42,
            remote_score_id: "remote-42".to_string(),
            status: "succeeded".to_string(),
            submitted_at: 120,
            log_path: String::new(),
            error: String::new(),
        })
        .unwrap();
    assert_eq!(
        existing_score_state(&network_db, &target, 42).unwrap(),
        ExistingScoreState::Submitted
    );
}
