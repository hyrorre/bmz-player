use super::*;
use crate::ir::bokutachi::Game;
use crate::ir::bokutachi::tests::{Reply, server, setup};
use crate::storage::network_db::{IrJobKind, IrJobRetryFilter, NewIrScoreJob};
use serde_json::{Value, json};

pub(crate) fn stage(index: usize, mode: KeyMode) -> StageEvidence {
    StageEvidence {
        chart_md5: format!("{:032x}", index + 1),
        chart_sha256: format!("{:064x}", index + 1),
        score_history_id: index as i64 + 1,
        played_at: 100 + index as i64,
        source_key_mode: Some(mode),
        primary_key_mode: mode,
        conversion: KeyModeConversionConfig::Off,
        native_scoring: true,
        ln_policy: LnScorePolicy::ForceLn,
        rule_mode: RuleMode::Lr2Oraja,
        gauge: GaugeType::Class,
        clear: ClearType::Normal,
        gauge_value: 80.0,
        gauge_carry: vec![(GaugeType::Class, 80.0)],
        completed: true,
        normal_score: true,
        has_bms_random: false,
        arrange: ArrangeOption::Normal,
        arrange_2p: ArrangeOption::Normal,
        double_option: DoubleOption::Off,
    }
}

fn play(mode: KeyMode) -> DanPlay {
    DanPlay {
        course_id: 7,
        course_key: "original-attempt".into(),
        release: true,
        constraints: CourseConstraints::default(),
        ln_policy: LnScorePolicy::ForceLn,
        rule_mode: RuleMode::Lr2Oraja,
        final_gauge: GaugeType::Class,
        final_clear: ClearType::Normal,
        course_clear: true,
        course_failed: false,
        stages: std::array::from_fn(|i| stage(i, mode)),
    }
}

#[test]
fn ineligible_captured_courses_do_not_create_outbox_payloads() {
    let provider = IrProviderConfig::bokutachi();
    let mut not_cleared = play(KeyMode::K7);
    not_cleared.course_clear = false;
    let mut private = play(KeyMode::K14);
    private.release = false;
    for bad in [play(KeyMode::K9), not_cleared, private] {
        // capture validates the request before the enqueue boundary.
        let capture = bad.request().map(|_| bad);
        assert!(capture.is_err());
        assert!(DanJob::new(&provider, 1, &capture).unwrap().is_none());
    }
    let missing_stages = Err(anyhow::anyhow!("four actually played stages are required"));
    assert!(DanJob::new(&provider, 1, &missing_stages).unwrap().is_none());
    assert!(DanJob::new(&provider, 1, &Ok(play(KeyMode::K7))).unwrap().is_some());
}

