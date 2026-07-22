use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use bmz_chart::import::import_bms_chart;
use bmz_chart::model::PlayableChart;
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::course::{
    CourseClassConstraint, CourseGaugeConstraint, CourseJudgeConstraint, CourseSpeedConstraint,
};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::rule::RuleMode;
use bmz_gameplay::score::{JudgeCounts, ScoreState};
use rusqlite::{Connection, OpenFlags, Row};

use super::common::hex_to_hash;
use super::library_db::LibraryDatabase;
use super::score_db::{
    CourseScoreInsert, ImportedScoreReconciliation, ScoreDatabase, ScoreRecord, ScoreSourceKind,
    decode_beatoraja_ghost,
};
use crate::ln_policy::{
    LnPolicySetting, LnScorePolicy, expected_scored_note_count_for_policy, score_ln_policy,
};
use crate::select_options::{ArrangeOption, DoubleOption};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScoreImportKind {
    #[default]
    Lr2,
    Beatoraja,
    Lr2Oraja,
    Lr2OrajaDx,
}

impl ScoreImportKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lr2 => "LR2",
            Self::Beatoraja => "beatoraja",
            Self::Lr2Oraja => "LR2oraja",
            Self::Lr2OrajaDx => "LR2oraja (DX Mode)",
        }
    }

    const fn rule_mode_enum(self) -> RuleMode {
        match self {
            Self::Beatoraja => RuleMode::Beatoraja,
            Self::Lr2 | Self::Lr2Oraja => RuleMode::Lr2Oraja,
            Self::Lr2OrajaDx => RuleMode::Dx,
        }
    }

    const fn rule_mode(self) -> &'static str {
        match self {
            Self::Beatoraja => "Beatoraja",
            Self::Lr2 | Self::Lr2Oraja => "Lr2Oraja",
            Self::Lr2OrajaDx => "Dx",
        }
    }

    const fn uses_lr2_schema(self) -> bool {
        matches!(self, Self::Lr2)
    }

    const fn source_kind(self) -> ScoreSourceKind {
        match self {
            Self::Lr2 => ScoreSourceKind::Lr2,
            Self::Beatoraja => ScoreSourceKind::Beatoraja,
            Self::Lr2Oraja => ScoreSourceKind::Lr2Oraja,
            Self::Lr2OrajaDx => ScoreSourceKind::Lr2OrajaDx,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScoreImportRequest {
    pub path: PathBuf,
    pub kind: ScoreImportKind,
    /// 外部score DBには入力デバイスの記録がないため、ユーザーが指定する。
    pub device_type: InputDeviceKind,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreImportReport {
    pub scanned: u32,
    pub matched: u32,
    pub imported: u32,
    pub corrected: u32,
    pub skipped: u32,
    pub failed: u32,
}

impl ScoreImportReport {
    pub fn summary(&self) -> String {
        format!(
            "scanned {}, matched {}, imported {}, corrected {}, skipped {}, failed {}",
            self.scanned, self.matched, self.imported, self.corrected, self.skipped, self.failed
        )
    }
}

pub fn import_scores(
    request: &ScoreImportRequest,
    library_db: &mut LibraryDatabase,
    score_db: &mut ScoreDatabase,
    imported_at: i64,
) -> Result<ScoreImportReport> {
    if !request.path.is_file() {
        bail!("score database file does not exist: {}", request.path.display());
    }

    let source = Connection::open_with_flags(
        &request.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open score database: {}", request.path.display()))?;

    if request.kind.uses_lr2_schema() {
        import_lr2_scores_with_device_type(
            &source,
            request.kind,
            library_db,
            score_db,
            imported_at,
            request.device_type,
        )
    } else {
        import_beatoraja_scores_with_device_type(
            &source,
            request.kind,
            library_db,
            score_db,
            imported_at,
            request.device_type,
        )
    }
}

#[cfg(test)]
fn import_lr2_scores(
    source: &Connection,
    kind: ScoreImportKind,
    library_db: &mut LibraryDatabase,
    score_db: &mut ScoreDatabase,
    imported_at: i64,
) -> Result<ScoreImportReport> {
    import_lr2_scores_with_device_type(
        source,
        kind,
        library_db,
        score_db,
        imported_at,
        InputDeviceKind::Keyboard,
    )
}

fn import_lr2_scores_with_device_type(
    source: &Connection,
    kind: ScoreImportKind,
    library_db: &mut LibraryDatabase,
    score_db: &mut ScoreDatabase,
    imported_at: i64,
    device_type: InputDeviceKind,
) -> Result<ScoreImportReport> {
    ensure_table(source, "score")?;
    // Owned index of canonical LR2-dan courses (md5 stage sequence -> score-db
    // course identity snapshot), built once before the row loop so the immutable
    // borrow of `library_db` is released before we start inserting course scores.
    let course_index = build_lr2_course_index(library_db)?;
    let mut report = ScoreImportReport::default();
    let mut chart_cache: HashMap<[u8; 32], Arc<PlayableChart>> = HashMap::new();
    let mut stmt = source.prepare(
        "SELECT hash, clear, perfect, great, good, bad, poor,
                totalnotes, maxcombo, minbp, playcount, clearcount, ghost, rseed, op_best
         FROM score",
    )?;
    let rows = stmt.query_map([], lr2_row)?;
    for row in rows {
        report.scanned += 1;
        let row = match row {
            Ok(row) => row,
            Err(error) => {
                report.failed += 1;
                tracing::warn!(%error, "failed to read LR2 score row");
                continue;
            }
        };
        // `op_best` describes the arrangement that produced LR2's best EX
        // score.  LR2's SCATTER / CONVERGE layouts cannot be represented by
        // BMZ, so reject the whole aggregate row rather than recording a
        // misleading option.  This happens before course handling as course
        // rows use the same source table and field.
        let options = match lr2_import_options(row.op_best) {
            Ok(options) => options,
            Err(error) => {
                report.skipped += 1;
                tracing::warn!(
                    hash = %row.md5,
                    op_best = row.op_best,
                    ?error,
                    "skipped LR2 score with unsupported best-play option"
                );
                continue;
            }
        };
        // LR2 stores course (dan) results in the same `score` table, keyed by a
        // 32-char marker segment followed by the constituent chart md5s (e.g. a
        // 160-char key for a 4-song course).  Resolve these to bmz courses and
        // import a course score for each canonical match (see import_lr2_course).
        if is_course_hash(&row.md5, 32) {
            import_lr2_course(
                &row,
                &course_index,
                score_db,
                kind.rule_mode_enum(),
                imported_at,
                &mut report,
            )?;
            continue;
        }
        let md5 = match hex_to_hash::<16>(&row.md5) {
            Ok(md5) => md5,
            Err(error) => {
                report.failed += 1;
                tracing::warn!(md5 = %row.md5, %error, "invalid LR2 score md5");
                continue;
            }
        };
        let Some(chart_sha256) = library_db.chart_sha256_by_md5(md5)? else {
            report.skipped += 1;
            continue;
        };
        report.matched += 1;

        let ex_score = lr2_ex_score(&row);
        if !score_summary_is_sane(row.total_notes, row.max_combo, ex_score) {
            report.failed += 1;
            tracing::warn!(
                md5 = %row.md5,
                source_notes = row.total_notes,
                max_combo = row.max_combo,
                ex_score,
                "LR2 score summary exceeds source note count"
            );
            continue;
        }
        let resolved = match resolve_import_ln_policy(
            library_db,
            chart_sha256,
            LnScorePolicy::ForceLn,
            row.total_notes,
            &mut chart_cache,
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                report.failed += 1;
                tracing::warn!(
                    md5 = %row.md5,
                    source_notes = row.total_notes,
                    "LR2 score source note count does not match expected note count"
                );
                continue;
            }
            Err(error) => {
                report.failed += 1;
                tracing::warn!(md5 = %row.md5, %error, "failed to resolve LR2 import chart");
                continue;
            }
        };

        let clear_type = lr2_clear_type(row.clear);
        let mut record = imported_score_record(
            chart_sha256,
            imported_at,
            clear_type,
            resolved.expected_notes,
            score_state_from_lr2(&row, resolved.expected_notes),
            row.random_seed,
            kind.rule_mode(),
            resolved.ln_policy,
        );
        record.source_kind = kind.source_kind();
        if record.source_kind == ScoreSourceKind::Beatoraja {
            record.seed_scheme = crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1.to_string();
        }
        record.arrange = options.arrange.to_persistent_str().to_string();
        record.arrange_2p = options.arrange_2p.to_persistent_str().to_string();
        record.applied_double_option = options.applied_double_option;
        record.double_option = options.applied_double_option.score_bucket();
        record.device_type = device_type;
        match score_db.reconcile_imported_score_device_type(&record)? {
            ImportedScoreReconciliation::Missing => {}
            ImportedScoreReconciliation::Unchanged => {
                report.skipped += 1;
                continue;
            }
            ImportedScoreReconciliation::Corrected => {
                report.corrected += 1;
                continue;
            }
        }
        score_db.insert_score(&record)?;
        report.imported += 1;
    }
    Ok(report)
}

#[cfg(test)]
fn import_beatoraja_scores(
    source: &Connection,
    kind: ScoreImportKind,
    library_db: &LibraryDatabase,
    score_db: &mut ScoreDatabase,
    imported_at: i64,
) -> Result<ScoreImportReport> {
    import_beatoraja_scores_with_device_type(
        source,
        kind,
        library_db,
        score_db,
        imported_at,
        InputDeviceKind::Keyboard,
    )
}

fn import_beatoraja_scores_with_device_type(
    source: &Connection,
    kind: ScoreImportKind,
    library_db: &LibraryDatabase,
    score_db: &mut ScoreDatabase,
    imported_at: i64,
    device_type: InputDeviceKind,
) -> Result<ScoreImportReport> {
    let table = if table_exists(source, "score")? {
        "score"
    } else if table_exists(source, "scoredatalog")? {
        "scoredatalog"
    } else {
        bail!("beatoraja score database must contain score or scoredatalog table");
    };

    let mut report = ScoreImportReport::default();
    let mut chart_cache: HashMap<[u8; 32], Arc<PlayableChart>> = HashMap::new();
    let sql = format!(
        "SELECT sha256, mode, clear, epg, lpg, egr, lgr, egd, lgd, ebd, lbd,
                epr, lpr, ems, lms, notes, combo, minbp, ghost, seed, date, option
         FROM {table}"
    );
    let mut stmt = source.prepare(&sql)?;
    let rows = stmt.query_map([], beatoraja_row)?;
    for row in rows {
        report.scanned += 1;
        let row = match row {
            Ok(row) => row,
            Err(error) => {
                report.failed += 1;
                tracing::warn!(%error, "failed to read beatoraja score row");
                continue;
            }
        };
        // beatoraja stores course (dan) results in the same `score` table, keyed
        // by the concatenation of every constituent chart sha256.  A single chart
        // hash is 64 hex chars, so a course key is a multiple of 64 longer than 64
        // (e.g. 256 for a 4-song course).  These are not importable as single-chart
        // scores: bmz models course results in dedicated tables, and the concatenated
        // key cannot be unambiguously mapped back to a bmz course (table-defined
        // courses sharing a song set differ only by constraint, which the key omits).
        // Treat them as skipped rather than failed, and keep the log quiet.
        if is_course_hash(&row.sha256, 64) {
            report.skipped += 1;
            tracing::debug!(len = row.sha256.len(), "skipped beatoraja course score");
            continue;
        }
        let chart_sha256 = match hex_to_hash::<32>(&row.sha256) {
            Ok(sha256) => sha256,
            Err(error) => {
                report.failed += 1;
                tracing::warn!(sha256 = %row.sha256, %error, "invalid beatoraja score sha256");
                continue;
            }
        };
        if library_db.chart_id_by_sha256(chart_sha256)?.is_none() {
            report.skipped += 1;
            continue;
        }
        report.matched += 1;

        let setting = beatoraja_mode_to_ln_setting(row.mode);
        let charts = library_db.list_charts_by_sha256(chart_sha256)?;
        let Some(chart_item) = charts.first() else {
            report.skipped += 1;
            continue;
        };
        let ln_policy = score_ln_policy(setting, chart_item.ln_profile);
        let ex_score = beatoraja_ex_score(&row);
        if !score_summary_is_sane(row.total_notes, row.max_combo, ex_score) {
            report.failed += 1;
            tracing::warn!(
                sha256 = %row.sha256,
                source_notes = row.total_notes,
                max_combo = row.max_combo,
                ex_score,
                "beatoraja score summary exceeds source note count"
            );
            continue;
        }
        let resolved = match resolve_import_ln_policy(
            library_db,
            chart_sha256,
            ln_policy,
            row.total_notes,
            &mut chart_cache,
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                report.failed += 1;
                tracing::warn!(
                    sha256 = %row.sha256,
                    mode = row.mode,
                    source_notes = row.total_notes,
                    policy = ln_policy.as_str(),
                    "beatoraja score source note count does not match expected note count"
                );
                continue;
            }
            Err(error) => {
                report.failed += 1;
                tracing::warn!(
                    sha256 = %row.sha256,
                    %error,
                    "failed to resolve beatoraja import chart"
                );
                continue;
            }
        };

        let clear_type = beatoraja_clear_type(row.clear);
        let (arrange, arrange_2p) = beatoraja_arrange_options(row.option, &chart_item.mode);
        let mut record = imported_score_record(
            chart_sha256,
            normalize_imported_played_at(row.date).unwrap_or(imported_at),
            clear_type,
            resolved.expected_notes,
            score_state_from_beatoraja(&row, resolved.expected_notes),
            row.random_seed,
            kind.rule_mode(),
            resolved.ln_policy,
        );
        record.arrange = arrange.to_persistent_str().to_string();
        record.arrange_2p = arrange_2p.to_persistent_str().to_string();
        record.applied_double_option = beatoraja_double_option(row.option);
        record.double_option = record.applied_double_option.score_bucket();
        record.source_kind = kind.source_kind();
        if record.source_kind == ScoreSourceKind::Beatoraja {
            record.seed_scheme = crate::storage::replay::SEED_SCHEME_BEATORAJA_24BIT_V1.to_string();
        }
        record.device_type = device_type;
        match score_db.reconcile_imported_score_device_type(&record)? {
            ImportedScoreReconciliation::Missing => {}
            ImportedScoreReconciliation::Unchanged => {
                report.skipped += 1;
                continue;
            }
            ImportedScoreReconciliation::Corrected => {
                report.corrected += 1;
                continue;
            }
        }
        score_db.insert_score(&record)?;
        report.imported += 1;
    }
    Ok(report)
}

#[derive(Debug, Clone, Copy)]
struct ResolvedImportLnPolicy {
    ln_policy: LnScorePolicy,
    expected_notes: u32,
}

fn beatoraja_mode_to_ln_setting(mode: i64) -> LnPolicySetting {
    match mode {
        0 => LnPolicySetting::AutoLn,
        1 => LnPolicySetting::AutoCn,
        2 => LnPolicySetting::AutoHcn,
        other => {
            tracing::debug!(mode = other, "unknown beatoraja score.mode; treating as AutoLn");
            LnPolicySetting::AutoLn
        }
    }
}

/// Maps beatoraja's decimal-packed score option to the two arrangement slots
/// which BMZ records for an attempt.  The low digit is 1P, the tens digit is
/// 2P; the hundreds digit is handled separately by
/// [`beatoraja_double_option`].
fn beatoraja_arrange_options(option: i64, chart_mode: &str) -> (ArrangeOption, ArrangeOption) {
    if option < 0 {
        tracing::debug!(option, "negative beatoraja score.option; using Normal arrange");
        return (ArrangeOption::Normal, ArrangeOption::Normal);
    }
    (
        beatoraja_arrange_option(option % 10, chart_mode),
        beatoraja_arrange_option((option / 10) % 10, chart_mode),
    )
}

fn beatoraja_arrange_option(random_option: i64, chart_mode: &str) -> ArrangeOption {
    let general = match random_option {
        0 => ArrangeOption::Normal,
        1 => ArrangeOption::Mirror,
        2 => ArrangeOption::Random,
        3 => ArrangeOption::RRandom,
        4 => ArrangeOption::SRandom,
        5 => ArrangeOption::Spiral,
        6 => ArrangeOption::HRandom,
        7 => ArrangeOption::AllScratch,
        8 => ArrangeOption::RandomEx,
        9 => ArrangeOption::SRandomEx,
        _ => {
            tracing::debug!(random_option, "unknown beatoraja random option; using Normal");
            ArrangeOption::Normal
        }
    };

    // beatoraja has a distinct POP'N option table.  BMZ does not implement
    // CONVERGE or the playable-only variants, so retain an equivalent normal/
    // random class without claiming an unsupported arrangement was reproduced.
    if chart_mode != "9K" {
        return general;
    }
    match random_option {
        7 => {
            tracing::debug!("beatoraja PMS CONVERGE has no BMZ equivalent; using Normal");
            ArrangeOption::Normal
        }
        8 => {
            tracing::debug!("approximating beatoraja PMS RANDOM PLAYABLE as Random");
            ArrangeOption::Random
        }
        9 => {
            tracing::debug!("approximating beatoraja PMS S-RANDOM PLAYABLE as SRandom");
            ArrangeOption::SRandom
        }
        _ => general,
    }
}

/// Reads beatoraja's hundreds digit as the actual DP option selected for the
/// attempt.  This is intentionally separate from
/// [`DoubleOptionScoreBucket`](crate::select_options::DoubleOptionScoreBucket):
/// BMZ groups OFF and FLIP scores together, but history must retain which of
/// those two layouts the player used.
fn beatoraja_double_option(option: i64) -> DoubleOption {
    if option < 0 {
        return DoubleOption::Off;
    }
    match option / 100 {
        0 => DoubleOption::Off,
        1 => DoubleOption::Flip,
        2 => DoubleOption::Battle,
        3 => DoubleOption::BattleAutoScratch,
        double_option => {
            tracing::debug!(double_option, "unknown beatoraja double option; using Off bucket");
            DoubleOption::Off
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lr2ImportOptions {
    arrange: ArrangeOption,
    arrange_2p: ArrangeOption,
    applied_double_option: DoubleOption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lr2ImportOptionError {
    Negative,
    Scatter { player: u8 },
    Converge { player: u8 },
    UnknownArrange { player: u8, option: i64 },
    UnknownDouble { option: i64 },
}

/// Decodes LR2's decimal-packed `score.op_best` setting.
///
/// The units digit is the gauge and intentionally ignored: LR2 keeps aggregate
/// best values, so it need not identify the play that supplied every stored
/// field.  Tens/hundreds are the 1P/2P arrangements and the thousands digit
/// records DP FLIP.
fn lr2_import_options(op_best: i64) -> Result<Lr2ImportOptions, Lr2ImportOptionError> {
    if op_best < 0 {
        return Err(Lr2ImportOptionError::Negative);
    }
    let arrange = lr2_arrange_option((op_best / 10) % 10, 1)?;
    let arrange_2p = lr2_arrange_option((op_best / 100) % 10, 2)?;
    let applied_double_option = match (op_best / 1000) % 10 {
        0 => DoubleOption::Off,
        1 => DoubleOption::Flip,
        option => return Err(Lr2ImportOptionError::UnknownDouble { option }),
    };
    Ok(Lr2ImportOptions { arrange, arrange_2p, applied_double_option })
}

fn lr2_arrange_option(option: i64, player: u8) -> Result<ArrangeOption, Lr2ImportOptionError> {
    match option {
        0 => Ok(ArrangeOption::Normal),
        1 => Ok(ArrangeOption::Mirror),
        2 => Ok(ArrangeOption::Random),
        3 => Ok(ArrangeOption::SRandom),
        4 => Err(Lr2ImportOptionError::Scatter { player }),
        5 => Err(Lr2ImportOptionError::Converge { player }),
        option => Err(Lr2ImportOptionError::UnknownArrange { player, option }),
    }
}

fn lr2_ex_score(row: &Lr2ScoreRow) -> u64 {
    u64::from(row.perfect) * 2 + u64::from(row.great)
}

fn beatoraja_ex_score(row: &BeatorajaScoreRow) -> u64 {
    (u64::from(row.epg) + u64::from(row.lpg)) * 2 + u64::from(row.egr) + u64::from(row.lgr)
}

fn score_summary_is_sane(total_notes: u32, max_combo: u32, ex_score: u64) -> bool {
    total_notes > 0
        && max_combo <= total_notes
        && ex_score <= u64::from(total_notes).saturating_mul(2)
}

fn resolve_import_ln_policy(
    library_db: &LibraryDatabase,
    chart_sha256: [u8; 32],
    initial_policy: LnScorePolicy,
    source_notes: u32,
    chart_cache: &mut HashMap<[u8; 32], Arc<PlayableChart>>,
) -> Result<Option<ResolvedImportLnPolicy>> {
    let expected =
        expected_notes_for_policy(library_db, chart_sha256, initial_policy, chart_cache)?;
    if source_notes == expected {
        return Ok(Some(ResolvedImportLnPolicy {
            ln_policy: initial_policy,
            expected_notes: expected,
        }));
    }
    if initial_policy != LnScorePolicy::ForceLn {
        let force_expected = expected_notes_for_policy(
            library_db,
            chart_sha256,
            LnScorePolicy::ForceLn,
            chart_cache,
        )?;
        if source_notes == force_expected {
            return Ok(Some(ResolvedImportLnPolicy {
                ln_policy: LnScorePolicy::ForceLn,
                expected_notes: force_expected,
            }));
        }
    }
    Ok(None)
}

fn expected_notes_for_policy(
    library_db: &LibraryDatabase,
    chart_sha256: [u8; 32],
    policy: LnScorePolicy,
    chart_cache: &mut HashMap<[u8; 32], Arc<PlayableChart>>,
) -> Result<u32> {
    let charts = library_db.list_charts_by_sha256(chart_sha256)?;
    let Some(item) = charts.first() else {
        bail!("chart missing from library while resolving import note count");
    };
    // No long notes: every policy collapses to ForceLn / base total_notes.
    if !item.ln_profile.has_any_ln() {
        return Ok(item.total_notes);
    }
    // ForceLn never scores long ends separately, so base total_notes is enough.
    if policy == LnScorePolicy::ForceLn {
        return Ok(item.total_notes);
    }
    let chart = load_import_chart(library_db, chart_sha256, item.chart_id, chart_cache)?;
    Ok(expected_scored_note_count_for_policy(&chart, policy))
}

fn load_import_chart(
    library_db: &LibraryDatabase,
    chart_sha256: [u8; 32],
    chart_id: i64,
    chart_cache: &mut HashMap<[u8; 32], Arc<PlayableChart>>,
) -> Result<Arc<PlayableChart>> {
    if let Some(chart) = chart_cache.get(&chart_sha256) {
        return Ok(Arc::clone(chart));
    }
    #[cfg(test)]
    if let Some(chart) = take_test_import_chart(chart_sha256) {
        let chart = Arc::new(chart);
        chart_cache.insert(chart_sha256, Arc::clone(&chart));
        return Ok(chart);
    }
    let Some(path) = library_db.primary_chart_file_path(chart_id)? else {
        bail!("chart file path missing for chart id {chart_id}");
    };
    let imported = import_bms_chart(Path::new(&path), None, false)
        .with_context(|| format!("failed to import chart for score note-count check: {path}"))?;
    let chart = Arc::new(imported.chart);
    chart_cache.insert(chart_sha256, Arc::clone(&chart));
    Ok(chart)
}

#[cfg(test)]
thread_local! {
    static TEST_IMPORT_CHARTS: std::cell::RefCell<HashMap<[u8; 32], PlayableChart>> =
        std::cell::RefCell::new(HashMap::new());
}

#[cfg(test)]
fn set_test_import_chart(sha256: [u8; 32], chart: PlayableChart) {
    TEST_IMPORT_CHARTS.with(|maps| {
        maps.borrow_mut().insert(sha256, chart);
    });
}

#[cfg(test)]
fn take_test_import_chart(sha256: [u8; 32]) -> Option<PlayableChart> {
    TEST_IMPORT_CHARTS.with(|maps| maps.borrow().get(&sha256).cloned())
}

#[cfg(test)]
fn clear_test_import_charts() {
    TEST_IMPORT_CHARTS.with(|maps| maps.borrow_mut().clear());
}

fn imported_score_record(
    chart_sha256: [u8; 32],
    played_at: i64,
    clear_type: ClearType,
    total_notes: u32,
    score: ScoreState,
    random_seed: Option<i64>,
    rule_mode: &str,
    ln_policy: LnScorePolicy,
) -> ScoreRecord {
    ScoreRecord {
        chart_sha256,
        ln_policy,
        double_option: crate::select_options::DoubleOptionScoreBucket::Off,
        applied_double_option: DoubleOption::Off,
        played_at,
        clear_type,
        gauge_type: gauge_type_for_clear(clear_type),
        gauge_value: gauge_value_for_clear(clear_type),
        total_notes,
        playtime_seconds: 0,
        score,
        count_unprocessed_notes: clear_type == ClearType::Failed,
        random_seed,
        seed_scheme: crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3.to_string(),
        arrange: "Normal".to_string(),
        arrange_2p: "Normal".to_string(),
        gauge_option: String::new(),
        rule_mode: rule_mode.to_string(),
        assist_mask: 0,
        autoplay: false,
        device_type: InputDeviceKind::Keyboard,
        replay_path: String::new(),
        source_kind: ScoreSourceKind::Local,
    }
}

/// Imports an LR2 course (dan) result into every canonical bmz course it matches.
///
/// LR2 course keys cannot be mapped to a single bmz course unambiguously, but for
/// dan认定 the play options are canonical: normal+mirror class, free HS, no judge
/// constraint, LR2 gauge.  After filtering candidates to that set, the only
/// remaining ambiguity is the LN constraint, and we deliberately import into every
/// matching LN variant (a course whose charts contain no LN scores identically with
/// or without the constraint, and LR2 dan is always LN-on).  Per-chart breakdown is
/// not available from LR2's aggregate course row, so `charts`/`replays` are empty.
fn import_lr2_course(
    row: &Lr2ScoreRow,
    course_index: &HashMap<Vec<[u8; 16]>, Vec<CourseImportTarget>>,
    score_db: &mut ScoreDatabase,
    rule_mode: RuleMode,
    imported_at: i64,
    report: &mut ScoreImportReport,
) -> Result<()> {
    let Some(stages) = lr2_course_stage_md5s(&row.md5) else {
        report.skipped += 1;
        tracing::debug!(len = row.md5.len(), "LR2 course key not splittable into stage md5s");
        return Ok(());
    };
    let Some(targets) = course_index.get(&stages) else {
        report.skipped += 1;
        tracing::debug!(stages = stages.len(), "LR2 course has no matching bmz course");
        return Ok(());
    };
    for target in targets {
        let insert = lr2_course_score_insert(row, target, rule_mode, imported_at);
        score_db.insert_course_score(&insert)?;
        report.imported += 1;
    }
    report.matched += 1;
    Ok(())
}

/// Splits an LR2 course key into its constituent chart md5s, dropping the leading
/// 32-char marker segment.  Returns `None` if the remainder is not a whole number
/// of 32-char md5s or any md5 is not valid hex.
fn lr2_course_stage_md5s(hash: &str) -> Option<Vec<[u8; 16]>> {
    if hash.len() <= 32 || !(hash.len() - 32).is_multiple_of(32) {
        return None;
    }
    let mut stages = Vec::with_capacity((hash.len() - 32) / 32);
    let mut start = 32;
    while start < hash.len() {
        stages.push(hex_to_hash::<16>(&hash[start..start + 32]).ok()?);
        start += 32;
    }
    Some(stages)
}

#[derive(Debug, Clone)]
struct CourseImportTarget {
    course_hash: String,
    source: String,
    course_key: String,
    title: String,
    kind: String,
    constraints_json: String,
    chart_sha256s_json: String,
}

/// Builds a course score from an LR2 aggregate course row and the matched bmz
/// course identity snapshot. Per-chart breakdown is not available from LR2.
fn lr2_course_score_insert(
    row: &Lr2ScoreRow,
    target: &CourseImportTarget,
    rule_mode: RuleMode,
    imported_at: i64,
) -> CourseScoreInsert {
    let clear_type = lr2_clear_type(row.clear);
    let course_failed = matches!(clear_type, ClearType::NoPlay | ClearType::Failed);
    CourseScoreInsert {
        course_hash: target.course_hash.clone(),
        rule_mode,
        source: target.source.clone(),
        course_key: target.course_key.clone(),
        title: target.title.clone(),
        kind: target.kind.clone(),
        constraints_json: target.constraints_json.clone(),
        chart_sha256s_json: target.chart_sha256s_json.clone(),
        ex_score: row.perfect * 2 + row.great,
        max_ex_score: row.total_notes * 2,
        clear_type: clear_type.as_str().to_string(),
        gauge_type: GaugeType::Normal.as_str().to_string(),
        gauge_value: gauge_value_for_clear(clear_type),
        max_combo: row.max_combo,
        bp: row.min_bp,
        course_failed,
        course_clear: !course_failed,
        arrange: "Normal".to_string(),
        trophies_json: "[]".to_string(),
        played_at: imported_at,
        charts: Vec::new(),
        replays: Vec::new(),
        achieved_trophies: Vec::new(),
    }
}

/// Builds an index of canonical LR2-dan courses, keyed by their ordered stage md5
/// sequence.  Courses are kept only if their constraints match the canonical LR2
/// dan profile (normal+mirror class, free HS, normal judge, LR2 gauge); the LN
/// dimension is intentionally not filtered (see [`import_lr2_course`]).  Courses
/// with any entry lacking an md5 are skipped (they cannot be matched by md5).
fn build_lr2_course_index(
    library_db: &LibraryDatabase,
) -> Result<HashMap<Vec<[u8; 16]>, Vec<CourseImportTarget>>> {
    let mut index: HashMap<Vec<[u8; 16]>, Vec<CourseImportTarget>> = HashMap::new();
    for course in library_db.list_courses()? {
        let constraints = &course.definition.constraints;
        if constraints.class != CourseClassConstraint::GradeMirrorAllowed
            || constraints.speed != CourseSpeedConstraint::Free
            || constraints.judge != CourseJudgeConstraint::Normal
            || constraints.gauge != CourseGaugeConstraint::Lr2
        {
            continue;
        }
        let mut key = Vec::with_capacity(course.definition.entries.len());
        let mut complete = true;
        for entry in &course.definition.entries {
            match entry.md5.as_deref().and_then(|md5| hex_to_hash::<16>(md5).ok()) {
                Some(md5) => key.push(md5),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if complete && !key.is_empty() {
            let Some(identity) =
                crate::ir::course_payload::course_identity_from_stored(library_db, &course)
            else {
                continue;
            };
            index.entry(key).or_default().push(CourseImportTarget {
                course_hash: identity.course_hash,
                source: course.source.clone(),
                course_key: course.definition.key.clone(),
                title: course.definition.title.clone(),
                kind: identity.definition.kind,
                constraints_json: identity.constraints_json,
                chart_sha256s_json: identity.chart_sha256s_json,
            });
        }
    }
    Ok(index)
}

/// Returns true when `hash` is a course key rather than a single-chart hash.
///
/// Both LR2 and beatoraja store course (dan) results in the same `score` table,
/// keyed by a concatenation of the constituent chart hashes (plus, for LR2, a
/// leading marker segment).  A single chart hash has a fixed width
/// (`single_len`: 32 for LR2 md5, 64 for beatoraja sha256), so a course key is a
/// non-zero multiple of that width longer than a single hash.  These cannot be
/// imported as single-chart scores, so callers skip them rather than fail.
fn is_course_hash(hash: &str, single_len: usize) -> bool {
    let len = hash.len();
    len > single_len && len.is_multiple_of(single_len)
}

fn ensure_table(conn: &Connection, table: &str) -> Result<()> {
    if table_exists(conn, table)? {
        Ok(())
    } else {
        bail!("score database must contain {table} table")
    }
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [table],
            |_| Ok(()),
        )
        .is_ok())
}

#[derive(Debug)]
struct Lr2ScoreRow {
    md5: String,
    clear: i64,
    perfect: u32,
    great: u32,
    good: u32,
    bad: u32,
    poor: u32,
    total_notes: u32,
    max_combo: u32,
    min_bp: u32,
    play_count: u32,
    clear_count: u32,
    ghost: String,
    random_seed: Option<i64>,
    op_best: i64,
}

fn lr2_row(row: &Row<'_>) -> rusqlite::Result<Lr2ScoreRow> {
    Ok(Lr2ScoreRow {
        md5: row.get(0)?,
        clear: row.get(1)?,
        perfect: row.get(2)?,
        great: row.get(3)?,
        good: row.get(4)?,
        bad: row.get(5)?,
        poor: row.get(6)?,
        total_notes: row.get(7)?,
        max_combo: row.get(8)?,
        min_bp: row.get(9)?,
        play_count: row.get(10)?,
        clear_count: row.get(11)?,
        ghost: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
        random_seed: row.get(13)?,
        op_best: row.get(14)?,
    })
}

#[derive(Debug)]
struct BeatorajaScoreRow {
    sha256: String,
    mode: i64,
    clear: i64,
    epg: u32,
    lpg: u32,
    egr: u32,
    lgr: u32,
    egd: u32,
    lgd: u32,
    ebd: u32,
    lbd: u32,
    epr: u32,
    lpr: u32,
    ems: u32,
    lms: u32,
    total_notes: u32,
    max_combo: u32,
    min_bp: u32,
    ghost: String,
    random_seed: Option<i64>,
    date: i64,
    option: i64,
}

fn beatoraja_row(row: &Row<'_>) -> rusqlite::Result<BeatorajaScoreRow> {
    Ok(BeatorajaScoreRow {
        sha256: row.get(0)?,
        mode: row.get(1)?,
        clear: row.get(2)?,
        epg: row.get(3)?,
        lpg: row.get(4)?,
        egr: row.get(5)?,
        lgr: row.get(6)?,
        egd: row.get(7)?,
        lgd: row.get(8)?,
        ebd: row.get(9)?,
        lbd: row.get(10)?,
        epr: row.get(11)?,
        lpr: row.get(12)?,
        ems: row.get(13)?,
        lms: row.get(14)?,
        total_notes: row.get(15)?,
        max_combo: row.get(16)?,
        min_bp: row.get(17)?,
        ghost: row.get::<_, Option<String>>(18)?.unwrap_or_default(),
        random_seed: row.get(19)?,
        date: row.get(20)?,
        option: row.get(21)?,
    })
}

fn score_state_from_lr2(row: &Lr2ScoreRow, expected_notes: u32) -> ScoreState {
    let ghost = decode_lr2_ghost(&row.ghost, expected_notes);
    let _ = (row.min_bp, row.play_count, row.clear_count, row.total_notes);
    ScoreState {
        judges: JudgeCounts {
            fast_pgreat: row.perfect,
            fast_great: row.great,
            fast_good: row.good,
            fast_bad: row.bad,
            fast_poor: row.poor,
            ..Default::default()
        },
        combo: 0,
        max_combo: row.max_combo,
        past_notes: expected_notes,
        ghost,
        empty_poor_breaks_combo: false,
    }
}

fn score_state_from_beatoraja(row: &BeatorajaScoreRow, expected_notes: u32) -> ScoreState {
    let ghost = decode_external_ghost(&row.ghost, expected_notes);
    let _ = (row.min_bp, row.total_notes);
    ScoreState {
        judges: JudgeCounts {
            fast_pgreat: row.epg,
            slow_pgreat: row.lpg,
            fast_great: row.egr,
            slow_great: row.lgr,
            fast_good: row.egd,
            slow_good: row.lgd,
            fast_bad: row.ebd,
            slow_bad: row.lbd,
            fast_poor: row.epr,
            slow_poor: row.lpr,
            fast_empty_poor: row.ems,
            slow_empty_poor: row.lms,
        },
        combo: 0,
        max_combo: row.max_combo,
        past_notes: expected_notes,
        ghost,
        empty_poor_breaks_combo: false,
    }
}

fn decode_external_ghost(encoded: &str, total_notes: u32) -> Vec<u8> {
    if encoded.is_empty() {
        return Vec::new();
    }
    match decode_beatoraja_ghost(encoded, total_notes) {
        Ok(ghost) => ghost,
        Err(error) => {
            tracing::warn!(%error, "failed to decode imported score ghost");
            Vec::new()
        }
    }
}

/// Decodes LR2's `score.ghost` column into bmz's per-note judge array.
///
/// The LR2 format (see OpenLR2 `LR2_ghost.cpp` `EncodeGhostData`/`DecodeGhostData`)
/// is a run-length encoding of per-note judge symbols `@ A B C D E` (= judge codes
/// 0..=5), wrapped in two layers of bigram dictionary compression.  We reverse the
/// dictionaries (layer 2 then layer 1, as LR2 does), expand the run-length runs,
/// then map LR2 judge codes to bmz's (`5 - code`): E/5=PGreat→0, D/4=Great→1,
/// C/3=Good→2, B/2=Bad→3, A/1=Poor→4.  Code 0 (`@`) is an empty poor not tied to a
/// scoreable note and is dropped.  The result is padded with Poor / truncated to
/// `total_notes`, mirroring [`decode_beatoraja_ghost`].
fn decode_lr2_ghost(encoded: &str, total_notes: u32) -> Vec<u8> {
    if encoded.is_empty() {
        return Vec::new();
    }

    let mut layer2 = String::with_capacity(encoded.len() * 2);
    for c in encoded.chars() {
        match lr2_ghost_layer2_symbol(c) {
            Some(replacement) => layer2.push_str(replacement),
            None => layer2.push(c),
        }
    }
    let mut expanded = String::with_capacity(layer2.len() * 2);
    for c in layer2.chars() {
        match lr2_ghost_layer1_symbol(c) {
            Some(replacement) => expanded.push_str(replacement),
            None => expanded.push(c),
        }
    }

    let mut ghost: Vec<u8> = Vec::with_capacity(total_notes as usize);
    let mut current: Option<u8> = None;
    let mut rep: i64 = -1;
    for c in expanded.chars() {
        let o = c as u32;
        if (0x40..=0x45).contains(&o) {
            if let Some(code) = current {
                push_lr2_run(&mut ghost, code, if rep == 0 { 1 } else { rep });
            }
            rep = 0;
            current = Some((o - 0x40) as u8);
        } else if c.is_ascii_digit() {
            let digit = (o - 0x30) as i64;
            rep = if rep == 0 { digit } else { rep * 10 + digit };
        }
    }
    if let Some(code) = current {
        push_lr2_run(&mut ghost, code, if rep == 0 { 1 } else { rep });
    }

    let expected = total_notes as usize;
    if expected > 0 {
        if ghost.len() < expected {
            ghost.resize(expected, 4);
        } else {
            ghost.truncate(expected);
        }
    }
    ghost
}

/// Appends `count` copies of an LR2 judge `code` (0..=5) to a bmz ghost, mapping
/// LR2 codes to bmz judge codes via `5 - code`.  Code 0 (empty poor) is not a
/// scoreable note and is skipped.
fn push_lr2_run(ghost: &mut Vec<u8>, code: u8, count: i64) {
    if (1..=5).contains(&code) {
        let bmz_code = 5 - code;
        for _ in 0..count.max(1) {
            ghost.push(bmz_code);
        }
    }
}

/// LR2 ghost layer-2 dictionary (`q`..`z`), reversed on decode before layer 1.
fn lr2_ghost_layer2_symbol(c: char) -> Option<&'static str> {
    Some(match c {
        'q' => "XX",
        'r' => "X1",
        's' => "X2",
        't' => "X3",
        'u' => "X4",
        'v' => "X5",
        'w' => "X6",
        'x' => "X7",
        'y' => "X8",
        'z' => "X9",
        _ => return None,
    })
}

/// LR2 ghost layer-1 dictionary (`F`..`p`), reversed after layer 2 on decode.
fn lr2_ghost_layer1_symbol(c: char) -> Option<&'static str> {
    Some(match c {
        'F' => "E1",
        'G' => "E2",
        'H' => "E3",
        'I' => "E4",
        'J' => "E5",
        'K' => "E6",
        'L' => "E7",
        'M' => "E8",
        'N' => "E9",
        'P' => "EC",
        'Q' => "EB",
        'R' => "EA",
        'S' => "D2",
        'T' => "D3",
        'U' => "D4",
        'V' => "D5",
        'W' => "D6",
        'X' => "DE",
        'Y' => "DC",
        'a' => "DB",
        'b' => "DA",
        'c' => "C2",
        'd' => "C3",
        'e' => "C4",
        'f' => "C5",
        'g' => "CE",
        'h' => "CD",
        'i' => "CB",
        'j' => "CA",
        'k' => "AB",
        'l' => "AC",
        'm' => "AD",
        'n' => "AE",
        'o' => "A2",
        'p' => "A3",
        _ => return None,
    })
}

fn normalize_imported_played_at(value: i64) -> Option<i64> {
    if value <= 0 {
        None
    } else if value >= 100_000_000_000 {
        Some(value / 1000)
    } else {
        Some(value)
    }
}

fn lr2_clear_type(clear: i64) -> ClearType {
    match clear {
        0 => ClearType::NoPlay,
        1 => ClearType::Failed,
        2 => ClearType::Easy,
        3 => ClearType::Normal,
        4 => ClearType::Hard,
        5 => ClearType::FullCombo,
        6 => ClearType::Perfect,
        _ => ClearType::NoPlay,
    }
}

fn beatoraja_clear_type(clear: i64) -> ClearType {
    match clear {
        0 => ClearType::NoPlay,
        1 => ClearType::Failed,
        2 => ClearType::AssistEasy,
        3 => ClearType::LightAssistEasy,
        4 => ClearType::Easy,
        5 => ClearType::Normal,
        6 => ClearType::Hard,
        7 => ClearType::ExHard,
        8 => ClearType::FullCombo,
        9 => ClearType::Perfect,
        10 => ClearType::Max,
        _ => ClearType::NoPlay,
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, LongNotePair, LongNoteStyle, PlayableChart};
    use bmz_core::ids::NoteId;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};
    use rusqlite::params;

    use super::*;
    use crate::select_options::DoubleOptionScoreBucket;
    use crate::storage::common::hash_to_hex;
    use crate::storage::library_db::{ChartImportRecord, LibraryDatabase};
    use crate::storage::migration::{LIBRARY_MIGRATIONS, SCORE_MIGRATIONS, run_migrations};
    use crate::storage::score_db::ScoreKey;
    use bmz_gameplay::rule::RuleMode;

    #[test]
    fn lr2_import_maps_md5_and_clear_type() {
        let (mut library_db, mut score_db, sha256, md5) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source(&source, &md5);

        let report = import_lr2_scores_with_device_type(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
            InputDeviceKind::Controller,
        )
        .unwrap();

        assert_eq!(report.imported, 1);
        let best = score_db
            .best_scores_for_charts(&[
                ScoreKey::new(sha256, LnScorePolicy::ForceLn).with_rule_mode(RuleMode::Lr2Oraja)
            ])
            .unwrap();
        assert_eq!(best[0].clear_type, "Hard");
        assert_eq!(best[0].ex_score, 222);
        assert_eq!(best[0].ln_policy, LnScorePolicy::ForceLn);
        assert_eq!(best[0].device_type, InputDeviceKind::Controller);
    }

    #[test]
    fn lr2_import_preserves_supported_op_best_arrangements_and_flip() {
        let (mut library_db, mut score_db, _, md5) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source(&source, &md5);
        // Gauge=1 (ignored), 1P=RANDOM, 2P=S-RANDOM, DP=FLIP.
        source.execute("UPDATE score SET op_best = 1321", []).unwrap();

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.imported, 1);
        let options: (String, String, String, String) = score_db
            .conn()
            .query_row(
                "SELECT arrange, arrange_2p, double_option, applied_double_option
                 FROM score_history",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            options,
            ("Random".to_string(), "SRandom".to_string(), "Off".to_string(), "Flip".to_string(),)
        );
    }

    #[test]
    fn lr2_import_skips_unsupported_op_best_arrangements() {
        for op_best in [40, 500, 60, 2_000] {
            let (mut library_db, mut score_db, _, md5) = open_test_databases();
            let source = Connection::open_in_memory().unwrap();
            create_lr2_source(&source, &md5);
            source.execute("UPDATE score SET op_best = ?1", [op_best]).unwrap();

            let report = import_lr2_scores(
                &source,
                ScoreImportKind::Lr2,
                &mut library_db,
                &mut score_db,
                1_700_000_000,
            )
            .unwrap();

            assert_eq!(report.scanned, 1, "op_best={op_best}");
            assert_eq!(report.skipped, 1, "op_best={op_best}");
            assert_eq!(report.imported, 0, "op_best={op_best}");
            assert_eq!(report.failed, 0, "op_best={op_best}");
        }
    }

    #[test]
    fn beatoraja_import_preserves_fast_slow_counts_and_current_schema_fields() {
        let (library_db, mut score_db, sha256, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);
        // 1P=ROTATE, 2P=MIRROR, double=FLIP.  FLIP shares the Off score
        // bucket, but is retained as the applied option in history.
        source.execute("UPDATE score SET option = 113", []).unwrap();

        let report = import_beatoraja_scores_with_device_type(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
            InputDeviceKind::Controller,
        )
        .unwrap();

        assert_eq!(report.imported, 1);
        let row: (
            (String, u32, u32, u32, String, String, String, String),
            (String, String, String, String, i64),
        ) = score_db
            .conn()
            .query_row(
                "SELECT clear_type, fast_pgreat, slow_pgreat, slow_empty_poor,
                    rule_mode, ln_policy, double_option, applied_double_option, arrange, arrange_2p,
                    device_type, source_kind, played_at
                 FROM score_history",
                [],
                |row| {
                    Ok((
                        (
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                        ),
                        (row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?),
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                (
                    "ExHard".to_string(),
                    100,
                    10,
                    1,
                    "Beatoraja".to_string(),
                    "ForceLn".to_string(),
                    "Off".to_string(),
                    "Flip".to_string(),
                ),
                (
                    "RRandom".to_string(),
                    "Mirror".to_string(),
                    "controller".to_string(),
                    "Beatoraja".to_string(),
                    1_700_000_001,
                ),
            )
        );
    }

    #[test]
    fn beatoraja_option_maps_both_arrange_slots_and_double_bucket() {
        assert_eq!(
            beatoraja_arrange_options(213, "7K"),
            (ArrangeOption::RRandom, ArrangeOption::Mirror)
        );
        assert_eq!(beatoraja_double_option(213).score_bucket(), DoubleOptionScoreBucket::Battle);
        assert_eq!(beatoraja_double_option(100), DoubleOption::Flip);
        // FLIP is intentionally in the existing Off score bucket, while its
        // actual option is retained by `ScoreRecord::applied_double_option`.
        assert_eq!(beatoraja_double_option(100).score_bucket(), DoubleOptionScoreBucket::Off);
        assert_eq!(beatoraja_double_option(200), DoubleOption::Battle);
        assert_eq!(
            beatoraja_double_option(300).score_bucket(),
            DoubleOptionScoreBucket::BattleAutoScratch
        );
        assert_eq!(
            beatoraja_arrange_options(7, "9K"),
            (ArrangeOption::Normal, ArrangeOption::Normal)
        );
        assert_eq!(
            beatoraja_arrange_options(8, "9K"),
            (ArrangeOption::Random, ArrangeOption::Normal)
        );
        assert_eq!(
            beatoraja_arrange_options(9, "9K"),
            (ArrangeOption::SRandom, ArrangeOption::Normal)
        );
    }

    #[test]
    fn beatoraja_import_skips_identical_scores_from_same_source_kind() {
        let (library_db, mut score_db, sha256, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

        let first = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(first.imported, 1);

        // A timestamp change alone does not make an external score a new play.
        source.execute("UPDATE score SET date = ?1", params![1_700_000_002_000_i64]).unwrap();
        let duplicate = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(duplicate.imported, 0);
        assert_eq!(duplicate.skipped, 1);

        // Provenance is part of the duplicate key: the same score imported from
        // LR2oraja remains a separate history entry.
        let distinct_source = import_beatoraja_scores(
            &source,
            ScoreImportKind::Lr2Oraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(distinct_source.imported, 1);
        let source_kinds: Vec<String> = score_db
            .conn()
            .prepare("SELECT source_kind FROM score_history ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(source_kinds, vec!["Beatoraja", "Lr2Oraja"]);
    }

    #[test]
    fn beatoraja_reimport_corrects_device_type_without_adding_history() {
        let (library_db, mut score_db, sha256, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

        let first = import_beatoraja_scores_with_device_type(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
            InputDeviceKind::Keyboard,
        )
        .unwrap();
        assert_eq!(first.imported, 1);

        let corrected = import_beatoraja_scores_with_device_type(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
            InputDeviceKind::Controller,
        )
        .unwrap();
        assert_eq!(corrected.imported, 0);
        assert_eq!(corrected.corrected, 1);
        assert_eq!(corrected.skipped, 0);

        let history_count: u32 = score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
            .unwrap();
        let history_device: String = score_db
            .conn()
            .query_row("SELECT device_type FROM score_history", [], |row| row.get(0))
            .unwrap();
        let best = score_db
            .best_scores_for_charts(&[ScoreKey::new(sha256, LnScorePolicy::ForceLn)])
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(history_count, 1);
        assert_eq!(history_device, "controller");
        assert_eq!(best.device_type, InputDeviceKind::Controller);
    }

    #[test]
    fn lr2oraja_dx_import_sets_dx_rule_mode() {
        let (library_db, mut score_db, sha256, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

        import_beatoraja_scores(
            &source,
            ScoreImportKind::Lr2OrajaDx,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        let rule_mode: String = score_db
            .conn()
            .query_row("SELECT rule_mode FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rule_mode, "Dx");
    }

    #[test]
    fn lr2oraja_import_uses_beatoraja_schema_and_rule_mode() {
        let (library_db, mut score_db, sha256, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Lr2Oraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.imported, 1);
        let rule_mode: String = score_db
            .conn()
            .query_row("SELECT rule_mode FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rule_mode, "Lr2Oraja");
    }

    #[test]
    fn beatoraja_import_mode_cn_on_undefined_ln_sets_force_cn() {
        clear_test_import_charts();
        let (mut library_db, mut score_db, sha256, _) =
            open_test_databases_with_chart(undefined_ln_chart(2, 2));
        set_test_import_chart(sha256, undefined_ln_chart(2, 2));
        let source = Connection::open_in_memory().unwrap();
        // ForceCn expected = 2 base + 2 CN ends = 4
        create_beatoraja_source_with_score(
            &source,
            &hash_to_hex(&sha256),
            1_700_000_001_000,
            1,
            7,
            4,
            4,
            4,
        );

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.imported, 1);
        let ln_policy: String = score_db
            .conn()
            .query_row("SELECT ln_policy FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ln_policy, "ForceCn");
        clear_test_import_charts();
        let _ = &mut library_db;
    }

    #[test]
    fn beatoraja_import_falls_back_to_force_ln_when_only_ln_expected_matches() {
        clear_test_import_charts();
        let (library_db, mut score_db, sha256, _) =
            open_test_databases_with_chart(undefined_ln_chart(2, 2));
        set_test_import_chart(sha256, undefined_ln_chart(2, 2));
        let source = Connection::open_in_memory().unwrap();
        // mode=1 -> ForceCn expects 4, but source notes=2 match ForceLn only.
        create_beatoraja_source_with_score(
            &source,
            &hash_to_hex(&sha256),
            1_700_000_001_000,
            1,
            7,
            2,
            2,
            2,
        );

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.imported, 1);
        let ln_policy: String = score_db
            .conn()
            .query_row("SELECT ln_policy FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ln_policy, "ForceLn");
        clear_test_import_charts();
    }

    #[test]
    fn beatoraja_import_fails_when_source_note_count_mismatches_all_policies() {
        clear_test_import_charts();
        let (library_db, mut score_db, sha256, _) =
            open_test_databases_with_chart(undefined_ln_chart(2, 2));
        set_test_import_chart(sha256, undefined_ln_chart(2, 2));
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source_with_score(
            &source,
            &hash_to_hex(&sha256),
            1_700_000_001_000,
            1,
            7,
            3,
            3,
            3,
        );

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(report.imported, 0);
        clear_test_import_charts();
    }

    #[test]
    fn beatoraja_import_accepts_failed_row_with_fewer_judgements() {
        clear_test_import_charts();
        let (library_db, mut score_db, sha256, _) =
            open_test_databases_with_chart(undefined_ln_chart(2, 2));
        set_test_import_chart(sha256, undefined_ln_chart(2, 2));
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source_with_score(
            &source,
            &hash_to_hex(&sha256),
            1_700_000_001_000,
            1,
            1,
            4,
            3,
            3,
        );

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.failed, 0);
        let clear_type: String = score_db
            .conn()
            .query_row("SELECT clear_type FROM score_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(clear_type, "Failed");
        clear_test_import_charts();
    }

    #[test]
    fn beatoraja_import_accepts_more_judgements_than_source_notes() {
        let (library_db, mut score_db, sha256, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source_with_score(
            &source,
            &hash_to_hex(&sha256),
            1_700_000_001_000,
            0,
            7,
            128,
            129,
            80,
        );

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn lr2_import_accepts_empty_poor_in_judge_total() {
        let (mut library_db, mut score_db, _, md5) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source_with_score(&source, &hash_to_hex(&md5), 128, 64, 100, 22, 3, 2, 20);

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.failed, 0);
        assert_eq!(report.imported, 1);
    }

    #[test]
    fn lr2_import_fails_when_source_note_count_mismatches() {
        let (mut library_db, mut score_db, _, md5) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source_with_score(&source, &hash_to_hex(&md5), 127, 64, 100, 20, 3, 2, 1);

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(report.imported, 0);
    }

    #[test]
    fn import_score_summary_sanity_checks_ex_score_and_combo() {
        assert!(score_summary_is_sane(128, 128, 256));
        assert!(!score_summary_is_sane(0, 0, 0));
        assert!(!score_summary_is_sane(128, 129, 256));
        assert!(!score_summary_is_sane(128, 128, 257));
    }

    #[test]
    fn lr2_import_skips_unregistered_md5() {
        let (mut library_db, mut score_db, _, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source(&source, &[9; 16]);

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.imported, 0);
    }

    #[test]
    fn beatoraja_import_skips_course_scores_without_failing() {
        let (library_db, mut score_db, _, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        // A 4-song course key: four 64-char hashes concatenated (256 chars).
        let course_key = "a".repeat(256);
        create_beatoraja_source_with_sha256(&source, &course_key, 1_700_000_001_000, 0);

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(report.imported, 0);
    }

    #[test]
    fn lr2_import_skips_course_scores_without_failing() {
        let (mut library_db, mut score_db, _, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        // An LR2 course key: a 32-char marker plus four 32-char md5s (160 chars).
        let course_key = "0".repeat(32) + &"a".repeat(128);
        create_lr2_source_with_hash(&source, &course_key);

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(report.imported, 0);
    }

    #[test]
    fn lr2_course_import_resolves_canonical_and_fans_out_ln_variants() {
        use bmz_core::course::{
            CourseConstraints, CourseDefinition, CourseEntry, CourseKind, CourseLnConstraint,
        };

        let (mut library_db, mut score_db, _, _) = open_test_databases();
        let stage_md5s = [
            "11111111111111111111111111111111",
            "22222222222222222222222222222222",
            "33333333333333333333333333333333",
            "44444444444444444444444444444444",
        ];
        let stage_sha256s = ["11".repeat(32), "22".repeat(32), "33".repeat(32), "44".repeat(32)];
        let entries: Vec<CourseEntry> = stage_md5s
            .iter()
            .enumerate()
            .map(|(i, m)| CourseEntry {
                title_hint: format!("stage{i}"),
                md5: Some(m.to_string()),
                sha256: Some(stage_sha256s[i].clone()),
                chart_id: None,
            })
            .collect();
        let course =
            |key: &str, judge: CourseJudgeConstraint, ln: CourseLnConstraint| CourseDefinition {
                key: key.to_string(),
                title: key.to_string(),
                kind: CourseKind::Dan,
                entries: entries.clone(),
                constraints: CourseConstraints {
                    class: CourseClassConstraint::GradeMirrorAllowed,
                    speed: CourseSpeedConstraint::Free,
                    judge,
                    gauge: CourseGaugeConstraint::Lr2,
                    ln,
                    source_constraints: Vec::new(),
                },
                trophies: Vec::new(),
                release: true,
            };
        // Two canonical variants differing only by LN -> both receive the score.
        library_db
            .upsert_course(
                "table:x",
                &course("dan_default", CourseJudgeConstraint::Normal, CourseLnConstraint::Default),
                0,
                1,
            )
            .unwrap();
        library_db
            .upsert_course(
                "table:x",
                &course("dan_ln", CourseJudgeConstraint::Normal, CourseLnConstraint::Ln),
                1,
                1,
            )
            .unwrap();
        // Non-canonical (no_good judge) sharing the same songs -> must be ignored.
        library_db
            .upsert_course(
                "table:x",
                &course("dan_nogood", CourseJudgeConstraint::NoGood, CourseLnConstraint::Default),
                2,
                1,
            )
            .unwrap();

        // LR2 course record: 32-char marker + the four stage md5s (160 chars).
        let hash = "0".repeat(32) + &stage_md5s.concat();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source_with_hash(&source, &hash);

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.matched, 1);
        // Fanned out into the two canonical LN variants, not the no_good course.
        assert_eq!(report.imported, 2);
        let count: i64 = score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM course_scores", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let distinct_hashes: i64 = score_db
            .conn()
            .query_row("SELECT COUNT(DISTINCT course_hash) FROM course_scores", [], |r| r.get(0))
            .unwrap();
        assert_eq!(distinct_hashes, 2);
        // The imported course score reflects the LR2 aggregate row (clear=4 -> Hard,
        // ex = perfect*2 + great = 222).
        let (clear, ex): (String, u32) = score_db
            .conn()
            .query_row("SELECT clear_type, ex_score FROM course_scores LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(clear, "Hard");
        assert_eq!(ex, 222);

        // Course rows share LR2's `score` table: do not create another course
        // history entry when its best score used SCATTER.
        source.execute("UPDATE score SET op_best = 40", []).unwrap();
        let skipped = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_001,
        )
        .unwrap();
        assert_eq!(skipped.skipped, 1);
        assert_eq!(skipped.imported, 0);
        let count_after_skip: i64 = score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM course_scores", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_after_skip, 2);
    }

    #[test]
    fn decode_lr2_ghost_handles_plain_symbols() {
        // No dictionary tokens, no run counts: B A E D -> Bad, Poor, PGreat, Great.
        assert_eq!(decode_lr2_ghost("BAED", 4), vec![3, 4, 0, 1]);
        // Single PGreat.
        assert_eq!(decode_lr2_ghost("E", 1), vec![0]);
    }

    #[test]
    fn decode_lr2_ghost_expands_dictionary_and_runs() {
        // Real LR2 ghost captured from a player DB.  Exercises both dictionary
        // layers (m,c,k,S,c,b,Z tokens), a run count (`@2`, `8`) and the leading
        // empty-poor (`@`) that must be dropped.  Validated against the LR2 score
        // row's judge counts.
        let ghost = decode_lr2_ghost("@2mBckScb8Z", 20);
        assert_eq!(ghost, vec![4, 1, 3, 2, 2, 4, 3, 1, 1, 2, 2, 1, 4, 4, 4, 4, 4, 4, 4, 4]);
    }

    #[test]
    fn decode_lr2_ghost_pads_and_truncates_to_total_notes() {
        // Aborted play: decoded ghost shorter than the chart -> pad with Poor (4).
        let padded = decode_lr2_ghost("E", 4);
        assert_eq!(padded, vec![0, 4, 4, 4]);
        // Over-long ghost is truncated to the note count.
        let truncated = decode_lr2_ghost("E", 0);
        assert_eq!(truncated, vec![0]); // total_notes 0 leaves the decode untouched
        let truncated = decode_lr2_ghost("BAED", 2);
        assert_eq!(truncated, vec![3, 4]);
    }

    #[test]
    fn lr2_score_state_decodes_ghost() {
        let row = Lr2ScoreRow {
            md5: "0".repeat(32),
            clear: 4,
            perfect: 100,
            great: 21,
            good: 3,
            bad: 2,
            poor: 1,
            total_notes: 4,
            max_combo: 64,
            min_bp: 3,
            play_count: 2,
            clear_count: 1,
            ghost: "BAED".to_string(),
            random_seed: Some(123),
            op_best: 0,
        };
        let state = score_state_from_lr2(&row, 4);
        assert_eq!(state.ghost, vec![3, 4, 0, 1]);
        assert_eq!(state.judges.fast_pgreat, 100);
    }

    #[test]
    fn is_course_hash_classifies_by_length() {
        // beatoraja sha256 width.
        assert!(!is_course_hash(&"a".repeat(64), 64));
        assert!(is_course_hash(&"a".repeat(128), 64));
        assert!(is_course_hash(&"a".repeat(256), 64));
        // LR2 md5 width.
        assert!(!is_course_hash(&"a".repeat(32), 32));
        assert!(is_course_hash(&"a".repeat(160), 32));
        // Genuinely malformed (not a multiple of the width) stays a hard failure.
        assert!(!is_course_hash(&"a".repeat(100), 64));
        assert!(!is_course_hash("", 64));
    }

    fn open_test_databases() -> (LibraryDatabase, ScoreDatabase, [u8; 32], [u8; 16]) {
        open_test_databases_with_chart(chart())
    }

    fn open_test_databases_with_chart(
        chart: PlayableChart,
    ) -> (LibraryDatabase, ScoreDatabase, [u8; 32], [u8; 16]) {
        let mut library_conn = Connection::open_in_memory().unwrap();
        super::super::common::configure_connection(&library_conn).unwrap();
        run_migrations(&mut library_conn, LIBRARY_MIGRATIONS).unwrap();
        let mut library_db = LibraryDatabase::from_connection(library_conn);
        let sha256 = chart.identity.file_sha256;
        let md5 = chart.identity.file_md5;
        library_db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: Path::new("/songs/import.bms"),
                file_size: 10,
                modified_at: 1,
                scanned_at: 1,
                chart: &chart,
            })
            .unwrap();

        let mut score_conn = Connection::open_in_memory().unwrap();
        super::super::common::configure_connection(&score_conn).unwrap();
        run_migrations(&mut score_conn, SCORE_MIGRATIONS).unwrap();
        (library_db, ScoreDatabase::from_connection(score_conn), sha256, md5)
    }

    fn chart() -> PlayableChart {
        let mut chart = PlayableChart {
            identity: compute_chart_identity(b"score import test"),
            metadata: ChartMetadata {
                title: "Import Target".to_string(),
                artist: "artist".to_string(),
                initial_bpm: 128.0,
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
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
            bga_asset_by_bmp_key: HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 128,
            end_time: TimeUs(10_000_000),
        };
        chart.identity.file_md5 = [1; 16];
        chart.identity.file_sha256 = [2; 32];
        chart
    }

    fn undefined_ln_chart(total_notes: u32, long_pairs: u32) -> PlayableChart {
        let mut chart = chart();
        chart.total_notes = total_notes;
        chart.long_notes = (0..long_pairs)
            .map(|index| LongNotePair {
                lane: Lane::Key1,
                style: LongNoteStyle::ChannelPair,
                mode: None,
                start_note_id: NoteId(index * 2 + 1),
                end_note_id: NoteId(index * 2 + 2),
                start_tick: ChartTick(0),
                end_tick: ChartTick(192),
                start_time: TimeUs(0),
                end_time: TimeUs(1_000_000),
                sound: None,
            })
            .collect();
        chart
    }

    fn create_lr2_source(conn: &Connection, md5: &[u8; 16]) {
        create_lr2_source_with_hash(conn, &hash_to_hex(md5));
    }

    fn create_lr2_source_with_hash(conn: &Connection, hash: &str) {
        // `poor` includes Empty Poor in LR2 and may make the judge sum exceed totalnotes.
        create_lr2_source_with_score(conn, hash, 128, 64, 100, 22, 3, 2, 10);
    }

    #[allow(clippy::too_many_arguments)]
    fn create_lr2_source_with_score(
        conn: &Connection,
        hash: &str,
        total_notes: u32,
        max_combo: u32,
        perfect: u32,
        great: u32,
        good: u32,
        bad: u32,
        poor: u32,
    ) {
        conn.execute_batch(
            "CREATE TABLE score (
                hash TEXT, clear INTEGER, perfect INTEGER, great INTEGER,
                good INTEGER, bad INTEGER, poor INTEGER, totalnotes INTEGER,
                maxcombo INTEGER, minbp INTEGER, playcount INTEGER, clearcount INTEGER,
                ghost TEXT, rseed INTEGER, op_best INTEGER
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO score VALUES (?1, 4, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 3, 2, 1, '', 123, 0)",
            params![hash, perfect, great, good, bad, poor, total_notes, max_combo],
        )
        .unwrap();
    }

    fn create_beatoraja_source(conn: &Connection, sha256: &[u8; 32], date: i64, mode: i64) {
        create_beatoraja_source_with_sha256(conn, &hash_to_hex(sha256), date, mode);
    }

    fn create_beatoraja_source_with_sha256(conn: &Connection, sha256: &str, date: i64, mode: i64) {
        // Default no-LN chart expects 128 scored notes.
        create_beatoraja_source_with_score(conn, sha256, date, mode, 7, 128, 128, 80);
    }

    #[allow(clippy::too_many_arguments)]
    fn create_beatoraja_source_with_score(
        conn: &Connection,
        sha256: &str,
        date: i64,
        mode: i64,
        clear: i64,
        total_notes: u32,
        judged: u32,
        max_combo: u32,
    ) {
        // Split judged across fast/slow buckets for schema coverage; empty poor
        // (ems/lms) is excluded from the import note-count check.
        let epg = judged.saturating_sub(28).min(judged);
        let rem = judged.saturating_sub(epg);
        let lpg = rem.min(10);
        let rem = rem.saturating_sub(lpg);
        let egr = rem.min(5);
        let rem = rem.saturating_sub(egr);
        let lgr = rem.min(3);
        let rem = rem.saturating_sub(lgr);
        let egd = rem.min(2);
        let rem = rem.saturating_sub(egd);
        let lgd = rem.min(1);
        let rem = rem.saturating_sub(lgd);
        let ebd = rem.min(2);
        let rem = rem.saturating_sub(ebd);
        let lbd = rem.min(1);
        let rem = rem.saturating_sub(lbd);
        let epr = rem.min(3);
        let lpr = rem.saturating_sub(epr);

        if conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='score'",
                [],
                |_| Ok(()),
            )
            .is_err()
        {
            conn.execute_batch(
                "CREATE TABLE score (
                    sha256 TEXT, mode INTEGER, clear INTEGER, epg INTEGER, lpg INTEGER,
                    egr INTEGER, lgr INTEGER, egd INTEGER, lgd INTEGER,
                    ebd INTEGER, lbd INTEGER, epr INTEGER, lpr INTEGER,
                    ems INTEGER, lms INTEGER, notes INTEGER, combo INTEGER,
                    minbp INTEGER, ghost TEXT, seed INTEGER, date INTEGER, option INTEGER
                );",
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO score VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 3, 1, ?14, ?15, 2, '', 456, ?16, 0
            )",
            params![
                sha256,
                mode,
                clear,
                epg,
                lpg,
                egr,
                lgr,
                egd,
                lgd,
                ebd,
                lbd,
                epr,
                lpr,
                total_notes,
                max_combo,
                date
            ],
        )
        .unwrap();
    }
}
