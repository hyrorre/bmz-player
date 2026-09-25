use super::*;

/// Imports an LR2 course (dan) result into every canonical bmz course it matches.
///
/// LR2 course keys cannot be mapped to a single bmz course unambiguously, but for
/// dan认定 the play options are canonical: normal+mirror class, free HS, no judge
/// constraint, LR2 gauge.  After filtering candidates to that set, the only
/// remaining ambiguity is the LN constraint, and we deliberately import into every
/// matching LN variant (a course whose charts contain no LN scores identically with
/// or without the constraint, and LR2 dan is always LN-on).  Per-chart breakdown is
/// not available from LR2's aggregate course row, so `charts`/`replays` are empty.
pub(super) fn import_lr2_course(
    row: &Lr2ScoreRow,
    course_index: &HashMap<Vec<[u8; 16]>, Vec<CourseImportTarget>>,
    score_db: &mut ScoreDatabase,
    rule_mode: RuleMode,
    imported_at: i64,
    report: &mut ScoreImportReport,
) -> Result<()> {
    let Some(stages) = lr2_course_stage_md5s(&row.md5) else {
        report.skipped += 1;
        report
            .issue(ScoreImportIssueKind::Unsupported, "LR2 course key is not a stage MD5 sequence");
        tracing::debug!(len = row.md5.len(), "LR2 course key not splittable into stage md5s");
        return Ok(());
    };
    let Some(targets) = course_index.get(&stages) else {
        report.skipped += 1;
        report.issue(ScoreImportIssueKind::MissingChart, "no matching LR2 course definition");
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
pub(super) fn lr2_course_stage_md5s(hash: &str) -> Option<Vec<[u8; 16]>> {
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
pub(super) struct CourseImportTarget {
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
pub(super) fn lr2_course_score_insert(
    row: &Lr2ScoreRow,
    target: &CourseImportTarget,
    rule_mode: RuleMode,
    imported_at: i64,
) -> CourseScoreInsert {
    let clear_type = lr2_clear_type(row.clear);
    let course_failed = matches!(clear_type, ClearType::NoPlay | ClearType::Failed);
    CourseScoreInsert {
        course_hash: target.course_hash.clone(),
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
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
pub(super) fn build_lr2_course_index(
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
pub(super) fn is_course_hash(hash: &str, single_len: usize) -> bool {
    let len = hash.len();
    len > single_len && len.is_multiple_of(single_len)
}

pub(super) fn ensure_table(conn: &Connection, table: &str) -> Result<()> {
    if table_exists(conn, table)? {
        Ok(())
    } else {
        bail!("score database must contain {table} table")
    }
}

pub(super) fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [table],
            |_| Ok(()),
        )
        .is_ok())
}
