//! リザルト画面用の IR 送信・ランキング表示状態。
//!
//! 通常プレイ終了時またはリザルト遷移時に [`spawn_result_ir_task`] で
//! バックグラウンドタスクを起動し、pending スコアジョブの即時送信と、
//! 設定に応じたランキング prefetch を行う。
//! タブ切り替えで未取得 scope を選んだ場合は [`ResultIrState::request_scope`]
//! で遅延取得する。スレッド間は mpsc channel で結果だけ受け渡す。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use bmz_gameplay::rule::RuleMode;

use crate::config::profile_config::IrConfig;
use crate::ir::bmz_official::{BmzOfficialIrClient, IrCourseRankingRequest, IrRankingRequest};
use crate::ir::sync::{IrSyncReport, ensure_fresh_credentials, sync_pending_ir_jobs};
use crate::ir::types::{IrCourseRankingResult, IrRankingResult, IrRankingScope};
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
    Loaded(ResultIrRanking),
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
    Ranking { scope: IrRankingScope, result: Result<ResultIrRanking, String> },
}

#[derive(Debug, Clone)]
pub struct ResultIrRanking {
    pub scope: IrRankingScope,
    pub entries: Vec<ResultIrRankingEntry>,
    pub clear_rate: Option<u32>,
    pub self_rank: Option<u32>,
    pub total: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ResultIrRankingEntry {
    pub rank: u32,
    pub player_name: String,
    pub ex_score: u32,
    pub clear: String,
    pub bp: u32,
    pub max_combo: u32,
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

#[derive(Debug, Clone)]
pub enum ResultIrTarget {
    Chart {
        chart_sha256_hex: String,
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        rule_mode: RuleMode,
    },
    Course {
        course_hash: String,
        gauge: String,
        ln_policy: String,
    },
}

impl ResultIrTarget {
    fn supports_scope(&self, scope: IrRankingScope) -> bool {
        match self {
            Self::Chart { .. } => {
                matches!(scope, IrRankingScope::Global | IrRankingScope::SelfAndRivals)
            }
            Self::Course { .. } => matches!(scope, IrRankingScope::Global),
        }
    }

    fn is_course(&self) -> bool {
        matches!(self, Self::Course { .. })
    }
}

#[derive(Debug, Clone)]
struct ResultIrTaskQuery {
    profile_root: PathBuf,
    provider: String,
    base_url: String,
    target: ResultIrTarget,
}

pub struct ResultIrState {
    pub submit: IrSubmitState,
    pub global: RankingLoadState,
    pub self_and_rivals: RankingLoadState,
    pub active_tab: ResultRankingTab,
    ir_connect_begin_at: Option<Instant>,
    ir_connect_success_at: Option<Instant>,
    ir_connect_fail_at: Option<Instant>,
    query: ResultIrTaskQuery,
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
                    self.update_submit_timer(submitted, failed, self.submit_message_is_error());
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

    fn submit_message_is_error(&self) -> bool {
        matches!(&self.submit, IrSubmitState::Done { submitted: 0, failed: 0, message: Some(_) })
    }

    fn update_submit_timer(&mut self, submitted: u32, failed: u32, error: bool) {
        let attempted = submitted > 0 || failed > 0 || error;
        if !attempted {
            self.ir_connect_begin_at = None;
            self.ir_connect_success_at = None;
            self.ir_connect_fail_at = None;
            return;
        }

        let now = Instant::now();
        self.ir_connect_begin_at.get_or_insert(now);
        if failed > 0 || error {
            self.ir_connect_fail_at = Some(now);
            self.ir_connect_success_at = None;
        } else {
            self.ir_connect_success_at = Some(now);
            self.ir_connect_fail_at = None;
        }
    }

    /// タブ選択を切り替え、未取得ならその scope の取得タスクを起動する。
    pub fn select_tab(&mut self, tab: ResultRankingTab) {
        if !self.supports_tab(tab) {
            return;
        }
        self.active_tab = tab;
        let scope = scope_for_tab(tab);
        if matches!(self.scope_slot(scope), Some(RankingLoadState::NotRequested)) {
            self.request_scope(scope);
        }
    }

    pub fn request_scope(&mut self, scope: IrRankingScope) {
        if !self.query.target.supports_scope(scope) {
            return;
        }
        if let Some(slot) = self.scope_slot(scope) {
            *slot = RankingLoadState::Loading;
        }
        spawn_ranking_fetch(self.query.clone(), scope, self.sender.clone());
    }

    pub fn supports_tab(&self, tab: ResultRankingTab) -> bool {
        self.query.target.supports_scope(scope_for_tab(tab))
    }

    pub fn is_course(&self) -> bool {
        self.query.target.is_course()
    }

    /// Result スキンの `NUMBER_IR_*` / `OPTION_IR_*` に渡す snapshot を作る。
    ///
    /// スキン表示は beatoraja 同様グローバルランキングを基準にする。
    pub fn skin_snapshot(&self) -> bmz_render::scene::ResultIrSnapshot {
        use bmz_render::scene::{ResultIrSnapshot, ResultIrState as SkinIrState};
        let snapshot = match &self.global {
            RankingLoadState::NotRequested | RankingLoadState::Loading => {
                ResultIrSnapshot { state: SkinIrState::Loading, ..Default::default() }
            }
            RankingLoadState::Failed(_) => {
                ResultIrSnapshot { state: SkinIrState::Failed, ..Default::default() }
            }
            RankingLoadState::Loaded(ranking) => result_ir_ranking_to_skin_snapshot(ranking),
        };
        self.with_connect_timers(snapshot)
    }

    fn with_connect_timers(
        &self,
        mut snapshot: bmz_render::scene::ResultIrSnapshot,
    ) -> bmz_render::scene::ResultIrSnapshot {
        snapshot.connect_begin_ms = self.ir_connect_begin_at.map(elapsed_since_ms);
        snapshot.connect_success_ms = self.ir_connect_success_at.map(elapsed_since_ms);
        snapshot.connect_fail_ms = self.ir_connect_fail_at.map(elapsed_since_ms);
        snapshot
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
    result_ir_ranking_to_skin_snapshot(&chart_ranking_to_result_ir_ranking(ranking))
}

fn result_ir_ranking_to_skin_snapshot(
    ranking: &ResultIrRanking,
) -> bmz_render::scene::ResultIrSnapshot {
    use bmz_render::scene::{
        IR_RANKING_ENTRY_SLOTS, ResultIrRankingEntrySnapshot, ResultIrRankingName,
        ResultIrSnapshot, ResultIrState as SkinIrState,
    };
    let mut entries = [ResultIrRankingEntrySnapshot::default(); IR_RANKING_ENTRY_SLOTS];
    for (slot, entry) in entries.iter_mut().zip(ranking.entries.iter()) {
        *slot = ResultIrRankingEntrySnapshot {
            rank: Some(i64::from(entry.rank)),
            ex_score: Some(i64::from(entry.ex_score)),
            player_name: ResultIrRankingName::from_display_name(&entry.player_name),
        };
    }
    ResultIrSnapshot {
        state: SkinIrState::Loaded,
        rank: ranking.self_rank.map(i64::from),
        total_player: ranking.total.map(i64::from).or(Some(ranking.entries.len() as i64)),
        clear_rate: ranking.clear_rate.map(i64::from),
        previous_rank: None,
        entries,
        ..Default::default()
    }
}

fn chart_ranking_to_result_ir_ranking(ranking: &IrRankingResult) -> ResultIrRanking {
    ResultIrRanking {
        scope: ranking.ranking.scope,
        entries: ranking
            .ranking
            .entries
            .iter()
            .map(|entry| ResultIrRankingEntry {
                rank: entry.rank,
                player_name: entry.player.display_name.clone(),
                ex_score: entry.score.ex_score,
                clear: entry.score.clear.clone(),
                bp: entry.score.min_bp,
                max_combo: entry.score.max_combo,
            })
            .collect(),
        clear_rate: ranking.ranking.clear_rate,
        self_rank: ranking.ranking.self_summary.as_ref().map(|own| own.rank),
        total: ranking.ranking.pagination.and_then(|pagination| pagination.total),
    }
}

fn course_ranking_to_result_ir_ranking(ranking: &IrCourseRankingResult) -> ResultIrRanking {
    ResultIrRanking {
        scope: ranking.ranking.scope,
        entries: ranking
            .ranking
            .entries
            .iter()
            .map(|entry| ResultIrRankingEntry {
                rank: entry.rank,
                player_name: entry.player.display_name.clone(),
                ex_score: entry.score.ex_score,
                clear: entry.score.clear.clone(),
                bp: entry.score.bp,
                max_combo: entry.score.max_combo,
            })
            .collect(),
        clear_rate: None,
        self_rank: None,
        total: Some(ranking.ranking.entries.len() as u32),
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
    spawn_result_ir_task_for_target(
        profile_root,
        score_db_path,
        ir_config,
        ResultIrTarget::Chart { chart_sha256_hex, ln_policy, double_option, rule_mode },
    )
}

pub fn spawn_course_result_ir_task(
    profile_root: PathBuf,
    score_db_path: PathBuf,
    ir_config: &IrConfig,
    course_hash: String,
    gauge: String,
    ln_policy: String,
) -> Option<ResultIrState> {
    spawn_result_ir_task_for_target(
        profile_root,
        score_db_path,
        ir_config,
        ResultIrTarget::Course { course_hash, gauge, ln_policy },
    )
}

fn spawn_result_ir_task_for_target(
    profile_root: PathBuf,
    score_db_path: PathBuf,
    ir_config: &IrConfig,
    target: ResultIrTarget,
) -> Option<ResultIrState> {
    let provider = ir_config.providers.iter().find(|provider| {
        provider.enabled
            && !provider.base_url.is_empty()
            && crate::ir::provider_key::configured_provider_key(provider).is_some()
    })?;
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)?;
    let query = ResultIrTaskQuery {
        profile_root,
        provider: provider_key.to_string(),
        base_url: provider.base_url.clone(),
        target,
    };
    let (sender, receiver) = channel();

    let mut state = ResultIrState {
        submit: IrSubmitState::Sending,
        global: RankingLoadState::NotRequested,
        self_and_rivals: RankingLoadState::NotRequested,
        active_tab: ResultRankingTab::Global,
        ir_connect_begin_at: Some(Instant::now()),
        ir_connect_success_at: None,
        ir_connect_fail_at: None,
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
    let prefetch_rivals = query.target.supports_scope(IrRankingScope::SelfAndRivals)
        && state_prefetch_rivals(&ir_config);
    tokio::spawn(async move {
        let now = now_unix_seconds();
        let outcome = async {
            let mut score_db = crate::storage::score_db::ScoreDatabase::open(&score_db_path)?;
            sync_pending_ir_jobs(&mut score_db, &submit_query.profile_root, &ir_config, now, 20)
                .await
        }
        .await;
        let mut included_global_ranking = None;
        let event = match outcome {
            Ok(report) => {
                included_global_ranking = included_global_ranking_for_query(&submit_query, &report);
                ResultIrEvent::Submit {
                    submitted: report.submitted,
                    failed: report.failed,
                    message: report.messages.first().cloned(),
                }
            }
            Err(error) => ResultIrEvent::Submit {
                submitted: 0,
                failed: 0,
                message: Some(format!("{error:#}")),
            },
        };
        let _ = submit_sender.send(event);
        let included_global_loaded = included_global_ranking.is_some();
        if let Some(ranking) = included_global_ranking {
            let _ = submit_sender.send(ResultIrEvent::Ranking {
                scope: IrRankingScope::Global,
                result: Ok(ranking),
            });
        }
        // 送信完了後に prefetch する。best 更新前のランキングを返さないため。
        if prefetch_global && !included_global_loaded {
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

fn elapsed_since_ms(started_at: Instant) -> i32 {
    started_at.elapsed().as_millis().min(i32::MAX as u128) as i32
}

fn state_prefetch_rivals(ir_config: &IrConfig) -> bool {
    ir_config.prefetch_rival_ranking_on_score_submit
}

fn included_global_ranking_for_query(
    query: &ResultIrTaskQuery,
    report: &IrSyncReport,
) -> Option<ResultIrRanking> {
    let ResultIrTarget::Chart { chart_sha256_hex, .. } = &query.target else {
        return None;
    };
    report
        .included_rankings
        .iter()
        .find(|ranking| {
            ranking.chart.sha256 == *chart_sha256_hex
                && ranking.ranking.scope == IrRankingScope::Global
        })
        .map(chart_ranking_to_result_ir_ranking)
}

fn spawn_ranking_fetch(
    query: ResultIrTaskQuery,
    scope: IrRankingScope,
    sender: Sender<ResultIrEvent>,
) {
    tokio::spawn(async move {
        fetch_ranking_and_send(&query, scope, &sender).await;
    });
}

async fn fetch_ranking_and_send(
    query: &ResultIrTaskQuery,
    scope: IrRankingScope,
    sender: &Sender<ResultIrEvent>,
) {
    let result = fetch_result_ranking(query, scope).await.map_err(|error| format!("{error:#}"));
    let _ = sender.send(ResultIrEvent::Ranking { scope, result });
}

async fn fetch_result_ranking(
    query: &ResultIrTaskQuery,
    scope: IrRankingScope,
) -> anyhow::Result<ResultIrRanking> {
    match &query.target {
        ResultIrTarget::Chart { chart_sha256_hex, ln_policy, double_option, rule_mode } => {
            let ranking = fetch_ranking(
                &ResultIrQuery {
                    profile_root: query.profile_root.clone(),
                    provider: query.provider.clone(),
                    base_url: query.base_url.clone(),
                    chart_sha256_hex: chart_sha256_hex.clone(),
                    ln_policy: *ln_policy,
                    double_option: *double_option,
                    rule_mode: *rule_mode,
                },
                scope,
            )
            .await?;
            Ok(chart_ranking_to_result_ir_ranking(&ranking))
        }
        ResultIrTarget::Course { course_hash, gauge, ln_policy } => {
            if scope != IrRankingScope::Global {
                anyhow::bail!("course IR ranking supports global scope only");
            }
            let client = BmzOfficialIrClient::anonymous(&query.base_url)?;
            let ranking = client
                .fetch_course_ranking(
                    course_hash,
                    &IrCourseRankingRequest {
                        gauge: gauge.clone(),
                        ln_policy: ln_policy.clone(),
                        limit: 20,
                    },
                )
                .await?;
            Ok(course_ranking_to_result_ir_ranking(&ranking))
        }
    }
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
    use std::path::PathBuf;

    use bmz_gameplay::rule::RuleMode;

    use crate::ir::sync::IrSyncReport;
    use crate::ir::types::{
        IrCourseRankingBody, IrCourseRankingCourseRef, IrCourseRankingEntry, IrCourseRankingResult,
        IrCourseRankingScore, IrRankingBody, IrRankingChartRef, IrRankingEntry,
        IrRankingPagination, IrRankingPlayer, IrRankingResult, IrRankingScope, IrRankingScore,
        IrRankingSelfRef,
    };
    use crate::ln_policy::LnScorePolicy;
    use crate::select_options::DoubleOptionScoreBucket;

    use super::{
        ResultIrTarget, ResultIrTaskQuery, course_ranking_to_result_ir_ranking,
        included_global_ranking_for_query, ranking_to_ir_snapshot,
    };

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

    #[test]
    fn course_ranking_snapshot_uses_course_score_fields() {
        let ranking = IrCourseRankingResult {
            course: IrCourseRankingCourseRef { course_hash: "ab".repeat(32) },
            rule: None,
            ranking: IrCourseRankingBody {
                scope: IrRankingScope::Global,
                entries: vec![IrCourseRankingEntry {
                    rank: 2,
                    player: IrRankingPlayer {
                        id: "player-2".to_string(),
                        display_name: "course-player".to_string(),
                    },
                    score: IrCourseRankingScore {
                        course_score_id: "course-score-1".to_string(),
                        clear: "Normal".to_string(),
                        course_clear: true,
                        ex_score: 1234,
                        max_combo: 456,
                        bp: 7,
                        device_type: Some("keyboard".to_string()),
                        played_at: None,
                        verification: Some("signed".to_string()),
                    },
                }],
            },
        };

        let display = course_ranking_to_result_ir_ranking(&ranking);

        assert_eq!(display.total, Some(1));
        assert_eq!(display.entries[0].rank, 2);
        assert_eq!(display.entries[0].player_name, "course-player");
        assert_eq!(display.entries[0].ex_score, 1234);
        assert_eq!(display.entries[0].bp, 7);
    }

    #[test]
    fn included_global_ranking_uses_only_current_chart() {
        let query = ResultIrTaskQuery {
            profile_root: PathBuf::new(),
            provider: "bmz-official".to_string(),
            base_url: "https://ir.example.test".to_string(),
            target: ResultIrTarget::Chart {
                chart_sha256_hex: "current".to_string(),
                ln_policy: LnScorePolicy::AutoLn,
                double_option: DoubleOptionScoreBucket::Off,
                rule_mode: RuleMode::Beatoraja,
            },
        };
        let report = IrSyncReport {
            submitted: 1,
            failed: 0,
            messages: Vec::new(),
            included_rankings: vec![
                IrRankingResult {
                    chart: IrRankingChartRef { sha256: "other".to_string() },
                    ranking: IrRankingBody {
                        scope: IrRankingScope::Global,
                        entries: Vec::new(),
                        clear_rate: None,
                        self_summary: None,
                        pagination: None,
                    },
                },
                IrRankingResult {
                    chart: IrRankingChartRef { sha256: "current".to_string() },
                    ranking: IrRankingBody {
                        scope: IrRankingScope::Global,
                        entries: Vec::new(),
                        clear_rate: Some(75),
                        self_summary: None,
                        pagination: Some(IrRankingPagination {
                            limit: 20,
                            offset: 0,
                            total: Some(2),
                            has_more: false,
                        }),
                    },
                },
            ],
        };

        let ranking = included_global_ranking_for_query(&query, &report).unwrap();

        assert_eq!(ranking.scope, IrRankingScope::Global);
        assert_eq!(ranking.clear_rate, Some(75));
        assert_eq!(ranking.total, Some(2));
    }
}
