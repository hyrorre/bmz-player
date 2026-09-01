use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use bmz_chart::model::PlayableChart;
use bmz_core::clear::ClearType;
use bmz_core::ids::NoteId;
use bmz_core::input::InputDeviceKind;
use bmz_core::lane::KeyMode;
use bmz_core::replay::ReplayEvent;
use bmz_core::time::TimeUs;
use bmz_gameplay::gauge::{GaugeCarryValue, GaugeState};
#[cfg(test)]
use bmz_gameplay::judge::model::JudgeWindows;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::session::{
    AssistLevel, AssistRuntime, GameSession, PlayState, ResultJudgementDetail,
};

use crate::config::profile_config::{IrConfig, KeyModeConversionConfig, ReplayConfig};
use crate::ir::payload::{IrSubmissionContext, build_score_submission};
use crate::ln_policy::ChartLnProfile;
use crate::paths::ProfilePaths;
use crate::screens::play_session::AppliedArrange;
use crate::screens::result_model::{ResultGraphCollector, ResultSummary};
use crate::storage::network_db::{IrJobKind, NetworkDatabase, NewIrScoreJob};
use crate::storage::play_result::{
    StorePlayResultMode, StorePlayResultRequest, StoredPlayResult, course_stage_clear_type,
    store_play_result,
};
use crate::storage::score_db::{ScoreDatabase, ScoreKey};

#[derive(Debug, Clone)]
pub struct FinishedPlaySession {
    pub result: PlayResult,
    pub stored: StoredPlayResult,
    pub summary: ResultSummary,
    pub gauge_carry: Vec<GaugeCarryValue>,
    pub course_combo: u32,
    pub course_max_combo: u32,
    pub replay_playback: bool,
    pub arrange: crate::select_options::ArrangeOption,
    pub applied_arrange: AppliedArrange,
    /// IR ランキング照会に使うスコア分離キー。
    pub ln_policy: crate::ln_policy::LnScorePolicy,
    pub double_option: crate::select_options::DoubleOptionScoreBucket,
    pub rule_mode: bmz_gameplay::rule::RuleMode,
    pub assist: AssistRuntime,
    /// score_best、履歴、またはclear-only集計のいずれかを永続化した。
    pub score_data_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishResultMode {
    Normal,
    CourseStage,
}

impl FinishResultMode {
    fn store_mode(self) -> StorePlayResultMode {
        match self {
            Self::Normal => StorePlayResultMode::Normal,
            Self::CourseStage => StorePlayResultMode::CourseStage,
        }
    }

    fn summary_clear_type(self, clear_type: ClearType) -> ClearType {
        match self {
            Self::Normal => clear_type,
            Self::CourseStage => course_stage_clear_type(clear_type),
        }
    }

    fn enqueue_score_ir(self) -> bool {
        match self {
            Self::Normal | Self::CourseStage => true,
        }
    }
}

pub fn play_result_from_session(session: &GameSession) -> PlayResult {
    let mut result = PlayResult::from_states_with_total_notes(
        &session.chart,
        &session.score,
        &session.gauge,
        session.scored_total_notes,
        session.state,
        session.autoplay.as_ref().is_some_and(|autoplay| autoplay.is_full()),
    );
    if !matches!(result.clear_type, ClearType::NoPlay | ClearType::Failed) {
        result.clear_type = match session.assist.level {
            AssistLevel::None => result.clear_type,
            AssistLevel::LightAssist => ClearType::LightAssistEasy,
            AssistLevel::Assist => ClearType::AssistEasy,
        };
    }
    result
}

pub fn store_session_result(
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    ir_config: &IrConfig,
    session: &GameSession,
    played_at: i64,
    applied_arrange: &AppliedArrange,
    score_key: ScoreKey,
    practice_mode: bool,
) -> Result<StoredPlayResult> {
    Ok(finish_session_result(
        score_db,
        network_db,
        FinishSessionResultRequest {
            profile_paths,
            replay_config,
            ir_config,
            session,
            played_at,
            applied_arrange,
            source_ln_profile: ChartLnProfile::from_chart(&session.chart),
            chart_length_ms: None,
            play_duration_ms: None,
            target_ex_score: None,
            score_key,
            practice_mode,
            finish_mode: FinishResultMode::Normal,
        },
    )?
    .stored)
}

pub struct FinishSessionResultRequest<'a> {
    pub profile_paths: &'a ProfilePaths,
    pub replay_config: &'a ReplayConfig,
    pub ir_config: &'a IrConfig,
    pub session: &'a GameSession,
    pub played_at: i64,
    pub applied_arrange: &'a AppliedArrange,
    pub source_ln_profile: ChartLnProfile,
    pub chart_length_ms: Option<u64>,
    pub play_duration_ms: Option<u64>,
    pub target_ex_score: Option<u32>,
    pub score_key: ScoreKey,
    pub practice_mode: bool,
    pub finish_mode: FinishResultMode,
}

