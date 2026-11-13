use super::client::{Chart, PollResult, parse_poll, select_pms_chart, validate_chart};
use super::convert::{ChartMatch, context, convert};
use super::*;
use crate::config::profile_config::{IrConfig, IrProviderConfig, KeyModeConversionConfig};
use crate::ir::types::IrScoreSubmission;
use crate::ln_policy::LnScorePolicy;
use crate::storage::network_db::{IrJobKind, NetworkDatabase, NewIrScoreJob};
use bmz_core::lane::KeyMode;
use serde_json::{Value, json};

fn payload(game: Game) -> IrScoreSubmission {
    let mode = match game {
        Game::Bms7k => KeyMode::K7,
        Game::Bms14k => KeyMode::K14,
        _ => KeyMode::K9,
    };
    let ctx = PlayContext {
        version: 1,
        origin: DEFAULT_BASE_URL.into(),
        new_normal_single: true,
        course_stage: None,
        normal_score_and_effective_assist_passed: true,
        source_key_mode: mode,
        primary_key_mode: mode,
        conversion: KeyModeConversionConfig::Off,
        scoring_scope: ScoringScope::Native,
        game,
    };
    serde_json::from_value(json!({
        "client": {"name":"BMZ", "version":"0.5.1", "platform":"test"},
        "chart": {"kaleid":bmz_chart::kaleid::KaleidProvenance::imported(), "sha256":"ab".repeat(32), "ln_profile":{"has_undefined_ln":true,"has_defined_ln":true,"has_defined_cn":true,"has_defined_hcn":true},
            "mode":mode.as_str(), "notes":{"total":999,"ln":0,"cn":0,"hcn":0,"mine":0}, "features":{"random":false,"stop":false,"ln":true,"cn":true,"hcn":true,"mine":false}},
        "rule": {"play_mode":"single", "key_mode":mode.as_str(), "gauge":"Hard", "ln_policy":"ForceLn", "effective_ln_mode":"ln", "judge_algorithm":"bmz_v1", "scoring":"bms_ex_score_v1", "rule_mode":if game.is_pms() {"Beatoraja"} else {"Lr2Oraja"}},
        "result": {"clear":"Hard", "played_at":1794182400i64, "judges":{
            "fast":{"pgreat":1,"great":2,"good":3,"bad":4,"poor":5,"empty_poor":6},
            "slow":{"pgreat":10,"great":20,"good":30,"bad":40,"poor":50,"empty_poor":60}},
            "ex_score":44,"max_combo":10,"notes":100,"min_bp":123,"min_cb":12},
        "play_options":{"bokutachi":ctx,"device_type":if game==Game::PmsController {"controller"} else {"keyboard"},
            "source_kind":"local","applied_double_option":"off","arrange_1p":"f-random","arrange_2p":"mirror","seed":"123"},
        "idempotency_key":"test-play-1"
    })).unwrap()
}

fn matcher(game: Game, p: &IrScoreSubmission) -> ChartMatch {
    ChartMatch {
        match_type: if game.is_pms() { "tachiSongID" } else { "bmsChartHash" }.into(),
        identifier: if game.is_pms() { "123".into() } else { p.chart.sha256.clone() },
        difficulty: game.is_pms().then(|| "CHART".into()),
    }
}

#[test]
fn kaleid_requires_effective_ln_and_preserves_original_feature_exclusions() {
    use crate::ir::types::IrEffectiveLnMode;
    use bmz_chart::kaleid::{KaleidFeatures, KaleidProvenance};
    for game in [Game::Bms7k, Game::Bms14k, Game::PmsController, Game::PmsKeyboard] {
        let mut p = payload(game);
        p.chart.kaleid.source =
            KaleidFeatures { hln: true, typed_end: true, bar_line: true, ..Default::default() };
        p.chart.kaleid.declared.hln = true;
        p.chart.ln_profile.has_defined_hln = true;
        let original = p.chart.sha256.clone();
        assert!(context(&p).is_ok()); // HLN converted to FORCE LN.
        assert!(crate::ir::bms_ir::ensure_score_payload_supported(&p).is_err());
        assert!(convert(&p, matcher(game, &p)).is_ok());
        assert_eq!(p.chart.sha256, original);
        for flags in [
            KaleidFeatures { conditional: true, ..Default::default() },
            KaleidFeatures { free_zone: true, ..Default::default() },
            KaleidFeatures { potion: true, ..Default::default() },
            KaleidFeatures { dummy: true, ..Default::default() },
        ] {
            for source in [true, false] {
                let mut bad = p.clone();
                if source {
                    bad.chart.kaleid.source = bad.chart.kaleid.source.union(flags);
                } else {
                    bad.chart.kaleid.effective = flags;
                }
                assert!(context(&bad).is_err());
            }
        }
        for mode in [IrEffectiveLnMode::Cn, IrEffectiveLnMode::Hcn, IrEffectiveLnMode::Hln] {
            let mut bad = p.clone();
            bad.rule.effective_ln_mode = mode;
            assert!(context(&bad).is_err());
        }
        p.chart.kaleid.effective.hln = true;
        assert!(context(&p).is_err());
        p.chart.kaleid.effective.hln = false;
        p.rule.ln_policy = LnScorePolicy::ForceHln;
        assert!(context(&p).is_err());
        p.rule.ln_policy = LnScorePolicy::ForceLn;
        for provenance in [
            KaleidProvenance::default(),
            KaleidProvenance { version: 2, ..KaleidProvenance::imported() },
            KaleidProvenance {
                diagnostics: vec!["invalid extension".into()],
                ..KaleidProvenance::imported()
            },
        ] {
            p.chart.kaleid = provenance;
            assert!(context(&p).is_err());
        }
    }
}

