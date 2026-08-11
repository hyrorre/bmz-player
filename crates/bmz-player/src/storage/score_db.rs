use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE;
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::rule::RuleMode;
use bmz_gameplay::score::ScoreState;
use bmz_render::snapshot::{DisplayJudgeCounts, FastSlowJudgeCounts};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};
pub use super::course_score_db::{
    CourseBestScore, CourseReplayRecord, CourseReplaySlotRecord, CourseScoreChartRecord,
    CourseScoreEntry, CourseScoreInsert,
};
use crate::config::profile_config::ReplaySlotRule;
use crate::ln_policy::LnScorePolicy;
use crate::select_options::{DoubleOption, DoubleOptionScoreBucket};

mod codec;
mod database_course;
mod database_history;
mod database_import;
mod database_query;
mod database_replay;
mod database_stats;
mod reconcile;
mod rows;
mod write;

pub use codec::{decode_beatoraja_ghost, encode_beatoraja_ghost};
use reconcile::*;
use rows::*;
use write::*;

pub struct ScoreDatabase {
    conn: Connection,
}

/// Each score key occupies four SQLite bind variables. Keep batches below the
/// historical 999-variable default while leaving room for future predicates.
const SCORE_KEY_LOOKUP_BATCH_SIZE: usize = 200;

/// Score history provenance.  This is intentionally not part of [`ScoreKey`]:
/// imported results and locally played results compete for the same best score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScoreSourceKind {
    #[default]
    Local,
    Beatoraja,
    Lr2,
    Lr2Oraja,
    Lr2OrajaDx,
}

