use super::*;

pub(super) fn display_judge_counts(session: &GameSession) -> DisplayJudgeCounts {
    display_judge_counts_for_score(&session.score)
}

pub(super) fn display_judge_counts_for_score(
    score: &bmz_gameplay::score::ScoreState,
) -> DisplayJudgeCounts {
    let judges = &score.judges;
    DisplayJudgeCounts {
        pgreat: judges.fast_pgreat + judges.slow_pgreat,
        great: judges.fast_great + judges.slow_great,
        good: judges.fast_good + judges.slow_good,
        bad: judges.fast_bad + judges.slow_bad,
        poor: judges.fast_poor + judges.slow_poor,
        empty_poor: judges.fast_empty_poor + judges.slow_empty_poor,
    }
}

pub(super) fn ghost_ex_score_at_progress(ghost: &[u8], past_notes: u32) -> u32 {
    ghost.iter().take(past_notes as usize).map(|&judge| ghost_ex_value(judge)).sum()
}

pub(super) fn ghost_ex_value(judge: u8) -> u32 {
    match judge {
        0 => 2,
        1 => 1,
        _ => 0,
    }
}

pub(super) fn display_fast_slow_counts(
    session: &GameSession,
) -> bmz_render::snapshot::FastSlowJudgeCounts {
    let judges = &session.score.judges;
    bmz_render::snapshot::FastSlowJudgeCounts {
        fast_pgreat: judges.fast_pgreat,
        slow_pgreat: judges.slow_pgreat,
        fast_great: judges.fast_great,
        slow_great: judges.slow_great,
        fast_good: judges.fast_good,
        slow_good: judges.slow_good,
        fast_bad: judges.fast_bad,
        slow_bad: judges.slow_bad,
        fast_poor: judges.fast_poor,
        slow_poor: judges.slow_poor,
        fast_empty_poor: judges.fast_empty_poor,
        slow_empty_poor: judges.slow_empty_poor,
    }
}

pub(super) fn display_judgement(
    event: &JudgementEvent,
    combo: u32,
    display_time: TimeUs,
) -> DisplayJudgement {
    DisplayJudgement {
        lane: event.lane,
        judge: event.judge,
        side: Some(event.side),
        text: format!("{}{}", judge_text(event.judge), side_suffix(event.side)),
        combo: if event.judge == Judge::EmptyPoor { 0 } else { combo },
        delta_us: event.delta.0,
        time: display_time,
        is_miss: event.judge == Judge::Poor,
        timing_ms_suppressed: false,
    }
}

/// FAST/SLOW 表示フィルタを適用し、非表示対象の判定の side と text を除去する。
///
/// - `Auto`: PGREAT は常に非表示。GREAT 以下は常時表示（beatoraja 準拠）。threshold_ms 無視。
/// - `ThresholdMs`: PGREAT かつ |delta| < threshold_ms なら非表示。GREAT 以下は常時表示。
///   bmz 独自拡張なので ±ms 数値表示 (ref 525) も合わせて非表示にする。
pub fn apply_fast_slow_display_filter(
    snapshot: &mut RenderSnapshot,
    threshold_ms: u32,
    scope: crate::config::profile_config::FastSlowDisplayScope,
) {
    use crate::config::profile_config::FastSlowDisplayScope;
    for judgement in &mut snapshot.recent_judgements {
        let suppress = match scope {
            FastSlowDisplayScope::Auto => judgement.judge == Judge::PGreat,
            FastSlowDisplayScope::ThresholdMs => {
                judgement.judge == Judge::PGreat
                    && threshold_ms > 0
                    && judgement.delta_us.unsigned_abs() / 1_000 < threshold_ms as u64
            }
        };
        if suppress {
            judgement.side = None;
            // ThresholdMs は bmz 独自拡張なので ±ms 数値表示 (ref 525) も隠す。
            // Auto は beatoraja 準拠のため 525 は隠さない (beatoraja は常に供給する)。
            judgement.timing_ms_suppressed = scope == FastSlowDisplayScope::ThresholdMs;
            let base = judgement
                .text
                .strip_suffix(" FAST")
                .or_else(|| judgement.text.strip_suffix(" SLOW"))
                .unwrap_or(&judgement.text);
            judgement.text = base.to_string();
        }
    }
}

/// `render_now` の時点で有効な BPM を返す。
#[cfg(test)]
pub(super) fn current_bpm(chart: &bmz_chart::model::PlayableChart, render_now: TimeUs) -> f64 {
    let mut bpm = chart.metadata.initial_bpm;
    for event in &chart.timing_events {
        if event.time > render_now {
            break;
        }
        if let TimingEventKind::BpmChange { bpm: b } = event.kind {
            bpm = b;
        }
    }
    bpm
}

pub(super) fn chart_min_bpm(chart: &bmz_chart::model::PlayableChart) -> f64 {
    chart
        .timing_events
        .iter()
        .filter_map(
            |e| if let TimingEventKind::BpmChange { bpm } = e.kind { Some(bpm) } else { None },
        )
        .fold(chart.metadata.initial_bpm, f64::min)
}

pub(super) fn chart_max_bpm(chart: &bmz_chart::model::PlayableChart) -> f64 {
    chart
        .timing_events
        .iter()
        .filter_map(
            |e| if let TimingEventKind::BpmChange { bpm } = e.kind { Some(bpm) } else { None },
        )
        .fold(chart.metadata.initial_bpm, f64::max)
}

pub(super) fn judge_text(judge: Judge) -> &'static str {
    match judge {
        Judge::PGreat => "PGREAT",
        Judge::Great => "GREAT",
        Judge::Good => "GOOD",
        Judge::Bad => "BAD",
        Judge::Poor => "POOR",
        Judge::EmptyPoor => "EMPTY POOR",
    }
}

pub(super) fn side_suffix(side: TimingSide) -> &'static str {
    match side {
        TimingSide::Fast => " FAST",
        TimingSide::Slow => " SLOW",
    }
}