#[derive(Debug, Clone)]
struct FinishSessionSnapshot {
    chart: Arc<PlayableChart>,
    skin_attempt: bmz_render::snapshot::SkinAttemptState,
    result: PlayResult,
    primary_key_mode: KeyMode,
    replay_events: Vec<ReplayEvent>,
    replay_playback: bool,
    replay_lane_mask: bool,
    rule_mode: bmz_gameplay::rule::RuleMode,
    gauge_carry: Vec<GaugeCarryValue>,
    course_combo: u32,
    course_max_combo: u32,
    result_judgements: HashMap<NoteId, ResultJudgementDetail>,
    failed_gauge: Option<GaugeState>,
    assist: AssistRuntime,
}

impl FinishSessionSnapshot {
    fn from_session(
        session: &GameSession,
        source_ln_profile: ChartLnProfile,
        applied_arrange: &AppliedArrange,
    ) -> Self {
        let source_key_mode = match applied_arrange.key_mode_conversion {
            KeyModeConversionConfig::SpToDp => match session.chart.metadata.key_mode {
                KeyMode::K10 => KeyMode::K5,
                KeyMode::K14 => KeyMode::K7,
                mode => mode,
            },
            KeyModeConversionConfig::SevenToNine | KeyModeConversionConfig::SevenToSix => {
                KeyMode::K7
            }
            KeyModeConversionConfig::Off
                if matches!(
                    applied_arrange.double_option,
                    crate::select_options::DoubleOption::Battle
                        | crate::select_options::DoubleOption::BattleAutoScratch
                ) =>
            {
                match session.chart.metadata.key_mode {
                    KeyMode::K10 => KeyMode::K5,
                    KeyMode::K14 => KeyMode::K7,
                    mode => mode,
                }
            }
            KeyModeConversionConfig::Off => session.primary_key_mode,
        };
        let session_mode_index = usize::from(session.session_mode_index);
        let chart = if applied_arrange.key_mode_conversion == KeyModeConversionConfig::SevenToNine
            && matches!(
                applied_arrange.seven_to_nine_rule_mode,
                crate::config::profile_config::SevenToNineRuleMode::Keys7
            ) {
            let mut source_rule_chart = (*session.chart).clone();
            source_rule_chart.metadata.key_mode = KeyMode::K7;
            Arc::new(source_rule_chart)
        } else {
            Arc::clone(&session.chart)
        };
        Self {
            chart,
            skin_attempt: bmz_render::snapshot::SkinAttemptState {
                source_key_mode: Some(source_key_mode),
                effective_key_mode: Some(session.primary_key_mode),
                seven_to_six: applied_arrange.seven_to_six(),
                seven_to_nine_pattern: if applied_arrange.key_mode_conversion
                    == KeyModeConversionConfig::SevenToNine
                {
                    applied_arrange.seven_to_nine_pattern.value()
                } else {
                    0
                },
                seven_to_nine_type: applied_arrange.seven_to_nine_type.value(),
                source_ln_profile_bits: Some(crate::skin_extension::source_ln_profile_bits(
                    source_ln_profile,
                )),
                session_mode_index: Some(session_mode_index),
                double_option_index: Some(crate::skin_extension::double_option_index(
                    applied_arrange.double_option,
                )),
                hsfix_index: usize::try_from(session.hsfix_index).ok(),
                gauge_auto_shift_index: Some(crate::skin_extension::gauge_auto_shift_index(
                    session.gauge.auto_shift_mode,
                )),
                bottom_shiftable_gauge_index: Some(
                    crate::skin_extension::bottom_shiftable_gauge_index(
                        session.gauge.bottom_shiftable_gauge,
                    ),
                ),
                judge_algorithm_index: Some(crate::skin_extension::judge_algorithm_index(
                    session.judge.algorithm,
                )),
                ln_mode_index: Some(crate::skin_extension::long_note_mode_index(
                    session.chart.metadata.long_note_mode,
                )),
                has_bga: Some(session.chart.metadata.has_bga),
                has_random_sequence: Some(session.chart.metadata.has_bms_random),
            },
            result: play_result_from_session(session),
            primary_key_mode: session.primary_key_mode,
            replay_events: session.replay_recorder.events.clone(),
            replay_playback: session.replay_player.is_some() && session.replay_lane_mask.is_none(),
            replay_lane_mask: session.replay_lane_mask.is_some(),
            rule_mode: session.rule_mode,
            gauge_carry: session.gauge.carry_values(),
            course_combo: session.display_combo(),
            course_max_combo: session.display_max_combo(),
            result_judgements: session.result_judgements.clone(),
            failed_gauge: (session.state == PlayState::Failed).then(|| session.gauge.clone()),
            assist: session.assist,
        }
    }
}

