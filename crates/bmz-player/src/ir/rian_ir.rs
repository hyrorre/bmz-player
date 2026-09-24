//! rianIR の legacy HTTP API adapter。
//!
//! BMZ 内部の provider-neutral payload と rianIR wire payload の変換を
//! この module に閉じ込める。rianIR 側の既存 API / DB schema は変更しない。

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use base64::Engine;
use bmz_chart::model::LongNoteMode;
use bmz_gameplay::rule::RuleMode;
use reqwest::Url;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::config::profile_config::IrProviderConfig;
use crate::ln_policy::{ChartLnProfile, LnPolicySetting, LnScorePolicy};
use crate::select_options::DoubleOption;

use super::types::{
    IrAuthTokens, IrChartLnProfile, IrCourseRankingBody, IrCourseRankingCourseRef,
    IrCourseRankingEntry, IrCourseRankingResult, IrCourseRankingScore, IrJudgePayload,
    IrJudgeSidePayload, IrPlayerInfo, IrRankingBody, IrRankingChartRef, IrRankingEntry,
    IrRankingPagination, IrRankingPlayer, IrRankingResult, IrRankingScope, IrRankingScore,
    IrRankingSelfRef, IrRivalEntry, IrRivalProfile, IrScopedRankingResponse, IrScoreSubmission,
    IrSubmitResponse,
};

pub const RIAN_IR_PROVIDER: &str = "rian-ir";
pub const RIAN_IR_DEFAULT_BASE_URL: &str = "https://rianir.link/api/";
pub const RIAN_IR_PUBLIC_BASE_URL: &str = "https://rianir.link/";
/// rianIR が返すランキングの既定上限。
pub const RIAN_IR_RANKING_LIMIT: u32 = 100;

/// Build the public rianIR ranking page for a chart.
///
/// The configured rianIR URL is normally the API base, but the profile UI
/// also allows the public origin. Both forms must resolve to the web app's
/// `/ranking?sha256=...` route rather than an API endpoint.
pub fn chart_page_url(base_url: &str, sha256: &str) -> Result<String> {
    public_page_url(base_url, "ranking", "sha256", sha256)
}

/// Build the public rianIR ranking page for a course.
pub fn course_page_url(base_url: &str, course_hash: &str) -> Result<String> {
    public_page_url(base_url, "ranking/course_ranking", "course_sha256", course_hash)
}

fn public_page_url(base_url: &str, route: &str, query: &str, value: &str) -> Result<String> {
    let mut base = Url::parse(base_url).context("invalid rianIR public URL")?;
    base.set_query(None);
    base.set_fragment(None);

    let mut path = base.path().trim_end_matches('/').to_string();
    if path.is_empty() {
        path.push('/');
    } else if path == "/api" {
        path = "/".to_string();
    } else if path.ends_with("/api") {
        path.truncate(path.len() - "/api".len());
        if path.is_empty() {
            path.push('/');
        }
    }
    if !path.ends_with('/') {
        path.push('/');
    }
    base.set_path(&path);

    let mut page = base.join(route)?;
    page.query_pairs_mut().append_pair(query, value);
    Ok(page.to_string())
}

