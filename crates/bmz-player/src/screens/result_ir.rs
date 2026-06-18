//! リザルト画面用の IR 送信・ランキング表示状態。
//!
//! リザルト遷移時に [`spawn_result_ir_task`] でバックグラウンドタスクを起動し、
//! pending スコアジョブの即時送信と、設定に応じたランキング prefetch を行う。
//! タブ切り替えで未取得 scope を選んだ場合は [`ResultIrState::request_scope`]
//! で遅延取得する。スレッド間は mpsc channel で結果だけ受け渡す。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use bmz_gameplay::rule::RuleMode;

use crate::config::profile_config::IrConfig;
use crate::ir::bmz_official::{BmzOfficialIrClient, IrRankingRequest};
use crate::ir::sync::{ensure_fresh_credentials, sync_pending_ir_jobs};
use crate::ir::types::{IrRankingResult, IrRankingScope};
use crate::ln_policy::LnScorePolicy;
use crate::select_options::DoubleOptionScoreBucket;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultRankingTab {
    Global,
    SelfAndRivals,
}

#[derive(Debug, Clone)]
pub enum RankingLoadState {
    NotRequested,
    Loading,
    Loaded(IrRankingResult),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum IrSubmitState {
    Sending,
    Done { submitted: u32, failed: u32, message: Option<String> },
}

#[derive(Debug)]
pub enum ResultIrEvent {
    Submit { submitted: u32, failed: u32, message: Option<String> },
    Ranking { scope: IrRankingScope, result: Result<IrRankingResult, String> },
}

/// ランキング照会に必要なクエリ条件。タブ遅延取得でも使い回す。
#[derive(Debug, Clone)]
pub struct ResultIrQuery {
    pub profile_root: PathBuf,
    pub provider: String,
    pub base_url: String,
    pub chart_sha256_hex: String,
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub rule_mode: RuleMode,
}

pub struct ResultIrState {
    pub submit: IrSubmitState,
    pub global: RankingLoadState,
    pub self_and_rivals: RankingLoadState,
    pub active_tab: ResultRankingTab,
    query: ResultIrQuery,
    sender: Sender<ResultIrEvent>,
    receiver: Receiver<ResultIrEvent>,
}

impl ResultIrState {
    /// 受信済みイベントを状態へ反映する。毎フレーム呼ぶ。
    pub fn poll(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                ResultIrEvent::Submit { submitted, failed, message } => {
                    self.submit = IrSubmitState::Done { submitted, failed, message };
                }
                ResultIrEvent::Ranking { scope, result } => {
                    let slot = self.scope_slot(scope);
                    if let Some(slot) = slot {
                        *slot = match result {
                            Ok(ranking) => RankingLoadState::Loaded(ranking),
                            Err(error) => RankingLoadState::Failed(error),
                        };
                    }
                }
            }
        }
    }

    /// タブ選択を切り替え、未取得ならその scope の取得タスクを起動する。
    pub fn select_tab(&mut self, tab: ResultRankingTab) {
        self.active_tab = tab;
        let scope = scope_for_tab(tab);
        if matches!(self.scope_slot(scope), Some(RankingLoadState::NotRequested)) {
            self.request_scope(scope);
        }
    }

    pub fn request_scope(&mut self, scope: IrRankingScope) {
        if let Some(slot) = self.scope_slot(scope) {
            *slot = RankingLoadState::Loading;
        }
        spawn_ranking_fetch(self.query.clone(), scope, self.sender.clone());
    }

    /// Result スキンの `NUMBER_IR_*` / `OPTION_IR_*` に渡す snapshot を作る。
    ///
    /// スキン表示は beatoraja 同様グローバルランキングを基準にする。
    pub fn skin_snapshot(&self) -> bmz_render::scene::ResultIrSnapshot {
        use bmz_render::scene::{ResultIrSnapshot, ResultIrState as SkinIrState};
        match &self.global {
            RankingLoadState::NotRequested | RankingLoadState::Loading => {
                ResultIrSnapshot { state: SkinIrState::Loading, ..Default::default() }
            }
            RankingLoadState::Failed(_) => {
                ResultIrSnapshot { state: SkinIrState::Failed, ..Default::default() }
            }
            RankingLoadState::Loaded(ranking) => ranking_to_ir_snapshot(ranking),
        }
    }

    pub fn active_state(&self) -> &RankingLoadState {
        match self.active_tab {
            ResultRankingTab::Global => &self.global,
            ResultRankingTab::SelfAndRivals => &self.self_and_rivals,
        }
    }

    fn scope_slot(&mut self, scope: IrRankingScope) -> Option<&mut RankingLoadState> {
        match scope {
            IrRankingScope::Global => Some(&mut self.global),
            IrRankingScope::SelfAndRivals => Some(&mut self.self_and_rivals),
            _ => None,
        }
    }
}

