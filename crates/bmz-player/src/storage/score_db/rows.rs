use super::*;

pub(super) fn best_score_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<BestScoreSummary> {
    let sha256_hex: String = row.get(0)?;
    let chart_sha256 = hex_to_hash::<32>(&sha256_hex)?;
    let ln_policy = ln_policy_from_row(row, 1)?;
    let double_option = double_option_from_row(row, 2)?;
    let rule_mode = rule_mode_from_row(row, 3)?;
    let arrange: Option<String> = row.get(28)?;
    let arrange_2p: Option<String> = row.get(29)?;
    let applied_double_option: Option<String> = row.get(30)?;
    let play_options = arrange.zip(arrange_2p).zip(applied_double_option).map(
        |((arrange, arrange_2p), double_option)| {
            use crate::select_options::ArrangeOption;
            bmz_render::snapshot::SkinBestScoreOptions {
                arrange_1p: bmz_render::skin::extended_arrange_index(
                    ArrangeOption::from_persistent_str(&arrange).as_str(),
                ),
                arrange_2p: bmz_render::skin::extended_arrange_index(
                    ArrangeOption::from_persistent_str(&arrange_2p).as_str(),
                ),
                double_option: bmz_render::skin::select_double_option_index(
                    DoubleOption::from_persistent_str(&double_option).as_str(),
                ),
            }
        },
    );

    Ok(BestScoreSummary {
        play_options,
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

pub(super) fn replay_slot_record_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ReplaySlotRecord> {
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
        source_kind: score_source_kind_from_row(row, 13)?,
        source_path: row.get(14)?,
        source_fingerprint: row.get(15)?,
    })
}

pub(super) fn score_history_entry_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ScoreHistoryEntry> {
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

pub(super) fn player_stats_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlayerStats> {
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

pub(super) fn daily_player_stats_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<DailyPlayerStats> {
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

pub(super) fn device_type_from_row(
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

pub(super) fn score_source_kind_from_row(
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
