use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::course::{
    CourseClassConstraint, CourseGaugeConstraint, CourseJudgeConstraint, CourseSpeedConstraint,
};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::rule::RuleMode;
use bmz_gameplay::score::{JudgeCounts, ScoreState};
use rusqlite::{Connection, OpenFlags, Row};

use super::common::hex_to_hash;
use super::library_db::{CHART_IMPORT_VERSION, ChartListItem, ChartSource, LibraryDatabase};
use super::score_db::{
    CourseScoreInsert, ImportedScoreReconciliation, ScoreDatabase, ScoreRecord, ScoreSourceKind,
    decode_beatoraja_ghost,
};
use crate::ln_policy::{LnPolicySetting, LnScorePolicy, score_ln_policy};
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
    pub issues: Vec<ScoreImportIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreImportIssueKind {
    MissingChart,
    Duplicate,
    NoteCountMismatch,
    ReadFailure,
    Metadata,
    InvalidScore,
    Unsupported,
}

impl ScoreImportIssueKind {
    fn message_key(self) -> &'static str {
        match self {
            Self::MissingChart => "score-import-reason-missing",
            Self::Duplicate => "score-import-reason-duplicate",
            Self::NoteCountMismatch => "score-import-reason-notes",
            Self::ReadFailure => "score-import-reason-read",
            Self::Metadata => "score-import-reason-metadata",
            Self::InvalidScore => "score-import-reason-invalid",
            Self::Unsupported => "score-import-reason-unsupported",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::MissingChart => "missing chart",
            Self::Duplicate => "duplicate",
            Self::NoteCountMismatch => "note-count mismatch",
            Self::ReadFailure => "read failure",
            Self::Metadata => "library metadata",
            Self::InvalidScore => "invalid score",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScoreImportIssue {
    pub kind: ScoreImportIssueKind,
    pub count: u32,
    pub example: String,
}

impl ScoreImportReport {
    pub fn summary_for_locale(&self, locale: crate::i18n::AppLocale) -> String {
        use crate::i18n::{FluentArgs, Localizer};
        let text = Localizer::new(locale);
        let mut args = FluentArgs::new();
        for (name, count) in [
            ("scanned", self.scanned),
            ("matched", self.matched),
            ("imported", self.imported),
            ("corrected", self.corrected),
            ("skipped", self.skipped),
            ("failed", self.failed),
        ] {
            args.set(name, i64::from(count));
        }
        let mut summary = text.format("score-import-summary", &args);
        for issue in &self.issues {
            summary.push_str(&format!(
                "\n{} {}: {}",
                text.text(issue.kind.message_key()),
                issue.count,
                issue.example
            ));
        }
        summary
    }

    fn issue(&mut self, kind: ScoreImportIssueKind, example: impl Into<String>) {
        if let Some(issue) = self.issues.iter_mut().find(|issue| issue.kind == kind) {
            issue.count += 1;
        } else {
            self.issues.push(ScoreImportIssue { kind, count: 1, example: example.into() });
        }
    }

    pub fn summary(&self) -> String {
        let mut summary = format!(
            "scanned {}, matched {}, imported {}, corrected {}, skipped {}, failed {}",
            self.scanned, self.matched, self.imported, self.corrected, self.skipped, self.failed
        );
        for issue in &self.issues {
            summary.push_str(&format!(
                "\n{} {}: {}",
                issue.kind.label(),
                issue.count,
                issue.example
            ));
        }
        summary
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
    let mut chart_cache: HashMap<[u8; 32], ChartSource> = HashMap::new();
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
                report.issue(ScoreImportIssueKind::ReadFailure, format!("{error:#}"));
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
                report.issue(
                    ScoreImportIssueKind::Unsupported,
                    format!("{}: op_best={} {error:?}", row.md5, row.op_best),
                );
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
                report.issue(ScoreImportIssueKind::InvalidScore, format!("{}: {error:#}", row.md5));
                tracing::warn!(md5 = %row.md5, %error, "invalid LR2 score md5");
                continue;
            }
        };
        let Some(chart_sha256) = library_db.chart_sha256_by_md5(md5)? else {
            report.skipped += 1;
            report.issue(ScoreImportIssueKind::MissingChart, row.md5.clone());
            continue;
        };
        report.matched += 1;

        let ex_score = lr2_ex_score(&row);
        if !score_summary_is_sane(row.total_notes, row.max_combo, ex_score) {
            report.failed += 1;
            report.issue(
                ScoreImportIssueKind::InvalidScore,
                format!(
                    "{}: notes={}, combo={}, EX={ex_score}",
                    row.md5, row.total_notes, row.max_combo
                ),
            );
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
                let detail = note_count_mismatch_details(
                    &chart_cache[&chart_sha256].chart,
                    LnScorePolicy::ForceLn,
                    row.total_notes,
                );
                report.issue(ScoreImportIssueKind::NoteCountMismatch, detail.clone());
                tracing::warn!(
                    md5 = %row.md5,
                    detail,
                    source_notes = row.total_notes,
                    "LR2 score source note count does not match expected note count"
                );
                continue;
            }
            Err(error) => {
                report.failed += 1;
                report.issue(ScoreImportIssueKind::Metadata, format!("{}: {error:#}", row.md5));
                tracing::warn!(md5 = %row.md5, error = %format!("{error:#}"), "failed to resolve LR2 import chart");
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
        match score_db.reconcile_imported_score(&record)? {
            ImportedScoreReconciliation::Missing => {}
            ImportedScoreReconciliation::Unchanged => {
                report.skipped += 1;
                report.issue(ScoreImportIssueKind::Duplicate, row.md5.clone());
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
    let mut chart_cache: HashMap<[u8; 32], ChartSource> = HashMap::new();
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
                report.issue(ScoreImportIssueKind::ReadFailure, format!("{error:#}"));
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
            report.issue(ScoreImportIssueKind::Unsupported, "beatoraja course score");
            tracing::debug!(len = row.sha256.len(), "skipped beatoraja course score");
            continue;
        }
        let chart_sha256 = match hex_to_hash::<32>(&row.sha256) {
            Ok(sha256) => sha256,
            Err(error) => {
                report.failed += 1;
                report.issue(
                    ScoreImportIssueKind::InvalidScore,
                    format!("{}: {error:#}", row.sha256),
                );
                tracing::warn!(sha256 = %row.sha256, %error, "invalid beatoraja score sha256");
                continue;
            }
        };
        if library_db.chart_id_by_sha256(chart_sha256)?.is_none() {
            report.skipped += 1;
            report.issue(ScoreImportIssueKind::MissingChart, row.sha256.clone());
            continue;
        }
        report.matched += 1;

        let setting = beatoraja_mode_to_ln_setting(row.mode);
        let chart_item = match import_chart_metadata(library_db, chart_sha256, &mut chart_cache) {
            Ok(chart) => chart,
            Err(error) => {
                report.failed += 1;
                report.issue(ScoreImportIssueKind::Metadata, format!("{}: {error:#}", row.sha256));
                tracing::warn!(sha256 = %row.sha256, error = %format!("{error:#}"), "failed to resolve beatoraja import metadata");
                continue;
            }
        };
        let ln_policy = score_ln_policy(setting, chart_item.ln_profile);
        let ex_score = beatoraja_ex_score(&row);
        if !score_summary_is_sane(row.total_notes, row.max_combo, ex_score) {
            report.failed += 1;
            report.issue(
                ScoreImportIssueKind::InvalidScore,
                format!(
                    "{}: notes={}, combo={}, EX={ex_score}",
                    row.sha256, row.total_notes, row.max_combo
                ),
            );
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
                let detail = format!(
                    "mode={} {}",
                    row.mode,
                    note_count_mismatch_details(&chart_item, ln_policy, row.total_notes)
                );
                report.issue(ScoreImportIssueKind::NoteCountMismatch, detail.clone());
                tracing::warn!(
                    sha256 = %row.sha256,
                    detail,
                    mode = row.mode,
                    source_notes = row.total_notes,
                    policy = ln_policy.as_str(),
                    "beatoraja score source note count does not match expected note count"
                );
                continue;
            }
            Err(error) => {
                report.failed += 1;
                report.issue(ScoreImportIssueKind::Metadata, format!("{}: {error:#}", row.sha256));
                tracing::warn!(
                    sha256 = %row.sha256,
                    error = %format!("{error:#}"),
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
        match score_db.reconcile_imported_score(&record)? {
            ImportedScoreReconciliation::Missing => {}
            ImportedScoreReconciliation::Unchanged => {
                report.skipped += 1;
                report.issue(ScoreImportIssueKind::Duplicate, row.sha256.clone());
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

mod course;
mod ghost;
mod options;
mod rows;

use course::*;
use ghost::*;
use options::*;
use rows::*;

#[cfg(test)]
#[path = "score_import/tests.rs"]
mod tests;