#[tokio::test]
async fn rejected_kaleid_queue_stops_before_network_and_explicit_retry_keeps_the_reason() {
    for provenance in [
        bmz_chart::kaleid::KaleidProvenance::default(),
        bmz_chart::kaleid::KaleidProvenance {
            source: bmz_chart::kaleid::KaleidFeatures { conditional: true, ..Default::default() },
            ..bmz_chart::kaleid::KaleidProvenance::imported()
        },
    ] {
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, json) = setup(root.path(), "http://127.0.0.1:1", Game::Bms7k);
        let mut p: IrScoreSubmission = serde_json::from_str(&json).unwrap();
        p.chart.kaleid = provenance;
        db.conn()
            .execute(
                "UPDATE ir_score_jobs SET payload_json = ?1",
                [serde_json::to_string(&p).unwrap()],
            )
            .unwrap();
        assert_eq!(run(&mut db, root.path(), &config, 100, false).await.failed, 1);
        assert!(job(&db).last_error.contains("Bokutachi:"));
        assert!(!job(&db).last_error.contains("connect"));
        assert_eq!(run(&mut db, root.path(), &config, 1000, false).await.processed, 0);
        assert_eq!(resend_selected(&mut db, root.path(), &config, 1001).await.failed, 1);
    }
}

fn change_context(p: &mut IrScoreSubmission, f: impl FnOnce(&mut PlayContext)) {
    let mut c: PlayContext = serde_json::from_value(p.play_options["bokutachi"].clone()).unwrap();
    f(&mut c);
    p.play_options.insert("bokutachi".into(), serde_json::to_value(c).unwrap());
}

#[test]
fn course_stage_lamps_require_saved_provenance_and_preserve_single_metrics() {
    use bmz_core::clear::ClearType;
    for game in [Game::Bms7k, Game::Bms14k, Game::PmsController, Game::PmsKeyboard] {
        for source_clear in [
            ClearType::Failed,
            ClearType::Normal,
            ClearType::Hard,
            ClearType::FullCombo,
            ClearType::Perfect,
            ClearType::Max,
        ] {
            let mut p = payload(game);
            p.result.clear =
                crate::storage::play_result::course_stage_clear_type(source_clear).as_str().into();
            p.play_options.insert("course_stage".into(), json!(true));
            change_context(&mut p, |c| {
                c.version = 2;
                c.new_normal_single = false;
                c.course_stage = Some(CourseStageContext {
                    source_clear,
                    completed: source_clear != ClearType::Failed,
                });
            });
            let wire = serde_json::to_value(convert(&p, matcher(game, &p)).unwrap()).unwrap();
            let score = &wire["scores"][0];
            assert_eq!(
                score["lamp"],
                if source_clear as u8 >= ClearType::FullCombo as u8 {
                    "FULL COMBO"
                } else {
                    "NO PLAY"
                }
            );
            assert_eq!(score["score"], 44);
            assert_eq!(score["optional"]["bp"], 123);
            assert_eq!(score["optional"]["maxCombo"], 10);
            assert_eq!(wire["meta"]["game"], game.as_str());
            assert_eq!(score["judgements"][if game.is_pms() { "cool" } else { "pgreat" }], 11);
            assert_eq!(context(&p).unwrap().game, game);
            let frozen = serde_json::to_string(&p).unwrap();
            let again: IrScoreSubmission = serde_json::from_str(&frozen).unwrap();
            assert_eq!(
                wire,
                serde_json::to_value(convert(&again, matcher(game, &again)).unwrap()).unwrap()
            );
            change_context(&mut p, |c| {
                c.course_stage.as_mut().unwrap().source_clear = ClearType::NoPlay
            });
            assert!(convert(&p, matcher(game, &p)).is_err());
        }
        let mut normal = payload(game);
        normal.result.clear = "NoPlay".into();
        assert!(convert(&normal, matcher(game, &normal)).is_err());
    }
}

#[test]
fn eligibility_uses_saved_normalized_ln_and_provenance() {
    for game in [Game::Bms7k, Game::Bms14k, Game::PmsController, Game::PmsKeyboard] {
        let p = payload(game);
        assert_eq!(context(&p).unwrap().game, game);
        for ln in [
            LnScorePolicy::AutoLn,
            LnScorePolicy::AutoCn,
            LnScorePolicy::AutoHcn,
            LnScorePolicy::ForceCn,
            LnScorePolicy::ForceHcn,
        ] {
            let mut bad = p.clone();
            bad.rule.ln_policy = ln;
            assert!(context(&bad).is_err());
        }
        let mut bad = p.clone();
        bad.chart.features.random = true;
        assert!(context(&bad).is_err());
        let mut bad = p.clone();
        bad.play_options.remove("bokutachi");
        assert!(context(&bad).is_err());
        let mut bad = p.clone();
        bad.play_options.insert("course_stage".into(), json!(true));
        assert!(context(&bad).is_err());
        let mut bad = p.clone();
        bad.play_options.insert("source_kind".into(), json!("local_backfill"));
        assert!(context(&bad).is_err());
        for option in ["battle", "battle-as", "corrupt"] {
            let mut bad = p.clone();
            bad.play_options.insert("applied_double_option".into(), json!(option));
            assert!(context(&bad).is_err());
        }
        let mut flip = p.clone();
        flip.play_options.insert("applied_double_option".into(), json!("flip"));
        assert!(context(&flip).is_ok());
        let mut converted = p.clone();
        change_context(&mut converted, |c| c.conversion = KeyModeConversionConfig::SevenToNine);
        assert!(context(&converted).is_err());
        let mut assisted = p.clone();
        change_context(&mut assisted, |c| c.normal_score_and_effective_assist_passed = false);
        assert!(context(&assisted).is_err());
        let mut course = p.clone();
        change_context(&mut course, |c| c.new_normal_single = false);
        assert!(context(&course).is_err());
        // Setting bits alone do not reject ADD LN or ineffective assists.
        let mut settings = p.clone();
        settings.play_options.insert("assist_mask".into(), json!(65535));
        assert!(context(&settings).is_ok());
    }
    for rule in ["Lr2Oraja", "Dx", ""] {
        let mut p = payload(Game::PmsKeyboard);
        p.rule.rule_mode = rule.into();
        assert!(context(&p).is_err());
    }
    for rule in ["Beatoraja", "Dx"] {
        let mut p = payload(Game::Bms7k);
        p.rule.rule_mode = rule.into();
        assert!(context(&p).is_err());
    }
    let mut p = payload(Game::PmsKeyboard);
    p.play_options.insert("device_type".into(), json!("controller"));
    assert!(context(&p).is_err());
    p.play_options.remove("device_type");
    assert!(context(&p).is_err());
    p.play_options.insert("device_type".into(), json!("unknown"));
    assert!(context(&p).is_err());
}

