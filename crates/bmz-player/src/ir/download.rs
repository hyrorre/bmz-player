use anyhow::{Context, Result, bail};
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::score::{JudgeCounts, ScoreState};
use rusqlite::{OptionalExtension, params};

use crate::ln_policy::LnScorePolicy;
use crate::select_options::DoubleOptionScoreBucket;
use crate::storage::common::hex_to_hash;
use crate::storage::network_db::NetworkDatabase;
use crate::storage::score_db::{
    ScoreDatabase, ScoreHistorySourceKey, ScoreHistorySourceRecord, ScoreRecord,
    ScoreSourceInsertOutcome,
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
    pub messages: Vec<String>,
}

pub async fn download_ir_scores(
    client: &BmzOfficialIrClient,
    provider_key: &str,
    account_id: &str,
    score_db: &mut ScoreDatabase,
    network_db: &NetworkDatabase,
    options: &IrScoreDownloadOptions,
    now: i64,
) -> Result<IrScoreDownloadReport> {
    let limit = options.limit.max(1);
    let mut report = IrScoreDownloadReport {
        provider_key: provider_key.to_string(),
        account_id: account_id.to_string(),
        ..IrScoreDownloadReport::default()
    };
    let mut cursor = None;

    while report.action_count() < limit {
        let page_limit = 100;
        let page = client
            .fetch_own_scores(&IrOwnScoreHistoryRequest {
                limit: page_limit,
                offset: 0,
                cursor: cursor.clone(),
            })
            .await?;
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
        network_db.local_score_id_for_remote_score(provider_key, account_id, &entry.score_id)?
        && score_history_exists(score_db, local_score_id)?
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

    let count_unprocessed_notes =
        score_record_uses_unprocessed_notes(&score, entry.notes, entry.min_bp, entry.min_cb)?;
    let rule_mode =
        if entry.rule_mode.trim().is_empty() { "Beatoraja" } else { entry.rule_mode.as_str() };

    Ok(ScoreRecord {
        chart_sha256: hex_to_hash::<32>(&entry.chart_sha256)?,
        ln_policy: LnScorePolicy::from_str_opt(&entry.ln_policy)
            .with_context(|| format!("unsupported LN policy: {}", entry.ln_policy))?,
        double_option: double_option_from_ir(&entry.double_option),
        played_at: entry.played_at.unwrap_or(entry.server_received_at),
        clear_type,
        gauge_type: gauge_type_from_ir(&entry.gauge).or_else(|| gauge_type_for_clear(clear_type)),
        gauge_value: gauge_value_for_clear(clear_type),
        total_notes: entry.notes,
        playtime_seconds: 0,
        score,
        count_unprocessed_notes,
        random_seed: entry.random_seed,
        arrange: arrange_from_ir(entry.arrange_1p.as_deref()),
        gauge_option: entry.gauge.clone(),
        rule_mode: rule_mode.to_string(),
        assist_mask: entry.assist_mask.unwrap_or(0),
        autoplay: false,
        device_type: device_type_from_ir(&entry.device_type),
        replay_path: String::new(),
    })
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

fn gauge_value_for_clear(clear_type: ClearType) -> f32 {
    match clear_type {
        ClearType::NoPlay | ClearType::Failed => 0.0,
        _ => 100.0,
    }
}

fn double_option_from_ir(value: &str) -> DoubleOptionScoreBucket {
    DoubleOptionScoreBucket::from_ir_query_or_off(Some(value))
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
mod tests {
    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{NETWORK_MIGRATIONS, SCORE_MIGRATIONS, run_migrations};
    use crate::storage::network_db::{IrJobKind, NewIrScoreJob, NewIrScoreSubmission};
    use rusqlite::Connection;

    fn open_score_db() -> ScoreDatabase {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        ScoreDatabase::from_connection(conn)
    }

    fn open_network_db() -> NetworkDatabase {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, NETWORK_MIGRATIONS).unwrap();
        NetworkDatabase::from_connection(conn)
    }

    fn score_entry(id: &str) -> IrOwnScoreHistoryEntry {
        IrOwnScoreHistoryEntry {
            score_id: id.to_string(),
            chart_sha256: "11".repeat(32),
            clear: "Hard".to_string(),
            ex_score: 21,
            max_combo: 12,
            min_bp: 1,
            min_cb: 1,
            bp: 1,
            cb: 1,
            gauge: "HARD".to_string(),
            ln_policy: "ForceLn".to_string(),
            double_option: "off".to_string(),
            rule_mode: "Beatoraja".to_string(),
            judges: IrJudgePayload {
                fast: super::super::types::IrJudgeSidePayload {
                    pgreat: 10,
                    great: 1,
                    good: 0,
                    bad: 0,
                    poor: 0,
                    empty_poor: 0,
                },
                slow: super::super::types::IrJudgeSidePayload {
                    pgreat: 0,
                    great: 0,
                    good: 0,
                    bad: 1,
                    poor: 0,
                    empty_poor: 0,
                },
            },
            notes: 12,
            pass_notes: 12,
            device_type: "controller".to_string(),
            arrange_1p: Some("random".to_string()),
            arrange_2p: None,
            random_seed: Some(123),
            assist_mask: Some(4),
            played_at: Some(1_700_000_000),
            server_received_at: 1_700_000_005,
            verification: "signed".to_string(),
            replay_hash: None,
        }
    }

    #[test]
    fn score_record_from_ir_preserves_judges_and_options() {
        let record = score_record_from_ir_entry(&score_entry("remote-1")).unwrap();

        assert_eq!(record.chart_sha256, [0x11; 32]);
        assert_eq!(record.clear_type, ClearType::Hard);
        assert_eq!(record.gauge_type, Some(GaugeType::Hard));
        assert_eq!(record.gauge_value, 100.0);
        assert_eq!(record.score.ex_score(), 21);
        assert_eq!(record.score.bp(), 1);
        assert_eq!(record.count_unprocessed_notes, false);
        assert_eq!(record.random_seed, Some(123));
        assert_eq!(record.arrange, "Random");
        assert_eq!(record.assist_mask, 4);
        assert_eq!(record.device_type, InputDeviceKind::Controller);
    }

    #[test]
    fn score_record_from_ir_reconstructs_unprocessed_failed_bp() {
        let mut entry = score_entry("remote-1");
        entry.clear = "Failed".to_string();
        entry.gauge = "HARD".to_string();
        entry.notes = 20;
        entry.pass_notes = 12;
        entry.min_bp = 9;
        entry.min_cb = 9;

        let record = score_record_from_ir_entry(&entry).unwrap();

        assert!(record.count_unprocessed_notes);
        assert_eq!(record.score.bp_with_unprocessed_notes(record.total_notes), 9);
        assert_eq!(record.score.cb_with_unprocessed_notes(record.total_notes), 9);
    }

    #[test]
    fn import_ir_score_entries_inserts_once_and_skips_source_duplicate() {
        let mut score_db = open_score_db();
        let network_db = open_network_db();
        let mut report = IrScoreDownloadReport::default();

        import_ir_score_entries(
            &mut score_db,
            &network_db,
            "provider-1",
            "account-1",
            &[score_entry("remote-1")],
            false,
            1_800_000_000,
            &mut report,
        )
        .unwrap();
        import_ir_score_entries(
            &mut score_db,
            &network_db,
            "provider-1",
            "account-1",
            &[score_entry("remote-1")],
            false,
            1_800_000_000,
            &mut report,
        )
        .unwrap();

        assert_eq!(report.imported, 1);
        assert_eq!(report.skipped_existing, 1);
        let count: i64 = score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn import_ir_score_entries_links_previously_uploaded_local_score() {
        let mut score_db = open_score_db();
        let mut network_db = open_network_db();
        let local_history_id = score_db
            .insert_score(&score_record_from_ir_entry(&score_entry("local")).unwrap())
            .unwrap();
        let job_id = network_db
            .enqueue_ir_score_job(&NewIrScoreJob {
                provider: "provider-1".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: local_history_id,
                chart_sha256: [0x11; 32],
                ln_policy: LnScorePolicy::ForceLn,
                payload_json: "{}".to_string(),
                now: 1,
            })
            .unwrap();
        network_db
            .insert_ir_score_submission(&NewIrScoreSubmission {
                job_id,
                provider: "provider-1".to_string(),
                account_id: "account-1".to_string(),
                kind: IrJobKind::Score,
                local_score_id: local_history_id,
                remote_score_id: "remote-1".to_string(),
                status: "succeeded".to_string(),
                submitted_at: 2,
                log_path: String::new(),
                error: String::new(),
            })
            .unwrap();
        let mut report = IrScoreDownloadReport::default();

        import_ir_score_entries(
            &mut score_db,
            &network_db,
            "provider-1",
            "account-1",
            &[score_entry("remote-1")],
            false,
            1_800_000_000,
            &mut report,
        )
        .unwrap();

        assert_eq!(report.imported, 0);
        assert_eq!(report.linked_existing, 1);
        let count: i64 = score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn import_limit_skips_known_scores_before_importing_next_candidate() {
        let mut score_db = open_score_db();
        let network_db = open_network_db();
        let entries = [
            score_entry("remote-1"),
            score_entry("remote-2"),
            score_entry("remote-3"),
            score_entry("remote-4"),
        ];
        let mut initial_report = IrScoreDownloadReport::default();
        import_ir_score_entries(
            &mut score_db,
            &network_db,
            "provider-1",
            "account-1",
            &entries[..2],
            false,
            1_800_000_000,
            &mut initial_report,
        )
        .unwrap();
        let mut next_report = IrScoreDownloadReport::default();

        let consumed = import_ir_score_entries_up_to(
            &mut score_db,
            &network_db,
            "provider-1",
            "account-1",
            &entries,
            false,
            1_800_000_001,
            &mut next_report,
            Some(1),
        )
        .unwrap();

        assert_eq!(consumed, 3);
        assert_eq!(next_report.scanned, 3);
        assert_eq!(next_report.skipped_existing, 2);
        assert_eq!(next_report.imported, 1);
        let count: i64 = score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn dry_run_reports_invalid_scores_without_counting_them_as_candidates() {
        let mut score_db = open_score_db();
        let network_db = open_network_db();
        let mut invalid = score_entry("remote-invalid");
        invalid.ex_score += 1;
        let mut report = IrScoreDownloadReport::default();

        import_ir_score_entries(
            &mut score_db,
            &network_db,
            "provider-1",
            "account-1",
            &[invalid],
            true,
            1_800_000_000,
            &mut report,
        )
        .unwrap();

        assert_eq!(report.candidates, 0);
        assert_eq!(report.failed, 1);
        assert_eq!(report.messages.len(), 1);
        let count: i64 = score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
