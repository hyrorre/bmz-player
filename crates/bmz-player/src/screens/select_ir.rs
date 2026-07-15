//! 選曲画面用の IR ランキング遅延取得キャッシュ。
//!
//! beatoraja の `MusicSelector` + `RankingData` 相当。カーソルが曲行に
//! 一定時間とどまったらグローバルランキングを取得し、`NUMBER_IR_RANK` /
//! `NUMBER_IR_TOTALPLAYER` / `OPTION_IR_*` skin property へ供給する。

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::{ResultIrSnapshot, ResultIrState as SkinIrState, SelectRivalSnapshot};

use crate::config::profile_config::IrConfig;
use crate::ir::types::{IrRankingResult, IrRankingScope};
use crate::ln_policy::LnScorePolicy;
use crate::screens::result_ir::{
    ResultIrQuery, ResultIrRanking, ranking_to_ir_snapshot, result_ir_ranking_to_skin_snapshot,
};
use crate::select_options::DoubleOptionScoreBucket;
use crate::select_options::TargetOption;
use crate::storage::common::{hash_to_hex, hex_to_hash};

/// カーソルがとどまってから取得を始めるまでの待ち時間。
/// 連打スクロールで全行を取得しに行かないためのデバウンス。
const FETCH_DEBOUNCE: Duration = Duration::from_millis(400);
/// キャッシュ上限。超えたら全クリアして作り直す (LRU は持たない)。
const CACHE_CAPACITY: usize = 256;

type FetchResult =
    (String, [u8; 32], Instant, Result<(IrRankingResult, Option<IrRankingResult>), String>);

/// カーソル譜面ごとのキャッシュ済み IR 表示データ。
#[derive(Debug, Clone)]
struct CachedChartIr {
    ir: ResultIrSnapshot,
    rival: Option<SelectRivalSnapshot>,
    global_ex_scores: Vec<u32>,
    rival_ex_scores: Vec<u32>,
    completed_at: Instant,
}

pub struct SelectIrRanking {
    cache: HashMap<[u8; 32], CachedChartIr>,
    in_flight: Option<(String, [u8; 32], Instant)>,
    pending: Option<([u8; 32], Instant)>,
    /// キャッシュが前提とするランキング条件 (rule mode / 解決済み score key)。
    /// 変わったらキャッシュごと破棄する。
    context: String,
    sender: Sender<FetchResult>,
    receiver: Receiver<FetchResult>,
}

impl Default for SelectIrRanking {
    fn default() -> Self {
        let (sender, receiver) = channel();
        Self {
            cache: HashMap::new(),
            in_flight: None,
            pending: None,
            context: String::new(),
            sender,
            receiver,
        }
    }
}