#[test]
fn direct_manual_wire_contract_preserves_this_play_and_device() {
    for game in [Game::Bms7k, Game::Bms14k, Game::PmsKeyboard, Game::PmsController] {
        let p = payload(game);
        let wire = serde_json::to_value(convert(&p, matcher(game, &p)).unwrap()).unwrap();
        assert_eq!(wire["meta"]["game"], game.as_str());
        assert_eq!(wire["meta"]["service"], "BMZ Player");
        assert!(wire["meta"].get("playtype").is_none());
        let s = &wire["scores"][0];
        assert_eq!(s["score"], 44);
        assert_eq!(s["timeAchieved"], 1794182400000i64);
        assert_eq!(s["optional"], json!({"bp":123,"maxCombo":10}));
        assert_eq!(s["judgements"]["poor"], 121);
        assert_eq!(s["judgements"][if game.is_pms() { "cool" } else { "pgreat" }], 11);
        assert!(s["judgements"].get(if game.is_pms() { "pgreat" } else { "cool" }).is_none());
        assert_eq!(
            s["scoreMeta"]["random"],
            if game == Game::Bms14k { json!(["RANDOM", "MIRROR"]) } else { json!("RANDOM") }
        );
        assert_eq!(s["scoreMeta"].as_object().unwrap().len(), 1);
        assert!(s.get("seed").is_none());
        assert!(s.get("ln_policy").is_none());
        if game.is_pms() {
            assert_eq!(s["matchType"], "tachiSongID");
            assert_eq!(s["identifier"], "123");
            assert_eq!(s["difficulty"], "CHART");
        }
    }
    let mut p = payload(Game::Bms7k);
    change_context(&mut p, |c| c.scoring_scope = ScoringScope::DisplayBattle7k);
    p.chart.mode = "14K".into();
    p.rule.key_mode = "14K".into();
    p.play_options.insert("arrange_2p".into(), json!("corrupt-opponent"));
    assert_eq!(
        serde_json::to_value(convert(&p, matcher(Game::Bms7k, &p)).unwrap()).unwrap()["meta"]["game"],
        "bms-7k"
    );
    p.result.played_at = i64::MAX;
    assert!(convert(&p, matcher(Game::Bms7k, &p)).is_err());
}

#[test]
fn lamps_and_arrangements_do_not_invent_defaults() {
    let mut p = payload(Game::Bms14k);
    for (clear, lamp) in [
        ("Failed", "FAILED"),
        ("AssistEasy", "ASSIST CLEAR"),
        ("LightAssistEasy", "ASSIST CLEAR"),
        ("Easy", "EASY CLEAR"),
        ("Normal", "CLEAR"),
        ("Hard", "HARD CLEAR"),
        ("ExHard", "EX HARD CLEAR"),
        ("FullCombo", "FULL COMBO"),
        ("Perfect", "FULL COMBO"),
        ("Max", "FULL COMBO"),
    ] {
        p.result.clear = clear.into();
        assert_eq!(
            serde_json::to_value(convert(&p, matcher(Game::Bms14k, &p)).unwrap()).unwrap()["scores"]
                [0]["lamp"],
            lamp
        );
    }
    for clear in ["NoPlay", "broken"] {
        p.result.clear = clear.into();
        assert!(convert(&p, matcher(Game::Bms14k, &p)).is_err());
    }
    p.result.clear = "Failed".into();
    for (arrange, expected) in [
        ("normal", "NONRAN"),
        ("mirror", "MIRROR"),
        ("random", "RANDOM"),
        ("r-random", "R-RANDOM"),
        ("s-random", "S-RANDOM"),
        ("f-random", "RANDOM"),
        ("mf-random", "RANDOM"),
    ] {
        p.play_options.insert("arrange_1p".into(), json!(arrange));
        assert_eq!(
            serde_json::to_value(convert(&p, matcher(Game::Bms14k, &p)).unwrap()).unwrap()["scores"]
                [0]["scoreMeta"]["random"][0],
            expected
        );
    }
    p.play_options.insert("arrange_2p".into(), json!("spiral"));
    assert_eq!(
        serde_json::to_value(convert(&p, matcher(Game::Bms14k, &p)).unwrap()).unwrap()["scores"][0]
            ["scoreMeta"],
        json!({})
    );
    p.play_options.insert("arrange_2p".into(), json!("broken"));
    assert!(convert(&p, matcher(Game::Bms14k, &p)).is_err());
}

fn chart(game: Game) -> Value {
    json!({"game":game.as_str(),"chartID":"chart-1","song":{"id":123},"difficulty":"CHART","isPrimary":true,"data":{"hashSHA256":"ab".repeat(32),"notecount":100}})
}

#[test]
fn pms_candidates_and_resolved_chart_must_agree() {
    let c: Chart = serde_json::from_value(chart(Game::PmsController)).unwrap();
    let k: Chart = serde_json::from_value(chart(Game::PmsKeyboard)).unwrap();
    assert_eq!(
        select_pms_chart(vec![k, c.clone()], Game::PmsController, &c.data.hash).unwrap().game,
        "pms-controller"
    );
    assert!(select_pms_chart(vec![], Game::PmsController, &c.data.hash).is_err());
    let mut other = c.clone();
    other.is_primary = false;
    assert!(select_pms_chart(vec![other], Game::PmsController, &c.data.hash).is_err());
    let mut other = c.clone();
    other.chart_id = "other".into();
    assert!(select_pms_chart(vec![c.clone(), other], Game::PmsController, &c.data.hash).is_err());
    assert!(validate_chart(&c, Game::PmsController, &c.data.hash, 100, Some(&c)).is_ok());
    assert!(validate_chart(&c, Game::PmsController, &c.data.hash, 999, Some(&c)).is_err());
    assert!(validate_chart(&c, Game::PmsKeyboard, &c.data.hash, 100, Some(&c)).is_err());
    assert!(validate_chart(&c, Game::PmsController, &"cd".repeat(32), 100, Some(&c)).is_err());
    let mut other = c.clone();
    other.chart_id = "other".into();
    assert!(validate_chart(&other, Game::PmsController, &c.data.hash, 100, Some(&c)).is_err());
    for notes in [json!(-1), json!(1.5), json!("100"), Value::Null] {
        let mut invalid = chart(Game::Bms7k);
        invalid["data"]["notecount"] = notes;
        assert!(serde_json::from_value::<Chart>(invalid).is_err());
    }
}

