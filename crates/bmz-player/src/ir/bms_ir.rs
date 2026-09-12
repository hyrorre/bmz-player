//! Native bmz-player adapter for BMS-IR.
//!
//! The durable queue keeps bmz-player's provider-neutral score payload. This
//! module owns only BMS-IR authentication, eligibility, request wrapping, and
//! response decoding.

use anyhow::{Context, Result, bail};
use bmz_chart::model::ChartSourceFormat;
use bmz_gameplay::rule::RuleMode;
use reqwest::{StatusCode, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::config::profile_config::IrProviderConfig;
use crate::ln_policy::{ChartLnProfile, LnScorePolicy};
use crate::select_options::DoubleOption;

use super::bmz_official::{IrCourseRankingRequest, IrOwnScoreHistoryRequest, IrRankingRequest};
use super::http_error::{
    http_response_error, retry_after_header, retry_after_seconds_from_error, status_code_from_error,
};
use super::rian_ir::{RianRivalScore, RianRivalScoresResponse, RianTableChart, RianTableResource};
use super::types::{
    IrAuthTokens, IrCourseRankingResult, IrOwnScoreHistoryResult, IrPlayerInfo, IrRankingResult,
    IrRivalsResponse, IrScoreSubmission, IrSubmitResponse,
};

pub const BMS_IR_PROVIDER: &str = "bms-ir";
pub const BMS_IR_PRODUCTION_BASE_URL: &str = "https://www.bms-ir.org";

/// Compile-time override for local integration builds. Normal release builds
/// keep the production endpoint and profile files cannot redirect the fixed
/// BMS-IR credential target.
pub const BMS_IR_DEFAULT_BASE_URL: &str = match option_env!("BMZ_BMS_IR_BASE_URL") {
    Some(value) => value,
    None => BMS_IR_PRODUCTION_BASE_URL,
};

const BMS_IR_KEY_MODES: &[&str] = &["4K", "5K", "6K", "7K", "8K", "9K", "10K", "14K", "24K", "48K"];

fn is_supported_key_mode(mode: &str) -> bool {
    BMS_IR_KEY_MODES.contains(&mode)
}

#[derive(Debug, Clone)]
pub struct BmsIrClient {
    base_url: Url,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct BmsIrSubmitOutcome {
    pub redacted_request_json: String,
    pub response_json: String,
}

#[derive(Debug, serde::Deserialize)]
struct BmsIrLoginResponse {
    ok: bool,
    player_id: u64,
}

#[derive(Debug, serde::Deserialize)]
struct BmsIrRivalsResponse {
    rivals: Vec<super::types::IrRivalEntry>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    complete: Option<bool>,
    #[serde(default)]
    truncated: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
struct BmsIrRivalScoresResponse {
    scores: Vec<RianRivalScore>,
    etag: String,
    not_modified: bool,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    complete: Option<bool>,
    #[serde(default)]
    truncated: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
struct BmsIrTablesResponse {
    data: Vec<RianTableResource>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    complete: Option<bool>,
    #[serde(default)]
    truncated: Option<bool>,
}

pub fn is_bms_ir_provider(provider: &str) -> bool {
    matches!(provider.trim().to_ascii_lowercase().as_str(), "bms-ir" | "bmsir" | "bms_ir")
}

pub fn is_bms_ir_config(provider: &IrProviderConfig) -> bool {
    is_bms_ir_provider(&provider.provider) || is_bms_ir_provider(&provider.provider_key)
}

/// Resolves the only origin that may receive a BMS-IR game token. A caller may
/// spell that same origin with a trailing slash, but a different origin is
/// rejected before any HTTP request or provider entry is created.
pub fn fixed_base_url(requested: Option<&str>) -> Result<String> {
    let configured = parse_http_base_url(BMS_IR_DEFAULT_BASE_URL)?;
    if let Some(requested) = requested {
        let requested = parse_http_base_url(requested)?;
        if requested.origin() != configured.origin() {
            bail!("BMS-IR does not allow a custom base URL");
        }
    }
    Ok(BMS_IR_DEFAULT_BASE_URL.to_string())
}

pub fn score_submission_supported(
    rule_mode: RuleMode,
    source_format: ChartSourceFormat,
    _source_ln_profile: ChartLnProfile,
    _ln_policy: LnScorePolicy,
    double_option: DoubleOption,
) -> bool {
    matches!(rule_mode, RuleMode::Beatoraja | RuleMode::Lr2Oraja)
        && matches!(
            source_format,
            ChartSourceFormat::Bms | ChartSourceFormat::Bmson | ChartSourceFormat::Pms
        )
        && matches!(
            double_option,
            DoubleOption::Off
                | DoubleOption::Flip
                | DoubleOption::Battle
                | DoubleOption::BattleAutoScratch
        )
}

pub fn ensure_score_payload_supported(payload: &IrScoreSubmission) -> Result<()> {
    if crate::ir::backfill::is_local_backfill_submission(payload) {
        bail!("BMS-IR local score backfill is disabled");
    }
    if !matches!(payload.rule.rule_mode.as_str(), "Beatoraja" | "Lr2Oraja") {
        bail!("BMS-IR rule mode is not supported");
    }
    if payload.rule.judge_algorithm != "bmz_v1" || payload.rule.scoring != "bms_ex_score_v1" {
        bail!("BMS-IR score algorithm is not supported");
    }
    if !matches!(payload.chart.source_format.as_str(), "bms" | "bmson" | "pms") {
        bail!("BMS-IR chart source format is not supported");
    }
    if !is_supported_key_mode(&payload.chart.mode) {
        bail!("BMS-IR key mode is not supported");
    }
    let course_stage =
        payload.play_options.get("course_stage").and_then(Value::as_bool).unwrap_or(false);
    let clear_supported = if course_stage {
        matches!(payload.result.clear.as_str(), "NoPlay" | "FullCombo" | "Perfect" | "Max")
    } else {
        matches!(
            payload.result.clear.as_str(),
            "Failed" | "Easy" | "Normal" | "Hard" | "ExHard" | "FullCombo" | "Perfect" | "Max"
        )
    };
    if !clear_supported {
        bail!("BMS-IR clear type is not supported");
    }
    let double_option = payload
        .play_options
        .get("applied_double_option")
        .or_else(|| payload.play_options.get("double_option"))
        .and_then(Value::as_str)
        .unwrap_or("off");
    if !matches!(double_option, "off" | "flip" | "battle" | "battle_auto_scratch") {
        bail!("BMS-IR double option is not supported");
    }
    if payload.play_options.get("assist_mask").and_then(Value::as_u64).unwrap_or(0) != 0 {
        bail!("BMS-IR assisted scores are not supported");
    }
    Ok(())
}

impl BmsIrClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = fixed_base_url(Some(base_url))?;
        Self::new_with_timeout(&base_url, std::time::Duration::from_secs(15))
    }

    fn new_with_timeout(base_url: &str, timeout: std::time::Duration) -> Result<Self> {
        let mut base_url = parse_http_base_url(base_url)?;
        base_url.set_query(None);
        base_url.set_fragment(None);
        Ok(Self {
            base_url,
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .context("failed to build BMS-IR HTTP client")?,
        })
    }

    pub async fn login(&self, player_id: &str, game_token: &str) -> Result<IrAuthTokens> {
        let player_id = parse_player_id(player_id)?;
        if game_token.trim().is_empty() {
            bail!("BMS-IR game token is empty");
        }
        let response = self
            .http
            .post(self.endpoint("/api/bmz-player/v1/login")?)
            .json(&serde_json::json!({
                "player_id": player_id,
                "game_token": game_token,
            }))
            .send()
            .await
            .context("failed to send BMS-IR login request")?;
        let decoded: BmsIrLoginResponse = decode_response(response, "BMS-IR login").await?;
        if !decoded.ok || decoded.player_id != player_id {
            bail!("BMS-IR login response did not confirm the requested player ID");
        }
        Ok(IrAuthTokens {
            provider_key: BMS_IR_PROVIDER.to_string(),
            access_token: game_token.to_string(),
            refresh_token: String::new(),
            expires_at: None,
            player: IrPlayerInfo {
                id: player_id.to_string(),
                email: None,
                display_name: Some(player_id.to_string()),
            },
        })
    }

    pub async fn submit_score(
        &self,
        payload: &IrScoreSubmission,
        player_id: &str,
        game_token: &str,
        include_ranking: bool,
    ) -> Result<BmsIrSubmitOutcome> {
        ensure_score_payload_supported(payload)?;
        let player_id = parse_player_id(player_id)?;
        if game_token.trim().is_empty() {
            bail!("BMS-IR game token is empty");
        }
        let request = score_request_value(player_id, game_token, payload, include_ranking)?;
        let redacted_request_json = redacted_score_request_json(&request)?;
        let response = self
            .http
            .post(self.endpoint("/api/bmz-player/v1/score")?)
            .json(&request)
            .send()
            .await
            .context("failed to send BMS-IR score request")?;
        let decoded: IrSubmitResponse =
            decode_response(response, "BMS-IR score submission").await?;
        if !decoded.accepted {
            bail!("BMS-IR did not accept the score");
        }
        Ok(BmsIrSubmitOutcome {
            redacted_request_json,
            response_json: serde_json::to_string(&decoded)?,
        })
    }

    pub async fn submit_course_score(
        &self,
        payload: &Value,
        player_id: &str,
        game_token: &str,
    ) -> Result<BmsIrSubmitOutcome> {
        let player_id = parse_player_id(player_id)?;
        let request =
            authenticated_request_value(player_id, game_token, "course_score", payload.clone())?;
        let redacted_request_json = redacted_score_request_json(&request)?;
        let response = self
            .http
            .post(self.endpoint("/api/bmz-player/v1/course-score")?)
            .json(&request)
            .send()
            .await
            .context("failed to send BMS-IR course score request")?;
        let decoded: Value = decode_response(response, "BMS-IR course score submission").await?;
        if decoded.get("accepted").and_then(Value::as_bool) != Some(true) {
            bail!("BMS-IR did not accept the course score");
        }
        Ok(BmsIrSubmitOutcome {
            redacted_request_json,
            response_json: serde_json::to_string(&decoded)?,
        })
    }

    pub async fn fetch_ranking(
        &self,
        chart_sha256: &str,
        request: &IrRankingRequest,
        player_id: &str,
        game_token: &str,
    ) -> Result<IrRankingResult> {
        let body = serde_json::json!({
            "chart_sha256": chart_sha256,
            "scope": request.scope,
            "ln_policy": request.ln_policy,
            "double_option": request.double_option.ir_value(),
            "rule_mode": request.rule_mode.as_str(),
            "limit": request.limit,
            "offset": request.offset,
        });
        self.post_authenticated(
            "/api/bmz-player/v1/ranking",
            player_id,
            game_token,
            body,
            "BMS-IR ranking fetch",
        )
        .await
    }

    pub async fn fetch_course_ranking(
        &self,
        course_hash: &str,
        course_key: &str,
        request: &IrCourseRankingRequest,
        rule_mode: RuleMode,
        player_id: &str,
        game_token: &str,
    ) -> Result<IrCourseRankingResult> {
        let body = serde_json::json!({
            "course_hash": course_hash,
            "course_key": course_key,
            "gauge": request.gauge,
            "ln_policy": request.ln_policy,
            "rule_mode": rule_mode.as_str(),
            "limit": request.limit,
        });
        self.post_authenticated(
            "/api/bmz-player/v1/course-ranking",
            player_id,
            game_token,
            body,
            "BMS-IR course ranking fetch",
        )
        .await
    }

    pub async fn get_rivals(&self, player_id: &str, game_token: &str) -> Result<IrRivalsResponse> {
        let response: BmsIrRivalsResponse = self
            .post_authenticated(
                "/api/bmz-player/v1/rivals",
                player_id,
                game_token,
                Value::Object(Default::default()),
                "BMS-IR rivals fetch",
            )
            .await?;
        ensure_complete_snapshot(
            response.error.as_ref(),
            response.complete,
            response.truncated,
            "BMS-IR rivals",
        )?;
        if response.rivals.iter().any(|rival| rival.player_id.trim().is_empty()) {
            bail!("BMS-IR rivals response contains an empty player ID");
        }
        Ok(IrRivalsResponse { rivals: response.rivals })
    }

    pub async fn fetch_rival_scores(
        &self,
        rival_id: &str,
        rule_mode: RuleMode,
        etag: Option<&str>,
        player_id: &str,
        game_token: &str,
    ) -> Result<RianRivalScoresResponse> {
        let response: BmsIrRivalScoresResponse = self
            .post_authenticated(
                "/api/bmz-player/v1/rival-scores",
                player_id,
                game_token,
                serde_json::json!({
                    "rival_id": rival_id,
                    "rule_mode": rule_mode.as_str(),
                    "etag": etag.unwrap_or_default(),
                }),
                "BMS-IR rival scores fetch",
            )
            .await?;
        ensure_complete_snapshot(
            response.error.as_ref(),
            response.complete,
            response.truncated,
            "BMS-IR rival scores",
        )?;
        if !response.not_modified {
            for score in &response.scores {
                if !is_exact_hex(&score.sha256, 64) {
                    bail!("BMS-IR rival scores response contains an invalid SHA-256");
                }
            }
        }
        Ok(RianRivalScoresResponse {
            scores: response.scores,
            etag: response.etag,
            not_modified: response.not_modified,
        })
    }

    pub async fn fetch_tables(
        &self,
        player_id: &str,
        game_token: &str,
    ) -> Result<Vec<RianTableResource>> {
        let response: BmsIrTablesResponse = self
            .post_authenticated(
                "/api/bmz-player/v1/tables",
                player_id,
                game_token,
                Value::Object(Default::default()),
                "BMS-IR tables fetch",
            )
            .await?;
        ensure_complete_snapshot(
            response.error.as_ref(),
            response.complete,
            response.truncated,
            "BMS-IR tables",
        )?;
        validate_table_resources(&response.data)?;
        Ok(response.data)
    }

    pub async fn fetch_own_scores(
        &self,
        request: &IrOwnScoreHistoryRequest,
        player_id: &str,
        game_token: &str,
    ) -> Result<IrOwnScoreHistoryResult> {
        self.post_authenticated(
            "/api/bmz-player/v1/me/scores",
            player_id,
            game_token,
            serde_json::json!({
                "limit": request.limit,
                "offset": request.offset,
                "cursor_received_at_ms": request
                    .cursor
                    .as_ref()
                    .map(|cursor| cursor.server_received_at_ms),
                "cursor_score_id": request.cursor.as_ref().map(|cursor| &cursor.score_id),
            }),
            "BMS-IR own score history fetch",
        )
        .await
    }

    async fn post_authenticated<T: DeserializeOwned>(
        &self,
        path: &str,
        player_id: &str,
        game_token: &str,
        payload: Value,
        label: &str,
    ) -> Result<T> {
        let request = authenticated_fields_value(parse_player_id(player_id)?, game_token, payload)?;
        let endpoint = self.endpoint(path)?;
        const MAX_ATTEMPTS: usize = 3;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self.http.post(endpoint.clone()).json(&request).send().await;
            let response = match response {
                Ok(response) => response,
                Err(error)
                    if attempt + 1 < MAX_ATTEMPTS && is_retryable_transport_error(&error) =>
                {
                    tokio::time::sleep(fallback_retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => {
                    return Err(error).with_context(|| format!("failed to send {label} request"));
                }
            };
            match decode_response(response, label).await {
                Ok(decoded) => return Ok(decoded),
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    let Some(delay) = retry_delay_for_error(&error, attempt) else {
                        return Err(error);
                    };
                    tokio::time::sleep(delay).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("bounded BMS-IR retry loop always returns")
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url.join(path).context("failed to build BMS-IR endpoint URL")
    }
}

fn parse_http_base_url(value: &str) -> Result<Url> {
    let url = Url::parse(value.trim()).context("invalid BMS-IR base URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        bail!("BMS-IR base URL must use HTTP or HTTPS");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("BMS-IR base URL must not contain credentials");
    }
    Ok(url)
}

fn ensure_complete_snapshot(
    error: Option<&Value>,
    complete: Option<bool>,
    truncated: Option<bool>,
    label: &str,
) -> Result<()> {
    if error.is_some() {
        bail!("{label} response contains a server error");
    }
    if complete == Some(false) || truncated == Some(true) {
        bail!("{label} response is incomplete");
    }
    Ok(())
}

fn is_exact_hex(value: &str, length: usize) -> bool {
    let value = value.trim();
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_table_chart(chart: &RianTableChart) -> Result<()> {
    let md5 = chart.md5.trim();
    let sha256 = chart.sha256.trim();
    if md5.is_empty() && sha256.is_empty() {
        bail!("BMS-IR tables response contains a chart without a hash");
    }
    if (!md5.is_empty() && !is_exact_hex(md5, 32))
        || (!sha256.is_empty() && !is_exact_hex(sha256, 64))
    {
        bail!("BMS-IR tables response contains an invalid chart hash");
    }
    Ok(())
}

fn validate_table_resources(resources: &[RianTableResource]) -> Result<()> {
    for resource in resources {
        if resource.id.trim().is_empty() || resource.attributes.name.trim().is_empty() {
            bail!("BMS-IR tables response contains an invalid table");
        }
        for folder in &resource.attributes.folders {
            if folder.name.trim().is_empty() {
                bail!("BMS-IR tables response contains an unnamed folder");
            }
            for chart in &folder.charts {
                validate_table_chart(chart)?;
            }
        }
        for course in &resource.attributes.courses {
            if course.name.trim().is_empty() {
                bail!("BMS-IR tables response contains an unnamed course");
            }
            for chart in &course.charts {
                validate_table_chart(chart)?;
            }
        }
    }
    Ok(())
}

fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

fn fallback_retry_delay(attempt: usize) -> std::time::Duration {
    std::time::Duration::from_millis(100_u64.saturating_mul(1_u64 << attempt.min(8)))
}

fn retry_delay_for_error(error: &anyhow::Error, attempt: usize) -> Option<std::time::Duration> {
    let status = status_code_from_error(error)?;
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
        || (status.is_client_error() && status != StatusCode::TOO_MANY_REQUESTS)
    {
        return None;
    }
    if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE {
        let seconds = retry_after_seconds_from_error(error)
            .unwrap_or_else(|| fallback_retry_delay(attempt).as_secs().max(1));
        return Some(std::time::Duration::from_secs(seconds.min(300)));
    }
    status.is_server_error().then(|| fallback_retry_delay(attempt))
}

pub fn chart_page_url(base_url: &str, sha256: &str) -> Result<String> {
    public_search_url(base_url, "/new/songs", "keyword", sha256)
}

pub fn course_page_url(base_url: &str, course_hash: &str) -> Result<String> {
    public_search_url(base_url, "/new/courses", "q", course_hash)
}

fn public_search_url(base_url: &str, path: &str, key: &str, value: &str) -> Result<String> {
    let mut url = Url::parse(base_url.trim()).context("invalid BMS-IR public URL")?;
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    url.query_pairs_mut().append_pair(key, value);
    Ok(url.to_string())
}

fn parse_player_id(value: &str) -> Result<u64> {
    let player_id = value.trim().parse::<u64>().context("BMS-IR ID must be a positive integer")?;
    if player_id == 0 {
        bail!("BMS-IR ID must be a positive integer");
    }
    Ok(player_id)
}

fn score_request_value<T: Serialize>(
    player_id: u64,
    game_token: &str,
    score: &T,
    include_ranking: bool,
) -> Result<Value> {
    Ok(serde_json::json!({
        "player_id": player_id,
        "game_token": game_token,
        "include_ranking": include_ranking,
        "score": score,
    }))
}

fn authenticated_request_value(
    player_id: u64,
    game_token: &str,
    payload_key: &str,
    payload: Value,
) -> Result<Value> {
    let mut object = serde_json::Map::new();
    object.insert(payload_key.to_string(), payload);
    authenticated_fields_value(player_id, game_token, Value::Object(object))
}

fn authenticated_fields_value(
    player_id: u64,
    game_token: &str,
    mut payload: Value,
) -> Result<Value> {
    if game_token.trim().is_empty() {
        bail!("BMS-IR game token is empty");
    }
    let object = payload.as_object_mut().context("BMS-IR request body must be an object")?;
    object.insert("player_id".to_string(), Value::from(player_id));
    object.insert("game_token".to_string(), Value::String(game_token.to_string()));
    Ok(payload)
}

fn redacted_score_request_json(request: &Value) -> Result<String> {
    let mut redacted = request.clone();
    if let Some(object) = redacted.as_object_mut() {
        object.insert("game_token".to_string(), Value::String("<redacted>".to_string()));
    }
    Ok(serde_json::to_string(&redacted)?)
}

async fn decode_response<T: DeserializeOwned>(
    response: reqwest::Response,
    context: &str,
) -> Result<T> {
    let status = response.status();
    let retry_after = retry_after_header(&response);
    let body =
        response.bytes().await.with_context(|| format!("failed to read {context} response"))?;
    if !status.is_success() {
        return Err(http_response_error(
            context,
            status,
            &String::from_utf8_lossy(&body),
            retry_after.as_deref(),
        ));
    }
    serde_json::from_slice(&body).with_context(|| format!("failed to decode {context} response"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    use super::*;
    use crate::ir::types::{
        IrChartFeatures, IrChartLnProfile, IrChartNotes, IrChartPayload, IrClientInfo,
        IrEffectiveLnMode, IrJudgePayload, IrJudgeSidePayload, IrResultPayload, IrRulePayload,
    };

    struct MockResponse<'a> {
        status: &'a str,
        headers: &'a [(&'a str, &'a str)],
        body: &'a str,
    }

    fn spawn_mock_server(
        responses: Vec<MockResponse<'static>>,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                sender.send(request).unwrap();
                let extra_headers = response
                    .headers
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}\r\n"))
                    .collect::<String>();
                write!(
                    stream,
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
                    response.status,
                    response.body.len(),
                    extra_headers,
                    response.body
                )
                .unwrap();
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn spawn_timeout_server(
        attempts: usize,
        delay: std::time::Duration,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..attempts {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                let _ = sender.send(request);
                std::thread::sleep(delay);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{{\"rivals\":[]}}"
                );
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_length {
                break;
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    fn request_json(request: &str) -> Value {
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
    }

    fn ok(body: &'static str) -> MockResponse<'static> {
        MockResponse { status: "200 OK", headers: &[], body }
    }

    fn sample_score_payload() -> IrScoreSubmission {
        IrScoreSubmission {
            client: IrClientInfo {
                name: "BMZ".to_string(),
                version: "0.3.0".to_string(),
                platform: "test".to_string(),
            },
            chart: IrChartPayload {
                source_format: "bms".to_string(),
                sha256: "ab".repeat(32),
                md5: Some("cd".repeat(16)),
                length_ms: Some(120_000),
                ln_profile: IrChartLnProfile::default(),
                title: "Test chart".to_string(),
                subtitle: String::new(),
                genre: String::new(),
                artist: "Test artist".to_string(),
                subartists: Vec::new(),
                mode: "7K".to_string(),
                level: Some(12),
                difficulty: "Another".to_string(),
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
                key_mode: "7K".to_string(),
                gauge: "Hard".to_string(),
                ln_policy: LnScorePolicy::ForceLn,
                effective_ln_mode: IrEffectiveLnMode::Ln,
                judge_algorithm: "bmz_v1".to_string(),
                scoring: "bms_ex_score_v1".to_string(),
                rule_mode: "Beatoraja".to_string(),
            },
            result: IrResultPayload {
                clear: "Hard".to_string(),
                played_at: 1_700_000_000,
                duration_ms: Some(100_000),
                judges: IrJudgePayload {
                    fast: IrJudgeSidePayload {
                        pgreat: 40,
                        great: 5,
                        good: 1,
                        bad: 1,
                        poor: 2,
                        empty_poor: 1,
                    },
                    slow: IrJudgeSidePayload {
                        pgreat: 40,
                        great: 5,
                        good: 1,
                        bad: 1,
                        poor: 2,
                        empty_poor: 1,
                    },
                },
                ex_score: 170,
                max_combo: 90,
                notes: 100,
                pass_notes: None,
                min_bp: 8,
                min_cb: 2,
                ghost: None,
            },
            play_options: BTreeMap::from([(
                "applied_double_option".to_string(),
                serde_json::json!("off"),
            )]),
            replay: None,
            evidence: BTreeMap::new(),
            idempotency_key: "score-contract-test".to_string(),
        }
    }

    #[test]
    fn provider_aliases_and_queue_eligibility_are_narrow() {
        assert!(is_bms_ir_provider("bms-ir"));
        assert!(is_bms_ir_provider("BMSIR"));
        assert!(!is_bms_ir_provider("bmz"));

        let profile = ChartLnProfile { has_undefined_ln: true, ..Default::default() };
        assert!(score_submission_supported(
            RuleMode::Beatoraja,
            ChartSourceFormat::Bms,
            profile,
            LnScorePolicy::AutoLn,
            DoubleOption::Off,
        ));
        assert!(score_submission_supported(
            RuleMode::Lr2Oraja,
            ChartSourceFormat::Bms,
            profile,
            LnScorePolicy::AutoLn,
            DoubleOption::Off,
        ));
        assert!(score_submission_supported(
            RuleMode::Beatoraja,
            ChartSourceFormat::Bmson,
            profile,
            LnScorePolicy::ForceCn,
            DoubleOption::Off,
        ));
        assert!(score_submission_supported(
            RuleMode::Beatoraja,
            ChartSourceFormat::Pms,
            profile,
            LnScorePolicy::AutoLn,
            DoubleOption::Battle,
        ));
    }

    #[test]
    fn course_stage_payload_accepts_only_rounded_clear_types() {
        let mut payload = sample_score_payload();
        payload.play_options.insert("course_stage".to_string(), serde_json::json!(true));
        payload.result.clear = "NoPlay".to_string();
        assert!(ensure_score_payload_supported(&payload).is_ok());

        payload.result.clear = "Normal".to_string();
        assert!(ensure_score_payload_supported(&payload).is_err());
    }

    #[test]
    fn canonical_bms_ir_key_modes_include_4k_6k_and_8k() {
        for mode in BMS_IR_KEY_MODES {
            assert!(is_supported_key_mode(mode), "{mode}");
        }
        assert!(!is_supported_key_mode("3K"));
    }

    #[test]
    fn request_log_redacts_game_token_without_changing_score() {
        let request = score_request_value(
            123,
            "secret-game-token",
            &serde_json::json!({"idempotency_key": "score-1"}),
            false,
        )
        .unwrap();
        let redacted = redacted_score_request_json(&request).unwrap();
        assert!(!redacted.contains("secret-game-token"));
        let decoded: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(decoded["game_token"], "<redacted>");
        assert_eq!(decoded["include_ranking"], false);
        assert_eq!(decoded["score"]["idempotency_key"], "score-1");
    }

    #[test]
    fn fixed_base_url_rejects_other_origins_and_accepts_its_own_trailing_slash() {
        assert!(fixed_base_url(Some("https://attacker.example/collect")).is_err());
        let configured = Url::parse(BMS_IR_DEFAULT_BASE_URL).unwrap();
        let same_origin = format!("{}/", configured.origin().ascii_serialization());
        assert_eq!(fixed_base_url(Some(&same_origin)).unwrap(), BMS_IR_DEFAULT_BASE_URL);
    }

    #[tokio::test]
    async fn login_http_contract_sends_player_id_and_game_token() {
        let (base_url, requests) = spawn_mock_server(vec![ok(r#"{"ok":true,"player_id":123}"#)]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();

        let credentials = client.login("123", "secret-token").await.unwrap();

        assert_eq!(credentials.player.id, "123");
        let request = requests.recv().unwrap();
        assert!(request.starts_with("POST /api/bmz-player/v1/login HTTP/1.1"));
        let json = request_json(&request);
        assert_eq!(json["player_id"], 123);
        assert_eq!(json["game_token"], "secret-token");
    }

    #[tokio::test]
    async fn authenticated_endpoint_contracts_include_credentials() {
        let paths = [
            "/api/bmz-player/v1/score",
            "/api/bmz-player/v1/course-score",
            "/api/bmz-player/v1/ranking",
            "/api/bmz-player/v1/course-ranking",
            "/api/bmz-player/v1/rivals",
            "/api/bmz-player/v1/rival-scores",
            "/api/bmz-player/v1/tables",
            "/api/bmz-player/v1/me/scores",
        ];
        let (base_url, requests) = spawn_mock_server(paths.iter().map(|_| ok("{}")).collect());
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();

        for path in paths {
            let response: Value = client
                .post_authenticated(
                    path,
                    "123",
                    "secret-token",
                    serde_json::json!({"probe": path}),
                    "contract probe",
                )
                .await
                .unwrap();
            assert_eq!(response, serde_json::json!({}));
            let request = requests.recv().unwrap();
            assert!(request.starts_with(&format!("POST {path} HTTP/1.1")));
            let json = request_json(&request);
            assert_eq!(json["player_id"], 123);
            assert_eq!(json["game_token"], "secret-token");
            assert_eq!(json["probe"], path);
        }
    }

    #[tokio::test]
    async fn public_api_http_contracts_round_trip_all_bms_ir_resources() {
        let chart_hash: &'static str = Box::leak("ab".repeat(32).into_boxed_str());
        let course_hash: &'static str = Box::leak("cd".repeat(32).into_boxed_str());
        let ranking: &'static str = Box::leak(
            format!(
                r#"{{"chart":{{"sha256":"{chart_hash}"}},"ranking":{{"scope":"global","entries":[]}}}}"#
            )
            .into_boxed_str(),
        );
        let course_ranking: &'static str = Box::leak(
            format!(
                r#"{{"course":{{"course_hash":"{course_hash}"}},"ranking":{{"scope":"global","entries":[]}}}}"#
            )
            .into_boxed_str(),
        );
        let valid_rival_scores: &'static str = Box::leak(
            format!(
                r#"{{"scores":[{{"sha256":"{chart_hash}","ln_mode":0,"ex_score":1,"clear_type":1,"max_combo":1,"min_bp":0,"play_option":0,"arrange_1p":"OFF","arrange_2p":"OFF","double_option":"OFF","play_seed":null}}],"etag":"etag-2","not_modified":false}}"#
            )
            .into_boxed_str(),
        );
        let (base_url, requests) = spawn_mock_server(vec![
            ok(r#"{"accepted":true,"score_id":"score-1","best_updated":true}"#),
            ok(r#"{"accepted":true,"course_score_id":"course-score-1"}"#),
            ok(ranking),
            ok(course_ranking),
            ok(r#"{"rivals":[]}"#),
            ok(valid_rival_scores),
            ok(r#"{"scores":[],"etag":"etag-2","not_modified":true}"#),
            ok(r#"{"data":[]}"#),
            ok(r#"{"scores":[],"pagination":{"limit":20,"offset":0,"total":0,"has_more":false}}"#),
        ]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();

        client.submit_score(&sample_score_payload(), "123", "token", true).await.unwrap();
        let score_request = requests.recv().unwrap();
        assert!(score_request.starts_with("POST /api/bmz-player/v1/score HTTP/1.1"));
        assert_eq!(request_json(&score_request)["score"]["idempotency_key"], "score-contract-test");
        assert_eq!(request_json(&score_request)["include_ranking"], true);

        client
            .submit_course_score(&serde_json::json!({"course_hash": course_hash}), "123", "token")
            .await
            .unwrap();
        assert!(
            requests.recv().unwrap().starts_with("POST /api/bmz-player/v1/course-score HTTP/1.1")
        );

        let ranking_request = IrRankingRequest {
            scope: super::super::types::IrRankingScope::Global,
            ln_policy: "ForceLn".to_string(),
            double_option: Default::default(),
            rule_mode: RuleMode::Beatoraja,
            limit: 20,
            offset: 0,
        };
        assert_eq!(
            client
                .fetch_ranking(chart_hash, &ranking_request, "123", "token")
                .await
                .unwrap()
                .chart
                .sha256,
            chart_hash
        );
        assert!(requests.recv().unwrap().starts_with("POST /api/bmz-player/v1/ranking HTTP/1.1"));

        let course_request = IrCourseRankingRequest {
            gauge: "Class".to_string(),
            ln_policy: "ForceLn".to_string(),
            limit: 20,
        };
        assert_eq!(
            client
                .fetch_course_ranking(
                    course_hash,
                    "efefefefefefefefefefefefefefefefcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                    &course_request,
                    RuleMode::Beatoraja,
                    "123",
                    "token",
                )
                .await
                .unwrap()
                .course
                .course_hash,
            course_hash
        );
        let course_ranking_request = requests.recv().unwrap();
        assert!(
            course_ranking_request.starts_with("POST /api/bmz-player/v1/course-ranking HTTP/1.1")
        );
        assert_eq!(
            request_json(&course_ranking_request)["course_key"],
            "efefefefefefefefefefefefefefefefcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd"
        );

        assert!(client.get_rivals("123", "token").await.unwrap().rivals.is_empty());
        assert!(requests.recv().unwrap().starts_with("POST /api/bmz-player/v1/rivals HTTP/1.1"));

        let scores = client
            .fetch_rival_scores("456", RuleMode::Beatoraja, Some("etag-1"), "123", "token")
            .await
            .unwrap();
        assert_eq!(scores.etag, "etag-2");
        assert_eq!(scores.scores.len(), 1);
        assert!(
            requests.recv().unwrap().starts_with("POST /api/bmz-player/v1/rival-scores HTTP/1.1")
        );

        let not_modified = client
            .fetch_rival_scores("456", RuleMode::Beatoraja, Some("etag-2"), "123", "token")
            .await
            .unwrap();
        assert!(not_modified.not_modified);
        let _ = requests.recv().unwrap();

        assert!(client.fetch_tables("123", "token").await.unwrap().is_empty());
        assert!(requests.recv().unwrap().starts_with("POST /api/bmz-player/v1/tables HTTP/1.1"));

        let history = client
            .fetch_own_scores(
                &IrOwnScoreHistoryRequest { limit: 20, offset: 0, cursor: None },
                "123",
                "token",
            )
            .await
            .unwrap();
        assert!(history.scores.is_empty());
        assert!(requests.recv().unwrap().starts_with("POST /api/bmz-player/v1/me/scores HTTP/1.1"));
    }

    #[tokio::test]
    async fn missing_snapshot_fields_are_errors_but_explicit_empty_snapshots_are_valid() {
        let (base_url, _requests) = spawn_mock_server(vec![ok("{}")]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.get_rivals("123", "token").await.is_err());

        let (base_url, _requests) = spawn_mock_server(vec![ok(r#"{"error":"upstream failed"}"#)]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.fetch_tables("123", "token").await.is_err());

        let invalid_table = r#"{
            "data":[{"id":"table-1","attributes":{"name":"Table","folders":[{
                "name":"1","charts":[{"sha256":"bad-hash"}]
            }]}}]
        }"#;
        let (base_url, _requests) = spawn_mock_server(vec![ok(invalid_table)]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.fetch_tables("123", "token").await.is_err());

        let (base_url, _requests) =
            spawn_mock_server(vec![ok(r#"{"data":[],"error":"partial failure"}"#)]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.fetch_tables("123", "token").await.is_err());

        let (base_url, _requests) =
            spawn_mock_server(vec![ok(r#"{"rivals":[]}"#), ok(r#"{"data":[]}"#)]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.get_rivals("123", "token").await.unwrap().rivals.is_empty());
        assert!(client.fetch_tables("123", "token").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn truncated_or_incomplete_snapshots_are_rejected() {
        let (base_url, _requests) = spawn_mock_server(vec![
            ok(r#"{"rivals":[],"truncated":true}"#),
            ok(r#"{"data":[],"complete":false}"#),
        ]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();

        assert!(client.get_rivals("123", "token").await.is_err());
        assert!(client.fetch_tables("123", "token").await.is_err());
    }

    #[tokio::test]
    async fn server_side_score_rejection_is_an_error() {
        let (base_url, _requests) = spawn_mock_server(vec![ok(
            r#"{"accepted":false,"score_id":null,"best_updated":false}"#,
        )]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();

        assert!(client.submit_score(&sample_score_payload(), "123", "token", true).await.is_err());
    }

    #[tokio::test]
    async fn invalid_rival_score_hash_rejects_the_whole_snapshot() {
        let body = r#"{
            "scores":[{
                "sha256":"bad-hash","ln_mode":0,"ex_score":1,"clear_type":1,
                "max_combo":1,"min_bp":0,"play_option":0,"arrange_1p":"OFF",
                "arrange_2p":"OFF","double_option":"OFF","play_seed":null
            }],
            "etag":"new-etag","not_modified":false
        }"#;
        let (base_url, _requests) = spawn_mock_server(vec![ok(body)]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();

        let error = client
            .fetch_rival_scores("456", RuleMode::Beatoraja, Some("old-etag"), "123", "token")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("invalid SHA-256"));
    }

    #[tokio::test]
    async fn retry_after_is_honored_with_bounded_retries_and_auth_errors_do_not_retry() {
        let rate_limited = MockResponse {
            status: "429 Too Many Requests",
            headers: &[("Retry-After", "0")],
            body: r#"{"message":"slow down"}"#,
        };
        let (base_url, requests) = spawn_mock_server(vec![rate_limited, ok(r#"{"rivals":[]}"#)]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.get_rivals("123", "token").await.is_ok());
        assert_eq!(requests.try_iter().count(), 2);

        let unauthorized = MockResponse {
            status: "401 Unauthorized",
            headers: &[],
            body: r#"{"message":"invalid token"}"#,
        };
        let (base_url, requests) = spawn_mock_server(vec![unauthorized]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.get_rivals("123", "token").await.is_err());
        assert_eq!(requests.try_iter().count(), 1);

        let unavailable = || MockResponse {
            status: "503 Service Unavailable",
            headers: &[("Retry-After", "0")],
            body: r#"{"message":"busy"}"#,
        };
        let (base_url, requests) =
            spawn_mock_server(vec![unavailable(), unavailable(), unavailable()]);
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_secs(2)).unwrap();
        assert!(client.get_rivals("123", "token").await.is_err());
        assert_eq!(requests.try_iter().count(), 3);
    }

    #[tokio::test]
    async fn transport_timeouts_retry_only_to_the_attempt_limit() {
        let (base_url, requests) = spawn_timeout_server(3, std::time::Duration::from_millis(60));
        let client =
            BmsIrClient::new_with_timeout(&base_url, std::time::Duration::from_millis(20)).unwrap();

        assert!(client.get_rivals("123", "token").await.is_err());
        assert_eq!(requests.try_iter().count(), 3);
    }

    #[test]
    fn invalid_retry_after_falls_back_and_large_values_are_capped() {
        let rate_limited = http_response_error(
            "BMS-IR request",
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"message":"busy"}"#,
            Some("30"),
        );
        assert_eq!(
            retry_delay_for_error(&rate_limited, 0),
            Some(std::time::Duration::from_secs(30))
        );

        let unavailable = http_response_error(
            "BMS-IR request",
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"message":"busy"}"#,
            Some("10"),
        );
        assert_eq!(
            retry_delay_for_error(&unavailable, 0),
            Some(std::time::Duration::from_secs(10))
        );

        let missing = http_response_error(
            "BMS-IR request",
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"message":"busy"}"#,
            None,
        );
        assert_eq!(retry_delay_for_error(&missing, 0), Some(std::time::Duration::from_secs(1)));

        let malformed = http_response_error(
            "BMS-IR request",
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"message":"busy"}"#,
            Some("later"),
        );
        assert_eq!(retry_delay_for_error(&malformed, 0), Some(std::time::Duration::from_secs(1)));

        let excessive = http_response_error(
            "BMS-IR request",
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"message":"busy"}"#,
            Some("9999"),
        );
        assert_eq!(retry_delay_for_error(&excessive, 0), Some(std::time::Duration::from_secs(300)));
    }
}
