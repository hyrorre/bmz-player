//! 選曲画面用の IR ランキング遅延取得キャッシュ。
//!
//! beatoraja の `MusicSelector` + `RankingData` 相当。カーソルが曲行に
//! 一定時間とどまったらグローバルランキングを取得し、`NUMBER_IR_RANK` /
//! `NUMBER_IR_TOTALPLAYER` / `OPTION_IR_*` skin property へ供給する。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::{
    ResultIrScope as SkinIrScope, ResultIrSnapshot, ResultIrState as SkinIrState,
    SelectRivalJudgeCounts, SelectRivalSnapshot,
};

use crate::config::profile_config::{
    IrConfig, IrProviderConfig, ProfileConfig, RivalEntry, RivalSourceConfig,
};
use crate::ir::ranking::ChartRankingQuery as ResultIrQuery;
use crate::ir::types::{IrCourseRankingResult, IrRankingResult, IrRankingScope};
use crate::ln_policy::LnScorePolicy;
use crate::screens::play_start::SeedScheme;
use crate::screens::result_ir::{
    ResultIrRanking, chart_ranking_to_result_ir_ranking, course_ranking_to_result_ir_ranking,
    ranking_to_ir_snapshot, result_ir_ranking_to_skin_snapshot,
    result_ir_ranking_to_skin_snapshot_at,
};
use crate::select_options::DoubleOptionScoreBucket;
use crate::select_options::{ResolvedTarget, TargetOption};
use crate::storage::common::{hash_to_hex, hex_to_hash};
use crate::storage::network_db::{BmzRivalScoreIdentity, IrRivalScoreRecord};

/// 選曲中にG-BATTLEの相手として選べるIRランキング行。
/// 描画用snapshotとは分離し、リプレイ取得に必要な識別情報を失わない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectIrBattleEntry {
    pub rank: u32,
    pub player_id: String,
    pub player_name: String,
    pub score_id: Option<String>,
    pub ex_score: u32,
    pub clear: String,
    pub bp: u32,
    pub max_combo: u32,
    pub gauge: Option<String>,
    pub verification: Option<String>,
    pub arrange_1p: Option<String>,
    pub arrange_2p: Option<String>,
    pub random_seed: Option<i64>,
    pub seed_scheme: SeedScheme,
    pub double_option: Option<String>,
}

/// カーソルがとどまってから取得を始めるまでの待ち時間。
/// 連打スクロールで全行を取得しに行かないためのデバウンス。
const FETCH_DEBOUNCE: Duration = Duration::from_millis(400);
/// キャッシュ上限。超えたら全クリアして作り直す (LRU は持たない)。
const CACHE_CAPACITY: usize = 256;

type FetchResult = (
    String,
    [u8; 32],
    Instant,
    Result<(IrRankingResult, Option<IrRankingResult>, Option<IrRankingResult>), String>,
);
type CourseFetchResult =
    (String, SelectCourseIrTarget, Instant, Result<IrCourseRankingResult, String>);
type RivalScoreKey = ([u8; 32], u8, Option<BmzRivalScoreIdentity>);
type RivalScores = HashMap<RivalScoreKey, IrRivalScoreRecord>;
type RivalFetchResult = (SelectRivalFetchTarget, Instant, Result<RivalScores, String>);

#[derive(PartialEq, Eq)]
enum RivalDiskState {
    Idle,
    Pending,
    Loading,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectRivalFetchTarget {
    pub provider: String,
    pub base_url: String,
    pub rival_id: String,
    pub display_name: String,
    pub body: String,
    pub rule_mode: RuleMode,
    pub bmz_account_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SelectRivalFetchKey {
    provider: String,
    base_url: String,
    rival_id: String,
    body: String,
    rule_mode: RuleMode,
}

impl From<&SelectRivalFetchTarget> for SelectRivalFetchKey {
    fn from(target: &SelectRivalFetchTarget) -> Self {
        Self {
            provider: target.provider.clone(),
            base_url: target.base_url.clone(),
            rival_id: target.rival_id.clone(),
            body: target.body.clone(),
            rule_mode: target.rule_mode,
        }
    }
}

impl SelectRivalFetchTarget {
    pub fn from_profile(profile: &ProfileConfig) -> Option<Self> {
        let provider = select_rival_provider(profile)?;
        let provider_key = crate::ir::provider_key::configured_provider_key(provider)?;
        let active = profile.rival.active_rival.trim();
        if active.is_empty() {
            return None;
        }
        let rival = selectable_rivals(profile, provider_key).find(|entry| entry.id == active)?;
        let rule_mode = profile.play.rule_mode;
        let bmz_account_id = crate::ir::bmz_official::is_bmz_rival_provider(provider)
            .then(|| provider.account_id.trim().to_string());
        let body = if let Some(account) = &bmz_account_id {
            format!("bmz-rivals-v1:{}:{account}:{}", rule_mode.as_str(), provider.base_url)
        } else {
            crate::ir::rian_ir::body_for_rule_mode(rule_mode).to_string()
        };
        Some(Self {
            provider: provider_key.to_string(),
            base_url: provider.base_url.clone(),
            rival_id: rival.ir_user_id.clone(),
            display_name: rival.display_name.clone(),
            body,
            rule_mode,
            bmz_account_id,
        })
    }
}

fn select_rival_provider(profile: &ProfileConfig) -> Option<&IrProviderConfig> {
    let provider = crate::ir::provider_key::primary_provider_config(&profile.ir)?;
    let supported = if crate::ir::bms_ir::is_bms_ir_config(provider) {
        profile.play.rule_mode != RuleMode::Dx
    } else if crate::ir::bmz_official::is_bmz_rival_provider(provider) {
        !provider.account_id.trim().is_empty()
    } else {
        crate::ir::rian_ir::is_rian_ir_config(provider)
    };
    supported.then_some(provider)
}

fn selectable_rivals<'a>(
    profile: &'a ProfileConfig,
    provider_key: &'a str,
) -> impl Iterator<Item = &'a RivalEntry> {
    profile.rival.entries.iter().filter(move |entry| {
        matches!(entry.source, RivalSourceConfig::Ir)
            && entry.ir_service == provider_key
            && !entry.ir_user_id.is_empty()
    })
}

/// The empty ID represents NONE. No ID is returned if there is nothing to cycle.
pub fn next_select_rival_id(profile: &ProfileConfig, forward: bool) -> Option<String> {
    let ids = selectable_rival_ids(profile);
    if ids.is_empty() {
        return None;
    }

    let active = profile.rival.active_rival.trim();
    let position = ids.iter().position(|id| id == active).map_or(0, |index| index + 1);
    let count = ids.len() + 1;
    let next = if forward { (position + 1) % count } else { (position + count - 1) % count };
    Some(if next == 0 { String::new() } else { ids[next - 1].clone() })
}

