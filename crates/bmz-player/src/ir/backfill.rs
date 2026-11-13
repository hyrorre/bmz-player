use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use bmz_chart::model::LongNoteMode;
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::gauge::gauge_total_for_chart;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config::profile_config::{IrConfig, IrProviderConfig};
use crate::ln_policy::{LnScorePolicy, played_ln_mode};
use crate::select_options::{ArrangeOption, DoubleOption, DoubleOptionScoreBucket};
use crate::storage::common::{hash_to_hex, hex_to_hash};
use crate::storage::library_db::{ChartAnalysis, ChartListItem, LibraryDatabase};
use crate::storage::network_db::{
    IrBackfillResendOutcome, IrJobKind, NetworkDatabase, NewIrScoreJob,
};
use crate::storage::score_db::{ScoreDatabase, ScoreSourceKind};

use super::download::IR_SCORE_DOWNLOAD_SOURCE;
use super::types::{
    IrChartBpm, IrChartFeatures, IrChartLnProfile, IrChartNotes, IrChartPayload, IrClientInfo,
    IrEffectiveLnMode, IrJudgePayload, IrJudgeSidePayload, IrReplayPayload, IrResultPayload,
    IrRulePayload, IrScoreSubmission,
};

pub const DEFAULT_UPLOAD_LOCAL_LIMIT: u32 = 200;
pub const LOCAL_BACKFILL_SOURCE: &str = "local_backfill";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrLocalUploadOptions {
    pub provider: Option<String>,
    pub limit: u32,
    pub dry_run: bool,
    pub resend: bool,
    pub include_course_stages: bool,
    pub include_replay: bool,
}

