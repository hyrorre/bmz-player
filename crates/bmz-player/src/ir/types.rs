use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ln_policy::LnScorePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub notes: IrChartNotes,
    pub features: IrChartFeatures,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrResultPayload {
    pub clear: String,
    pub played_at: i64,
    pub judges: IrJudgePayload,
    pub ex_score: u32,
    pub max_combo: u32,
    pub notes: u32,
    pub pass_notes: u32,
    pub min_bp: u32,
    pub min_cb: u32,
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
}

pub fn default_ranking_limit() -> u32 {
    100
}

/// `/api/v1/auth/login` / `/api/v1/auth/refresh` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrAuthTokens {
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

/// `/api/v1/rivals` のレスポンス。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrRivalsResponse {
    #[serde(default)]
    pub rivals: Vec<IrRivalEntry>,
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
pub struct IrRankingBody {
    pub scope: IrRankingScope,
    #[serde(default)]
    pub entries: Vec<IrRankingEntry>,
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
