use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ln_policy::LnScorePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrRankingScope {
    Global,
    SelfAndRivals,
    Rivals,
    SelfOnly,
    AroundSelf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrEffectiveLnMode {
    Ln,
    Cn,
    Hcn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrScoreSubmission {
    pub client: IrClientInfo,
    pub chart: IrChartPayload,
    pub rule: IrRulePayload,
    pub result: IrResultPayload,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub play_options: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<IrReplayPayload>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub evidence: BTreeMap<String, serde_json::Value>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrClientInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrChartPayload {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    pub ln_profile: IrChartLnProfile,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub genre: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subartists: Vec<String>,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub difficulty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<IrChartBpm>,
    pub notes: IrChartNotes,
    pub features: IrChartFeatures,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urls: Option<IrChartUrls>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IrChartUrls {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IrChartBpm {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct IrChartLnProfile {
    pub has_undefined_ln: bool,
    pub has_defined_ln: bool,
    pub has_defined_cn: bool,
    pub has_defined_hcn: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct IrChartNotes {
    pub total: u32,
    pub ln: u32,
    pub cn: u32,
    pub hcn: u32,
    pub mine: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct IrChartFeatures {
    pub random: bool,
    pub stop: bool,
    pub ln: bool,
    pub cn: bool,
    pub hcn: bool,
    pub mine: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRulePayload {
    pub play_mode: String,
    pub key_mode: String,
    pub gauge: String,
    pub ln_policy: LnScorePolicy,
    pub effective_ln_mode: IrEffectiveLnMode,
    pub judge_algorithm: String,
    pub scoring: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rule_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrResultPayload {
    pub clear: String,
    pub played_at: i64,
    pub judges: IrJudgePayload,
    pub ex_score: u32,
    pub max_combo: u32,
    pub notes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_notes: Option<u32>,
    pub min_bp: u32,
    pub min_cb: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghost: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IrJudgePayload {
    pub fast: IrJudgeSidePayload,
    pub slow: IrJudgeSidePayload,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IrJudgeSidePayload {
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
    pub empty_poor: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrReplayPayload {
    pub hash: String,
    pub format: String,
    pub upload_intent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrSubmitOptions {
    #[serde(default)]
    pub ranking_scopes: Vec<IrRankingScope>,
    #[serde(default = "default_ranking_limit")]
    pub ranking_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrSubmitResponse {
    pub accepted: bool,
    pub score_id: Option<String>,
    pub best_updated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_best: Option<IrPreviousBest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rankings: BTreeMap<IrRankingScope, IrScopedRankingResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrScopedRankingResponse {
    pub succeeded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<IrRankingResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrPreviousBest {
    pub clear_type: String,
    pub ex_score: u32,
    pub max_combo: u32,
    pub min_bp: u32,
    pub min_cb: u32,
}

pub fn default_ranking_limit() -> u32 {
    100
}

/// `GET /api/v1/me/scores` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrOwnScoreHistoryResult {
    #[serde(default)]
    pub scores: Vec<IrOwnScoreHistoryEntry>,
    pub pagination: IrPagination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrPagination {
    pub limit: u32,
    pub offset: u32,
    pub total: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrOwnScoreHistoryEntry {
    pub score_id: String,
    pub chart_sha256: String,
    pub clear: String,
    pub ex_score: u32,
    pub max_combo: u32,
    pub min_bp: u32,
    pub min_cb: u32,
    pub bp: u32,
    pub cb: u32,
    pub gauge: String,
    pub ln_policy: String,
    pub double_option: String,
    pub rule_mode: String,
    pub judges: IrJudgePayload,
    pub notes: u32,
    pub pass_notes: u32,
    #[serde(default)]
    pub device_type: String,
    #[serde(default)]
    pub arrange_1p: Option<String>,
    #[serde(default)]
    pub arrange_2p: Option<String>,
    #[serde(default)]
    pub random_seed: Option<i64>,
    #[serde(default)]
    pub assist_mask: Option<u32>,
    #[serde(default)]
    pub played_at: Option<i64>,
    pub server_received_at: i64,
    pub verification: String,
    #[serde(default)]
    pub replay_hash: Option<String>,
}

/// `/api/v1/auth/login` / `/api/v1/auth/refresh` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrAuthTokens {
    pub provider_key: String,
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub player: IrPlayerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrPlayerInfo {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrMeResponse {
    pub player: IrPlayerInfo,
}

/// `/api/v1/scores/{id}/replay/upload-url` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrReplayUploadTarget {
    pub upload_url: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub required_hash: Option<String>,
}

/// `GET /api/v1/scores/{id}/replay` (ダウンロード URL 発行) のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrReplayDownloadTarget {
    pub download_url: String,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub status: Option<String>,
}

/// `/api/v1/scores/{id}/replay/verify` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrReplayVerifyResult {
    pub status: String,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub hash: Option<String>,
}

/// `/api/v1/rivals` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRivalsResponse {
    #[serde(default)]
    pub rivals: Vec<IrRivalEntry>,
}

/// `/api/v1/device-keys` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrDeviceKeysResponse {
    #[serde(default)]
    pub device_keys: Vec<IrDeviceKeyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrDeviceKeyEntry {
    pub id: String,
    pub public_key: String,
    pub algorithm: String,
    #[serde(default)]
    pub revoked_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRivalEntry {
    pub player_id: String,
    #[serde(default)]
    pub relation_type: String,
    #[serde(default)]
    pub profile: Option<IrRivalProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRivalProfile {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub bio: Option<String>,
}

/// `/api/v1/charts/{sha256}/ranking` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRankingResult {
    pub chart: IrRankingChartRef,
    pub ranking: IrRankingBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRankingChartRef {
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrCourseRankingResult {
    pub course: IrCourseRankingCourseRef,
    #[serde(default)]
    pub rule: Option<IrCourseRankingRuleRef>,
    pub ranking: IrCourseRankingBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrCourseRankingCourseRef {
    pub course_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrCourseRankingRuleRef {
    #[serde(default)]
    pub gauge: Option<String>,
    #[serde(default)]
    pub ln_policy: Option<String>,
    #[serde(default)]
    pub scoring: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrCourseRankingBody {
    pub scope: IrRankingScope,
    #[serde(default)]
    pub entries: Vec<IrCourseRankingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrCourseRankingEntry {
    pub rank: u32,
    pub player: IrRankingPlayer,
    pub score: IrCourseRankingScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrCourseRankingScore {
    pub course_score_id: String,
    pub clear: String,
    #[serde(default)]
    pub course_clear: bool,
    pub ex_score: u32,
    pub max_combo: u32,
    pub bp: u32,
    #[serde(default)]
    pub device_type: Option<String>,
    #[serde(default)]
    pub played_at: Option<String>,
    #[serde(default)]
    pub verification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRankingBody {
    pub scope: IrRankingScope,
    #[serde(default)]
    pub entries: Vec<IrRankingEntry>,
    /// 全プレイヤー中のクリア率 (%)。
    #[serde(default)]
    pub clear_rate: Option<u32>,
    #[serde(rename = "self", default)]
    pub self_summary: Option<IrRankingSelfRef>,
    #[serde(default)]
    pub pagination: Option<IrRankingPagination>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IrRankingPagination {
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
    /// scope 内の総エントリ数。
    #[serde(default)]
    pub total: Option<u32>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRankingSelfRef {
    pub rank: u32,
    #[serde(default)]
    pub score_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRankingEntry {
    pub rank: u32,
    #[serde(default)]
    pub scope_rank: Option<u32>,
    pub player: IrRankingPlayer,
    pub score: IrRankingScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRankingPlayer {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRankingScore {
    pub clear: String,
    pub ex_score: u32,
    pub max_combo: u32,
    pub min_bp: u32,
    pub min_cb: u32,
    #[serde(default)]
    pub device_type: Option<String>,
    #[serde(default)]
    pub played_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_response_decodes_included_global_ranking() {
        let response: IrSubmitResponse = serde_json::from_value(serde_json::json!({
            "accepted": true,
            "score_id": "score-1",
            "best_updated": true,
            "rankings": {
                "global": {
                    "succeeded": true,
                    "data": {
                        "chart": { "sha256": "abc" },
                        "ranking": {
                            "scope": "global",
                            "entries": [],
                            "clear_rate": 100,
                            "pagination": {
                                "limit": 20,
                                "offset": 0,
                                "total": 1,
                                "has_more": false
                            }
                        }
                    }
                }
            }
        }))
        .unwrap();

        let global = response.rankings.get(&IrRankingScope::Global).unwrap();
        assert!(global.succeeded);
        assert_eq!(global.data.as_ref().unwrap().chart.sha256, "abc");
        assert_eq!(
            global.data.as_ref().unwrap().ranking.pagination.as_ref().unwrap().total,
            Some(1)
        );
    }
}
