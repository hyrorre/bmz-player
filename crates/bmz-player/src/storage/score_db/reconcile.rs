use super::*;

pub(super) fn source_score_history_match(
    conn: &Connection,
    record: &ScoreRecord,
) -> Result<Option<SourceScoreHistoryMatch>> {
    Ok(source_score_history_candidates(conn, record, false, false)?.into_iter().next())
}

/// Only aggregate callers may ignore metadata which upstream updates on a BP
/// improvement. Never select an arbitrary representative from multiple matches.
pub(super) fn source_aggregate_history_match(
    conn: &Connection,
    record: &ScoreRecord,
) -> Result<SourceScoreHistoryLookup> {
    if let Some(exact) =
        source_score_history_candidates(conn, record, true, false)?.into_iter().next()
    {
        return Ok(SourceScoreHistoryLookup::Found(exact));
    }
    let candidates = source_score_history_candidates(conn, record, true, true)?;
    // Proven aggregate origins sort first. Two rows suffice to detect multiple
    // aggregate candidates; unknown origins must never be inferred from judges.
    match candidates.as_slice() {
        [] => Ok(SourceScoreHistoryLookup::Missing),
        [only, rest @ ..] if only.is_aggregate && !rest.iter().any(|row| row.is_aggregate) => {
            Ok(SourceScoreHistoryLookup::Found(*only))
        }
        _ => Ok(SourceScoreHistoryLookup::Ambiguous),
    }
}

fn source_score_history_candidates(
    conn: &Connection,
    record: &ScoreRecord,
    aggregate: bool,
    ignore_options: bool,
) -> Result<Vec<SourceScoreHistoryMatch>> {
    let judges = &record.score.judges;
    let mut statement = conn.prepare(
        "SELECT id, device_type, source_min_bp,
                EXISTS (SELECT 1 FROM score_history_sources AS origin
                        WHERE origin.score_history_id = score_history.id
                          AND origin.source = 'score_db_aggregate') AS is_aggregate
         FROM score_history
         WHERE source_kind = ?1
           AND chart_sha256 = ?2
           AND ln_policy = ?3
           AND double_option = ?4
           AND clear_type = ?5
           AND gauge_type = ?6
           AND (?7 IS NULL OR gauge_value = ?7)
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
           AND (?34 OR random_seed IS ?25)
           AND (?34 OR arrange = ?26)
           AND (?34 OR arrange_2p = ?27)
           AND gauge_option = ?28
           AND rule_mode = ?29
           AND assist_mask = ?30
           AND autoplay = ?31
           AND (?34 OR applied_double_option = ?32)
           AND seed_scheme = ?33
           AND (NOT ?35 OR course_score_id IS NULL)
         ORDER BY CASE WHEN ?34 THEN is_aggregate ELSE 0 END DESC,
         EXISTS (
            SELECT 1
            FROM score_best
            WHERE score_best.best_score_history_id = score_history.id
         ) DESC,
         id ASC
         LIMIT CASE WHEN ?34 THEN 2 ELSE 1 END",
    )?;
    statement
        .query_map(
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
                ignore_options,
                aggregate,
            ],
            |row| {
                Ok(SourceScoreHistoryMatch {
                    history_id: row.get(0)?,
                    device_type: device_type_from_row(row, 1)?,
                    source_min_bp: row.get(2)?,
                    is_aggregate: row.get(3)?,
                })
            },
        )?
        .collect::<rusqlite::Result<_>>()
        .map_err(Into::into)
}

pub(super) fn mark_score_history_aggregate(conn: &Connection, history_id: i64) -> Result<bool> {
    // Use the existing provenance table. This origin is neither an IR identity
    // nor a submitted score; the history foreign key also removes it on cleanup.
    Ok(conn.execute(
        "INSERT OR IGNORE INTO score_history_sources (
            score_history_id, source, provider, account_id, remote_score_id,
            verification, server_received_at, imported_at
         ) SELECT id, 'score_db_aggregate', source_kind, '', CAST(id AS TEXT),
                  '', 0, CAST(strftime('%s', 'now') AS INTEGER)
           FROM score_history WHERE id = ?1 AND course_score_id IS NULL",
        [history_id],
    )? > 0)
}

