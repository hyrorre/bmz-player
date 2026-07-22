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
    pub gauge_value: f32,
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
            gauge_value: result.gauge_value,
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
    pub gauge_value: f32,
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
    pub gauge_value: f32,
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

impl ScoreDatabase {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        configure_connection(&conn)?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub(crate) fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn insert_score(&mut self, record: &ScoreRecord) -> Result<i64> {
        self.insert_score_with_mode(record, ScoreInsertMode::Full)
    }

    pub fn insert_score_with_mode(
        &mut self,
        record: &ScoreRecord,
        mode: ScoreInsertMode,
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let previous_best = previous_best_snapshot(
            &tx,
            ScoreKey::with_options(
                record.chart_sha256,
                record.ln_policy,
                record.double_option,
                record_rule_mode(record),
            ),
        )?;
        insert_score_history(&tx, record, previous_best.as_ref())?;
        let history_id = tx.last_insert_rowid();
        if mode == ScoreInsertMode::Full {
            upsert_score_best(&tx, record, history_id)?;
            update_player_stats(&tx, record)?;
        }
        tx.commit()?;
        Ok(history_id)
    }

    /// Returns whether an imported score with the same persisted score contents
    /// and provenance already exists. `played_at` is intentionally excluded:
    /// LR2 does not retain a per-score timestamp, and re-importing a source
    /// database must not create duplicates merely because the import time changed.
    pub fn has_same_score_from_source(&self, record: &ScoreRecord) -> Result<bool> {
        Ok(source_score_history_match(&self.conn, record)?
            .is_some_and(|existing| existing.device_type == record.device_type))
    }

