use anyhow::{Result, bail};
use bmz_chart::model::LongNoteMode;
use bmz_core::clear::ClearType;
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::session::{GameSession, PlayState};

use crate::config::profile_config::{IrConfig, ReplayConfig};
use crate::ir::payload::{IrSubmissionContext, build_score_submission};
use crate::paths::ProfilePaths;
use crate::screens::play_session::AppliedArrange;
use crate::screens::result_model::ResultSummary;
use crate::storage::play_result::{StorePlayResultRequest, StoredPlayResult, store_play_result};
use crate::storage::score_db::NewIrScoreJob;
use crate::storage::score_db::{ScoreDatabase, ScoreKey};

#[derive(Debug, Clone)]
pub struct FinishedPlaySession {
    pub result: PlayResult,
    pub stored: StoredPlayResult,
    pub summary: ResultSummary,
    pub replay_playback: bool,
    pub arrange: crate::select_options::ArrangeOption,
    pub applied_arrange: AppliedArrange,
    /// IR ランキング照会に使うスコア分離キー。
    pub ln_policy: crate::ln_policy::LnScorePolicy,
}

pub fn play_result_from_session(session: &GameSession) -> PlayResult {
    PlayResult::from_states(
        &session.chart,
        &session.score,
        &session.gauge,
        session.state,
        session.autoplay.is_some(),
    )
}

pub fn store_session_result(
    score_db: &mut ScoreDatabase,
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
        profile_paths,
        replay_config,
        ir_config,
        session,
        played_at,
        applied_arrange,
        None,
        score_key,
        practice_mode,
    )?
    .stored)
}