pub(super) fn update_score_best_device_type_from_history(
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

pub(super) fn legacy_beatoraja_matching_history_ids(
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

pub(super) fn delete_score_history_ids(conn: &Connection, history_ids: &[i64]) -> Result<u32> {
    const DELETE_CHUNK_SIZE: usize = 500;
    let mut deleted = 0_u32;
    for ids in history_ids.chunks(DELETE_CHUNK_SIZE) {
        let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
        let sql = format!("DELETE FROM score_history WHERE id IN ({placeholders})");
        deleted = deleted.saturating_add(conn.execute(&sql, params_from_iter(ids.iter()))? as u32);
    }
    Ok(deleted)
}

pub(super) fn rebuild_score_aggregates(conn: &Connection) -> Result<()> {
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
        END = score_best.rule_mode";
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
    let affected = format!("EXISTS (SELECT 1 FROM cleanup_removed AS h WHERE {SCORE_KEY})");
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
             WHERE {affected} AND NOT EXISTS (SELECT 1 FROM score_history AS h WHERE {SCORE_KEY})"
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
                clear_count = {clear_count} WHERE {affected}",
            clear_type = clear_value("clear_type"),
            gauge_type = clear_value("gauge_type"),
            gauge_value = clear_value("gauge_value"),
            ex_score = score_value("ex_score"),
            bp = aggregate(
                "MIN(CASE WHEN h.source_min_bp IS NULL THEN h.bp
                ELSE MIN(h.bp, h.source_min_bp) END)"
            ),
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
         FROM score_history",
        params![preserved_playtime_seconds],
    )?;
    Ok(())
}

// Cleanup must not reconstruct history-less assisted plays from score_history.
// Keep the original aggregates and subtract only the histories being removed.
// Temporary tables live inside the caller's transaction, including on failure.
pub(super) fn preserve_cleanup_aggregates(conn: &Connection, ids: &[i64]) -> Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE cleanup_stats AS SELECT * FROM player_stats;
         CREATE TEMP TABLE cleanup_removed AS SELECT * FROM score_history WHERE 0;
         CREATE TEMP TABLE cleanup_history AS
            SELECT chart_sha256, ln_policy, double_option, rule_mode, clear_type, COUNT(*) AS plays
            FROM score_history GROUP BY chart_sha256, ln_policy, double_option, rule_mode, clear_type;",
    )?;
    for ids in ids.chunks(500) {
        let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(",");
        conn.execute(
            &format!("INSERT INTO cleanup_removed SELECT * FROM score_history WHERE id IN ({placeholders}) AND id NOT IN (SELECT id FROM cleanup_removed)"),
            params_from_iter(ids.iter()),
        )?;
    }
    conn.execute_batch(
        "CREATE TEMP TABLE cleanup_best AS SELECT o.* FROM score_best o
         WHERE EXISTS (SELECT 1 FROM cleanup_removed h
             WHERE h.chart_sha256 = o.chart_sha256 AND h.ln_policy = o.ln_policy
             AND h.double_option = o.double_option
             AND CASE h.rule_mode WHEN 'Lr2Oraja' THEN 'Lr2Oraja' WHEN 'Dx' THEN 'Dx'
                 ELSE 'Beatoraja' END = o.rule_mode);",
    )?;
    Ok(())
}