struct FinishSessionSnapshotResultRequest<'a> {
    profile_paths: &'a ProfilePaths,
    replay_config: &'a ReplayConfig,
    ir_config: &'a IrConfig,
    snapshot: &'a FinishSessionSnapshot,
    played_at: i64,
    applied_arrange: &'a AppliedArrange,
    source_ln_profile: ChartLnProfile,
    chart_length_ms: Option<u64>,
    play_duration_ms: Option<u64>,
    target_ex_score: Option<u32>,
    score_key: ScoreKey,
    practice_mode: bool,
    finish_mode: FinishResultMode,
}

pub fn finish_session_result(
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    request: FinishSessionResultRequest<'_>,
) -> Result<FinishedPlaySession> {
    finish_session_result_when(score_db, network_db, request, FinishSessionReadiness::Terminal)
}

fn finish_session_result_when(
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    request: FinishSessionResultRequest<'_>,
    readiness: FinishSessionReadiness,
) -> Result<FinishedPlaySession> {
    let FinishSessionResultRequest {
        profile_paths,
        replay_config,
        ir_config,
        session,
        played_at,
        applied_arrange,
        source_ln_profile,
        chart_length_ms,
        play_duration_ms,
        target_ex_score,
        score_key,
        practice_mode,
        finish_mode,
    } = request;
    ensure_storable_session(session, readiness)?;
    let snapshot = FinishSessionSnapshot::from_session(session, source_ln_profile, applied_arrange);
    finish_session_snapshot_result(
        score_db,
        network_db,
        FinishSessionSnapshotResultRequest {
            profile_paths,
            replay_config,
            ir_config,
            snapshot: &snapshot,
            played_at,
            applied_arrange,
            source_ln_profile,
            chart_length_ms,
            play_duration_ms,
            target_ex_score,
            score_key,
            practice_mode,
            finish_mode,
        },
    )
}