/// 取得済みグローバルランキングをスキン用 snapshot に変換する。
pub fn ranking_to_ir_snapshot(ranking: &IrRankingResult) -> bmz_render::scene::ResultIrSnapshot {
    use bmz_render::scene::{
        IR_RANKING_ENTRY_SLOTS, ResultIrRankingEntrySnapshot, ResultIrRankingName,
        ResultIrSnapshot, ResultIrState as SkinIrState,
    };
    let mut entries = [ResultIrRankingEntrySnapshot::default(); IR_RANKING_ENTRY_SLOTS];
    for (slot, entry) in entries.iter_mut().zip(ranking.ranking.entries.iter()) {
        *slot = ResultIrRankingEntrySnapshot {
            rank: Some(i64::from(entry.rank)),
            ex_score: Some(i64::from(entry.score.ex_score)),
            player_name: ResultIrRankingName::from_display_name(&entry.player.display_name),
        };
    }
    ResultIrSnapshot {
        state: SkinIrState::Loaded,
        rank: ranking.ranking.self_summary.as_ref().map(|own| i64::from(own.rank)),
        total_player: ranking
            .ranking
            .pagination
            .and_then(|pagination| pagination.total)
            .map(i64::from)
            .or(Some(ranking.ranking.entries.len() as i64)),
        clear_rate: ranking.ranking.clear_rate.map(i64::from),
        previous_rank: None,
        entries,
    }
}

fn scope_for_tab(tab: ResultRankingTab) -> IrRankingScope {
    match tab {
        ResultRankingTab::Global => IrRankingScope::Global,
        ResultRankingTab::SelfAndRivals => IrRankingScope::SelfAndRivals,
    }
}

/// リザルト遷移時に呼ぶ。IR 未設定なら `None`。
///
/// 起動するタスク:
/// 1. pending スコアジョブの即時送信 (このリザルト分を含む)
/// 2. prefetch 設定が ON の scope のランキング取得
///
/// prefetch が両方 OFF でも、パネル表示時のタブ選択で遅延取得できる。
pub fn spawn_result_ir_task(
    profile_root: PathBuf,
    score_db_path: PathBuf,
    ir_config: &IrConfig,
    chart_sha256_hex: String,
    ln_policy: LnScorePolicy,
    double_option: DoubleOptionScoreBucket,
    rule_mode: RuleMode,
) -> Option<ResultIrState> {
    let provider = ir_config
        .providers
        .iter()
        .find(|provider| provider.enabled && !provider.base_url.is_empty())?;
    let query = ResultIrQuery {
        profile_root,
        provider: provider.provider.clone(),
        base_url: provider.base_url.clone(),
        chart_sha256_hex,
        ln_policy,
        double_option,
        rule_mode,
    };
    let (sender, receiver) = channel();

    let mut state = ResultIrState {
        submit: IrSubmitState::Sending,
        global: RankingLoadState::NotRequested,
        self_and_rivals: RankingLoadState::NotRequested,
        active_tab: ResultRankingTab::Global,
        query: query.clone(),
        sender: sender.clone(),
        receiver,
    };

    let submit_sender = sender.clone();
    let ir_config = ir_config.clone();
    let submit_query = query.clone();
    // global は Result スキンの NUMBER_IR_RANK / OPTION_IR_* 表示にも使うため、
    // prefetch 設定に関わらず常に取得する。rivals scope のみ設定に従う。
    let prefetch_global = true;
    let prefetch_rivals = state_prefetch_rivals(&ir_config);
    tokio::spawn(async move {
        let now = now_unix_seconds();
        let outcome = async {
            let mut score_db = crate::storage::score_db::ScoreDatabase::open(&score_db_path)?;
            sync_pending_ir_jobs(&mut score_db, &submit_query.profile_root, &ir_config, now, 20)
                .await
        }
        .await;
        let event = match outcome {
            Ok(report) => ResultIrEvent::Submit {
                submitted: report.submitted,
                failed: report.failed,
                message: report.messages.first().cloned(),
            },
            Err(error) => ResultIrEvent::Submit {
                submitted: 0,
                failed: 0,
                message: Some(format!("{error:#}")),
            },
        };
        let _ = submit_sender.send(event);
        // 送信完了後に prefetch する。best 更新前のランキングを返さないため。
        if prefetch_global {
            fetch_ranking_and_send(&submit_query, IrRankingScope::Global, &submit_sender).await;
        }
        if prefetch_rivals {
            fetch_ranking_and_send(&submit_query, IrRankingScope::SelfAndRivals, &submit_sender)
                .await;
        }
    });

    if prefetch_global {
        state.global = RankingLoadState::Loading;
    }
    if prefetch_rivals {
        state.self_and_rivals = RankingLoadState::Loading;
    }
    Some(state)
}