#[test]
fn completed_imports_have_four_outcomes_and_strict_identity() {
    for scores in [vec![], vec!["score-1"]] {
        for errors in [json!([]), json!([{"type":"warning","message":"detail"}])] {
            let value = json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":scores,"errors":errors}});
            let PollResult::Completed { score_id, errors: actual } =
                parse_poll(value.clone(), "42").unwrap()
            else {
                panic!()
            };
            assert_eq!(score_id.is_some(), !scores.is_empty());
            assert_eq!(actual.len(), errors.as_array().unwrap().len());
            assert!(parse_poll(value, "43").is_err());
        }
    }
    for value in [
        json!({}),
        json!({"importStatus":"completed"}),
        json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":["a","b"],"errors":[]}}),
        json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":[]}}),
    ] {
        assert!(parse_poll(value, "42").is_err());
    }
}

pub(super) struct Reply {
    pub(super) path: &'static str,
    pub(super) status: u16,
    pub(super) body: Value,
    pub(super) auth: bool,
    pub(super) retry: Option<&'static str>,
}

pub(super) async fn server(replies: Vec<Reply>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let mut bodies = Vec::new();
        for reply in replies {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .map(|v| v.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        assert!(
                            String::from_utf8_lossy(&bytes).starts_with(reply.path),
                            "{}",
                            String::from_utf8_lossy(&bytes)
                        );
                        assert_eq!(headers.contains("authorization: bearer test-key"), reply.auth);
                        assert!(!headers.contains("x-user-intent"));
                        assert!(!headers.contains("idempotency-key"));
                        if reply.path == "POST /ir/beatoraja/submit-course" {
                            assert!(headers.contains("x-tachiir-version: v4"));
                        }
                        bodies.push(if length > 0 {
                            serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
                        } else {
                            Value::Null
                        });
                        break;
                    }
                }
            }
            let body = reply.body.to_string();
            let retry = reply.retry.map(|r| format!("Retry-After: {r}\r\n")).unwrap_or_default();
            stream.write_all(format!("HTTP/1.1 {} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\n{retry}Connection: close\r\n\r\n{body}",reply.status,body.len()).as_bytes()).await.unwrap();
        }
        bodies
    });
    (url, handle)
}

fn ok(path: &'static str, body: Value) -> Reply {
    Reply { path, status: 200, body: json!({"success":true,"body":body}), auth: false, retry: None }
}

async fn run(
    db: &mut NetworkDatabase,
    root: &Path,
    config: &IrConfig,
    now: i64,
    manual: bool,
) -> crate::ir::sync::IrSyncReport {
    crate::ir::sync::sync_pending_ir_jobs(
        db,
        &root.join("score.db"),
        root,
        &root.join("logs"),
        config,
        now,
        20,
        manual,
        crate::ir::sync::IrSyncThrottle::none(),
    )
    .await
    .unwrap()
}
use std::path::Path;

async fn resend_selected(
    db: &mut NetworkDatabase,
    root: &Path,
    config: &IrConfig,
    now: i64,
) -> crate::ir::sync::IrSyncReport {
    crate::ir::sync::sync_pending_ir_jobs_filtered(
        db,
        &root.join("score.db"),
        root,
        &root.join("logs"),
        config,
        crate::ir::sync::IrSyncJobFilter {
            provider_key: PROVIDER,
            account_id: "42",
            kind: IrJobKind::Score,
            local_score_id: Some(1),
        },
        now,
        1,
        crate::storage::network_db::IrJobRetryFilter::ExplicitResend,
        crate::ir::sync::IrSyncThrottle::none(),
    )
    .await
    .unwrap()
}

pub(super) fn setup(root: &Path, origin: &str, game: Game) -> (NetworkDatabase, IrConfig, String) {
    let path = root.join("network.db");
    crate::storage::migration::migrate_network_db(&path).unwrap();
    let mut db = NetworkDatabase::open(&path).unwrap();
    let mut provider = IrProviderConfig::bokutachi();
    provider.enabled = true;
    provider.provider_key = PROVIDER.into();
    provider.account_id = "42".into();
    provider.base_url = origin.into();
    crate::ir::credentials::save_credentials(
        root,
        &crate::ir::credentials::IrStoredCredentials {
            provider: PROVIDER.into(),
            origin: origin.into(),
            account_id: "42".into(),
            display_name: "42".into(),
            access_token: "test-key".into(),
            refresh_token: String::new(),
            expires_at: Some(0),
        },
    )
    .unwrap();
    let mut p = payload(game);
    change_context(&mut p, |c| c.origin = origin.into());
    let json = serde_json::to_string(&p).unwrap();
    db.enqueue_ir_score_job(&NewIrScoreJob {
        provider: PROVIDER.into(),
        account_id: "42".into(),
        kind: IrJobKind::Score,
        local_score_id: 1,
        chart_sha256: [0xab; 32],
        ln_policy: LnScorePolicy::ForceLn,
        payload_json: json.clone(),
        now: 100,
    })
    .unwrap();
    (db, IrConfig { providers: vec![provider], ..IrConfig::default() }, json)
}