fn finish_session_snapshot_result(
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    request: FinishSessionSnapshotResultRequest<'_>,
) -> Result<FinishedPlaySession> {
    let FinishSessionSnapshotResultRequest {
        profile_paths,
        replay_config,
        ir_config,
        snapshot,
        played_at,
        applied_arrange,
        source_ln_profile,
        chart_length_ms,
        play_duration_ms,
        target_ex_score,
        score_key,
        practice_mode,
        finish_mode,
    } = request;
    let result = snapshot.result.clone();
    let summary_clear_type = finish_mode.summary_clear_type(result.clear_type);
    let replay_playback = snapshot.replay_playback;
    let conversion_persistence_disabled = applied_arrange.score_persistence_disabled();
    let previous_best = (!conversion_persistence_disabled)
        .then(|| score_db.best_scores_for_charts(&[score_key]).ok())
        .flatten()
        .and_then(|mut bests| bests.pop());
    // オートプレイ / リプレイ再生 / プラクティス時はスコア・リプレイをDBに保存しない
    // （リザルト画面の表示のみ行う）。
    let full_autoplay = result.autoplay;
    let score_data_changed =
        !full_autoplay && !replay_playback && !practice_mode && !conversion_persistence_disabled;
    let stored =
        if full_autoplay || replay_playback || practice_mode || conversion_persistence_disabled {
            StoredPlayResult {
                score_history_id: 0,
                played_at,
                replay_path: String::new(),
                replay_sha256: None,
                slot_paths: [None, None, None, None],
                device_type: InputDeviceKind::Keyboard,
            }
        } else {
            let arrange = applied_arrange.arrange;
            let arrange_seed = applied_arrange.seed;
            let random_seed = applied_arrange.packed_beatoraja_seed(snapshot.primary_key_mode);
            let arrange_pattern = applied_arrange.pattern.clone();
            store_play_result(
                score_db,
                profile_paths,
                replay_config,
                &result,
                StorePlayResultRequest {
                    played_at,
                    playtime_seconds: chart_playtime_seconds(&snapshot.chart),
                    ln_policy: score_key.ln_policy,
                    double_option: score_key.double_option,
                    applied_double_option: applied_arrange.double_option,
                    random_seed,
                    gauge_option: String::new(),
                    rule_mode: snapshot.rule_mode.as_str().to_string(),
                    assist_mask: snapshot.assist.configured_mask,
                    replay_events: snapshot.replay_events.clone(),
                    arrange,
                    arrange_2p: applied_arrange.arrange_2p,
                    arrange_seed,
                    arrange_seed_2p: applied_arrange.seed_2p,
                    bms_random_choices: applied_arrange.bms_random_choices.clone(),
                    bms_switch_choices: applied_arrange.bms_switch_choices.clone(),
                    seed_scheme: if applied_arrange.legacy_seed {
                        crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3.to_string()
                    } else {
                        crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1.to_string()
                    },
                    s_random_scheme: applied_arrange.s_random_scheme,
                    s_random_scheme_2p: applied_arrange.s_random_scheme_2p,
                    h_random_threshold_ms: applied_arrange.h_random_threshold_ms,
                    arrange_pattern,
                    update_score: snapshot.assist.score_update_enabled(),
                    mode: finish_mode.store_mode(),
                },
            )?
        };
    let mut summary = ResultSummary::from_play_result(&result, &stored, &snapshot.chart);
    summary.skin_attempt = snapshot.skin_attempt;
    summary.key_mode = snapshot.primary_key_mode;
    summary.clear_type = summary_clear_type;
    summary.arrange = applied_arrange.arrange.as_str().to_string();
    summary.arrange_2p = applied_arrange.arrange_2p.as_str().to_string();
    summary.lane_shuffle_pattern = applied_arrange.pattern.clone().unwrap_or_default();
    summary.target_ex_score = target_ex_score;
    summary.saved_replay_slots = stored.slot_paths.each_ref().map(Option::is_some);
    if let Some(best) = &previous_best {
        summary.previous_best_ex_score = Some(best.ex_score);
        summary.previous_best_clear_type = clear_type_from_name(&best.clear_type);
        summary.previous_best_max_combo = Some(best.max_combo);
        summary.previous_best_bp = Some(best.bp);
    }
    // 過去ベストスコア・ベストコンボを ResultSummary にフィルする。
    // 今回のスコアが直前に upsert_score_best されているので、`best_*` は
    // 「現在の最高記録」を返す。差分表示は `current - best` として 0 になり得る。
    if !conversion_persistence_disabled
        && let Ok(bests) = score_db.best_scores_for_charts(&[score_key])
        && let Some(best) = bests.into_iter().next()
    {
        summary.best_ex_score = Some(best.ex_score);
        summary.best_clear_type = clear_type_from_name(&best.clear_type);
        summary.best_max_combo = Some(best.max_combo);
        summary.best_bp = Some(best.bp);
    }
    if !conversion_persistence_disabled
        && let Ok(slots) = score_db.replay_slots_for_chart(score_key)
    {
        summary.replay_slots = slots.each_ref().map(Option::is_some);
        for (index, saved) in summary.saved_replay_slots.iter().enumerate() {
            if *saved {
                summary.replay_slots[index] = true;
            }
        }
    }
    if finish_mode.enqueue_score_ir()
        && snapshot.assist.score_update_enabled()
        && !conversion_persistence_disabled
    {
        let mut ir_result = result.clone();
        ir_result.clear_type = summary_clear_type;
        enqueue_ir_jobs(
            network_db,
            ir_config,
            EnqueueIrJobsRequest {
                snapshot,
                result: &ir_result,
                stored: &stored,
                played_at,
                score_key,
                applied_arrange,
                source_ln_profile,
                finish_mode,
                chart_length_ms,
                play_duration_ms,
                summary: &mut summary,
                previous_best: previous_best.as_ref(),
            },
        );
    }

    Ok(FinishedPlaySession {
        result,
        stored,
        summary,
        gauge_carry: snapshot.gauge_carry.clone(),
        course_combo: snapshot.course_combo,
        course_max_combo: snapshot.course_max_combo,
        replay_playback,
        arrange: applied_arrange.arrange,
        applied_arrange: applied_arrange.clone(),
        ln_policy: score_key.ln_policy,
        double_option: score_key.double_option,
        rule_mode: score_key.rule_mode,
        assist: snapshot.assist,
        score_data_changed,
    })
}