#[test]
fn eligibility_checks_each_stage_and_actual_course_clear() {
    for mode in [KeyMode::K7, KeyMode::K14] {
        let good = play(mode);
        let wire = serde_json::to_value(good.request().unwrap()).unwrap();
        assert_eq!(wire["score"], json!({"clear":"Normal","option":0,"lntype":0}));
        assert_eq!(
            wire["course"]["charts"],
            json!(good.stages.each_ref().map(|s| json!({"md5":s.chart_md5})))
        );
        for index in 0..4 {
            let mutations: Vec<fn(&mut StageEvidence)> = vec![
                |s| s.chart_md5.clear(),
                |s| s.chart_md5 = "ab".repeat(32),
                |s| s.chart_sha256.clear(),
                |s| s.completed = false,
                |s| s.clear = ClearType::Failed,
                |s| s.clear = ClearType::Easy,
                |s| s.normal_score = false,
                |s| s.has_bms_random = true,
                |s| s.source_key_mode = None,
                |s| s.primary_key_mode = KeyMode::K9,
                |s| s.conversion = KeyModeConversionConfig::SevenToNine,
                |s| s.ln_policy = LnScorePolicy::AutoLn,
                |s| s.ln_policy = LnScorePolicy::ForceCn,
                |s| s.rule_mode = RuleMode::Beatoraja,
                |s| s.gauge = GaugeType::Normal,
                |s| s.gauge = GaugeType::AssistEasy,
                |s| s.gauge_value = 0.0,
                |s| s.gauge_carry.clear(),
                |s| s.gauge_carry[0].1 = 0.0,
                |s| s.gauge_carry[0].1 = f32::NAN,
                |s| s.gauge_carry.push(s.gauge_carry[0]),
                |s| s.score_history_id = 0,
                |s| s.native_scoring = false,
            ];
            for mutate in mutations {
                let mut bad = good.clone();
                mutate(&mut bad.stages[index]);
                assert!(bad.request().is_err());
            }
        }
        let mutations: Vec<fn(&mut DanPlay)> = vec![
            |p| p.release = false,
            |p| p.course_clear = false,
            |p| p.course_failed = true,
            |p| p.final_clear = ClearType::Easy,
            |p| p.final_clear = ClearType::LightAssistEasy,
            |p| p.final_clear = ClearType::NoPlay,
            |p| p.final_gauge = GaugeType::Easy,
            |p| p.ln_policy = LnScorePolicy::AutoLn,
            |p| p.rule_mode = RuleMode::Dx,
        ];
        for mutate in mutations {
            let mut bad = good.clone();
            mutate(&mut bad);
            assert!(bad.request().is_err());
        }
    }
    assert!(play(KeyMode::K9).request().is_err());

    for (gauge, clear) in [
        (GaugeType::Class, ClearType::Normal),
        (GaugeType::ExClass, ClearType::Hard),
        (GaugeType::ExHardClass, ClearType::ExHard),
    ] {
        let mut p = play(KeyMode::K14);
        p.final_gauge = gauge;
        p.final_clear = clear;
        for s in &mut p.stages {
            s.gauge = gauge;
            s.clear = clear;
            s.gauge_carry = vec![(gauge, 80.0)];
        }
        assert_eq!(p.request().unwrap().score.clear, clear);
    }

    let mut switched = play(KeyMode::K7);
    // Earlier results may select a harder surviving gauge. Course carry proves
    // that the final CLASS gauge also survived every complete stage.
    switched.stages[0].gauge = GaugeType::ExHardClass;
    switched.stages[0].clear = ClearType::ExHard;
    switched.stages[1].gauge = GaugeType::ExClass;
    switched.stages[1].clear = ClearType::Hard;
    assert!(switched.request().is_ok());
    switched.stages[0].gauge_carry[0].1 = 0.0;
    assert!(switched.request().is_err());
}

#[test]
fn packed_option_is_never_rounded_and_all_stages_must_agree() {
    for (left, right, flip, expected) in [
        (false, false, false, 0),
        (true, false, false, 1),
        (false, true, false, 10),
        (true, true, false, 11),
        (false, false, true, 100),
        (true, true, true, 111),
    ] {
        let mut p = play(KeyMode::K14);
        p.constraints.class = CourseClassConstraint::GradeMirrorAllowed;
        for s in &mut p.stages {
            s.arrange = if left { ArrangeOption::Mirror } else { ArrangeOption::Normal };
            s.arrange_2p = if right { ArrangeOption::Mirror } else { ArrangeOption::Normal };
            s.double_option = if flip { DoubleOption::Flip } else { DoubleOption::Off };
            assert_eq!(packed_option(s).unwrap(), expected);
        }
        assert_eq!(p.request().is_ok(), expected <= 1);
    }
    let mut p = play(KeyMode::K7);
    p.stages[0].arrange_2p = ArrangeOption::Mirror;
    p.stages[0].double_option = DoubleOption::Flip;
    assert_eq!(
        packed_option(&p.stages[0]).unwrap(),
        0,
        "unused SP settings are not packed by beatoraja"
    );
    p.stages[1].arrange = ArrangeOption::Mirror;
    assert!(p.request().is_err());
    for a in [ArrangeOption::Random, ArrangeOption::FRandom, ArrangeOption::MFRandom] {
        for s in &mut p.stages {
            s.arrange = a;
        }
        assert!(p.request().is_err());
    }
    for s in &mut p.stages {
        s.arrange = ArrangeOption::Mirror;
    }
    p.constraints.class = CourseClassConstraint::Grade;
    assert!(p.request().is_err());
}