    /// 同一出所の既存履歴が入力デバイスだけ異なる場合に、その自己申告値を補正する。
    /// 集計のdevice_typeは、その履歴がEXスコアの出所である場合だけ更新する。
    pub fn reconcile_imported_score_device_type(
        &mut self,
        record: &ScoreRecord,
    ) -> Result<ImportedScoreReconciliation> {
        if record.source_kind == ScoreSourceKind::Local {
            return Ok(ImportedScoreReconciliation::Missing);
        }
        let Some(existing) = source_score_history_match(&self.conn, record)? else {
            return Ok(ImportedScoreReconciliation::Missing);
        };
        if existing.device_type == record.device_type {
            return Ok(ImportedScoreReconciliation::Unchanged);
        }

        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE score_history
             SET device_type = ?1
             WHERE id = ?2 AND device_type = ?3",
            params![
                record.device_type.as_str(),
                existing.history_id,
                existing.device_type.as_str()
            ],
        )?;
        update_score_best_device_type_from_history(&tx, existing.history_id, record.device_type)?;
        tx.commit()?;
        Ok(ImportedScoreReconciliation::Corrected)
    }

    /// source_kind 導入前に Local として保存された beatoraja import 候補を調べる。
    ///
    /// Local の通常プレイと完全に区別する情報は失われているため、呼び出し側で
    /// dry-run の結果を確認してから [`Self::purge_legacy_beatoraja_imports`] を実行する。
    pub fn legacy_beatoraja_cleanup_plan(&self) -> Result<LegacyBeatorajaCleanupPlan> {
        Ok(LegacyBeatorajaCleanupPlan {
            legacy_history_ids: legacy_beatoraja_matching_history_ids(&self.conn, "legacy")?,
            retained_beatoraja_history_ids: legacy_beatoraja_matching_history_ids(
                &self.conn, "imported",
            )?,
        })
    }

    /// 同じ source_kind 内で、譜面・プレイ日時・スコア内訳・seed が完全一致する
    /// 通常プレイ履歴を返す。course stage は重複整理の対象にしない。
    pub fn same_source_duplicate_history_ids(&self, history_id: i64) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(
            "SELECT duplicate.id
             FROM score_history AS target
             JOIN score_history AS duplicate
               ON duplicate.id != target.id
              AND duplicate.source_kind = target.source_kind
              AND duplicate.course_score_id IS NULL
              AND duplicate.chart_sha256 = target.chart_sha256
              AND duplicate.played_at = target.played_at
              AND duplicate.ex_score = target.ex_score
              AND duplicate.bp = target.bp
              AND duplicate.cb = target.cb
              AND duplicate.max_combo = target.max_combo
              AND duplicate.fast_pgreat = target.fast_pgreat
              AND duplicate.slow_pgreat = target.slow_pgreat
              AND duplicate.fast_great = target.fast_great
              AND duplicate.slow_great = target.slow_great
              AND duplicate.fast_good = target.fast_good
              AND duplicate.slow_good = target.slow_good
              AND duplicate.fast_bad = target.fast_bad
              AND duplicate.slow_bad = target.slow_bad
              AND duplicate.fast_poor = target.fast_poor
              AND duplicate.slow_poor = target.slow_poor
              AND duplicate.fast_empty_poor = target.fast_empty_poor
              AND duplicate.slow_empty_poor = target.slow_empty_poor
              AND duplicate.random_seed IS target.random_seed
             WHERE target.id = ?1
               AND target.course_score_id IS NULL
             ORDER BY duplicate.id",
        )?;
        statement
            .query_map(params![history_id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// 指定した通常プレイ履歴を削除し、残存履歴から集計を再構築する。
    pub fn purge_score_history_ids_and_rebuild(&mut self, history_ids: &[i64]) -> Result<u32> {
        if history_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let removed_history = delete_score_history_ids(&tx, history_ids)?;
        rebuild_score_aggregates(&tx)?;
        tx.commit()?;
        Ok(removed_history)
    }

    /// 指定した旧 Local 候補を削除し、通常譜面の score_best と player_stats を
    /// 残存履歴から再集計する。コース stage 履歴は集計から除外したまま維持する。
    pub fn purge_legacy_beatoraja_imports(
        &mut self,
        plan: &LegacyBeatorajaCleanupPlan,
    ) -> Result<LegacyBeatorajaCleanupReport> {
        let legacy_history_ids = &plan.legacy_history_ids;
        if legacy_history_ids.is_empty() {
            return Ok(LegacyBeatorajaCleanupReport {
                retained_beatoraja_history: plan.retained_beatoraja_history_ids.len() as u32,
                ..LegacyBeatorajaCleanupReport::default()
            });
        }

        let removed_legacy_history =
            self.purge_score_history_ids_and_rebuild(legacy_history_ids)?;
        Ok(LegacyBeatorajaCleanupReport {
            removed_legacy_history,
            retained_beatoraja_history: plan.retained_beatoraja_history_ids.len() as u32,
        })
    }

    pub fn score_history_id_for_source(&self, key: &ScoreHistorySourceKey) -> Result<Option<i64>> {
        score_history_id_for_source(&self.conn, key)
    }

    pub fn attach_score_history_source(
        &mut self,
        score_history_id: i64,
        source: &ScoreHistorySourceRecord,
    ) -> Result<bool> {
        let inserted = insert_score_history_source(&self.conn, score_history_id, source, true)?;
        Ok(inserted > 0)
    }

    pub fn insert_score_with_source(
        &mut self,
        record: &ScoreRecord,
        source: &ScoreHistorySourceRecord,
    ) -> Result<ScoreSourceInsertOutcome> {
        if let Some(history_id) = self.score_history_id_for_source(&source.key)? {
            return Ok(ScoreSourceInsertOutcome::Duplicate { history_id });
        }

        let tx = self.conn.transaction()?;
        let previous_best = previous_best_snapshot(
            &tx,
            ScoreKey::with_options(
                record.chart_sha256,
                record.ln_policy,
                record.double_option,
                record_rule_mode(record),
            ),
        )?;
        insert_score_history(&tx, record, previous_best.as_ref())?;
        let history_id = tx.last_insert_rowid();
        upsert_score_best(&tx, record, history_id)?;
        update_player_stats(&tx, record)?;
        insert_score_history_source(&tx, history_id, source, false)?;
        tx.commit()?;
        Ok(ScoreSourceInsertOutcome::Inserted { history_id })
    }

    pub fn player_info(&self) -> Result<PlayerInfo> {
        self.conn
            .query_row(
                "SELECT player_uuid, display_name, created_at, updated_at
                 FROM player_info
                 WHERE id = 1",
                [],
                |row| {
                    Ok(PlayerInfo {
                        player_uuid: row.get(0)?,
                        display_name: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn set_player_display_name(&mut self, display_name: &str, updated_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE player_info
             SET display_name = ?1, updated_at = ?2
             WHERE id = 1",
            params![display_name, updated_at],
        )?;
        Ok(())
    }

    pub fn player_stats(&self) -> Result<PlayerStats> {
        self.conn
            .query_row(
                "SELECT
                    play_count,
                    clear_count,
                    playtime_seconds,
                    max_combo,
                    fast_pgreat,
                    slow_pgreat,
                    fast_great,
                    slow_great,
                    fast_good,
                    slow_good,
                    fast_bad,
                    slow_bad,
                    fast_poor,
                    slow_poor,
                    fast_empty_poor,
                    slow_empty_poor,
                    updated_at
                 FROM player_stats
                 WHERE id = 1",
                [],
                player_stats_from_row,
            )
            .map_err(Into::into)
    }

    /// Aggregate locally played score history inside `[start_at, end_at)`.
    pub fn daily_player_stats_between(
        &self,
        start_at: i64,
        end_at: i64,
    ) -> Result<DailyPlayerStats> {
        self.conn
            .query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(CASE
                        WHEN clear_type NOT IN ('NoPlay', 'Failed') THEN 1 ELSE 0
                    END), 0),
                    COALESCE(SUM(fast_pgreat + slow_pgreat), 0),
                    COALESCE(SUM(fast_great + slow_great), 0),
                    COALESCE(SUM(fast_good + slow_good), 0),
                    COALESCE(SUM(fast_bad + slow_bad), 0),
                    COALESCE(SUM(fast_poor + slow_poor), 0),
                    COALESCE(SUM(fast_empty_poor + slow_empty_poor), 0),
                    COALESCE(SUM(CASE
                        WHEN old_ex_score IS NULL OR ex_score > old_ex_score THEN 1 ELSE 0
                    END), 0),
                    COALESCE(SUM(CASE WHEN old_clear_type IS NULL OR
                        CASE clear_type
                            WHEN 'Failed' THEN 1 WHEN 'AssistEasy' THEN 2
                            WHEN 'LightAssistEasy' THEN 3 WHEN 'Easy' THEN 4
                            WHEN 'Normal' THEN 5 WHEN 'Hard' THEN 6
                            WHEN 'ExHard' THEN 7 WHEN 'FullCombo' THEN 8
                            WHEN 'Perfect' THEN 9 WHEN 'Max' THEN 10 ELSE 0 END
                        > CASE old_clear_type
                            WHEN 'Failed' THEN 1 WHEN 'AssistEasy' THEN 2
                            WHEN 'LightAssistEasy' THEN 3 WHEN 'Easy' THEN 4
                            WHEN 'Normal' THEN 5 WHEN 'Hard' THEN 6
                            WHEN 'ExHard' THEN 7 WHEN 'FullCombo' THEN 8
                            WHEN 'Perfect' THEN 9 WHEN 'Max' THEN 10 ELSE 0 END
                        THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE
                        WHEN old_bp IS NULL OR fast_bad + slow_bad + fast_poor + slow_poor < old_bp
                        THEN 1 ELSE 0 END), 0)
                 FROM score_history
                 WHERE source_kind = 'Local'
                   AND autoplay = 0
                   AND played_at >= ?1
                   AND played_at < ?2",
                params![start_at, end_at],
                daily_player_stats_from_row,
            )
            .map_err(Into::into)
    }

    /// Aggregate the current calendar day using the host's local timezone.
    pub fn current_local_day_player_stats(&self) -> Result<DailyPlayerStats> {
        self.current_local_day_player_stats_with_start_hour(0)
    }

    pub fn current_local_day_player_stats_with_start_hour(
        &self,
        day_start_hour: u8,
    ) -> Result<DailyPlayerStats> {
        let (start_at, end_at) = self.current_daily_statistics_range(day_start_hour)?;
        self.daily_player_stats_between(start_at, end_at)
    }

    pub fn current_daily_statistics_range(&self, day_start_hour: u8) -> Result<(i64, i64)> {
        let hour = day_start_hour.min(23);
        let shift_to_day = format!("-{hour} hours");
        let shift_from_day = format!("+{hour} hours");
        let (calendar_start, end_at): (i64, i64) = self.conn.query_row(
            "SELECT
                CAST(strftime('%s', date('now', 'localtime', ?1), ?2, 'utc') AS INTEGER),
                CAST(strftime('%s', date('now', 'localtime', ?1), '+1 day', ?2, 'utc') AS INTEGER)",
            params![shift_to_day, shift_from_day],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let reset_at: i64 = self.conn.query_row(
            "SELECT reset_at FROM daily_statistics_state WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        Ok((calendar_start.max(reset_at).min(end_at), end_at))
    }

    pub fn reset_daily_statistics(&self, reset_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE daily_statistics_state SET reset_at = ?1 WHERE id = 1",
            params![reset_at],
        )?;
        Ok(())
    }

    pub fn daily_recent_chart_sha256s_between(
        &self,
        start_at: i64,
        end_at: i64,
        limit: usize,
    ) -> Result<Vec<[u8; 32]>> {
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256
             FROM score_history
             WHERE source_kind = 'Local'
               AND autoplay = 0
               AND played_at >= ?1
               AND played_at < ?2
             ORDER BY played_at DESC, id DESC",
        )?;
        let mut rows = stmt.query(params![start_at, end_at])?;
        let mut hashes = Vec::with_capacity(limit);
        let mut previous_hex = None;
        while hashes.len() < limit {
            let Some(row) = rows.next()? else { break };
            let hex: String = row.get(0)?;
            if previous_hex.as_deref() == Some(hex.as_str()) {
                continue;
            }
            previous_hex = Some(hex.clone());
            hashes.push(hex_to_hash::<32>(&hex)?);
        }
        Ok(hashes)
    }

    pub fn best_ex_score(&self, key: ScoreKey) -> Result<Option<u32>> {
        self.conn
            .query_row(
                "SELECT ex_score FROM score_best
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
                   AND rule_mode = ?4",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                ],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn best_ghost(&self, key: ScoreKey, total_notes: u32) -> Result<Option<Vec<u8>>> {
        let Some(ghost) = self
            .conn
            .query_row(
                "SELECT ghost FROM score_best
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
                   AND rule_mode = ?4",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        else {
            return Ok(None);
        };
        if ghost.is_empty() {
            return Ok(None);
        }
        decode_beatoraja_ghost(&ghost, total_notes).map(Some)
    }

    pub fn best_scores_for_charts(&self, keys: &[ScoreKey]) -> Result<Vec<BestScoreSummary>> {
        let mut seen = HashSet::with_capacity(keys.len());
        let unique_keys = keys.iter().copied().filter(|key| seen.insert(*key)).collect::<Vec<_>>();
        let mut found = HashMap::with_capacity(unique_keys.len());

        for chunk in unique_keys.chunks(SCORE_KEY_LOOKUP_BATCH_SIZE) {
            let placeholders =
                std::iter::repeat_n("(?, ?, ?, ?)", chunk.len()).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT
                    chart_sha256,
                    ln_policy,
                    double_option,
                    rule_mode,
                    clear_type,
                    gauge_type,
                    gauge_value,
                    ex_score,
                    bp,
                    cb,
                    max_combo,
                    fast_pgreat,
                    slow_pgreat,
                    fast_great,
                    slow_great,
                    fast_good,
                    slow_good,
                    fast_bad,
                    slow_bad,
                    fast_poor,
                    slow_poor,
                    fast_empty_poor,
                    slow_empty_poor,
                    play_count,
                    clear_count,
                    device_type,
                    played_at,
                    replay_path
                FROM score_best
                WHERE (chart_sha256, ln_policy, double_option, rule_mode)
                    IN ({placeholders})"
            );
            let params = score_key_query_params(chunk);
            let mut stmt = self.conn.prepare(&sql)?;
            let rows =
                stmt.query_map(params_from_iter(params.iter()), best_score_summary_from_row)?;
            for row in rows {
                let summary = row?;
                let key = ScoreKey::with_options(
                    summary.chart_sha256,
                    summary.ln_policy,
                    summary.double_option,
                    summary.rule_mode,
                );
                found.insert(key, summary);
            }
        }

        Ok(keys.iter().filter_map(|key| found.get(key).cloned()).collect())
    }

    pub fn replay_slots_for_charts(&self, keys: &[ScoreKey]) -> Result<Vec<ReplaySlotSummary>> {
        let mut seen = HashSet::with_capacity(keys.len());
        let unique_keys = keys.iter().copied().filter(|key| seen.insert(*key)).collect::<Vec<_>>();
        let mut found: HashMap<ScoreKey, [bool; 4]> = HashMap::with_capacity(unique_keys.len());

        for chunk in unique_keys.chunks(SCORE_KEY_LOOKUP_BATCH_SIZE) {
            let placeholders =
                std::iter::repeat_n("(?, ?, ?, ?)", chunk.len()).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT chart_sha256, ln_policy, double_option, rule_mode, slot
                 FROM replay_slots
                 WHERE (chart_sha256, ln_policy, double_option, rule_mode)
                    IN ({placeholders})"
            );
            let params = score_key_query_params(chunk);
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
                let sha256_hex: String = row.get(0)?;
                let key = ScoreKey::with_options(
                    hex_to_hash::<32>(&sha256_hex)?,
                    ln_policy_from_row(row, 1)?,
                    double_option_from_row(row, 2)?,
                    rule_mode_from_row(row, 3)?,
                );
                Ok((key, row.get::<_, u8>(4)?))
            })?;
            for row in rows {
                let (key, slot) = row?;
                let replay_slots = found.entry(key).or_default();
                if (slot as usize) < 4 {
                    replay_slots[slot as usize] = true;
                }
            }
        }

        Ok(keys
            .iter()
            .filter_map(|key| {
                found.get(key).copied().map(|replay_slots| ReplaySlotSummary {
                    chart_sha256: key.chart_sha256,
                    ln_policy: key.ln_policy,
                    double_option: key.double_option,
                    rule_mode: key.rule_mode,
                    replay_slots,
                })
            })
            .collect())
    }

    pub fn replay_slot(&self, key: ScoreKey, slot: u8) -> Result<Option<ReplaySlotRecord>> {
        self.conn
            .query_row(
                "SELECT chart_sha256, ln_policy, double_option, rule_mode, slot, rule, replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank
                 FROM replay_slots
                 WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
                   AND rule_mode = ?4 AND slot = ?5",
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                    slot,
                ],
                replay_slot_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replay_slots_for_chart(&self, key: ScoreKey) -> Result<[Option<ReplaySlotRecord>; 4]> {
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256, ln_policy, double_option, rule_mode, slot, rule, replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank
             FROM replay_slots
             WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
               AND rule_mode = ?4",
        )?;
        let rows = stmt
            .query_map(
                params![
                    hash_to_hex(&key.chart_sha256),
                    key.ln_policy.as_str(),
                    key.double_option.as_str(),
                    key.rule_mode.as_str(),
                ],
                replay_slot_record_from_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out: [Option<ReplaySlotRecord>; 4] = [None, None, None, None];
        for record in rows {
            let slot = record.slot as usize;
            if slot < out.len() {
                out[slot] = Some(record);
            }
        }
        Ok(out)
    }

    pub fn upsert_replay_slot(&mut self, record: &ReplaySlotRecord) -> Result<()> {
        if record.slot > 3 {
            bail!("replay slot must be in 0..=3 (got {})", record.slot);
        }
        self.conn.execute(
            "INSERT INTO replay_slots (
                chart_sha256, ln_policy, double_option, rule_mode, slot, rule, replay_path, played_at,
                ex_score, bp, cb, max_combo, clear_rank
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(chart_sha256, ln_policy, double_option, rule_mode, slot) DO UPDATE SET
                rule = excluded.rule,
                replay_path = excluded.replay_path,
                played_at = excluded.played_at,
                ex_score = excluded.ex_score,
                bp = excluded.bp,
                cb = excluded.cb,
                max_combo = excluded.max_combo,
                clear_rank = excluded.clear_rank",
            params![
                hash_to_hex(&record.chart_sha256),
                record.ln_policy.as_str(),
                record.double_option.as_str(),
                record.rule_mode.as_str(),
                record.slot,
                record.rule.as_str(),
                record.replay_path,
                record.played_at,
                record.ex_score,
                record.bp,
                record.cb,
                record.max_combo,
                record.clear_rank,
            ],
        )?;
        Ok(())
    }

    pub fn insert_course_score(&mut self, record: &CourseScoreInsert) -> Result<i64> {
        super::course_score_db::insert_course_score(&mut self.conn, record)
    }

    pub fn best_course_score(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
    ) -> Result<Option<CourseBestScore>> {
        super::course_score_db::best_course_score(&self.conn, course_hash, rule_mode)
    }

    pub fn best_course_clear(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
    ) -> Result<Option<bmz_core::clear::ClearType>> {
        super::course_score_db::best_course_clear(&self.conn, course_hash, rule_mode)
    }

    pub fn list_course_score_charts(
        &self,
        course_score_id: i64,
    ) -> Result<Vec<CourseScoreChartRecord>> {
        super::course_score_db::list_course_score_charts(&self.conn, course_score_id)
    }

    pub fn list_course_replays(&self, course_score_id: i64) -> Result<Vec<CourseReplayRecord>> {
        super::course_score_db::list_course_replays(&self.conn, course_score_id)
    }

    pub fn latest_course_score_id(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
    ) -> Result<Option<i64>> {
        super::course_score_db::latest_course_score_id(&self.conn, course_hash, rule_mode)
    }

    pub fn list_recent_course_scores(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<CourseScoreEntry>> {
        super::course_score_db::list_recent_course_scores(
            &self.conn,
            course_hash,
            rule_mode,
            limit,
            offset,
        )
    }

    pub fn course_score_entry_by_id(
        &self,
        course_score_id: i64,
    ) -> Result<Option<CourseScoreEntry>> {
        super::course_score_db::course_score_entry_by_id(&self.conn, course_score_id)
    }

    pub fn upsert_course_replay_slot(&mut self, record: &CourseReplaySlotRecord) -> Result<()> {
        super::course_score_db::upsert_course_replay_slot(&mut self.conn, record)
    }

    pub fn course_replay_slot(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
        slot: u8,
    ) -> Result<Option<CourseReplaySlotRecord>> {
        super::course_score_db::course_replay_slot(&self.conn, course_hash, rule_mode, slot)
    }

    pub fn course_replay_slots_for_course(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
    ) -> Result<[Option<CourseReplaySlotRecord>; 4]> {
        super::course_score_db::course_replay_slots_for_course(&self.conn, course_hash, rule_mode)
    }

    pub fn course_replay_slot_presence(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
    ) -> Result<[bool; 4]> {
        super::course_score_db::course_replay_slot_presence(&self.conn, course_hash, rule_mode)
    }

    pub fn achieved_trophy_names_for_course(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
    ) -> Result<Vec<String>> {
        super::course_score_db::achieved_trophy_names_for_course(&self.conn, course_hash, rule_mode)
    }

    pub fn best_course_score_for_trophy(
        &self,
        course_hash: &str,
        rule_mode: RuleMode,
        trophy_name: &str,
    ) -> Result<Option<CourseBestScore>> {
        super::course_score_db::best_course_score_for_trophy(
            &self.conn,
            course_hash,
            rule_mode,
            trophy_name,
        )
    }

    /// Tag the given `score_history` rows with a course attempt id.
    ///
    /// `course_score_id` references this score DB's `course_scores.id`.
    pub fn tag_score_history_with_course(
        &mut self,
        score_history_ids: &[i64],
        course_score_id: i64,
    ) -> Result<usize> {
        if score_history_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut total = 0_usize;
        {
            let mut stmt =
                tx.prepare("UPDATE score_history SET course_score_id = ?1 WHERE id = ?2")?;
            for id in score_history_ids {
                total += stmt.execute(params![course_score_id, id])?;
            }
        }
        tx.commit()?;
        Ok(total)
    }

    /// IR replay upload 用に、score_history 行の replay_path を引く。
    /// 行が無い / 空文字なら None。
    pub fn replay_path_for_history(&self, score_history_id: i64) -> Result<Option<String>> {
        let path: Option<String> = self
            .conn
            .query_row(
                "SELECT replay_path FROM score_history WHERE id = ?1",
                params![score_history_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(path.filter(|path| !path.is_empty()))
    }

    pub fn recent_history(&self, limit: u32, offset: u32) -> Result<Vec<ScoreHistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                chart_sha256,
                played_at,
                clear_type,
                gauge_type,
                gauge_value,
                total_notes,
                ex_score,
                bp,
                cb,
                max_combo,
                autoplay,
                replay_path,
                course_score_id,
                ln_policy,
                old_clear_type,
                old_ex_score,
                old_max_combo,
                old_bp,
                old_cb,
                device_type,
                source_kind,
                applied_double_option
            FROM score_history
            ORDER BY played_at DESC, id DESC
            LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], score_history_entry_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn best_score_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BestScoreSummary> {
    let sha256_hex: String = row.get(0)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let ln_policy = ln_policy_from_row(row, 1)?;
    let double_option = double_option_from_row(row, 2)?;
    let rule_mode = rule_mode_from_row(row, 3)?;

    Ok(BestScoreSummary {
        chart_sha256,
        ln_policy,
        double_option,
        rule_mode,
        clear_type: row.get(4)?,
        gauge_type: row.get(5)?,
        gauge_value: row.get(6)?,
        ex_score: row.get(7)?,
        bp: row.get(8)?,
        cb: row.get(9)?,
        max_combo: row.get(10)?,
        judge_counts: DisplayJudgeCounts {
            pgreat: row.get::<_, u32>(11)? + row.get::<_, u32>(12)?,
            great: row.get::<_, u32>(13)? + row.get::<_, u32>(14)?,
            good: row.get::<_, u32>(15)? + row.get::<_, u32>(16)?,
            bad: row.get::<_, u32>(17)? + row.get::<_, u32>(18)?,
            poor: row.get::<_, u32>(19)? + row.get::<_, u32>(20)?,
            empty_poor: row.get::<_, u32>(21)? + row.get::<_, u32>(22)?,
        },
        fast_slow_counts: FastSlowJudgeCounts {
            fast_pgreat: row.get(11)?,
            slow_pgreat: row.get(12)?,
            fast_great: row.get(13)?,
            slow_great: row.get(14)?,
            fast_good: row.get(15)?,
            slow_good: row.get(16)?,
            fast_bad: row.get(17)?,
            slow_bad: row.get(18)?,
            fast_poor: row.get(19)?,
            slow_poor: row.get(20)?,
            fast_empty_poor: row.get(21)?,
            slow_empty_poor: row.get(22)?,
        },
        play_count: row.get(23)?,
        clear_count: row.get(24)?,
        device_type: device_type_from_row(row, 25)?,
        played_at: row.get(26)?,
        replay_path: row.get(27)?,
    })
}

fn replay_slot_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReplaySlotRecord> {
    let sha256_hex: String = row.get(0)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let ln_policy = ln_policy_from_row(row, 1)?;
    let double_option = double_option_from_row(row, 2)?;
    let rule_mode = rule_mode_from_row(row, 3)?;
    let rule_str: String = row.get(5)?;
    let rule = ReplaySlotRule::from_str_opt(&rule_str).unwrap_or(ReplaySlotRule::Always);

    Ok(ReplaySlotRecord {
        chart_sha256,
        ln_policy,
        double_option,
        rule_mode,
        slot: row.get(4)?,
        rule,
        replay_path: row.get(6)?,
        played_at: row.get(7)?,
        ex_score: row.get(8)?,
        bp: row.get(9)?,
        cb: row.get(10)?,
        max_combo: row.get(11)?,
        clear_rank: row.get(12)?,
    })
}

fn score_history_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScoreHistoryEntry> {
    let sha256_hex: String = row.get(1)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let old_clear_type: Option<String> = row.get(15)?;
    let previous_best = if let Some(clear_type) = old_clear_type {
        Some(PreviousBestSnapshot {
            clear_type,
            ex_score: row.get(16)?,
            max_combo: row.get(17)?,
            bp: row.get(18)?,
            cb: row.get(19)?,
        })
    } else {
        None
    };

    Ok(ScoreHistoryEntry {
        id: row.get(0)?,
        chart_sha256,
        ln_policy: ln_policy_from_row(row, 14)?,
        applied_double_option: applied_double_option_from_row(row, 22)?,
        played_at: row.get(2)?,
        clear_type: row.get(3)?,
        gauge_type: row.get(4)?,
        gauge_value: row.get(5)?,
        total_notes: row.get(6)?,
        ex_score: row.get(7)?,
        bp: row.get(8)?,
        cb: row.get(9)?,
        max_combo: row.get(10)?,
        autoplay: row.get(11)?,
        replay_path: row.get(12)?,
        course_score_id: row.get(13)?,
        device_type: device_type_from_row(row, 20)?,
        source_kind: score_source_kind_from_row(row, 21)?,
        previous_best,
    })
}

fn player_stats_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlayerStats> {
    Ok(PlayerStats {
        play_count: row.get(0)?,
        clear_count: row.get(1)?,
        playtime_seconds: row.get(2)?,
        max_combo: row.get(3)?,
        fast_pgreat: row.get(4)?,
        slow_pgreat: row.get(5)?,
        fast_great: row.get(6)?,
        slow_great: row.get(7)?,
        fast_good: row.get(8)?,
        slow_good: row.get(9)?,
        fast_bad: row.get(10)?,
        slow_bad: row.get(11)?,
        fast_poor: row.get(12)?,
        slow_poor: row.get(13)?,
        fast_empty_poor: row.get(14)?,
        slow_empty_poor: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn daily_player_stats_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DailyPlayerStats> {
    Ok(DailyPlayerStats {
        play_count: row.get(0)?,
        clear_count: row.get(1)?,
        pgreat: row.get(2)?,
        great: row.get(3)?,
        good: row.get(4)?,
        bad: row.get(5)?,
        poor: row.get(6)?,
        empty_poor: row.get(7)?,
        score_update_count: row.get(8)?,
        clear_update_count: row.get(9)?,
        miss_count_update_count: row.get(10)?,
    })
}

fn device_type_from_row(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<InputDeviceKind> {
    let value: String = row.get(index)?;
    match value.as_str() {
        "keyboard" => Ok(InputDeviceKind::Keyboard),
        "controller" => Ok(InputDeviceKind::Controller),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid input device type: {value}").into(),
        )),
    }
}

fn score_source_kind_from_row(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<ScoreSourceKind> {
    let value: String = row.get(index)?;
    ScoreSourceKind::from_str_opt(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid score source kind: {value}").into(),
        )
    })
}

fn source_score_history_match(
    conn: &Connection,
    record: &ScoreRecord,
) -> Result<Option<SourceScoreHistoryMatch>> {
    let judges = &record.score.judges;
    conn.query_row(
        "SELECT id, device_type
         FROM score_history
         WHERE source_kind = ?1
           AND chart_sha256 = ?2
           AND ln_policy = ?3
           AND double_option = ?4
           AND clear_type = ?5
           AND gauge_type = ?6
           AND gauge_value = ?7
           AND total_notes = ?8
           AND ex_score = ?9
           AND bp = ?10
           AND cb = ?11
           AND max_combo = ?12
           AND fast_pgreat = ?13
           AND slow_pgreat = ?14
           AND fast_great = ?15
           AND slow_great = ?16
           AND fast_good = ?17
           AND slow_good = ?18
           AND fast_bad = ?19
           AND slow_bad = ?20
           AND fast_poor = ?21
           AND slow_poor = ?22
           AND fast_empty_poor = ?23
           AND slow_empty_poor = ?24
           AND random_seed IS ?25
           AND arrange = ?26
           AND arrange_2p = ?27
           AND gauge_option = ?28
           AND rule_mode = ?29
           AND assist_mask = ?30
           AND autoplay = ?31
           AND applied_double_option = ?32
           AND seed_scheme = ?33
         ORDER BY id ASC
         LIMIT 1",
        params![
            record.source_kind.as_str(),
            hash_to_hex(&record.chart_sha256),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.total_notes,
            record.score.ex_score(),
            score_record_bp(record),
            score_record_cb(record),
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.random_seed,
            record.arrange.as_str(),
            record.arrange_2p.as_str(),
            record.gauge_option.as_str(),
            record.rule_mode.as_str(),
            record.assist_mask,
            record.autoplay,
            record.applied_double_option.to_persistent_str(),
            record.seed_scheme.as_str(),
        ],
        |row| {
            Ok(SourceScoreHistoryMatch {
                history_id: row.get(0)?,
                device_type: device_type_from_row(row, 1)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn update_score_best_device_type_from_history(
    conn: &Connection,
    history_id: i64,
    device_type: InputDeviceKind,
) -> Result<()> {
    conn.execute(
        "UPDATE score_best
         SET device_type = ?1
         WHERE best_score_history_id = ?2",
        params![device_type.as_str(), history_id],
    )?;
    Ok(())
}

fn legacy_beatoraja_matching_history_ids(
    conn: &Connection,
    selected_alias: &str,
) -> Result<Vec<i64>> {
    let (selected, counterpart) = match selected_alias {
        "legacy" => ("legacy", "imported"),
        "imported" => ("imported", "legacy"),
        _ => unreachable!("invalid legacy beatoraja cleanup alias"),
    };
    let selected_source_kind = if selected == "legacy" { "Local" } else { "Beatoraja" };
    let counterpart_source_kind = if counterpart == "legacy" { "Local" } else { "Beatoraja" };
    let sql = format!(
        "SELECT DISTINCT {selected}.id
         FROM score_history AS {selected}
         WHERE {selected}.source_kind = ?1
           AND {selected}.course_score_id IS NULL
           AND EXISTS (
               SELECT 1
               FROM score_history AS {counterpart}
               WHERE {counterpart}.source_kind = ?2
                 AND {counterpart}.course_score_id IS NULL
                 AND {counterpart}.chart_sha256 = {selected}.chart_sha256
                 AND {counterpart}.played_at = {selected}.played_at
                 AND {counterpart}.ex_score = {selected}.ex_score
                 AND {counterpart}.bp = {selected}.bp
                 AND {counterpart}.cb = {selected}.cb
                 AND {counterpart}.max_combo = {selected}.max_combo
                 AND {counterpart}.fast_pgreat = {selected}.fast_pgreat
                 AND {counterpart}.slow_pgreat = {selected}.slow_pgreat
                 AND {counterpart}.fast_great = {selected}.fast_great
                 AND {counterpart}.slow_great = {selected}.slow_great
                 AND {counterpart}.fast_good = {selected}.fast_good
                 AND {counterpart}.slow_good = {selected}.slow_good
                 AND {counterpart}.fast_bad = {selected}.fast_bad
                 AND {counterpart}.slow_bad = {selected}.slow_bad
                 AND {counterpart}.fast_poor = {selected}.fast_poor
                 AND {counterpart}.slow_poor = {selected}.slow_poor
                 AND {counterpart}.fast_empty_poor = {selected}.fast_empty_poor
                 AND {counterpart}.slow_empty_poor = {selected}.slow_empty_poor
                 AND {counterpart}.random_seed IS {selected}.random_seed
           )
         ORDER BY {selected}.id"
    );
    let mut statement = conn.prepare(&sql)?;
    statement
        .query_map(params![selected_source_kind, counterpart_source_kind], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn delete_score_history_ids(conn: &Connection, history_ids: &[i64]) -> Result<u32> {
    const DELETE_CHUNK_SIZE: usize = 500;
    let mut deleted = 0_u32;
    for ids in history_ids.chunks(DELETE_CHUNK_SIZE) {
        let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
        let sql = format!("DELETE FROM score_history WHERE id IN ({placeholders})");
        deleted = deleted.saturating_add(conn.execute(&sql, params_from_iter(ids.iter()))? as u32);
    }
    Ok(deleted)
}

fn rebuild_score_aggregates(conn: &Connection) -> Result<()> {
    // score_history は playtime_seconds を保持していないため、過去の通常プレイの
    // 総プレイ時間は復元できない。候補は外部 import に限定し、既存値を保全する。
    let preserved_playtime_seconds: u64 = conn
        .query_row("SELECT playtime_seconds FROM player_stats WHERE id = 1", [], |row| row.get(0))
        .optional()?
        .unwrap_or(0);
    const SCORE_KEY: &str = "h.chart_sha256 = score_best.chart_sha256
        AND h.ln_policy = score_best.ln_policy
        AND h.double_option = score_best.double_option
        AND CASE h.rule_mode
            WHEN 'Lr2Oraja' THEN 'Lr2Oraja'
            WHEN 'Dx' THEN 'Dx'
            ELSE 'Beatoraja'
        END = score_best.rule_mode
        AND h.course_score_id IS NULL";
    const CLEAR_RANK: &str = "CASE h.clear_type
        WHEN 'NoPlay' THEN 0
        WHEN 'Failed' THEN 1
        WHEN 'AssistEasy' THEN 2
        WHEN 'LightAssistEasy' THEN 3
        WHEN 'Easy' THEN 4
        WHEN 'Normal' THEN 5
        WHEN 'Hard' THEN 6
        WHEN 'ExHard' THEN 7
        WHEN 'FullCombo' THEN 8
        WHEN 'Perfect' THEN 9
        WHEN 'Max' THEN 10
        ELSE 0
    END";
    let score_source = format!(
        "SELECT h.id FROM score_history AS h
         WHERE {SCORE_KEY}
         ORDER BY h.ex_score DESC, h.bp ASC, h.cb ASC, h.max_combo DESC, h.id ASC
         LIMIT 1"
    );
    let clear_source = format!(
        "SELECT h.id FROM score_history AS h
         WHERE {SCORE_KEY}
         ORDER BY {CLEAR_RANK} DESC, h.id ASC
         LIMIT 1"
    );
    let score_value = |column: &str| {
        format!(
            "(SELECT h.{column} FROM score_history AS h
              WHERE h.id = ({score_source}))"
        )
    };
    let clear_value = |column: &str| {
        format!(
            "(SELECT h.{column} FROM score_history AS h
              WHERE h.id = ({clear_source}))"
        )
    };
    let aggregate = |expression: &str| {
        format!("(SELECT {expression} FROM score_history AS h WHERE {SCORE_KEY})")
    };

    conn.execute(
        &format!(
            "DELETE FROM score_best
             WHERE NOT EXISTS (SELECT 1 FROM score_history AS h WHERE {SCORE_KEY})"
        ),
        [],
    )?;
    conn.execute(
        &format!(
            "UPDATE score_best SET
                clear_type = {clear_type},
                gauge_type = {gauge_type},
                gauge_value = {gauge_value},
                ex_score = {ex_score},
                bp = {bp},
                cb = {cb},
                max_combo = {max_combo},
                fast_pgreat = {fast_pgreat},
                slow_pgreat = {slow_pgreat},
                fast_great = {fast_great},
                slow_great = {slow_great},
                fast_good = {fast_good},
                slow_good = {slow_good},
                fast_bad = {fast_bad},
                slow_bad = {slow_bad},
                fast_poor = {fast_poor},
                slow_poor = {slow_poor},
                fast_empty_poor = {fast_empty_poor},
                slow_empty_poor = {slow_empty_poor},
                played_at = {played_at},
                replay_path = {replay_path},
                device_type = {device_type},
                ghost = CASE
                    WHEN best_score_history_id = ({score_source}) THEN ghost
                    ELSE ''
                END,
                best_score_history_id = ({score_source}),
                play_count = {play_count},
                clear_count = {clear_count}",
            clear_type = clear_value("clear_type"),
            gauge_type = clear_value("gauge_type"),
            gauge_value = clear_value("gauge_value"),
            ex_score = score_value("ex_score"),
            bp = aggregate("MIN(h.bp)"),
            cb = aggregate("MIN(h.cb)"),
            max_combo = aggregate("MAX(h.max_combo)"),
            fast_pgreat = score_value("fast_pgreat"),
            slow_pgreat = score_value("slow_pgreat"),
            fast_great = score_value("fast_great"),
            slow_great = score_value("slow_great"),
            fast_good = score_value("fast_good"),
            slow_good = score_value("slow_good"),
            fast_bad = score_value("fast_bad"),
            slow_bad = score_value("slow_bad"),
            fast_poor = score_value("fast_poor"),
            slow_poor = score_value("slow_poor"),
            fast_empty_poor = score_value("fast_empty_poor"),
            slow_empty_poor = score_value("slow_empty_poor"),
            played_at = score_value("played_at"),
            replay_path = score_value("replay_path"),
            device_type = score_value("device_type"),
            play_count = aggregate("COUNT(*)"),
            clear_count = aggregate(
                "SUM(CASE WHEN h.clear_type NOT IN ('NoPlay', 'Failed') THEN 1 ELSE 0 END)"
            ),
        ),
        [],
    )?;
    conn.execute("DELETE FROM player_stats", [])?;
    conn.execute(
        "INSERT INTO player_stats (
            id, play_count, clear_count, playtime_seconds, max_combo,
            fast_pgreat, slow_pgreat, fast_great, slow_great,
            fast_good, slow_good, fast_bad, slow_bad,
            fast_poor, slow_poor, fast_empty_poor, slow_empty_poor, updated_at
         )
         SELECT
            1,
            COUNT(*),
            COALESCE(SUM(CASE WHEN clear_type NOT IN ('NoPlay', 'Failed') THEN 1 ELSE 0 END), 0),
            ?1,
            COALESCE(MAX(max_combo), 0),
            COALESCE(SUM(fast_pgreat), 0),
            COALESCE(SUM(slow_pgreat), 0),
            COALESCE(SUM(fast_great), 0),
            COALESCE(SUM(slow_great), 0),
            COALESCE(SUM(fast_good), 0),
            COALESCE(SUM(slow_good), 0),
            COALESCE(SUM(fast_bad), 0),
            COALESCE(SUM(slow_bad), 0),
            COALESCE(SUM(fast_poor), 0),
            COALESCE(SUM(slow_poor), 0),
            COALESCE(SUM(fast_empty_poor), 0),
            COALESCE(SUM(slow_empty_poor), 0),
            COALESCE(MAX(played_at), 0)
         FROM score_history
         WHERE course_score_id IS NULL",
        params![preserved_playtime_seconds],
    )?;
    Ok(())
}

fn ln_policy_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<LnScorePolicy> {
    let value: String = row.get(index)?;
    LnScorePolicy::from_str_opt(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid LN score policy: {value}").into(),
        )
    })
}

fn double_option_from_row(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<DoubleOptionScoreBucket> {
    let value: String = row.get(index)?;
    Ok(DoubleOptionScoreBucket::from_str_or_off(&value))
}

fn applied_double_option_from_row(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<DoubleOption> {
    let value: String = row.get(index)?;
    Ok(DoubleOption::from_persistent_str(&value))
}

fn rule_mode_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<RuleMode> {
    let value: String = row.get(index)?;
    RuleMode::from_str_opt(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid rule mode: {value}").into(),
        )
    })
}

fn previous_best_snapshot(
    conn: &Connection,
    key: ScoreKey,
) -> Result<Option<PreviousBestSnapshot>> {
    conn.query_row(
        "SELECT clear_type, ex_score, max_combo, bp, cb
         FROM score_best
         WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
           AND rule_mode = ?4",
        params![
            hash_to_hex(&key.chart_sha256),
            key.ln_policy.as_str(),
            key.double_option.as_str(),
            key.rule_mode.as_str(),
        ],
        |row| {
            Ok(PreviousBestSnapshot {
                clear_type: row.get(0)?,
                ex_score: row.get(1)?,
                max_combo: row.get(2)?,
                bp: row.get(3)?,
                cb: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn insert_score_history(
    conn: &Connection,
    record: &ScoreRecord,
    previous_best: Option<&PreviousBestSnapshot>,
) -> Result<()> {
    let judges = &record.score.judges;
    let bp = score_record_bp(record);
    let cb = score_record_cb(record);
    conn.execute(
        "INSERT INTO score_history (
            chart_sha256,
            ln_policy,
            double_option,
            played_at,
            clear_type,
            gauge_type,
            gauge_value,
            total_notes,
            ex_score,
            bp,
            cb,
            max_combo,
            fast_pgreat,
            slow_pgreat,
            fast_great,
            slow_great,
            fast_good,
            slow_good,
            fast_bad,
            slow_bad,
            fast_poor,
            slow_poor,
            fast_empty_poor,
            slow_empty_poor,
            random_seed,
            arrange,
            arrange_2p,
            gauge_option,
            rule_mode,
            assist_mask,
            autoplay,
            device_type,
            replay_path,
            source_kind,
            applied_double_option,
            old_clear_type,
            old_ex_score,
            old_max_combo,
            old_bp,
            old_cb,
            seed_scheme
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
            ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41
        )",
        params![
            hash_to_hex(&record.chart_sha256),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
            record.played_at,
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.total_notes,
            record.score.ex_score(),
            bp,
            cb,
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.random_seed,
            record.arrange.as_str(),
            record.arrange_2p.as_str(),
            record.gauge_option.as_str(),
            record.rule_mode.as_str(),
            record.assist_mask,
            record.autoplay,
            record.device_type.as_str(),
            record.replay_path.as_str(),
            record.source_kind.as_str(),
            record.applied_double_option.to_persistent_str(),
            previous_best.map(|best| best.clear_type.as_str()),
            previous_best.map(|best| best.ex_score),
            previous_best.map(|best| best.max_combo),
            previous_best.map(|best| best.bp),
            previous_best.map(|best| best.cb),
            record.seed_scheme.as_str(),
        ],
    )?;
    Ok(())
}

fn score_history_id_for_source(
    conn: &Connection,
    key: &ScoreHistorySourceKey,
) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT score_history_id
         FROM score_history_sources
         WHERE source = ?1
           AND provider = ?2
           AND account_id = ?3
           AND remote_score_id = ?4",
        params![
            key.source.as_str(),
            key.provider.as_str(),
            key.account_id.as_str(),
            key.remote_score_id.as_str(),
        ],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn insert_score_history_source(
    conn: &Connection,
    score_history_id: i64,
    source: &ScoreHistorySourceRecord,
    ignore_duplicate: bool,
) -> Result<usize> {
    let insert = if ignore_duplicate { "INSERT OR IGNORE" } else { "INSERT" };
    let sql = format!(
        "{insert} INTO score_history_sources (
            score_history_id, source, provider, account_id, remote_score_id,
            verification, server_received_at, imported_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    );
    conn.execute(
        &sql,
        params![
            score_history_id,
            source.key.source.as_str(),
            source.key.provider.as_str(),
            source.key.account_id.as_str(),
            source.key.remote_score_id.as_str(),
            source.verification.as_str(),
            source.server_received_at,
            source.imported_at,
        ],
    )
    .map_err(Into::into)
}

fn record_rule_mode(record: &ScoreRecord) -> RuleMode {
    RuleMode::from_str_opt(&record.rule_mode).unwrap_or(RuleMode::Beatoraja)
}

fn update_player_stats(conn: &Connection, record: &ScoreRecord) -> Result<()> {
    let judges = &record.score.judges;
    let clear_increment = u32::from(is_counted_clear(record.clear_type));
    conn.execute(
        "INSERT INTO player_stats (
            id,
            play_count,
            clear_count,
            playtime_seconds,
            max_combo,
            fast_pgreat,
            slow_pgreat,
            fast_great,
            slow_great,
            fast_good,
            slow_good,
            fast_bad,
            slow_bad,
            fast_poor,
            slow_poor,
            fast_empty_poor,
            slow_empty_poor,
            updated_at
        ) VALUES (
            1, 1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
        )
        ON CONFLICT(id) DO UPDATE SET
            play_count = play_count + 1,
            clear_count = clear_count + excluded.clear_count,
            playtime_seconds = playtime_seconds + excluded.playtime_seconds,
            max_combo = max(max_combo, excluded.max_combo),
            fast_pgreat = fast_pgreat + excluded.fast_pgreat,
            slow_pgreat = slow_pgreat + excluded.slow_pgreat,
            fast_great = fast_great + excluded.fast_great,
            slow_great = slow_great + excluded.slow_great,
            fast_good = fast_good + excluded.fast_good,
            slow_good = slow_good + excluded.slow_good,
            fast_bad = fast_bad + excluded.fast_bad,
            slow_bad = slow_bad + excluded.slow_bad,
            fast_poor = fast_poor + excluded.fast_poor,
            slow_poor = slow_poor + excluded.slow_poor,
            fast_empty_poor = fast_empty_poor + excluded.fast_empty_poor,
            slow_empty_poor = slow_empty_poor + excluded.slow_empty_poor,
            updated_at = max(updated_at, excluded.updated_at)",
        params![
            clear_increment,
            record.playtime_seconds,
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.played_at,
        ],
    )?;
    Ok(())
}

fn upsert_score_best(conn: &Connection, record: &ScoreRecord, history_id: i64) -> Result<()> {
    let judges = &record.score.judges;
    let ghost = encode_beatoraja_ghost(&record.score.ghost)?;
    let clear_increment = u32::from(is_counted_clear(record.clear_type));
    let bp = score_record_bp(record);
    let cb = score_record_cb(record);
    let rule_mode = record_rule_mode(record);
    let inserted = conn.execute(
        "INSERT INTO score_best (
            chart_sha256,
            ln_policy,
            double_option,
            rule_mode,
            clear_type,
            gauge_type,
            gauge_value,
            ex_score,
            bp,
            cb,
            max_combo,
            fast_pgreat,
            slow_pgreat,
            fast_great,
            slow_great,
            fast_good,
            slow_good,
            fast_bad,
            slow_bad,
            fast_poor,
            slow_poor,
            fast_empty_poor,
            slow_empty_poor,
            played_at,
            replay_path,
            ghost,
            device_type,
            best_score_history_id,
            play_count,
            clear_count
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30
        )
        ON CONFLICT(chart_sha256, ln_policy, double_option, rule_mode) DO NOTHING",
        params![
            hash_to_hex(&record.chart_sha256),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
            rule_mode.as_str(),
            record.clear_type.as_str(),
            gauge_type_str(record.gauge_type),
            record.gauge_value,
            record.score.ex_score(),
            bp,
            cb,
            record.score.max_combo,
            judges.fast_pgreat,
            judges.slow_pgreat,
            judges.fast_great,
            judges.slow_great,
            judges.fast_good,
            judges.slow_good,
            judges.fast_bad,
            judges.slow_bad,
            judges.fast_poor,
            judges.slow_poor,
            judges.fast_empty_poor,
            judges.slow_empty_poor,
            record.played_at,
            record.replay_path.as_str(),
            ghost,
            record.device_type.as_str(),
            history_id,
            1_u32,
            clear_increment,
        ],
    )?;
    if inserted > 0 {
        return Ok(());
    }

    let chart_sha256 = hash_to_hex(&record.chart_sha256);
    conn.execute(
        "UPDATE score_best
         SET play_count = play_count + 1,
             clear_count = clear_count + ?2
         WHERE chart_sha256 = ?1 AND ln_policy = ?3 AND double_option = ?4
           AND rule_mode = ?5",
        params![
            chart_sha256,
            clear_increment,
            record.ln_policy.as_str(),
            record.double_option.as_str(),
            rule_mode.as_str(),
        ],
    )?;

    let current = conn.query_row(
        "SELECT ex_score, clear_type, bp, cb, max_combo
         FROM score_best
         WHERE chart_sha256 = ?1 AND ln_policy = ?2 AND double_option = ?3
           AND rule_mode = ?4",
        params![
            hash_to_hex(&record.chart_sha256),
            record.ln_policy.as_str(),
            record.double_option.as_str(),
            rule_mode.as_str(),
        ],
        |row| {
            let clear_type: String = row.get(1)?;
            Ok(ScoreBestRank {
                ex_score: row.get(0)?,
                clear_rank: clear_rank_from_name(&clear_type),
                bp: row.get(2)?,
                cb: row.get(3)?,
                max_combo: row.get(4)?,
            })
        },
    )?;
    let should_update_score = score_best_should_update_score(record, current);
    let should_update_clear = score_best_should_update_clear(record, current);
    if !should_update_score {
        conn.execute(
            "UPDATE score_best SET
                bp = min(bp, ?2),
                cb = min(cb, ?3),
                max_combo = max(max_combo, ?4)
             WHERE chart_sha256 = ?1 AND ln_policy = ?5 AND double_option = ?6
               AND rule_mode = ?7",
            params![
                hash_to_hex(&record.chart_sha256),
                bp,
                cb,
                record.score.max_combo,
                record.ln_policy.as_str(),
                record.double_option.as_str(),
                rule_mode.as_str(),
            ],
        )?;
    } else {
        conn.execute(
            "UPDATE score_best SET
                ex_score = ?2,
                bp = ?3,
                cb = ?4,
                max_combo = ?5,
                fast_pgreat = ?6,
                slow_pgreat = ?7,
                fast_great = ?8,
                slow_great = ?9,
                fast_good = ?10,
                slow_good = ?11,
                fast_bad = ?12,
                slow_bad = ?13,
                fast_poor = ?14,
                slow_poor = ?15,
                fast_empty_poor = ?16,
                slow_empty_poor = ?17,
                played_at = ?18,
                replay_path = ?19,
                ghost = ?20,
                device_type = ?21,
                best_score_history_id = ?22
             WHERE chart_sha256 = ?1 AND ln_policy = ?23 AND double_option = ?24
               AND rule_mode = ?25",
            params![
                hash_to_hex(&record.chart_sha256),
                record.score.ex_score(),
                bp,
                cb,
                record.score.max_combo,
                judges.fast_pgreat,
                judges.slow_pgreat,
                judges.fast_great,
                judges.slow_great,
                judges.fast_good,
                judges.slow_good,
                judges.fast_bad,
                judges.slow_bad,
                judges.fast_poor,
                judges.slow_poor,
                judges.fast_empty_poor,
                judges.slow_empty_poor,
                record.played_at,
                record.replay_path.as_str(),
                ghost,
                record.device_type.as_str(),
                history_id,
                record.ln_policy.as_str(),
                record.double_option.as_str(),
                rule_mode.as_str(),
            ],
        )?;
    }

    if should_update_clear {
        conn.execute(
            "UPDATE score_best SET
                clear_type = ?2,
                gauge_type = ?3,
                gauge_value = ?4
             WHERE chart_sha256 = ?1 AND ln_policy = ?5 AND double_option = ?6
               AND rule_mode = ?7",
            params![
                hash_to_hex(&record.chart_sha256),
                record.clear_type.as_str(),
                gauge_type_str(record.gauge_type),
                record.gauge_value,
                record.ln_policy.as_str(),
                record.double_option.as_str(),
                rule_mode.as_str(),
            ],
        )?;
    }
    Ok(())
}

fn gauge_type_str(gauge_type: Option<GaugeType>) -> &'static str {
    gauge_type.map(GaugeType::as_str).unwrap_or("")
}

fn is_counted_clear(clear_type: ClearType) -> bool {
    !matches!(clear_type, ClearType::NoPlay | ClearType::Failed)
}

fn score_best_should_update_score(record: &ScoreRecord, current: ScoreBestRank) -> bool {
    let next = ScoreBestRank {
        ex_score: record.score.ex_score(),
        clear_rank: record.clear_type as u8,
        bp: score_record_bp(record),
        cb: score_record_cb(record),
        max_combo: record.score.max_combo,
    };
    (next.ex_score, std::cmp::Reverse(next.bp), std::cmp::Reverse(next.cb), next.max_combo)
        > (
            current.ex_score,
            std::cmp::Reverse(current.bp),
            std::cmp::Reverse(current.cb),
            current.max_combo,
        )
}

fn score_best_should_update_clear(record: &ScoreRecord, current: ScoreBestRank) -> bool {
    record.clear_type as u8 > current.clear_rank
}

fn score_record_bp(record: &ScoreRecord) -> u32 {
    if record.count_unprocessed_notes {
        record.score.bp_with_unprocessed_notes(record.total_notes)
    } else {
        record.score.bp()
    }
}

fn score_record_cb(record: &ScoreRecord) -> u32 {
    if record.count_unprocessed_notes {
        record.score.cb_with_unprocessed_notes(record.total_notes)
    } else {
        record.score.cb()
    }
}

fn clear_rank_from_name(value: &str) -> u8 {
    match value {
        "NoPlay" => ClearType::NoPlay as u8,
        "Failed" => ClearType::Failed as u8,
        "AssistEasy" => ClearType::AssistEasy as u8,
        "LightAssistEasy" => ClearType::LightAssistEasy as u8,
        "Easy" => ClearType::Easy as u8,
        "Normal" => ClearType::Normal as u8,
        "Hard" => ClearType::Hard as u8,
        "ExHard" => ClearType::ExHard as u8,
        "FullCombo" => ClearType::FullCombo as u8,
        "Perfect" => ClearType::Perfect as u8,
        "Max" => ClearType::Max as u8,
        _ => ClearType::NoPlay as u8,
    }
}

pub fn encode_beatoraja_ghost(ghost: &[u8]) -> Result<String> {
    if ghost.is_empty() {
        return Ok(String::new());
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(ghost)?;
    Ok(URL_SAFE.encode(encoder.finish()?))
}

pub fn decode_beatoraja_ghost(encoded: &str, total_notes: u32) -> Result<Vec<u8>> {
    let expected_len = total_notes as usize;
    if encoded.is_empty() {
        return Ok(vec![4; expected_len]);
    }

    let compressed = URL_SAFE.decode(encoded)?;
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut decoded = Vec::with_capacity(expected_len);
    decoder.read_to_end(&mut decoded)?;
    if decoded.len() < expected_len {
        decoded.resize(expected_len, 4);
    } else if decoded.len() > expected_len {
        decoded.truncate(expected_len);
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::ids::NoteId;
    use bmz_core::input::InputDeviceKind;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::judge::model::JudgementEvent;
    use bmz_gameplay::result::PlayResult;
    use bmz_gameplay::score::ScoreState;

    use super::*;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

    fn score_with_ex_score(ex_score: u32) -> ScoreState {
        let mut score = ScoreState::default();
        for index in 0..(ex_score / 2) {
            score.apply(&JudgementEvent {
                note_id: Some(NoteId(index)),
                lane: Lane::Key1,
                judge: Judge::PGreat,
                side: TimingSide::Slow,
                delta: TimeUs(0),
                time: TimeUs(index as i64),
                affects_score: true,
            });
        }
        score
    }

    fn record(ex_score: u32, clear_type: ClearType) -> ScoreRecord {
        ScoreRecord {
            chart_sha256: [7; 32],
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_000,
            clear_type,
            gauge_type: Some(GaugeType::Normal),
            gauge_value: 82.0,
            total_notes: ex_score / 2,
            playtime_seconds: 0,
            score: score_with_ex_score(ex_score),
            count_unprocessed_notes: clear_type == ClearType::Failed,
            random_seed: None,
            seed_scheme: String::new(),
            arrange: "Normal".to_string(),
            arrange_2p: "Normal".to_string(),
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            autoplay: false,
            device_type: InputDeviceKind::Keyboard,
            replay_path: String::new(),
            source_kind: ScoreSourceKind::Local,
        }
    }

    fn key(sha: [u8; 32]) -> ScoreKey {
        ScoreKey::new(sha, LnScorePolicy::ForceLn)
    }

    fn insert_test_course_score(db: &mut ScoreDatabase, course_hash: &str) -> i64 {
        db.conn_mut()
            .execute(
                "INSERT INTO course_scores (
                    course_hash, source, course_key, title, kind, constraints_json,
                    chart_sha256s_json, ex_score, max_ex_score, clear_type, gauge_type,
                    gauge_value, max_combo, bp, course_failed, course_clear, arrange,
                    trophies_json, played_at, rule_mode
                 ) VALUES (
                    ?1, '', '', '', '', '{}',
                    '[]', 0, 0, 'NoPlay', '',
                    0.0, 0, 0, 0, 0, 'Normal',
                    '[]', 0, 'Beatoraja'
                 )",
                params![course_hash],
            )
            .unwrap();
        db.conn().last_insert_rowid()
    }

    #[test]
    fn insert_score_persists_enum_strings_and_empty_values() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut record = record(20, ClearType::Normal);
        record.gauge_type = None;
        record.rule_mode = "Dx".to_string();
        record.arrange = "Random".to_string();
        record.arrange_2p = "Mirror".to_string();
        record.applied_double_option = DoubleOption::Flip;
        record.source_kind = ScoreSourceKind::Beatoraja;
        record.seed_scheme = "beatoraja_24bit_v1".to_string();
        record.device_type = InputDeviceKind::Controller;
        db.insert_score(&record).unwrap();

        let (
            clear_type,
            gauge_type,
            gauge_option,
            rule_mode,
            arrange,
            arrange_2p,
            device_type,
            replay_path,
            source_kind,
            applied_double_option,
        ): (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ) = db
            .conn()
            .query_row(
                "SELECT clear_type, gauge_type, gauge_option, rule_mode, arrange, arrange_2p, device_type, replay_path, source_kind, applied_double_option FROM score_history",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(clear_type, "Normal");
        assert_eq!(gauge_type, "");
        assert_eq!(gauge_option, "");
        assert_eq!(rule_mode, "Dx");
        assert_eq!(arrange, "Random");
        assert_eq!(arrange_2p, "Mirror");
        assert_eq!(device_type, "controller");
        assert_eq!(replay_path, "");
        assert_eq!(source_kind, "Beatoraja");
        assert_eq!(applied_double_option, "Flip");
        let seed_scheme: String = db
            .conn()
            .query_row("SELECT seed_scheme FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(seed_scheme, "beatoraja_24bit_v1");
        assert_eq!(db.recent_history(1, 0).unwrap()[0].source_kind, ScoreSourceKind::Beatoraja);
        assert_eq!(db.recent_history(1, 0).unwrap()[0].applied_double_option, DoubleOption::Flip);
    }

    #[test]
    fn same_score_from_source_ignores_time_but_keeps_score_context_distinct() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut imported = record(20, ClearType::Normal);
        imported.source_kind = ScoreSourceKind::Beatoraja;
        imported.random_seed = Some(1234);
        imported.arrange = "Random".to_string();
        imported.arrange_2p = "Mirror".to_string();
        imported.applied_double_option = DoubleOption::Flip;
        imported.rule_mode = "Beatoraja".to_string();
        db.insert_score(&imported).unwrap();

        let mut same = imported.clone();
        same.played_at += 60;
        assert!(db.has_same_score_from_source(&same).unwrap());

        let mut different_source = same.clone();
        different_source.source_kind = ScoreSourceKind::Lr2Oraja;
        assert!(!db.has_same_score_from_source(&different_source).unwrap());

        let mut different_seed = same.clone();
        different_seed.random_seed = Some(1235);
        assert!(!db.has_same_score_from_source(&different_seed).unwrap());

        let mut different_arrange_2p = same.clone();
        different_arrange_2p.arrange_2p = "Random".to_string();
        assert!(!db.has_same_score_from_source(&different_arrange_2p).unwrap());

        let mut different_applied_double_option = same.clone();
        different_applied_double_option.applied_double_option = DoubleOption::Off;
        assert!(!db.has_same_score_from_source(&different_applied_double_option).unwrap());

        let mut different_judges = same;
        different_judges.score.judges.fast_empty_poor = 1;
        assert!(!db.has_same_score_from_source(&different_judges).unwrap());
    }

    #[test]
    fn imported_score_reconciliation_updates_history_and_its_best_device() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut imported = record(20, ClearType::Normal);
        imported.source_kind = ScoreSourceKind::Beatoraja;
        let history_id = db.insert_score(&imported).unwrap();

        let mut corrected = imported.clone();
        corrected.played_at += 60;
        corrected.device_type = InputDeviceKind::Controller;
        assert_eq!(
            db.reconcile_imported_score_device_type(&corrected).unwrap(),
            ImportedScoreReconciliation::Corrected
        );
        assert!(db.has_same_score_from_source(&corrected).unwrap());
        assert_eq!(
            db.reconcile_imported_score_device_type(&corrected).unwrap(),
            ImportedScoreReconciliation::Unchanged
        );

        let history_device: String = db
            .conn()
            .query_row(
                "SELECT device_type FROM score_history WHERE id = ?1",
                params![history_id],
                |row| row.get(0),
            )
            .unwrap();
        let (best_history_id, best_device): (i64, String) = db
            .conn()
            .query_row("SELECT best_score_history_id, device_type FROM score_best", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(history_device, "controller");
        assert_eq!(best_history_id, history_id);
        assert_eq!(best_device, "controller");
    }

    #[test]
    fn imported_score_reconciliation_does_not_change_local_best_device() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut imported = record(20, ClearType::Normal);
        imported.source_kind = ScoreSourceKind::Beatoraja;
        db.insert_score(&imported).unwrap();

        let local = record(40, ClearType::Normal);
        let local_history_id = db.insert_score(&local).unwrap();

        let mut corrected = imported.clone();
        corrected.device_type = InputDeviceKind::Controller;
        assert_eq!(
            db.reconcile_imported_score_device_type(&corrected).unwrap(),
            ImportedScoreReconciliation::Corrected
        );

        let (best_history_id, best_device): (i64, String) = db
            .conn()
            .query_row("SELECT best_score_history_id, device_type FROM score_best", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(best_history_id, local_history_id);
        assert_eq!(best_device, "keyboard");
    }

    #[test]
    fn legacy_beatoraja_cleanup_removes_matching_local_history_and_rebuilds_aggregates() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut legacy = record(20, ClearType::Normal);
        legacy.playtime_seconds = 10;
        legacy.random_seed = Some(1234);
        let legacy_first_id = db.insert_score(&legacy).unwrap();
        let legacy_second_id = db.insert_score(&legacy).unwrap();

        let mut imported = legacy.clone();
        imported.playtime_seconds = 20;
        imported.arrange = "Random".to_string();
        imported.source_kind = ScoreSourceKind::Beatoraja;
        imported.device_type = InputDeviceKind::Controller;
        let imported_id = db.insert_score(&imported).unwrap();

        let mut ordinary = record(30, ClearType::Hard);
        ordinary.chart_sha256 = [8; 32];
        ordinary.playtime_seconds = 30;
        ordinary.played_at += 1;
        db.insert_score(&ordinary).unwrap();

        let plan = db.legacy_beatoraja_cleanup_plan().unwrap();
        assert_eq!(plan.legacy_history_ids, vec![legacy_first_id, legacy_second_id]);
        assert_eq!(plan.retained_beatoraja_history_ids, vec![imported_id]);

        let report = db.purge_legacy_beatoraja_imports(&plan).unwrap();
        assert_eq!(report.removed_legacy_history, 2);
        assert_eq!(report.retained_beatoraja_history, 1);
        assert!(db.legacy_beatoraja_cleanup_plan().unwrap().legacy_history_ids.is_empty());

        let history_ids: Vec<i64> = db
            .conn()
            .prepare("SELECT id FROM score_history ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(history_ids, vec![imported_id, imported_id + 1]);

        let imported_best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(imported_best.play_count, 1);
        assert_eq!(imported_best.device_type, InputDeviceKind::Controller);
        let best_history_id: i64 = db
            .conn()
            .query_row(
                "SELECT best_score_history_id FROM score_best WHERE chart_sha256 = ?1",
                params![hash_to_hex(&[7; 32])],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(best_history_id, imported_id);

        let stats = db.player_stats().unwrap();
        assert_eq!(stats.play_count, 2);
        assert_eq!(stats.clear_count, 2);
        assert_eq!(stats.playtime_seconds, 70);
    }

    #[test]
    fn same_source_duplicate_history_ids_match_the_cleanup_fingerprint() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut first = record(20, ClearType::Normal);
        first.random_seed = Some(1234);
        let first_id = db.insert_score(&first).unwrap();
        let duplicate_id = db.insert_score(&first).unwrap();

        let mut different = first;
        different.played_at += 1;
        let different_id = db.insert_score(&different).unwrap();

        assert_eq!(db.same_source_duplicate_history_ids(duplicate_id).unwrap(), vec![first_id]);
        assert!(db.same_source_duplicate_history_ids(different_id).unwrap().is_empty());
    }

    #[test]
    fn best_score_keeps_higher_ex_score() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        db.insert_score(&record(10, ClearType::Hard)).unwrap();
        db.insert_score(&record(30, ClearType::Easy)).unwrap();

        assert_eq!(db.best_ex_score(key([7; 32])).unwrap(), Some(30));
    }

    #[test]
    fn score_best_updates_clear_lamp_independently_from_ex_score() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(40, ClearType::Normal)).unwrap();
        let mut hard = record(20, ClearType::Hard);
        hard.gauge_type = Some(GaugeType::Hard);
        hard.gauge_value = 12.0;
        db.insert_score(&hard).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 40);
        assert_eq!(best.clear_type, "Hard");
        assert_eq!(best.gauge_type, "Hard");
        assert_eq!(best.gauge_value, 12.0);
    }

    #[test]
    fn score_best_does_not_downgrade_clear_lamp_on_higher_ex_score() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut hard = record(20, ClearType::Hard);
        hard.gauge_type = Some(GaugeType::Hard);
        hard.gauge_value = 12.0;
        db.insert_score(&hard).unwrap();
        let mut normal = record(40, ClearType::Normal);
        normal.gauge_type = Some(GaugeType::Normal);
        normal.gauge_value = 82.0;
        db.insert_score(&normal).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 40);
        assert_eq!(best.clear_type, "Hard");
        assert_eq!(best.gauge_type, "Hard");
        assert_eq!(best.gauge_value, 12.0);
    }

    #[test]
    fn score_best_updates_only_max_combo_when_lower_ex_score_improves_combo() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut initial = record(40, ClearType::Normal);
        initial.score.judges.fast_bad = 3;
        initial.score.judges.fast_empty_poor = 2;
        db.insert_score(&initial).unwrap();

        let mut combo = record(20, ClearType::Easy);
        combo.score.max_combo = 50;
        combo.score.judges.fast_bad = 4;
        combo.score.judges.fast_empty_poor = 2;
        db.insert_score(&combo).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 40);
        assert_eq!(best.clear_type, "Normal");
        assert_eq!(best.bp, 5);
        assert_eq!(best.cb, 3);
        assert_eq!(best.max_combo, 50);
        assert_eq!(best.judge_counts.pgreat, 20);
        assert_eq!(best.judge_counts.bad, 3);
        assert_eq!(best.judge_counts.empty_poor, 2);
    }

    #[test]
    fn score_best_updates_only_bp_when_lower_ex_score_improves_bp() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut initial = record(40, ClearType::Normal);
        initial.score.judges.fast_bad = 3;
        initial.score.judges.fast_empty_poor = 3;
        db.insert_score(&initial).unwrap();

        let mut lower_bp = record(20, ClearType::Easy);
        lower_bp.score.judges.fast_bad = 3;
        lower_bp.score.judges.fast_empty_poor = 1;
        db.insert_score(&lower_bp).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 40);
        assert_eq!(best.clear_type, "Normal");
        assert_eq!(best.bp, 4);
        assert_eq!(best.cb, 3);
        assert_eq!(best.max_combo, 20);
        assert_eq!(best.judge_counts.pgreat, 20);
        assert_eq!(best.judge_counts.bad, 3);
        assert_eq!(best.judge_counts.empty_poor, 3);
    }

    #[test]
    fn score_best_updates_only_cb_when_lower_ex_score_improves_cb() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut initial = record(40, ClearType::Normal);
        initial.score.judges.fast_bad = 4;
        initial.score.judges.fast_empty_poor = 1;
        db.insert_score(&initial).unwrap();

        let mut lower_cb = record(20, ClearType::Easy);
        lower_cb.score.judges.fast_bad = 2;
        lower_cb.score.judges.fast_empty_poor = 3;
        db.insert_score(&lower_cb).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 40);
        assert_eq!(best.clear_type, "Normal");
        assert_eq!(best.bp, 5);
        assert_eq!(best.cb, 2);
        assert_eq!(best.max_combo, 20);
        assert_eq!(best.judge_counts.pgreat, 20);
        assert_eq!(best.judge_counts.bad, 4);
        assert_eq!(best.judge_counts.empty_poor, 1);
    }

    #[test]
    fn score_best_is_separate_per_double_option() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        let mut battle = record(60, ClearType::Hard);
        battle.double_option = DoubleOptionScoreBucket::Battle;
        db.insert_score(&battle).unwrap();

        let off_key = key([7; 32]);
        let battle_key = ScoreKey::with_double_option(
            [7; 32],
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Battle,
        );

        assert_eq!(db.best_ex_score(off_key).unwrap(), Some(20));
        assert_eq!(db.best_ex_score(battle_key).unwrap(), Some(60));

        let summaries = db.best_scores_for_charts(&[off_key, battle_key]).unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].double_option, DoubleOptionScoreBucket::Off);
        assert_eq!(summaries[0].ex_score, 20);
        assert_eq!(summaries[1].double_option, DoubleOptionScoreBucket::Battle);
        assert_eq!(summaries[1].ex_score, 60);
    }

    #[test]
    fn score_best_is_separate_per_rule_mode() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut beatoraja = record(20, ClearType::Normal);
        beatoraja.rule_mode = RuleMode::Beatoraja.as_str().to_string();
        let mut dx = record(80, ClearType::Hard);
        dx.rule_mode = RuleMode::Dx.as_str().to_string();

        db.insert_score(&beatoraja).unwrap();
        db.insert_score(&dx).unwrap();

        let beatoraja_key = key([7; 32]).with_rule_mode(RuleMode::Beatoraja);
        let dx_key = key([7; 32]).with_rule_mode(RuleMode::Dx);

        assert_eq!(db.best_ex_score(beatoraja_key).unwrap(), Some(20));
        assert_eq!(db.best_ex_score(dx_key).unwrap(), Some(80));

        let summaries = db.best_scores_for_charts(&[beatoraja_key, dx_key]).unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].rule_mode, RuleMode::Beatoraja);
        assert_eq!(summaries[0].ex_score, 20);
        assert_eq!(summaries[1].rule_mode, RuleMode::Dx);
        assert_eq!(summaries[1].ex_score, 80);
    }

    #[test]
    fn best_score_tiebreaks_by_lower_bp_then_lower_cb() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut high_bp = record(20, ClearType::Normal);
        high_bp.score.judges.fast_bad = 3;
        high_bp.score.judges.fast_empty_poor = 2;
        db.insert_score(&high_bp).unwrap();

        let mut lower_bp = record(20, ClearType::Normal);
        lower_bp.score.judges.fast_bad = 2;
        lower_bp.score.judges.fast_empty_poor = 2;
        db.insert_score(&lower_bp).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.bp, 4);
        assert_eq!(best.cb, 2);

        let mut higher_cb = record(20, ClearType::Normal);
        higher_cb.score.judges.fast_bad = 4;
        higher_cb.score.judges.fast_empty_poor = 1;
        db.insert_score(&higher_cb).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.bp, 4);
        assert_eq!(best.cb, 2);

        let mut lower_cb = record(20, ClearType::Normal);
        lower_cb.score.judges.fast_great = 6;
        lower_cb.score.judges.slow_good = 5;
        lower_cb.score.judges.fast_bad = 1;
        lower_cb.score.judges.fast_empty_poor = 3;
        db.insert_score(&lower_cb).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.bp, 4);
        assert_eq!(best.cb, 1);
        assert_eq!(best.judge_counts.pgreat, 10);
        assert_eq!(best.judge_counts.great, 6);
        assert_eq!(best.judge_counts.good, 5);
        assert_eq!(best.judge_counts.bad, 1);
        assert_eq!(best.judge_counts.empty_poor, 3);
        assert_eq!(best.fast_slow_counts.fast_great, 6);
        assert_eq!(best.fast_slow_counts.slow_good, 5);
        assert_eq!(best.fast_slow_counts.fast_empty_poor, 3);
        assert_eq!(best.play_count, 4);
        assert_eq!(best.clear_count, 4);
    }

    #[test]
    fn score_best_counts_every_play_but_only_clear_results() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        let mut failed = record(10, ClearType::Failed);
        failed.played_at = 2;
        db.insert_score(&failed).unwrap();
        let mut clear = record(10, ClearType::Easy);
        clear.played_at = 3;
        db.insert_score(&clear).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 20);
        assert_eq!(best.clear_type, "Normal");
        assert_eq!(best.play_count, 3);
        assert_eq!(best.clear_count, 2);
    }

    #[test]
    fn score_best_keeps_independent_bp_cb_and_max_combo_records() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut best_score = record(20, ClearType::Normal);
        best_score.score.max_combo = 30;
        best_score.score.judges.fast_bad = 4;
        db.insert_score(&best_score).unwrap();

        let mut lower_score = record(10, ClearType::Failed);
        lower_score.played_at = 2;
        lower_score.score.max_combo = 80;
        lower_score.score.judges.fast_bad = 2;
        db.insert_score(&lower_score).unwrap();

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.ex_score, 20);
        assert_eq!(best.clear_type, "Normal");
        assert_eq!(best.max_combo, 80);
        assert_eq!(best.bp, 2);
        assert_eq!(best.cb, 2);
        assert_eq!(best.play_count, 2);
        assert_eq!(best.clear_count, 1);
    }

    #[test]
    fn failed_score_counts_unprocessed_notes_for_bp_and_cb_records() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut failed = record(0, ClearType::Failed);
        failed.total_notes = 100;
        db.insert_score(&failed).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        assert_eq!(history[0].bp, 100);
        assert_eq!(history[0].cb, 100);

        let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
        assert_eq!(best.clear_type, "Failed");
        assert_eq!(best.bp, 100);
        assert_eq!(best.cb, 100);
    }

    #[test]
    fn score_best_is_separate_per_ln_policy() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut ln = record(20, ClearType::Normal);
        ln.ln_policy = LnScorePolicy::ForceLn;
        let mut cn = record(40, ClearType::Hard);
        cn.ln_policy = LnScorePolicy::ForceCn;

        db.insert_score(&ln).unwrap();
        db.insert_score(&cn).unwrap();

        assert_eq!(db.best_ex_score(key([7; 32])).unwrap(), Some(20));
        assert_eq!(
            db.best_ex_score(ScoreKey::new([7; 32], LnScorePolicy::ForceCn)).unwrap(),
            Some(40)
        );
    }

    #[test]
    fn player_info_is_created_and_display_name_updates() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let info = db.player_info().unwrap();
        assert_eq!(info.player_uuid.len(), 32);
        assert!(info.player_uuid.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(info.display_name, "");

        db.set_player_display_name("hyrorre", 1_700_000_099).unwrap();

        let info = db.player_info().unwrap();
        assert_eq!(info.display_name, "hyrorre");
        assert_eq!(info.updated_at, 1_700_000_099);
    }

    #[test]
    fn player_stats_accumulates_profile_wide_scores() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut first = record(20, ClearType::Normal);
        first.played_at = 10;
        first.playtime_seconds = 120;
        first.score.judges.fast_great = 3;
        first.score.judges.slow_bad = 2;
        let mut failed = record(10, ClearType::Failed);
        failed.played_at = 20;
        failed.playtime_seconds = 30;
        failed.score.max_combo = 99;
        failed.score.judges.fast_empty_poor = 4;

        db.insert_score(&first).unwrap();
        db.insert_score(&failed).unwrap();

        let stats = db.player_stats().unwrap();
        assert_eq!(stats.play_count, 2);
        assert_eq!(stats.clear_count, 1);
        assert_eq!(stats.playtime_seconds, 150);
        assert_eq!(stats.max_combo, 99);
        assert_eq!(stats.fast_pgreat, 0);
        assert_eq!(stats.slow_pgreat, 15);
        assert_eq!(stats.fast_great, 3);
        assert_eq!(stats.slow_bad, 2);
        assert_eq!(stats.fast_empty_poor, 4);
        assert_eq!(stats.updated_at, 20);
    }

    #[test]
    fn daily_player_stats_aggregates_only_local_history_inside_range() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut played = record(0, ClearType::Normal);
        played.played_at = 110;
        played.score = ScoreState::default();
        played.score.judges.fast_pgreat = 2;
        played.score.judges.slow_great = 3;
        played.score.judges.fast_good = 4;
        played.score.judges.slow_bad = 5;
        played.score.judges.fast_poor = 6;
        played.score.judges.slow_empty_poor = 7;
        db.insert_score(&played).unwrap();

        let mut failed = record(0, ClearType::Failed);
        failed.chart_sha256 = [8; 32];
        failed.played_at = 120;
        failed.score = ScoreState::default();
        failed.score.judges.slow_pgreat = 11;
        db.insert_score(&failed).unwrap();

        let mut outside = record(0, ClearType::Normal);
        outside.chart_sha256 = [9; 32];
        outside.played_at = 99;
        outside.score = ScoreState::default();
        outside.score.judges.fast_pgreat = 100;
        db.insert_score(&outside).unwrap();

        let mut imported = record(0, ClearType::Normal);
        imported.chart_sha256 = [10; 32];
        imported.played_at = 130;
        imported.source_kind = ScoreSourceKind::Beatoraja;
        imported.score = ScoreState::default();
        imported.score.judges.fast_pgreat = 200;
        db.insert_score(&imported).unwrap();

        let stats = db.daily_player_stats_between(100, 200).unwrap();
        assert_eq!(
            stats,
            DailyPlayerStats {
                play_count: 2,
                clear_count: 1,
                pgreat: 13,
                great: 3,
                good: 4,
                bad: 5,
                poor: 6,
                empty_poor: 7,
                score_update_count: 2,
                clear_update_count: 2,
                miss_count_update_count: 2,
            }
        );
        assert_eq!(
            db.daily_recent_chart_sha256s_between(100, 200, 10).unwrap(),
            vec![[8; 32], [7; 32]]
        );
        db.reset_daily_statistics(i64::MAX).unwrap();
        let (reset_start, reset_end) = db.current_daily_statistics_range(0).unwrap();
        assert_eq!(reset_start, reset_end);
        assert_eq!(db.current_local_day_player_stats().unwrap(), DailyPlayerStats::default());
    }

    #[test]
    fn player_stats_migration_backfills_existing_history() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, &SCORE_MIGRATIONS[..7]).unwrap();
        conn.execute(
            "INSERT INTO score_history (
                chart_sha256, ln_policy, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, rule_mode, assist_mask, autoplay,
                replay_path, ghost
            ) VALUES (
                ?1, 'ForceLn', 10, 'Normal', 'Normal', 80.0,
                10, 20, 1, 1, 8,
                1, 2, 3, 4,
                5, 6, 7, 8,
                9, 10, 11, 12,
                NULL, '', 'Beatoraja', 0, 0,
                '', ''
            )",
            params![hash_to_hex(&[1; 32])],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO score_history (
                chart_sha256, ln_policy, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, rule_mode, assist_mask, autoplay,
                replay_path, ghost
            ) VALUES (
                ?1, 'ForceLn', 20, 'Failed', 'Normal', 20.0,
                10, 10, 5, 5, 12,
                2, 3, 4, 5,
                6, 7, 8, 9,
                10, 11, 12, 13,
                NULL, '', 'Beatoraja', 0, 0,
                '', ''
            )",
            params![hash_to_hex(&[2; 32])],
        )
        .unwrap();

        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let db = ScoreDatabase { conn };

        let stats = db.player_stats().unwrap();
        assert_eq!(stats.play_count, 2);
        assert_eq!(stats.clear_count, 1);
        assert_eq!(stats.playtime_seconds, 0);
        assert_eq!(stats.max_combo, 12);
        assert_eq!(stats.fast_pgreat, 3);
        assert_eq!(stats.slow_empty_poor, 25);
        assert_eq!(stats.updated_at, 20);
    }

    #[test]
    fn score_history_migration_drops_history_ghost_and_sanitizes_course_links() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, &SCORE_MIGRATIONS[..18]).unwrap();
        conn.execute(
            "INSERT INTO score_history (
                chart_sha256, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, assist_mask, autoplay,
                replay_path, ghost, course_score_id
            ) VALUES (
                ?1, 10, 'Normal', 'Normal', 80.0,
                10, 20, 1, 1, 8,
                1, 2, 3, 4,
                5, 6, 7, 8,
                9, 10, 11, 12,
                NULL, '', 0, 0,
                '', 'legacy-ghost', 9999
            )",
            params![hash_to_hex(&[1; 32])],
        )
        .unwrap();

        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(score_history)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(!columns.iter().any(|column| column == "ghost"));

        let course_score_id: Option<i64> = conn
            .query_row("SELECT course_score_id FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(course_score_id, None);

        let (source_kind, arrange_2p, applied_double_option): (String, String, String) = conn
            .query_row(
                "SELECT source_kind, arrange_2p, applied_double_option FROM score_history",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(source_kind, ScoreSourceKind::Local.as_str());
        assert_eq!(arrange_2p, "Normal");
        assert_eq!(applied_double_option, DoubleOption::Off.to_persistent_str());
    }

    #[test]
    fn score_history_records_previous_best_snapshot() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut first = record(20, ClearType::Normal);
        first.played_at = 10;
        first.score.max_combo = 15;
        first.score.judges.fast_bad = 2;
        db.insert_score(&first).unwrap();

        let mut second = record(30, ClearType::Hard);
        second.played_at = 20;
        db.insert_score(&second).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        assert_eq!(history[1].previous_best, None);
        assert_eq!(
            history[0].previous_best,
            Some(PreviousBestSnapshot {
                clear_type: "Normal".to_string(),
                ex_score: 20,
                max_combo: 15,
                bp: 2,
                cb: 2,
            })
        );
    }

    #[test]
    fn score_history_previous_best_is_separate_per_ln_policy() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut ln = record(20, ClearType::Normal);
        ln.ln_policy = LnScorePolicy::ForceLn;
        ln.played_at = 10;
        let mut cn_first = record(40, ClearType::Hard);
        cn_first.ln_policy = LnScorePolicy::ForceCn;
        cn_first.played_at = 20;
        let mut cn_second = record(10, ClearType::Easy);
        cn_second.ln_policy = LnScorePolicy::ForceCn;
        cn_second.played_at = 30;

        db.insert_score(&ln).unwrap();
        db.insert_score(&cn_first).unwrap();
        db.insert_score(&cn_second).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        assert_eq!(
            history[0].previous_best.as_ref().map(|best| (best.clear_type.as_str(), best.ex_score)),
            Some(("Hard", 40))
        );
        assert_eq!(history[1].previous_best, None);
        assert_eq!(history[2].previous_best, None);
    }

    #[test]
    fn beatoraja_ghost_round_trips_as_gzip_urlsafe_base64() {
        let ghost = vec![0, 1, 2, 3, 4];

        let encoded = encode_beatoraja_ghost(&ghost).unwrap();
        let decoded = decode_beatoraja_ghost(&encoded, ghost.len() as u32).unwrap();

        assert_eq!(decoded, ghost);
    }

    #[test]
    fn insert_score_persists_best_ghost_for_current_best() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        db.insert_score(&record(20, ClearType::Normal)).unwrap();
        db.insert_score(&record(10, ClearType::Hard)).unwrap();

        assert_eq!(db.best_ghost(key([7; 32]), 10).unwrap(), Some(vec![0; 10]));
    }

    #[test]
    fn class_gauge_types_round_trip_via_score_history_and_best() {
        // 段位ゲージで終わったプレイが score_history / score_best 経由で
        // `"Class" / "ExClass" / "ExHardClass"` の文字列として正しく永続化・
        // 復元されることを担保する。
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let cases = [
            ([10u8; 32], GaugeType::Class, "Class"),
            ([11u8; 32], GaugeType::ExClass, "ExClass"),
            ([12u8; 32], GaugeType::ExHardClass, "ExHardClass"),
        ];

        // ex_score は (sha[0], 段位ごと) で順に上げ、score_best が上書きされて
        // 残ることを保証する。
        for (i, (sha, gauge, _)) in cases.iter().enumerate() {
            let mut rec = record(20 + i as u32 * 10, ClearType::Hard);
            rec.chart_sha256 = *sha;
            rec.gauge_type = Some(*gauge);
            rec.gauge_value = 42.0 + i as f32;
            db.insert_score(&rec).unwrap();
        }

        // score_history: GaugeType::as_str() の文字列で素直に入る。
        let history = db.recent_history(10, 0).unwrap();
        let mut history_map: std::collections::HashMap<[u8; 32], String> =
            history.into_iter().map(|entry| (entry.chart_sha256, entry.gauge_type)).collect();
        for (sha, _, expected) in &cases {
            assert_eq!(history_map.remove(sha).as_deref(), Some(*expected), "history {sha:?}");
        }

        // score_best: 同じく文字列でラウンドトリップ、gauge_value も保持される。
        let keys: Vec<_> = cases.iter().map(|(sha, _, _)| key(*sha)).collect();
        let best = db.best_scores_for_charts(&keys).unwrap();
        assert_eq!(best.len(), 3);
        let mut by_sha: std::collections::HashMap<_, _> =
            best.into_iter().map(|s| (s.chart_sha256, s)).collect();
        for (i, (sha, _, expected_label)) in cases.iter().enumerate() {
            let summary = by_sha.remove(sha).expect("best entry exists");
            assert_eq!(summary.gauge_type, *expected_label);
            assert_eq!(summary.gauge_value, 42.0 + i as f32);
        }
    }

    #[test]
    fn gauge_type_str_matches_enum_display_for_class_gauges() {
        assert_eq!(gauge_type_str(Some(GaugeType::Class)), "Class");
        assert_eq!(gauge_type_str(Some(GaugeType::ExClass)), "ExClass");
        assert_eq!(gauge_type_str(Some(GaugeType::ExHardClass)), "ExHardClass");
        // sanity: 非段位ゲージも従来通り。
        assert_eq!(gauge_type_str(Some(GaugeType::Normal)), "Normal");
        assert_eq!(gauge_type_str(None), "");
    }

    #[test]
    fn best_scores_for_charts_returns_existing_scores() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        let mut first = record(20, ClearType::Normal);
        first.chart_sha256 = [1; 32];
        first.replay_path = "replay/one.bzr".to_string();
        let mut second = record(10, ClearType::Easy);
        second.chart_sha256 = [2; 32];
        second.gauge_type = None;

        db.insert_score(&first).unwrap();
        db.insert_score(&second).unwrap();

        let scores =
            db.best_scores_for_charts(&[key([2; 32]), key([3; 32]), key([1; 32])]).unwrap();

        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].chart_sha256, [2; 32]);
        assert_eq!(scores[0].gauge_type, "");
        assert_eq!(scores[1].chart_sha256, [1; 32]);
        assert_eq!(scores[1].replay_path, "replay/one.bzr");
    }

    fn sample_slot(slot: u8, ex_score: u32) -> ReplaySlotRecord {
        ReplaySlotRecord {
            chart_sha256: [1; 32],
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            rule_mode: RuleMode::Beatoraja,
            slot,
            rule: ReplaySlotRule::Always,
            replay_path: format!("replay/{slot}.toml"),
            played_at: 1_700_000_000 + slot as i64,
            ex_score,
            bp: 0,
            cb: 0,
            max_combo: ex_score,
            clear_rank: ClearType::Normal as u8,
        }
    }

    #[test]
    fn replay_slots_for_charts_reports_slot_presence_from_new_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
        db.upsert_replay_slot(&sample_slot(2, 30)).unwrap();

        let slots = db.replay_slots_for_charts(&[key([2; 32]), key([1; 32])]).unwrap();

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].chart_sha256, [1; 32]);
        assert_eq!(slots[0].replay_slots, [true, false, true, false]);
    }

    #[test]
    fn select_score_lookups_batch_more_keys_than_one_sqlite_variable_chunk() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };

        let mut first = record(20, ClearType::Normal);
        first.chart_sha256 = [1; 32];
        let mut second = record(10, ClearType::Easy);
        second.chart_sha256 = [2; 32];
        db.insert_score(&first).unwrap();
        db.insert_score(&second).unwrap();
        db.upsert_replay_slot(&sample_slot(0, 20)).unwrap();
        db.upsert_replay_slot(&sample_slot(2, 20)).unwrap();
        let mut second_slot = sample_slot(1, 10);
        second_slot.chart_sha256 = [2; 32];
        db.upsert_replay_slot(&second_slot).unwrap();

        let mut keys = (0..SCORE_KEY_LOOKUP_BATCH_SIZE * 2 + 1)
            .map(|index| {
                let mut sha = [0; 32];
                sha[..8].copy_from_slice(&(index as u64 + 100).to_le_bytes());
                key(sha)
            })
            .collect::<Vec<_>>();
        keys.extend([key([2; 32]), key([1; 32]), key([1; 32])]);

        let scores = db.best_scores_for_charts(&keys).unwrap();
        let slots = db.replay_slots_for_charts(&keys).unwrap();

        assert_eq!(
            scores.iter().map(|score| score.chart_sha256).collect::<Vec<_>>(),
            [[2; 32], [1; 32], [1; 32]]
        );
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].chart_sha256, [2; 32]);
        assert_eq!(slots[0].replay_slots, [false, true, false, false]);
        assert_eq!(slots[1].chart_sha256, [1; 32]);
        assert_eq!(slots[1].replay_slots, [true, false, true, false]);
        assert_eq!(slots[2], slots[1]);
    }

    #[test]
    fn upsert_replay_slot_overwrites_same_slot() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
        let mut updated = sample_slot(0, 99);
        updated.replay_path = "replay/updated.toml".to_string();
        db.upsert_replay_slot(&updated).unwrap();

        let record = db.replay_slot(key([1; 32]), 0).unwrap().unwrap();
        assert_eq!(record.ex_score, 99);
        assert_eq!(record.replay_path, "replay/updated.toml");
    }

    #[test]
    fn replay_slots_for_chart_returns_all_four_slots() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
        db.upsert_replay_slot(&sample_slot(3, 30)).unwrap();

        let slots = db.replay_slots_for_chart(key([1; 32])).unwrap();

        assert!(slots[0].is_some());
        assert!(slots[1].is_none());
        assert!(slots[2].is_none());
        assert_eq!(slots[3].as_ref().unwrap().ex_score, 30);
    }

    #[test]
    fn replay_slots_are_separate_per_ln_policy() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        let mut ln = sample_slot(0, 10);
        ln.ln_policy = LnScorePolicy::ForceLn;
        let mut cn = sample_slot(0, 99);
        cn.ln_policy = LnScorePolicy::ForceCn;

        db.upsert_replay_slot(&ln).unwrap();
        db.upsert_replay_slot(&cn).unwrap();

        let ln_slot = db.replay_slot(key([1; 32]), 0).unwrap().unwrap();
        let cn_slot =
            db.replay_slot(ScoreKey::new([1; 32], LnScorePolicy::ForceCn), 0).unwrap().unwrap();
        assert_eq!(ln_slot.ex_score, 10);
        assert_eq!(cn_slot.ex_score, 99);
    }

    #[test]
    fn replay_slots_are_separate_per_rule_mode() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase { conn };
        let mut beatoraja = sample_slot(0, 10);
        beatoraja.rule_mode = RuleMode::Beatoraja;
        let mut dx = sample_slot(0, 99);
        dx.rule_mode = RuleMode::Dx;

        db.upsert_replay_slot(&beatoraja).unwrap();
        db.upsert_replay_slot(&dx).unwrap();

        let beatoraja_slot =
            db.replay_slot(key([1; 32]).with_rule_mode(RuleMode::Beatoraja), 0).unwrap().unwrap();
        let dx_slot =
            db.replay_slot(key([1; 32]).with_rule_mode(RuleMode::Dx), 0).unwrap().unwrap();
        assert_eq!(beatoraja_slot.ex_score, 10);
        assert_eq!(dx_slot.ex_score, 99);
    }

    #[test]
    fn score_record_can_be_built_from_play_result() {
        let result = PlayResult {
            chart_sha256: [9; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Hard,
            gauge_value: 76.5,
            total_notes: 1,
            score: score_with_ex_score(2),
            autoplay: true,
        };

        let record = ScoreRecord::from_play_result(
            &result,
            ScoreRecordMetadata::new(
                LnScorePolicy::ForceCn,
                DoubleOptionScoreBucket::Battle,
                1_700_000_040,
                Some(123),
                "Normal",
                "Hard",
                "Lr2Oraja",
                0,
                InputDeviceKind::Controller,
                "",
            )
            .with_arrange_2p("Mirror")
            .with_source_kind(ScoreSourceKind::Lr2Oraja),
        );

        assert_eq!(record.chart_sha256, [9; 32]);
        assert_eq!(record.ln_policy, LnScorePolicy::ForceCn);
        assert_eq!(record.double_option, DoubleOptionScoreBucket::Battle);
        assert_eq!(record.played_at, 1_700_000_040);
        assert_eq!(record.clear_type, ClearType::Normal);
        assert_eq!(record.gauge_type, Some(GaugeType::Hard));
        assert_eq!(record.gauge_value, 76.5);
        assert_eq!(record.device_type, InputDeviceKind::Controller);
        assert_eq!(record.arrange_2p, "Mirror");
        assert_eq!(record.source_kind, ScoreSourceKind::Lr2Oraja);
        assert_eq!(record.score.ex_score(), 2);
        assert!(record.autoplay);
        assert_eq!(record.gauge_option, "Hard");
        assert_eq!(record.rule_mode, "Lr2Oraja");
        assert_eq!(record.replay_path, "");
    }

    #[test]
    fn tag_score_history_with_course_updates_only_given_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);

        let mut r1 = record(20, ClearType::Normal);
        r1.chart_sha256 = [1; 32];
        let mut r2 = record(30, ClearType::Easy);
        r2.chart_sha256 = [2; 32];
        let mut r3 = record(10, ClearType::Failed);
        r3.chart_sha256 = [3; 32];
        let id1 = db.insert_score(&r1).unwrap();
        let id2 = db.insert_score(&r2).unwrap();
        let id3 = db.insert_score(&r3).unwrap();

        let course_score_id = insert_test_course_score(&mut db, "course-a");

        // Tag the first two with a real course score, leave r3 untouched.
        let updated = db.tag_score_history_with_course(&[id1, id2], course_score_id).unwrap();
        assert_eq!(updated, 2);

        let rows: Vec<(i64, Option<i64>)> = db
            .conn()
            .prepare("SELECT id, course_score_id FROM score_history ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![(id1, Some(course_score_id)), (id2, Some(course_score_id)), (id3, None)]
        );
    }

    #[test]
    fn tag_score_history_with_course_no_op_on_empty_list() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        assert_eq!(db.tag_score_history_with_course(&[], 1).unwrap(), 0);
    }

    #[test]
    fn recent_history_returns_newest_scores_first() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        let mut older = record(20, ClearType::Normal);
        older.played_at = 1;
        older.chart_sha256 = [1; 32];
        let mut newer = record(10, ClearType::Easy);
        newer.played_at = 2;
        newer.chart_sha256 = [2; 32];
        newer.autoplay = true;

        db.insert_score(&older).unwrap();
        db.insert_score(&newer).unwrap();

        let history = db.recent_history(10, 0).unwrap();

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].chart_sha256, [2; 32]);
        assert_eq!(history[0].played_at, 2);
        assert!(history[0].autoplay);
        assert_eq!(history[1].chart_sha256, [1; 32]);
    }

    #[test]
    fn recent_history_exposes_course_score_id_when_tagged() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);

        let mut solo = record(20, ClearType::Normal);
        solo.chart_sha256 = [1; 32];
        let solo_id = db.insert_score(&solo).unwrap();

        let mut course_play = record(30, ClearType::Easy);
        course_play.chart_sha256 = [2; 32];
        let course_play_id = db.insert_score(&course_play).unwrap();

        let course_score_id = insert_test_course_score(&mut db, "course-a");

        // Tag the course-attempt row only.
        db.tag_score_history_with_course(&[course_play_id], course_score_id).unwrap();

        let history = db.recent_history(10, 0).unwrap();
        let by_id: std::collections::HashMap<i64, &ScoreHistoryEntry> =
            history.iter().map(|h| (h.id, h)).collect();
        assert_eq!(by_id.get(&solo_id).unwrap().course_score_id, None);
        assert_eq!(by_id.get(&course_play_id).unwrap().course_score_id, Some(course_score_id));
    }

    #[test]
    fn deleting_course_score_nulls_history_course_link() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);

        let mut course_play = record(30, ClearType::Easy);
        course_play.chart_sha256 = [2; 32];
        let course_play_id = db.insert_score(&course_play).unwrap();
        let course_score_id = insert_test_course_score(&mut db, "course-a");
        db.tag_score_history_with_course(&[course_play_id], course_score_id).unwrap();

        db.conn_mut()
            .execute("DELETE FROM course_scores WHERE id = ?1", params![course_score_id])
            .unwrap();

        let course_score_id_after_delete: Option<i64> = db
            .conn()
            .query_row(
                "SELECT course_score_id FROM score_history WHERE id = ?1",
                params![course_play_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(course_score_id_after_delete, None);
    }

    #[test]
    fn score_db_migrations_do_not_leave_ir_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM sqlite_master
                 WHERE type = 'table' AND name LIKE 'ir_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn score_db_migrations_drop_redundant_prefix_indexes() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();

        for index in
            ["idx_score_best_chart", "idx_replay_slots_chart", "idx_score_course_replay_slots_hash"]
        {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    params![index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "{index} should be covered by a PRIMARY KEY prefix");
        }
    }

    #[test]
    fn chart_sha256_columns_are_lowercase_hex_text() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut db = ScoreDatabase::from_connection(conn);
        db.insert_score(&record(20, ClearType::Normal)).unwrap();

        let (hist_typeof, best_typeof, best_hex): (String, String, String) = db
            .conn()
            .query_row(
                "SELECT
                    (SELECT typeof(chart_sha256) FROM score_history LIMIT 1),
                    (SELECT typeof(chart_sha256) FROM score_best LIMIT 1),
                    (SELECT chart_sha256 FROM score_best LIMIT 1)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(hist_typeof, "text");
        assert_eq!(best_typeof, "text");
        assert_eq!(best_hex.len(), 64);
        assert!(best_hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }
}
