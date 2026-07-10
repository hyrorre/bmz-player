use std::fmt;

use anyhow::{Context, Result, bail};
use bmz_gameplay::rule::RuleMode;
use reqwest::{Url, header};

use crate::select_options::DoubleOptionScoreBucket;

use super::types::{
    IrAuthTokens, IrCourseRankingResult, IrDeviceKeysResponse, IrMeResponse,
    IrOwnScoreHistoryResult, IrRankingResult, IrRankingScope, IrReplayDownloadTarget,
    IrReplayUploadTarget, IrReplayVerifyResult, IrRivalsResponse, IrScoreSubmission,
    IrSubmitOptions, IrSubmitResponse,
};

#[derive(Debug, Clone)]
pub struct BmzOfficialIrClient {
    base_url: Url,
    access_token: Option<String>,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct IrRankingRequest {
    pub scope: IrRankingScope,
    pub ln_policy: String,
    pub double_option: DoubleOptionScoreBucket,
    pub rule_mode: RuleMode,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct IrCourseRankingRequest {
    pub gauge: String,
    pub ln_policy: String,
    pub limit: u32,
}

#[derive(Debug)]
struct IrHttpResponseError {
    summary: String,
    retry_after_seconds: Option<u64>,
}

impl fmt::Display for IrHttpResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.summary)
    }
}

impl std::error::Error for IrHttpResponseError {}

pub(crate) fn retry_after_seconds_from_error(error: &anyhow::Error) -> Option<u64> {
    error.chain().find_map(|cause| {
        cause.downcast_ref::<IrHttpResponseError>().and_then(|error| error.retry_after_seconds)
    })
}

#[derive(Debug, Clone)]
pub struct IrOwnScoreHistoryRequest {
    pub limit: u32,
    pub offset: u32,
}

impl BmzOfficialIrClient {
    pub fn new(base_url: &str, access_token: impl Into<String>) -> Result<Self> {
        Ok(Self {
            base_url: parse_base_url(base_url)?,
            access_token: Some(access_token.into()),
            http: reqwest::Client::new(),
        })
    }

    /// ログイン前 (token なし) のクライアント。`login` / `refresh` / 匿名 ranking 用。
    pub fn anonymous(base_url: &str) -> Result<Self> {
        Ok(Self {
            base_url: parse_base_url(base_url)?,
            access_token: None,
            http: reqwest::Client::new(),
        })
    }