#[test]
fn constraint_mapping_preserves_forbidden_and_unknown_constraints() {
    let mut p = play(KeyMode::K7);
    assert!(p.request().is_ok(), "GAUGE_LR2 text is not mandatory");
    for name in ["cn", "HCN", "gauge_7k", "GAUGE_UNKNOWN"] {
        p.constraints.source_constraints = vec![name.into()];
        assert!(p.request().is_err(), "{name}");
    }
    p.constraints = CourseConstraints::from_beatoraja_names([
        "grade_mirror",
        "no_speed",
        "no_good",
        "gauge_lr2",
        "ln",
        "FUTURE_CONSTRAINT",
    ]);
    let request = p.request().unwrap();
    assert_eq!(
        request.course.constraint,
        ["MIRROR", "NO_SPEED", "NO_GOOD", "GAUGE_LR2", "LN", "FUTURE_CONSTRAINT"]
    );
    p.constraints.ln = CourseLnConstraint::Hcn;
    assert!(p.request().is_err());
    p.constraints.ln = CourseLnConstraint::Default;
    p.constraints.gauge = CourseGaugeConstraint::Keys9;
    assert!(p.request().is_err());
}

fn response(description: &str) -> Value {
    json!({"success":true,"description":description,"body":{"set":"genocideDan","value":"NORMAL_1"}})
}

#[test]
fn synchronous_response_contract_distinguishes_all_three_successes() {
    assert!(matches!(
        parse_response(response("Successfully updated class.")).unwrap(),
        DanOutcome::Updated { .. }
    ));
    assert!(matches!(
        parse_response(response("Class not updated.")).unwrap(),
        DanOutcome::Unchanged { .. }
    ));
    assert_eq!(parse_response(json!({"success":true,"description":"Class not updated, as you didn't clear this course."})).unwrap(), DanOutcome::NotCleared);
    for value in [
        json!({"success":true}),
        response("Unknown success"),
        json!({"success":false,"description":"Successfully updated class."}),
        json!({"success":true,"description":"Successfully updated class.","body":{"set":"genocideDan","value":1}}),
        json!({"success":true,"description":"Class not updated.","body":{}}),
    ] {
        assert!(parse_response(value).is_err());
    }
}

fn reply(status: u16, body: Value) -> Reply {
    Reply { path: "POST /ir/beatoraja/submit-course", status, body, auth: true, retry: None }
}

fn enqueue(db: &mut NetworkDatabase, provider: &IrProviderConfig) -> (i64, String) {
    let payload = DanJob::new(provider, 77, &Ok(play(KeyMode::K14))).unwrap().unwrap();
    let json = serde_json::to_string(&payload).unwrap();
    let id = db
        .enqueue_ir_score_job(&NewIrScoreJob {
            provider: provider.provider_key.clone(),
            account_id: provider.account_id.clone(),
            kind: IrJobKind::Course,
            local_score_id: 77,
            chart_sha256: [1; 32],
            ln_policy: LnScorePolicy::ForceLn,
            payload_json: json.clone(),
            now: 100,
        })
        .unwrap();
    (id, json)
}

async fn run(
    db: &mut NetworkDatabase,
    root: &std::path::Path,
    config: &crate::config::profile_config::IrConfig,
    now: i64,
    manual: bool,
) -> crate::ir::sync::IrSyncReport {
    use crate::ir::sync::*;
    sync_pending_ir_jobs_filtered(
        db,
        &root.join("score.db"),
        root,
        &root.join("logs"),
        config,
        IrSyncJobFilter {
            provider_key: "bokutachi",
            account_id: "42",
            kind: IrJobKind::Course,
            local_score_id: Some(77),
        },
        now,
        10,
        IrJobRetryFilter::from_ignore_backoff(manual),
        IrSyncThrottle::none(),
    )
    .await
    .unwrap()
}