pub fn rian_ln_mode_for_chart(
    profile: crate::ln_policy::ChartLnProfile,
    policy: LnScorePolicy,
) -> u8 {
    match crate::ln_policy::played_ln_mode(profile, policy) {
        None => 0,
        Some(bmz_chart::model::LongNoteMode::Ln) => 1,
        Some(bmz_chart::model::LongNoteMode::Cn) => 2,
        Some(bmz_chart::model::LongNoteMode::Hcn) => 3,
        Some(bmz_chart::model::LongNoteMode::Hln) => 4,
    }
}

pub use crate::ir::ranking::CourseRankingTarget as SelectCourseIrTarget;

/// Local rivals work without an IR account; IR entries belong to the primary provider.
pub fn selectable_rival_ids(profile: &ProfileConfig) -> Vec<String> {
    let provider =
        select_rival_provider(profile).and_then(crate::ir::provider_key::configured_provider_key);
    profile
        .rival
        .entries
        .iter()
        .filter(|entry| match entry.source {
            RivalSourceConfig::LocalProfile => {
                entry.profile_id != profile.id
                    && crate::paths::validate_profile_id(&entry.profile_id).is_ok()
            }
            RivalSourceConfig::Ir => {
                provider == Some(entry.ir_service.as_str()) && !entry.ir_user_id.is_empty()
            }
            _ => false,
        })
        .map(|entry| entry.id.clone())
        .collect()
}

/// カーソル譜面ごとのキャッシュ済み IR 表示データ。
#[derive(Debug, Clone)]
struct CachedChartIr {
    global: ResultIrSnapshot,
    global_ranking: Option<ResultIrRanking>,
    global_scroll_offset: usize,
    self_and_rivals: Option<ResultIrSnapshot>,
    self_and_rivals_ranking: Option<ResultIrRanking>,
    self_and_rivals_scroll_offset: usize,
    rival: Option<SelectRivalSnapshot>,
    global_battle_entries: Vec<SelectIrBattleEntry>,
    self_and_rivals_battle_entries: Vec<SelectIrBattleEntry>,
    battle_entries_loaded: bool,
    global_targets: Vec<ResolvedTarget>,
    rival_targets: Vec<ResolvedTarget>,
    completed_at: Instant,
}

/// Select スキンへ供給するランキングの範囲。
///
/// Result と異なり選曲画面は既存スキンが常に global を見るため、
/// `snapshot_for` はこの値を参照しない。対応スキンだけが
/// [`SelectIrRanking::active_snapshot_for`] を使う。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectIrRankingScope {
    #[default]
    Global,
    SelfAndRivals,
}

/// 選曲キャッシュから Result へ渡すランキング。
#[derive(Debug, Clone)]
pub struct SelectIrResultRankings {
    pub global: ResultIrRanking,
    pub self_and_rivals: Option<ResultIrRanking>,
}

/// [`SelectIrRanking::result_chart_ranking`] の結果。
#[derive(Debug, Clone)]
pub enum SelectIrResultLookup {
    /// 選曲キャッシュに Global ranking がある。
    Loaded(Box<SelectIrResultRankings>),
    /// 選曲キャッシュ側で取得中。
    Loading,
    /// 選曲キャッシュ側の取得が失敗している。
    Failed,
    /// IR 未設定、または選曲キャッシュの条件が今回のプレイと異なる。
    Unavailable,
}

#[derive(Debug, Clone)]
struct CachedCourseIr {
    ir: ResultIrSnapshot,
    ranking: Option<ResultIrRanking>,
    scroll_offset: usize,
    completed_at: Instant,
}

pub struct SelectIrRanking {
    local_rival: local_rival::LocalRivalState,
    cache: HashMap<[u8; 32], CachedChartIr>,
    in_flight: Option<(String, [u8; 32], Instant)>,
    pending: Option<([u8; 32], Instant)>,
    /// キャッシュが前提とするランキング条件 (rule mode / 解決済み score key)。
    /// 変わったらキャッシュごと破棄する。
    context: String,
    sender: Sender<FetchResult>,
    receiver: Receiver<FetchResult>,
    course_cache: HashMap<SelectCourseIrTarget, CachedCourseIr>,
    course_in_flight: Option<(String, SelectCourseIrTarget, Instant)>,
    course_pending: Option<(SelectCourseIrTarget, Instant)>,
    course_sender: Sender<CourseFetchResult>,
    course_receiver: Receiver<CourseFetchResult>,
    active_scope: SelectIrRankingScope,
    active_rival: Option<SelectRivalFetchTarget>,
    active_rival_scores: RivalScores,
    rival_disk_state: RivalDiskState,
    rival_disk_result: Option<Receiver<anyhow::Result<RivalScores>>>,
    rival_in_flight: Option<(SelectRivalFetchTarget, Instant)>,
    rival_pending: Option<Instant>,
    rival_fetched_this_session: HashSet<SelectRivalFetchKey>,
    rival_sender: Sender<RivalFetchResult>,
    rival_receiver: Receiver<RivalFetchResult>,
}

impl Default for SelectIrRanking {
    fn default() -> Self {
        let (sender, receiver) = channel();
        let (course_sender, course_receiver) = channel();
        let (rival_sender, rival_receiver) = channel();
        Self {
            local_rival: Default::default(),
            cache: HashMap::new(),
            in_flight: None,
            pending: None,
            context: String::new(),
            sender,
            receiver,
            course_cache: HashMap::new(),
            course_in_flight: None,
            course_pending: None,
            course_sender,
            course_receiver,
            active_scope: SelectIrRankingScope::Global,
            active_rival: None,
            active_rival_scores: HashMap::new(),
            rival_disk_state: RivalDiskState::Idle,
            rival_disk_result: None,
            rival_in_flight: None,
            rival_pending: None,
            rival_fetched_this_session: HashSet::new(),
            rival_sender,
            rival_receiver,
        }
    }
}

mod fetch;
mod local_rival;
pub use local_rival::LocalRivalLoadError;
mod update;
mod view;