#[tokio::test]
async fn accepted_import_survives_reopen_and_manual_retry_without_posting_again() {
    let mut accepted = ok(
        "POST /ir/direct-manual/import",
        json!({"importID":"import-1","url":"https://attacker.invalid/poll"}),
    );
    accepted.status = 202;
    accepted.auth = true;
    let (origin,server)=server(vec![ok("POST /api/v1/games/bms-7k/charts/resolve",json!({"chart":chart(Game::Bms7k)})),accepted,
        ok("GET /api/v1/imports/import-1/poll-status",json!({"importStatus":"ongoing"})),
        Reply {path:"GET /api/v1/imports/import-1/poll-status",status:503,body:json!({}),auth:false,retry:None},
        ok("GET /api/v1/imports/import-1/poll-status",json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":["score-real"],"errors":[{"type":"warning","message":"kept warning"}]}}))]).await;
    let root = tempfile::tempdir().unwrap();
    let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
    let report = run(&mut db, root.path(), &config, 100, false).await;
    assert_eq!((report.submitted, report.failed), (0, 0));
    assert_eq!(
        db.conn()
            .query_row("SELECT COUNT(*) FROM ir_score_success_ledger", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.ir_score_jobs_for_local_score(IrJobKind::Score, 1).unwrap()[0].payload_json,
        original
    );
    drop(db);
    let mut db = NetworkDatabase::open(&root.path().join("network.db")).unwrap();
    assert_eq!(run(&mut db, root.path(), &config, 105, false).await.submitted, 0);
    assert_eq!(run(&mut db, root.path(), &config, 115, false).await.failed, 1);
    assert_eq!(
        db.ir_score_jobs_for_local_score(IrJobKind::Score, 1).unwrap()[0].payload_json,
        original
    );
    assert_eq!(run(&mut db, root.path(), &config, 116, true).await.submitted, 1);
    let saved = db.ir_score_jobs_for_local_score(IrJobKind::Score, 1).unwrap();
    assert_eq!(saved[0].status, "succeeded");
    assert!(saved[0].last_error.contains("kept warning"));
    assert_eq!(
        db.conn()
            .query_row("SELECT remote_score_id FROM ir_score_success_ledger", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "score-real"
    );
    assert_eq!(
        db.conn()
            .query_row("SELECT COUNT(*) FROM ir_score_jobs", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    let requests = server.await.unwrap();
    assert_eq!(requests[1]["scores"][0]["timeAchieved"], 1794182400000i64);
}

#[tokio::test]
async fn api_key_requires_identity_and_current_permission() {
    for body in [
        json!({"whoami":null,"permissions":["submit_score"]}),
        json!({"whoami":42,"permissions":["score_submit"]}),
        json!({"whoami":0,"permissions":["submit_score"]}),
        json!({"whoami":42,"permissions":["submit_score"]}),
    ] {
        let valid = body["whoami"] == 42 && body["permissions"][0] == "submit_score";
        let mut reply = ok("GET /api/v1/status", body);
        reply.auth = true;
        let (origin, server) = server(vec![reply]).await;
        assert_eq!(
            BokutachiClient::new(&origin).unwrap().validate_key("test-key").await.is_ok(),
            valid
        );
        server.await.unwrap();
    }
    let mut reply = ok("GET /api/v1/status", json!({"whoami":42,"permissions":["submit_score"]}));
    reply.body["success"] = json!(false);
    reply.auth = true;
    let (origin, server) = server(vec![reply]).await;
    assert!(BokutachiClient::new(&origin).unwrap().validate_key("test-key").await.is_err());
    server.await.unwrap();
}

fn accepted() -> Reply {
    let mut reply = ok("POST /ir/direct-manual/import", json!({"importID":"import-1"}));
    reply.auth = true;
    reply.status = 202;
    reply
}

#[tokio::test]
async fn converted_hln_matches_registered_counts_and_old_accepted_imports_only_poll() {
    for matching in [true, false] {
        let mut registered = chart(Game::Bms7k);
        if !matching {
            registered["data"]["notecount"] = json!(101);
        }
        let mut replies =
            vec![ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":registered}))];
        if matching {
            replies.push(accepted());
            replies.push(ok("GET /api/v1/imports/import-1/poll-status", json!({
                "importStatus":"completed", "import":{"userID":42,"scoreIDs":["hln-ln-score"],"errors":[]}
            })));
        }
        let (origin, server) = server(replies).await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, json) = setup(root.path(), &origin, Game::Bms7k);
        let mut p: IrScoreSubmission = serde_json::from_str(&json).unwrap();
        p.chart.kaleid.source.hln = true;
        p.chart.ln_profile.has_defined_hln = true;
        db.conn()
            .execute(
                "UPDATE ir_score_jobs SET payload_json = ?1",
                [serde_json::to_string(&p).unwrap()],
            )
            .unwrap();
        run(&mut db, root.path(), &config, 100, false).await;
        if matching {
            // Model an import already accepted by an older client. Checking its
            // status must remain possible without enabling a new POST.
            p.chart.kaleid = Default::default();
            db.conn()
                .execute(
                    "UPDATE ir_score_jobs SET payload_json = ?1",
                    [serde_json::to_string(&p).unwrap()],
                )
                .unwrap();
            assert_eq!(run(&mut db, root.path(), &config, 110, false).await.submitted, 1);
        } else {
            assert_eq!(job(&db).status, "ineligible");
            assert_eq!(run(&mut db, root.path(), &config, 110, false).await.processed, 0);
        }
        let requests = server.await.unwrap();
        assert_eq!(requests[0]["identifier"], p.chart.sha256);
        assert_eq!(requests.len(), if matching { 3 } else { 1 });
    }
}

fn error(path: &'static str, status: u16, auth: bool, retry: Option<&'static str>) -> Reply {
    Reply { path, status, body: json!({"success":false,"description":"test error"}), auth, retry }
}

fn job(db: &NetworkDatabase) -> crate::storage::network_db::IrScoreJobRecord {
    db.ir_score_jobs_for_local_score(IrJobKind::Score, 1).unwrap().remove(0)
}

#[tokio::test]
async fn pms_worker_posts_verified_song_id_and_saved_device_only() {
    for game in [Game::PmsController, Game::PmsKeyboard] {
        let resolve = if game == Game::PmsController {
            "POST /api/v1/games/pms-controller/charts/resolve"
        } else {
            "POST /api/v1/games/pms-keyboard/charts/resolve"
        };
        let (origin, server) = server(vec![
            ok(
                "GET /api/v1/search/chart-hash?search=",
                json!({"charts":[chart(Game::PmsKeyboard),chart(Game::PmsController)]}),
            ),
            ok(resolve, json!({"chart":chart(game)})),
            accepted(),
        ])
        .await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, original) = setup(root.path(), &origin, game);
        let result = run(&mut db, root.path(), &config, 100, false).await;
        assert_eq!((result.submitted, result.failed), (0, 0), "{result:?}");
        assert_eq!(job(&db).payload_json, original);
        let requests = server.await.unwrap();
        assert_eq!(
            requests[1],
            json!({"matchType":"tachiSongID","identifier":"123","difficulty":"CHART"})
        );
        assert_eq!(requests[2]["meta"]["game"], game.as_str());
        assert_eq!(requests[2]["scores"][0]["matchType"], "tachiSongID");
        assert_eq!(requests[2]["scores"][0]["identifier"], "123");
        assert_eq!(requests[2]["scores"][0]["difficulty"], "CHART");
        assert!(requests[2]["scores"][0]["judgements"].get("pgreat").is_none());
    }
}

#[tokio::test]
async fn completed_worker_outcomes_never_invent_success_or_repost() {
    for (body, status, submitted, failed) in [
        (json!({"userID":42,"scoreIDs":["score-real"],"errors":[]}), "succeeded", 1, 0),
        (json!({"userID":42,"scoreIDs":[],"errors":[]}), "no_change", 0, 0),
        (
            json!({"userID":42,"scoreIDs":[],"errors":[{"type":"InvalidScore","message":"invalid score"}]}),
            "failed",
            0,
            1,
        ),
        (json!({"userID":43,"scoreIDs":["score-real"],"errors":[]}), "failed", 0, 1),
        (json!({"userID":42,"scoreIDs":["a","b"],"errors":[]}), "failed", 0, 1),
        (json!({"userID":42,"scoreIDs":[]}), "failed", 0, 1),
    ] {
        let (origin, server) = server(vec![
            ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
            accepted(),
            ok(
                "GET /api/v1/imports/import-1/poll-status",
                json!({"importStatus":"completed","import":body}),
            ),
        ])
        .await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
        assert_eq!(run(&mut db, root.path(), &config, 100, false).await.failed, 0);
        let result = run(&mut db, root.path(), &config, 105, false).await;
        assert_eq!((result.submitted, result.failed), (submitted, failed), "{result:?}");
        assert_eq!(job(&db).status, status);
        assert_eq!(
            db.conn()
                .query_row("SELECT COUNT(*) FROM ir_score_success_ledger", [], |r| r
                    .get::<_, u32>(0))
                .unwrap(),
            submitted
        );
        if submitted == 0 {
            assert_eq!(job(&db).payload_json, original);
        }
        assert_eq!(run(&mut db, root.path(), &config, 1000, false).await.submitted, 0);
        assert!(db.bokutachi_progress(1).unwrap().unwrap().contains("import-1"));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn chart_failures_stop_before_import_and_manual_retry_re_resolves() {
    for reply in [
        error("POST /api/v1/games/bms-7k/charts/resolve", 404, false, None),
        ok("POST /api/v1/games/bms-7k/charts/resolve", {
            let mut c = chart(Game::Bms7k);
            c["data"]["notecount"] = json!(999);
            json!({"chart":c})
        }),
    ] {
        let (origin, server) = server(vec![
            reply,
            ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
            accepted(),
        ])
        .await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
        assert_eq!(run(&mut db, root.path(), &config, 100, false).await.failed, 0);
        assert_eq!(job(&db).status, "ineligible");
        assert_eq!(job(&db).next_attempt_at, crate::storage::network_db::IR_JOB_NO_AUTO_RETRY_AT);
        assert_eq!(run(&mut db, root.path(), &config, 200, false).await.failed, 0);
        // Bulk manual sync must not repeatedly re-resolve terminal chart failures.
        assert_eq!(run(&mut db, root.path(), &config, 201, true).await.processed, 0);
        assert_eq!(job(&db).status, "ineligible");
        assert_eq!(resend_selected(&mut db, root.path(), &config, 202).await.failed, 0);
        assert_eq!(job(&db).status, "pending");
        assert_eq!(job(&db).payload_json, original);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn bulk_manual_sync_does_not_revisit_an_ineligible_chart() {
    for scoped in [false, true] {
        let (origin, server) =
            server(vec![error("POST /api/v1/games/bms-7k/charts/resolve", 404, false, None)]).await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, _) = setup(root.path(), &origin, Game::Bms7k);
        let score_db = root.path().join("score.db");
        let logs = root.path().join("logs");
        let ctx = crate::ir::manual::IrSyncContext {
            score_db: &score_db,
            profile_root: root.path(),
            logs_dir: &logs,
            ir_config: &config,
        };
        let scope = if scoped {
            crate::ir::manual::IrManualSyncScope::Kind {
                provider_key: PROVIDER,
                account_id: "42",
                kind: IrJobKind::Score,
            }
        } else {
            crate::ir::manual::IrManualSyncScope::Any
        };
        let (_sender, cancel) = tokio::sync::watch::channel(false);
        for _ in 0..2 {
            let outcome =
                crate::ir::manual::sync_jobs(&ctx, &mut db, scope, 20, &mut |_| {}, &cancel)
                    .await
                    .unwrap();
            assert_eq!(outcome.report.failed, 0);
        }
        assert_eq!(job(&db).status, "ineligible");
        assert_eq!(job(&db).attempt_count, 1);
        assert_eq!(server.await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn poll_404_grace_then_unconfirmed_retains_id_and_manual_retry_uses_get() {
    let path = "GET /api/v1/imports/import-1/poll-status";
    let (origin, server) = server(vec![
        ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
        accepted(),
        error(path, 404, false, None),
        error(path, 404, false, None),
        ok(
            path,
            json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":[],"errors":[]}}),
        ),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
    run(&mut db, root.path(), &config, 100, false).await;
    assert_eq!(run(&mut db, root.path(), &config, 105, false).await.failed, 0);
    assert_eq!(job(&db).status, "pending");
    assert_eq!(run(&mut db, root.path(), &config, 221, false).await.failed, 1);
    assert_eq!(job(&db).next_attempt_at, crate::storage::network_db::IR_JOB_NO_AUTO_RETRY_AT);
    assert_eq!(job(&db).payload_json, original);
    assert_eq!(run(&mut db, root.path(), &config, 222, true).await.failed, 0);
    assert_eq!(job(&db).status, "no_change");
    server.await.unwrap();
}

#[tokio::test]
async fn post_errors_classify_auth_content_and_retry_after_without_mutating_request() {
    for (status, retry) in [
        (401, None),
        (403, None),
        (400, None),
        (429, Some("90")),
        (429, None),
        (408, None),
        (503, None),
    ] {
        let (origin, server) = server(vec![
            ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
            error("POST /ir/direct-manual/import", status, true, retry),
            accepted(),
        ])
        .await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
        assert_eq!(run(&mut db, root.path(), &config, 100, false).await.failed, 1);
        let next = job(&db).next_attempt_at;
        if [400, 401, 403].contains(&status) {
            assert_eq!(next, crate::storage::network_db::IR_JOB_NO_AUTO_RETRY_AT);
        } else if status == 429 {
            assert_eq!(next, if retry.is_some() { 190 } else { 161 });
            assert_eq!(db.reserve_bokutachi_post(&origin, 101).unwrap(), Some(next));
        } else {
            assert_eq!(next, 160);
        }
        assert_eq!(job(&db).payload_json, original);
        assert_eq!(run(&mut db, root.path(), &config, 300, true).await.failed, 0);
        let requests = server.await.unwrap();
        assert_eq!(requests[1], requests[2]);
    }
}

#[tokio::test]
async fn confirmed_failed_import_requires_individual_resend_and_preserves_request_after_restart() {
    for (status, description) in [
        (400, "Import tracker rejected the request."),
        (404, "There is no ongoing import here."),
        (500, "An internal service error has occurred with this import."),
    ] {
        let failure = || Reply {
            path: "GET /api/v1/imports/import-1/poll-status",
            status,
            body: json!({"success": false, "description": description}),
            auth: false,
            retry: None,
        };
        let mut replies = vec![
            ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
            accepted(),
            failure(),
        ];
        if status == 404 {
            replies.push(failure());
        }
        let mut accepted_again = accepted();
        accepted_again.body["body"]["importID"] = json!("import-2");
        replies.push(accepted_again);
        replies.push(ok("GET /api/v1/imports/import-2/poll-status",
            json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":["score-2"],"errors":[]}})));
        let (origin, server) = server(replies).await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
        run(&mut db, root.path(), &config, 100, false).await;
        let receipt = root.path().join("ir/bokutachi-receipts/1.json");
        let stale_receipt = std::fs::read(&receipt).unwrap();
        run(&mut db, root.path(), &config, 105, false).await;
        if status == 404 {
            assert_eq!(job(&db).status, "pending");
            run(&mut db, root.path(), &config, 221, false).await;
        }
        assert_eq!(job(&db).status, "failed");
        assert_eq!(job(&db).next_attempt_at, crate::storage::network_db::IR_JOB_NO_AUTO_RETRY_AT);
        assert!(db.bokutachi_progress(1).unwrap().unwrap().contains("poll_failed"));
        // Automatic processing and bulk manual retry never create another import.
        assert_eq!(run(&mut db, root.path(), &config, 240, false).await.processed, 0);
        assert_eq!(run(&mut db, root.path(), &config, 240, true).await.failed, 1);
        let mut credentials =
            crate::ir::credentials::load_credentials(root.path(), PROVIDER, &origin)
                .unwrap()
                .unwrap();
        credentials.account_id = "43".into();
        crate::ir::credentials::save_credentials(root.path(), &credentials).unwrap();
        assert_eq!(resend_selected(&mut db, root.path(), &config, 250).await.failed, 1);
        let progress: Value =
            serde_json::from_str(&db.bokutachi_progress(1).unwrap().unwrap()).unwrap();
        assert_eq!(progress["import_id"], "import-1");
        credentials.account_id = "42".into();
        crate::ir::credentials::save_credentials(root.path(), &credentials).unwrap();

        // Persist the explicit restart before waiting for the origin's rate limit.
        db.defer_bokutachi_origin(&origin, 400).unwrap();
        assert_eq!(resend_selected(&mut db, root.path(), &config, 300).await.failed, 0);
        let progress: Value =
            serde_json::from_str(&db.bokutachi_progress(1).unwrap().unwrap()).unwrap();
        assert_eq!(progress["retired_import_ids"], json!(["import-1"]));
        assert!(progress["import_id"].is_null());
        assert_eq!(job(&db).payload_json, original);
        // Simulate an old receipt surviving removal. It cannot resurrect the failed import.
        std::fs::write(&receipt, stale_receipt).unwrap();
        drop(db);
        let mut db = NetworkDatabase::open(&root.path().join("network.db")).unwrap();
        assert_eq!(run(&mut db, root.path(), &config, 400, false).await.failed, 0);
        assert_eq!(job(&db).payload_json, original);
        assert_eq!(run(&mut db, root.path(), &config, 405, false).await.submitted, 1);
        let progress: Value =
            serde_json::from_str(&db.bokutachi_progress(1).unwrap().unwrap()).unwrap();
        assert_eq!(progress["import_id"], "import-2");
        assert_eq!(progress["retired_import_ids"], json!(["import-1"]));
        let requests = server.await.unwrap();
        assert_eq!(requests[1], requests[requests.len() - 2]);
    }
}

#[tokio::test]
async fn unconfirmed_server_error_retains_import_even_on_selected_resend() {
    let path = "GET /api/v1/imports/import-1/poll-status";
    let (origin, server) = server(vec![
        ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
        accepted(),
        error(path, 500, false, None),
        ok(
            path,
            json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":[],"errors":[]}}),
        ),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
    run(&mut db, root.path(), &config, 100, false).await;
    assert_eq!(run(&mut db, root.path(), &config, 105, false).await.failed, 1);
    assert_eq!(resend_selected(&mut db, root.path(), &config, 106).await.failed, 0);
    assert_eq!(job(&db).status, "no_change");
    assert_eq!(job(&db).payload_json, original);
    server.await.unwrap();
}

#[tokio::test]
async fn another_accounts_key_cannot_submit_old_jobs_and_unsupported_jobs_never_call_http() {
    let root = tempfile::tempdir().unwrap();
    let (mut db, config, _) = setup(root.path(), "http://127.0.0.1:1", Game::PmsController);
    let mut credentials =
        crate::ir::credentials::load_credentials(root.path(), PROVIDER, "http://127.0.0.1:1")
            .unwrap()
            .unwrap();
    credentials.account_id = "43".into();
    crate::ir::credentials::save_credentials(root.path(), &credentials).unwrap();
    assert_eq!(run(&mut db, root.path(), &config, 100, false).await.failed, 1);
    assert!(job(&db).last_error.contains("another account"));
    for kind in [IrJobKind::Replay, IrJobKind::Course, IrJobKind::Attestation] {
        db.conn()
            .execute("UPDATE ir_score_jobs SET kind = ?1, status = 'pending'", [kind.as_str()])
            .unwrap();
        assert_eq!(run(&mut db, root.path(), &config, 101, true).await.failed, 1);
        let message: String =
            db.conn().query_row("SELECT last_error FROM ir_score_jobs", [], |r| r.get(0)).unwrap();
        assert!(
            message.contains(if kind == IrJobKind::Course {
                "missing or invalid saved course evidence"
            } else {
                "not supported"
            }),
            "{message}"
        );
    }
    assert!(db.enqueue_ir_score_attestation_jobs(PROVIDER, "42", 100).is_err());
    assert!(
        crate::ir::backfill::resolve_local_upload_target(&config, Some(PROVIDER))
            .unwrap_err()
            .to_string()
            .contains("not supported")
    );
    let query = crate::ir::ranking::ChartRankingQuery {
        profile_root: root.path().into(),
        provider: PROVIDER.into(),
        base_url: "http://127.0.0.1:1".into(),
        chart_sha256_hex: "ab".repeat(32),
        ln_policy: LnScorePolicy::ForceLn,
        double_option: crate::select_options::DoubleOptionScoreBucket::Off,
        rule_mode: bmz_gameplay::rule::RuleMode::Lr2Oraja,
    };
    for scope in
        [crate::ir::types::IrRankingScope::Global, crate::ir::types::IrRankingScope::SelfAndRivals]
    {
        assert!(
            crate::ir::ranking::fetch_ranking(&query, scope)
                .await
                .unwrap_err()
                .to_string()
                .contains("not supported")
        );
    }
}

#[test]
fn retry_after_supports_seconds_and_http_dates() {
    let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
    assert_eq!(super::client::retry_after_seconds("90", now), Some(90));
    assert_eq!(super::client::retry_after_seconds("Thu, 01 Jan 1970 00:03:10 GMT", now), Some(91));
    assert_eq!(super::client::retry_after_seconds("bad", now), None);
}

#[tokio::test]
async fn accepted_receipt_and_completion_db_failures_recover_without_reposting() {
    for fail_accept_save in [true, false] {
        let completed = || {
            ok(
                "GET /api/v1/imports/import-1/poll-status",
                json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":["real-score"],"errors":[]}}),
            )
        };
        let mut responses = vec![
            ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
            accepted(),
            completed(),
        ];
        if !fail_accept_save {
            responses.push(completed());
        }
        let (origin, server) = server(responses).await;
        let root = tempfile::tempdir().unwrap();
        let (mut db, config, original) = setup(root.path(), &origin, Game::Bms7k);
        db.conn()
            .execute_batch(if fail_accept_save {
                "CREATE TRIGGER fail_save BEFORE UPDATE ON bokutachi_imports
             WHEN json_extract(NEW.progress_json, '$.phase') = 'poll'
             BEGIN SELECT RAISE(ABORT, 'simulated accepted save failure'); END;"
            } else {
                "CREATE TRIGGER fail_save BEFORE UPDATE ON ir_score_jobs
             WHEN NEW.status = 'succeeded'
             BEGIN SELECT RAISE(ABORT, 'simulated completion failure'); END;"
            })
            .unwrap();
        let report = run(&mut db, root.path(), &config, 100, false).await;
        assert_eq!(report.failed, u32::from(fail_accept_save));
        if !fail_accept_save {
            assert_eq!(run(&mut db, root.path(), &config, 105, false).await.failed, 1);
        }
        assert_eq!(job(&db).payload_json, original);
        assert_eq!(
            db.conn()
                .query_row("SELECT COUNT(*) FROM ir_score_success_ledger", [], |r| r
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        db.conn().execute_batch("DROP TRIGGER fail_save").unwrap();
        drop(db);
        let mut db = NetworkDatabase::open(&root.path().join("network.db")).unwrap();
        assert_eq!(run(&mut db, root.path(), &config, 110, true).await.submitted, 1);
        assert_eq!(job(&db).status, "succeeded");
        assert_eq!(server.await.unwrap().iter().filter(|v| v.get("scores").is_some()).count(), 1);
    }
}

#[tokio::test]
async fn manual_retry_reports_pending_as_processed_and_no_change_as_terminal() {
    let (origin, server) = server(vec![
        ok("POST /api/v1/games/bms-7k/charts/resolve", json!({"chart":chart(Game::Bms7k)})),
        accepted(),
        ok(
            "GET /api/v1/imports/import-1/poll-status",
            json!({"importStatus":"completed","import":{"userID":42,"scoreIDs":[],"errors":[]}}),
        ),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let (mut db, config, _) = setup(root.path(), &origin, Game::Bms7k);
    let score_db = root.path().join("score.db");
    let logs = root.path().join("logs");
    let ctx = crate::ir::manual::IrSyncContext {
        score_db: &score_db,
        profile_root: root.path(),
        logs_dir: &logs,
        ir_config: &config,
    };
    for expected in ["pending", "no_change"] {
        let report =
            crate::ir::manual::resend_job(&ctx, &mut db, PROVIDER, "42", IrJobKind::Score, 1)
                .await
                .unwrap();
        assert_eq!((report.submitted, report.failed, report.skipped_busy), (0, 0, 0));
        assert_eq!(job(&db).status, expected);
    }
    let report = crate::ir::manual::resend_job(&ctx, &mut db, PROVIDER, "42", IrJobKind::Score, 1)
        .await
        .unwrap();
    assert_eq!(report.skipped_busy, 1);
    assert!(db.list_ir_score_jobs(&Default::default()).unwrap().is_empty());
    assert_eq!(
        db.list_ir_score_jobs(&crate::storage::network_db::IrScoreJobListFilter {
            statuses: &[crate::storage::network_db::IrScoreJobStatus::NoChange],
            ..Default::default()
        })
        .unwrap()
        .len(),
        1
    );
    server.await.unwrap();
}
