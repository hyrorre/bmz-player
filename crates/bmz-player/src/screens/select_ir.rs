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

use crate::config::profile_config::{IrConfig, ProfileConfig, RivalSourceConfig};
use crate::ir::bmz_official::{BmzOfficialIrClient, IrCourseRankingRequest};
use crate::ir::types::{IrCourseRankingResult, IrRankingResult, IrRankingScope};
use crate::ln_policy::LnScorePolicy;
use crate::screens::result_ir::{
    ResultIrQuery, ResultIrRanking, course_ranking_to_result_ir_ranking, ranking_to_ir_snapshot,
    result_ir_ranking_to_skin_snapshot,
};
use crate::select_options::DoubleOptionScoreBucket;
use crate::select_options::TargetOption;
use crate::storage::common::{hash_to_hex, hex_to_hash};
use crate::storage::network_db::IrRivalScoreRecord;

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
type RivalFetchResult = (SelectRivalFetchTarget, Instant, Result<Vec<IrRivalScoreRecord>, String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectRivalFetchTarget {
    pub provider: String,
    pub base_url: String,
    pub rival_id: String,
    pub display_name: String,
    pub body: String,
    pub rule_mode: RuleMode,
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
        let provider = crate::ir::provider_key::primary_provider_config(&profile.ir)?;
        if !crate::ir::rian_ir::is_rian_ir_config(provider)
            && !crate::ir::bms_ir::is_bms_ir_config(provider)
        {
            return None;
        }
        if crate::ir::bms_ir::is_bms_ir_config(provider) && profile.play.rule_mode == RuleMode::Dx {
            return None;
        }
        let provider_key = crate::ir::provider_key::configured_provider_key(provider)?.to_string();
        let active = profile.rival.active_rival.trim();
        if active.is_empty() {
            return None;
        }
        let rival = profile.rival.entries.iter().find(|entry| {
            entry.id == active
                && matches!(entry.source, RivalSourceConfig::Ir)
                && entry.ir_service == provider_key
                && !entry.ir_user_id.is_empty()
        })?;
        Some(Self {
            provider: provider_key,
            base_url: provider.base_url.clone(),
            rival_id: rival.ir_user_id.clone(),
            display_name: rival.display_name.clone(),
            body: crate::ir::rian_ir::body_for_rule_mode(profile.play.rule_mode).to_string(),
            rule_mode: profile.play.rule_mode,
        })
    }
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
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectCourseIrTarget {
    pub course_hash: String,
    pub rian_course_hash_v1: String,
    pub bms_ir_course_key: Option<String>,
    pub gauge: String,
    pub ln_policy: String,
    pub rule_mode: RuleMode,
}

/// カーソル譜面ごとのキャッシュ済み IR 表示データ。
#[derive(Debug, Clone)]
struct CachedChartIr {
    global: ResultIrSnapshot,
    self_and_rivals: Option<ResultIrSnapshot>,
    rival: Option<SelectRivalSnapshot>,
    global_battle_entries: Vec<SelectIrBattleEntry>,
    self_and_rivals_battle_entries: Vec<SelectIrBattleEntry>,
    battle_entries_loaded: bool,
    global_ex_scores: Vec<u32>,
    rival_ex_scores: Vec<u32>,
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

#[derive(Debug, Clone)]
struct CachedCourseIr {
    ir: ResultIrSnapshot,
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
    course_cache: HashMap<SelectCourseIrTarget, CachedCourseIr>,
    course_in_flight: Option<(String, SelectCourseIrTarget, Instant)>,
    course_pending: Option<(SelectCourseIrTarget, Instant)>,
    course_sender: Sender<CourseFetchResult>,
    course_receiver: Receiver<CourseFetchResult>,
    active_scope: SelectIrRankingScope,
    active_rival: Option<SelectRivalFetchTarget>,
    active_rival_scores: HashMap<([u8; 32], u8), IrRivalScoreRecord>,
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
            rival_in_flight: None,
            rival_pending: None,
            rival_fetched_this_session: HashSet::new(),
            rival_sender,
            rival_receiver,
        }
    }
}

mod fetch;
mod update;
mod view;

