//! リザルト画面用の IR 送信・ランキング表示状態。
//!
//! 通常プレイ終了時またはリザルト遷移時に [`spawn_result_ir_task`] で
//! バックグラウンドタスクを起動し、pending スコアジョブの即時送信と、
//! 設定に応じたランキング prefetch を行う。
//! タブ切り替えで未取得 scope を選んだ場合は [`ResultIrState::request_scope`]
//! で遅延取得する。スレッド間は mpsc channel で結果だけ受け渡す。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use bmz_gameplay::rule::RuleMode;

use crate::config::profile_config::IrConfig;
use crate::ir::bmz_official::{BmzOfficialIrClient, IrCourseRankingRequest, IrRankingRequest};
use crate::ir::sync::{
    IR_SYNC_BATCH_LIMIT, IrSyncJobFilter, IrSyncReport, IrSyncThrottle, ensure_fresh_credentials,
    sync_pending_ir_jobs, sync_pending_ir_jobs_filtered,
};
use crate::ir::types::{IrCourseRankingResult, IrRankingResult, IrRankingScope, IrSubmitResponse};
use crate::ln_policy::LnScorePolicy;
use crate::select_options::DoubleOptionScoreBucket;
use crate::storage::network_db::{IrJobKind, IrScoreJobRecord, IrScoreJobStatus};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrSubmitState {
    Sending,
    Done { submitted: u32, failed: u32, message: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultIrProviderSubmission {
    pub provider_key: String,
    pub display_name: String,
    pub primary: bool,
    pub state: IrSubmitState,
}

#[derive(Debug)]
pub enum ResultIrEvent {
    Submit { provider: String, submitted: u32, failed: u32, message: Option<String> },
    Ranking { provider: String, scope: IrRankingScope, result: Result<ResultIrRanking, String> },
}

#[derive(Debug, Clone)]
pub struct ResultIrRanking {
    pub scope: IrRankingScope,
    pub entries: Vec<ResultIrRankingEntry>,
    pub clear_rate: Option<u32>,
    pub self_rank: Option<u32>,
    pub previous_rank: Option<u32>,
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

#[derive(Debug, Clone)]
pub struct ResultIrLoadedChartRanking {
    pub chart_sha256_hex: String,
    pub ranking: ResultIrRanking,
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
        local_score_id: i64,
        chart_sha256_hex: String,
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        rule_mode: RuleMode,
    },
    Course {
        local_score_id: i64,
        course_hash: String,
        rian_course_hash_v1: String,
        bms_ir_course_key: Option<String>,
        gauge: String,
        ln_policy: String,
        rule_mode: RuleMode,
    },
}

#[derive(Debug, Clone)]
pub struct ResultIrCourseHashes {
    pub local: String,
    pub rian_v1: String,
    pub bms_ir: Option<String>,
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

    fn submission_job(&self) -> (IrJobKind, i64) {
        match self {
            Self::Chart { local_score_id, .. } => (IrJobKind::Score, *local_score_id),
            Self::Course { local_score_id, .. } => (IrJobKind::Course, *local_score_id),
        }
    }

    fn matches_chart_result(
        &self,
        local_score_id: i64,
        chart_sha256_hex: &str,
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        rule_mode: RuleMode,
    ) -> bool {
        matches!(
            self,
            Self::Chart {
                local_score_id: state_score_id,
                chart_sha256_hex: state_chart_sha256,
                ln_policy: state_ln_policy,
                double_option: state_double_option,
                rule_mode: state_rule_mode,
            } if *state_score_id == local_score_id
                && state_chart_sha256 == chart_sha256_hex
                && *state_ln_policy == ln_policy
                && *state_double_option == double_option
                && *state_rule_mode == rule_mode
        )
    }
}

impl ResultIrTaskQuery {
    fn supports_scope(&self, scope: IrRankingScope) -> bool {
        self.target.supports_scope(scope)
            && (!crate::ir::rian_ir::is_rian_ir_provider(&self.provider)
                || scope == IrRankingScope::Global)
    }
}

#[derive(Debug, Clone)]
struct ResultIrTaskQuery {
    profile_root: PathBuf,
    provider: String,
    account_id: String,
    base_url: String,
    target: ResultIrTarget,
}

#[derive(Debug, Clone)]
struct ResultIrSubmissionTarget {
    provider: String,
    account_id: String,
}

pub struct ResultIrState {
    pub submit: IrSubmitState,
    pub provider_submissions: Vec<ResultIrProviderSubmission>,
    pub global: RankingLoadState,
    pub self_and_rivals: RankingLoadState,
    pub active_tab: ResultRankingTab,
    ir_connect_begin_at: Option<Instant>,
    ir_connect_success_at: Option<Instant>,
    ir_connect_fail_at: Option<Instant>,
    provider_name: bmz_render::scene::ResultIrRankingName,
    user_name: bmz_render::scene::ResultIrRankingName,
    global_skin_scroll_offset: usize,
    self_and_rivals_skin_scroll_offset: usize,
    query: ResultIrTaskQuery,
    sender: Sender<ResultIrEvent>,
    receiver: Receiver<ResultIrEvent>,
}

impl ResultIrState {
    /// この state が指定された単曲リザルトのために作られたものかを返す。
    ///
    /// 同じ譜面をクイックリトライしても chart hash は変わらないため、保存された
    /// score history ID まで照合して旧試行の state を再利用しない。
    pub fn matches_chart_result(
        &self,
        local_score_id: i64,
        chart_sha256_hex: &str,
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        rule_mode: RuleMode,
    ) -> bool {
        self.query.target.matches_chart_result(
            local_score_id,
            chart_sha256_hex,
            ln_policy,
            double_option,
            rule_mode,
        )
    }

    /// 受信済みイベントを状態へ反映する。毎フレーム呼ぶ。
    pub fn poll(&mut self) -> Vec<ResultIrLoadedChartRanking> {
        let mut loaded_chart_rankings = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                ResultIrEvent::Submit { provider, submitted, failed, message } => {
                    let state = IrSubmitState::Done { submitted, failed, message };
                    if let Some(provider_state) = self
                        .provider_submissions
                        .iter_mut()
                        .find(|entry| entry.provider_key == provider)
                    {
                        provider_state.state = state.clone();
                    }
                    if provider == self.query.provider {
                        self.submit = state;
                        self.update_submit_timer(submitted, failed, self.submit_message_is_error());
                    }
                }
                ResultIrEvent::Ranking { provider, scope, result } => {
                    if provider != self.query.provider {
                        tracing::warn!(
                            event_provider = provider,
                            primary_provider = self.query.provider,
                            "ignored Result ranking from a non-primary IR provider",
                        );
                        continue;
                    }
                    match result {
                        Ok(ranking) => {
                            if let Some(loaded) = self.loaded_chart_ranking(&ranking) {
                                loaded_chart_rankings.push(loaded);
                            }
                            if let Some(slot) = self.scope_slot(scope) {
                                *slot = RankingLoadState::Loaded(ranking);
                            }
                        }
                        Err(error) => {
                            if let Some(slot) = self.scope_slot(scope) {
                                *slot = RankingLoadState::Failed(error);
                            }
                        }
                    }
                }
            }
        }
        loaded_chart_rankings
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
        if !self.query.supports_scope(scope) {
            return;
        }
        if let Some(slot) = self.scope_slot(scope) {
            *slot = RankingLoadState::Loading;
        }
        spawn_ranking_fetch(self.query.clone(), scope, self.sender.clone());
    }

    pub fn supports_tab(&self, tab: ResultRankingTab) -> bool {
        self.query.supports_scope(scope_for_tab(tab))
    }

    pub fn is_course(&self) -> bool {
        self.query.target.is_course()
    }

    /// 既存スキン互換の global ranking snapshot を作る。
    pub fn skin_snapshot(&self) -> bmz_render::scene::ResultIrSnapshot {
        self.skin_snapshot_for_binding(bmz_render::skin::ResultIrScopeBinding::Global)
    }

    /// Result スキンの scope binding に従う snapshot を作る。
    ///
    /// `Global` は beatoraja 互換の既存 ref を常に全体ランキングへ束縛する。
    /// `Active` は BMZ 拡張を宣言したスキンだけが選択中タブを受け取る。
    pub fn skin_snapshot_for_binding(
        &self,
        binding: bmz_render::skin::ResultIrScopeBinding,
    ) -> bmz_render::scene::ResultIrSnapshot {
        use bmz_render::scene::{ResultIrSnapshot, ResultIrState as SkinIrState};
        let tab = match binding {
            bmz_render::skin::ResultIrScopeBinding::Global => ResultRankingTab::Global,
            bmz_render::skin::ResultIrScopeBinding::Active => self.active_tab,
        };
        let snapshot = match self.state_for_tab(tab) {
            RankingLoadState::NotRequested | RankingLoadState::Loading => {
                ResultIrSnapshot { state: SkinIrState::Loading, ..Default::default() }
            }
            RankingLoadState::Failed(_) => {
                ResultIrSnapshot { state: SkinIrState::Failed, ..Default::default() }
            }
            RankingLoadState::Loaded(ranking) => {
                result_ir_ranking_to_skin_snapshot_at(ranking, self.skin_scroll_offset_for(tab))
            }
        };
        self.with_connect_timers(snapshot, tab)
    }

    fn with_connect_timers(
        &self,
        mut snapshot: bmz_render::scene::ResultIrSnapshot,
        tab: ResultRankingTab,
    ) -> bmz_render::scene::ResultIrSnapshot {
        snapshot.connect_begin_ms = self.ir_connect_begin_at.map(elapsed_since_ms);
        snapshot.connect_success_ms = self.ir_connect_success_at.map(elapsed_since_ms);
        snapshot.connect_fail_ms = self.ir_connect_fail_at.map(elapsed_since_ms);
        snapshot.online = true;
        snapshot.provider_name = self.provider_name;
        snapshot.user_name = self.user_name;
        snapshot.scope = match tab {
            ResultRankingTab::Global => bmz_render::scene::ResultIrScope::Global,
            ResultRankingTab::SelfAndRivals => bmz_render::scene::ResultIrScope::Rival,
        };
        snapshot.global_scope_supported = self.supports_tab(ResultRankingTab::Global);
        snapshot.rival_scope_supported = self.supports_tab(ResultRankingTab::SelfAndRivals);
        snapshot
    }

    pub fn active_state(&self) -> &RankingLoadState {
        self.state_for_tab(self.active_tab)
    }

    fn state_for_tab(&self, tab: ResultRankingTab) -> &RankingLoadState {
        match tab {
            ResultRankingTab::Global => &self.global,
            ResultRankingTab::SelfAndRivals => &self.self_and_rivals,
        }
    }

    pub fn set_skin_scroll_rate(&mut self, value: f32) {
        let max = self.skin_scroll_max();
        let offset = ((value.clamp(0.0, 1.0) * max as f32).round() as usize).min(max);
        self.set_skin_scroll_offset(offset);
    }

    /// 表示中の Result IR ランキングを行単位で相対移動する。
    ///
    /// 正は末尾方向、負は先頭方向。実際に表示位置が変わった場合だけ true を返す。
    pub fn scroll_skin_rows(&mut self, rows: i32) -> bool {
        let max = self.skin_scroll_max();
        let stored = self.skin_scroll_offset_for(self.active_tab);
        let current = stored.min(max);
        let next = if rows >= 0 {
            current.saturating_add(rows as usize).min(max)
        } else {
            current.saturating_sub(rows.unsigned_abs() as usize)
        };
        if next == current {
            if stored != current {
                self.set_skin_scroll_offset(current);
            }
            return false;
        }
        self.set_skin_scroll_offset(next);
        true
    }

    fn skin_scroll_max(&self) -> usize {
        match self.active_state() {
            RankingLoadState::Loaded(ranking) => {
                ranking.entries.len().saturating_sub(bmz_render::scene::IR_RANKING_ENTRY_SLOTS)
            }
            _ => 0,
        }
    }

    fn skin_scroll_offset_for(&self, tab: ResultRankingTab) -> usize {
        match tab {
            ResultRankingTab::Global => self.global_skin_scroll_offset,
            ResultRankingTab::SelfAndRivals => self.self_and_rivals_skin_scroll_offset,
        }
    }

    fn set_skin_scroll_offset(&mut self, offset: usize) {
        match self.active_tab {
            ResultRankingTab::Global => self.global_skin_scroll_offset = offset,
            ResultRankingTab::SelfAndRivals => self.self_and_rivals_skin_scroll_offset = offset,
        }
    }

    fn scope_slot(&mut self, scope: IrRankingScope) -> Option<&mut RankingLoadState> {
        match scope {
            IrRankingScope::Global => Some(&mut self.global),
            IrRankingScope::SelfAndRivals => Some(&mut self.self_and_rivals),
            _ => None,
        }
    }

    fn loaded_chart_ranking(
        &self,
        ranking: &ResultIrRanking,
    ) -> Option<ResultIrLoadedChartRanking> {
        let ResultIrTarget::Chart { chart_sha256_hex, .. } = &self.query.target else {
            return None;
        };
        Some(ResultIrLoadedChartRanking {
            chart_sha256_hex: chart_sha256_hex.clone(),
            ranking: ranking.clone(),
        })
    }
}

