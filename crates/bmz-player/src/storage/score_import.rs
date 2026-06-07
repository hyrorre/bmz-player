use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::course::{
    CourseClassConstraint, CourseGaugeConstraint, CourseJudgeConstraint, CourseSpeedConstraint,
};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::score::{JudgeCounts, ScoreState};
use rusqlite::{Connection, OpenFlags, Row};

use super::common::hex_to_hash;
use super::course_db::CourseScoreInsert;
use super::library_db::LibraryDatabase;
use super::score_db::{ScoreDatabase, ScoreRecord, decode_beatoraja_ghost};
use crate::ln_policy::LnScorePolicy;

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

    const fn rule_mode(self) -> &'static str {
        match self {
            Self::Beatoraja => "Beatoraja",
            Self::Lr2 | Self::Lr2Oraja => "Lr2Oraja",
            Self::Lr2OrajaDx => "Dx",
        }
    }

    const fn uses_lr2_schema(self) -> bool {
        matches!(self, Self::Lr2 | Self::Lr2Oraja | Self::Lr2OrajaDx)
    }
}

#[derive(Debug, Clone)]
pub struct ScoreImportRequest {
    pub path: PathBuf,
    pub kind: ScoreImportKind,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreImportReport {
    pub scanned: u32,
    pub matched: u32,
    pub imported: u32,
    pub skipped: u32,
    pub failed: u32,
}

impl ScoreImportReport {
    pub fn summary(&self) -> String {
        format!(
            "scanned {}, matched {}, imported {}, skipped {}, failed {}",
            self.scanned, self.matched, self.imported, self.skipped, self.failed
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
        import_lr2_scores(&source, request.kind, library_db, score_db, imported_at)
    } else {
        import_beatoraja_scores(&source, request.kind, library_db, score_db, imported_at)
    }
}

fn import_lr2_scores(
    source: &Connection,
    kind: ScoreImportKind,
    library_db: &mut LibraryDatabase,
    score_db: &mut ScoreDatabase,
    imported_at: i64,
) -> Result<ScoreImportReport> {
    ensure_table(source, "score")?;
    // Owned index of canonical LR2-dan courses (md5 stage sequence -> course ids),
    // built once before the row loop so the immutable borrow of `library_db` is
    // released before we start inserting course scores into it.
    let course_index = build_lr2_course_index(library_db)?;
    let mut report = ScoreImportReport::default();
    let mut stmt = source.prepare(
        "SELECT hash, clear, perfect, great, good, bad, poor,
                totalnotes, maxcombo, minbp, playcount, clearcount, ghost, rseed
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
        // LR2 stores course (dan) results in the same `score` table, keyed by a
        // 32-char marker segment followed by the constituent chart md5s (e.g. a
        // 160-char key for a 4-song course).  Resolve these to bmz courses and
        // import a course score for each canonical match (see import_lr2_course).
        if is_course_hash(&row.md5, 32) {
            import_lr2_course(&row, &course_index, library_db, imported_at, &mut report)?;
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

        let clear_type = lr2_clear_type(row.clear);
        let record = imported_score_record(
            chart_sha256,
            imported_at,
            clear_type,
            row.total_notes,
            score_state_from_lr2(&row),
            row.random_seed,
            kind.rule_mode(),
        );
        score_db.insert_score(&record)?;
        report.imported += 1;
    }
    Ok(report)
}

fn import_beatoraja_scores(
    source: &Connection,
    kind: ScoreImportKind,
    library_db: &LibraryDatabase,
    score_db: &mut ScoreDatabase,
    imported_at: i64,
) -> Result<ScoreImportReport> {
    let table = if table_exists(source, "score")? {
        "score"
    } else if table_exists(source, "scoredatalog")? {
        "scoredatalog"
    } else {
        bail!("beatoraja score database must contain score or scoredatalog table");
    };

    let mut report = ScoreImportReport::default();
    let sql = format!(
        "SELECT sha256, clear, epg, lpg, egr, lgr, egd, lgd, ebd, lbd,
                epr, lpr, ems, lms, notes, combo, minbp, ghost, seed, date
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

        let clear_type = beatoraja_clear_type(row.clear);
        let record = imported_score_record(
            chart_sha256,
            normalize_imported_played_at(row.date).unwrap_or(imported_at),
            clear_type,
            row.total_notes,
            score_state_from_beatoraja(&row),
            row.random_seed,
            kind.rule_mode(),
        );
        score_db.insert_score(&record)?;
        report.imported += 1;
    }
    Ok(report)
}

fn imported_score_record(
    chart_sha256: [u8; 32],
    played_at: i64,
    clear_type: ClearType,
    total_notes: u32,
    score: ScoreState,
    random_seed: Option<i64>,
    rule_mode: &str,
) -> ScoreRecord {
    ScoreRecord {
        chart_sha256,
        ln_policy: LnScorePolicy::ForceLn,
        played_at,
        clear_type,
        gauge_type: gauge_type_for_clear(clear_type),
        gauge_value: gauge_value_for_clear(clear_type),
        total_notes,
        score,
        random_seed,
        gauge_option: String::new(),
        rule_mode: rule_mode.to_string(),
        assist_mask: 0,
        autoplay: false,
        device_type: InputDeviceKind::Keyboard,
        replay_path: String::new(),
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
    course_index: &HashMap<Vec<[u8; 16]>, Vec<i64>>,
    library_db: &mut LibraryDatabase,
    imported_at: i64,
    report: &mut ScoreImportReport,
) -> Result<()> {
    let Some(stages) = lr2_course_stage_md5s(&row.md5) else {
        report.skipped += 1;
        tracing::debug!(len = row.md5.len(), "LR2 course key not splittable into stage md5s");
        return Ok(());
    };
    let Some(course_ids) = course_index.get(&stages) else {
        report.skipped += 1;
        tracing::debug!(stages = stages.len(), "LR2 course has no matching bmz course");
        return Ok(());
    };
    let record = lr2_course_score_insert(row, imported_at);
    for &course_id in course_ids {
        let mut insert = record.clone();
        insert.course_id = course_id;
        library_db.insert_course_score(&insert)?;
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

/// Builds a course score from an LR2 aggregate course row.  `course_id` is left as
/// 0 and filled in per matching course by the caller.
fn lr2_course_score_insert(row: &Lr2ScoreRow, imported_at: i64) -> CourseScoreInsert {
    let clear_type = lr2_clear_type(row.clear);
    let course_failed = matches!(clear_type, ClearType::NoPlay | ClearType::Failed);
    CourseScoreInsert {
        course_id: 0,
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
) -> Result<HashMap<Vec<[u8; 16]>, Vec<i64>>> {
    let mut index: HashMap<Vec<[u8; 16]>, Vec<i64>> = HashMap::new();
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
            index.entry(key).or_default().push(course.id);
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
    })
}

#[derive(Debug)]
struct BeatorajaScoreRow {
    sha256: String,
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
}

fn beatoraja_row(row: &Row<'_>) -> rusqlite::Result<BeatorajaScoreRow> {
    Ok(BeatorajaScoreRow {
        sha256: row.get(0)?,
        clear: row.get(1)?,
        epg: row.get(2)?,
        lpg: row.get(3)?,
        egr: row.get(4)?,
        lgr: row.get(5)?,
        egd: row.get(6)?,
        lgd: row.get(7)?,
        ebd: row.get(8)?,
        lbd: row.get(9)?,
        epr: row.get(10)?,
        lpr: row.get(11)?,
        ems: row.get(12)?,
        lms: row.get(13)?,
        total_notes: row.get(14)?,
        max_combo: row.get(15)?,
        min_bp: row.get(16)?,
        ghost: row.get::<_, Option<String>>(17)?.unwrap_or_default(),
        random_seed: row.get(18)?,
        date: row.get(19)?,
    })
}

fn score_state_from_lr2(row: &Lr2ScoreRow) -> ScoreState {
    let ghost = decode_lr2_ghost(&row.ghost, row.total_notes);
    let _ = (row.min_bp, row.play_count, row.clear_count);
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
        past_notes: row.total_notes,
        ghost,
    }
}

fn score_state_from_beatoraja(row: &BeatorajaScoreRow) -> ScoreState {
    let ghost = decode_external_ghost(&row.ghost, row.total_notes);
    let _ = row.min_bp;
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
        past_notes: row.total_notes,
        ghost,
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
    use bmz_chart::model::{ChartMetadata, PlayableChart};
    use bmz_core::time::TimeUs;
    use rusqlite::params;

    use super::*;
    use crate::storage::common::hash_to_hex;
    use crate::storage::library_db::{ChartImportRecord, LibraryDatabase};
    use crate::storage::migration::{LIBRARY_MIGRATIONS, SCORE_MIGRATIONS, run_migrations};
    use crate::storage::score_db::ScoreKey;

    #[test]
    fn lr2_import_maps_md5_and_clear_type() {
        let (mut library_db, mut score_db, sha256, md5) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source(&source, &md5);

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.imported, 1);
        let best = score_db
            .best_scores_for_charts(&[ScoreKey::new(sha256, LnScorePolicy::ForceLn)])
            .unwrap();
        assert_eq!(best[0].clear_type, "Hard");
        assert_eq!(best[0].ex_score, 221);
        assert_eq!(best[0].ln_policy, LnScorePolicy::ForceLn);
        assert_eq!(best[0].device_type, InputDeviceKind::Keyboard);
    }

    #[test]
    fn beatoraja_import_preserves_fast_slow_counts_and_current_schema_fields() {
        let (library_db, mut score_db, sha256, _) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source(&source, &sha256, 1_700_000_001_000);

        let report = import_beatoraja_scores(
            &source,
            ScoreImportKind::Beatoraja,
            &library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.imported, 1);
        let row: (String, u32, u32, u32, String, String, String, i64) = score_db
            .conn()
            .query_row(
                "SELECT clear_type, fast_pgreat, slow_pgreat, slow_empty_poor,
                    rule_mode, ln_policy, device_type, played_at
                 FROM score_history",
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
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "ExHard".to_string(),
                10,
                3,
                1,
                "Beatoraja".to_string(),
                "ForceLn".to_string(),
                "keyboard".to_string(),
                1_700_000_001,
            )
        );
    }

    #[test]
    fn lr2oraja_dx_import_sets_dx_rule_mode() {
        let (mut library_db, mut score_db, _, md5) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source(&source, &md5);

        import_lr2_scores(
            &source,
            ScoreImportKind::Lr2OrajaDx,
            &mut library_db,
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
        create_beatoraja_source_with_sha256(&source, &course_key, 1_700_000_001_000);

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
        let entries: Vec<CourseEntry> = stage_md5s
            .iter()
            .enumerate()
            .map(|(i, m)| CourseEntry {
                title_hint: format!("stage{i}"),
                md5: Some(m.to_string()),
                sha256: None,
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
        let count: i64 = library_db
            .conn()
            .query_row("SELECT COUNT(*) FROM course_scores", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        // The imported course score reflects the LR2 aggregate row (clear=4 -> Hard,
        // ex = perfect*2 + great = 221).
        let (clear, ex): (String, u32) = library_db
            .conn()
            .query_row("SELECT clear_type, ex_score FROM course_scores LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(clear, "Hard");
        assert_eq!(ex, 221);
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
        };
        let state = score_state_from_lr2(&row);
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
        let mut library_conn = Connection::open_in_memory().unwrap();
        super::super::common::configure_connection(&library_conn).unwrap();
        run_migrations(&mut library_conn, LIBRARY_MIGRATIONS).unwrap();
        let mut library_db = LibraryDatabase::from_connection(library_conn);
        let chart = chart();
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

    fn create_lr2_source(conn: &Connection, md5: &[u8; 16]) {
        create_lr2_source_with_hash(conn, &hash_to_hex(md5));
    }

    fn create_lr2_source_with_hash(conn: &Connection, hash: &str) {
        conn.execute_batch(
            "CREATE TABLE score (
                hash TEXT, clear INTEGER, perfect INTEGER, great INTEGER,
                good INTEGER, bad INTEGER, poor INTEGER, totalnotes INTEGER,
                maxcombo INTEGER, minbp INTEGER, playcount INTEGER, clearcount INTEGER,
                ghost TEXT, rseed INTEGER
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO score VALUES (?1, 4, 100, 21, 3, 2, 1, 128, 64, 3, 2, 1, '', 123)",
            params![hash],
        )
        .unwrap();
    }

    fn create_beatoraja_source(conn: &Connection, sha256: &[u8; 32], date: i64) {
        create_beatoraja_source_with_sha256(conn, &hash_to_hex(sha256), date);
    }

    fn create_beatoraja_source_with_sha256(conn: &Connection, sha256: &str, date: i64) {
        conn.execute_batch(
            "CREATE TABLE score (
                sha256 TEXT, clear INTEGER, epg INTEGER, lpg INTEGER,
                egr INTEGER, lgr INTEGER, egd INTEGER, lgd INTEGER,
                ebd INTEGER, lbd INTEGER, epr INTEGER, lpr INTEGER,
                ems INTEGER, lms INTEGER, notes INTEGER, combo INTEGER,
                minbp INTEGER, ghost TEXT, seed INTEGER, date INTEGER
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO score VALUES (?1, 7, 10, 3, 4, 2, 1, 1, 0, 0, 2, 1, 3, 1, 128, 80, 2, '', 456, ?2)",
            params![sha256, date],
        )
        .unwrap();
    }
}
