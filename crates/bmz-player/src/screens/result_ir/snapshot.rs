use super::*;

/// beatoraja の初期位置を基準に、末尾で表示行が空かないよう補正する。
pub(super) fn initial_skin_scroll_offset(ranking: &ResultIrRanking) -> usize {
    let offset = ranking.self_rank.filter(|rank| *rank > 10).map_or(0, |rank| (rank - 5) as usize);
    offset.min(ranking.entries.len().saturating_sub(bmz_render::scene::IR_RANKING_ENTRY_SLOTS))
}

#[cfg(test)]
mod initial_position_tests {
    use super::*;

    #[test]
    fn initial_position_follows_beatoraja_without_empty_trailing_rows() {
        for (count, rank, expected) in [
            (100, None, 0),
            (100, Some(1), 0),
            (100, Some(10), 0),
            (100, Some(11), 6),
            (100, Some(50), 45),
            (100, Some(95), 90),
            (100, Some(100), 90),
            (100, Some(150), 90),
            (10, Some(10), 0),
            (5, Some(5), 0),
            (0, None, 0),
        ] {
            let ranking = ResultIrRanking {
                scope: IrRankingScope::Global,
                entries: (1..=count)
                    .map(|rank| ResultIrRankingEntry {
                        rank,
                        player_name: format!("player-{rank}"),
                        ex_score: 0,
                        clear: "Normal".to_string(),
                        bp: 0,
                        max_combo: 0,
                    })
                    .collect(),
                clear_rate: None,
                clear_counts: None,
                self_rank: rank,
                previous_rank: None,
                total: Some(count),
            };
            let offset = initial_skin_scroll_offset(&ranking);
            assert_eq!(offset, expected, "count={count}, rank={rank:?}");
            let snapshot = result_ir_ranking_to_skin_snapshot_at(&ranking, offset);
            if count >= 10 {
                assert!(snapshot.entries.iter().all(|entry| entry.rank.is_some()));
            }
        }
    }
}

/// 取得済みグローバルランキングをスキン用 snapshot に変換する。
pub fn ranking_to_ir_snapshot(ranking: &IrRankingResult) -> bmz_render::scene::ResultIrSnapshot {
    result_ir_ranking_to_skin_snapshot(&chart_ranking_to_result_ir_ranking(ranking))
}

pub(crate) fn result_ir_ranking_to_skin_snapshot(
    ranking: &ResultIrRanking,
) -> bmz_render::scene::ResultIrSnapshot {
    result_ir_ranking_to_skin_snapshot_at(ranking, 0)
}

pub(super) fn result_ir_ranking_to_skin_snapshot_at(
    ranking: &ResultIrRanking,
    requested_offset: usize,
) -> bmz_render::scene::ResultIrSnapshot {
    use bmz_render::scene::{
        IR_RANKING_ENTRY_SLOTS, ResultIrRankingEntrySnapshot, ResultIrRankingName,
        ResultIrSnapshot, ResultIrState as SkinIrState,
    };
    let scroll_max = ranking.entries.len().saturating_sub(IR_RANKING_ENTRY_SLOTS);
    let scroll_offset = requested_offset.min(scroll_max);
    let mut entries = [ResultIrRankingEntrySnapshot::default(); IR_RANKING_ENTRY_SLOTS];
    for (slot, entry) in entries.iter_mut().zip(ranking.entries.iter().skip(scroll_offset)) {
        *slot = ResultIrRankingEntrySnapshot {
            rank: Some(i64::from(entry.rank)),
            ex_score: Some(i64::from(entry.ex_score)),
            clear_index: bmz_core::clear::ClearType::from_label(&entry.clear)
                .map(|clear| i64::from(clear as u8)),
            player_name: ResultIrRankingName::from_display_name(&entry.player_name),
        };
    }
    ResultIrSnapshot {
        state: SkinIrState::Loaded,
        rank: ranking.self_rank.map(i64::from),
        total_player: ranking.total.map(i64::from).or(Some(ranking.entries.len() as i64)),
        clear_rate: ranking.clear_rate.map(i64::from),
        clear_counts: ranking.clear_counts,
        previous_rank: ranking.previous_rank.map(i64::from),
        scroll_offset,
        scroll_max,
        entries,
        ..Default::default()
    }
}

pub(super) fn chart_ranking_to_result_ir_ranking(ranking: &IrRankingResult) -> ResultIrRanking {
    chart_ranking_to_result_ir_ranking_with_previous(ranking, None)
}

pub(super) fn chart_ranking_to_result_ir_ranking_with_previous(
    ranking: &IrRankingResult,
    previous_rank: Option<u32>,
) -> ResultIrRanking {
    ResultIrRanking {
        scope: ranking.ranking.scope,
        entries: ranking
            .ranking
            .entries
            .iter()
            .map(|entry| ResultIrRankingEntry {
                rank: entry.rank,
                player_name: entry.player.display_name.clone(),
                ex_score: entry.score.ex_score,
                clear: entry.score.clear.clone(),
                bp: entry.score.min_bp,
                max_combo: entry.score.max_combo,
            })
            .collect(),
        clear_rate: ranking.ranking.clear_rate,
        clear_counts: complete_ranking_clear_counts(ranking),
        self_rank: ranking.ranking.self_summary.as_ref().map(|own| own.rank),
        previous_rank,
        total: ranking.ranking.pagination.and_then(|pagination| pagination.total),
    }
}

pub(crate) fn course_ranking_to_result_ir_ranking(
    ranking: &IrCourseRankingResult,
) -> ResultIrRanking {
    ResultIrRanking {
        scope: ranking.ranking.scope,
        entries: ranking
            .ranking
            .entries
            .iter()
            .map(|entry| ResultIrRankingEntry {
                rank: entry.rank,
                player_name: entry.player.display_name.clone(),
                ex_score: entry.score.ex_score,
                clear: entry.score.clear.clone(),
                bp: entry.score.bp,
                max_combo: entry.score.max_combo,
            })
            .collect(),
        clear_rate: None,
        clear_counts: None,
        self_rank: None,
        previous_rank: None,
        total: Some(ranking.ranking.entries.len() as u32),
    }
}

fn complete_ranking_clear_counts(ranking: &IrRankingResult) -> Option<[u32; 11]> {
    let ranking = &ranking.ranking;
    let pagination = ranking.pagination?;
    if pagination.offset != 0
        || pagination.has_more
        || usize::try_from(pagination.total?).ok()? != ranking.entries.len()
    {
        return None;
    }
    let mut counts = [0_u32; 11];
    for entry in &ranking.entries {
        let clear = bmz_core::clear::ClearType::from_label(&entry.score.clear)?;
        counts[clear as usize] += 1;
    }
    Some(counts)
}

pub(super) fn scope_for_tab(tab: ResultRankingTab) -> IrRankingScope {
    match tab {
        ResultRankingTab::Global => IrRankingScope::Global,
        ResultRankingTab::SelfAndRivals => IrRankingScope::SelfAndRivals,
    }
}