fn saved(db: &NetworkDatabase, id: i64) -> (String, i64, String, String) {
    db.conn()
        .query_row(
            "SELECT status,next_attempt_at,payload_json,last_error FROM ir_score_jobs WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
}

#[tokio::test]
async fn synchronous_http_results_do_not_create_scores_or_touch_stage_jobs() {
    for (status, body, expected, retryable) in [
        (200, response("Successfully updated class."), "succeeded", false),
        (200, response("Class not updated."), "no_change", false),
        (
            200,
            json!({"success":true,"description":"Class not updated, as you didn't clear this course."}),
            "not_cleared",
            false,
        ),
        (404, json!({"success":false,"description":"Unsupported course."}), "ineligible", false),
        (400, json!({"success":false,"description":"Bad condition"}), "failed", false),
        (401, json!({"success":false,"description":"Unauthorized"}), "failed", false),
        (403, json!({"success":false,"description":"Forbidden"}), "failed", false),
        (429, json!({"success":false,"description":"Slow down"}), "failed", true),
        (503, json!({"success":false,"description":"Unavailable"}), "failed", true),
        (200, json!({"success":true}), "failed", false),
        (202, response("Successfully updated class."), "failed", false),
    ] {
        let (origin, server) = server(vec![reply(status, body)]).await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, mut config, _) = setup(root.path(), &origin, Game::Bms7k);
        config.providers[0].send_policy =
            crate::config::profile_config::IrSendPolicyConfig::UpdateScore;
        let (id, original) = enqueue(&mut db, &config.providers[0]);
        let report = run(&mut db, root.path(), &config, 100, false).await;
        assert_eq!(report.submitted, 0, "a class update is not a submitted score");
        let (state, next, payload, _) = saved(&db, id);
        assert_eq!(state, expected, "HTTP {status}");
        assert_eq!(payload, original);
        if retryable {
            assert!((160..1000).contains(&next));
        } else {
            assert_eq!(run(&mut db, root.path(), &config, 10000, false).await.processed, 0);
        }
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM ir_score_submissions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            db.conn()
                .query_row("SELECT COUNT(*) FROM ir_score_success_ledger", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.conn()
                .query_row(
                    "SELECT COUNT(*) FROM ir_score_jobs WHERE kind IN ('replay','attestation')",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(saved(&db, 1).0, "pending", "stage job is independent");
        assert!(db.bokutachi_progress(id).unwrap().is_none());
        let bodies = server.await.unwrap();
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0]["course"]["charts"][1]["md5"], format!("{:032x}", 2));
        assert_eq!(bodies[0]["score"].as_object().unwrap().len(), 3);
    }
}

#[tokio::test]
async fn restart_and_retry_keep_the_exact_dan_request() {
    let mut limited = reply(429, json!({"success":false}));
    limited.retry = Some("75");
    let (origin, server) = server(vec![
        limited,
        reply(503, json!({"success":false})),
        reply(200, response("Successfully updated class.")),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let (mut db, mut config, _) = setup(root.path(), &origin, Game::Bms7k);
    let (id, original) = enqueue(&mut db, &config.providers[0]);
    run(&mut db, root.path(), &config, 100, false).await;
    assert_eq!(saved(&db, id).1, 175);
    run(&mut db, root.path(), &config, 101, true).await;
    assert_eq!(saved(&db, id).0, "pending");
    drop(db);
    let mut db = NetworkDatabase::open(&root.path().join("network.db")).unwrap();
    config.providers[0].send_policy =
        crate::config::profile_config::IrSendPolicyConfig::CompleteSong;
    run(&mut db, root.path(), &config, 175, false).await;
    run(&mut db, root.path(), &config, 1000, true).await;
    assert_eq!(saved(&db, id).0, "succeeded");
    assert_eq!(saved(&db, id).2, original);
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests.windows(2).all(|w| w[0] == w[1]));
}

#[tokio::test]
async fn legacy_and_account_mismatched_dan_jobs_never_use_fallback_endpoints() {
    let root = tempfile::tempdir().unwrap();
    let (mut db, config, _) = setup(root.path(), "http://127.0.0.1:1", Game::Bms7k);
    let (id, original) = enqueue(&mut db, &config.providers[0]);
    let mut changed: Value = serde_json::from_str(&original).unwrap();
    changed["account_id"] = json!("other");
    for payload in [changed.to_string(), json!({"course":{},"score":{}}).to_string()] {
        db.conn()
            .execute(
                "UPDATE ir_score_jobs SET payload_json=?1,status='pending' WHERE id=?2",
                rusqlite::params![payload, id],
            )
            .unwrap();
        run(&mut db, root.path(), &config, 200, true).await;
        let row = saved(&db, id);
        assert_eq!(row.0, "failed");
        assert!(!row.3.contains("error sending request"));
        assert!(row.3.contains("saved course") || row.3.contains("account/origin"));
    }
}
