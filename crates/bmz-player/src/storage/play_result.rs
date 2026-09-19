use anyhow::Result;
use bmz_core::clear::ClearType;
use bmz_core::input::{InputDeviceKind, InputKind};
use bmz_core::replay::ReplayEvent;
use bmz_gameplay::result::PlayResult;

use crate::config::profile_config::{ReplayConfig, ReplaySlotRule};
use crate::ln_policy::LnScorePolicy;
use crate::paths::ProfilePaths;
use crate::screens::play_session::SRandomScheme;
use crate::select_options::{ArrangeOption, DoubleOption, DoubleOptionScoreBucket};

use super::replay::{
    ReplayFile, replay_file_name, replay_slot_file_name, save_replay, save_replay_with_hash,
};
use super::score_db::{
    ReplaySlotRecord, ScoreDatabase, ScoreInsertMode, ScoreRecord, ScoreRecordMetadata,
};

#[derive(Debug, Clone)]
pub struct StoredPlayResult {
    pub score_history_id: i64,
    pub played_at: i64,
    pub replay_path: String,
    /// 保存したリプレイファイル内容の SHA256 (hex)。保存時に serialize 済み
    /// バイト列から計算するので、IR 送信時にファイルを読み直す必要がない。
    pub replay_sha256: Option<String>,
    pub slot_paths: [Option<String>; 4],
    pub device_type: InputDeviceKind,
}

#[derive(Debug, Clone)]
pub struct StorePlayResultRequest {
    pub played_at: i64,
    pub playtime_seconds: u32,
    pub ln_policy: LnScorePolicy,
    /// Score aggregation bucket. FLIP deliberately shares the Off bucket.
    pub double_option: DoubleOptionScoreBucket,
    /// DP option actually applied to the chart, retained in score history.
    pub applied_double_option: DoubleOption,
    pub random_seed: Option<i64>,
    pub gauge_option: String,
    pub rule_mode: String,
    pub assist_mask: u32,
    pub replay_events: Vec<ReplayEvent>,
    pub branch_decisions: Option<Vec<bmz_core::replay::BranchDecision>>,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub arrange_seed: Option<i64>,
    pub arrange_seed_2p: Option<i64>,
    pub bms_random_choices: Vec<i32>,
    pub bms_switch_choices: Vec<u64>,
    pub seed_scheme: String,
    pub s_random_scheme: SRandomScheme,
    pub s_random_scheme_2p: Option<SRandomScheme>,
    pub h_random_threshold_ms: Option<u32>,
    pub arrange_pattern: Option<Vec<u8>>,
    /// false の場合は beatoraja の `updateScore=false` と同様に、
    /// クリアランプ・回数・profile 全体統計だけを更新する。
    pub update_score: bool,
    pub mode: StorePlayResultMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorePlayResultMode {
    Normal,
    CourseStage,
}

impl StorePlayResultMode {
    fn score_insert_mode(self) -> ScoreInsertMode {
        match self {
            Self::Normal => ScoreInsertMode::Full,
            Self::CourseStage => ScoreInsertMode::Full,
        }
    }

    fn save_replay_slots(self) -> bool {
        match self {
            Self::Normal => true,
            Self::CourseStage => false,
        }
    }