use fetch::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{
        IrProviderConfig, IrProviderRoleConfig, IrSendPolicyConfig,
    };
    use crate::ir::types::{
        IrCourseRankingBody, IrCourseRankingCourseRef, IrCourseRankingEntry, IrCourseRankingResult,
        IrCourseRankingScore, IrJudgePayload, IrJudgeSidePayload, IrRankingBody, IrRankingChartRef,
        IrRankingEntry, IrRankingPagination, IrRankingPlayer, IrRankingScore, IrRankingSelfRef,
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
        }
    }

    fn missing_profile_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bmz-select-ir-{name}-{}", std::process::id()))
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
                self_and_rivals: Some(ranking_to_ir_snapshot(&raw_self_and_rivals_ranking(
                    sha, 1, 1750, 2,
                ))),
                rival: Some(SelectRivalSnapshot {
                    display_name: "RivalOne".to_string(),
                    ex_score: 1500,
                    clear_index: 6,
                    max_combo: 700,
                    bp: 12,
                    judge_counts: None,
                }),
                global_battle_entries: Vec::new(),
                self_and_rivals_battle_entries: Vec::new(),
                battle_entries_loaded: true,
                global_ex_scores: vec![1800, 1600, 1400],
                rival_ex_scores: vec![1500, 1200],
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
        // 既存の単一ライバル/targetは Rivals scope のまま変えない。
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

        assert!(select_ir.toggle_scope(&ir_config(true), Some(sha)));
        assert_eq!(select_ir.active_scope(), SelectIrRankingScope::Global);

        select_ir.clear();
        let cleared = select_ir.snapshot_for(&ir_config(true), Some(sha));
        assert_eq!(cleared.state, SkinIrState::Waiting);
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
                        entries: vec![IrCourseRankingEntry {
                            rank: 1,
                            player: IrRankingPlayer {
                                id: "player-1".to_string(),
                                display_name: "CourseTop".to_string(),
                            },
                            score: IrCourseRankingScore {
                                course_score_id: "course-score-1".to_string(),
                                clear: "Hard".to_string(),
                                course_clear: true,
                                ex_score: 4321,
                                max_combo: 999,
                                bp: 12,
                                device_type: None,
                                played_at: None,
                                verification: None,
                            },
                        }],
                    },
                }),
            ))
            .unwrap();

        select_ir.update_course(
            &ir_config(true),
            &missing_profile_root("course-ranking"),
            "course-context",
            Some(target.clone()),
        );
        let snapshot = select_ir.course_snapshot_for(&ir_config(true), Some(&target));

        assert_eq!(snapshot.state, SkinIrState::Loaded);
        assert_eq!(snapshot.total_player, Some(1));
        assert_eq!(snapshot.entries[0].rank, Some(1));
        assert_eq!(snapshot.entries[0].ex_score, Some(4321));
        assert_eq!(snapshot.entries[0].player_name.as_str(), "CourseTop");
    }

    #[test]
    fn result_global_ranking_updates_cached_snapshot() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let old_global_battle_entries = battle_entries(&raw_global_ranking(sha, 9, 1200, 10));
        let old_rival_battle_entries =
            battle_entries(&raw_self_and_rivals_ranking(sha, 4, 1500, 5));
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
                self_and_rivals: None,
                rival: Some(SelectRivalSnapshot {
                    display_name: "RivalOne".to_string(),
                    ex_score: 1500,
                    clear_index: 6,
                    max_combo: 700,
                    bp: 12,
                    judge_counts: None,
                }),
                global_battle_entries: old_global_battle_entries,
                self_and_rivals_battle_entries: old_rival_battle_entries,
                battle_entries_loaded: true,
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
    fn active_rian_rival_snapshot_exposes_bp_and_clear() {
        let mut select_ir = SelectIrRanking::default();
        let target = rival_fetch_target("160", "beatoraja");
        let sha256 = [7u8; 32];
        select_ir.active_rival = Some(target);
        select_ir.active_rival_scores.insert(
            (sha256, 2),
            IrRivalScoreRecord {
                chart_sha256: sha256,
                ln_mode: 2,
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

        let rival = select_ir.active_rival_snapshot(sha256, 2).unwrap();
        assert_eq!(rival.display_name, "Rival 160");
        assert_eq!(rival.ex_score, 1500);
        assert_eq!(rival.clear_index, 6);
        assert_eq!(rival.bp, 12);
    }

    #[test]
    fn successful_rival_fetch_is_not_scheduled_again_this_session() {
        let mut select_ir = SelectIrRanking::default();
        let first = rival_fetch_target("1001", "beatoraja");
        let second = rival_fetch_target("1002", "beatoraja");
        let requested_at = Instant::now();
        let root = missing_profile_root("reuse-success");

        select_ir.active_rival = Some(first.clone());
        select_ir.rival_in_flight = Some((first.clone(), requested_at));
        select_ir.rival_sender.send((first.clone(), requested_at, Ok(Vec::new()))).unwrap();
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
        let root = missing_profile_root("first-refresh");

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
        let root = missing_profile_root("reuse-stale");

        select_ir.active_rival = Some(second.clone());
        select_ir.rival_in_flight = Some((first.clone(), requested_at));
        select_ir.rival_sender.send((first.clone(), requested_at, Ok(Vec::new()))).unwrap();
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
        let root = missing_profile_root("retry-failure");

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
        let root = missing_profile_root("cache-key");

        select_ir.rival_fetched_this_session.insert((&fetched).into());
        select_ir.active_rival = Some(fetched);

        select_ir.update_rival(Some(other_body), &root);
        assert!(select_ir.rival_pending.is_some());

        select_ir.update_rival(Some(other_base_url), &root);
        assert!(select_ir.rival_pending.is_some());
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
        );
        assert!(!select_ir.cache.contains_key(&sha));
        assert!(select_ir.in_flight.is_none());
    }

    #[test]
    fn stale_fetch_result_does_not_cancel_newer_refresh_or_override_result_cache() {
        let mut select_ir = SelectIrRanking::default();
        let sha = [7u8; 32];
        let config = ir_config(false);
        let root = std::env::temp_dir();
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
        let root = std::env::temp_dir();

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