impl SelectIrRanking {
    /// 毎フレーム呼ぶ。取得完了の取り込みと、カーソル譜面の取得予約を行う。
    /// `context` は rule mode / 解決済み LN policy / DOUBLE など
    /// ランキング条件を表す文字列。変わったらキャッシュを破棄する。
    pub fn update(
        &mut self,
        ir_config: &IrConfig,
        profile_root: &Path,
        context: &str,
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        rule_mode: RuleMode,
        selected: Option<[u8; 32]>,
    ) {
        if self.context != context {
            self.context = context.to_string();
            self.clear();
        }
        while let Ok((result_context, sha256, requested_at, result)) = self.receiver.try_recv() {
            if self.in_flight.as_ref().is_some_and(|(context, in_flight_sha, _)| {
                context == &result_context && *in_flight_sha == sha256
            }) {
                self.in_flight = None;
            }
            if result_context != self.context {
                tracing::debug!(
                    old_context = %result_context,
                    current_context = %self.context,
                    "discarding stale select IR ranking fetch"
                );
                continue;
            }
            if self.cache.get(&sha256).is_some_and(|entry| entry.completed_at >= requested_at) {
                tracing::debug!("discarding stale select IR ranking fetch");
                continue;
            }
            let completed_at = Instant::now();
            let entry = match result {
                Ok((global, rivals)) => CachedChartIr {
                    ir: ranking_to_ir_snapshot(&global),
                    rival: rivals.as_ref().and_then(top_rival_snapshot),
                    global_ex_scores: ranking_ex_scores(&global),
                    rival_ex_scores: rivals.as_ref().map(ranking_ex_scores).unwrap_or_default(),
                    completed_at,
                },
                Err(error) => {
                    tracing::debug!(%error, "select IR ranking fetch failed");
                    CachedChartIr {
                        ir: ResultIrSnapshot { state: SkinIrState::Failed, ..Default::default() },
                        rival: None,
                        global_ex_scores: Vec::new(),
                        rival_ex_scores: Vec::new(),
                        completed_at,
                    }
                }
            };
            self.insert_entry(sha256, entry);
        }

        let Some(provider) = enabled_provider(ir_config) else {
            return;
        };
        let Some(sha256) = selected else {
            self.pending = None;
            return;
        };
        if self.cache.contains_key(&sha256)
            || self.in_flight.as_ref().is_some_and(|(_, in_flight_sha, _)| *in_flight_sha == sha256)
        {
            self.pending = None;
            return;
        }
        match self.pending {
            Some((pending_sha, since)) if pending_sha == sha256 => {
                if since.elapsed() >= FETCH_DEBOUNCE && self.in_flight.is_none() {
                    self.pending = None;
                    let requested_at = Instant::now();
                    self.in_flight = Some((self.context.clone(), sha256, requested_at));
                    spawn_fetch(
                        ResultIrQuery {
                            profile_root: profile_root.to_path_buf(),
                            provider: provider.0,
                            base_url: provider.1,
                            chart_sha256_hex: hash_to_hex(&sha256),
                            ln_policy,
                            double_option,
                            rule_mode,
                        },
                        self.context.clone(),
                        sha256,
                        requested_at,
                        self.sender.clone(),
                    );
                }
            }
            _ => self.pending = Some((sha256, Instant::now())),
        }
    }

    /// Result 画面でスコア送信と同時に取得した Global ranking を選曲キャッシュへ反映する。
    pub fn cache_result_global_ranking(
        &mut self,
        chart_sha256_hex: &str,
        ranking: &ResultIrRanking,
    ) {
        if ranking.scope != IrRankingScope::Global {
            return;
        }
        let Ok(sha256) = hex_to_hash::<32>(chart_sha256_hex) else {
            tracing::warn!(
                chart = chart_sha256_hex,
                "discarding IR ranking for invalid chart hash"
            );
            return;
        };
        let (rival, rival_ex_scores) = self
            .cache
            .get(&sha256)
            .map(|entry| (entry.rival.clone(), entry.rival_ex_scores.clone()))
            .unwrap_or_default();
        self.insert_entry(
            sha256,
            CachedChartIr {
                ir: result_ir_ranking_to_skin_snapshot(ranking),
                rival,
                global_ex_scores: result_ranking_ex_scores(ranking),
                rival_ex_scores,
                completed_at: Instant::now(),
            },
        );
    }