impl Default for IrLocalUploadOptions {
    fn default() -> Self {
        Self {
            provider: None,
            limit: DEFAULT_UPLOAD_LOCAL_LIMIT,
            dry_run: false,
            resend: false,
            include_course_stages: false,
            include_replay: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct IrLocalUploadReport {
    pub provider_key: String,
    pub account_id: String,
    pub scanned: u32,
    pub candidates: u32,
    pub enqueued: u32,
    pub skipped_already_submitted: u32,
    pub skipped_already_queued: u32,
    pub skipped_resend_payload_unavailable: u32,
    pub skipped_busy: u32,
    pub skipped_missing_chart: u32,
    pub skipped_course_stage: u32,
    pub skipped_autoplay: u32,
    pub missing_replays: u32,
    /// 読み取り・payload作成に失敗して飛ばした履歴数。詳細は `messages`。
    pub skipped_invalid: u32,
    pub limit_reached: bool,
    pub messages: Vec<String>,
}

impl IrLocalUploadReport {
    fn record_invalid(&mut self, score_id: i64, error: &anyhow::Error) {
        self.skipped_invalid += 1;
        self.messages.push(format!("score {score_id}: {error:#}"));
    }
}

pub fn is_local_backfill_submission(payload: &IrScoreSubmission) -> bool {
    payload
        .play_options
        .get("submission_source")
        .and_then(Value::as_str)
        .is_some_and(|value| value == LOCAL_BACKFILL_SOURCE)
}

pub fn resolve_local_upload_target(
    ir_config: &IrConfig,
    requested: Option<&str>,
) -> Result<(String, String)> {
    let target = resolve_target_provider(ir_config, requested)?;
    Ok((target.provider_key, target.account_id))
}

/// 走査を一度に読む `score_history` の行数。全履歴をメモリへ載せない。
const SCORE_HISTORY_SCAN_PAGE_SIZE: u32 = 512;

/// 同じ実行で投入batchを繰り返すときの走査状態。
#[derive(Debug, Default)]
pub(crate) struct IrLocalUploadScan {
    /// resendで処理済み、または読み取りに失敗した履歴。
    pub(crate) processed: HashSet<i64>,
    /// 最後に走査した履歴の `(played_at, id)`。次のbatchはこの後から読む。
    resume_after: Option<(i64, i64)>,
}

pub fn enqueue_local_score_jobs(
    profile_root: &Path,
    ir_config: &IrConfig,
    score_db: &ScoreDatabase,
    library_db: &LibraryDatabase,
    network_db: &mut NetworkDatabase,
    options: &IrLocalUploadOptions,
    now: i64,
) -> Result<IrLocalUploadReport> {
    enqueue_local_score_jobs_with_scan(
        profile_root,
        ir_config,
        score_db,
        library_db,
        network_db,
        options,
        now,
        &mut IrLocalUploadScan::default(),
    )
}

/// `scan` の続きから履歴を走査して投入する。
///
/// 走査は `(played_at, id)` 順で、batch上限に達した行の次から再開する。上限に達するまでの
/// 候補は全て投入済みなので、同じ実行内で前の行を読み直す必要はない。
pub(crate) fn enqueue_local_score_jobs_with_scan(
    profile_root: &Path,
    ir_config: &IrConfig,
    score_db: &ScoreDatabase,
    library_db: &LibraryDatabase,
    network_db: &mut NetworkDatabase,
    options: &IrLocalUploadOptions,
    now: i64,
    scan: &mut IrLocalUploadScan,
) -> Result<IrLocalUploadReport> {
    let target = resolve_target_provider(ir_config, options.provider.as_deref())?;
    let limit = options.limit.max(1);
    let mut report = IrLocalUploadReport {
        provider_key: target.provider_key.clone(),
        account_id: target.account_id.clone(),
        ..IrLocalUploadReport::default()
    };
    let processed = &mut scan.processed;

    'scan: loop {
        let page =
            load_score_history_page(score_db, scan.resume_after, SCORE_HISTORY_SCAN_PAGE_SIZE)?;
        if page.is_empty() {
            break;
        }
        for (key, row) in page {
            let previous_key = scan.resume_after.replace(key);
            report.scanned += 1;
            // resendでは処理済みの履歴、全モードで読み取りに失敗した履歴を同じ実行内で再走査しない。
            let row = match row {
                Ok(row) if processed.contains(&row.id) => continue,
                Ok(row) => row,
                Err(invalid) => {
                    if processed.insert(invalid.id) {
                        report.record_invalid(invalid.id, &invalid.error);
                    }
                    continue;
                }
            };

            if !options.include_course_stages && row.course_score_id.is_some() {
                report.skipped_course_stage += 1;
                continue;
            }
            if row.autoplay {
                report.skipped_autoplay += 1;
                continue;
            }
            if has_score_from_target_ir_source(score_db, &target, row.id)? {
                report.skipped_already_submitted += 1;
                continue;
            }
            if options.resend {
                match network_db.enqueue_backfill_score_resend(
                    &target.provider_key,
                    &target.account_id,
                    row.id,
                    now,
                    None,
                    options.dry_run,
                )? {
                    IrBackfillResendOutcome::Enqueued if options.dry_run => {
                        report.candidates += 1;
                        continue;
                    }
                    IrBackfillResendOutcome::Enqueued => {
                        processed.insert(row.id);
                        report.candidates += 1;
                        report.enqueued += 1;
                        if report.enqueued >= limit {
                            report.limit_reached = true;
                            break 'scan;
                        }
                        continue;
                    }
                    IrBackfillResendOutcome::AlreadyPending => {
                        processed.insert(row.id);
                        report.skipped_already_queued += 1;
                        continue;
                    }
                    IrBackfillResendOutcome::Busy => {
                        processed.insert(row.id);
                        report.skipped_busy += 1;
                        continue;
                    }
                    IrBackfillResendOutcome::PayloadUnavailable => {
                        processed.insert(row.id);
                        report.skipped_resend_payload_unavailable += 1;
                        continue;
                    }
                    IrBackfillResendOutcome::NeedsPayload => {}
                }
            }
            if !options.resend {
                match existing_score_state(network_db, &target, row.id).with_context(|| {
                    format!("failed to check IR submission state for score {}", row.id)
                })? {
                    ExistingScoreState::Submitted => {
                        report.skipped_already_submitted += 1;
                        continue;
                    }
                    ExistingScoreState::Queued => {
                        report.skipped_already_queued += 1;
                        continue;
                    }
                    ExistingScoreState::None => {}
                }
            }

            let prepared = match prepare_submission_inputs(profile_root, library_db, &row, options)
            {
                Ok(Some(prepared)) => prepared,
                Ok(None) => {
                    report.skipped_missing_chart += 1;
                    continue;
                }
                Err(error) => {
                    processed.insert(row.id);
                    report.record_invalid(row.id, &error);
                    continue;
                }
            };
            let PreparedSubmissionInputs { chart, analysis, replay_hash } = prepared;
            if options.include_replay
                && row.replay_path.as_deref().is_some_and(|path| !path.is_empty())
                && replay_hash.is_none()
            {
                report.missing_replays += 1;
            }

            report.candidates += 1;
            if options.dry_run {
                continue;
            }
            if report.enqueued >= limit {
                report.limit_reached = true;
                // この行は投入していないので、次のbatchはこの行から読む。
                scan.resume_after = previous_key;
                break 'scan;
            }

            let payload =
                build_local_score_submission(&row, &chart, analysis.as_ref(), replay_hash);
            let new_job = NewIrScoreJob {
                provider: target.provider_key.clone(),
                account_id: target.account_id.clone(),
                kind: IrJobKind::Score,
                local_score_id: row.id,
                chart_sha256: row.chart_sha256,
                ln_policy: row.ln_policy,
                payload_json: serde_json::to_string(&payload)?,
                now,
            };
            if options.resend {
                match network_db.enqueue_backfill_score_resend(
                    &target.provider_key,
                    &target.account_id,
                    row.id,
                    now,
                    Some(&new_job),
                    false,
                )? {
                    IrBackfillResendOutcome::Enqueued => {
                        report.enqueued += 1;
                        processed.insert(row.id);
                    }
                    IrBackfillResendOutcome::AlreadyPending => {
                        processed.insert(row.id);
                        report.skipped_already_queued += 1;
                    }
                    IrBackfillResendOutcome::Busy => {
                        processed.insert(row.id);
                        report.skipped_busy += 1;
                    }
                    IrBackfillResendOutcome::PayloadUnavailable => {
                        processed.insert(row.id);
                        report.skipped_resend_payload_unavailable += 1;
                    }
                    IrBackfillResendOutcome::NeedsPayload => unreachable!("payload provided"),
                }
            } else {
                network_db.enqueue_ir_score_job(&new_job)?;
                report.enqueued += 1;
            }
            if report.enqueued >= limit {
                report.limit_reached = true;
                break 'scan;
            }
        }
    }

    Ok(report)
}

struct PreparedSubmissionInputs {
    chart: ChartListItem,
    analysis: Option<ChartAnalysis>,
    replay_hash: Option<String>,
}

/// 1履歴分の送信に必要な譜面情報とreplay hashを集める。譜面が無ければ `None`。
///
/// ここでの失敗（ライブラリの読み取り、replayファイルの読み取り）はその履歴だけを飛ばす。
fn prepare_submission_inputs(
    profile_root: &Path,
    library_db: &LibraryDatabase,
    row: &BackfillScoreRow,
    options: &IrLocalUploadOptions,
) -> Result<Option<PreparedSubmissionInputs>> {
    let Some(chart) = primary_chart_for_score(library_db, row.chart_sha256)? else {
        return Ok(None);
    };
    if options.dry_run {
        // 件数だけを数える。payload用の解析情報は読まず、replayはhashせず存在だけ確かめる。
        let replay_present = options.include_replay
            && replay_file_path(profile_root, row.replay_path.as_deref())
                .is_some_and(|path| path.is_file());
        return Ok(Some(PreparedSubmissionInputs {
            chart,
            analysis: None,
            replay_hash: replay_present.then(String::new),
        }));
    }
    let analysis = library_db.chart_analysis_by_chart_id(chart.chart_id)?;
    let replay_hash = if options.include_replay {
        replay_hash(profile_root, row.replay_path.as_deref())?
    } else {
        None
    };
    Ok(Some(PreparedSubmissionInputs { chart, analysis, replay_hash }))
}

fn has_score_from_target_ir_source(
    score_db: &ScoreDatabase,
    target: &TargetProvider,
    local_score_id: i64,
) -> Result<bool> {
    let exists: bool = score_db.conn().query_row(
        "SELECT EXISTS (
            SELECT 1 FROM score_history_sources
            WHERE score_history_id = ?1
              AND source = ?2
              AND provider = ?3
              AND account_id = ?4
        )",
        rusqlite::params![
            local_score_id,
            IR_SCORE_DOWNLOAD_SOURCE,
            target.provider_key,
            target.account_id,
        ],
        |row| row.get(0),
    )?;
    Ok(exists)
}

#[derive(Debug, Clone)]
struct TargetProvider {
    provider_key: String,
    account_id: String,
}

fn resolve_target_provider(
    ir_config: &IrConfig,
    requested: Option<&str>,
) -> Result<TargetProvider> {
    let provider =
        if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
            ir_config
                .providers
                .iter()
                .find(|entry| {
                    entry.provider == requested
                        || crate::ir::provider_key::configured_provider_key(entry)
                            .is_some_and(|provider_key| provider_key == requested)
                })
                .with_context(|| format!("IR provider is not configured: {requested}"))?
        } else if !ir_config.primary_provider.trim().is_empty() {
            crate::ir::provider_key::provider_config_for_key(
                ir_config,
                ir_config.primary_provider.trim(),
            )
            .with_context(|| {
                format!("primary IR provider is not configured: {}", ir_config.primary_provider)
            })?
        } else {
            ir_config
                .providers
                .iter()
                .find(|entry| {
                    entry.enabled
                        && !entry.base_url.is_empty()
                        && crate::ir::provider_key::configured_provider_key(entry).is_some()
                })
                .context("no IR provider configured; run `bmz ir login` first")?
        };
    target_from_provider(provider)
}

