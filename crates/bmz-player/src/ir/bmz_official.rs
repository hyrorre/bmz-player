use anyhow::{Context, Result, bail};
use reqwest::Url;

use crate::select_options::DoubleOptionScoreBucket;

use super::types::{
    IrAuthTokens, IrDeviceKeysResponse, IrMeResponse, IrRankingResult, IrRankingScope,
    IrReplayDownloadTarget, IrReplayUploadTarget, IrReplayVerifyResult, IrRivalsResponse,
    IrScoreSubmission, IrSubmitOptions, IrSubmitResponse,
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
    pub gauge: String,
    pub ln_policy: String,
    pub double_option: DoubleOptionScoreBucket,
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

    /// 署名付き URL へ replay 本体を PUT する。
    pub async fn upload_replay(&self, upload_url: &str, bytes: Vec<u8>) -> Result<()> {
        let response = self
            .http
            .put(upload_url)
            .header("content-type", "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .context("failed to upload BMZ IR replay")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("BMZ IR replay upload failed: {status} {body}");
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
        let mut url = self.base_url.join(&format!("/api/v1/charts/{chart_sha256_hex}/ranking"))?;
        url.query_pairs_mut()
            .append_pair("scope", scope_query_value(&request.scope))
            .append_pair("gauge", &request.gauge)
            .append_pair("ln_policy", &request.ln_policy)
            .append_pair("limit", &request.limit.to_string())
            .append_pair("offset", &request.offset.to_string());
        if let Some(double_option) = request.double_option.ir_query_value() {
            url.query_pairs_mut().append_pair("double_option", double_option);
        }
        let mut builder = self.http.get(url);
        if let Some(token) = &self.access_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await.context("failed to send BMZ IR ranking request")?;
        decode_response(response, "BMZ IR ranking fetch").await
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
        let body = response.text().await.unwrap_or_default();
        bail!("{label} failed: {status} {body}");
    }
    response.json().await.with_context(|| format!("failed to decode {label} response"))
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
    fn anonymous_client_rejects_authenticated_calls() {
        let client = BmzOfficialIrClient::anonymous("https://ir.example.test").unwrap();
        assert!(client.require_token().is_err());
    }
}