fn chart_playtime_seconds(chart: &bmz_chart::model::PlayableChart) -> u32 {
    (chart.end_time.0.max(0) / 1_000_000).min(i64::from(u32::MAX)) as u32
}

fn clear_type_from_name(name: &str) -> Option<ClearType> {
    match name {
        "NoPlay" => Some(ClearType::NoPlay),
        "Failed" => Some(ClearType::Failed),
        "AssistEasy" => Some(ClearType::AssistEasy),
        "LightAssistEasy" => Some(ClearType::LightAssistEasy),
        "Easy" => Some(ClearType::Easy),
        "Normal" => Some(ClearType::Normal),
        "Hard" => Some(ClearType::Hard),
        "ExHard" => Some(ClearType::ExHard),
        "FullCombo" => Some(ClearType::FullCombo),
        "Perfect" => Some(ClearType::Perfect),
        "Max" => Some(ClearType::Max),
        _ => None,
    }
}

struct EnqueueIrJobsRequest<'a> {
    snapshot: &'a FinishSessionSnapshot,
    result: &'a PlayResult,
    stored: &'a StoredPlayResult,
    played_at: i64,
    score_key: ScoreKey,
    applied_arrange: &'a AppliedArrange,
    source_ln_profile: ChartLnProfile,
    finish_mode: FinishResultMode,
    chart_length_ms: Option<u64>,
    play_duration_ms: Option<u64>,
    summary: &'a mut ResultSummary,
    previous_best: Option<&'a crate::storage::score_db::BestScoreSummary>,
}