mod snapshot;
mod task;

pub use snapshot::ranking_to_ir_snapshot;
pub(crate) use snapshot::{
    course_ranking_to_result_ir_ranking, result_ir_ranking_to_skin_snapshot,
};
pub(crate) use task::fetch_ranking_with_limit;
pub use task::{spawn_course_result_ir_task, spawn_result_ir_task};

use snapshot::*;
use task::*;
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::mpsc::channel;

    use bmz_gameplay::rule::RuleMode;

    use crate::ir::sync::{IrIncludedRanking, IrSyncReport};
    use crate::ir::types::{
        IrCourseRankingBody, IrCourseRankingCourseRef, IrCourseRankingEntry, IrCourseRankingResult,
        IrCourseRankingScore, IrRankingBody, IrRankingChartRef, IrRankingEntry,
        IrRankingPagination, IrRankingPlayer, IrRankingResult, IrRankingScope, IrRankingScore,
        IrRankingSelfRef, IrScopedRankingResponse, IrSubmitResponse,
    };
    use crate::ln_policy::LnScorePolicy;
    use crate::select_options::DoubleOptionScoreBucket;
    use crate::storage::network_db::IrJobKind;

    use super::{
        IrSubmitState, RankingLoadState, ResultIrEvent, ResultIrProviderSubmission,
        ResultIrRanking, ResultIrRankingEntry, ResultIrState, ResultIrTarget, ResultIrTaskQuery,
        ResultRankingTab, course_ranking_to_result_ir_ranking, included_global_ranking_for_query,
        included_global_ranking_from_response, ranking_to_ir_snapshot,
        result_ir_ranking_to_skin_snapshot_at, result_ranking_limit,
    };

    #[test]
    fn chart_result_ranking_limits_default_to_one_hundred() {
        assert_eq!(result_ranking_limit("bmz-official"), 100);
        assert_eq!(result_ranking_limit(crate::ir::rian_ir::RIAN_IR_PROVIDER), 100);
    }

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
                        score_id: Some("score-1".to_string()),
                        clear: "Perfect".to_string(),
                        ex_score: 46,
                        max_combo: 28,
                        min_bp: 0,
                        min_cb: 0,
                        gauge: Some("Groove".to_string()),
                        arrange_1p: Some("Normal".to_string()),
                        arrange_2p: None,
                        random_seed: None,
                        double_option: None,
                        verification: Some("verified_play".to_string()),
                        judges: None,
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
        assert_eq!(
            snapshot.entries[0].clear_index,
            Some(i64::from(bmz_core::clear::ClearType::Perfect as u8))
        );
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
    fn ranking_snapshot_scrolls_the_ten_visible_skin_rows() {
        let ranking = ResultIrRanking {
            scope: IrRankingScope::Global,
            entries: (1..=15)
                .map(|rank| ResultIrRankingEntry {
                    rank,
                    player_name: format!("player-{rank}"),
                    ex_score: rank * 10,
                    clear: "Normal".to_string(),
                    bp: 0,
                    max_combo: 0,
                })
                .collect(),
            clear_rate: None,
            self_rank: None,
            previous_rank: Some(18),
            total: Some(15),
        };

        let snapshot = result_ir_ranking_to_skin_snapshot_at(&ranking, 3);

        assert_eq!(snapshot.scroll_offset, 3);
        assert_eq!(snapshot.scroll_max, 5);
        assert_eq!(snapshot.previous_rank, Some(18));
        assert_eq!(snapshot.entries[0].rank, Some(4));
        assert_eq!(snapshot.entries[9].rank, Some(13));
    }

    #[test]
    fn active_tab_uses_its_own_skin_rows_and_scroll_offset() {
        let ranking = |scope, first_rank, count| ResultIrRanking {
            scope,
            entries: (first_rank..first_rank + count)
                .map(|rank| ResultIrRankingEntry {
                    rank,
                    player_name: format!("player-{rank}"),
                    ex_score: rank * 10,
                    clear: "Normal".to_string(),
                    bp: 0,
                    max_combo: 0,
                })
                .collect(),
            clear_rate: None,
            self_rank: None,
            previous_rank: None,
            total: Some(count),
        };
        let (sender, receiver) = channel::<ResultIrEvent>();
        let event_sender = sender.clone();
        let mut state = ResultIrState {
            submit: IrSubmitState::Sending,
            provider_submissions: vec![
                ResultIrProviderSubmission {
                    provider_key: "bmz-official".to_string(),
                    display_name: "BMZ IR".to_string(),
                    primary: true,
                    state: IrSubmitState::Sending,
                },
                ResultIrProviderSubmission {
                    provider_key: crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string(),
                    display_name: "rianIR".to_string(),
                    primary: false,
                    state: IrSubmitState::Sending,
                },
            ],
            global: RankingLoadState::Loaded(ranking(IrRankingScope::Global, 1, 15)),
            self_and_rivals: RankingLoadState::Loaded(ranking(
                IrRankingScope::SelfAndRivals,
                101,
                12,
            )),
            active_tab: ResultRankingTab::Global,
            ir_connect_begin_at: None,
            ir_connect_success_at: None,
            ir_connect_fail_at: None,
            provider_name: bmz_render::scene::ResultIrRankingName::default(),
            user_name: bmz_render::scene::ResultIrRankingName::default(),
            global_skin_scroll_offset: 3,
            self_and_rivals_skin_scroll_offset: 1,
            query: ResultIrTaskQuery {
                profile_root: PathBuf::new(),
                provider: "bmz-official".to_string(),
                account_id: "account-1".to_string(),
                base_url: "https://ir.example.test".to_string(),
                target: ResultIrTarget::Chart {
                    local_score_id: 1,
                    chart_sha256_hex: "chart".to_string(),
                    ln_policy: LnScorePolicy::AutoLn,
                    double_option: DoubleOptionScoreBucket::Off,
                    rule_mode: RuleMode::Beatoraja,
                },
            },
            sender,
            receiver,
        };

        assert_eq!(state.skin_snapshot().entries[0].rank, Some(4));

        state.active_tab = ResultRankingTab::SelfAndRivals;
        // 既存スキンの standard ref は active tab が変わっても global のまま。
        assert_eq!(state.skin_snapshot().entries[0].rank, Some(4));
        assert_eq!(
            state.skin_snapshot_for_binding(bmz_render::skin::ResultIrScopeBinding::Active).entries
                [0]
            .rank,
            Some(102)
        );

        state.set_skin_scroll_rate(1.0);
        assert_eq!(
            state
                .skin_snapshot_for_binding(bmz_render::skin::ResultIrScopeBinding::Active)
                .scroll_offset,
            2
        );
        state.active_tab = ResultRankingTab::Global;
        assert_eq!(state.skin_snapshot().scroll_offset, 3);

        assert!(state.scroll_skin_rows(1));
        assert_eq!(state.skin_snapshot().scroll_offset, 4);
        assert!(state.scroll_skin_rows(-100));
        assert_eq!(state.skin_snapshot().scroll_offset, 0);
        assert!(!state.scroll_skin_rows(-1));

        state.global = RankingLoadState::Loading;
        assert!(!state.scroll_skin_rows(1));
        assert_eq!(state.skin_snapshot().scroll_offset, 0);

        event_sender
            .send(ResultIrEvent::Ranking {
                provider: crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string(),
                scope: IrRankingScope::Global,
                result: Ok(ranking(IrRankingScope::Global, 201, 1)),
            })
            .unwrap();
        event_sender
            .send(ResultIrEvent::Submit {
                provider: crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string(),
                submitted: 1,
                failed: 0,
                message: None,
            })
            .unwrap();

        assert!(state.poll().is_empty());
        assert!(matches!(state.global, RankingLoadState::Loading));
        assert_eq!(state.submit, IrSubmitState::Sending);
        assert_eq!(
            state.provider_submissions[1].state,
            IrSubmitState::Done { submitted: 1, failed: 0, message: None }
        );

        event_sender
            .send(ResultIrEvent::Ranking {
                provider: "bmz-official".to_string(),
                scope: IrRankingScope::Global,
                result: Ok(ranking(IrRankingScope::Global, 301, 1)),
            })
            .unwrap();
        let loaded = state.poll();
        assert_eq!(loaded.len(), 1);
        assert!(matches!(state.global, RankingLoadState::Loaded(_)));
    }

    #[test]
    fn included_global_ranking_uses_only_current_result_attempt() {
        let query = ResultIrTaskQuery {
            profile_root: PathBuf::new(),
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            base_url: "https://ir.example.test".to_string(),
            target: ResultIrTarget::Chart {
                local_score_id: 2,
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
                IrIncludedRanking {
                    provider: "bmz-official".to_string(),
                    account_id: "account-1".to_string(),
                    kind: IrJobKind::Score,
                    local_score_id: 1,
                    previous_rank: None,
                    ranking: IrRankingResult {
                        chart: IrRankingChartRef { sha256: "current".to_string() },
                        ranking: IrRankingBody {
                            scope: IrRankingScope::Global,
                            entries: Vec::new(),
                            clear_rate: Some(25),
                            self_summary: None,
                            pagination: None,
                        },
                    },
                },
                IrIncludedRanking {
                    provider: "bmz-official".to_string(),
                    account_id: "account-1".to_string(),
                    kind: IrJobKind::Course,
                    local_score_id: 2,
                    previous_rank: None,
                    ranking: IrRankingResult {
                        chart: IrRankingChartRef { sha256: "current".to_string() },
                        ranking: IrRankingBody {
                            scope: IrRankingScope::Global,
                            entries: Vec::new(),
                            clear_rate: Some(50),
                            self_summary: None,
                            pagination: None,
                        },
                    },
                },
                IrIncludedRanking {
                    provider: "bmz-official".to_string(),
                    account_id: "account-1".to_string(),
                    kind: IrJobKind::Score,
                    local_score_id: 2,
                    previous_rank: Some(9),
                    ranking: IrRankingResult {
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
                },
            ],
        };

        let ranking = included_global_ranking_for_query(&query, &report).unwrap();

        assert_eq!(ranking.scope, IrRankingScope::Global);
        assert_eq!(ranking.clear_rate, Some(75));
        assert_eq!(ranking.previous_rank, Some(9));
        assert_eq!(ranking.total, Some(2));
    }

    #[test]
    fn stored_submission_response_restores_previous_rank_for_current_attempt() {
        let query = ResultIrTaskQuery {
            profile_root: PathBuf::new(),
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            base_url: "https://ir.example.test".to_string(),
            target: ResultIrTarget::Chart {
                local_score_id: 2,
                chart_sha256_hex: "current".to_string(),
                ln_policy: LnScorePolicy::AutoLn,
                double_option: DoubleOptionScoreBucket::Off,
                rule_mode: RuleMode::Beatoraja,
            },
        };
        let response = IrSubmitResponse {
            accepted: true,
            score_id: Some("remote-2".to_string()),
            best_updated: true,
            previous_best: None,
            rankings: BTreeMap::from([(
                IrRankingScope::Global,
                IrScopedRankingResponse {
                    succeeded: true,
                    previous_rank: Some(9),
                    data: Some(IrRankingResult {
                        chart: IrRankingChartRef { sha256: "current".to_string() },
                        ranking: IrRankingBody {
                            scope: IrRankingScope::Global,
                            entries: Vec::new(),
                            clear_rate: Some(75),
                            self_summary: None,
                            pagination: None,
                        },
                    }),
                    error: None,
                },
            )]),
        };

        let ranking = included_global_ranking_from_response(&query, &response).unwrap();

        assert_eq!(ranking.previous_rank, Some(9));
        assert_eq!(ranking.clear_rate, Some(75));
    }

    #[test]
    fn chart_result_target_rejects_previous_retry_with_same_chart_hash() {
        let target = ResultIrTarget::Chart {
            local_score_id: 42,
            chart_sha256_hex: "same-chart".to_string(),
            ln_policy: LnScorePolicy::AutoLn,
            double_option: DoubleOptionScoreBucket::Off,
            rule_mode: RuleMode::Beatoraja,
        };

        assert!(target.matches_chart_result(
            42,
            "same-chart",
            LnScorePolicy::AutoLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
        ));
        assert!(!target.matches_chart_result(
            41,
            "same-chart",
            LnScorePolicy::AutoLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
        ));
    }
}