    /// 選択中譜面のスキン用 snapshot。IR 未設定なら Offline。
    pub fn snapshot_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
    ) -> ResultIrSnapshot {
        if enabled_provider(ir_config).is_none() {
            return ResultIrSnapshot::default();
        }
        let Some(sha256) = selected else {
            return ResultIrSnapshot::default();
        };
        self.cache
            .get(&sha256)
            .map(|entry| {
                let mut snapshot = entry.ir.clone();
                match snapshot.state {
                    SkinIrState::Loaded => {
                        snapshot.connect_success_ms = Some(elapsed_since_ms(entry.completed_at));
                    }
                    SkinIrState::Failed => {
                        snapshot.connect_fail_ms = Some(elapsed_since_ms(entry.completed_at));
                    }
                    _ => {}
                }
                snapshot
            })
            .unwrap_or_else(|| {
                let begin_ms = self
                    .in_flight
                    .as_ref()
                    .filter(|(_, in_flight_sha, _)| *in_flight_sha == sha256)
                    .map(|(_, _, started_at)| elapsed_since_ms(*started_at));
                ResultIrSnapshot {
                    state: if begin_ms.is_some() {
                        SkinIrState::Loading
                    } else {
                        SkinIrState::Waiting
                    },
                    connect_begin_ms: begin_ms,
                    ..Default::default()
                }
            })
    }

    /// 選択中譜面のライバルベスト (最上位 1 名)。未取得 / IR 未設定なら None。
    pub fn rival_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
    ) -> Option<SelectRivalSnapshot> {
        enabled_provider(ir_config)?;
        self.cache.get(&selected?).and_then(|entry| entry.rival.clone())
    }

    pub fn target_ex_score_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
        target: TargetOption,
        local_best_ex_score: Option<u32>,
    ) -> Option<u32> {
        enabled_provider(ir_config)?;
        let entry = self.cache.get(&selected?)?;
        match target {
            TargetOption::IrTop => entry.global_ex_scores.first().copied(),
            TargetOption::IrNext => {
                next_ex_score_above(&entry.global_ex_scores, local_best_ex_score.unwrap_or(0))
            }
            TargetOption::RivalTop => entry.rival_ex_scores.first().copied(),
            TargetOption::RivalNext => {
                next_ex_score_above(&entry.rival_ex_scores, local_best_ex_score.unwrap_or(0))
            }
            TargetOption::RivalIndex(index) => {
                entry.rival_ex_scores.get(index.saturating_sub(1) as usize).copied()
            }
            _ => None,
        }
    }

    /// ログイン状態が変わったとき等にキャッシュを破棄する。
    pub fn clear(&mut self) {
        self.cache.clear();
        self.in_flight = None;
        self.pending = None;
    }

    fn insert_entry(&mut self, sha256: [u8; 32], entry: CachedChartIr) {
        if self.cache.len() >= CACHE_CAPACITY && !self.cache.contains_key(&sha256) {
            self.cache.clear();
        }
        self.cache.insert(sha256, entry);
        if self.in_flight.as_ref().is_some_and(|(_, in_flight_sha, _)| *in_flight_sha == sha256) {
            self.in_flight = None;
        }
        if self.pending.as_ref().is_some_and(|(pending_sha, _)| *pending_sha == sha256) {
            self.pending = None;
        }
    }
}

fn enabled_provider(ir_config: &IrConfig) -> Option<(String, String)> {
    ir_config
        .providers
        .iter()
        .find(|provider| {
            provider.enabled
                && !provider.base_url.is_empty()
                && crate::ir::provider_key::configured_provider_key(provider).is_some()
        })
        .map(|provider| {
            (
                crate::ir::provider_key::configured_provider_key(provider).unwrap().to_string(),
                provider.base_url.clone(),
            )
        })
}

fn spawn_fetch(
    query: ResultIrQuery,
    context: String,
    sha256: [u8; 32],
    requested_at: Instant,
    sender: Sender<FetchResult>,
) {
    tracing::debug!(chart = %query.chart_sha256_hex, "fetching select IR ranking");
    tokio::spawn(async move {
        let result = async {
            let global =
                crate::screens::result_ir::fetch_ranking(&query, IrRankingScope::Global).await?;
            // rivals scope は要認証。未ログイン等で失敗してもライバル表示を
            // 諦めるだけで、グローバルランキング表示は維持する。
            let rivals =
                crate::screens::result_ir::fetch_ranking(&query, IrRankingScope::Rivals).await.ok();
            anyhow::Ok((global, rivals))
        }
        .await
        .map_err(|error| format!("{error:#}"));
        let _ = sender.send((context, sha256, requested_at, result));
    });
}

/// rivals scope ランキングの先頭 (ライバル中ベスト) をスキン用に変換する。
fn top_rival_snapshot(rivals: &IrRankingResult) -> Option<SelectRivalSnapshot> {
    let entry = rivals.ranking.entries.first()?;
    Some(SelectRivalSnapshot {
        display_name: entry.player.display_name.clone(),
        ex_score: entry.score.ex_score,
        max_combo: entry.score.max_combo,
        bp: entry.score.min_bp,
    })
}

fn ranking_ex_scores(ranking: &IrRankingResult) -> Vec<u32> {
    ranking.ranking.entries.iter().map(|entry| entry.score.ex_score).collect()
}

fn result_ranking_ex_scores(ranking: &ResultIrRanking) -> Vec<u32> {
    ranking.entries.iter().map(|entry| entry.ex_score).collect()
}

fn elapsed_since_ms(started_at: Instant) -> i32 {
    started_at.elapsed().as_millis().min(i32::MAX as u128) as i32
}