fn state_prefetch_rivals(ir_config: &IrConfig) -> bool {
    ir_config.prefetch_rival_ranking_on_score_submit
}

fn spawn_ranking_fetch(query: ResultIrQuery, scope: IrRankingScope, sender: Sender<ResultIrEvent>) {
    tokio::spawn(async move {
        fetch_ranking_and_send(&query, scope, &sender).await;
    });
}

async fn fetch_ranking_and_send(
    query: &ResultIrQuery,
    scope: IrRankingScope,
    sender: &Sender<ResultIrEvent>,
) {
    let result = fetch_ranking(query, scope).await.map_err(|error| format!("{error:#}"));
    let _ = sender.send(ResultIrEvent::Ranking { scope, result });
}

pub(crate) async fn fetch_ranking(
    query: &ResultIrQuery,
    scope: IrRankingScope,
) -> anyhow::Result<IrRankingResult> {
    let now = now_unix_seconds();
    let mut client = BmzOfficialIrClient::anonymous(&query.base_url)?;
    // self / rivals scope は認証必須。global は匿名でも可。
    match ensure_fresh_credentials(&query.profile_root, &query.provider, &query.base_url, now).await
    {
        Ok(credentials) => client.set_access_token(credentials.access_token),
        Err(error) if scope != IrRankingScope::Global => return Err(error),
        Err(_) => {}
    }
    client
        .fetch_ranking(
            &query.chart_sha256_hex,
            &IrRankingRequest {
                scope,
                ln_policy: query.ln_policy.as_str().to_string(),
                double_option: query.double_option,
                rule_mode: query.rule_mode,
                limit: 20,
                offset: 0,
            },
        )
        .await
}

fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use crate::ir::types::{
        IrRankingBody, IrRankingChartRef, IrRankingEntry, IrRankingPagination, IrRankingPlayer,
        IrRankingResult, IrRankingScope, IrRankingScore, IrRankingSelfRef,
    };

    use super::ranking_to_ir_snapshot;

    #[test]
    fn ranking_snapshot_carries_skin_ranking_rows() {
        let ranking = IrRankingResult {
            chart: IrRankingChartRef { sha256: "abc".to_string() },
            ranking: IrRankingBody {
                scope: IrRankingScope::Global,
                entries: vec![IrRankingEntry {
                    rank: 1,
                    scope_rank: None,
                    player: IrRankingPlayer {
                        id: "player-1".to_string(),
                        display_name: "hyrorre".to_string(),
                    },
                    score: IrRankingScore {
                        clear: "Perfect".to_string(),
                        ex_score: 46,
                        max_combo: 28,
                        min_bp: 0,
                        min_cb: 0,
                        device_type: None,
                        played_at: None,
                    },
                }],
                clear_rate: Some(100),
                self_summary: Some(IrRankingSelfRef { rank: 1, score_id: None }),
                pagination: Some(IrRankingPagination {
                    limit: 20,
                    offset: 0,
                    total: Some(1),
                    has_more: false,
                }),
            },
        };

        let snapshot = ranking_to_ir_snapshot(&ranking);
        assert_eq!(snapshot.rank, Some(1));
        assert_eq!(snapshot.total_player, Some(1));
        assert_eq!(snapshot.clear_rate, Some(100));
        assert_eq!(snapshot.entries[0].rank, Some(1));
        assert_eq!(snapshot.entries[0].ex_score, Some(46));
        assert_eq!(snapshot.entries[0].player_name.as_str(), "hyrorre");
    }
}