pub(super) fn restore_cleanup_aggregates(conn: &Connection) -> Result<()> {
    const MATCH: &str = "h.chart_sha256 = o.chart_sha256 AND h.ln_policy = o.ln_policy
        AND h.double_option = o.double_option
        AND CASE h.rule_mode WHEN 'Lr2Oraja' THEN 'Lr2Oraja' WHEN 'Dx' THEN 'Dx'
            ELSE 'Beatoraja' END = o.rule_mode";
    const BEST_MATCH: &str = "o.chart_sha256 = score_best.chart_sha256
        AND o.ln_policy = score_best.ln_policy AND o.double_option = score_best.double_option
        AND o.rule_mode = score_best.rule_mode";
    let affected = format!("EXISTS (SELECT 1 FROM cleanup_removed h WHERE {MATCH})");
    let unrecorded = format!(
        "o.play_count > (SELECT COALESCE(SUM(plays), 0) FROM cleanup_history h WHERE {MATCH})"
    );
    // Keep a neutral row when the last numeric history was removed but assisted
    // plays still exist. Unaffected keys are never rewritten.
    conn.execute(
        &format!("INSERT OR IGNORE INTO score_best SELECT o.* FROM cleanup_best o WHERE {affected} AND {unrecorded}"),
        [],
    )?;
    let numeric_columns = [
        "ex_score",
        "bp",
        "cb",
        "max_combo",
        "fast_pgreat",
        "slow_pgreat",
        "fast_great",
        "slow_great",
        "fast_good",
        "slow_good",
        "fast_bad",
        "slow_bad",
        "fast_poor",
        "slow_poor",
        "fast_empty_poor",
        "slow_empty_poor",
    ];
    let reset =
        numeric_columns.iter().map(|column| format!("{column} = 0")).collect::<Vec<_>>().join(",");
    conn.execute(
        &format!("UPDATE score_best SET {reset}, best_score_history_id = NULL, replay_path = '', ghost = ''
            WHERE best_score_history_id IN (SELECT id FROM cleanup_removed)"),
        [],
    )?;
    for (column, condition) in
        [("play_count", "1"), ("clear_count", "h.clear_type NOT IN ('NoPlay', 'Failed')")]
    {
        conn.execute(
            &format!(
                "UPDATE score_best SET {column} = (SELECT MAX(0, o.{column} -
                (SELECT COUNT(*) FROM cleanup_removed h WHERE {MATCH} AND {condition}))
                FROM cleanup_best o WHERE {BEST_MATCH})
                WHERE EXISTS (SELECT 1 FROM cleanup_best o WHERE {BEST_MATCH} AND {affected})"
            ),
            [],
        )?;
    }
    // A lamp with no history provenance cannot be reconstructed. Preserve it
    // when it belongs to unrecorded plays; ordinary history lamps still rebuild.
    for column in ["clear_type", "gauge_type", "gauge_value"] {
        conn.execute(
            &format!("UPDATE score_best SET {column} = (SELECT o.{column} FROM cleanup_best o WHERE {BEST_MATCH})
                WHERE EXISTS (SELECT 1 FROM cleanup_best o WHERE {BEST_MATCH} AND {affected} AND {unrecorded}
                    AND NOT EXISTS (SELECT 1 FROM cleanup_history h WHERE {MATCH} AND h.clear_type = o.clear_type))"),
            [],
        )?;
    }
    // Profile counters include assisted plays too. Subtraction preserves those
    // contributions, unlike replacing the counters with sums of remaining rows.
    conn.execute_batch(
        "DELETE FROM player_stats; INSERT INTO player_stats SELECT * FROM cleanup_stats;",
    )?;
    for column in [
        "play_count",
        "clear_count",
        "fast_pgreat",
        "slow_pgreat",
        "fast_great",
        "slow_great",
        "fast_good",
        "slow_good",
        "fast_bad",
        "slow_bad",
        "fast_poor",
        "slow_poor",
        "fast_empty_poor",
        "slow_empty_poor",
    ] {
        let removed = match column {
            "play_count" => "COUNT(*)".to_string(),
            "clear_count" => {
                "SUM(CASE WHEN clear_type NOT IN ('NoPlay', 'Failed') THEN 1 ELSE 0 END)"
                    .to_string()
            }
            _ => format!("SUM({column})"),
        };
        conn.execute(&format!("UPDATE player_stats SET {column} = MAX(0, {column} - (SELECT COALESCE({removed}, 0) FROM cleanup_removed))"), [])?;
    }
    // Without unrecorded plays, maxima can be recomputed exactly. Otherwise keep
    // the existing maximum: older schemas did not retain its assisted provenance.
    conn.execute_batch(
        "UPDATE player_stats SET max_combo = (SELECT COALESCE(MAX(max_combo), 0) FROM score_history)
         WHERE (SELECT play_count FROM cleanup_stats) = (SELECT COALESCE(SUM(plays), 0) FROM cleanup_history);
         DROP TABLE cleanup_best; DROP TABLE cleanup_stats;
         DROP TABLE cleanup_removed; DROP TABLE cleanup_history;",
    )?;
    Ok(())
}