#[derive(Debug, Clone)]
pub struct RianIrClient {
    base_url: Url,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct RianSubmitOutcome {
    pub redacted_request_json: String,
    pub response_json: String,
}

#[derive(Debug, serde::Deserialize)]
struct RianLoginResponse {
    data: RianLoginData,
}

#[derive(Debug, serde::Deserialize)]
struct RianLoginData {
    attributes: RianLoginAttributes,
    meta: RianLoginMeta,
}

#[derive(Debug, serde::Deserialize)]
struct RianLoginAttributes {
    player_name: String,
}

#[derive(Debug, serde::Deserialize)]
struct RianLoginMeta {
    api_token: String,
}

#[derive(Debug, serde::Deserialize)]
struct RianRankingResponse {
    #[serde(default)]
    data: Vec<RianRankingResource>,
}

#[derive(Debug, serde::Deserialize)]
struct RianRankingResource {
    #[serde(default)]
    id: String,
    #[serde(default)]
    attributes: Map<String, Value>,
}

#[derive(Debug, serde::Deserialize)]
struct RianScoreSubmitResponse {
    #[serde(default)]
    score_id: Option<String>,
    #[serde(default)]
    ranking: Option<RianSubmitRanking>,
}

#[derive(Debug, serde::Deserialize)]
struct RianSubmitRanking {
    #[serde(default)]
    succeeded: bool,
    #[serde(default)]
    previous_rank: Option<u32>,
    #[serde(default)]
    current_rank: Option<u32>,
    #[serde(default)]
    total: Option<u32>,
    #[serde(default)]
    entries: Vec<RianRankingResource>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RianTableResource {
    pub id: String,
    pub attributes: RianTable,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RianTable {
    pub name: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub folders: Vec<RianTableFolder>,
    #[serde(default)]
    pub courses: Vec<RianTableCourse>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RianTableFolder {
    pub name: String,
    #[serde(default)]
    pub charts: Vec<RianTableChart>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RianTableChart {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub subartist: String,
    #[serde(default)]
    pub md5: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub level: serde_json::Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RianTableCourse {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub constraint: Vec<String>,
    #[serde(default)]
    pub charts: Vec<RianTableChart>,
}

#[derive(Debug, serde::Deserialize)]
struct RianTablesResponse {
    #[serde(default)]
    data: Vec<RianTableResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RianRivalScore {
    pub sha256: String,
    pub ln_mode: u8,
    pub ex_score: u32,
    pub clear_type: i32,
    pub max_combo: u32,
    pub min_bp: i32,
    pub play_option: i32,
    pub arrange_1p: String,
    pub arrange_2p: String,
    pub double_option: String,
    pub play_seed: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RianRivalScoresResponse {
    pub scores: Vec<RianRivalScore>,
    pub etag: String,
    pub not_modified: bool,
}

mod client;
mod ranking;
mod request;

pub use request::{
    body_for_rule_mode, body_for_rule_name, course_submission_supported, is_rian_ir_config,
    is_rian_ir_provider, score_duration_plausible, score_submission_supported,
};

#[cfg(test)]
use client::parse_base_url;
use ranking::*;
use request::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::{
        IrChartFeatures, IrChartLnProfile, IrChartNotes, IrChartPayload, IrClientInfo,
        IrEffectiveLnMode, IrRulePayload,
    };

    fn sample_payload() -> IrScoreSubmission {
        IrScoreSubmission {
            client: IrClientInfo {
                name: "BMZ".to_string(),
                version: "0.1.11".to_string(),
                platform: "test".to_string(),
            },
            chart: IrChartPayload {
                source_format: "bms".to_string(),
                sha256: "ab".repeat(32),
                md5: Some("cd".repeat(16)),
                length_ms: Some(123_456),
                ln_profile: IrChartLnProfile { has_defined_cn: true, ..Default::default() },
                title: "タイトル".to_string(),
                subtitle: String::new(),
                genre: "genre".to_string(),
                artist: "artist".to_string(),
                subartists: Vec::new(),
                mode: "8K".to_string(),
                level: Some(12),
                difficulty: String::new(),
                total: None,
                judge: None,
                bpm: None,
                notes: IrChartNotes { total: 100, ..Default::default() },
                features: IrChartFeatures::default(),
                urls: None,
                headers: BTreeMap::new(),
            },
            rule: IrRulePayload {
                play_mode: "single".to_string(),
                key_mode: "8K".to_string(),
                gauge: "Hard".to_string(),
                ln_policy: LnScorePolicy::ForceCn,
                effective_ln_mode: IrEffectiveLnMode::Cn,
                judge_algorithm: "bmz_v1".to_string(),
                scoring: "bms_ex_score_v1".to_string(),
                rule_mode: "Beatoraja".to_string(),
            },
            result: super::super::types::IrResultPayload {
                clear: "Hard".to_string(),
                played_at: 1_700_000_000,
                duration_ms: Some(120_000),
                judges: IrJudgePayload {
                    fast: IrJudgeSidePayload {
                        pgreat: 50,
                        great: 10,
                        good: 1,
                        bad: 2,
                        poor: 3,
                        empty_poor: 4,
                    },
                    slow: IrJudgeSidePayload {
                        pgreat: 40,
                        great: 5,
                        good: 0,
                        bad: 1,
                        poor: 2,
                        empty_poor: 1,
                    },
                },
                ex_score: 195,
                max_combo: 90,
                notes: 100,
                pass_notes: None,
                min_bp: 8,
                min_cb: 9,
                ghost: None,
            },
            play_options: BTreeMap::from([
                ("arrange_1p".to_string(), json!("f-random")),
                ("arrange_2p".to_string(), json!("mf-random")),
                ("applied_double_option".to_string(), json!("off")),
                ("random_seed".to_string(), json!("123456")),
            ]),
            replay: None,
            evidence: BTreeMap::new(),
            idempotency_key: "test".to_string(),
        }
    }

    #[test]
    fn provider_aliases_are_recognized() {
        assert!(is_rian_ir_provider("rian-ir"));
        assert!(is_rian_ir_provider("rianIR"));
        assert!(!is_rian_ir_provider("bmz-official"));
    }

    #[test]
    fn score_eligibility_accepts_auto_and_rejects_battle() {
        assert!(score_submission_supported(LnScorePolicy::ForceLn, DoubleOption::Off));
        assert!(score_submission_supported(LnScorePolicy::ForceHcn, DoubleOption::Flip));
        assert!(score_submission_supported(LnScorePolicy::AutoLn, DoubleOption::Off));
        assert!(score_submission_supported(LnScorePolicy::AutoHcn, DoubleOption::Off));
        assert!(!score_submission_supported(
            LnScorePolicy::ForceCn,
            DoubleOption::BattleAutoScratch
        ));
        assert!(course_submission_supported(LnPolicySetting::ForceLn, DoubleOption::Off));
        assert!(course_submission_supported(LnPolicySetting::AutoLn, DoubleOption::Off));
        assert!(course_submission_supported(LnPolicySetting::AutoHcn, DoubleOption::Flip));
        assert!(!course_submission_supported(LnPolicySetting::ForceHcn, DoubleOption::Battle));
    }

    #[test]
    fn rule_modes_keep_legacy_body_contract() {
        assert_eq!(body_for_rule_mode(RuleMode::Beatoraja), "beatoraja");
        assert_eq!(body_for_rule_mode(RuleMode::Lr2Oraja), "LR2oraja");
        assert_eq!(body_for_rule_mode(RuleMode::Dx), "DX MODE");
    }

    #[test]
    fn score_request_uses_canonical_ln_modes_and_extended_key_modes() {
        let request = score_request(&sample_payload(), "player", "token").unwrap();
        assert_eq!(request["body"], "beatoraja");
        assert!(request.get("rule_mode").is_none());
        assert_eq!(request["client"], "bmz-player");
        assert_eq!(request["play_mode"], "beat-8k");
        assert_eq!(request["ln_mode_format"], "canonical-v1");
        assert_eq!(request["song_ln_mode"], 2);
        assert_eq!(request["ln_mode"], 2);
        assert_eq!(request["arrange_1p"], "f-random");
        assert_eq!(request["arrange_2p"], "mf-random");
        assert_eq!(request["play_seed"], 123456);
        assert_eq!(request["pgreat"], 90);
        assert_eq!(request["poor"], 5);
        assert_eq!(request["miss"], 5);
        assert_eq!(request["length"], 123.456);
        assert_eq!(request["length_ms"], 123_456);
        assert_eq!(request["play_duration"], 120.0);
        assert_eq!(request["play_duration_ms"], 120_000);
        assert_eq!(request["has_random"], false);
    }

    #[test]
    fn score_request_sends_random_flag_without_zeroing_length() {
        let mut payload = sample_payload();
        payload.chart.features.random = true;

        let request = score_request(&payload, "player", "token").unwrap();

        assert_eq!(request["length_ms"], 123_456);
        assert_eq!(request["has_random"], true);
    }

    #[test]
    fn legacy_queued_score_keeps_legacy_duration_shape() {
        let mut payload = sample_payload();
        payload.chart.length_ms = None;

        let request = score_request(&payload, "player", "token").unwrap();

        assert_eq!(request["length"], 0.0);
        assert_eq!(request["play_duration"], 120.0);
        assert!(request.get("length_ms").is_none());
        assert!(request.get("play_duration_ms").is_none());
        assert!(request.get("has_random").is_none());
    }

    #[test]
    fn score_duration_preflight_matches_server_bounds_and_exemptions() {
        assert!(score_duration_plausible("Hard", Some(100_000), Some(80_000), false));
        assert!(score_duration_plausible("Hard", Some(100_000), Some(129_999), false));
        assert!(!score_duration_plausible("Hard", Some(100_000), Some(79_999), false));
        assert!(!score_duration_plausible("Hard", Some(100_000), Some(130_001), false));
        assert!(score_duration_plausible("Failed", Some(100_000), Some(1_000), false));
        assert!(score_duration_plausible("Hard", Some(100_000), Some(1_000), true));
        assert!(score_duration_plausible("Hard", None, Some(1_000), false));
    }

    #[test]
    fn score_request_maps_source_and_played_ln_mode_matrix() {
        let cases = [
            (IrChartLnProfile::default(), LnScorePolicy::ForceCn, 0, 0),
            (
                IrChartLnProfile { has_undefined_ln: true, ..Default::default() },
                LnScorePolicy::ForceLn,
                1,
                1,
            ),
            (
                IrChartLnProfile { has_undefined_ln: true, ..Default::default() },
                LnScorePolicy::AutoCn,
                1,
                2,
            ),
            (
                IrChartLnProfile { has_defined_cn: true, ..Default::default() },
                LnScorePolicy::ForceLn,
                2,
                1,
            ),
            (
                IrChartLnProfile {
                    has_defined_ln: true,
                    has_defined_cn: true,
                    ..Default::default()
                },
                LnScorePolicy::AutoLn,
                2,
                2,
            ),
            (
                IrChartLnProfile {
                    has_undefined_ln: true,
                    has_defined_cn: true,
                    ..Default::default()
                },
                LnScorePolicy::AutoHcn,
                2,
                3,
            ),
            (
                IrChartLnProfile {
                    has_defined_ln: true,
                    has_defined_cn: true,
                    has_defined_hcn: true,
                    ..Default::default()
                },
                LnScorePolicy::AutoLn,
                3,
                3,
            ),
            (
                IrChartLnProfile { has_defined_hcn: true, ..Default::default() },
                LnScorePolicy::ForceLn,
                3,
                1,
            ),
        ];

        for (profile, policy, expected_song, expected_played) in cases {
            let mut payload = sample_payload();
            payload.chart.ln_profile = profile;
            payload.rule.ln_policy = policy;
            let request = score_request(&payload, "player", "token").unwrap();
            assert_eq!(request["ln_mode_format"], "canonical-v1");
            assert_eq!(request["song_ln_mode"], expected_song, "profile={profile:?}");
            assert_eq!(
                request["ln_mode"], expected_played,
                "profile={profile:?}, policy={policy:?}"
            );
        }
    }

    #[test]
    fn all_supported_key_modes_have_canonical_rian_names() {
        assert_eq!(play_mode("4K"), "beat-4k");
        assert_eq!(play_mode("5K"), "beat-5k");
        assert_eq!(play_mode("6K"), "beat-6k");
        assert_eq!(play_mode("7K"), "beat-7k");
        assert_eq!(play_mode("8K"), "beat-8k");
        assert_eq!(play_mode("9K"), "popn-9k");
        assert_eq!(play_mode("10K"), "beat-10k");
        assert_eq!(play_mode("14K"), "beat-14k");
    }

    #[test]
    fn course_request_uses_body_and_rian_course_hash_v1() {
        let local_course_hash = "ef".repeat(32);
        let payload = json!({
            "course": {
                "course_hash": local_course_hash,
                "title": "段位",
                "charts": ["ab".repeat(32), "cd".repeat(32)],
                "constraints": { "grade": true },
            },
            "rule": {
                "gauge": "Class",
                "ln_policy": "ForceHcn",
                "effective_ln_mode": 3,
                "rule_mode": "Dx",
            },
            "result": {
                "clear": "Normal",
                "ex_score": 4000,
                "max_ex_score": 6000,
                "total_notes": 3000,
                "max_combo": 1200,
                "bp": 10,
                "played_at": 1_700_000_001_i64,
                "judges": {
                    "pgreat": 1900,
                    "great": 200,
                    "good": 3,
                    "bad": 4,
                    "poor": 5,
                    "empty_poor": 6,
                },
            },
            "play_options": {
                "option": "AllScratch",
                "random_seed": "281474976710655",
            },
        });
        let request = course_request(&payload, "player", "token").unwrap();
        assert_eq!(
            request["course_sha256"],
            "c3a672ab2881fdd8efb583ff04e94fa88c9ff730941eb72063dadc59101f6d77"
        );
        assert_eq!(
            request["signature"],
            signature(
                "token",
                &[
                    "player".to_string(),
                    "c3a672ab2881fdd8efb583ff04e94fa88c9ff730941eb72063dadc59101f6d77".to_string(),
                    "4000".to_string(),
                    "1200".to_string(),
                    "1700000001".to_string(),
                ],
            )
            .unwrap()
        );
        assert_eq!(request["body"], "DX MODE");
        assert!(request.get("rule_mode").is_none());
        assert_eq!(request["ln_mode"], 3);
        assert_eq!(request["ln_mode_format"], "canonical-v1");
        assert_eq!(request["arrange_1p"], "all-scratch");
        assert_eq!(request["play_seed"], 281_474_976_710_655_i64);
        assert_eq!(request["constraint"], json!(["grade"]));
        assert!(request["tracks"].as_array().unwrap().is_empty());
        assert_eq!(request["miss"], 6);
    }

    #[test]
    fn course_request_accepts_auto_with_canonical_effective_mode() {
        let payload = json!({
            "course": {
                "title": "Course",
                "charts": ["ab".repeat(32)],
                "constraints": { "ln": "cn" },
            },
            "rule": {
                "gauge": "Class",
                "ln_policy": "AutoLn",
                "effective_ln_mode": 3,
                "rule_mode": "Beatoraja",
            },
            "result": {
                "clear": "Normal",
                "ex_score": 100,
                "max_ex_score": 200,
                "total_notes": 100,
                "max_combo": 50,
                "bp": 1,
                "played_at": 1_700_000_001_i64,
            },
        });

        let request = course_request(&payload, "player", "token").unwrap();
        assert_eq!(request["ln_mode"], 3);
        assert_eq!(request["ln_mode_format"], "canonical-v1");
    }

    #[test]
    fn ranking_conversion_accepts_string_database_values() {
        let resource = RianRankingResource {
            id: "score-1".to_string(),
            attributes: serde_json::from_value(json!({
                "player_name": "login-id",
                "display_name": "Player",
                "clear_type": "6",
                "ex_score": "1999",
                "max_combo": "999",
                "min_bp": "4",
                "pgreat": "900",
                "great": "199",
                "miss": "1",
                "play_date": "2026-07-28 12:00:00",
            }))
            .unwrap(),
        };
        let ranking = convert_score_ranking(&"ab".repeat(32), vec![resource], 20, Some("login-id"));
        let entry = &ranking.ranking.entries[0];
        assert_eq!(entry.rank, 1);
        assert_eq!(entry.player.display_name, "Player");
        assert_eq!(entry.score.clear, "Hard");
        assert_eq!(entry.score.ex_score, 1999);
        assert_eq!(entry.score.judges.unwrap().fast.empty_poor, 1);
        assert_eq!(ranking.ranking.self_summary.as_ref().unwrap().rank, 1);
    }

    #[test]
    fn ranking_conversion_keeps_top_100_and_self_at_rank_100() {
        let resources = (1..=101)
            .map(|rank| RianRankingResource {
                id: format!("score-{rank}"),
                attributes: serde_json::from_value(json!({
                    "player_name": format!("login-{rank}"),
                    "display_name": format!("Player {rank}"),
                    "ex_score": 10_000 - rank,
                }))
                .unwrap(),
            })
            .collect();

        let ranking = convert_score_ranking(
            &"ab".repeat(32),
            resources,
            RIAN_IR_RANKING_LIMIT,
            Some("login-100"),
        );

        assert_eq!(ranking.ranking.entries.len(), 100);
        assert_eq!(ranking.ranking.entries.last().unwrap().rank, 100);
        assert_eq!(ranking.ranking.self_summary.as_ref().unwrap().rank, 100);
        assert_eq!(ranking.ranking.pagination.unwrap().limit, 100);
        assert_eq!(ranking.ranking.pagination.unwrap().total, Some(101));
        assert!(ranking.ranking.pagination.unwrap().has_more);
    }

    #[test]
    fn ranking_conversion_uses_competition_ranks_for_ties() {
        let resources = [2000, 1900, 1900, 1800]
            .into_iter()
            .enumerate()
            .map(|(index, ex_score)| RianRankingResource {
                id: format!("score-{index}"),
                attributes: serde_json::from_value(json!({
                    "player_name": format!("login-{index}"),
                    "ex_score": ex_score,
                }))
                .unwrap(),
            })
            .collect();

        let ranking = convert_score_ranking(&"ab".repeat(32), resources, 20, Some("login-2"));

        assert_eq!(
            ranking.ranking.entries.iter().map(|entry| entry.rank).collect::<Vec<_>>(),
            vec![1, 2, 2, 4]
        );
        assert_eq!(ranking.ranking.self_summary.unwrap().rank, 2);
    }

    #[test]
    fn submission_ranking_accepts_explicit_current_previous_and_total() {
        let decoded: RianScoreSubmitResponse = serde_json::from_value(json!({
            "status": "success",
            "score_id": "42",
            "ranking": {
                "succeeded": true,
                "previous_rank": 12,
                "current_rank": 8,
                "total": 341,
                "entries": [{
                    "type": "scores",
                    "id": "42",
                    "attributes": {
                        "player_name": "login-id",
                        "display_name": "Player",
                        "ex_score": 2000
                    }
                }]
            }
        }))
        .unwrap();
        let raw = decoded.ranking.unwrap();
        let ranking = convert_score_submission_ranking(
            &"ab".repeat(32),
            raw.entries,
            RIAN_IR_RANKING_LIMIT,
            Some("login-id"),
            raw.current_rank,
            raw.total,
        );

        assert_eq!(decoded.score_id.as_deref(), Some("42"));
        assert_eq!(raw.previous_rank, Some(12));
        assert_eq!(ranking.ranking.self_summary.unwrap().rank, 8);
        let pagination = ranking.ranking.pagination.unwrap();
        assert_eq!(pagination.limit, RIAN_IR_RANKING_LIMIT);
        assert_eq!(pagination.total, Some(341));
        assert!(pagination.has_more);
    }

    #[test]
    fn hmac_matches_rfc_4231_vector() {
        assert_eq!(
            hmac_sha256_hex(&[0x0b; 20], b"Hi There"),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn submission_logs_redact_api_token_and_signature() {
        let request = score_request(&sample_payload(), "player", "secret-token").unwrap();
        let logged = redacted_request_json(&request).unwrap();
        assert!(!logged.contains("secret-token"));
        assert!(logged.contains("[REDACTED]"));
    }

    #[test]
    fn base_url_accepts_origin_and_api_prefix() {
        assert_eq!(
            parse_base_url("https://example.test").unwrap().as_str(),
            "https://example.test/api/"
        );
        assert_eq!(
            parse_base_url("https://example.test/api/").unwrap().as_str(),
            "https://example.test/api/"
        );
    }

    #[test]
    fn submit_only_score_request_omits_ranking_query() {
        let client = RianIrClient::new("https://example.test/").unwrap();

        assert_eq!(
            client.score_submit_url(false).unwrap().as_str(),
            "https://example.test/api/score/score.php"
        );
        assert_eq!(
            client.score_submit_url(true).unwrap().as_str(),
            "https://example.test/api/score/score.php?include=ranking"
        );
    }

    #[test]
    fn chart_page_url_uses_rian_ranking_route_for_origin_and_api_urls() {
        let sha256 = "ab".repeat(32);

        assert_eq!(
            chart_page_url("https://rianir.link/", &sha256).unwrap(),
            format!("https://rianir.link/ranking?sha256={sha256}")
        );
        assert_eq!(
            chart_page_url("https://rianir.link/api/", &sha256).unwrap(),
            format!("https://rianir.link/ranking?sha256={sha256}")
        );
        assert_eq!(
            chart_page_url("https://example.test/rianir/api/", &sha256).unwrap(),
            format!("https://example.test/rianir/ranking?sha256={sha256}")
        );
    }

    #[test]
    fn course_page_url_uses_rian_course_ranking_route() {
        let course_sha256 = "course-hash";

        assert_eq!(
            course_page_url("https://rianir.link/api/", course_sha256).unwrap(),
            "https://rianir.link/ranking/course_ranking?course_sha256=course-hash"
        );
        assert_eq!(
            course_page_url("https://example.test/rianir/api/", course_sha256).unwrap(),
            "https://example.test/rianir/ranking/course_ranking?course_sha256=course-hash"
        );
    }

    #[test]
    fn tables_response_accepts_dynamic_folders_and_courses() {
        let response: RianTablesResponse = serde_json::from_value(json!({
            "data": [{
                "type": "difficulty-tables",
                "id": "0",
                "attributes": {
                    "name": "rianIR POPULAR",
                    "symbol": "POP",
                    "folders": [{
                        "name": "24H POPULAR SONGS",
                        "charts": [{
                            "title": "Song",
                            "artist": "Artist",
                            "md5": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                            "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                            "level": "Top 20"
                        }]
                    }],
                    "courses": [{
                        "name": "Course",
                        "sha256": "course-hash",
                        "constraint": ["grade", "ln"],
                        "charts": [{
                            "title": "Song",
                            "subtitle": "[Another]",
                            "artist": "Artist",
                            "subartist": "",
                            "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                            "level": 12
                        }]
                    }]
                }
            }]
        }))
        .unwrap();

        assert_eq!(response.data[0].attributes.folders[0].charts[0].level, json!("Top 20"));
        assert_eq!(response.data[0].attributes.courses[0].constraint, vec!["grade", "ln"]);
    }

    #[test]
    fn compact_rival_score_attributes_keep_structured_f_random() {
        let attributes: Map<String, Value> = serde_json::from_value(json!({
            "sha256": "ab".repeat(32),
            "ln_mode": "1",
            "ex_score": "1900",
            "clear_type": "5",
            "max_combo": "800",
            "min_bp": "9",
            "play_option": "0",
            "arrange_1p": "f-random",
            "arrange_2p": "normal",
            "double_option": "off",
            "play_seed": "123456"
        }))
        .unwrap();
        assert_eq!(string_attr(&attributes, "arrange_1p"), "f-random");
        assert_eq!(int_attr(&attributes, "play_option"), 0);
        assert_eq!(int_attr(&attributes, "play_seed"), 123456);
    }

    #[test]
    fn ranking_entry_keeps_arrangement_seed_for_g_battle() {
        let attributes: Map<String, Value> = serde_json::from_value(json!({
            "player_name": "rival",
            "ex_score": "1900",
            "arrange_1p": "random",
            "arrange_2p": "mirror",
            "double_option": "flip",
            "play_seed": "16777218"
        }))
        .unwrap();

        let entry = score_ranking_entry(0, RianRankingResource { id: String::new(), attributes });

        assert_eq!(entry.score.arrange_1p.as_deref(), Some("random"));
        assert_eq!(entry.score.arrange_2p.as_deref(), Some("mirror"));
        assert_eq!(entry.score.double_option.as_deref(), Some("flip"));
        assert_eq!(entry.score.random_seed, Some(16_777_218));
    }

    #[test]
    fn ranking_entry_decodes_legacy_packed_arrangement_for_g_battle() {
        let attributes: Map<String, Value> = serde_json::from_value(json!({
            "player_name": "rival",
            "play_option": 121,
            "play_seed": 42
        }))
        .unwrap();

        let entry = score_ranking_entry(0, RianRankingResource { id: String::new(), attributes });

        assert_eq!(entry.score.arrange_1p.as_deref(), Some("mirror"));
        assert_eq!(entry.score.arrange_2p.as_deref(), Some("random"));
        assert_eq!(entry.score.double_option.as_deref(), Some("flip"));
    }
}