pub fn finish_session_result(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    ir_config: &IrConfig,
    session: &GameSession,
    played_at: i64,
    applied_arrange: &AppliedArrange,
    target_ex_score: Option<u32>,
    score_key: ScoreKey,
    practice_mode: bool,
) -> Result<FinishedPlaySession> {
    ensure_storable_state(session.state)?;
    let result = play_result_from_session(session);
    let replay_playback = session.replay_player.is_some();
    let previous_best =
        score_db.best_scores_for_charts(&[score_key]).ok().and_then(|mut bests| bests.pop());
    // オートプレイ / リプレイ再生 / プラクティス時はスコア・リプレイをDBに保存しない
    // （リザルト画面の表示のみ行う）。
    let stored = if session.autoplay.is_some() || replay_playback || practice_mode {
        StoredPlayResult {
            score_history_id: 0,
            replay_path: String::new(),
            slot_paths: [None, None, None, None],
            device_type: InputDeviceKind::Keyboard,
        }
    } else {
        let arrange = applied_arrange.arrange;
        let arrange_seed = applied_arrange.seed;
        let arrange_pattern = applied_arrange.pattern.clone();
        store_play_result(
            score_db,
            profile_paths,
            replay_config,
            &result,
            StorePlayResultRequest {
                played_at,
                ln_policy: score_key.ln_policy,
                random_seed: arrange_seed,
                gauge_option: String::new(),
                rule_mode: session.rule_mode.as_str().to_string(),
                assist_mask: 0,
                replay_events: session.replay_recorder.events.clone(),
                arrange,
                arrange_seed,
                arrange_pattern,
            },
        )?
    };
    let mut summary = ResultSummary::from_play_result(&result, &stored, &session.chart);
    summary.arrange = applied_arrange.arrange.as_str().to_string();
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
    if let Ok(bests) = score_db.best_scores_for_charts(&[score_key])
        && let Some(best) = bests.into_iter().next()
    {
        summary.best_ex_score = Some(best.ex_score);
        summary.best_clear_type = clear_type_from_name(&best.clear_type);
        summary.best_max_combo = Some(best.max_combo);
        summary.best_bp = Some(best.bp);
    }
    if let Ok(slots) = score_db.replay_slots_for_chart(score_key) {
        summary.replay_slots = slots.each_ref().map(Option::is_some);
        for (index, saved) in summary.saved_replay_slots.iter().enumerate() {
            if *saved {
                summary.replay_slots[index] = true;
            }
        }
    }
    enqueue_ir_jobs(
        score_db,
        profile_paths,
        ir_config,
        session,
        &result,
        &stored,
        played_at,
        score_key,
        applied_arrange,
        &mut summary,
        previous_best.as_ref(),
    );

    Ok(FinishedPlaySession {
        result,
        stored,
        summary,
        replay_playback,
        arrange: applied_arrange.arrange,
        applied_arrange: applied_arrange.clone(),
        ln_policy: score_key.ln_policy,
    })
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

#[allow(clippy::too_many_arguments)]
fn enqueue_ir_jobs(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    ir_config: &IrConfig,
    session: &GameSession,
    result: &PlayResult,
    stored: &StoredPlayResult,
    played_at: i64,
    score_key: ScoreKey,
    applied_arrange: &AppliedArrange,
    summary: &mut ResultSummary,
    previous_best: Option<&crate::storage::score_db::BestScoreSummary>,
) {
    if stored.score_history_id <= 0 {
        return;
    }
    let enabled: Vec<_> = ir_config
        .providers
        .iter()
        .filter(|provider| {
            provider.enabled && should_send_ir_score(provider.send_policy, result, previous_best)
        })
        .collect();
    if enabled.is_empty() {
        return;
    }
    let payload = build_score_submission(
        &session.chart,
        result,
        IrSubmissionContext {
            played_at,
            ln_policy: score_key.ln_policy,
            effective_ln_mode: effective_ln_mode_from_score_policy(score_key.ln_policy),
            gauge_option: result.gauge_type.as_str().to_string(),
            device_type: stored.device_type,
            idempotency_key: format!("bmz-score-{}", stored.score_history_id),
            arrange: applied_arrange.arrange,
            arrange_seed: applied_arrange.seed,
            random_seed: applied_arrange.seed,
            rule_mode: session.rule_mode.as_str().to_string(),
            replay_hash: replay_file_hash(profile_paths, &stored.replay_path),
        },
    );
    let Ok(payload_json) = serde_json::to_string(&payload) else {
        summary.ir_last_error = Some("failed to serialize IR payload".to_string());
        return;
    };
    for provider in enabled {
        match score_db.enqueue_ir_score_job(&NewIrScoreJob {
            provider: provider.provider.clone(),
            account_id: provider.account_id.clone(),
            kind: crate::storage::score_db::IrJobKind::Score,
            local_score_id: stored.score_history_id,
            chart_sha256: result.chart_sha256,
            ln_policy: score_key.ln_policy,
            payload_json: payload_json.clone(),
            now: played_at,
        }) {
            Ok(_) => summary.ir_queued_jobs += 1,
            Err(error) => {
                summary.ir_last_error = Some(error.to_string());
                tracing::warn!(provider = provider.provider, %error, "failed to enqueue IR score job");
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

/// 保存済みリプレイファイルの SHA256 (hex)。ファイルが無ければ None。
fn replay_file_hash(profile_paths: &ProfilePaths, replay_path: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    if replay_path.is_empty() {
        return None;
    }
    let bytes = std::fs::read(profile_paths.root_dir.join(replay_path)).ok()?;
    Some(crate::storage::common::hash_to_hex(&Sha256::digest(&bytes)))
}

fn effective_ln_mode_from_score_policy(policy: crate::ln_policy::LnScorePolicy) -> LongNoteMode {
    match policy {
        crate::ln_policy::LnScorePolicy::AutoLn | crate::ln_policy::LnScorePolicy::ForceLn => {
            LongNoteMode::Ln
        }
        crate::ln_policy::LnScorePolicy::AutoCn | crate::ln_policy::LnScorePolicy::ForceCn => {
            LongNoteMode::Cn
        }
        crate::ln_policy::LnScorePolicy::AutoHcn | crate::ln_policy::LnScorePolicy::ForceHcn => {
            LongNoteMode::Hcn
        }
    }
}

pub fn finish_session_result_once(
    cached: &mut Option<FinishedPlaySession>,
    score_db: &mut ScoreDatabase,
    request: FinishSessionResultOnceRequest<'_>,
) -> Result<FinishedPlaySession> {
    if let Some(finished) = cached.clone() {
        return Ok(finished);
    }

    let finished = finish_session_result(
        score_db,
        request.profile_paths,
        request.replay_config,
        request.ir_config,
        request.session,
        request.played_at,
        request.applied_arrange,
        request.target_ex_score,
        request.score_key,
        request.practice_mode,
    )?;
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
    pub target_ex_score: Option<u32>,
    pub score_key: ScoreKey,
    pub practice_mode: bool,
}

fn ensure_storable_state(state: PlayState) -> Result<()> {
    match state {
        PlayState::Finished | PlayState::Failed => Ok(()),
        PlayState::Ready | PlayState::Playing => bail!("play session is not finished yet"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};
    use bmz_core::clear::ClearType;
    use bmz_core::ids::NoteId;
    use bmz_core::input::{InputDeviceKind, InputKind, InputSource};
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};
    use bmz_gameplay::input::backend::NullInputBackend;
    use bmz_gameplay::input::binding::LaneBinding;
    use bmz_gameplay::input::system::InputSystem;
    use bmz_gameplay::input::translator::DefaultInputTranslator;
    use bmz_gameplay::judge::engine::JudgeEngine;
    use bmz_gameplay::replay::ReplayRecorder;
    use bmz_gameplay::session::{BgmScheduler, GameSession, PlayAudioMix, PlayOffsets, PlayState};
    use rusqlite::Connection;

    use super::*;
    use crate::config::play::DEFAULT_JUDGE_WINDOW;
    use crate::config::profile_config::{IrConfig, IrProviderConfig, ReplayConfig};
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

    #[test]
    fn should_send_ir_score_follows_policy() {
        use crate::config::profile_config::IrSendPolicyConfig;
        use crate::storage::score_db::BestScoreSummary;

        let mut result = play_result_from_session(&session());
        result.gauge_value = 0.0;
        // Failed (ゲージ 0) は CompleteSong では送らない。
        assert!(should_send_ir_score(IrSendPolicyConfig::Always, &result, None));
        assert!(!should_send_ir_score(IrSendPolicyConfig::CompleteSong, &result, None));
        result.gauge_value = 12.0;
        assert!(should_send_ir_score(IrSendPolicyConfig::CompleteSong, &result, None));

        // UpdateScore: ベストが無ければ送る。
        assert!(should_send_ir_score(IrSendPolicyConfig::UpdateScore, &result, None));

        let best = BestScoreSummary {
            chart_sha256: [0; 32],
            ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
            clear_type: "Hard".to_string(),
            gauge_type: "Normal".to_string(),
            gauge_value: 100.0,
            ex_score: 100,
            bp: 0,
            cb: 0,
            max_combo: 100,
            judge_counts: Default::default(),
            fast_slow_counts: Default::default(),
            play_count: 1,
            clear_count: 1,
            device_type: InputDeviceKind::Keyboard,
            played_at: 0,
            replay_path: String::new(),
        };
        // session() の結果は EX 0 / Failed / combo 0 / BP 1 なので全項目で劣る。
        assert!(!should_send_ir_score(IrSendPolicyConfig::UpdateScore, &result, Some(&best)));
        // EX が改善すれば送る。
        let mut improved = result.clone();
        improved.score.judges.fast_pgreat = 100;
        assert!(should_send_ir_score(IrSendPolicyConfig::UpdateScore, &improved, Some(&best)));
    }

    #[test]
    fn play_result_from_session_uses_session_state() {
        let session = session();

        let result = play_result_from_session(&session);

        assert_eq!(result.chart_sha256, session.chart.identity.file_sha256);
        assert_eq!(result.clear_type, ClearType::Failed);
        assert!(!result.autoplay);
    }

    #[test]
    fn store_session_result_writes_replay_events() {
        let root = make_temp_dir("finish-session");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let mut session = session();
        session.replay_recorder.record(bmz_core::input::InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(10),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        });

        let stored = store_session_result(
            &mut score_db,
            &paths,
            &replay_config,
            &crate::config::profile_config::IrConfig::default(),
            &session,
            1_700_000_100,
            &AppliedArrange::default(),
            score_key(&session),
            false,
        )
        .unwrap();

        assert!(stored.score_history_id > 0);
        assert!(!stored.replay_path.is_empty());
        assert!(root.join(&stored.replay_path).exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_session_result_returns_summary() {
        let root = make_temp_dir("finish-summary");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let session = session();
        let lane_shuffle_pattern = (0..bmz_core::lane::LANE_COUNT as u8).rev().collect::<Vec<_>>();
        let applied_arrange = AppliedArrange {
            arrange: crate::select_options::ArrangeOption::Random,
            seed: Some(42),
            pattern: Some(lane_shuffle_pattern.clone()),
        };

        let finished = finish_session_result(
            &mut score_db,
            &paths,
            &replay_config,
            &crate::config::profile_config::IrConfig::default(),
            &session,
            1_700_000_102,
            &applied_arrange,
            Some(1600),
            score_key(&session),
            false,
        )
        .unwrap();

        assert_eq!(finished.summary.score_history_id, finished.stored.score_history_id);
        assert_eq!(finished.summary.clear_type, finished.result.clear_type);
        assert_eq!(finished.summary.arrange, "RANDOM");
        assert_eq!(finished.summary.lane_shuffle_pattern, lane_shuffle_pattern);
        assert_eq!(finished.summary.target_ex_score, Some(1600));
        assert_eq!(finished.summary.saved_replay_slots, [true; 4]);
        assert_eq!(finished.summary.replay_slots, [true; 4]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_session_result_enqueues_ir_jobs_for_enabled_providers() {
        let root = make_temp_dir("finish-ir");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: false,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let ir_config = IrConfig {
            primary_provider: "bmz-official".to_string(),
            providers: vec![IrProviderConfig {
                provider: "bmz-official".to_string(),
                base_url: String::new(),
                enabled: true,
                account_display_name: "Player".to_string(),
                account_id: "account-1".to_string(),
                send_policy: Default::default(),
                role: Default::default(),
                last_login_at: None,
                last_success_at: None,
            }],
            ..IrConfig::default()
        };
        let session = session();

        let finished = finish_session_result(
            &mut score_db,
            &paths,
            &replay_config,
            &ir_config,
            &session,
            1_700_000_108,
            &AppliedArrange::default(),
            None,
            score_key(&session),
            false,
        )
        .unwrap();

        assert_eq!(finished.summary.ir_queued_jobs, 1);
        assert_eq!(score_db.pending_ir_score_jobs(1_700_000_108, 10).unwrap().len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_session_result_once_reuses_cached_result() {
        let root = make_temp_dir("finish-once");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let session = session();
        let mut cached = None;

        let first = finish_session_result_once(
            &mut cached,
            &mut score_db,
            FinishSessionResultOnceRequest {
                profile_paths: &paths,
                replay_config: &replay_config,
                ir_config: &crate::config::profile_config::IrConfig::default(),
                session: &session,
                played_at: 1_700_000_103,
                applied_arrange: &AppliedArrange::default(),
                target_ex_score: None,
                score_key: score_key(&session),
                practice_mode: false,
            },
        )
        .unwrap();
        let second = finish_session_result_once(
            &mut cached,
            &mut score_db,
            FinishSessionResultOnceRequest {
                profile_paths: &paths,
                replay_config: &replay_config,
                ir_config: &crate::config::profile_config::IrConfig::default(),
                session: &session,
                played_at: 1_700_000_104,
                applied_arrange: &AppliedArrange::default(),
                target_ex_score: None,
                score_key: score_key(&session),
                practice_mode: false,
            },
        )
        .unwrap();

        assert_eq!(first.stored.score_history_id, second.stored.score_history_id);
        assert_eq!(score_db.recent_history(10, 0).unwrap().len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_session_result_skips_storage_for_autoplay() {
        let root = make_temp_dir("finish-autoplay");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let mut session = session();
        session.autoplay = Some(bmz_gameplay::autoplay::AutoplayController::default());

        let finished = finish_session_result(
            &mut score_db,
            &paths,
            &replay_config,
            &crate::config::profile_config::IrConfig::default(),
            &session,
            1_700_000_105,
            &AppliedArrange::default(),
            None,
            score_key(&session),
            false,
        )
        .unwrap();

        // オートプレイ時はDB保存・リプレイ保存をしない。
        assert_eq!(finished.stored.score_history_id, 0);
        assert!(finished.stored.replay_path.is_empty());
        assert!(finished.stored.slot_paths.iter().all(Option::is_none));
        assert_eq!(score_db.recent_history(10, 0).unwrap().len(), 0);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_session_result_skips_storage_for_replay_playback() {
        let root = make_temp_dir("finish-replay");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let mut session = session();
        session.replay_player = Some(bmz_gameplay::replay::ReplayPlayer::default());

        let finished = finish_session_result(
            &mut score_db,
            &paths,
            &replay_config,
            &crate::config::profile_config::IrConfig::default(),
            &session,
            1_700_000_106,
            &AppliedArrange::default(),
            None,
            score_key(&session),
            false,
        )
        .unwrap();

        assert!(finished.replay_playback);
        assert_eq!(finished.stored.score_history_id, 0);
        assert!(finished.stored.replay_path.is_empty());
        assert!(finished.stored.slot_paths.iter().all(Option::is_none));
        assert_eq!(score_db.recent_history(10, 0).unwrap().len(), 0);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_session_result_rejects_unfinished_session() {
        let root = make_temp_dir("unfinished-session");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let mut session = session();
        session.state = PlayState::Playing;

        let result = store_session_result(
            &mut score_db,
            &paths,
            &replay_config,
            &crate::config::profile_config::IrConfig::default(),
            &session,
            1_700_000_101,
            &AppliedArrange::default(),
            score_key(&session),
            false,
        );

        assert!(result.is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    fn session() -> GameSession {
        let chart = Arc::new(chart());
        let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
            chart.metadata.initial_bpm,
            &chart.timing_events,
        );
        GameSession {
            chart: Arc::clone(&chart),
            timing_map,
            audio_clock: bmz_audio::clock::AudioClock::stopped(48_000),
            input_system: InputSystem {
                backend: Box::new(NullInputBackend),
                translator: Box::new(DefaultInputTranslator {
                    binding: LaneBinding { entries: Vec::new() },
                }),
            },
            judge: JudgeEngine::new(DEFAULT_JUDGE_WINDOW),
            base_judge_window: DEFAULT_JUDGE_WINDOW,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            score: Default::default(),
            gauge: bmz_gameplay::gauge::GaugeState::new(
                bmz_core::clear::GaugeType::Normal,
                160.0,
                chart.total_notes,
            ),
            replay_recorder: ReplayRecorder::default(),
            replay_player: None,
            autoplay: None,
            recent_inputs: Vec::new(),
            lane_keyon_started_at: Default::default(),
            lane_keyoff_started_at: Default::default(),
            lane_scratch_direction: Default::default(),
            lane_scratch_angle_delta_ms: Default::default(),
            scratch_angle_last_render_at: None,
            lane_auto_release_at: Default::default(),
            recent_judgements: Vec::new(),
            result_judgements: Default::default(),
            hit_error_ring: bmz_gameplay::hit_error::HitErrorRing::default(),
            full_combo_started_at: None,
            bgm_scheduler: BgmScheduler::default(),
            offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
            audio_mix: PlayAudioMix { master_volume: 1.0, key_volume: 1.0, bgm_volume: 1.0 },
            hispeed: 2.0,
            hispeed_mode: bmz_gameplay::session::HispeedMode::Normal,
            target_green_number: 300,
            lift: 0.0,
            lane_cover: 0.0,
            lane_cover_visible: true,
            lane_cover_changing: false,
            lanecover_enabled: false,
            lift_enabled: true,
            hidden_enabled: false,
            hidden_cover: 0.0,
            skin_offsets: Vec::new(),
            bga_enabled: true,
            poor_bga_duration_us: 500_000,
            bga_stretch: 1,
            show_ln_tail_cap: false,
            lane_hcn_timer: [None; bmz_core::lane::LANE_COUNT],
            lane_hcn_keysound_muted: [None; bmz_core::lane::LANE_COUNT],
            pending_keysound_volumes: Vec::new(),
            hsfix_index: 0,
            input_timestamp_anchor: None,
            pending_mine_hits: Vec::new(),
            state: PlayState::Finished,
            last_hcn_gauge_at: None,
        }
    }

    fn chart() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: None,
            damage: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(note);

        PlayableChart {
            identity: compute_chart_identity(b"finish-session"),
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),

            scroll_events: Vec::new(),

            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(0),
        }
    }

    fn score_key(session: &GameSession) -> ScoreKey {
        ScoreKey::new(session.chart.identity.file_sha256, crate::ln_policy::LnScorePolicy::ForceLn)
    }

    fn make_temp_dir(label: &str) -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-player-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
