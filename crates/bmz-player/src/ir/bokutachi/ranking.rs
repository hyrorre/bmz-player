use std::collections::HashMap;

use anyhow::{Result, ensure};
use bmz_core::clear::ClearType;
use reqwest::StatusCode;
use serde::Deserialize;

use super::Game;
use super::client::{BokutachiClient, IneligibleChart};
use crate::ir::types::{
    IrRankingBody, IrRankingChartRef, IrRankingEntry, IrRankingPagination, IrRankingPlayer,
    IrRankingResult, IrRankingScope, IrRankingScore, IrRankingSelfRef,
};

pub(crate) const RANKING_LIMIT: u32 = 100;

#[derive(Deserialize)]
struct Leaderboard {
    pbs: Vec<PersonalBest>,
    users: Vec<User>,
}

#[derive(Deserialize)]
struct User {
    id: u64,
    username: String,
}

#[derive(Deserialize)]
struct PersonalBest {
    #[serde(rename = "userID")]
    user_id: u64,
    #[serde(rename = "chartID")]
    chart_id: String,
    game: String,
    #[serde(rename = "rankingData")]
    ranking_data: Rank,
    #[serde(rename = "scoreData")]
    score: Score,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rank {
    rank: u32,
    out_of: u32,
}

#[derive(Deserialize)]
struct Score {
    #[serde(rename = "score")]
    ex_score: u32,
    lamp: String,
    #[serde(default)]
    optional: ScoreMetrics,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScoreMetrics {
    bp: Option<u32>,
    max_combo: Option<u32>,
}

impl BokutachiClient {
    pub(crate) async fn fetch_chart_ranking(
        &self,
        game: Game,
        hash: &str,
        limit: u32,
        account_id: Option<u64>,
    ) -> Result<IrRankingResult> {
        let limit = limit.clamp(1, RANKING_LIMIT);
        let (chart, _) = match self.resolve_chart(game, hash, None).await {
            Ok(chart) => chart,
            Err(error)
                if error.downcast_ref::<IneligibleChart>().is_some()
                    || crate::ir::http_error::status_code_from_error(&error)
                        == Some(StatusCode::NOT_FOUND) =>
            {
                return Ok(ranking_result(hash, limit, 0, Vec::new(), None));
            }
            Err(error) => return Err(error),
        };
        let Leaderboard { pbs, users } = self
            .request(
                self.get(&format!("/api/v1/games/{}/charts/{}/pbs", game.as_str(), chart.chart_id)),
                None,
                None,
            )
            .await?;
        for pb in &pbs {
            pb.validate(game, &chart.chart_id)?;
        }
        let total = pbs.first().map_or(0, |pb| pb.ranking_data.out_of);
        let own_rank = match account_id {
            Some(account_id) => match pbs.iter().find(|pb| pb.user_id == account_id) {
                Some(pb) => Some(pb.ranking_data.rank),
                None if total > pbs.len() as u32 => {
                    self.fetch_own_rank(game, &chart.chart_id, account_id).await
                }
                None => None,
            },
            None => None,
        };
        let usernames = users.into_iter().map(|user| (user.id, user.username)).collect();
        let entries =
            pbs.into_iter().take(limit as usize).map(|pb| pb.into_entry(&usernames)).collect();
        Ok(ranking_result(hash, limit, total, entries, own_rank))
    }

    async fn fetch_own_rank(&self, game: Game, chart_id: &str, account_id: u64) -> Option<u32> {
        #[derive(Deserialize)]
        struct OwnBest {
            pb: PersonalBest,
        }
        let result: Result<OwnBest> = self
            .request(
                self.get(&format!(
                    "/api/v1/users/{account_id}/games/{}/pbs/{chart_id}",
                    game.as_str()
                )),
                None,
                None,
            )
            .await;
        match result {
            Ok(best)
                if best.pb.user_id == account_id && best.pb.validate(game, chart_id).is_ok() =>
            {
                Some(best.pb.ranking_data.rank)
            }
            Ok(_) => None,
            Err(error) => {
                if crate::ir::http_error::status_code_from_error(&error)
                    != Some(StatusCode::NOT_FOUND)
                {
                    tracing::debug!(%error, "Bokutachi: failed to fetch own chart rank");
                }
                None
            }
        }
    }
}

impl PersonalBest {
    fn validate(&self, game: Game, chart_id: &str) -> Result<()> {
        ensure!(
            self.game == game.as_str() && self.chart_id == chart_id,
            "Bokutachi: ranking response refers to a different chart"
        );
        ensure!(
            self.ranking_data.rank > 0 && self.ranking_data.rank <= self.ranking_data.out_of,
            "Bokutachi: invalid ranking position"
        );
        Ok(())
    }

    fn into_entry(self, usernames: &HashMap<u64, String>) -> IrRankingEntry {
        let id = self.user_id.to_string();
        let display_name = usernames.get(&self.user_id).cloned().unwrap_or_else(|| id.clone());
        IrRankingEntry {
            rank: self.ranking_data.rank,
            scope_rank: Some(self.ranking_data.rank),
            player: IrRankingPlayer { id, display_name },
            score: IrRankingScore {
                score_id: None,
                clear: clear_label(&self.score.lamp).to_string(),
                ex_score: self.score.ex_score,
                max_combo: self.score.optional.max_combo.unwrap_or_default(),
                min_bp: self.score.optional.bp.unwrap_or_default(),
                min_cb: 0,
                gauge: None,
                arrange_1p: None,
                arrange_2p: None,
                random_seed: None,
                double_option: None,
                verification: None,
                judges: None,
                device_type: None,
                played_at: None,
            },
        }
    }
}

fn clear_label(lamp: &str) -> &str {
    match lamp {
        "NO PLAY" => ClearType::NoPlay.as_str(),
        "FAILED" => ClearType::Failed.as_str(),
        "ASSIST CLEAR" => ClearType::AssistEasy.as_str(),
        "EASY CLEAR" => ClearType::Easy.as_str(),
        "CLEAR" => ClearType::Normal.as_str(),
        "HARD CLEAR" => ClearType::Hard.as_str(),
        "EX HARD CLEAR" => ClearType::ExHard.as_str(),
        "FULL COMBO" => ClearType::FullCombo.as_str(),
        _ => lamp,
    }
}

fn ranking_result(
    hash: &str,
    limit: u32,
    total: u32,
    entries: Vec<IrRankingEntry>,
    own_rank: Option<u32>,
) -> IrRankingResult {
    let has_more = total > entries.len() as u32;
    IrRankingResult {
        chart: IrRankingChartRef { sha256: hash.to_string() },
        ranking: IrRankingBody {
            scope: IrRankingScope::Global,
            entries,
            clear_rate: None,
            self_summary: own_rank.map(|rank| IrRankingSelfRef { rank, score_id: None }),
            pagination: Some(IrRankingPagination {
                limit,
                offset: 0,
                total: Some(total),
                has_more,
            }),
        },
    }
}