fn target_from_provider(provider: &IrProviderConfig) -> Result<TargetProvider> {
    if crate::ir::bokutachi::is_config(provider) {
        return Err(crate::ir::bokutachi::unsupported());
    }
    if crate::ir::bms_ir::is_bms_ir_config(provider) {
        bail!("BMS-IR local score backfill is disabled");
    }
    if crate::ir::rian_ir::is_rian_ir_config(provider) {
        bail!("rianIR local score backfill is disabled");
    }
    if !provider.enabled {
        bail!("IR provider '{}' is disabled", provider.provider);
    }
    if provider.base_url.trim().is_empty() {
        bail!("IR provider '{}' has no base_url; run `bmz ir login` first", provider.provider);
    }
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
        .context("IR provider key is not set; log in again")?;
    if provider.account_id.trim().is_empty() {
        bail!("IR account id is empty for provider '{}'; run `bmz ir login` first", provider_key);
    }
    Ok(TargetProvider {
        provider_key: provider_key.to_string(),
        account_id: provider.account_id.clone(),
    })
}

fn has_successful_score_submission(
    network_db: &NetworkDatabase,
    target: &TargetProvider,
    local_score_id: i64,
) -> Result<bool> {
    network_db.has_successful_ir_score_submission(
        &target.provider_key,
        &target.account_id,
        IrJobKind::Score,
        local_score_id,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingScoreState {
    None,
    Queued,
    Submitted,
}

fn existing_score_state(
    network_db: &NetworkDatabase,
    target: &TargetProvider,
    local_score_id: i64,
) -> Result<ExistingScoreState> {
    if has_successful_score_submission(network_db, target, local_score_id)? {
        return Ok(ExistingScoreState::Submitted);
    }
    if network_db.has_ir_score_job(
        &target.provider_key,
        &target.account_id,
        IrJobKind::Score,
        local_score_id,
    )? {
        return Ok(ExistingScoreState::Queued);
    }
    Ok(ExistingScoreState::None)
}

#[derive(Debug, Clone)]
struct BackfillScoreRow {
    id: i64,
    idempotency_key: String,
    chart_sha256: [u8; 32],
    ln_policy: LnScorePolicy,
    double_option: DoubleOptionScoreBucket,
    applied_double_option: DoubleOption,
    played_at: i64,
    clear_type: String,
    gauge_type: String,
    total_notes: u32,
    ex_score: u32,
    bp: u32,
    cb: u32,
    max_combo: u32,
    fast_pgreat: u32,
    slow_pgreat: u32,
    fast_great: u32,
    slow_great: u32,
    fast_good: u32,
    slow_good: u32,
    fast_bad: u32,
    slow_bad: u32,
    fast_poor: u32,
    slow_poor: u32,
    fast_empty_poor: u32,
    slow_empty_poor: u32,
    random_seed: Option<i64>,
    seed_scheme: String,
    arrange: ArrangeOption,
    arrange_2p: ArrangeOption,
    gauge_option: String,
    rule_mode: String,
    assist_mask: u32,
    autoplay: bool,
    device_type: InputDeviceKind,
    replay_path: Option<String>,
    course_score_id: Option<i64>,
    source_kind: ScoreSourceKind,
}

/// 値を解釈できなかった `score_history` 行。他の行の処理は続ける。
struct InvalidScoreRow {
    id: i64,
    error: anyhow::Error,
}

type ScoreHistoryPage = Vec<((i64, i64), std::result::Result<BackfillScoreRow, InvalidScoreRow>)>;

/// `(played_at, id)` が `after` より後の履歴を最大 `limit` 行、その順で読む。
fn load_score_history_page(
    score_db: &ScoreDatabase,
    after: Option<(i64, i64)>,
    limit: u32,
) -> Result<ScoreHistoryPage> {
    let mut stmt = score_db.conn().prepare(
        "SELECT
            id,
            chart_sha256,
            ln_policy,
            double_option,
            played_at,
            clear_type,
            gauge_type,
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
            gauge_option,
            rule_mode,
            assist_mask,
            autoplay,
            device_type,
            replay_path,
            course_score_id,
            arrange_2p,
            applied_double_option,
            source_kind,
            seed_scheme,
            score_history_ir_identity.idempotency_key
         FROM score_history
         JOIN score_history_ir_identity
           ON score_history_ir_identity.score_history_id = score_history.id
         WHERE ?1 IS NULL
            OR played_at > ?1
            OR (played_at = ?1 AND score_history.id > ?2)
         ORDER BY played_at ASC, score_history.id ASC
         LIMIT ?3",
    )?;
    let (after_played_at, after_id) = after.unzip();
    let rows = stmt.query_map(rusqlite::params![after_played_at, after_id, limit], |row| {
        let id: i64 = row.get(0)?;
        let played_at: i64 = row.get(4)?;
        Ok((
            (played_at, id),
            score_history_row_from_row(row)
                .map_err(|error| InvalidScoreRow { id, error: anyhow::Error::new(error) }),
        ))
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

fn score_history_row_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BackfillScoreRow> {
    let sha256_hex: String = row.get(1)?;
    let ln_policy: String = row.get(2)?;
    let double_option: String = row.get(3)?;
    let arrange: String = row.get(25)?;
    let device_type: String = row.get(30)?;
    let replay_path: String = row.get(31)?;
    let arrange_2p: String = row.get(33)?;
    let applied_double_option: String = row.get(34)?;
    let source_kind: String = row.get(35)?;
    let seed_scheme: String = row.get(36)?;
    Ok(BackfillScoreRow {
        id: row.get(0)?,
        idempotency_key: row.get(37)?,
        chart_sha256: hex_to_hash::<32>(&sha256_hex)?,
        ln_policy: LnScorePolicy::from_str_opt(&ln_policy).unwrap_or(LnScorePolicy::ForceLn),
        double_option: DoubleOptionScoreBucket::from_str_or_off(&double_option),
        applied_double_option: DoubleOption::from_persistent_str(&applied_double_option),
        played_at: row.get(4)?,
        clear_type: row.get(5)?,
        gauge_type: row.get(6)?,
        total_notes: row.get(7)?,
        ex_score: row.get(8)?,
        bp: row.get(9)?,
        cb: row.get(10)?,
        max_combo: row.get(11)?,
        fast_pgreat: row.get(12)?,
        slow_pgreat: row.get(13)?,
        fast_great: row.get(14)?,
        slow_great: row.get(15)?,
        fast_good: row.get(16)?,
        slow_good: row.get(17)?,
        fast_bad: row.get(18)?,
        slow_bad: row.get(19)?,
        fast_poor: row.get(20)?,
        slow_poor: row.get(21)?,
        fast_empty_poor: row.get(22)?,
        slow_empty_poor: row.get(23)?,
        random_seed: row.get(24)?,
        seed_scheme,
        arrange: ArrangeOption::from_persistent_str(&arrange),
        arrange_2p: ArrangeOption::from_persistent_str(&arrange_2p),
        gauge_option: row.get(26)?,
        rule_mode: row.get(27)?,
        assist_mask: row.get(28)?,
        autoplay: row.get(29)?,
        device_type: device_type_from_str(&device_type),
        replay_path: if replay_path.is_empty() { None } else { Some(replay_path) },
        course_score_id: row.get(32)?,
        source_kind: ScoreSourceKind::from_str_opt(&source_kind).unwrap_or_default(),
    })
}

fn primary_chart_for_score(
    library_db: &LibraryDatabase,
    sha256: [u8; 32],
) -> Result<Option<ChartListItem>> {
    Ok(library_db.preferred_chart_metadata(sha256)?.into_iter().next().map(|source| source.chart))
}

fn replay_file_path(profile_root: &Path, replay_path: Option<&str>) -> Option<std::path::PathBuf> {
    replay_path.filter(|value| !value.is_empty()).map(|path| profile_root.join(path))
}

fn replay_hash(profile_root: &Path, replay_path: Option<&str>) -> Result<Option<String>> {
    let Some(path) = replay_file_path(profile_root, replay_path) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read replay file: {}", path.display()))?;
    Ok(Some(hash_to_hex(&Sha256::digest(bytes))))
}

fn build_local_score_submission(
    row: &BackfillScoreRow,
    chart: &ChartListItem,
    analysis: Option<&ChartAnalysis>,
    replay_hash: Option<String>,
) -> IrScoreSubmission {
    let double_multiplier = match row.double_option {
        DoubleOptionScoreBucket::Off => 1,
        DoubleOptionScoreBucket::Battle | DoubleOptionScoreBucket::BattleAutoScratch => 2,
    };
    let expected_total_notes =
        chart.scored_total_notes(row.ln_policy).saturating_mul(double_multiplier);
    // Preserve a larger historical count for forward compatibility, while
    // repairing legacy BMZ CN/HCN rows that stored the base library count.
    let scored_total_notes = row.total_notes.max(expected_total_notes);
    let mut play_options = BTreeMap::new();
    let arrange_1p = arrange_option_ir(row.arrange);
    play_options
        .insert("device_type".to_string(), Value::String(row.device_type.as_str().to_string()));
    play_options.insert("option".to_string(), Value::String(arrange_1p.clone()));
    play_options.insert("arrange_1p".to_string(), Value::String(arrange_1p));
    play_options.insert("arrange_2p".to_string(), Value::String(arrange_option_ir(row.arrange_2p)));
    play_options.insert(
        "double_option".to_string(),
        Value::String(row.double_option.ir_value().to_string()),
    );
    play_options.insert(
        "applied_double_option".to_string(),
        Value::String(backfill_applied_double_option(row).ir_value().to_string()),
    );
    play_options.insert(
        "source_kind".to_string(),
        Value::String(score_source_kind_ir(row.source_kind).to_string()),
    );
    if let Some(seed) = row.random_seed {
        play_options.insert("random_seed".to_string(), serde_json::json!(seed.to_string()));
        if row.seed_scheme == crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1 {
            play_options.insert("seed".to_string(), serde_json::json!(seed.to_string()));
        }
    }
    if !row.seed_scheme.is_empty() {
        play_options.insert("seed_scheme".to_string(), Value::String(row.seed_scheme.clone()));
    }
    if !row.rule_mode.is_empty() {
        play_options.insert("rule_mode".to_string(), Value::String(row.rule_mode.clone()));
    }
    if row.assist_mask != 0 {
        play_options.insert("assist_mask".to_string(), serde_json::json!(row.assist_mask));
    }
    play_options
        .insert("submission_source".to_string(), Value::String(LOCAL_BACKFILL_SOURCE.to_string()));
    play_options.insert("local_score_history_id".to_string(), serde_json::json!(row.id));

    IrScoreSubmission {
        client: IrClientInfo {
            name: "BMZ".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        },
        chart: chart_payload_from_library(chart, analysis, scored_total_notes, row.ln_policy),
        rule: IrRulePayload {
            play_mode: play_mode_payload(&chart.mode),
            key_mode: chart.mode.clone(),
            gauge: if row.gauge_option.is_empty() {
                row.gauge_type.clone()
            } else {
                row.gauge_option.clone()
            },
            ln_policy: row.ln_policy,
            effective_ln_mode: effective_ln_mode_payload(chart.ln_profile, row.ln_policy),
            judge_algorithm: "bmz_v1".to_string(),
            scoring: "bms_ex_score_v1".to_string(),
            rule_mode: row.rule_mode.clone(),
        },
        result: IrResultPayload {
            clear: row.clear_type.clone(),
            played_at: row.played_at,
            duration_ms: None,
            judges: IrJudgePayload {
                fast: IrJudgeSidePayload {
                    pgreat: row.fast_pgreat,
                    great: row.fast_great,
                    good: row.fast_good,
                    bad: row.fast_bad,
                    poor: row.fast_poor,
                    empty_poor: row.fast_empty_poor,
                },
                slow: IrJudgeSidePayload {
                    pgreat: row.slow_pgreat,
                    great: row.slow_great,
                    good: row.slow_good,
                    bad: row.slow_bad,
                    poor: row.slow_poor,
                    empty_poor: row.slow_empty_poor,
                },
            },
            ex_score: row.ex_score,
            max_combo: row.max_combo,
            notes: scored_total_notes,
            pass_notes: Some(judged_note_count(row).min(scored_total_notes)),
            min_bp: row.bp,
            min_cb: row.cb,
            ghost: None,
        },
        play_options,
        replay: replay_hash.map(|hash| IrReplayPayload {
            hash,
            format: "bmz-replay-v1".to_string(),
            upload_intent: "later".to_string(),
        }),
        evidence: Default::default(),
        idempotency_key: row.idempotency_key.clone(),
    }
}

fn judged_note_count(row: &BackfillScoreRow) -> u32 {
    [
        row.fast_pgreat,
        row.slow_pgreat,
        row.fast_great,
        row.slow_great,
        row.fast_good,
        row.slow_good,
        row.fast_bad,
        row.slow_bad,
        row.fast_poor,
        row.slow_poor,
    ]
    .into_iter()
    .fold(0_u32, u32::saturating_add)
}

fn backfill_applied_double_option(row: &BackfillScoreRow) -> DoubleOption {
    if row.applied_double_option.score_bucket() == row.double_option {
        row.applied_double_option
    } else {
        match row.double_option {
            DoubleOptionScoreBucket::Off => DoubleOption::Off,
            DoubleOptionScoreBucket::Battle => DoubleOption::Battle,
            DoubleOptionScoreBucket::BattleAutoScratch => DoubleOption::BattleAutoScratch,
        }
    }
}

fn score_source_kind_ir(source_kind: ScoreSourceKind) -> &'static str {
    match source_kind {
        ScoreSourceKind::Local => "local",
        ScoreSourceKind::Beatoraja => "beatoraja",
        ScoreSourceKind::Lr2 => "lr2",
        ScoreSourceKind::Lr2Oraja => "lr2oraja",
        ScoreSourceKind::Lr2OrajaDx => "lr2oraja_dx",
    }
}

fn chart_payload_from_library(
    chart: &ChartListItem,
    analysis: Option<&ChartAnalysis>,
    score_total_notes: u32,
    ln_policy: LnScorePolicy,
) -> IrChartPayload {
    let mine_notes = analysis
        .map(|analysis| analysis.lane_notes.iter().map(|lane| lane.mines).sum())
        .unwrap_or_else(|| u32::from(chart.has_mines));
    let (ln, cn, hcn) = long_note_counts_for_policy(chart, ln_policy);
    let metadata_total = if chart.bms_total > 0.0 { Some(chart.bms_total) } else { None };
    let gauge_total = gauge_total_for_chart(metadata_total, score_total_notes);

    IrChartPayload {
        source_format: String::new(),
        sha256: hash_to_hex(&chart.sha256),
        md5: Some(hash_to_hex(&chart.md5)),
        // Backfill scores do not have a hardware-clock play duration to pair with this value.
        length_ms: None,
        ln_profile: IrChartLnProfile {
            has_undefined_ln: chart.ln_profile.has_undefined_ln,
            has_defined_ln: chart.ln_profile.has_defined_ln,
            has_defined_cn: chart.ln_profile.has_defined_cn,
            has_defined_hcn: chart.ln_profile.has_defined_hcn,
        },
        title: chart.title.clone(),
        subtitle: chart.subtitle.clone(),
        genre: chart.genre.clone(),
        artist: chart.artist.clone(),
        subartists: subartists(&chart.subartist),
        mode: chart.mode.clone(),
        level: parse_play_level(&chart.play_level),
        difficulty: chart.difficulty_name.clone(),
        total: Some(gauge_total),
        judge: chart.judge_rank,
        bpm: Some(IrChartBpm { min: Some(chart.min_bpm), max: Some(chart.max_bpm) }),
        notes: IrChartNotes { total: score_total_notes, ln, cn, hcn, mine: mine_notes },
        features: IrChartFeatures {
            random: false,
            stop: false,
            ln: chart.ln_profile.has_undefined_ln || chart.ln_profile.has_defined_ln,
            cn: chart.ln_profile.has_defined_cn,
            hcn: chart.ln_profile.has_defined_hcn,
            mine: chart.has_mines || mine_notes > 0,
        },
        urls: None,
        headers: Default::default(),
    }
}

fn long_note_counts_for_policy(chart: &ChartListItem, policy: LnScorePolicy) -> (u32, u32, u32) {
    let total = chart.ln_counts.total_pairs();
    match policy {
        LnScorePolicy::ForceLn => (total, 0, 0),
        LnScorePolicy::ForceCn => (total, total, 0),
        LnScorePolicy::ForceHcn => (total, 0, total),
        LnScorePolicy::AutoLn | LnScorePolicy::AutoCn | LnScorePolicy::AutoHcn => {
            (total, chart.ln_counts.defined_cn_pairs, chart.ln_counts.defined_hcn_pairs)
        }
    }
}

fn effective_ln_mode_payload(
    profile: crate::ln_policy::ChartLnProfile,
    policy: LnScorePolicy,
) -> IrEffectiveLnMode {
    match played_ln_mode(profile, policy).unwrap_or(LongNoteMode::Ln) {
        LongNoteMode::Ln => IrEffectiveLnMode::Ln,
        LongNoteMode::Cn => IrEffectiveLnMode::Cn,
        LongNoteMode::Hcn => IrEffectiveLnMode::Hcn,
    }
}

fn arrange_option_ir(arrange: ArrangeOption) -> String {
    arrange.as_str().to_ascii_lowercase()
}

fn play_mode_payload(value: &str) -> String {
    match value {
        "10K" | "14K" => "double",
        _ => "single",
    }
    .to_string()
}

fn subartists(value: &str) -> Vec<String> {
    if value.is_empty() { Vec::new() } else { vec![value.to_string()] }
}

fn parse_play_level(value: &str) -> Option<i32> {
    let trimmed = value.trim();
    if trimmed.is_empty() { None } else { trimmed.parse().ok() }
}

fn device_type_from_str(value: &str) -> InputDeviceKind {
    match value {
        "controller" => InputDeviceKind::Controller,
        _ => InputDeviceKind::Keyboard,
    }
}

#[cfg(test)]
#[path = "backfill/tests.rs"]
pub(crate) mod tests;