fn enqueue_ir_jobs(
    network_db: &mut NetworkDatabase,
    ir_config: &IrConfig,
    request: EnqueueIrJobsRequest<'_>,
) {
    let EnqueueIrJobsRequest {
        snapshot,
        result,
        stored,
        played_at,
        score_key,
        applied_arrange,
        source_ln_profile,
        finish_mode,
        chart_length_ms,
        play_duration_ms,
        summary,
        previous_best,
    } = request;
    if stored.score_history_id <= 0 {
        return;
    }
    let enabled: Vec<_> = ir_config
        .providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && should_send_ir_score(provider.send_policy, result, previous_best)
                && (!crate::ir::rian_ir::is_rian_ir_config(provider)
                    || crate::ir::rian_ir::score_submission_supported(
                        score_key.ln_policy,
                        applied_arrange.double_option,
                    ))
                && (!crate::ir::bms_ir::is_bms_ir_config(provider)
                    || crate::ir::bms_ir::score_submission_supported(
                        snapshot.rule_mode,
                        snapshot.chart.metadata.source_format,
                        source_ln_profile,
                        score_key.ln_policy,
                        applied_arrange.double_option,
                    ))
        })
        .collect();
    if enabled.is_empty() {
        return;
    }
    let payload = build_score_submission(
        &snapshot.chart,
        result,
        IrSubmissionContext {
            played_at,
            play_duration_ms,
            chart_length_ms,
            ln_policy: score_key.ln_policy,
            source_ln_profile,
            gauge_option: result.gauge_type.as_str().to_string(),
            device_type: stored.device_type,
            idempotency_key: format!("bmz-score-{}", stored.score_history_id),
            arrange: applied_arrange.arrange,
            arrange_2p: if snapshot.replay_lane_mask {
                crate::select_options::ArrangeOption::Normal
            } else {
                applied_arrange.arrange_2p
            },
            double_option: score_key.double_option,
            applied_double_option: applied_arrange.double_option,
            arrange_seed: applied_arrange.packed_beatoraja_seed(snapshot.primary_key_mode),
            random_seed: applied_arrange.packed_beatoraja_seed(snapshot.primary_key_mode),
            seed_scheme: if applied_arrange.legacy_seed {
                crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3.to_string()
            } else {
                crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1.to_string()
            },
            bms_random_choices: applied_arrange.bms_random_choices.clone(),
            bms_switch_choices: applied_arrange.bms_switch_choices.clone(),
            rule_mode: snapshot.rule_mode.as_str().to_string(),
            course_stage: finish_mode == FinishResultMode::CourseStage,
            // 保存時に serialize 済みバイト列から計算した hash。プレイ終了
            // 直後のフレームでリプレイファイルを読み直さない。
            replay_hash: stored.replay_sha256.clone(),
        },
    );
    let Ok(payload_json) = serde_json::to_string(&payload) else {
        summary.ir_last_error = Some("failed to serialize IR payload".to_string());
        return;
    };
    for provider in enabled {
        if crate::ir::rian_ir::is_rian_ir_config(provider)
            && !crate::ir::rian_ir::score_duration_plausible(
                result.clear_type.as_str(),
                chart_length_ms,
                play_duration_ms,
                snapshot.chart.metadata.has_bms_random,
            )
        {
            let error = format!(
                "rianIR duration check rejected locally: length_ms={}, play_duration_ms={}",
                chart_length_ms.unwrap_or_default(),
                play_duration_ms.unwrap_or_default()
            );
            summary.ir_last_error = Some(error.clone());
            tracing::warn!(%error, "skipped implausible rianIR score job");
            continue;
        }
        let Some(provider_key) = crate::ir::provider_key::configured_provider_key(provider) else {
            summary.ir_last_error = Some(format!(
                "IR provider '{}' is missing provider_key; log in again",
                provider.provider
            ));
            continue;
        };
        match network_db.enqueue_ir_score_job(&NewIrScoreJob {
            provider: provider_key.to_string(),
            account_id: provider.account_id.clone(),
            kind: IrJobKind::Score,
            local_score_id: stored.score_history_id,
            chart_sha256: result.chart_sha256,
            ln_policy: score_key.ln_policy,
            payload_json: payload_json.clone(),
            now: played_at,
        }) {
            Ok(_) => summary.ir_queued_jobs += 1,
            Err(error) => {
                summary.ir_last_error = Some(error.to_string());
                tracing::warn!(provider = provider.provider, provider_key, %error, "failed to enqueue IR score job");
            }
        }
    }
}

/// 送信ポリシーによる IR ジョブ作成可否。
///
/// - `Always`: 常に送る
/// - `CompleteSong`: 最終ゲージが 0 より大きい場合だけ送る
/// - `UpdateScore`: EX / clear / max combo / BP / CB のいずれかが
///   ローカルベストから改善した場合 (または初プレイ) だけ送る
///
/// サーバー側でも best 更新判定は別途行われるため、これはクライアント側の
/// 送信量制御にすぎない。
fn should_send_ir_score(
    policy: crate::config::profile_config::IrSendPolicyConfig,
    result: &PlayResult,
    previous_best: Option<&crate::storage::score_db::BestScoreSummary>,
) -> bool {
    use crate::config::profile_config::IrSendPolicyConfig;
    match policy {
        IrSendPolicyConfig::Always => true,
        IrSendPolicyConfig::CompleteSong => result.gauge_value > 0.0,
        IrSendPolicyConfig::UpdateScore => {
            let Some(best) = previous_best else {
                return true;
            };
            let best_clear_rank =
                clear_type_from_name(&best.clear_type).map(|clear| clear as i32).unwrap_or(0);
            result.score.ex_score() > best.ex_score
                || (result.clear_type as i32) > best_clear_rank
                || result.score.max_combo > best.max_combo
                || result.record_bp() < best.bp
                || result.record_cb() < best.cb
        }
    }
}

