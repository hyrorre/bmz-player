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
use crate::screens::result_ir::{ResultIrQuery, ranking_to_ir_snapshot};
use crate::select_options::DoubleOptionScoreBucket;
use crate::select_options::TargetOption;
use crate::storage::common::hash_to_hex;

/// カーソルがとどまってから取得を始めるまでの待ち時間。
/// 連打スクロールで全行を取得しに行かないためのデバウンス。
const FETCH_DEBOUNCE: Duration = Duration::from_millis(400);
/// キャッシュ上限。超えたら全クリアして作り直す (LRU は持たない)。
const CACHE_CAPACITY: usize = 256;

type FetchResult = (String, [u8; 32], Result<(IrRankingResult, Option<IrRankingResult>), String>);

/// カーソル譜面ごとのキャッシュ済み IR 表示データ。
#[derive(Debug, Clone)]
struct CachedChartIr {
    ir: ResultIrSnapshot,
    rival: Option<SelectRivalSnapshot>,
    global_ex_scores: Vec<u32>,
    rival_ex_scores: Vec<u32>,
}

pub struct SelectIrRanking {
    cache: HashMap<[u8; 32], CachedChartIr>,
    in_flight: Option<(String, [u8; 32])>,
    pending: Option<([u8; 32], Instant)>,
    /// キャッシュが前提とするランキング条件 (rule mode / LN ポリシー設定)。
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
    /// `context` は rule mode / LN ポリシー設定など「ランキング条件を決める
    /// プロファイル設定」を表す文字列。変わったらキャッシュを破棄する。
    /// (譜面ごとに解決される `ln_policy` 自体は含めないこと)
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
        while let Ok((result_context, sha256, result)) = self.receiver.try_recv() {
            if self.in_flight.as_ref().is_some_and(|(context, in_flight_sha)| {
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
            if self.cache.len() >= CACHE_CAPACITY {
                self.cache.clear();
            }
            let entry = match result {
                Ok((global, rivals)) => CachedChartIr {
                    ir: ranking_to_ir_snapshot(&global),
                    rival: rivals.as_ref().and_then(top_rival_snapshot),
                    global_ex_scores: ranking_ex_scores(&global),
                    rival_ex_scores: rivals.as_ref().map(ranking_ex_scores).unwrap_or_default(),
                },
                Err(error) => {
                    tracing::debug!(%error, "select IR ranking fetch failed");
                    CachedChartIr {
                        ir: ResultIrSnapshot { state: SkinIrState::Failed, ..Default::default() },
                        rival: None,
                        global_ex_scores: Vec::new(),
                        rival_ex_scores: Vec::new(),
                    }
                }
            };
            self.cache.insert(sha256, entry);
        }

        let Some(provider) = enabled_provider(ir_config) else {
            return;
        };
        let Some(sha256) = selected else {
            self.pending = None;
            return;
        };
        if self.cache.contains_key(&sha256)
            || self.in_flight.as_ref().is_some_and(|(_, in_flight_sha)| *in_flight_sha == sha256)
        {
            self.pending = None;
            return;
        }
        match self.pending {
            Some((pending_sha, since)) if pending_sha == sha256 => {
                if since.elapsed() >= FETCH_DEBOUNCE && self.in_flight.is_none() {
                    self.pending = None;
                    self.in_flight = Some((self.context.clone(), sha256));
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
                        self.sender.clone(),
                    );
                }
            }
            _ => self.pending = Some((sha256, Instant::now())),
        }
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
            .map(|entry| entry.ir.clone())
            .unwrap_or(ResultIrSnapshot { state: SkinIrState::Loading, ..Default::default() })
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
}

fn enabled_provider(ir_config: &IrConfig) -> Option<(String, String)> {
    ir_config
        .providers
        .iter()
        .find(|provider| provider.enabled && !provider.base_url.is_empty())
        .map(|provider| (provider.provider.clone(), provider.base_url.clone()))
}

fn spawn_fetch(
    query: ResultIrQuery,
    context: String,
    sha256: [u8; 32],
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
        let _ = sender.send((context, sha256, result));
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

    fn ir_config(enabled: bool) -> IrConfig {
        IrConfig {
            primary_provider: "bmz-official".to_string(),
            providers: vec![IrProviderConfig {
                provider: "bmz-official".to_string(),
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

    #[test]
    fn snapshot_is_offline_without_provider_and_loading_when_uncached() {
        let select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];

        let offline = select_ir.snapshot_for(&ir_config(false), Some(sha));
        assert_eq!(offline.state, SkinIrState::Offline);

        let loading = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(loading.state, SkinIrState::Loading);

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
        assert_eq!(cleared.state, SkinIrState::Loading);
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
        select_ir.in_flight = Some(("old".to_string(), sha));
        select_ir.sender.send(("old".to_string(), sha, Err("stale".to_string()))).unwrap();

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
}