    fn stored_clear_type(self, clear_type: ClearType) -> ClearType {
        match self {
            Self::Normal => clear_type,
            Self::CourseStage => course_stage_clear_type(clear_type),
        }
    }
}

pub fn course_stage_clear_type(clear_type: ClearType) -> ClearType {
    match clear_type {
        ClearType::FullCombo | ClearType::Perfect | ClearType::Max => clear_type,
        _ => ClearType::NoPlay,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CandidateMetrics {
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub clear_rank: u8,
}

pub fn store_play_result(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    result: &PlayResult,
    request: StorePlayResultRequest,
) -> Result<StoredPlayResult> {
    let arrange = request.arrange;
    let arrange_2p = request.arrange_2p;
    let arrange_seed = request.arrange_seed;
    let arrange_seed_2p = request.arrange_seed_2p;
    let bms_random_choices = request.bms_random_choices.clone();
    let bms_switch_choices = request.bms_switch_choices.clone();
    let arrange_pattern = request.arrange_pattern.clone();
    let replay_events = request.replay_events.clone();
    let rule_mode = bmz_gameplay::rule::RuleMode::from_str_opt(&request.rule_mode)
        .unwrap_or(bmz_gameplay::rule::RuleMode::Beatoraja);
    let device_type = classify_replay_device_type(&replay_events);

    let (replay_path, replay_sha256) = if request.update_score
        && should_save_replay(replay_config, result)
    {
        let file_name = replay_file_name(result.chart_sha256, request.played_at);
        let path = profile_paths.replay_dir.join(&file_name);
        let replay = ReplayFile::new_with_policy(
            result.chart_sha256,
            request.ln_policy,
            request.double_option,
            request.played_at,
            request.random_seed,
            arrange,
            arrange_2p,
            arrange_seed,
            arrange_pattern.clone(),
            replay_events.clone(),
        )
        .with_branch_decisions(request.branch_decisions.clone())
        .with_randomization(arrange_seed_2p, bms_random_choices.clone(), bms_switch_choices.clone())
        .with_seed_scheme(request.seed_scheme.clone())
        .with_s_random_schemes(request.s_random_scheme, request.s_random_scheme_2p)
        .with_playback_metadata(
            request.applied_double_option,
            result.gauge_type,
            request.h_random_threshold_ms,
        );
        let hash = save_replay_with_hash(&path, &replay)?;
        (format!("replay/{file_name}"), Some(hash))
    } else {
        (String::new(), None)
    };

    let mut record = ScoreRecord::from_play_result(
        result,
        ScoreRecordMetadata::new(
            request.ln_policy,
            request.double_option,
            request.played_at,
            request.random_seed,
            arrange.to_persistent_str(),
            request.gauge_option,
            request.rule_mode.clone(),
            request.assist_mask,
            device_type,
            replay_path.clone(),
        )
        .with_applied_double_option(request.applied_double_option)
        .with_arrange_2p(arrange_2p.to_persistent_str())
        .with_seed_scheme(request.seed_scheme.clone())
        .with_playtime_seconds(request.playtime_seconds),
    );
    record.clear_type = request.mode.stored_clear_type(result.clear_type);
    let score_history_id = if request.update_score {
        score_db.insert_score_with_mode(&record, request.mode.score_insert_mode())?
    } else {
        score_db.update_score_clear_only(&record)?;
        0
    };

    let mut slot_paths: [Option<String>; 4] = [None, None, None, None];
    if request.update_score
        && request.mode.save_replay_slots()
        && should_save_replay(replay_config, result)
    {
        let candidate = candidate_metrics(result);
        for (slot_index, &rule) in replay_config.slot_rules.iter().enumerate() {
            let slot = slot_index as u8;
            let key = super::score_db::ScoreKey::with_options(
                result.chart_sha256,
                request.ln_policy,
                request.double_option,
                rule_mode,
            );
            let prev = score_db.replay_slot(key, slot)?;
            if !evaluate_slot_update(rule, prev.as_ref(), &candidate) {
                continue;
            }
            let file_name = replay_slot_file_name(
                result.chart_sha256,
                request.ln_policy,
                request.double_option,
                rule_mode,
                slot,
            );
            let path = profile_paths.replay_dir.join(&file_name);
            let replay = ReplayFile::new_with_policy(
                result.chart_sha256,
                request.ln_policy,
                request.double_option,
                request.played_at,
                request.random_seed,
                arrange,
                arrange_2p,
                arrange_seed,
                arrange_pattern.clone(),
                replay_events.clone(),
            )
            .with_branch_decisions(request.branch_decisions.clone())
            .with_randomization(
                arrange_seed_2p,
                bms_random_choices.clone(),
                bms_switch_choices.clone(),
            )
            .with_seed_scheme(request.seed_scheme.clone())
            .with_s_random_schemes(request.s_random_scheme, request.s_random_scheme_2p)
            .with_playback_metadata(
                request.applied_double_option,
                result.gauge_type,
                request.h_random_threshold_ms,
            );
            save_replay(&path, &replay)?;
            let rel_path = format!("replay/{file_name}");
            score_db.upsert_replay_slot(&ReplaySlotRecord {
                chart_sha256: result.chart_sha256,
                ln_policy: request.ln_policy,
                double_option: request.double_option,
                rule_mode,
                slot,
                rule,
                replay_path: rel_path.clone(),
                played_at: request.played_at,
                ex_score: Some(candidate.ex_score),
                bp: Some(candidate.bp),
                cb: Some(candidate.cb),
                max_combo: Some(candidate.max_combo),
                clear_rank: Some(candidate.clear_rank),
                source_kind: super::score_db::ScoreSourceKind::Local,
                source_path: String::new(),
                source_fingerprint: String::new(),
            })?;
            slot_paths[slot_index] = Some(rel_path);
        }
    }

    Ok(StoredPlayResult {
        score_history_id,
        played_at: request.played_at,
        replay_path,
        replay_sha256,
        slot_paths,
        device_type,
    })
}

pub fn save_existing_replay_to_slot(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    result: &PlayResult,
    stored: &StoredPlayResult,
    ln_policy: LnScorePolicy,
    double_option: DoubleOptionScoreBucket,
    rule_mode: bmz_gameplay::rule::RuleMode,
    slot: u8,
) -> Result<Option<String>> {
    if slot > 3 || stored.replay_path.is_empty() || result.autoplay {
        return Ok(None);
    }
    let source = profile_paths.root_dir.join(&stored.replay_path);
    if !source.is_file() {
        return Ok(None);
    }
    std::fs::create_dir_all(&profile_paths.replay_dir)?;
    let file_name =
        replay_slot_file_name(result.chart_sha256, ln_policy, double_option, rule_mode, slot);
    let path = profile_paths.replay_dir.join(&file_name);
    std::fs::copy(&source, &path)?;
    let rel_path = format!("replay/{file_name}");
    let candidate = candidate_metrics(result);
    score_db.upsert_replay_slot(&ReplaySlotRecord {
        chart_sha256: result.chart_sha256,
        ln_policy,
        double_option,
        rule_mode,
        slot,
        rule: ReplaySlotRule::Always,
        replay_path: rel_path.clone(),
        played_at: stored.played_at,
        ex_score: Some(candidate.ex_score),
        bp: Some(candidate.bp),
        cb: Some(candidate.cb),
        max_combo: Some(candidate.max_combo),
        clear_rank: Some(candidate.clear_rank),
        source_kind: super::score_db::ScoreSourceKind::Local,
        source_path: String::new(),
        source_fingerprint: String::new(),
    })?;
    Ok(Some(rel_path))
}

pub fn candidate_metrics(result: &PlayResult) -> CandidateMetrics {
    CandidateMetrics {
        ex_score: result.score.ex_score(),
        bp: result.record_bp(),
        cb: result.record_cb(),
        max_combo: result.score.max_combo,
        clear_rank: result.clear_type as u8,
    }
}

pub fn classify_replay_device_type(events: &[ReplayEvent]) -> InputDeviceKind {
    let (keyboard, controller) = events.iter().filter(|event| event.kind == InputKind::Press).fold(
        (0_u32, 0_u32),
        |(keyboard, controller), event| match event.device_kind {
            InputDeviceKind::Keyboard => (keyboard + 1, controller),
            InputDeviceKind::Controller => (keyboard, controller + 1),
        },
    );
    if controller > keyboard { InputDeviceKind::Controller } else { InputDeviceKind::Keyboard }
}

fn evaluate_slot_update(
    rule: ReplaySlotRule,
    prev: Option<&ReplaySlotRecord>,
    next: &CandidateMetrics,
) -> bool {
    let prev_metrics = prev.and_then(|p| Some((p.ex_score?, p.bp?, p.max_combo?, p.clear_rank?)));
    slot_rule_passes(rule, prev_metrics, next)
}

/// Rule-only comparison shared by per-chart `replay_slots` and per-course
/// `course_replay_slots`.  `prev` is `(ex_score, bp, max_combo,
/// clear_rank)` of the row currently in the slot, or `None` if the slot is
/// empty (in which case any enabled rule passes — the first record always wins).
pub fn slot_rule_passes(
    rule: ReplaySlotRule,
    prev: Option<(u32, u32, u32, u8)>,
    next: &CandidateMetrics,
) -> bool {
    if matches!(rule, ReplaySlotRule::Disabled) {
        return false;
    }
    if matches!(rule, ReplaySlotRule::Always) {
        return true;
    }
    let Some((prev_ex, prev_bp, prev_combo, prev_clear)) = prev else {
        return true;
    };
    match rule {
        ReplaySlotRule::Disabled => false,
        ReplaySlotRule::Always => true,
        ReplaySlotRule::ScoreUpdate => next.ex_score > prev_ex,
        ReplaySlotRule::BpUpdate => next.bp < prev_bp,
        ReplaySlotRule::MaxComboUpdate => next.max_combo > prev_combo,
        ReplaySlotRule::ClearUpdate => next.clear_rank > prev_clear,
    }
}

fn should_save_replay(config: &ReplayConfig, result: &PlayResult) -> bool {
    // オートプレイの記録は保存しない (save_autoplay_runs は廃止: 常に false)
    // 失敗ランは保存する (save_failed_runs は廃止: 常に true)
    config.auto_save && !result.autoplay
}

#[cfg(test)]
#[path = "play_result/tests.rs"]
mod tests;
