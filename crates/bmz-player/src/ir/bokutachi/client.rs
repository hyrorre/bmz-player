use anyhow::{Context, Result, ensure};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::convert::{ChartMatch, Game, ImportRequest};
use crate::ir::types::{IrAuthTokens, IrPlayerInfo};

#[derive(Debug)]
pub(crate) struct IneligibleChart(pub String);

impl std::fmt::Display for IneligibleChart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for IneligibleChart {}

pub struct BokutachiClient {
    http: Client,
    origin: String,
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    body: T,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Chart {
    pub game: String,
    #[serde(rename = "chartID")]
    pub chart_id: String,
    pub data: ChartData,
    pub song: Song,
    pub difficulty: String,
    pub is_primary: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ChartData {
    #[serde(rename = "hashSHA256")]
    pub hash: String,
    pub notecount: u32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Song {
    pub id: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ResolvedChart {
    pub chart_id: String,
    pub matcher: ChartMatch,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ImportError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

#[derive(Debug)]
pub(crate) enum PollResult {
    Ongoing,
    Completed { score_id: Option<String>, errors: Vec<ImportError> },
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl BokutachiClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let origin = crate::ir::credentials::credential_origin(base_url)?;
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            // Even same-origin redirects are unnecessary for this fixed API contract.
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { http, origin })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
        key: Option<&str>,
        expected: Option<StatusCode>,
    ) -> Result<T> {
        let response: Envelope<T> = self.response(request, key, expected).await?;
        ensure!(response.success, "Bokutachi: API did not report success");
        Ok(response.body)
    }

    async fn response<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
        key: Option<&str>,
        expected: Option<StatusCode>,
    ) -> Result<T> {
        let response = request.send().await?;
        let status = response.status();
        let retry = crate::ir::http_error::retry_after_header(&response)
            .and_then(|value| retry_after_seconds(&value, std::time::SystemTime::now()))
            .map(|seconds| seconds.to_string());
        let body = response.text().await?;
        // Servers/proxies can echo request headers. Never retain the API key.
        let body = match key.filter(|v| !v.is_empty()) {
            Some(key) => body.replace(key, "[redacted]"),
            None => body,
        };
        if !status.is_success() {
            let description = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("description").and_then(|v| v.as_str()).map(str::to_owned));
            let safe = serde_json::json!({"message": description.unwrap_or_else(|| "Bokutachi request failed".into())}).to_string();
            return Err(crate::ir::http_error::http_response_error(
                "Bokutachi",
                status,
                &safe,
                retry.as_deref(),
            ));
        }
        ensure!(expected.is_none_or(|s| s == status), "Bokutachi: unexpected HTTP success status");
        serde_json::from_str(&body).context("Bokutachi: invalid API response")
    }

    pub(crate) async fn submit_dan(
        &self,
        key: &str,
        request: &super::dan::DanRequest,
    ) -> Result<super::dan::DanOutcome> {
        let response = self
            .response(
                self.http
                    .post(format!("{}/ir/beatoraja/submit-course", self.origin))
                    .bearer_auth(key)
                    // Protocol compatibility version, not a claim to be beatoraja/LR2oraja.
                    .header("X-TachiIR-Version", "v4")
                    .json(request),
                Some(key),
                Some(StatusCode::OK),
            )
            .await?;
        super::dan::parse_response(response)
    }

    pub async fn validate_key(&self, key: &str) -> Result<IrAuthTokens> {
        #[derive(Deserialize)]
        struct Status {
            whoami: Option<u64>,
            permissions: Vec<String>,
        }
        ensure!(!key.trim().is_empty(), "Bokutachi: enter an API key");
        let status: Status = self
            .request(
                self.http.get(format!("{}/api/v1/status", self.origin)).bearer_auth(key),
                Some(key),
                None,
            )
            .await?;
        let id =
            status.whoami.filter(|v| *v > 0).context("Bokutachi: API key is not authenticated")?;
        ensure!(
            status.permissions.iter().any(|p| p == "submit_score"),
            "Bokutachi: API key needs submit_score permission"
        );
        Ok(IrAuthTokens {
            provider_key: super::PROVIDER.into(),
            access_token: key.into(),
            refresh_token: String::new(),
            expires_at: None,
            player: IrPlayerInfo {
                id: id.to_string(),
                email: None,
                display_name: Some(id.to_string()),
            },
        })
    }

    pub(crate) async fn resolve(
        &self,
        game: Game,
        hash: &str,
        notes: u32,
    ) -> Result<ResolvedChart> {
        let selected = if game.is_pms() {
            #[derive(Deserialize)]
            struct Search {
                charts: Vec<Chart>,
            }
            let found: Search = self
                .request(
                    self.http
                        .get(format!("{}/api/v1/search/chart-hash", self.origin))
                        .query(&[("search", hash)]),
                    None,
                    None,
                )
                .await?;
            Some(select_pms_chart(found.charts, game, hash)?)
        } else {
            None
        };
        let matcher = match &selected {
            Some(chart) => ChartMatch {
                match_type: "tachiSongID".into(),
                identifier: chart.song.id.to_string(),
                difficulty: Some("CHART".into()),
            },
            None => ChartMatch {
                match_type: "bmsChartHash".into(),
                identifier: hash.into(),
                difficulty: None,
            },
        };
        #[derive(Deserialize)]
        struct Resolved {
            chart: Chart,
        }
        let found: Resolved = self
            .request(
                self.http
                    .post(format!("{}/api/v1/games/{}/charts/resolve", self.origin, game.as_str()))
                    .json(&matcher),
                None,
                None,
            )
            .await?;
        validate_chart(&found.chart, game, hash, notes, selected.as_ref())?;
        Ok(ResolvedChart { chart_id: found.chart.chart_id, matcher })
    }

    pub(crate) async fn import(&self, key: &str, request: &ImportRequest) -> Result<String> {
        #[derive(Deserialize)]
        struct Accepted {
            #[serde(rename = "importID")]
            import_id: String,
        }
        let accepted: Accepted = self
            .request(
                self.http
                    .post(format!("{}/ir/direct-manual/import", self.origin))
                    .bearer_auth(key)
                    .json(request),
                Some(key),
                Some(StatusCode::ACCEPTED),
            )
            .await?;
        ensure!(valid_id(&accepted.import_id), "Bokutachi: invalid import ID");
        Ok(accepted.import_id)
    }

    pub(crate) async fn poll(&self, id: &str, account: &str) -> Result<PollResult> {
        ensure!(valid_id(id), "Bokutachi: invalid saved import ID");
        let value: serde_json::Value = self
            .request(
                self.http.get(format!("{}/api/v1/imports/{id}/poll-status", self.origin)),
                None,
                None,
            )
            .await?;
        parse_poll(value, account)
    }
}

pub(crate) fn retry_after_seconds(value: &str, now: std::time::SystemTime) -> Option<u64> {
    value.trim().parse().ok().or_else(|| {
        httpdate::parse_http_date(value.trim()).ok().map(|date| {
            date.duration_since(now).map(|delay| delay.as_secs().saturating_add(1)).unwrap_or(0)
        })
    })
}

pub(crate) fn select_pms_chart(charts: Vec<Chart>, game: Game, hash: &str) -> Result<Chart> {
    let mut candidates = charts.into_iter().filter(|c| {
        c.game == game.as_str()
            && c.data.hash.eq_ignore_ascii_case(hash)
            && c.difficulty == "CHART"
            && c.is_primary
    });
    let selected = candidates.next().ok_or_else(|| {
        IneligibleChart("Bokutachi: chart is not registered in this input device division".into())
    })?;
    ensure!(
        candidates.all(|c| c.chart_id == selected.chart_id && c.song.id == selected.song.id),
        IneligibleChart("Bokutachi: ambiguous chart hash; submission stopped".into())
    );
    Ok(selected)
}

pub(crate) fn validate_chart(
    chart: &Chart,
    game: Game,
    hash: &str,
    notes: u32,
    selected: Option<&Chart>,
) -> Result<()> {
    ensure!(valid_id(&chart.chart_id), "Bokutachi: invalid chart ID");
    ensure!(
        chart.game == game.as_str(),
        IneligibleChart("Bokutachi: resolved chart game differs".into())
    );
    ensure!(
        chart.data.hash.eq_ignore_ascii_case(hash),
        IneligibleChart("Bokutachi: resolved chart SHA256 differs".into())
    );
    ensure!(
        chart.data.notecount == notes,
        IneligibleChart(format!(
            "Bokutachi: scoring note count differs (local {notes}, remote {})",
            chart.data.notecount
        ))
    );
    if let Some(selected) = selected {
        ensure!(
            chart.chart_id == selected.chart_id
                && chart.song.id == selected.song.id
                && chart.difficulty == "CHART"
                && chart.is_primary,
            IneligibleChart("Bokutachi: song ID now resolves to a different primary chart".into())
        );
    }
    Ok(())
}

pub(crate) fn parse_poll(value: serde_json::Value, account: &str) -> Result<PollResult> {
    match value.get("importStatus").and_then(|v| v.as_str()) {
        Some("ongoing") => Ok(PollResult::Ongoing),
        Some("completed") => {
            #[derive(Deserialize)]
            struct Completed {
                #[serde(rename = "userID")]
                user_id: u64,
                #[serde(rename = "scoreIDs")]
                score_ids: Vec<String>,
                errors: Vec<ImportError>,
            }
            let import: Completed = serde_json::from_value(
                value.get("import").context("Bokutachi: missing completed import")?.clone(),
            )
            .context("Bokutachi: invalid completed import")?;
            ensure!(
                import.user_id.to_string() == account,
                "Bokutachi: completed import belongs to another account"
            );
            ensure!(
                import.score_ids.len() <= 1 && import.score_ids.iter().all(|id| valid_id(id)),
                "Bokutachi: unexpected completed score IDs"
            );
            Ok(PollResult::Completed {
                score_id: import.score_ids.into_iter().next(),
                errors: import.errors,
            })
        }
        _ => anyhow::bail!("Bokutachi: unknown import status"),
    }
}