pub fn finish_session_result_once(
    cached: &mut Option<FinishedPlaySession>,
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    request: FinishSessionResultOnceRequest<'_>,
) -> Result<FinishedPlaySession> {
    finish_session_result_once_when(
        cached,
        score_db,
        network_db,
        request,
        FinishSessionReadiness::Terminal,
    )
}

/// 判定結果が確定済みなら、Play の終了演出を待たずに結果を一度だけ保存する。
pub fn finish_settled_session_result_once(
    cached: &mut Option<FinishedPlaySession>,
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    request: FinishSessionResultOnceRequest<'_>,
    settled_at: TimeUs,
) -> Result<FinishedPlaySession> {
    finish_session_result_once_when(
        cached,
        score_db,
        network_db,
        request,
        FinishSessionReadiness::SettledAt(settled_at),
    )
}

fn finish_session_result_once_when(
    cached: &mut Option<FinishedPlaySession>,
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    request: FinishSessionResultOnceRequest<'_>,
    readiness: FinishSessionReadiness,
) -> Result<FinishedPlaySession> {
    if let Some(finished) = cached.clone() {
        return Ok(finished);
    }

    let mut finished = finish_session_result_when(
        score_db,
        network_db,
        FinishSessionResultRequest {
            profile_paths: request.profile_paths,
            replay_config: request.replay_config,
            ir_config: request.ir_config,
            session: request.session,
            played_at: request.played_at,
            applied_arrange: request.applied_arrange,
            source_ln_profile: request.source_ln_profile,
            chart_length_ms: request.chart_length_ms,
            play_duration_ms: request.play_duration_ms,
            target_ex_score: request.target_ex_score,
            score_key: request.score_key,
            practice_mode: request.practice_mode,
            finish_mode: request.finish_mode,
        },
        readiness,
    )?;
    finished.summary.target_name = request.target_name.replace('_', " ");
    *cached = Some(finished.clone());
    Ok(finished)
}

pub struct FinishSessionResultOnceRequest<'a> {
    pub profile_paths: &'a ProfilePaths,
    pub replay_config: &'a ReplayConfig,
    pub ir_config: &'a IrConfig,
    pub session: &'a GameSession,
    pub played_at: i64,
    pub applied_arrange: &'a AppliedArrange,
    pub source_ln_profile: ChartLnProfile,
    pub chart_length_ms: Option<u64>,
    pub play_duration_ms: Option<u64>,
    pub target_ex_score: Option<u32>,
    pub target_name: &'a str,
    pub score_key: ScoreKey,
    pub practice_mode: bool,
    pub finish_mode: FinishResultMode,
}

struct FinishSessionResultJob {
    profile_paths: ProfilePaths,
    replay_config: ReplayConfig,
    ir_config: IrConfig,
    snapshot: FinishSessionSnapshot,
    played_at: i64,
    applied_arrange: AppliedArrange,
    source_ln_profile: ChartLnProfile,
    chart_length_ms: Option<u64>,
    play_duration_ms: Option<u64>,
    target_ex_score: Option<u32>,
    target_name: String,
    score_key: ScoreKey,
    practice_mode: bool,
    finish_mode: FinishResultMode,
    result_graph: ResultGraphCollector,
}

pub struct PendingFinishedPlaySession {
    receiver: Receiver<Result<FinishedPlaySession>>,
    worker: Option<JoinHandle<()>>,
    started_at: Instant,
}

