use anyhow::{Context, Result, bail};
use reqwest::Url;

use super::types::{IrRankingScope, IrScoreSubmission, IrSubmitOptions, IrSubmitResponse};

#[derive(Debug, Clone)]
pub struct BmzOfficialIrClient {
    base_url: Url,
    access_token: String,
    http: reqwest::Client,
}

impl BmzOfficialIrClient {
    pub fn new(base_url: &str, access_token: impl Into<String>) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(base_url).context("invalid BMZ IR base URL")?,
            access_token: access_token.into(),
            http: reqwest::Client::new(),
        })
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
            .bearer_auth(&self.access_token)
            .json(request)
            .send()
            .await
            .context("failed to send BMZ IR score submission")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("BMZ IR score submission failed: {status} {body}");
        }
        response.json().await.context("failed to decode BMZ IR submit response")
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
}