    pub fn set_access_token(&mut self, access_token: impl Into<String>) {
        self.access_token = Some(access_token.into());
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<IrAuthTokens> {
        let url = self.base_url.join("/api/v1/auth/login")?;
        let response = self
            .http
            .post(url)
            .json(&serde_json::json!({
                "email": email,
                "password": password,
                "client_type": "desktop",
            }))
            .send()
            .await
            .context("failed to send BMZ IR login request")?;
        decode_response(response, "BMZ IR login").await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<IrAuthTokens> {
        let url = self.base_url.join("/api/v1/auth/refresh")?;
        let response = self
            .http
            .post(url)
            .json(&serde_json::json!({
                "refresh_token": refresh_token,
                "client_type": "desktop",
            }))
            .send()
            .await
            .context("failed to send BMZ IR refresh request")?;
        decode_response(response, "BMZ IR token refresh").await
    }

    pub async fn logout(&self, refresh_token: &str) -> Result<()> {
        let url = self.base_url.join("/api/v1/auth/logout")?;
        let response = self
            .http
            .post(url)
            .bearer_auth(self.require_token()?)
            .json(&serde_json::json!({ "refresh_token": refresh_token }))
            .send()
            .await
            .context("failed to send BMZ IR logout request")?;
        let _: serde_json::Value = decode_response(response, "BMZ IR logout").await?;
        Ok(())
    }

    pub async fn me(&self) -> Result<IrMeResponse> {
        let url = self.base_url.join("/api/v1/me")?;
        let response = self
            .http
            .get(url)
            .bearer_auth(self.require_token()?)
            .send()
            .await
            .context("failed to send BMZ IR me request")?;
        decode_response(response, "BMZ IR me").await
    }

    /// score の replay アップロード用署名 URL を取得する。
    pub async fn replay_upload_url(&self, score_id: &str) -> Result<IrReplayUploadTarget> {
        let url = self.base_url.join(&format!("/api/v1/scores/{score_id}/replay/upload-url"))?;
        let response = self
            .http
            .post(url)
            .bearer_auth(self.require_token()?)
            .send()
            .await
            .context("failed to request BMZ IR replay upload URL")?;
        decode_response(response, "BMZ IR replay upload URL").await
    }

    /// upload-url で得た endpoint へ replay 本体を PUT する。
    /// server 側は認証 + 所有者チェックを行うため Bearer トークンを付ける。
    pub async fn upload_replay(&self, upload_url: &str, bytes: Vec<u8>) -> Result<()> {
        let response = self
            .http
            .put(upload_url)
            .bearer_auth(self.require_token()?)
            .header("content-type", "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .context("failed to upload BMZ IR replay")?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = retry_after_header(&response);
            let body = response.text().await.unwrap_or_default();
            return Err(http_response_error(
                "BMZ IR replay upload",
                status,
                &body,
                retry_after.as_deref(),
            ));
        }
        Ok(())
    }

    /// 公開リプレイをダウンロードする。戻り値は (bytes, 申告 hash)。
    pub async fn download_replay(&self, score_id: &str) -> Result<(Vec<u8>, String)> {
        let url = self.base_url.join(&format!("/api/v1/scores/{score_id}/replay"))?;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to request BMZ IR replay download URL")?;
        let target: IrReplayDownloadTarget =
            decode_response(response, "BMZ IR replay download URL").await?;
        let body = self
            .http
            .get(&target.download_url)
            .send()
            .await
            .context("failed to download BMZ IR replay")?;
        let status = body.status();
        if !status.is_success() {
            bail!("BMZ IR replay download failed: {status}");
        }
        let bytes = body.bytes().await.context("failed to read BMZ IR replay body")?.to_vec();
        Ok((bytes, target.hash.unwrap_or_default()))
    }

    /// アップロード済み replay の hash 検証をサーバーへ依頼する。
    pub async fn verify_replay(&self, score_id: &str) -> Result<IrReplayVerifyResult> {
        let url = self.base_url.join(&format!("/api/v1/scores/{score_id}/replay/verify"))?;
        let response = self
            .http
            .post(url)
            .bearer_auth(self.require_token()?)
            .send()
            .await
            .context("failed to request BMZ IR replay verification")?;
        decode_response(response, "BMZ IR replay verification").await
    }

    /// 自分の device key を失効させる。
    pub async fn revoke_device_key(&self, key_id: &str) -> Result<()> {
        let url = self.base_url.join(&format!("/api/v1/device-keys/{key_id}"))?;
        let response = self
            .http
            .delete(url)
            .bearer_auth(self.require_token()?)
            .send()
            .await
            .context("failed to send BMZ IR device key revocation")?;
        let _: serde_json::Value =
            decode_response(response, "BMZ IR device key revocation").await?;
        Ok(())
    }

    /// 自分の device key 一覧を取得する。
    pub async fn list_device_keys(&self) -> Result<IrDeviceKeysResponse> {
        let url = self.base_url.join("/api/v1/device-keys")?;
        let response = self
            .http
            .get(url)
            .bearer_auth(self.require_token()?)
            .send()
            .await
            .context("failed to send BMZ IR device key list request")?;
        decode_response(response, "BMZ IR device key list").await
    }

    pub async fn fetch_ranking(
        &self,
        chart_sha256_hex: &str,
        request: &IrRankingRequest,
    ) -> Result<IrRankingResult> {
        let url = self.chart_ranking_url(chart_sha256_hex, request)?;
        let mut builder = self.http.get(url);
        if let Some(token) = &self.access_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await.context("failed to send BMZ IR ranking request")?;
        decode_response(response, "BMZ IR ranking fetch").await
    }

    pub async fn fetch_course_ranking(
        &self,
        course_hash: &str,
        request: &IrCourseRankingRequest,
    ) -> Result<IrCourseRankingResult> {
        let url = self.course_ranking_url(course_hash, request)?;
        let mut builder = self.http.get(url);
        if let Some(token) = &self.access_token {
            builder = builder.bearer_auth(token);
        }
        let response =
            builder.send().await.context("failed to send BMZ IR course ranking request")?;
        decode_response(response, "BMZ IR course ranking fetch").await
    }

    pub async fn fetch_own_scores(
        &self,
        request: &IrOwnScoreHistoryRequest,
    ) -> Result<IrOwnScoreHistoryResult> {
        let mut url = self.base_url.join("/api/v1/me/scores")?;
        url.query_pairs_mut()
            .append_pair("limit", &request.limit.to_string())
            .append_pair("offset", &request.offset.to_string());
        let response = self
            .http
            .get(url)
            .bearer_auth(self.require_token()?)
            .send()
            .await
            .context("failed to send BMZ IR own score history request")?;
        decode_response(response, "BMZ IR own score history fetch").await
    }

    /// Ed25519 公開鍵をサーバーへ登録し、device key id を返す。
    pub async fn register_device_key(&self, public_key_hex: &str) -> Result<String> {
        let url = self.base_url.join("/api/v1/device-keys")?;
        let response = self
            .http
            .post(url)
            .bearer_auth(self.require_token()?)
            .json(&serde_json::json!({ "public_key": public_key_hex, "algorithm": "ed25519" }))
            .send()
            .await
            .context("failed to send BMZ IR device key registration")?;
        let value: serde_json::Value =
            decode_response(response, "BMZ IR device key registration").await?;
        value
            .get("id")
            .and_then(|id| id.as_str())
            .map(str::to_string)
            .context("device key registration response missing id")
    }

    pub async fn get_rivals(&self) -> Result<IrRivalsResponse> {
        let url = self.base_url.join("/api/v1/rivals")?;
        let response = self
            .http
            .get(url)
            .bearer_auth(self.require_token()?)
            .send()
            .await
            .context("failed to send BMZ IR rivals request")?;
        decode_response(response, "BMZ IR rivals fetch").await
    }

    pub async fn set_rival(&self, target_player_id: &str, add: bool) -> Result<()> {
        let url = self.base_url.join("/api/v1/rivals")?;
        let action = if add { "add" } else { "remove" };
        let response = self
            .http
            .post(url)
            .bearer_auth(self.require_token()?)
            .json(&serde_json::json!({ "target_player_id": target_player_id, "action": action }))
            .send()
            .await
            .context("failed to send BMZ IR rival update")?;
        let _: serde_json::Value = decode_response(response, "BMZ IR rival update").await?;
        Ok(())
    }

    /// コーススコアを送信する (`POST /api/v1/course-scores`)。
    pub async fn submit_course_score(
        &self,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = self.base_url.join("/api/v1/course-scores")?;
        let response = self
            .http
            .post(url)
            .bearer_auth(self.require_token()?)
            .json(payload)
            .send()
            .await
            .context("failed to send BMZ IR course score submission")?;
        decode_response(response, "BMZ IR course score submission").await
    }

    pub async fn submit_score(
        &self,
        request: &IrScoreSubmission,
        options: &IrSubmitOptions,
    ) -> Result<IrSubmitResponse> {
        let url = self.score_submit_url(options)?;
        let response = self
            .http
            .post(url)
            .bearer_auth(self.require_token()?)
            .json(request)
            .send()
            .await
            .context("failed to send BMZ IR score submission")?;
        decode_response(response, "BMZ IR score submission").await
    }

    fn require_token(&self) -> Result<&str> {
        self.access_token.as_deref().context("BMZ IR access token is not set; login first")
    }

    fn score_submit_url(&self, options: &IrSubmitOptions) -> Result<Url> {
        let mut url = self.base_url.join("/api/v1/scores")?;
        if !options.ranking_scopes.is_empty() {
            let scopes =
                options.ranking_scopes.iter().map(scope_query_value).collect::<Vec<_>>().join(",");
            url.query_pairs_mut()
                .append_pair("include", "rankings")
                .append_pair("ranking_scopes", &scopes)
                .append_pair("ranking_limit", &options.ranking_limit.to_string());
        }
        Ok(url)
    }

    fn chart_ranking_url(&self, chart_sha256_hex: &str, request: &IrRankingRequest) -> Result<Url> {
        let mut url = self.base_url.join(&format!("/api/v1/charts/{chart_sha256_hex}/ranking"))?;
        url.query_pairs_mut()
            .append_pair("scope", scope_query_value(&request.scope))
            .append_pair("ln_policy", &request.ln_policy)
            .append_pair("rule_mode", request.rule_mode.as_str())
            .append_pair("limit", &request.limit.to_string())
            .append_pair("offset", &request.offset.to_string());
        if let Some(double_option) = request.double_option.ir_query_value() {
            url.query_pairs_mut().append_pair("double_option", double_option);
        }
        Ok(url)
    }

    fn course_ranking_url(
        &self,
        course_hash: &str,
        request: &IrCourseRankingRequest,
    ) -> Result<Url> {
        let mut url = self.base_url.join(&format!("/api/v1/courses/{course_hash}/ranking"))?;
        url.query_pairs_mut()
            .append_pair("gauge", &request.gauge)
            .append_pair("ln_policy", &request.ln_policy)
            .append_pair("limit", &request.limit.to_string());
        Ok(url)
    }
}

fn parse_base_url(base_url: &str) -> Result<Url> {
    Url::parse(base_url).context("invalid BMZ IR base URL")
}

async fn decode_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    label: &str,
) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        let retry_after = retry_after_header(&response);
        let body = response.text().await.unwrap_or_default();
        return Err(http_response_error(label, status, &body, retry_after.as_deref()));
    }
    response.json().await.with_context(|| format!("failed to decode {label} response"))
}

