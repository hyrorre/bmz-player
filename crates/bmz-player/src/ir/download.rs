use anyhow::{Context, Result, bail};
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::score::{JudgeCounts, ScoreState};
use rusqlite::{OptionalExtension, params};

use crate::ln_policy::LnScorePolicy;
use crate::select_options::{DoubleOption, DoubleOptionScoreBucket};
use crate::storage::common::hex_to_hash;
use crate::storage::network_db::NetworkDatabase;
use crate::storage::score_db::{
    ScoreDatabase, ScoreHistorySourceKey, ScoreHistorySourceRecord, ScoreRecord,
    ScoreSourceInsertOutcome, ScoreSourceKind,
};

use super::bmz_official::{BmzOfficialIrClient, IrOwnScoreHistoryRequest};
use super::types::{IrJudgePayload, IrOwnScoreHistoryEntry};

pub const DEFAULT_DOWNLOAD_SCORES_LIMIT: u32 = 200;
pub const IR_SCORE_DOWNLOAD_SOURCE: &str = "bmz_ir";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrScoreDownloadOptions {
    pub provider: Option<String>,
    pub limit: u32,
    pub dry_run: bool,
}

impl Default for IrScoreDownloadOptions {
    fn default() -> Self {
        Self { provider: None, limit: DEFAULT_DOWNLOAD_SCORES_LIMIT, dry_run: false }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IrScoreDownloadReport {
    pub provider_key: String,
    pub account_id: String,
    pub pages: u32,
    pub scanned: u32,
    pub candidates: u32,
    pub imported: u32,
    pub linked_existing: u32,
    pub skipped_existing: u32,
    pub failed: u32,
    pub limit_reached: bool,
    /// 中断要求により途中で止めた場合に true。取り込み済みの分は保存されている。
    pub cancelled: bool,
    pub messages: Vec<String>,
}

pub enum IrOwnScoreHistoryClient {
    BmzOfficial(BmzOfficialIrClient),
    BmsIr { client: crate::ir::bms_ir::BmsIrClient, player_id: String, game_token: String },
}

impl IrOwnScoreHistoryClient {
    async fn fetch(
        &self,
        request: &IrOwnScoreHistoryRequest,
    ) -> Result<super::types::IrOwnScoreHistoryResult> {
        match self {
            Self::BmzOfficial(client) => client.fetch_own_scores(request).await,
            Self::BmsIr { client, player_id, game_token } => {
                client.fetch_own_scores(request, player_id, game_token).await
            }
        }
    }
}

pub async fn download_ir_scores(
    client: &IrOwnScoreHistoryClient,
    provider_key: &str,
    account_id: &str,
    score_db: &mut ScoreDatabase,
    network_db: &NetworkDatabase,
    options: &IrScoreDownloadOptions,
    now: i64,
) -> Result<IrScoreDownloadReport> {
    download_ir_scores_with_hooks(
        client,
        provider_key,
        account_id,
        score_db,
        network_db,
        options,
        now,
        &mut |_| {},
        None,
    )
    .await
}

/// `download_ir_scores` に、ページ取り込みごとの通知と中断要求を足した版。
///
/// 中断はページ取得前と取得待ち中に確認する。取り込み中のページは最後まで処理するため、
/// 中断後も保存済みの状態は常にページ単位で整合する。
pub async fn download_ir_scores_with_hooks(
    client: &IrOwnScoreHistoryClient,
    provider_key: &str,
    account_id: &str,
    score_db: &mut ScoreDatabase,
    network_db: &NetworkDatabase,
    options: &IrScoreDownloadOptions,
    now: i64,
    on_page: &mut dyn FnMut(&IrScoreDownloadReport),
    cancel: Option<&tokio::sync::watch::Receiver<bool>>,
) -> Result<IrScoreDownloadReport> {
    let limit = options.limit.max(1);
    let mut report = IrScoreDownloadReport {
        provider_key: provider_key.to_string(),
        account_id: account_id.to_string(),
        ..IrScoreDownloadReport::default()
    };
    let mut cursor = None;

    while report.action_count() < limit {
        if cancel.is_some_and(|cancel| *cancel.borrow()) {
            report.cancelled = true;
            break;
        }
        let page_limit = 100;
        let request =
            IrOwnScoreHistoryRequest { limit: page_limit, offset: 0, cursor: cursor.clone() };
        let page = match cancel {
            Some(cancel) => {
                let mut cancel = cancel.clone();
                tokio::select! {
                    page = client.fetch(&request) => page?,
                    true = async { cancel.wait_for(|requested| *requested).await.is_ok() } => {
                        report.cancelled = true;
                        break;
                    }
                }
            }
            None => client.fetch(&request).await?,
        };
        report.pages += 1;
        if page.scores.is_empty() {
            break;
        }

        let consumed = import_ir_score_entries_up_to(
            score_db,
            network_db,
            provider_key,
            account_id,
            &page.scores,
            options.dry_run,
            now,
            &mut report,
            Some(limit),
        )?;
        on_page(&report);

        if consumed < page.scores.len() {
            report.limit_reached = true;
            break;
        }
        if !page.pagination.has_more {
            break;
        }
        if report.action_count() >= limit {
            report.limit_reached = true;
            break;
        }
        let next_cursor = page
            .pagination
            .next_cursor
            .context("IR score history response is missing the next page cursor")?;
        if cursor.as_ref() == Some(&next_cursor) {
            bail!("IR score history cursor did not advance");
        }
        cursor = Some(next_cursor);
    }

    Ok(report)
}

pub fn import_ir_score_entries(
    score_db: &mut ScoreDatabase,
    network_db: &NetworkDatabase,
    provider_key: &str,
    account_id: &str,
    entries: &[IrOwnScoreHistoryEntry],
    dry_run: bool,
    now: i64,
    report: &mut IrScoreDownloadReport,
) -> Result<()> {
    import_ir_score_entries_up_to(
        score_db,
        network_db,
        provider_key,
        account_id,
        entries,
        dry_run,
        now,
        report,
        None,
    )?;
    Ok(())
}

fn import_ir_score_entries_up_to(
    score_db: &mut ScoreDatabase,
    network_db: &NetworkDatabase,
    provider_key: &str,
    account_id: &str,
    entries: &[IrOwnScoreHistoryEntry],
    dry_run: bool,
    now: i64,
    report: &mut IrScoreDownloadReport,
    action_limit: Option<u32>,
) -> Result<usize> {
    let mut consumed = 0;
    for entry in entries {
        if action_limit.is_some_and(|limit| report.action_count() >= limit) {
            break;
        }
        consumed += 1;
        report.scanned += 1;
        if let Err(error) = import_ir_score_entry(
            score_db,
            network_db,
            provider_key,
            account_id,
            entry,
            dry_run,
            now,
            report,
        ) {
            report.failed += 1;
            report.messages.push(format!("{}: {error:#}", entry.score_id));
        }
    }
    Ok(consumed)
}

impl IrScoreDownloadReport {
    fn action_count(&self) -> u32 {
        self.candidates.saturating_add(self.linked_existing)
    }
}

fn import_ir_score_entry(
    score_db: &mut ScoreDatabase,
    network_db: &NetworkDatabase,
    provider_key: &str,
    account_id: &str,
    entry: &IrOwnScoreHistoryEntry,
    dry_run: bool,
    now: i64,
    report: &mut IrScoreDownloadReport,
) -> Result<()> {
    let source = source_record(provider_key, account_id, entry, now);
    if score_db.score_history_id_for_source(&source.key)?.is_some() {
        report.skipped_existing += 1;
        return Ok(());
    }

    if let Some(local_score_id) =
        uploaded_local_score_id(score_db, network_db, provider_key, account_id, entry)?
    {
        let linked = dry_run || score_db.attach_score_history_source(local_score_id, &source)?;
        if linked {
            report.linked_existing += 1;
        } else {
            report.skipped_existing += 1;
        }
        return Ok(());
    }

    let record = score_record_from_ir_entry(entry)?;
    report.candidates += 1;
    if dry_run {
        return Ok(());
    }

    match score_db.insert_score_with_source(&record, &source)? {
        ScoreSourceInsertOutcome::Inserted { .. } => report.imported += 1,
        ScoreSourceInsertOutcome::Duplicate { .. } => report.skipped_existing += 1,
    }
    Ok(())
}

pub fn score_record_from_ir_entry(entry: &IrOwnScoreHistoryEntry) -> Result<ScoreRecord> {
    let clear_type = clear_type_from_ir(&entry.clear)
        .with_context(|| format!("unsupported clear type: {}", entry.clear))?;
    let mut score = score_state_from_ir(&entry.judges);
    score.max_combo = entry.max_combo;
    score.past_notes = entry.pass_notes.min(entry.notes);

    if score.ex_score() != entry.ex_score {
        bail!(
            "judge counts produce EX {} but IR score reports {}",
            score.ex_score(),
            entry.ex_score
        );
    }
    if score.bp() != entry.bp || score.cb() != entry.cb {
        bail!(
            "judge counts produce BP/CB {}/{} but IR score reports {}/{}",
            score.bp(),
            score.cb(),
            entry.bp,
            entry.cb
        );
    }

    let count_unprocessed_notes = match score_record_uses_unprocessed_notes(
        &score,
        entry.notes,
        entry.min_bp,
        entry.min_cb,
    ) {
        Ok(value) => value,
        Err(error) => {
            let Some(legacy_pass_notes) = legacy_failed_pass_notes(entry, &score) else {
                return Err(error);
            };
            score.past_notes = legacy_pass_notes;
            score_record_uses_unprocessed_notes(&score, entry.notes, entry.min_bp, entry.min_cb)?
        }
    };
    let rule_mode =
        if entry.rule_mode.trim().is_empty() { "Beatoraja" } else { entry.rule_mode.as_str() };

    let double_option = double_option_from_ir(&entry.double_option);
    let source_kind = source_kind_from_ir(&entry.source_kind);
    let seed_scheme = if entry.seed_scheme.trim().is_empty() {
        match source_kind {
            ScoreSourceKind::Beatoraja => {
                crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1.to_string()
            }
            _ => crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3.to_string(),
        }
    } else {
        entry.seed_scheme.clone()
    };

    Ok(ScoreRecord {
        source_min_bp: None,
        chart_sha256: hex_to_hash::<32>(&entry.chart_sha256)?,
        ln_policy: LnScorePolicy::from_str_opt(&entry.ln_policy)
            .with_context(|| format!("unsupported LN policy: {}", entry.ln_policy))?,
        double_option,
        applied_double_option: applied_double_option_from_ir(
            &entry.applied_double_option,
            double_option,
        ),
        played_at: entry.played_at.unwrap_or(entry.server_received_at),
        clear_type,
        gauge_type: gauge_type_from_ir(&entry.gauge).or_else(|| gauge_type_for_clear(clear_type)),
        gauge_value: None,
        total_notes: entry.notes,
        playtime_seconds: entry
            .duration_ms
            .map(|duration_ms| duration_ms / 1_000)
            .unwrap_or(0)
            .min(u64::from(u32::MAX)) as u32,
        score,
        count_unprocessed_notes,
        random_seed: entry.random_seed,
        seed_scheme,
        arrange: arrange_from_ir(entry.arrange_1p.as_deref()),
        arrange_2p: arrange_from_ir(entry.arrange_2p.as_deref()),
        gauge_option: entry.gauge.clone(),
        rule_mode: rule_mode.to_string(),
        assist_mask: entry.assist_mask.unwrap_or(0),
        autoplay: false,
        device_type: device_type_from_ir(&entry.device_type),
        replay_path: String::new(),
        source_kind,
    })
}

fn legacy_failed_pass_notes(entry: &IrOwnScoreHistoryEntry, score: &ScoreState) -> Option<u32> {
    if clear_type_from_ir(&entry.clear)? != ClearType::Failed || entry.pass_notes != entry.notes {
        return None;
    }

    let unprocessed_bp = entry.min_bp.checked_sub(score.bp())?;
    let unprocessed_cb = entry.min_cb.checked_sub(score.cb())?;
    if unprocessed_bp == 0 || unprocessed_bp != unprocessed_cb {
        return None;
    }
    let inferred_pass_notes = entry.notes.checked_sub(unprocessed_bp)?;
    let judged_notes = [
        score.judges.fast_pgreat,
        score.judges.slow_pgreat,
        score.judges.fast_great,
        score.judges.slow_great,
        score.judges.fast_good,
        score.judges.slow_good,
        score.judges.fast_bad,
        score.judges.slow_bad,
        score.judges.fast_poor,
        score.judges.slow_poor,
    ]
    .into_iter()
    .fold(0_u32, u32::saturating_add);
    (inferred_pass_notes == judged_notes).then_some(inferred_pass_notes)
}

fn score_state_from_ir(judges: &IrJudgePayload) -> ScoreState {
    ScoreState {
        judges: JudgeCounts {
            fast_pgreat: judges.fast.pgreat,
            slow_pgreat: judges.slow.pgreat,
            fast_great: judges.fast.great,
            slow_great: judges.slow.great,
            fast_good: judges.fast.good,
            slow_good: judges.slow.good,
            fast_bad: judges.fast.bad,
            slow_bad: judges.slow.bad,
            fast_poor: judges.fast.poor,
            slow_poor: judges.slow.poor,
            fast_empty_poor: judges.fast.empty_poor,
            slow_empty_poor: judges.slow.empty_poor,
        },
        ..ScoreState::default()
    }
}

fn score_record_uses_unprocessed_notes(
    score: &ScoreState,
    total_notes: u32,
    min_bp: u32,
    min_cb: u32,
) -> Result<bool> {
    if score.bp() == min_bp && score.cb() == min_cb {
        return Ok(false);
    }
    if score.bp_with_unprocessed_notes(total_notes) == min_bp
        && score.cb_with_unprocessed_notes(total_notes) == min_cb
    {
        return Ok(true);
    }
    bail!("IR min BP/CB {min_bp}/{min_cb} cannot be reproduced from judges and pass_notes")
}

fn source_record(
    provider_key: &str,
    account_id: &str,
    entry: &IrOwnScoreHistoryEntry,
    now: i64,
) -> ScoreHistorySourceRecord {
    ScoreHistorySourceRecord {
        key: ScoreHistorySourceKey {
            source: IR_SCORE_DOWNLOAD_SOURCE.to_string(),
            provider: provider_key.to_string(),
            account_id: account_id.to_string(),
            remote_score_id: entry.score_id.clone(),
        },
        verification: entry.verification.clone(),
        server_received_at: entry.server_received_at,
        imported_at: now,
    }
}

/// IR上のscoreが、このクライアントから送ったローカル履歴のものなら、その履歴IDを返す。
///
/// remote score IDの成功台帳で引けない送信（応答を失った送信、score IDの無い応答、
/// 台帳追加前にpruningされた履歴、送信中のjob）も、投稿時のidempotency keyで照合する。
/// keyで引いた履歴は譜面が一致する場合だけ採用する。
fn uploaded_local_score_id(
    score_db: &ScoreDatabase,
    network_db: &NetworkDatabase,
    provider_key: &str,
    account_id: &str,
    entry: &IrOwnScoreHistoryEntry,
) -> Result<Option<i64>> {
    if let Some(local_score_id) =
        network_db.local_score_id_for_remote_score(provider_key, account_id, &entry.score_id)?
        && score_history_exists(score_db, local_score_id)?
    {
        return Ok(Some(local_score_id));
    }
    let Some(key) = entry.idempotency_key.as_deref().filter(|key| !key.is_empty()) else {
        return Ok(None);
    };
    let candidates = [
        network_db.local_score_id_for_submission_key(provider_key, account_id, key)?,
        score_db.score_history_id_for_ir_idempotency_key(key)?,
    ];
    for local_score_id in candidates.into_iter().flatten() {
        if score_history_chart_matches(score_db, local_score_id, &entry.chart_sha256)? {
            return Ok(Some(local_score_id));
        }
    }
    Ok(None)
}

fn score_history_chart_matches(
    score_db: &ScoreDatabase,
    score_history_id: i64,
    chart_sha256: &str,
) -> Result<bool> {
    let stored: Option<String> = score_db
        .conn()
        .query_row(
            "SELECT chart_sha256 FROM score_history WHERE id = ?1",
            params![score_history_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(stored.is_some_and(|stored| stored.eq_ignore_ascii_case(chart_sha256)))
}

fn score_history_exists(score_db: &ScoreDatabase, score_history_id: i64) -> Result<bool> {
    Ok(score_db
        .conn()
        .query_row(
            "SELECT 1 FROM score_history WHERE id = ?1 LIMIT 1",
            params![score_history_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn clear_type_from_ir(value: &str) -> Option<ClearType> {
    ClearType::from_label(value).or_else(|| match normalize_token(value).as_str() {
        "noplay" => Some(ClearType::NoPlay),
        "failed" => Some(ClearType::Failed),
        "assisteasy" => Some(ClearType::AssistEasy),
        "lightassisteasy" => Some(ClearType::LightAssistEasy),
        "easy" => Some(ClearType::Easy),
        "clear" | "normal" => Some(ClearType::Normal),
        "hard" => Some(ClearType::Hard),
        "exhard" => Some(ClearType::ExHard),
        "fullcombo" => Some(ClearType::FullCombo),
        "perfect" => Some(ClearType::Perfect),
        "max" => Some(ClearType::Max),
        _ => None,
    })
}

fn gauge_type_from_ir(value: &str) -> Option<GaugeType> {
    match normalize_token(value).as_str() {
        "assisteasy" | "aeasy" => Some(GaugeType::AssistEasy),
        "easy" => Some(GaugeType::Easy),
        "normal" | "groove" | "clear" => Some(GaugeType::Normal),
        "hard" => Some(GaugeType::Hard),
        "exhard" => Some(GaugeType::ExHard),
        "hazard" => Some(GaugeType::Hazard),
        "class" => Some(GaugeType::Class),
        "exclass" => Some(GaugeType::ExClass),
        "exhardclass" => Some(GaugeType::ExHardClass),
        _ => None,
    }
}

fn gauge_type_for_clear(clear_type: ClearType) -> Option<GaugeType> {
    match clear_type {
        ClearType::AssistEasy | ClearType::LightAssistEasy => Some(GaugeType::AssistEasy),
        ClearType::Easy => Some(GaugeType::Easy),
        ClearType::Normal | ClearType::FullCombo | ClearType::Perfect | ClearType::Max => {
            Some(GaugeType::Normal)
        }
        ClearType::Hard => Some(GaugeType::Hard),
        ClearType::ExHard => Some(GaugeType::ExHard),
        ClearType::NoPlay | ClearType::Failed => None,
    }
}

fn double_option_from_ir(value: &str) -> DoubleOptionScoreBucket {
    DoubleOptionScoreBucket::from_ir_query_or_off(Some(value))
}

fn applied_double_option_from_ir(
    value: &str,
    score_bucket: DoubleOptionScoreBucket,
) -> DoubleOption {
    match normalize_token(value).as_str() {
        "flip" => DoubleOption::Flip,
        "battle" => DoubleOption::Battle,
        "battleautoscratch" | "battleassist" => DoubleOption::BattleAutoScratch,
        _ => match score_bucket {
            DoubleOptionScoreBucket::Off => DoubleOption::Off,
            DoubleOptionScoreBucket::Battle => DoubleOption::Battle,
            DoubleOptionScoreBucket::BattleAutoScratch => DoubleOption::BattleAutoScratch,
        },
    }
}

fn source_kind_from_ir(value: &str) -> ScoreSourceKind {
    match normalize_token(value).as_str() {
        "beatoraja" => ScoreSourceKind::Beatoraja,
        "lr2" => ScoreSourceKind::Lr2,
        "lr2oraja" => ScoreSourceKind::Lr2Oraja,
        "lr2orajadx" => ScoreSourceKind::Lr2OrajaDx,
        _ => ScoreSourceKind::Local,
    }
}

fn arrange_from_ir(value: Option<&str>) -> String {
    match value.map(normalize_token).as_deref() {
        Some("mirror") => "Mirror",
        Some("random") => "Random",
        Some("rrandom") => "RRandom",
        Some("srandom") => "SRandom",
        Some("spiral") => "Spiral",
        Some("hrandom") => "HRandom",
        Some("allscratch") => "AllScratch",
        Some("randomex") => "RandomEx",
        Some("srandomex") => "SRandomEx",
        Some("frandom") => "FRandom",
        Some("mfrandom") => "MFRandom",
        _ => "Normal",
    }
    .to_string()
}

fn device_type_from_ir(value: &str) -> InputDeviceKind {
    if value.eq_ignore_ascii_case("controller") {
        InputDeviceKind::Controller
    } else {
        InputDeviceKind::Keyboard
    }
}

fn normalize_token(value: &str) -> String {
    value.chars().filter(|ch| ch.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
#[path = "download/tests.rs"]
mod tests;