impl ScoreSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Beatoraja => "Beatoraja",
            Self::Lr2 => "Lr2",
            Self::Lr2Oraja => "Lr2Oraja",
            Self::Lr2OrajaDx => "Lr2OrajaDx",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "Local" => Some(Self::Local),
            "Beatoraja" => Some(Self::Beatoraja),
            "Lr2" => Some(Self::Lr2),
            "Lr2Oraja" => Some(Self::Lr2Oraja),
            "Lr2OrajaDx" => Some(Self::Lr2OrajaDx),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInfo {
    pub player_uuid: String,
    pub display_name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStats {
    pub play_count: u64,
    pub clear_count: u64,
    pub playtime_seconds: u64,
    pub max_combo: u32,
    pub fast_pgreat: u64,
    pub slow_pgreat: u64,
    pub fast_great: u64,
    pub slow_great: u64,
    pub fast_good: u64,
    pub slow_good: u64,
    pub fast_bad: u64,
    pub slow_bad: u64,
    pub fast_poor: u64,
    pub slow_poor: u64,
    pub fast_empty_poor: u64,
    pub slow_empty_poor: u64,
    pub updated_at: i64,
}

/// Profile-wide score aggregates for one local-time day.
///
/// Unlike [`PlayerStats`], this is derived from `score_history` on demand so
/// the day boundary does not require another set of persisted counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DailyPlayerStats {
    pub play_count: u64,
    pub clear_count: u64,
    pub pgreat: u64,
    pub great: u64,
    pub good: u64,
    pub bad: u64,
    pub poor: u64,
    pub empty_poor: u64,
    pub score_update_count: u64,
    pub clear_update_count: u64,
    pub miss_count_update_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteCountAggregate {
    pub label: String,
    pub play_count: u64,
    pub total_notes: u64,
}

/// Local play timestamps where a chart improved its lamp or EX score.
///
/// The map returned by [`ScoreDatabase::chart_update_times_since`] is keyed by
/// the same score aggregation dimensions as `score_best`, so LN policy and
/// rule mode changes do not leak into a virtual folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChartUpdateTimes {
    pub lamp: Vec<i64>,
    pub score: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScoreKey {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub rule_mode: RuleMode,
}

impl ScoreKey {
    pub const fn new(chart_sha256: [u8; 32], ln_policy: LnScorePolicy) -> Self {
        Self {
            chart_sha256,
            ln_policy,
            double_option: DoubleOptionScoreBucket::Off,
            rule_mode: RuleMode::Beatoraja,
        }
    }

    pub const fn with_double_option(
        chart_sha256: [u8; 32],
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
    ) -> Self {
        Self { chart_sha256, ln_policy, double_option, rule_mode: RuleMode::Beatoraja }
    }

    pub const fn with_options(
        chart_sha256: [u8; 32],
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        rule_mode: RuleMode,
    ) -> Self {
        Self { chart_sha256, ln_policy, double_option, rule_mode }
    }

    pub const fn with_rule_mode(self, rule_mode: RuleMode) -> Self {
        Self { rule_mode, ..self }
    }
}

fn score_key_query_params(keys: &[ScoreKey]) -> Vec<String> {
    let mut params = Vec::with_capacity(keys.len() * 4);
    for key in keys {
        params.push(hash_to_hex(&key.chart_sha256));
        params.push(key.ln_policy.as_str().to_string());
        params.push(key.double_option.as_str().to_string());
        params.push(key.rule_mode.as_str().to_string());
    }
    params
}

#[derive(Debug, Clone)]
pub struct ScoreRecord {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    /// Score aggregation key. FLIP deliberately shares the Off bucket.
    pub double_option: DoubleOptionScoreBucket,
    /// The DP option actually applied to this play, retained independently
    /// from the aggregation bucket so FLIP history is not lost.
    pub applied_double_option: DoubleOption,
    pub played_at: i64,
    pub clear_type: ClearType,
    pub gauge_type: Option<GaugeType>,
    /// Final gauge value. Imported histories may not provide this value.
    pub gauge_value: Option<f32>,
    pub total_notes: u32,
    pub playtime_seconds: u32,
    pub score: ScoreState,
    pub count_unprocessed_notes: bool,
    pub random_seed: Option<i64>,
    pub seed_scheme: String,
    pub arrange: String,
    pub arrange_2p: String,
    pub gauge_option: String,
    pub rule_mode: String,
    pub assist_mask: u32,
    pub autoplay: bool,
    pub device_type: InputDeviceKind,
    pub replay_path: String,
    pub source_kind: ScoreSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreInsertMode {
    Full,
    HistoryOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreHistorySourceKey {
    pub source: String,
    pub provider: String,
    pub account_id: String,
    pub remote_score_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreHistorySourceRecord {
    pub key: ScoreHistorySourceKey,
    pub verification: String,
    pub server_received_at: i64,
    pub imported_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreSourceInsertOutcome {
    Inserted { history_id: i64 },
    Duplicate { history_id: i64 },
}

/// 外部score DBを再インポートしたときの、既存履歴との照合結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportedScoreReconciliation {
    Missing,
    Unchanged,
    Corrected,
}

/// source_kind 導入前に Local として保存された外部 import の整理対象。
///
/// 判定は譜面、プレイ日時、EX、判定内訳、BP、コンボ、seed に限定する。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyBeatorajaCleanupPlan {
    pub legacy_history_ids: Vec<i64>,
    pub retained_beatoraja_history_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegacyBeatorajaCleanupReport {
    pub removed_legacy_history: u32,
    pub retained_beatoraja_history: u32,
}

#[derive(Debug, Clone)]
pub struct ScoreRecordMetadata {
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub applied_double_option: DoubleOption,
    pub played_at: i64,
    pub playtime_seconds: u32,
    pub random_seed: Option<i64>,
    pub seed_scheme: String,
    pub arrange: String,
    pub arrange_2p: String,
    pub gauge_option: String,
    pub rule_mode: String,
    pub assist_mask: u32,
    pub device_type: InputDeviceKind,
    pub replay_path: String,
    pub source_kind: ScoreSourceKind,
}

impl ScoreRecord {
    pub fn from_play_result(result: &PlayResult, metadata: ScoreRecordMetadata) -> Self {
        let ScoreRecordMetadata {
            ln_policy,
            double_option,
            applied_double_option,
            played_at,
            playtime_seconds,
            random_seed,
            seed_scheme,
            arrange,
            arrange_2p,
            gauge_option,
            rule_mode,
            assist_mask,
            device_type,
            replay_path,
            source_kind,
        } = metadata;

        Self {
            chart_sha256: result.chart_sha256,
            ln_policy,
            double_option,
            applied_double_option,
            played_at,
            clear_type: result.clear_type,
            gauge_type: Some(result.gauge_type),
            gauge_value: Some(result.gauge_value),
            total_notes: result.total_notes,
            playtime_seconds,
            score: result.score.clone(),
            count_unprocessed_notes: result.clear_type == ClearType::Failed,
            random_seed,
            seed_scheme,
            arrange,
            arrange_2p,
            gauge_option,
            rule_mode,
            assist_mask,
            autoplay: result.autoplay,
            device_type,
            replay_path,
            source_kind,
        }
    }
}

impl ScoreRecordMetadata {
    pub fn new(
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        played_at: i64,
        random_seed: Option<i64>,
        arrange: impl Into<String>,
        gauge_option: impl Into<String>,
        rule_mode: impl Into<String>,
        assist_mask: u32,
        device_type: InputDeviceKind,
        replay_path: impl Into<String>,
    ) -> Self {
        Self {
            ln_policy,
            double_option,
            applied_double_option: DoubleOption::Off,
            played_at,
            playtime_seconds: 0,
            random_seed,
            seed_scheme: String::new(),
            arrange: arrange.into(),
            arrange_2p: "Normal".to_string(),
            gauge_option: gauge_option.into(),
            rule_mode: rule_mode.into(),
            assist_mask,
            device_type,
            replay_path: replay_path.into(),
            source_kind: ScoreSourceKind::Local,
        }
    }

    pub fn with_playtime_seconds(mut self, playtime_seconds: u32) -> Self {
        self.playtime_seconds = playtime_seconds;
        self
    }

    pub fn with_arrange_2p(mut self, arrange_2p: impl Into<String>) -> Self {
        self.arrange_2p = arrange_2p.into();
        self
    }

    pub fn with_seed_scheme(mut self, seed_scheme: impl Into<String>) -> Self {
        self.seed_scheme = seed_scheme.into();
        self
    }

    pub const fn with_applied_double_option(mut self, double_option: DoubleOption) -> Self {
        self.applied_double_option = double_option;
        self
    }

    pub fn with_source_kind(mut self, source_kind: ScoreSourceKind) -> Self {
        self.source_kind = source_kind;
        if source_kind == ScoreSourceKind::Beatoraja && self.seed_scheme.is_empty() {
            self.seed_scheme = "beatoraja_24bit_v1".to_string();
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BestScoreSummary {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub rule_mode: RuleMode,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: Option<f32>,
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    pub play_count: u32,
    pub clear_count: u32,
    pub device_type: InputDeviceKind,
    pub played_at: i64,
    pub replay_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScoreBestRank {
    ex_score: u32,
    clear_rank: u8,
    bp: u32,
    cb: u32,
    max_combo: u32,
}

#[derive(Debug, Clone, Copy)]
struct SourceScoreHistoryMatch {
    history_id: i64,
    device_type: InputDeviceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousBestSnapshot {
    pub clear_type: String,
    pub ex_score: u32,
    pub max_combo: u32,
    pub bp: u32,
    pub cb: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaySlotSummary {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub rule_mode: RuleMode,
    pub replay_slots: [bool; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaySlotRecord {
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub rule_mode: RuleMode,
    pub slot: u8,
    pub rule: ReplaySlotRule,
    pub replay_path: String,
    pub played_at: i64,
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub clear_rank: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreHistoryEntry {
    pub id: i64,
    pub chart_sha256: [u8; 32],
    pub ln_policy: LnScorePolicy,
    /// The DP option actually applied to this play. This is separate from the
    /// score bucket so a FLIP play is distinguishable from Off.
    pub applied_double_option: DoubleOption,
    pub played_at: i64,
    pub clear_type: String,
    pub gauge_type: String,
    pub gauge_value: Option<f32>,
    pub total_notes: u32,
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub autoplay: bool,
    pub device_type: InputDeviceKind,
    pub replay_path: String,
    pub source_kind: ScoreSourceKind,
    /// `score.db`'s `course_scores.id` if this chart play happened as part
    /// of a course attempt, otherwise `None`.
    pub course_score_id: Option<i64>,
    pub previous_best: Option<PreviousBestSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreHistoryDayEntry {
    pub local_day: String,
    pub local_minute: String,
    pub entry: ScoreHistoryEntry,
}

#[cfg(test)]
#[path = "score_db/tests.rs"]
mod tests;