fn http_response_error(
    label: &str,
    status: reqwest::StatusCode,
    body: &str,
    retry_after: Option<&str>,
) -> anyhow::Error {
    anyhow::Error::new(IrHttpResponseError {
        summary: format!("{label} failed: {}", response_error_summary(status, body, retry_after)),
        retry_after_seconds: retry_after.and_then(parse_retry_after_seconds),
    })
}

fn parse_retry_after_seconds(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

/// エラー本文はトークン等の秘匿情報を含み得るため、そのままログへ流さない。
/// サーバー制御の短い statusMessage / message だけを抜き出し、それ以外の
/// 本文は捨てる (h3 の createError は JSON でこれらのキーを返す)。
fn response_error_summary(
    status: reqwest::StatusCode,
    body: &str,
    retry_after: Option<&str>,
) -> String {
    const MAX_MESSAGE_CHARS: usize = 200;

    #[derive(serde::Deserialize)]
    struct ErrorBody {
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        message: Option<String>,
    }

    let message = serde_json::from_str::<ErrorBody>(body)
        .ok()
        .and_then(|body| body.status_message.or(body.message))
        .map(|message| message.trim().to_string())
        .filter(|message| !message.is_empty());
    let mut summary = match message {
        Some(message) => {
            let truncated: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
            format!("{status} {truncated}")
        }
        None => format!("{status} (response body omitted, {} bytes)", body.len()),
    };
    if let Some(retry_after) = retry_after.filter(|value| !value.trim().is_empty()) {
        summary.push_str(" (retry after ");
        summary.push_str(retry_after.trim());
        summary.push('s');
        summary.push(')');
    }
    summary
}

fn retry_after_header(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn scope_query_value(scope: &IrRankingScope) -> &'static str {
    match scope {
        IrRankingScope::Global => "global",
        IrRankingScope::SelfAndRivals => "self_and_rivals",
        IrRankingScope::Rivals => "rivals",
        IrRankingScope::SelfOnly => "self",
        IrRankingScope::AroundSelf => "around_self",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_url_includes_optional_ranking_scopes() {
        let client = BmzOfficialIrClient::new("https://ir.example.test", "token").unwrap();
        let url = client
            .score_submit_url(&IrSubmitOptions {
                ranking_scopes: vec![IrRankingScope::Global, IrRankingScope::SelfAndRivals],
                ranking_limit: 50,
            })
            .unwrap();

        assert_eq!(
            url.as_str(),
            "https://ir.example.test/api/v1/scores?include=rankings&ranking_scopes=global%2Cself_and_rivals&ranking_limit=50"
        );
    }

    #[test]
    fn course_ranking_url_includes_rule_filters() {
        let client = BmzOfficialIrClient::anonymous("https://ir.example.test").unwrap();
        let url = client
            .course_ranking_url(
                &"ab".repeat(32),
                &IrCourseRankingRequest {
                    gauge: "Class".to_string(),
                    ln_policy: "AutoLn".to_string(),
                    limit: 20,
                },
            )
            .unwrap();

        assert_eq!(
            url.as_str(),
            format!(
                "https://ir.example.test/api/v1/courses/{}/ranking?gauge=Class&ln_policy=AutoLn&limit=20",
                "ab".repeat(32)
            )
        );
    }

    #[test]
    fn anonymous_client_rejects_authenticated_calls() {
        let client = BmzOfficialIrClient::anonymous("https://ir.example.test").unwrap();
        assert!(client.require_token().is_err());
    }

    #[test]
    fn error_summary_extracts_server_status_message() {
        let summary = response_error_summary(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"statusCode":429,"statusMessage":"Too many requests","stack":[]}"#,
            None,
        );
        assert_eq!(summary, "429 Too Many Requests Too many requests");
    }

    #[test]
    fn error_summary_includes_retry_after_when_present() {
        let summary = response_error_summary(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"statusCode":429,"statusMessage":"Too many requests","stack":[]}"#,
            Some("42"),
        );
        assert_eq!(summary, "429 Too Many Requests Too many requests (retry after 42s)");
    }

    #[test]
    fn http_error_exposes_retry_after_seconds_to_sync() {
        let error = http_response_error(
            "BMZ IR score submission",
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"statusMessage":"Too many requests"}"#,
            Some("123"),
        );

        assert_eq!(retry_after_seconds_from_error(&error), Some(123));
        assert!(error.to_string().contains("retry after 123s"));
    }

    #[test]
    fn error_summary_falls_back_to_message_field() {
        let summary = response_error_summary(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"message":"rule.ln_policy is invalid"}"#,
            None,
        );
        assert_eq!(summary, "400 Bad Request rule.ln_policy is invalid");
    }

    #[test]
    fn error_summary_omits_non_json_body() {
        let summary = response_error_summary(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "secret token leaked in html error page",
            None,
        );
        assert_eq!(summary, "500 Internal Server Error (response body omitted, 38 bytes)");
        assert!(!summary.contains("secret"));
    }

    #[test]
    fn error_summary_truncates_long_messages() {
        let long = "a".repeat(500);
        let summary = response_error_summary(
            reqwest::StatusCode::BAD_REQUEST,
            &format!(r#"{{"statusMessage":"{long}"}}"#),
            None,
        );
        assert_eq!(summary.len(), "400 Bad Request ".len() + 200);
    }
}