use fetch::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{IrProviderRoleConfig, IrSendPolicyConfig};
    use crate::ir::types::{
        IrCourseRankingBody, IrCourseRankingCourseRef, IrCourseRankingEntry, IrCourseRankingResult,
        IrCourseRankingScore, IrJudgePayload, IrJudgeSidePayload, IrRankingBody, IrRankingChartRef,
        IrRankingEntry, IrRankingPagination, IrRankingPlayer, IrRankingScore, IrRankingSelfRef,
        IrRivalEntry, IrRivalProfile,
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
                account_display_name: "Player".to_string(),
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
            clear_counts: None,
            self_rank: Some(rank),
            previous_rank: None,
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
                        score_id: Some("score-1".to_string()),
                        clear: "Hard".to_string(),
                        ex_score,
                        max_combo: 300,
                        min_bp: 2,
                        min_cb: 2,
                        gauge: Some("Groove".to_string()),
                        arrange_1p: Some("Normal".to_string()),
                        arrange_2p: None,
                        random_seed: None,
                        seed_scheme: None,
                        double_option: None,
                        verification: Some("verified_play".to_string()),
                        judges: None,
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
    fn complete_ir_population_survives_visible_row_truncation() {
        let mut ranking = raw_global_ranking([1; 32], 1, 2000, 12);
        let entry = ranking.ranking.entries[0].clone();
        ranking.ranking.entries = (1..=12)
            .map(|rank| {
                let mut entry = entry.clone();
                entry.rank = rank;
                entry.player.id = format!("player-{rank}");
                entry.score.clear = if rank <= 2 { "FullCombo" } else { "Hard" }.to_string();
                entry
            })
            .collect();
        let snapshot = ranking_to_ir_snapshot(&ranking);
        assert_eq!(snapshot.entries.len(), 10);
        assert_eq!(snapshot.clear_counts, Some([0, 0, 0, 0, 0, 0, 10, 0, 2, 0, 0]));
        ranking.ranking.entries[0].score.clear = "Unknown".to_string();
        assert_eq!(ranking_to_ir_snapshot(&ranking).clear_counts, None);
        ranking.ranking.entries[0].score.clear = "FullCombo".to_string();
        ranking.ranking.pagination.as_mut().unwrap().has_more = true;
        assert_eq!(ranking_to_ir_snapshot(&ranking).clear_counts, None);
        ranking.ranking.pagination.as_mut().unwrap().has_more = false;
        ranking.ranking.pagination.as_mut().unwrap().offset = 1;
        assert_eq!(ranking_to_ir_snapshot(&ranking).clear_counts, None);
        ranking.ranking.pagination.as_mut().unwrap().offset = 0;
        ranking.ranking.pagination.as_mut().unwrap().total = Some(200);
        assert_eq!(ranking_to_ir_snapshot(&ranking).clear_counts, None);
        ranking.ranking.pagination = None;
        assert_eq!(ranking_to_ir_snapshot(&ranking).clear_counts, None);
    }

    fn raw_self_and_rivals_ranking(
        sha256: [u8; 32],
        rank: u32,
        ex_score: u32,
        total: u32,
    ) -> IrRankingResult {
        let mut ranking = raw_global_ranking(sha256, rank, ex_score, total);
        ranking.ranking.scope = IrRankingScope::SelfAndRivals;
        ranking
    }

    fn rival_fetch_target(rival_id: &str, body: &str) -> SelectRivalFetchTarget {
        SelectRivalFetchTarget {
            provider: "rianir".to_string(),
            base_url: "https://ir.example.test/api/".to_string(),
            rival_id: rival_id.to_string(),
            display_name: format!("Rival {rival_id}"),
            body: body.to_string(),
            rule_mode: RuleMode::Beatoraja,
            bmz_account_id: None,
        }
    }

    /// A profile root that does not exist, inside a directory the TempDir owns. Never the
    /// shared temp root itself: credentials found there would change what the tests see.
    fn missing_profile_root(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp =
            tempfile::Builder::new().prefix(&format!("bmz-select-ir-{name}-")).tempdir().unwrap();
        let root = temp.path().join("missing-profile");
        (temp, root)
    }

    #[test]
    fn resolved_targets_preserve_names_through_fetch_and_result_cache() {
        let sha = [7u8; 32];
        let mut raw = raw_global_ranking(sha, 1, 1800, 3);
        raw.ranking.entries[0].player.display_name = "TOP_名前".to_string();
        let top = ranking_targets(&raw).pop().unwrap();
        assert_eq!(top, ResolvedTarget { name: "TOP_名前".to_string(), ex_score: 1800 });
        let rows = vec![
            top.clone(),
            ResolvedTarget { name: "NEXT_名前".to_string(), ex_score: 1600 },
            ResolvedTarget { name: "Self".to_string(), ex_score: 1400 },
        ];
        assert_eq!(next_target_above(&rows, 1500), Some(rows[1].clone()));
        assert_eq!(next_target_above(&rows, 2000), Some(top));
        assert_eq!(next_target_above(&[], 0), None);

        let mut select_ir = SelectIrRanking::default();
        assert!(
            select_ir
                .resolved_target_for(&ir_config(true), Some(sha), TargetOption::IrTop, None)
                .is_none()
        );
        let mut ranking = result_global_ranking(1, 1800, 3);
        ranking.entries[0].player_name = "RANK_AAA".to_string();
        select_ir.cache_result_global_ranking(&hash_to_hex(&sha), &ranking);
        assert_eq!(
            select_ir.resolved_target_for(&ir_config(true), Some(sha), TargetOption::IrTop, None),
            Some(ResolvedTarget { name: "RANK_AAA".to_string(), ex_score: 1800 })
        );
        assert!(
            select_ir
                .resolved_target_for(&ir_config(false), Some(sha), TargetOption::IrTop, None)
                .is_none()
        );
        ranking.entries.clear();
        select_ir.cache_result_global_ranking(&hash_to_hex(&sha), &ranking);
        assert!(
            select_ir
                .resolved_target_for(&ir_config(true), Some(sha), TargetOption::IrTop, None)
                .is_none()
        );
    }

    #[test]
    fn snapshot_keeps_provider_online_without_a_ranking_target() {
        let select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];

        let offline = select_ir.snapshot_for(&ir_config(false), Some(sha));
        assert!(!offline.online);
        assert_eq!(offline.state, SkinIrState::Offline);

        let waiting = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert!(waiting.online);
        assert_eq!(waiting.state, SkinIrState::Waiting);
        assert_eq!(waiting.provider_name.as_str(), "BMZ IR");
        assert_eq!(waiting.user_name.as_str(), "Player");

        let none = select_ir.snapshot_for(&ir_config(true), None);
        assert!(none.online);
        assert_eq!(none.state, SkinIrState::Offline);
        assert_eq!(none.provider_name.as_str(), "BMZ IR");
        assert_eq!(none.user_name.as_str(), "Player");
    }

    #[test]
    fn cached_snapshot_is_returned() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        select_ir.cache.insert(
            sha,
            CachedChartIr {
                global: ResultIrSnapshot {
                    state: SkinIrState::Loaded,
                    rank: Some(2),
                    total_player: Some(10),
                    clear_rate: None,
                    previous_rank: None,
                    ..Default::default()
                },
                global_ranking: None,
                global_scroll_offset: 0,
                self_and_rivals: Some(ranking_to_ir_snapshot(&raw_self_and_rivals_ranking(
                    sha, 1, 1750, 2,
                ))),
                self_and_rivals_ranking: Some(chart_ranking_to_result_ir_ranking(
                    &raw_self_and_rivals_ranking(sha, 1, 1750, 2),
                )),
                self_and_rivals_scroll_offset: 0,
                rival: Some(SelectRivalSnapshot {
                    display_name: "RivalOne".to_string(),
                    numeric_score_available: true,
                    ex_score: 1500,
                    clear_index: 6,
                    max_combo: 700,
                    bp: 12,
                    judge_counts: None,
                }),
                global_battle_entries: Vec::new(),
                self_and_rivals_battle_entries: Vec::new(),
                battle_entries_loaded: true,
                global_targets: vec![1800, 1600, 1400]
                    .into_iter()
                    .map(|ex_score| ResolvedTarget { name: format!("Player {ex_score}"), ex_score })
                    .collect(),
                rival_targets: vec![1500, 1200]
                    .into_iter()
                    .map(|ex_score| ResolvedTarget { name: format!("Player {ex_score}"), ex_score })
                    .collect(),
                completed_at: Instant::now(),
            },
        );

        let snapshot = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(snapshot.state, SkinIrState::Loaded);
        assert_eq!(snapshot.rank, Some(2));
        assert_eq!(snapshot.scope, SkinIrScope::Global);
        assert!(snapshot.rival_scope_supported);

        assert!(select_ir.select_scope(
            &ir_config(true),
            Some(sha),
            SelectIrRankingScope::SelfAndRivals,
        ));
        let active = select_ir.snapshot_for_binding(
            &ir_config(true),
            Some(sha),
            bmz_render::skin::IrScopeBinding::Active,
        );
        assert_eq!(active.rank, Some(1));
        assert_eq!(active.total_player, Some(2));
        assert_eq!(active.scope, SkinIrScope::Rival);
        assert!(active.rival_scope_supported);
        assert!(select_ir.rival_for(&ir_config(true), Some(sha)).is_none());
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

        assert!(select_ir.toggle_scope(&ir_config(true), Some(sha)));
        assert_eq!(select_ir.active_scope(), SelectIrRankingScope::Global);

        select_ir.clear();
        let cleared = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(cleared.state, SkinIrState::Waiting);
    }

    #[test]
    fn skin_scroll_rate_moves_chart_ranking_window() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [8u8; 32];
        let ranking = ResultIrRanking {
            scope: IrRankingScope::Global,
            entries: (1..=25)
                .map(|rank| ResultIrRankingEntry {
                    rank,
                    player_name: format!("Player{rank}"),
                    ex_score: 2000 - rank,
                    clear: "Hard".to_string(),
                    bp: rank,
                    max_combo: 1000 - rank,
                })
                .collect(),
            clear_rate: Some(80),
            clear_counts: None,
            self_rank: Some(8),
            previous_rank: None,
            total: Some(25),
        };
        select_ir.cache_result_global_ranking(&hash_to_hex(&sha), &ranking);

        select_ir.set_skin_scroll_rate(
            Some(sha),
            None,
            bmz_render::skin::IrScopeBinding::Global,
            0.5,
        );
        let middle = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(middle.scroll_offset, 8);
        assert_eq!(middle.scroll_max, 15);
        assert_eq!(middle.entries[0].rank, Some(9));
        assert_eq!(middle.entries[9].rank, Some(18));

        select_ir.set_skin_scroll_rate(
            Some(sha),
            None,
            bmz_render::skin::IrScopeBinding::Global,
            1.0,
        );
        let end = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(end.scroll_offset, 15);
        assert_eq!(end.entries[0].rank, Some(16));
        assert_eq!(end.entries[9].rank, Some(25));
    }

    #[test]
    fn course_fetch_result_populates_select_skin_ranking_rows() {
        let mut select_ir = SelectIrRanking::default();
        let target = SelectCourseIrTarget {
            course_hash: "ab".repeat(32),
            rian_course_hash_v1: "cd".repeat(32),
            bms_ir_course_key: Some("ef".repeat(48)),
            gauge: "Class".to_string(),
            ln_policy: "auto".to_string(),
            rule_mode: RuleMode::Beatoraja,
        };
        let requested_at = Instant::now();
        select_ir.context = "course-context".to_string();
        select_ir.course_in_flight =
            Some((select_ir.context.clone(), target.clone(), requested_at));
        // A was requested again after leaving it. The old failure must not cancel or satisfy the new request.
        let (_temp, root) = missing_profile_root("course-ranking");
        select_ir
            .course_sender
            .send((
                select_ir.context.clone(),
                target.clone(),
                requested_at - Duration::from_millis(1),
                Err("old request".to_string()),
            ))
            .unwrap();
        select_ir.update_course(&ir_config(true), &root, "course-context", Some(target.clone()));
        assert!(select_ir.course_cache.is_empty());
        assert_eq!(select_ir.course_in_flight.as_ref().unwrap().2, requested_at);
        select_ir
            .course_sender
            .send((
                select_ir.context.clone(),
                target.clone(),
                requested_at,
                Ok(IrCourseRankingResult {
                    course: IrCourseRankingCourseRef { course_hash: target.course_hash.clone() },
                    rule: None,
                    ranking: IrCourseRankingBody {
                        scope: IrRankingScope::Global,
                        entries: (1..=15)
                            .map(|rank| IrCourseRankingEntry {
                                rank,
                                player: IrRankingPlayer {
                                    id: format!("player-{rank}"),
                                    display_name: if rank == 1 {
                                        "CourseTop".to_string()
                                    } else {
                                        format!("Course{rank}")
                                    },
                                },
                                score: IrCourseRankingScore {
                                    course_score_id: format!("course-score-{rank}"),
                                    clear: "Hard".to_string(),
                                    course_clear: true,
                                    ex_score: 4321 - rank,
                                    max_combo: 999,
                                    bp: 12,
                                    device_type: None,
                                    played_at: None,
                                    verification: None,
                                },
                            })
                            .collect(),
                        self_summary: None,
                        pagination: None,
                    },
                }),
            ))
            .unwrap();

        select_ir.update_course(&ir_config(true), &root, "course-context", Some(target.clone()));
        let snapshot = select_ir.course_snapshot_for(&ir_config(true), Some(&target));

        assert_eq!(snapshot.state, SkinIrState::Loaded);
        assert_eq!(snapshot.total_player, Some(15));
        assert_eq!(snapshot.entries[0].rank, Some(1));
        assert_eq!(snapshot.entries[0].ex_score, Some(4320));
        assert_eq!(snapshot.entries[0].player_name.as_str(), "CourseTop");

        select_ir.set_skin_scroll_rate(
            None,
            Some(&target),
            bmz_render::skin::IrScopeBinding::Global,
            1.0,
        );
        let end = select_ir.course_snapshot_for(&ir_config(true), Some(&target));
        assert_eq!(end.scroll_offset, 5);
        assert_eq!(end.scroll_max, 5);
        assert_eq!(end.entries[0].rank, Some(6));
        assert_eq!(end.entries[0].player_name.as_str(), "Course6");
    }

    #[test]
    fn result_global_ranking_updates_cached_snapshot() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let old_global_battle_entries =
            battle_entries(&raw_global_ranking(sha, 9, 1200, 10), SeedScheme::Beatoraja24);
        let old_rival_battle_entries =
            battle_entries(&raw_self_and_rivals_ranking(sha, 4, 1500, 5), SeedScheme::Beatoraja24);
        select_ir.cache.insert(
            sha,
            CachedChartIr {
                global: ResultIrSnapshot {
                    state: SkinIrState::Loaded,
                    rank: Some(9),
                    total_player: Some(10),
                    previous_rank: None,
                    ..Default::default()
                },
                global_ranking: None,
                global_scroll_offset: 0,
                self_and_rivals: None,
                self_and_rivals_ranking: None,
                self_and_rivals_scroll_offset: 0,
                rival: Some(SelectRivalSnapshot {
                    display_name: "RivalOne".to_string(),
                    numeric_score_available: true,
                    ex_score: 1500,
                    clear_index: 6,
                    max_combo: 700,
                    bp: 12,
                    judge_counts: None,
                }),
                global_battle_entries: old_global_battle_entries,
                self_and_rivals_battle_entries: old_rival_battle_entries,
                battle_entries_loaded: true,
                global_targets: vec![1200]
                    .into_iter()
                    .map(|ex_score| ResolvedTarget { name: format!("Player {ex_score}"), ex_score })
                    .collect(),
                rival_targets: vec![1500]
                    .into_iter()
                    .map(|ex_score| ResolvedTarget { name: format!("Player {ex_score}"), ex_score })
                    .collect(),
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
        assert!(select_ir.rival_for(&ir_config(true), Some(sha)).is_none());
        assert!(select_ir.battle_entries_for(Some(sha)).is_empty());
        let cached = &select_ir.cache[&sha];
        assert!(cached.self_and_rivals_battle_entries.is_empty());
        assert!(!cached.battle_entries_loaded);
        let (pending_sha, pending_since) = select_ir.pending.unwrap();
        assert_eq!(pending_sha, sha);
        assert!(pending_since.elapsed() >= FETCH_DEBOUNCE);
    }

    #[test]
    fn rival_snapshot_sums_fast_and_slow_judges() {
        let mut ranking = raw_global_ranking([7u8; 32], 1, 1500, 3);
        ranking.ranking.entries[0].score.judges = Some(IrJudgePayload {
            fast: IrJudgeSidePayload {
                pgreat: 500,
                great: 30,
                good: 4,
                bad: 2,
                poor: 1,
                empty_poor: 9,
            },
            slow: IrJudgeSidePayload {
                pgreat: 400,
                great: 20,
                good: 3,
                bad: 1,
                poor: 2,
                empty_poor: 8,
            },
        });

        let rival = top_rival_snapshot(&ranking).unwrap();
        assert_eq!(rival.clear_index, 6);
        assert_eq!(
            rival.judge_counts,
            Some(SelectRivalJudgeCounts { pgreat: 900, great: 50, good: 7, bad: 3, poor: 3 })
        );
    }

    #[test]
    fn rian_rival_clear_index_is_bounded_to_skin_range() {
        assert_eq!(view::rival_clear_index(-1), 0);
        assert_eq!(view::rival_clear_index(6), 6);
        assert_eq!(view::rival_clear_index(11), 10);
    }

    #[test]
    fn bms_ir_rival_cycle_uses_only_the_selected_player_and_ignores_late_scores_after_none() {
        let mut profile = ProfileConfig::new_default("test", "Test", 0);
        let bms_ir = crate::ir::bms_ir::BMS_IR_PROVIDER;
        profile.ir.primary_provider = bms_ir.to_string();
        let provider =
            profile.ir.providers.iter_mut().find(|provider| provider.provider == bms_ir).unwrap();
        provider.provider_key = bms_ir.to_string();
        provider.enabled = true;
        provider.account_id = "123".to_string();
        profile.play.rule_mode = RuleMode::Beatoraja;
        let rival = |id: &str, name: &str| IrRivalEntry {
            player_id: id.to_string(),
            relation_type: "rival".to_string(),
            profile: Some(IrRivalProfile { display_name: name.to_string(), bio: None }),
        };
        crate::ir_cmd::sync_ir_rivals_into_profile(
            &mut profile,
            bms_ir,
            &[rival("1", "A"), rival("2", "B")],
        );
        crate::ir_cmd::sync_ir_rivals_into_profile(&mut profile, "rian-ir", &[rival("3", "C")]);

        let (_temp, root) = missing_profile_root("bms-rival-cycle");
        let mut select_ir = SelectIrRanking::default();
        let sha = [7; 32];
        let context = "bms-rival-cycle";
        let requested_at = Instant::now();
        select_ir.context = context.to_string();
        select_ir.in_flight = Some((context.to_string(), sha, requested_at));
        let mut rivals = raw_global_ranking(sha, 1, 1700, 1);
        rivals.ranking.scope = IrRankingScope::Rivals;
        select_ir
            .sender
            .send((
                context.to_string(),
                sha,
                requested_at,
                Ok((raw_global_ranking(sha, 1, 1700, 1), None, Some(rivals))),
            ))
            .unwrap();
        select_ir.update(
            &profile.ir,
            &root,
            context,
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            profile.play.rule_mode,
            Some(sha),
            None,
            None,
        );
        assert!(select_ir.active_rival_display_name().is_none());
        assert!(select_ir.rival_for(&profile.ir, Some(sha)).is_none());
        assert_eq!(
            select_ir.target_ex_score_for(&profile.ir, Some(sha), TargetOption::RivalTop, None),
            Some(1700)
        );

        let cycle_forward = |profile: &mut ProfileConfig, select_ir: &mut SelectIrRanking| {
            profile.rival.active_rival = next_select_rival_id(profile, true).unwrap();
            let target = SelectRivalFetchTarget::from_profile(profile);
            select_ir.update_rival(target.clone(), &root);
            target
        };
        let first = cycle_forward(&mut profile, &mut select_ir).unwrap();
        assert_eq!(first.rival_id, "1");
        assert_eq!(select_ir.active_rival_display_name(), Some("A"));
        let rival_snapshot = |select_ir: &SelectIrRanking, sha| {
            select_ir.active_rival_snapshot(
                sha,
                0,
                LnScorePolicy::ForceLn,
                DoubleOptionScoreBucket::Off,
            )
        };
        assert!(rival_snapshot(&select_ir, sha).is_none());

        let score = IrRivalScoreRecord {
            chart_sha256: sha,
            ln_mode: 0,
            bmz_identity: None,
            ex_score: 1500,
            clear_type: 6,
            max_combo: 700,
            min_bp: 12,
            play_option: 0,
            arrange_1p: String::new(),
            arrange_2p: String::new(),
            double_option: String::new(),
            play_seed: Some(2179),
        };
        select_ir.rival_in_flight = Some((first.clone(), requested_at));
        select_ir
            .rival_sender
            .send((
                first.clone(),
                requested_at,
                Ok(index_rival_scores(&first.provider, vec![score.clone()])),
            ))
            .unwrap();
        select_ir.update_rival(Some(first), &root);
        let snapshot = rival_snapshot(&select_ir, sha).unwrap();
        assert_eq!(snapshot.display_name, "A");
        assert_eq!(snapshot.ex_score, 1500);
        assert_eq!(
            select_ir
                .active_rival_score(sha, 0, LnScorePolicy::ForceLn, DoubleOptionScoreBucket::Off)
                .unwrap()
                .play_seed,
            None
        );
        assert!(rival_snapshot(&select_ir, [8; 32]).is_none());
        assert_eq!(select_ir.active_rival_display_name(), Some("A"));

        let second = cycle_forward(&mut profile, &mut select_ir).unwrap();
        assert_eq!(second.rival_id, "2");
        assert_eq!(select_ir.active_rival_display_name(), Some("B"));
        assert!(rival_snapshot(&select_ir, sha).is_none());
        select_ir.rival_in_flight = Some((second.clone(), requested_at));
        assert!(cycle_forward(&mut profile, &mut select_ir).is_none());
        assert!(profile.rival.active_rival.is_empty());
        let scores = index_rival_scores(&second.provider, vec![score]);
        select_ir.rival_sender.send((second, requested_at, Ok(scores))).unwrap();
        select_ir.update_rival(None, &root);
        assert!(select_ir.active_rival_display_name().is_none());
        assert!(rival_snapshot(&select_ir, sha).is_none());
        assert!(select_ir.rival_for(&profile.ir, Some(sha)).is_none());

        profile.rival.active_rival = next_select_rival_id(&profile, false).unwrap();
        assert_eq!(SelectRivalFetchTarget::from_profile(&profile).unwrap().rival_id, "2");
        profile.play.rule_mode = RuleMode::Dx;
        assert!(next_select_rival_id(&profile, true).is_none());
        assert!(SelectRivalFetchTarget::from_profile(&profile).is_none());
    }

    #[test]
    fn active_rian_rival_snapshot_exposes_bp_and_clear() {
        let mut select_ir = SelectIrRanking::default();
        let target = rival_fetch_target("160", "beatoraja");
        let sha256 = [7u8; 32];
        select_ir.active_rival = Some(target);
        select_ir.active_rival_scores.insert(
            (sha256, 2, None),
            IrRivalScoreRecord {
                chart_sha256: sha256,
                ln_mode: 2,
                bmz_identity: None,
                ex_score: 1500,
                clear_type: 6,
                max_combo: 700,
                min_bp: 12,
                play_option: 0,
                arrange_1p: String::new(),
                arrange_2p: String::new(),
                double_option: String::new(),
                play_seed: None,
            },
        );

        let rival = select_ir
            .active_rival_snapshot(sha256, 2, LnScorePolicy::ForceCn, DoubleOptionScoreBucket::Off)
            .unwrap();
        assert_eq!(rival.display_name, "Rival 160");
        assert_eq!(rival.ex_score, 1500);
        assert_eq!(rival.clear_index, 6);
        assert_eq!(rival.bp, 12);
    }

    #[test]
    fn stale_disk_read_cannot_replace_fresh_rival_scores() {
        let temp = tempfile::tempdir().unwrap();
        let target = rival_fetch_target("42", "beatoraja");
        let mut ir = SelectIrRanking::default();
        let (tx, rx) = channel();
        ir.active_rival = Some(target.clone());
        ir.rival_disk_state = RivalDiskState::Loading;
        ir.rival_disk_result = Some(rx);
        let score = convert_bmz_rival_score(crate::ir::bmz_official::BmzRivalBestScore {
            chart_sha256: "11".repeat(32),
            ln_policy: LnScorePolicy::ForceLn.as_str().into(),
            double_option: DoubleOptionScoreBucket::Off.ir_value().into(),
            ex_score: 200,
            clear_type: "clear".into(),
            max_combo: 100,
            min_bp: 0,
            arrange_1p: None,
            arrange_2p: None,
            applied_double_option: None,
            play_seed: None,
        })
        .unwrap();
        let scores = index_rival_scores(&target.provider, vec![score]);
        ir.rival_sender.send((target.clone(), Instant::now(), Ok(scores))).unwrap();
        ir.update_rival(Some(target.clone()), temp.path());
        tx.send(Ok(RivalScores::new())).unwrap();
        ir.update_rival(Some(target), temp.path());
        assert_eq!(ir.active_rival_scores.len(), 1);
    }

    #[test]
    #[ignore = "rival selection timing with a populated local database"]
    fn measure_rival_selection() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network.db");
        crate::storage::migration::migrate_network_db(&path).unwrap();
        let target = rival_fetch_target("42", "beatoraja");
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<20000)
             INSERT INTO ir_rival_scores(provider, rival_id, body, chart_sha256, ln_mode,
                 ex_score, clear_type, max_combo, min_bp, play_option, fetched_at)
             SELECT ?1, ?2, ?3, printf('%064x',i), 0, 1500, 6, 700, 12, 0, 1 FROM n",
                rusqlite::params![target.provider, target.rival_id, target.body],
            )
            .unwrap();
        let mut timings = Vec::new();
        let mut ready = Vec::new();
        for _ in 0..11 {
            let mut ir = SelectIrRanking::default();
            ir.rival_fetched_this_session.insert((&target).into());
            let start = Instant::now();
            ir.update_rival(Some(target.clone()), temp.path());
            timings.push(start.elapsed());
            while ir.active_rival_scores.len() != 20000 {
                assert!(start.elapsed() < Duration::from_secs(10));
                std::thread::yield_now();
                ir.update_rival(Some(target.clone()), temp.path());
            }
            ready.push(start.elapsed());
        }
        timings.sort();
        ready.sort();
        eprintln!("selection median {:?}, ready median {:?}", timings[5], ready[5]);
    }

    #[test]
    fn bmz_rival_cache_preserves_ln_policy_and_double_option() {
        use crate::storage::{migration::migrate_network_db, network_db::NetworkDatabase};

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network.db");
        migrate_network_db(&path).unwrap();
        let mut db = NetworkDatabase::open(&path).unwrap();
        let sha = [7; 32];
        let mut target = rival_fetch_target("42", "bmz-rivals-v1");
        target.provider = "bmz".to_string();
        target.bmz_account_id = Some("owner".to_string());

        let off = DoubleOptionScoreBucket::Off;
        let battle = DoubleOptionScoreBucket::Battle;
        let ln = LnScorePolicy::ForceLn;
        let cn = LnScorePolicy::ForceCn;
        let score = |ln_policy: LnScorePolicy, double_option: DoubleOptionScoreBucket, ex_score| {
            convert_bmz_rival_score(crate::ir::bmz_official::BmzRivalBestScore {
                chart_sha256: hash_to_hex(&sha),
                ln_policy: ln_policy.as_str().to_string(),
                double_option: double_option.ir_value().to_string(),
                ex_score,
                clear_type: "hard_clear".to_string(),
                max_combo: 700,
                min_bp: 12,
                arrange_1p: Some("random".to_string()),
                arrange_2p: Some("normal".to_string()),
                applied_double_option: None,
                play_seed: Some(42),
            })
            .unwrap()
        };
        db.replace_rival_scores(
            &target.provider,
            &target.rival_id,
            &target.body,
            &[score(ln, off, 1300), score(cn, off, 1400), score(cn, battle, 1500)],
            "",
            1,
        )
        .unwrap();
        let mut select_ir = SelectIrRanking::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            select_ir.update_rival(Some(target.clone()), temp.path());
            if !select_ir.active_rival_scores.is_empty() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(select_ir.active_rival_display_name(), Some("Rival 42"));
        for (policy, bucket, expected) in [(ln, off, 1300), (cn, off, 1400), (cn, battle, 1500)] {
            let snapshot = select_ir.active_rival_snapshot(sha, 0, policy, bucket).unwrap();
            assert_eq!(snapshot.ex_score, expected);
            assert_eq!(snapshot.clear_index, bmz_core::clear::ClearType::Hard as i64);
        }
        assert!(select_ir.active_rival_score(sha, 0, ln, battle).is_none());
        assert_eq!(select_ir.active_rival_score(sha, 0, cn, off).unwrap().play_seed, Some(42));
        select_ir.update_rival(None, temp.path());
        assert!(select_ir.active_rival_score(sha, 0, cn, off).is_none());
    }

    #[test]
    fn successful_rival_fetch_is_not_scheduled_again_this_session() {
        let mut select_ir = SelectIrRanking::default();
        let first = rival_fetch_target("1001", "beatoraja");
        let second = rival_fetch_target("1002", "beatoraja");
        let requested_at = Instant::now();
        let (_temp, root) = missing_profile_root("reuse-success");

        select_ir.active_rival = Some(first.clone());
        select_ir.rival_in_flight = Some((first.clone(), requested_at));
        select_ir.rival_sender.send((first.clone(), requested_at, Ok(RivalScores::new()))).unwrap();
        select_ir.update_rival(Some(first.clone()), &root);

        assert!(select_ir.rival_fetched_this_session.contains(&SelectRivalFetchKey::from(&first)));
        assert!(select_ir.rival_pending.is_none());
        assert!(select_ir.rival_in_flight.is_none());

        select_ir.update_rival(Some(second), &root);
        assert!(select_ir.rival_pending.is_some());

        select_ir.update_rival(Some(first), &root);
        assert!(select_ir.rival_pending.is_none());
        assert!(select_ir.rival_in_flight.is_none());
    }

    #[test]
    fn first_rival_selection_still_schedules_session_refresh() {
        let mut select_ir = SelectIrRanking::default();
        let target = rival_fetch_target("1001", "beatoraja");
        let (_temp, root) = missing_profile_root("first-refresh");

        select_ir.update_rival(Some(target), &root);

        assert!(select_ir.rival_fetched_this_session.is_empty());
        assert!(select_ir.rival_pending.is_some());
        assert!(select_ir.rival_in_flight.is_none());
    }

    #[test]
    fn stale_successful_rival_fetch_is_reused_when_selected_again() {
        let mut select_ir = SelectIrRanking::default();
        let first = rival_fetch_target("1001", "beatoraja");
        let second = rival_fetch_target("1002", "beatoraja");
        let requested_at = Instant::now();
        let (_temp, root) = missing_profile_root("reuse-stale");

        select_ir.active_rival = Some(second.clone());
        select_ir.rival_in_flight = Some((first.clone(), requested_at));
        select_ir.rival_sender.send((first.clone(), requested_at, Ok(RivalScores::new()))).unwrap();
        select_ir.update_rival(Some(second), &root);

        assert!(select_ir.rival_fetched_this_session.contains(&SelectRivalFetchKey::from(&first)));
        select_ir.update_rival(Some(first), &root);
        assert!(select_ir.rival_pending.is_none());
    }

    #[test]
    fn failed_rival_fetch_is_scheduled_after_reselection() {
        let mut select_ir = SelectIrRanking::default();
        let first = rival_fetch_target("1001", "beatoraja");
        let second = rival_fetch_target("1002", "beatoraja");
        let requested_at = Instant::now();
        let (_temp, root) = missing_profile_root("retry-failure");

        select_ir.active_rival = Some(first.clone());
        select_ir.rival_in_flight = Some((first.clone(), requested_at));
        select_ir
            .rival_sender
            .send((first.clone(), requested_at, Err("offline".to_string())))
            .unwrap();
        select_ir.update_rival(Some(first.clone()), &root);

        assert!(select_ir.rival_fetched_this_session.is_empty());
        select_ir.update_rival(Some(second), &root);
        select_ir.update_rival(Some(first), &root);
        assert!(select_ir.rival_pending.is_some());
    }

    #[test]
    fn rival_fetch_session_key_includes_body_and_base_url() {
        let mut select_ir = SelectIrRanking::default();
        let fetched = rival_fetch_target("1001", "beatoraja");
        let mut other_body = fetched.clone();
        other_body.body = "DX MODE".to_string();
        let mut other_base_url = fetched.clone();
        other_base_url.base_url = "https://other.example.test/api/".to_string();
        let (_temp, root) = missing_profile_root("cache-key");

        select_ir.rival_fetched_this_session.insert((&fetched).into());
        select_ir.active_rival = Some(fetched);

        select_ir.update_rival(Some(other_body), &root);
        assert!(select_ir.rival_pending.is_some());

        select_ir.update_rival(Some(other_base_url), &root);
        assert!(select_ir.rival_pending.is_some());
    }

    #[test]
    fn result_lookup_reuses_select_cache_without_clearing_other_contexts() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let (_temp, root) = missing_profile_root("result-lookup");
        let lookup = |select_ir: &mut SelectIrRanking, context: &str| {
            select_ir.result_chart_ranking(
                &ir_config(true),
                &root,
                context,
                LnScorePolicy::ForceLn,
                DoubleOptionScoreBucket::Off,
                RuleMode::Beatoraja,
                sha,
                None,
                None,
            )
        };

        select_ir.context = "ctx".to_string();

        // 選曲で始めた取得がプレイ中に完了していても、Result で取り込める。
        let requested_at = Instant::now();
        select_ir.in_flight = Some(("ctx".to_string(), sha, requested_at));
        assert!(matches!(lookup(&mut select_ir, "ctx"), SelectIrResultLookup::Loading));
        select_ir
            .sender
            .send((
                "ctx".to_string(),
                sha,
                requested_at,
                Ok((
                    raw_global_ranking(sha, 2, 1500, 4),
                    Some(raw_self_and_rivals_ranking(sha, 1, 1500, 2)),
                    None,
                )),
            ))
            .unwrap();
        match lookup(&mut select_ir, "ctx") {
            SelectIrResultLookup::Loaded(rankings) => {
                assert_eq!(rankings.global.self_rank, Some(2));
                assert_eq!(rankings.self_and_rivals.unwrap().self_rank, Some(1));
            }
            other => panic!("expected cached ranking, got {other:?}"),
        }
        assert!(select_ir.in_flight.is_none());

        // リプレイの条件が選曲と異なる場合は、選曲キャッシュを捨てない。
        assert!(matches!(lookup(&mut select_ir, "other"), SelectIrResultLookup::Unavailable));
        assert_eq!(select_ir.context, "ctx");
        assert!(select_ir.cache.contains_key(&sha));
        assert!(select_ir.in_flight.is_none());

        let mut failed = SelectIrRanking {
            context: "ctx".to_string(),
            in_flight: Some(("ctx".to_string(), sha, requested_at)),
            ..Default::default()
        };
        failed.sender.send(("ctx".to_string(), sha, requested_at, Err("offline".into()))).unwrap();
        assert!(matches!(lookup(&mut failed, "ctx"), SelectIrResultLookup::Failed));
        assert!(failed.in_flight.is_none());

        let mut offline = SelectIrRanking { context: "ctx".to_string(), ..Default::default() };
        assert!(matches!(
            offline.result_chart_ranking(
                &ir_config(false),
                &root,
                "ctx",
                LnScorePolicy::ForceLn,
                DoubleOptionScoreBucket::Off,
                RuleMode::Beatoraja,
                sha,
                None,
                None,
            ),
            SelectIrResultLookup::Unavailable
        ));
    }

    #[tokio::test]
    async fn result_lookup_fetches_a_missing_ranking_once_into_the_select_cache() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let other_sha = [8u8; 32];
        let (_temp, root) = missing_profile_root("result-fetch");
        select_ir.context = "ctx".to_string();
        let lookup = |select_ir: &mut SelectIrRanking| {
            select_ir.result_chart_ranking(
                &ir_config(true),
                &root,
                "ctx",
                LnScorePolicy::ForceLn,
                DoubleOptionScoreBucket::Off,
                RuleMode::Beatoraja,
                sha,
                None,
                None,
            )
        };

        // 別譜面の取得中は、その完了まで始めない。
        let other_requested_at = Instant::now();
        select_ir.in_flight = Some(("ctx".to_string(), other_sha, other_requested_at));
        assert!(matches!(lookup(&mut select_ir), SelectIrResultLookup::Loading));
        assert_eq!(select_ir.in_flight.as_ref().unwrap().1, other_sha);
        select_ir.in_flight = None;

        select_ir.pending = Some((sha, Instant::now()));
        assert!(matches!(lookup(&mut select_ir), SelectIrResultLookup::Loading));
        let (context, in_flight_sha, requested_at) = select_ir.in_flight.clone().unwrap();
        assert_eq!((context.as_str(), in_flight_sha), ("ctx", sha));
        assert!(select_ir.pending.is_none());

        // 取得中は次のフレームでも取り直さない。
        assert!(matches!(lookup(&mut select_ir), SelectIrResultLookup::Loading));
        assert_eq!(select_ir.in_flight.as_ref().unwrap().2, requested_at);
    }

    #[test]
    fn update_debounces_before_fetching() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(true);
        let (_temp, root) = missing_profile_root("debounce");

        // 1回目はデバウンス予約のみで取得を開始しない。
        select_ir.update(
            &config,
            &root,
            "ctx",
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
            Some(sha),
            None,
            None,
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
            None,
            None,
        );
        assert!(select_ir.pending.is_none());
    }

    #[test]
    fn stale_fetch_result_is_discarded_after_context_change() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(false);
        let (_temp, root) = missing_profile_root("context-change");
        let requested_at = Instant::now();

        select_ir.context = "new".to_string();
        select_ir.in_flight = Some(("old".to_string(), sha, requested_at));
        select_ir
            .sender
            .send(("old".to_string(), sha, requested_at, Err("stale".to_string())))
            .unwrap();

        select_ir.update(
            &config,
            &root,
            "new",
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
            Some(sha),
            None,
            None,
        );
        assert!(!select_ir.cache.contains_key(&sha));
        assert!(select_ir.in_flight.is_none());
    }

    #[test]
    fn stale_fetch_result_does_not_cancel_newer_refresh_or_override_result_cache() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(false);
        let (_temp, root) = missing_profile_root("stale-response");
        let stale_requested_at = Instant::now();

        select_ir.context = "ctx".to_string();
        select_ir.in_flight = Some(("ctx".to_string(), sha, stale_requested_at));
        select_ir
            .cache_result_global_ranking(&hash_to_hex(&sha), &result_global_ranking(1, 2000, 3));
        let refresh_requested_at = Instant::now();
        select_ir.in_flight = Some(("ctx".to_string(), sha, refresh_requested_at));
        select_ir
            .sender
            .send((
                "ctx".to_string(),
                sha,
                stale_requested_at,
                Ok((raw_global_ranking(sha, 9, 1200, 10), None, None)),
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
            None,
            None,
        );

        let snapshot = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(snapshot.rank, Some(1));
        assert_eq!(
            select_ir.target_ex_score_for(&ir_config(true), Some(sha), TargetOption::IrTop, None),
            Some(2000)
        );
        assert!(select_ir.in_flight.as_ref().is_some_and(
            |(context, in_flight_sha, requested_at)| {
                context == "ctx" && *in_flight_sha == sha && *requested_at == refresh_requested_at
            }
        ));
    }

    #[test]
    fn refreshed_fetch_replaces_invalidated_g_battle_entries() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(false);
        let (_temp, root) = missing_profile_root("context-reuse");

        select_ir.context = "ctx".to_string();
        select_ir
            .cache_result_global_ranking(&hash_to_hex(&sha), &result_global_ranking(1, 2000, 3));
        let requested_at = Instant::now();
        select_ir.in_flight = Some(("ctx".to_string(), sha, requested_at));
        select_ir
            .sender
            .send((
                "ctx".to_string(),
                sha,
                requested_at,
                Ok((raw_global_ranking(sha, 1, 2100, 3), None, None)),
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
            None,
            None,
        );

        let entries = select_ir.battle_entries_for(Some(sha));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rank, 1);
        assert_eq!(entries[0].ex_score, 2100);
        assert_eq!(entries[0].score_id.as_deref(), Some("score-1"));
        assert!(select_ir.cache[&sha].battle_entries_loaded);
        assert!(select_ir.pending.is_none());
        assert!(select_ir.in_flight.is_none());
    }
}