impl PendingFinishedPlaySession {
    pub fn try_recv(&self) -> Result<Option<FinishedPlaySession>> {
        match self.receiver.try_recv() {
            Ok(result) => result.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => bail!("play result save worker disconnected"),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// アプリ終了時に保存ワーカーの副作用を完了させる。
    pub fn wait_for_completion(mut self) -> Result<()> {
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| anyhow::anyhow!("play result save worker panicked"))?;
        }
        match self.receiver.try_recv() {
            Ok(result) => result.map(|_| ()),
            Err(TryRecvError::Empty) => bail!("play result save worker completed without a result"),
            Err(TryRecvError::Disconnected) => bail!("play result save worker disconnected"),
        }
    }
}

/// 判定確定済みのセッションを不変スナップショット化し、結果保存を別スレッドで開始する。
pub fn spawn_settled_session_result(
    request: FinishSessionResultOnceRequest<'_>,
    settled_at: TimeUs,
    result_graph: ResultGraphCollector,
) -> Result<PendingFinishedPlaySession> {
    ensure_storable_session(request.session, FinishSessionReadiness::SettledAt(settled_at))?;
    let job = FinishSessionResultJob {
        profile_paths: request.profile_paths.clone(),
        replay_config: request.replay_config.clone(),
        ir_config: request.ir_config.clone(),
        snapshot: FinishSessionSnapshot::from_session(
            request.session,
            request.source_ln_profile,
            request.applied_arrange,
        ),
        played_at: request.played_at,
        applied_arrange: request.applied_arrange.clone(),
        source_ln_profile: request.source_ln_profile,
        chart_length_ms: request.chart_length_ms,
        play_duration_ms: request.play_duration_ms,
        target_ex_score: request.target_ex_score,
        target_name: request.target_name.to_string(),
        score_key: request.score_key,
        practice_mode: request.practice_mode,
        finish_mode: request.finish_mode,
        result_graph,
    };
    let (sender, receiver) = mpsc::channel();
    let worker =
        thread::Builder::new().name("bmz-play-result-save".to_string()).spawn(move || {
            let result = finish_session_result_job(job);
            let _ = sender.send(result);
        })?;
    Ok(PendingFinishedPlaySession { receiver, worker: Some(worker), started_at: Instant::now() })
}

fn finish_session_result_job(job: FinishSessionResultJob) -> Result<FinishedPlaySession> {
    let mut score_db = ScoreDatabase::open(&job.profile_paths.score_db)?;
    let mut network_db = NetworkDatabase::open(&job.profile_paths.network_db)?;
    let mut finished = finish_session_snapshot_result(
        &mut score_db,
        &mut network_db,
        FinishSessionSnapshotResultRequest {
            profile_paths: &job.profile_paths,
            replay_config: &job.replay_config,
            ir_config: &job.ir_config,
            snapshot: &job.snapshot,
            played_at: job.played_at,
            applied_arrange: &job.applied_arrange,
            source_ln_profile: job.source_ln_profile,
            chart_length_ms: job.chart_length_ms,
            play_duration_ms: job.play_duration_ms,
            target_ex_score: job.target_ex_score,
            score_key: job.score_key,
            practice_mode: job.practice_mode,
            finish_mode: job.finish_mode,
        },
    )?;
    finished.summary.graph = Arc::new(job.result_graph.snapshot_for_result_parts(
        &job.snapshot.chart,
        &job.snapshot.result_judgements,
        job.snapshot.failed_gauge.as_ref(),
    ));
    finished.summary.target_name = job.target_name.replace('_', " ");
    Ok(finished)
}

#[derive(Debug, Clone, Copy)]
enum FinishSessionReadiness {
    Terminal,
    SettledAt(TimeUs),
}

fn ensure_storable_session(session: &GameSession, readiness: FinishSessionReadiness) -> Result<()> {
    if matches!(session.state, PlayState::Finished | PlayState::Failed) {
        return Ok(());
    }
    match readiness {
        FinishSessionReadiness::SettledAt(now)
            if session.state == PlayState::Playing
                && bmz_gameplay::session::result_is_settled(session, now) =>
        {
            Ok(())
        }
        FinishSessionReadiness::SettledAt(_) => bail!("play session result is not settled yet"),
        FinishSessionReadiness::Terminal => bail!("play session is not finished yet"),
    }
}

#[cfg(test)]
#[path = "play_finish/tests.rs"]
mod tests;