fn next_ex_score_above(scores_desc: &[u32], current_ex_score: u32) -> Option<u32> {
    if scores_desc.is_empty() {
        return None;
    }
    for (index, &score) in scores_desc.iter().enumerate() {
        if score <= current_ex_score {
            return Some(scores_desc[index.saturating_sub(1)]);
        }
    }
    scores_desc.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{
        IrProviderConfig, IrProviderRoleConfig, IrSendPolicyConfig,
    };
    use crate::ir::types::{
        IrRankingBody, IrRankingChartRef, IrRankingEntry, IrRankingPagination, IrRankingPlayer,
        IrRankingScore, IrRankingSelfRef,
    };
    use crate::screens::result_ir::ResultIrRankingEntry;

    fn ir_config(enabled: bool) -> IrConfig {
        IrConfig {
            primary_provider: "bmz-official".to_string(),
            providers: vec![IrProviderConfig {
                provider: "bmz-official".to_string(),
                provider_key: "bmz-official".to_string(),
                base_url: "http://localhost:0".to_string(),
                enabled,
                account_display_name: String::new(),
                account_id: String::new(),
                send_policy: IrSendPolicyConfig::default(),
                role: IrProviderRoleConfig::default(),
                last_login_at: None,
                last_success_at: None,
            }],
            ..IrConfig::default()
        }
    }

    fn result_global_ranking(rank: u32, ex_score: u32, total: u32) -> ResultIrRanking {
        ResultIrRanking {
            scope: IrRankingScope::Global,
            entries: vec![ResultIrRankingEntry {
                rank,
                player_name: "player".to_string(),
                ex_score,
                clear: "Hard".to_string(),
                bp: 2,
                max_combo: 300,
            }],
            clear_rate: Some(80),
            self_rank: Some(rank),
            total: Some(total),
        }
    }

    fn raw_global_ranking(
        sha256: [u8; 32],
        rank: u32,
        ex_score: u32,
        total: u32,
    ) -> IrRankingResult {
        IrRankingResult {
            chart: IrRankingChartRef { sha256: hash_to_hex(&sha256) },
            ranking: IrRankingBody {
                scope: IrRankingScope::Global,
                entries: vec![IrRankingEntry {
                    rank,
                    scope_rank: None,
                    player: IrRankingPlayer {
                        id: "player".to_string(),
                        display_name: "player".to_string(),
                    },
                    score: IrRankingScore {
                        clear: "Hard".to_string(),
                        ex_score,
                        max_combo: 300,
                        min_bp: 2,
                        min_cb: 2,
                        device_type: None,
                        played_at: None,
                    },
                }],
                clear_rate: Some(80),
                self_summary: Some(IrRankingSelfRef { rank, score_id: None }),
                pagination: Some(IrRankingPagination {
                    limit: 20,
                    offset: 0,
                    total: Some(total),
                    has_more: false,
                }),
            },
        }
    }

    #[test]
    fn snapshot_is_offline_without_provider_and_waiting_when_uncached() {
        let select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];

        let offline = select_ir.snapshot_for(&ir_config(false), Some(sha));
        assert_eq!(offline.state, SkinIrState::Offline);

        let waiting = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(waiting.state, SkinIrState::Waiting);

        let none = select_ir.snapshot_for(&ir_config(true), None);
        assert_eq!(none.state, SkinIrState::Offline);
    }

    #[test]
    fn cached_snapshot_is_returned() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        select_ir.cache.insert(
            sha,
            CachedChartIr {
                ir: ResultIrSnapshot {
                    state: SkinIrState::Loaded,
                    rank: Some(2),
                    total_player: Some(10),
                    clear_rate: None,
                    previous_rank: None,
                    ..Default::default()
                },
                rival: Some(SelectRivalSnapshot {
                    display_name: "RivalOne".to_string(),
                    ex_score: 1500,
                    max_combo: 700,
                    bp: 12,
                }),
                global_ex_scores: vec![1800, 1600, 1400],
                rival_ex_scores: vec![1500, 1200],
                completed_at: Instant::now(),
            },
        );

        let snapshot = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(snapshot.state, SkinIrState::Loaded);
        assert_eq!(snapshot.rank, Some(2));
        let rival = select_ir.rival_for(&ir_config(true), Some(sha)).unwrap();
        assert_eq!(rival.display_name, "RivalOne");
        assert_eq!(rival.ex_score, 1500);
        assert_eq!(
            select_ir.target_ex_score_for(&ir_config(true), Some(sha), TargetOption::IrTop, None),
            Some(1800)
        );
        assert_eq!(
            select_ir.target_ex_score_for(
                &ir_config(true),
                Some(sha),
                TargetOption::IrNext,
                Some(1500)
            ),
            Some(1600)
        );
        assert_eq!(
            select_ir.target_ex_score_for(
                &ir_config(true),
                Some(sha),
                TargetOption::RivalIndex(2),
                Some(1500)
            ),
            Some(1200)
        );
        assert!(select_ir.rival_for(&ir_config(false), Some(sha)).is_none());

        select_ir.clear();
        let cleared = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(cleared.state, SkinIrState::Waiting);
    }

    #[test]
    fn result_global_ranking_updates_cached_snapshot() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        select_ir.cache.insert(
            sha,
            CachedChartIr {
                ir: ResultIrSnapshot {
                    state: SkinIrState::Loaded,
                    rank: Some(9),
                    total_player: Some(10),
                    previous_rank: None,
                    ..Default::default()
                },
                rival: Some(SelectRivalSnapshot {
                    display_name: "RivalOne".to_string(),
                    ex_score: 1500,
                    max_combo: 700,
                    bp: 12,
                }),
                global_ex_scores: vec![1200],
                rival_ex_scores: vec![1500],
                completed_at: Instant::now(),
            },
        );

        select_ir
            .cache_result_global_ranking(&hash_to_hex(&sha), &result_global_ranking(1, 2000, 3));

        let snapshot = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(snapshot.state, SkinIrState::Loaded);
        assert_eq!(snapshot.rank, Some(1));
        assert_eq!(snapshot.total_player, Some(3));
        assert_eq!(
            select_ir.target_ex_score_for(&ir_config(true), Some(sha), TargetOption::IrTop, None),
            Some(2000)
        );
        let rival = select_ir.rival_for(&ir_config(true), Some(sha)).unwrap();
        assert_eq!(rival.display_name, "RivalOne");
    }

    #[test]
    fn update_debounces_before_fetching() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(true);
        let root = std::env::temp_dir();

        // 1回目はデバウンス予約のみで取得を開始しない。
        select_ir.update(
            &config,
            &root,
            "ctx",
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
            Some(sha),
        );
        assert!(select_ir.in_flight.is_none());
        assert!(select_ir.pending.is_some());
        assert_eq!(select_ir.snapshot_for(&config, Some(sha)).state, SkinIrState::Waiting);

        select_ir.in_flight = Some(("ctx".to_string(), sha, Instant::now()));
        assert_eq!(select_ir.snapshot_for(&config, Some(sha)).state, SkinIrState::Loading);
        select_ir.in_flight = None;

        // 選択が外れたら予約は破棄。
        select_ir.update(
            &config,
            &root,
            "ctx",
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
            None,
        );
        assert!(select_ir.pending.is_none());
    }

    #[test]
    fn stale_fetch_result_is_discarded_after_context_change() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(false);
        let root = std::env::temp_dir();

        select_ir.context = "new".to_string();
        select_ir.in_flight = Some(("old".to_string(), sha, Instant::now()));
        select_ir
            .sender
            .send(("old".to_string(), sha, Instant::now(), Err("stale".to_string())))
            .unwrap();

        select_ir.update(
            &config,
            &root,
            "new",
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
            Some(sha),
        );
        assert!(!select_ir.cache.contains_key(&sha));
        assert!(select_ir.in_flight.is_none());
    }

    #[test]
    fn stale_fetch_result_does_not_override_newer_result_cache() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(false);
        let root = std::env::temp_dir();
        let requested_at = Instant::now();

        select_ir.context = "ctx".to_string();
        select_ir.in_flight = Some(("ctx".to_string(), sha, requested_at));
        select_ir
            .cache_result_global_ranking(&hash_to_hex(&sha), &result_global_ranking(1, 2000, 3));
        select_ir
            .sender
            .send((
                "ctx".to_string(),
                sha,
                requested_at,
                Ok((raw_global_ranking(sha, 9, 1200, 10), None)),
            ))
            .unwrap();

        select_ir.update(
            &config,
            &root,
            "ctx",
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
            Some(sha),
        );

        let snapshot = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(snapshot.rank, Some(1));
        assert_eq!(
            select_ir.target_ex_score_for(&ir_config(true), Some(sha), TargetOption::IrTop, None),
            Some(2000)
        );
        assert!(select_ir.in_flight.is_none());
    }
}
